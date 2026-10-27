//! Existing config schemas and atomic writes; blocking I/O stays off the UI thread.

use super::*;
use crate::config::{
    PaneNodeSnapshot, WORKSPACE_SNAPSHOT_VERSION, WorkspaceTabSnapshot, WorkspaceWindowSnapshot,
};
use std::collections::HashSet;

impl Owner {
    pub(super) async fn load(&mut self) -> Result<()> {
        let store = self.store.clone();
        let (sessions, snapshot) = tokio::task::spawn_blocking(move || {
            Ok::<_, anyhow::Error>((store.load()?, store.load_workspace()?))
        })
        .await
        .context("workspace loading task failed")??;
        ensure!(
            sessions
                .sessions
                .iter()
                .map(|profile| profile.id)
                .collect::<HashSet<_>>()
                .len()
                == sessions.sessions.len(),
            "session store has duplicate profile identities"
        );
        if let Some(snapshot) = &snapshot {
            validate_layout(snapshot)?;
        }
        self.sessions = sessions;
        if let Some(snapshot) = snapshot {
            self.restore(snapshot)?;
        } else {
            self.layout(WorkspaceAction::NewLocal)?;
        }
        (self.configure_uploads)(usize::from(
            self.sessions.settings.sftp_transfers.global_uploads,
        ));
        self.ready = true;
        self.status = "Workspace ready · saved sessions use fresh authentication".into();
        self.publish()
    }

    fn restore(&mut self, snapshot: WorkspaceSnapshot) -> Result<()> {
        // Every recovered terminal is idle, including local shells. Recovery
        // never opens a device, starts a process, or authenticates implicitly.
        self.settings_tab = snapshot
            .tabs
            .iter()
            .find(|tab| tab.kind == "settings")
            .map(|tab| tab.id);
        for pane in snapshot.tabs.iter().filter(|tab| tab.kind != "settings") {
            self.insert_pane(pane.id, pane.profile_id, false, pane.title.clone())?;
            if let Some(owned) = self.panes.get_mut(&pane.id) {
                owned.local = pane.kind == "local-terminal";
            }
        }
        let window = snapshot.windows.first();
        let roots = window
            .map(|window| window.panes.clone())
            .unwrap_or_default();
        let mut assigned = HashSet::new();
        for root in roots {
            let id = snapshot
                .tabs
                .iter()
                .find(|tab| contains(&root, tab.id))
                .map(|tab| tab.id)
                .unwrap_or_else(|| first_leaf(&root));
            let focused = window
                .and_then(|window| window.focused_tab_id)
                .filter(|focused| contains(&root, *focused))
                .unwrap_or(id);
            let tree =
                PaneTree::from_snapshot(id, root, focused).context("invalid restored pane tree")?;
            assigned.extend(tree.tab_ids());
            self.tabs.push(tree);
        }
        for pane in &snapshot.tabs {
            if !assigned.contains(&pane.id) {
                self.tabs.push(PaneTree::new(pane.id));
            }
        }
        let active = window
            .and_then(|window| window.active_tab_id)
            .or(snapshot.active_tab_id);
        self.active = self
            .tabs
            .iter()
            .find(|tree| active.is_some_and(|id| tree.contains(id)))
            .or(self.tabs.first())
            .map(PaneTree::workspace_tab_id);
        self.last_checkpoint = Some(snapshot);
        Ok(())
    }

    pub(super) async fn open_file(&mut self, path: std::path::PathBuf) -> Result<()> {
        let snapshot = tokio::task::spawn_blocking(move || ConfigStore::load_workspace_file(&path))
            .await
            .context("workspace import task failed")??;
        validate_layout(&snapshot)?;
        self.checkpoint().await?;
        let ids = self.panes.keys().copied().collect::<Vec<_>>();
        for id in ids {
            self.close_worker(id);
        }
        self.tabs.clear();
        self.settings_tab = None;
        self.active = None;
        self.publish()?;
        ensure!(
            std::mem::take(&mut self.shutdowns)
                .finish(Duration::from_secs(20))
                .await,
            "old workspace cleanup exceeded the deadline"
        );
        self.restore(snapshot)?;
        self.last_checkpoint = None;
        self.checkpoint().await?;
        self.status = "Workspace opened; resume terminals explicitly".into();
        Ok(())
    }

    pub(super) async fn save_file(&mut self, path: std::path::PathBuf) -> Result<()> {
        ensure!(
            path.extension()
                .is_some_and(|extension| extension == "axssh"),
            "exported workspace files must use the .axssh extension"
        );
        let snapshot = self.snapshot()?;
        tokio::task::spawn_blocking(move || ConfigStore::save_workspace_file(&path, &snapshot))
            .await
            .context("workspace export task failed")??;
        self.status = "Workspace file saved".into();
        Ok(())
    }

    pub(super) fn snapshot(&self) -> Result<WorkspaceSnapshot> {
        let mut tabs = Vec::with_capacity(self.panes.len());
        for tree in &self.tabs {
            if self.settings_tab == Some(tree.workspace_tab_id()) {
                tabs.push(WorkspaceTabSnapshot {
                    id: tree.workspace_tab_id(),
                    title: "Settings".into(),
                    kind: "settings".into(),
                    ..Default::default()
                });
                continue;
            }
            let mut ids = tree.tab_ids();
            ids.retain(|id| *id != tree.workspace_tab_id());
            ids.insert(0, tree.workspace_tab_id());
            for id in ids {
                let pane = self
                    .panes
                    .get(&id)
                    .context("pane route missing during snapshot")?;
                let view = pane.terminal.client().view()?;
                let (local, profile_id) = match view.identity {
                    super::super::ConnectionIdentity::Local if view.generation == 0 => {
                        (pane.local, pane.profile_id)
                    }
                    super::super::ConnectionIdentity::Local => (true, None),
                    identity => (
                        false,
                        identity.profile_id().filter(|id| {
                            self.sessions
                                .sessions
                                .iter()
                                .any(|profile| profile.id == *id)
                        }),
                    ),
                };
                tabs.push(WorkspaceTabSnapshot {
                    id,
                    title: pane.title.clone(),
                    kind: if local { "local-terminal" } else { "terminal" }.into(),
                    profile_id: profile_id.filter(|id| {
                        self.sessions
                            .sessions
                            .iter()
                            .any(|profile| profile.id == *id)
                    }),
                    // Deliberately omit terminal text, transient status, trust-once
                    // pins, credentials and connection attempt identities.
                    ..Default::default()
                });
            }
        }
        let focused = self
            .tabs
            .iter()
            .find(|tree| Some(tree.workspace_tab_id()) == self.active)
            .map(PaneTree::focused_tab_id);
        let snapshot = WorkspaceSnapshot {
            version: WORKSPACE_SNAPSHOT_VERSION,
            active_tab_id: focused,
            windows: vec![WorkspaceWindowSnapshot {
                id: Uuid::nil(),
                tab_ids: tabs.iter().map(|tab| tab.id).collect(),
                active_tab_id: focused,
                focused_tab_id: focused,
                panes: self.tabs.iter().map(PaneTree::snapshot).collect(),
                placement: None,
            }],
            tabs,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub(super) async fn checkpoint(&mut self) -> Result<()> {
        let snapshot = self.snapshot()?;
        if self.last_checkpoint.as_ref() == Some(&snapshot) {
            return Ok(());
        }
        let store = self.store.clone();
        let saved = tokio::task::spawn_blocking(move || {
            store.save_workspace(&snapshot)?;
            Ok::<_, anyhow::Error>(snapshot)
        })
        .await
        .context("workspace persistence task failed")??;
        self.last_checkpoint = Some(saved);
        Ok(())
    }
}

fn validate_layout(snapshot: &WorkspaceSnapshot) -> Result<()> {
    snapshot.validate()?;
    ensure!(
        snapshot
            .tabs
            .iter()
            .filter(|tab| tab.kind != "settings")
            .count()
            <= MAX_TERMINALS,
        "experimental workspace cannot exceed {MAX_TERMINALS} terminals"
    );
    ensure!(
        snapshot.windows.len() <= 1
            && snapshot
                .windows
                .first()
                .is_none_or(|window| window.id.is_nil()),
        "detached workspaces require the later window migration"
    );
    ensure!(
        snapshot.tabs.iter().all(|tab| matches!(
            tab.kind.as_str(),
            "terminal" | "local-terminal" | "settings"
        )),
        "workspace contains a feature not yet supported by the experiment"
    );
    let settings: Vec<_> = snapshot
        .tabs
        .iter()
        .filter(|tab| tab.kind == "settings")
        .collect();
    ensure!(
        settings.len() <= 1,
        "workspace contains multiple settings tabs"
    );
    let mut assigned = HashSet::new();
    if let Some(window) = snapshot.windows.first() {
        for root in &window.panes {
            if let Some(settings) = settings.first() {
                ensure!(
                    !contains(root, settings.id)
                        || matches!(root, PaneNodeSnapshot::Leaf(id) if *id == settings.id),
                    "settings cannot be part of a terminal split"
                );
            }
            let tree = PaneTree::from_snapshot(first_leaf(root), root.clone(), first_leaf(root))
                .context("invalid pane tree")?;
            ensure!(
                tree.tab_ids().into_iter().all(|id| assigned.insert(id)),
                "terminal appears in more than one pane tree"
            );
        }
    }
    Ok(())
}

fn first_leaf(node: &PaneNodeSnapshot) -> Uuid {
    match node {
        PaneNodeSnapshot::Leaf(id) => *id,
        PaneNodeSnapshot::Split { first, .. } => first_leaf(first),
    }
}

fn contains(node: &PaneNodeSnapshot, id: Uuid) -> bool {
    match node {
        PaneNodeSnapshot::Leaf(leaf) => *leaf == id,
        PaneNodeSnapshot::Split { first, second, .. } => {
            contains(first, id) || contains(second, id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_settings_cannot_be_duplicated_or_inserted_into_split() {
        let settings = Uuid::new_v4();
        let terminal = Uuid::new_v4();
        let mut snapshot = WorkspaceSnapshot {
            version: WORKSPACE_SNAPSHOT_VERSION,
            tabs: vec![
                WorkspaceTabSnapshot {
                    id: settings,
                    kind: "settings".into(),
                    ..Default::default()
                },
                WorkspaceTabSnapshot {
                    id: terminal,
                    kind: "terminal".into(),
                    ..Default::default()
                },
            ],
            windows: vec![WorkspaceWindowSnapshot {
                id: Uuid::nil(),
                tab_ids: vec![settings, terminal],
                panes: vec![PaneNodeSnapshot::Split {
                    axis: "columns".into(),
                    ratio_milli: 500,
                    first: Box::new(PaneNodeSnapshot::Leaf(settings)),
                    second: Box::new(PaneNodeSnapshot::Leaf(terminal)),
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(validate_layout(&snapshot).is_err());
        snapshot.windows.clear();
        snapshot.tabs[1].kind = "settings".into();
        assert!(validate_layout(&snapshot).is_err());
    }
}
