//! Bounded command/event worker for one authenticated SSH transport.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Context, Result};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{Duration, MissedTickBehavior, interval, timeout};
use tracing::{debug, info, warn};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::config::{SessionProfile, X11Settings};
use crate::sftp::{SftpBrowserEvent, SftpBrowserHandle, validate_remote_path};

use super::x11::{X11ChannelRequest, X11Dispatcher, X11Forwarding, X11PreparationError};
use super::{SshConnection, SshError, SshEvent, SshShell, X11RequestStatus};

const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(7);
const OUTPUT_FLUSH_INTERVAL: Duration = Duration::from_millis(16);
const MAX_COLUMNS: u32 = 300;
const MAX_ROWS: u32 = 100;
const COMMAND_CAPACITY: usize = 32;
const EVENT_CAPACITY: usize = 32;
const SFTP_EVENT_CAPACITY: usize = 16;
const MAX_X11_RELAYS: usize = 8;
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
    X11ForwardingEnabled,
    X11ForwardingUnavailable(String),
    Disconnected,
    AuthenticationFailed,
    PrivateKeyFailed(String),
    HostKeyRejected {
        expected: Option<String>,
        actual: String,
    },
    Failed(String),
}

enum SshCommand {
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
    Disconnect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalSize {
    columns: u32,
    rows: u32,
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
            session_id,
            profile,
            secret,
            columns,
            rows,
            SshSessionMode::Terminal,
            x11_settings,
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
            session_id,
            profile,
            secret,
            1,
            1,
            SshSessionMode::Sftp,
            X11Settings::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_with_mode(
        runtime: &Handle,
        session_id: Uuid,
        profile: SessionProfile,
        secret: Zeroizing<String>,
        columns: u32,
        rows: u32,
        mode: SshSessionMode,
        x11_settings: X11Settings,
    ) -> (Self, mpsc::Receiver<SshSessionEvent>) {
        let initial_size = TerminalSize {
            columns: columns.clamp(1, MAX_COLUMNS),
            rows: rows.clamp(1, MAX_ROWS),
        };
        let (command_tx, command_rx) = mpsc::channel(COMMAND_CAPACITY);
        let (resize_tx, resize_rx) = watch::channel(initial_size);
        let (event_tx, event_rx) = mpsc::channel(EVENT_CAPACITY);
        let task = runtime.spawn(run_session(
            session_id,
            profile,
            secret,
            mode,
            x11_settings,
            command_rx,
            resize_rx,
            event_tx,
        ));
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

    pub fn request_resize(&self, columns: u32, rows: u32) -> Result<()> {
        validate_terminal_size(columns, rows)?;
        self.resize_tx
            .send(TerminalSize { columns, rows })
            .map_err(|_| anyhow::anyhow!("cannot update terminal size after SSH worker stopped"))
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
    if columns == 0 || rows == 0 {
        anyhow::bail!("terminal dimensions must be greater than zero");
    }
    if columns > MAX_COLUMNS || rows > MAX_ROWS {
        anyhow::bail!("terminal dimensions cannot exceed {MAX_COLUMNS}x{MAX_ROWS}");
    }
    Ok(())
}

async fn run_session(
    session_id: Uuid,
    profile: SessionProfile,
    secret: Zeroizing<String>,
    mode: SshSessionMode,
    x11_settings: X11Settings,
    mut command_rx: mpsc::Receiver<SshCommand>,
    mut resize_rx: watch::Receiver<TerminalSize>,
    event_tx: mpsc::Sender<SshSessionEvent>,
) {
    let x11_requested = x11_requested_for(mode, &profile);
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
                    | Some(SshCommand::CloseSftp) => {
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
    let (connection, mut x11_forwarding, x11_dispatcher, mut x11_requests) = match connection_result
    {
        Ok(startup) => startup,
        Err(error) => {
            if let Some(SshError::HostKeyRejected { expected, actual }) =
                error.downcast_ref::<SshError>()
            {
                send_event(
                    &event_tx,
                    SshSessionEvent::HostKeyRejected {
                        expected: expected.clone(),
                        actual: actual.clone(),
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
        run_sftp_session(connection, session_id, command_rx, event_tx).await;
        return;
    }

    let initial_size = *resize_rx.borrow_and_update();
    let (mut shell, x11_request_status) = match connection
        .open_shell(
            initial_size.columns,
            initial_size.rows,
            x11_forwarding.as_ref(),
        )
        .await
    {
        Ok(shell) => shell,
        Err(error) => {
            warn!(session_id = %session_id, %error, "SSH worker failed to open interactive shell");
            send_event(
                &event_tx,
                SshSessionEvent::Failed(bounded_error_message(&error)),
                session_id,
            )
            .await;
            return;
        }
    };

    match x11_request_status {
        X11RequestStatus::Enabled => {
            if let Some(dispatcher) = &x11_dispatcher {
                dispatcher.enable();
            }
        }
        X11RequestStatus::Rejected => {
            if let Some(dispatcher) = &x11_dispatcher {
                dispatcher.disable();
            }
            x11_requests = None;
            x11_forwarding = None;
        }
        X11RequestStatus::NotRequested => {}
    }

    if !send_event(&event_tx, SshSessionEvent::Connected, session_id).await {
        close_shell(&shell, session_id).await;
        return;
    }
    if x11_request_status == X11RequestStatus::Enabled {
        if !send_event(&event_tx, SshSessionEvent::X11ForwardingEnabled, session_id).await {
            close_shell(&shell, session_id).await;
            return;
        }
    } else if x11_requested {
        let message = "The SSH server rejected X11 forwarding".to_owned();
        if !send_event(
            &event_tx,
            SshSessionEvent::X11ForwardingUnavailable(message),
            session_id,
        )
        .await
        {
            close_shell(&shell, session_id).await;
            return;
        }
    }

    let mut output_flush = interval(OUTPUT_FLUSH_INTERVAL);
    output_flush.set_missed_tick_behavior(MissedTickBehavior::Skip);
    output_flush.tick().await;
    let mut output = Vec::new();
    let mut output_received_at = None;
    let mut awaiting_output_after_input = None;
    let mut sftp: Option<SftpBrowserHandle> = None;
    let mut sftp_events: Option<mpsc::Receiver<SftpBrowserEvent>> = None;
    let mut x11_relays = JoinSet::new();
    let mut x11_unavailable_reported = false;
    let mut failed = false;
    loop {
        tokio::select! {
            command = command_rx.recv() => {
                match command {
                    Some(SshCommand::Send {
                        input_sequence,
                        queued_at,
                        data,
                    }) => {
                        tracing::debug!(
                            target: LATENCY_TARGET,
                            event = "ssh-input",
                            stage = "worker-dequeued",
                            session_id = %session_id,
                            input_sequence,
                            queue_us = elapsed_micros(queued_at),
                            "SSH worker dequeued terminal input"
                        );
                        let send_started_at = Instant::now();
                        if let Err(error) = shell.send(data).await {
                            tracing::debug!(
                                target: LATENCY_TARGET,
                                event = "ssh-input",
                                stage = "russh-call-failed",
                                session_id = %session_id,
                                input_sequence,
                                call_us = elapsed_micros(send_started_at),
                                "russh terminal data call failed"
                            );
                            warn!(session_id = %session_id, %error, "failed to send terminal input");
                            send_event(
                                &event_tx,
                                SshSessionEvent::Failed(bounded_error_message(&error)),
                                session_id,
                            ).await;
                            failed = true;
                            break;
                        }
                        tracing::debug!(
                            target: LATENCY_TARGET,
                            event = "ssh-input",
                            stage = "russh-call-complete",
                            session_id = %session_id,
                            input_sequence,
                            call_us = elapsed_micros(send_started_at),
                            "russh terminal data call completed"
                        );
                        awaiting_output_after_input = Some((input_sequence, Instant::now()));
                        continue;
                    }
                    Some(SshCommand::OpenSftp { path }) => {
                        if sftp.as_ref().is_some_and(SftpBrowserHandle::is_finished) {
                            if let Some(browser) = sftp.take()
                                && let Err(error) = browser.shutdown().await
                            {
                                warn!(session_id = %session_id, %error, "failed to join stopped SFTP browser");
                            }
                            sftp_events = None;
                        }
                        if sftp.is_some() {
                            if let Some(browser) = sftp.as_ref()
                                && let Err(error) = browser.request_list(path)
                            {
                                send_sftp_event(
                                    &event_tx,
                                    SftpBrowserEvent::Failed(bounded_error_message(&error)),
                                    session_id,
                                ).await;
                            }
                        } else {
                            let (browser_event_tx, browser_event_rx) =
                                mpsc::channel(SFTP_EVENT_CAPACITY);
                            match connection.open_sftp_stream().await {
                                Ok(stream) => match SftpBrowserHandle::spawn(
                                    &Handle::current(),
                                    stream,
                                    path,
                                    browser_event_tx,
                                ) {
                                    Ok(browser) => {
                                        sftp = Some(browser);
                                        sftp_events = Some(browser_event_rx);
                                    }
                                    Err(error) => {
                                        send_sftp_event(
                                            &event_tx,
                                            SftpBrowserEvent::Failed(bounded_error_message(&error)),
                                            session_id,
                                        ).await;
                                        send_sftp_event(
                                            &event_tx,
                                            SftpBrowserEvent::Closed,
                                            session_id,
                                        ).await;
                                    }
                                },
                                Err(error) => {
                                    send_sftp_event(
                                        &event_tx,
                                        SftpBrowserEvent::Failed(bounded_error_message(&error)),
                                        session_id,
                                    ).await;
                                    send_sftp_event(
                                        &event_tx,
                                        SftpBrowserEvent::Closed,
                                        session_id,
                                    ).await;
                                }
                            }
                        }
                        continue;
                    }
                    Some(SshCommand::ListSftp { path }) => {
                        match sftp.as_ref() {
                            Some(browser) => {
                                if let Err(error) = browser.request_list(path) {
                                    send_sftp_event(
                                        &event_tx,
                                        SftpBrowserEvent::Failed(bounded_error_message(&error)),
                                        session_id,
                                    ).await;
                                }
                            }
                            None => {
                                send_sftp_event(
                                    &event_tx,
                                    SftpBrowserEvent::Failed("SFTP browser is not open".to_owned()),
                                    session_id,
                                ).await;
                                send_sftp_event(
                                    &event_tx,
                                    SftpBrowserEvent::Closed,
                                    session_id,
                                ).await;
                            }
                        }
                        continue;
                    }
                    Some(SshCommand::LoadMoreSftp) => {
                        match sftp.as_ref() {
                            Some(browser) => {
                                if let Err(error) = browser.request_load_more() {
                                    send_sftp_event(
                                        &event_tx,
                                        SftpBrowserEvent::Failed(bounded_error_message(&error)),
                                        session_id,
                                    ).await;
                                }
                            }
                            None => {
                                send_sftp_event(
                                    &event_tx,
                                    SftpBrowserEvent::Failed("SFTP browser is not open".to_owned()),
                                    session_id,
                                ).await;
                                send_sftp_event(
                                    &event_tx,
                                    SftpBrowserEvent::Closed,
                                    session_id,
                                ).await;
                            }
                        }
                        continue;
                    }
                    Some(SshCommand::CloseSftp) => {
                        if let Some(browser) = sftp.take()
                            && let Err(error) = browser.shutdown().await
                        {
                            warn!(session_id = %session_id, %error, "failed to close SFTP browser");
                        }
                        sftp_events = None;
                        send_sftp_event(&event_tx, SftpBrowserEvent::Closed, session_id).await;
                        continue;
                    }
                    Some(SshCommand::Disconnect) => {
                        info!(session_id = %session_id, "SSH disconnect requested");
                    }
                    None => {
                        info!(session_id = %session_id, "SSH controller dropped; disconnecting worker");
                    }
                }
                break;
            }
            changed = resize_rx.changed() => {
                if changed.is_err() {
                    info!(session_id = %session_id, "terminal resize controller dropped; disconnecting worker");
                    break;
                }
                let size = *resize_rx.borrow_and_update();
                if let Err(error) = shell.resize(size.columns, size.rows).await {
                    warn!(session_id = %session_id, %error, "failed to resize remote terminal");
                    send_event(
                        &event_tx,
                        SshSessionEvent::Failed(bounded_error_message(&error)),
                        session_id,
                    ).await;
                    failed = true;
                    break;
                }
                if !send_event(
                    &event_tx,
                    SshSessionEvent::Resized {
                        columns: size.columns,
                        rows: size.rows,
                    },
                    session_id,
                ).await {
                    break;
                }
            }
            event = shell.next_event() => {
                match event {
                    Ok(Some(SshEvent::Output(data))) => {
                        let received_at = Instant::now();
                        output_received_at.get_or_insert(received_at);
                        let input_observation = awaiting_output_after_input.take();
                        let flush_for_input = input_observation.is_some();
                        if let Some((input_sequence, send_completed_at)) = input_observation {
                            tracing::debug!(
                                target: LATENCY_TARGET,
                                event = "ssh-output",
                                stage = "first-output-after-input",
                                session_id = %session_id,
                                input_sequence,
                                since_send_call_us = elapsed_micros(send_completed_at),
                                association = "temporal-only",
                                "SSH worker received output after terminal input"
                            );
                        }
                        output.extend_from_slice(&data);
                        if (flush_for_input || output.len() >= MAX_OUTPUT_BATCH_BYTES)
                            && !flush_output(
                                &event_tx,
                                &mut output,
                                &mut output_received_at,
                                session_id,
                            ).await
                        {
                            break;
                        }
                    }
                    Ok(Some(SshEvent::Disconnected)) => {
                        info!(session_id = %session_id, "SSH shell closed by remote peer");
                        break;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        warn!(session_id = %session_id, %error, "SSH shell receive failed");
                        send_event(
                            &event_tx,
                            SshSessionEvent::Failed(bounded_error_message(&error)),
                            session_id,
                        ).await;
                        failed = true;
                        break;
                    }
                }
            }
            event = receive_sftp_event(&mut sftp_events), if sftp_events.is_some() => {
                match event {
                    Some(event) => {
                        let closed = matches!(event, SftpBrowserEvent::Closed);
                        if !send_sftp_event(&event_tx, event, session_id).await {
                            break;
                        }
                        if closed {
                            if let Some(browser) = sftp.take()
                                && let Err(error) = browser.shutdown().await
                            {
                                warn!(session_id = %session_id, %error, "failed to join closed SFTP browser");
                            }
                            sftp_events = None;
                        }
                    }
                    None => {
                        if let Some(browser) = sftp.take()
                            && let Err(error) = browser.shutdown().await
                        {
                            warn!(session_id = %session_id, %error, "failed to join SFTP browser after event channel closed");
                        }
                        sftp_events = None;
                    }
                }
            }
            request = receive_x11_request(&mut x11_requests), if x11_requests.is_some() => {
                match request {
                    Some(request) if x11_relays.len() >= MAX_X11_RELAYS => {
                        request.reject(russh::ChannelOpenFailure::ResourceShortage).await;
                    }
                    Some(request) => {
                        let Some(forwarding) = x11_forwarding.clone() else {
                            request
                                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                                .await;
                            continue;
                        };
                        x11_relays.spawn(async move { forwarding.relay(request).await });
                    }
                    None => {
                        x11_requests = None;
                    }
                }
            }
            relay = x11_relays.join_next(), if !x11_relays.is_empty() => {
                match relay {
                    Some(Ok(Ok(()))) | None => {}
                    Some(Ok(Err(error))) => {
                        warn!(session_id = %session_id, %error, "SSH X11 relay stopped with an error");
                        if error.downcast_ref::<X11PreparationError>().is_some()
                            && !x11_unavailable_reported
                        {
                            x11_unavailable_reported = true;
                            if !send_event(
                                &event_tx,
                                SshSessionEvent::X11ForwardingUnavailable(bounded_error_message(&error)),
                                session_id,
                            )
                            .await
                            {
                                break;
                            }
                        }
                    }
                    Some(Err(error)) if error.is_cancelled() => {}
                    Some(Err(error)) => {
                        warn!(session_id = %session_id, %error, "SSH X11 relay task failed");
                    }
                }
            }
            _ = output_flush.tick() => {
                if !flush_output(
                    &event_tx,
                    &mut output,
                    &mut output_received_at,
                    session_id,
                ).await {
                    break;
                }
            }
        }
    }

    flush_output(&event_tx, &mut output, &mut output_received_at, session_id).await;
    if let Some(browser) = sftp
        && let Err(error) = browser.shutdown().await
    {
        warn!(session_id = %session_id, %error, "failed to shut down SFTP browser");
    }
    if let Some(dispatcher) = &x11_dispatcher {
        dispatcher.disable();
    }
    drop(x11_requests.take());
    shutdown_x11_relays(&mut x11_relays, session_id).await;
    close_shell(&shell, session_id).await;
    if let Err(error) = connection.disconnect().await {
        warn!(session_id = %session_id, %error, "SSH transport disconnect failed");
    }
    if !failed {
        send_event(&event_tx, SshSessionEvent::Disconnected, session_id).await;
    }
}

fn x11_requested_for(mode: SshSessionMode, profile: &SessionProfile) -> bool {
    mode == SshSessionMode::Terminal && profile.ssh().is_some_and(|config| config.x11_forwarding)
}

async fn receive_x11_request(
    requests: &mut Option<mpsc::Receiver<X11ChannelRequest>>,
) -> Option<X11ChannelRequest> {
    match requests {
        Some(requests) => requests.recv().await,
        None => None,
    }
}

async fn shutdown_x11_relays(relays: &mut JoinSet<Result<()>>, session_id: Uuid) {
    relays.abort_all();
    if timeout(X11_RELAY_SHUTDOWN_TIMEOUT, async {
        while relays.join_next().await.is_some() {}
    })
    .await
    .is_err()
    {
        warn!(session_id = %session_id, "SSH X11 relay shutdown timed out");
    }
}

async fn run_sftp_session(
    connection: SshConnection,
    session_id: Uuid,
    mut command_rx: mpsc::Receiver<SshCommand>,
    event_tx: mpsc::Sender<SshSessionEvent>,
) {
    let (browser_event_tx, mut browser_events) = mpsc::channel(SFTP_EVENT_CAPACITY);
    let stream = match connection.open_sftp_stream().await {
        Ok(stream) => stream,
        Err(error) => {
            warn!(session_id = %session_id, %error, "SFTP-only worker failed to open subsystem");
            send_event(
                &event_tx,
                SshSessionEvent::Failed(bounded_error_message(&error)),
                session_id,
            )
            .await;
            let _ = connection.disconnect().await;
            return;
        }
    };
    let browser = match SftpBrowserHandle::spawn(
        &Handle::current(),
        stream,
        "~".to_owned(),
        browser_event_tx,
    ) {
        Ok(browser) => browser,
        Err(error) => {
            warn!(session_id = %session_id, %error, "SFTP-only worker failed to start browser");
            send_event(
                &event_tx,
                SshSessionEvent::Failed(bounded_error_message(&error)),
                session_id,
            )
            .await;
            let _ = connection.disconnect().await;
            return;
        }
    };
    if !send_event(&event_tx, SshSessionEvent::Connected, session_id).await {
        let _ = browser.shutdown().await;
        let _ = connection.disconnect().await;
        return;
    }

    let mut failed = false;
    let mut browser_error: Option<String> = None;
    loop {
        tokio::select! {
            command = command_rx.recv() => {
                let result = match command {
                    Some(SshCommand::OpenSftp { path })
                    | Some(SshCommand::ListSftp { path }) => browser.request_list(path),
                    Some(SshCommand::LoadMoreSftp) => browser.request_load_more(),
                    Some(SshCommand::CloseSftp | SshCommand::Disconnect) | None => break,
                    Some(SshCommand::Send { .. }) => {
                        Err(anyhow::anyhow!("SFTP-only session cannot accept terminal input"))
                    }
                };
                if let Err(error) = result {
                    failed = true;
                    send_event(
                        &event_tx,
                        SshSessionEvent::Failed(bounded_error_message(&error)),
                        session_id,
                    ).await;
                    break;
                }
            }
            event = browser_events.recv() => {
                let Some(event) = event else {
                    failed = true;
                    send_event(
                        &event_tx,
                        SshSessionEvent::Failed("SFTP browser event channel closed".to_owned()),
                        session_id,
                    ).await;
                    break;
                };
                let closed = matches!(event, SftpBrowserEvent::Closed);
                if let SftpBrowserEvent::Failed(message) = &event {
                    browser_error = Some(message.clone());
                }
                if !send_sftp_event(&event_tx, event, session_id).await {
                    break;
                }
                if closed {
                    if let Some(message) = browser_error.take() {
                        failed = true;
                        send_event(
                            &event_tx,
                            SshSessionEvent::Failed(message),
                            session_id,
                        )
                        .await;
                    }
                    break;
                }
            }
        }
    }

    if let Err(error) = browser.shutdown().await {
        warn!(session_id = %session_id, %error, "failed to shut down SFTP-only browser");
    }
    if let Err(error) = connection.disconnect().await {
        warn!(session_id = %session_id, %error, "SFTP-only transport disconnect failed");
    }
    if !failed {
        send_event(&event_tx, SshSessionEvent::Disconnected, session_id).await;
    }
}

async fn receive_sftp_event(
    events: &mut Option<mpsc::Receiver<SftpBrowserEvent>>,
) -> Option<SftpBrowserEvent> {
    match events {
        Some(events) => events.recv().await,
        None => None,
    }
}

async fn send_sftp_event(
    event_tx: &mpsc::Sender<SshSessionEvent>,
    event: SftpBrowserEvent,
    session_id: Uuid,
) -> bool {
    send_event(event_tx, SshSessionEvent::Sftp(event), session_id).await
}

async fn close_shell(shell: &SshShell, session_id: Uuid) {
    match timeout(DISCONNECT_TIMEOUT, shell.disconnect()).await {
        Ok(Ok(())) => info!(session_id = %session_id, "SSH shell disconnected"),
        Ok(Err(error)) => warn!(session_id = %session_id, %error, "SSH shell disconnect failed"),
        Err(_) => warn!(session_id = %session_id, "SSH shell disconnect timed out"),
    }
}

async fn flush_output(
    event_tx: &mpsc::Sender<SshSessionEvent>,
    output: &mut Vec<u8>,
    output_received_at: &mut Option<Instant>,
    session_id: Uuid,
) -> bool {
    if output.is_empty() {
        return true;
    }
    let data = std::mem::take(output);
    let received_at = output_received_at.take().unwrap_or_else(Instant::now);
    send_event(
        event_tx,
        SshSessionEvent::Output { data, received_at },
        session_id,
    )
    .await
}

fn elapsed_micros(started_at: Instant) -> u64 {
    u64::try_from(started_at.elapsed().as_micros()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod x11_tests {
    use std::future::pending;

    use super::*;

    #[test]
    fn x11_is_requested_only_for_opted_in_terminal_profiles() {
        let mut ssh = SessionProfile::new("ssh", "host.example", "alice");
        assert!(x11_requested_for(SshSessionMode::Terminal, &ssh));
        ssh.ssh_mut().expect("profile should be SSH").x11_forwarding = false;
        assert!(!x11_requested_for(SshSessionMode::Terminal, &ssh));
        ssh.ssh_mut().expect("profile should be SSH").x11_forwarding = true;
        assert!(!x11_requested_for(SshSessionMode::Sftp, &ssh));

        let telnet = SessionProfile::new_telnet("telnet", "host.example");
        assert!(!x11_requested_for(SshSessionMode::Terminal, &telnet));
    }

    #[tokio::test]
    async fn relay_shutdown_aborts_and_joins_active_tasks() {
        let mut relays = JoinSet::new();
        relays.spawn(async {
            pending::<()>().await;
            Ok::<(), anyhow::Error>(())
        });
        shutdown_x11_relays(&mut relays, Uuid::new_v4()).await;
        assert!(relays.is_empty());
    }
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
        assert!(validate_terminal_size(MAX_COLUMNS + 1, 24).is_err());
        assert!(validate_terminal_size(80, MAX_ROWS + 1).is_err());
    }
}
