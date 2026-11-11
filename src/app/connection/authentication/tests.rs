use super::*;

struct Fixture {
    state: Arc<Mutex<AppState>>,
    profile: SessionProfile,
    tab_id: Uuid,
    directory: PathBuf,
    target: ConnectionTarget,
}

impl Fixture {
    fn new(storage: CredentialStorage, automatic: bool, target: ConnectionTarget) -> Self {
        let directory =
            std::env::temp_dir().join(format!("axssh-credential-flow-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let mut profile = SessionProfile::new("Credential fixture", "127.0.0.1", "fixture");
        let ssh = profile.ssh_mut().unwrap();
        ssh.credential_storage = Some(storage);
        ssh.credential_vault_key_saved = automatic;
        let mut app = AppState::new(
            ConfigStore::new(directory.join("config.json")),
            SessionStore {
                sessions: vec![profile.clone()],
                ..Default::default()
            },
        );
        let tab_id = match target {
            ConnectionTarget::Terminal => app.open_terminal_tab(&profile),
            ConnectionTarget::Sftp => {
                app.open_sftp_tab_with_companion_at_path(&profile, None, None)
            }
        };
        Self {
            state: Arc::new(Mutex::new(app)),
            profile,
            tab_id,
            directory,
            target,
        }
    }

    fn lookup(&self) -> CredentialLookup {
        self.state
            .lock()
            .unwrap()
            .terminal_mut(self.tab_id)
            .unwrap()
            .set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
                vault_unlock_only: false,
            });
        CredentialLookup {
            tab_id: self.tab_id,
            profile: self.profile.clone(),
            target: self.target,
            lookup_id: set_loading_stored_credential(&self.state, self.tab_id, self.profile.id)
                .unwrap(),
            vault_unlock_only: self.profile.ssh().unwrap().credential_storage
                == Some(CredentialStorage::EncryptedVault)
                && !self.profile.ssh().unwrap().credential_vault_key_saved,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).unwrap();
    }
}

#[test]
fn prompt_storage_uses_explicit_selection_only_when_remembering() {
    assert_eq!(
        selected_storage_for_remember(false, "encrypted-vault"),
        None
    );
    assert_eq!(
        selected_storage_for_remember(true, "encrypted-vault"),
        Some(CredentialStorage::EncryptedVault)
    );
    assert_eq!(
        selected_storage_for_remember(true, "system-keyring"),
        Some(CredentialStorage::SystemKeyring)
    );
}

#[tokio::test]
async fn denied_or_unavailable_credentials_require_a_choice_and_preserve_saved_credentials() {
    for target in [ConnectionTarget::Terminal, ConnectionTarget::Sftp] {
        for (storage, automatic) in [
            (CredentialStorage::SystemKeyring, false),
            (CredentialStorage::EncryptedVault, true),
            (CredentialStorage::EncryptedVault, false),
        ] {
            for reason in [
                "authorization cancelled",
                "access denied",
                "service unavailable",
                "vault password incorrect",
            ] {
                let fixture = Fixture::new(storage, automatic, target);
                fixture
                    .lookup()
                    .complete(
                        &Handle::current(),
                        fixture.state.clone(),
                        slint::Weak::default(),
                        Err(anyhow::anyhow!(reason)),
                    )
                    .await;
                let app = fixture.state.lock().unwrap();
                let terminal = app.terminal(fixture.tab_id).unwrap();
                assert!(matches!(
                    terminal.ssh_phase(),
                    Some(SshConnectionPhase::StoredCredentialUnavailable { .. })
                ));
                assert!(terminal.worker.is_none());
                assert!(terminal.status.contains(reason));
                assert!(matches!(
                    app.active_security_prompt(),
                    ActiveSecurityPrompt::Authentication {
                        credential_unavailable: true,
                        ..
                    }
                ));
                assert_eq!(
                    app.sessions.sessions[0], fixture.profile,
                    "read errors must retain the saved credential reference"
                );
                assert!(!fixture.directory.join("config.json").exists());
            }
        }
    }
}

#[tokio::test]
async fn missing_password_requests_server_password_and_clears_only_the_missing_reference() {
    for target in [ConnectionTarget::Terminal, ConnectionTarget::Sftp] {
        for storage in [
            CredentialStorage::SystemKeyring,
            CredentialStorage::EncryptedVault,
        ] {
            // No native store is touched: this tests the actual result handler
            // with a missing system entry or a missing user-unlocked vault file.
            let fixture = Fixture::new(storage, false, target);
            fixture
                .lookup()
                .complete(
                    &Handle::current(),
                    fixture.state.clone(),
                    slint::Weak::default(),
                    Ok(None),
                )
                .await;
            let app = fixture.state.lock().unwrap();
            assert!(matches!(
                app.active_security_prompt(),
                ActiveSecurityPrompt::Authentication {
                    vault_unlock_only: false,
                    credential_unavailable: false,
                    ..
                }
            ));
            assert_eq!(
                app.sessions.sessions[0].ssh().unwrap().credential_storage,
                None
            );
            assert_eq!(
                app.config.load().unwrap().sessions[0]
                    .ssh()
                    .unwrap()
                    .credential_storage,
                None
            );
        }
    }
}

#[tokio::test]
async fn obsolete_credential_results_cannot_start_workers_clear_policy_or_replace_a_new_lookup() {
    for result_kind in 0..3 {
        for change in 0..4 {
            let fixture = Fixture::new(
                CredentialStorage::SystemKeyring,
                false,
                ConnectionTarget::Terminal,
            );
            let old = fixture.lookup();
            let new_id = {
                let mut app = fixture.state.lock().unwrap();
                match change {
                    0 => {
                        app.terminal_mut(fixture.tab_id)
                            .unwrap()
                            .set_ssh_phase(SshConnectionPhase::Idle);
                        None
                    }
                    1 => {
                        let id = Uuid::new_v4();
                        app.terminal_mut(fixture.tab_id)
                            .unwrap()
                            .set_ssh_phase(SshConnectionPhase::LoadingStoredCredential(id));
                        Some(id)
                    }
                    2 => {
                        app.sessions.sessions[0].name = "edited during authorization".into();
                        None
                    }
                    _ => {
                        app.close_tab(fixture.tab_id).unwrap();
                        None
                    }
                }
            };
            let result = match result_kind {
                0 => Ok(Some(zeroize::Zeroizing::new("test-only-password".into()))),
                1 => Ok(None),
                _ => Err(anyhow::anyhow!("test-only denial")),
            };
            old.complete(
                &Handle::current(),
                fixture.state.clone(),
                slint::Weak::default(),
                result,
            )
            .await;
            let app = fixture.state.lock().unwrap();
            assert_eq!(
                app.sessions.sessions[0].ssh().unwrap().credential_storage,
                Some(CredentialStorage::SystemKeyring)
            );
            if let Some(terminal) = app.terminal(fixture.tab_id) {
                assert!(terminal.worker.is_none());
                if let Some(new_id) = new_id {
                    assert!(
                        matches!(terminal.ssh_phase(), Some(SshConnectionPhase::LoadingStoredCredential(id)) if *id == new_id)
                    );
                } else if change == 0 {
                    assert!(matches!(
                        terminal.ssh_phase(),
                        Some(SshConnectionPhase::Idle)
                    ));
                }
            }
            assert!(!fixture.directory.join("config.json").exists());
        }
    }
}

#[tokio::test]
async fn credential_missing_while_waiting_for_persistence_cannot_clear_a_new_lookup() {
    let fixture = Fixture::new(
        CredentialStorage::SystemKeyring,
        false,
        ConnectionTarget::Terminal,
    );
    let lookup = fixture.lookup();
    let gate = fixture
        .state
        .lock()
        .unwrap()
        .persistence_coordinator
        .gate
        .clone();
    let guard = gate.lock().await;
    let runtime = Handle::current();
    let mut completion = Box::pin(lookup.complete(
        &runtime,
        fixture.state.clone(),
        slint::Weak::default(),
        Ok(None),
    ));
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(
            std::future::Future::poll(completion.as_mut(), cx).is_pending()
        ))
        .await
    );
    let new_id = Uuid::new_v4();
    fixture
        .state
        .lock()
        .unwrap()
        .terminal_mut(fixture.tab_id)
        .unwrap()
        .set_ssh_phase(SshConnectionPhase::LoadingStoredCredential(new_id));
    drop(guard);
    completion.await;
    assert!(!fixture.directory.join("config.json").exists());
    assert!(
        matches!(fixture.state.lock().unwrap().terminal(fixture.tab_id).unwrap().ssh_phase(), Some(SshConnectionPhase::LoadingStoredCredential(id)) if *id == new_id)
    );
}

#[tokio::test]
async fn successful_credential_read_starts_connection_without_another_password_prompt() {
    for target in [ConnectionTarget::Terminal, ConnectionTarget::Sftp] {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let mut fixture = Fixture::new(CredentialStorage::SystemKeyring, false, target);
        fixture.profile.ssh_mut().unwrap().port = listener.local_addr().unwrap().port();
        fixture.state.lock().unwrap().sessions.sessions[0] = fixture.profile.clone();
        fixture
            .lookup()
            .complete(
                &Handle::current(),
                fixture.state.clone(),
                slint::Weak::default(),
                Ok(Some(zeroize::Zeroizing::new("test-only-password".into()))),
            )
            .await;
        let worker = {
            let mut app = fixture.state.lock().unwrap();
            assert!(matches!(
                app.active_security_prompt(),
                ActiveSecurityPrompt::None
            ));
            let terminal = app.terminal_mut(fixture.tab_id).unwrap();
            assert!(terminal.worker_running);
            terminal.cancel_reconnect();
            terminal.set_ssh_attempt(None);
            terminal.worker.take().unwrap()
        };
        tokio::time::timeout(Duration::from_secs(3), worker.shutdown())
            .await
            .unwrap()
            .unwrap();
    }
}
