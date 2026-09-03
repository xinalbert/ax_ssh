//! Bounded RFC 854 Telnet worker with explicit option negotiation.

use anyhow::{Context, Result};
use libmudtelnet_rs::Parser;
use libmudtelnet_rs::bytes::{Buf, Bytes, BytesMut};
use libmudtelnet_rs::compatibility::CompatibilityTable;
use libmudtelnet_rs::events::TelnetEvents;
use libmudtelnet_rs::telnet::op_command::{DO, DONT, IAC, SB, SE, WILL, WONT};
use libmudtelnet_rs::telnet::op_option::{BINARY, ECHO, NAWS, SGA};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant as TokioInstant, sleep, timeout};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::config::TelnetConfig;
use crate::terminal_dimensions::TerminalSize;
use crate::terminal_input::try_queue_tokio_motion;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(7);
const OUTPUT_FLUSH_INTERVAL: Duration = Duration::from_millis(16);
const COMMAND_CAPACITY: usize = 32;
const EVENT_CAPACITY: usize = 32;
const MAX_INPUT_BYTES: usize = 16 * 1024;
const MAX_OUTPUT_BATCH_BYTES: usize = 16 * 1024;
const MAX_PROTOCOL_FRAME_BYTES: usize = 64 * 1024;
const MAX_ERROR_CHARS: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TelnetSessionEvent {
    Connected,
    Output(Vec<u8>),
    Disconnected,
    Failed(String),
}

enum TelnetCommand {
    Send(Vec<u8>),
    Disconnect,
}

#[derive(Debug, PartialEq, Eq)]
enum InboundFrame {
    Parser(Bytes),
    LiteralData(Bytes),
}

#[derive(Debug, Default)]
struct TelnetFrameBuffer {
    pending: BytesMut,
}

impl TelnetFrameBuffer {
    fn receive(&mut self, data: &[u8]) -> Result<Vec<InboundFrame>> {
        let buffered = self
            .pending
            .len()
            .checked_add(data.len())
            .context("Telnet protocol frame length overflow")?;
        anyhow::ensure!(
            buffered <= MAX_PROTOCOL_FRAME_BYTES,
            "Telnet protocol frame exceeds {MAX_PROTOCOL_FRAME_BYTES} bytes"
        );
        self.pending.extend_from_slice(data);

        let mut frames = Vec::new();
        loop {
            if self.pending.is_empty() {
                break;
            }

            if self.pending[0] != IAC {
                let length = self
                    .pending
                    .iter()
                    .position(|byte| *byte == IAC)
                    .unwrap_or(self.pending.len());
                frames.push(InboundFrame::Parser(self.pending.split_to(length).freeze()));
                continue;
            }

            if self.pending.len() < 2 {
                break;
            }
            match self.pending[1] {
                IAC => {
                    self.pending.advance(2);
                    frames.push(InboundFrame::LiteralData(Bytes::from_static(&[IAC])));
                }
                WILL | WONT | DO | DONT => {
                    if self.pending.len() < 3 {
                        break;
                    }
                    frames.push(InboundFrame::Parser(self.pending.split_to(3).freeze()));
                }
                SB => {
                    let Some(length) = subnegotiation_frame_length(&self.pending) else {
                        break;
                    };
                    frames.push(InboundFrame::Parser(self.pending.split_to(length).freeze()));
                }
                _ => {
                    frames.push(InboundFrame::Parser(self.pending.split_to(2).freeze()));
                }
            }
        }
        Ok(frames)
    }
}

fn subnegotiation_frame_length(buffer: &[u8]) -> Option<usize> {
    let mut index = 2;
    while index + 1 < buffer.len() {
        if buffer[index] != IAC {
            index += 1;
            continue;
        }
        match buffer[index + 1] {
            IAC => index += 2,
            SE => return Some(index + 2),
            _ => index += 2,
        }
    }
    None
}

pub struct TelnetSessionHandle {
    command_tx: mpsc::Sender<TelnetCommand>,
    resize_tx: watch::Sender<TerminalSize>,
    task: JoinHandle<()>,
}

impl TelnetSessionHandle {
    pub fn spawn(
        runtime: &Handle,
        session_id: Uuid,
        config: TelnetConfig,
        columns: u32,
        rows: u32,
    ) -> (Self, mpsc::Receiver<TelnetSessionEvent>) {
        let initial_size = terminal_size(columns, rows);
        let (command_tx, command_rx) = mpsc::channel(COMMAND_CAPACITY);
        let (resize_tx, resize_rx) = watch::channel(initial_size);
        let (event_tx, event_rx) = mpsc::channel(EVENT_CAPACITY);
        let task = runtime.spawn(run_telnet_session(
            session_id, config, command_rx, resize_rx, event_tx,
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

    pub fn request_disconnect(&self) -> Result<()> {
        self.command_tx
            .try_send(TelnetCommand::Disconnect)
            .map_err(|error| anyhow::anyhow!("cannot request Telnet disconnect: {error}"))
    }

    pub fn request_send(&self, data: Vec<u8>) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        if data.len() > MAX_INPUT_BYTES {
            anyhow::bail!("terminal input cannot exceed {MAX_INPUT_BYTES} bytes");
        }
        self.command_tx
            .try_send(TelnetCommand::Send(data))
            .map_err(|error| anyhow::anyhow!("cannot queue Telnet input: {error}"))
    }

    /// Returns `false` when a pointer-motion frame is dropped under normal backpressure.
    pub fn request_send_motion(&self, data: Vec<u8>) -> Result<bool> {
        if data.is_empty() {
            return Ok(true);
        }
        if data.len() > MAX_INPUT_BYTES {
            anyhow::bail!("terminal input cannot exceed {MAX_INPUT_BYTES} bytes");
        }
        try_queue_tokio_motion(
            &self.command_tx,
            TelnetCommand::Send(data),
            "cannot queue Telnet mouse motion after worker stopped",
        )
    }

    pub fn request_resize(&self, columns: u32, rows: u32) -> Result<()> {
        if self.task.is_finished() || self.resize_tx.receiver_count() == 0 {
            anyhow::bail!("cannot resize after Telnet worker stopped");
        }
        let size = terminal_size(columns, rows);
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

    pub async fn shutdown(mut self) -> Result<()> {
        if !self.task.is_finished()
            && let Err(error) = queue_disconnect(&self.command_tx)
        {
            debug!(%error, "Telnet worker disconnect command was not queued during shutdown");
        }
        match timeout(WORKER_SHUTDOWN_TIMEOUT, &mut self.task).await {
            Ok(joined) => joined.context("Telnet worker task failed during shutdown"),
            Err(_) => {
                self.task.abort();
                match self.task.await {
                    Err(error) if error.is_cancelled() => {
                        warn!("Telnet worker exceeded shutdown timeout and was aborted");
                        Ok(())
                    }
                    Err(error) => Err(error).context("failed to abort Telnet worker task"),
                    Ok(()) => Ok(()),
                }
            }
        }
    }
}

fn queue_disconnect(
    command_tx: &mpsc::Sender<TelnetCommand>,
) -> Result<(), mpsc::error::TrySendError<TelnetCommand>> {
    command_tx.try_send(TelnetCommand::Disconnect)
}

async fn run_telnet_session(
    session_id: Uuid,
    config: TelnetConfig,
    mut command_rx: mpsc::Receiver<TelnetCommand>,
    mut resize_rx: watch::Receiver<TerminalSize>,
    event_tx: mpsc::Sender<TelnetSessionEvent>,
) {
    let connect = timeout(
        CONNECT_TIMEOUT,
        TcpStream::connect((&*config.host, config.port)),
    )
    .await;
    let stream = match connect {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => {
            send_event(
                &event_tx,
                TelnetSessionEvent::Failed(bounded_error(&error)),
                session_id,
            )
            .await;
            return;
        }
        Err(_) => {
            send_event(
                &event_tx,
                TelnetSessionEvent::Failed("Telnet connection timed out".into()),
                session_id,
            )
            .await;
            return;
        }
    };
    let (mut reader, mut writer) = stream.into_split();
    let mut parser = terminal_parser();
    for event in [
        parser.negotiate(WILL, SGA),
        parser.negotiate(DO, SGA),
        parser.negotiate(WILL, NAWS),
    ] {
        if let Err(error) = write_parser_event(&mut writer, event).await {
            send_event(
                &event_tx,
                TelnetSessionEvent::Failed(bounded_error(&error)),
                session_id,
            )
            .await;
            return;
        }
    }
    if !send_event(&event_tx, TelnetSessionEvent::Connected, session_id).await {
        return;
    }
    info!(session_id = %session_id, host = %config.host, port = config.port, "Telnet connected without encryption");

    let output_flush = sleep(OUTPUT_FLUSH_INTERVAL);
    tokio::pin!(output_flush);
    let mut output = Vec::new();
    let mut read_buffer = [0_u8; MAX_OUTPUT_BATCH_BYTES];
    let mut frame_buffer = TelnetFrameBuffer::default();
    let mut naws_enabled = false;
    let mut failed = false;
    loop {
        tokio::select! {
            command = command_rx.recv() => {
                match command {
                    Some(TelnetCommand::Send(data)) => {
                        let escaped = Parser::escape_iac(data);
                        if let Err(error) = writer.write_all(&escaped).await {
                            send_event(
                                &event_tx,
                                TelnetSessionEvent::Failed(bounded_error(&error)),
                                session_id,
                            ).await;
                            failed = true;
                            break;
                        }
                    }
                    Some(TelnetCommand::Disconnect) | None => break,
                }
            }
            changed = resize_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                let size = *resize_rx.borrow_and_update();
                if naws_enabled
                    && let Err(error) = send_window_size(&mut parser, &mut writer, size).await
                {
                    send_event(
                        &event_tx,
                        TelnetSessionEvent::Failed(bounded_error(&error)),
                        session_id,
                    ).await;
                    failed = true;
                    break;
                }
            }
            read = reader.read(&mut read_buffer) => {
                let arm_output_flush = output.is_empty();
                let read = match read {
                    Ok(0) => break,
                    Ok(read) => read,
                    Err(error) => {
                        send_event(
                            &event_tx,
                            TelnetSessionEvent::Failed(bounded_error(&error)),
                            session_id,
                        ).await;
                        failed = true;
                        break;
                    }
                };
                let frames = match frame_buffer.receive(&read_buffer[..read]) {
                    Ok(frames) => frames,
                    Err(error) => {
                        send_event(
                            &event_tx,
                            TelnetSessionEvent::Failed(bounded_error(&error)),
                            session_id,
                        ).await;
                        failed = true;
                        break;
                    }
                };
                let current_size = *resize_rx.borrow();
                match handle_inbound_frames(
                    &mut parser,
                    &mut writer,
                    frames,
                    &mut naws_enabled,
                    current_size,
                    &event_tx,
                    &mut output,
                    session_id,
                ).await {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(error) => {
                        send_event(
                            &event_tx,
                            TelnetSessionEvent::Failed(bounded_error(&error)),
                            session_id,
                        ).await;
                        failed = true;
                        break;
                    }
                }
                if arm_output_flush && !output.is_empty() {
                    output_flush
                        .as_mut()
                        .reset(TokioInstant::now() + OUTPUT_FLUSH_INTERVAL);
                }
            }
            _ = &mut output_flush, if !output.is_empty() => {
                if !flush_output(&event_tx, &mut output, session_id).await {
                    break;
                }
            }
        }
    }
    flush_output(&event_tx, &mut output, session_id).await;
    if let Err(error) = writer.shutdown().await {
        debug!(session_id = %session_id, error = %error, "Telnet socket shutdown failed");
    }
    if !failed {
        send_event(&event_tx, TelnetSessionEvent::Disconnected, session_id).await;
    }
}

fn terminal_parser() -> Parser {
    let mut options = CompatibilityTable::new();
    options.support(BINARY);
    options.support_remote(ECHO);
    options.support(SGA);
    options.support_local(NAWS);
    Parser::with_support_and_capacity(1024, options)
}

#[allow(clippy::too_many_arguments)]
async fn handle_inbound_frames(
    parser: &mut Parser,
    writer: &mut OwnedWriteHalf,
    frames: Vec<InboundFrame>,
    naws_enabled: &mut bool,
    size: TerminalSize,
    event_tx: &mpsc::Sender<TelnetSessionEvent>,
    output: &mut Vec<u8>,
    session_id: Uuid,
) -> Result<bool> {
    for frame in frames {
        let events = match frame {
            InboundFrame::Parser(bytes) => parser.receive(&bytes),
            InboundFrame::LiteralData(bytes) => vec![TelnetEvents::DataReceive(bytes)],
        };
        for event in events {
            match event {
                TelnetEvents::DataReceive(data) => {
                    if !append_output(event_tx, output, &data, session_id).await {
                        return Ok(false);
                    }
                }
                TelnetEvents::DataSend(data) => writer
                    .write_all(&data)
                    .await
                    .context("failed to send Telnet negotiation response")?,
                TelnetEvents::Negotiation(negotiation) => match negotiation {
                    negotiation if negotiation.option == NAWS && negotiation.command == DO => {
                        if !*naws_enabled {
                            *naws_enabled = true;
                            send_window_size(parser, writer, size).await?;
                        }
                    }
                    negotiation if negotiation.option == NAWS && negotiation.command == DONT => {
                        *naws_enabled = false;
                    }
                    _ => {}
                },
                TelnetEvents::DecompressImmediate(_) => {
                    anyhow::bail!("Telnet compression is not supported")
                }
                TelnetEvents::IAC(_) | TelnetEvents::Subnegotiation(_) => {}
                _ => {}
            }
        }
    }
    Ok(true)
}

async fn write_parser_event(writer: &mut OwnedWriteHalf, event: TelnetEvents) -> Result<()> {
    let TelnetEvents::DataSend(data) = event else {
        anyhow::bail!("Telnet parser did not produce outbound data")
    };
    writer
        .write_all(&data)
        .await
        .context("failed to send Telnet protocol data")
}

async fn send_window_size(
    parser: &mut Parser,
    writer: &mut OwnedWriteHalf,
    size: TerminalSize,
) -> Result<()> {
    let mut payload = Vec::with_capacity(4);
    payload.extend_from_slice(&(size.columns() as u16).to_be_bytes());
    payload.extend_from_slice(&(size.rows() as u16).to_be_bytes());
    let event = parser
        .subnegotiation(NAWS, payload)
        .context("Telnet server has not enabled NAWS")?;
    write_parser_event(writer, event)
        .await
        .context("failed to send Telnet window size")
}

async fn append_output(
    event_tx: &mpsc::Sender<TelnetSessionEvent>,
    output: &mut Vec<u8>,
    mut data: &[u8],
    session_id: Uuid,
) -> bool {
    while !data.is_empty() {
        let available = MAX_OUTPUT_BATCH_BYTES - output.len();
        let length = available.min(data.len());
        output.extend_from_slice(&data[..length]);
        data = &data[length..];
        if output.len() == MAX_OUTPUT_BATCH_BYTES
            && !flush_output(event_tx, output, session_id).await
        {
            return false;
        }
    }
    true
}

async fn flush_output(
    event_tx: &mpsc::Sender<TelnetSessionEvent>,
    output: &mut Vec<u8>,
    session_id: Uuid,
) -> bool {
    if output.is_empty() {
        return true;
    }
    let data = std::mem::take(output);
    send_event(event_tx, TelnetSessionEvent::Output(data), session_id).await
}

async fn send_event(
    event_tx: &mpsc::Sender<TelnetSessionEvent>,
    event: TelnetSessionEvent,
    session_id: Uuid,
) -> bool {
    if event_tx.send(event).await.is_err() {
        debug!(session_id = %session_id, "Telnet event receiver dropped");
        false
    } else {
        true
    }
}

fn terminal_size(columns: u32, rows: u32) -> TerminalSize {
    TerminalSize::backend(columns, rows)
}

fn bounded_error(error: &impl std::fmt::Display) -> String {
    error.to_string().chars().take(MAX_ERROR_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_command_queue_does_not_block_disconnect_request() {
        let (sender, _receiver) = mpsc::channel(COMMAND_CAPACITY);
        for _ in 0..COMMAND_CAPACITY {
            sender
                .try_send(TelnetCommand::Send(Vec::new()))
                .expect("test command should fill the queue");
        }
        assert!(matches!(
            queue_disconnect(&sender),
            Err(mpsc::error::TrySendError::Full(TelnetCommand::Disconnect))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_aborts_a_worker_that_never_returns_with_a_full_queue() {
        let (command_tx, _receiver) = mpsc::channel(COMMAND_CAPACITY);
        for _ in 0..COMMAND_CAPACITY {
            command_tx
                .try_send(TelnetCommand::Send(Vec::new()))
                .expect("test command should fill the queue");
        }
        let (resize_tx, _resize_rx) = watch::channel(TerminalSize::backend(80, 24));
        let task = tokio::spawn(std::future::pending::<()>());
        let handle = TelnetSessionHandle {
            command_tx,
            resize_tx,
            task,
        };
        let shutdown = tokio::spawn(handle.shutdown());
        tokio::task::yield_now().await;
        tokio::time::advance(WORKER_SHUTDOWN_TIMEOUT + Duration::from_millis(1)).await;
        assert!(shutdown.await.expect("shutdown task should join").is_ok());
    }

    use libmudtelnet_rs::telnet::op_command::NOP;
    use tokio::net::TcpListener;

    #[test]
    fn fragmented_frames_preserve_data_and_control_boundaries() {
        let wire = [
            b'A', b'\r', b'\n', IAC, IAC, b'B', IAC, NOP, IAC, WILL, 99, IAC, SB, 24, 1, b'x', IAC,
            IAC, b'y', IAC, SE,
        ];
        let mut frame_buffer = TelnetFrameBuffer::default();
        let mut frames = Vec::new();
        for byte in wire {
            frames.extend(
                frame_buffer
                    .receive(&[byte])
                    .expect("fragment should parse"),
            );
        }
        assert!(frame_buffer.pending.is_empty());

        let mut parser = terminal_parser();
        let mut output = Vec::new();
        let mut sent = Vec::new();
        let mut saw_nop = false;
        for frame in frames {
            let events = match frame {
                InboundFrame::Parser(bytes) => parser.receive(&bytes),
                InboundFrame::LiteralData(bytes) => vec![TelnetEvents::DataReceive(bytes)],
            };
            for event in events {
                match event {
                    TelnetEvents::DataReceive(data) => output.extend_from_slice(&data),
                    TelnetEvents::DataSend(data) => sent.extend_from_slice(&data),
                    TelnetEvents::IAC(command) if command.command == NOP => saw_nop = true,
                    _ => {}
                }
            }
        }

        assert_eq!(output, b"A\r\n\xffB");
        assert!(saw_nop);
        assert!(sent.windows(3).any(|window| window == [IAC, DONT, 99]));
    }

    #[test]
    fn repeated_resize_does_not_notify_telnet_watch_receiver() {
        let initial = terminal_size(80, 24);
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

        let changed = terminal_size(100, 30);
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn loopback_filters_controls_escapes_iac_and_sends_naws() {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("listener should bind");
        let address = listener
            .local_addr()
            .expect("listener should have an address");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("client should connect");
            let wire = [
                IAC, WILL, ECHO, IAC, WILL, 99, IAC, DO, NAWS, IAC, NOP, b'T', b'E', b'L', b'N',
                b'E', b'T', b'_', b'O', b'K', b'\r', b'\n', IAC, IAC, b'!',
            ];
            for byte in wire {
                stream
                    .write_all(&[byte])
                    .await
                    .expect("fragment should write");
                tokio::task::yield_now().await;
            }

            let mut received = Vec::new();
            let mut buffer = [0; 256];
            // Wait for the client to close its write half instead of dropping
            // the socket as soon as the expected responses arrive. On Windows,
            // that early drop can reset the client's pending socket operation.
            loop {
                let read = timeout(Duration::from_secs(2), stream.read(&mut buffer))
                    .await
                    .expect("client should respond")
                    .expect("response should read");
                if read == 0 {
                    break;
                }
                received.extend_from_slice(&buffer[..read]);
            }
            received
        });

        let (worker, mut events) = TelnetSessionHandle::spawn(
            &Handle::current(),
            Uuid::new_v4(),
            TelnetConfig {
                host: address.ip().to_string(),
                port: address.port(),
            },
            80,
            24,
        );
        let expected = b"TELNET_OK\r\n\xff!";
        let mut output = Vec::new();
        while let Some(event) = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("worker should emit an event")
        {
            match event {
                TelnetSessionEvent::Connected => {
                    worker.request_resize(255, 40).expect("resize should queue");
                    worker
                        .request_send(vec![b'G', b'O', IAC, b'\r'])
                        .expect("input should queue");
                }
                TelnetSessionEvent::Output(data) => {
                    output.extend(data);
                    if output.len() >= expected.len() {
                        break;
                    }
                }
                TelnetSessionEvent::Failed(message) => panic!("worker failed: {message}"),
                TelnetSessionEvent::Disconnected => break,
            }
        }
        worker.shutdown().await.expect("worker should stop");
        let received = server.await.expect("server should finish");

        assert_eq!(output, expected);
        assert!(
            received
                .windows(10)
                .any(|window| window == [IAC, SB, NAWS, 0, IAC, IAC, 0, 40, IAC, SE])
        );
        assert!(
            received
                .windows(5)
                .any(|window| window == [b'G', b'O', IAC, IAC, b'\r'])
        );
        assert!(received.windows(3).any(|window| window == [IAC, DONT, 99]));
        assert!(received.windows(3).any(|window| window == [IAC, DO, ECHO]));
        assert!(
            !received
                .windows(3)
                .any(|window| window == [IAC, WONT, ECHO])
        );
    }
}
