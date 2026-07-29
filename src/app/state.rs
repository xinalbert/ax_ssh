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
            .sessions
            .iter()
            .map(|profile| normalize_group_name(&profile.group_name))
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
        self.open_singleton_tab(WorkspaceTabKind::SessionEditor, "New session")
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

    pub(super) fn close_tab(&mut self, tab_id: Uuid) -> Option<ClosedTab> {
        let index = self.tabs.iter().position(|tab| tab.id == tab_id)?;
        let mut tab = self.tabs.remove(index);
        let worker = match &mut tab.kind {
            WorkspaceTabKind::Terminal(terminal) => terminal.worker.take(),
            WorkspaceTabKind::Settings | WorkspaceTabKind::SessionEditor => None,
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
                WorkspaceTabKind::Settings | WorkspaceTabKind::SessionEditor => None,
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
            WorkspaceTabKind::SessionEditor => ActiveTabSnapshot {
                id: Some(tab.id),
                kind: "session-editor",
                title: tab.title.clone(),
                ..ActiveTabSnapshot::default()
            },
        }
    }

    pub(super) fn active_tab_id(&self) -> Option<Uuid> {
        self.active_tab_id
    }

    pub(super) fn active_terminal(&self) -> Option<&TerminalTabState> {
        self.active_tab_id.and_then(|id| self.terminal(id))
    }

    pub(super) fn active_terminal_mut(&mut self) -> Option<&mut TerminalTabState> {
        self.active_tab_id.and_then(|id| self.terminal_mut(id))
    }

    pub(super) fn terminal(&self, tab_id: Uuid) -> Option<&TerminalTabState> {
        self.tabs.iter().find_map(|tab| {
            if tab.id != tab_id {
                return None;
            }
            match &tab.kind {
                WorkspaceTabKind::Terminal(terminal) => Some(terminal),
                WorkspaceTabKind::Settings | WorkspaceTabKind::SessionEditor => None,
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
                WorkspaceTabKind::Settings | WorkspaceTabKind::SessionEditor => None,
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
    SessionEditor,
}

impl WorkspaceTabKind {
    fn same_page(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Settings, Self::Settings) | (Self::SessionEditor, Self::SessionEditor)
        )
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Terminal(_) => "terminal",
            Self::Settings => "settings",
            Self::SessionEditor => "session-editor",
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
            status: "Ready".to_owned(),
            terminal: None,
            connected: false,
            worker_running: false,
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

pub(super) fn prepare_authentication_retry(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    session_id: Uuid,
    attempt_id: Uuid,
    clear_credential_marker: bool,
) -> Result<bool> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    if !matches_attempt(&app, tab_id, session_id, attempt_id) {
        return Ok(false);
    }
    if let Some(terminal) = app.terminal_mut(tab_id) {
        terminal.worker = None;
        terminal.set_ssh_attempt(None);
        terminal.connected = false;
        terminal.worker_running = false;
    }
    app.pending_auth = Some(PendingAuth {
        tab_id,
        profile_id: session_id,
    });

    if clear_credential_marker {
        let mut candidate = app.sessions.clone();
        let profile = candidate
            .sessions
            .iter_mut()
            .find(|profile| profile.id == session_id)
            .context("session not found while clearing credential marker")?;
        profile.credential_stored = false;
        match app.config.save(&candidate) {
            Ok(()) => app.sessions = candidate,
            Err(error) => warn!(
                session_id = %session_id,
                %error,
                "failed to clear rejected credential marker"
            ),
        }
    }
    Ok(true)
}

pub(super) fn prepare_host_key_retry(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    session_id: Uuid,
    attempt_id: Uuid,
    prompt: PendingHostKey,
) -> Result<bool> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    if !matches_attempt(&app, tab_id, session_id, attempt_id) {
        return Ok(false);
    }
    if let Some(terminal) = app.terminal_mut(tab_id) {
        terminal.worker = None;
        terminal.set_ssh_attempt(None);
        terminal.connected = false;
        terminal.worker_running = false;
    }
    app.pending_trust = Some(prompt);
    Ok(true)
}

pub(super) fn retire_session_attempt(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    session_id: Uuid,
    attempt_id: Uuid,
) -> bool {
    match state.lock() {
        Ok(mut app) if matches_attempt(&app, tab_id, session_id, attempt_id) => {
            if let Some(terminal) = app.terminal_mut(tab_id) {
                terminal.worker = None;
                terminal.set_ssh_attempt(None);
                terminal.connected = false;
                terminal.worker_running = false;
            }
            true
        }
        Ok(_) => false,
        Err(_) => {
            error!(
                tab_id = %tab_id,
                session_id = %session_id,
                %attempt_id,
                "state lock poisoned while retiring SSH worker"
            );
            false
        }
    }
}

pub(super) fn set_credential_marker(
    state: &Arc<Mutex<AppState>>,
    session_id: Uuid,
    stored: bool,
) -> Result<()> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut candidate = app.sessions.clone();
    let profile = candidate
        .sessions
        .iter_mut()
        .find(|profile| profile.id == session_id)
        .context("session not found while updating credential marker")?;
    if profile.credential_stored == stored {
        return Ok(());
    }
    profile.credential_stored = stored;
    app.config.save(&candidate)?;
    app.sessions = candidate;
    Ok(())
}

pub(super) fn session_attempt_is_active(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    session_id: Uuid,
    attempt_id: Uuid,
) -> bool {
    state
        .lock()
        .is_ok_and(|app| matches_attempt(&app, tab_id, session_id, attempt_id))
}

fn matches_attempt(app: &AppState, tab_id: Uuid, session_id: Uuid, attempt_id: Uuid) -> bool {
    app.terminal(tab_id)
        .and_then(TerminalTabState::ssh_route)
        .is_some_and(|route| route == (session_id, Some(attempt_id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> AppState {
        AppState::new(
            ConfigStore::new(
                std::env::temp_dir().join(format!("ax-ssh-tabs-{}.json", Uuid::new_v4())),
            ),
            SessionStore::default(),
        )
    }

    #[test]
    fn same_profile_opens_independent_terminal_tabs() {
        let mut state = test_state();
        let profile = SessionProfile::new("Local", "localhost", "alice");

        let first = state.open_terminal_tab(&profile);
        let second = state.open_terminal_tab(&profile);

        assert_ne!(first, second);
        assert_eq!(state.tab_summaries().len(), 2);
        assert_eq!(state.tab_summaries()[0].title, "Local #1");
        assert_eq!(state.tab_summaries()[1].title, "Local #2");
        assert_eq!(state.active_tab_id(), Some(second));
    }

    #[test]
    fn settings_and_session_editor_tabs_are_singletons() {
        let mut state = test_state();

        assert_eq!(state.open_settings_tab(), state.open_settings_tab());
        assert_eq!(
            state.open_session_editor_tab(),
            state.open_session_editor_tab()
        );
        assert_eq!(state.tab_summaries().len(), 2);
    }

    #[test]
    fn closing_active_tab_selects_a_neighbor() {
        let mut state = test_state();
        let profile = SessionProfile::new("Local", "localhost", "alice");
        let first = state.open_terminal_tab(&profile);
        let second = state.open_terminal_tab(&profile);

        state.close_tab(second).expect("second tab should close");

        assert_eq!(state.active_tab_id(), Some(first));
    }

    #[test]
    fn retiring_one_duplicate_profile_attempt_does_not_touch_the_other() {
        let mut state = test_state();
        let profile = SessionProfile::new("Local", "localhost", "alice");
        let first = state.open_terminal_tab(&profile);
        let second = state.open_terminal_tab(&profile);
        let first_attempt = Uuid::new_v4();
        let second_attempt = Uuid::new_v4();
        state
            .terminal_mut(first)
            .expect("first terminal should exist")
            .set_ssh_attempt(Some(first_attempt));
        state
            .terminal_mut(second)
            .expect("second terminal should exist")
            .set_ssh_attempt(Some(second_attempt));
        let state = Arc::new(Mutex::new(state));

        assert!(retire_session_attempt(
            &state,
            first,
            profile.id,
            first_attempt
        ));
        let state = state.lock().expect("state should remain readable");
        assert_eq!(
            state.terminal(first).and_then(TerminalTabState::ssh_route),
            Some((profile.id, None))
        );
        assert_eq!(
            state.terminal(second).and_then(TerminalTabState::ssh_route),
            Some((profile.id, Some(second_attempt)))
        );
    }

    #[test]
    fn local_shell_tabs_have_unique_ids_and_independent_numbers() {
        let mut state = test_state();

        let first = state.open_local_shell_tab();
        let second = state.open_local_shell_tab();

        assert_ne!(first, second);
        assert_eq!(state.tab_summaries()[0].title, "Local Shell #1");
        assert_eq!(state.tab_summaries()[1].title, "Local Shell #2");
        assert!(
            state
                .terminal(first)
                .is_some_and(TerminalTabState::is_local)
        );
        assert!(
            state
                .terminal(second)
                .is_some_and(TerminalTabState::is_local)
        );
    }
}
