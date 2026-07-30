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
fn closing_active_tab_selects_a_neighbor() {
    let mut state = test_state();
    let profile = SessionProfile::new("Local", "localhost", "alice");
    let first = state.open_terminal_tab(&profile);
    let second = state.open_terminal_tab(&profile);

    state.close_tab(second).expect("second tab should close");

    assert_eq!(state.active_tab_id(), Some(first));
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
