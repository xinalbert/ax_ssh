use super::*;
use tokio::time::timeout;

struct Fixture {
    state: Arc<Mutex<AppState>>,
    profile: SessionProfile,
    tab: Uuid,
    target: ConnectionTarget,
}

impl Fixture {
    fn new(target: ConnectionTarget) -> Self {
        let profile = SessionProfile::new("Fixture", "127.0.0.1", "fixture");
        let mut app = AppState::new(
            ConfigStore::new(
                std::env::temp_dir().join(format!("axssh-start-{}.json", Uuid::new_v4())),
            ),
            SessionStore {
                sessions: vec![profile.clone()],
                ..Default::default()
            },
        );
        let tab = match target {
            ConnectionTarget::Terminal => app.open_terminal_tab(&profile),
            ConnectionTarget::Sftp => {
                app.open_sftp_tab_with_companion_at_path(&profile, None, Some("/initial".into()))
            }
        };
        Self {
            state: Arc::new(Mutex::new(app)),
            profile,
            tab,
            target,
        }
    }

    fn phase(&self, phase: SshConnectionPhase) {
        assert!(
            self.state
                .lock()
                .unwrap()
                .terminal_mut(self.tab)
                .unwrap()
                .set_ssh_phase(phase)
        );
    }

    fn start(
        &self,
        profile: SessionProfile,
        source: AuthenticationStart,
        target: ConnectionTarget,
    ) -> Result<()> {
        start_session_worker(
            &Handle::current(),
            self.state.clone(),
            slint::Weak::default(),
            self.tab,
            profile,
            zeroize::Zeroizing::new("fixture-only".into()),
            None,
            false,
            source,
            target,
        )
    }
}

#[tokio::test]
async fn authentication_transitions_require_matching_tab_profile_and_phase() {
    for target in [ConnectionTarget::Terminal, ConnectionTarget::Sftp] {
        let fixture = Fixture::new(target);
        assert!(!set_loading_stored_credential(
            &fixture.state,
            fixture.tab,
            fixture.profile.id
        ));
        assert!(!set_awaiting_authentication(
            &fixture.state,
            fixture.tab,
            fixture.profile.id,
            false
        ));
        fixture.phase(SshConnectionPhase::AwaitingAuthentication {
            vault_unlock_only: false,
        });
        assert!(!set_loading_stored_credential(
            &fixture.state,
            Uuid::new_v4(),
            fixture.profile.id
        ));
        assert!(!set_loading_stored_credential(
            &fixture.state,
            fixture.tab,
            Uuid::new_v4()
        ));
        assert!(set_loading_stored_credential(
            &fixture.state,
            fixture.tab,
            fixture.profile.id
        ));
        assert!(!set_loading_stored_credential(
            &fixture.state,
            fixture.tab,
            fixture.profile.id
        ));
        assert!(set_awaiting_authentication(
            &fixture.state,
            fixture.tab,
            fixture.profile.id,
            true
        ));
        assert!(matches!(
            fixture
                .state
                .lock()
                .unwrap()
                .terminal(fixture.tab)
                .unwrap()
                .ssh_phase(),
            Some(SshConnectionPhase::AwaitingAuthentication {
                vault_unlock_only: true
            })
        ));
        fixture.phase(SshConnectionPhase::Idle);
        assert!(!set_awaiting_authentication(
            &fixture.state,
            fixture.tab,
            fixture.profile.id,
            false
        ));
        let local = fixture.state.lock().unwrap().open_local_shell_tab();
        assert!(!terminal_has_phase(
            &fixture.state.lock().unwrap(),
            local,
            fixture.profile.id,
            |_| true
        ));
    }
}

#[tokio::test]
async fn worker_start_rejects_wrong_phase_changed_profile_and_target_without_creating_worker() {
    for target in [ConnectionTarget::Terminal, ConnectionTarget::Sftp] {
        let fixture = Fixture::new(target);
        assert!(
            fixture
                .start(fixture.profile.clone(), AuthenticationStart::Prompt, target)
                .unwrap_err()
                .to_string()
                .contains("not awaiting authentication")
        );
        fixture.phase(SshConnectionPhase::AwaitingAuthentication {
            vault_unlock_only: false,
        });
        assert!(
            fixture
                .start(
                    fixture.profile.clone(),
                    AuthenticationStart::StoredCredential,
                    target
                )
                .is_err()
        );
        let mut changed = fixture.profile.clone();
        changed.name = "changed".into();
        assert!(
            fixture
                .start(changed, AuthenticationStart::Prompt, target)
                .unwrap_err()
                .to_string()
                .contains("profile changed")
        );
        let wrong_target = match target {
            ConnectionTarget::Terminal => ConnectionTarget::Sftp,
            ConnectionTarget::Sftp => ConnectionTarget::Terminal,
        };
        assert!(
            fixture
                .start(
                    fixture.profile.clone(),
                    AuthenticationStart::Prompt,
                    wrong_target
                )
                .unwrap_err()
                .to_string()
                .contains("already has a worker")
        );
        assert!(
            fixture
                .state
                .lock()
                .unwrap()
                .terminal(fixture.tab)
                .unwrap()
                .worker
                .is_none()
        );
        fixture.state.lock().unwrap().sessions.sessions.clear();
        assert!(
            fixture
                .start(fixture.profile.clone(), AuthenticationStart::Prompt, target)
                .is_err()
        );
    }
}

#[tokio::test]
async fn terminal_and_sftp_workers_start_once_and_shutdown_with_their_owner() {
    for target in [ConnectionTarget::Terminal, ConnectionTarget::Sftp] {
        // Hold a loopback listener without speaking SSH. No external destination,
        // system known_hosts write, credential store or host-key bypass is used.
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let mut fixture = Fixture::new(target);
        fixture.profile.ssh_mut().unwrap().port = listener.local_addr().unwrap().port();
        fixture.state.lock().unwrap().sessions.sessions = vec![fixture.profile.clone()];
        for source in [
            AuthenticationStart::Prompt,
            AuthenticationStart::StoredCredential,
        ] {
            fixture.phase(match source {
                AuthenticationStart::Prompt => SshConnectionPhase::AwaitingAuthentication {
                    vault_unlock_only: false,
                },
                AuthenticationStart::StoredCredential => {
                    SshConnectionPhase::LoadingStoredCredential
                }
            });
            fixture
                .start(fixture.profile.clone(), source, fixture.target)
                .unwrap();
            {
                let app = fixture.state.lock().unwrap();
                let terminal = app.terminal(fixture.tab).unwrap();
                assert!(
                    terminal.worker_running && !terminal.connected && terminal.reconnect_enabled
                );
                assert!(matches!(terminal.worker, Some(TerminalWorker::Ssh(_))));
                assert!(terminal.ssh_route().unwrap().1.is_some());
                assert!(matches!(
                    terminal.ssh_phase(),
                    Some(SshConnectionPhase::Idle)
                ));
                assert!(terminal.status.starts_with("Connecting to"));
                assert_eq!(
                    app.sessions.sessions[0], fixture.profile,
                    "SFTP path override is volatile"
                );
            }
            fixture.phase(SshConnectionPhase::AwaitingAuthentication {
                vault_unlock_only: false,
            });
            assert!(
                fixture
                    .start(fixture.profile.clone(), AuthenticationStart::Prompt, target)
                    .is_err()
            );
            let worker = {
                let mut app = fixture.state.lock().unwrap();
                let terminal = app.terminal_mut(fixture.tab).unwrap();
                terminal.cancel_reconnect();
                terminal.set_ssh_attempt(None);
                terminal.worker.take().unwrap()
            };
            timeout(Duration::from_secs(3), worker.shutdown())
                .await
                .unwrap()
                .unwrap();
        }
        drop(listener);
    }
}
