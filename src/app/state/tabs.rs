mod refresh;
mod snapshots;
mod terminal;

use super::*;
use ax_ssh::config::{WORKSPACE_SNAPSHOT_VERSION, WorkspaceSnapshot};
use ax_ssh::terminal_dimensions::TerminalSize;

impl AppState {
    pub(in crate::app) fn microphone_in_use(&self) -> bool {
        self.tabs.iter().any(|tab| matches!(&tab.kind, WorkspaceTabKind::Terminal(terminal) if terminal.media.has_job()))
    }

    pub(in crate::app) fn profile_system_releases(&self) -> HashMap<Uuid, String> {
        let mut releases = self
            .sessions
            .sessions
            .iter()
            .filter_map(|profile| {
                self.system_detection(profile.id)
                    .map(|system| (profile.id, system.release().to_owned()))
            })
            .collect::<HashMap<_, _>>();
        for (profile_id, release) in self.tabs.iter().rev().filter_map(|tab| {
            let WorkspaceTabKind::Terminal(terminal) = &tab.kind else {
                return None;
            };
            let (profile_id, _) = terminal.ssh_route()?;
            Some((profile_id, terminal.system_release.clone()?))
        }) {
            releases.entry(profile_id).or_insert(release);
        }
        releases
    }

    pub(in crate::app) fn new(config: ConfigStore, sessions: SessionStore) -> Self {
        Self {
            config,
            sessions,
            tabs: Vec::new(),
            active_tab_id: None,
            terminal_numbers: HashMap::new(),
            profile_mutations: HashMap::new(),
            system_detections: HashMap::new(),
            system_release_probes: HashMap::new(),
            persistence_coordinator: Arc::new(PersistenceCoordinator::default()),
            local_terminal_number: 0,
            session_filter: String::new(),
            serial_ports: Vec::new(),
            terminal_query_palette: TerminalQueryPalette::default(),
            ui_refresh: UiRefreshState::default(),
        }
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
            title: title.clone(),
            default_title: title,
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
            default_title: title.to_owned(),
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
        let mut terminal = TerminalModel::new(
            usize::from(self.sessions.settings.terminal.default_columns),
            usize::from(self.sessions.settings.terminal.default_rows),
            self.sessions.settings.terminal.scrollback_lines as usize,
        );
        terminal.set_osc52_clipboard_enabled(self.sessions.settings.terminal.osc52_clipboard);
        terminal.set_query_palette(self.terminal_query_palette);
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
            default_title: format!("{} #{}", profile.name, number),
            kind: WorkspaceTabKind::Terminal(Box::new(TerminalTabState {
                media: super::super::terminal_media::MediaState::default(),
                backend,
                worker: None,
                terminal: Some(terminal),
                pending_terminal_snapshot: None,
                output_frame_pending: false,
                published_terminal_snapshot: None,
                published_terminal_state: None,
                status: "Preparing connection...".to_owned(),
                connected: false,
                connection_latency: ConnectionLatency::default(),
                system_release: None,
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
                pending_clipboard_read: None,
                next_clipboard_read_token: 0,
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
            default_title: format!("{} SFTP", profile.name),
            kind: WorkspaceTabKind::Terminal(Box::new(TerminalTabState {
                media: super::super::terminal_media::MediaState::default(),
                backend: TerminalBackend::Sftp {
                    profile_id: profile.id,
                    attempt_id: None,
                },
                worker: None,
                terminal: None,
                pending_terminal_snapshot: None,
                output_frame_pending: false,
                published_terminal_snapshot: None,
                published_terminal_state: None,
                status: "Preparing SFTP connection...".to_owned(),
                connected: false,
                connection_latency: ConnectionLatency::default(),
                system_release: None,
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
                pending_clipboard_read: None,
                next_clipboard_read_token: 0,
            })),
            companion_tab_id: None,
        };
        self.insert_connection_tab(tab, profile.id, ConnectionTarget::Sftp, companion_tab_id)
    }

    pub(in crate::app) fn open_local_shell_tab(&mut self) -> Uuid {
        self.local_terminal_number = self.local_terminal_number.saturating_add(1);
        let id = Uuid::new_v4();
        let mut terminal = TerminalModel::new(
            usize::from(self.sessions.settings.terminal.default_columns),
            usize::from(self.sessions.settings.terminal.default_rows),
            self.sessions.settings.terminal.scrollback_lines as usize,
        );
        terminal.set_osc52_clipboard_enabled(self.sessions.settings.terminal.osc52_clipboard);
        terminal.set_query_palette(self.terminal_query_palette);
        self.tabs.push(WorkspaceTab {
            id,
            title: format!("Local Shell #{}", self.local_terminal_number),
            default_title: format!("Local Shell #{}", self.local_terminal_number),
            kind: WorkspaceTabKind::Terminal(Box::new(TerminalTabState {
                media: super::super::terminal_media::MediaState::default(),
                backend: TerminalBackend::Local,
                worker: None,
                terminal: Some(terminal),
                pending_terminal_snapshot: None,
                output_frame_pending: false,
                published_terminal_snapshot: None,
                published_terminal_state: None,
                status: "Starting local shell...".to_owned(),
                connected: false,
                connection_latency: ConnectionLatency::default(),
                system_release: None,
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
                pending_clipboard_read: None,
                next_clipboard_read_token: 0,
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
            terminal.media.cancel();
            if let Some(worker) = terminal.worker.take() {
                workers.push(worker);
            }
            if let Some(probe) = terminal.take_pending_probe() {
                pending_probes.push(probe);
            }
        }
        (workers, pending_probes)
    }

    pub(in crate::app) fn active_tab_id(&self) -> Option<Uuid> {
        self.active_tab_id
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
}
