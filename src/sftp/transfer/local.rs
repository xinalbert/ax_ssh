use super::*;

/// A local download target rooted in the directory visible in the SFTP pane.
/// Components are validated before construction so a remote path cannot escape
/// that selected directory when it is materialized locally.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LocalDownloadTarget {
    root: PathBuf,
    components: Vec<String>,
    transfer_id: Uuid,
}

impl LocalDownloadTarget {
    pub(super) fn new(root: PathBuf, components: Vec<String>) -> Result<Self> {
        if root.as_os_str().is_empty() {
            anyhow::bail!("local download directory is empty");
        }
        validate_local_components(&components)?;
        Ok(Self {
            root,
            components,
            transfer_id: Uuid::new_v4(),
        })
    }

    pub(super) fn validate_directory(path: &Path) -> Result<()> {
        if path.as_os_str().is_empty() {
            anyhow::bail!("local download directory is empty");
        }
        let metadata = fs::symlink_metadata(path)
            .with_context(|| format!("cannot inspect local download directory {path:?}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            anyhow::bail!("local download directory must be a non-symbolic-link directory");
        }
        Ok(())
    }

    pub(super) fn display_name(&self) -> String {
        self.components.join("/")
    }

    fn prepare(&self) -> Result<LocalPendingFile> {
        let root = fs::canonicalize(&self.root)
            .with_context(|| format!("cannot resolve local download directory {:?}", self.root))?;
        Self::validate_directory(&root)?;
        let mut parent = root.clone();
        for component in self
            .components
            .iter()
            .take(self.components.len().saturating_sub(1))
        {
            parent.push(component);
            if !parent.exists() {
                fs::create_dir(&parent).with_context(|| {
                    format!("cannot create local download directory {parent:?}")
                })?;
            }
            let metadata = fs::symlink_metadata(&parent)
                .with_context(|| format!("cannot inspect local download directory {parent:?}"))?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                anyhow::bail!("local download path contains a symbolic link or non-directory");
            }
        }
        let canonical_parent = fs::canonicalize(&parent)
            .with_context(|| format!("cannot resolve local download parent {parent:?}"))?;
        if !canonical_parent.starts_with(&root) {
            anyhow::bail!("local download target escaped the selected directory");
        }
        let name = self
            .components
            .last()
            .context("local download target is missing its file name")?;
        let final_path = canonical_parent.join(name);
        if final_path.exists() {
            anyhow::bail!("local download target already exists: {final_path:?}");
        }
        let part_path = canonical_parent.join(format!(".{name}.axssh-{}.part", self.transfer_id));
        LocalPendingFile::create(root, canonical_parent, part_path, final_path)
    }
}

pub(super) struct LocalPendingFile {
    file: Option<LocalFile>,
    root: PathBuf,
    parent: PathBuf,
    part_path: PathBuf,
    final_path: PathBuf,
    cleanup_path: Option<PathBuf>,
}

impl LocalPendingFile {
    fn create(
        root: PathBuf,
        parent: PathBuf,
        part_path: PathBuf,
        final_path: PathBuf,
    ) -> Result<Self> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&part_path)
            .with_context(|| format!("cannot create partial local download {part_path:?}"))?;
        Ok(Self {
            file: Some(file),
            root,
            parent,
            part_path: part_path.clone(),
            final_path,
            cleanup_path: Some(part_path),
        })
    }

    fn file_mut(&mut self) -> Result<&mut LocalFile> {
        self.file
            .as_mut()
            .context("local download file was already finalized")
    }

    fn finish(mut self) -> Result<PathBuf> {
        let mut file = self
            .file
            .take()
            .context("local download file was already finalized")?;
        file.flush().context("cannot flush local download")?;
        file.sync_all().context("cannot sync local download")?;
        drop(file);
        verify_local_download_parent(&self.root, &self.parent)?;
        publish_local_download_without_overwrite(&self.part_path, &self.final_path)?;
        self.cleanup_path = Some(self.final_path.clone());
        sync_local_directory(&self.parent)?;
        self.cleanup_path = None;
        Ok(self.final_path.clone())
    }
}

impl Drop for LocalPendingFile {
    fn drop(&mut self) {
        if let Some(path) = self.cleanup_path.take()
            && let Err(error) = fs::remove_file(&path)
            && error.kind() != io::ErrorKind::NotFound
        {
            debug!(path = %path.display(), %error, "failed to remove incomplete local download");
        }
    }
}

pub(super) async fn prepare_local_download(
    target: LocalDownloadTarget,
    cancellation: &TransferCancellation,
    deadline: Instant,
) -> Result<LocalPendingFile> {
    let task = tokio::task::spawn_blocking(move || target.prepare());
    await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "preparing local download",
        task,
    )
    .await?
    .context("local download preparation task failed")?
}

pub(super) fn write_local_file(
    mut pending: LocalPendingFile,
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
            .context("local download byte count overflowed")?;
        if written > expected_bytes {
            anyhow::bail!("local download writer received more bytes than expected");
        }
        pending
            .file_mut()?
            .write_all(&chunk)
            .context("cannot write local download")?;
    }
    if cancellation.is_cancelled() {
        return Err(cancelled_error());
    }
    if written != expected_bytes {
        return Err(IncompleteLocalWrite {
            expected: expected_bytes,
            actual: written,
        }
        .into());
    }
    pending.finish()
}

pub(super) async fn await_local_writer_success(
    writer: &mut JoinHandle<Result<PathBuf>>,
    cancellation: &TransferCancellation,
    deadline: Instant,
) -> Result<PathBuf> {
    if cancellation.is_cancelled() {
        let _ = await_local_writer_cleanup(writer).await;
        return Err(cancelled_error());
    }
    tokio::select! {
        _ = cancellation.cancelled() => {
            let _ = await_local_writer_cleanup(writer).await;
            Err(cancelled_error())
        }
        result = timeout_at(deadline, &mut *writer) => match result {
            Ok(joined) => joined.context("local download writer task failed")?,
            Err(_) => {
                cancellation.cancel();
                let _ = await_local_writer_cleanup(writer).await;
                anyhow::bail!("SFTP download exceeded the {}-second overall timeout", DOWNLOAD_TIMEOUT.as_secs())
            }
        },
    }
}

pub(super) async fn await_local_writer_cleanup(
    writer: &mut JoinHandle<Result<PathBuf>>,
) -> Option<anyhow::Error> {
    match timeout(WRITER_CLEANUP_TIMEOUT, &mut *writer).await {
        Ok(Ok(Ok(path))) => {
            remove_local_download_best_effort(path, "transfer failed after publication").await;
            None
        }
        Ok(Ok(Err(error))) => Some(error),
        Ok(Err(error)) => {
            Some(anyhow::Error::from(error).context("local download writer task failed"))
        }
        Err(_) => {
            debug!("timed out waiting for local download writer cleanup");
            None
        }
    }
}

pub(super) async fn remove_local_download_best_effort(path: PathBuf, reason: &'static str) {
    let display_path = path.clone();
    match tokio::task::spawn_blocking(move || fs::remove_file(path)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) if error.kind() == io::ErrorKind::NotFound => {}
        Ok(Err(error)) => {
            debug!(path = %display_path.display(), %error, reason, "failed to remove local download");
        }
        Err(error) => {
            debug!(path = %display_path.display(), %error, reason, "failed to join local download cleanup task");
        }
    }
}

pub(super) fn is_incomplete_local_write(error: &anyhow::Error) -> bool {
    error.downcast_ref::<IncompleteLocalWrite>().is_some()
}

fn validate_local_components(components: &[String]) -> Result<()> {
    if components.is_empty() || components.len() > MAX_RECURSIVE_DOWNLOAD_DEPTH + 1 {
        anyhow::bail!("local download path has an invalid depth");
    }
    for component in components {
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.contains(['/', '\\'])
            || component.chars().any(char::is_control)
            || component.chars().count() > MAX_NAME_CHARS
            || Path::new(component)
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            anyhow::bail!("local download path has an unsafe component");
        }
    }
    Ok(())
}

fn verify_local_download_parent(root: &Path, parent: &Path) -> Result<()> {
    LocalDownloadTarget::validate_directory(root)?;
    let root = fs::canonicalize(root).context("cannot resolve selected local directory")?;
    let parent = fs::canonicalize(parent).context("cannot resolve local download parent")?;
    if !parent.starts_with(root) {
        anyhow::bail!("local download parent escaped the selected directory");
    }
    Ok(())
}

fn publish_local_download_without_overwrite(part_path: &Path, final_path: &Path) -> Result<()> {
    // `rename` can replace an entry created after the earlier existence check.
    // Linking the fully synced file creates the final name only when it is still
    // absent, so a concurrent local file is preserved rather than overwritten.
    fs::hard_link(part_path, final_path).with_context(|| {
        format!("cannot publish local download without overwrite {final_path:?}")
    })?;
    if let Err(error) = fs::remove_file(part_path) {
        let _ = fs::remove_file(final_path);
        return Err(error).context("cannot remove published local download partial file");
    }
    Ok(())
}

fn sync_local_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        LocalFile::open(path)
            .context("cannot open local download directory for sync")?
            .sync_all()
            .context("cannot sync local download directory")?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[derive(Debug)]
struct IncompleteLocalWrite {
    expected: u64,
    actual: u64,
}

impl fmt::Display for IncompleteLocalWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "local download write ended after {} of {} bytes",
            self.actual, self.expected
        )
    }
}

impl StdError for IncompleteLocalWrite {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_target_rejects_escape_components() {
        let directory = std::env::temp_dir().join(format!("axssh-target-{}", Uuid::new_v4()));
        fs::create_dir(&directory).expect("fixture directory should be created");

        assert!(LocalDownloadTarget::new(directory.clone(), vec!["..".to_owned()]).is_err());
        assert!(
            LocalDownloadTarget::new(directory.clone(), vec!["nested/file".to_owned()]).is_err()
        );
        assert!(
            LocalDownloadTarget::new(directory.clone(), vec!["nested\\file".to_owned()]).is_err()
        );
        assert!(LocalDownloadTarget::new(directory.clone(), vec!["report.txt".to_owned()]).is_ok());

        fs::remove_dir(directory).expect("fixture directory should be removed");
    }

    #[test]
    fn cancelled_local_writer_removes_its_partial_file() {
        let directory = std::env::temp_dir().join(format!("axssh-target-{}", Uuid::new_v4()));
        fs::create_dir(&directory).expect("fixture directory should be created");
        let target = LocalDownloadTarget::new(directory.clone(), vec!["report.txt".to_owned()])
            .expect("local target should validate");
        let pending = target.prepare().expect("partial file should be created");
        let part_path = pending.part_path.clone();
        let (chunk_tx, chunk_rx) = mpsc::channel(WRITER_QUEUE_CAPACITY);
        drop(chunk_tx);
        let cancellation = TransferCancellation::new();
        cancellation.cancel();

        let error = write_local_file(pending, chunk_rx, 1, &cancellation)
            .expect_err("cancelled local write should fail");

        assert!(is_cancelled(&error));
        assert!(!part_path.exists());
        fs::remove_dir(directory).expect("fixture directory should be removed");
    }

    #[test]
    fn local_publish_preserves_a_concurrent_destination() {
        let directory = std::env::temp_dir().join(format!("axssh-target-{}", Uuid::new_v4()));
        fs::create_dir(&directory).expect("fixture directory should be created");
        let part_path = directory.join(".report.txt.axssh-test.part");
        let final_path = directory.join("report.txt");
        fs::write(&part_path, b"download").expect("partial fixture should be created");
        fs::write(&final_path, b"existing").expect("destination fixture should be created");

        assert!(publish_local_download_without_overwrite(&part_path, &final_path).is_err());
        assert_eq!(
            fs::read(&final_path).expect("destination should remain readable"),
            b"existing"
        );
        assert_eq!(
            fs::read(&part_path).expect("partial should remain readable"),
            b"download"
        );

        fs::remove_file(part_path).expect("partial fixture should be removed");
        fs::remove_file(final_path).expect("destination fixture should be removed");
        fs::remove_dir(directory).expect("fixture directory should be removed");
    }
}
