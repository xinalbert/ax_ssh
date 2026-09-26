//! Debounced workspace checkpoints, with a single bounded background writer.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ax_ssh::config::{ConfigStore, WorkspaceSnapshot};
use tokio::runtime::Handle;
use tokio::sync::watch;
use tracing::warn;

use super::{AppState, WindowRouter};

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const QUIET_PERIOD: Duration = Duration::from_secs(1);
const MAX_DIRTY_DELAY: Duration = Duration::from_secs(5);
const TEXT_CHECKPOINT_INTERVAL: Duration = Duration::from_secs(30);

pub(super) struct WorkspaceAutosave {
    timer: slint::Timer,
    sender: watch::Sender<Option<WorkspaceSnapshot>>,
    writer: tokio::task::JoinHandle<()>,
}

#[derive(Default)]
struct CheckpointSchedule {
    observed: Option<WorkspaceSnapshot>,
    dirty_since: Option<Instant>,
    last_change: Option<Instant>,
    last_checkpoint: Option<Instant>,
}

impl CheckpointSchedule {
    fn observe(&mut self, layout: WorkspaceSnapshot, now: Instant) -> bool {
        if self.observed.as_ref() != Some(&layout) {
            self.observed = Some(layout);
            self.dirty_since.get_or_insert(now);
            self.last_change = Some(now);
        }
        let dirty_due = self
            .dirty_since
            .is_some_and(|first| now.duration_since(first) >= MAX_DIRTY_DELAY)
            || self.dirty_since.is_some()
                && self
                    .last_change
                    .is_some_and(|last| now.duration_since(last) >= QUIET_PERIOD);
        let text_due = self
            .last_checkpoint
            .is_some_and(|last| now.duration_since(last) >= TEXT_CHECKPOINT_INTERVAL);
        if dirty_due || text_due {
            self.dirty_since = None;
            self.last_checkpoint = Some(now);
            true
        } else {
            false
        }
    }
}

impl WorkspaceAutosave {
    pub(super) fn start(
        state: Arc<Mutex<AppState>>,
        router: WindowRouter,
        runtime: &Handle,
    ) -> anyhow::Result<Self> {
        let config = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned starting workspace autosave"))?
            .config
            .clone();
        let (sender, receiver) = watch::channel(None);
        let writer = runtime.spawn(write_snapshots(config, receiver));
        let sender_for_timer = sender.clone();
        let schedule = std::cell::RefCell::new(CheckpointSchedule::default());
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::Repeated, POLL_INTERVAL, move || {
            router.capture_placements();
            if let Ok(app) = state.lock()
                && schedule
                    .borrow_mut()
                    .observe(router.layout_snapshot(&app), Instant::now())
            {
                // Only one pending snapshot is retained while disk I/O is in flight.
                sender_for_timer.send_replace(Some(router.snapshot(&app)));
            }
        });
        Ok(Self {
            timer,
            sender,
            writer,
        })
    }

    /// Called after the UI loop stops, before workers and the runtime are shut down.
    pub(super) async fn finish(self, snapshot: Option<WorkspaceSnapshot>) {
        self.timer.stop();
        drop(self.timer);
        if let Some(snapshot) = snapshot {
            self.sender.send_replace(Some(snapshot));
        }
        drop(self.sender);
        if let Err(error) = self.writer.await {
            warn!(%error, "workspace autosave writer failed during shutdown");
        }
    }
}

async fn write_snapshots(
    config: ConfigStore,
    mut receiver: watch::Receiver<Option<WorkspaceSnapshot>>,
) {
    let mut last_written = None;
    while receiver.changed().await.is_ok() {
        let Some(snapshot) = receiver.borrow_and_update().clone() else {
            continue;
        };
        if last_written.as_ref() == Some(&snapshot) {
            continue;
        }
        let config = config.clone();
        // Returning the owned snapshot avoids retaining another terminal-text copy.
        match tokio::task::spawn_blocking(move || {
            config.save_workspace(&snapshot).map(|()| snapshot)
        })
        .await
        {
            Ok(Ok(snapshot)) => last_written = Some(snapshot),
            Ok(Err(error)) => warn!(%error, "failed to save automatic workspace checkpoint"),
            Err(error) => warn!(%error, "workspace checkpoint task failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_ssh::config::{WORKSPACE_SNAPSHOT_VERSION, WorkspaceWindowSnapshot};
    use uuid::Uuid;

    fn snapshot(id: Uuid) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            version: WORKSPACE_SNAPSHOT_VERSION,
            windows: vec![WorkspaceWindowSnapshot {
                id,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn checkpoints_debounce_but_continuous_changes_cannot_starve_them() {
        let mut schedule = CheckpointSchedule::default();
        let start = Instant::now();
        let a = snapshot(Uuid::nil());
        assert!(!schedule.observe(a.clone(), start));
        assert!(!schedule.observe(a.clone(), start + Duration::from_millis(500)));
        assert!(schedule.observe(a.clone(), start + QUIET_PERIOD));
        assert!(!schedule.observe(a.clone(), start + Duration::from_secs(2)));
        for second in 3..8 {
            assert!(!schedule.observe(
                snapshot(Uuid::new_v4()),
                start + Duration::from_secs(second)
            ));
        }
        assert!(schedule.observe(a.clone(), start + Duration::from_secs(8)));
        assert!(schedule.observe(a, start + Duration::from_secs(38)));
    }

    #[tokio::test]
    async fn closed_writer_drains_the_latest_checkpoint_after_inflight_writes() {
        let root = std::env::temp_dir().join(format!("axssh-autosave-{}", Uuid::new_v4()));
        let config = ConfigStore::new(root.join("sessions.json"));
        let (sender, receiver) = watch::channel(None);
        let writer = tokio::spawn(write_snapshots(config.clone(), receiver));
        sender.send_replace(Some(snapshot(Uuid::new_v4())));
        tokio::task::yield_now().await;
        for _ in 0..20 {
            sender.send_replace(Some(snapshot(Uuid::new_v4())));
        }
        let final_snapshot = snapshot(Uuid::nil());
        sender.send_replace(Some(final_snapshot.clone()));
        drop(sender);
        writer.await.expect("writer joins");
        assert_eq!(
            config.load_workspace().expect("load checkpoint"),
            Some(final_snapshot)
        );
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
