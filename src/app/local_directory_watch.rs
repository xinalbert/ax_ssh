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
    let should_start = match state.lock() {
        Ok(mut app) => app
            .terminal_mut(tab_id)
            .filter(|terminal| terminal.is_sftp())
            .is_some_and(|terminal| terminal.sftp.local.mark_watcher_running()),
        Err(_) => false,
    };
    if !should_start {
        return;
    }

    let runtime = runtime.clone();
    runtime.clone().spawn(async move {
        // The callback never blocks the notify backend: one slot represents
        // "the displayed directory changed" regardless of burst size.
        let (event_tx, event_rx) = std_mpsc::sync_channel(1);
        let mut watcher = match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            if !matches!(result, Ok(ref event) if matches!(event.kind, EventKind::Access(_))) {
                let _ = event_tx.try_send(());
            }
        }) {
            Ok(watcher) => watcher,
            Err(error) => {
                warn!(tab_id = %tab_id, %error, "could not start local directory watcher");
                mark_watcher_stopped(&state, tab_id);
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
            let Some(directory) = current_local_directory(&state, tab_id) else {
                break;
            };
            if watched_directory.as_deref() != Some(directory.as_str()) {
                if let Some(previous) = watched_directory.take()
                    && let Err(error) = watcher.unwatch(Path::new(&previous))
                {
                    warn!(tab_id = %tab_id, %error, "could not release previous local directory watch");
                }
                if attempted_directory.as_deref() != Some(directory.as_str()) {
                    attempted_directory = Some(directory.clone());
                    retry_after = Instant::now();
                }
                if Instant::now() >= retry_after {
                    match watcher.watch(Path::new(&directory), RecursiveMode::NonRecursive) {
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
            let request = queue_local_change_refresh(&state, tab_id, watched);
            if let Some((request_id, path)) = request {
                dispatch_active_snapshot(&ui, &state);
                super::sftp_bridge::load_local_directory(
                    &runtime,
                    state.clone(),
                    ui.clone(),
                    tab_id,
                    request_id,
                    path,
                );
            }
        }
        // Dropping the watcher stops native notifications for this Tab.
        mark_watcher_stopped(&state, tab_id);
    });
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
