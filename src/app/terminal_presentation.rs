use std::future::pending;
use std::time::Instant as StdInstant;

use tokio::sync::{mpsc, watch};
use tokio::time::{Duration, Instant, sleep_until};
use uuid::Uuid;

use super::global_window_router;

const FOCUSED_FAST_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 60);
const FOCUSED_BALANCED_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 30);
const FOCUSED_SUSTAINED_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 20);
const QUIET_RESET_INTERVAL: Duration = Duration::from_millis(250);
const BALANCED_AFTER: Duration = Duration::from_millis(500);
const SUSTAINED_AFTER: Duration = Duration::from_secs(2);
const MAX_EVENTS_BEFORE_PRESENT: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerminalPresentationMode {
    Hidden,
    Unfocused,
    Focused,
}

/// Timer ceilings for dirty terminal UI publication.
/// The route decides whether a pane is focused, visible-unfocused, or hidden.
/// Output enters the bounded latest-frame UI gate at its frame deadline.
/// Software uses the configured focused rate without the sustained-output
/// reduction; parsing, protocol replies and input never wait for presentation.
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
    output_hold_until: Option<Instant>,
    queued_events_processed: usize,
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
            output_hold_until: None,
            queued_events_processed: 0,
        }
    }

    /// Process already queued worker events before drawing their latest state.
    /// The fixed event budget returns control to the monitor's ready-first
    /// select and yields to Tokio, so continuous output cannot starve an
    /// eligible redraw or another task. An empty queue adds no wait and no
    /// output is buffered outside the worker channel.
    pub(super) async fn try_recv_before_present<T>(
        &mut self,
        events: &mut mpsc::Receiver<T>,
    ) -> Option<T> {
        if self.queued_events_processed >= MAX_EVENTS_BEFORE_PRESENT {
            self.queued_events_processed = 0;
            tokio::task::yield_now().await;
            return None;
        }
        match events.try_recv() {
            Ok(event) => {
                self.queued_events_processed += 1;
                Some(event)
            }
            Err(_) => {
                self.queued_events_processed = 0;
                None
            }
        }
    }

    pub(super) fn record_output(
        &mut self,
        output_received_at: Option<StdInstant>,
        hold_for: Option<Duration>,
    ) {
        self.state.record_output(Instant::now());
        self.output_hold_until = hold_for.map(|duration| Instant::now() + duration);
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
        self.output_hold_until = None;
    }

    pub(super) async fn wait_until_ready(&mut self, tab_id: Uuid) -> TerminalPresentationReady {
        loop {
            let now = Instant::now();
            let mode = terminal_presentation_mode(tab_id);
            let policy = terminal_presentation_policy();
            let deadline = self.deadline(mode, policy, now);
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
                            if self.is_ready(mode, policy, now) {
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

    fn deadline(
        &mut self,
        mode: TerminalPresentationMode,
        policy: TerminalPresentationPolicy,
        now: Instant,
    ) -> Option<Instant> {
        let deadline = self.state.deadline(mode, policy, now)?;
        let hold_until = self.output_hold_until.filter(|until| *until > now);
        self.output_hold_until = hold_until;
        Some(hold_until.map_or(deadline, |until| deadline.max(until)))
    }

    fn is_ready(
        &mut self,
        mode: TerminalPresentationMode,
        policy: TerminalPresentationPolicy,
        now: Instant,
    ) -> bool {
        self.deadline(mode, policy, now)
            .is_some_and(|deadline| deadline <= now)
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
    wait_for_watch_change(route_changes).await;
}

async fn wait_for_policy_change(
    policy_changes: &mut Option<watch::Receiver<TerminalPresentationPolicy>>,
) {
    wait_for_watch_change(policy_changes).await;
}

/// Wait once for a watch update, permanently disabling a receiver whose sender
/// has gone away. `watch::Receiver::changed` immediately returns an error once
/// its sender is dropped; retaining that receiver in a repeating `select!`
/// would otherwise turn an idle monitor into a busy loop.
async fn wait_for_watch_change<T>(changes: &mut Option<watch::Receiver<T>>) {
    if let Some(receiver) = changes.as_mut() {
        if receiver.changed().await.is_err() {
            *changes = None;
        }
        return;
    }
    pending::<()>().await;
}

#[derive(Debug, Default)]
struct TerminalPresentationState {
    dirty: bool,
    first_frame: bool,
    pending_since: Option<Instant>,
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
            self.first_frame = true;
        }
        self.pending_since.get_or_insert(now);
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
            TerminalPresentationMode::Focused if self.first_frame => {
                // A chunk can end between a status update and cursor restoration.
                // Later arrivals share this fixed first-frame window, including
                // after idle; they never extend its deadline.
                self.pending_since
                    .map(|since| since + FOCUSED_FAST_INTERVAL.min(policy.focused_interval()))
            }
            TerminalPresentationMode::Focused => {
                let interval = if policy.software_renderer {
                    policy.focused_interval()
                } else {
                    self.focused_interval(policy)
                };
                Some(
                    self.last_presented_at
                        .map_or(now, |presented_at| presented_at + interval),
                )
            }
            TerminalPresentationMode::Unfocused => {
                Some(self.last_presented_at.map_or(now, |presented_at| {
                    presented_at + policy.unfocused_interval()
                }))
            }
        }
    }

    fn mark_presented(&mut self, now: Instant) {
        self.dirty = false;
        self.first_frame = false;
        self.pending_since = None;
        self.last_presented_at = Some(now);
    }

    fn clear_pending(&mut self) {
        self.dirty = false;
        self.first_frame = false;
        self.pending_since = None;
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
mod tests;
