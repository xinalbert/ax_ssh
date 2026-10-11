//! System-associated editing of private local copies; no document content enters Slint.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, mpsc as std_mpsc};

use anyhow::{Context, Result};
use ax_ssh::sftp::{SftpWriteEvent, SftpWriteOperation};
use directories::ProjectDirs;
use notify::{EventKind, RecursiveMode, Watcher};
use sha1::{Digest, Sha1};
use tokio::runtime::Handle;
use tokio::time::{Duration, Instant, MissedTickBehavior, interval};
use uuid::Uuid;

use super::{AppState, AppWindow, TerminalTabState, dispatch_active_snapshot};

const MAX_TAB_EDITS: usize = 8;
const MAX_RETAINED_COPIES: usize = 128;
const DEBOUNCE: Duration = Duration::from_millis(500);
type ContentStamp = [u8; 20];

// This digest only suppresses unchanged saves; it is not a trust boundary.
fn content_stamp(data: &[u8]) -> ContentStamp {
    Sha1::digest(data).into()
}

#[derive(Clone)]
pub(super) struct RemoteEdit {
    id: Uuid,
    path: String,
    local_path: Option<PathBuf>,
    expected_size: u64,
    expected_modified: Option<u32>,
    uploaded: ContentStamp,
    observed: ContentStamp,
    pending: Option<(Uuid, ContentStamp)>,
    upload_requested: bool,
    failed: bool,
    status: String,
}

pub(super) struct RemoteEdits {
    files: Vec<RemoteEdit>,
    current: Option<Uuid>,
    pub(super) auto_upload: bool,
}

impl Default for RemoteEdits {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            current: None,
            auto_upload: true,
        }
    }
}

impl RemoteEdits {
    pub(super) fn current(&self) -> Option<&RemoteEdit> {
        self.files.iter().find(|edit| Some(edit.id) == self.current)
    }

    pub(super) fn presentation(&self) -> (Option<String>, String, String, bool) {
        match self.current() {
            Some(edit) => (
                Some(edit.path.clone()),
                edit.local_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                edit.status.clone(),
                edit.local_path.is_some() && edit.pending.is_none(),
            ),
            None => (None, String::new(), String::new(), false),
        }
    }

    pub(super) fn open(&mut self, path: String) -> Result<(Uuid, Option<PathBuf>, bool)> {
        if let Some(edit) = self.files.iter().find(|edit| edit.path == path) {
            self.current = Some(edit.id);
            return Ok((edit.id, edit.local_path.clone(), false));
        }
        if self.files.len() >= MAX_TAB_EDITS {
            anyhow::bail!(
                "at most {MAX_TAB_EDITS} files can be edited per tab; stop one edit first"
            );
        }
        let id = Uuid::new_v4();
        self.files.push(RemoteEdit {
            id,
            path,
            local_path: None,
            expected_size: 0,
            expected_modified: None,
            uploaded: [0; 20],
            observed: [0; 20],
            pending: None,
            upload_requested: false,
            failed: false,
            status: "Downloading edit copy".into(),
        });
        self.current = Some(id);
        Ok((id, None, true))
    }

    pub(super) fn stop_current(&mut self) {
        if let Some(id) = self.current.take() {
            self.files.retain(|edit| edit.id != id);
        }
        self.current = self.files.last().map(|edit| edit.id);
    }

    pub(super) fn request_upload(&mut self) -> Result<()> {
        let id = self.current.context("no external file is being edited")?;
        let edit = self
            .files
            .iter_mut()
            .find(|edit| edit.id == id)
            .context("edit is no longer available")?;
        if edit.local_path.is_none() {
            anyhow::bail!("the local edit copy is not ready");
        }
        edit.upload_requested = true;
        edit.failed = false;
        Ok(())
    }

    pub(super) fn apply_result(&mut self, event: &SftpWriteEvent) -> bool {
        match event {
            SftpWriteEvent::Updated {
                operation_id,
                path,
                size,
                modified,
            } => {
                let Some(edit) = self.files.iter_mut().find(|edit| {
                    edit.path == *path && edit.pending.is_some_and(|(id, _)| id == *operation_id)
                }) else {
                    return false;
                };
                if let Some((_, uploaded)) = edit.pending.take() {
                    edit.uploaded = uploaded;
                }
                edit.expected_size = *size;
                edit.expected_modified = *modified;
                edit.failed = false;
                edit.status = if edit.uploaded == edit.observed {
                    "Saved to remote"
                } else {
                    "Local changes pending"
                }
                .into();
                true
            }
            SftpWriteEvent::Failed {
                operation_id,
                message,
            } => {
                let Some(index) = self.files.iter().position(|edit| {
                    edit.id == *operation_id
                        || edit.pending.is_some_and(|(id, _)| id == *operation_id)
                }) else {
                    return false;
                };
                if self.files[index].local_path.is_none() {
                    let removed = self.files.remove(index);
                    if self.current == Some(removed.id) {
                        self.current = self.files.last().map(|edit| edit.id);
                    }
                } else {
                    let edit = &mut self.files[index];
                    edit.pending = None;
                    edit.failed = true;
                    edit.status = format!("Upload failed; local copy retained: {message}");
                }
                true
            }
            _ => false,
        }
    }
}

fn connected_edit(terminal: &TerminalTabState, attempt_id: Uuid, id: Uuid) -> Option<&RemoteEdit> {
    if !terminal.is_sftp()
        || !terminal.connected
        || terminal.ssh_route().and_then(|(_, attempt)| attempt) != Some(attempt_id)
    {
        return None;
    }
    terminal
        .sftp
        .external_edits
        .files
        .iter()
        .find(|edit| edit.id == id)
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
        let stamp = content_stamp(&data);
        let copy_path = path.clone();
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
            let Some(edit) = terminal
                .sftp
                .external_edits
                .files
                .iter_mut()
                .find(|edit| edit.id == id && edit.path == path)
            else {
                return false;
            };
            edit.local_path = Some(local_path.clone());
            edit.expected_size = expected_size;
            edit.expected_modified = expected_modified;
            edit.uploaded = stamp;
            edit.observed = stamp;
            edit.status = "Opening system application".into();
            true
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
            && let Some(edit) = terminal
                .sftp
                .external_edits
                .files
                .iter_mut()
                .find(|edit| edit.id == id)
        {
            match result {
                Ok(()) => edit.status = "Editing in system application".into(),
                Err(error) => {
                    edit.status =
                        format!("System application failed; local copy retained: {error:#}")
                }
            }
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
                    connected_edit(terminal, attempt_id, id)
                        .cloned()
                        .map(|edit| {
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
            let pending_changes = auto_upload
                && edit.pending.is_none()
                && !edit.failed
                && edit.uploaded != edit.observed;
            if !requested && !due && !fallback && !pending_changes && !limit_changed {
                continue;
            }
            if edit.pending.is_some() {
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
        && let Some(edit) = terminal
            .sftp
            .external_edits
            .files
            .iter_mut()
            .find(|edit| edit.id == id)
    {
        edit.upload_requested = false;
        edit.status = format!("Local edit unavailable; copy retained: {error:#}");
    }
}

fn queue_saved_copy(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    attempt_id: Uuid,
    id: Uuid,
    data: Vec<u8>,
) -> bool {
    let stamp = content_stamp(&data);
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
    let auto_upload = terminal.sftp.external_edits.auto_upload;
    let Some(edit) = terminal
        .sftp
        .external_edits
        .files
        .iter_mut()
        .find(|edit| edit.id == id)
    else {
        return false;
    };
    queue_edit_save(
        edit,
        auto_upload,
        max_bytes,
        data,
        stamp,
        |operation_id, operation| {
            terminal
                .worker
                .as_ref()
                .context("SFTP worker is unavailable")?
                .request_sftp_write(operation_id, operation)
        },
    )
}

fn queue_edit_save(
    edit: &mut RemoteEdit,
    auto_upload: bool,
    max_bytes: u64,
    data: Vec<u8>,
    stamp: ContentStamp,
    queue: impl FnOnce(Uuid, SftpWriteOperation) -> Result<()>,
) -> bool {
    let changed = edit.observed != stamp;
    edit.observed = stamp;
    // Settings may have changed while the blocking reader was running.
    if data.len() as u64 > max_bytes {
        edit.upload_requested = false;
        edit.status = format!(
            "Local edit unavailable; copy retained: file exceeds the {max_bytes}-byte edit limit"
        );
        return true;
    }
    if stamp == edit.uploaded {
        edit.upload_requested = false;
        if edit.status.starts_with("Local edit unavailable") {
            edit.status = "Editing in system application".into();
            return true;
        }
        return changed;
    }
    if edit.pending.is_some() || (!edit.upload_requested && (!auto_upload || edit.failed)) {
        if changed && !edit.failed {
            edit.status = "Local changes pending".into();
        }
        return changed;
    }
    let operation_id = Uuid::new_v4();
    let queued = queue(
        operation_id,
        SftpWriteOperation::WriteEditFile {
            path: edit.path.clone(),
            max_bytes,
            data,
            expected_size: edit.expected_size,
            expected_modified: edit.expected_modified,
        },
    );
    edit.upload_requested = false;
    match queued {
        Ok(()) => {
            edit.pending = Some((operation_id, stamp));
            edit.status = "Uploading saved changes".into();
        }
        Err(error) => {
            edit.failed = true;
            edit.status = format!("Cannot upload; local copy retained: {error:#}");
        }
    }
    true
}

fn private_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path).context("cannot create private edit directory")?;
    let metadata = fs::symlink_metadata(path).context("cannot inspect edit directory")?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        anyhow::bail!("edit directory is not a regular directory");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .context("cannot protect edit directory")?;
    }
    Ok(())
}

fn create_copy(remote_path: &str, data: &[u8], max_bytes: u64) -> Result<PathBuf> {
    let root = ProjectDirs::from("com", "axsoft", "ax_ssh")
        .context("cannot determine edit cache directory")?
        .cache_dir()
        .join("sftp-edit");
    create_copy_in(&root, remote_path, data, max_bytes)
}

fn create_copy_in(root: &Path, remote_path: &str, data: &[u8], max_bytes: u64) -> Result<PathBuf> {
    // Serialize quota inspection/publication across this process's tabs.
    static CREATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _creation = CREATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| anyhow::anyhow!("edit cache creation lock poisoned"))?;
    if data.len() as u64 > max_bytes {
        anyhow::bail!("edit copy exceeds the size limit");
    }
    private_directory(root)?;
    let mut entries = fs::read_dir(root).context("cannot inspect retained edit copies")?;
    for _ in 0..MAX_RETAINED_COPIES {
        match entries.next() {
            Some(entry) => {
                entry.context("cannot inspect retained edit copy")?;
            }
            None => return write_copy(root, remote_path, data),
        }
    }
    anyhow::bail!(
        "edit cache holds {MAX_RETAINED_COPIES} copies; preserve needed files and clear old copies in {root:?}"
    )
}

fn write_copy(root: &Path, remote_path: &str, data: &[u8]) -> Result<PathBuf> {
    let original = remote_path
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .context("remote file name missing")?;
    // Preserve the extension and association, while replacing Windows-invalid
    // characters and avoiding reserved device basenames with a fixed prefix.
    let name: String = original
        .chars()
        .map(|ch| {
            if ch.is_control() || "<>:\"/\\|?*".contains(ch) {
                '_'
            } else {
                ch
            }
        })
        .collect();
    let name = format!("edit-{}", name.trim_end_matches([' ', '.']));
    if name.len() > 240 {
        anyhow::bail!("remote name is too long for a local edit copy");
    }
    let folder = root.join(Uuid::new_v4().to_string());
    fs::create_dir(&folder).context("cannot create unique edit directory")?;
    private_directory(&folder)?;
    let path = folder.join(name);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options
            .open(&path)
            .context("cannot create local edit copy")?;
        file.write_all(data)
            .context("cannot write local edit copy")?;
        file.sync_all().context("cannot sync local edit copy")?;
        Ok::<_, anyhow::Error>(path.clone())
    })();
    if result.is_err() {
        remove_unused_copy(&path);
    }
    result
}

fn remove_unused_copy(path: &Path) {
    if let Some(folder) = path.parent()
        && let Err(error) = fs::remove_dir_all(folder)
    {
        tracing::warn!(%error, "cannot remove unused edit directory");
    }
}

fn open_read_copy(path: &Path, max_bytes: u64) -> Result<File> {
    let parent = path.parent().context("edit copy has no parent")?;
    let directory = fs::symlink_metadata(parent).context("cannot inspect local edit directory")?;
    let metadata = fs::symlink_metadata(path).context("cannot inspect local edit file")?;
    if directory.file_type().is_symlink()
        || !directory.is_dir()
        || metadata.file_type().is_symlink()
        || !metadata.is_file()
    {
        anyhow::bail!("local edit copy must remain a regular file in its original directory");
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file: File = options.open(path).context("cannot read local edit copy")?;
    let initial = file.metadata().context("cannot inspect opened edit copy")?;
    if !initial.is_file() || initial.file_type().is_symlink() || initial.len() > max_bytes {
        anyhow::bail!("local edit copy is not a regular file within the {max_bytes}-byte limit");
    }
    Ok(file)
}

fn read_copy(path: &Path, max_bytes: u64) -> Result<Vec<u8>> {
    read_open_copy(open_read_copy(path, max_bytes)?, max_bytes)
}

fn read_open_copy(mut file: File, max_bytes: u64) -> Result<Vec<u8>> {
    let initial = file
        .metadata()
        .context("cannot inspect edit file before reading")?;
    // An external editor may grow the file after open_read_copy validated it.
    // Recheck before reserving memory, as well as after the bounded read.
    if initial.len() > max_bytes {
        anyhow::bail!("local edit copy exceeds the {max_bytes}-byte edit limit");
    }
    let before = super::local_files::local_file_fingerprint(&file)?;
    let mut data = Vec::with_capacity(initial.len() as usize);
    (&mut file)
        .take(max_bytes + 1)
        .read_to_end(&mut data)
        .context("cannot read saved edit copy")?;
    if data.len() as u64 != initial.len()
        || data.len() as u64 > max_bytes
        || before != super::local_files::local_file_fingerprint(&file)?
    {
        anyhow::bail!("local edit copy changed while reading; save again or retry upload");
    }
    Ok(data)
}

#[cfg(test)]
mod tests;
