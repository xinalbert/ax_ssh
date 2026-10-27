//! Single owner for experimental tabs, pane lifetimes and serialized persistence.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::{
    PaneDirection, PaneLayout, PaneTree, SftpClient, SftpSession, TerminalClient, TerminalSession,
    WorkerShutdowns,
};
use crate::config::{AppSettings, ConfigStore, SessionProfile, SessionStore, WorkspaceSnapshot};

mod catalog;
mod layout;
mod persistence;

const MAX_TERMINALS: usize = 32;
const COMMAND_CAPACITY: usize = 64;

/// Owned metadata; live terminals are accessed through their bounded intent facade.
#[derive(Clone)]
pub struct WorkspaceView {
    pub ready: bool,
    pub busy: bool,
    pub status: String,
    pub error: Option<String>,
    pub catalog_revision: u64,
    pub settings: Arc<AppSettings>,
    pub profiles: Arc<Vec<SessionProfile>>,
    pub groups: Arc<Vec<String>>,
    pub tabs: Vec<WorkspaceTabView>,
    pub active_tab_id: Option<Uuid>,
}

#[derive(Clone)]
pub struct WorkspaceTabView {
    pub settings: bool,
    pub id: Uuid,
    pub title: String,
    pub layout: PaneLayout,
    pub panes: Vec<WorkspacePaneView>,
}

#[derive(Clone)]
pub struct WorkspacePaneView {
    pub id: Uuid,
    pub title: String,
    pub profile: Option<SessionProfile>,
    pub local: bool,
    pub sftp: bool,
}

/// Layout intents contain identities and bounded values, never transport handles.
pub enum WorkspaceAction {
    OpenSettings,
    NewLocal,
    NewConnection,
    OpenProfile(Uuid),
    Activate(Uuid),
    Cycle(i32),
    Focus(Uuid),
    FocusDirection(PaneDirection),
    Split(PaneDirection),
    ClosePane(Uuid),
    CloseTab(Uuid),
    MoveTab { id: Uuid, index: usize },
    ResizeSplit { tab: Uuid, divider: i32, ratio: f32 },
    SaveNow,
    OpenFile(std::path::PathBuf),
    SaveFile(std::path::PathBuf),
}

/// Serialized profile/group transactions using the config domain's validation.
pub enum CatalogAction {
    Settings(Box<AppSettings>),
    Upsert(Box<SessionProfile>),
    Delete(Uuid),
    AddGroup(String),
    RenameGroup { old: String, new: String },
    RemoveGroup(String),
    Import(String),
    Export(crate::session_transfer::SessionExportTarget),
}

/// Exact completion identity; failed writes never commit the draft to memory.
pub struct CatalogResult {
    pub request: Uuid,
    pub revision: u64,
    pub error: Option<String>,
    /// Sanitized JSON, only present for a successful explicit export request.
    pub exported: Option<String>,
}

enum Command {
    Layout(WorkspaceAction),
    Catalog {
        request: Uuid,
        revision: u64,
        action: CatalogAction,
    },
}

struct Shared {
    view: WorkspaceView,
    clients: HashMap<Uuid, TerminalClient>,
    sftp_clients: HashMap<Uuid, SftpClient>,
    pending_catalog: std::collections::HashSet<Uuid>,
    catalog_results: HashMap<Uuid, CatalogResult>,
}

#[derive(Clone)]
pub struct WorkspaceClient {
    commands: mpsc::Sender<Command>,
    shared: Arc<Mutex<Shared>>,
}

impl WorkspaceClient {
    pub fn view(&self) -> Result<WorkspaceView> {
        Ok(self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("workspace view lock poisoned"))?
            .view
            .clone())
    }

    pub fn terminal(&self, id: Uuid) -> Result<Option<TerminalClient>> {
        Ok(self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("workspace view lock poisoned"))?
            .clients
            .get(&id)
            .cloned())
    }

    pub fn sftp(&self, id: Uuid) -> Result<Option<SftpClient>> {
        Ok(self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("workspace view lock poisoned"))?
            .sftp_clients
            .get(&id)
            .cloned())
    }

    pub fn action(&self, action: WorkspaceAction) -> Result<()> {
        if let WorkspaceAction::OpenFile(path) | WorkspaceAction::SaveFile(path) = &action {
            let path = path.to_str().context("workspace path must be UTF-8")?;
            ensure!(
                !path.is_empty() && path.len() <= 4096 && !path.chars().any(char::is_control),
                "invalid workspace path"
            );
        }
        self.send(Command::Layout(action))
    }

    pub fn catalog(&self, revision: u64, action: CatalogAction) -> Result<Uuid> {
        if let CatalogAction::Import(text) = &action {
            ensure!(
                text.len() <= crate::session_transfer::MAX_SESSION_TRANSFER_BYTES,
                "session import exceeds the 256 KiB limit"
            );
        }
        if let CatalogAction::Export(crate::session_transfer::SessionExportTarget::Group(name)) =
            &action
        {
            ensure!(name.len() <= 1024, "group input exceeds the command limit");
        }
        if let CatalogAction::Settings(settings) = &action {
            ensure!(
                serde_json::to_vec(settings)?.len() <= 64 * 1024,
                "settings exceed the command limit"
            );
            settings.shortcuts.validate()?;
        }
        if let CatalogAction::Upsert(profile) = &action {
            profile.validate()?;
        }
        let valid_group = |group: &str| group.len() <= 1024;
        ensure!(
            match &action {
                CatalogAction::AddGroup(name) | CatalogAction::RemoveGroup(name) =>
                    valid_group(name),
                CatalogAction::RenameGroup { old, new } => valid_group(old) && valid_group(new),
                _ => true,
            },
            "group input exceeds the command limit"
        );
        let request = Uuid::new_v4();
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("workspace view lock poisoned"))?;
        ensure!(shared.view.ready, "workspace is not ready");
        ensure!(
            shared.pending_catalog.len() + shared.catalog_results.len() < COMMAND_CAPACITY,
            "unconsumed session transactions exceed the limit"
        );
        self.send(Command::Catalog {
            request,
            revision,
            action,
        })?;
        shared.pending_catalog.insert(request);
        Ok(request)
    }

    pub fn take_catalog_result(&self, request: Uuid) -> Result<Option<CatalogResult>> {
        Ok(self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("workspace view lock poisoned"))?
            .catalog_results
            .remove(&request))
    }

    fn send(&self, command: Command) -> Result<()> {
        self.commands
            .try_send(command)
            .map_err(|_| anyhow::anyhow!("workspace queue is full or closed"))
    }
}

pub struct Workspace {
    client: WorkspaceClient,
    stop: watch::Sender<bool>,
    task: JoinHandle<Result<()>>,
}

impl Workspace {
    /// Uses a separate application directory; never opens the default frontend's files.
    pub fn experimental_store() -> Result<ConfigStore> {
        let dirs = directories::ProjectDirs::from("com", "axsoft", "ax_ssh_egui")
            .context("cannot determine experimental config directory")?;
        Ok(ConfigStore::new(dirs.config_dir().join("sessions.json")))
    }

    pub fn spawn(runtime: &Handle, store: ConfigStore, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self::start(
            runtime,
            store,
            wake,
            Arc::new(crate::sftp::configure_global_upload_limit),
        )
    }

    // Keep process-global policy injectable without exposing a second public
    // startup API or changing actual limits in concurrent controller tests.
    fn start(
        runtime: &Handle,
        store: ConfigStore,
        wake: Arc<dyn Fn() + Send + Sync>,
        configure_uploads: Arc<dyn Fn(usize) + Send + Sync>,
    ) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            view: WorkspaceView {
                ready: false,
                busy: false,
                status: "Loading workspace…".into(),
                error: None,
                catalog_revision: 0,
                settings: Arc::new(AppSettings::default()),
                profiles: Arc::new(Vec::new()),
                groups: Arc::new(Vec::new()),
                tabs: Vec::new(),
                active_tab_id: None,
            },
            clients: HashMap::new(),
            sftp_clients: HashMap::new(),
            pending_catalog: std::collections::HashSet::new(),
            catalog_results: HashMap::new(),
        }));
        let (commands, receiver) = mpsc::channel(COMMAND_CAPACITY);
        let (stop, cancelled) = watch::channel(false);
        let client = WorkspaceClient {
            commands,
            shared: shared.clone(),
        };
        let runtime_handle = runtime.clone();
        let task = runtime.spawn(async move {
            let mut owner = Owner {
                runtime: runtime_handle,
                store,
                sessions: SessionStore::default(),
                panes: HashMap::new(),
                tabs: Vec::new(),
                active: None,
                settings_tab: None,
                revision: 0,
                shared,
                wake,
                shutdowns: WorkerShutdowns::default(),
                last_checkpoint: None,
                ready: false,
                busy: false,
                status: "Loading workspace…".into(),
                error: None,
                configure_uploads,
            };
            let result = owner.run(receiver, cancelled).await;
            let cleanup = owner.finish().await;
            result.and(cleanup)
        });
        Self { client, stop, task }
    }

    pub fn client(&self) -> WorkspaceClient {
        self.client.clone()
    }

    pub async fn shutdown(self) -> Result<()> {
        self.stop.send_replace(true);
        self.task.await.context("workspace owner task failed")?
    }
}

struct Pane {
    title: String,
    profile_id: Option<Uuid>,
    local: bool,
    terminal: TerminalSession,
    sftp: Option<SftpSession>,
}

struct Owner {
    runtime: Handle,
    store: ConfigStore,
    sessions: SessionStore,
    panes: HashMap<Uuid, Pane>,
    tabs: Vec<PaneTree>,
    active: Option<Uuid>,
    settings_tab: Option<Uuid>,
    revision: u64,
    shared: Arc<Mutex<Shared>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    shutdowns: WorkerShutdowns,
    last_checkpoint: Option<WorkspaceSnapshot>,
    ready: bool,
    busy: bool,
    status: String,
    error: Option<String>,
    configure_uploads: Arc<dyn Fn(usize) + Send + Sync>,
}

impl Owner {
    async fn run(
        &mut self,
        mut commands: mpsc::Receiver<Command>,
        mut stop: watch::Receiver<bool>,
    ) -> Result<()> {
        if let Err(error) = self.load().await {
            self.error = Some(format!("{error:#}").chars().take(1024).collect());
            self.status = "Workspace could not be loaded; files were preserved".into();
            self.publish()?;
        }
        let mut checkpoints = tokio::time::interval(Duration::from_secs(2));
        checkpoints.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                result = stop.changed() => {
                    if result.is_err() || *stop.borrow() {
                        commands.close();
                        while let Some(command) = commands.recv().await { self.dispatch(command).await?; }
                        break;
                    }
                }
                command = commands.recv() => {
                    let Some(command) = command else { break; };
                    self.dispatch(command).await?;
                }
                _ = checkpoints.tick(), if self.ready => {
                    if let Err(error) = self.checkpoint().await { self.record_error(error); self.publish()?; }
                    self.shutdowns.reap();
                }
            }
        }
        Ok(())
    }

    async fn dispatch(&mut self, command: Command) -> Result<()> {
        if !self.ready {
            return Ok(());
        }
        self.error = None;
        let result = match command {
            Command::Layout(WorkspaceAction::SaveNow) => self.checkpoint().await,
            Command::Layout(WorkspaceAction::OpenFile(path)) => {
                self.busy = true;
                self.status = "Opening workspace…".into();
                self.publish()?;
                let result = self.open_file(path).await;
                self.busy = false;
                result
            }
            Command::Layout(WorkspaceAction::SaveFile(path)) => {
                self.busy = true;
                self.publish()?;
                let result = self.save_file(path).await;
                self.busy = false;
                result
            }
            Command::Layout(action) => self.layout(action),
            Command::Catalog {
                request,
                revision,
                action,
            } => {
                let result = self.mutate_catalog(revision, action).await;
                let mut shared = self
                    .shared
                    .lock()
                    .map_err(|_| anyhow::anyhow!("workspace view lock poisoned"))?;
                shared.pending_catalog.remove(&request);
                shared.catalog_results.insert(
                    request,
                    CatalogResult {
                        request,
                        revision: self.revision,
                        error: result
                            .as_ref()
                            .err()
                            .map(|error| format!("{error:#}").chars().take(1024).collect()),
                        exported: result.as_ref().ok().and_then(Clone::clone),
                    },
                );
                result.map(|_| ())
            }
        };
        if let Err(error) = result {
            self.record_error(error);
        }
        self.publish()
    }

    fn record_error(&mut self, error: anyhow::Error) {
        self.error = Some(format!("{error:#}").chars().take(1024).collect());
    }

    fn publish(&self) -> Result<()> {
        let tabs = self
            .tabs
            .iter()
            .map(|tree| {
                let panes: Vec<_> = tree
                    .tab_ids()
                    .into_iter()
                    .filter_map(|id| {
                        self.panes.get(&id).map(|pane| WorkspacePaneView {
                            id,
                            title: pane.title.clone(),
                            local: pane.local,
                            sftp: pane.sftp.is_some(),
                            profile: pane.profile_id.and_then(|profile_id| {
                                self.sessions
                                    .sessions
                                    .iter()
                                    .find(|profile| profile.id == profile_id)
                                    .cloned()
                            }),
                        })
                    })
                    .collect();
                WorkspaceTabView {
                    settings: self.settings_tab == Some(tree.workspace_tab_id()),
                    id: tree.workspace_tab_id(),
                    title: self
                        .panes
                        .get(&tree.workspace_tab_id())
                        .map(|pane| pane.title.clone())
                        .unwrap_or_else(|| "Settings".into()),
                    layout: tree.layout(),
                    panes,
                }
            })
            .collect();
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("workspace view lock poisoned"))?;
        shared.clients = self
            .panes
            .iter()
            .map(|(id, pane)| (*id, pane.terminal.client()))
            .collect();
        shared.sftp_clients = self
            .panes
            .iter()
            .filter_map(|(id, pane)| pane.sftp.as_ref().map(|sftp| (*id, sftp.client())))
            .collect();
        shared.view = WorkspaceView {
            ready: self.ready,
            busy: self.busy,
            status: self.status.clone(),
            error: self.error.clone(),
            catalog_revision: self.revision,
            settings: Arc::new(self.sessions.settings.clone()),
            profiles: Arc::new(self.sessions.sessions.clone()),
            groups: Arc::new(self.sessions.groups.clone()),
            tabs,
            active_tab_id: self.active,
        };
        drop(shared);
        (self.wake)();
        Ok(())
    }

    async fn finish(&mut self) -> Result<()> {
        let persistence = if self.ready {
            self.checkpoint().await
        } else {
            Ok(())
        };
        let ids = self.panes.keys().copied().collect::<Vec<_>>();
        for id in ids {
            self.close_worker(id);
        }
        let cleanup = std::mem::take(&mut self.shutdowns)
            .finish(Duration::from_secs(20))
            .await;
        persistence?;
        ensure!(cleanup, "workspace worker cleanup exceeded the deadline");
        Ok(())
    }
}

#[cfg(test)]
mod tests;
