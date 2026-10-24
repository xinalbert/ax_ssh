//! Shared external-edit state and private byte-preserving copies. UI and watchers stay with their owners.
use crate::sftp::{SftpWriteEvent, SftpWriteOperation};
use anyhow::{Context, Result};
use sha1::{Digest, Sha1};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;
const MAX_TAB_EDITS: usize = 8;
const MAX_RETAINED_COPIES: usize = 128;
pub type ContentStamp = [u8; 20];

// This digest only suppresses unchanged saves; it is not a trust boundary.
pub fn content_stamp(data: &[u8]) -> ContentStamp {
    Sha1::digest(data).into()
}

#[derive(Clone)]
struct RemoteEdit {
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

pub struct RemoteEdits {
    files: Vec<RemoteEdit>,
    current: Option<Uuid>,
    pub auto_upload: bool,
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
    pub fn current(&self) -> Option<RemoteEditView> {
        self.files
            .iter()
            .find(|edit| Some(edit.id) == self.current)
            .map(RemoteEdit::view)
    }
    pub fn get(&self, id: Uuid) -> Option<RemoteEditView> {
        self.files
            .iter()
            .find(|edit| edit.id == id)
            .map(RemoteEdit::view)
    }
    #[cfg(feature = "egui-ui")]
    pub(crate) fn snapshots(&self) -> Vec<RemoteEditView> {
        self.files.iter().map(RemoteEdit::view).collect()
    }
    pub fn attach(
        &mut self,
        id: Uuid,
        local_path: PathBuf,
        stamp: ContentStamp,
        expected_size: u64,
        expected_modified: Option<u32>,
    ) -> bool {
        let Some(edit) = self
            .files
            .iter_mut()
            .find(|edit| edit.id == id && edit.local_path.is_none())
        else {
            return false;
        };
        edit.local_path = Some(local_path);
        edit.expected_size = expected_size;
        edit.expected_modified = expected_modified;
        edit.uploaded = stamp;
        edit.observed = edit.uploaded;
        edit.status = "Opening system application".into();
        true
    }
    pub fn set_status(&mut self, id: Uuid, status: String) {
        if let Some(edit) = self.files.iter_mut().find(|edit| edit.id == id) {
            edit.status = status.chars().take(1024).collect();
        }
    }
    pub fn copy_error(&mut self, id: Uuid, error: &anyhow::Error) {
        if let Some(edit) = self.files.iter_mut().find(|edit| edit.id == id) {
            edit.upload_requested = false;
            edit.status = format!("Local edit unavailable; copy retained: {error:#}")
                .chars()
                .take(1024)
                .collect();
        }
    }
    pub fn save(
        &mut self,
        id: Uuid,
        max_bytes: u64,
        data: Vec<u8>,
        queue: impl FnOnce(Uuid, SftpWriteOperation) -> Result<()>,
    ) -> bool {
        let Some(edit) = self.files.iter_mut().find(|edit| edit.id == id) else {
            return false;
        };
        let stamp = content_stamp(&data);
        queue_edit_save(edit, self.auto_upload, max_bytes, data, stamp, queue)
    }
    #[cfg(feature = "egui-ui")]
    pub(crate) fn stop(&mut self, id: Uuid) {
        self.files.retain(|edit| edit.id != id);
        if self.current == Some(id) {
            self.current = self.files.last().map(|edit| edit.id);
        }
    }
    #[cfg(feature = "egui-ui")]
    pub(crate) fn request_upload_for(&mut self, id: Uuid) -> Result<()> {
        anyhow::ensure!(
            self.files.iter().any(|edit| edit.id == id),
            "edit is no longer available"
        );
        self.current = Some(id);
        self.request_upload()
    }

    pub fn presentation(&self) -> (Option<String>, String, String, bool) {
        match self.current() {
            Some(edit) => (
                Some(edit.path.clone()),
                edit.local_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                edit.status.clone(),
                edit.local_path.is_some() && !edit.pending,
            ),
            None => (None, String::new(), String::new(), false),
        }
    }

    pub fn open(&mut self, path: String) -> Result<(Uuid, Option<PathBuf>, bool)> {
        crate::sftp::validate_remote_path(&path)?;
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

    pub fn stop_current(&mut self) {
        if let Some(id) = self.current.take() {
            self.files.retain(|edit| edit.id != id);
        }
        self.current = self.files.last().map(|edit| edit.id);
    }

    pub fn request_upload(&mut self) -> Result<()> {
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

    pub fn apply_result(&mut self, event: &SftpWriteEvent) -> bool {
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

#[derive(Clone, Debug)]
pub struct RemoteEditView {
    pub id: Uuid,
    pub path: String,
    pub local_path: Option<PathBuf>,
    pub status: String,
    pub pending: bool,
    pub upload_requested: bool,
    pub failed: bool,
    pub changed: bool,
}
impl RemoteEdit {
    fn view(&self) -> RemoteEditView {
        RemoteEditView {
            id: self.id,
            path: self.path.clone(),
            local_path: self.local_path.clone(),
            status: self.status.clone(),
            pending: self.pending.is_some(),
            upload_requested: self.upload_requested,
            failed: self.failed,
            changed: self.uploaded != self.observed,
        }
    }
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

pub fn create_copy_in(
    root: &Path,
    remote_path: &str,
    data: &[u8],
    max_bytes: u64,
) -> Result<PathBuf> {
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

pub fn remove_unused_copy(path: &Path) {
    if let Some(folder) = path.parent()
        && let Err(error) = fs::remove_dir_all(folder)
    {
        tracing::warn!(%error, "cannot remove unused edit directory");
    }
}

pub fn open_read_copy(path: &Path, max_bytes: u64) -> Result<File> {
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

pub fn read_copy(path: &Path, max_bytes: u64) -> Result<Vec<u8>> {
    read_open_copy(open_read_copy(path, max_bytes)?, max_bytes)
}

pub fn read_open_copy(mut file: File, max_bytes: u64) -> Result<Vec<u8>> {
    let initial = file
        .metadata()
        .context("cannot inspect edit file before reading")?;
    // An external editor may grow the file after open_read_copy validated it.
    // Recheck before reserving memory, as well as after the bounded read.
    if initial.len() > max_bytes {
        anyhow::bail!("local edit copy exceeds the {max_bytes}-byte edit limit");
    }
    let before = crate::file_identity::local_file_fingerprint(&file)?;
    let mut data = Vec::with_capacity(initial.len() as usize);
    (&mut file)
        .take(max_bytes + 1)
        .read_to_end(&mut data)
        .context("cannot read saved edit copy")?;
    if data.len() as u64 != initial.len()
        || data.len() as u64 > max_bytes
        || before != crate::file_identity::local_file_fingerprint(&file)?
    {
        anyhow::bail!("local edit copy changed while reading; save again or retry upload");
    }
    Ok(data)
}

#[cfg(test)]
mod tests;
