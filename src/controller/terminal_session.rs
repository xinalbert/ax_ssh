//! One task owns the active transport, parser, credentials and bounded presentation.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use crate::config::SessionProfile;
use uuid::Uuid;
use zeroize::Zeroizing;

mod actions;
mod reconnect;
mod transport;
use transport::{Event, Events, Transport};

use crate::local_shell::{LocalShellEvent, LocalShellHandle};
use crate::terminal::{
    TerminalKey, TerminalModel, TerminalModifiers, TerminalMouseEvent, TerminalQueryPalette,
    TerminalSelectionRange, TerminalSnapshot, encode_key,
};
use crate::terminal_dimensions::TerminalSize;

use super::{
    ConnectionIdentity, MAX_MOUSE_WHEEL_REPORTS, TerminalOutput, TerminalViewport,
    encode_terminal_mouse_input,
};

const COMMAND_CAPACITY: usize = 64;
const SCROLLBACK_LINES: usize = 10_000;
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const MAX_COMMAND_TEXT_BYTES: usize = 1024 * 1024;

/// Owned visible terminal state; no transport handles or credentials cross this boundary.
#[derive(Clone)]
pub struct TerminalView {
    pub terminal: TerminalSnapshot,
    pub status: String,
    pub connected: bool,
    pub selection_revision: i32,
    pub generation: u64,
    pub identity: ConnectionIdentity,
    pub prompt: Option<TerminalPrompt>,
}

/// Connection-scoped prompt containing public connection metadata only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalPrompt {
    pub id: Uuid,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub kind: TerminalPromptKind,
}

/// A host-key decision or request for a fresh ephemeral authentication secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalPromptKind {
    HostKey {
        expected: Option<String>,
        actual: String,
        confirmable: bool,
    },
    Authentication,
}

/// A bounded terminal-model selection calculation requested by the frontend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionKind {
    Semantic,
    Line,
}

/// Result tagged with the UI request and viewport identities that produced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSelection {
    pub request: u64,
    pub revision: i32,
    pub range: Option<TerminalSelectionRange>,
}

struct Shared {
    view: TerminalView,
    clipboard: Option<String>,
    selection: Option<TerminalSelection>,
}

struct QueuedCommand {
    generation: u64,
    command: Command,
}

enum Command {
    Connect(Box<SessionProfile>, Zeroizing<String>, bool),
    Local(String),
    Disconnect,
    Respond(Uuid, Zeroizing<String>),
    Key(TerminalKey, TerminalModifiers),
    Paste(String),
    Resize(TerminalSize, u32, u32),
    Scroll(i32),
    Copy(TerminalSelectionRange, i32),
    Selection {
        kind: SelectionKind,
        row: usize,
        column: usize,
        request: u64,
        revision: i32,
    },
    Focus(bool),
    Mouse {
        event: TerminalMouseEvent,
        pixel_x: usize,
        pixel_y: usize,
        repeat: u16,
    },
}

/// Cloneable bounded intent interface. Overload is reported to the caller.
#[derive(Clone)]
pub struct TerminalClient {
    commands: mpsc::Sender<QueuedCommand>,
    generation: Option<u64>,
    shared: Arc<Mutex<Shared>>,
}

impl TerminalClient {
    /// Copies the latest complete visible state, including row revisions.
    pub fn view(&self) -> Result<TerminalView> {
        Ok(self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal view lock poisoned"))?
            .view
            .clone())
    }

    /// Binds terminal input to the connection displayed by the caller.
    pub fn for_view(&self, view: &TerminalView) -> Self {
        Self {
            generation: Some(view.generation),
            ..self.clone()
        }
    }

    fn queued(&self, command: Command) -> Result<QueuedCommand> {
        let generation = match self.generation {
            Some(generation) => generation,
            None => {
                self.shared
                    .lock()
                    .map_err(|_| anyhow::anyhow!("terminal view lock poisoned"))?
                    .view
                    .generation
            }
        };
        Ok(QueuedCommand {
            generation,
            command,
        })
    }

    fn send(&self, command: Command) -> Result<()> {
        self.commands
            .try_send(self.queued(command)?)
            .map_err(|_| anyhow::anyhow!("terminal input queue is full or closed"))
    }

    /// Starts an explicitly requested temporary profile; secrets never enter snapshots.
    pub fn connect(&self, profile: SessionProfile, secret: Zeroizing<String>) -> Result<()> {
        self.connect_with_reconnect(profile, secret, false)
    }

    /// Starts a temporary connection with an explicit bounded automatic-retry policy.
    /// Passwords and key passphrases are requested again rather than retained for retries.
    pub fn connect_with_reconnect(
        &self,
        profile: SessionProfile,
        secret: Zeroizing<String>,
        reconnect: bool,
    ) -> Result<()> {
        profile.validate()?;
        anyhow::ensure!(
            secret.len() <= 16 * 1024,
            "authentication secret exceeds the input limit"
        );
        self.send(Command::Connect(Box::new(profile), secret, reconnect))
    }

    /// Replaces the current connection with an explicitly requested local shell.
    pub fn local(&self, shell: String) -> Result<()> {
        anyhow::ensure!(
            !shell.is_empty() && shell.len() <= 4096 && !shell.chars().any(char::is_control),
            "invalid local shell"
        );
        self.send(Command::Local(shell))
    }

    /// Cancels the connection and any outstanding trust/authentication prompt.
    pub fn disconnect(&self) -> Result<()> {
        self.send(Command::Disconnect)
    }

    /// Confirms the exact current prompt with a fresh, short-lived secret.
    pub fn respond(&self, id: Uuid, secret: Zeroizing<String>) -> Result<()> {
        anyhow::ensure!(
            secret.len() <= 16 * 1024,
            "authentication secret exceeds the input limit"
        );
        self.send(Command::Respond(id, secret))
    }

    /// Encodes a key with the live terminal mode in its owning task.
    pub fn key(&self, key: TerminalKey, modifiers: TerminalModifiers) -> Result<()> {
        if let TerminalKey::Text(text) = &key
            && text.len() > MAX_COMMAND_TEXT_BYTES
        {
            bail!("terminal text exceeds the input limit");
        }
        self.send(Command::Key(key, modifiers))
    }

    /// Sends one bounded paste transaction using the existing bracketed-paste encoder.
    pub fn paste(&self, text: String) -> Result<()> {
        if text.len() > MAX_COMMAND_TEXT_BYTES {
            bail!("terminal paste exceeds the input limit");
        }
        self.send(Command::Paste(text))
    }

    /// Updates the model and PTY using the same normalized cell and physical dimensions.
    pub fn resize(&self, size: TerminalSize, cell_width: u32, cell_height: u32) -> Result<()> {
        self.send(Command::Resize(size, cell_width, cell_height))
    }

    /// Scrolls local history without discarding incoming output.
    pub fn scroll(&self, lines: i32) -> Result<()> {
        self.send(Command::Scroll(lines.clamp(-10_000, 10_000)))
    }

    /// Copies a viewport selection only if its identity is still current.
    pub fn copy(&self, selection: TerminalSelectionRange, revision: i32) -> Result<()> {
        self.send(Command::Copy(selection, revision))
    }

    /// Takes one completed copy result; it is never logged or persisted.
    pub fn take_clipboard(&self) -> Result<Option<String>> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal view lock poisoned"))?;
        if self
            .generation
            .is_some_and(|generation| generation != shared.view.generation)
        {
            return Ok(None);
        }
        Ok(shared.clipboard.take())
    }

    /// Requests a model-defined selection for the current visible viewport.
    pub fn select(
        &self,
        kind: SelectionKind,
        row: usize,
        column: usize,
        request: u64,
        revision: i32,
    ) -> Result<()> {
        self.send(Command::Selection {
            kind,
            row,
            column,
            request,
            revision,
        })
    }

    /// Takes a completed selection only when it belongs to the current UI request.
    pub fn take_selection_matching(
        &self,
        request: u64,
        revision: i32,
    ) -> Result<Option<TerminalSelection>> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal selection lock poisoned"))?;
        if self
            .generation
            .is_some_and(|generation| generation != shared.view.generation)
        {
            return Ok(None);
        }
        if shared
            .selection
            .as_ref()
            .is_some_and(|selection| selection.request == request && selection.revision == revision)
        {
            Ok(shared.selection.take())
        } else {
            Ok(None)
        }
    }

    /// Reports native focus only when the terminal protocol requests it.
    pub fn focus(&self, focused: bool) -> Result<()> {
        self.send(Command::Focus(focused))
    }

    /// Queues a pointer report with one-based physical grid coordinates.
    ///
    /// Wheel repetition is one bounded transaction. Motion may be dropped under
    /// backpressure while preserving one command slot for a release or key.
    pub fn mouse(
        &self,
        event: TerminalMouseEvent,
        pixel_x: usize,
        pixel_y: usize,
        repeat: u16,
    ) -> Result<()> {
        let wheel = matches!(
            event.button,
            crate::terminal::TerminalMouseButton::WheelUp
                | crate::terminal::TerminalMouseButton::WheelDown
                | crate::terminal::TerminalMouseButton::WheelLeft
                | crate::terminal::TerminalMouseButton::WheelRight
        );
        if !(1..=MAX_MOUSE_WHEEL_REPORTS).contains(&i32::from(repeat))
            || (repeat > 1
                && (!wheel || event.kind != crate::terminal::TerminalMouseEventKind::Press))
        {
            bail!("invalid terminal mouse repetition");
        }
        let command = Command::Mouse {
            event,
            pixel_x,
            pixel_y,
            repeat,
        };
        if event.kind == crate::terminal::TerminalMouseEventKind::Motion {
            crate::terminal_input::try_queue_tokio_motion(
                &self.commands,
                self.queued(command)?,
                "terminal controller stopped",
            )?;
            Ok(())
        } else {
            self.send(command)
        }
    }
}

/// Owns cancellation and completion for one terminal, independently of the GUI lifetime.
pub struct TerminalSession {
    client: TerminalClient,
    stop: watch::Sender<bool>,
    task: JoinHandle<Result<()>>,
}

impl TerminalSession {
    /// Starts a local shell on the supplied runtime. Wake notifications contain no terminal data.
    pub fn spawn(runtime: &Handle, shell: String, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self::start(runtime, Some(shell), wake)
    }

    /// Creates a disconnected model without starting a shell, network or serial worker.
    pub fn idle(runtime: &Handle, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self::start(runtime, None, wake)
    }

    fn start(runtime: &Handle, shell: Option<String>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let status = if shell.is_some() {
            "Starting local shell…"
        } else {
            "Disconnected; connect with fresh credentials"
        }
        .to_owned();
        let mut model = new_model(TerminalSize::model(80, 24));
        let shared = Arc::new(Mutex::new(Shared {
            view: TerminalView {
                terminal: model.snapshot(),
                status: status.clone(),
                connected: false,
                selection_revision: 0,
                generation: 0,
                identity: ConnectionIdentity::Local,
                prompt: None,
            },
            clipboard: None,
            selection: None,
        }));
        let (commands, receiver) = mpsc::channel(COMMAND_CAPACITY);
        let (stop, cancelled) = watch::channel(false);
        let client = TerminalClient {
            commands,
            generation: None,
            shared: shared.clone(),
        };
        let task = runtime.spawn(async move {
            let (transport, events) = match shell {
                Some(shell) => {
                    let (pty, events) = LocalShellHandle::spawn(shell, 80, 24);
                    (Some(Transport::Local(pty)), Events::Local(events))
                }
                None => (None, Events::Idle),
            };
            let mut worker = Worker {
                model,
                transport,
                events,
                profile: None,
                pending_secret: None,
                prompt: None,
                generation: 0,
                identity: ConnectionIdentity::Local,
                reconnect: super::ReconnectState::new(false),
                retry_deadline: None,
                shared,
                wake,
                status,
                connected: false,
                last_resize: None,
                viewport: TerminalViewport::default(),
            };
            let result = worker.run(receiver, cancelled).await;
            if let Err(error) = &result {
                worker.connected = false;
                worker.status = format!("{error:#}").chars().take(512).collect();
                if let Err(publish_error) = worker.publish() {
                    tracing::warn!(%publish_error, "cannot publish terminal controller failure");
                }
            }
            let cleanup = worker.stop_transport().await;
            result.and(cleanup)
        });
        Self { client, stop, task }
    }

    /// Returns the UI intent/snapshot facade.
    pub fn client(&self) -> TerminalClient {
        self.client.clone()
    }

    pub(crate) fn cancel(&self) {
        self.stop.send_replace(true);
    }

    /// Cancels and joins the worker after the native event loop has returned.
    pub async fn shutdown(self) -> Result<()> {
        self.stop.send_replace(true);
        self.task.await.context("terminal controller task failed")?
    }
}

struct Worker {
    model: TerminalModel,
    transport: Option<Transport>,
    events: Events,
    profile: Option<SessionProfile>,
    pending_secret: Option<Zeroizing<String>>,
    prompt: Option<TerminalPrompt>,
    generation: u64,
    identity: ConnectionIdentity,
    reconnect: super::ReconnectState,
    retry_deadline: Option<(tokio::time::Instant, u64)>,
    shared: Arc<Mutex<Shared>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    status: String,
    connected: bool,
    last_resize: Option<(TerminalSize, u32, u32)>,
    viewport: TerminalViewport,
}

impl Worker {
    async fn run(
        &mut self,
        mut commands: mpsc::Receiver<QueuedCommand>,
        mut stop: watch::Receiver<bool>,
    ) -> Result<()> {
        let mut frames = tokio::time::interval(FRAME_INTERVAL);
        frames.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut dirty = true;
        loop {
            tokio::select! {
                biased;
                result = stop.changed() => {
                    if result.is_err() || *stop.borrow() { break; }
                }
                _ = frames.tick(), if dirty => {
                    self.viewport.prepare(Some(&mut self.model));
                    if self.model.synchronized_output_remaining().is_none() {
                        self.publish()?;
                        dirty = false;
                    }
                }
                command = commands.recv() => {
                    let Some(command) = command else { break; };
                    if let Err(error) = self.dispatch(command).await {
                        self.status = format!("{error:#}").chars().take(512).collect();
                        self.publish()?;
                    }
                    dirty = true;
                }
                _ = reconnect::wait(self.retry_deadline.map(|(deadline, _)| deadline)), if self.retry_deadline.is_some() => {
                    if let Err(error) = self.retry().await {
                        self.failed(format!("{error:#}")).await?;
                    }
                    dirty = true;
                }
                event = self.events.next() => {
                    if let Err(error) = self.event(event).await {
                        self.failed(format!("{error:#}")).await?;
                    }
                    dirty = true;
                }
            }
        }
        Ok(())
    }

    fn publish(&mut self) -> Result<()> {
        self.viewport.prepare(Some(&mut self.model));
        let Some(terminal) = self.viewport.snapshot(Some(&mut self.model)) else {
            return Ok(());
        };
        self.shared
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal view lock poisoned"))?
            .view = TerminalView {
            terminal,
            status: self.status.clone(),
            connected: self.connected,
            selection_revision: self.viewport.selection_revision(),
            generation: self.generation,
            identity: self.identity,
            prompt: self.prompt.clone(),
        };
        (self.wake)();
        Ok(())
    }

    fn command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Resize(size, width, height) => {
                if self.last_resize == Some((size, width, height)) {
                    return Ok(());
                }
                if let Some(transport) = &self.transport {
                    transport.resize(size)?;
                }
                self.last_resize = Some((size, width, height));
                self.viewport.resize(&mut self.model, size);
                for response in
                    self.model
                        .set_window_size(size.columns(), size.rows(), width, height)
                {
                    self.transport()?.send(response)?;
                }
            }
            Command::Scroll(lines) => {
                self.viewport.scroll(&mut self.model, lines);
            }
            Command::Copy(range, revision) if revision == self.viewport.selection_revision() => {
                let text = self.model.selection_text(
                    range.start_row,
                    range.start_column,
                    range.end_row,
                    range.end_column,
                );
                self.shared
                    .lock()
                    .map_err(|_| anyhow::anyhow!("terminal copy lock poisoned"))?
                    .clipboard = Some(text);
            }
            Command::Copy(..) => {}
            Command::Selection {
                kind,
                row,
                column,
                request,
                revision,
            } => {
                let range = (revision == self.viewport.selection_revision())
                    .then(|| match kind {
                        SelectionKind::Semantic => self.model.semantic_selection_range(row, column),
                        SelectionKind::Line => self.model.line_selection_range(row, column),
                    })
                    .flatten();
                self.shared
                    .lock()
                    .map_err(|_| anyhow::anyhow!("terminal selection lock poisoned"))?
                    .selection = Some(TerminalSelection {
                    request,
                    revision,
                    range,
                });
            }
            Command::Key(key, modifiers) if self.connected => {
                if let Some(bytes) = encode_key(&key, modifiers, self.model.application_cursor()) {
                    self.transport()?.send(bytes)?;
                    self.viewport.scroll_to_bottom(&mut self.model);
                }
            }
            Command::Paste(text) if self.connected => {
                let bytes = self
                    .model
                    .encode_paste(&text)
                    .context("paste is empty or exceeds the terminal limit")?;
                self.transport()?.paste(bytes)?;
                self.viewport.scroll_to_bottom(&mut self.model);
            }
            Command::Focus(focused) if self.connected => {
                if let Some(bytes) = self.model.encode_focus_event(focused) {
                    self.transport()?.send(bytes)?;
                }
            }
            Command::Mouse {
                event,
                pixel_x,
                pixel_y,
                repeat,
            } if self.connected => {
                if let Some(bytes) = mouse_bytes(&self.model, event, pixel_x, pixel_y, repeat)? {
                    if event.kind == crate::terminal::TerminalMouseEventKind::Motion {
                        self.transport()?.motion(bytes)?;
                    } else {
                        self.transport()?.send(bytes)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn new_model(size: TerminalSize) -> TerminalModel {
    let mut model = TerminalModel::new(
        size.columns() as usize,
        size.rows() as usize,
        SCROLLBACK_LINES,
    );
    let foreground = crate::terminal::TerminalQueryColor {
        red: 220,
        green: 223,
        blue: 230,
    };
    model.set_query_palette(TerminalQueryPalette {
        foreground,
        background: crate::terminal::TerminalQueryColor {
            red: 24,
            green: 26,
            blue: 32,
        },
        cursor: foreground,
        ..Default::default()
    });
    model
}

fn mouse_bytes(
    model: &TerminalModel,
    event: TerminalMouseEvent,
    pixel_x: usize,
    pixel_y: usize,
    repeat: u16,
) -> Result<Option<Vec<u8>>> {
    let Some(bytes) =
        encode_terminal_mouse_input(model, event, pixel_x, pixel_y, i32::from(repeat), false)
    else {
        return Ok(None);
    };
    if bytes.len() > crate::terminal_input::TERMINAL_INPUT_CHUNK_BYTES {
        bail!("terminal mouse report batch is too large");
    }
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod remote_tests;
