use super::*;

impl TerminalTabState {
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
    ) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_open_sftp_file(transfer_id, path),
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

    pub(in crate::app) async fn shutdown(self) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.shutdown().await,
            Self::Telnet(worker) => worker.shutdown().await,
            Self::Serial(worker) => worker.shutdown().await,
            Self::Local(worker) => worker.shutdown().await,
        }
    }
}
