use super::*;
use ax_ssh::config::CredentialStorage;

#[test]
fn credential_choice_callbacks_require_explicit_manual_entry_and_allow_retry_cancel_disconnect() {
    isolated(
        "credential_access::credential_choice_callbacks_require_explicit_manual_entry_and_allow_retry_cancel_disconnect",
        || {
            let harness = Harness::new();
            assert!(
                window_router::GLOBAL_WINDOW_ROUTER
                    .set(harness.router.clone())
                    .is_ok()
            );
            let tab_id = {
                let mut app = harness.state.lock().unwrap();
                let ssh = app.sessions.sessions[0].ssh_mut().unwrap();
                ssh.credential_storage = Some(CredentialStorage::EncryptedVault);
                ssh.credential_vault_key_saved = false;
                let profile = app.sessions.sessions[0].clone();
                let tab_id = app.open_terminal_tab(&profile);
                assert!(harness.router.include_tab(MAIN_WINDOW_ID, tab_id));
                assert!(
                    harness
                        .router
                        .activate_tab(MAIN_WINDOW_ID, tab_id, &mut app)
                );
                tab_id
            };
            let choice = || {
                harness
                    .state
                    .lock()
                    .unwrap()
                    .terminal_mut(tab_id)
                    .unwrap()
                    .set_ssh_phase(SshConnectionPhase::StoredCredentialUnavailable {
                        vault_unlock_only: true,
                    });
                refresh_workspace(&harness.ui.as_weak(), &harness.state);
                harness.status();
                assert!(harness.ui.get_password_dialog_open());
                assert!(harness.ui.get_password_credential_unavailable());
            };
            choice();
            assert!(
                !harness.ui.invoke_authenticate_session(
                    "test-only".into(),
                    "".into(),
                    false,
                    "system-keyring".into()
                ),
                "a pending choice must reject password submission"
            );
            harness.ui.invoke_resolve_credential_access(false);
            harness.status();
            assert!(harness.ui.get_password_dialog_open());
            assert!(!harness.ui.get_password_credential_unavailable());
            assert!(!harness.ui.get_password_vault_unlock_only());
            choice();
            harness.ui.invoke_resolve_credential_access(true);
            harness.status();
            assert!(!harness.ui.get_password_credential_unavailable());
            assert!(
                harness.ui.get_password_vault_unlock_only(),
                "retry must return to vault unlock, not server password entry"
            );
            choice();
            harness.ui.invoke_cancel_password_dialog();
            harness.status();
            assert!(!harness.ui.get_password_dialog_open());
            harness.ui.invoke_resolve_credential_access(false);
            assert!(matches!(
                harness
                    .state
                    .lock()
                    .unwrap()
                    .terminal(tab_id)
                    .unwrap()
                    .ssh_phase(),
                Some(SshConnectionPhase::Idle)
            ));
            harness
                .state
                .lock()
                .unwrap()
                .terminal_mut(tab_id)
                .unwrap()
                .set_ssh_phase(SshConnectionPhase::LoadingStoredCredential(Uuid::new_v4()));
            harness.ui.invoke_disconnect_session();
            harness.status();
            assert!(matches!(
                harness
                    .state
                    .lock()
                    .unwrap()
                    .terminal(tab_id)
                    .unwrap()
                    .ssh_phase(),
                Some(SshConnectionPhase::Idle)
            ));
            assert!(!harness.ui.get_password_dialog_open());
            assert_eq!(
                harness.state.lock().unwrap().sessions.sessions[0]
                    .ssh()
                    .unwrap()
                    .credential_storage,
                Some(CredentialStorage::EncryptedVault)
            );
        },
    );
}
