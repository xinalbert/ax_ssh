use super::*;
use ax_ssh::config::{WORKSPACE_SNAPSHOT_VERSION, WorkspaceSnapshot};
use ax_ssh::terminal_dimensions::TerminalSize;

impl AppState {
    pub(in crate::app) fn new(config: ConfigStore, sessions: SessionStore) -> Self {
        Self {
            config,
            sessions,
            tabs: Vec::new(),
            active_tab_id: None,
            terminal_numbers: HashMap::new(),
            profile_mutations: HashMap::new(),
            persistence_coordinator: Arc::new(PersistenceCoordinator::default()),
            local_terminal_number: 0,
            serial_ports: Vec::new(),
            ui_refresh: UiRefreshState::default(),
        }
    }

    pub(in crate::app) fn request_full_ui_refresh(&mut self) -> bool {
        self.request_ui_refresh(None, None)
    }

    pub(in crate::app) fn request_terminal_ui_refresh(
        &mut self,
        tab_id: Uuid,
        output_received_at: Option<Instant>,
    ) -> bool {
        self.request_ui_refresh(Some(tab_id), output_received_at)
    }

    fn request_ui_refresh(
        &mut self,
        terminal_id: Option<Uuid>,
        output_received_at: Option<Instant>,
    ) -> bool {
        if let Some(tab_id) = terminal_id {
            if !self.ui_refresh.full {
                self.ui_refresh.terminal_ids.insert(tab_id);
            }
        } else {
            self.ui_refresh.full = true;
            self.ui_refresh.terminal_ids.clear();
        }
        if let Some(received_at) = output_received_at {
            self.ui_refresh.earliest_output_received_at = Some(
                self.ui_refresh
                    .earliest_output_received_at
                    .map_or(received_at, |current| current.min(received_at)),
            );
        }

        if !self.ui_refresh.pending {
            self.ui_refresh.generation = self.ui_refresh.generation.saturating_add(1);
            self.ui_refresh.pending = true;
            return true;
        }

        self.ui_refresh.coalesced_requests = self.ui_refresh.coalesced_requests.saturating_add(1);
        if self.ui_refresh.in_progress {
            // The UI has already taken its batch, so this mutation cannot be
            // represented by that snapshot and requires one bounded follow-up.
            self.ui_refresh.generation = self.ui_refresh.generation.saturating_add(1);
        }
        false
    }

    pub(in crate::app) fn take_ui_refresh_batch(&mut self) -> Option<UiRefreshBatch> {
        if !self.ui_refresh.pending || self.ui_refresh.in_progress {
            return None;
        }
        self.ui_refresh.in_progress = true;
        Some(UiRefreshBatch {
            generation: self.ui_refresh.generation,
            full: std::mem::take(&mut self.ui_refresh.full),
            terminal_ids: std::mem::take(&mut self.ui_refresh.terminal_ids),
            earliest_output_received_at: self.ui_refresh.earliest_output_received_at.take(),
            coalesced_requests: std::mem::take(&mut self.ui_refresh.coalesced_requests),
        })
    }

    pub(in crate::app) fn finish_ui_refresh(&mut self, generation: u64) -> bool {
        self.ui_refresh.in_progress = false;
        if self.ui_refresh.generation == generation {
            self.ui_refresh.pending = false;
            false
        } else {
            true
        }
    }

    pub(in crate::app) fn cancel_ui_refresh(&mut self) {
        self.ui_refresh = UiRefreshState::default();
    }

    pub(in crate::app) fn open_settings_tab(&mut self) -> Uuid {
        self.open_singleton_tab(WorkspaceTabKind::Settings, "Settings")
    }

    pub(in crate::app) fn open_session_editor_tab(&mut self) -> Uuid {
        self.open_session_editor(SessionEditorState::default())
    }

    pub(in crate::app) fn open_session_editor_for_group(&mut self, group_name: &str) -> Uuid {
        self.open_session_editor(SessionEditorState {
            draft_id: Uuid::new_v4(),
            group_name: normalize_group_name(group_name),
            profile_id: None,
        })
    }

    pub(in crate::app) fn open_session_editor_for_profile(&mut self, profile_id: Uuid) -> bool {
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
    pub(in crate::app) fn open_terminal_tab(&mut self, profile: &SessionProfile) -> Uuid {
        self.open_terminal_tab_with_companion(profile, None)
    }

    pub(in crate::app) fn open_terminal_tab_with_companion(
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
            kind: WorkspaceTabKind::Terminal(Box::new(TerminalTabState {
                backend,
                worker: None,
                terminal: Some(terminal),
                pending_terminal_snapshot: None,
                published_terminal_state: None,
                status: "Preparing connection...".to_owned(),
                connected: false,
                worker_running: false,
                selection_revision: 0,
                sftp: SftpBrowserState::default(),
                sftp_initial_path: None,
                ssh_phase: SshConnectionPhase::Idle,
                reconnect_generation: 0,
                reconnect_attempt: 0,
                reconnecting: false,
                reconnect_enabled: true,
                pending_auth_secret: None,
            })),
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
    pub(in crate::app) fn open_sftp_tab(&mut self, profile: &SessionProfile) -> Uuid {
        self.open_sftp_tab_with_companion(profile, None)
    }

    #[cfg(test)]
    pub(in crate::app) fn open_sftp_tab_with_companion(
        &mut self,
        profile: &SessionProfile,
        companion_tab_id: Option<Uuid>,
    ) -> Uuid {
        self.open_sftp_tab_with_companion_at_path(profile, companion_tab_id, None)
    }

    pub(in crate::app) fn open_sftp_tab_with_companion_at_path(
        &mut self,
        profile: &SessionProfile,
        companion_tab_id: Option<Uuid>,
        initial_path: Option<String>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        let local_path = profile
            .ssh()
            .map(|ssh| ssh.sftp_local_path.as_str())
            .unwrap_or_default();
        let tab = WorkspaceTab {
            id,
            title: format!("{} SFTP", profile.name),
            kind: WorkspaceTabKind::Terminal(Box::new(TerminalTabState {
                backend: TerminalBackend::Sftp {
                    profile_id: profile.id,
                    attempt_id: None,
                },
                worker: None,
                terminal: None,
                pending_terminal_snapshot: None,
                published_terminal_state: None,
                status: "Preparing SFTP connection...".to_owned(),
                connected: false,
                worker_running: false,
                selection_revision: 0,
                sftp: SftpBrowserState::for_standalone_tab(local_path),
                sftp_initial_path: initial_path,
                ssh_phase: SshConnectionPhase::Idle,
                reconnect_generation: 0,
                reconnect_attempt: 0,
                reconnecting: false,
                reconnect_enabled: true,
                pending_auth_secret: None,
            })),
            companion_tab_id: None,
        };
        self.insert_connection_tab(tab, profile.id, ConnectionTarget::Sftp, companion_tab_id)
    }

    pub(in crate::app) fn open_local_shell_tab(&mut self) -> Uuid {
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
            kind: WorkspaceTabKind::Terminal(Box::new(TerminalTabState {
                backend: TerminalBackend::Local,
                worker: None,
                terminal: Some(terminal),
                pending_terminal_snapshot: None,
                published_terminal_state: None,
                status: "Starting local shell...".to_owned(),
                connected: false,
                worker_running: true,
                selection_revision: 0,
                sftp: SftpBrowserState::default(),
                sftp_initial_path: None,
                ssh_phase: SshConnectionPhase::Idle,
                reconnect_generation: 0,
                reconnect_attempt: 0,
                reconnecting: false,
                reconnect_enabled: false,
                pending_auth_secret: None,
            })),
            companion_tab_id: None,
        });
        self.active_tab_id = Some(id);
        id
    }

    pub(in crate::app) fn activate_tab(&mut self, tab_id: Uuid) -> bool {
        if self.tabs.iter().any(|tab| tab.id == tab_id) {
            self.active_tab_id = Some(tab_id);
            true
        } else {
            false
        }
    }

    #[cfg(test)]
    pub(in crate::app) fn cycle_tab(&mut self, next: bool) -> Option<Uuid> {
        if self.tabs.len() < 2 {
            return None;
        }
        let active_index = self
            .active_tab_id
            .and_then(|active_id| self.tabs.iter().position(|tab| tab.id == active_id))?;
        let target_index = if next {
            (active_index + 1) % self.tabs.len()
        } else {
            active_index.checked_sub(1).unwrap_or(self.tabs.len() - 1)
        };
        let target_id = self.tabs[target_index].id;
        self.active_tab_id = Some(target_id);
        Some(target_id)
    }

    pub(in crate::app) fn switch_ssh_sftp_tab(&mut self) -> Option<SshSftpNavigation> {
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

    #[cfg(test)]
    pub(in crate::app) fn move_tab(&mut self, tab_id: Uuid, target_index: usize) -> bool {
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

    pub(in crate::app) fn move_tab_for(
        &mut self,
        tab_id: Uuid,
        target_index: usize,
        visible_tab_ids: &[Uuid],
    ) -> bool {
        let Some(source_index) = visible_tab_ids.iter().position(|id| *id == tab_id) else {
            return false;
        };
        let target_index = target_index.min(visible_tab_ids.len().saturating_sub(1));
        if source_index == target_index {
            return true;
        }
        let mut desired_order = visible_tab_ids.to_vec();
        let moved_id = desired_order.remove(source_index);
        desired_order.insert(target_index, moved_id);
        let Some(global_source_index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return false;
        };
        let tab = self.tabs.remove(global_source_index);
        let insertion_index = if let Some(next_id) = desired_order.get(target_index + 1) {
            self.tabs.iter().position(|tab| tab.id == *next_id)
        } else if let Some(previous_id) = target_index
            .checked_sub(1)
            .and_then(|index| desired_order.get(index))
        {
            self.tabs
                .iter()
                .position(|tab| tab.id == *previous_id)
                .map(|index| index + 1)
        } else {
            Some(0)
        };
        let Some(insertion_index) = insertion_index else {
            self.tabs
                .insert(global_source_index.min(self.tabs.len()), tab);
            return false;
        };
        self.tabs.insert(insertion_index, tab);
        true
    }

    pub(in crate::app) fn close_tab(&mut self, tab_id: Uuid) -> Option<ClosedTab> {
        let index = self.tabs.iter().position(|tab| tab.id == tab_id)?;
        let mut tab = self.tabs.remove(index);
        if let Some(companion_tab_id) = tab.companion_tab_id.take()
            && let Some(companion) = self.tabs.iter_mut().find(|tab| tab.id == companion_tab_id)
            && companion.companion_tab_id == Some(tab_id)
        {
            companion.companion_tab_id = None;
        }
        let (kind, worker, pending_probe) = match &mut tab.kind {
            WorkspaceTabKind::Terminal(terminal) => (
                ClosedTabKind::Terminal {
                    release_file_icon_cache: terminal.is_sftp()
                        && !self.tabs.iter().any(|tab| {
                            matches!(
                                &tab.kind,
                                WorkspaceTabKind::Terminal(terminal) if terminal.is_sftp()
                            )
                        }),
                },
                terminal.worker.take(),
                terminal.take_pending_probe(),
            ),
            WorkspaceTabKind::Settings => (ClosedTabKind::Settings, None, None),
            WorkspaceTabKind::SessionEditor(_) => {
                self.serial_ports.clear();
                (ClosedTabKind::SessionEditor, None, None)
            }
        };
        if self.active_tab_id == Some(tab_id) {
            self.active_tab_id = self
                .tabs
                .get(index)
                .or_else(|| self.tabs.last())
                .map(|tab| tab.id);
        }
        Some(ClosedTab {
            kind,
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

    pub(in crate::app) fn drain_runtime_resources(
        &mut self,
    ) -> (Vec<TerminalWorker>, Vec<PendingProbe>) {
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

    pub(in crate::app) fn tab_summaries(&self) -> Vec<WorkspaceTabSummary> {
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

    #[cfg(test)]
    pub(in crate::app) fn active_snapshot(&mut self) -> ActiveTabSnapshot {
        self.snapshot_for(self.active_tab_id)
    }

    pub(in crate::app) fn snapshot_for(&mut self, tab_id: Option<Uuid>) -> ActiveTabSnapshot {
        self.snapshot_for_with_terminal(tab_id, true)
    }

    pub(in crate::app) fn snapshot_without_terminal_for(
        &mut self,
        tab_id: Option<Uuid>,
    ) -> ActiveTabSnapshot {
        self.snapshot_for_with_terminal(tab_id, false)
    }

    fn snapshot_for_with_terminal(
        &mut self,
        tab_id: Option<Uuid>,
        include_terminal: bool,
    ) -> ActiveTabSnapshot {
        let Some(active_id) = tab_id else {
            return ActiveTabSnapshot::default();
        };
        let security_prompt = self.security_prompt_for(Some(active_id));
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == active_id) else {
            return ActiveTabSnapshot::default();
        };
        match &mut tab.kind {
            WorkspaceTabKind::Terminal(terminal) => {
                let is_sftp = terminal.is_sftp();
                let mut sftp = if is_sftp {
                    terminal.sftp.snapshot(terminal.connected)
                } else {
                    SftpBrowserSnapshot::default()
                };
                if is_sftp && sftp.status.is_empty() {
                    sftp.status = terminal.status.clone();
                }
                ActiveTabSnapshot {
                    id: Some(tab.id),
                    kind: terminal.backend.kind(),
                    title: tab.title.clone(),
                    status: terminal.status.clone(),
                    notice: terminal.notice_snapshot(),
                    editor: None,
                    terminal: include_terminal
                        .then(|| terminal.terminal_snapshot_for_ui())
                        .flatten(),
                    connected: terminal.connected,
                    selection_revision: terminal.selection_revision,
                    sftp,
                    security_prompt,
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

    pub(in crate::app) fn active_tab_id(&self) -> Option<Uuid> {
        self.active_tab_id
    }

    pub(in crate::app) fn workspace_tab_snapshots(
        &self,
    ) -> Vec<ax_ssh::config::WorkspaceTabSnapshot> {
        self.tabs
            .iter()
            .map(|tab| {
                let (kind, profile_id, terminal_text, sftp_remote_path, sftp_local_path, status) =
                    match &tab.kind {
                        WorkspaceTabKind::Terminal(terminal) => (
                            terminal.backend.kind().to_owned(),
                            terminal.profile_id(),
                            terminal
                                .terminal
                                .as_ref()
                                .map(TerminalModel::contents)
                                .unwrap_or_default(),
                            terminal.sftp.path.clone(),
                            terminal.sftp.local.path.clone(),
                            terminal.status.clone(),
                        ),
                        WorkspaceTabKind::Settings => (
                            "settings".to_owned(),
                            None,
                            String::new(),
                            String::new(),
                            String::new(),
                            String::new(),
                        ),
                        WorkspaceTabKind::SessionEditor(editor) => (
                            "session-editor".to_owned(),
                            editor.profile_id,
                            String::new(),
                            String::new(),
                            String::new(),
                            String::new(),
                        ),
                    };
                ax_ssh::config::WorkspaceTabSnapshot {
                    id: tab.id,
                    title: tab.title.clone(),
                    kind,
                    profile_id,
                    companion_tab_id: tab.companion_tab_id,
                    terminal_text,
                    sftp_remote_path,
                    sftp_local_path,
                    status,
                }
            })
            .collect()
    }

    pub(in crate::app) fn workspace_snapshot(&self) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            version: WORKSPACE_SNAPSHOT_VERSION,
            tabs: self.workspace_tab_snapshots(),
            active_tab_id: self.active_tab_id,
            windows: Vec::new(),
        }
    }

    pub(in crate::app) fn set_active_tab_from_snapshot(&mut self, tab_id: Option<Uuid>) {
        if let Some(tab_id) = tab_id
            && self.tabs.iter().any(|tab| tab.id == tab_id)
        {
            self.active_tab_id = Some(tab_id);
        }
    }

    pub(in crate::app) fn restored_connection_targets(
        &self,
    ) -> Vec<(Uuid, Uuid, ConnectionTarget)> {
        self.tabs
            .iter()
            .filter_map(|tab| {
                let WorkspaceTabKind::Terminal(terminal) = &tab.kind else {
                    return None;
                };
                terminal
                    .profile_id()
                    .map(|profile_id| (tab.id, profile_id, terminal.connection_target()))
            })
            .collect()
    }

    pub(in crate::app) fn restored_local_tabs(&self) -> Vec<Uuid> {
        self.tabs
            .iter()
            .filter_map(|tab| {
                matches!(&tab.kind, WorkspaceTabKind::Terminal(terminal) if terminal.is_local())
                    .then_some(tab.id)
            })
            .collect()
    }

    pub(in crate::app) fn restore_workspace_tabs(
        &mut self,
        snapshots: &[ax_ssh::config::WorkspaceTabSnapshot],
    ) -> Vec<Uuid> {
        // A replacement workspace must not let a queued refresh for a removed
        // Tab run against a newly restored Tab that happens to reuse its UUID.
        self.ui_refresh = UiRefreshState::default();
        self.serial_ports.clear();
        self.tabs.clear();
        self.active_tab_id = None;
        let mut restored = Vec::new();
        for snapshot in snapshots {
            if self.tabs.iter().any(|tab| tab.id == snapshot.id) {
                continue;
            }
            let tab = match snapshot.kind.as_str() {
                "terminal" | "sftp" => {
                    let terminal = if let Some(profile_id) = snapshot.profile_id {
                        let Some(profile) = self
                            .sessions
                            .sessions
                            .iter()
                            .find(|profile| profile.id == profile_id)
                        else {
                            continue;
                        };
                        let backend = match (&profile.connection, snapshot.kind.as_str()) {
                            (ax_ssh::config::ConnectionProfile::Ssh(_), "sftp") => {
                                TerminalBackend::Sftp {
                                    profile_id,
                                    attempt_id: None,
                                }
                            }
                            (ax_ssh::config::ConnectionProfile::Telnet(_), "sftp")
                            | (ax_ssh::config::ConnectionProfile::Serial(_), "sftp") => {
                                continue;
                            }
                            (ax_ssh::config::ConnectionProfile::Ssh(_), _) => {
                                TerminalBackend::Ssh {
                                    profile_id,
                                    attempt_id: None,
                                }
                            }
                            (ax_ssh::config::ConnectionProfile::Telnet(_), _) => {
                                TerminalBackend::Telnet {
                                    profile_id,
                                    attempt_id: None,
                                }
                            }
                            (ax_ssh::config::ConnectionProfile::Serial(_), _) => {
                                TerminalBackend::Serial {
                                    profile_id,
                                    attempt_id: None,
                                }
                            }
                        };
                        let terminal = if snapshot.kind == "sftp" {
                            None
                        } else {
                            Some(TerminalModel::from_text(
                                &snapshot.terminal_text,
                                usize::from(self.sessions.settings.terminal.default_columns),
                                usize::from(self.sessions.settings.terminal.default_rows),
                                self.sessions.settings.terminal.scrollback_lines as usize,
                            ))
                        };
                        let local_path = if snapshot.sftp_local_path.is_empty() {
                            profile
                                .ssh()
                                .map(|ssh| ssh.sftp_local_path.as_str())
                                .unwrap_or_default()
                        } else {
                            &snapshot.sftp_local_path
                        };
                        let mut sftp = if snapshot.kind == "sftp" {
                            SftpBrowserState::for_standalone_tab(local_path)
                        } else {
                            SftpBrowserState::default()
                        };
                        sftp.path = snapshot.sftp_remote_path.clone();
                        Box::new(TerminalTabState {
                            backend,
                            worker: None,
                            terminal,
                            pending_terminal_snapshot: None,
                            published_terminal_state: None,
                            status: if snapshot.status.is_empty() {
                                "Restored; reconnecting...".to_owned()
                            } else {
                                snapshot.status.clone()
                            },
                            connected: false,
                            worker_running: false,
                            selection_revision: 0,
                            sftp,
                            sftp_initial_path: (!snapshot.sftp_remote_path.is_empty())
                                .then(|| snapshot.sftp_remote_path.clone()),
                            ssh_phase: SshConnectionPhase::Idle,
                            reconnect_generation: 0,
                            reconnect_attempt: 0,
                            reconnecting: false,
                            reconnect_enabled: true,
                            pending_auth_secret: None,
                        })
                    } else {
                        let terminal = TerminalModel::from_text(
                            &snapshot.terminal_text,
                            usize::from(self.sessions.settings.terminal.default_columns),
                            usize::from(self.sessions.settings.terminal.default_rows),
                            self.sessions.settings.terminal.scrollback_lines as usize,
                        );
                        Box::new(TerminalTabState {
                            backend: TerminalBackend::Local,
                            worker: None,
                            terminal: Some(terminal),
                            pending_terminal_snapshot: None,
                            published_terminal_state: None,
                            status: snapshot.status.clone(),
                            connected: false,
                            worker_running: false,
                            selection_revision: 0,
                            sftp: SftpBrowserState::default(),
                            sftp_initial_path: None,
                            ssh_phase: SshConnectionPhase::Idle,
                            reconnect_generation: 0,
                            reconnect_attempt: 0,
                            reconnecting: false,
                            reconnect_enabled: false,
                            pending_auth_secret: None,
                        })
                    };
                    WorkspaceTabKind::Terminal(terminal)
                }
                "settings" => WorkspaceTabKind::Settings,
                "session-editor" => WorkspaceTabKind::SessionEditor(SessionEditorState {
                    draft_id: Uuid::new_v4(),
                    profile_id: snapshot.profile_id,
                    group_name: String::new(),
                }),
                _ => continue,
            };
            self.tabs.push(WorkspaceTab {
                id: snapshot.id,
                title: snapshot.title.clone(),
                kind: tab,
                companion_tab_id: None,
            });
            restored.push(snapshot.id);
        }
        for snapshot in snapshots {
            if !restored.contains(&snapshot.id) {
                continue;
            }
            let companion = snapshot.companion_tab_id.filter(|id| restored.contains(id));
            if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == snapshot.id) {
                tab.companion_tab_id = companion;
            }
        }
        self.active_tab_id = restored.first().copied();
        restored
    }

    pub(in crate::app) fn active_editor_profile_id(&self) -> Option<Option<Uuid>> {
        self.active_editor_identity()
            .map(|(_, _, profile_id)| profile_id)
    }

    pub(in crate::app) fn active_editor_identity(&self) -> Option<(Uuid, Uuid, Option<Uuid>)> {
        let active_id = self.active_tab_id?;
        let tab = self.tabs.iter().find(|tab| tab.id == active_id)?;
        let WorkspaceTabKind::SessionEditor(editor) = &tab.kind else {
            return None;
        };
        Some((tab.id, editor.draft_id, editor.profile_id))
    }

    pub(in crate::app) fn editor_matches(&self, tab_id: Uuid, draft_id: Uuid) -> bool {
        self.tabs.iter().any(|tab| {
            tab.id == tab_id
                && matches!(
                    &tab.kind,
                    WorkspaceTabKind::SessionEditor(editor) if editor.draft_id == draft_id
                )
        })
    }

    pub(in crate::app) fn begin_profile_mutation(&mut self, profile_id: Uuid) -> Uuid {
        let token = Uuid::new_v4();
        self.profile_mutations.insert(profile_id, token);
        token
    }

    pub(in crate::app) fn profile_mutation_is_current(
        &self,
        profile_id: Uuid,
        token: Uuid,
    ) -> bool {
        self.profile_mutations.get(&profile_id) == Some(&token)
    }

    pub(in crate::app) fn profile_mutation_is_pending(&self, profile_id: Uuid) -> bool {
        self.profile_mutations.contains_key(&profile_id)
    }

    pub(in crate::app) fn finish_profile_mutation(&mut self, profile_id: Uuid, token: Uuid) {
        if self.profile_mutation_is_current(profile_id, token) {
            self.profile_mutations.remove(&profile_id);
        }
    }

    pub(in crate::app) fn replace_serial_ports(&mut self, ports: Vec<SerialPortDescriptor>) {
        self.serial_ports = ports;
    }

    pub(in crate::app) fn clear_serial_ports(&mut self) {
        self.serial_ports.clear();
    }

    pub(in crate::app) fn has_settings_tab(&self) -> bool {
        self.tabs
            .iter()
            .any(|tab| matches!(tab.kind, WorkspaceTabKind::Settings))
    }

    pub(in crate::app) fn has_session_editor_tab(&self) -> bool {
        self.tabs
            .iter()
            .any(|tab| matches!(tab.kind, WorkspaceTabKind::SessionEditor(_)))
    }

    pub(in crate::app) fn serial_ports(&self) -> &[SerialPortDescriptor] {
        &self.serial_ports
    }

    pub(in crate::app) fn active_terminal(&self) -> Option<&TerminalTabState> {
        self.active_tab_id.and_then(|id| self.terminal(id))
    }

    pub(in crate::app) fn active_terminal_mut(&mut self) -> Option<&mut TerminalTabState> {
        self.active_tab_id.and_then(|id| self.terminal_mut(id))
    }

    #[cfg(test)]
    pub(in crate::app) fn resize_active_terminal(&mut self, columns: u32, rows: u32) -> Result<()> {
        let tab_id = self.active_tab_id.context("no active terminal")?;
        self.resize_terminal(tab_id, columns, rows).map(|_| ())
    }

    pub(in crate::app) fn resize_terminal(
        &mut self,
        tab_id: Uuid,
        columns: u32,
        rows: u32,
    ) -> Result<bool> {
        let terminal = self
            .terminal_mut(tab_id)
            .context("terminal tab not found")?;
        let current_size = terminal
            .terminal
            .as_ref()
            .context("terminal tab has no terminal model")?;
        let size = TerminalSize::model(columns as usize, rows as usize);
        if current_size.size() == size {
            return Ok(false);
        }
        if let Some(worker) = terminal.worker.as_ref() {
            worker.request_resize(size.columns(), size.rows())?;
        }
        let model = terminal
            .terminal
            .as_mut()
            .context("terminal tab has no terminal model")?;
        if model.resize(size.columns() as usize, size.rows() as usize) {
            terminal.discard_pending_terminal_snapshot();
            terminal.invalidate_selection();
        }
        Ok(true)
    }

    pub(in crate::app) fn scroll_terminal(&mut self, tab_id: Uuid, lines: i32) -> bool {
        let Some(terminal) = self.terminal_mut(tab_id) else {
            return false;
        };
        let changed = terminal
            .terminal
            .as_mut()
            .is_some_and(|model| model.scroll(lines));
        if changed {
            terminal.discard_pending_terminal_snapshot();
            terminal.invalidate_selection();
        }
        changed
    }

    pub(in crate::app) fn scroll_terminal_to_bottom(&mut self, tab_id: Uuid) -> bool {
        let Some(terminal) = self.terminal_mut(tab_id) else {
            return false;
        };
        let changed = terminal
            .terminal
            .as_mut()
            .is_some_and(TerminalModel::scroll_to_bottom);
        if changed {
            terminal.discard_pending_terminal_snapshot();
            terminal.invalidate_selection();
        }
        changed
    }

    pub(in crate::app) fn pane_session_source(&self, tab_id: Uuid) -> Option<PaneSessionSource> {
        let terminal = self.terminal(tab_id)?;
        if terminal.is_sftp() {
            return None;
        }
        if terminal.is_local() {
            return Some(PaneSessionSource::LocalShell);
        }
        terminal
            .profile_id()
            .map(PaneSessionSource::ProfileConnection)
    }

    pub(in crate::app) fn terminal_companion_id(&self, tab_id: Uuid) -> Option<Uuid> {
        let companion_id = self
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)?
            .companion_tab_id?;
        self.terminal(companion_id)
            .is_some_and(|terminal| !terminal.is_sftp())
            .then_some(companion_id)
    }

    pub(in crate::app) fn sftp_companion_id(&self, tab_id: Uuid) -> Option<Uuid> {
        let companion_id = self
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)?
            .companion_tab_id?;
        self.terminal(companion_id)
            .is_some_and(TerminalTabState::is_sftp)
            .then_some(companion_id)
    }

    pub(in crate::app) fn terminal(&self, tab_id: Uuid) -> Option<&TerminalTabState> {
        self.tabs.iter().find_map(|tab| {
            if tab.id != tab_id {
                return None;
            }
            match &tab.kind {
                WorkspaceTabKind::Terminal(terminal) => Some(terminal.as_ref()),
                WorkspaceTabKind::Settings | WorkspaceTabKind::SessionEditor(_) => None,
            }
        })
    }

    pub(in crate::app) fn terminal_mut(&mut self, tab_id: Uuid) -> Option<&mut TerminalTabState> {
        self.tabs.iter_mut().find_map(|tab| {
            if tab.id != tab_id {
                return None;
            }
            match &mut tab.kind {
                WorkspaceTabKind::Terminal(terminal) => Some(terminal.as_mut()),
                WorkspaceTabKind::Settings | WorkspaceTabKind::SessionEditor(_) => None,
            }
        })
    }

    pub(in crate::app) fn apply_scrollback_setting(&mut self) {
        let scrollback_lines = self.sessions.settings.terminal.scrollback_lines as usize;
        for tab in &mut self.tabs {
            if let WorkspaceTabKind::Terminal(terminal) = &mut tab.kind
                && let Some(model) = terminal.terminal.as_mut()
            {
                model.set_scrollback_lines(scrollback_lines);
                terminal.discard_pending_terminal_snapshot();
            }
        }
    }

    #[cfg(test)]
    pub(in crate::app) fn active_security_prompt(&self) -> ActiveSecurityPrompt {
        self.security_prompt_for(self.active_tab_id)
    }

    pub(in crate::app) fn security_prompt_for(&self, tab_id: Option<Uuid>) -> ActiveSecurityPrompt {
        let Some(tab_id) = tab_id else {
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
                | SshConnectionPhase::ConfirmingHostKey(_)
                | SshConnectionPhase::LoadingStoredCredential,
            )
            | None => ActiveSecurityPrompt::None,
        }
    }

    /// Returns the terminal tab and its SSH/SFTP companion as one movable UI group.
    ///
    /// The transfer contains identifiers only. Runtime workers, pending probes, and
    /// authentication state remain owned by `AppState` while the group is displayed by
    /// another native window.
    #[cfg(test)]
    pub(in crate::app) fn workspace_transfer_for(
        &self,
        tab_id: Uuid,
        source_window_id: Uuid,
    ) -> Option<WorkspaceTransfer> {
        self.workspace_transfer_for_terminal_panes(&[tab_id], source_window_id, tab_id)
    }

    pub(in crate::app) fn workspace_transfer_for_terminal_panes(
        &self,
        pane_tab_ids: &[Uuid],
        source_window_id: Uuid,
        active_tab_id: Uuid,
    ) -> Option<WorkspaceTransfer> {
        if pane_tab_ids.is_empty()
            || !pane_tab_ids.iter().all(|tab_id| {
                self.terminal(*tab_id)
                    .is_some_and(|terminal| !terminal.is_sftp())
            })
        {
            return None;
        }
        let mut included = pane_tab_ids.iter().copied().collect::<HashSet<_>>();
        for tab_id in pane_tab_ids {
            if let Some(companion_id) = self
                .tabs
                .iter()
                .find(|tab| tab.id == *tab_id)
                .and_then(|tab| tab.companion_tab_id)
            {
                included.insert(companion_id);
            }
        }
        let tab_ids = self
            .tabs
            .iter()
            .filter(|tab| included.contains(&tab.id))
            .map(|tab| tab.id)
            .collect();
        Some(WorkspaceTransfer {
            source_window_id,
            tab_ids,
            active_tab_id: Some(active_tab_id),
        })
    }

    pub(in crate::app) fn workspace_transfer_for_sftp(
        &self,
        tab_id: Uuid,
        source_window_id: Uuid,
    ) -> Option<WorkspaceTransfer> {
        self.terminal(tab_id)
            .is_some_and(TerminalTabState::is_sftp)
            .then_some(WorkspaceTransfer {
                source_window_id,
                tab_ids: vec![tab_id],
                active_tab_id: Some(tab_id),
            })
    }

    pub(in crate::app) fn tab_summaries_for(&self, tab_ids: &[Uuid]) -> Vec<WorkspaceTabSummary> {
        let allowed = tab_ids.iter().copied().collect::<HashSet<_>>();
        self.tabs
            .iter()
            .filter(|tab| allowed.contains(&tab.id))
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
}
