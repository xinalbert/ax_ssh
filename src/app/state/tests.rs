use super::*;
use ax_ssh::config::{AuthMethod, CredentialStorage};

fn test_state() -> AppState {
    AppState::new(
        ConfigStore::new(std::env::temp_dir().join(format!("ax-ssh-tabs-{}.json", Uuid::new_v4()))),
        SessionStore::default(),
    )
}

#[test]
fn ui_refresh_gate_includes_requests_that_arrive_before_the_snapshot() {
    let mut state = test_state();
    let tab_id = Uuid::new_v4();

    assert!(state.request_terminal_ui_refresh(tab_id, None));
    assert!(!state.request_full_ui_refresh());
    let batch = state
        .take_ui_refresh_batch()
        .expect("pending refresh batch");
    assert!(batch.full);
    assert!(batch.terminal_ids.is_empty());
    assert_eq!(batch.coalesced_requests, 1);
    assert!(!state.finish_ui_refresh(batch.generation));

    assert!(state.request_terminal_ui_refresh(tab_id, None));
}

#[test]
fn ui_refresh_gate_coalesces_terminal_output_before_snapshot_without_a_follow_up() {
    let mut state = test_state();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();

    assert!(state.request_terminal_ui_refresh(first, None));
    assert!(!state.request_terminal_ui_refresh(second, None));
    let batch = state
        .take_ui_refresh_batch()
        .expect("coalesced refresh batch");
    assert_eq!(batch.terminal_ids, HashSet::from([first, second]));
    assert_eq!(batch.coalesced_requests, 1);
    assert!(!state.finish_ui_refresh(batch.generation));
}

#[test]
fn ui_refresh_gate_follows_up_only_for_requests_after_the_snapshot() {
    let mut state = test_state();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();

    assert!(state.request_terminal_ui_refresh(first, None));
    let batch = state
        .take_ui_refresh_batch()
        .expect("pending refresh batch");
    assert_eq!(batch.terminal_ids, HashSet::from([first]));
    assert!(!state.request_terminal_ui_refresh(second, None));
    assert!(state.finish_ui_refresh(batch.generation));

    let follow_up = state
        .take_ui_refresh_batch()
        .expect("request after snapshot must remain pending");
    assert_eq!(follow_up.terminal_ids, HashSet::from([second]));
    assert_eq!(follow_up.coalesced_requests, 1);
    assert!(!state.finish_ui_refresh(follow_up.generation));
}

#[test]
fn same_profile_opens_independent_terminal_tabs() {
    let mut state = test_state();
    let profile = SessionProfile::new("Local", "localhost", "alice");

    let first = state.open_terminal_tab(&profile);
    let second = state.open_terminal_tab(&profile);

    assert_ne!(first, second);
    assert_eq!(state.tab_summaries().len(), 2);
    assert_eq!(state.tab_summaries()[0].title, "Local #1");
    assert_eq!(state.tab_summaries()[1].title, "Local #2");
    assert_eq!(state.active_tab_id(), Some(second));
}

#[test]
fn terminal_notice_shows_retry_for_exhausted_reconnects() {
    let mut state = test_state();
    let profile = SessionProfile::new("remote", "remote.example", "alice");
    let tab_id = state.open_terminal_tab(&profile);
    let terminal = state
        .terminal_mut(tab_id)
        .expect("terminal tab should exist");
    terminal.worker_running = false;
    terminal.status = "Reconnect failed after 5 attempts; retry manually".to_owned();

    let notice = terminal.notice_snapshot();

    assert!(notice.visible);
    assert_eq!(notice.severity, "error");
    assert_eq!(notice.primary_action, "retry");
    assert_eq!(notice.primary_label, "Retry");
    assert_eq!(notice.secondary_action, "close-tab");
}

#[test]
fn terminal_notice_hides_security_prompt_phases() {
    let mut state = test_state();
    let profile = SessionProfile::new("remote", "remote.example", "alice");
    let tab_id = state.open_terminal_tab(&profile);
    let terminal = state
        .terminal_mut(tab_id)
        .expect("terminal tab should exist");
    terminal.status = "Cannot start password connection: missing password".to_owned();
    assert!(
        terminal.set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
            vault_unlock_only: false,
        })
    );

    assert!(!terminal.notice_snapshot().visible);
}

#[test]
fn terminal_notice_hides_user_requested_disconnect() {
    let mut state = test_state();
    let profile = SessionProfile::new("remote", "remote.example", "alice");
    let tab_id = state.open_terminal_tab(&profile);
    let terminal = state
        .terminal_mut(tab_id)
        .expect("terminal tab should exist");
    terminal.cancel_reconnect();
    terminal.status = "Disconnected".to_owned();

    assert!(!terminal.notice_snapshot().visible);
}

#[test]
fn terminal_notice_allows_local_shell_restart_after_failure() {
    let mut state = test_state();
    let tab_id = state.open_local_shell_tab();
    let terminal = state
        .terminal_mut(tab_id)
        .expect("local terminal tab should exist");
    terminal.worker_running = false;
    terminal.status = "Local shell failed: process exited before startup".to_owned();

    let notice = terminal.notice_snapshot();

    assert!(notice.visible);
    assert_eq!(notice.primary_action, "retry");
    assert_eq!(notice.primary_label, "Restart");
    assert_eq!(notice.secondary_action, "close-tab");
}

#[test]
fn terminal_notice_stays_with_its_tab_when_switching_tabs() {
    let mut state = test_state();
    let first = state.open_local_shell_tab();
    let second = state.open_local_shell_tab();
    let first_terminal = state
        .terminal_mut(first)
        .expect("first terminal tab should exist");
    first_terminal.worker_running = false;
    first_terminal.status = "Local shell failed: process exited".to_owned();

    assert!(state.activate_tab(first));
    assert!(state.active_snapshot().notice.visible);

    assert!(state.activate_tab(second));
    assert!(!state.active_snapshot().notice.visible);

    assert!(state.activate_tab(first));
    assert_eq!(state.active_snapshot().notice.primary_label, "Restart");
}

#[test]
fn workspace_transfer_keeps_ssh_and_sftp_companion_together() {
    let mut state = test_state();
    let profile = SessionProfile::new("remote", "remote.example", "alice");
    let terminal_id = state.open_terminal_tab(&profile);
    let sftp_id = state.open_sftp_tab_with_companion(&profile, Some(terminal_id));

    let source_window_id = Uuid::new_v4();
    let transfer = state
        .workspace_transfer_for(terminal_id, source_window_id)
        .expect("terminal should be transferable");

    assert_eq!(transfer.source_window_id, source_window_id);
    assert_eq!(transfer.active_tab_id, Some(terminal_id));
    assert_eq!(transfer.tab_ids, vec![terminal_id, sftp_id]);
    assert_eq!(state.tab_summaries_for(&transfer.tab_ids).len(), 2);
    assert_eq!(state.snapshot_for(Some(sftp_id)).id, Some(sftp_id));
}

#[test]
fn terminal_pane_transfer_keeps_every_terminal_and_its_companion() {
    let mut state = test_state();
    let first_profile = SessionProfile::new("first", "first.example", "alice");
    let second_profile = SessionProfile::new("second", "second.example", "bob");
    let first_terminal = state.open_terminal_tab(&first_profile);
    let second_terminal = state.open_terminal_tab(&second_profile);
    let sftp_id = state.open_sftp_tab_with_companion(&first_profile, Some(first_terminal));

    let transfer = state
        .workspace_transfer_for_terminal_panes(
            &[first_terminal, second_terminal],
            Uuid::new_v4(),
            second_terminal,
        )
        .expect("terminal panes should be transferable");

    assert_eq!(transfer.active_tab_id, Some(second_terminal));
    assert!(transfer.tab_ids.contains(&first_terminal));
    assert!(transfer.tab_ids.contains(&second_terminal));
    assert!(transfer.tab_ids.contains(&sftp_id));
    assert_eq!(transfer.tab_ids.len(), 3);
}

#[test]
fn standalone_sftp_tab_can_transfer_without_becoming_a_terminal_pane() {
    let mut state = test_state();
    let profile = SessionProfile::new("remote", "remote.example", "alice");
    let sftp_id = state.open_sftp_tab(&profile);

    let transfer = state
        .workspace_transfer_for_sftp(sftp_id, Uuid::new_v4())
        .expect("standalone SFTP tab should be transferable");

    assert_eq!(transfer.tab_ids, vec![sftp_id]);
    assert_eq!(transfer.active_tab_id, Some(sftp_id));
}

#[test]
fn new_ssh_terminal_does_not_inherit_a_one_time_password() {
    let mut state = test_state();
    let profile = SessionProfile::new("remote", "remote.example", "alice");
    let source = state.open_terminal_tab(&profile);
    assert!(
        state
            .terminal_mut(source)
            .expect("source terminal should exist")
            .set_pending_auth_secret(zeroize::Zeroizing::new("temporary-password".to_owned()))
    );

    let child = state.open_terminal_tab(&profile);

    assert!(
        state
            .terminal_mut(child)
            .expect("child terminal should exist")
            .take_pending_auth_secret()
            .is_none()
    );
    assert_eq!(
        state
            .terminal_mut(source)
            .expect("source terminal should remain")
            .take_pending_auth_secret()
            .expect("source one-time password should remain scoped to its tab")
            .as_str(),
        "temporary-password"
    );
}

#[test]
fn non_terminal_tabs_cannot_be_detached() {
    let mut state = test_state();
    let settings_id = state.open_settings_tab();

    assert!(
        state
            .workspace_transfer_for(settings_id, Uuid::new_v4())
            .is_none()
    );
}

#[test]
fn each_ssh_tab_keeps_its_own_authentication_prompt() {
    let mut state = test_state();
    let mut first_profile = SessionProfile::new("first", "first.example", "alice");
    first_profile
        .ssh_mut()
        .expect("profile should be SSH")
        .host_key_fingerprint = Some("SHA256:first".into());
    let mut second_profile = SessionProfile::new("second", "second.example", "bob");
    second_profile
        .ssh_mut()
        .expect("profile should be SSH")
        .host_key_fingerprint = Some("SHA256:second".into());
    state.sessions.upsert(first_profile.clone());
    state.sessions.upsert(second_profile.clone());

    let first_tab = state.open_terminal_tab(&first_profile);
    let second_tab = state.open_terminal_tab(&second_profile);
    state
        .terminal_mut(first_tab)
        .expect("first terminal should exist")
        .set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
            vault_unlock_only: false,
        });
    state
        .terminal_mut(second_tab)
        .expect("second terminal should exist")
        .set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
            vault_unlock_only: true,
        });

    assert!(state.activate_tab(first_tab));
    match state.active_security_prompt() {
        ActiveSecurityPrompt::Authentication {
            tab_id,
            profile,
            vault_unlock_only,
        } => {
            assert_eq!(tab_id, first_tab);
            assert_eq!(profile.id, first_profile.id);
            assert!(!vault_unlock_only);
        }
        ActiveSecurityPrompt::None | ActiveSecurityPrompt::HostKey(_) => {
            panic!("first tab should render its authentication prompt")
        }
    }

    assert!(state.activate_tab(second_tab));
    match state.active_security_prompt() {
        ActiveSecurityPrompt::Authentication {
            tab_id,
            profile,
            vault_unlock_only,
        } => {
            assert_eq!(tab_id, second_tab);
            assert_eq!(profile.id, second_profile.id);
            assert!(vault_unlock_only);
        }
        ActiveSecurityPrompt::None | ActiveSecurityPrompt::HostKey(_) => {
            panic!("second tab should render its authentication prompt")
        }
    }
}

#[test]
fn one_time_password_is_tab_scoped_and_cleared_on_idle() {
    let mut state = test_state();
    let profile = SessionProfile::new("temporary", "temporary.example", "alice");
    let tab_id = state.open_terminal_tab(&profile);
    let terminal = state.terminal_mut(tab_id).expect("terminal should exist");

    assert!(
        terminal.set_pending_auth_secret(zeroize::Zeroizing::new("temporary-password".to_owned()))
    );
    assert_eq!(
        terminal
            .take_pending_auth_secret()
            .expect("password should be available once")
            .as_str(),
        "temporary-password"
    );
    assert!(terminal.take_pending_auth_secret().is_none());

    assert!(
        terminal.set_pending_auth_secret(zeroize::Zeroizing::new("host-key-password".to_owned()))
    );
    assert!(
        terminal.set_ssh_phase(SshConnectionPhase::AwaitingHostKey(PendingHostKey {
            tab_id,
            profile_id: profile.id,
            host: "temporary.example".to_owned(),
            port: 22,
            fingerprint: "SHA256:temporary".to_owned(),
            public_key: None,
            changed: false,
            revoked: false,
        }))
    );
    assert_eq!(
        terminal
            .take_pending_auth_secret()
            .expect("host-key confirmation should preserve the one-time password")
            .as_str(),
        "host-key-password"
    );

    assert!(
        terminal.set_pending_auth_secret(zeroize::Zeroizing::new("second-password".to_owned()))
    );
    assert!(terminal.set_ssh_phase(SshConnectionPhase::Idle));
    assert!(terminal.take_pending_auth_secret().is_none());
}

#[test]
fn closing_one_pending_tab_keeps_another_tabs_authentication_prompt() {
    let mut state = test_state();
    let first_profile = SessionProfile::new("first", "first.example", "alice");
    let mut second_profile = SessionProfile::new("second", "second.example", "bob");
    second_profile
        .ssh_mut()
        .expect("profile should be SSH")
        .host_key_fingerprint = Some("SHA256:second".into());
    state.sessions.upsert(first_profile.clone());
    state.sessions.upsert(second_profile.clone());

    let first_tab = state.open_terminal_tab(&first_profile);
    let second_tab = state.open_terminal_tab(&second_profile);
    let (first_cancel, mut first_cancelled) = oneshot::channel();
    state
        .terminal_mut(first_tab)
        .expect("first terminal should exist")
        .set_ssh_phase(SshConnectionPhase::Probing(PendingProbe {
            tab_id: first_tab,
            profile_id: first_profile.id,
            cancel: first_cancel,
        }));
    state
        .terminal_mut(second_tab)
        .expect("second terminal should exist")
        .set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
            vault_unlock_only: false,
        });

    let closed = state.close_tab(first_tab).expect("first tab should close");
    closed
        .pending_probe
        .expect("closing a probing tab should return its cancellation sender")
        .cancel
        .send(())
        .expect("probe task should still receive cancellation");
    assert!(first_cancelled.try_recv().is_ok());
    assert!(state.activate_tab(second_tab));
    assert!(matches!(
        state.active_security_prompt(),
        ActiveSecurityPrompt::Authentication { tab_id, profile, .. }
            if tab_id == second_tab && profile.id == second_profile.id
    ));
}

#[test]
fn stale_retry_from_a_closed_duplicate_tab_is_ignored() {
    let mut state = test_state();
    let profile = SessionProfile::new("duplicate", "duplicate.example", "alice");
    let first_tab = state.open_terminal_tab(&profile);
    let second_tab = state.open_terminal_tab(&profile);
    let first_attempt = Uuid::new_v4();
    let second_attempt = Uuid::new_v4();
    state
        .terminal_mut(first_tab)
        .expect("first terminal should exist")
        .set_ssh_attempt(Some(first_attempt));
    state
        .terminal_mut(second_tab)
        .expect("second terminal should exist")
        .set_ssh_attempt(Some(second_attempt));
    state.close_tab(first_tab).expect("first tab should close");
    let state = Arc::new(Mutex::new(state));

    assert!(
        !prepare_authentication_retry(&state, first_tab, profile.id, first_attempt,)
            .expect("stale retry should be ignored without error")
    );
    let state = state.lock().expect("state should remain readable");
    assert_eq!(
        state
            .terminal(second_tab)
            .and_then(TerminalTabState::ssh_route),
        Some((profile.id, Some(second_attempt)))
    );
    assert!(matches!(
        state
            .terminal(second_tab)
            .and_then(TerminalTabState::ssh_phase),
        Some(SshConnectionPhase::Idle)
    ));
}

#[test]
fn stale_stored_credential_cleanup_cannot_reopen_a_closed_tab() {
    let mut state = test_state();
    let profile = SessionProfile::new("duplicate", "duplicate.example", "alice");
    let first_tab = state.open_terminal_tab(&profile);
    let second_tab = state.open_terminal_tab(&profile);
    let first_attempt = Uuid::new_v4();
    let second_attempt = Uuid::new_v4();
    state
        .terminal_mut(first_tab)
        .expect("first terminal should exist")
        .set_ssh_attempt(Some(first_attempt));
    state
        .terminal_mut(second_tab)
        .expect("second terminal should exist")
        .set_ssh_attempt(Some(second_attempt));
    let state = Arc::new(Mutex::new(state));

    assert!(
        prepare_stored_credential_retry(&state, first_tab, profile.id, first_attempt,)
            .expect("current stored credential retry should transition")
    );
    state
        .lock()
        .expect("state should remain writable")
        .close_tab(first_tab)
        .expect("first tab should close");

    assert!(!finish_stored_credential_retry(
        &state, first_tab, profile.id,
    ));
    let state = state.lock().expect("state should remain readable");
    assert_eq!(
        state
            .terminal(second_tab)
            .and_then(TerminalTabState::ssh_route),
        Some((profile.id, Some(second_attempt)))
    );
    assert!(matches!(
        state
            .terminal(second_tab)
            .and_then(TerminalTabState::ssh_phase),
        Some(SshConnectionPhase::Idle)
    ));
}

#[test]
fn stale_credential_lookup_cannot_clear_a_closed_tabs_storage_reference() {
    let mut state = test_state();
    let mut profile = SessionProfile::new("duplicate", "duplicate.example", "alice");
    profile
        .ssh_mut()
        .expect("profile should be SSH")
        .credential_storage = Some(CredentialStorage::SystemKeyring);
    state.sessions.upsert(profile.clone());
    let tab_id = state.open_terminal_tab(&profile);
    state
        .terminal_mut(tab_id)
        .expect("terminal should exist")
        .set_ssh_phase(SshConnectionPhase::LoadingStoredCredential);
    let state = Arc::new(Mutex::new(state));

    state
        .lock()
        .expect("state should remain writable")
        .close_tab(tab_id)
        .expect("tab should close");

    assert!(
        !set_credential_storage_while_loading(&state, tab_id, profile.id, None)
            .expect("stale credential result should be ignored without error")
    );
    let state = state.lock().expect("state should remain readable");
    assert_eq!(
        state.sessions.sessions[0]
            .ssh()
            .expect("profile should remain SSH")
            .credential_storage,
        Some(CredentialStorage::SystemKeyring)
    );
}

#[test]
fn settings_and_session_editor_tabs_are_singletons() {
    let mut state = test_state();

    let settings = state.open_settings_tab();
    assert!(state.has_settings_tab());
    state.open_local_shell_tab();
    assert_ne!(state.active_tab_id(), Some(settings));
    assert_eq!(settings, state.open_settings_tab());
    assert_eq!(state.active_tab_id(), Some(settings));
    let editor = state.open_session_editor_tab();
    assert_eq!(editor, state.open_session_editor_tab());
    assert!(state.has_session_editor_tab());
    assert_eq!(state.tab_summaries().len(), 3);

    let closed_editor = state.close_tab(editor).expect("editor should close");
    assert_eq!(closed_editor.kind, ClosedTabKind::SessionEditor);
    assert!(!state.has_session_editor_tab());

    let closed_settings = state.close_tab(settings).expect("settings should close");
    assert_eq!(closed_settings.kind, ClosedTabKind::Settings);
    assert!(!state.has_settings_tab());
}

#[test]
fn closing_the_last_sftp_tab_releases_the_shared_icon_cache() {
    let mut state = test_state();
    let profile = SessionProfile::new("SFTP", "sftp.example", "alice");
    let first = state.open_sftp_tab(&profile);
    let second = state.open_sftp_tab(&profile);

    let first_closed = state.close_tab(first).expect("first SFTP tab should close");
    assert_eq!(
        first_closed.kind,
        ClosedTabKind::Terminal {
            release_file_icon_cache: false,
        }
    );

    let second_closed = state
        .close_tab(second)
        .expect("second SFTP tab should close");
    assert_eq!(
        second_closed.kind,
        ClosedTabKind::Terminal {
            release_file_icon_cache: true,
        }
    );
}

#[test]
fn session_editor_can_switch_between_group_defaults_and_existing_profiles() {
    let mut state = test_state();
    let mut profile = SessionProfile::new("Production", "prod.example", "alice");
    profile.group_name = "Critical".into();
    profile
        .ssh_mut()
        .expect("profile should be SSH")
        .credential_storage = Some(CredentialStorage::SystemKeyring);
    state.sessions.upsert(profile.clone());

    let editor_id = state.open_session_editor_for_group(" Staging ");
    let group_editor = state.active_snapshot().editor.expect("editor should exist");
    assert_eq!(group_editor.profile_id, None);
    assert_eq!(group_editor.group_name, "Staging");
    let group_draft_id = group_editor.draft_id;

    assert!(state.open_session_editor_for_profile(profile.id));
    assert_eq!(state.active_tab_id(), Some(editor_id));
    let profile_editor = state.active_snapshot().editor.expect("editor should exist");
    assert_eq!(profile_editor.profile_id, Some(profile.id));
    assert_ne!(profile_editor.draft_id, group_draft_id);
    assert_eq!(profile_editor.name, "Production");
    assert_eq!(profile_editor.group_name, "Critical");
    assert_eq!(
        state.sessions.sessions[0]
            .ssh()
            .expect("profile should remain SSH")
            .credential_storage,
        Some(CredentialStorage::SystemKeyring)
    );
}

#[test]
fn session_editor_maps_ssh_agent_without_a_private_key_path() {
    let mut state = test_state();
    let mut profile = SessionProfile::new("Agent", "agent.example", "alice");
    profile.ssh_mut().expect("profile should be SSH").auth = AuthMethod::SshAgent;
    state.sessions.upsert(profile.clone());

    assert!(state.open_session_editor_for_profile(profile.id));
    let editor = state.active_snapshot().editor.expect("editor should exist");
    assert_eq!(editor.auth_method, "SSH agent");
    assert!(editor.private_key_path.is_empty());
    assert!(editor.credential_storage.is_empty());
}

#[test]
fn closing_active_tab_selects_a_neighbor() {
    let mut state = test_state();
    let profile = SessionProfile::new("Local", "localhost", "alice");
    let first = state.open_terminal_tab(&profile);
    let second = state.open_terminal_tab(&profile);

    state.close_tab(second).expect("second tab should close");

    assert_eq!(state.active_tab_id(), Some(first));
}

#[test]
fn cycling_workspace_tabs_wraps_in_both_directions() {
    let mut state = test_state();
    assert_eq!(state.cycle_tab(true), None);

    let first = state.open_local_shell_tab();
    assert_eq!(state.cycle_tab(true), None);
    assert_eq!(state.active_tab_id(), Some(first));

    let second = state.open_local_shell_tab();
    let third = state.open_local_shell_tab();
    assert_eq!(state.cycle_tab(true), Some(first));
    assert_eq!(state.cycle_tab(false), Some(third));

    assert!(state.activate_tab(first));
    assert_eq!(state.cycle_tab(false), Some(third));
    assert_eq!(state.cycle_tab(true), Some(first));
    assert_eq!(state.cycle_tab(true), Some(second));
}

#[test]
fn moving_workspace_tabs_changes_position_without_renumbering_instances() {
    let mut state = test_state();
    let first = state.open_local_shell_tab();
    let second = state.open_local_shell_tab();
    let third = state.open_local_shell_tab();

    assert!(state.move_tab(third, 0));
    let summaries = state.tab_summaries();
    assert_eq!(
        summaries
            .iter()
            .map(|tab| tab.title.as_str())
            .collect::<Vec<_>>(),
        ["Local Shell #3", "Local Shell #1", "Local Shell #2"]
    );
    assert_eq!(state.active_tab_id(), Some(third));

    assert!(state.move_tab(first, 2));
    assert_eq!(
        state
            .tab_summaries()
            .iter()
            .map(|tab| tab.id)
            .collect::<Vec<_>>(),
        vec![third, second, first]
    );
}

#[test]
fn moving_visible_tabs_ignores_hidden_pane_sessions() {
    let mut state = test_state();
    let first = state.open_local_shell_tab();
    let hidden_pane = state.open_local_shell_tab();
    let second = state.open_local_shell_tab();

    assert!(state.move_tab_for(first, 1, &[first, second]));
    assert_eq!(
        state
            .tab_summaries()
            .into_iter()
            .filter(|tab| tab.id != hidden_pane)
            .map(|tab| tab.id)
            .collect::<Vec<_>>(),
        vec![second, first]
    );
}

#[test]
fn retiring_one_duplicate_profile_attempt_does_not_touch_the_other() {
    let mut state = test_state();
    let profile = SessionProfile::new("Local", "localhost", "alice");
    let first = state.open_terminal_tab(&profile);
    let second = state.open_terminal_tab(&profile);
    let first_attempt = Uuid::new_v4();
    let second_attempt = Uuid::new_v4();
    state
        .terminal_mut(first)
        .expect("first terminal should exist")
        .set_ssh_attempt(Some(first_attempt));
    state
        .terminal_mut(second)
        .expect("second terminal should exist")
        .set_ssh_attempt(Some(second_attempt));
    state
        .terminal_mut(first)
        .expect("first terminal should exist")
        .sftp
        .open = true;
    state
        .terminal_mut(second)
        .expect("second terminal should exist")
        .sftp
        .open = true;
    let state = Arc::new(Mutex::new(state));

    assert!(retire_session_attempt(
        &state,
        first,
        profile.id,
        first_attempt
    ));
    let state = state.lock().expect("state should remain readable");
    assert_eq!(
        state.terminal(first).and_then(TerminalTabState::ssh_route),
        Some((profile.id, None))
    );
    assert_eq!(
        state.terminal(second).and_then(TerminalTabState::ssh_route),
        Some((profile.id, Some(second_attempt)))
    );
    assert!(
        !state
            .terminal(first)
            .expect("first terminal should remain")
            .sftp
            .open
    );
    assert!(
        state
            .terminal(second)
            .expect("second terminal should remain")
            .sftp
            .open
    );
}

#[test]
fn local_shell_tabs_have_unique_ids_and_independent_numbers() {
    let mut state = test_state();

    let first = state.open_local_shell_tab();
    let second = state.open_local_shell_tab();

    assert_ne!(first, second);
    assert_eq!(state.tab_summaries()[0].title, "Local Shell #1");
    assert_eq!(state.tab_summaries()[1].title, "Local Shell #2");
    assert!(
        state
            .terminal(first)
            .is_some_and(TerminalTabState::is_local)
    );
    assert!(
        state
            .terminal(second)
            .is_some_and(TerminalTabState::is_local)
    );
}

#[test]
fn resizing_the_active_terminal_updates_its_snapshot_immediately() {
    let mut state = test_state();
    state.open_local_shell_tab();

    state
        .resize_active_terminal(12, 4)
        .expect("active terminal should be resized");
    let snapshot = state.active_snapshot();
    let terminal = snapshot
        .terminal
        .expect("active terminal snapshot should contain the terminal grid");

    assert_eq!(terminal.max_columns, 12);
    assert_eq!(terminal.lines.len(), 4);
}

#[test]
fn switching_terminal_tabs_exposes_each_tab_grid_size() {
    let mut state = test_state();
    let first = state.open_local_shell_tab();
    state
        .resize_active_terminal(12, 4)
        .expect("first terminal should be resized");

    let second = state.open_local_shell_tab();
    state
        .resize_active_terminal(20, 6)
        .expect("second terminal should be resized");

    assert!(state.activate_tab(first));
    let first_snapshot = state
        .active_snapshot()
        .terminal
        .expect("first terminal snapshot should be present");
    assert_eq!(
        (first_snapshot.max_columns, first_snapshot.lines.len()),
        (12, 4)
    );

    assert!(state.activate_tab(second));
    let second_snapshot = state
        .active_snapshot()
        .terminal
        .expect("second terminal snapshot should be present");
    assert_eq!(
        (second_snapshot.max_columns, second_snapshot.lines.len()),
        (20, 6)
    );
}

#[test]
fn resizing_a_terminal_by_id_does_not_resize_another_pane() {
    let mut state = test_state();
    let first = state.open_local_shell_tab();
    let second = state.open_local_shell_tab();

    state
        .resize_terminal(first, 12, 4)
        .expect("first terminal should resize");
    state
        .resize_terminal(second, 20, 6)
        .expect("second terminal should resize");

    let first_snapshot = state
        .snapshot_for(Some(first))
        .terminal
        .expect("first snapshot should contain a terminal");
    let second_snapshot = state
        .snapshot_for(Some(second))
        .terminal
        .expect("second snapshot should contain a terminal");
    assert_eq!(
        (first_snapshot.max_columns, first_snapshot.lines.len()),
        (12, 4)
    );
    assert_eq!(
        (second_snapshot.max_columns, second_snapshot.lines.len()),
        (20, 6)
    );
}

#[test]
fn terminal_resize_invalidates_local_selection_revision() {
    let mut state = test_state();
    let tab_id = state.open_local_shell_tab();
    let before = state.snapshot_for(Some(tab_id)).selection_revision;

    assert!(
        state
            .resize_terminal(tab_id, 12, 4)
            .expect("terminal should resize")
    );

    let after = state.snapshot_for(Some(tab_id)).selection_revision;
    assert_eq!(after, before + 1);

    assert!(
        !state
            .resize_terminal(tab_id, 12, 4)
            .expect("duplicate terminal resize should be ignored")
    );

    assert_eq!(state.snapshot_for(Some(tab_id)).selection_revision, after);
}

#[test]
fn terminal_scroll_invalidates_selection_only_when_the_viewport_changes() {
    let mut state = test_state();
    let tab_id = state.open_local_shell_tab();
    let terminal = state
        .terminal_mut(tab_id)
        .and_then(|terminal| terminal.terminal.as_mut())
        .expect("local terminal should have a model");
    terminal.resize(10, 3);
    for index in 0..32 {
        terminal.process(format!("line-{index}\r\n").as_bytes());
    }
    let before = state.snapshot_for(Some(tab_id)).selection_revision;

    assert!(state.scroll_terminal(tab_id, 1));
    let after = state.snapshot_for(Some(tab_id)).selection_revision;
    assert_eq!(after, before + 1);

    assert!(!state.scroll_terminal(tab_id, 0));
    assert_eq!(state.snapshot_for(Some(tab_id)).selection_revision, after);

    assert!(state.scroll_terminal_to_bottom(tab_id));
    let bottom = state.snapshot_for(Some(tab_id)).selection_revision;
    assert_eq!(bottom, after + 1);

    assert!(!state.scroll_terminal_to_bottom(tab_id));
    assert_eq!(state.snapshot_for(Some(tab_id)).selection_revision, bottom);
}

#[test]
fn sftp_snapshots_require_connected_sftp_tabs_and_remain_isolated() {
    let mut state = test_state();
    let first_profile = SessionProfile::new("first", "first.example", "alice");
    let second_profile = SessionProfile::new("second", "second.example", "bob");
    let first = state.open_sftp_tab(&first_profile);
    let second = state.open_sftp_tab(&second_profile);
    let local = state.open_local_shell_tab();

    assert!(state.activate_tab(first));
    assert!(!state.active_snapshot().sftp.available);

    let first_terminal = state
        .terminal_mut(first)
        .expect("first SFTP terminal should exist");
    first_terminal.connected = true;
    let first_sftp = &mut first_terminal.sftp;
    first_sftp.open = true;
    first_sftp.path = "/home/alice".to_owned();
    first_sftp.entries.push(SftpEntry {
        name: "notes.txt".to_owned(),
        path: "/home/alice/notes.txt".to_owned(),
        is_dir: false,
        is_symlink: false,
        size: 42,
        modified: None,
    });

    assert!(state.activate_tab(first));
    let first_snapshot = state.active_snapshot().sftp;
    assert!(first_snapshot.available);
    assert!(first_snapshot.open);
    assert_eq!(first_snapshot.path, "/home/alice");
    assert_eq!(first_snapshot.entries.len(), 1);

    state
        .terminal_mut(second)
        .expect("second SFTP terminal should exist")
        .connected = true;
    assert!(state.activate_tab(second));
    let second_snapshot = state.active_snapshot().sftp;
    assert!(second_snapshot.available);
    assert!(!second_snapshot.open);
    assert!(second_snapshot.entries.is_empty());

    assert!(state.activate_tab(local));
    assert!(!state.active_snapshot().sftp.available);
}

#[test]
fn sftp_transfer_state_covers_progress_pause_resume_and_terminal_phases() {
    let mut sftp = SftpBrowserState::default();
    let transfer_id = Uuid::new_v4();

    sftp.queue_transfer(transfer_id, "report.txt".to_owned(), 100)
        .expect("transfer should be queued");
    assert_eq!(sftp.transfers[0].phase, SftpTransferPhase::Queued);
    assert!(sftp.transfers[0].phase.cancellable());

    sftp.start_transfer(transfer_id, "report.txt".to_owned(), 100);
    sftp.update_transfer_progress(transfer_id, 150, 100);
    assert_eq!(sftp.transfers[0].phase, SftpTransferPhase::Downloading);
    assert_eq!(sftp.transfers[0].downloaded_bytes, 100);
    assert_eq!(sftp.transfers[0].status, "100%");

    assert!(sftp.request_transfer_pause(transfer_id));
    assert_eq!(sftp.transfers[0].phase, SftpTransferPhase::Pausing);
    assert!(sftp.pause_transfer(transfer_id));
    assert_eq!(sftp.transfers[0].phase, SftpTransferPhase::Paused);
    assert!(sftp.request_transfer_resume(transfer_id));
    assert_eq!(sftp.transfers[0].phase, SftpTransferPhase::Resuming);
    assert!(sftp.resume_transfer(transfer_id, 100, 100));
    assert_eq!(sftp.transfers[0].phase, SftpTransferPhase::Downloading);

    assert!(sftp.request_transfer_cancel(transfer_id));
    assert_eq!(sftp.transfers[0].phase, SftpTransferPhase::Cancelling);
    assert!(!sftp.transfer_is_cancellable(transfer_id));
    assert!(!sftp.request_transfer_cancel(transfer_id));
    sftp.finish_transfer(
        transfer_id,
        SftpTransferPhase::Cancelled,
        "Cancelled".to_owned(),
    );
    assert!(!sftp.request_transfer_cancel(transfer_id));

    let failed_id = Uuid::new_v4();
    sftp.queue_transfer(failed_id, "broken.txt".to_owned(), 0)
        .expect("second transfer should be queued");
    sftp.start_transfer(failed_id, "broken.txt".to_owned(), 0);
    sftp.finish_transfer(
        failed_id,
        SftpTransferPhase::Failed,
        "opener failed".to_owned(),
    );
    assert_eq!(sftp.transfers[1].phase, SftpTransferPhase::Failed);
    assert!(!sftp.transfers[1].phase.cancellable());
}

#[test]
fn sftp_transfer_state_ignores_late_events_after_cancellation() {
    let mut sftp = SftpBrowserState::default();
    let transfer_id = Uuid::new_v4();

    sftp.queue_transfer(transfer_id, "report.txt".to_owned(), 100)
        .expect("transfer should be queued");
    assert!(sftp.request_transfer_cancel(transfer_id));

    sftp.start_transfer(transfer_id, "report.txt".to_owned(), 100);
    sftp.update_transfer_progress(transfer_id, 50, 100);
    assert!(!sftp.complete_download(transfer_id, 100));

    assert_eq!(sftp.transfers[0].phase, SftpTransferPhase::Cancelling);
    assert_eq!(sftp.transfers[0].downloaded_bytes, 0);
    sftp.finish_transfer(
        transfer_id,
        SftpTransferPhase::Cancelled,
        "Cancelled".to_owned(),
    );
    assert_eq!(sftp.transfers[0].phase, SftpTransferPhase::Cancelled);
}

#[test]
fn sftp_editor_tracks_remote_fingerprint_and_debounced_revision() {
    let mut sftp = SftpBrowserState {
        editor_path: Some("/home/alice/readme.txt".to_owned()),
        editor_text: "old".to_owned(),
        editor_expected_size: Some(3),
        editor_expected_modified: Some(10),
        ..SftpBrowserState::default()
    };
    let revision = sftp.editor_revision;
    assert!(sftp.set_editor_text("new".to_owned()).is_some());
    assert!(!sftp.editor_is_current("/home/alice/readme.txt", revision));
    sftp.editor_remote_changed = true;
    sftp.set_editor_auto_upload(true);
    let snapshot = sftp.snapshot(true);
    assert!(snapshot.editor_remote_changed);
    assert!(snapshot.editor_auto_upload);
    assert!(snapshot.editor_revision > revision);
}

#[test]
fn sftp_transfer_selection_counts_only_actionable_active_rows() {
    let mut sftp = SftpBrowserState::default();
    let downloading = Uuid::new_v4();
    let paused = Uuid::new_v4();
    let completed = Uuid::new_v4();
    let failed = Uuid::new_v4();

    for (id, name) in [
        (downloading, "downloading.txt"),
        (paused, "paused.txt"),
        (completed, "completed.txt"),
    ] {
        sftp.queue_transfer(id, name.to_owned(), 10)
            .expect("transfer should queue");
        sftp.start_transfer(id, name.to_owned(), 10);
    }
    assert!(sftp.request_transfer_pause(paused));
    assert!(sftp.pause_transfer(paused));
    assert!(sftp.complete_download(completed, 10));
    sftp.record_transfer_failure(failed, "failed.txt".to_owned(), "remote error".to_owned());

    for id in [downloading, paused] {
        assert!(sftp.toggle_transfer_selection(id, true));
    }
    assert!(!sftp.toggle_transfer_selection(completed, true));
    assert!(!sftp.toggle_transfer_selection(failed, true));
    let snapshot = sftp.snapshot(true);

    assert_eq!(snapshot.transfer_selected_active_count, 2);
    assert_eq!(snapshot.transfer_selected_pausable_count, 1);
    assert_eq!(snapshot.transfer_selected_resumable_count, 1);
    assert_eq!(
        sftp.selected_transfer_ids_for_active_page(),
        vec![downloading, paused]
    );
}

#[test]
fn sftp_navigation_history_survives_failures_and_resets_forward_branch() {
    let mut sftp = SftpBrowserState {
        path: "/home/alice".to_owned(),
        ..SftpBrowserState::default()
    };

    assert_eq!(
        sftp.begin_navigation(SftpNavigation::Direct, Some("/var".to_owned()))
            .expect("direct navigation should be queued"),
        "/var"
    );
    sftp.complete_navigation("/var".to_owned());

    let _ = sftp
        .begin_navigation(SftpNavigation::Back, None)
        .expect("back navigation should be available");
    sftp.cancel_navigation();
    assert!(sftp.begin_navigation(SftpNavigation::Back, None).is_ok());
    sftp.complete_navigation("/home/alice".to_owned());

    assert!(sftp.begin_navigation(SftpNavigation::Forward, None).is_ok());
    sftp.cancel_navigation();
    assert!(sftp.begin_navigation(SftpNavigation::Forward, None).is_ok());
    sftp.complete_navigation("/var".to_owned());

    sftp.begin_navigation(SftpNavigation::Direct, Some("/tmp".to_owned()))
        .expect("new direct navigation should be queued");
    sftp.complete_navigation("/tmp".to_owned());
    assert!(
        sftp.begin_navigation(SftpNavigation::Forward, None)
            .is_err()
    );
}

#[test]
fn sftp_selection_tracks_rows_and_select_all() {
    let mut sftp = SftpBrowserState {
        entries: vec![
            SftpEntry {
                name: "one.txt".to_owned(),
                path: "/home/alice/one.txt".to_owned(),
                is_dir: false,
                is_symlink: false,
                size: 1,
                modified: None,
            },
            SftpEntry {
                name: "two.txt".to_owned(),
                path: "/home/alice/two.txt".to_owned(),
                is_dir: false,
                is_symlink: false,
                size: 2,
                modified: None,
            },
        ],
        ..SftpBrowserState::default()
    };

    assert!(sftp.toggle_selection("/home/alice/one.txt", true));
    assert_eq!(sftp.selected_count(), 1);
    assert!(!sftp.all_selected());
    sftp.select_all(true);
    assert!(sftp.all_selected());
    sftp.select_all(false);
    assert_eq!(sftp.selected_count(), 0);
    assert!(!sftp.toggle_selection("/home/alice/missing.txt", true));
}

#[test]
fn sftp_tab_is_a_separate_ssh_target() {
    let mut state = test_state();
    let mut profile = SessionProfile::new("server", "server.example", "alice");
    profile
        .ssh_mut()
        .expect("profile should use SSH")
        .sftp_local_path = "/tmp/axssh-sftp-default".into();

    let terminal = state.open_terminal_tab(&profile);
    let sftp = state.open_sftp_tab(&profile);

    assert_eq!(state.tab_summaries()[0].kind, "terminal");
    assert_eq!(state.tab_summaries()[1].kind, "sftp");
    assert_eq!(
        state
            .terminal(sftp)
            .and_then(TerminalTabState::ssh_route)
            .map(|(profile_id, _)| profile_id),
        Some(profile.id)
    );
    assert_eq!(
        state
            .terminal(sftp)
            .map(TerminalTabState::connection_target),
        Some(ConnectionTarget::Sftp)
    );
    assert!(state.terminal(sftp).is_some_and(TerminalTabState::is_sftp));
    assert!(
        state
            .terminal(sftp)
            .is_some_and(|terminal| terminal.terminal.is_none())
    );
    assert!(
        state
            .terminal(terminal)
            .is_some_and(|terminal| terminal.terminal.is_some())
    );
    assert_eq!(
        state
            .terminal(terminal)
            .map(TerminalTabState::connection_target),
        Some(ConnectionTarget::Terminal)
    );

    let snapshot = state.active_snapshot().sftp;
    assert_eq!(snapshot.local.path, "/tmp/axssh-sftp-default");
    assert!(!snapshot.local.loading);
    assert!(snapshot.local.entries.is_empty());
}

#[test]
fn ssh_sftp_shortcut_pairs_duplicate_profiles_by_runtime_tab() {
    let mut state = test_state();
    let profile = SessionProfile::new("server", "server.example", "alice");
    let first_terminal = state.open_terminal_tab(&profile);
    let second_terminal = state.open_terminal_tab(&profile);

    assert!(state.activate_tab(first_terminal));
    assert_eq!(
        state.switch_ssh_sftp_tab(),
        Some(SshSftpNavigation::Connect {
            profile_id: profile.id,
            target: ConnectionTarget::Sftp,
            companion_tab_id: first_terminal,
        })
    );
    let first_sftp = state.open_sftp_tab_with_companion(&profile, Some(first_terminal));
    assert_eq!(
        state
            .tab_summaries()
            .iter()
            .map(|tab| tab.id)
            .collect::<Vec<_>>(),
        vec![first_terminal, first_sftp, second_terminal]
    );

    assert!(state.activate_tab(second_terminal));
    assert_eq!(
        state.switch_ssh_sftp_tab(),
        Some(SshSftpNavigation::Connect {
            profile_id: profile.id,
            target: ConnectionTarget::Sftp,
            companion_tab_id: second_terminal,
        })
    );
}

#[test]
fn ssh_sftp_shortcut_activates_existing_companion_in_both_directions() {
    let mut state = test_state();
    let profile = SessionProfile::new("server", "server.example", "alice");
    let terminal = state.open_terminal_tab(&profile);
    let sftp = state.open_sftp_tab_with_companion(&profile, Some(terminal));

    assert_eq!(
        state.switch_ssh_sftp_tab(),
        Some(SshSftpNavigation::Activated(terminal))
    );
    assert_eq!(state.active_tab_id(), Some(terminal));
    assert_eq!(
        state.switch_ssh_sftp_tab(),
        Some(SshSftpNavigation::Activated(sftp))
    );
    assert_eq!(state.active_tab_id(), Some(sftp));
    assert_eq!(state.tab_summaries().len(), 2);
}

#[test]
fn targeted_sftp_companion_path_stays_on_the_runtime_tab() {
    let mut state = test_state();
    let profile = SessionProfile::new("server", "server.example", "alice");
    let terminal = state.open_terminal_tab(&profile);
    let sftp = state.open_sftp_tab_with_companion_at_path(
        &profile,
        Some(terminal),
        Some("/srv/releases".to_owned()),
    );

    assert_eq!(state.sftp_companion_id(terminal), Some(sftp));
    assert_eq!(
        state
            .terminal(sftp)
            .and_then(|terminal| terminal.sftp_initial_path.as_deref()),
        Some("/srv/releases")
    );
    assert!(
        state
            .terminal(terminal)
            .is_some_and(|terminal| terminal.sftp_initial_path.is_none())
    );
}

#[test]
fn standalone_sftp_shortcut_plans_an_adjacent_terminal_companion() {
    let mut state = test_state();
    let profile = SessionProfile::new("server", "server.example", "alice");
    let sftp = state.open_sftp_tab(&profile);

    assert_eq!(
        state.switch_ssh_sftp_tab(),
        Some(SshSftpNavigation::Connect {
            profile_id: profile.id,
            target: ConnectionTarget::Terminal,
            companion_tab_id: sftp,
        })
    );
    let terminal = state.open_terminal_tab_with_companion(&profile, Some(sftp));
    assert_eq!(
        state
            .tab_summaries()
            .iter()
            .map(|tab| tab.id)
            .collect::<Vec<_>>(),
        vec![terminal, sftp]
    );
    assert_eq!(
        state.switch_ssh_sftp_tab(),
        Some(SshSftpNavigation::Activated(sftp))
    );
}

#[test]
fn closing_a_companion_unlinks_the_surviving_tab() {
    let mut state = test_state();
    let profile = SessionProfile::new("server", "server.example", "alice");
    let terminal = state.open_terminal_tab(&profile);
    let sftp = state.open_sftp_tab_with_companion(&profile, Some(terminal));

    state.close_tab(terminal).expect("terminal should close");
    assert_eq!(state.active_tab_id(), Some(sftp));
    assert_eq!(
        state.switch_ssh_sftp_tab(),
        Some(SshSftpNavigation::Connect {
            profile_id: profile.id,
            target: ConnectionTarget::Terminal,
            companion_tab_id: sftp,
        })
    );
}

#[test]
fn reconnect_state_is_bounded_and_generation_cancellable() {
    let mut state = test_state();
    let profile = SessionProfile::new_telnet("console", "127.0.0.1");
    let tab_id = state.open_terminal_tab(&profile);
    let terminal = state.terminal_mut(tab_id).expect("terminal should exist");
    let generation = terminal.reconnect_generation();
    for expected in 1..=TerminalTabState::MAX_RECONNECT_ATTEMPTS {
        assert_eq!(terminal.begin_reconnect(), Some((generation, expected)));
        assert!(terminal.finish_reconnect_attempt(generation));
    }
    assert_eq!(terminal.begin_reconnect(), None);
    terminal.reconnect_attempt = 0;
    terminal.reconnecting = false;
    terminal.cancel_reconnect();
    assert!(!terminal.reconnect_current(generation));
}

#[test]
fn reconnect_does_not_replace_terminal_buffer() {
    let mut state = test_state();
    let profile = SessionProfile::new_telnet("console", "127.0.0.1");
    let tab_id = state.open_terminal_tab(&profile);
    let terminal = state.terminal_mut(tab_id).expect("terminal should exist");
    terminal
        .terminal
        .as_mut()
        .expect("terminal model should exist")
        .process(b"preserved output");
    let before = terminal.terminal.as_mut().expect("model").snapshot();
    terminal.begin_reconnect();
    terminal.finish_reconnect_attempt(terminal.reconnect_generation());
    let after = terminal.terminal.as_mut().expect("model").snapshot();
    assert_eq!(before.lines, after.lines);
}

#[test]
fn control_only_terminal_output_skips_ui_publication_but_cursor_changes_do_not() {
    let mut state = test_state();
    let profile = SessionProfile::new_telnet("console", "127.0.0.1");
    let tab_id = state.open_terminal_tab(&profile);
    let terminal = state.terminal_mut(tab_id).expect("terminal should exist");

    assert!(terminal.terminal_snapshot_for_ui().is_some());
    terminal
        .terminal
        .as_mut()
        .expect("terminal model should exist")
        .process(b"\x1b[31m");
    assert!(
        !terminal.prepare_terminal_output_snapshot(),
        "an attribute-only control sequence must not schedule a redraw"
    );

    terminal
        .terminal
        .as_mut()
        .expect("terminal model should exist")
        .process(b"\x1b[2C");
    assert!(
        terminal.prepare_terminal_output_snapshot(),
        "cursor movement remains a visible update"
    );
    let snapshot = terminal
        .terminal_snapshot_for_ui()
        .expect("pending cursor snapshot should be retained for the UI");
    assert_eq!(snapshot.cursor_column, 2);
}

#[test]
fn pending_terminal_snapshot_merges_later_dirty_rows_before_ui_consumes_it() {
    let mut state = test_state();
    let profile = SessionProfile::new_telnet("console", "127.0.0.1");
    let tab_id = state.open_terminal_tab(&profile);
    let terminal = state.terminal_mut(tab_id).expect("terminal should exist");

    assert!(terminal.terminal_snapshot_for_ui().is_some());
    terminal
        .terminal
        .as_mut()
        .expect("terminal model should exist")
        .process(b"\x1b[1;1Hfirst");
    assert!(terminal.prepare_terminal_output_snapshot());
    terminal
        .terminal
        .as_mut()
        .expect("terminal model should exist")
        .process(b"\x1b[2;1Hsecond");

    let snapshot = terminal
        .terminal_snapshot_for_ui()
        .expect("latest coalesced snapshot should be available");
    assert!(snapshot.dirty_rows.contains(&0));
    assert!(snapshot.dirty_rows.contains(&1));
}

#[test]
fn terminal_resize_discards_a_stale_pending_output_snapshot() {
    let mut state = test_state();
    let profile = SessionProfile::new_telnet("console", "127.0.0.1");
    let tab_id = state.open_terminal_tab(&profile);
    {
        let terminal = state.terminal_mut(tab_id).expect("terminal should exist");
        assert!(terminal.terminal_snapshot_for_ui().is_some());
        terminal
            .terminal
            .as_mut()
            .expect("terminal model should exist")
            .process(b"before resize");
        assert!(terminal.prepare_terminal_output_snapshot());
    }

    assert!(
        state
            .resize_terminal(tab_id, 96, 30)
            .expect("resize should work")
    );
    let snapshot = state
        .terminal_mut(tab_id)
        .expect("terminal should exist")
        .terminal_snapshot_for_ui()
        .expect("resized snapshot should be available");
    assert_eq!(snapshot.max_columns, 96);
    assert_eq!(snapshot.lines.len(), 30);
}
