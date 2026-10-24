use super::*;

impl Owner {
    pub(super) fn insert_pane(
        &mut self,
        id: Uuid,
        profile_id: Option<Uuid>,
        local: bool,
        title: String,
    ) -> Result<()> {
        self.shutdowns.reap();
        ensure!(
            self.panes.len() + self.shutdowns.pending() < MAX_TERMINALS,
            "workspace cannot exceed {MAX_TERMINALS} active or closing terminals"
        );
        ensure!(
            !self.panes.contains_key(&id),
            "terminal identity already exists"
        );
        let terminal = if local {
            TerminalSession::spawn(
                &self.runtime,
                self.sessions.settings.terminal.local_shell.clone(),
                self.wake.clone(),
            )
        } else {
            TerminalSession::idle(&self.runtime, self.wake.clone())
        };
        let sftp = (!local).then(|| SftpSession::idle(&self.runtime, self.wake.clone()));
        self.panes.insert(
            id,
            Pane {
                title,
                profile_id,
                local,
                terminal,
                sftp,
            },
        );
        Ok(())
    }

    pub(super) fn close_worker(&mut self, id: Uuid) {
        if let Some(pane) = self.panes.get(&id) {
            pane.terminal.cancel();
            if let Some(sftp) = &pane.sftp {
                sftp.cancel();
            }
        }
        if let Some(pane) = self.panes.remove(&id) {
            drop(
                self.shutdowns
                    .start(&self.runtime, pane.terminal.shutdown()),
            );
            if let Some(sftp) = pane.sftp {
                drop(self.shutdowns.start(&self.runtime, sftp.shutdown()));
            }
        }
    }

    pub(super) fn layout(&mut self, action: WorkspaceAction) -> Result<()> {
        match action {
            WorkspaceAction::OpenSettings => {
                let id = *self.settings_tab.get_or_insert_with(Uuid::new_v4);
                if !self.tabs.iter().any(|tab| tab.workspace_tab_id() == id) {
                    self.tabs.push(PaneTree::new(id));
                }
                self.active = Some(id);
            }
            WorkspaceAction::NewLocal
            | WorkspaceAction::NewConnection
            | WorkspaceAction::OpenProfile(_) => {
                let id = Uuid::new_v4();
                let (profile, local, title) = match action {
                    WorkspaceAction::NewLocal => (None, true, "Local shell".into()),
                    WorkspaceAction::OpenProfile(profile_id) => {
                        let profile = self
                            .sessions
                            .sessions
                            .iter()
                            .find(|profile| profile.id == profile_id)
                            .context("session no longer exists")?;
                        (Some(profile_id), false, profile.name.clone())
                    }
                    _ => (None, false, "New connection".into()),
                };
                self.insert_pane(id, profile, local, title)?;
                self.tabs.push(PaneTree::new(id));
                self.active = Some(id);
            }
            WorkspaceAction::Activate(id) => {
                ensure!(
                    self.tabs.iter().any(|tab| tab.workspace_tab_id() == id),
                    "tab no longer exists"
                );
                self.active = Some(id);
            }
            WorkspaceAction::Cycle(offset) => {
                if !self.tabs.is_empty() {
                    let index = self
                        .tabs
                        .iter()
                        .position(|tab| Some(tab.workspace_tab_id()) == self.active)
                        .unwrap_or(0);
                    let next = (index as i64 + i64::from(offset)).rem_euclid(self.tabs.len() as i64)
                        as usize;
                    self.active = Some(self.tabs[next].workspace_tab_id());
                }
            }
            WorkspaceAction::Focus(id) => {
                let tree = self
                    .tabs
                    .iter_mut()
                    .find(|tree| tree.contains(id))
                    .context("pane no longer exists")?;
                tree.set_focused(id);
                self.active = Some(tree.workspace_tab_id());
            }
            WorkspaceAction::FocusDirection(direction) => {
                let index = self.active_index()?;
                self.tabs[index].focus_direction(direction);
            }
            WorkspaceAction::Split(direction) => {
                let index = self.active_index()?;
                ensure!(
                    self.tabs[index].pane_count() < super::super::MAX_TERMINAL_PANES,
                    "tab cannot exceed eight panes"
                );
                let id = Uuid::new_v4();
                self.insert_pane(id, None, true, "Local shell".into())?;
                ensure!(
                    self.tabs[index].split_focused(direction, id),
                    "cannot split the focused pane"
                );
            }
            WorkspaceAction::CloseTab(id) => {
                let index = self
                    .tabs
                    .iter()
                    .position(|tree| tree.workspace_tab_id() == id)
                    .context("tab no longer exists")?;
                for pane in self.tabs[index].tab_ids() {
                    self.close_worker(pane);
                }
                self.tabs.remove(index);
                if self.settings_tab == Some(id) {
                    self.settings_tab = None;
                }
                if self.active == Some(id) {
                    self.active = self
                        .tabs
                        .get(index.min(self.tabs.len().saturating_sub(1)))
                        .map(PaneTree::workspace_tab_id);
                }
            }
            WorkspaceAction::ClosePane(id) => {
                let index = self
                    .tabs
                    .iter()
                    .position(|tree| tree.contains(id))
                    .context("pane no longer exists")?;
                let old_root = self.tabs[index].workspace_tab_id();
                if self.tabs[index].pane_count() == 1 {
                    return self.layout(WorkspaceAction::CloseTab(old_root));
                }
                self.close_worker(id);
                self.tabs[index].remove(id);
                if id == old_root {
                    let root = self.tabs[index].root_tab_id();
                    self.tabs[index] = PaneTree::from_snapshot(
                        root,
                        self.tabs[index].snapshot(),
                        self.tabs[index].focused_tab_id(),
                    )
                    .context("cannot promote remaining pane")?;
                    if self.active == Some(old_root) {
                        self.active = Some(root);
                    }
                }
            }
            WorkspaceAction::MoveTab { id, index } => {
                let previous = self
                    .tabs
                    .iter()
                    .position(|tree| tree.workspace_tab_id() == id)
                    .context("tab no longer exists")?;
                let tree = self.tabs.remove(previous);
                self.tabs.insert(index.min(self.tabs.len()), tree);
            }
            WorkspaceAction::ResizeSplit {
                tab,
                divider,
                ratio,
            } => {
                let tree = self
                    .tabs
                    .iter_mut()
                    .find(|tree| tree.workspace_tab_id() == tab)
                    .context("tab no longer exists")?;
                ensure!(tree.resize_split(divider, ratio), "invalid pane divider");
            }
            WorkspaceAction::SaveNow
            | WorkspaceAction::OpenFile(_)
            | WorkspaceAction::SaveFile(_) => {}
        }
        Ok(())
    }

    fn active_index(&self) -> Result<usize> {
        ensure!(
            self.active != self.settings_tab || self.settings_tab.is_none(),
            "settings cannot contain terminal panes"
        );
        self.tabs
            .iter()
            .position(|tree| Some(tree.workspace_tab_id()) == self.active)
            .context("no active terminal tab")
    }
}
