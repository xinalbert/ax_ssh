//! System-associated editing of private local copies; no document content enters Slint.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc as std_mpsc};

use anyhow::{Context, Result};
use ax_ssh::sftp::SftpWriteEvent;
use directories::ProjectDirs;
use notify::{EventKind, RecursiveMode, Watcher};
use tokio::runtime::Handle;
use tokio::time::{Duration, Instant, MissedTickBehavior, interval};
use uuid::Uuid;

use super::{AppState, AppWindow, TerminalTabState, dispatch_active_snapshot};
pub(super) use ax_ssh::remote_edit::RemoteEdits;
use ax_ssh::remote_edit::{
    RemoteEditView as RemoteEdit, content_stamp, create_copy_in, open_read_copy, read_copy,
    read_open_copy, remove_unused_copy,
};

const DEBOUNCE: Duration = Duration::from_millis(500);
fn connected_edit(terminal: &TerminalTabState, attempt_id: Uuid, id: Uuid) -> Option<RemoteEdit> {
    if !terminal.is_sftp()
        || !terminal.connected
        || terminal.ssh_route().and_then(|(_, attempt)| attempt) != Some(attempt_id)
    {
        return None;
    }
    terminal.sftp.external_edits.get(id)
}

pub(super) fn finish_download(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    attempt_id: Uuid,
    event: SftpWriteEvent,
) {
    let SftpWriteEvent::EditFile {
        operation_id: id,
        path,
        data,
        expected_size,
        expected_modified,
    } = event
    else {
        return;
    };
    let runtime = runtime.clone();
    runtime.clone().spawn(async move {
        let Some(max_bytes) = state.lock().ok().and_then(|app| {
            app.terminal(tab_id)
                .and_then(|terminal| connected_edit(terminal, attempt_id, id))?;
            Some(
                app.sessions
                    .settings
                    .sftp_transfers
                    .remote_edit_max_file_bytes(),
            )
        }) else {
            return;
        };
        let copy_path = path.clone();
        let stamp = content_stamp(&data);
        let created =
            tokio::task::spawn_blocking(move || create_copy(&copy_path, &data, max_bytes))
                .await
                .context("local edit copy task failed")
                .and_then(|result| result);
        let local_path = match created {
            Ok(path) => path,
            Err(error) => {
                fail_download(&state, tab_id, attempt_id, id, &error);
                dispatch_active_snapshot(&ui, &state);
                return;
            }
        };
        let accepted = state.lock().ok().is_some_and(|mut app| {
            let Some(terminal) = app.terminal_mut(tab_id) else {
                return false;
            };
            if connected_edit(terminal, attempt_id, id).is_none() {
                return false;
            }
            if !terminal
                .sftp
                .external_edits
                .get(id)
                .is_some_and(|edit| edit.path == path)
            {
                return false;
            }
            terminal.sftp.external_edits.attach(
                id,
                local_path.clone(),
                stamp,
                expected_size,
                expected_modified,
            )
        });
        if !accepted {
            // No opener has seen this copy, so a cancelled download can remove it.
            let cleanup = local_path.clone();
            if let Err(error) =
                tokio::task::spawn_blocking(move || remove_unused_copy(&cleanup)).await
            {
                tracing::warn!(%error, "cancelled edit copy cleanup task failed");
            }
            return;
        }
        watch_copy(
            &runtime,
            state.clone(),
            ui.clone(),
            tab_id,
            attempt_id,
            id,
            local_path.clone(),
        );
        open_copy(&runtime, state, ui, tab_id, attempt_id, id, local_path);
    });
}

pub(super) fn reopen_current(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
) -> Result<()> {
    let (attempt_id, id, path) = {
        let app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("application state lock poisoned"))?;
        let terminal = app.terminal(tab_id).context("SFTP tab closed")?;
        let edit = terminal
            .sftp
            .external_edits
            .current()
            .context("no external edit")?;
        (
            terminal
                .ssh_route()
                .and_then(|(_, attempt)| attempt)
                .context("SFTP tab is disconnected")?,
            edit.id,
            edit.local_path.clone().context("edit copy is not ready")?,
        )
    };
    open_copy(runtime, state, ui, tab_id, attempt_id, id, path);
    Ok(())
}

fn open_copy(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    attempt_id: Uuid,
    id: Uuid,
    path: PathBuf,
) {
    runtime.spawn(async move {
        let Some(max_bytes) = state.lock().ok().and_then(|app| {
            app.terminal(tab_id)
                .and_then(|terminal| connected_edit(terminal, attempt_id, id))?;
            Some(
                app.sessions
                    .settings
                    .sftp_transfers
                    .remote_edit_max_file_bytes(),
            )
        }) else {
            return;
        };
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || {
                read_copy(&path, max_bytes)?;
                // Windows waits only for the system launcher, so association
                // errors are observable; the external document lifetime is separate.
                #[cfg(windows)]
                let opened = open::that(&path);
                #[cfg(not(windows))]
                let opened = open::that_detached(&path);
                opened.context(
                    "cannot open the system-associated application; check the file association",
                )
            }),
        )
        .await
        .context("system opener timed out")
        .and_then(|result| result.context("system opener task failed"))
        .and_then(|result| result);
        if let Ok(mut app) = state.lock()
            && let Some(terminal) = app.terminal_mut(tab_id)
            && connected_edit(terminal, attempt_id, id).is_some()
        {
            let status = match result {
                Ok(()) => "Editing in system application".into(),
                Err(error) => format!("System application failed; local copy retained: {error:#}"),
            };
            terminal.sftp.external_edits.set_status(id, status);
        }
        dispatch_active_snapshot(&ui, &state);
    });
}

fn fail_download(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    attempt_id: Uuid,
    id: Uuid,
    error: &anyhow::Error,
) {
    if let Ok(mut app) = state.lock()
        && let Some(terminal) = app.terminal_mut(tab_id)
        && connected_edit(terminal, attempt_id, id).is_some()
    {
        terminal
            .sftp
            .external_edits
            .apply_result(&SftpWriteEvent::Failed {
                operation_id: id,
                message: format!("{error:#}"),
            });
        terminal.sftp.status = format!("Cannot prepare external edit: {error:#}");
    }
}

fn watch_copy(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    attempt_id: Uuid,
    id: Uuid,
    path: PathBuf,
) {
    runtime.spawn(async move {
        let (sender, receiver) = std_mpsc::sync_channel(1);
        let mut watcher =
            match notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if !matches!(event, Ok(ref event) if matches!(event.kind, EventKind::Access(_))) {
                    // One signal represents any burst, without blocking the backend.
                    match sender.try_send(()) {
                        Ok(())
                        | Err(std_mpsc::TrySendError::Full(()))
                        | Err(std_mpsc::TrySendError::Disconnected(())) => {}
                    }
                }
            }) {
                Ok(watcher) => Some(watcher),
                Err(error) => {
                    tracing::warn!(%error, "edit watcher unavailable; using bounded polling");
                    None
                }
            };
        if let Some(watcher) = watcher.as_mut()
            && let Some(parent) = path.parent()
            && let Err(error) = watcher.watch(parent, RecursiveMode::NonRecursive)
        {
            tracing::warn!(%error, "edit watcher unavailable; using bounded polling");
        }
        let mut checks = interval(Duration::from_millis(250));
        checks.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut dirty_at = Some(Instant::now());
        let mut last_check = Instant::now();
        let mut last_fingerprint = None;
        let mut last_limit = None;
        loop {
            checks.tick().await;
            let edit = state.lock().ok().and_then(|app| {
                app.terminal(tab_id).and_then(|terminal| {
                    connected_edit(terminal, attempt_id, id).map(|edit| {
                        (
                            edit,
                            terminal.sftp.external_edits.auto_upload,
                            app.sessions
                                .settings
                                .sftp_transfers
                                .remote_edit_max_file_bytes(),
                        )
                    })
                })
            });
            let Some((edit, auto_upload, max_bytes)) = edit else {
                break;
            };
            if receiver.try_recv().is_ok() {
                while receiver.try_recv().is_ok() {}
                dirty_at = Some(Instant::now());
            }
            let limit_changed = last_limit != Some(max_bytes);
            let requested = edit.upload_requested;
            let due = dirty_at.is_some_and(|at| at.elapsed() >= DEBOUNCE);
            let fallback = last_check.elapsed() >= Duration::from_secs(5);
            // An ack may release a second save even without another file event.
            let pending_changes = auto_upload && !edit.pending && !edit.failed && edit.changed;
            if !requested && !due && !fallback && !pending_changes && !limit_changed {
                continue;
            }
            if edit.pending {
                continue;
            }
            last_check = Instant::now();
            last_limit = Some(max_bytes);
            dirty_at = None;
            let local = path.clone();
            let force = requested || due || pending_changes || limit_changed;
            let read = tokio::task::spawn_blocking(move || {
                let file = open_read_copy(&local, max_bytes)?;
                let fingerprint = super::local_files::local_file_fingerprint(&file)?;
                if !force && last_fingerprint == Some(fingerprint) {
                    return Ok(None);
                }
                let data = read_open_copy(file, max_bytes)?;
                Ok::<_, anyhow::Error>(Some((data, fingerprint)))
            })
            .await
            .context("edit file read task failed")
            .and_then(|result| result);
            let changed = match read {
                Ok(Some((data, fingerprint))) => {
                    last_fingerprint = Some(fingerprint);
                    queue_saved_copy(&state, tab_id, attempt_id, id, data)
                }
                Ok(None) => false,
                Err(error) => {
                    last_fingerprint = None;
                    set_copy_error(&state, tab_id, attempt_id, id, &error);
                    true
                }
            };
            if changed {
                dispatch_active_snapshot(&ui, &state);
            }
        }
        // Copies survive stop/disconnect/close: the external editor may still
        // own the document, and unuploaded changes must remain recoverable.
    });
}

fn set_copy_error(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    attempt_id: Uuid,
    id: Uuid,
    error: &anyhow::Error,
) {
    if let Ok(mut app) = state.lock()
        && let Some(terminal) = app.terminal_mut(tab_id)
        && connected_edit(terminal, attempt_id, id).is_some()
    {
        terminal.sftp.external_edits.copy_error(id, error);
    }
}

fn queue_saved_copy(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    attempt_id: Uuid,
    id: Uuid,
    data: Vec<u8>,
) -> bool {
    let Ok(mut app) = state.lock() else {
        return false;
    };
    let max_bytes = app
        .sessions
        .settings
        .sftp_transfers
        .remote_edit_max_file_bytes();
    let Some(terminal) = app.terminal_mut(tab_id) else {
        return false;
    };
    if connected_edit(terminal, attempt_id, id).is_none() {
        return false;
    }
    let worker = terminal.worker.as_ref();
    terminal
        .sftp
        .external_edits
        .save(id, max_bytes, data, |operation_id, operation| {
            worker
                .context("SFTP worker is unavailable")?
                .request_sftp_write(operation_id, operation)
        })
}

fn create_copy(remote_path: &str, data: &[u8], max_bytes: u64) -> Result<PathBuf> {
    let root = ProjectDirs::from("com", "axsoft", "ax_ssh")
        .context("cannot determine edit cache directory")?
        .cache_dir()
        .join("sftp-edit");
    create_copy_in(&root, remote_path, data, max_bytes)
}

#[cfg(test)]
mod tests;
