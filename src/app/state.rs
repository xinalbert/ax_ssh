//! Application state, workspace tabs, and connection-attempt transitions.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result};
use tokio::sync::oneshot;
use tracing::error;
use uuid::Uuid;

use ax_ssh::config::{ConfigStore, SessionProfile, SessionStore, normalize_group_name};
use ax_ssh::local_shell::LocalShellHandle;
use ax_ssh::serial::{SerialPortDescriptor, SerialSessionHandle};
use ax_ssh::sftp::SftpEntry;
use ax_ssh::ssh::SshSessionHandle;
use ax_ssh::telnet::TelnetSessionHandle;
use ax_ssh::terminal::{TerminalModel, TerminalSnapshot};

use super::local_files::{LocalDirectoryEntry, default_local_directory};

mod editor;
mod sftp;
mod tabs;
mod terminal;

pub(super) struct AppState {
    pub(super) config: ConfigStore,
    pub(super) sessions: SessionStore,
    tabs: Vec<WorkspaceTab>,
    active_tab_id: Option<Uuid>,
    terminal_numbers: HashMap<Uuid, u32>,
    profile_mutations: HashMap<Uuid, Uuid>,
    local_terminal_number: u32,
    serial_ports: Vec<SerialPortDescriptor>,
    ui_refresh_pending: AtomicBool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WorkspaceTransfer {
    pub(super) source_window_id: Uuid,
    pub(super) tab_ids: Vec<Uuid>,
    pub(super) active_tab_id: Option<Uuid>,
}

enum WorkspaceTabKind {
    Terminal(Box<TerminalTabState>),
    Settings,
    SessionEditor(SessionEditorState),
}

impl WorkspaceTabKind {
    fn same_page(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Settings, Self::Settings) | (Self::SessionEditor(_), Self::SessionEditor(_))
        )
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Terminal(terminal) => terminal.backend.kind(),
            Self::Settings => "settings",
            Self::SessionEditor(_) => "session-editor",
        }
    }
}

struct SessionEditorState {
    draft_id: Uuid,
    profile_id: Option<Uuid>,
    group_name: String,
}

impl Default for SessionEditorState {
    fn default() -> Self {
        Self {
            draft_id: Uuid::new_v4(),
            profile_id: None,
            group_name: String::new(),
        }
    }
}

struct WorkspaceTab {
    id: Uuid,
    title: String,
    kind: WorkspaceTabKind,
    companion_tab_id: Option<Uuid>,
}

impl WorkspaceTab {
    fn ssh_connection_target(&self) -> Option<(Uuid, ConnectionTarget, Option<Uuid>)> {
        let WorkspaceTabKind::Terminal(terminal) = &self.kind else {
            return None;
        };
        terminal.ssh_route().map(|(profile_id, _)| {
            (
                profile_id,
                terminal.connection_target(),
                self.companion_tab_id,
            )
        })
    }
}

pub(super) struct TerminalTabState {
    pub(super) backend: TerminalBackend,
    pub(super) worker: Option<TerminalWorker>,
    pub(super) terminal: Option<TerminalModel>,
    pub(super) status: String,
    pub(super) connected: bool,
    pub(super) worker_running: bool,
    pub(super) sftp: SftpBrowserState,
    pub(super) ssh_phase: SshConnectionPhase,
    pending_auth_secret: Option<zeroize::Zeroizing<String>>,
}

const SFTP_HISTORY_LIMIT: usize = 128;
const SFTP_TRANSFER_HISTORY_LIMIT: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SftpNavigation {
    Direct,
    Back,
    Forward,
}

struct PendingSftpNavigation {
    kind: SftpNavigation,
    from: String,
    requested: String,
}

#[derive(Default)]
pub(super) struct SftpBrowserState {
    pub(super) open: bool,
    pub(super) loading: bool,
    pub(super) home: String,
    pub(super) path: String,
    pub(super) entries: Vec<SftpEntry>,
    pub(super) has_more: bool,
    pub(super) truncated: bool,
    pub(super) status: String,
    pub(super) selected: HashSet<String>,
    back_history: VecDeque<String>,
    forward_history: VecDeque<String>,
    pending_navigation: Option<PendingSftpNavigation>,
    pub(super) local: LocalDirectoryState,
    pub(super) transfers: VecDeque<SftpTransferState>,
}

#[derive(Default)]
pub(super) struct LocalDirectoryState {
    pub(super) loading: bool,
    pub(super) path: String,
    pub(super) entries: Vec<LocalDirectoryEntry>,
    pub(super) truncated: bool,
    pub(super) status: String,
    pub(super) request_id: u64,
    pub(super) selected: HashSet<String>,
}

#[derive(Clone, Default)]
pub(super) struct SftpBrowserSnapshot {
    pub(super) available: bool,
    pub(super) open: bool,
    pub(super) loading: bool,
    pub(super) home: String,
    pub(super) path: String,
    pub(super) entries: Vec<SftpEntry>,
    pub(super) has_more: bool,
    pub(super) truncated: bool,
    pub(super) status: String,
    pub(super) can_go_back: bool,
    pub(super) can_go_forward: bool,
    pub(super) selected_count: usize,
    pub(super) all_selected: bool,
    pub(super) selected: HashSet<String>,
    pub(super) local: LocalDirectorySnapshot,
    pub(super) transfers: Vec<SftpTransferSnapshot>,
}

#[derive(Clone, Default)]
pub(super) struct LocalDirectorySnapshot {
    pub(super) loading: bool,
    pub(super) path: String,
    pub(super) entries: Vec<LocalDirectoryEntry>,
    pub(super) truncated: bool,
    pub(super) status: String,
    pub(super) selected_count: usize,
    pub(super) all_selected: bool,
    pub(super) selected: HashSet<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SftpTransferPhase {
    Queued,
    Downloading,
    Cancelling,
    Opening,
    Completed,
    Cancelled,
    Failed,
}

pub(super) struct SftpTransferState {
    pub(super) id: Uuid,
    pub(super) name: String,
    pub(super) phase: SftpTransferPhase,
    pub(super) downloaded_bytes: u64,
    pub(super) total_bytes: u64,
    pub(super) bytes_per_second: u64,
    started_at: Option<Instant>,
    pub(super) status: String,
}

#[derive(Clone)]
pub(super) struct SftpTransferSnapshot {
    pub(super) id: Uuid,
    pub(super) name: String,
    pub(super) phase: SftpTransferPhase,
    pub(super) downloaded_bytes: u64,
    pub(super) total_bytes: u64,
    pub(super) bytes_per_second: u64,
    pub(super) status: String,
}

pub(super) enum TerminalBackend {
    Ssh {
        profile_id: Uuid,
        attempt_id: Option<Uuid>,
    },
    Sftp {
        profile_id: Uuid,
        attempt_id: Option<Uuid>,
    },
    Telnet {
        profile_id: Uuid,
        attempt_id: Option<Uuid>,
    },
    Serial {
        profile_id: Uuid,
        attempt_id: Option<Uuid>,
    },
    Local,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PaneSessionSource {
    LocalShell,
    ProfileConnection(Uuid),
}

pub(super) enum TerminalWorker {
    Ssh(SshSessionHandle),
    Telnet(TelnetSessionHandle),
    Serial(SerialSessionHandle),
    Local(LocalShellHandle),
}

pub(super) struct WorkspaceTabSummary {
    pub(super) id: Uuid,
    pub(super) title: String,
    pub(super) kind: &'static str,
    pub(super) connected: bool,
}

pub(super) struct ActiveTabSnapshot {
    pub(super) id: Option<Uuid>,
    pub(super) kind: &'static str,
    pub(super) title: String,
    pub(super) status: String,
    pub(super) editor: Option<SessionEditorSnapshot>,
    pub(super) terminal: Option<TerminalSnapshot>,
    pub(super) connected: bool,
    pub(super) worker_running: bool,
    pub(super) sftp: SftpBrowserSnapshot,
    pub(super) security_prompt: ActiveSecurityPrompt,
}

impl Default for ActiveTabSnapshot {
    fn default() -> Self {
        Self {
            id: None,
            kind: "empty",
            title: "Workspace".to_owned(),
            status: String::new(),
            editor: None,
            terminal: None,
            connected: false,
            worker_running: false,
            sftp: SftpBrowserSnapshot::default(),
            security_prompt: ActiveSecurityPrompt::None,
        }
    }
}

pub(super) struct SessionEditorSnapshot {
    pub(super) draft_id: Uuid,
    pub(super) profile_id: Option<Uuid>,
    pub(super) name: String,
    pub(super) group_name: String,
    pub(super) protocol: &'static str,
    pub(super) host: String,
    pub(super) port: String,
    pub(super) username: String,
    pub(super) auth_method: &'static str,
    pub(super) private_key_path: String,
    pub(super) sftp_remote_path: String,
    pub(super) sftp_local_path: String,
    pub(super) credential_storage: String,
    pub(super) default_credential_storage: String,
    pub(super) x11_forwarding: bool,
    pub(super) serial_port: String,
    pub(super) serial_baud_rate: String,
    pub(super) serial_data_bits: &'static str,
    pub(super) serial_stop_bits: &'static str,
    pub(super) serial_parity: &'static str,
    pub(super) serial_flow_control: &'static str,
}

impl Default for SessionEditorSnapshot {
    fn default() -> Self {
        Self {
            draft_id: Uuid::nil(),
            profile_id: None,
            name: String::new(),
            group_name: String::new(),
            protocol: "SSH",
            host: String::new(),
            port: "22".to_owned(),
            username: String::new(),
            auth_method: "Password",
            private_key_path: String::new(),
            sftp_remote_path: "~".to_owned(),
            sftp_local_path: default_local_directory(),
            credential_storage: String::new(),
            default_credential_storage: "system-keyring".to_owned(),
            x11_forwarding: true,
            serial_port: String::new(),
            serial_baud_rate: "115200".to_owned(),
            serial_data_bits: "8",
            serial_stop_bits: "1",
            serial_parity: "none",
            serial_flow_control: "none",
        }
    }
}

pub(super) struct ClosedTab {
    pub(super) kind: ClosedTabKind,
    pub(super) worker: Option<TerminalWorker>,
    pub(super) pending_probe: Option<PendingProbe>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ClosedTabKind {
    Terminal { release_file_icon_cache: bool },
    Settings,
    SessionEditor,
}

#[derive(Clone)]
pub(super) struct PendingHostKey {
    pub(super) tab_id: Uuid,
    pub(super) profile_id: Uuid,
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) fingerprint: String,
    pub(super) changed: bool,
}

pub(super) struct PendingProbe {
    pub(super) tab_id: Uuid,
    pub(super) profile_id: Uuid,
    pub(super) cancel: oneshot::Sender<()>,
}

pub(super) enum SshConnectionPhase {
    Idle,
    Probing(PendingProbe),
    AwaitingHostKey(PendingHostKey),
    AwaitingAuthentication { vault_unlock_only: bool },
    LoadingStoredCredential,
}

pub(super) enum ActiveSecurityPrompt {
    None,
    HostKey(PendingHostKey),
    Authentication {
        tab_id: Uuid,
        profile: SessionProfile,
        vault_unlock_only: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConnectionTarget {
    Terminal,
    Sftp,
}

impl ConnectionTarget {
    const fn opposite(self) -> Self {
        match self {
            Self::Terminal => Self::Sftp,
            Self::Sftp => Self::Terminal,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SshSftpNavigation {
    Activated(Uuid),
    Connect {
        profile_id: Uuid,
        target: ConnectionTarget,
        companion_tab_id: Uuid,
    },
}

pub(super) enum ConnectionStart {
    Authenticate {
        tab_id: Uuid,
        profile: SessionProfile,
        target: ConnectionTarget,
    },
    Probe {
        tab_id: Uuid,
        profile: SessionProfile,
        cancelled: oneshot::Receiver<()>,
        target: ConnectionTarget,
    },
}

mod transitions;

pub(super) use self::transitions::{
    finish_stored_credential_retry, prepare_authentication_retry, prepare_host_key_retry,
    prepare_stored_credential_retry, retire_session_attempt, session_attempt_is_active,
    set_credential_storage, set_credential_storage_while_loading,
};

#[cfg(test)]
mod tests;
