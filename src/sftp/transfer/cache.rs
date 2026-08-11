use super::*;

pub(super) async fn prepare_cache_file(
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

pub(super) async fn await_writer_success(
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

pub(super) async fn await_writer_cleanup(
    writer: &mut JoinHandle<Result<PathBuf>>,
) -> Option<anyhow::Error> {
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

pub(super) async fn remove_cache_file_best_effort(path: PathBuf, reason: &'static str) {
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

#[derive(Debug)]
pub(super) struct CacheTarget {
    pub(super) directory: PathBuf,
    pub(super) part_path: PathBuf,
    pub(super) final_path: PathBuf,
}

impl CacheTarget {
    pub(super) fn new(directory: &Path, cache_id: Uuid, remote_name: &str) -> Self {
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

pub(super) struct PendingCacheFile {
    file: Option<LocalFile>,
    pub(super) target: CacheTarget,
    cleanup_path: Option<PathBuf>,
}

impl PendingCacheFile {
    pub(super) fn create(target: CacheTarget, reserved_bytes: u64) -> Result<Self> {
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

    pub(super) fn file_mut(&mut self) -> Result<&mut LocalFile> {
        self.file
            .as_mut()
            .context("private cache file was already finalized")
    }

    pub(super) fn finish(mut self) -> Result<PathBuf> {
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

pub(super) fn write_cache_file(
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
pub(super) struct CacheWriterStopped;

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

pub(super) fn is_incomplete_cache_write(error: &anyhow::Error) -> bool {
    error.downcast_ref::<IncompleteCacheWrite>().is_some()
}

pub(super) fn cache_namespace() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "axsoft", "ax_ssh")
        .context("cannot determine the AxSSH cache directory")?;
    Ok(dirs.cache_dir().join("sftp-open"))
}

pub(super) fn ensure_cache_namespace(path: &Path) -> Result<()> {
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

pub(super) fn enforce_cache_quota(
    cache_dir: &Path,
    incoming_bytes: u64,
    now: SystemTime,
) -> Result<()> {
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

pub(super) fn sanitize_basename(name: &str) -> String {
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

pub(super) fn cleanup_stale_cache_at(cache_dir: &Path, now: SystemTime) -> Result<usize> {
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

pub(super) fn managed_cache_name(name: &str) -> Option<bool> {
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

pub(super) fn cache_entry_is_stale(modified: SystemTime, now: SystemTime, is_part: bool) -> bool {
    let stale_after = if is_part {
        PART_STALE_AFTER
    } else {
        CACHE_STALE_AFTER
    };
    now.duration_since(modified)
        .is_ok_and(|age| age >= stale_after)
}

pub(super) async fn cleanup_stale_sftp_open_cache_impl() -> Result<usize> {
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
