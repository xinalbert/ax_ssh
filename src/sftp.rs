//! Bounded SFTP v3 browsing and read-only transfers over authenticated SSH.

mod transfer;

pub(crate) use transfer::{SFTP_TRANSFER_EVENT_CAPACITY, SftpDownloadHandle, SftpDownloadRequest};
pub use transfer::{SftpTransferEvent, cleanup_stale_sftp_open_cache};

use std::collections::VecDeque;
use std::io;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};

use anyhow::{Context, Result};
use russh_sftp::client::{Config, RawSftpSession};
use russh_sftp::protocol::{File, StatusCode};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};
use tracing::{debug, warn};

const COMMAND_CAPACITY: usize = 16;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_PATH_CHARS: usize = 4096;
const MAX_NAME_CHARS: usize = 512;
const MAX_PAGE_ENTRIES: usize = 250;
const MAX_DIRECTORY_ENTRIES: usize = 2_000;
const MAX_DIRECTORY_TEXT_BYTES: usize = 2 * 1024 * 1024;
const MAX_PACKET_BYTES: u32 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SftpEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SftpBrowserEvent {
    Opened {
        home: String,
    },
    DirectoryPage {
        path: String,
        entries: Vec<SftpEntry>,
        append: bool,
        has_more: bool,
        truncated: bool,
    },
    Failed(String),
    Closed,
}

enum SftpBrowserCommand {
    List(String),
    LoadMore,
    Close,
}

pub(crate) struct SftpBrowserHandle {
    command_tx: mpsc::Sender<SftpBrowserCommand>,
    task: Option<JoinHandle<()>>,
}

impl SftpBrowserHandle {
    pub(crate) fn spawn<S>(
        runtime: &Handle,
        stream: S,
        initial_path: String,
        event_tx: mpsc::Sender<SftpBrowserEvent>,
    ) -> Result<Self>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        validate_remote_path(&initial_path)?;
        let (command_tx, command_rx) = mpsc::channel(COMMAND_CAPACITY);
        let task = runtime.spawn(run_browser(stream, initial_path, command_rx, event_tx));
        Ok(Self {
            command_tx,
            task: Some(task),
        })
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.task.as_ref().is_none_or(|task| task.is_finished())
    }

    pub(crate) fn request_list(&self, path: String) -> Result<()> {
        validate_remote_path(&path)?;
        self.command_tx
            .try_send(SftpBrowserCommand::List(path))
            .map_err(|error| anyhow::anyhow!("cannot queue SFTP directory request: {error}"))
    }

    pub(crate) fn request_load_more(&self) -> Result<()> {
        self.command_tx
            .try_send(SftpBrowserCommand::LoadMore)
            .map_err(|error| anyhow::anyhow!("cannot queue SFTP page request: {error}"))
    }

    pub(crate) async fn shutdown(mut self) -> Result<()> {
        if !self.is_finished() {
            match self.command_tx.try_send(SftpBrowserCommand::Close) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    debug!("SFTP browser command queue full during shutdown");
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    debug!("SFTP browser command receiver already closed during shutdown");
                }
            }
        }
        let Some(mut task) = self.task.take() else {
            return Ok(());
        };
        match timeout(SHUTDOWN_TIMEOUT, &mut task).await {
            Ok(joined) => joined.context("SFTP browser task failed during shutdown"),
            Err(_) => {
                task.abort();
                match task.await {
                    Err(error) if error.is_cancelled() => {
                        warn!("SFTP browser exceeded shutdown timeout and was aborted");
                        Ok(())
                    }
                    Err(error) => Err(error).context("failed to abort SFTP browser task"),
                    Ok(()) => Ok(()),
                }
            }
        }
    }
}

impl Drop for SftpBrowserHandle {
    fn drop(&mut self) {
        // The worker normally joins this task through `shutdown`. If the
        // owning SSH worker is aborted by its outer deadline, dropping the
        // handle must still stop the browser rather than detach it.
        if let Some(task) = self.task.as_ref()
            && !task.is_finished()
        {
            task.abort();
        }
    }
}

struct DirectoryCursor {
    path: String,
    handle: String,
    pending: VecDeque<File>,
    scanned_entries: usize,
    total_text_bytes: usize,
    done: bool,
    truncated: bool,
}

struct PacketLimitedStream<S> {
    inner: S,
    packet: Vec<u8>,
    packet_filled: usize,
    packet_emitted: usize,
    packet_target: Option<usize>,
    failed: bool,
}

impl<S> PacketLimitedStream<S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            packet: vec![0; 4],
            packet_filled: 0,
            packet_emitted: 0,
            packet_target: None,
            failed: false,
        }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for PacketLimitedStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.as_mut().get_mut();
        if this.failed {
            return Poll::Ready(Ok(()));
        }
        let initial_filled = output.filled().len();
        loop {
            if output.remaining() == 0 {
                return Poll::Ready(Ok(()));
            }

            let target = this.packet_target.unwrap_or(4);
            if this.packet_filled < target {
                let mut packet_output = ReadBuf::new(&mut this.packet[this.packet_filled..target]);
                match Pin::new(&mut this.inner).poll_read(cx, &mut packet_output) {
                    Poll::Pending if output.filled().len() > initial_filled => {
                        return Poll::Ready(Ok(()));
                    }
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(error)) => {
                        this.failed = true;
                        return Poll::Ready(Err(error));
                    }
                    Poll::Ready(Ok(())) if packet_output.filled().is_empty() => {
                        if this.packet_filled == 0 && this.packet_target.is_none() {
                            return Poll::Ready(Ok(()));
                        }
                        this.failed = true;
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "SFTP stream ended inside a packet",
                        )));
                    }
                    Poll::Ready(Ok(())) => {
                        this.packet_filled += packet_output.filled().len();
                    }
                }
                if this.packet_filled < target {
                    continue;
                }
            }

            if this.packet_target.is_none() {
                let packet_len = u32::from_be_bytes([
                    this.packet[0],
                    this.packet[1],
                    this.packet[2],
                    this.packet[3],
                ]) as usize;
                if packet_len == 0 || packet_len > MAX_PACKET_BYTES as usize {
                    this.failed = true;
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        if packet_len == 0 {
                            "SFTP packet payload cannot be empty".to_owned()
                        } else {
                            format!(
                                "SFTP packet length {packet_len} exceeds the {MAX_PACKET_BYTES}-byte limit"
                            )
                        },
                    )));
                }
                let target = 4 + packet_len;
                this.packet.resize(target, 0);
                this.packet_target = Some(target);
                continue;
            }

            let Some(target) = this.packet_target else {
                this.failed = true;
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "SFTP packet parser lost its validated frame length",
                )));
            };
            let count = output.remaining().min(target - this.packet_emitted);
            let end = this.packet_emitted + count;
            output.put_slice(&this.packet[this.packet_emitted..end]);
            this.packet_emitted = end;
            if this.packet_emitted == target {
                this.packet.resize(4, 0);
                this.packet_filled = 0;
                this.packet_emitted = 0;
                this.packet_target = None;
            }
        }
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for PacketLimitedStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buffer)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

async fn run_browser<S>(
    stream: S,
    initial_path: String,
    mut command_rx: mpsc::Receiver<SftpBrowserCommand>,
    event_tx: mpsc::Sender<SftpBrowserEvent>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let result = run_browser_inner(stream, initial_path, &mut command_rx, &event_tx).await;
    if let Err(error) = result {
        let _ = event_tx
            .send(SftpBrowserEvent::Failed(bounded_error(&error)))
            .await;
    }
    let _ = event_tx.send(SftpBrowserEvent::Closed).await;
}

async fn run_browser_inner<S>(
    stream: S,
    initial_path: String,
    command_rx: &mut mpsc::Receiver<SftpBrowserCommand>,
    event_tx: &mpsc::Sender<SftpBrowserEvent>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let session = RawSftpSession::new_with_config(
        PacketLimitedStream::new(stream),
        Config {
            max_packet_len: MAX_PACKET_BYTES,
            max_concurrent_writes: 1,
            request_timeout_secs: REQUEST_TIMEOUT.as_secs(),
        },
    );
    timeout(REQUEST_TIMEOUT, session.init())
        .await
        .context("SFTP handshake timed out")?
        .context("SFTP handshake failed")?;

    let home = canonicalize(&session, ".").await?;
    event_tx
        .send(SftpBrowserEvent::Opened { home: home.clone() })
        .await
        .context("SFTP event receiver dropped")?;

    let initial = resolve_remote_path(&home, &home, &initial_path)?;
    let mut initial_cursor = open_directory(&session, initial).await?;
    emit_page(&session, &mut initial_cursor, false, event_tx).await?;
    let mut cursor = Some(initial_cursor);

    while let Some(command) = command_rx.recv().await {
        match command {
            SftpBrowserCommand::List(path) => {
                let current = cursor_path(cursor.as_ref()).unwrap_or(&home).to_owned();
                let next = async {
                    let resolved = resolve_remote_path(&current, &home, &path)?;
                    let canonical = canonicalize(&session, &resolved).await?;
                    let mut next = open_directory(&session, canonical).await?;
                    emit_page(&session, &mut next, false, event_tx).await?;
                    Ok::<_, anyhow::Error>(next)
                }
                .await;
                match next {
                    Ok(next) => {
                        if let Some(previous) = cursor.replace(next) {
                            close_cursor(&session, previous).await;
                        }
                    }
                    Err(error) => {
                        send_request_error(event_tx, &error).await;
                    }
                }
            }
            SftpBrowserCommand::LoadMore => {
                if let Some(cursor) = cursor.as_mut()
                    && !cursor.done
                    && let Err(error) = emit_page(&session, cursor, true, event_tx).await
                {
                    send_request_error(event_tx, &error).await;
                }
            }
            SftpBrowserCommand::Close => break,
        }
    }

    if let Some(cursor) = cursor {
        close_cursor(&session, cursor).await;
    }
    session
        .close_session()
        .context("failed to close SFTP session")?;
    Ok(())
}

async fn send_request_error(event_tx: &mpsc::Sender<SftpBrowserEvent>, error: &anyhow::Error) {
    let _ = event_tx
        .send(SftpBrowserEvent::Failed(bounded_error(error)))
        .await;
}

fn cursor_path(cursor: Option<&DirectoryCursor>) -> Option<&str> {
    cursor.map(|cursor| cursor.path.as_str())
}

async fn canonicalize(session: &RawSftpSession, path: &str) -> Result<String> {
    validate_remote_path(path)?;
    let names = timeout(REQUEST_TIMEOUT, session.realpath(path.to_owned()))
        .await
        .context("SFTP realpath timed out")?
        .with_context(|| format!("cannot resolve remote path {path:?}"))?;
    let path = names
        .files
        .first()
        .map(|file| file.filename.clone())
        .context("SFTP server returned an empty realpath response")?;
    validate_remote_path(&path)?;
    Ok(path)
}

async fn open_directory(session: &RawSftpSession, path: String) -> Result<DirectoryCursor> {
    validate_remote_path(&path)?;
    let handle = timeout(REQUEST_TIMEOUT, session.opendir(path.clone()))
        .await
        .context("SFTP open directory timed out")?
        .with_context(|| format!("cannot open remote directory {path:?}"))?
        .handle;
    Ok(DirectoryCursor {
        path,
        handle,
        pending: VecDeque::new(),
        scanned_entries: 0,
        total_text_bytes: 0,
        done: false,
        truncated: false,
    })
}

async fn emit_page(
    session: &RawSftpSession,
    cursor: &mut DirectoryCursor,
    append: bool,
    event_tx: &mpsc::Sender<SftpBrowserEvent>,
) -> Result<()> {
    let entries = read_page(session, cursor).await?;
    event_tx
        .send(SftpBrowserEvent::DirectoryPage {
            path: cursor.path.clone(),
            entries,
            append,
            has_more: !cursor.done,
            truncated: cursor.truncated,
        })
        .await
        .context("SFTP event receiver dropped")
}

async fn read_page(
    session: &RawSftpSession,
    cursor: &mut DirectoryCursor,
) -> Result<Vec<SftpEntry>> {
    let mut page = Vec::with_capacity(MAX_PAGE_ENTRIES);
    while page.len() < MAX_PAGE_ENTRIES && !cursor.done {
        if cursor.pending.is_empty() {
            match timeout(REQUEST_TIMEOUT, session.readdir(cursor.handle.clone()))
                .await
                .context("SFTP read directory timed out")?
            {
                Ok(names) if names.files.is_empty() => {
                    cursor.done = true;
                    break;
                }
                Ok(names) => cursor.pending.extend(names.files),
                Err(russh_sftp::client::error::Error::Status(status))
                    if status.status_code == StatusCode::Eof =>
                {
                    cursor.done = true;
                    break;
                }
                Err(error) => return Err(error).context("cannot read remote directory"),
            }
        }

        while page.len() < MAX_PAGE_ENTRIES {
            let Some(file) = cursor.pending.pop_front() else {
                break;
            };
            if cursor.scanned_entries >= MAX_DIRECTORY_ENTRIES {
                cursor.done = true;
                cursor.truncated = true;
                cursor.pending.clear();
                break;
            }
            cursor.scanned_entries += 1;
            let scan_limit_reached = cursor.scanned_entries == MAX_DIRECTORY_ENTRIES;
            if let Some(entry) = bounded_entry(&cursor.path, file) {
                let text_bytes = entry.name.len().saturating_add(entry.path.len());
                if cursor.total_text_bytes.saturating_add(text_bytes) > MAX_DIRECTORY_TEXT_BYTES {
                    cursor.done = true;
                    cursor.truncated = true;
                    cursor.pending.clear();
                    break;
                }
                cursor.total_text_bytes += text_bytes;
                page.push(entry);
            }
            if scan_limit_reached {
                cursor.done = true;
                cursor.truncated = true;
                cursor.pending.clear();
                break;
            }
        }
    }

    if cursor.done {
        let handle = std::mem::take(&mut cursor.handle);
        if !handle.is_empty() {
            close_handle(session, handle).await;
        }
    }
    Ok(page)
}

fn bounded_entry(parent: &str, file: File) -> Option<SftpEntry> {
    let name = file.filename;
    if name == "."
        || name == ".."
        || name.is_empty()
        || name.chars().count() > MAX_NAME_CHARS
        || name.chars().any(char::is_control)
        || name.contains('/')
    {
        return None;
    }
    let path = join_remote(parent, &name);
    if validate_remote_path(&path).is_err() {
        return None;
    }
    let file_type = file.attrs.file_type();
    Some(SftpEntry {
        name,
        path,
        is_dir: file_type.is_dir(),
        is_symlink: file_type.is_symlink(),
        size: file.attrs.size.unwrap_or(0),
        modified: file.attrs.mtime,
    })
}

async fn close_cursor(session: &RawSftpSession, cursor: DirectoryCursor) {
    if !cursor.handle.is_empty() {
        close_handle(session, cursor.handle).await;
    }
}

async fn close_handle(session: &RawSftpSession, handle: String) {
    match timeout(REQUEST_TIMEOUT, session.close(handle)).await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => debug!(%error, "failed to close SFTP directory handle"),
        Err(_) => debug!("timed out closing SFTP directory handle"),
    }
}

fn resolve_remote_path(current: &str, home: &str, path: &str) -> Result<String> {
    validate_remote_path(path)?;
    let candidate = if path.is_empty() || path == "." {
        current.to_owned()
    } else if path == "~" {
        home.to_owned()
    } else if let Some(rest) = path.strip_prefix("~/") {
        join_remote(home, rest)
    } else if path.starts_with('/') {
        path.to_owned()
    } else {
        join_remote(current, path)
    };
    normalize_remote_path(&candidate)
}

fn normalize_remote_path(path: &str) -> Result<String> {
    let absolute = path.starts_with('/');
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    let normalized = if absolute {
        format!("/{}", parts.join("/"))
    } else if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    };
    validate_remote_path(&normalized)?;
    Ok(normalized)
}

fn join_remote(parent: &str, child: &str) -> String {
    if parent == "/" {
        format!("/{child}")
    } else {
        format!("{}/{child}", parent.trim_end_matches('/'))
    }
}

pub(crate) fn validate_remote_path(path: &str) -> Result<()> {
    if path.chars().count() > MAX_PATH_CHARS {
        anyhow::bail!("remote path cannot exceed {MAX_PATH_CHARS} characters");
    }
    if path.chars().any(char::is_control) {
        anyhow::bail!("remote path cannot contain control characters");
    }
    Ok(())
}

fn bounded_error(error: &anyhow::Error) -> String {
    const MAX_ERROR_CHARS: usize = 512;
    let message = format!("{error:#}");
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn packet_limiter_preserves_fragmented_valid_frames() {
        let (reader, mut writer) = tokio::io::duplex(32);
        let writer_task = tokio::spawn(async move {
            writer
                .write_all(&[0, 0])
                .await
                .expect("first header fragment should write");
            writer
                .write_all(&[0, 3, b'a'])
                .await
                .expect("second frame fragment should write");
            writer
                .write_all(b"bc")
                .await
                .expect("remaining payload should write");
        });
        let mut limited = PacketLimitedStream::new(reader);
        let mut packet = [0_u8; 7];

        limited
            .read_exact(&mut packet)
            .await
            .expect("valid fragmented frame should pass through");
        writer_task.await.expect("writer task should finish");

        assert_eq!(packet, [0, 0, 0, 3, b'a', b'b', b'c']);
    }

    #[tokio::test]
    async fn packet_limiter_supports_tiny_reads_and_consecutive_frames() {
        let (reader, mut writer) = tokio::io::duplex(32);
        writer
            .write_all(&[0, 0, 0, 1, b'a', 0, 0, 0, 2, b'b', b'c'])
            .await
            .expect("two frames should write");
        drop(writer);
        let mut limited = PacketLimitedStream::new(reader);
        let mut bytes = Vec::new();
        let mut byte = [0_u8; 1];

        while limited
            .read(&mut byte)
            .await
            .expect("tiny packet read should succeed")
            != 0
        {
            bytes.push(byte[0]);
        }

        assert_eq!(bytes, [0, 0, 0, 1, b'a', 0, 0, 0, 2, b'b', b'c']);
    }

    #[tokio::test]
    async fn packet_limiter_rejects_oversized_frames_before_payload_allocation() {
        let (reader, mut writer) = tokio::io::duplex(32);
        writer
            .write_all(&(MAX_PACKET_BYTES + 1).to_be_bytes())
            .await
            .expect("oversized header should write");
        let mut limited = PacketLimitedStream::new(reader);
        let mut byte = [0_u8; 1];

        let error = limited
            .read_exact(&mut byte)
            .await
            .expect_err("oversized frame should be rejected");

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("exceeds"));
        assert_eq!(
            limited
                .read(&mut byte)
                .await
                .expect("failed limiter should become EOF"),
            0
        );
    }

    #[tokio::test]
    async fn packet_limiter_rejects_truncated_frames() {
        let (reader, mut writer) = tokio::io::duplex(32);
        writer
            .write_all(&[0, 0, 0, 3, b'a'])
            .await
            .expect("partial frame should write");
        drop(writer);
        let mut limited = PacketLimitedStream::new(reader);
        let mut packet = [0_u8; 7];

        let error = limited
            .read_exact(&mut packet)
            .await
            .expect_err("truncated frame should be rejected");

        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        assert_eq!(
            limited
                .read(&mut packet[..1])
                .await
                .expect("truncated limiter should become EOF"),
            0
        );
    }

    #[test]
    fn remote_paths_resolve_against_current_and_home() {
        assert_eq!(
            resolve_remote_path("/srv/app/current", "/home/alice", "../logs")
                .expect("relative path should resolve"),
            "/srv/app/logs"
        );
        assert_eq!(
            resolve_remote_path("/srv/app", "/home/alice", "~/notes")
                .expect("home path should resolve"),
            "/home/alice/notes"
        );
        assert_eq!(
            resolve_remote_path("/srv/app", "/home/alice", "notes ")
                .expect("valid trailing spaces should be preserved"),
            "/srv/app/notes "
        );
    }

    #[test]
    fn remote_paths_reject_control_characters_and_excessive_length() {
        assert!(validate_remote_path("/tmp/line\nbreak").is_err());
        assert!(validate_remote_path(&"a".repeat(MAX_PATH_CHARS + 1)).is_err());
    }

    #[test]
    fn entries_reject_ambiguous_or_unsafe_names() {
        let attrs = russh_sftp::protocol::FileAttributes::empty();
        assert!(bounded_entry("/tmp", File::new("..", attrs.clone())).is_none());
        assert!(bounded_entry("/tmp", File::new("bad/name", attrs.clone())).is_none());
        assert!(bounded_entry("/tmp", File::new("line\nbreak", attrs)).is_none());
    }
}
