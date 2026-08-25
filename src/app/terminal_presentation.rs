use std::future::pending;
use std::time::Instant as StdInstant;

use tokio::sync::watch;
use tokio::time::{Duration, Instant, sleep_until};
use uuid::Uuid;

use super::global_window_router;

const FOCUSED_FAST_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 60);
const FOCUSED_BALANCED_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 30);
const FOCUSED_SUSTAINED_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 20);
const QUIET_RESET_INTERVAL: Duration = Duration::from_millis(250);
const BALANCED_AFTER: Duration = Duration::from_millis(500);
const SUSTAINED_AFTER: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerminalPresentationMode {
    Hidden,
    Unfocused,
    Focused,
}

/// Timer ceilings for dirty terminal UI publication on non-software renderers.
/// The route decides whether a pane is focused, visible-unfocused, or hidden;
/// `winit-software` instead submits immediately into the bounded latest-frame
/// UI gate and never delays terminal parsing or transport work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TerminalPresentationPolicy {
    focused_fps: u16,
    unfocused_fps: u16,
    software_renderer: bool,
}

impl TerminalPresentationPolicy {
    pub(super) const fn new(focused_fps: u16, unfocused_fps: u16) -> Self {
        Self {
            focused_fps,
            unfocused_fps,
            software_renderer: false,
        }
    }

    pub(super) const fn with_software_renderer(mut self, software_renderer: bool) -> Self {
        self.software_renderer = software_renderer;
        self
    }

    pub(super) const fn software_renderer(self) -> bool {
        self.software_renderer
    }

    fn focused_interval(self) -> Duration {
        frame_interval(self.focused_fps)
    }

    fn unfocused_interval(self) -> Duration {
        frame_interval(self.unfocused_fps)
    }
}

impl Default for TerminalPresentationPolicy {
    fn default() -> Self {
        Self::new(60, 4)
    }
}

fn frame_interval(fps: u16) -> Duration {
    Duration::from_secs(1) / u32::from(fps.max(1))
}

pub(super) struct TerminalPresentationReady {
    pub(super) output_received_at: Option<StdInstant>,
}

pub(super) struct TerminalPresentation {
    state: TerminalPresentationState,
    route_changes: Option<watch::Receiver<u64>>,
    policy_changes: Option<watch::Receiver<TerminalPresentationPolicy>>,
    earliest_output_received_at: Option<StdInstant>,
}

impl TerminalPresentation {
    pub(super) fn new() -> Self {
        Self {
            state: TerminalPresentationState::default(),
            route_changes: global_window_router()
                .map(|router| router.subscribe_terminal_presentation_changes()),
            policy_changes: global_window_router()
                .map(|router| router.subscribe_terminal_presentation_policy()),
            earliest_output_received_at: None,
        }
    }

    pub(super) fn record_output(&mut self, output_received_at: Option<StdInstant>) {
        self.state.record_output(Instant::now());
        if let Some(received_at) = output_received_at {
            self.earliest_output_received_at = Some(
                self.earliest_output_received_at
                    .map_or(received_at, |current| current.min(received_at)),
            );
        }
    }

    pub(super) fn has_pending_output(&self) -> bool {
        self.state.is_dirty()
    }

    pub(super) fn clear_pending_output(&mut self) {
        self.state.clear_pending();
        self.earliest_output_received_at = None;
    }

    pub(super) async fn wait_until_ready(&mut self, tab_id: Uuid) -> TerminalPresentationReady {
        loop {
            let now = Instant::now();
            let mode = terminal_presentation_mode(tab_id);
            let policy = terminal_presentation_policy();
            let deadline = self.state.deadline(mode, policy, now);
            if deadline.is_some_and(|deadline| deadline <= now) {
                self.state.mark_presented(now);
                return TerminalPresentationReady {
                    output_received_at: self.earliest_output_received_at.take(),
                };
            }
            match deadline {
                Some(deadline) => {
                    tokio::select! {
                        _ = sleep_until(deadline) => {
                            let now = Instant::now();
                            let mode = terminal_presentation_mode(tab_id);
                            let policy = terminal_presentation_policy();
                            if self.state.is_ready(mode, policy, now) {
                                self.state.mark_presented(now);
                                return TerminalPresentationReady {
                                    output_received_at: self.earliest_output_received_at.take(),
                                };
                            }
                        }
                        _ = wait_for_route_change(&mut self.route_changes) => {}
                        _ = wait_for_policy_change(&mut self.policy_changes) => {}
                    }
                }
                None => {
                    tokio::select! {
                        _ = wait_for_route_change(&mut self.route_changes) => {}
                        _ = wait_for_policy_change(&mut self.policy_changes) => {}
                    }
                }
            }
        }
    }
}

fn terminal_presentation_mode(tab_id: Uuid) -> TerminalPresentationMode {
    global_window_router().map_or(TerminalPresentationMode::Focused, |router| {
        router.terminal_presentation_mode(tab_id)
    })
}

fn terminal_presentation_policy() -> TerminalPresentationPolicy {
    global_window_router().map_or_else(TerminalPresentationPolicy::default, |router| {
        router.terminal_presentation_policy()
    })
}

async fn wait_for_route_change(route_changes: &mut Option<watch::Receiver<u64>>) {
    match route_changes {
        Some(route_changes) => {
            let _ = route_changes.changed().await;
        }
        None => pending::<()>().await,
    }
}

async fn wait_for_policy_change(
    policy_changes: &mut Option<watch::Receiver<TerminalPresentationPolicy>>,
) {
    match policy_changes {
        Some(policy_changes) => {
            let _ = policy_changes.changed().await;
        }
        None => pending::<()>().await,
    }
}

#[derive(Debug, Default)]
struct TerminalPresentationState {
    dirty: bool,
    immediate_after_quiet: bool,
    burst_started_at: Option<Instant>,
    last_output_at: Option<Instant>,
    last_presented_at: Option<Instant>,
}

impl TerminalPresentationState {
    fn record_output(&mut self, now: Instant) {
        let starts_new_burst = self.last_output_at.is_none_or(|last_output_at| {
            now.saturating_duration_since(last_output_at) >= QUIET_RESET_INTERVAL
        });
        if starts_new_burst {
            self.burst_started_at = Some(now);
            self.immediate_after_quiet = true;
        }
        self.last_output_at = Some(now);
        self.dirty = true;
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn deadline(
        &self,
        mode: TerminalPresentationMode,
        policy: TerminalPresentationPolicy,
        now: Instant,
    ) -> Option<Instant> {
        if !self.dirty {
            return None;
        }
        match mode {
            TerminalPresentationMode::Hidden => None,
            TerminalPresentationMode::Focused | TerminalPresentationMode::Unfocused
                if policy.software_renderer =>
            {
                // Software frames enter the AppState single-slot refresh gate
                // immediately. If the UI is busy, newer output is merged when
                // the pending snapshot is consumed instead of accumulating
                // timer-delayed work.
                Some(now)
            }
            TerminalPresentationMode::Focused if self.immediate_after_quiet => Some(now),
            TerminalPresentationMode::Focused => {
                Some(self.last_presented_at.map_or(now, |presented_at| {
                    presented_at + self.focused_interval(policy)
                }))
            }
            TerminalPresentationMode::Unfocused => {
                Some(self.last_presented_at.map_or(now, |presented_at| {
                    presented_at + policy.unfocused_interval()
                }))
            }
        }
    }

    fn is_ready(
        &self,
        mode: TerminalPresentationMode,
        policy: TerminalPresentationPolicy,
        now: Instant,
    ) -> bool {
        self.deadline(mode, policy, now)
            .is_some_and(|deadline| deadline <= now)
    }

    fn mark_presented(&mut self, now: Instant) {
        self.dirty = false;
        self.immediate_after_quiet = false;
        self.last_presented_at = Some(now);
    }

    fn clear_pending(&mut self) {
        self.dirty = false;
        self.immediate_after_quiet = false;
    }

    fn focused_interval(&self, policy: TerminalPresentationPolicy) -> Duration {
        let burst_duration = self.last_output_at.zip(self.burst_started_at).map_or(
            Duration::ZERO,
            |(last_output_at, burst_started_at)| {
                last_output_at.saturating_duration_since(burst_started_at)
            },
        );
        if burst_duration >= SUSTAINED_AFTER {
            FOCUSED_SUSTAINED_INTERVAL.max(policy.focused_interval())
        } else if burst_duration >= BALANCED_AFTER {
            FOCUSED_BALANCED_INTERVAL.max(policy.focused_interval())
        } else {
            FOCUSED_FAST_INTERVAL.max(policy.focused_interval())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_POLICY: TerminalPresentationPolicy = TerminalPresentationPolicy::new(60, 4);
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
    fn focused_first_output_is_immediate_then_frame_bounded() {
        let mut state = TerminalPresentationState::default();
        let started_at = Instant::now();
        state.record_output(started_at);

        assert_eq!(
            state.deadline(
                TerminalPresentationMode::Focused,
                DEFAULT_POLICY,
                started_at
            ),
            Some(started_at)
        );
        state.mark_presented(started_at);
        state.record_output(started_at + Duration::from_millis(1));

        assert_eq!(
            state.deadline(
                TerminalPresentationMode::Focused,
                DEFAULT_POLICY,
                started_at + Duration::from_millis(1)
            ),
            Some(started_at + FOCUSED_FAST_INTERVAL)
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
    fn focused_output_returns_to_immediate_after_quiet_period() {
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
            Some(resumed_at)
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
            Some(started_at + FOCUSED_FAST_INTERVAL)
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
    fn software_visible_output_is_immediate_without_a_timer_cap() {
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
            Some(next_output_at)
        );
        assert_eq!(
            state.deadline(
                TerminalPresentationMode::Unfocused,
                software_policy,
                next_output_at
            ),
            Some(next_output_at)
        );
    }
}
