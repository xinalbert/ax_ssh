//! Application state, workspace tabs, and connection-attempt transitions.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tokio::sync::oneshot;
use tracing::{error, warn};
use uuid::Uuid;

use ax_ssh::config::{ConfigStore, SessionProfile, SessionStore, normalize_group_name};
use ax_ssh::local_shell::LocalShellHandle;
use ax_ssh::ssh::SshSessionHandle;
use ax_ssh::terminal::{TerminalModel, TerminalSnapshot};

pub(super) struct AppState {
    pub(super) config: ConfigStore,
    pub(super) sessions: SessionStore,
    pub(super) pending_probe: Option<PendingProbe>,
    pub(super) pending_trust: Option<PendingHostKey>,
    pub(super) pending_auth: Option<PendingAuth>,
    pub(super) expanded_groups: BTreeSet<String>,
    tabs: Vec<WorkspaceTab>,
    active_tab_id: Option<Uuid>,
    terminal_numbers: HashMap<Uuid, u32>,
    local_terminal_number: u32,
}

impl AppState {
    pub(super) fn new(config: ConfigStore, sessions: SessionStore) -> Self {
        let expanded_groups = sessions
            .groups
            .iter()
            .cloned()
            .chain(
                sessions
                    .sessions
                    .iter()
                    .map(|profile| normalize_group_name(&profile.group_name)),
            )
            .collect();
        Self {
            config,
            sessions,
            pending_probe: None,
            pending_trust: None,
            pending_auth: None,
            expanded_groups,
            tabs: Vec::new(),
            active_tab_id: None,
            terminal_numbers: HashMap::new(),
            local_terminal_number: 0,
        }
    }

    pub(super) fn prompt_flow_busy(&self) -> bool {
        self.pending_probe.is_some() || self.pending_trust.is_some() || self.pending_auth.is_some()
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
        });
        self.active_tab_id = Some(id);
        id
    }

    pub(super) fn open_terminal_tab(&mut self, profile: &SessionProfile) -> Uuid {
        let number = self.terminal_numbers.entry(profile.id).or_default();
        *number = number.saturating_add(1);
        let id = Uuid::new_v4();
        let terminal = TerminalModel::new(
            usize::from(self.sessions.settings.terminal.default_columns),
            usize::from(self.sessions.settings.terminal.default_rows),
            self.sessions.settings.terminal.scrollback_lines as usize,
        );
        self.tabs.push(WorkspaceTab {
            id,
            title: format!("{} #{}", profile.name, number),
            kind: WorkspaceTabKind::Terminal(TerminalTabState {
                backend: TerminalBackend::Ssh {
                    profile_id: profile.id,
                    attempt_id: None,
                },
                worker: None,
                terminal,
                status: "Preparing connection...".to_owned(),
                connected: false,
                worker_running: false,
            }),
        });
        self.active_tab_id = Some(id);
        id
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
            }),
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
        let worker = match &mut tab.kind {
            WorkspaceTabKind::Terminal(terminal) => terminal.worker.take(),
            WorkspaceTabKind::Settings | WorkspaceTabKind::SessionEditor(_) => None,
        };
        let pending_probe = self
            .pending_probe
            .as_ref()
            .is_some_and(|pending| pending.tab_id == tab_id)
            .then(|| self.pending_probe.take())
            .flatten();
        let dismissed_prompt = self
            .pending_trust
            .as_ref()
            .is_some_and(|pending| pending.tab_id == tab_id)
            || self
                .pending_auth
                .as_ref()
                .is_some_and(|pending| pending.tab_id == tab_id);
        if self
            .pending_trust
            .as_ref()
            .is_some_and(|pending| pending.tab_id == tab_id)
        {
            self.pending_trust = None;
        }
        if self
            .pending_auth
            .as_ref()
            .is_some_and(|pending| pending.tab_id == tab_id)
        {
            self.pending_auth = None;
        }
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
            dismissed_prompt,
        })
    }

    pub(super) fn drain_runtime_resources(
        &mut self,
    ) -> (Vec<TerminalWorker>, Option<PendingProbe>) {
        let workers = self
            .tabs
            .iter_mut()
            .filter_map(|tab| match &mut tab.kind {
                WorkspaceTabKind::Terminal(terminal) => terminal.worker.take(),
                WorkspaceTabKind::Settings | WorkspaceTabKind::SessionEditor(_) => None,
            })
            .collect();
        (workers, self.pending_probe.take())
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
            WorkspaceTabKind::Terminal(terminal) => ActiveTabSnapshot {
                id: Some(tab.id),
                kind: "terminal",
                title: tab.title.clone(),
                status: terminal.status.clone(),
                editor: None,
                terminal: Some(terminal.terminal.snapshot()),
                connected: terminal.connected,
                worker_running: terminal.worker_running,
            },
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

    pub(super) fn active_terminal(&self) -> Option<&TerminalTabState> {
        self.active_tab_id.and_then(|id| self.terminal(id))
    }

    pub(super) fn active_terminal_mut(&mut self) -> Option<&mut TerminalTabState> {
        self.active_tab_id.and_then(|id| self.terminal_mut(id))
    }

    pub(super) fn resize_active_terminal_model(
        &mut self,
        columns: usize,
        rows: usize,
    ) -> Option<()> {
        self.active_terminal_mut()?.terminal.resize(columns, rows);
        Some(())
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
            Self::Terminal(_) => "terminal",
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
        let (auth_method, private_key_path) = match &profile.auth {
            ax_ssh::config::AuthMethod::Password => ("Password", String::new()),
            ax_ssh::config::AuthMethod::PrivateKey { path } => {
                ("Private key", path.to_string_lossy().into_owned())
            }
        };
        SessionEditorSnapshot {
            draft_id: self.draft_id,
            profile_id: Some(profile.id),
            name: profile.name.clone(),
            group_name: profile.group_name.clone(),
            host: profile.host.clone(),
            port: profile.port.to_string(),
            username: profile.username.clone(),
            auth_method,
            private_key_path,
            credential_stored: profile.credential_stored,
        }
    }
}

struct WorkspaceTab {
    id: Uuid,
    title: String,
    kind: WorkspaceTabKind,
}

pub(super) struct TerminalTabState {
    pub(super) backend: TerminalBackend,
    pub(super) worker: Option<TerminalWorker>,
    pub(super) terminal: TerminalModel,
    pub(super) status: String,
    pub(super) connected: bool,
    pub(super) worker_running: bool,
}

impl TerminalTabState {
    pub(super) fn ssh_route(&self) -> Option<(Uuid, Option<Uuid>)> {
        match self.backend {
            TerminalBackend::Ssh {
                profile_id,
                attempt_id,
            } => Some((profile_id, attempt_id)),
            TerminalBackend::Local => None,
        }
    }

    pub(super) fn set_ssh_attempt(&mut self, attempt_id: Option<Uuid>) -> bool {
        match &mut self.backend {
            TerminalBackend::Ssh {
                attempt_id: current,
                ..
            } => {
                *current = attempt_id;
                true
            }
            TerminalBackend::Local => false,
        }
    }

    pub(super) fn is_local(&self) -> bool {
        matches!(self.backend, TerminalBackend::Local)
    }
}

pub(super) enum TerminalBackend {
    Ssh {
        profile_id: Uuid,
        attempt_id: Option<Uuid>,
    },
    Local,
}

pub(super) enum TerminalWorker {
    Ssh(SshSessionHandle),
    Local(LocalShellHandle),
}

impl TerminalWorker {
    pub(super) fn request_disconnect(&self) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_disconnect(),
            Self::Local(worker) => worker.request_disconnect(),
        }
    }

    pub(super) fn request_send(&self, data: Vec<u8>) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_send(data),
            Self::Local(worker) => worker.request_send(data),
        }
    }

    pub(super) fn request_resize(&self, columns: u32, rows: u32) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.request_resize(columns, rows),
            Self::Local(worker) => worker.request_resize(columns, rows),
        }
    }

    pub(super) async fn shutdown(self) -> Result<()> {
        match self {
            Self::Ssh(worker) => worker.shutdown().await,
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
        }
    }
}

pub(super) struct SessionEditorSnapshot {
    pub(super) draft_id: Uuid,
    pub(super) profile_id: Option<Uuid>,
    pub(super) name: String,
    pub(super) group_name: String,
    pub(super) host: String,
    pub(super) port: String,
    pub(super) username: String,
    pub(super) auth_method: &'static str,
    pub(super) private_key_path: String,
    pub(super) credential_stored: bool,
}

impl Default for SessionEditorSnapshot {
    fn default() -> Self {
        Self {
            draft_id: Uuid::nil(),
            profile_id: None,
            name: String::new(),
            group_name: String::new(),
            host: String::new(),
            port: "22".to_owned(),
            username: String::new(),
            auth_method: "Password",
            private_key_path: String::new(),
            credential_stored: false,
        }
    }
}

pub(super) struct ClosedTab {
    pub(super) worker: Option<TerminalWorker>,
    pub(super) pending_probe: Option<PendingProbe>,
    pub(super) dismissed_prompt: bool,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PendingAuth {
    pub(super) tab_id: Uuid,
    pub(super) profile_id: Uuid,
}

pub(super) enum ConnectionStart {
    Authenticate {
        tab_id: Uuid,
        profile: SessionProfile,
    },
    Probe {
        tab_id: Uuid,
        profile: SessionProfile,
        cancelled: oneshot::Receiver<()>,
    },
}

mod transitions;

pub(super) use self::transitions::{
    prepare_authentication_retry, prepare_host_key_retry, retire_session_attempt,
    session_attempt_is_active, set_credential_marker,
};

#[cfg(test)]
mod tests;
