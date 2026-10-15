//! Tab snapshots state without generated UI types.

use super::*;

impl AppState {
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
                local: matches!(
                    &tab.kind,
                    WorkspaceTabKind::Terminal(terminal) if terminal.is_local()
                ),
                system_icon: match &tab.kind {
                    WorkspaceTabKind::Terminal(terminal) if terminal.is_local() => self
                        .sessions
                        .settings
                        .appearance
                        .local_shell_icon_override
                        .clone(),
                    WorkspaceTabKind::Terminal(terminal) => terminal
                        .profile_id()
                        .and_then(|id| {
                            self.sessions
                                .sessions
                                .iter()
                                .find(|profile| profile.id == id)
                        })
                        .and_then(|profile| {
                            profile.system_icon_override.clone().or_else(|| {
                                self.system_detection(profile.id)
                                    .and_then(|system| system.icon_id().map(str::to_owned))
                            })
                        }),
                    _ => None,
                },
                latency: match &tab.kind {
                    WorkspaceTabKind::Terminal(terminal) if terminal.ssh_route().is_some() => {
                        terminal.connection_latency.snapshot(Instant::now())
                    }
                    _ => ConnectionLatencySnapshot::default(),
                },
                system_release: match &tab.kind {
                    WorkspaceTabKind::Terminal(terminal) => terminal
                        .profile_id()
                        .and_then(|id| self.system_detection(id))
                        .map(|system| system.release().to_owned())
                        .or_else(|| terminal.system_release.clone()),
                    _ => None,
                },
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
                    media: terminal.media.snapshot(),
                    id: Some(tab.id),
                    profile_id: terminal.profile_id(),
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

    pub(in crate::app) fn workspace_tab_snapshots(
        &self,
        include_text: bool,
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
                                .filter(|_| include_text)
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
                let title = match tab.kind {
                    WorkspaceTabKind::Terminal(_) => tab.default_title.clone(),
                    WorkspaceTabKind::Settings | WorkspaceTabKind::SessionEditor(_) => {
                        tab.title.clone()
                    }
                };
                ax_ssh::config::WorkspaceTabSnapshot {
                    id: tab.id,
                    title,
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
        self.workspace_snapshot_with_text(true)
    }

    pub(in crate::app) fn workspace_layout_snapshot(&self) -> WorkspaceSnapshot {
        self.workspace_snapshot_with_text(false)
    }

    fn workspace_snapshot_with_text(&self, include_text: bool) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            version: WORKSPACE_SNAPSHOT_VERSION,
            tabs: self.workspace_tab_snapshots(include_text),
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
                            // Start restored text at the minimum grid. The first
                            // real pane resize can then grow columns and rows
                            // without shrinking a mostly empty default screen.
                            Some(TerminalModel::from_text(
                                &snapshot.terminal_text,
                                usize::from(ax_ssh::terminal_dimensions::MIN_TERMINAL_COLUMNS),
                                usize::from(ax_ssh::terminal_dimensions::MIN_TERMINAL_ROWS),
                                self.sessions.settings.terminal.scrollback_lines as usize,
                            ))
                            .map(|mut terminal| {
                                terminal.set_osc52_clipboard_enabled(
                                    self.sessions.settings.terminal.osc52_clipboard,
                                );
                                terminal
                            })
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
                            media: crate::app::terminal_media::MediaState::default(),
                            backend,
                            worker: None,
                            terminal,
                            pending_terminal_snapshot: None,
                            output_frame_pending: false,
                            published_terminal_snapshot: None,
                            published_terminal_state: None,
                            status: if snapshot.status.is_empty() {
                                "Restored; reconnecting...".to_owned()
                            } else {
                                snapshot.status.clone()
                            },
                            connected: false,
                            connection_latency: ConnectionLatency::default(),
                            system_release: None,
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
                            pending_clipboard_read: None,
                            next_clipboard_read_token: 0,
                        })
                    } else {
                        let terminal = TerminalModel::from_text(
                            &snapshot.terminal_text,
                            usize::from(ax_ssh::terminal_dimensions::MIN_TERMINAL_COLUMNS),
                            usize::from(ax_ssh::terminal_dimensions::MIN_TERMINAL_ROWS),
                            self.sessions.settings.terminal.scrollback_lines as usize,
                        );
                        let mut terminal = terminal;
                        terminal.set_osc52_clipboard_enabled(
                            self.sessions.settings.terminal.osc52_clipboard,
                        );
                        Box::new(TerminalTabState {
                            media: crate::app::terminal_media::MediaState::default(),
                            backend: TerminalBackend::Local,
                            worker: None,
                            terminal: Some(terminal),
                            pending_terminal_snapshot: None,
                            output_frame_pending: false,
                            published_terminal_snapshot: None,
                            published_terminal_state: None,
                            status: snapshot.status.clone(),
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
                            reconnect_enabled: false,
                            pending_auth_secret: None,
                            next_clipboard_read_token: 0,
                            pending_clipboard_read: None,
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
                default_title: snapshot.title.clone(),
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
                local: matches!(
                    &tab.kind,
                    WorkspaceTabKind::Terminal(terminal) if terminal.is_local()
                ),
                system_icon: match &tab.kind {
                    WorkspaceTabKind::Terminal(terminal) if terminal.is_local() => self
                        .sessions
                        .settings
                        .appearance
                        .local_shell_icon_override
                        .clone(),
                    WorkspaceTabKind::Terminal(terminal) => terminal
                        .profile_id()
                        .and_then(|id| {
                            self.sessions
                                .sessions
                                .iter()
                                .find(|profile| profile.id == id)
                        })
                        .and_then(|profile| {
                            profile.system_icon_override.clone().or_else(|| {
                                self.system_detection(profile.id)
                                    .and_then(|system| system.icon_id().map(str::to_owned))
                            })
                        }),
                    _ => None,
                },
                latency: match &tab.kind {
                    WorkspaceTabKind::Terminal(terminal) if terminal.ssh_route().is_some() => {
                        terminal.connection_latency.snapshot(Instant::now())
                    }
                    _ => ConnectionLatencySnapshot::default(),
                },
                system_release: match &tab.kind {
                    WorkspaceTabKind::Terminal(terminal) => terminal
                        .profile_id()
                        .and_then(|id| self.system_detection(id))
                        .map(|system| system.release().to_owned())
                        .or_else(|| terminal.system_release.clone()),
                    _ => None,
                },
            })
            .collect()
    }
}
