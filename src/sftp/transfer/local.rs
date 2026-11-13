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

    pub(super) fn final_path(&self) -> PathBuf {
        self.components
            .iter()
            .fold(self.root.clone(), |path, component| path.join(component))
    }

    /// Create a directory target without replacing an existing local entry.
    ///
    /// The selected root is canonicalized first and every new component is
    /// created with `create_dir`, so a concurrent file, directory, or symlink
    /// at the promised destination causes the operation to fail. Existing
    /// intermediate components must remain real directories. The final
    /// component is accepted only when reconnect recovery explicitly permits
    /// the already materialized directory tree.
    pub(super) fn prepare_directory(&self, allow_existing_target: bool) -> Result<PathBuf> {
        let root = fs::canonicalize(&self.root)
            .with_context(|| format!("cannot resolve local download directory {:?}", self.root))?;
        Self::validate_directory(&root)?;
        let mut current = root.clone();
        for (index, component) in self.components.iter().enumerate() {
            current.push(component);
            let is_final = index + 1 == self.components.len();
            match fs::create_dir(&current) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    if is_final && !allow_existing_target {
                        anyhow::bail!("local download directory already exists: {current:?}");
                    }
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("cannot create local download directory {current:?}")
                    });
                }
            }
            let metadata = fs::symlink_metadata(&current)
                .with_context(|| format!("cannot inspect local download directory {current:?}"))?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                anyhow::bail!("local download path contains a symbolic link or non-directory");
            }
            let canonical = fs::canonicalize(&current)
                .with_context(|| format!("cannot resolve local download directory {current:?}"))?;
            if !canonical.starts_with(&root) {
                anyhow::bail!("local download directory escaped the selected directory");
            }
            sync_local_directory(&canonical)?;
            current = canonical;
        }
        Ok(current)
    }

    #[cfg(test)]
    fn prepare(&self) -> Result<LocalPendingFile> {
        self.prepare_with_choice(None, None, false)
    }

    fn prepare_with_choice(
        &self,
        choice: Option<SftpConflictChoice>,
        expected: Option<LocalDownloadFingerprint>,
        interactive: bool,
    ) -> Result<LocalPendingFile> {
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
                match fs::create_dir(&parent) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => {
                        return Err(error).with_context(|| {
                            format!("cannot create local download directory {parent:?}")
                        });
                    }
                }
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
        let mut final_path = canonical_parent.join(name);
        let existing = local_download_fingerprint(&final_path)?;
        let overwrite = match (existing, choice) {
            (Some(local), None) if interactive => return Err(LocalDownloadConflict(local).into()),
            (Some(_), None) => {
                anyhow::bail!("local download target already exists: {final_path:?}")
            }
            (Some(_), Some(SftpConflictChoice::Skip)) => return Err(DownloadSkipped.into()),
            (Some(local), Some(SftpConflictChoice::Overwrite)) => {
                if expected.as_ref().is_some_and(|expected| *expected != local) {
                    anyhow::bail!("local download target changed since confirmation");
                }
                Some(local)
            }
            (Some(_), Some(SftpConflictChoice::KeepBoth)) => {
                final_path = available_local_download_name(&canonical_parent, name)?;
                None
            }
            (None, Some(SftpConflictChoice::Overwrite)) if expected.is_some() => {
                anyhow::bail!("local download target disappeared since confirmation");
            }
            (None, _) => None,
        };
        let part_path = canonical_parent.join(format!(".{name}.axssh-{}.part", self.transfer_id));
        let mut pending = LocalPendingFile::create(root, canonical_parent, part_path, final_path)?;
        pending.overwrite = overwrite;
        pending.keep_both = (choice == Some(SftpConflictChoice::KeepBoth)).then(|| name.clone());
        Ok(pending)
    }
}

pub(super) struct LocalPendingFile {
    file: Option<LocalFile>,
    root: PathBuf,
    parent: PathBuf,
    part_path: PathBuf,
    final_path: PathBuf,
    cleanup_path: Option<PathBuf>,
    overwrite: Option<LocalDownloadFingerprint>,
    keep_both: Option<String>,
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
            overwrite: None,
            keep_both: None,
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
        if let Some(expected) = &self.overwrite {
            if local_download_fingerprint(&self.final_path)?.as_ref() != Some(expected) {
                anyhow::bail!("local download target changed before replacement");
            }
            fs::rename(&self.part_path, &self.final_path)
                .context("cannot atomically replace local download target")?;
            // Publication is the commit point. Later cancellation or a sync
            // error must never delete the completed replacement.
            self.cleanup_path = None;
        } else {
            let mut retries = 0;
            loop {
                match publish_local_download_without_overwrite(&self.part_path, &self.final_path) {
                    Err(error)
                        if self.keep_both.is_some()
                            && error.downcast_ref::<io::Error>().is_some_and(|error| {
                                error.kind() == io::ErrorKind::AlreadyExists
                            })
                            && retries < 10_000 =>
                    {
                        let name = self
                            .keep_both
                            .as_deref()
                            .context("missing keep-both name")?;
                        self.final_path = available_local_download_name(&self.parent, name)?;
                        retries += 1;
                    }
                    result => {
                        result?;
                        break;
                    }
                }
            }
            self.cleanup_path = Some(self.final_path.clone());
        }
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
    choice: Option<SftpConflictChoice>,
    expected: Option<LocalDownloadFingerprint>,
    interactive: bool,
    cancellation: &TransferCancellation,
    deadline: Instant,
) -> Result<LocalPendingFile> {
    let task = tokio::task::spawn_blocking(move || {
        target.prepare_with_choice(choice, expected, interactive)
    });
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

pub(super) async fn prepare_local_directory(
    target: LocalDownloadTarget,
    allow_existing: bool,
) -> Result<PathBuf> {
    tokio::task::spawn_blocking(move || target.prepare_directory(allow_existing))
        .await
        .context("local download directory task failed")?
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
    preserve_published: bool,
) -> Result<PathBuf> {
    if cancellation.is_cancelled() {
        let _ = await_local_writer_cleanup(writer, preserve_published).await;
        return Err(cancelled_error());
    }
    tokio::select! {
        _ = cancellation.cancelled() => {
            let _ = await_local_writer_cleanup(writer, preserve_published).await;
            Err(cancelled_error())
        }
        result = timeout_at(deadline, &mut *writer) => match result {
            Ok(joined) => joined.context("local download writer task failed")?,
            Err(_) => {
                cancellation.cancel();
                let _ = await_local_writer_cleanup(writer, preserve_published).await;
                anyhow::bail!("SFTP download exceeded the {}-second overall timeout", DOWNLOAD_TIMEOUT.as_secs())
            }
        },
    }
}

pub(super) async fn await_local_writer_cleanup(
    writer: &mut JoinHandle<Result<PathBuf>>,
    preserve_published: bool,
) -> Option<anyhow::Error> {
    match timeout(WRITER_CLEANUP_TIMEOUT, &mut *writer).await {
        Ok(Ok(Ok(path))) => {
            if !preserve_published {
                remove_local_download_best_effort(path, "transfer failed after publication").await;
            }
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

#[derive(Debug)]
pub(super) struct LocalDownloadConflict(pub(super) LocalDownloadFingerprint);

impl fmt::Display for LocalDownloadConflict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("local download target already exists")
    }
}

impl StdError for LocalDownloadConflict {}

#[derive(Debug)]
pub(super) struct DownloadSkipped;

impl fmt::Display for DownloadSkipped {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("download skipped")
    }
}

impl StdError for DownloadSkipped {}

fn local_download_fingerprint(path: &Path) -> Result<Option<LocalDownloadFingerprint>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("cannot inspect local download target"),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        anyhow::bail!("local download target is a symbolic link or non-regular file");
    }
    Ok(Some(LocalDownloadFingerprint {
        identity: local_file_identity(&metadata),
    }))
}

fn available_local_download_name(parent: &Path, name: &str) -> Result<PathBuf> {
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(name);
    let extension = path.extension().and_then(|name| name.to_str());
    for index in 1..=10_000 {
        let name = match extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = parent.join(name);
        match fs::symlink_metadata(&candidate) {
            Ok(_) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(candidate),
            Err(error) => return Err(error).context("cannot inspect numbered download target"),
        }
    }
    anyhow::bail!("cannot find a free numbered download target")
}

fn validate_local_components(components: &[String]) -> Result<()> {
    if components.is_empty() {
        anyhow::bail!("local download path has no file name");
    }
    for component in components {
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.contains(['/', '\\'])
            || component.chars().any(char::is_control)
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

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("axssh-conflict-{}", Uuid::new_v4()));
            fs::create_dir(&path).expect("test directory");
            Self(fs::canonicalize(path).expect("canonical test directory"))
        }

        fn target(&self) -> LocalDownloadTarget {
            LocalDownloadTarget::new(self.0.clone(), vec!["report.txt".into()]).expect("target")
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("fixture cleanup");
        }
    }

    #[test]
    fn download_conflict_choices_preserve_skip_number_and_replace() {
        let dir = TestDirectory::new();
        let path = dir.0.join("report.txt");
        fs::write(&path, b"original").expect("original");
        let target = dir.target();
        let conflict = target
            .prepare_with_choice(None, None, true)
            .err()
            .expect("must ask")
            .downcast::<LocalDownloadConflict>()
            .expect("typed conflict");
        assert_eq!(conflict.0.size(), 8);
        assert_eq!(fs::read(&path).expect("original"), b"original");
        let skip = target
            .prepare_with_choice(Some(SftpConflictChoice::Skip), None, true)
            .err()
            .expect("skip");
        assert!(skip.is::<DownloadSkipped>());
        fs::write(dir.0.join("report (1).txt"), b"occupied").expect("numbered name");
        let mut keep = target
            .prepare_with_choice(Some(SftpConflictChoice::KeepBoth), None, true)
            .expect("keep both");
        keep.file_mut()
            .expect("part")
            .write_all(b"copy")
            .expect("write");
        let kept = keep.finish().expect("publish copy");
        assert_eq!(kept, dir.0.join("report (2).txt"));
        assert_eq!(fs::read(&path).expect("original"), b"original");
        let mut overwrite = target
            .prepare_with_choice(Some(SftpConflictChoice::Overwrite), Some(conflict.0), true)
            .expect("overwrite");
        overwrite
            .file_mut()
            .expect("part")
            .write_all(b"new")
            .expect("write");
        assert_eq!(fs::read(&path).expect("original until commit"), b"original");
        assert_eq!(overwrite.finish().expect("atomic replace"), path);
        assert_eq!(fs::read(&path).expect("new bytes"), b"new");
        assert!(fs::read_dir(&dir.0).expect("directory").all(|entry| {
            !entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .ends_with(".part")
        }));
    }

    #[test]
    fn download_overwrite_rechecks_before_writing_and_publication() {
        let dir = TestDirectory::new();
        let path = dir.0.join("report.txt");
        fs::write(&path, b"original").expect("original");
        let original = local_download_fingerprint(&path)
            .expect("metadata")
            .expect("file");
        fs::write(&path, b"changed after prompt").expect("change");
        assert!(
            dir.target()
                .prepare_with_choice(Some(SftpConflictChoice::Overwrite), Some(original), true)
                .is_err()
        );
        let current = local_download_fingerprint(&path)
            .expect("metadata")
            .expect("file");
        let mut pending = dir
            .target()
            .prepare_with_choice(Some(SftpConflictChoice::Overwrite), Some(current), true)
            .expect("part");
        let part = pending.part_path.clone();
        pending
            .file_mut()
            .expect("part")
            .write_all(b"download")
            .expect("write");
        fs::write(&path, b"edited during download").expect("concurrent change");
        assert!(pending.finish().is_err());
        assert_eq!(
            fs::read(path).expect("kept changes"),
            b"edited during download"
        );
        assert!(!part.exists());
    }

    #[test]
    fn concurrent_keep_both_downloads_choose_distinct_numbered_names() {
        let dir = TestDirectory::new();
        fs::write(dir.0.join("report.txt"), b"original").expect("original");
        let mut first = dir
            .target()
            .prepare_with_choice(Some(SftpConflictChoice::KeepBoth), None, true)
            .expect("first");
        let mut second = dir
            .target()
            .prepare_with_choice(Some(SftpConflictChoice::KeepBoth), None, true)
            .expect("second");
        first
            .file_mut()
            .expect("first part")
            .write_all(b"first")
            .expect("write");
        second
            .file_mut()
            .expect("second part")
            .write_all(b"second")
            .expect("write");
        assert_eq!(
            first.finish().expect("first publish"),
            dir.0.join("report (1).txt")
        );
        assert_eq!(
            second.finish().expect("second publish"),
            dir.0.join("report (2).txt")
        );
        assert_eq!(
            fs::read(dir.0.join("report (1).txt")).expect("first"),
            b"first"
        );
        assert_eq!(
            fs::read(dir.0.join("report.txt")).expect("original"),
            b"original"
        );
    }

    #[tokio::test]
    async fn cancellation_after_overwrite_commit_keeps_the_published_replacement() {
        let dir = TestDirectory::new();
        let path = dir.0.join("report.txt");
        fs::write(&path, b"original").expect("original");
        let mut pending = dir
            .target()
            .prepare_with_choice(Some(SftpConflictChoice::Overwrite), None, true)
            .expect("part");
        pending
            .file_mut()
            .expect("part")
            .write_all(b"complete replacement")
            .expect("write");
        let published = pending.finish().expect("commit");
        let mut writer = tokio::task::spawn_blocking(move || Ok(published));
        let cancellation = TransferCancellation::new();
        cancellation.cancel();
        assert!(
            await_local_writer_success(
                &mut writer,
                &cancellation,
                Instant::now() + REQUEST_TIMEOUT,
                true
            )
            .await
            .is_err()
        );
        assert_eq!(
            fs::read(path).expect("keep committed replacement"),
            b"complete replacement"
        );
    }

    #[test]
    fn incomplete_or_cancelled_overwrite_keeps_original_and_removes_partial() {
        for cancelled in [false, true] {
            let dir = TestDirectory::new();
            let path = dir.0.join("report.txt");
            fs::write(&path, b"original").expect("original");
            let pending = dir
                .target()
                .prepare_with_choice(Some(SftpConflictChoice::Overwrite), None, true)
                .expect("part");
            let part = pending.part_path.clone();
            let (tx, rx) = mpsc::channel(2);
            tx.try_send(b"short".to_vec()).expect("chunk");
            drop(tx);
            let cancellation = TransferCancellation::new();
            if cancelled {
                cancellation.cancel();
            }
            assert!(write_local_file(pending, rx, 100, &cancellation).is_err());
            assert_eq!(fs::read(path).expect("kept original"), b"original");
            assert!(!part.exists());
        }
    }

    #[cfg(unix)]
    #[test]
    fn download_conflict_rejects_symlink_targets_and_parents() {
        let dir = TestDirectory::new();
        let outside = TestDirectory::new();
        let path = dir.0.join("report.txt");
        let original = outside.0.join("original.txt");
        fs::write(&original, b"outside").expect("original");
        std::os::unix::fs::symlink(&original, &path).expect("link");
        for choice in [
            None,
            Some(SftpConflictChoice::Skip),
            Some(SftpConflictChoice::KeepBoth),
            Some(SftpConflictChoice::Overwrite),
        ] {
            assert!(
                dir.target()
                    .prepare_with_choice(choice, None, true)
                    .is_err()
            );
        }
        std::os::unix::fs::symlink(&outside.0, dir.0.join("linked")).expect("parent link");
        let target =
            LocalDownloadTarget::new(dir.0.clone(), vec!["linked".into(), "original.txt".into()])
                .expect("target");
        assert!(
            target
                .prepare_with_choice(Some(SftpConflictChoice::Overwrite), None, true)
                .is_err()
        );
        assert_eq!(fs::read(original).expect("outside unchanged"), b"outside");
    }

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
    fn local_target_has_no_client_component_or_total_path_length_cap() {
        let components = vec!["n".repeat(600), "d".repeat(4_500)];
        assert!(validate_local_components(&components).is_ok());
        assert!(validate_local_components(&["line\nbreak".to_owned()]).is_err());
    }

    #[test]
    fn local_directory_target_creates_empty_directory_without_overwrite() {
        let root = std::env::temp_dir().join(format!("axssh-directory-{}", Uuid::new_v4()));
        fs::create_dir(&root).expect("fixture directory should be created");
        let target = LocalDownloadTarget::new(root.clone(), vec!["empty".to_owned()])
            .expect("directory target should validate");

        let created = target
            .prepare_directory(false)
            .expect("directory target should be created");
        assert!(created.is_dir());
        assert!(target.prepare_directory(false).is_err());
        assert!(target.prepare_directory(true).is_ok());

        fs::remove_dir_all(root).expect("fixture directory should be removed");
    }

    #[test]
    fn local_directory_target_rejects_an_existing_non_directory() {
        let root = std::env::temp_dir().join(format!("axssh-directory-{}", Uuid::new_v4()));
        fs::create_dir(&root).expect("fixture directory should be created");
        fs::write(root.join("occupied"), b"existing").expect("fixture file should be created");
        let target = LocalDownloadTarget::new(root.clone(), vec!["occupied".to_owned()])
            .expect("directory target should validate");

        assert!(target.prepare_directory(false).is_err());
        assert!(target.prepare_directory(true).is_err());
        assert!(root.join("occupied").is_file());

        fs::remove_dir_all(root).expect("fixture directory should be removed");
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
