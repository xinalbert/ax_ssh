use super::*;

#[test]
fn live_workers_and_inflight_retries_do_not_consume_the_bounded_budget() {
    let mut state = ReconnectState::new(true);
    let generation = state.generation();
    assert_eq!(state.begin(true), None);
    assert_eq!(state.attempt(), 0);
    for attempt in 1..=ReconnectState::MAX_ATTEMPTS {
        assert_eq!(state.begin(false), Some((generation, attempt)));
        assert_eq!(state.begin(false), None);
        assert_eq!(state.attempt(), attempt);
        assert!(state.finish_attempt(generation));
    }
    assert_eq!(state.begin(false), None);
    assert!(state.mark_connected(generation));
    assert_eq!(state.attempt(), 0);
    assert_eq!(state.begin(false), Some((generation, 1)));
}

#[test]
fn cancelled_generations_cannot_finish_or_reenable_their_replacement() {
    let mut state = ReconnectState::new(true);
    let (old, _) = state.begin(false).unwrap();
    state.cancel();
    assert!(!state.enabled());
    assert!(!state.current(old));
    assert!(!state.finish_attempt(old));
    assert!(!state.mark_connected(old));
    assert_eq!(state.begin(false), None);

    state.prepare_manual_retry(true);
    let (current, attempt) = state.begin(false).unwrap();
    assert_ne!(current, old);
    assert_eq!(attempt, 1);
    assert!(!state.finish_attempt(old));
    assert!(!state.mark_connected(old));
    assert!(state.current(current));
    assert!(state.reconnecting());
}

#[test]
fn manual_retry_preserves_the_owners_local_terminal_policy() {
    let mut state = ReconnectState::new(false);
    assert_eq!(state.begin(false), None);
    state.prepare_manual_retry(false);
    assert_eq!(state.generation(), 1);
    assert_eq!(state.begin(false), None);
    state.enable();
    assert_eq!(state.begin(false), Some((1, 1)));
    state.prepare_manual_retry(false);
    assert_eq!(state.attempt(), 0);
    assert!(!state.reconnecting());
    assert!(!state.enabled());
}

#[test]
fn reconnect_state_is_independent_between_connections() {
    let mut first = ReconnectState::new(true);
    let mut second = ReconnectState::new(true);
    let (generation, _) = second.begin(false).unwrap();
    first.cancel();
    assert!(second.current(generation));
    assert_eq!(second.attempt(), 1);
    assert!(second.mark_connected(generation));
    assert!(!first.enabled());
}

#[test]
fn generation_wrap_still_rejects_the_previous_generation() {
    let mut state = ReconnectState {
        generation: u64::MAX,
        ..ReconnectState::new(true)
    };
    let old = state.begin(false).unwrap().0;
    state.cancel();
    assert_eq!(state.generation(), 0);
    assert!(!state.mark_connected(old));
    state.prepare_manual_retry(true);
    assert_eq!(state.begin(false), Some((1, 1)));
    assert!(!state.finish_attempt(old));
    assert!(state.current(1));
}
