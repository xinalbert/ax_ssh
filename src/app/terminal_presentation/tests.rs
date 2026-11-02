use super::*;

const DEFAULT_POLICY: TerminalPresentationPolicy = TerminalPresentationPolicy::new(60, 4);

#[tokio::test(start_paused = true)]
async fn queued_event_budget_yields_to_ready_presentation_without_losing_events() {
    let mut presentation = TerminalPresentation::new();
    let (sender, mut events) = mpsc::channel(MAX_EVENTS_BEFORE_PRESENT + 1);
    for value in 0..=MAX_EVENTS_BEFORE_PRESENT {
        sender.try_send(value).expect("queued event");
    }
    presentation.record_output(None, None);
    for value in 0..MAX_EVENTS_BEFORE_PRESENT {
        assert_eq!(
            presentation.try_recv_before_present(&mut events).await,
            Some(value)
        );
    }
    assert!(
        presentation
            .try_recv_before_present(&mut events)
            .await
            .is_none()
    );
    tokio::time::advance(FOCUSED_FAST_INTERVAL).await;
    assert!(presentation.is_ready(
        TerminalPresentationMode::Focused,
        DEFAULT_POLICY.with_software_renderer(true),
        Instant::now()
    ));
    assert_eq!(
        events.len(),
        1,
        "redraw gets a turn before the queue is empty"
    );
    assert_eq!(
        presentation.try_recv_before_present(&mut events).await,
        Some(MAX_EVENTS_BEFORE_PRESENT)
    );
}

#[tokio::test(start_paused = true)]
async fn empty_and_closed_event_queues_add_no_presentation_wait() {
    let mut presentation = TerminalPresentation::new();
    let (sender, mut events) = mpsc::channel::<u8>(1);
    let started_at = Instant::now();
    assert!(
        presentation
            .try_recv_before_present(&mut events)
            .await
            .is_none()
    );
    assert_eq!(Instant::now(), started_at);
    sender
        .try_send(7)
        .expect("queued event after an empty read");
    assert_eq!(
        presentation.try_recv_before_present(&mut events).await,
        Some(7)
    );
    drop(sender);
    assert!(
        presentation
            .try_recv_before_present(&mut events)
            .await
            .is_none()
    );
    assert!(
        events.recv().await.is_none(),
        "closed channel remains observable"
    );
    assert_eq!(Instant::now(), started_at);
}

#[test]
fn clean_state_has_no_deadline() {
    let state = TerminalPresentationState::default();
    let now = Instant::now();

    assert_eq!(
        state.deadline(TerminalPresentationMode::Focused, DEFAULT_POLICY, now),
        None
    );
    assert_eq!(
        state.deadline(TerminalPresentationMode::Unfocused, DEFAULT_POLICY, now),
        None
    );
    assert_eq!(
        state.deadline(TerminalPresentationMode::Hidden, DEFAULT_POLICY, now),
        None
    );
}

#[test]
fn focused_output_coalesces_each_pending_batch_within_the_frame_cadence() {
    let mut state = TerminalPresentationState::default();
    let started_at = Instant::now();
    state.record_output(started_at);

    assert_eq!(
        state.deadline(
            TerminalPresentationMode::Focused,
            DEFAULT_POLICY,
            started_at
        ),
        Some(started_at + FOCUSED_FAST_INTERVAL)
    );
    state.mark_presented(started_at);
    state.record_output(started_at + Duration::from_millis(1));

    assert_eq!(
        state.deadline(
            TerminalPresentationMode::Focused,
            DEFAULT_POLICY,
            started_at + Duration::from_millis(1)
        ),
        Some(started_at + Duration::from_millis(1) + FOCUSED_FAST_INTERVAL)
    );
}

#[test]
fn focused_continuous_output_adapts_to_balanced_and_sustained_rates() {
    let mut state = TerminalPresentationState::default();
    let started_at = Instant::now();
    state.record_output(started_at);
    state.mark_presented(started_at);

    for elapsed_ms in (100..=500).step_by(100) {
        state.record_output(started_at + Duration::from_millis(elapsed_ms));
    }
    assert_eq!(
        state.focused_interval(DEFAULT_POLICY),
        FOCUSED_BALANCED_INTERVAL
    );

    for elapsed_ms in (600..=2_000).step_by(100) {
        state.record_output(started_at + Duration::from_millis(elapsed_ms));
    }
    assert_eq!(
        state.focused_interval(DEFAULT_POLICY),
        FOCUSED_SUSTAINED_INTERVAL
    );
}

#[test]
fn focused_output_collects_a_new_frame_after_quiet_period() {
    let mut state = TerminalPresentationState::default();
    let started_at = Instant::now();
    state.record_output(started_at);
    state.mark_presented(started_at);
    let resumed_at = started_at + QUIET_RESET_INTERVAL;
    state.record_output(resumed_at);

    assert_eq!(
        state.deadline(
            TerminalPresentationMode::Focused,
            DEFAULT_POLICY,
            resumed_at
        ),
        Some(resumed_at + FOCUSED_FAST_INTERVAL)
    );
    assert_eq!(
        state.focused_interval(DEFAULT_POLICY),
        FOCUSED_FAST_INTERVAL
    );
}

#[test]
fn unfocused_output_is_limited_to_four_hertz() {
    let mut state = TerminalPresentationState::default();
    let started_at = Instant::now();
    state.record_output(started_at);
    state.mark_presented(started_at);
    let next_output_at = started_at + Duration::from_millis(1);
    state.record_output(next_output_at);

    assert_eq!(
        state.deadline(
            TerminalPresentationMode::Unfocused,
            DEFAULT_POLICY,
            next_output_at
        ),
        Some(started_at + DEFAULT_POLICY.unfocused_interval())
    );
}

#[test]
fn hidden_output_has_no_deadline() {
    let mut state = TerminalPresentationState::default();
    let now = Instant::now();
    state.record_output(now);

    assert_eq!(
        state.deadline(TerminalPresentationMode::Hidden, DEFAULT_POLICY, now),
        None
    );
    assert!(state.is_dirty());
}

#[test]
fn pending_output_uses_new_mode_after_focus_change() {
    let mut state = TerminalPresentationState::default();
    let started_at = Instant::now();
    state.record_output(started_at);
    state.mark_presented(started_at);
    let next_output_at = started_at + Duration::from_millis(1);
    state.record_output(next_output_at);

    assert_eq!(
        state.deadline(
            TerminalPresentationMode::Unfocused,
            DEFAULT_POLICY,
            next_output_at
        ),
        Some(started_at + DEFAULT_POLICY.unfocused_interval())
    );
    assert_eq!(
        state.deadline(
            TerminalPresentationMode::Focused,
            DEFAULT_POLICY,
            next_output_at
        ),
        Some(next_output_at + FOCUSED_FAST_INTERVAL)
    );
}

#[test]
fn configured_frame_rates_limit_focused_and_unfocused_output() {
    let policy = TerminalPresentationPolicy::new(15, 2);
    let mut state = TerminalPresentationState::default();
    let started_at = Instant::now();
    state.record_output(started_at);
    state.mark_presented(started_at);
    let next_output_at = started_at + Duration::from_millis(1);
    state.record_output(next_output_at);

    assert_eq!(
        state.deadline(TerminalPresentationMode::Focused, policy, next_output_at),
        Some(started_at + policy.focused_interval())
    );
    assert_eq!(
        state.deadline(TerminalPresentationMode::Unfocused, policy, next_output_at),
        Some(started_at + policy.unfocused_interval())
    );
}

#[test]
fn software_uses_configured_frame_cadence_and_keeps_the_unfocused_cap() {
    let software_policy = DEFAULT_POLICY.with_software_renderer(true);
    let mut state = TerminalPresentationState::default();
    let started_at = Instant::now();
    state.record_output(started_at);
    state.mark_presented(started_at);
    let next_output_at = started_at + Duration::from_millis(1);
    state.record_output(next_output_at);
    assert_eq!(
        state.deadline(
            TerminalPresentationMode::Focused,
            software_policy,
            next_output_at
        ),
        Some(next_output_at + software_policy.focused_interval())
    );
    assert_eq!(
        state.deadline(
            TerminalPresentationMode::Unfocused,
            software_policy,
            next_output_at
        ),
        Some(started_at + software_policy.unfocused_interval())
    );
}

#[test]
fn redraw_coalescing_extends_the_soft_deadline_but_keeps_a_fixed_cap() {
    for software in [false, true] {
        let policy = DEFAULT_POLICY.with_software_renderer(software);
        let mut state = TerminalPresentationState::default();
        let started_at = Instant::now();
        state.record_output(started_at);
        for elapsed_ms in [1, 4, 8, 12, 16, 20, 30, 40] {
            let now = started_at + Duration::from_millis(elapsed_ms);
            state.record_output(now);
            let deadline = state
                .deadline(TerminalPresentationMode::Focused, policy, now)
                .expect("pending redraw");
            assert!(deadline <= started_at + Duration::from_millis(34));
            assert!(deadline > started_at + Duration::from_millis(16));
            if elapsed_ms >= 20 {
                assert!(deadline >= started_at + Duration::from_millis(33));
            }
            if elapsed_ms == 40 {
                assert!(deadline < now, "continuous output must be ready by the cap");
            }
        }
    }
}

#[test]
fn software_does_not_apply_the_sustained_output_reduction() {
    let mut state = TerminalPresentationState::default();
    let started_at = Instant::now();
    state.record_output(started_at);
    state.mark_presented(started_at);
    for elapsed_ms in (100..=2_000).step_by(100) {
        state.record_output(started_at + Duration::from_millis(elapsed_ms));
    }
    let now = started_at + Duration::from_secs(2);
    state.mark_presented(now);
    state.record_output(now + Duration::from_millis(1));
    assert_eq!(
        state.deadline(
            TerminalPresentationMode::Focused,
            DEFAULT_POLICY.with_software_renderer(true),
            now + Duration::from_millis(1)
        ),
        Some(now + Duration::from_millis(1) + DEFAULT_POLICY.focused_interval())
    );
    assert_eq!(
        state.focused_interval(DEFAULT_POLICY),
        FOCUSED_SUSTAINED_INTERVAL
    );
}

#[tokio::test(start_paused = true)]
async fn redraw_continuous_fragments_publish_without_a_quiet_gap() {
    let mut presentation = TerminalPresentation::new();
    let tab_id = Uuid::from_u128(1);
    for _ in 0..3 {
        let started_at = Instant::now();
        presentation.record_output(None, None);
        loop {
            if tokio::time::timeout(
                Duration::from_millis(1),
                presentation.wait_until_ready(tab_id),
            )
            .await
            .is_ok()
            {
                break;
            }
            assert!(Instant::now() - started_at < Duration::from_millis(34));
            presentation.record_output(None, None);
        }
        assert!(Instant::now() - started_at >= Duration::from_millis(33));
        assert!(Instant::now() - started_at <= Duration::from_millis(34));
        assert!(!presentation.has_pending_output());
    }
}

#[test]
fn redraw_high_fps_keeps_the_coalescing_budget_short() {
    for software in [false, true] {
        let policy = TerminalPresentationPolicy::new(120, 4).with_software_renderer(software);
        let mut state = TerminalPresentationState::default();
        let started_at = Instant::now();
        state.record_output(started_at);
        for elapsed_ms in [1, 4, 8, 12, 16, 20] {
            let now = started_at + Duration::from_millis(elapsed_ms);
            state.record_output(now);
            let deadline = state
                .deadline(TerminalPresentationMode::Focused, policy, now)
                .expect("pending redraw");
            assert!(deadline <= started_at + Duration::from_millis(17));
            if elapsed_ms == 20 {
                assert!(deadline < now);
            }
        }
    }
}

#[tokio::test]
async fn closed_watch_is_disabled_after_one_wakeup() {
    let (sender, receiver) = watch::channel(0_u8);
    drop(sender);
    let mut changes = Some(receiver);

    wait_for_watch_change(&mut changes).await;

    assert!(changes.is_none());
}
