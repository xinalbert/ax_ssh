//! Application state, workspace tabs, and connection-attempt transitions.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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

pub(super) struct AppState {
    pub(super) config: ConfigStore,
    pub(super) sessions: SessionStore,
    tabs: Vec<WorkspaceTab>,
    active_tab_id: Option<Uuid>,
    terminal_numbers: HashMap<Uuid, u32>,
    local_terminal_number: u32,
    serial_ports: Vec<SerialPortDescriptor>,
}

impl AppState {
    pub(super) fn new(config: ConfigStore, sessions: SessionStore) -> Self {
        Self {
            config,
            sessions,
            tabs: Vec::new(),
            active_tab_id: None,
            terminal_numbers: HashMap::new(),
            local_terminal_number: 0,
            serial_ports: Vec::new(),
        }
    }

    pub(super) fn open_settings_tab(&mut self) -> Uuid {
        self.open_singleton_tab(WorkspaceTabKind::Settings, "Settings")
    }

    pub(super) fn open_session_editor_tab(&mut self) -> Uuid {
        self.open_session_editor(SessionEditorState::default())
    }

    pub(super) fn open_session_editor_for_group(&mut self, group_name: &str) -> Uuid {
        self.open_session_editor(SessionEditorState {
            draft_id: Uuid::new_v4(),
            group_name: normalize_group_name(group_name),
            profile_id: None,
        })
    }

    pub(super) fn open_session_editor_for_profile(&mut self, profile_id: Uuid) -> bool {
        if !self
            .sessions
            .sessions
            .iter()
            .any(|profile| profile.id == profile_id)
        {
            return false;
        }
        self.open_session_editor(SessionEditorState {
            draft_id: Uuid::new_v4(),
            profile_id: Some(profile_id),
            group_name: String::new(),
        });
        true
    }

    fn open_session_editor(&mut self, editor: SessionEditorState) -> Uuid {
        let title = editor
            .profile_id
            .and_then(|profile_id| {
                self.sessions
                    .sessions
                    .iter()
                    .find(|profile| profile.id == profile_id)
                    .map(|profile| format!("Edit {}", profile.name))
            })
            .unwrap_or_else(|| "New session".to_owned());
        if let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|tab| matches!(tab.kind, WorkspaceTabKind::SessionEditor(_)))
        {
            tab.title = title;
            tab.kind = WorkspaceTabKind::SessionEditor(editor);
            self.active_tab_id = Some(tab.id);
            return tab.id;
        }
        let id = Uuid::new_v4();
        self.tabs.push(WorkspaceTab {
            id,
            title,
            kind: WorkspaceTabKind::SessionEditor(editor),
            companion_tab_id: None,
        });
        self.active_tab_id = Some(id);
        id
    }

    fn open_singleton_tab(&mut self, kind: WorkspaceTabKind, title: &str) -> Uuid {
        if let Some(tab) = self.tabs.iter().find(|tab| tab.kind.same_page(&kind)) {
            self.active_tab_id = Some(tab.id);
            return tab.id;
        }
        let id = Uuid::new_v4();
        self.tabs.push(WorkspaceTab {
            id,
            title: title.to_owned(),
            kind,
            companion_tab_id: None,
        });
        self.active_tab_id = Some(id);
        id
    }

    #[cfg(test)]
    pub(super) fn open_terminal_tab(&mut self, profile: &SessionProfile) -> Uuid {
        self.open_terminal_tab_with_companion(profile, None)
    }

    pub(super) fn open_terminal_tab_with_companion(
        &mut self,
        profile: &SessionProfile,
        companion_tab_id: Option<Uuid>,
    ) -> Uuid {
        let number = self.terminal_numbers.entry(profile.id).or_default();
        *number = number.saturating_add(1);
        let id = Uuid::new_v4();
        let terminal = TerminalModel::new(
            usize::from(self.sessions.settings.terminal.default_columns),
            usize::from(self.sessions.settings.terminal.default_rows),
            self.sessions.settings.terminal.scrollback_lines as usize,
        );
        let backend = match profile.connection {
            ax_ssh::config::ConnectionProfile::Ssh(_) => TerminalBackend::Ssh {
                profile_id: profile.id,
                attempt_id: None,
            },
            ax_ssh::config::ConnectionProfile::Telnet(_) => TerminalBackend::Telnet {
                profile_id: profile.id,
                attempt_id: None,
            },
            ax_ssh::config::ConnectionProfile::Serial(_) => TerminalBackend::Serial {
                profile_id: profile.id,
                attempt_id: None,
            },
        };
        let tab = WorkspaceTab {
            id,
            title: format!("{} #{}", profile.name, number),
            kind: WorkspaceTabKind::Terminal(TerminalTabState {
                backend,
                worker: None,
                terminal,
                status: "Preparing connection...".to_owned(),
                connected: false,
                worker_running: false,
                sftp: SftpBrowserState::default(),
                ssh_phase: SshConnectionPhase::Idle,
            }),
            companion_tab_id: None,
        };
        self.insert_connection_tab(
            tab,
            profile.id,
            ConnectionTarget::Terminal,
            companion_tab_id,
        )
    }

    #[cfg(test)]
    pub(super) fn open_sftp_tab(&mut self, profile: &SessionProfile) -> Uuid {
        self.open_sftp_tab_with_companion(profile, None)
    }

    pub(super) fn open_sftp_tab_with_companion(
        &mut self,
        profile: &SessionProfile,
        companion_tab_id: Option<Uuid>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        let terminal = TerminalModel::new(
            usize::from(self.sessions.settings.terminal.default_columns),
            usize::from(self.sessions.settings.terminal.default_rows),
            self.sessions.settings.terminal.scrollback_lines as usize,
        );
        let tab = WorkspaceTab {
            id,
            title: format!("{} SFTP", profile.name),
            kind: WorkspaceTabKind::Terminal(TerminalTabState {
                backend: TerminalBackend::Sftp {
                    profile_id: profile.id,
                    attempt_id: None,
                },
                worker: None,
                terminal,
                status: "Preparing SFTP connection...".to_owned(),
                connected: false,
                worker_running: false,
                sftp: SftpBrowserState::for_standalone_tab(),
                ssh_phase: SshConnectionPhase::Idle,
            }),
            companion_tab_id: None,
        };
        self.insert_connection_tab(tab, profile.id, ConnectionTarget::Sftp, companion_tab_id)
    }

    pub(super) fn open_local_shell_tab(&mut self) -> Uuid {
        self.local_terminal_number = self.local_terminal_number.saturating_add(1);
        let id = Uuid::new_v4();
        let terminal = TerminalModel::new(
            usize::from(self.sessions.settings.terminal.default_columns),
            usize::from(self.sessions.settings.terminal.default_rows),
            self.sessions.settings.terminal.scrollback_lines as usize,
        );
        self.tabs.push(WorkspaceTab {
            id,
            title: format!("Local Shell #{}", self.local_terminal_number),
            kind: WorkspaceTabKind::Terminal(TerminalTabState {
                backend: TerminalBackend::Local,
                worker: None,
                terminal,
                status: "Starting local shell...".to_owned(),
                connected: false,
                worker_running: true,
                sftp: SftpBrowserState::default(),
                ssh_phase: SshConnectionPhase::Idle,
            }),
            companion_tab_id: None,
        });
        self.active_tab_id = Some(id);
        id
    }

    pub(super) fn activate_tab(&mut self, tab_id: Uuid) -> bool {
        if self.tabs.iter().any(|tab| tab.id == tab_id) {
            self.active_tab_id = Some(tab_id);
            true
        } else {
            false
        }
    }

    pub(super) fn switch_ssh_sftp_tab(&mut self) -> Option<SshSftpNavigation> {
        let active_tab_id = self.active_tab_id?;
        let (profile_id, current_target, companion_tab_id) = self
            .tabs
            .iter()
            .find(|tab| tab.id == active_tab_id)
            .and_then(WorkspaceTab::ssh_connection_target)?;
        let target = current_target.opposite();
        if let Some(companion_tab_id) = companion_tab_id {
            let companion_matches = self.tabs.iter().any(|tab| {
                tab.id == companion_tab_id
                    && tab.ssh_connection_target().is_some_and(
                        |(candidate_profile_id, candidate_target, _)| {
                            candidate_profile_id == profile_id && candidate_target == target
                        },
                    )
            });
            if companion_matches {
                self.active_tab_id = Some(companion_tab_id);
                return Some(SshSftpNavigation::Activated(companion_tab_id));
            }
            self.unlink_companion(active_tab_id);
        }
        Some(SshSftpNavigation::Connect {
            profile_id,
            target,
            companion_tab_id: active_tab_id,
        })
    }

    pub(super) fn move_tab(&mut self, tab_id: Uuid, target_index: usize) -> bool {
        let Some(source_index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return false;
        };
        let target_index = target_index.min(self.tabs.len().saturating_sub(1));
        if source_index == target_index {
            return true;
        }
        let tab = self.tabs.remove(source_index);
        self.tabs.insert(target_index, tab);
        true
    }

    pub(super) fn close_tab(&mut self, tab_id: Uuid) -> Option<ClosedTab> {
        let index = self.tabs.iter().position(|tab| tab.id == tab_id)?;
        let mut tab = self.tabs.remove(index);
        if let Some(companion_tab_id) = tab.companion_tab_id.take()
            && let Some(companion) = self.tabs.iter_mut().find(|tab| tab.id == companion_tab_id)
            && companion.companion_tab_id == Some(tab_id)
        {
            companion.companion_tab_id = None;
        }
        let (worker, pending_probe) = match &mut tab.kind {
            WorkspaceTabKind::Terminal(terminal) => {
                (terminal.worker.take(), terminal.take_pending_probe())
            }
            WorkspaceTabKind::Settings | WorkspaceTabKind::SessionEditor(_) => (None, None),
        };
        if self.active_tab_id == Some(tab_id) {
            self.active_tab_id = self
                .tabs
                .get(index)
                .or_else(|| self.tabs.last())
                .map(|tab| tab.id);
        }
        Some(ClosedTab {
            worker,
            pending_probe,
        })
    }

    fn insert_connection_tab(
        &mut self,
        tab: WorkspaceTab,
        profile_id: Uuid,
        target: ConnectionTarget,
        companion_tab_id: Option<Uuid>,
    ) -> Uuid {
        let id = tab.id;
        let companion_tab_id = companion_tab_id.filter(|companion_tab_id| {
            self.tabs.iter().any(|candidate| {
                candidate.id == *companion_tab_id
                    && candidate.ssh_connection_target().is_some_and(
                        |(candidate_profile_id, candidate_target, _)| {
                            candidate_profile_id == profile_id
                                && candidate_target == target.opposite()
                        },
                    )
            })
        });
        let insert_index = companion_tab_id
            .and_then(|companion_tab_id| {
                self.tabs.iter().position(|tab| tab.id == companion_tab_id)
            })
            .map(|index| match target {
                ConnectionTarget::Terminal => index,
                ConnectionTarget::Sftp => index.saturating_add(1),
            })
            .unwrap_or(self.tabs.len());
        self.tabs.insert(insert_index, tab);
        if let Some(companion_tab_id) = companion_tab_id {
            self.link_companions(id, companion_tab_id);
        }
        self.active_tab_id = Some(id);
        id
    }

    fn link_companions(&mut self, first_tab_id: Uuid, second_tab_id: Uuid) {
        self.unlink_companion(first_tab_id);
        self.unlink_companion(second_tab_id);
        if let Some(first) = self.tabs.iter_mut().find(|tab| tab.id == first_tab_id) {
            first.companion_tab_id = Some(second_tab_id);
        }
        if let Some(second) = self.tabs.iter_mut().find(|tab| tab.id == second_tab_id) {
            second.companion_tab_id = Some(first_tab_id);
        }
    }

    fn unlink_companion(&mut self, tab_id: Uuid) {
        let companion_tab_id = self
            .tabs
            .iter_mut()
            .find(|tab| tab.id == tab_id)
            .and_then(|tab| tab.companion_tab_id.take());
        if let Some(companion_tab_id) = companion_tab_id
            && let Some(companion) = self.tabs.iter_mut().find(|tab| tab.id == companion_tab_id)
            && companion.companion_tab_id == Some(tab_id)
        {
            companion.companion_tab_id = None;
        }
    }

    pub(super) fn drain_runtime_resources(&mut self) -> (Vec<TerminalWorker>, Vec<PendingProbe>) {
        let mut workers = Vec::new();
        let mut pending_probes = Vec::new();
        for tab in &mut self.tabs {
            let WorkspaceTabKind::Terminal(terminal) = &mut tab.kind else {
                continue;
            };
            if let Some(worker) = terminal.worker.take() {
                workers.push(worker);
            }
            if let Some(probe) = terminal.take_pending_probe() {
                pending_probes.push(probe);
            }
        }
        (workers, pending_probes)
    }

    pub(super) fn tab_summaries(&self) -> Vec<WorkspaceTabSummary> {
        self.tabs
            .iter()
            .map(|tab| WorkspaceTabSummary {
                id: tab.id,
                title: tab.title.clone(),
                kind: tab.kind.name(),
                connected: matches!(
                    &tab.kind,
                    WorkspaceTabKind::Terminal(terminal) if terminal.connected
                ),
            })
            .collect()
    }

    pub(super) fn active_snapshot(&self) -> ActiveTabSnapshot {
        let Some(active_id) = self.active_tab_id else {
            return ActiveTabSnapshot::default();
        };
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == active_id) else {
            return ActiveTabSnapshot::default();
        };
        match &tab.kind {
            WorkspaceTabKind::Terminal(terminal) => {
                let mut sftp = terminal.sftp.snapshot(terminal.ssh_route().is_some());
                if terminal.is_sftp() && sftp.status.is_empty() {
                    sftp.status = terminal.status.clone();
                }
                ActiveTabSnapshot {
                    id: Some(tab.id),
                    kind: terminal.backend.kind(),
                    title: tab.title.clone(),
                    status: terminal.status.clone(),
                    editor: None,
                    terminal: Some(terminal.terminal.snapshot()),
                    connected: terminal.connected,
                    worker_running: terminal.worker_running,
                    sftp,
                    security_prompt: self.active_security_prompt(),
                }
            }
            WorkspaceTabKind::Settings => ActiveTabSnapshot {
                id: Some(tab.id),
                kind: "settings",
                title: tab.title.clone(),
                ..ActiveTabSnapshot::default()
            },
            WorkspaceTabKind::SessionEditor(editor) => {
                let editor = editor.snapshot(&self.sessions);
                ActiveTabSnapshot {
                    id: Some(tab.id),
                    kind: "session-editor",
                    title: tab.title.clone(),
                    editor: Some(editor),
                    ..ActiveTabSnapshot::default()
                }
            }
        }
    }

    pub(super) fn active_tab_id(&self) -> Option<Uuid> {
        self.active_tab_id
    }

    pub(super) fn active_editor_profile_id(&self) -> Option<Option<Uuid>> {
        let active_id = self.active_tab_id?;
        let tab = self.tabs.iter().find(|tab| tab.id == active_id)?;
        let WorkspaceTabKind::SessionEditor(editor) = &tab.kind else {
            return None;
        };
        Some(editor.profile_id)
    }

    pub(super) fn replace_serial_ports(&mut self, ports: Vec<SerialPortDescriptor>) {
        self.serial_ports = ports;
    }

    pub(super) fn serial_ports(&self) -> &[SerialPortDescriptor] {
        &self.serial_ports
    }

    pub(super) fn active_terminal(&self) -> Option<&TerminalTabState> {
        self.active_tab_id.and_then(|id| self.terminal(id))
    }

    pub(super) fn active_terminal_mut(&mut self) -> Option<&mut TerminalTabState> {
        self.active_tab_id.and_then(|id| self.terminal_mut(id))
    }

    pub(super) fn resize_active_terminal(&mut self, columns: u32, rows: u32) -> Result<()> {
        let terminal = self.active_terminal_mut().context("no active terminal")?;
        if let Some(worker) = terminal.worker.as_ref() {
            worker.request_resize(columns, rows)?;
        }
        terminal.terminal.resize(columns as usize, rows as usize);
        Ok(())
    }

    pub(super) fn terminal(&self, tab_id: Uuid) -> Option<&TerminalTabState> {
        self.tabs.iter().find_map(|tab| {
            if tab.id != tab_id {
                return None;
            }
            match &tab.kind {
                WorkspaceTabKind::Terminal(terminal) => Some(terminal),
                WorkspaceTabKind::Settings | WorkspaceTabKind::SessionEditor(_) => None,
            }
        })
    }

    pub(super) fn terminal_mut(&mut self, tab_id: Uuid) -> Option<&mut TerminalTabState> {
        self.tabs.iter_mut().find_map(|tab| {
            if tab.id != tab_id {
                return None;
            }
            match &mut tab.kind {
                WorkspaceTabKind::Terminal(terminal) => Some(terminal),
                WorkspaceTabKind::Settings | WorkspaceTabKind::SessionEditor(_) => None,
            }
        })
    }

    pub(super) fn apply_scrollback_setting(&mut self) {
        let scrollback_lines = self.sessions.settings.terminal.scrollback_lines as usize;
        for tab in &mut self.tabs {
            if let WorkspaceTabKind::Terminal(terminal) = &mut tab.kind {
                terminal.terminal.set_scrollback_lines(scrollback_lines);
            }
        }
    }

    pub(super) fn active_security_prompt(&self) -> ActiveSecurityPrompt {
        let Some(tab_id) = self.active_tab_id else {
            return ActiveSecurityPrompt::None;
        };
        let Some(terminal) = self.terminal(tab_id) else {
            return ActiveSecurityPrompt::None;
        };
        let Some((profile_id, _)) = terminal.ssh_route() else {
            return ActiveSecurityPrompt::None;
        };
        let Some(profile) = self
            .sessions
            .sessions
            .iter()
            .find(|profile| profile.id == profile_id)
            .cloned()
        else {
            return ActiveSecurityPrompt::None;
        };
        match terminal.ssh_phase() {
            Some(SshConnectionPhase::AwaitingHostKey(prompt))
                if prompt.tab_id == tab_id && prompt.profile_id == profile_id =>
            {
                ActiveSecurityPrompt::HostKey(prompt.clone())
            }
            Some(SshConnectionPhase::AwaitingAuthentication { vault_unlock_only }) => {
                ActiveSecurityPrompt::Authentication {
                    tab_id,
                    profile,
                    vault_unlock_only: *vault_unlock_only,
                }
            }
            Some(
                SshConnectionPhase::Idle
                | SshConnectionPhase::Probing(_)
                | SshConnectionPhase::AwaitingHostKey(_)
                | SshConnectionPhase::LoadingStoredCredential,
            )
            | None => ActiveSecurityPrompt::None,
        }
    }
}

enum WorkspaceTabKind {
    Terminal(TerminalTabState),
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

impl SessionEditorState {
    fn snapshot(&self, sessions: &SessionStore) -> SessionEditorSnapshot {
        let Some(profile) = self.profile_id.and_then(|profile_id| {
            sessions
                .sessions
                .iter()
                .find(|profile| profile.id == profile_id)
        }) else {
            return SessionEditorSnapshot {
                draft_id: self.draft_id,
                group_name: self.group_name.clone(),
                ..SessionEditorSnapshot::default()
            };
        };
        let (
            protocol,
            host,
            port,
            username,
            auth_method,
            private_key_path,
            x11_forwarding,
            serial_port,
            serial_baud_rate,
            serial_data_bits,
            serial_stop_bits,
            serial_parity,
            serial_flow_control,
        ) = match &profile.connection {
            ax_ssh::config::ConnectionProfile::Ssh(config) => {
                let (auth_method, private_key_path) = match &config.auth {
                    ax_ssh::config::AuthMethod::Password => ("Password", String::new()),
                    ax_ssh::config::AuthMethod::PrivateKey { path } => {
                        ("Private key", path.to_string_lossy().into_owned())
                    }
                };
                (
                    "SSH",
                    config.host.clone(),
                    config.port.to_string(),
                    config.username.clone(),
                    auth_method,
                    private_key_path,
                    config.x11_forwarding,
                    String::new(),
                    "115200".to_owned(),
                    "8",
                    "1",
                    "none",
                    "none",
                )
            }
            ax_ssh::config::ConnectionProfile::Telnet(config) => (
                "Telnet",
                config.host.clone(),
                config.port.to_string(),
                String::new(),
                "Password",
                String::new(),
                false,
                String::new(),
                "115200".to_owned(),
                "8",
                "1",
                "none",
                "none",
            ),
            ax_ssh::config::ConnectionProfile::Serial(config) => (
                "Serial",
                String::new(),
                "23".to_owned(),
                String::new(),
                "Password",
                String::new(),
                false,
                config.port_name.clone(),
                config.baud_rate.to_string(),
                config.data_bits.as_setting(),
                config.stop_bits.as_setting(),
                config.parity.as_setting(),
                config.flow_control.as_setting(),
            ),
        };
        SessionEditorSnapshot {
            draft_id: self.draft_id,
            profile_id: Some(profile.id),
            name: profile.name.clone(),
            group_name: profile.group_name.clone(),
            protocol,
            host,
            port,
            username,
            auth_method,
            private_key_path,
            x11_forwarding,
            serial_port,
            serial_baud_rate,
            serial_data_bits,
            serial_stop_bits,
            serial_parity,
            serial_flow_control,
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
    pub(super) terminal: TerminalModel,
    pub(super) status: String,
    pub(super) connected: bool,
    pub(super) worker_running: bool,
    pub(super) sftp: SftpBrowserState,
    pub(super) ssh_phase: SshConnectionPhase,
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
    pub(super) local: LocalDirectoryState,
}

impl SftpBrowserState {
    fn for_standalone_tab() -> Self {
        Self {
            local: LocalDirectoryState {
                path: default_local_directory(),
                status: "Local directory ready".to_owned(),
                ..LocalDirectoryState::default()
            },
            ..Self::default()
        }
    }

    fn snapshot(&self, available: bool) -> SftpBrowserSnapshot {
        SftpBrowserSnapshot {
            available,
            open: self.open,
            loading: self.loading,
            home: self.home.clone(),
            path: self.path.clone(),
            entries: self.entries.clone(),
            has_more: self.has_more,
            truncated: self.truncated,
            status: self.status.clone(),
            local: self.local.snapshot(),
        }
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Default)]
pub(super) struct LocalDirectoryState {
    pub(super) loading: bool,
    pub(super) path: String,
    pub(super) entries: Vec<LocalDirectoryEntry>,
    pub(super) truncated: bool,
    pub(super) status: String,
    pub(super) request_id: u64,
}

impl LocalDirectoryState {
    pub(super) fn begin_load(&mut self, path: String) -> u64 {
        self.request_id = self.request_id.wrapping_add(1);
        self.path = path;
        self.loading = true;
        self.status = "Loading local directory...".to_owned();
        self.request_id
    }

    pub(super) fn complete(
        &mut self,
        path: String,
        entries: Vec<LocalDirectoryEntry>,
        truncated: bool,
    ) {
        self.loading = false;
        self.path = path;
        self.entries = entries;
        self.truncated = truncated;
        self.status = if truncated {
            "Local directory limit reached".to_owned()
        } else {
            format!("{} items", self.entries.len())
        };
    }

    pub(super) fn fail(&mut self, message: String) {
        self.loading = false;
        self.status = message;
    }

    fn snapshot(&self) -> LocalDirectorySnapshot {
        LocalDirectorySnapshot {
            loading: self.loading,
            path: self.path.clone(),
            entries: self.entries.clone(),
            truncated: self.truncated,
            status: self.status.clone(),
        }
    }
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
    pub(super) local: LocalDirectorySnapshot,
}

#[derive(Clone, Default)]
pub(super) struct LocalDirectorySnapshot {
    pub(super) loading: bool,
    pub(super) path: String,
    pub(super) entries: Vec<LocalDirectoryEntry>,
    pub(super) truncated: bool,
    pub(super) status: String,
}

impl TerminalTabState {
    pub(super) fn ssh_route(&self) -> Option<(Uuid, Option<Uuid>)> {
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

    pub(super) fn telnet_route(&self) -> Option<(Uuid, Option<Uuid>)> {
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

    pub(super) fn serial_route(&self) -> Option<(Uuid, Option<Uuid>)> {
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

    pub(super) fn set_ssh_attempt(&mut self, attempt_id: Option<Uuid>) -> bool {
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

    pub(super) fn set_telnet_attempt(&mut self, attempt_id: Option<Uuid>) -> bool {
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

    pub(super) fn set_serial_attempt(&mut self, attempt_id: Option<Uuid>) -> bool {
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

    pub(super) fn is_local(&self) -> bool {
        matches!(self.backend, TerminalBackend::Local)
    }

    pub(super) fn ssh_phase(&self) -> Option<&SshConnectionPhase> {
        matches!(
            self.backend,
            TerminalBackend::Ssh { .. } | TerminalBackend::Sftp { .. }
        )
        .then_some(&self.ssh_phase)
    }

    pub(super) fn set_ssh_phase(&mut self, phase: SshConnectionPhase) -> bool {
        if !matches!(
            self.backend,
            TerminalBackend::Ssh { .. } | TerminalBackend::Sftp { .. }
        ) {
            return false;
        }
        self.ssh_phase = phase;
        true
    }

    pub(super) fn take_pending_probe(&mut self) -> Option<PendingProbe> {
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

    pub(super) fn connection_target(&self) -> ConnectionTarget {
        if matches!(self.backend, TerminalBackend::Sftp { .. }) {
            ConnectionTarget::Sftp
        } else {
            ConnectionTarget::Terminal
        }
    }

    pub(super) fn is_sftp(&self) -> bool {
        matches!(self.backend, TerminalBackend::Sftp { .. })
    }
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

impl TerminalBackend {
    fn kind(&self) -> &'static str {
        match self {
            Self::Sftp { .. } => "sftp",
            Self::Ssh { .. } | Self::Telnet { .. } | Self::Serial { .. } | Self::Local => {
                "terminal"
            }
        }
    }
}

pub(super) enum TerminalWorker {
    Ssh(SshSessionHandle),
    Telnet(TelnetSessionHandle),
    Serial(SerialSessionHandle),
    Local(LocalShellHandle),
}

impl TerminalWorker {
    pub(super) fn request_disconnect(&self) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_disconnect(),
            Self::Telnet(worker) => worker.request_disconnect(),
            Self::Serial(worker) => worker.request_disconnect(),
            Self::Local(worker) => worker.request_disconnect(),
        }
    }

    pub(super) fn request_send(&self, data: Vec<u8>) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_send(data),
            Self::Telnet(worker) => worker.request_send(data),
            Self::Serial(worker) => worker.request_send(data),
            Self::Local(worker) => worker.request_send(data),
        }
    }

    pub(super) fn request_resize(&self, columns: u32, rows: u32) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_resize(columns, rows),
            Self::Telnet(worker) => worker.request_resize(columns, rows),
            Self::Serial(_) => Ok(()),
            Self::Local(worker) => worker.request_resize(columns, rows),
        }
    }

    pub(super) fn request_list_sftp(&self, path: String) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_list_sftp(path),
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    pub(super) fn request_load_more_sftp(&self) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_load_more_sftp(),
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    pub(super) fn request_close_sftp(&self) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_close_sftp(),
            Self::Telnet(_) | Self::Serial(_) | Self::Local(_) => {
                anyhow::bail!("SFTP is available only for SSH sessions")
            }
        }
    }

    pub(super) async fn shutdown(self) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.shutdown().await,
            Self::Telnet(worker) => worker.shutdown().await,
            Self::Serial(worker) => worker.shutdown().await,
            Self::Local(worker) => worker.shutdown().await,
        }
    }
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
    pub(super) worker: Option<TerminalWorker>,
    pub(super) pending_probe: Option<PendingProbe>,
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
