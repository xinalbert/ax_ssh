//! Watches the displayed local SFTP directory without putting filesystem work on the UI thread.

use std::path::Path;
use std::sync::{Arc, Mutex, mpsc as std_mpsc};

use notify::{EventKind, RecursiveMode, Watcher};
use slint::Weak;
use tokio::runtime::Handle;
use tokio::time::{Duration, Instant, MissedTickBehavior, interval, sleep};
use tracing::warn;
use uuid::Uuid;

use super::{AppState, AppWindow, dispatch_active_snapshot};

const WATCH_CHECK_INTERVAL: Duration = Duration::from_millis(250);
const EVENT_DEBOUNCE: Duration = Duration::from_millis(150);
const WATCH_RETRY_INTERVAL: Duration = Duration::from_secs(5);

pub(super) fn ensure_local_directory_watch(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: Weak<AppWindow>,
    tab_id: Uuid,
) {
    let refresh_runtime = runtime.clone();
    let refresh_state = state.clone();
    spawn_directory_watch(
        runtime,
        state,
        tab_id,
        |event_tx| notify::recommended_watcher(move |result| record_change(&event_tx, result)),
        move |request_id, path| {
            dispatch_active_snapshot(&ui, &refresh_state);
            super::sftp_bridge::load_local_directory(
                &refresh_runtime,
                refresh_state.clone(),
                ui.clone(),
                tab_id,
                request_id,
                path,
            );
        },
    );
}

// Only native notification acquisition is replaced in tests. The same task,
// single-slot queue, deadlines and application gates run in both paths.
trait DirectoryWatcher: Send + 'static {
    fn watch(&mut self, path: &Path) -> notify::Result<()>;
    fn unwatch(&mut self, path: &Path) -> notify::Result<()>;
}

impl DirectoryWatcher for notify::RecommendedWatcher {
    fn watch(&mut self, path: &Path) -> notify::Result<()> {
        Watcher::watch(self, path, RecursiveMode::NonRecursive)
    }

    fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        Watcher::unwatch(self, path)
    }
}

struct WatchRegistration {
    state: Arc<Mutex<AppState>>,
    tab_id: Uuid,
}

impl Drop for WatchRegistration {
    fn drop(&mut self) {
        mark_watcher_stopped(&self.state, self.tab_id);
    }
}

fn record_change(events: &std_mpsc::SyncSender<()>, result: notify::Result<notify::Event>) {
    if !matches!(result, Ok(ref event) if matches!(event.kind, EventKind::Access(_))) {
        // A full slot already represents the change; never block the backend.
        let _ = events.try_send(());
    }
}

fn spawn_directory_watch<W: DirectoryWatcher>(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    tab_id: Uuid,
    create: impl FnOnce(std_mpsc::SyncSender<()>) -> notify::Result<W> + Send + 'static,
    mut refresh: impl FnMut(u64, String) + Send + 'static,
) -> Option<tokio::task::JoinHandle<()>> {
    let should_start = state.lock().ok().is_some_and(|mut app| {
        app.terminal_mut(tab_id)
            .filter(|terminal| terminal.is_sftp())
            .is_some_and(|terminal| terminal.sftp.local.mark_watcher_running())
    });
    if !should_start {
        return None;
    }
    // Capture before spawning so aborting even an unpolled task clears its lease.
    let registration = WatchRegistration { state, tab_id };
    Some(runtime.spawn(async move {
        let state = &registration.state;
        let (event_tx, event_rx) = std_mpsc::sync_channel(1);
        let mut watcher = match create(event_tx) {
            Ok(watcher) => watcher,
            Err(error) => {
                warn!(tab_id = %tab_id, %error, "could not start local directory watcher");
                return;
            }
        };
        let mut watched_directory: Option<String> = None;
        let mut attempted_directory: Option<String> = None;
        let mut retry_after = Instant::now();
        let mut checks = interval(WATCH_CHECK_INTERVAL);
        checks.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            checks.tick().await;
            let Some(directory) = current_local_directory(state, tab_id) else {
                break;
            };
            if watched_directory.as_deref() != Some(directory.as_str()) {
                if let Some(previous) = watched_directory.take()
                    && let Err(error) = watcher.unwatch(Path::new(&previous))
                {
                    warn!(tab_id = %tab_id, %error, "could not release previous local directory watch");
                }
                if attempted_directory.as_deref() != Some(directory.as_str()) {
                    // Notifications queued for the old directory must not refresh
                    // a newly selected directory.
                    while event_rx.try_recv().is_ok() {}
                    attempted_directory = Some(directory.clone());
                    retry_after = Instant::now();
                }
                if Instant::now() >= retry_after {
                    match watcher.watch(Path::new(&directory)) {
                        Ok(()) => watched_directory = Some(directory),
                        Err(error) => {
                            retry_after = Instant::now() + WATCH_RETRY_INTERVAL;
                            warn!(tab_id = %tab_id, %error, "could not watch local directory");
                        }
                    }
                }
            }

            if event_rx.try_recv().is_err() {
                continue;
            }
            sleep(EVENT_DEBOUNCE).await;
            while event_rx.try_recv().is_ok() {}
            let Some(watched) = watched_directory.as_deref() else {
                continue;
            };
            if let Some((request_id, path)) = queue_local_change_refresh(state, tab_id, watched) {
                refresh(request_id, path);
            }
        }
        // Dropping the native watcher stops notifications; the registration
        // also resets the state on early failure or runtime cancellation.
    }))
}

fn current_local_directory(state: &Arc<Mutex<AppState>>, tab_id: Uuid) -> Option<String> {
    let app = state.lock().ok()?;
    let terminal = app.terminal(tab_id)?;
    (terminal.is_sftp() && terminal.sftp.local.loaded && !terminal.sftp.local.path.is_empty())
        .then(|| terminal.sftp.local.path.clone())
}

fn queue_local_change_refresh(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    watched: &str,
) -> Option<(u64, String)> {
    let mut app = state.lock().ok()?;
    let terminal = app.terminal_mut(tab_id)?;
    if !terminal.is_sftp() {
        return None;
    }
    match terminal
        .sftp
        .local
        .begin_refresh_after_local_change(watched)
    {
        Ok(request) => request,
        Err(error) => {
            warn!(tab_id = %tab_id, %error, "could not refresh watched local directory");
            None
        }
    }
}

fn mark_watcher_stopped(state: &Arc<Mutex<AppState>>, tab_id: Uuid) {
    if let Ok(mut app) = state.lock()
        && let Some(terminal) = app.terminal_mut(tab_id)
    {
        terminal.sftp.local.mark_watcher_stopped();
    }
}

#[cfg(test)]
mod tests;
