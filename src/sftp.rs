//! Bounded SFTP v3 browsing, transfers, and guarded write operations over authenticated SSH.

mod edit;
mod transfer;

#[cfg(feature = "egui-ui")]
pub(crate) use transfer::snapshot_local_file_for_open_at;
pub(crate) use transfer::{
    MAX_RECURSIVE_DOWNLOAD_FILES, SFTP_TRANSFER_EVENT_CAPACITY, SftpDownloadHandle,
    SftpDownloadRequest, SftpDownloadRoot, SftpUploadHandle, SftpUploadRequest,
    TransferRateLimiter, discover_download_requests,
};
pub use transfer::{
    SftpTransferEvent, SftpUploadConflictChoice, cleanup_stale_sftp_open_cache,
    configure_global_upload_limit, snapshot_local_file_for_open, transfer_name_matches_filter,
};

use std::collections::VecDeque;
use std::io;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};
use uuid::Uuid;

use anyhow::{Context, Result};
use russh_sftp::client::{Config, RawSftpSession, error::Error as SftpClientError};
use russh_sftp::protocol::{File, FileAttributes, OpenFlags, StatusCode};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};
use tracing::{debug, warn};

const COMMAND_CAPACITY: usize = 16;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_PATH_CHARS: usize = 4096;
const MAX_NAME_CHARS: usize = 512;
const MAX_PAGE_ENTRIES: usize = 250;
const MAX_DIRECTORY_TEXT_BYTES: usize = 2 * 1024 * 1024;
const MAX_PACKET_BYTES: u32 = 256 * 1024;
pub const MAX_UPLOAD_BYTES: u64 = 1024 * 1024 * 1024 * 1024;

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
        request_id: Option<u64>,
        path: String,
        entries: Vec<SftpEntry>,
        append: bool,
        has_more: bool,
        truncated: bool,
    },
    Failed {
        request_id: Option<u64>,
        message: String,
    },
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SftpWriteOperation {
    Remove {
        path: String,
        directory: bool,
    },
    Rename {
        old_path: String,
        new_path: String,
    },
    ReadEditFile {
        path: String,
        max_bytes: u64,
    },
    WriteEditFile {
        path: String,
        max_bytes: u64,
        data: Vec<u8>,
        expected_size: u64,
        expected_modified: Option<u32>,
    },
    Upload {
        path: String,
        data: Vec<u8>,
    },
    CheckMetadata {
        path: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SftpWriteResult {
    Completed {
        path: String,
    },
    Updated {
        path: String,
        size: u64,
        modified: Option<u32>,
    },
    EditFile {
        path: String,
        data: Vec<u8>,
        expected_size: u64,
        expected_modified: Option<u32>,
    },
    Metadata {
        path: String,
        size: u64,
        modified: Option<u32>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SftpWriteEvent {
    Completed {
        operation_id: uuid::Uuid,
        path: String,
    },
    Updated {
        operation_id: uuid::Uuid,
        path: String,
        size: u64,
        modified: Option<u32>,
    },
    EditFile {
        operation_id: uuid::Uuid,
        path: String,
        data: Vec<u8>,
        expected_size: u64,
        expected_modified: Option<u32>,
    },
    Metadata {
        operation_id: uuid::Uuid,
        path: String,
        size: u64,
        modified: Option<u32>,
    },
    Failed {
        operation_id: uuid::Uuid,
        message: String,
    },
}

enum SftpBrowserCommand {
    List {
        request_id: Option<u64>,
        path: String,
    },
    LoadMore {
        request_id: u64,
    },
    Close,
}

pub(crate) struct SftpBrowserHandle {
    command_tx: mpsc::Sender<SftpBrowserCommand>,
    task: Option<JoinHandle<()>>,
}

pub(crate) async fn execute_sftp_write<S>(
    stream: S,
    operation: SftpWriteOperation,
) -> Result<SftpWriteResult>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let session = RawSftpSession::new_with_config(
        PacketLimitedStream::new(stream),
        Config {
            max_packet_len: MAX_PACKET_BYTES,
            max_concurrent_reads: 16,
            max_concurrent_writes: 1,
            max_write_packet_len: MAX_PACKET_BYTES,
            request_timeout_secs: REQUEST_TIMEOUT.as_secs(),
        },
    );
    let version = timeout(REQUEST_TIMEOUT, session.init())
        .await
        .context("SFTP handshake timed out")?
        .context("SFTP handshake failed")?;
    let result = async {
        Ok::<_, anyhow::Error>(match operation {
            SftpWriteOperation::Remove { path, directory } => {
                validate_remote_path(&path)?;
                let attrs = timeout(REQUEST_TIMEOUT, session.lstat(path.clone()))
                    .await
                    .context("SFTP delete type check timed out")??
                    .attrs;
                if attrs.is_symlink() {
                    anyhow::bail!("symbolic links cannot be deleted through this SFTP action");
                }
                if attrs.is_dir() != directory {
                    anyhow::bail!("remote entry type changed; delete was rejected");
                }
                if !attrs.is_dir() && !attrs.is_regular() {
                    anyhow::bail!("only regular files and directories can be deleted");
                }
                if directory {
                    timeout(REQUEST_TIMEOUT, session.rmdir(path.clone()))
                        .await
                        .context("SFTP directory removal timed out")??;
                } else {
                    timeout(REQUEST_TIMEOUT, session.remove(path.clone()))
                        .await
                        .context("SFTP file removal timed out")??;
                }
                SftpWriteResult::Completed { path }
            }
            SftpWriteOperation::Rename { old_path, new_path } => {
                validate_remote_path(&old_path)?;
                validate_remote_path(&new_path)?;
                if old_path == new_path {
                    anyhow::bail!("old and new remote paths are identical");
                }
                let old_attrs = timeout(REQUEST_TIMEOUT, session.lstat(old_path.clone()))
                    .await
                    .context("SFTP rename source check timed out")??
                    .attrs;
                if old_attrs.is_symlink() || (!old_attrs.is_dir() && !old_attrs.is_regular()) {
                    anyhow::bail!("remote rename source must be a regular file or directory");
                }
                ensure_remote_target_absent(&session, &new_path, "rename").await?;
                timeout(REQUEST_TIMEOUT, session.rename(old_path, new_path.clone()))
                    .await
                    .context("SFTP rename timed out")??;
                SftpWriteResult::Completed { path: new_path }
            }
            SftpWriteOperation::ReadEditFile { path, max_bytes } => {
                edit::read_edit_file(&session, path, max_bytes).await?
            }
            SftpWriteOperation::WriteEditFile {
                path,
                max_bytes,
                data,
                expected_size,
                expected_modified,
            } => {
                edit::write_edit_file(
                    &session,
                    &version,
                    path,
                    data,
                    max_bytes,
                    expected_size,
                    expected_modified,
                )
                .await?
            }
            SftpWriteOperation::CheckMetadata { path } => {
                validate_remote_path(&path)?;
                let attrs = timeout(REQUEST_TIMEOUT, session.lstat(path.clone()))
                    .await
                    .context("SFTP metadata check timed out")??
                    .attrs;
                if attrs.is_symlink() || !attrs.is_regular() {
                    anyhow::bail!("remote monitored file is no longer a regular file");
                }
                SftpWriteResult::Metadata {
                    path,
                    size: attrs.size.unwrap_or(0),
                    modified: attrs.mtime,
                }
            }
            SftpWriteOperation::Upload { path, data } => {
                validate_remote_path(&path)?;
                let limit = MAX_UPLOAD_BYTES;
                if data.len() as u64 > limit {
                    anyhow::bail!("upload content exceeds the {limit}-byte limit");
                }
                let name = path
                    .rsplit('/')
                    .next()
                    .filter(|name| !name.is_empty())
                    .context("remote upload target is missing a file name")?;
                if name.starts_with('.') || name.chars().any(char::is_control) {
                    anyhow::bail!("remote upload target name is invalid");
                }
                ensure_remote_target_absent(&session, &path, "upload").await?;
                let parent = path.strip_suffix(name).unwrap_or("").trim_end_matches('/');
                let temporary = format!("{parent}/.{name}.axssh-upload-{}", Uuid::new_v4());
                let handle = timeout(
                    REQUEST_TIMEOUT,
                    session.open(
                        temporary.clone(),
                        OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
                        FileAttributes::default(),
                    ),
                )
                .await
                .context("SFTP upload temp open timed out")??
                .handle;
                let write_result = write_remote_bytes(&session, &handle, &data).await;
                let _ = timeout(REQUEST_TIMEOUT, session.close(handle)).await;
                if let Err(error) = write_result {
                    let _ = timeout(REQUEST_TIMEOUT, session.remove(temporary.clone())).await;
                    return Err(error);
                }
                if let Err(error) = timeout(
                    REQUEST_TIMEOUT,
                    session.rename(temporary.clone(), path.clone()),
                )
                .await
                .context("SFTP upload publish timed out")?
                {
                    let _ = timeout(REQUEST_TIMEOUT, session.remove(temporary)).await;
                    return Err(error.into());
                }
                SftpWriteResult::Completed { path }
            }
        })
    }
    .await;
    if let Err(error) = session.close_session() {
        debug!(%error, "failed to close SFTP operation session");
    }
    result
}

async fn ensure_remote_target_absent(
    session: &RawSftpSession,
    path: &str,
    operation: &str,
) -> Result<()> {
    match timeout(REQUEST_TIMEOUT, session.lstat(path.to_owned())).await {
        Ok(Ok(_)) => anyhow::bail!("remote target already exists; {operation} was rejected"),
        Ok(Err(russh_sftp::client::error::Error::Status(status)))
            if status.status_code == StatusCode::NoSuchFile =>
        {
            Ok(())
        }
        Ok(Err(error)) => Err(error).context("SFTP target existence check failed"),
        Err(_) => anyhow::bail!("SFTP target existence check timed out"),
    }
}

async fn write_remote_bytes(session: &RawSftpSession, handle: &str, data: &[u8]) -> Result<()> {
    use transfer::pipeline::{REQUEST_WINDOW, RequestWindow};
    let mut requests = RequestWindow::new();
    for (index, chunk) in data.chunks(64 * 1024).enumerate() {
        if requests.len() == REQUEST_WINDOW {
            requests
                .next()
                .await
                .context("missing SFTP write acknowledgement")??;
        }
        requests
            .push(async move {
                timeout(
                    REQUEST_TIMEOUT,
                    session.write(
                        handle.to_owned(),
                        (index * 64 * 1024) as u64,
                        chunk.to_vec(),
                    ),
                )
                .await
                .context("SFTP write timed out")?
                .context("SFTP write failed")?;
                Ok(())
            })
            .await?;
    }
    while let Some(result) = requests.next().await {
        result?;
    }
    Ok(())
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

    pub(crate) fn request_list(&self, request_id: Option<u64>, path: String) -> Result<()> {
        validate_remote_path(&path)?;
        self.command_tx
            .try_send(SftpBrowserCommand::List { request_id, path })
            .map_err(|error| anyhow::anyhow!("cannot queue SFTP directory request: {error}"))
    }

    pub(crate) fn request_load_more(&self, request_id: u64) -> Result<()> {
        self.command_tx
            .try_send(SftpBrowserCommand::LoadMore { request_id })
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
    total_text_bytes: usize,
    done: bool,
    truncated: bool,
}

/// Notify the browser when the SFTP reader loses its channel while no UI
/// command is pending. `russh_sftp` owns the stream after initialization.
struct BrowserObservedStream<S> {
    inner: S,
    closed: Option<oneshot::Sender<()>>,
}

impl<S> BrowserObservedStream<S> {
    fn new(inner: S, closed: oneshot::Sender<()>) -> Self {
        Self {
            inner,
            closed: Some(closed),
        }
    }

    fn notify_closed(&mut self) {
        if let Some(closed) = self.closed.take() {
            let _ = closed.send(());
        }
    }
}

impl<S> Drop for BrowserObservedStream<S> {
    fn drop(&mut self) {
        self.notify_closed();
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for BrowserObservedStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let filled = output.filled().len();
        let had_space = output.remaining() > 0;
        let result = Pin::new(&mut self.inner).poll_read(cx, output);
        let closed = match &result {
            Poll::Ready(Ok(())) => had_space && output.filled().len() == filled,
            Poll::Ready(Err(_)) => true,
            Poll::Pending => false,
        };
        if closed {
            self.notify_closed();
        }
        result
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for BrowserObservedStream<S> {
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
            .send(SftpBrowserEvent::Failed {
                request_id: None,
                message: bounded_error(&error),
            })
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
    let (channel_closed_tx, mut channel_closed_rx) = oneshot::channel();
    let session = RawSftpSession::new_with_config(
        BrowserObservedStream::new(PacketLimitedStream::new(stream), channel_closed_tx),
        Config {
            max_packet_len: MAX_PACKET_BYTES,
            max_concurrent_reads: 16,
            max_concurrent_writes: 1,
            max_write_packet_len: MAX_PACKET_BYTES,
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
    emit_page(&session, &mut initial_cursor, None, false, event_tx).await?;
    let mut cursor = Some(initial_cursor);

    loop {
        let command = tokio::select! {
            biased;
            _ = &mut channel_closed_rx => {
                anyhow::bail!("SFTP subsystem channel closed unexpectedly");
            }
            command = command_rx.recv() => command,
        };
        let Some(command) = command else {
            break;
        };
        match command {
            SftpBrowserCommand::List { request_id, path } => {
                let current = cursor_path(cursor.as_ref()).unwrap_or(&home).to_owned();
                let next = async {
                    let resolved = resolve_remote_path(&current, &home, &path)?;
                    let canonical = canonicalize(&session, &resolved).await?;
                    let mut next = open_directory(&session, canonical).await?;
                    emit_page(&session, &mut next, request_id, false, event_tx).await?;
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
                        if browser_transport_failed(&error) {
                            return Err(error.context("SFTP browser connection failed"));
                        }
                        send_request_error(event_tx, request_id, &error).await;
                    }
                }
            }
            SftpBrowserCommand::LoadMore { request_id } => match cursor.as_mut() {
                Some(cursor) if !cursor.done => {
                    if let Err(error) =
                        emit_page(&session, cursor, Some(request_id), true, event_tx).await
                    {
                        if browser_transport_failed(&error) {
                            return Err(error.context("SFTP browser connection failed"));
                        }
                        send_request_error(event_tx, Some(request_id), &error).await;
                    }
                }
                Some(_) => {
                    let error = anyhow::anyhow!("SFTP directory has no additional page");
                    send_request_error(event_tx, Some(request_id), &error).await;
                }
                None => {
                    let error = anyhow::anyhow!("SFTP directory is not open");
                    send_request_error(event_tx, Some(request_id), &error).await;
                }
            },
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

fn browser_transport_failed(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.is::<tokio::time::error::Elapsed>()
            || cause
                .downcast_ref::<SftpClientError>()
                .is_some_and(|error| match error {
                    SftpClientError::Status(status) => matches!(
                        status.status_code,
                        StatusCode::NoConnection | StatusCode::ConnectionLost
                    ),
                    SftpClientError::Limited(_) => false,
                    _ => true,
                })
    })
}

async fn send_request_error(
    event_tx: &mpsc::Sender<SftpBrowserEvent>,
    request_id: Option<u64>,
    error: &anyhow::Error,
) {
    let _ = event_tx
        .send(SftpBrowserEvent::Failed {
            request_id,
            message: bounded_error(error),
        })
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
        total_text_bytes: 0,
        done: false,
        truncated: false,
    })
}

async fn emit_page(
    session: &RawSftpSession,
    cursor: &mut DirectoryCursor,
    request_id: Option<u64>,
    append: bool,
    event_tx: &mpsc::Sender<SftpBrowserEvent>,
) -> Result<()> {
    let entries = read_page(session, cursor).await?;
    event_tx
        .send(SftpBrowserEvent::DirectoryPage {
            request_id,
            path: cursor.path.clone(),
            entries,
            append,
            has_more: !cursor.done,
            truncated: cursor.truncated,
        })
        .await
        .context("SFTP event receiver dropped")?;
    // Publish the page before waiting for the server to acknowledge CLOSE.
    if cursor.done {
        let handle = std::mem::take(&mut cursor.handle);
        if !handle.is_empty() {
            close_handle(session, handle).await;
        }
    }
    Ok(())
}

async fn read_page(
    session: &RawSftpSession,
    cursor: &mut DirectoryCursor,
) -> Result<Vec<SftpEntry>> {
    let mut page = Vec::with_capacity(MAX_PAGE_ENTRIES);
    while page.len() < MAX_PAGE_ENTRIES && !cursor.done {
        if cursor.pending.is_empty() {
            // A server may return much less than 250 entries per packet. Do
            // not wait for another round trip before showing useful results.
            if !page.is_empty() {
                break;
            }
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
mod tests;
