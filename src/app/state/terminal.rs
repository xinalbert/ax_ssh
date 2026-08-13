use super::*;

impl TerminalTabState {
    pub(in crate::app) const MAX_RECONNECT_ATTEMPTS: u8 = 5;

    pub(in crate::app) fn begin_reconnect(&mut self) -> Option<(u64, u8)> {
        if !self.reconnect_enabled || self.reconnecting || self.worker.is_some() {
            return None;
        }
        let attempt = self.reconnect_attempt.checked_add(1)?;
        if attempt > Self::MAX_RECONNECT_ATTEMPTS {
            return None;
        }
        self.reconnect_attempt = attempt;
        self.reconnecting = true;
        Some((self.reconnect_generation, attempt))
    }

    pub(in crate::app) fn reconnect_current(&self, generation: u64) -> bool {
        self.reconnect_enabled && self.reconnect_generation == generation && self.reconnecting
    }

    pub(in crate::app) fn finish_reconnect_attempt(&mut self, generation: u64) -> bool {
        if self.reconnect_generation != generation {
            return false;
        }
        self.reconnecting = false;
        true
    }

    pub(in crate::app) fn mark_reconnect_connected(&mut self, generation: u64) -> bool {
        if self.reconnect_generation != generation {
            return false;
        }
        self.reconnect_attempt = 0;
        self.reconnecting = false;
        self.reconnect_enabled = true;
        true
    }

    pub(in crate::app) fn cancel_reconnect(&mut self) {
        self.reconnect_generation = self.reconnect_generation.wrapping_add(1);
        self.reconnecting = false;
        self.reconnect_enabled = false;
    }

    pub(in crate::app) fn enable_reconnect(&mut self) {
        self.reconnect_enabled = true;
    }

    pub(in crate::app) fn reconnect_generation(&self) -> u64 {
        self.reconnect_generation
    }

    pub(in crate::app) fn profile_id(&self) -> Option<Uuid> {
        match self.backend {
            TerminalBackend::Ssh { profile_id, .. }
            | TerminalBackend::Sftp { profile_id, .. }
            | TerminalBackend::Telnet { profile_id, .. }
            | TerminalBackend::Serial { profile_id, .. } => Some(profile_id),
            TerminalBackend::Local => None,
        }
    }

    pub(in crate::app) fn ssh_route(&self) -> Option<(Uuid, Option<Uuid>)> {
        match self.backend {
            TerminalBackend::Ssh {
                profile_id,
                attempt_id,
            }
            | TerminalBackend::Sftp {
                profile_id,
                attempt_id,
            } => Some((profile_id, attempt_id)),
            TerminalBackend::Telnet { .. }
            | TerminalBackend::Serial { .. }
            | TerminalBackend::Local => None,
        }
    }

    pub(in crate::app) fn telnet_route(&self) -> Option<(Uuid, Option<Uuid>)> {
        match self.backend {
            TerminalBackend::Telnet {
                profile_id,
                attempt_id,
            } => Some((profile_id, attempt_id)),
            TerminalBackend::Ssh { .. }
            | TerminalBackend::Sftp { .. }
            | TerminalBackend::Serial { .. }
            | TerminalBackend::Local => None,
        }
    }

    pub(in crate::app) fn serial_route(&self) -> Option<(Uuid, Option<Uuid>)> {
        match self.backend {
            TerminalBackend::Serial {
                profile_id,
                attempt_id,
            } => Some((profile_id, attempt_id)),
            TerminalBackend::Ssh { .. }
            | TerminalBackend::Sftp { .. }
            | TerminalBackend::Telnet { .. }
            | TerminalBackend::Local => None,
        }
    }

    pub(in crate::app) fn set_ssh_attempt(&mut self, attempt_id: Option<Uuid>) -> bool {
        match &mut self.backend {
            TerminalBackend::Ssh {
                attempt_id: current,
                ..
            }
            | TerminalBackend::Sftp {
                attempt_id: current,
                ..
            } => {
                *current = attempt_id;
                true
            }
            TerminalBackend::Telnet { .. }
            | TerminalBackend::Serial { .. }
            | TerminalBackend::Local => false,
        }
    }

    pub(in crate::app) fn set_telnet_attempt(&mut self, attempt_id: Option<Uuid>) -> bool {
        match &mut self.backend {
            TerminalBackend::Telnet {
                attempt_id: current,
                ..
            } => {
                *current = attempt_id;
                true
            }
            TerminalBackend::Ssh { .. }
            | TerminalBackend::Sftp { .. }
            | TerminalBackend::Serial { .. }
            | TerminalBackend::Local => false,
        }
    }

    pub(in crate::app) fn set_serial_attempt(&mut self, attempt_id: Option<Uuid>) -> bool {
        match &mut self.backend {
            TerminalBackend::Serial {
                attempt_id: current,
                ..
            } => {
                *current = attempt_id;
                true
            }
            TerminalBackend::Ssh { .. }
            | TerminalBackend::Sftp { .. }
            | TerminalBackend::Telnet { .. }
            | TerminalBackend::Local => false,
        }
    }

    pub(in crate::app) fn is_local(&self) -> bool {
        matches!(self.backend, TerminalBackend::Local)
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
        if matches!(self.backend, TerminalBackend::Sftp { .. }) {
            ConnectionTarget::Sftp
        } else {
            ConnectionTarget::Terminal
        }
    }

    pub(in crate::app) fn is_sftp(&self) -> bool {
        matches!(self.backend, TerminalBackend::Sftp { .. })
    }
}

impl TerminalBackend {
    pub(super) fn kind(&self) -> &'static str {
        match self {
            Self::Sftp { .. } => "sftp",
            Self::Ssh { .. } | Self::Telnet { .. } | Self::Serial { .. } | Self::Local => {
                "terminal"
            }
        }
    }
}

impl TerminalWorker {
    pub(in crate::app) fn request_disconnect(&self) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_disconnect(),
            Self::Telnet(worker) => worker.request_disconnect(),
            Self::Serial(worker) => worker.request_disconnect(),
            Self::Local(worker) => worker.request_disconnect(),
        }
    }

    pub(in crate::app) fn request_send(&self, data: Vec<u8>) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_send(data),
            Self::Telnet(worker) => worker.request_send(data),
            Self::Serial(worker) => worker.request_send(data),
            Self::Local(worker) => worker.request_send(data),
        }
    }

    pub(in crate::app) fn request_resize(&self, columns: u32, rows: u32) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_resize(columns, rows),
            Self::Telnet(worker) => worker.request_resize(columns, rows),
            Self::Serial(_) => Ok(()),
            Self::Local(worker) => worker.request_resize(columns, rows),
        }
    }

    pub(in crate::app) fn request_list_sftp(&self, path: String) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_list_sftp(path),
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    pub(in crate::app) fn request_load_more_sftp(&self) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_load_more_sftp(),
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    pub(in crate::app) fn request_close_sftp(&self) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_close_sftp(),
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
    ) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_open_sftp_file(transfer_id, path, local_directory),
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    pub(in crate::app) fn request_open_sftp_upload(
        &self,
        transfer_id: Uuid,
        path: String,
        data: Vec<u8>,
    ) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_open_sftp_upload(transfer_id, path, data),
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
