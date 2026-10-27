use super::*;
use crate::session_transfer::{SessionExportTarget, export_session_profile};

fn transfer_fixture() -> Fixture {
    let fixture = Fixture::new();
    let mut sessions = fixture.store.load().unwrap();
    sessions
        .sessions
        .push(SessionProfile::new("fixture", "example.invalid", "fixture"));
    sessions.add_group("Empty").unwrap();
    fixture.store.save(&sessions).unwrap();
    fixture
}

#[tokio::test]
async fn transfer_round_trip_keeps_settings_and_live_pane_identity_and_rejects_old_revisions() {
    let fixture = transfer_fixture();
    let workspace = fixture.start();
    let client = workspace.client();
    let view = wait(&client, |view| view.ready).await;
    let source = view.profiles[0].clone();
    let empty = client
        .catalog(
            0,
            CatalogAction::Export(SessionExportTarget::Group("Empty".into())),
        )
        .unwrap();
    let empty = result(&client, empty).await;
    assert!(empty.error.is_none());
    assert_eq!(empty.revision, 0);
    let json: serde_json::Value = serde_json::from_str(&empty.exported.unwrap()).unwrap();
    assert_eq!(json["kind"], "group");
    assert_eq!(json["profiles"].as_array().unwrap().len(), 0);
    client
        .action(WorkspaceAction::OpenProfile(source.id))
        .unwrap();
    let open = wait(&client, |view| view.tabs.len() == 1).await;
    let pane = open.tabs[0].panes[0].id;
    let generation = client
        .terminal(pane)
        .unwrap()
        .unwrap()
        .view()
        .unwrap()
        .generation;
    let request = client
        .catalog(
            0,
            CatalogAction::Export(SessionExportTarget::Server(source.id)),
        )
        .unwrap();
    let exported = result(&client, request).await;
    assert!(exported.error.is_none());
    assert_eq!(exported.revision, 0);
    let text = exported.exported.unwrap();
    let request = client.catalog(0, CatalogAction::Import(text)).unwrap();
    assert!(result(&client, request).await.error.is_none());
    let current = wait(&client, |view| view.catalog_revision == 1).await;
    assert_eq!(*current.settings, *view.settings);
    assert_eq!(current.profiles.len(), 2);
    let imported = current.profiles.iter().find(|p| p.id != source.id).unwrap();
    assert_ne!(imported.id, Uuid::nil());
    assert_eq!(imported.name, format!("{} Copy", source.name));
    assert_eq!(current.tabs[0].panes[0].id, pane);
    let terminal = client.terminal(pane).unwrap().unwrap().view().unwrap();
    assert_eq!(terminal.generation, generation);
    assert!(!terminal.connected);
    let request = client
        .catalog(
            0,
            CatalogAction::Export(SessionExportTarget::Server(source.id)),
        )
        .unwrap();
    let stale = result(&client, request).await;
    assert!(stale.error.is_some());
    assert!(stale.exported.is_none());
    let saved = fixture.store.load().unwrap();
    assert_eq!(saved.sessions.len(), 2);
    assert_eq!(saved.settings, *view.settings);
    workspace.shutdown().await.unwrap();
}

#[tokio::test]
async fn invalid_and_failed_transfer_writes_leave_catalog_and_disk_unchanged_then_retry() {
    let fixture = transfer_fixture();
    let workspace = fixture.start();
    let client = workspace.client();
    let view = wait(&client, |view| view.ready).await;
    let saved = fixture.store.load().unwrap();
    let original = std::fs::read(fixture.store.path()).unwrap();
    for text in ["{}".to_owned(), "not JSON".into(), serde_json::json!({"format":"axssh-session-export","version":2,"kind":"group","group_name":"Injected","profiles":[]}).to_string()] {
        let request = client.catalog(0, CatalogAction::Import(text)).unwrap();
        assert!(result(&client, request).await.error.is_some());
        assert_eq!(client.view().unwrap().catalog_revision, 0);
        assert_eq!(std::fs::read(fixture.store.path()).unwrap(), original);
    }
    let text = export_session_profile(&saved, &view.profiles[0].id.to_string()).unwrap();
    let backup = fixture.directory.join("original.json");
    std::fs::rename(fixture.store.path(), &backup).unwrap();
    std::fs::create_dir(fixture.store.path()).unwrap();
    let request = client
        .catalog(0, CatalogAction::Import(text.clone()))
        .unwrap();
    assert!(result(&client, request).await.error.is_some());
    assert_eq!(*client.view().unwrap().profiles, *view.profiles);
    assert_eq!(client.view().unwrap().catalog_revision, 0);
    assert_eq!(std::fs::read(&backup).unwrap(), original);
    std::fs::remove_dir(fixture.store.path()).unwrap();
    std::fs::rename(backup, fixture.store.path()).unwrap();
    let request = client.catalog(0, CatalogAction::Import(text)).unwrap();
    assert!(result(&client, request).await.error.is_none());
    assert_eq!(fixture.store.load().unwrap().sessions.len(), 2);
    assert!(
        client
            .catalog(
                1,
                CatalogAction::Import(
                    "x".repeat(crate::session_transfer::MAX_SESSION_TRANSFER_BYTES + 1)
                )
            )
            .is_err()
    );
    workspace.shutdown().await.unwrap();
}

#[tokio::test]
async fn export_receipts_share_the_bounded_budget_and_shutdown_drains_accepted_requests() {
    let fixture = transfer_fixture();
    let workspace = fixture.start();
    let client = workspace.client();
    let view = wait(&client, |view| view.ready).await;
    let target = SessionExportTarget::Server(view.profiles[0].id);
    let requests: Vec<_> = (0..COMMAND_CAPACITY)
        .map(|_| {
            client
                .catalog(0, CatalogAction::Export(target.clone()))
                .unwrap()
        })
        .collect();
    assert!(
        client
            .catalog(0, CatalogAction::Export(target.clone()))
            .is_err()
    );
    workspace.shutdown().await.unwrap();
    assert!(client.catalog(0, CatalogAction::Export(target)).is_err());
    for request in requests {
        let receipt = client.take_catalog_result(request).unwrap().unwrap();
        assert!(receipt.error.is_none());
        assert!(receipt.exported.is_some());
    }
    assert_eq!(
        fixture.store.load().unwrap().sessions.len(),
        view.profiles.len()
    );
}
