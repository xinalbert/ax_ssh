//! Bounded read-only SFTP downloads into AxSSH's private open-file cache.

mod cache;

use self::cache::*;

use std::error::Error as StdError;
use std::fmt;
use std::fs::{self, File as LocalFile, OpenOptions};
use std::future::Future;
use std::io::{self, Read, Write};
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
/// Call this explicitly when maintenance is requested. Download quota enforcement
/// performs the same stale-file cleanup before creating a new cache target, so an
/// application that never uses remote file opening does not need to scan this
/// directory at startup. Directories and unrecognized names are left untouched.
pub async fn cleanup_stale_sftp_open_cache() -> Result<usize> {
    cleanup_stale_sftp_open_cache_impl().await
}

/// Copies one already-validated local file handle into the private open cache.
///
/// The caller must validate the opened handle against the directory snapshot
/// before passing ownership here. Copying from the handle, rather than
/// reopening its path, fixes the file identity across the validation/open
/// boundary and prevents a later path replacement from changing the target.
pub fn snapshot_local_file_for_open(mut source: LocalFile, name: &str) -> Result<PathBuf> {
    snapshot_local_file_for_open_at(&mut source, name, &cache_namespace()?)
}

fn snapshot_local_file_for_open_at(
    source: &mut LocalFile,
    name: &str,
    cache_dir: &Path,
) -> Result<PathBuf> {
    let metadata = source
        .metadata()
        .context("cannot inspect validated local file")?;
    if !metadata.is_file() {
        anyhow::bail!("validated local file is no longer a regular file");
    }
    let expected_bytes = metadata.len();
    if expected_bytes > MAX_DOWNLOAD_BYTES {
        anyhow::bail!(
            "local file is {expected_bytes} bytes, exceeding the {MAX_DOWNLOAD_BYTES}-byte open limit"
        );
    }

    ensure_cache_namespace(cache_dir)?;
    let _quota = CACHE_QUOTA_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| anyhow::anyhow!("SFTP cache quota lock is poisoned"))?;
    enforce_cache_quota(cache_dir, expected_bytes, SystemTime::now())?;
    let mut pending = PendingCacheFile::create(
        CacheTarget::new(cache_dir, Uuid::new_v4(), name),
        expected_bytes,
    )?;
    let copied = io::copy(
        &mut Read::by_ref(source).take(expected_bytes.saturating_add(1)),
        pending.file_mut()?,
    )
    .context("cannot copy validated local file into the private open cache")?;
    if copied != expected_bytes {
        anyhow::bail!(
            "local file changed while creating its open snapshot: expected {expected_bytes} bytes, copied {copied}"
        );
    }
    pending.finish()
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

    #[test]
    fn local_open_snapshot_copies_from_the_validated_handle() {
        let test_root = TestCacheDir::new();
        let source_path = test_root.path().with_extension("source");
        fs::write(&source_path, b"validated contents").expect("source fixture should write");
        let mut source = LocalFile::open(&source_path).expect("source fixture should open");

        let snapshot = snapshot_local_file_for_open_at(&mut source, "notes.txt", test_root.path())
            .expect("validated handle should publish a private snapshot");

        assert_eq!(
            fs::read(&snapshot).expect("snapshot should remain readable"),
            b"validated contents"
        );
        assert_eq!(
            snapshot.parent(),
            Some(test_root.path()),
            "snapshot must stay directly inside the private namespace"
        );
        fs::remove_file(source_path).expect("source fixture should be removed");
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
