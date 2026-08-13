//! Bounded SFTP downloads with private-cache opening kept as a separate flow.

mod cache;
mod local;

use self::cache::*;
use self::local::*;

use std::error::Error as StdError;
use std::fmt;
use std::fs::{self, File as LocalFile, OpenOptions};
use std::future::Future;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use russh_sftp::client::{Config, RawSftpSession};
use russh_sftp::protocol::{File, FileAttributes, OpenFlags, StatusCode};
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
pub(crate) const MAX_RECURSIVE_DOWNLOAD_FILES: usize = 512;
pub(crate) const MAX_RECURSIVE_DOWNLOAD_DIRECTORIES: usize = 256;
pub(crate) const MAX_RECURSIVE_DOWNLOAD_DEPTH: usize = 16;
pub(crate) const MAX_RECURSIVE_DOWNLOAD_TEXT_BYTES: usize = 512 * 1024;
pub(crate) const MAX_RECURSIVE_DOWNLOAD_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_RECURSIVE_DOWNLOAD_ENTRIES: usize = 4_096;
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
    Queued {
        transfer_id: Uuid,
        remote_path: String,
        name: String,
        total_bytes: u64,
    },
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
    Paused {
        transfer_id: Uuid,
        downloaded_bytes: u64,
        total_bytes: u64,
    },
    Resumed {
        transfer_id: Uuid,
        downloaded_bytes: u64,
        total_bytes: u64,
    },
    DiscoveryFailed {
        transfer_id: Uuid,
        name: String,
        message: String,
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
pub(crate) struct SftpUploadRequest {
    transfer_id: Uuid,
    remote_path: String,
    name: String,
    data: Vec<u8>,
}

impl SftpUploadRequest {
    pub(crate) fn new(transfer_id: Uuid, remote_path: String, data: Vec<u8>) -> Result<Self> {
        validate_remote_path(&remote_path)?;
        if data.len() as u64 > super::MAX_UPLOAD_BYTES {
            anyhow::bail!(
                "upload content exceeds the {}-byte limit",
                super::MAX_UPLOAD_BYTES
            );
        }
        let name = remote_path
            .rsplit('/')
            .next()
            .filter(|name| !name.is_empty())
            .context("remote upload target is missing a file name")?;
        if name.starts_with('.')
            || name == "."
            || name == ".."
            || name.chars().any(char::is_control)
            || name.contains(['/', '\\'])
            || name.chars().count() > MAX_NAME_CHARS
        {
            anyhow::bail!("remote upload target name is invalid");
        }
        let name = name.to_owned();
        Ok(Self {
            transfer_id,
            remote_path,
            name,
            data,
        })
    }

    pub(crate) fn transfer_id(&self) -> Uuid {
        self.transfer_id
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn total_bytes(&self) -> u64 {
        self.data.len() as u64
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SftpDownloadRequest {
    transfer_id: Uuid,
    remote_path: String,
    name: String,
    total_bytes: u64,
    local_target: Option<LocalDownloadTarget>,
}

impl SftpDownloadRequest {
    #[cfg(test)]
    pub(crate) fn new(transfer_id: Uuid, remote_path: String) -> Result<Self> {
        let name = validate_download_path(&remote_path)?.to_owned();
        Ok(Self {
            transfer_id,
            remote_path,
            name,
            total_bytes: 0,
            local_target: None,
        })
    }

    pub(crate) fn for_local_download(
        transfer_id: Uuid,
        remote_path: String,
        local_directory: PathBuf,
        local_components: Vec<String>,
        total_bytes: u64,
    ) -> Result<Self> {
        validate_download_path(&remote_path)?;
        let local_target = LocalDownloadTarget::new(local_directory, local_components)?;
        let name = local_target.display_name();
        Ok(Self {
            transfer_id,
            remote_path,
            name,
            total_bytes,
            local_target: Some(local_target),
        })
    }

    pub(crate) fn transfer_id(&self) -> Uuid {
        self.transfer_id
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn remote_path(&self) -> &str {
        &self.remote_path
    }

    pub(crate) fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SftpDownloadRoot {
    transfer_id: Uuid,
    remote_path: String,
    local_directory: PathBuf,
    name: String,
}

impl SftpDownloadRoot {
    pub(crate) fn new(
        transfer_id: Uuid,
        remote_path: String,
        local_directory: PathBuf,
    ) -> Result<Self> {
        let name = validate_download_path(&remote_path)?.to_owned();
        if local_directory.as_os_str().is_empty() {
            anyhow::bail!("local download directory is empty");
        }
        Ok(Self {
            transfer_id,
            remote_path,
            local_directory,
            name,
        })
    }

    pub(crate) fn transfer_id(&self) -> Uuid {
        self.transfer_id
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

pub(crate) struct SftpDownloadHandle {
    transfer_id: Uuid,
    cancellation: TransferCancellation,
    task: JoinHandle<()>,
}

pub(crate) struct SftpUploadHandle {
    transfer_id: Uuid,
    cancellation: TransferCancellation,
    task: JoinHandle<()>,
}

impl SftpUploadHandle {
    pub(crate) fn spawn<S>(
        runtime: &Handle,
        stream: S,
        request: SftpUploadRequest,
        event_tx: mpsc::Sender<SftpTransferEvent>,
    ) -> Self
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let transfer_id = request.transfer_id;
        let cancellation = TransferCancellation::new();
        let task = runtime.spawn(run_upload(stream, request, cancellation.clone(), event_tx));
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

    pub(crate) fn pause(&self) {
        self.cancellation.pause();
    }

    pub(crate) fn resume(&self) {
        self.cancellation.resume();
    }

    pub(crate) async fn shutdown(mut self) -> Result<()> {
        if !self.task.is_finished() {
            self.cancellation.cancel();
        }
        match timeout(SHUTDOWN_TIMEOUT, &mut self.task).await {
            Ok(joined) => joined.context("SFTP upload task failed during shutdown"),
            Err(_) => {
                self.task.abort();
                match (&mut self.task).await {
                    Err(error) if error.is_cancelled() => Ok(()),
                    Err(error) => Err(error).context("failed to abort SFTP upload task"),
                    Ok(()) => Ok(()),
                }
            }
        }
    }
}

impl Drop for SftpUploadHandle {
    fn drop(&mut self) {
        if !self.task.is_finished() {
            self.cancellation.cancel();
            self.task.abort();
        }
    }
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

    pub(crate) fn pause(&self) {
        self.cancellation.pause();
    }

    pub(crate) fn resume(&self) {
        self.cancellation.resume();
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
    let writes_local_target = request.local_target.is_some();
    let result = if writes_local_target {
        download_to_local(stream, &request, &cancellation, &event_tx).await
    } else {
        match cache_namespace() {
            Ok(cache_dir) => {
                download_to_cache(stream, &request, &cancellation, &event_tx, &cache_dir).await
            }
            Err(error) => Err(error),
        }
    };
    let terminal_event = match result {
        Ok((local_path, _total_bytes)) if cancellation.is_cancelled() => {
            if writes_local_target {
                remove_local_download_best_effort(
                    local_path,
                    "transfer was cancelled after local publication",
                )
                .await;
            } else {
                remove_cache_file_best_effort(
                    local_path,
                    "transfer was cancelled after cache publication",
                )
                .await;
            }
            SftpTransferEvent::Cancelled { transfer_id }
        }
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
        if writes_local_target {
            remove_local_download_best_effort(path, "terminal transfer event was not delivered")
                .await;
        } else {
            remove_cache_file_best_effort(path, "terminal transfer event was not delivered").await;
        }
    }
}

async fn run_upload<S>(
    stream: S,
    request: SftpUploadRequest,
    cancellation: TransferCancellation,
    event_tx: mpsc::Sender<SftpTransferEvent>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let total_bytes = request.data.len() as u64;
    let event = SftpTransferEvent::Started {
        transfer_id: request.transfer_id,
        remote_path: request.remote_path.clone(),
        name: request.name.clone(),
        total_bytes,
    };
    if send_transfer_state(&event_tx, event).await.is_err() {
        return;
    }
    let result = upload_initialized(stream, &request, &cancellation, &event_tx).await;
    let terminal = match result {
        Ok(()) if cancellation.is_cancelled() => SftpTransferEvent::Cancelled {
            transfer_id: request.transfer_id,
        },
        Ok(()) => SftpTransferEvent::Completed {
            transfer_id: request.transfer_id,
            local_path: PathBuf::new(),
            total_bytes,
        },
        Err(error) if is_cancelled(&error) => SftpTransferEvent::Cancelled {
            transfer_id: request.transfer_id,
        },
        Err(error) => SftpTransferEvent::Failed {
            transfer_id: request.transfer_id,
            message: bounded_error(&error),
        },
    };
    let _ = send_transfer_state(&event_tx, terminal).await;
}

async fn upload_initialized<S>(
    stream: S,
    request: &SftpUploadRequest,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
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
    await_step(
        cancellation,
        Instant::now() + DOWNLOAD_TIMEOUT,
        REQUEST_TIMEOUT,
        "SFTP upload handshake",
        session.init(),
    )
    .await?
    .context("SFTP upload handshake failed")?;
    let parent = request
        .remote_path
        .strip_suffix(&request.name)
        .unwrap_or("")
        .trim_end_matches('/');
    let temporary = format!(
        "{parent}/.{}.axssh-upload-{}",
        request.name, request.transfer_id
    );
    let target_check =
        ensure_remote_target_absent_for_transfer(&session, &request.remote_path).await;
    if let Err(error) = target_check {
        let _ = session.close_session();
        return Err(error);
    }
    let handle = await_step(
        cancellation,
        Instant::now() + DOWNLOAD_TIMEOUT,
        REQUEST_TIMEOUT,
        "opening remote upload file",
        session.open(
            temporary.clone(),
            OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
            FileAttributes::default(),
        ),
    )
    .await?
    .context("cannot open remote upload file")?
    .handle;
    let mut offset = 0_u64;
    let mut next_progress = PROGRESS_STEP_BYTES.min(request.data.len() as u64);
    let result = async {
        while offset < request.data.len() as u64 {
            cancellation.wait_until_running().await?;
            let end = (offset as usize + DOWNLOAD_CHUNK_BYTES as usize).min(request.data.len());
            let chunk = request.data[offset as usize..end].to_vec();
            await_step(
                cancellation,
                Instant::now() + DOWNLOAD_TIMEOUT,
                REQUEST_TIMEOUT,
                "writing remote upload",
                session.write(handle.clone(), offset, chunk),
            )
            .await?
            .context("cannot write remote upload")?;
            offset = end as u64;
            if offset >= next_progress || offset == request.data.len() as u64 {
                send_progress(
                    event_tx,
                    SftpTransferEvent::Progress {
                        transfer_id: request.transfer_id,
                        downloaded_bytes: offset,
                        total_bytes: request.data.len() as u64,
                    },
                )?;
                next_progress = offset.saturating_add(PROGRESS_STEP_BYTES);
            }
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let _ = timeout(REQUEST_TIMEOUT, session.close(handle)).await;
    if result.is_ok() && !cancellation.is_cancelled() {
        if let Err(error) = timeout(
            REQUEST_TIMEOUT,
            session.rename(temporary.clone(), request.remote_path.clone()),
        )
        .await
        .context("publishing remote upload timed out")?
        {
            let _ = timeout(REQUEST_TIMEOUT, session.remove(temporary.clone())).await;
            return Err(error.into());
        }
    } else {
        let _ = timeout(REQUEST_TIMEOUT, session.remove(temporary.clone())).await;
    }
    let _ = session.close_session();
    result
}

async fn ensure_remote_target_absent_for_transfer(
    session: &RawSftpSession,
    path: &str,
) -> Result<()> {
    match timeout(REQUEST_TIMEOUT, session.lstat(path.to_owned())).await {
        Ok(Ok(_)) => anyhow::bail!("remote target already exists; upload was rejected"),
        Ok(Err(russh_sftp::client::error::Error::Status(status)))
            if status.status_code == StatusCode::NoSuchFile =>
        {
            Ok(())
        }
        Ok(Err(error)) => Err(error).context("SFTP upload target check failed"),
        Err(_) => anyhow::bail!("SFTP upload target check timed out"),
    }
}

async fn download_to_local<S>(
    stream: S,
    request: &SftpDownloadRequest,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
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

    let result =
        download_initialized_local(&session, request, cancellation, event_tx, deadline).await;
    if let Err(error) = session.close_session() {
        debug!(%error, "failed to close local SFTP download session");
    }
    result
}

async fn download_initialized_local(
    session: &RawSftpSession,
    request: &SftpDownloadRequest,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
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

    let result = download_open_local_handle(
        session,
        &handle,
        request,
        initial,
        cancellation,
        event_tx,
        deadline,
    )
    .await;
    close_remote_handle(session, handle).await;
    result
}

#[allow(clippy::too_many_arguments)]
async fn download_open_local_handle(
    session: &RawSftpSession,
    handle: &str,
    request: &SftpDownloadRequest,
    initial: RemoteFileMetadata,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
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
    ensure_same_remote_file(initial, validate_regular_metadata(&opened.attrs)?)?;
    let reopened =
        lstat_regular_file(session, &request.remote_path, cancellation, deadline).await?;
    ensure_same_remote_file(initial, reopened)?;

    send_transfer_state(
        event_tx,
        SftpTransferEvent::Started {
            transfer_id: request.transfer_id,
            remote_path: request.remote_path.clone(),
            name: request.name.clone(),
            total_bytes: initial.size,
        },
    )
    .await?;

    let target = request
        .local_target
        .clone()
        .context("local SFTP download is missing its target")?;
    let pending = prepare_local_download(target, cancellation, deadline).await?;
    let (chunk_tx, chunk_rx) = mpsc::channel(WRITER_QUEUE_CAPACITY);
    let writer_cancel = cancellation.clone();
    let expected_bytes = initial.size;
    let mut writer = tokio::task::spawn_blocking(move || {
        write_local_file(pending, chunk_rx, expected_bytes, &writer_cancel)
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
            let local_path =
                await_local_writer_success(&mut writer, cancellation, deadline).await?;
            Ok((local_path, initial.size))
        }
        Err(stream_error) => {
            let writer_error = await_local_writer_cleanup(&mut writer).await;
            if is_cancelled(&stream_error) {
                return Err(stream_error);
            }
            match writer_error {
                Some(error) if !is_incomplete_local_write(&error) && !is_cancelled(&error) => {
                    Err(error)
                }
                _ => Err(stream_error),
            }
        }
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
        if cancellation.is_paused() {
            send_transfer_state(
                event_tx,
                SftpTransferEvent::Paused {
                    transfer_id,
                    downloaded_bytes,
                    total_bytes: expected.size,
                },
            )
            .await?;
            cancellation.wait_until_running().await?;
            send_transfer_state(
                event_tx,
                SftpTransferEvent::Resumed {
                    transfer_id,
                    downloaded_bytes,
                    total_bytes: expected.size,
                },
            )
            .await?;
        }
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
            "writing local download",
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

async fn send_transfer_state(
    event_tx: &mpsc::Sender<SftpTransferEvent>,
    event: SftpTransferEvent,
) -> Result<()> {
    timeout(REQUEST_TIMEOUT, event_tx.send(event))
        .await
        .context("timed out reporting SFTP transfer state")?
        .context("SFTP transfer event receiver dropped")
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
    paused: AtomicBool,
    notify: Notify,
}

impl TransferCancellation {
    fn new() -> Self {
        Self {
            inner: Arc::new(TransferCancellationInner {
                cancelled: AtomicBool::new(false),
                paused: AtomicBool::new(false),
                notify: Notify::new(),
            }),
        }
    }

    fn cancel(&self) {
        if !self.inner.cancelled.swap(true, Ordering::AcqRel) {
            self.inner.notify.notify_waiters();
        }
    }

    fn pause(&self) {
        if !self.is_cancelled() && !self.inner.paused.swap(true, Ordering::AcqRel) {
            self.inner.notify.notify_waiters();
        }
    }

    fn resume(&self) {
        if self.inner.paused.swap(false, Ordering::AcqRel) {
            self.inner.notify.notify_waiters();
        }
    }

    fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Acquire)
    }

    fn is_paused(&self) -> bool {
        self.inner.paused.load(Ordering::Acquire)
    }

    async fn wait_until_running(&self) -> Result<()> {
        loop {
            if self.is_cancelled() {
                return Err(cancelled_error());
            }
            if !self.is_paused() {
                return Ok(());
            }
            let notified = self.inner.notify.notified();
            if self.is_cancelled() {
                return Err(cancelled_error());
            }
            if !self.is_paused() {
                return Ok(());
            }
            tokio::select! {
                _ = self.cancelled() => return Err(cancelled_error()),
                _ = notified => {}
            }
        }
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

pub(crate) async fn discover_download_requests<S>(
    stream: S,
    root: SftpDownloadRoot,
) -> Result<Vec<SftpDownloadRequest>>
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
        .context("SFTP recursive-download handshake timed out")?
        .context("SFTP recursive-download handshake failed")?;
    let result = match timeout(
        DOWNLOAD_TIMEOUT,
        discover_initialized_download_requests(&session, &root),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(anyhow::anyhow!(
            "SFTP recursive download exceeded the overall timeout"
        )),
    };
    if let Err(error) = session.close_session() {
        debug!(%error, "failed to close SFTP recursive-download session");
    }
    result
}

async fn discover_initialized_download_requests(
    session: &RawSftpSession,
    root: &SftpDownloadRoot,
) -> Result<Vec<SftpDownloadRequest>> {
    let attrs = timeout(REQUEST_TIMEOUT, session.lstat(root.remote_path.clone()))
        .await
        .context("inspecting recursive-download root timed out")?
        .with_context(|| format!("cannot inspect remote path {:?}", root.remote_path))?;
    if attrs.attrs.is_symlink() {
        anyhow::bail!("remote symbolic links cannot be downloaded");
    }
    if attrs.attrs.is_regular() {
        let total_bytes = validate_regular_metadata(&attrs.attrs)?.size;
        return Ok(vec![SftpDownloadRequest::for_local_download(
            root.transfer_id,
            root.remote_path.clone(),
            root.local_directory.clone(),
            vec![root.name.clone()],
            total_bytes,
        )?]);
    }
    if !attrs.attrs.is_dir() {
        anyhow::bail!("remote path is neither a regular file nor a directory");
    }

    let mut requests = Vec::new();
    let mut pending = std::collections::VecDeque::from([(
        root.remote_path.clone(),
        vec![root.name.clone()],
        0_usize,
    )]);
    let mut directories = 1_usize;
    let mut scanned_entries = 0_usize;
    let mut total_text_bytes = root.remote_path.len().saturating_add(root.name.len());
    let mut total_bytes = 0_u64;
    while let Some((remote_directory, local_components, depth)) = pending.pop_front() {
        let handle = timeout(REQUEST_TIMEOUT, session.opendir(remote_directory.clone()))
            .await
            .context("opening remote download directory timed out")?
            .with_context(|| format!("cannot open remote directory {remote_directory:?}"))?
            .handle;
        let directory_result = discover_directory_entries(
            session,
            &handle,
            &remote_directory,
            &local_components,
            depth,
            &mut pending,
            &mut requests,
            &mut scanned_entries,
            &mut directories,
            &mut total_text_bytes,
            &mut total_bytes,
            root,
        )
        .await;
        close_remote_handle(session, handle).await;
        directory_result?;
    }
    if requests.is_empty() {
        anyhow::bail!("remote directory contains no regular files eligible for download");
    }
    Ok(requests)
}

#[allow(clippy::too_many_arguments)]
async fn discover_directory_entries(
    session: &RawSftpSession,
    handle: &str,
    remote_directory: &str,
    local_components: &[String],
    depth: usize,
    pending: &mut std::collections::VecDeque<(String, Vec<String>, usize)>,
    requests: &mut Vec<SftpDownloadRequest>,
    scanned_entries: &mut usize,
    directories: &mut usize,
    total_text_bytes: &mut usize,
    total_bytes: &mut u64,
    root: &SftpDownloadRoot,
) -> Result<()> {
    loop {
        let response = timeout(REQUEST_TIMEOUT, session.readdir(handle.to_owned()))
            .await
            .context("reading remote download directory timed out")?;
        let files = match response {
            Ok(names) if names.files.is_empty() => break,
            Ok(names) => names.files,
            Err(russh_sftp::client::error::Error::Status(status))
                if status.status_code == StatusCode::Eof =>
            {
                break;
            }
            Err(error) => return Err(error).context("cannot read remote download directory"),
        };
        for file in files {
            reserve_recursive_entry(scanned_entries)?;
            let Some((name, path, kind, size)) = bounded_discovery_entry(remote_directory, file)
            else {
                continue;
            };
            *total_text_bytes = total_text_bytes
                .saturating_add(name.len())
                .saturating_add(path.len());
            if *total_text_bytes > MAX_RECURSIVE_DOWNLOAD_TEXT_BYTES {
                anyhow::bail!(
                    "remote download tree exceeds the {MAX_RECURSIVE_DOWNLOAD_TEXT_BYTES}-byte text limit"
                );
            }
            let mut child_components = local_components.to_vec();
            child_components.push(name);
            match kind {
                DiscoveryEntryKind::Directory => {
                    if depth >= MAX_RECURSIVE_DOWNLOAD_DEPTH {
                        anyhow::bail!(
                            "remote download tree exceeds the {MAX_RECURSIVE_DOWNLOAD_DEPTH}-level depth limit"
                        );
                    }
                    reserve_recursive_directory(directories)?;
                    pending.push_back((path, child_components, depth + 1));
                }
                DiscoveryEntryKind::RegularFile => {
                    if requests.len() >= MAX_RECURSIVE_DOWNLOAD_FILES {
                        anyhow::bail!(
                            "remote download tree exceeds the {MAX_RECURSIVE_DOWNLOAD_FILES}-file limit"
                        );
                    }
                    *total_bytes = total_bytes.saturating_add(size);
                    if *total_bytes > MAX_RECURSIVE_DOWNLOAD_TOTAL_BYTES {
                        anyhow::bail!(
                            "remote download tree exceeds the {MAX_RECURSIVE_DOWNLOAD_TOTAL_BYTES}-byte limit"
                        );
                    }
                    requests.push(SftpDownloadRequest::for_local_download(
                        if requests.is_empty() {
                            root.transfer_id
                        } else {
                            Uuid::new_v4()
                        },
                        path,
                        root.local_directory.clone(),
                        child_components,
                        size,
                    )?);
                }
            }
        }
    }
    Ok(())
}

fn reserve_recursive_entry(scanned_entries: &mut usize) -> Result<()> {
    *scanned_entries = scanned_entries.saturating_add(1);
    if *scanned_entries > MAX_RECURSIVE_DOWNLOAD_ENTRIES {
        anyhow::bail!(
            "remote download tree exceeds the {MAX_RECURSIVE_DOWNLOAD_ENTRIES}-entry scan limit"
        );
    }
    Ok(())
}

fn reserve_recursive_directory(directories: &mut usize) -> Result<()> {
    *directories = directories.saturating_add(1);
    if *directories > MAX_RECURSIVE_DOWNLOAD_DIRECTORIES {
        anyhow::bail!(
            "remote download tree exceeds the {MAX_RECURSIVE_DOWNLOAD_DIRECTORIES}-directory limit"
        );
    }
    Ok(())
}

enum DiscoveryEntryKind {
    Directory,
    RegularFile,
}

fn bounded_discovery_entry(
    parent: &str,
    file: File,
) -> Option<(String, String, DiscoveryEntryKind, u64)> {
    let name = file.filename;
    if name == "."
        || name == ".."
        || name.is_empty()
        || name.chars().count() > MAX_NAME_CHARS
        || name.chars().any(char::is_control)
        || name.contains(['/', '\\'])
    {
        return None;
    }
    let path = join_remote_path(parent, &name);
    if validate_remote_path(&path).is_err() || file.attrs.is_symlink() {
        return None;
    }
    if file.attrs.is_dir() {
        return Some((name, path, DiscoveryEntryKind::Directory, 0));
    }
    if !file.attrs.is_regular() {
        return None;
    }
    let size = file.attrs.size?;
    if size > MAX_DOWNLOAD_BYTES {
        return None;
    }
    Some((name, path, DiscoveryEntryKind::RegularFile, size))
}

fn join_remote_path(parent: &str, child: &str) -> String {
    if parent == "/" {
        format!("/{child}")
    } else {
        format!("{parent}/{child}")
    }
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

    #[test]
    fn recursive_discovery_entries_skip_unsafe_and_non_regular_paths() {
        let mut regular = FileAttributes::empty();
        regular.set_regular(true);
        regular.size = Some(7);
        let regular = File::new("report.txt", regular);
        assert!(matches!(
            bounded_discovery_entry("/srv", regular),
            Some((name, path, DiscoveryEntryKind::RegularFile, 7))
                if name == "report.txt" && path == "/srv/report.txt"
        ));

        let mut directory = FileAttributes::empty();
        directory.set_dir(true);
        assert!(matches!(
            bounded_discovery_entry("/srv", File::new("nested", directory)),
            Some((name, path, DiscoveryEntryKind::Directory, 0))
                if name == "nested" && path == "/srv/nested"
        ));

        let mut symlink = FileAttributes::empty();
        symlink.set_symlink(true);
        assert!(bounded_discovery_entry("/srv", File::new("link", symlink)).is_none());
        assert!(
            bounded_discovery_entry("/srv", File::new("../escape", FileAttributes::empty()))
                .is_none()
        );
        assert!(
            bounded_discovery_entry("/srv", File::new("nested\\file", FileAttributes::empty()))
                .is_none()
        );
    }

    #[test]
    fn recursive_discovery_budgets_bound_scanned_entries_and_pending_directories() {
        let mut entries = MAX_RECURSIVE_DOWNLOAD_ENTRIES;
        assert!(reserve_recursive_entry(&mut entries).is_err());

        let mut directories = MAX_RECURSIVE_DOWNLOAD_DIRECTORIES;
        assert!(reserve_recursive_directory(&mut directories).is_err());
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
    async fn loopback_download_publishes_into_the_selected_local_directory() {
        let content = Arc::new(b"downloaded locally".to_vec());
        let server = DownloadTestServer {
            content: content.clone(),
            reported_size: content.len() as u64,
            reads: Arc::new(Mutex::new(Vec::new())),
        };
        let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        russh_sftp::server::run(server_stream, server).await;
        let test_root = TestCacheDir::new();
        fs::create_dir(test_root.path()).expect("local directory fixture should be created");
        let request = SftpDownloadRequest::for_local_download(
            Uuid::new_v4(),
            "/srv/report.bin".to_owned(),
            test_root.path().to_owned(),
            vec!["selected".to_owned(), "report.bin".to_owned()],
            content.len() as u64,
        )
        .expect("local download request should validate");
        let cancellation = TransferCancellation::new();
        let (event_tx, mut event_rx) = mpsc::channel(SFTP_TRANSFER_EVENT_CAPACITY);

        let (local_path, total_bytes) =
            download_to_local(client_stream, &request, &cancellation, &event_tx)
                .await
                .expect("loopback local download should complete");

        assert_eq!(total_bytes, content.len() as u64);
        let canonical_root = fs::canonicalize(test_root.path())
            .expect("local directory fixture should resolve canonically");
        assert_eq!(local_path, canonical_root.join("selected/report.bin"));
        assert_eq!(
            fs::read(&local_path).expect("local download should read"),
            *content
        );
        assert!(
            fs::read_dir(
                local_path
                    .parent()
                    .expect("local path should have a parent")
            )
            .expect("local download directory should read")
            .all(|entry| !entry
                .expect("local directory entry should read")
                .file_name()
                .to_string_lossy()
                .ends_with(".part"))
        );
        assert!(matches!(
            event_rx.try_recv(),
            Ok(SftpTransferEvent::Started { .. })
        ));
        assert!(matches!(
            event_rx.try_recv(),
            Ok(SftpTransferEvent::Progress { .. })
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
