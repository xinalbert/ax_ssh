use super::*;
use tokio::sync::oneshot;
use tokio::time::{Duration, timeout};

struct Fixture {
    state: Arc<Mutex<AppState>>,
    persistence: Arc<PersistenceCoordinator>,
    config: ConfigStore,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("axssh-session-transaction-{}", Uuid::new_v4()));
        let config = ConfigStore::new(root.join("sessions.json"));
        let mut sessions = SessionStore::default();
        sessions.upsert(SessionProfile::new("Source", "fixture.invalid", "fixture"));
        config.save(&sessions).expect("fixture config");
        let app = AppState::new(config.clone(), sessions);
        let persistence = app.persistence_coordinator.clone();
        Self {
            state: Arc::new(Mutex::new(app)),
            persistence,
            config,
            root,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).expect("remove fixture config");
    }
}

#[tokio::test]
async fn slow_disk_does_not_lock_state_or_persist_settings_previews() {
    let fixture = Fixture::new();
    let committed = fixture.config.load().unwrap();
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    let state = fixture.state.clone();
    let persistence = fixture.persistence.clone();
    let task = tokio::spawn(async move {
        update_session_store_with(
            &state,
            &persistence,
            |candidate| {
                candidate.add_group("New group")?;
                Ok(())
            },
            move |config, candidate| {
                started_tx.send(()).expect("notify paused disk write");
                release_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("release disk write");
                config.save(candidate)
            },
        )
        .await
    });
    timeout(Duration::from_secs(5), started_rx)
        .await
        .unwrap()
        .unwrap();
    {
        let mut app = fixture
            .state
            .try_lock()
            .expect("UI can access state while disk is paused");
        assert!(
            app.sessions.groups.is_empty(),
            "candidate must not publish before persistence"
        );
        app.sessions.settings.appearance.application_font_family = "Preview while saving".into();
    }
    assert!(fixture.persistence.gate.try_lock().is_err());
    release_tx.send(()).unwrap();
    timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let disk = fixture.config.load().unwrap();
    assert_eq!(disk.groups, ["New group"]);
    assert_eq!(disk.settings, committed.settings);
    let app = fixture.state.lock().unwrap();
    assert_eq!(app.sessions.groups, disk.groups);
    assert_eq!(
        app.sessions.settings.appearance.application_font_family,
        "Preview while saving"
    );
}

#[tokio::test]
async fn failed_disk_write_leaves_memory_and_persisted_store_unchanged() {
    let fixture = Fixture::new();
    let before = fixture.config.load().unwrap();
    let result = update_session_store_with(
        &fixture.state,
        &fixture.persistence,
        |candidate| {
            candidate.add_group("Must not publish")?;
            Ok(())
        },
        |_, _| anyhow::bail!("injected disk failure"),
    )
    .await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("injected disk failure")
    );
    assert_eq!(fixture.state.lock().unwrap().sessions, before);
    assert_eq!(fixture.config.load().unwrap(), before);
}

#[tokio::test]
async fn cancelling_async_caller_keeps_disk_serialized_until_blocking_commit_finishes() {
    let fixture = Fixture::new();
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    let state = fixture.state.clone();
    let persistence = fixture.persistence.clone();
    let first = tokio::spawn(async move {
        update_session_store_with(
            &state,
            &persistence,
            |candidate| {
                candidate.add_group("First")?;
                Ok(())
            },
            move |config, candidate| {
                started_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                config.save(candidate)
            },
        )
        .await
    });
    timeout(Duration::from_secs(5), started_rx)
        .await
        .unwrap()
        .unwrap();
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    assert!(
        fixture.persistence.gate.try_lock().is_err(),
        "cancelled caller must not release an in-flight write's gate"
    );
    release_tx.send(()).unwrap();
    timeout(
        Duration::from_secs(5),
        update_session_group(
            &fixture.state,
            &fixture.persistence,
            "new-group",
            "",
            "Second",
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let saved = fixture.config.load().unwrap();
    assert_eq!(saved.groups, ["First", "Second"]);
    assert_eq!(saved, fixture.state.lock().unwrap().sessions);
}

#[tokio::test]
async fn imported_profiles_are_persisted_without_credentials_or_host_trust() {
    let fixture = Fixture::new();
    let mut source = fixture.state.lock().unwrap().sessions.sessions[0].clone();
    let ssh = source.ssh_mut().unwrap();
    ssh.credential_storage = Some(CredentialStorage::SystemKeyring);
    ssh.host_key_fingerprint = Some("SHA256:fixture".into());
    let exported = serialize_session_transfer(SessionTransferEnvelope {
        format: SESSION_TRANSFER_FORMAT.into(),
        version: SESSION_TRANSFER_VERSION,
        kind: SessionTransferKind::Server,
        group_name: None,
        profiles: vec![source.clone()],
    })
    .unwrap();
    import_session_transfer(
        &fixture.state,
        &fixture.persistence,
        &exported,
        SessionImportMode::Automatic,
        "",
    )
    .await
    .unwrap();
    let saved = fixture.config.load().unwrap();
    assert_eq!(saved.sessions.len(), 2);
    let imported = saved
        .sessions
        .iter()
        .find(|profile| profile.id != source.id)
        .unwrap();
    let ssh = imported.ssh().unwrap();
    assert!(ssh.credential_storage.is_none());
    assert!(ssh.host_key_fingerprint.is_none());
    assert_eq!(saved, fixture.state.lock().unwrap().sessions);
}
