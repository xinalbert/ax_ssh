//! Bounded read-only SFTP downloads into AxSSH's private open-file cache.

use std::error::Error as StdError;
use std::fmt;
use std::fs::{self, File as LocalFile, OpenOptions};
use std::future::Future;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use russh_sftp::client::{Config, RawSftpSession};
use russh_sftp::protocol::{FileAttributes, OpenFlags, StatusCode};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::runtime::Handle;
use tokio::sync::{Notify, mpsc};
use tokio::task::{JoinError, JoinHandle};
use tokio::time::{Duration, Instant, timeout, timeout_at};
use tracing::{debug, warn};
use uuid::Uuid;

use super::{
    MAX_NAME_CHARS, MAX_PACKET_BYTES, PacketLimitedStream, REQUEST_TIMEOUT, bounded_error,
    validate_remote_path,
};

pub(crate) const SFTP_TRANSFER_EVENT_CAPACITY: usize = 32;

const DOWNLOAD_CHUNK_BYTES: u32 = 64 * 1024;
const WRITER_QUEUE_CAPACITY: usize = 2;
const PROGRESS_STEP_BYTES: u64 = 1024 * 1024;
const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const WRITER_CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);
const CACHE_STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
const PART_STALE_AFTER: Duration = Duration::from_secs(60 * 60);
const MAX_CACHE_BASENAME_BYTES: usize = 160;
const MAX_CACHE_SCAN_ENTRIES: usize = 4096;
const MAX_CACHE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_CACHE_FILES: usize = 128;

static CACHE_QUOTA_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SftpTransferEvent {
    Started {
        transfer_id: Uuid,
        remote_path: String,
        name: String,
        total_bytes: u64,
    },
    Progress {
        transfer_id: Uuid,
        downloaded_bytes: u64,
        total_bytes: u64,
    },
    Completed {
        transfer_id: Uuid,
        local_path: PathBuf,
        total_bytes: u64,
    },
    Cancelled {
        transfer_id: Uuid,
    },
    Failed {
        transfer_id: Uuid,
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SftpDownloadRequest {
    transfer_id: Uuid,
    remote_path: String,
    name: String,
}

impl SftpDownloadRequest {
    pub(crate) fn new(transfer_id: Uuid, remote_path: String) -> Result<Self> {
        let name = validate_download_path(&remote_path)?.to_owned();
        Ok(Self {
            transfer_id,
            remote_path,
            name,
        })
    }

    pub(crate) fn transfer_id(&self) -> Uuid {
        self.transfer_id
    }
}

pub(crate) struct SftpDownloadHandle {
    transfer_id: Uuid,
    cancellation: TransferCancellation,
    task: JoinHandle<()>,
}

impl SftpDownloadHandle {
    pub(crate) fn spawn<S>(
        runtime: &Handle,
        stream: S,
        request: SftpDownloadRequest,
        event_tx: mpsc::Sender<SftpTransferEvent>,
    ) -> Self
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let transfer_id = request.transfer_id;
        let cancellation = TransferCancellation::new();
        let task_cancellation = cancellation.clone();
        let task = runtime.spawn(run_download(stream, request, task_cancellation, event_tx));
        Self {
            transfer_id,
            cancellation,
            task,
        }
    }

    pub(crate) fn transfer_id(&self) -> Uuid {
        self.transfer_id
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.task.is_finished()
    }

    pub(crate) fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub(crate) async fn shutdown(mut self) -> Result<()> {
        if !self.task.is_finished() {
            self.cancellation.cancel();
        }
        match timeout(SHUTDOWN_TIMEOUT, &mut self.task).await {
            Ok(joined) => joined.context("SFTP download task failed during shutdown"),
            Err(_) => {
                self.task.abort();
                match (&mut self.task).await {
                    Err(error) if error.is_cancelled() => {
                        warn!(
                            transfer_id = %self.transfer_id,
                            "SFTP download exceeded shutdown timeout and was aborted"
                        );
                        Ok(())
                    }
                    Err(error) => Err(error).context("failed to abort SFTP download task"),
                    Ok(()) => Ok(()),
                }
            }
        }
    }
}

impl Drop for SftpDownloadHandle {
    fn drop(&mut self) {
        if !self.task.is_finished() {
            self.cancellation.cancel();
            self.task.abort();
        }
    }
}

/// Removes only stale, AxSSH-named files directly inside the private cache.
///
/// Call this from the Tokio runtime during application startup. Directories and
/// unrecognized names are deliberately left untouched, and removal failures are
/// best-effort so an external application holding a cached file cannot block
/// startup.
pub async fn cleanup_stale_sftp_open_cache() -> Result<usize> {
    tokio::task::spawn_blocking(|| {
        let cache_dir = cache_namespace()?;
        ensure_cache_namespace(&cache_dir)?;
        let _quota = CACHE_QUOTA_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .map_err(|_| anyhow::anyhow!("SFTP cache quota lock is poisoned"))?;
        cleanup_stale_cache_at(&cache_dir, SystemTime::now())
    })
    .await
    .context("SFTP cache cleanup task failed")?
}

async fn run_download<S>(
    stream: S,
    request: SftpDownloadRequest,
    cancellation: TransferCancellation,
    event_tx: mpsc::Sender<SftpTransferEvent>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let transfer_id = request.transfer_id;
    let result = match cache_namespace() {
        Ok(cache_dir) => {
            download_to_cache(stream, &request, &cancellation, &event_tx, &cache_dir).await
        }
        Err(error) => Err(error),
    };
    let terminal_event = match result {
        Ok((local_path, total_bytes)) => SftpTransferEvent::Completed {
            transfer_id,
            local_path,
            total_bytes,
        },
        Err(error) if is_cancelled(&error) => SftpTransferEvent::Cancelled { transfer_id },
        Err(error) => SftpTransferEvent::Failed {
            transfer_id,
            message: bounded_error(&error),
        },
    };
    let cleanup_path = match &terminal_event {
        SftpTransferEvent::Completed { local_path, .. } => Some(local_path.clone()),
        _ => None,
    };
    let delivered = match timeout(REQUEST_TIMEOUT, event_tx.send(terminal_event)).await {
        Ok(Ok(())) => true,
        Ok(Err(_)) => {
            debug!(%transfer_id, "SFTP transfer event receiver dropped");
            false
        }
        Err(_) => {
            debug!(%transfer_id, "timed out sending terminal SFTP transfer event");
            false
        }
    };
    if !delivered && let Some(path) = cleanup_path {
        remove_cache_file_best_effort(path, "terminal transfer event was not delivered").await;
    }
}

async fn download_to_cache<S>(
    stream: S,
    request: &SftpDownloadRequest,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
    cache_dir: &Path,
) -> Result<(PathBuf, u64)>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let deadline = Instant::now() + DOWNLOAD_TIMEOUT;
    let session = RawSftpSession::new_with_config(
        PacketLimitedStream::new(stream),
        Config {
            max_packet_len: MAX_PACKET_BYTES,
            max_concurrent_writes: 1,
            request_timeout_secs: REQUEST_TIMEOUT.as_secs(),
        },
    );
    await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "SFTP transfer handshake",
        session.init(),
    )
    .await?
    .context("SFTP transfer handshake failed")?;

    let result = download_initialized(
        &session,
        request,
        cancellation,
        event_tx,
        cache_dir,
        deadline,
    )
    .await;
    if let Err(error) = session.close_session() {
        debug!(%error, "failed to close SFTP transfer session");
    }
    result
}

async fn download_initialized(
    session: &RawSftpSession,
    request: &SftpDownloadRequest,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
    cache_dir: &Path,
    deadline: Instant,
) -> Result<(PathBuf, u64)> {
    let initial = lstat_regular_file(session, &request.remote_path, cancellation, deadline).await?;
    let handle = await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "opening remote file",
        session.open(
            request.remote_path.clone(),
            OpenFlags::READ,
            FileAttributes::empty(),
        ),
    )
    .await?
    .with_context(|| format!("cannot open remote file {:?}", request.remote_path))?
    .handle;

    let result = download_open_handle(
        session,
        &handle,
        request,
        initial,
        cancellation,
        event_tx,
        cache_dir,
        deadline,
    )
    .await;
    close_remote_handle(session, handle).await;
    result
}

#[allow(clippy::too_many_arguments)]
async fn download_open_handle(
    session: &RawSftpSession,
    handle: &str,
    request: &SftpDownloadRequest,
    initial: RemoteFileMetadata,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
    cache_dir: &Path,
    deadline: Instant,
) -> Result<(PathBuf, u64)> {
    let opened = await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "validating opened remote file",
        session.fstat(handle.to_owned()),
    )
    .await?
    .context("cannot inspect opened remote file")?;
    let opened = validate_regular_metadata(&opened.attrs)?;
    ensure_same_remote_file(initial, opened)?;

    // Recheck the path after opening so a path replaced with a symlink is
    // rejected even if the already-open handle still points at a regular file.
    let reopened =
        lstat_regular_file(session, &request.remote_path, cancellation, deadline).await?;
    ensure_same_remote_file(initial, reopened)?;

    await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "announcing SFTP transfer",
        event_tx.send(SftpTransferEvent::Started {
            transfer_id: request.transfer_id,
            remote_path: request.remote_path.clone(),
            name: request.name.clone(),
            total_bytes: initial.size,
        }),
    )
    .await?
    .context("SFTP transfer event receiver dropped")?;

    let pending = prepare_cache_file(
        cache_dir.to_owned(),
        request.name.clone(),
        initial.size,
        cancellation,
        deadline,
    )
    .await?;
    let (chunk_tx, chunk_rx) = mpsc::channel(WRITER_QUEUE_CAPACITY);
    let writer_cancel = cancellation.clone();
    let expected_bytes = initial.size;
    let mut writer = tokio::task::spawn_blocking(move || {
        write_cache_file(pending, chunk_rx, expected_bytes, &writer_cancel)
    });

    let stream_result = stream_remote_file(
        session,
        handle,
        &request.remote_path,
        request.transfer_id,
        initial,
        cancellation,
        event_tx,
        &chunk_tx,
        deadline,
    )
    .await;
    drop(chunk_tx);

    match stream_result {
        Ok(()) => {
            let local_path = await_writer_success(&mut writer, cancellation, deadline).await?;
            Ok((local_path, initial.size))
        }
        Err(stream_error) => {
            let writer_error = await_writer_cleanup(&mut writer).await;
            if is_cancelled(&stream_error) {
                return Err(stream_error);
            }
            match writer_error {
                Some(error) if !is_incomplete_cache_write(&error) && !is_cancelled(&error) => {
                    Err(error)
                }
                _ => Err(stream_error),
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn stream_remote_file(
    session: &RawSftpSession,
    handle: &str,
    remote_path: &str,
    transfer_id: Uuid,
    expected: RemoteFileMetadata,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
    chunk_tx: &mpsc::Sender<Vec<u8>>,
    deadline: Instant,
) -> Result<()> {
    let mut downloaded_bytes = 0_u64;
    let mut next_progress = PROGRESS_STEP_BYTES.min(expected.size);
    while downloaded_bytes < expected.size {
        let remaining = expected.size - downloaded_bytes;
        let requested = remaining.min(u64::from(DOWNLOAD_CHUNK_BYTES)) as u32;
        let response = await_step(
            cancellation,
            deadline,
            REQUEST_TIMEOUT,
            "reading remote file",
            session.read(handle.to_owned(), downloaded_bytes, requested),
        )
        .await?;
        let data = match response {
            Ok(data) => data.data,
            Err(russh_sftp::client::error::Error::Status(status))
                if status.status_code == StatusCode::Eof =>
            {
                anyhow::bail!(
                    "remote file ended after {downloaded_bytes} of {} bytes",
                    expected.size
                );
            }
            Err(error) => return Err(error).context("cannot read remote file"),
        };
        if data.is_empty() {
            anyhow::bail!(
                "remote file returned an empty data packet after {downloaded_bytes} of {} bytes",
                expected.size
            );
        }
        if data.len() > requested as usize || data.len() as u64 > remaining {
            anyhow::bail!("remote file returned more bytes than requested");
        }

        let chunk_bytes = data.len() as u64;
        await_step(
            cancellation,
            deadline,
            REQUEST_TIMEOUT,
            "writing local cache",
            chunk_tx.send(data),
        )
        .await?
        .map_err(|_| CacheWriterStopped)?;
        downloaded_bytes += chunk_bytes;

        if downloaded_bytes >= next_progress || downloaded_bytes == expected.size {
            send_progress(
                event_tx,
                SftpTransferEvent::Progress {
                    transfer_id,
                    downloaded_bytes,
                    total_bytes: expected.size,
                },
            )?;
            next_progress = downloaded_bytes
                .saturating_add(PROGRESS_STEP_BYTES)
                .min(expected.size);
        }
    }

    // Reading one byte beyond the validated size catches growth even when a
    // server's later metadata response is stale.
    let eof = await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "confirming remote end of file",
        session.read(handle.to_owned(), expected.size, 1),
    )
    .await?;
    match eof {
        Err(russh_sftp::client::error::Error::Status(status))
            if status.status_code == StatusCode::Eof => {}
        Ok(data) if data.data.is_empty() => {}
        Ok(_) => anyhow::bail!("remote file grew while it was being downloaded"),
        Err(error) => return Err(error).context("cannot confirm remote end of file"),
    }

    let final_handle = await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "revalidating downloaded remote file",
        session.fstat(handle.to_owned()),
    )
    .await?
    .context("cannot revalidate downloaded remote file")?;
    ensure_same_remote_file(expected, validate_regular_metadata(&final_handle.attrs)?)?;
    let final_path = lstat_regular_file(session, remote_path, cancellation, deadline).await?;
    ensure_same_remote_file(expected, final_path)?;
    Ok(())
}

async fn lstat_regular_file(
    session: &RawSftpSession,
    path: &str,
    cancellation: &TransferCancellation,
    deadline: Instant,
) -> Result<RemoteFileMetadata> {
    let attrs = await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "inspecting remote file",
        session.lstat(path.to_owned()),
    )
    .await?
    .with_context(|| format!("cannot inspect remote file {path:?}"))?;
    validate_regular_metadata(&attrs.attrs)
}

async fn close_remote_handle(session: &RawSftpSession, handle: String) {
    match timeout(REQUEST_TIMEOUT, session.close(handle)).await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => debug!(%error, "failed to close SFTP download handle"),
        Err(_) => debug!("timed out closing SFTP download handle"),
    }
}

async fn prepare_cache_file(
    cache_dir: PathBuf,
    name: String,
    expected_bytes: u64,
    cancellation: &TransferCancellation,
    deadline: Instant,
) -> Result<PendingCacheFile> {
    let cache_id = Uuid::new_v4();
    let task = tokio::task::spawn_blocking(move || {
        ensure_cache_namespace(&cache_dir)?;
        let _quota = CACHE_QUOTA_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .map_err(|_| anyhow::anyhow!("SFTP cache quota lock is poisoned"))?;
        enforce_cache_quota(&cache_dir, expected_bytes, SystemTime::now())?;
        PendingCacheFile::create(
            CacheTarget::new(&cache_dir, cache_id, &name),
            expected_bytes,
        )
    });
    await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "preparing local cache",
        task,
    )
    .await?
    .context("SFTP cache preparation task failed")?
}

async fn await_writer_success(
    writer: &mut JoinHandle<Result<PathBuf>>,
    cancellation: &TransferCancellation,
    deadline: Instant,
) -> Result<PathBuf> {
    if cancellation.is_cancelled() {
        let _ = await_writer_cleanup(writer).await;
        return Err(cancelled_error());
    }

    tokio::select! {
        _ = cancellation.cancelled() => {
            if let Some(error) = await_writer_cleanup(writer).await
                && !is_cancelled(&error)
                && !is_incomplete_cache_write(&error)
            {
                debug!(%error, "cache writer also failed while cancelling SFTP transfer");
            }
            Err(cancelled_error())
        }
        result = timeout_at(deadline, &mut *writer) => {
            match result {
                Ok(joined) => flatten_writer_result(joined),
                Err(_) => {
                    cancellation.cancel();
                    let _ = await_writer_cleanup(writer).await;
                    anyhow::bail!("SFTP download exceeded the {}-second overall timeout", DOWNLOAD_TIMEOUT.as_secs())
                }
            }
        }
    }
}

async fn await_writer_cleanup(writer: &mut JoinHandle<Result<PathBuf>>) -> Option<anyhow::Error> {
    match timeout(WRITER_CLEANUP_TIMEOUT, &mut *writer).await {
        Ok(Ok(Ok(path))) => {
            remove_cache_file_best_effort(path, "transfer failed after cache publication").await;
            None
        }
        Ok(Ok(Err(error))) => Some(error),
        Ok(Err(error)) => Some(anyhow::Error::from(error).context("SFTP cache writer task failed")),
        Err(_) => {
            debug!("timed out waiting for SFTP cache writer cleanup");
            None
        }
    }
}

async fn remove_cache_file_best_effort(path: PathBuf, reason: &'static str) {
    let display_path = path.clone();
    match tokio::task::spawn_blocking(move || fs::remove_file(path)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) if error.kind() == io::ErrorKind::NotFound => {}
        Ok(Err(error)) => {
            debug!(path = %display_path.display(), %error, reason, "failed to remove SFTP cache file");
        }
        Err(error) => {
            debug!(path = %display_path.display(), %error, reason, "failed to join SFTP cache cleanup task");
        }
    }
}

fn flatten_writer_result(
    result: std::result::Result<Result<PathBuf>, JoinError>,
) -> Result<PathBuf> {
    result.context("SFTP cache writer task failed")?
}

fn send_progress(
    event_tx: &mpsc::Sender<SftpTransferEvent>,
    event: SftpTransferEvent,
) -> Result<()> {
    match event_tx.try_send(event) {
        Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => Ok(()),
        Err(mpsc::error::TrySendError::Closed(_)) => {
            anyhow::bail!("SFTP transfer event receiver dropped")
        }
    }
}

async fn await_step<T, F>(
    cancellation: &TransferCancellation,
    overall_deadline: Instant,
    step_timeout: Duration,
    operation: &'static str,
    future: F,
) -> Result<T>
where
    F: Future<Output = T>,
{
    if cancellation.is_cancelled() {
        return Err(cancelled_error());
    }
    let now = Instant::now();
    if now >= overall_deadline {
        anyhow::bail!(
            "SFTP download exceeded the {}-second overall timeout",
            DOWNLOAD_TIMEOUT.as_secs()
        );
    }
    let step_deadline = (now + step_timeout).min(overall_deadline);
    tokio::select! {
        _ = cancellation.cancelled() => Err(cancelled_error()),
        result = timeout_at(step_deadline, future) => match result {
            Ok(result) => Ok(result),
            Err(_) if step_deadline == overall_deadline => anyhow::bail!(
                "SFTP download exceeded the {}-second overall timeout",
                DOWNLOAD_TIMEOUT.as_secs()
            ),
            Err(_) => anyhow::bail!("{operation} timed out"),
        },
    }
}

#[derive(Clone)]
struct TransferCancellation {
    inner: Arc<TransferCancellationInner>,
}

struct TransferCancellationInner {
    cancelled: AtomicBool,
    notify: Notify,
}

impl TransferCancellation {
    fn new() -> Self {
        Self {
            inner: Arc::new(TransferCancellationInner {
                cancelled: AtomicBool::new(false),
                notify: Notify::new(),
            }),
        }
    }

    fn cancel(&self) {
        if !self.inner.cancelled.swap(true, Ordering::AcqRel) {
            self.inner.notify.notify_waiters();
        }
    }

    fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Acquire)
    }

    async fn cancelled(&self) {
        loop {
            let notified = self.inner.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

#[derive(Debug)]
struct TransferCancelled;

impl fmt::Display for TransferCancelled {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SFTP download cancelled")
    }
}

impl StdError for TransferCancelled {}

fn cancelled_error() -> anyhow::Error {
    TransferCancelled.into()
}

fn is_cancelled(error: &anyhow::Error) -> bool {
    error.downcast_ref::<TransferCancelled>().is_some()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RemoteFileMetadata {
    size: u64,
    modified: Option<u32>,
}

fn validate_regular_metadata(attrs: &FileAttributes) -> Result<RemoteFileMetadata> {
    if attrs.is_symlink() {
        anyhow::bail!("remote symbolic links cannot be downloaded for local opening");
    }
    if attrs.is_dir() {
        anyhow::bail!("remote directories cannot be downloaded for local opening");
    }
    if !attrs.is_regular() {
        anyhow::bail!("remote path is not a verified regular file");
    }
    let size = attrs
        .size
        .context("remote regular file did not report its size")?;
    if size > MAX_DOWNLOAD_BYTES {
        anyhow::bail!(
            "remote file is {size} bytes, exceeding the {MAX_DOWNLOAD_BYTES}-byte download limit"
        );
    }
    Ok(RemoteFileMetadata {
        size,
        modified: attrs.mtime,
    })
}

fn ensure_same_remote_file(expected: RemoteFileMetadata, actual: RemoteFileMetadata) -> Result<()> {
    if expected.size != actual.size {
        anyhow::bail!("remote file size changed while preparing the download");
    }
    if let (Some(expected), Some(actual)) = (expected.modified, actual.modified)
        && expected != actual
    {
        anyhow::bail!("remote file modification time changed while preparing the download");
    }
    Ok(())
}

fn validate_download_path(path: &str) -> Result<&str> {
    validate_remote_path(path)?;
    let name = path.rsplit('/').next().unwrap_or_default();
    if name.is_empty() || name == "." || name == ".." {
        anyhow::bail!("remote download path must name a file");
    }
    if name.chars().count() > MAX_NAME_CHARS {
        anyhow::bail!("remote file name cannot exceed {MAX_NAME_CHARS} characters");
    }
    Ok(name)
}

#[derive(Debug)]
struct CacheTarget {
    directory: PathBuf,
    part_path: PathBuf,
    final_path: PathBuf,
}

impl CacheTarget {
    fn new(directory: &Path, cache_id: Uuid, remote_name: &str) -> Self {
        let basename = sanitize_basename(remote_name);
        let final_name = format!("{cache_id}-{basename}");
        let part_name = format!(".{final_name}.part");
        Self {
            directory: directory.to_owned(),
            part_path: directory.join(part_name),
            final_path: directory.join(final_name),
        }
    }
}

struct PendingCacheFile {
    file: Option<LocalFile>,
    target: CacheTarget,
    cleanup_path: Option<PathBuf>,
}

impl PendingCacheFile {
    fn create(target: CacheTarget, reserved_bytes: u64) -> Result<Self> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&target.part_path)
            .with_context(|| format!("cannot create private cache file {:?}", target.part_path))?;
        file.set_len(reserved_bytes)
            .context("cannot reserve private SFTP cache space")?;
        let cleanup_path = Some(target.part_path.clone());
        Ok(Self {
            file: Some(file),
            target,
            cleanup_path,
        })
    }

    fn file_mut(&mut self) -> Result<&mut LocalFile> {
        self.file
            .as_mut()
            .context("private cache file was already finalized")
    }

    fn finish(mut self) -> Result<PathBuf> {
        let mut file = self
            .file
            .take()
            .context("private cache file was already finalized")?;
        file.flush().context("cannot flush private cache file")?;
        file.sync_all().context("cannot sync private cache file")?;
        drop(file);
        if self.target.final_path.exists() {
            anyhow::bail!("private cache target unexpectedly already exists");
        }
        fs::rename(&self.target.part_path, &self.target.final_path)
            .context("cannot atomically publish private cache file")?;
        self.cleanup_path = Some(self.target.final_path.clone());
        sync_cache_directory(&self.target.directory)?;
        self.cleanup_path = None;
        Ok(self.target.final_path.clone())
    }
}

impl Drop for PendingCacheFile {
    fn drop(&mut self) {
        if let Some(path) = self.cleanup_path.take()
            && let Err(error) = fs::remove_file(&path)
            && error.kind() != io::ErrorKind::NotFound
        {
            debug!(path = %path.display(), %error, "failed to clean partial SFTP cache file");
        }
    }
}

fn write_cache_file(
    mut pending: PendingCacheFile,
    mut chunks: mpsc::Receiver<Vec<u8>>,
    expected_bytes: u64,
    cancellation: &TransferCancellation,
) -> Result<PathBuf> {
    let mut written = 0_u64;
    while let Some(chunk) = chunks.blocking_recv() {
        if cancellation.is_cancelled() {
            return Err(cancelled_error());
        }
        written = written
            .checked_add(chunk.len() as u64)
            .context("local cache byte count overflowed")?;
        if written > expected_bytes {
            anyhow::bail!("local cache writer received more bytes than expected");
        }
        pending
            .file_mut()?
            .write_all(&chunk)
            .context("cannot write private SFTP cache file")?;
    }
    if cancellation.is_cancelled() {
        return Err(cancelled_error());
    }
    if written != expected_bytes {
        return Err(IncompleteCacheWrite {
            expected: expected_bytes,
            actual: written,
        }
        .into());
    }
    pending.finish()
}

#[derive(Debug)]
struct CacheWriterStopped;

impl fmt::Display for CacheWriterStopped {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("local cache writer stopped before download completed")
    }
}

impl StdError for CacheWriterStopped {}

#[derive(Debug)]
struct IncompleteCacheWrite {
    expected: u64,
    actual: u64,
}

impl fmt::Display for IncompleteCacheWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "private cache write ended after {} of {} bytes",
            self.actual, self.expected
        )
    }
}

impl StdError for IncompleteCacheWrite {}

fn is_incomplete_cache_write(error: &anyhow::Error) -> bool {
    error.downcast_ref::<IncompleteCacheWrite>().is_some()
}

fn cache_namespace() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "axsoft", "ax_ssh")
        .context("cannot determine the AxSSH cache directory")?;
    Ok(dirs.cache_dir().join("sftp-open"))
}

fn ensure_cache_namespace(path: &Path) -> Result<()> {
    fs::create_dir_all(path)
        .with_context(|| format!("cannot create SFTP cache directory {path:?}"))?;
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("cannot inspect SFTP cache directory {path:?}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        anyhow::bail!("SFTP cache namespace is not a private directory");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .context("cannot enforce private SFTP cache directory permissions")?;
    }
    Ok(())
}

fn sync_cache_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        LocalFile::open(path)
            .context("cannot open SFTP cache directory for sync")?
            .sync_all()
            .context("cannot sync SFTP cache directory")?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[derive(Debug)]
struct ManagedCacheEntry {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
    is_part: bool,
}

fn enforce_cache_quota(cache_dir: &Path, incoming_bytes: u64, now: SystemTime) -> Result<()> {
    if incoming_bytes > MAX_CACHE_BYTES {
        anyhow::bail!(
            "SFTP cache reservation is {incoming_bytes} bytes, exceeding the {MAX_CACHE_BYTES}-byte quota"
        );
    }

    cleanup_stale_cache_at(cache_dir, now)?;
    let mut entries = scan_managed_cache_entries(cache_dir)?;
    let mut total_bytes = entries
        .iter()
        .fold(0_u64, |total, entry| total.saturating_add(entry.size));
    let mut file_count = entries.len();

    entries.retain(|entry| !entry.is_part);
    entries.sort_by_key(|entry| entry.modified);
    for entry in entries {
        let within_bytes = total_bytes.saturating_add(incoming_bytes) <= MAX_CACHE_BYTES;
        let within_files = file_count.saturating_add(1) <= MAX_CACHE_FILES;
        if within_bytes && within_files {
            break;
        }
        if fs::remove_file(&entry.path).is_ok() {
            total_bytes = total_bytes.saturating_sub(entry.size);
            file_count = file_count.saturating_sub(1);
        }
    }

    if total_bytes.saturating_add(incoming_bytes) > MAX_CACHE_BYTES {
        anyhow::bail!(
            "SFTP cache quota would exceed {MAX_CACHE_BYTES} bytes; close an existing opened file and retry"
        );
    }
    if file_count.saturating_add(1) > MAX_CACHE_FILES {
        anyhow::bail!(
            "SFTP cache quota allows at most {MAX_CACHE_FILES} files; close an existing opened file and retry"
        );
    }
    Ok(())
}

fn scan_managed_cache_entries(cache_dir: &Path) -> Result<Vec<ManagedCacheEntry>> {
    let mut entries = Vec::new();
    let mut directory = fs::read_dir(cache_dir)
        .with_context(|| format!("cannot read SFTP cache directory {cache_dir:?}"))?;
    for entry in directory.by_ref().take(MAX_CACHE_SCAN_ENTRIES) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                debug!(%error, "cannot inspect an SFTP cache entry while enforcing quota");
                continue;
            }
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(is_part) = managed_cache_name(name) else {
            continue;
        };
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(error) => {
                debug!(path = %entry.path().display(), %error, "cannot inspect an SFTP cache entry while enforcing quota");
                continue;
            }
        };
        if metadata.is_dir() || metadata.file_type().is_symlink() {
            continue;
        }
        entries.push(ManagedCacheEntry {
            path: entry.path(),
            size: metadata.len(),
            modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            is_part,
        });
    }
    if directory.next().is_some() {
        anyhow::bail!(
            "SFTP cache directory exceeds the {MAX_CACHE_SCAN_ENTRIES}-entry scan budget"
        );
    }
    Ok(entries)
}

fn sanitize_basename(name: &str) -> String {
    let mut sanitized = String::with_capacity(name.len().min(MAX_CACHE_BASENAME_BYTES));
    for character in name.chars() {
        let safe = !character.is_control()
            && !matches!(
                character,
                '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*'
            );
        sanitized.push(if safe { character } else { '_' });
    }
    let sanitized = sanitized.trim_matches(|character| character == ' ' || character == '.');
    let sanitized = if sanitized.is_empty() {
        "remote-file"
    } else {
        sanitized
    };
    truncate_basename(sanitized, MAX_CACHE_BASENAME_BYTES)
}

fn truncate_basename(name: &str, max_bytes: usize) -> String {
    if name.len() <= max_bytes {
        return name.to_owned();
    }
    let split = name.rsplit_once('.').filter(|(stem, extension)| {
        !stem.is_empty() && !extension.is_empty() && extension.len() <= 32
    });
    let extension_bytes = split.map_or(0, |(_, extension)| extension.len() + 1);
    let stem_budget = max_bytes.saturating_sub(extension_bytes);
    let stem = truncate_utf8(split.map_or(name, |(stem, _)| stem), stem_budget);
    match split.map(|(_, extension)| extension) {
        Some(extension) if !stem.is_empty() => format!("{stem}.{extension}"),
        _ => truncate_utf8(name, max_bytes).to_owned(),
    }
}

fn truncate_utf8(value: &str, max_bytes: usize) -> &str {
    let mut end = value.len().min(max_bytes);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn cleanup_stale_cache_at(cache_dir: &Path, now: SystemTime) -> Result<usize> {
    let mut removed = 0;
    for entry in fs::read_dir(cache_dir)
        .with_context(|| format!("cannot read SFTP cache directory {cache_dir:?}"))?
        .take(MAX_CACHE_SCAN_ENTRIES)
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                debug!(%error, "cannot inspect an SFTP cache entry");
                continue;
            }
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(is_part) = managed_cache_name(name) else {
            continue;
        };
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(error) => {
                debug!(path = %entry.path().display(), %error, "cannot inspect an SFTP cache entry");
                continue;
            }
        };
        if metadata.is_dir() {
            continue;
        }
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if !cache_entry_is_stale(modified, now, is_part) {
            continue;
        }
        match fs::remove_file(entry.path()) {
            Ok(()) => removed += 1,
            Err(error) => debug!(
                path = %entry.path().display(),
                %error,
                "stale SFTP cache file remains in use or could not be removed"
            ),
        }
    }
    Ok(removed)
}

fn managed_cache_name(name: &str) -> Option<bool> {
    let (candidate, is_part) = match name
        .strip_prefix('.')
        .and_then(|name| name.strip_suffix(".part"))
    {
        Some(candidate) => (candidate, true),
        None => (name, false),
    };
    let (id, basename) = candidate.split_at_checked(36)?;
    let basename = basename.strip_prefix('-')?;
    if basename.is_empty() || Uuid::parse_str(id).is_err() {
        return None;
    }
    Some(is_part)
}

fn cache_entry_is_stale(modified: SystemTime, now: SystemTime, is_part: bool) -> bool {
    let stale_after = if is_part {
        PART_STALE_AFTER
    } else {
        CACHE_STALE_AFTER
    };
    now.duration_since(modified)
        .is_ok_and(|age| age >= stale_after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use russh_sftp::protocol::{Attrs, Data, Handle as RemoteHandle, Status};

    struct TestCacheDir(PathBuf);

    impl TestCacheDir {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("axssh-sftp-cache-{}", Uuid::new_v4())))
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestCacheDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct DownloadTestServer {
        content: Arc<Vec<u8>>,
        reported_size: u64,
        reads: Arc<Mutex<Vec<(u64, u32)>>>,
    }

    impl DownloadTestServer {
        fn attrs(&self, id: u32) -> Attrs {
            let mut attrs = FileAttributes::empty();
            attrs.set_regular(true);
            attrs.size = Some(self.reported_size);
            attrs.mtime = Some(1234);
            Attrs { id, attrs }
        }
    }

    impl russh_sftp::server::Handler for DownloadTestServer {
        type Error = StatusCode;

        fn unimplemented(&self) -> Self::Error {
            StatusCode::OpUnsupported
        }

        async fn open(
            &mut self,
            id: u32,
            filename: String,
            flags: OpenFlags,
            _attrs: FileAttributes,
        ) -> std::result::Result<RemoteHandle, Self::Error> {
            if filename != "/srv/report.bin" || !flags.contains(OpenFlags::READ) {
                return Err(StatusCode::PermissionDenied);
            }
            Ok(RemoteHandle {
                id,
                handle: "test-download".to_owned(),
            })
        }

        async fn close(
            &mut self,
            id: u32,
            handle: String,
        ) -> std::result::Result<Status, Self::Error> {
            if handle != "test-download" {
                return Err(StatusCode::BadMessage);
            }
            Ok(Status {
                id,
                status_code: StatusCode::Ok,
                error_message: "Ok".to_owned(),
                language_tag: "en-US".to_owned(),
            })
        }

        async fn read(
            &mut self,
            id: u32,
            handle: String,
            offset: u64,
            len: u32,
        ) -> std::result::Result<Data, Self::Error> {
            if handle != "test-download" {
                return Err(StatusCode::BadMessage);
            }
            self.reads
                .lock()
                .expect("read observations should lock")
                .push((offset, len));
            let start = usize::try_from(offset).map_err(|_| StatusCode::BadMessage)?;
            if start >= self.content.len() {
                return Err(StatusCode::Eof);
            }
            let end = start.saturating_add(len as usize).min(self.content.len());
            Ok(Data {
                id,
                data: self.content[start..end].to_vec(),
            })
        }

        async fn lstat(
            &mut self,
            id: u32,
            path: String,
        ) -> std::result::Result<Attrs, Self::Error> {
            if path != "/srv/report.bin" {
                return Err(StatusCode::NoSuchFile);
            }
            Ok(self.attrs(id))
        }

        async fn fstat(
            &mut self,
            id: u32,
            handle: String,
        ) -> std::result::Result<Attrs, Self::Error> {
            if handle != "test-download" {
                return Err(StatusCode::BadMessage);
            }
            Ok(self.attrs(id))
        }
    }

    #[test]
    fn download_paths_require_a_bounded_file_name() {
        assert_eq!(
            validate_download_path("/srv/report.txt").expect("file path should validate"),
            "report.txt"
        );
        assert!(validate_download_path("/srv/").is_err());
        assert!(validate_download_path("..").is_err());
        assert!(validate_download_path("/srv/line\nbreak").is_err());
        assert!(
            validate_download_path(&format!("/srv/{}", "a".repeat(MAX_NAME_CHARS + 1))).is_err()
        );
    }

    #[test]
    fn cache_basename_is_portable_and_preserves_extension() {
        assert_eq!(sanitize_basename("../bad\\name?.pdf"), "_bad_name_.pdf");
        assert_eq!(sanitize_basename("..."), "remote-file");
        assert_eq!(sanitize_basename(" report.txt. "), "report.txt");

        let long = format!("{}.json", "界".repeat(100));
        let sanitized = sanitize_basename(&long);
        assert!(sanitized.len() <= MAX_CACHE_BASENAME_BYTES);
        assert!(sanitized.ends_with(".json"));
        assert!(sanitized.is_char_boundary(sanitized.len()));
    }

    #[test]
    fn cache_target_stays_directly_inside_namespace() {
        let root = Path::new("/private/cache/sftp-open");
        let id = Uuid::parse_str("12345678-1234-4234-8234-123456789abc")
            .expect("fixture UUID should parse");
        let target = CacheTarget::new(root, id, "../../report?.txt");

        assert_eq!(target.final_path.parent(), Some(root));
        assert_eq!(target.part_path.parent(), Some(root));
        assert_eq!(
            target.final_path.file_name().and_then(|name| name.to_str()),
            Some("12345678-1234-4234-8234-123456789abc-_.._report_.txt")
        );
        assert_eq!(
            managed_cache_name("12345678-1234-4234-8234-123456789abc-report.txt"),
            Some(false)
        );
        assert_eq!(
            managed_cache_name(".12345678-1234-4234-8234-123456789abc-report.txt.part"),
            Some(true)
        );
        assert_eq!(managed_cache_name("notes.txt"), None);
    }

    #[test]
    fn metadata_rejects_links_directories_unknown_types_and_oversized_files() {
        let mut regular = FileAttributes::empty();
        regular.set_regular(true);
        regular.size = Some(MAX_DOWNLOAD_BYTES);
        assert_eq!(
            validate_regular_metadata(&regular)
                .expect("bounded regular file should pass")
                .size,
            MAX_DOWNLOAD_BYTES
        );

        let mut symlink = FileAttributes::empty();
        symlink.set_symlink(true);
        symlink.size = Some(1);
        assert!(validate_regular_metadata(&symlink).is_err());

        let mut directory = FileAttributes::empty();
        directory.set_dir(true);
        directory.size = Some(1);
        assert!(validate_regular_metadata(&directory).is_err());

        let mut unknown = FileAttributes::empty();
        unknown.size = Some(1);
        assert!(validate_regular_metadata(&unknown).is_err());

        regular.size = Some(MAX_DOWNLOAD_BYTES + 1);
        assert!(validate_regular_metadata(&regular).is_err());
    }

    #[tokio::test]
    async fn loopback_download_reads_bounded_chunks_and_atomically_publishes() {
        let content = Arc::new(
            (0..150_000)
                .map(|index| (index % 251) as u8)
                .collect::<Vec<_>>(),
        );
        let reads = Arc::new(Mutex::new(Vec::new()));
        let server = DownloadTestServer {
            content: content.clone(),
            reported_size: content.len() as u64,
            reads: reads.clone(),
        };
        let (client_stream, server_stream) = tokio::io::duplex(512 * 1024);
        russh_sftp::server::run(server_stream, server).await;
        let test_root = TestCacheDir::new();
        let request = SftpDownloadRequest::new(
            Uuid::parse_str("12345678-1234-4234-8234-123456789abc")
                .expect("fixture UUID should parse"),
            "/srv/report.bin".to_owned(),
        )
        .expect("download request should validate");
        let cancellation = TransferCancellation::new();
        let (event_tx, mut event_rx) = mpsc::channel(SFTP_TRANSFER_EVENT_CAPACITY);

        let (local_path, total_bytes) = download_to_cache(
            client_stream,
            &request,
            &cancellation,
            &event_tx,
            test_root.path(),
        )
        .await
        .expect("loopback download should complete");

        assert_eq!(total_bytes, content.len() as u64);
        assert_eq!(
            fs::read(&local_path).expect("downloaded cache file should read"),
            *content
        );
        assert!(
            fs::read_dir(test_root.path())
                .expect("cache directory should read")
                .all(|entry| !entry
                    .expect("cache entry should read")
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".part"))
        );
        let observed = reads.lock().expect("read observations should lock");
        assert_eq!(
            observed.as_slice(),
            &[
                (0, DOWNLOAD_CHUNK_BYTES),
                (u64::from(DOWNLOAD_CHUNK_BYTES), DOWNLOAD_CHUNK_BYTES),
                (2 * u64::from(DOWNLOAD_CHUNK_BYTES), 18_928),
                (150_000, 1),
            ]
        );
        drop(observed);
        assert!(matches!(
            event_rx.try_recv(),
            Ok(SftpTransferEvent::Started {
                total_bytes: 150_000,
                ..
            })
        ));
        assert!(matches!(
            event_rx.try_recv(),
            Ok(SftpTransferEvent::Progress {
                downloaded_bytes: 150_000,
                total_bytes: 150_000,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn truncated_loopback_download_never_publishes_a_cache_file() {
        let content = Arc::new(b"short".to_vec());
        let server = DownloadTestServer {
            content,
            reported_size: 10,
            reads: Arc::new(Mutex::new(Vec::new())),
        };
        let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        russh_sftp::server::run(server_stream, server).await;
        let test_root = TestCacheDir::new();
        let request = SftpDownloadRequest::new(Uuid::new_v4(), "/srv/report.bin".to_owned())
            .expect("download request should validate");
        let cancellation = TransferCancellation::new();
        let (event_tx, _event_rx) = mpsc::channel(SFTP_TRANSFER_EVENT_CAPACITY);

        let error = download_to_cache(
            client_stream,
            &request,
            &cancellation,
            &event_tx,
            test_root.path(),
        )
        .await
        .expect_err("truncated download should fail");

        assert!(error.to_string().contains("ended after 5 of 10 bytes"));
        assert_eq!(
            fs::read_dir(test_root.path())
                .expect("cache directory should read")
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_pending_step() {
        let cancellation = TransferCancellation::new();
        let trigger = cancellation.clone();
        let cancel_task = tokio::spawn(async move {
            tokio::task::yield_now().await;
            trigger.cancel();
        });
        let result = await_step(
            &cancellation,
            Instant::now() + Duration::from_secs(1),
            Duration::from_secs(1),
            "pending test operation",
            std::future::pending::<()>(),
        )
        .await;

        let error = result.expect_err("cancelled step should fail");
        assert!(is_cancelled(&error));
        cancel_task.await.expect("cancel task should finish");
    }

    #[tokio::test(start_paused = true)]
    async fn pending_protocol_step_obeys_its_timeout() {
        let result = await_step(
            &TransferCancellation::new(),
            Instant::now() + Duration::from_secs(60),
            Duration::from_secs(2),
            "test protocol request",
            std::future::pending::<()>(),
        )
        .await;

        assert_eq!(
            result
                .expect_err("pending step should time out")
                .to_string(),
            "test protocol request timed out"
        );
    }

    #[test]
    fn stale_policy_distinguishes_partial_and_completed_files() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(2 * 24 * 60 * 60);
        let two_hours_old = now - Duration::from_secs(2 * 60 * 60);
        assert!(cache_entry_is_stale(two_hours_old, now, true));
        assert!(!cache_entry_is_stale(two_hours_old, now, false));
    }

    #[test]
    fn cache_writer_publishes_only_a_complete_file() {
        let test_root = TestCacheDir::new();
        ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
        let target = CacheTarget::new(test_root.path(), Uuid::new_v4(), "notes.txt");
        let part_path = target.part_path.clone();
        let pending = PendingCacheFile::create(target, 7).expect("private part file should open");
        let (chunk_tx, chunk_rx) = mpsc::channel(WRITER_QUEUE_CAPACITY);
        chunk_tx
            .blocking_send(b"bounded".to_vec())
            .expect("test chunk should queue");
        drop(chunk_tx);

        let path = write_cache_file(pending, chunk_rx, 7, &TransferCancellation::new())
            .expect("complete cache file should publish");

        assert_eq!(
            fs::read(&path).expect("published file should read"),
            b"bounded"
        );
        assert!(!part_path.exists());
    }

    #[test]
    fn cancelled_cache_writer_removes_its_partial_file() {
        let test_root = TestCacheDir::new();
        ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
        let target = CacheTarget::new(test_root.path(), Uuid::new_v4(), "notes.txt");
        let part_path = target.part_path.clone();
        let pending = PendingCacheFile::create(target, 1).expect("private part file should open");
        let (chunk_tx, chunk_rx) = mpsc::channel(WRITER_QUEUE_CAPACITY);
        drop(chunk_tx);
        let cancellation = TransferCancellation::new();
        cancellation.cancel();

        let error = write_cache_file(pending, chunk_rx, 1, &cancellation)
            .expect_err("cancelled cache write should fail");

        assert!(is_cancelled(&error));
        assert!(!part_path.exists());
    }

    #[test]
    fn stale_cleanup_leaves_unrecognized_files_and_directories() {
        let test_root = TestCacheDir::new();
        ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
        let managed = test_root
            .path()
            .join(format!("{}-notes.txt", Uuid::new_v4()));
        let unrecognized = test_root.path().join("user-notes.txt");
        let directory = test_root
            .path()
            .join(format!("{}-directory", Uuid::new_v4()));
        fs::write(&managed, b"managed").expect("managed fixture should write");
        fs::write(&unrecognized, b"unrecognized").expect("unrecognized fixture should write");
        fs::create_dir(&directory).expect("directory fixture should create");
        let future = SystemTime::now() + CACHE_STALE_AFTER + Duration::from_secs(1);

        let removed =
            cleanup_stale_cache_at(test_root.path(), future).expect("stale cleanup should finish");

        assert_eq!(removed, 1);
        assert!(!managed.exists());
        assert!(unrecognized.exists());
        assert!(directory.exists());
    }

    #[test]
    fn stale_cleanup_stops_after_the_bounded_scan_budget() {
        let test_root = TestCacheDir::new();
        ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
        let id = Uuid::new_v4();
        for index in 0..=MAX_CACHE_SCAN_ENTRIES {
            fs::write(
                test_root.path().join(format!("{id}-entry-{index}")),
                b"stale",
            )
            .expect("managed cache fixture should write");
        }

        let future = SystemTime::now() + CACHE_STALE_AFTER + Duration::from_secs(1);
        let removed = cleanup_stale_cache_at(test_root.path(), future)
            .expect("bounded cleanup should finish");

        assert_eq!(removed, MAX_CACHE_SCAN_ENTRIES);
        assert_eq!(
            fs::read_dir(test_root.path())
                .expect("cache directory should read")
                .count(),
            1
        );
    }

    #[test]
    fn cache_quota_evicts_completed_files_before_rejecting_a_new_reservation() {
        let test_root = TestCacheDir::new();
        ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
        let target = CacheTarget::new(test_root.path(), Uuid::new_v4(), "old.bin");
        let file = LocalFile::create(&target.final_path).expect("completed fixture should create");
        file.set_len(MAX_CACHE_BYTES)
            .expect("completed fixture should reserve quota");

        enforce_cache_quota(test_root.path(), 1, SystemTime::now())
            .expect("old completed file should be evicted");

        assert!(!target.final_path.exists());
    }

    #[test]
    fn cache_quota_never_evicts_an_active_partial_file() {
        let test_root = TestCacheDir::new();
        ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
        let target = CacheTarget::new(test_root.path(), Uuid::new_v4(), "active.bin");
        let file = LocalFile::create(&target.part_path).expect("partial fixture should create");
        file.set_len(MAX_CACHE_BYTES)
            .expect("partial fixture should reserve quota");

        let error = enforce_cache_quota(test_root.path(), 1, SystemTime::now())
            .expect_err("active partial file should block an over-quota reservation");

        assert!(error.to_string().contains("quota"));
        assert!(target.part_path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn cache_files_and_namespace_use_private_unix_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let test_root = TestCacheDir::new();
        ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
        let target = CacheTarget::new(test_root.path(), Uuid::new_v4(), "notes.txt");
        let pending = PendingCacheFile::create(target, 0).expect("private part file should open");
        let directory_mode = fs::metadata(test_root.path())
            .expect("namespace metadata should exist")
            .permissions()
            .mode()
            & 0o777;
        let file_mode = fs::metadata(&pending.target.part_path)
            .expect("part metadata should exist")
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(directory_mode, 0o700);
        assert_eq!(file_mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn cache_namespace_rejects_a_symbolic_link() {
        use std::os::unix::fs::symlink;

        let test_root = TestCacheDir::new();
        let real_directory = test_root.path().join("real");
        let linked_namespace = test_root.path().join("sftp-open");
        fs::create_dir_all(&real_directory).expect("real cache fixture should create");
        symlink(&real_directory, &linked_namespace).expect("cache symlink fixture should create");

        let error = ensure_cache_namespace(&linked_namespace)
            .expect_err("cache namespace symlink should be rejected");

        assert!(error.to_string().contains("not a private directory"));
    }
}
