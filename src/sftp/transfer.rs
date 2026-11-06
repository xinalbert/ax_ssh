//! Bounded SFTP downloads with private-cache opening kept as a separate flow.

mod cache;
mod control;
mod discovery;
mod download;
mod local;
pub(super) mod pipeline;
mod upload;

pub use self::control::configure_global_upload_limit;
pub(crate) use self::control::{TransferRateLimiter, UploadPermit};
pub(crate) use self::discovery::discover_download_requests;
pub use self::discovery::transfer_name_matches_filter;

use self::cache::*;
use self::control::*;
use self::discovery::*;
use self::download::*;
use self::local::*;
use self::pipeline::{REQUEST_WINDOW, RequestWindow};
use self::upload::*;

use std::collections::HashSet;
use std::error::Error as StdError;
use std::fmt;
use std::fs::{self, File as LocalFile, OpenOptions};
use std::future::Future;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use russh_sftp::client::{Config, RawSftpSession};
use russh_sftp::protocol::{File, FileAttributes, OpenFlags, Packet, StatusCode};
use sha1::{Digest, Sha1};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::runtime::Handle;
use tokio::sync::{Mutex as AsyncMutex, Notify, mpsc};
use tokio::task::{JoinError, JoinHandle};
use tokio::time::{Duration, Instant, timeout, timeout_at};
use tracing::{debug, warn};
use uuid::Uuid;

use super::{
    MAX_PACKET_BYTES, PacketLimitedStream, REQUEST_TIMEOUT, bounded_error, validate_remote_path,
};

pub(crate) const SFTP_TRANSFER_EVENT_CAPACITY: usize = 32;
pub(crate) const SFTP_TRANSFER_QUEUE_WINDOW: usize = 512;

pub(crate) fn stable_child_transfer_id(root: Uuid, remote_path: &str) -> Uuid {
    let mut digest = Sha1::new();
    digest.update(root.as_bytes());
    digest.update([0]);
    digest.update(remote_path.as_bytes());
    let digest = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Uuid::from_bytes(bytes)
}

pub(crate) fn transfer_error_is_recoverable(error: &anyhow::Error) -> bool {
    // A lost rename response cannot prove whether the server published the file.
    // Replaying KeepBoth could publish a duplicate, so require explicit review.
    if error.downcast_ref::<UploadPublicationUncertain>().is_some() {
        return false;
    }
    error.chain().any(|cause| {
        cause.is::<tokio::time::error::Elapsed>()
            || matches!(
                cause.downcast_ref::<russh::Error>(),
                Some(
                    russh::Error::Disconnect
                        | russh::Error::HUP
                        | russh::Error::SendError
                        | russh::Error::ConnectionTimeout
                        | russh::Error::KeepaliveTimeout
                        | russh::Error::InactivityTimeout
                )
            )
            || cause
                .downcast_ref::<russh_sftp::client::error::Error>()
                .is_some_and(|error| !matches!(error, russh_sftp::client::error::Error::Status(_)))
            || cause.downcast_ref::<std::io::Error>().is_some_and(|error| {
                matches!(
                    error.kind(),
                    std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::NotConnected
                        | std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::TimedOut
                )
            })
    })
}

#[derive(Debug)]
struct UploadPublicationUncertain;

impl fmt::Display for UploadPublicationUncertain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "upload publication was not confirmed; check the remote target before retrying",
        )
    }
}

impl StdError for UploadPublicationUncertain {}

const DOWNLOAD_CHUNK_BYTES: u32 = 64 * 1024;
const WRITER_QUEUE_CAPACITY: usize = 2;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const MAX_CACHE_OPEN_BYTES: u64 = 512 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const WRITER_CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);
const CACHE_STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
const PART_STALE_AFTER: Duration = Duration::from_secs(60 * 60);
const MAX_CACHE_BASENAME_BYTES: usize = 160;
const MAX_CACHE_SCAN_ENTRIES: usize = 4096;
const MAX_CACHE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_CACHE_FILES: usize = 128;

static CACHE_QUOTA_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
const POSIX_RENAME_EXTENSION: &str = "posix-rename@openssh.com";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SftpUploadConflictChoice {
    Skip,
    Overwrite,
    KeepBoth,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SftpTransferEvent {
    Queued {
        transfer_id: Uuid,
        root_transfer_id: Option<Uuid>,
        remote_path: String,
        local_path: Option<PathBuf>,
        upload: bool,
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
        recoverable: bool,
    },
    DiscoveryFinished {
        transfer_id: Uuid,
        discovered_files: usize,
        skipped_entries: usize,
        failed_files: usize,
        cancelled: bool,
        message: Option<String>,
    },
    UploadConflict {
        transfer_id: Uuid,
        batch_id: Uuid,
        name: String,
        remote_size: Option<u64>,
        remote_modified: Option<u32>,
    },
    Skipped {
        transfer_id: Uuid,
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
        recoverable: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SftpUploadRequest {
    transfer_id: Uuid,
    batch_id: Uuid,
    remote_path: String,
    name: String,
    local_path: PathBuf,
    remote_directories: Vec<String>,
    total_bytes: u64,
    local_identity: Arc<LocalUploadIdentity>,
    conflict_choice: Option<SftpUploadConflictChoice>,
    expected_remote: Option<RemoteUploadFingerprint>,
    execution_id: Uuid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RemoteUploadFingerprint {
    size: Option<u64>,
    modified: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LocalUploadIdentity {
    length: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl SftpUploadRequest {
    pub(crate) fn from_local_file(
        transfer_id: Uuid,
        batch_id: Uuid,
        remote_path: String,
        local_path: PathBuf,
        total_bytes: u64,
        remote_directories: Vec<String>,
    ) -> Result<Self> {
        let metadata = fs::symlink_metadata(&local_path)
            .with_context(|| format!("cannot inspect local upload file {local_path:?}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            anyhow::bail!("local upload source is not a regular file");
        }
        if metadata.len() != total_bytes {
            anyhow::bail!("local upload file size changed before it was queued");
        }
        validate_remote_path(&remote_path)?;
        for directory in &remote_directories {
            validate_remote_path(directory)?;
            if directory.is_empty()
                || !remote_path.starts_with(&format!("{}/", directory.trim_end_matches('/')))
            {
                anyhow::bail!("remote upload directory is invalid");
            }
        }
        let name = remote_path
            .rsplit('/')
            .next()
            .filter(|name| !name.is_empty())
            .context("remote upload target is missing a file name")?;
        if name == "."
            || name == ".."
            || name.chars().any(char::is_control)
            || name.contains(['/', '\\'])
        {
            anyhow::bail!("remote upload target name is invalid");
        }
        let name = name.to_owned();
        Ok(Self {
            transfer_id,
            batch_id,
            remote_path,
            name,
            local_path,
            remote_directories,
            total_bytes,
            local_identity: Arc::new(local_upload_identity(&metadata)),
            conflict_choice: None,
            expected_remote: None,
            execution_id: Uuid::new_v4(),
        })
    }

    pub(crate) fn transfer_id(&self) -> Uuid {
        self.transfer_id
    }

    pub(crate) fn batch_id(&self) -> Uuid {
        self.batch_id
    }

    pub(crate) fn set_conflict_choice(&mut self, choice: SftpUploadConflictChoice) {
        self.conflict_choice = Some(choice);
    }

    pub(crate) fn set_expected_remote(&mut self, size: Option<u64>, modified: Option<u32>) {
        self.expected_remote = Some(RemoteUploadFingerprint { size, modified });
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn local_path(&self) -> &Path {
        &self.local_path
    }

    pub(crate) fn remote_path(&self) -> &str {
        &self.remote_path
    }

    pub(crate) fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    pub(crate) fn execution_id(&self) -> Uuid {
        self.execution_id
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

pub(crate) enum SftpDiscoveredDownload {
    Request {
        root_transfer_id: Uuid,
        request: SftpDownloadRequest,
    },
    Finished {
        root_transfer_id: Uuid,
        name: String,
        outcome: std::result::Result<SftpDiscoverySummary, String>,
        recoverable: bool,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SftpDiscoverySummary {
    pub(crate) discovered_files: usize,
    pub(crate) skipped_entries: usize,
    pub(crate) first_issue: Option<String>,
    pub(crate) is_directory: bool,
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

    /// Construct a single-file request for an explicit local final path.
    ///
    /// The worker still validates and canonicalizes the parent directory at
    /// publication time through `LocalDownloadTarget`; this constructor only
    /// decomposes the owned destination so the existing anti-traversal and
    /// no-overwrite writer path remains authoritative.
    pub(crate) fn for_explicit_local_path(
        transfer_id: Uuid,
        remote_path: String,
        local_path: PathBuf,
        total_bytes: u64,
    ) -> Result<Self> {
        if !local_path.is_absolute() {
            anyhow::bail!("local download target must be an absolute path");
        }
        let local_directory = local_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .context("local download target has no parent directory")?
            .to_owned();
        let name = local_path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .context("local download target has no UTF-8 file name")?
            .to_owned();
        Self::for_local_download(
            transfer_id,
            remote_path,
            local_directory,
            vec![name],
            total_bytes,
        )
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

    pub(crate) fn local_path(&self) -> Option<PathBuf> {
        self.local_target
            .as_ref()
            .map(LocalDownloadTarget::final_path)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SftpDownloadRoot {
    transfer_id: Uuid,
    remote_path: String,
    local_directory: PathBuf,
    name: String,
    filter_patterns: Vec<String>,
    expected_directory: bool,
    allow_existing_local_directories: bool,
    excluded_remote_paths: HashSet<String>,
}

impl SftpDownloadRoot {
    pub(crate) fn new(
        transfer_id: Uuid,
        remote_path: String,
        local_directory: PathBuf,
        filter_patterns: Vec<String>,
        expected_directory: bool,
    ) -> Result<Self> {
        Self::new_with_options(
            transfer_id,
            remote_path,
            local_directory,
            filter_patterns,
            expected_directory,
            false,
            HashSet::new(),
        )
    }

    pub(crate) fn new_with_existing_directories(
        transfer_id: Uuid,
        remote_path: String,
        local_directory: PathBuf,
        filter_patterns: Vec<String>,
        expected_directory: bool,
        excluded_remote_paths: HashSet<String>,
    ) -> Result<Self> {
        Self::new_with_options(
            transfer_id,
            remote_path,
            local_directory,
            filter_patterns,
            expected_directory,
            true,
            excluded_remote_paths,
        )
    }

    fn new_with_options(
        transfer_id: Uuid,
        remote_path: String,
        local_directory: PathBuf,
        filter_patterns: Vec<String>,
        expected_directory: bool,
        allow_existing_local_directories: bool,
        excluded_remote_paths: HashSet<String>,
    ) -> Result<Self> {
        let name = validate_download_path(&remote_path)?.to_owned();
        if local_directory.as_os_str().is_empty() {
            anyhow::bail!("local download directory is empty");
        }
        if filter_patterns.len() > 64
            || filter_patterns.iter().any(|pattern| {
                pattern.is_empty()
                    || pattern.chars().count() > 256
                    || pattern.chars().any(char::is_control)
                    || pattern.contains(['/', '\\'])
            })
            || filter_patterns
                .iter()
                .map(|pattern| pattern.chars().count())
                .sum::<usize>()
                > 16 * 1024
        {
            anyhow::bail!("SFTP transfer filter patterns are invalid or too large");
        }
        Ok(Self {
            transfer_id,
            remote_path,
            local_directory,
            name,
            filter_patterns,
            expected_directory,
            allow_existing_local_directories,
            excluded_remote_paths,
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

    pub(crate) fn local_path(&self) -> PathBuf {
        self.local_directory.join(&self.name)
    }

    pub(crate) fn filter_patterns(&self) -> &[String] {
        &self.filter_patterns
    }

    pub(crate) fn expected_directory(&self) -> bool {
        self.expected_directory
    }

    pub(crate) fn allow_existing_local_directories(&self) -> bool {
        self.allow_existing_local_directories
    }

    pub(crate) fn excludes_remote_path(&self, path: &str) -> bool {
        self.excluded_remote_paths.contains(path)
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
    pub(crate) fn reserve_global_slot() -> Result<UploadPermit> {
        global_upload_limiter().try_acquire()
    }

    pub(crate) fn spawn<F, Fut, S>(
        runtime: &Handle,
        opening: F,
        request: SftpUploadRequest,
        event_tx: mpsc::Sender<SftpTransferEvent>,
        global_slot: UploadPermit,
        rate_limiter: Arc<TransferRateLimiter>,
    ) -> Self
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = Result<S>> + Send + 'static,
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let transfer_id = request.transfer_id;
        let cancellation = TransferCancellation::with_limiter(rate_limiter);
        let task = runtime.spawn(run_upload(
            opening,
            request,
            cancellation.clone(),
            event_tx,
            global_slot,
        ));
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
    pub(crate) fn spawn<F, Fut, S>(
        runtime: &Handle,
        stream: S,
        reopening: F,
        request: SftpDownloadRequest,
        event_tx: mpsc::Sender<SftpTransferEvent>,
        rate_limiter: Arc<TransferRateLimiter>,
    ) -> Self
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = Result<S>> + Send + 'static,
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let transfer_id = request.transfer_id;
        let cancellation = TransferCancellation::with_limiter(rate_limiter);
        let task_cancellation = cancellation.clone();
        let task = runtime.spawn(run_download(
            stream,
            reopening,
            request,
            task_cancellation,
            event_tx,
        ));
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
pub fn snapshot_local_file_for_open(
    mut source: LocalFile,
    name: &str,
    max_file_bytes: u64,
    cancellation: &AtomicBool,
) -> Result<PathBuf> {
    snapshot_local_file_for_open_at(
        &mut source,
        name,
        &cache_namespace()?,
        max_file_bytes,
        cancellation,
    )
}

fn snapshot_local_file_for_open_at(
    source: &mut LocalFile,
    name: &str,
    cache_dir: &Path,
    max_file_bytes: u64,
    cancellation: &AtomicBool,
) -> Result<PathBuf> {
    if max_file_bytes == 0 {
        anyhow::bail!("local file open cache limit must be positive");
    }
    let metadata = source
        .metadata()
        .context("cannot inspect validated local file")?;
    if !metadata.is_file() {
        anyhow::bail!("validated local file is no longer a regular file");
    }
    let expected_bytes = metadata.len();
    if expected_bytes > max_file_bytes {
        anyhow::bail!(
            "local file is {expected_bytes} bytes, exceeding the {max_file_bytes}-byte open limit"
        );
    }

    ensure_cache_namespace(cache_dir)?;
    let _quota = CACHE_QUOTA_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| anyhow::anyhow!("SFTP cache quota lock is poisoned"))?;
    let cache_quota_bytes = MAX_CACHE_BYTES.max(max_file_bytes);
    enforce_cache_quota(
        cache_dir,
        expected_bytes,
        cache_quota_bytes,
        SystemTime::now(),
    )?;
    let mut pending = PendingCacheFile::create(
        CacheTarget::new(cache_dir, Uuid::new_v4(), name),
        expected_bytes,
    )?;
    let mut reader = Read::by_ref(source).take(expected_bytes.saturating_add(1));
    let mut buffer = [0_u8; 64 * 1024];
    let mut copied = 0_u64;
    loop {
        if cancellation.load(Ordering::Acquire) {
            anyhow::bail!("local file open snapshot was cancelled");
        }
        let count = reader
            .read(&mut buffer)
            .context("cannot read validated local file into the private open cache")?;
        if count == 0 {
            break;
        }
        pending
            .file_mut()?
            .write_all(&buffer[..count])
            .context("cannot write validated local file into the private open cache")?;
        copied = copied.saturating_add(count as u64);
    }
    if copied != expected_bytes {
        anyhow::bail!(
            "local file changed while creating its open snapshot: expected {expected_bytes} bytes, copied {copied}"
        );
    }
    if cancellation.load(Ordering::Acquire) {
        anyhow::bail!("local file open snapshot was cancelled");
    }
    pending.finish()
}

async fn run_download<F, Fut, S>(
    stream: S,
    mut reopening: F,
    request: SftpDownloadRequest,
    cancellation: TransferCancellation,
    event_tx: mpsc::Sender<SftpTransferEvent>,
) where
    F: FnMut() -> Fut + Send,
    Fut: Future<Output = Result<S>> + Send + 'static,
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let transfer_id = request.transfer_id;
    let writes_local_target = request.local_target.is_some();
    let mut first_stream = Some(stream);
    let mut retries = 0;
    let result = loop {
        let attempt = async {
            let stream = match first_stream.take() {
                Some(stream) => stream,
                None => tokio::select! {
                    _ = cancellation.cancelled() => return Err(cancelled_error()),
                    result = reopening() => result?,
                },
            };
            if writes_local_target {
                download_to_local(stream, &request, &cancellation, &event_tx).await
            } else {
                let cache_dir = cache_namespace()?;
                download_to_cache(stream, &request, &cancellation, &event_tx, &cache_dir).await
            }
        }
        .await;
        match attempt {
            Err(error) if transfer_error_is_recoverable(&error) && retries < 2 => {
                retries += 1;
                tracing::warn!(%transfer_id, retries, %error, "retrying failed SFTP file on the same connection");
                if let Err(error) = transfer_retry_delay(&cancellation, retries).await {
                    break Err(error);
                }
            }
            result => break result,
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
            recoverable: transfer_error_is_recoverable(&error),
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

async fn run_upload<F, Fut, S>(
    mut opening: F,
    mut request: SftpUploadRequest,
    cancellation: TransferCancellation,
    event_tx: mpsc::Sender<SftpTransferEvent>,
    _global_slot: UploadPermit,
) where
    F: FnMut() -> Fut + Send,
    Fut: Future<Output = Result<S>> + Send + 'static,
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let total_bytes = request.total_bytes;
    let mut retries = 0;
    let result = loop {
        let attempt = async {
            let stream = tokio::select! {
                _ = cancellation.cancelled() => return Err(cancelled_error()),
                result = opening() => result?,
            };
            send_transfer_state(
                &event_tx,
                SftpTransferEvent::Started {
                    transfer_id: request.transfer_id,
                    remote_path: request.remote_path.clone(),
                    name: request.name.clone(),
                    total_bytes,
                },
            )
            .await?;
            upload_initialized(stream, &request, &cancellation, &event_tx).await
        }
        .await;
        match attempt {
            Err(error) if transfer_error_is_recoverable(&error) && retries < 2 => {
                retries += 1;
                tracing::warn!(transfer_id = %request.transfer_id, retries, %error, "retrying failed SFTP upload on the same connection");
                if let Err(error) = transfer_retry_delay(&cancellation, retries).await {
                    break Err(error);
                }
                // A previous remote temporary file may remain after channel loss.
                // Never reuse it or publish unverified bytes from that execution.
                request.execution_id = Uuid::new_v4();
            }
            result => break result,
        }
    };
    let terminal = match result {
        Ok(UploadOutcome::Conflict(remote)) => SftpTransferEvent::UploadConflict {
            transfer_id: request.transfer_id,
            batch_id: request.batch_id,
            name: request.name.clone(),
            remote_size: remote.size,
            remote_modified: remote.modified,
        },
        Ok(UploadOutcome::Skipped) => SftpTransferEvent::Skipped {
            transfer_id: request.transfer_id,
        },
        Ok(UploadOutcome::Published) if cancellation.is_cancelled() => {
            SftpTransferEvent::Cancelled {
                transfer_id: request.transfer_id,
            }
        }
        Ok(UploadOutcome::Published) => SftpTransferEvent::Completed {
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
            recoverable: transfer_error_is_recoverable(&error),
        },
    };
    let _ = send_transfer_state(&event_tx, terminal).await;
}

async fn transfer_retry_delay(cancellation: &TransferCancellation, retry: u32) -> Result<()> {
    tokio::select! {
        _ = cancellation.cancelled() => Err(cancelled_error()),
        _ = tokio::time::sleep(Duration::from_secs(u64::from(retry))) => Ok(()),
    }
}

enum UploadOutcome {
    Published,
    Conflict(RemoteUploadFingerprint),
    Skipped,
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

#[cfg(test)]
mod tests;
