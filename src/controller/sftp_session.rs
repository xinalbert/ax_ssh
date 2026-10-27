//! Independent SFTP companion owner for the experimental frontend.
//!
//! The companion owns one SFTP-only SSH worker.  Directory pages and transfer
//! progress cross the boundary as bounded, owned snapshots; russh handles and
//! credentials never do.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::config::{SessionProfile, SftpTransferFilterSettings, SftpTransferSettings};
pub use crate::file_icons::FileIconKey as SftpIcon;
use crate::file_icons::{FileIcon, IconPrewarmReport, global_provider, prewarm_async};
use crate::sftp::{SftpBrowserEvent, SftpEntry, SftpTransferEvent, SftpUploadConflictChoice};
use crate::ssh::{HostKeyProbe, SshSessionEvent, SshSessionHandle, TrustDecision};

mod downloads;
pub use downloads::{MAX_SFTP_DOWNLOAD_ROOTS, SftpDownloadBatchResult, SftpDownloadFailure};
mod editing;
mod local_open;
mod mutations;
pub use mutations::SftpEntryAction;
mod transfer_state;
mod transfers;
pub use transfer_state::{
    MAX_SFTP_TRANSFER_BATCH, SftpTransferBatchFailure, SftpTransferBatchResult,
    SftpTransferControl, SftpTransferPhase,
};
pub use transfers::MAX_SFTP_UPLOAD_ROOTS;

use super::{ConnectionIdentity, TerminalPrompt, TerminalPromptKind};

const COMMAND_CAPACITY: usize = 64;
const MAX_ENTRIES: usize = 250;
const MAX_TRANSFERS: usize = 64;
const MAX_LOCAL_ENTRIES: usize = 250;
const MAX_LOCAL_PATH_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalEntryView {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub icon: SftpIcon,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SftpEntryView {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<u32>,
    pub icon: SftpIcon,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SftpTransferView {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub conflict: bool,
    pub phase: SftpTransferPhase,
    pub pausable: bool,
}

#[derive(Clone, Debug)]
pub struct SftpView {
    pub connected: bool,
    pub status: String,
    pub generation: u64,
    pub identity: ConnectionIdentity,
    pub prompt: Option<TerminalPrompt>,
    pub home: String,
    pub path: String,
    pub loading: bool,
    pub has_more: bool,
    pub truncated: bool,
    pub request_id: u64,
    pub entries: Vec<SftpEntryView>,
    pub local_path: String,
    pub local_loading: bool,
    pub local_entries: Vec<LocalEntryView>,
    pub local_revision: u64,
    pub upload_preparing: bool,
    pub icons: Arc<HashMap<SftpIcon, FileIcon>>,
    pub transfers: Vec<SftpTransferView>,
    pub last_batch: Option<SftpTransferBatchResult>,
    pub last_download_batch: Option<SftpDownloadBatchResult>,
    pub edits: Vec<crate::remote_edit::RemoteEditView>,
    pub edit_auto_upload: bool,
}

struct Shared {
    view: SftpView,
    mutation: Option<mutations::Slot>,
}

enum Command {
    DownloadSelected {
        paths: Vec<String>,
        request_id: u64,
        local_directory: String,
        local_revision: u64,
    },
    Mutate {
        id: Uuid,
        target: mutations::Target,
        action: SftpEntryAction,
    },
    Connect(Box<SessionProfile>, Zeroizing<String>),
    Respond(Uuid, Zeroizing<String>),
    List(String),
    LoadMore,
    Download {
        path: String,
        local_directory: PathBuf,
    },
    Cancel(Uuid),
    Pause(Uuid),
    Resume(Uuid),
    ControlBatch {
        ids: Vec<Uuid>,
        control: SftpTransferControl,
    },
    Upload {
        local_path: PathBuf,
        remote_path: String,
    },
    ResolveConflict {
        transfer_id: Uuid,
        choice: SftpUploadConflictChoice,
        apply_to_batch: bool,
    },
    ListLocal(PathBuf),
    OpenLocal {
        path: String,
        revision: u64,
    },
    UploadSelected {
        paths: Vec<String>,
        local_revision: u64,
        remote_directory: String,
        request_id: u64,
    },
    UploadRoots {
        paths: Vec<PathBuf>,
        remote_directory: String,
        request_id: u64,
        local_source: Option<transfers::LocalSource>,
    },
    DownloadDrop {
        path: String,
        request_id: u64,
        local_directory: String,
        local_revision: u64,
    },
    CancelPreparation,
    Edit(String),
    UploadEdit(Uuid),
    ReopenEdit(Uuid),
    StopEdit(Uuid),
    AutoUploadEdits(bool),
    Disconnect,
}

struct QueuedCommand {
    generation: u64,
    command: Command,
}

#[derive(Clone)]
pub struct SftpClient {
    commands: mpsc::Sender<QueuedCommand>,
    generation: Option<u64>,
    shared: Arc<Mutex<Shared>>,
}

impl SftpClient {
    pub fn view(&self) -> Result<SftpView> {
        Ok(self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("SFTP view lock poisoned"))?
            .view
            .clone())
    }

    pub fn for_view(&self, view: &SftpView) -> Self {
        Self {
            generation: Some(view.generation),
            ..self.clone()
        }
    }

    fn send(&self, command: Command) -> Result<()> {
        let generation = match self.generation {
            Some(generation) => generation,
            None => {
                self.shared
                    .lock()
                    .map_err(|_| anyhow::anyhow!("SFTP view lock poisoned"))?
                    .view
                    .generation
            }
        };
        self.commands
            .try_send(QueuedCommand {
                generation,
                command,
            })
            .map_err(|_| anyhow::anyhow!("SFTP companion queue is full or closed"))
    }

    pub fn connect(&self, profile: SessionProfile, secret: Zeroizing<String>) -> Result<()> {
        profile.validate()?;
        anyhow::ensure!(profile.ssh().is_some(), "SFTP requires an SSH profile");
        anyhow::ensure!(
            secret.len() <= 16 * 1024,
            "authentication secret exceeds the input limit"
        );
        self.send(Command::Connect(Box::new(profile), secret))
    }

    pub fn respond(&self, prompt: Uuid, secret: Zeroizing<String>) -> Result<()> {
        anyhow::ensure!(
            secret.len() <= 16 * 1024,
            "authentication secret exceeds the input limit"
        );
        self.send(Command::Respond(prompt, secret))
    }

    pub fn list(&self, path: String) -> Result<()> {
        crate::sftp::validate_remote_path(&path)?;
        self.send(Command::List(path))
    }

    pub fn load_more(&self) -> Result<()> {
        self.send(Command::LoadMore)
    }

    pub fn download(&self, path: String, local_directory: PathBuf) -> Result<()> {
        crate::sftp::validate_remote_path(&path)?;
        self.send(Command::Download {
            path,
            local_directory,
        })
    }

    pub fn control_transfers(&self, ids: Vec<Uuid>, control: SftpTransferControl) -> Result<()> {
        transfer_state::validate_batch(&ids)?;
        self.send(Command::ControlBatch { ids, control })
    }

    pub fn pause(&self, transfer_id: Uuid) -> Result<()> {
        self.send(Command::Pause(transfer_id))
    }

    pub fn resume(&self, transfer_id: Uuid) -> Result<()> {
        self.send(Command::Resume(transfer_id))
    }

    pub fn cancel(&self, transfer_id: Uuid) -> Result<()> {
        self.send(Command::Cancel(transfer_id))
    }

    pub fn upload(&self, local_path: PathBuf, remote_path: String) -> Result<()> {
        crate::sftp::validate_remote_path(&remote_path)?;
        transfers::validate_paths(std::slice::from_ref(&local_path))?;
        self.send(Command::Upload {
            local_path,
            remote_path,
        })
    }

    pub fn resolve_conflict(
        &self,
        transfer_id: Uuid,
        choice: SftpUploadConflictChoice,
    ) -> Result<()> {
        self.resolve_conflict_with_batch(transfer_id, choice, false)
    }

    pub fn resolve_conflict_with_batch(
        &self,
        transfer_id: Uuid,
        choice: SftpUploadConflictChoice,
        apply_to_batch: bool,
    ) -> Result<()> {
        self.send(Command::ResolveConflict {
            transfer_id,
            choice,
            apply_to_batch,
        })
    }

    pub fn disconnect(&self) -> Result<()> {
        self.send(Command::Disconnect)
    }

    pub fn upload_roots(
        &self,
        paths: Vec<PathBuf>,
        remote_directory: String,
        request_id: u64,
    ) -> Result<()> {
        transfers::validate_paths(&paths)?;
        crate::sftp::validate_remote_path(&remote_directory)?;
        self.send(Command::UploadRoots {
            paths,
            remote_directory,
            request_id,
            local_source: None,
        })
    }
    pub fn upload_local_drop(
        &self,
        path: String,
        local_revision: u64,
        remote_directory: String,
        request_id: u64,
    ) -> Result<()> {
        transfers::validate_paths(&[PathBuf::from(&path)])?;
        crate::sftp::validate_remote_path(&remote_directory)?;
        self.send(Command::UploadRoots {
            paths: vec![PathBuf::from(&path)],
            remote_directory,
            request_id,
            local_source: Some(transfers::LocalSource::Entry(path, local_revision)),
        })
    }
    pub fn download_drop(
        &self,
        path: String,
        request_id: u64,
        local_directory: String,
        local_revision: u64,
    ) -> Result<()> {
        crate::sftp::validate_remote_path(&path)?;
        validate_local_directory_input(Path::new(&local_directory))?;
        self.send(Command::DownloadDrop {
            path,
            request_id,
            local_directory,
            local_revision,
        })
    }
    pub fn cancel_upload_preparation(&self) -> Result<()> {
        self.send(Command::CancelPreparation)
    }

    pub fn edit(&self, path: String) -> Result<()> {
        crate::sftp::validate_remote_path(&path)?;
        self.send(Command::Edit(path))
    }
    pub fn upload_edit(&self, id: Uuid) -> Result<()> {
        self.send(Command::UploadEdit(id))
    }
    pub fn reopen_edit(&self, id: Uuid) -> Result<()> {
        self.send(Command::ReopenEdit(id))
    }
    pub fn stop_edit(&self, id: Uuid) -> Result<()> {
        self.send(Command::StopEdit(id))
    }
    pub fn auto_upload_edits(&self, enabled: bool) -> Result<()> {
        self.send(Command::AutoUploadEdits(enabled))
    }

    pub fn list_local(&self, path: PathBuf) -> Result<()> {
        validate_local_directory_input(&path)?;
        self.send(Command::ListLocal(path))
    }

    /// Opens a private snapshot of one file from the displayed local directory.
    pub fn open_local(&self, path: String, revision: u64) -> Result<()> {
        validate_local_directory_input(Path::new(&path))?;
        self.send(Command::OpenLocal { path, revision })
    }
}

pub struct SftpSession {
    client: SftpClient,
    stop: watch::Sender<bool>,
    settings: watch::Sender<Settings>,
    task: JoinHandle<Result<()>>,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct Settings {
    transfers: SftpTransferSettings,
    filters: SftpTransferFilterSettings,
}

impl SftpSession {
    pub fn idle(runtime: &Handle, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self::with_settings(
            runtime,
            SftpTransferSettings::default(),
            SftpTransferFilterSettings::default(),
            wake,
        )
    }

    pub(crate) fn with_settings(
        runtime: &Handle,
        transfers: SftpTransferSettings,
        filters: SftpTransferFilterSettings,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self::start(
            runtime,
            transfers,
            filters,
            wake,
            local_open::Opening::default(),
        )
    }

    fn start(
        runtime: &Handle,
        transfers: SftpTransferSettings,
        filters: SftpTransferFilterSettings,
        wake: Arc<dyn Fn() + Send + Sync>,
        opening: local_open::Opening,
    ) -> Self {
        let settings = Settings { transfers, filters };
        let shared = Arc::new(Mutex::new(Shared {
            mutation: None,
            view: SftpView {
                connected: false,
                status: "SFTP companion ready; connect with fresh credentials".into(),
                generation: 0,
                identity: ConnectionIdentity::Local,
                prompt: None,
                home: String::new(),
                path: String::new(),
                loading: false,
                has_more: false,
                truncated: false,
                request_id: 0,
                entries: Vec::new(),
                local_path: String::new(),
                local_loading: false,
                local_entries: Vec::new(),
                local_revision: 0,
                upload_preparing: false,
                icons: Arc::new(HashMap::new()),
                transfers: Vec::new(),
                last_batch: None,
                last_download_batch: None,
                edits: Vec::new(),
                edit_auto_upload: true,
            },
        }));
        let (commands, receiver) = mpsc::channel(COMMAND_CAPACITY);
        let (stop, cancelled) = watch::channel(false);
        let (settings_sender, updates) = watch::channel(settings.clone());
        let client = SftpClient {
            commands,
            generation: None,
            shared: shared.clone(),
        };
        let runtime_handle = runtime.clone();
        let task = runtime.spawn(async move {
            let mut worker = Worker {
                runtime: runtime_handle,
                commands: receiver,
                stop: cancelled,
                settings,
                updates,
                transport: None,
                events: None,
                profile: None,
                probe: None,
                probe_task: None,
                pending_secret: None,
                prompt: None,
                generation: 0,
                identity: ConnectionIdentity::Local,
                connected: false,
                status: "SFTP companion ready; connect with fresh credentials".into(),
                home: String::new(),
                path: String::new(),
                loading: false,
                has_more: false,
                truncated: false,
                request_id: 0,
                entries: Vec::new(),
                local_path: String::new(),
                local_loading: false,
                local_entries: Vec::new(),
                local_revision: 0,
                preparation: transfers::Preparation::default(),
                local_files: HashMap::new(),
                local_open: opening,
                icon_task: None,
                icons_dirty: false,
                transfers: BTreeMap::new(),
                last_batch: None,
                last_download_batch: None,
                mutation: None,
                editing: editing::Editing::default(),
                shared,
                wake,
            };
            let result = worker.run().await;
            let cleanup = worker.stop_transport().await;
            let preparation_cleanup = worker.preparation.finish().await;
            let local_open_cleanup = worker.local_open.finish().await;
            let editing_cleanup = worker.editing.finish().await;
            let icon_cleanup = if let Some(task) = worker.icon_task.take() {
                task.await
                    .context("SFTP icon worker failed during shutdown")
                    .map(|_| ())
            } else {
                Ok(())
            };
            result
                .and(cleanup)
                .and(preparation_cleanup)
                .and(local_open_cleanup)
                .and(editing_cleanup)
                .and(icon_cleanup)
        });
        Self {
            client,
            stop,
            settings: settings_sender,
            task,
        }
    }

    pub(crate) fn configure(
        &self,
        transfers: SftpTransferSettings,
        filters: SftpTransferFilterSettings,
    ) {
        let next = Settings { transfers, filters };
        self.settings.send_if_modified(|settings| {
            if *settings == next {
                return false;
            }
            *settings = next;
            true
        });
    }

    pub fn client(&self) -> SftpClient {
        self.client.clone()
    }

    pub fn cancel(&self) {
        self.stop.send_replace(true);
    }

    pub async fn shutdown(self) -> Result<()> {
        self.stop.send_replace(true);
        self.task.await.context("SFTP companion task failed")?
    }
}

struct Worker {
    runtime: Handle,
    commands: mpsc::Receiver<QueuedCommand>,
    stop: watch::Receiver<bool>,
    settings: Settings,
    updates: watch::Receiver<Settings>,
    transport: Option<SshSessionHandle>,
    events: Option<mpsc::Receiver<SshSessionEvent>>,
    profile: Option<SessionProfile>,
    probe: Option<tokio::sync::oneshot::Receiver<Result<HostKeyProbe>>>,
    probe_task: Option<JoinHandle<()>>,
    pending_secret: Option<Zeroizing<String>>,
    prompt: Option<TerminalPrompt>,
    generation: u64,
    identity: ConnectionIdentity,
    connected: bool,
    status: String,
    home: String,
    path: String,
    loading: bool,
    has_more: bool,
    truncated: bool,
    request_id: u64,
    entries: Vec<SftpEntryView>,
    local_path: String,
    local_loading: bool,
    local_entries: Vec<LocalEntryView>,
    local_revision: u64,
    local_files: HashMap<String, crate::local_files::LocalDirectoryEntry>,
    local_open: local_open::Opening,
    preparation: transfers::Preparation,
    icon_task: Option<JoinHandle<IconPrewarmReport>>,
    icons_dirty: bool,
    transfers: BTreeMap<Uuid, SftpTransferView>,
    last_batch: Option<SftpTransferBatchResult>,
    last_download_batch: Option<SftpDownloadBatchResult>,
    mutation: Option<mutations::Pending>,
    editing: editing::Editing,
    shared: Arc<Mutex<Shared>>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Worker {
    async fn run(&mut self) -> Result<()> {
        let mut edit_checks = tokio::time::interval(std::time::Duration::from_millis(250));
        edit_checks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                result = self.stop.changed() => {
                    if result.is_err() || *self.stop.borrow() { break; }
                }
                result = self.updates.changed() => {
                    result.context("SFTP settings owner closed")?;
                    self.settings = self.updates.borrow_and_update().clone();
                    self.local_open.cancel();
                    // A preparation scanned using the previous filters must not
                    // publish a batch after the saved policy changed.
                    self.preparation.cancel();
                    self.publish()?;
                }
                command = self.commands.recv() => {
                    let Some(command) = command else { break; };
                    if let Err(error) = self.dispatch_queued(command).await {
                        self.status = format!("{error:#}").chars().take(512).collect();
                        self.publish()?;
                    }
                }
                event = next_event(&mut self.events) => {
                    if let Some(event) = event { self.event(event).await?; }
                    else {
                        self.events = None;
                        self.connected = false;
                        self.reset_mutation()?;
                        self.interrupt_transfers();
                        self.preparation.cancel();
                        self.local_open.cancel();
                        self.editing.reset();
                        self.status = "SFTP disconnected".into();
                        self.publish()?;
                    }
                }
                probe = next_probe(&mut self.probe), if self.probe.is_some() => {
                    self.probe = None;
                    self.handle_probe(probe).await?;
                }
                _ = edit_checks.tick(), if self.connected => { self.check_edits()?; }
                result = editing::next_job(&mut self.editing.task), if self.editing.task.is_some() => {
                    self.editing.task = None;
                    if self.complete_edit_job(result.context("external edit task failed")?)? {
                        self.publish()?;
                    }
                }
                result = local_open::next_job(&mut self.local_open.task), if self.local_open.task.is_some() => {
                    self.local_open.task = None;
                    self.local_open.deadline = None;
                    let result = result.context("local file open task failed")?;
                    if result.generation == self.generation && result.revision == self.local_revision
                        && self.prompt.is_none()
                        && (!result.cancelled || self.status.starts_with("Opening local file")) {
                        self.status = result.result.unwrap_or_else(|error| format!("Cannot open local file: {error:#}")).chars().take(512).collect();
                        self.publish()?;
                    }
                }
                _ = local_open::deadline(self.local_open.deadline), if self.local_open.deadline.is_some() => {
                    self.local_open.cancel();
                    self.local_open.deadline = None;
                    self.status = "Local file opening timed out; waiting for cleanup".into();
                    self.publish()?;
                }
                result = transfers::next_preparation(&mut self.preparation.task), if self.preparation.task.is_some() => {
                    self.preparation.task = None;
                    if let Err(error) = self.complete_preparation(result.context("upload preparation task failed")?) {
                        self.status = format!("{error:#}").chars().take(1024).collect();
                    }
                    self.publish()?;
                }
                result = next_icons(&mut self.icon_task), if self.icon_task.is_some() => {
                    self.icon_task = None;
                    result.context("SFTP icon worker failed")?;
                    self.schedule_icons();
                    self.publish()?;
                }
            }
        }
        Ok(())
    }

    async fn dispatch_queued(&mut self, command: QueuedCommand) -> Result<()> {
        if command.generation == self.generation
            || matches!(command.command, Command::Connect(..) | Command::Disconnect)
        {
            self.dispatch(command.command).await?;
        } else if let Command::Mutate { id, .. } = command.command {
            self.finish_mutation(id, Err("SFTP connection changed".into()))?;
            self.publish()?;
        }
        Ok(())
    }

    async fn dispatch(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Mutate { id, target, action } => {
                self.mutate_entry(id, target, action, self.generation)?
            }
            Command::Connect(profile, secret) => {
                self.replace().await?;
                self.profile = Some(*profile);
                self.start_remote(secret)?;
            }
            Command::Respond(id, secret) => {
                let Some(prompt) = self.prompt.as_ref().filter(|prompt| prompt.id == id) else {
                    return Ok(());
                };
                if matches!(
                    &prompt.kind,
                    TerminalPromptKind::HostKey {
                        confirmable: false,
                        ..
                    }
                ) {
                    return Ok(());
                }
                if let TerminalPromptKind::HostKey { actual, .. } = &prompt.kind {
                    self.profile
                        .as_mut()
                        .and_then(SessionProfile::ssh_mut)
                        .context("missing SFTP SSH prompt target")?
                        .host_key_fingerprint = Some(actual.clone());
                }
                self.prompt = None;
                self.start_remote(secret)?;
            }
            Command::List(path) => {
                self.preparation.cancel();
                if !self.connected {
                    bail!("SFTP is not connected");
                }
                self.next_request();
                self.path = path.clone();
                self.loading = true;
                self.entries.clear();
                self.transport
                    .as_ref()
                    .context("SFTP worker is unavailable")?
                    .request_list_sftp(self.request_id, path)?;
            }
            Command::LoadMore => {
                if !self.connected || !self.has_more {
                    return Ok(());
                }
                self.loading = true;
                self.transport
                    .as_ref()
                    .context("SFTP worker is unavailable")?
                    .request_load_more_sftp(self.request_id)?;
            }
            Command::Download {
                path,
                local_directory,
            } => {
                self.queue_download(path, local_directory)?;
            }
            Command::Cancel(id) => self.control_transfer(id, transfer_state::Control::Cancel)?,
            Command::ControlBatch { ids, control } => self.control_batch(ids, control)?,
            Command::Pause(id) => self.control_transfer(id, transfer_state::Control::Pause)?,
            Command::Resume(id) => self.control_transfer(id, transfer_state::Control::Resume)?,
            Command::Upload {
                local_path,
                remote_path,
            } => {
                self.prepare_upload(
                    vec![local_path],
                    self.path.clone(),
                    self.request_id,
                    None,
                    Some(remote_path),
                )?;
            }
            Command::UploadSelected {
                paths,
                local_revision,
                remote_directory,
                request_id,
            } => {
                self.prepare_selected_upload(paths, local_revision, remote_directory, request_id)?
            }
            Command::UploadRoots {
                paths,
                remote_directory,
                request_id,
                local_source,
            } => {
                self.prepare_upload(paths, remote_directory, request_id, local_source, None)?;
            }
            Command::DownloadSelected {
                paths,
                request_id,
                local_directory,
                local_revision,
            } => self.download_selected(paths, request_id, local_directory, local_revision)?,
            Command::DownloadDrop {
                path,
                request_id,
                local_directory,
                local_revision,
            } => {
                self.validate_download_drop(&path, request_id, &local_directory, local_revision)?;
                self.queue_download(path, local_directory.into())?;
            }
            Command::CancelPreparation => {
                self.preparation.cancel();
                self.status = "Upload preparation cancelled".into();
            }
            Command::ResolveConflict {
                transfer_id,
                choice,
                apply_to_batch,
            } => {
                if let Some(worker) = &self.transport {
                    worker.request_resolve_sftp_upload_conflict(
                        transfer_id,
                        choice,
                        apply_to_batch,
                    )?;
                }
            }
            Command::ListLocal(path) => {
                self.preparation.cancel();
                self.last_download_batch = None;
                validate_local_directory_input(&path)?;
                if self.local_open.task.is_some() && self.status.starts_with("Opening local file") {
                    self.status = "Local file opening cancelled by directory navigation".into();
                }
                self.local_open.cancel();
                self.local_files.clear();
                self.local_revision = self
                    .local_revision
                    .checked_add(1)
                    .context("local directory revision exhausted")?;
                self.local_loading = true;
                self.publish()?;
                let listing = tokio::task::spawn_blocking(move || read_local_directory(path))
                    .await
                    .context("local directory listing task failed")
                    .and_then(|result| result);
                self.local_loading = false;
                let listing = listing?;
                self.local_path = listing.path;
                self.local_entries = listing.entries;
                self.local_files = listing.files;
                self.local_loading = false;
                self.icons_dirty = true;
                self.schedule_icons();
            }
            Command::OpenLocal { path, revision } => self.open_local(path, revision)?,
            Command::Edit(path) => {
                self.begin_edit(path)?;
            }
            Command::UploadEdit(id) => {
                self.editing.edits.request_upload_for(id)?;
                self.check_edits()?;
            }
            Command::ReopenEdit(id) => {
                self.reopen_edit(id)?;
            }
            Command::StopEdit(id) => {
                self.editing.stop(id);
            }
            Command::AutoUploadEdits(enabled) => {
                self.editing.edits.auto_upload = enabled;
            }
            Command::Disconnect => {
                self.replace().await?;
                self.status = "Disconnected".into();
            }
        }
        self.publish()
    }

    async fn replace(&mut self) -> Result<()> {
        self.stop_transport().await?;
        self.probe = None;
        self.generation = self
            .generation
            .checked_add(1)
            .context("SFTP generation exhausted")?;
        self.connected = false;
        self.prompt = None;
        self.profile = None;
        self.identity = ConnectionIdentity::Local;
        self.home.clear();
        self.path.clear();
        self.entries.clear();
        self.loading = false;
        self.has_more = false;
        self.truncated = false;
        self.request_id = 0;
        self.transfers.clear();
        self.last_batch = None;
        self.last_download_batch = None;
        Ok(())
    }

    fn start_remote(&mut self, secret: Zeroizing<String>) -> Result<()> {
        let profile = self.profile.as_ref().context("missing SFTP profile")?;
        let ssh = profile.ssh().context("SFTP requires SSH")?;
        self.identity = ConnectionIdentity::Sftp {
            profile_id: profile.id,
            attempt_id: Some(Uuid::new_v4()),
        };
        self.status = format!("Checking SSH host key for {}…", ssh.host);
        self.pending_secret = Some(secret);
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let probe_profile = profile.clone();
        self.probe_task = Some(self.runtime.spawn(async move {
            let _ = sender.send(crate::ssh::probe_host_key(&probe_profile).await);
        }));
        self.probe = Some(receiver);
        self.events = None;
        Ok(())
    }

    async fn handle_probe(&mut self, result: Option<Result<HostKeyProbe>>) -> Result<()> {
        if let Some(task) = self.probe_task.take() {
            task.await.context("SFTP host-key probe task failed")?;
        }
        let result = result.context("SFTP host-key probe stopped")??;
        let profile = self.profile.as_mut().context("missing SFTP profile")?;
        let expected = profile
            .ssh()
            .and_then(|ssh| ssh.host_key_fingerprint.clone());
        let blocked = matches!(
            result.decision,
            TrustDecision::Changed | TrustDecision::Revoked
        );
        let trusted = !blocked
            && (expected.as_deref() == Some(result.fingerprint.as_str())
                || (expected.is_none() && result.decision == TrustDecision::Trusted));
        if !trusted {
            self.prompt = Some(TerminalPrompt {
                id: Uuid::new_v4(),
                host: profile
                    .ssh()
                    .map(|ssh| ssh.host.clone())
                    .unwrap_or_default(),
                port: profile.ssh().map(|ssh| ssh.port).unwrap_or_default(),
                username: profile
                    .ssh()
                    .map(|ssh| ssh.username.clone())
                    .unwrap_or_default(),
                kind: TerminalPromptKind::HostKey {
                    expected,
                    actual: result.fingerprint,
                    confirmable: !blocked,
                },
            });
            self.pending_secret = None;
            self.status = "SFTP host-key confirmation required".into();
            self.probe = None;
            return self.publish();
        }
        if let Some(ssh) = profile.ssh_mut() {
            ssh.host_key_fingerprint = Some(result.fingerprint);
        }
        let secret = self
            .pending_secret
            .take()
            .context("missing SFTP authentication input")?;
        self.probe = None;
        self.start_authenticated(secret).await?;
        self.publish()
    }

    async fn start_authenticated(&mut self, secret: Zeroizing<String>) -> Result<()> {
        let profile = self.profile.as_ref().context("missing SFTP profile")?;
        let (worker, events) = SshSessionHandle::spawn_sftp(
            &self.runtime,
            Uuid::new_v4(),
            profile.clone(),
            secret,
            self.settings
                .transfers
                .policy(profile.ssh().and_then(|ssh| ssh.sftp_transfer_policy)),
        );
        self.transport = Some(worker);
        self.events = Some(events);
        self.status = "Connecting to SFTP…".into();
        Ok(())
    }

    async fn event(&mut self, event: SshSessionEvent) -> Result<()> {
        match event {
            SshSessionEvent::Sftp(SftpBrowserEvent::Opened { home }) => {
                self.home = home.clone();
                self.path = home;
                self.loading = true;
            }
            SshSessionEvent::Sftp(SftpBrowserEvent::DirectoryPage {
                request_id,
                path,
                entries,
                append,
                has_more,
                truncated,
            }) => {
                if !accepts_request(self.request_id, request_id) {
                    return Ok(());
                }
                self.path = path;
                self.loading = false;
                self.has_more = has_more;
                self.truncated = truncated;
                if append {
                    self.entries.extend(entries.into_iter().map(entry_view));
                } else {
                    self.entries = entries.into_iter().map(entry_view).collect();
                }
                self.entries.truncate(MAX_ENTRIES);
                self.icons_dirty = true;
                self.schedule_icons();
                self.status = format!("{} items", self.entries.len());
                self.connected = true;
            }
            SshSessionEvent::Sftp(SftpBrowserEvent::Failed {
                request_id,
                message,
            }) => {
                if request_id.is_none_or(|id| id == self.request_id) {
                    self.loading = false;
                    self.status = message;
                }
            }
            SshSessionEvent::Sftp(SftpBrowserEvent::Closed) | SshSessionEvent::Disconnected => {
                self.connected = false;
                self.interrupt_transfers();
                self.preparation.cancel();
                self.local_open.cancel();
                self.editing.reset();
                self.status = "SFTP disconnected".into();
            }
            SshSessionEvent::Connected => {
                self.connected = true;
                self.status = "SFTP connected".into();
            }
            SshSessionEvent::SftpTransfer(event) => self.transfer(event),
            SshSessionEvent::SftpWrite(event) => {
                if !self.mutation_event(&event)? {
                    self.edit_event(event)?;
                }
            }
            SshSessionEvent::HostKeyRejected {
                expected, actual, ..
            } => {
                self.connected = false;
                let confirmable = expected.is_none();
                self.prompt = Some(TerminalPrompt {
                    id: Uuid::new_v4(),
                    host: self
                        .profile
                        .as_ref()
                        .and_then(|profile| profile.ssh())
                        .map(|ssh| ssh.host.clone())
                        .unwrap_or_default(),
                    port: self
                        .profile
                        .as_ref()
                        .and_then(|profile| profile.ssh())
                        .map(|ssh| ssh.port)
                        .unwrap_or_default(),
                    username: self
                        .profile
                        .as_ref()
                        .and_then(|profile| profile.ssh())
                        .map(|ssh| ssh.username.clone())
                        .unwrap_or_default(),
                    kind: TerminalPromptKind::HostKey {
                        expected,
                        actual,
                        confirmable,
                    },
                });
                self.status = if confirmable {
                    "SFTP host key confirmation required"
                } else {
                    "SFTP host key rejected by trust policy"
                }
                .into();
                self.stop_transport().await?;
            }
            SshSessionEvent::HostKeyRevoked { actual, .. } => {
                self.connected = false;
                self.prompt = Some(TerminalPrompt {
                    id: Uuid::new_v4(),
                    host: self
                        .profile
                        .as_ref()
                        .and_then(|profile| profile.ssh())
                        .map(|ssh| ssh.host.clone())
                        .unwrap_or_default(),
                    port: self
                        .profile
                        .as_ref()
                        .and_then(|profile| profile.ssh())
                        .map(|ssh| ssh.port)
                        .unwrap_or_default(),
                    username: self
                        .profile
                        .as_ref()
                        .and_then(|profile| profile.ssh())
                        .map(|ssh| ssh.username.clone())
                        .unwrap_or_default(),
                    kind: TerminalPromptKind::HostKey {
                        expected: None,
                        actual,
                        confirmable: false,
                    },
                });
                self.status = "SFTP host key is revoked".into();
                self.stop_transport().await?;
            }
            SshSessionEvent::AuthenticationFailed | SshSessionEvent::PrivateKeyFailed(_) => {
                self.connected = false;
                self.prompt = Some(self.auth_prompt());
                self.status = "SFTP authentication failed; enter fresh credentials".into();
                self.stop_transport().await?;
            }
            SshSessionEvent::Failed(message) => {
                self.connected = false;
                self.status = message;
                self.stop_transport().await?;
            }
            _ => {}
        }
        if !self.connected {
            self.reset_mutation()?;
            self.interrupt_transfers();
        }
        self.publish()
    }

    fn auth_prompt(&self) -> TerminalPrompt {
        let ssh = self.profile.as_ref().and_then(|profile| profile.ssh());
        TerminalPrompt {
            id: Uuid::new_v4(),
            host: ssh.map(|ssh| ssh.host.clone()).unwrap_or_default(),
            port: ssh.map(|ssh| ssh.port).unwrap_or_default(),
            username: ssh.map(|ssh| ssh.username.clone()).unwrap_or_default(),
            kind: TerminalPromptKind::Authentication,
        }
    }

    fn next_request(&mut self) {
        self.last_download_batch = None;
        self.request_id = self.request_id.wrapping_add(1).max(1);
    }

    async fn stop_transport(&mut self) -> Result<()> {
        self.reset_mutation()?;
        self.local_open.cancel();
        self.probe = None;
        self.pending_secret = None;
        if let Some(task) = self.probe_task.take() {
            task.abort();
            match task.await {
                Ok(()) => {}
                Err(error) if error.is_cancelled() => {}
                Err(error) => return Err(error).context("SFTP host-key probe task failed"),
            }
        }
        self.preparation.cancel();
        self.editing.reset();
        self.events = None;
        if let Some(worker) = self.transport.take() {
            worker.shutdown().await?;
        }
        Ok(())
    }

    fn icon_keys(&self) -> Vec<SftpIcon> {
        let mut seen = HashSet::new();
        [SftpIcon::Folder, SftpIcon::Symlink, SftpIcon::GenericFile]
            .into_iter()
            .chain(
                self.entries
                    .iter()
                    .map(|entry| entry.icon.clone())
                    .chain(self.local_entries.iter().map(|entry| entry.icon.clone())),
            )
            .filter(|key| seen.insert(key.clone()))
            .take(64)
            .collect()
    }

    fn schedule_icons(&mut self) {
        if self.icons_dirty && self.icon_task.is_none() {
            self.icons_dirty = false;
            self.icon_task = Some(prewarm_async(&self.runtime, self.icon_keys()));
        }
    }

    fn publish(&self) -> Result<()> {
        let provider = global_provider();
        let icons = Arc::new(
            self.icon_keys()
                .into_iter()
                .map(|key| {
                    let image = provider.cached_icon(&key);
                    (key, image)
                })
                .collect(),
        );
        self.shared
            .lock()
            .map_err(|_| anyhow::anyhow!("SFTP view lock poisoned"))?
            .view = SftpView {
            connected: self.connected,
            status: self.status.clone(),
            generation: self.generation,
            identity: self.identity,
            prompt: self.prompt.clone(),
            home: self.home.clone(),
            path: self.path.clone(),
            loading: self.loading,
            has_more: self.has_more,
            truncated: self.truncated,
            request_id: self.request_id,
            entries: self.entries.clone(),
            local_path: self.local_path.clone(),
            local_loading: self.local_loading,
            local_entries: self.local_entries.clone(),
            local_revision: self.local_revision,
            upload_preparing: self.preparation.active(),
            icons,
            transfers: self.transfers.values().cloned().collect(),
            last_batch: self.last_batch.clone(),
            last_download_batch: self.last_download_batch.clone(),
            edits: self.editing.edits.snapshots(),
            edit_auto_upload: self.editing.edits.auto_upload,
        };
        (self.wake)();
        Ok(())
    }
}

fn accepts_request(current: u64, incoming: Option<u64>) -> bool {
    incoming.is_none_or(|incoming| incoming == current)
}

fn entry_view(entry: SftpEntry) -> SftpEntryView {
    SftpEntryView {
        icon: icon_for_name(&entry.name, entry.is_dir, entry.is_symlink),
        name: entry.name,
        path: entry.path,
        is_dir: entry.is_dir,
        is_symlink: entry.is_symlink,
        size: entry.size,
        modified: entry.modified,
    }
}

fn icon_for_name(name: &str, is_dir: bool, is_symlink: bool) -> SftpIcon {
    SftpIcon::for_entry(name, is_dir, is_symlink)
}

fn validate_local_directory_input(path: &Path) -> Result<()> {
    let text = path
        .to_str()
        .context("local directory path must be UTF-8")?;
    anyhow::ensure!(
        !text.is_empty()
            && text.len() <= MAX_LOCAL_PATH_BYTES
            && !text.chars().any(char::is_control),
        "invalid local directory path"
    );
    Ok(())
}

struct LocalListing {
    path: String,
    entries: Vec<LocalEntryView>,
    files: HashMap<String, crate::local_files::LocalDirectoryEntry>,
}

fn read_local_directory(path: PathBuf) -> Result<LocalListing> {
    validate_local_directory_input(&path)?;
    let listing = crate::local_files::read_directory(&path, MAX_LOCAL_ENTRIES, true)?;
    let mut files = HashMap::new();
    let entries = listing
        .entries
        .into_iter()
        .filter(|entry| !entry.is_symlink && (entry.is_dir || entry.is_regular_file()))
        .map(|entry| {
            let view = LocalEntryView {
                name: entry.name.clone(),
                path: entry.path.clone(),
                is_dir: entry.is_dir,
                size: entry.size,
                icon: icon_for_name(&entry.name, entry.is_dir, false),
            };
            files.insert(entry.path.clone(), entry);
            view
        })
        .collect();
    Ok(LocalListing {
        path: listing.path,
        entries,
        files,
    })
}

async fn next_icons(
    task: &mut Option<JoinHandle<IconPrewarmReport>>,
) -> std::result::Result<IconPrewarmReport, tokio::task::JoinError> {
    match task {
        Some(task) => task.await,
        None => std::future::pending().await,
    }
}

async fn next_event(
    events: &mut Option<mpsc::Receiver<SshSessionEvent>>,
) -> Option<SshSessionEvent> {
    match events {
        Some(events) => events.recv().await,
        None => std::future::pending().await,
    }
}

async fn next_probe(
    probe: &mut Option<tokio::sync::oneshot::Receiver<Result<HostKeyProbe>>>,
) -> Option<Result<HostKeyProbe>> {
    match probe {
        Some(receiver) => receiver.await.ok(),
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_directory_pages_are_ignored() {
        assert!(!accepts_request(4, Some(3)));
        assert!(accepts_request(4, Some(4)));
        assert!(accepts_request(4, None));
    }

    #[test]
    fn companion_view_contains_no_secret_material() {
        let view = SftpView {
            connected: false,
            status: "ready".into(),
            generation: 0,
            identity: ConnectionIdentity::Local,
            prompt: None,
            home: String::new(),
            path: String::new(),
            loading: false,
            has_more: false,
            truncated: false,
            request_id: 0,
            entries: Vec::new(),
            local_path: String::new(),
            local_loading: false,
            local_entries: Vec::new(),
            local_revision: 0,
            upload_preparing: false,
            icons: Arc::new(HashMap::new()),
            transfers: Vec::new(),
            last_batch: None,
            last_download_batch: None,
            edits: Vec::new(),
            edit_auto_upload: true,
        };
        assert!(!format!("{view:?}").contains("password"));
    }

    #[test]
    fn local_listing_is_bounded_and_skips_symlinks() {
        let root = std::env::temp_dir().join(format!("axssh-sftp-local-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("nested")).expect("fixture directory should exist");
        std::fs::write(root.join("file.txt"), b"fixture").expect("fixture file should exist");
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("file.txt"), root.join("link.txt"))
            .expect("fixture symlink should exist");
        let listing = read_local_directory(root.clone()).expect("listing should succeed");
        assert!(listing.entries.iter().any(|entry| entry.name == "file.txt"));
        assert!(listing.entries.iter().any(|entry| entry.name == "nested"));
        assert!(listing.entries.iter().all(|entry| entry.name != "link.txt"));
        assert!(listing.entries.len() <= MAX_LOCAL_ENTRIES);
        std::fs::remove_dir_all(root).expect("fixture directory should be removed");
    }

    #[test]
    fn local_directory_input_rejects_controls_and_oversized_paths() {
        assert!(validate_local_directory_input(Path::new("bad\npath")).is_err());
        assert!(
            validate_local_directory_input(Path::new(&"x".repeat(MAX_LOCAL_PATH_BYTES + 1)))
                .is_err()
        );
    }

    #[test]
    fn remote_icon_keys_keep_link_priority_and_bound_extensions() {
        let entry = |name: &str, is_dir, is_symlink| {
            entry_view(SftpEntry {
                name: name.into(),
                path: format!("/remote/{name}"),
                is_dir,
                is_symlink,
                size: 7,
                modified: Some(5),
            })
        };
        assert_eq!(entry("directory.json", true, false).icon, SftpIcon::Folder);
        assert_eq!(entry("directory.json", true, true).icon, SftpIcon::Symlink);
        assert_eq!(
            entry("FILE.JSON", false, false).icon,
            SftpIcon::Extension("json".into())
        );
        assert_eq!(entry(".hidden", false, false).icon, SftpIcon::GenericFile);
        assert_eq!(
            entry("file.invalid!", false, false).icon,
            SftpIcon::GenericFile
        );
    }

    #[tokio::test]
    async fn icon_or_command_wakeup_keeps_the_inflight_host_key_probe() {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let mut probe = Some(receiver);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(5), next_probe(&mut probe))
                .await
                .is_err()
        );
        assert!(probe.is_some());
        sender
            .send(Ok(HostKeyProbe {
                fingerprint: "fixture-observation".into(),
                decision: TrustDecision::Unknown,
                public_key: None,
            }))
            .unwrap();
        let result = next_probe(&mut probe).await.unwrap().unwrap();
        assert_eq!(result.fingerprint, "fixture-observation");
    }

    #[tokio::test]
    async fn closing_a_companion_cancels_and_joins_a_stalled_probe() {
        use tokio::io::AsyncReadExt as _;
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let mut profile = SessionProfile::new("SFTP probe fixture", "127.0.0.1", "fixture");
        profile.ssh_mut().unwrap().port = listener.local_addr().unwrap().port();
        let session = SftpSession::idle(&Handle::current(), Arc::new(|| {}));
        session
            .client()
            .connect(profile, Zeroizing::new(String::new()))
            .unwrap();
        let (mut stream, _) =
            tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
                .await
                .unwrap()
                .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), session.shutdown())
            .await
            .unwrap()
            .unwrap();
        let mut bytes = [0; 1024];
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                match stream.read(&mut bytes).await {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => break,
                    Err(error) => panic!("fixture peer read failed: {error}"),
                }
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn local_browser_publishes_bounded_icons_and_joins_prewarm_on_close() {
        let root = std::env::temp_dir().join(format!("axssh-sftp-icons-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        for index in 0..70 {
            std::fs::write(root.join(format!("item.e{index}")), b"icon fixture").unwrap();
        }
        let session = SftpSession::idle(&Handle::current(), Arc::new(|| {}));
        let client = session.client();
        client.list_local(root.clone()).unwrap();
        let view = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let view = client.view().unwrap();
                if view.local_entries.len() == 70 {
                    break view;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(view.icons.len(), 64);
        for key in [SftpIcon::Folder, SftpIcon::Symlink, SftpIcon::GenericFile] {
            let icon = &view.icons[&key];
            assert_eq!(
                (icon.width(), icon.height(), icon.rgba().len()),
                (24, 24, 24 * 24 * 4)
            );
        }
        assert!(!view.connected);
        session.shutdown().await.unwrap();
        assert!(client.list_local(root.clone()).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
