use super::*;

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
fn settings_and_session_editor_tabs_are_singletons() {
    let mut state = test_state();

    assert_eq!(state.open_settings_tab(), state.open_settings_tab());
    assert_eq!(
        state.open_session_editor_tab(),
        state.open_session_editor_tab()
    );
    assert_eq!(state.tab_summaries().len(), 2);
}

#[test]
fn session_editor_can_switch_between_group_defaults_and_existing_profiles() {
    let mut state = test_state();
    let mut profile = SessionProfile::new("Production", "prod.example", "alice");
    profile.group_name = "Critical".into();
    profile.credential_stored = true;
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
    assert!(profile_editor.credential_stored);
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
        .resize_active_terminal_model(12, 4)
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
        .resize_active_terminal_model(12, 4)
        .expect("first terminal should be resized");

    let second = state.open_local_shell_tab();
    state
        .resize_active_terminal_model(20, 6)
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
