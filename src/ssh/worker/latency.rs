//! One cancellation-safe, worker-owned request on an authenticated transport.

use std::future::{Future, pending};
use std::pin::Pin;

use tokio::time::{Instant, sleep_until};

use super::*;

const FOREGROUND_INTERVAL: Duration = Duration::from_secs(10);
const BACKGROUND_INTERVAL: Duration = Duration::from_secs(30);
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const SAMPLE_LIFETIME: Duration = Duration::from_secs(90);

type Probe = Pin<Box<dyn Future<Output = Result<Duration>> + Send>>;
type ProbeFactory = Box<dyn Fn() -> Probe + Send>;

pub(super) struct LatencyMonitor {
    probe_factory: ProbeFactory,
    probe: Option<Probe>,
    foreground_rx: Option<watch::Receiver<bool>>,
    foreground: bool,
    last_completion: Instant,
    next_probe: Instant,
    deadline: Instant,
    expires: Instant,
    timed_out: bool,
    expired: bool,
    stopped: bool,
}

impl LatencyMonitor {
    pub(super) fn new(connection: SshConnection, foreground_rx: watch::Receiver<bool>) -> Self {
        Self::with_probe(
            Box::new(move || {
                let connection = connection.clone();
                Box::pin(async move { connection.measure_latency().await })
            }),
            foreground_rx,
        )
    }

    fn with_probe(probe_factory: ProbeFactory, mut foreground_rx: watch::Receiver<bool>) -> Self {
        let now = Instant::now();
        let foreground = *foreground_rx.borrow_and_update();
        Self {
            probe_factory,
            probe: None,
            foreground_rx: Some(foreground_rx),
            foreground,
            last_completion: now,
            next_probe: now,
            deadline: now,
            expires: now + SAMPLE_LIFETIME,
            timed_out: false,
            expired: false,
            stopped: false,
        }
    }

    fn interval(&self) -> Duration {
        if self.foreground {
            FOREGROUND_INTERVAL
        } else {
            BACKGROUND_INTERVAL
        }
    }

    /// The request lives in `self`, so another worker-select branch cannot drop it.
    pub(super) async fn next_event(&mut self) -> SshSessionEvent {
        loop {
            if self.probe.is_none() && !self.stopped && Instant::now() >= self.next_probe {
                self.probe = Some((self.probe_factory)());
                self.deadline = Instant::now() + PROBE_TIMEOUT;
                self.timed_out = false;
            }
            tokio::select! {
                result = async {
                    match self.probe.as_mut() {
                        Some(probe) => probe.await,
                        None => pending().await,
                    }
                } => {
                    self.probe = None;
                    self.last_completion = Instant::now();
                    self.next_probe = self.last_completion + self.interval();
                    self.expires = self.last_completion + SAMPLE_LIFETIME;
                    self.expired = false;
                    return match result {
                        Ok(round_trip) => SshSessionEvent::LatencyMeasured {
                            round_trip,
                            measured_at: std::time::Instant::now(),
                        },
                        Err(error) => {
                            debug!(%error, "SSH latency response unavailable");
                            self.stopped = true;
                            self.expired = true;
                            SshSessionEvent::LatencyUnavailable
                        }
                    };
                }
                _ = sleep_until(self.deadline), if self.probe.is_some() && !self.timed_out => {
                    // Keep waiting for this exact reply. SSH global replies have no IDs.
                    self.timed_out = true;
                    self.expires = Instant::now() + SAMPLE_LIFETIME;
                    self.expired = false;
                    return SshSessionEvent::LatencyTimedOut;
                }
                _ = sleep_until(self.expires), if !self.expired => {
                    self.expired = true;
                    return SshSessionEvent::LatencyExpired;
                }
                _ = sleep_until(self.next_probe), if self.probe.is_none() && !self.stopped => {}
                changed = async {
                    match self.foreground_rx.as_mut() {
                        Some(rx) => rx.changed().await,
                        None => pending().await,
                    }
                } => {
                    if changed.is_err() {
                        self.foreground_rx = None;
                    } else if let Some(rx) = self.foreground_rx.as_mut() {
                        self.foreground = *rx.borrow_and_update();
                        self.next_probe = self.last_completion + self.interval();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::sync::oneshot;

    #[tokio::test(start_paused = true)]
    async fn timeout_and_select_cancellation_retain_one_request_and_accept_late_reply() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (reply_tx, reply_rx) = oneshot::channel();
        let reply = Arc::new(Mutex::new(Some(reply_rx)));
        let (foreground_tx, foreground_rx) = watch::channel(true);
        let mut monitor = LatencyMonitor::with_probe(
            Box::new({
                let calls = calls.clone();
                move || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    let rx = reply.lock().unwrap().take();
                    Box::pin(async move {
                        match rx {
                            Some(rx) => rx.await.context("test reply dropped"),
                            None => Ok(Duration::from_millis(20)),
                        }
                    })
                }
            }),
            foreground_rx,
        );
        // Cancellation by another session event must leave the pending probe intact.
        assert!(
            timeout(Duration::from_secs(1), monitor.next_event())
                .await
                .is_err()
        );
        assert_eq!(monitor.next_event().await, SshSessionEvent::LatencyTimedOut);
        foreground_tx.send(false).unwrap();
        assert_eq!(monitor.next_event().await, SshSessionEvent::LatencyExpired);
        assert!(
            timeout(Duration::from_secs(120), monitor.next_event())
                .await
                .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        reply_tx.send(Duration::from_secs(215)).unwrap();
        assert!(
            matches!(monitor.next_event().await, SshSessionEvent::LatencyMeasured { round_trip, .. } if round_trip == Duration::from_secs(215))
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let recovered_at = Instant::now();
        assert!(matches!(
            monitor.next_event().await,
            SshSessionEvent::LatencyMeasured { .. }
        ));
        assert_eq!(recovered_at.elapsed(), BACKGROUND_INTERVAL);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_worker_monitor_cancels_the_pending_probe() {
        struct ProbeOwner(Arc<AtomicUsize>);
        impl Drop for ProbeOwner {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let dropped = Arc::new(AtomicUsize::new(0));
        let (_tx, rx) = watch::channel(false);
        let mut monitor = LatencyMonitor::with_probe(
            Box::new({
                let dropped = dropped.clone();
                move || {
                    let owner = ProbeOwner(dropped.clone());
                    Box::pin(async move {
                        let _owner = owner;
                        pending().await
                    })
                }
            }),
            rx,
        );
        assert!(
            timeout(Duration::from_secs(1), monitor.next_event())
                .await
                .is_err()
        );
        assert_eq!(dropped.load(Ordering::SeqCst), 0);
        drop(monitor);
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn samples_immediately_then_uses_foreground_and_background_intervals() {
        let (tx, rx) = watch::channel(false);
        let mut monitor = LatencyMonitor::with_probe(
            Box::new(|| Box::pin(async { Ok(Duration::from_millis(20)) })),
            rx,
        );
        let started = Instant::now();
        assert!(matches!(
            monitor.next_event().await,
            SshSessionEvent::LatencyMeasured { .. }
        ));
        assert_eq!(started.elapsed(), Duration::ZERO);
        monitor.next_event().await;
        assert_eq!(started.elapsed(), BACKGROUND_INTERVAL);
        tx.send(true).unwrap();
        monitor.next_event().await;
        assert_eq!(started.elapsed(), BACKGROUND_INTERVAL + FOREGROUND_INTERVAL);
        drop(tx);
        monitor.next_event().await;
        assert_eq!(
            started.elapsed(),
            BACKGROUND_INTERVAL + FOREGROUND_INTERVAL * 2
        );
    }

    #[tokio::test(start_paused = true)]
    async fn transport_error_stops_sampling_instead_of_reporting_a_valid_reply() {
        let (_tx, rx) = watch::channel(true);
        let mut monitor = LatencyMonitor::with_probe(
            Box::new(|| Box::pin(async { anyhow::bail!("transport closed") })),
            rx,
        );
        assert_eq!(
            monitor.next_event().await,
            SshSessionEvent::LatencyUnavailable
        );
        assert!(
            timeout(Duration::from_secs(180), monitor.next_event())
                .await
                .is_err()
        );
    }
}
