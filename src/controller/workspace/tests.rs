use super::*;

mod transfer;
use crate::config::{
    PaneNodeSnapshot, WORKSPACE_SNAPSHOT_VERSION, WorkspaceTabSnapshot, WorkspaceWindowSnapshot,
};

struct Fixture {
    directory: std::path::PathBuf,
    store: ConfigStore,
}

impl Fixture {
    fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("axssh-egui-workspace-{}", Uuid::new_v4()));
        let store = ConfigStore::new(directory.join("sessions.json"));
        let mut sessions = SessionStore::default();
        sessions.settings.terminal.local_shell = "axssh-no-such-fixture-shell".into();
        sessions.settings.terminal.scrollback_lines = 1234;
        store.save(&sessions).unwrap();
        store
            .save_workspace(&WorkspaceSnapshot {
                version: WORKSPACE_SNAPSHOT_VERSION,
                ..Default::default()
            })
            .unwrap();
        Self { directory, store }
    }
    fn start(&self) -> Workspace {
        Workspace::spawn(&Handle::current(), self.store.clone(), Arc::new(|| {}))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).unwrap();
    }
}

async fn wait(
    client: &WorkspaceClient,
    predicate: impl Fn(&WorkspaceView) -> bool,
) -> WorkspaceView {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let view = client.view().unwrap();
            if predicate(&view) {
                return view;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("workspace transition should complete")
}

async fn result(client: &WorkspaceClient, request: Uuid) -> CatalogResult {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(result) = client.take_catalog_result(request).unwrap() {
                return result;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn catalog_commits_atomically_and_rejects_stale_editor_results() {
    let fixture = Fixture::new();
    let workspace = fixture.start();
    let client = workspace.client();
    wait(&client, |view| view.ready).await;
    let profile = SessionProfile::new("Saved fixture", "localhost", "fixture");
    let saved = client
        .catalog(0, CatalogAction::Upsert(Box::new(profile.clone())))
        .unwrap();
    let stale = client
        .catalog(0, CatalogAction::Delete(profile.id))
        .unwrap();
    assert!(result(&client, saved).await.error.is_none());
    assert!(
        result(&client, stale)
            .await
            .error
            .unwrap()
            .contains("changed")
    );
    assert!(client.take_catalog_result(saved).unwrap().is_none());
    let view = client.view().unwrap();
    assert_eq!(view.catalog_revision, 1);
    assert_eq!(&**view.profiles, std::slice::from_ref(&profile));
    let disk = fixture.store.load().unwrap();
    assert_eq!(disk.sessions, vec![profile]);
    assert_eq!(disk.settings.terminal.scrollback_lines, 1234);
    workspace.shutdown().await.unwrap();
}

#[tokio::test]
async fn failed_write_preserves_memory_revision_and_can_be_retried() {
    let fixture = Fixture::new();
    let workspace = fixture.start();
    let client = workspace.client();
    wait(&client, |view| view.ready).await;
    let backup = fixture.directory.join("original.json");
    std::fs::rename(fixture.store.path(), &backup).unwrap();
    std::fs::create_dir(fixture.store.path()).unwrap();
    let profile = SessionProfile::new_telnet("Fixture", "localhost");
    let failed = client
        .catalog(0, CatalogAction::Upsert(Box::new(profile.clone())))
        .unwrap();
    assert!(result(&client, failed).await.error.is_some());
    let view = client.view().unwrap();
    assert_eq!(view.catalog_revision, 0);
    assert!(view.profiles.is_empty());
    assert!(!view.busy);
    std::fs::remove_dir(fixture.store.path()).unwrap();
    std::fs::rename(backup, fixture.store.path()).unwrap();
    let retry = client
        .catalog(0, CatalogAction::Upsert(Box::new(profile)))
        .unwrap();
    assert!(result(&client, retry).await.error.is_none());
    workspace.shutdown().await.unwrap();
}

#[tokio::test]
async fn group_transactions_preserve_profile_identity_and_delete_keeps_live_routes() {
    let fixture = Fixture::new();
    let workspace = fixture.start();
    let client = workspace.client();
    wait(&client, |view| view.ready).await;
    let mut profile = SessionProfile::new("Fixture", "localhost", "fixture");
    profile.group_name = "Original".into();
    let request = client
        .catalog(0, CatalogAction::Upsert(Box::new(profile.clone())))
        .unwrap();
    result(&client, request).await;
    let request = client
        .catalog(
            1,
            CatalogAction::RenameGroup {
                old: "Original".into(),
                new: "Renamed".into(),
            },
        )
        .unwrap();
    assert!(result(&client, request).await.error.is_none());
    let request = client
        .catalog(2, CatalogAction::RemoveGroup("Renamed".into()))
        .unwrap();
    assert!(result(&client, request).await.error.is_none());
    assert_eq!(client.view().unwrap().profiles[0].id, profile.id);
    assert!(client.view().unwrap().profiles[0].group_name.is_empty());
    client
        .action(WorkspaceAction::OpenProfile(profile.id))
        .unwrap();
    let view = wait(&client, |view| view.tabs.len() == 1).await;
    let id = view.tabs[0].id;
    let request = client
        .catalog(3, CatalogAction::Delete(profile.id))
        .unwrap();
    assert!(result(&client, request).await.error.is_none());
    assert!(client.terminal(id).unwrap().is_some());
    assert!(
        !client
            .terminal(id)
            .unwrap()
            .unwrap()
            .view()
            .unwrap()
            .connected
    );
    workspace.shutdown().await.unwrap();
    assert_eq!(
        fixture.store.load_workspace().unwrap().unwrap().tabs[0].profile_id,
        None
    );
}

#[tokio::test]
async fn split_cap_root_promotion_and_close_preserve_surviving_terminal_ids() {
    let fixture = Fixture::new();
    let workspace = fixture.start();
    let client = workspace.client();
    wait(&client, |view| view.ready).await;
    client.action(WorkspaceAction::NewConnection).unwrap();
    let view = wait(&client, |view| view.tabs.len() == 1).await;
    let root = view.tabs[0].id;
    let removed = client.terminal(root).unwrap().unwrap();
    for count in 2..=8 {
        client
            .action(WorkspaceAction::Split(PaneDirection::Right))
            .unwrap();
        wait(&client, |view| view.tabs[0].panes.len() == count).await;
    }
    client
        .action(WorkspaceAction::Split(PaneDirection::Down))
        .unwrap();
    wait(&client, |view| view.error.is_some()).await;
    let before = client.view().unwrap();
    let survivors = before.tabs[0]
        .panes
        .iter()
        .map(|pane| pane.id)
        .filter(|id| *id != root)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(before.tabs[0].panes.len(), 8);
    client.action(WorkspaceAction::ClosePane(root)).unwrap();
    let after = wait(&client, |view| view.tabs[0].panes.len() == 7).await;
    assert_eq!(
        after.tabs[0]
            .panes
            .iter()
            .map(|pane| pane.id)
            .collect::<std::collections::HashSet<_>>(),
        survivors
    );
    assert_ne!(after.tabs[0].id, root);
    assert!(client.terminal(root).unwrap().is_none());
    client
        .action(WorkspaceAction::CloseTab(after.tabs[0].id))
        .unwrap();
    wait(&client, |view| view.tabs.is_empty()).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while removed.scroll(0).is_ok() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    workspace.shutdown().await.unwrap();
    assert!(
        fixture
            .store
            .load_workspace()
            .unwrap()
            .unwrap()
            .tabs
            .is_empty()
    );
}

#[tokio::test]
async fn recovery_preserves_layout_and_never_opens_remote_or_local_transports() {
    let fixture = Fixture::new();
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let mut sessions = fixture.store.load().unwrap();
    let mut profile = SessionProfile::new("Recovered", "127.0.0.1", "fixture");
    profile.ssh_mut().unwrap().port = listener.local_addr().unwrap().port();
    sessions.upsert(profile.clone());
    fixture.store.save(&sessions).unwrap();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let snapshot = WorkspaceSnapshot {
        version: WORKSPACE_SNAPSHOT_VERSION,
        active_tab_id: Some(second),
        tabs: vec![
            WorkspaceTabSnapshot {
                id: first,
                title: "Remote".into(),
                kind: "terminal".into(),
                profile_id: Some(profile.id),
                ..Default::default()
            },
            WorkspaceTabSnapshot {
                id: second,
                title: "Local".into(),
                kind: "local-terminal".into(),
                ..Default::default()
            },
        ],
        windows: vec![WorkspaceWindowSnapshot {
            id: Uuid::nil(),
            tab_ids: vec![first, second],
            active_tab_id: Some(second),
            focused_tab_id: Some(second),
            panes: vec![PaneNodeSnapshot::Split {
                axis: "columns".into(),
                ratio_milli: 650,
                first: Box::new(PaneNodeSnapshot::Leaf(second)),
                second: Box::new(PaneNodeSnapshot::Leaf(first)),
            }],
            ..Default::default()
        }],
    };
    fixture.store.save_workspace(&snapshot).unwrap();
    let workspace = fixture.start();
    let client = workspace.client();
    let view = wait(&client, |view| view.ready).await;
    assert_eq!(view.active_tab_id, Some(first));
    assert_eq!(view.tabs[0].title, "Remote");
    assert_eq!(view.tabs[0].layout.dividers[0].ratio, 0.65);
    assert!(
        view.tabs[0]
            .layout
            .panes
            .iter()
            .find(|pane| pane.tab_id == second)
            .unwrap()
            .focused
    );
    for id in [first, second] {
        let view = client.terminal(id).unwrap().unwrap().view().unwrap();
        assert!(!view.connected);
        assert!(view.prompt.is_none());
        assert!(view.status.starts_with("Disconnected"));
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
    workspace.shutdown().await.unwrap();
    let saved = fixture.store.load_workspace().unwrap().unwrap();
    assert!(
        saved
            .tabs
            .iter()
            .all(|tab| tab.terminal_text.is_empty() && tab.status.is_empty())
    );
    assert_eq!(saved.tabs[0].id, first);
}

#[tokio::test]
async fn malformed_workspace_is_preserved_without_automatic_overwrite() {
    let fixture = Fixture::new();
    let original = b"{incomplete workspace";
    std::fs::write(fixture.store.workspace_path(), original).unwrap();
    let workspace = fixture.start();
    let client = workspace.client();
    let view = wait(&client, |view| view.error.is_some()).await;
    assert!(!view.ready);
    client.action(WorkspaceAction::NewLocal).unwrap();
    workspace.shutdown().await.unwrap();
    assert_eq!(
        std::fs::read(fixture.store.workspace_path()).unwrap(),
        original
    );
}

#[tokio::test]
async fn accepted_profile_save_is_drained_before_exit() {
    let fixture = Fixture::new();
    let workspace = fixture.start();
    let client = workspace.client();
    wait(&client, |view| view.ready).await;
    let profile = SessionProfile::new("Exit fixture", "localhost", "fixture");
    let request = client
        .catalog(0, CatalogAction::Upsert(Box::new(profile.clone())))
        .unwrap();
    workspace.shutdown().await.unwrap();
    assert!(
        client
            .take_catalog_result(request)
            .unwrap()
            .unwrap()
            .error
            .is_none()
    );
    assert_eq!(fixture.store.load().unwrap().sessions, vec![profile]);
}

#[tokio::test]
async fn tab_reorder_activation_and_workspace_limit_are_bounded() {
    let fixture = Fixture::new();
    let workspace = fixture.start();
    let client = workspace.client();
    wait(&client, |view| view.ready).await;
    for count in 1..=MAX_TERMINALS {
        client.action(WorkspaceAction::NewConnection).unwrap();
        wait(&client, |view| view.tabs.len() == count).await;
    }
    let first = client.view().unwrap().tabs[0].id;
    client
        .action(WorkspaceAction::MoveTab {
            id: first,
            index: usize::MAX,
        })
        .unwrap();
    wait(&client, |view| view.tabs.last().unwrap().id == first).await;
    client.action(WorkspaceAction::Activate(first)).unwrap();
    wait(&client, |view| view.active_tab_id == Some(first)).await;
    client.action(WorkspaceAction::NewConnection).unwrap();
    let view = wait(&client, |view| view.error.is_some()).await;
    assert_eq!(view.tabs.len(), MAX_TERMINALS);
    workspace.shutdown().await.unwrap();
}

#[tokio::test]
async fn workspace_export_import_preserves_ids_and_rejects_invalid_replacement() {
    let fixture = Fixture::new();
    let workspace = fixture.start();
    let client = workspace.client();
    wait(&client, |view| view.ready).await;
    client.action(WorkspaceAction::NewConnection).unwrap();
    let original = wait(&client, |view| view.tabs.len() == 1).await.tabs[0].id;
    let path = fixture.directory.join("export.axssh");
    client
        .action(WorkspaceAction::SaveFile(path.clone()))
        .unwrap();
    wait(&client, |view| view.status == "Workspace file saved").await;
    assert_eq!(
        ConfigStore::load_workspace_file(&path).unwrap().tabs[0].id,
        original
    );
    client.action(WorkspaceAction::NewConnection).unwrap();
    wait(&client, |view| view.tabs.len() == 2).await;
    let invalid = fixture.directory.join("invalid.axssh");
    std::fs::write(&invalid, b"{broken").unwrap();
    client.action(WorkspaceAction::OpenFile(invalid)).unwrap();
    wait(&client, |view| view.error.is_some()).await;
    assert_eq!(client.view().unwrap().tabs.len(), 2);
    client.action(WorkspaceAction::OpenFile(path)).unwrap();
    let restored = wait(&client, |view| {
        view.tabs.len() == 1 && view.error.is_none() && !view.busy
    })
    .await;
    assert_eq!(restored.tabs[0].id, original);
    assert!(
        !client
            .terminal(original)
            .unwrap()
            .unwrap()
            .view()
            .unwrap()
            .connected
    );
    workspace.shutdown().await.unwrap();
}

#[tokio::test]
async fn settings_singleton_moves_restores_and_never_allocates_transport() {
    let fixture = Fixture::new();
    let workspace = fixture.start();
    let client = workspace.client();
    wait(&client, |view| view.ready).await;
    client.action(WorkspaceAction::NewConnection).unwrap();
    wait(&client, |view| view.tabs.len() == 1).await;
    client.action(WorkspaceAction::OpenSettings).unwrap();
    let view = wait(&client, |view| view.tabs.len() == 2).await;
    let id = view.active_tab_id.unwrap();
    assert!(view.tabs[1].settings);
    assert!(view.tabs[1].panes.is_empty());
    assert!(client.terminal(id).unwrap().is_none());
    assert!(client.sftp(id).unwrap().is_none());
    client.action(WorkspaceAction::OpenSettings).unwrap();
    client
        .action(WorkspaceAction::Split(PaneDirection::Right))
        .unwrap();
    wait(&client, |view| view.error.is_some()).await;
    assert_eq!(client.view().unwrap().tabs.len(), 2);
    client
        .action(WorkspaceAction::MoveTab { id, index: 0 })
        .unwrap();
    wait(&client, |view| view.tabs[0].id == id).await;
    workspace.shutdown().await.unwrap();
    let restarted = fixture.start();
    let client = restarted.client();
    let view = wait(&client, |view| view.ready).await;
    assert_eq!(view.active_tab_id, Some(id));
    assert!(view.tabs[0].settings);
    assert!(client.terminal(id).unwrap().is_none());
    client.action(WorkspaceAction::CloseTab(id)).unwrap();
    wait(&client, |view| view.tabs.len() == 1).await;
    restarted.shutdown().await.unwrap();
}

#[tokio::test]
async fn settings_save_is_atomic_preserves_other_domains_and_rejects_stale_revision() {
    let fixture = Fixture::new();
    let workspace = fixture.start();
    let client = workspace.client();
    let view = wait(&client, |view| view.ready).await;
    let mut settings = (*view.settings).clone();
    settings.appearance.terminal_font_size = 24;
    let mut theme = settings.appearance.theme.clone();
    theme.mode = crate::config::ThemeMode::Light;
    settings.set_theme(theme);
    let backup = fixture.directory.join("original.json");
    std::fs::rename(fixture.store.path(), &backup).unwrap();
    std::fs::create_dir(fixture.store.path()).unwrap();
    let failed = client
        .catalog(0, CatalogAction::Settings(Box::new(settings.clone())))
        .unwrap();
    assert!(result(&client, failed).await.error.is_some());
    assert_eq!(*client.view().unwrap().settings, *view.settings);
    assert_eq!(client.view().unwrap().catalog_revision, 0);
    std::fs::remove_dir(fixture.store.path()).unwrap();
    std::fs::rename(backup, fixture.store.path()).unwrap();
    let saved = client
        .catalog(0, CatalogAction::Settings(Box::new(settings.clone())))
        .unwrap();
    assert!(result(&client, saved).await.error.is_none());
    let view = wait(&client, |view| view.catalog_revision == 1).await;
    assert_eq!(*view.settings, settings);
    assert_eq!(fixture.store.load().unwrap().settings, settings);
    assert_eq!(view.settings.terminal.scrollback_lines, 1234);
    let stale = client
        .catalog(0, CatalogAction::Settings(Box::default()))
        .unwrap();
    assert!(result(&client, stale).await.error.is_some());
    assert_eq!(*client.view().unwrap().settings, settings);
    workspace.shutdown().await.unwrap();
}

#[tokio::test]
async fn only_committed_settings_reach_existing_and_new_panes() {
    let fixture = Fixture::new();
    let workspace = fixture.start();
    let client = workspace.client();
    wait(&client, |view| view.ready).await;
    client.action(WorkspaceAction::NewConnection).unwrap();
    let view = wait(&client, |view| view.tabs.len() == 1).await;
    let id = view.tabs[0].id;
    let terminal = client.terminal(id).unwrap().unwrap();
    let before = terminal.view().unwrap();
    let mut settings = (*view.settings).clone();
    settings.terminal.scrollback_lines = 777;
    settings.terminal.default_columns = 100;
    settings.terminal.default_rows = 40;
    let backup = fixture.directory.join("original.json");
    std::fs::rename(fixture.store.path(), &backup).unwrap();
    std::fs::create_dir(fixture.store.path()).unwrap();
    let failed = client
        .catalog(
            view.catalog_revision,
            CatalogAction::Settings(Box::new(settings.clone())),
        )
        .unwrap();
    assert!(result(&client, failed).await.error.is_some());
    assert_eq!(
        terminal.view().unwrap().selection_revision,
        before.selection_revision
    );
    std::fs::remove_dir(fixture.store.path()).unwrap();
    std::fs::rename(backup, fixture.store.path()).unwrap();
    let saved = client
        .catalog(
            view.catalog_revision,
            CatalogAction::Settings(Box::new(settings)),
        )
        .unwrap();
    assert!(result(&client, saved).await.error.is_none());
    tokio::time::timeout(Duration::from_secs(5), async {
        while terminal.view().unwrap().selection_revision == before.selection_revision {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        terminal.view().unwrap().terminal.max_columns,
        before.terminal.max_columns
    );
    client.action(WorkspaceAction::NewConnection).unwrap();
    let view = wait(&client, |view| view.tabs.len() == 2).await;
    let new_terminal = client
        .terminal(view.tabs[1].id)
        .unwrap()
        .unwrap()
        .view()
        .unwrap();
    assert_eq!(new_terminal.terminal.max_columns, 100);
    assert_eq!(new_terminal.terminal.lines.len(), 40);
    assert!(!new_terminal.connected);
    workspace.shutdown().await.unwrap();
}

#[tokio::test]
async fn global_upload_limit_applies_loaded_and_committed_values_without_profile_or_failed_save_effects()
 {
    let fixture = Fixture::new();
    let mut sessions = fixture.store.load().unwrap();
    sessions.settings.sftp_transfers.global_uploads = 6;
    fixture.store.save(&sessions).unwrap();
    let applied = Arc::new(Mutex::new(Vec::new()));
    let observed = applied.clone();
    let workspace = Workspace::start(
        &Handle::current(),
        fixture.store.clone(),
        Arc::new(|| {}),
        Arc::new(move |limit| observed.lock().unwrap().push(limit)),
    );
    let client = workspace.client();
    let view = wait(&client, |view| view.ready).await;
    assert_eq!(*applied.lock().unwrap(), [6]);
    let mut settings = (*view.settings).clone();
    settings.sftp_transfers.global_uploads = 3;
    let backup = fixture.directory.join("before-limit-save.json");
    std::fs::rename(fixture.store.path(), &backup).unwrap();
    std::fs::create_dir(fixture.store.path()).unwrap();
    let failed = client
        .catalog(0, CatalogAction::Settings(Box::new(settings.clone())))
        .unwrap();
    assert!(result(&client, failed).await.error.is_some());
    assert_eq!(*applied.lock().unwrap(), [6]);
    assert_eq!(
        client
            .view()
            .unwrap()
            .settings
            .sftp_transfers
            .global_uploads,
        6
    );
    std::fs::remove_dir(fixture.store.path()).unwrap();
    std::fs::rename(backup, fixture.store.path()).unwrap();
    let saved = client
        .catalog(0, CatalogAction::Settings(Box::new(settings.clone())))
        .unwrap();
    assert!(result(&client, saved).await.error.is_none());
    assert_eq!(*applied.lock().unwrap(), [6, 3]);
    assert_eq!(
        fixture
            .store
            .load()
            .unwrap()
            .settings
            .sftp_transfers
            .global_uploads,
        3
    );
    settings.sftp_transfers.global_uploads = 9;
    let stale = client
        .catalog(0, CatalogAction::Settings(Box::new(settings)))
        .unwrap();
    assert!(result(&client, stale).await.error.is_some());
    assert_eq!(*applied.lock().unwrap(), [6, 3]);
    let profile = SessionProfile::new("Upload limit fixture", "localhost", "fixture");
    let saved = client
        .catalog(1, CatalogAction::Upsert(Box::new(profile)))
        .unwrap();
    assert!(result(&client, saved).await.error.is_none());
    assert_eq!(*applied.lock().unwrap(), [6, 3]);
    let mut settings = (*client.view().unwrap().settings).clone();
    settings.sftp_transfers.global_uploads = u8::MAX;
    let normalized = client
        .catalog(2, CatalogAction::Settings(Box::new(settings)))
        .unwrap();
    assert!(result(&client, normalized).await.error.is_none());
    assert_eq!(
        *applied.lock().unwrap(),
        [6, 3, usize::from(crate::config::MAX_SFTP_GLOBAL_UPLOADS)]
    );
    workspace.shutdown().await.unwrap();
}

#[tokio::test]
async fn failed_workspace_load_does_not_change_the_process_upload_policy() {
    let fixture = Fixture::new();
    std::fs::write(fixture.store.path(), b"not a settings file").unwrap();
    let applied = Arc::new(Mutex::new(Vec::new()));
    let observed = applied.clone();
    let workspace = Workspace::start(
        &Handle::current(),
        fixture.store.clone(),
        Arc::new(|| {}),
        Arc::new(move |limit| observed.lock().unwrap().push(limit)),
    );
    let client = workspace.client();
    let view = wait(&client, |view| view.error.is_some()).await;
    assert!(!view.ready);
    assert!(applied.lock().unwrap().is_empty());
    workspace.shutdown().await.unwrap();
}
