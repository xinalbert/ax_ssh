use super::*;

#[derive(Clone, Copy, Debug)]
pub(in crate::app) enum TerminalInputKind {
    Key,
    Paste,
    Pointer,
    Focus,
    Protocol,
    Command,
}

impl TerminalInputKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Paste => "paste",
            Self::Pointer => "pointer",
            Self::Focus => "focus",
            Self::Protocol => "protocol",
            Self::Command => "command",
        }
    }
}

static NEXT_TERMINAL_INPUT_SEQUENCE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

impl TerminalTabState {
    pub(in crate::app) const MAX_RECONNECT_ATTEMPTS: u8 = ReconnectState::MAX_ATTEMPTS;

    pub(in crate::app) fn kind(&self) -> &'static str {
        if self.is_sftp() { "sftp" } else { "terminal" }
    }

    pub(in crate::app) fn offer_clipboard_read(
        &mut self,
        formatter: ClipboardLoadFormatter,
    ) -> Option<(u64, u64)> {
        if !self
            .terminal
            .as_ref()
            .is_some_and(TerminalModel::osc52_clipboard_enabled)
            || !self.connected
            || !self.worker_running
            || self.worker.is_none()
        {
            return None;
        }
        self.next_clipboard_read_token = self.next_clipboard_read_token.wrapping_add(1);
        if self.next_clipboard_read_token == 0 {
            self.next_clipboard_read_token = 1;
        }
        let token = self.next_clipboard_read_token;
        let generation = self.reconnect_generation();
        self.pending_clipboard_read = Some(PendingClipboardRead {
            formatter,
            token,
            generation,
        });
        Some((token, generation))
    }

    pub(in crate::app) fn clipboard_read_key(&self) -> Option<(u64, u64)> {
        let pending = self.pending_clipboard_read.as_ref()?;
        (self
            .terminal
            .as_ref()
            .is_some_and(TerminalModel::osc52_clipboard_enabled)
            && pending.generation == self.reconnect_generation()
            && self.connected
            && self.worker_running
            && self.worker.is_some())
        .then_some((pending.token, pending.generation))
    }

    pub(in crate::app) fn take_pending_clipboard_read(&mut self) -> Option<ClipboardLoadFormatter> {
        let (token, generation) = self.clipboard_read_key()?;
        if generation != self.reconnect_generation()
            || !self.connected
            || !self.worker_running
            || self.worker.is_none()
        {
            return None;
        }
        self.pending_clipboard_read.take().and_then(|pending| {
            (pending.token == token && pending.generation == generation)
                .then_some(pending.formatter)
        })
    }

    pub(in crate::app) fn clear_clipboard_read(&mut self, token: u64, generation: u64) -> bool {
        if self.pending_clipboard_read.as_ref().is_some_and(|pending| {
            pending.token == token
                && pending.generation == generation
                && generation == self.reconnect_generation()
        }) {
            self.pending_clipboard_read = None;
            true
        } else {
            false
        }
    }

    pub(in crate::app) fn clear_pending_clipboard_read(&mut self) -> bool {
        self.pending_clipboard_read.take().is_some()
    }

    #[cfg(test)]
    pub(in crate::app) fn set_test_pending_clipboard_read(
        &mut self,
        formatter: ClipboardLoadFormatter,
    ) {
        self.pending_clipboard_read = Some(PendingClipboardRead {
            formatter,
            token: 1,
            generation: self.reconnect_generation(),
        });
    }

    pub(in crate::app) fn defer_output_presentation(&mut self) {
        self.viewport.defer_output();
    }

    pub(in crate::app) fn prepare_terminal_output_snapshot(&mut self) -> bool {
        self.viewport.prepare(self.terminal.as_mut())
    }

    pub(in crate::app) fn terminal_snapshot_for_ui(&mut self) -> Option<TerminalSnapshot> {
        self.viewport.snapshot(self.terminal.as_mut())
    }

    pub(in crate::app) fn discard_pending_terminal_snapshot(&mut self) {
        self.viewport.discard_pending();
    }

    pub(in crate::app) fn selection_revision(&self) -> i32 {
        self.viewport.selection_revision()
    }

    pub(in crate::app) fn begin_reconnect(&mut self) -> Option<(u64, u8)> {
        self.reconnect.begin(self.worker.is_some())
    }

    pub(in crate::app) fn reconnect_current(&self, generation: u64) -> bool {
        self.reconnect.current(generation)
    }

    pub(in crate::app) fn finish_reconnect_attempt(&mut self, generation: u64) -> bool {
        self.reconnect.finish_attempt(generation)
    }

    pub(in crate::app) fn mark_reconnect_connected(&mut self, generation: u64) -> bool {
        self.reconnect.mark_connected(generation)
    }

    pub(in crate::app) fn cancel_reconnect(&mut self) {
        self.reconnect.cancel();
        self.clear_pending_clipboard_read();
        self.media.cancel();
    }

    pub(in crate::app) fn prepare_manual_retry(&mut self) {
        let enabled = !self.is_local();
        self.reconnect.prepare_manual_retry(enabled);
        self.clear_pending_clipboard_read();
        self.media.cancel();
    }

    pub(in crate::app) fn enable_reconnect(&mut self) {
        self.reconnect.enable();
    }

    pub(in crate::app) fn reconnect_generation(&self) -> u64 {
        self.reconnect.generation()
    }

    pub(in crate::app) fn reconnect_attempt(&self) -> u8 {
        self.reconnect.attempt()
    }

    pub(in crate::app) fn reconnecting(&self) -> bool {
        self.reconnect.reconnecting()
    }

    pub(in crate::app) fn reconnect_enabled(&self) -> bool {
        self.reconnect.enabled()
    }

    pub(in crate::app) fn profile_id(&self) -> Option<Uuid> {
        self.backend.profile_id()
    }

    pub(in crate::app) fn ssh_route(&self) -> Option<(Uuid, Option<Uuid>)> {
        self.backend.ssh_route()
    }

    pub(in crate::app) fn telnet_route(&self) -> Option<(Uuid, Option<Uuid>)> {
        self.backend.telnet_route()
    }

    pub(in crate::app) fn serial_route(&self) -> Option<(Uuid, Option<Uuid>)> {
        self.backend.serial_route()
    }

    pub(in crate::app) fn set_ssh_attempt(&mut self, attempt_id: Option<Uuid>) -> bool {
        let previous = self.backend.ssh_route();
        let accepted = self.backend.set_ssh_attempt(attempt_id);
        if accepted && previous != self.backend.ssh_route() {
            self.connection_latency = super::ConnectionLatency::default();
        }
        accepted
    }

    pub(in crate::app) fn set_telnet_attempt(&mut self, attempt_id: Option<Uuid>) -> bool {
        self.backend.set_telnet_attempt(attempt_id)
    }

    pub(in crate::app) fn set_serial_attempt(&mut self, attempt_id: Option<Uuid>) -> bool {
        self.backend.set_serial_attempt(attempt_id)
    }

    pub(in crate::app) fn is_local(&self) -> bool {
        self.backend.is_local()
    }

    pub(in crate::app) fn ssh_phase(&self) -> Option<&SshConnectionPhase> {
        matches!(
            self.backend,
            TerminalBackend::Ssh { .. } | TerminalBackend::Sftp { .. }
        )
        .then_some(&self.ssh_phase)
    }

    pub(in crate::app) fn set_ssh_phase(&mut self, phase: SshConnectionPhase) -> bool {
        if !matches!(
            self.backend,
            TerminalBackend::Ssh { .. } | TerminalBackend::Sftp { .. }
        ) {
            return false;
        }
        if matches!(phase, SshConnectionPhase::Idle) {
            self.pending_auth_secret = None;
        }
        self.ssh_phase = phase;
        true
    }

    pub(in crate::app) fn set_pending_auth_secret(
        &mut self,
        secret: zeroize::Zeroizing<String>,
    ) -> bool {
        if !matches!(
            self.backend,
            TerminalBackend::Ssh { .. } | TerminalBackend::Sftp { .. }
        ) {
            return false;
        }
        self.pending_auth_secret = Some(secret);
        true
    }

    pub(in crate::app) fn take_pending_auth_secret(
        &mut self,
    ) -> Option<zeroize::Zeroizing<String>> {
        if !matches!(
            self.backend,
            TerminalBackend::Ssh { .. } | TerminalBackend::Sftp { .. }
        ) {
            return None;
        }
        self.pending_auth_secret.take()
    }

    pub(in crate::app) fn take_pending_probe(&mut self) -> Option<PendingProbe> {
        if !matches!(
            self.backend,
            TerminalBackend::Ssh { .. } | TerminalBackend::Sftp { .. }
        ) {
            return None;
        }
        let phase = std::mem::replace(&mut self.ssh_phase, SshConnectionPhase::Idle);
        match phase {
            SshConnectionPhase::Probing(probe) => Some(probe),
            phase => {
                self.ssh_phase = phase;
                None
            }
        }
    }

    pub(in crate::app) fn connection_target(&self) -> ConnectionTarget {
        if self.backend.is_sftp() {
            ConnectionTarget::Sftp
        } else {
            ConnectionTarget::Terminal
        }
    }

    pub(in crate::app) fn is_sftp(&self) -> bool {
        self.backend.is_sftp()
    }

    pub(in crate::app) fn notice_snapshot(&self) -> TerminalNoticeSnapshot {
        if self.pending_clipboard_read.is_some() {
            return TerminalNoticeSnapshot::osc52_clipboard_read();
        }
        let status = self.status.trim();
        if status.is_empty()
            || self.connected
            || self.worker_running
            || self.security_prompt_pending()
            || self.user_requested_disconnect_status(status)
        {
            return TerminalNoticeSnapshot::default();
        }

        if self.reconnecting() || status.starts_with("Connection lost; reconnecting") {
            return TerminalNoticeSnapshot::reconnecting(status);
        }

        let restart = self.is_local();
        if self.failure_status(status) {
            return TerminalNoticeSnapshot::failed(status, restart);
        }
        if self.ended_status(status) {
            return TerminalNoticeSnapshot::ended(status, restart);
        }

        TerminalNoticeSnapshot::default()
    }

    fn security_prompt_pending(&self) -> bool {
        matches!(
            self.ssh_phase,
            SshConnectionPhase::Probing(_)
                | SshConnectionPhase::AwaitingHostKey(_)
                | SshConnectionPhase::ConfirmingHostKey(_)
                | SshConnectionPhase::AwaitingAuthentication { .. }
                | SshConnectionPhase::LoadingStoredCredential
        )
    }

    fn user_requested_disconnect_status(&self, status: &str) -> bool {
        !self.reconnect_enabled()
            && (status == "Disconnected"
                || status == "Disconnecting..."
                || status.ends_with(" worker stopped"))
    }

    fn failure_status(&self, status: &str) -> bool {
        let status = status.to_ascii_lowercase();
        status.contains("failed")
            || status.contains("cannot ")
            || status.contains("unavailable")
            || status.contains("not found")
            || status.contains("invalid")
    }

    fn ended_status(&self, status: &str) -> bool {
        status == "Disconnected"
            || status.ends_with("disconnected")
            || status.ends_with("worker stopped")
            || status.starts_with("Local shell exited:")
    }
}

impl TerminalWorker {
    pub(in crate::app) fn sftp_max_file_bytes(&self) -> Result<u64> {
        match self {
            Self::Ssh(worker) => Ok(worker.sftp_max_file_bytes()),
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    pub(in crate::app) fn request_disconnect(&self) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_disconnect(),
            Self::Telnet(worker) => worker.request_disconnect(),
            Self::Serial(worker) => worker.request_disconnect(),
            Self::Local(worker) => worker.request_disconnect(),
        }
    }

    pub(in crate::app) fn request_send(&self, data: Vec<u8>) -> Result<()> {
        self.request_send_kind(data, TerminalInputKind::Command)
    }

    pub(in crate::app) fn request_send_kind(
        &self,
        data: Vec<u8>,
        kind: TerminalInputKind,
    ) -> Result<()> {
        let bytes = data.len();
        let input_sequence =
            NEXT_TERMINAL_INPUT_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let started_at = std::time::Instant::now();
        let result = match self {
            Self::Ssh(worker) => worker.request_send(data),
            Self::Telnet(worker) => worker.request_send(data),
            Self::Serial(worker) => worker.request_send(data),
            Self::Local(worker) => worker.request_send(data),
        };
        tracing::debug!(
            target: "ax_ssh::latency",
            event = "terminal-input",
            stage = "worker-request",
            input_sequence,
            kind = kind.as_str(),
            bytes,
            outcome = if result.is_ok() { "queued" } else { "rejected" },
            elapsed_us = u64::try_from(started_at.elapsed().as_micros()).unwrap_or(u64::MAX),
            "terminal input request dispatched to transport"
        );
        result
    }

    /// OSC 52 uses raw protocol bytes, queued atomically through the existing
    /// bounded/chunked transport path. The formatter adds no paste markers.
    pub(in crate::app) fn request_send_clipboard_response(&self, data: Vec<u8>) -> Result<()> {
        const MAX_RESPONSE: usize =
            ax_ssh::terminal::MAX_OSC52_CLIPBOARD_BYTES.div_ceil(3) * 4 + 32;
        anyhow::ensure!(data.len() <= MAX_RESPONSE, "OSC 52 response is too large");
        self.request_send_paste(data)
    }

    pub(in crate::app) fn request_send_paste(&self, data: Vec<u8>) -> Result<()> {
        let bytes = data.len();
        let input_sequence =
            NEXT_TERMINAL_INPUT_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let started_at = std::time::Instant::now();
        let result = match self {
            Self::Ssh(worker) => worker.request_send_paste(data),
            Self::Telnet(worker) => worker.request_send_paste(data),
            Self::Serial(worker) => worker.request_send_paste(data),
            Self::Local(worker) => worker.request_send_paste(data),
        };
        tracing::debug!(
            target: "ax_ssh::latency",
            event = "terminal-input",
            stage = "worker-request",
            input_sequence,
            kind = TerminalInputKind::Paste.as_str(),
            bytes,
            outcome = if result.is_ok() { "queued" } else { "rejected" },
            elapsed_us = u64::try_from(started_at.elapsed().as_micros()).unwrap_or(u64::MAX),
            "terminal paste request dispatched to transport"
        );
        result
    }

    pub(in crate::app) fn request_send_motion_kind(
        &self,
        data: Vec<u8>,
        kind: TerminalInputKind,
    ) -> Result<bool> {
        let bytes = data.len();
        let input_sequence =
            NEXT_TERMINAL_INPUT_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let started_at = std::time::Instant::now();
        let result = match self {
            Self::Ssh(worker) => worker.request_send_motion(data),
            Self::Telnet(worker) => worker.request_send_motion(data),
            Self::Serial(worker) => worker.request_send_motion(data),
            Self::Local(worker) => worker.request_send_motion(data),
        };
        tracing::debug!(
            target: "ax_ssh::latency",
            event = "terminal-input",
            stage = "worker-request",
            input_sequence,
            kind = kind.as_str(),
            bytes,
            outcome = match result {
                Ok(true) => "queued",
                Ok(false) => "dropped",
                Err(_) => "rejected",
            },
            elapsed_us = u64::try_from(started_at.elapsed().as_micros()).unwrap_or(u64::MAX),
            "terminal input motion dispatched to transport"
        );
        result
    }

    pub(in crate::app) fn request_resize_with_pixels(
        &self,
        columns: u32,
        rows: u32,
        pixel_width: u32,
        pixel_height: u32,
    ) -> Result<()> {
        match self {
            Self::Ssh(worker) => {
                worker.request_resize_with_pixels(columns, rows, pixel_width, pixel_height)
            }
            Self::Telnet(worker) => worker.request_resize(columns, rows),
            Self::Serial(_) => Ok(()),
            Self::Local(worker) => {
                worker.request_resize_with_pixels(columns, rows, pixel_width, pixel_height)
            }
        }
    }

    pub(in crate::app) fn request_list_sftp(&self, request_id: u64, path: String) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_list_sftp(request_id, path),
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    pub(in crate::app) fn request_load_more_sftp(&self, request_id: u64) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_load_more_sftp(request_id),
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    pub(in crate::app) fn request_open_sftp_file(
        &self,
        transfer_id: Uuid,
        path: String,
        local_directory: std::path::PathBuf,
        filter_patterns: Vec<String>,
    ) -> Result<()> {
        match self {
            Self::Ssh(worker) => {
                worker.request_open_sftp_file(transfer_id, path, local_directory, filter_patterns)
            }
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub(in crate::app) fn request_open_sftp_file_at_local_path(
        &self,
        transfer_id: Uuid,
        path: String,
        local_path: std::path::PathBuf,
        total_bytes: u64,
    ) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_open_sftp_file_at_local_path(
                transfer_id,
                path,
                local_path,
                total_bytes,
            ),
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    pub(in crate::app) fn request_open_sftp_upload_batch(
        &self,
        batch_id: Uuid,
        files: Vec<(Uuid, String, std::path::PathBuf, u64, Vec<String>)>,
    ) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_open_sftp_upload_batch(batch_id, files),
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    pub(in crate::app) fn request_resolve_sftp_upload_conflict(
        &self,
        transfer_id: Uuid,
        choice: ax_ssh::sftp::SftpUploadConflictChoice,
        apply_to_batch: bool,
    ) -> Result<()> {
        match self {
            Self::Ssh(worker) => {
                worker.request_resolve_sftp_upload_conflict(transfer_id, choice, apply_to_batch)
            }
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    pub(in crate::app) fn request_cancel_sftp_transfer(&self, transfer_id: Uuid) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_cancel_sftp_transfer(transfer_id),
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    pub(in crate::app) fn request_pause_sftp_transfer(&self, transfer_id: Uuid) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_pause_sftp_transfer(transfer_id),
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    pub(in crate::app) fn request_resume_sftp_transfer(&self, transfer_id: Uuid) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_resume_sftp_transfer(transfer_id),
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    pub(in crate::app) fn request_sftp_write(
        &self,
        operation_id: Uuid,
        operation: ax_ssh::sftp::SftpWriteOperation,
    ) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_sftp_write(operation_id, operation),
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    pub(in crate::app) async fn shutdown(self) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.shutdown().await,
            Self::Telnet(worker) => worker.shutdown().await,
            Self::Serial(worker) => worker.shutdown().await,
            Self::Local(worker) => worker.shutdown().await,
        }
    }
}
