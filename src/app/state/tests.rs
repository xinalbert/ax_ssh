use super::*;
use ax_ssh::config::CredentialStorage;

fn test_state() -> AppState {
    AppState::new(
        ConfigStore::new(std::env::temp_dir().join(format!("ax-ssh-tabs-{}.json", Uuid::new_v4()))),
        SessionStore::default(),
    )
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
            changed: false,
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
    state.open_local_shell_tab();
    assert_ne!(state.active_tab_id(), Some(settings));
    assert_eq!(settings, state.open_settings_tab());
    assert_eq!(state.active_tab_id(), Some(settings));
    assert_eq!(
        state.open_session_editor_tab(),
        state.open_session_editor_tab()
    );
    assert_eq!(state.tab_summaries().len(), 3);
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
fn sftp_navigation_history_survives_failures_and_resets_forward_branch() {
    let mut sftp = SftpBrowserState::default();
    sftp.path = "/home/alice".to_owned();

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
    let mut sftp = SftpBrowserState::default();
    sftp.entries = vec![
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
    ];

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
    let profile = SessionProfile::new("server", "server.example", "alice");

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
    assert_eq!(
        state
            .terminal(terminal)
            .map(TerminalTabState::connection_target),
        Some(ConnectionTarget::Terminal)
    );

    let snapshot = state.active_snapshot().sftp;
    assert!(!snapshot.local.path.is_empty());
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
