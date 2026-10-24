//! External-copy jobs and observed saves owned by one SFTP companion.

use super::*;
use crate::file_identity::{LocalFileFingerprint, local_file_fingerprint};
use crate::remote_edit::{
    ContentStamp, RemoteEdits, content_stamp, create_copy_in, open_read_copy, read_copy,
    read_open_copy, remove_unused_copy,
};
use crate::sftp::{SftpWriteEvent, SftpWriteOperation};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

type Opener = Arc<dyn Fn(&Path) -> Result<()> + Send + Sync>;

pub(super) struct Editing {
    pub(super) edits: RemoteEdits,
    pub(super) observers: HashMap<Uuid, Observation>,
    permits: HashMap<Uuid, Arc<AtomicBool>>,
    pub(super) task: Option<JoinHandle<JobResult>>,
    pending_open: VecDeque<Uuid>,
    opener: Opener,
    root: Option<PathBuf>,
}

impl Default for Editing {
    fn default() -> Self {
        Self {
            edits: RemoteEdits::default(),
            observers: HashMap::new(),
            permits: HashMap::new(),
            task: None,
            pending_open: VecDeque::new(),
            root: None,
            opener: Arc::new(|path| {
                #[cfg(windows)]
                let result = open::that(path);
                #[cfg(not(windows))]
                let result = open::that_detached(path);
                result.context("cannot open system-associated editor")
            }),
        }
    }
}

pub(super) struct Observation {
    checked: Instant,
    fingerprint: Option<LocalFileFingerprint>,
    read_fingerprint: Option<LocalFileFingerprint>,
    changed_at: Instant,
}

pub(super) struct JobResult {
    generation: u64,
    id: Uuid,
    result: Result<Job>,
}

enum Job {
    Copy {
        path: PathBuf,
        stamp: ContentStamp,
        size: u64,
        modified: Option<u32>,
    },
    Read {
        fingerprint: LocalFileFingerprint,
        data: Option<Vec<u8>>,
    },
    Opened,
}

impl Editing {
    fn reap_contexts(&mut self) {
        self.permits.retain(|id, permit| {
            let retained = self.edits.get(*id).is_some();
            if !retained {
                permit.store(false, Ordering::Release);
            }
            retained
        });
        self.observers.retain(|id, _| self.edits.get(*id).is_some());
    }
    pub(super) fn reset(&mut self) {
        for permit in self.permits.values() {
            permit.store(false, Ordering::Release);
        }
        self.permits.clear();
        self.edits = RemoteEdits::default();
        self.observers.clear();
        self.pending_open.clear();
        // Blocking jobs retain ownership until joined/completed; late results
        // are filtered by edit UUID and connection generation before publication.
    }

    pub(super) fn stop(&mut self, id: Uuid) {
        if let Some(permit) = self.permits.remove(&id) {
            permit.store(false, Ordering::Release);
        }
        self.edits.stop(id);
        self.observers.remove(&id);
        self.pending_open.retain(|pending| *pending != id);
    }

    pub(super) async fn finish(&mut self) -> Result<()> {
        self.reset();
        if let Some(task) = self.task.take() {
            let result = task.await.context("external edit cleanup task failed")?;
            if let Ok(Job::Copy { path, .. }) = result.result {
                tokio::task::spawn_blocking(move || remove_unused_copy(&path))
                    .await
                    .context("unused edit copy cleanup failed")?;
            }
        }
        Ok(())
    }

    fn root(&self) -> Result<PathBuf> {
        if let Some(root) = &self.root {
            return Ok(root.clone());
        }
        Ok(
            directories::ProjectDirs::from("com", "axsoft", "ax_ssh_egui")
                .context("cannot determine experimental edit directory")?
                .cache_dir()
                .join("sftp-edit"),
        )
    }
}

fn max_bytes() -> u64 {
    crate::config::SftpTransferSettings::default().remote_edit_max_file_bytes()
}

impl Worker {
    pub(super) fn begin_edit(&mut self, path: String) -> Result<()> {
        anyhow::ensure!(
            self.connected && !self.loading,
            "SFTP directory is not ready"
        );
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.path == path)
            .context("remote entry is no longer in this directory")?;
        anyhow::ensure!(
            !entry.is_dir && !entry.is_symlink && entry.size <= max_bytes(),
            "edit requires a regular remote file within the edit limit"
        );
        let (id, local, download) = self.editing.edits.open(path.clone())?;
        self.editing
            .permits
            .entry(id)
            .or_insert_with(|| Arc::new(AtomicBool::new(true)));
        if local.is_some() {
            return self.reopen_edit(id);
        }
        if download {
            let result = self
                .transport
                .as_ref()
                .context("SFTP is disconnected")?
                .request_sftp_write(
                    id,
                    SftpWriteOperation::ReadEditFile {
                        path,
                        max_bytes: max_bytes(),
                    },
                );
            if let Err(error) = result {
                self.editing.edits.apply_result(&SftpWriteEvent::Failed {
                    operation_id: id,
                    message: format!("{error:#}"),
                });
                self.editing.reap_contexts();
                return Err(error);
            }
        }
        Ok(())
    }

    pub(super) fn reopen_edit(&mut self, id: Uuid) -> Result<()> {
        anyhow::ensure!(self.connected, "SFTP is disconnected");
        let edit = self
            .editing
            .edits
            .get(id)
            .context("edit is no longer available")?;
        anyhow::ensure!(edit.local_path.is_some(), "edit copy is not ready");
        if !self.editing.pending_open.contains(&id) {
            self.editing.pending_open.push_back(id);
        }
        self.check_edits()
    }

    pub(super) fn edit_event(&mut self, event: SftpWriteEvent) -> Result<()> {
        match event {
            SftpWriteEvent::EditFile {
                operation_id: id,
                path,
                data,
                expected_size,
                expected_modified,
            } => {
                let Some(edit) = self
                    .editing
                    .edits
                    .get(id)
                    .filter(|edit| edit.path == path && edit.local_path.is_none())
                else {
                    return Ok(());
                };
                // One local file job is admitted at a time; reject overload visibly,
                // allowing an explicit retry without blocking the SSH event drain.
                if self.editing.task.is_some() {
                    self.status = "Local edit worker is busy; retry Edit".into();
                    self.editing.edits.apply_result(&SftpWriteEvent::Failed {
                        operation_id: id,
                        message: "local edit worker is busy; retry Edit".into(),
                    });
                    self.editing.reap_contexts();
                    return Ok(());
                }
                let root = self.editing.root()?;
                let generation = self.generation;
                self.editing.task = Some(self.runtime.spawn_blocking(move || JobResult {
                    generation,
                    id,
                    result: create_copy_in(&root, &edit.path, &data, max_bytes()).map(|path| {
                        Job::Copy {
                            path,
                            stamp: content_stamp(&data),
                            size: expected_size,
                            modified: expected_modified,
                        }
                    }),
                }));
            }
            event => {
                if let SftpWriteEvent::Failed { message, .. } = &event {
                    self.status = format!("External edit failed: {message}")
                        .chars()
                        .take(1024)
                        .collect();
                }
                let changed = match &event {
                    SftpWriteEvent::Updated { path, .. } => Some(path.clone()),
                    _ => None,
                };
                if self.editing.edits.apply_result(&event)
                    && let Some(path) = changed
                {
                    // Refresh only the currently visible parent; navigation keeps
                    // its own request identity and isn't redirected by an ack.
                    if path
                        .rsplit_once('/')
                        .map(|(parent, _)| if parent.is_empty() { "/" } else { parent })
                        == Some(self.path.as_str())
                        && !self.loading
                    {
                        self.next_request();
                        if let Some(transport) = &self.transport {
                            match transport.request_list_sftp(self.request_id, self.path.clone()) {
                                Ok(()) => self.loading = true,
                                Err(error) => {
                                    self.status =
                                        format!("Cannot refresh edited directory: {error:#}")
                                }
                            }
                        }
                    }
                }
            }
        }
        self.editing.reap_contexts();
        Ok(())
    }

    pub(super) fn complete_edit_job(&mut self, result: JobResult) -> Result<bool> {
        let current = self.connected
            && result.generation == self.generation
            && self.editing.edits.get(result.id).is_some();
        if !current {
            if let Ok(Job::Copy { path, .. }) = result.result {
                // Keep cleanup owned by this same slot, never detach it.
                let generation = self.generation;
                self.editing.task = Some(self.runtime.spawn_blocking(move || {
                    remove_unused_copy(&path);
                    JobResult {
                        generation,
                        id: result.id,
                        result: Ok(Job::Opened),
                    }
                }));
            }
            return Ok(false);
        }
        match result.result {
            Ok(Job::Copy {
                path,
                stamp,
                size,
                modified,
            }) => {
                self.editing
                    .edits
                    .attach(result.id, path, stamp, size, modified);
                self.editing.observers.insert(
                    result.id,
                    Observation {
                        checked: Instant::now(),
                        fingerprint: None,
                        read_fingerprint: None,
                        changed_at: Instant::now(),
                    },
                );
                self.reopen_edit(result.id)?;
            }
            Ok(Job::Opened) => self
                .editing
                .edits
                .set_status(result.id, "Editing in system application".into()),
            Ok(Job::Read { fingerprint, data }) => {
                if let Some(observation) = self.editing.observers.get_mut(&result.id) {
                    if observation.fingerprint != Some(fingerprint) {
                        observation.changed_at = Instant::now();
                    }
                    observation.fingerprint = Some(fingerprint);
                    if data.is_some() {
                        observation.read_fingerprint = Some(fingerprint);
                    }
                }
                if let Some(data) = data {
                    let worker = self.transport.as_ref();
                    self.editing
                        .edits
                        .save(result.id, max_bytes(), data, |id, operation| {
                            worker
                                .context("SFTP is disconnected")?
                                .request_sftp_write(id, operation)
                        });
                } else {
                    return Ok(false);
                }
            }
            Err(error) => {
                self.status = format!("External edit failed: {error:#}")
                    .chars()
                    .take(1024)
                    .collect();
                if self
                    .editing
                    .edits
                    .get(result.id)
                    .is_some_and(|edit| edit.local_path.is_none())
                {
                    self.editing.edits.apply_result(&SftpWriteEvent::Failed {
                        operation_id: result.id,
                        message: format!("{error:#}"),
                    });
                } else {
                    self.editing.edits.copy_error(result.id, &error);
                }
            }
        }
        self.editing.reap_contexts();
        Ok(true)
    }

    pub(super) fn check_edits(&mut self) -> Result<()> {
        if !self.connected || self.editing.task.is_some() {
            return Ok(());
        }
        while let Some(id) = self.editing.pending_open.pop_front() {
            let Some(path) = self.editing.edits.get(id).and_then(|edit| edit.local_path) else {
                continue;
            };
            let opener = self.editing.opener.clone();
            let permit = self
                .editing
                .permits
                .get(&id)
                .cloned()
                .context("edit permit is no longer available")?;
            let generation = self.generation;
            self.editing.task = Some(self.runtime.spawn_blocking(move || {
                JobResult {
                    generation,
                    id,
                    result: read_copy(&path, max_bytes())
                        .and_then(|_| {
                            anyhow::ensure!(
                                permit.load(Ordering::Acquire),
                                "edit opening was cancelled"
                            );
                            opener(&path)
                        })
                        .map(|_| Job::Opened),
                }
            }));
            return Ok(());
        }
        let now = Instant::now();
        let next = self
            .editing
            .edits
            .snapshots()
            .into_iter()
            .filter(|edit| !edit.pending && edit.local_path.is_some())
            .filter_map(|edit| {
                self.editing
                    .observers
                    .get(&edit.id)
                    .map(|observation| (edit, observation.checked))
            })
            .filter(|(edit, checked)| {
                edit.upload_requested || now.duration_since(*checked) >= Duration::from_millis(500)
            })
            .min_by_key(|(_, checked)| *checked);
        let Some((edit, _)) = next else {
            return Ok(());
        };
        let observation = self
            .editing
            .observers
            .get_mut(&edit.id)
            .context("missing edit observation")?;
        observation.checked = now;
        let previous = observation.fingerprint;
        let read_fingerprint = observation.read_fingerprint;
        let stable = now.duration_since(observation.changed_at) >= Duration::from_millis(500);
        let force = edit.upload_requested
            || (self.editing.edits.auto_upload && edit.changed && !edit.failed);
        let path = edit.local_path.context("edit copy missing")?;
        let generation = self.generation;
        self.editing.task = Some(self.runtime.spawn_blocking(move || JobResult {
            generation,
            id: edit.id,
            result: (|| {
                let file = open_read_copy(&path, max_bytes())?;
                let fingerprint = local_file_fingerprint(&file)?;
                let data = if force
                    || (stable
                        && previous == Some(fingerprint)
                        && read_fingerprint != Some(fingerprint))
                {
                    Some(read_open_copy(file, max_bytes())?)
                } else {
                    None
                };
                Ok(Job::Read { fingerprint, data })
            })(),
        }));
        Ok(())
    }
}

pub(super) async fn next_job(
    task: &mut Option<JoinHandle<JobResult>>,
) -> std::result::Result<JobResult, tokio::task::JoinError> {
    match task {
        Some(task) => task.await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests;
