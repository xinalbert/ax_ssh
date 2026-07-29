//! Bounded command/event worker for one authenticated SSH transport.

use anyhow::{Context, Result};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{Duration, MissedTickBehavior, interval, timeout};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::config::SessionProfile;

use super::{SshConnection, SshError, SshEvent, SshShell};

const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(7);
const OUTPUT_FLUSH_INTERVAL: Duration = Duration::from_millis(16);
const MAX_COLUMNS: u32 = 300;
const MAX_ROWS: u32 = 100;
const COMMAND_CAPACITY: usize = 32;
const EVENT_CAPACITY: usize = 32;
const MAX_INPUT_BYTES: usize = 16 * 1024;
const MAX_OUTPUT_BATCH_BYTES: usize = 16 * 1024;
pub(super) const MAX_ERROR_CHARS: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshSessionEvent {
    Connected,
    Resized {
        columns: u32,
        rows: u32,
    },
    Output(Vec<u8>),
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
    Send(Vec<u8>),
    Disconnect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalSize {
    columns: u32,
    rows: u32,
}

/// UI-adjacent controller for one worker-owned SSH connection.
pub struct SshSessionHandle {
    command_tx: mpsc::Sender<SshCommand>,
    resize_tx: watch::Sender<TerminalSize>,
    task: JoinHandle<()>,
}

impl SshSessionHandle {
    pub fn spawn(
        runtime: &Handle,
        session_id: Uuid,
        profile: SessionProfile,
        secret: String,
        columns: u32,
        rows: u32,
    ) -> (Self, mpsc::Receiver<SshSessionEvent>) {
        let initial_size = TerminalSize {
            columns: columns.clamp(1, MAX_COLUMNS),
            rows: rows.clamp(1, MAX_ROWS),
        };
        let (command_tx, command_rx) = mpsc::channel(COMMAND_CAPACITY);
        let (resize_tx, resize_rx) = watch::channel(initial_size);
        let (event_tx, event_rx) = mpsc::channel(EVENT_CAPACITY);
        let task = runtime.spawn(run_session(
            session_id, profile, secret, command_rx, resize_rx, event_tx,
        ));
        (
            Self {
                command_tx,
                resize_tx,
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
        self.command_tx
            .try_send(SshCommand::Send(data))
            .map_err(|error| anyhow::anyhow!("cannot queue terminal input: {error}"))
    }

    pub fn request_resize(&self, columns: u32, rows: u32) -> Result<()> {
        validate_terminal_size(columns, rows)?;
        self.resize_tx
            .send(TerminalSize { columns, rows })
            .map_err(|_| anyhow::anyhow!("cannot update terminal size after SSH worker stopped"))
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
    secret: String,
    mut command_rx: mpsc::Receiver<SshCommand>,
    mut resize_rx: watch::Receiver<TerminalSize>,
    event_tx: mpsc::Sender<SshSessionEvent>,
) {
    let connect = SshConnection::connect(&profile, secret);
    tokio::pin!(connect);
    let connection_result = tokio::select! {
        result = &mut connect => Some(result),
        command = command_rx.recv() => {
            match command {
                Some(SshCommand::Disconnect) => {
                    info!(session_id = %session_id, "SSH connection attempt cancelled");
                }
                Some(SshCommand::Send(_)) => {
                    debug!(session_id = %session_id, "terminal command ignored before SSH authentication");
                }
                None => {
                    info!(session_id = %session_id, "SSH controller dropped during connection attempt");
                }
            }
            None
        }
    };
    let Some(connection_result) = connection_result else {
        send_event(&event_tx, SshSessionEvent::Disconnected, session_id).await;
        return;
    };
    let connection = match connection_result {
        Ok(connection) => connection,
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

    let initial_size = *resize_rx.borrow_and_update();
    let mut shell = match connection
        .open_shell(initial_size.columns, initial_size.rows)
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

    if !send_event(&event_tx, SshSessionEvent::Connected, session_id).await {
        close_shell(&shell, session_id).await;
        return;
    }

    let mut output_flush = interval(OUTPUT_FLUSH_INTERVAL);
    output_flush.set_missed_tick_behavior(MissedTickBehavior::Skip);
    output_flush.tick().await;
    let mut output = Vec::new();
    let mut failed = false;
    loop {
        tokio::select! {
            command = command_rx.recv() => {
                match command {
                    Some(SshCommand::Send(data)) => {
                        if let Err(error) = shell.send(data).await {
                            warn!(session_id = %session_id, %error, "failed to send terminal input");
                            send_event(
                                &event_tx,
                                SshSessionEvent::Failed(bounded_error_message(&error)),
                                session_id,
                            ).await;
                            failed = true;
                            break;
                        }
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
                        output.extend_from_slice(&data);
                        if output.len() >= MAX_OUTPUT_BATCH_BYTES
                            && !flush_output(&event_tx, &mut output, session_id).await
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
            _ = output_flush.tick() => {
                if !flush_output(&event_tx, &mut output, session_id).await {
                    break;
                }
            }
        }
    }

    flush_output(&event_tx, &mut output, session_id).await;
    close_shell(&shell, session_id).await;
    if !failed {
        send_event(&event_tx, SshSessionEvent::Disconnected, session_id).await;
    }
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
    session_id: Uuid,
) -> bool {
    if output.is_empty() {
        return true;
    }
    let data = std::mem::take(output);
    send_event(event_tx, SshSessionEvent::Output(data), session_id).await
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
