//! Application state, workspace tabs, and connection-attempt transitions.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result};
use tokio::sync::oneshot;
use tracing::error;
use uuid::Uuid;

use ax_ssh::config::{
    ConfigStore, SessionProfile, SessionStore, X11ForwardingMode, normalize_group_name,
};
pub(super) use ax_ssh::controller::ConnectionIdentity as TerminalBackend;
use ax_ssh::controller::ReconnectState;
use ax_ssh::local_shell::LocalShellHandle;
use ax_ssh::serial::{SerialPortDescriptor, SerialSessionHandle};
use ax_ssh::sftp::SftpEntry;
use ax_ssh::ssh::SshSessionHandle;
use ax_ssh::telnet::TelnetSessionHandle;
use ax_ssh::terminal::{
    ClipboardLoadFormatter, TerminalModel, TerminalQueryPalette, TerminalSnapshot,
};

use super::local_files::{LocalDirectoryEntry, default_local_directory};

mod editor;
mod latency;
use latency::{ConnectionLatency, ConnectionLatencySnapshot};
mod sftp;
mod system_detection;
mod tabs;
pub(super) mod terminal;

pub(super) struct AppState {
    pub(super) config: ConfigStore,
    pub(super) sessions: SessionStore,
    tabs: Vec<WorkspaceTab>,
    active_tab_id: Option<Uuid>,
    terminal_numbers: HashMap<Uuid, u32>,
    profile_mutations: HashMap<Uuid, Uuid>,
    system_detections: HashMap<Uuid, ax_ssh::config::SystemDetection>,
    system_release_probes: HashMap<Uuid, (Uuid, Uuid)>,
    pub(super) persistence_coordinator: Arc<PersistenceCoordinator>,
    pub(super) worker_shutdowns: super::worker_shutdown::WorkerShutdowns,
    local_terminal_number: u32,
    pub(super) session_filter: String,
    serial_ports: Vec<SerialPortDescriptor>,
    terminal_query_palette: TerminalQueryPalette,
    ui_refresh: UiRefreshState,
}

/// Serializes filesystem-backed configuration, credential, and trust mutations.
///
/// Per-profile tokens reject stale profile work; this process-wide gate keeps
/// whole-store configuration writes from overwriting one another.
#[derive(Default)]
pub(super) struct PersistenceCoordinator {
    pub(super) gate: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Default)]
struct UiRefreshState {
    pending: bool,
    in_progress: bool,
    generation: u64,
    full: bool,
    tabs: bool,
    terminal_ids: HashSet<Uuid>,
    earliest_output_received_at: Option<Instant>,
    coalesced_requests: u64,
}

pub(super) struct UiRefreshBatch {
    pub(super) generation: u64,
    pub(super) full: bool,
    pub(super) tabs: bool,
    pub(super) terminal_ids: HashSet<Uuid>,
    pub(super) earliest_output_received_at: Option<Instant>,
    pub(super) coalesced_requests: u64,
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
            Self::Terminal(terminal) => terminal.kind(),
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
    default_title: String,
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
    pub(super) media: super::terminal_media::MediaState,
    pub(super) backend: TerminalBackend,
    pub(super) worker: Option<TerminalWorker>,
    pub(super) terminal: Option<TerminalModel>,
    viewport: ax_ssh::controller::TerminalViewport,
    pub(super) status: String,
    pub(super) connected: bool,
    pub(super) connection_latency: ConnectionLatency,
    pub(super) system_release: Option<String>,
    pub(super) worker_running: bool,
    pub(super) sftp: SftpBrowserState,
    pub(super) sftp_initial_path: Option<String>,
    pub(super) ssh_phase: SshConnectionPhase,
    reconnect: ReconnectState,
    pending_auth_secret: Option<zeroize::Zeroizing<String>>,
    pending_clipboard_read: Option<PendingClipboardRead>,
    next_clipboard_read_token: u64,
}

struct PendingClipboardRead {
    formatter: ClipboardLoadFormatter,
    token: u64,
    generation: u64,
}

const SFTP_HISTORY_LIMIT: usize = 128;
// A recursively selected directory can contribute up to 512 individual files.
// Keep every live row addressable until it reaches a terminal state.
const SFTP_TRANSFER_HISTORY_LIMIT: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SftpNavigation {
    Direct,
    Back,
    Forward,
    Up,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SftpSortColumn {
    Name,
    Size,
    Modified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SftpSortState {
    pub(super) column: SftpSortColumn,
    pub(super) descending: bool,
}

impl Default for SftpSortState {
    fn default() -> Self {
        Self {
            column: SftpSortColumn::Modified,
            descending: true,
        }
    }
}

impl SftpSortState {
    pub(super) fn toggle_column(&mut self, column: &str) -> bool {
        let Some(column) = (match column {
            "name" => Some(SftpSortColumn::Name),
            "size" => Some(SftpSortColumn::Size),
            "modified" => Some(SftpSortColumn::Modified),
            _ => None,
        }) else {
            return false;
        };
        if self.column == column {
            self.descending = !self.descending;
        } else {
            self.column = column;
            self.descending = !matches!(column, SftpSortColumn::Name);
        }
        true
    }

    pub(super) fn column_name(self) -> &'static str {
        match self.column {
            SftpSortColumn::Name => "name",
            SftpSortColumn::Size => "size",
            SftpSortColumn::Modified => "modified",
        }
    }
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
    pub(super) request_id: u64,
    pub(super) selected: HashSet<String>,
    pub(super) sort: SftpSortState,
    back_history: VecDeque<String>,
    forward_history: VecDeque<String>,
    pending_navigation: Option<PendingSftpNavigation>,
    pending_auto_refresh: Option<String>,
    pub(super) local: LocalDirectoryState,
    pub(super) transfers: VecDeque<SftpTransferState>,
    pub(super) upload_conflicts: VecDeque<PendingUploadConflict>,
    pub(super) selected_transfers: HashSet<Uuid>,
    pub(super) rename_name: String,
    pub(super) external_edits: super::remote_edit::RemoteEdits,
}

#[derive(Default)]
pub(super) struct LocalDirectoryState {
    pub(super) loaded: bool,
    pub(super) loading: bool,
    watcher_running: bool,
    pub(super) path: String,
    pub(super) entries: Vec<LocalDirectoryEntry>,
    pub(super) has_more: bool,
    pub(super) truncated: bool,
    pub(super) skipped_entries: usize,
    pub(super) status: String,
    pub(super) request_id: u64,
    pub(super) selected: HashSet<String>,
    pub(super) sort: SftpSortState,
    back_history: VecDeque<String>,
    forward_history: VecDeque<String>,
    pending_navigation: Option<PendingSftpNavigation>,
    pending_auto_refresh: Option<String>,
    pending_entries: VecDeque<LocalDirectoryEntry>,
}

#[derive(Clone, Default)]
pub(super) struct SftpBrowserSnapshot {
    pub(super) available: bool,
    pub(super) open: bool,
    pub(super) loading: bool,
    pub(super) upload_ready: bool,
    pub(super) home: String,
    pub(super) path: String,
    pub(super) entries: Vec<SftpEntry>,
    pub(super) sort: SftpSortState,
    pub(super) has_more: bool,
    pub(super) truncated: bool,
    pub(super) status: String,
    pub(super) can_go_back: bool,
    pub(super) can_go_forward: bool,
    pub(super) can_go_up: bool,
    pub(super) selected_count: usize,
    pub(super) all_selected: bool,
    pub(super) selected: HashSet<String>,
    pub(super) local: LocalDirectorySnapshot,
    pub(super) transfers: Vec<SftpTransferSnapshot>,
    pub(super) upload_conflict: Option<PendingUploadConflict>,
    pub(super) transfer_selected_active_count: usize,
    pub(super) transfer_selected_pausable_count: usize,
    pub(super) transfer_selected_resumable_count: usize,
    pub(super) editor_path: Option<String>,
    pub(super) editor_local_path: String,
    pub(super) editor_status: String,
    pub(super) editor_can_upload: bool,
    pub(super) rename_name: String,
    pub(super) editor_auto_upload: bool,
}

#[derive(Clone, Default)]
pub(super) struct LocalDirectorySnapshot {
    pub(super) loaded: bool,
    pub(super) loading: bool,
    pub(super) upload_selection_ready: bool,
    pub(super) path: String,
    pub(super) entries: Vec<LocalDirectoryEntry>,
    pub(super) sort: SftpSortState,
    pub(super) has_more: bool,
    pub(super) truncated: bool,
    pub(super) status: String,
    pub(super) can_go_back: bool,
    pub(super) can_go_forward: bool,
    pub(super) can_go_up: bool,
    pub(super) selected_count: usize,
    pub(super) all_selected: bool,
    pub(super) selected: HashSet<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SftpTransferDirection {
    Download,
    Upload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SftpTransferPhase {
    Queued,
    AwaitingConflict,
    Skipped,
    Downloading,
    Uploading,
    Pausing,
    Paused,
    Resuming,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingUploadConflict {
    pub(super) transfer_id: Uuid,
    pub(super) batch_id: Uuid,
    pub(super) name: String,
    pub(super) remote_size: Option<u64>,
    pub(super) remote_modified: Option<u32>,
}

pub(super) struct SftpTransferState {
    pub(super) id: Uuid,
    pub(super) name: String,
    pub(super) direction: SftpTransferDirection,
    pub(super) phase: SftpTransferPhase,
    pub(super) pausable: bool,
    pub(super) downloaded_bytes: u64,
    pub(super) total_bytes: u64,
    pub(super) bytes_per_second: u64,
    started_at: Option<Instant>,
    pub(super) status: String,
    local_path: Option<PathBuf>,
    remote_path: Option<String>,
}

#[derive(Clone)]
pub(super) struct SftpTransferSnapshot {
    pub(super) id: Uuid,
    pub(super) name: String,
    pub(super) direction: SftpTransferDirection,
    pub(super) phase: SftpTransferPhase,
    pub(super) pausable: bool,
    pub(super) downloaded_bytes: u64,
    pub(super) total_bytes: u64,
    pub(super) bytes_per_second: u64,
    pub(super) status: String,
    pub(super) selected: bool,
    pub(super) has_local_path: bool,
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
    pub(super) latency: ConnectionLatencySnapshot,
    pub(super) local: bool,
    pub(super) system_icon: Option<String>,
    pub(super) system_release: Option<String>,
}

pub(super) struct ActiveTabSnapshot {
    pub(super) media: super::terminal_media::MediaSnapshot,
    pub(super) id: Option<Uuid>,
    pub(super) profile_id: Option<Uuid>,
    pub(super) kind: &'static str,
    pub(super) title: String,
    pub(super) status: String,
    pub(super) notice: TerminalNoticeSnapshot,
    pub(super) editor: Option<SessionEditorSnapshot>,
    pub(super) terminal: Option<TerminalSnapshot>,
    pub(super) connected: bool,
    pub(super) selection_revision: i32,
    pub(super) sftp: SftpBrowserSnapshot,
    pub(super) security_prompt: ActiveSecurityPrompt,
}

impl Default for ActiveTabSnapshot {
    fn default() -> Self {
        Self {
            media: super::terminal_media::MediaSnapshot::default(),
            id: None,
            profile_id: None,
            kind: "empty",
            title: "Workspace".to_owned(),
            status: String::new(),
            notice: TerminalNoticeSnapshot::default(),
            editor: None,
            terminal: None,
            connected: false,
            selection_revision: 0,
            sftp: SftpBrowserSnapshot::default(),
            security_prompt: ActiveSecurityPrompt::None,
        }
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) enum TerminalNoticeKind {
    #[default]
    None,
    ClipboardRead,
    Reconnecting,
    Ended,
    Failed,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub(super) struct TerminalNoticeSnapshot {
    pub(super) visible: bool,
    pub(super) severity: &'static str,
    pub(super) kind: TerminalNoticeKind,
    pub(super) message: String,
    pub(super) primary_action: &'static str,
    pub(super) restart: bool,
    pub(super) secondary_action: &'static str,
}

impl TerminalNoticeSnapshot {
    pub(super) fn osc52_clipboard_read() -> Self {
        Self {
            visible: true,
            severity: "warning",
            kind: TerminalNoticeKind::ClipboardRead,
            message: String::new(),
            primary_action: "allow-osc52-clipboard-read",
            restart: false,
            secondary_action: "deny-osc52-clipboard-read",
        }
    }

    pub(super) fn reconnecting(message: &str) -> Self {
        Self {
            visible: true,
            severity: "warning",
            kind: TerminalNoticeKind::Reconnecting,
            message: message.to_owned(),
            primary_action: "",
            restart: false,
            secondary_action: "close-tab",
        }
    }

    pub(super) fn ended(message: &str, restart: bool) -> Self {
        Self {
            visible: true,
            severity: "warning",
            kind: TerminalNoticeKind::Ended,
            message: message.to_owned(),
            primary_action: "retry",
            restart,
            secondary_action: "close-tab",
        }
    }

    pub(super) fn failed(message: &str, restart: bool) -> Self {
        Self {
            visible: true,
            severity: "error",
            kind: TerminalNoticeKind::Failed,
            message: message
                .strip_prefix("Connection failed: ")
                .unwrap_or(message)
                .to_owned(),
            primary_action: "retry",
            restart,
            secondary_action: "close-tab",
        }
    }
}

pub(super) struct SessionEditorSnapshot {
    pub(super) draft_id: Uuid,
    pub(super) profile_id: Option<Uuid>,
    pub(super) name: String,
    pub(super) group_name: String,
    pub(super) system_icon_override: String,
    pub(super) detected_system_release: String,
    pub(super) automatic_system_icon: String,
    pub(super) protocol: &'static str,
    pub(super) host: String,
    pub(super) port: String,
    pub(super) username: String,
    pub(super) auth_method: &'static str,
    pub(super) private_key_path: String,
    pub(super) sftp_remote_path: String,
    pub(super) sftp_local_path: String,
    pub(super) sftp_override_limits: bool,
    pub(super) sftp_per_tab_transfers: i32,
    pub(super) sftp_max_file_gib: i32,
    pub(super) sftp_rate_limit_mib_per_second: i32,
    pub(super) credential_storage: String,
    pub(super) default_credential_storage: String,
    pub(super) x11_forwarding_mode: String,
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
            system_icon_override: String::new(),
            detected_system_release: String::new(),
            automatic_system_icon: "server".into(),
            protocol: "SSH",
            host: String::new(),
            port: "22".to_owned(),
            username: String::new(),
            auth_method: "Password",
            private_key_path: String::new(),
            sftp_remote_path: "~".to_owned(),
            sftp_local_path: default_local_directory(),
            sftp_override_limits: false,
            sftp_per_tab_transfers: i32::from(ax_ssh::config::DEFAULT_SFTP_PER_TAB_TRANSFERS),
            sftp_max_file_gib: ax_ssh::config::DEFAULT_SFTP_MAX_FILE_GIB as i32,
            sftp_rate_limit_mib_per_second: 0,
            credential_storage: String::new(),
            default_credential_storage: "encrypted-vault".to_owned(),
            x11_forwarding_mode: X11ForwardingMode::default().as_setting().to_owned(),
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
    pub(super) public_key: Option<String>,
    pub(super) changed: bool,
    pub(super) revoked: bool,
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
    ConfirmingHostKey(PendingHostKey),
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
    set_credential_storage_while_loading,
};

#[cfg(test)]
mod tests;
