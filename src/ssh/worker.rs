//! Bounded command/event worker for one authenticated SSH transport.

mod sftp;
mod shell;

use self::sftp::run_sftp_session;
use self::shell::{TerminalSessionTask, run_terminal_session, x11_requested_for};

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Context, Result};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};
use tracing::{debug, info, warn};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::config::{SessionProfile, X11Settings};
use crate::sftp::{
    SftpBrowserEvent, SftpDownloadRoot, SftpTransferEvent, SftpUploadRequest, SftpWriteEvent,
    SftpWriteOperation, validate_remote_path,
};
use crate::terminal_dimensions::{TerminalSize, validate_backend_size};
use crate::terminal_input::try_queue_tokio_motion;

use super::x11::{X11Dispatcher, X11Forwarding};
use super::{SshConnection, SshError};

const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(7);
const OUTPUT_FLUSH_INTERVAL: Duration = Duration::from_millis(16);
const COMMAND_CAPACITY: usize = 32;
const EVENT_CAPACITY: usize = 32;
const SFTP_EVENT_CAPACITY: usize = 16;
const MAX_X11_RELAYS: usize = 8;
const MAX_SFTP_TRANSFERS: usize = 2;
const SFTP_OPEN_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const X11_RELAY_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_INPUT_BYTES: usize = 16 * 1024;
const MAX_OUTPUT_BATCH_BYTES: usize = 16 * 1024;
const LATENCY_TARGET: &str = "ax_ssh::latency";
pub(super) const MAX_ERROR_CHARS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SshSessionMode {
    Terminal,
    Sftp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshSessionEvent {
    Connected,
    Resized {
        columns: u32,
        rows: u32,
    },
    Output {
        data: Vec<u8>,
        received_at: Instant,
    },
    Sftp(SftpBrowserEvent),
    SftpTransfer(SftpTransferEvent),
    SftpWrite(SftpWriteEvent),
    X11ForwardingEnabled,
    X11ForwardingUnavailable(String),
    Disconnected,
    AuthenticationFailed,
    PrivateKeyFailed(String),
    HostKeyRejected {
        expected: Option<String>,
        actual: String,
        public_key: Option<String>,
    },
    HostKeyRevoked {
        actual: String,
        public_key: Option<String>,
    },
    Failed(String),
}

pub(crate) enum SshCommand {
    Send {
        input_sequence: u64,
        queued_at: Instant,
        data: Vec<u8>,
    },
    OpenSftp {
        path: String,
    },
    ListSftp {
        path: String,
    },
    LoadMoreSftp,
    CloseSftp,
    OpenSftpFile {
        root: SftpDownloadRoot,
    },
    OpenSftpUpload {
        request: SftpUploadRequest,
    },
    CancelSftpTransfer {
        transfer_id: Uuid,
    },
    PauseSftpTransfer {
        transfer_id: Uuid,
    },
    ResumeSftpTransfer {
        transfer_id: Uuid,
    },
    SftpWrite {
        operation_id: Uuid,
        operation: SftpWriteOperation,
    },
    Disconnect,
}

struct SshSessionLaunch {
    session_id: Uuid,
    profile: SessionProfile,
    secret: Zeroizing<String>,
    initial_size: TerminalSize,
    mode: SshSessionMode,
    x11_settings: X11Settings,
}

struct SshSessionTask {
    session_id: Uuid,
    profile: SessionProfile,
    secret: Zeroizing<String>,
    mode: SshSessionMode,
    x11_settings: X11Settings,
    command_rx: mpsc::Receiver<SshCommand>,
    resize_rx: watch::Receiver<TerminalSize>,
    event_tx: mpsc::Sender<SshSessionEvent>,
}

/// UI-adjacent controller for one worker-owned SSH connection.
pub struct SshSessionHandle {
    session_id: Uuid,
    command_tx: mpsc::Sender<SshCommand>,
    resize_tx: watch::Sender<TerminalSize>,
    next_input_sequence: AtomicU64,
    task: JoinHandle<()>,
}

impl SshSessionHandle {
    pub fn spawn(
        runtime: &Handle,
        session_id: Uuid,
        profile: SessionProfile,
        secret: Zeroizing<String>,
        columns: u32,
        rows: u32,
    ) -> (Self, mpsc::Receiver<SshSessionEvent>) {
        let x11_settings = X11Settings {
            launch_on_connect: false,
            ..X11Settings::default()
        };
        Self::spawn_with_x11_settings(
            runtime,
            session_id,
            profile,
            secret,
            columns,
            rows,
            x11_settings,
        )
    }

    pub fn spawn_with_x11_settings(
        runtime: &Handle,
        session_id: Uuid,
        profile: SessionProfile,
        secret: Zeroizing<String>,
        columns: u32,
        rows: u32,
        x11_settings: X11Settings,
    ) -> (Self, mpsc::Receiver<SshSessionEvent>) {
        Self::spawn_with_mode(
            runtime,
            SshSessionLaunch {
                session_id,
                profile,
                secret,
                initial_size: TerminalSize::backend(columns, rows),
                mode: SshSessionMode::Terminal,
                x11_settings,
            },
        )
    }

    pub fn spawn_sftp(
        runtime: &Handle,
        session_id: Uuid,
        profile: SessionProfile,
        secret: Zeroizing<String>,
    ) -> (Self, mpsc::Receiver<SshSessionEvent>) {
        Self::spawn_with_mode(
            runtime,
            SshSessionLaunch {
                session_id,
                profile,
                secret,
                initial_size: TerminalSize::backend(1, 1),
                mode: SshSessionMode::Sftp,
                x11_settings: X11Settings::default(),
            },
        )
    }

    fn spawn_with_mode(
        runtime: &Handle,
        launch: SshSessionLaunch,
    ) -> (Self, mpsc::Receiver<SshSessionEvent>) {
        let SshSessionLaunch {
            session_id,
            profile,
            secret,
            initial_size,
            mode,
            x11_settings,
        } = launch;
        let (command_tx, command_rx) = mpsc::channel(COMMAND_CAPACITY);
        let (resize_tx, resize_rx) = watch::channel(initial_size);
        let (event_tx, event_rx) = mpsc::channel(EVENT_CAPACITY);
        let task = runtime.spawn(run_session(SshSessionTask {
            session_id,
            profile,
            secret,
            mode,
            x11_settings,
            command_rx,
            resize_rx,
            event_tx,
        }));
        (
            Self {
                session_id,
                command_tx,
                resize_tx,
                next_input_sequence: AtomicU64::new(1),
                task,
            },
            event_rx,
        )
    }

    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }

    pub fn request_disconnect(&self) -> Result<()> {
        self.command_tx
            .try_send(SshCommand::Disconnect)
            .map_err(|error| anyhow::anyhow!("cannot request SSH disconnect: {error}"))
    }

    pub fn request_send(&self, data: Vec<u8>) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        if data.len() > MAX_INPUT_BYTES {
            anyhow::bail!("terminal input cannot exceed {MAX_INPUT_BYTES} bytes");
        }
        let input_sequence = self.next_input_sequence.fetch_add(1, Ordering::Relaxed);
        let command = SshCommand::Send {
            input_sequence,
            queued_at: Instant::now(),
            data,
        };
        match self.command_tx.try_send(command) {
            Ok(()) => {
                tracing::debug!(
                    target: LATENCY_TARGET,
                    event = "ssh-input",
                    stage = "worker-queued",
                    session_id = %self.session_id,
                    input_sequence,
                    "SSH terminal input queued"
                );
                Ok(())
            }
            Err(error) => {
                tracing::debug!(
                    target: LATENCY_TARGET,
                    event = "ssh-input",
                    stage = "worker-queue-rejected",
                    session_id = %self.session_id,
                    input_sequence,
                    "SSH terminal input queue rejected the command"
                );
                Err(anyhow::anyhow!("cannot queue terminal input: {error}"))
            }
        }
    }

    /// Returns `false` when a pointer-motion frame is dropped under normal backpressure.
    pub fn request_send_motion(&self, data: Vec<u8>) -> Result<bool> {
        if data.is_empty() {
            return Ok(true);
        }
        if data.len() > MAX_INPUT_BYTES {
            anyhow::bail!("terminal input cannot exceed {MAX_INPUT_BYTES} bytes");
        }
        let input_sequence = self.next_input_sequence.fetch_add(1, Ordering::Relaxed);
        let queued = try_queue_tokio_motion(
            &self.command_tx,
            SshCommand::Send {
                input_sequence,
                queued_at: Instant::now(),
                data,
            },
            "cannot queue SSH mouse motion after worker stopped",
        )?;
        tracing::debug!(
            target: LATENCY_TARGET,
            event = "ssh-input",
            stage = if queued { "worker-queued" } else { "worker-queue-dropped" },
            session_id = %self.session_id,
            input_sequence,
            "SSH terminal mouse motion queue result"
        );
        Ok(queued)
    }

    pub fn request_resize(&self, columns: u32, rows: u32) -> Result<()> {
        validate_terminal_size(columns, rows)?;
        if self.task.is_finished() || self.resize_tx.receiver_count() == 0 {
            anyhow::bail!("cannot update terminal size after SSH worker stopped");
        }
        let size = TerminalSize::backend(columns, rows);
        self.resize_tx.send_if_modified(|current| {
            if *current == size {
                false
            } else {
                *current = size;
                true
            }
        });
        Ok(())
    }

    pub fn request_open_sftp(&self, path: String) -> Result<()> {
        validate_remote_path(&path)?;
        self.command_tx
            .try_send(SshCommand::OpenSftp { path })
            .map_err(|error| anyhow::anyhow!("cannot queue SFTP open request: {error}"))
    }

    pub fn request_list_sftp(&self, path: String) -> Result<()> {
        validate_remote_path(&path)?;
        self.command_tx
            .try_send(SshCommand::ListSftp { path })
            .map_err(|error| anyhow::anyhow!("cannot queue SFTP directory request: {error}"))
    }

    pub fn request_load_more_sftp(&self) -> Result<()> {
        self.command_tx
            .try_send(SshCommand::LoadMoreSftp)
            .map_err(|error| anyhow::anyhow!("cannot queue SFTP page request: {error}"))
    }

    pub fn request_close_sftp(&self) -> Result<()> {
        self.command_tx
            .try_send(SshCommand::CloseSftp)
            .map_err(|error| anyhow::anyhow!("cannot queue SFTP close request: {error}"))
    }

    pub fn request_open_sftp_file(
        &self,
        transfer_id: Uuid,
        path: String,
        local_directory: std::path::PathBuf,
    ) -> Result<()> {
        let root = SftpDownloadRoot::new(transfer_id, path, local_directory)?;
        self.command_tx
            .try_send(SshCommand::OpenSftpFile { root })
            .map_err(|error| anyhow::anyhow!("cannot queue SFTP download request: {error}"))
    }

    pub fn request_cancel_sftp_transfer(&self, transfer_id: Uuid) -> Result<()> {
        self.command_tx
            .try_send(SshCommand::CancelSftpTransfer { transfer_id })
            .map_err(|error| anyhow::anyhow!("cannot queue SFTP transfer cancellation: {error}"))
    }

    pub fn request_open_sftp_upload(
        &self,
        transfer_id: Uuid,
        path: String,
        data: Vec<u8>,
    ) -> Result<()> {
        let request = SftpUploadRequest::new(transfer_id, path, data)?;
        self.command_tx
            .try_send(SshCommand::OpenSftpUpload { request })
            .map_err(|error| anyhow::anyhow!("cannot queue SFTP upload request: {error}"))
    }

    pub fn request_pause_sftp_transfer(&self, transfer_id: Uuid) -> Result<()> {
        self.command_tx
            .try_send(SshCommand::PauseSftpTransfer { transfer_id })
            .map_err(|error| anyhow::anyhow!("cannot queue SFTP transfer pause: {error}"))
    }

    pub fn request_resume_sftp_transfer(&self, transfer_id: Uuid) -> Result<()> {
        self.command_tx
            .try_send(SshCommand::ResumeSftpTransfer { transfer_id })
            .map_err(|error| anyhow::anyhow!("cannot queue SFTP transfer resume: {error}"))
    }

    pub fn request_sftp_write(
        &self,
        operation_id: Uuid,
        operation: SftpWriteOperation,
    ) -> Result<()> {
        self.command_tx
            .try_send(SshCommand::SftpWrite {
                operation_id,
                operation,
            })
            .map_err(|error| anyhow::anyhow!("cannot queue SFTP write request: {error}"))
    }

    pub async fn shutdown(mut self) -> Result<()> {
        if !self.task.is_finished() && self.command_tx.send(SshCommand::Disconnect).await.is_err() {
            debug!("SSH worker command receiver already closed during shutdown");
        }

        match timeout(WORKER_SHUTDOWN_TIMEOUT, &mut self.task).await {
            Ok(joined) => joined.context("SSH worker task failed during shutdown"),
            Err(_) => {
                self.task.abort();
                match self.task.await {
                    Err(error) if error.is_cancelled() => {
                        warn!("SSH worker exceeded shutdown timeout and was aborted");
                        Ok(())
                    }
                    Err(error) => Err(error).context("failed to abort SSH worker task"),
                    Ok(()) => Ok(()),
                }
            }
        }
    }
}

fn validate_terminal_size(columns: u32, rows: u32) -> Result<()> {
    validate_backend_size(columns, rows)
}

async fn run_session(task: SshSessionTask) {
    let SshSessionTask {
        session_id,
        profile,
        secret,
        mode,
        x11_settings,
        mut command_rx,
        resize_rx,
        event_tx,
    } = task;
    let x11_requested = x11_requested_for(mode, &profile);
    let initial_sftp_path = initial_sftp_path(&profile);
    let connect = async {
        let mut x11_forwarding = None;
        let mut x11_dispatcher = None;
        let mut x11_requests = None;
        if x11_requested {
            let (dispatcher, requests) = X11Dispatcher::channel();
            x11_forwarding = Some(X11Forwarding::new(x11_settings));
            x11_dispatcher = Some(dispatcher);
            x11_requests = Some(requests);
        }
        let connection =
            SshConnection::connect_with_x11(&profile, secret, x11_dispatcher.clone()).await?;
        Ok::<_, anyhow::Error>((connection, x11_forwarding, x11_dispatcher, x11_requests))
    };
    tokio::pin!(connect);
    let connection_result = loop {
        tokio::select! {
            result = &mut connect => break Some(result),
            command = command_rx.recv() => {
                match command {
                    Some(SshCommand::Disconnect) => {
                        info!(session_id = %session_id, "SSH connection attempt cancelled");
                        break None;
                    }
                    Some(SshCommand::Send { .. })
                    | Some(SshCommand::OpenSftp { .. })
                    | Some(SshCommand::ListSftp { .. })
                    | Some(SshCommand::LoadMoreSftp)
                    | Some(SshCommand::CloseSftp)
                    | Some(SshCommand::OpenSftpFile { .. })
                    | Some(SshCommand::OpenSftpUpload { .. })
                    | Some(SshCommand::CancelSftpTransfer { .. })
                    | Some(SshCommand::PauseSftpTransfer { .. })
                    | Some(SshCommand::ResumeSftpTransfer { .. })
                    | Some(SshCommand::SftpWrite { .. }) => {
                        debug!(session_id = %session_id, "session command ignored before SSH authentication");
                    }
                    None => {
                        info!(session_id = %session_id, "SSH controller dropped during connection attempt");
                        break None;
                    }
                }
            }
        }
    };
    let Some(connection_result) = connection_result else {
        send_event(&event_tx, SshSessionEvent::Disconnected, session_id).await;
        return;
    };
    let (connection, x11_forwarding, x11_dispatcher, x11_requests) = match connection_result {
        Ok(startup) => startup,
        Err(error) => {
            if let Some(SshError::HostKeyRevoked { actual, public_key }) =
                error.downcast_ref::<SshError>()
            {
                send_event(
                    &event_tx,
                    SshSessionEvent::HostKeyRevoked {
                        actual: actual.clone(),
                        public_key: public_key.clone(),
                    },
                    session_id,
                )
                .await;
            } else if let Some(SshError::HostKeyRejected {
                expected,
                actual,
                public_key,
            }) = error.downcast_ref::<SshError>()
            {
                send_event(
                    &event_tx,
                    SshSessionEvent::HostKeyRejected {
                        expected: expected.clone(),
                        actual: actual.clone(),
                        public_key: public_key.clone(),
                    },
                    session_id,
                )
                .await;
            } else if matches!(
                error.downcast_ref::<SshError>(),
                Some(SshError::AuthenticationFailed)
            ) {
                warn!(session_id = %session_id, "SSH worker authentication failed");
                send_event(&event_tx, SshSessionEvent::AuthenticationFailed, session_id).await;
            } else if let Some(SshError::PrivateKeyLoad(message)) = error.downcast_ref::<SshError>()
            {
                warn!(session_id = %session_id, "SSH worker could not load private key");
                send_event(
                    &event_tx,
                    SshSessionEvent::PrivateKeyFailed(bounded_text(message)),
                    session_id,
                )
                .await;
            } else {
                warn!(session_id = %session_id, %error, "SSH worker failed to connect");
                send_event(
                    &event_tx,
                    SshSessionEvent::Failed(bounded_error_message(&error)),
                    session_id,
                )
                .await;
            }
            return;
        }
    };

    if mode == SshSessionMode::Sftp {
        run_sftp_session(
            connection,
            session_id,
            initial_sftp_path,
            command_rx,
            event_tx,
        )
        .await;
        return;
    }

    run_terminal_session(TerminalSessionTask {
        connection,
        session_id,
        x11_requested,
        x11_forwarding,
        x11_dispatcher,
        x11_requests,
        command_rx,
        resize_rx,
        event_tx,
    })
    .await;
}

fn initial_sftp_path(profile: &SessionProfile) -> String {
    profile
        .ssh()
        .map(|ssh| ssh.sftp_remote_path.trim())
        .filter(|path| !path.is_empty())
        .unwrap_or("~")
        .to_owned()
}

async fn send_event(
    event_tx: &mpsc::Sender<SshSessionEvent>,
    event: SshSessionEvent,
    session_id: Uuid,
) -> bool {
    if event_tx.send(event).await.is_err() {
        debug!(session_id = %session_id, "SSH event receiver dropped");
        false
    } else {
        true
    }
}

pub(super) fn bounded_error_message(error: &anyhow::Error) -> String {
    bounded_text(&format!("{error:#}"))
}

fn bounded_text(message: &str) -> String {
    let mut chars = message.chars();
    let mut bounded = chars.by_ref().take(MAX_ERROR_CHARS).collect::<String>();
    if chars.next().is_some() {
        bounded.push_str("...");
    }
    bounded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_size_is_bounded() {
        assert!(validate_terminal_size(80, 24).is_ok());
        assert!(validate_terminal_size(0, 24).is_err());
        assert!(
            validate_terminal_size(
                u32::from(crate::terminal_dimensions::MAX_TERMINAL_COLUMNS) + 1,
                24
            )
            .is_err()
        );
        assert!(
            validate_terminal_size(
                80,
                u32::from(crate::terminal_dimensions::MAX_TERMINAL_ROWS) + 1
            )
            .is_err()
        );
    }

    #[test]
    fn repeated_resize_does_not_notify_ssh_watch_receiver() {
        let initial = TerminalSize::backend(80, 24);
        let (sender, receiver) = watch::channel(initial);
        assert!(!sender.send_if_modified(|current| {
            if *current == initial {
                false
            } else {
                *current = initial;
                true
            }
        }));
        assert!(
            !receiver
                .has_changed()
                .expect("watch receiver should be open")
        );

        let changed = TerminalSize::backend(100, 30);
        assert!(sender.send_if_modified(|current| {
            if *current == changed {
                false
            } else {
                *current = changed;
                true
            }
        }));
        assert!(
            receiver
                .has_changed()
                .expect("watch receiver should be open")
        );
    }

    #[test]
    fn sftp_initial_path_comes_from_the_profile() {
        let mut profile = SessionProfile::new("files", "host.example", "alice");
        profile
            .ssh_mut()
            .expect("profile should use SSH")
            .sftp_remote_path = "  /srv/releases  ".into();
        assert_eq!(initial_sftp_path(&profile), "/srv/releases");

        profile
            .ssh_mut()
            .expect("profile should use SSH")
            .sftp_remote_path = String::new();
        assert_eq!(initial_sftp_path(&profile), "~");
    }
}
