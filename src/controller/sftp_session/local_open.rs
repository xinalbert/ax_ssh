//! One retained native-open job, bound to a directory snapshot and cancellation.
use super::*;
use crate::local_files::{ValidatedLocalFile, validate_local_file_for_open};
use std::sync::atomic::{AtomicBool, Ordering};

type Opener = Arc<dyn Fn(&Path) -> Result<()> + Send + Sync>;
type Copier =
    Arc<dyn Fn(ValidatedLocalFile, &Path, u64, &AtomicBool) -> Result<PathBuf> + Send + Sync>;

pub(super) struct Opening {
    pub(super) task: Option<JoinHandle<Completion>>,
    pub(super) deadline: Option<tokio::time::Instant>,
    cancelled: Option<Arc<AtomicBool>>,
    root: Option<PathBuf>,
    opener: Opener,
    copier: Copier,
}

impl Default for Opening {
    fn default() -> Self {
        Self {
            task: None,
            deadline: None,
            cancelled: None,
            root: None,
            copier: Arc::new(|mut file, root, limit, cancelled| {
                crate::sftp::snapshot_local_file_for_open_at(
                    &mut file.file,
                    &file.name,
                    root,
                    limit,
                    cancelled,
                )
            }),
            opener: Arc::new(|path| {
                #[cfg(windows)]
                let result = open::that(path);
                #[cfg(not(windows))]
                let result = open::that_detached(path);
                result.context("cannot open system-associated application")
            }),
        }
    }
}

pub(super) struct Completion {
    pub(super) generation: u64,
    pub(super) revision: u64,
    pub(super) result: Result<String>,
    pub(super) cancelled: bool,
}

impl Opening {
    pub(super) fn cancel(&mut self) {
        if let Some(cancelled) = &self.cancelled {
            cancelled.store(true, Ordering::Release);
        }
        self.deadline = None;
    }
    pub(super) async fn finish(&mut self) -> Result<()> {
        self.cancel();
        if let Some(task) = self.task.take() {
            task.await
                .context("local file open cleanup failed")?
                .result?;
        }
        self.cancelled = None;
        self.deadline = None;
        Ok(())
    }
    fn root(&self) -> Result<PathBuf> {
        if let Some(root) = &self.root {
            return Ok(root.clone());
        }
        Ok(
            directories::ProjectDirs::from("com", "axsoft", "ax_ssh_egui")
                .context("cannot determine experimental local open directory")?
                .cache_dir()
                .join("sftp-open"),
        )
    }
}

impl Worker {
    pub(super) fn open_local(&mut self, path: String, revision: u64) -> Result<()> {
        anyhow::ensure!(
            !self.local_loading && revision == self.local_revision,
            "local directory changed; refresh before opening"
        );
        anyhow::ensure!(
            self.local_open.task.is_none(),
            "another local file open is pending"
        );
        let entry = self
            .local_files
            .get(&path)
            .cloned()
            .context("file is no longer in the local directory snapshot")?;
        anyhow::ensure!(
            entry.is_regular_file(),
            "local entry is not an ordinary file"
        );
        let limit = self.settings.transfers.local_open_max_file_bytes();
        anyhow::ensure!(
            entry.size <= limit,
            "local file exceeds the saved open limit"
        );
        let cancelled = Arc::new(AtomicBool::new(false));
        self.local_open.cancelled = Some(cancelled.clone());
        let root = self.local_open.root()?;
        let directory = self.local_path.clone();
        let copier = self.local_open.copier.clone();
        let opener = self.local_open.opener.clone();
        let generation = self.generation;
        self.status = format!("Opening local file {}…", entry.name)
            .chars()
            .take(512)
            .collect();
        self.local_open.deadline =
            Some(tokio::time::Instant::now() + std::time::Duration::from_secs(30 * 60));
        self.local_open.task = Some(self.runtime.spawn_blocking(move || {
            let result = (|| {
                if cancelled.load(Ordering::Acquire) {
                    return Ok("Local file opening cancelled".into());
                }
                let validated = validate_local_file_for_open(&directory, &entry)?;
                let snapshot = match copier(validated, &root, limit, &cancelled) {
                    Ok(path) => path,
                    Err(_) if cancelled.load(Ordering::Acquire) => {
                        return Ok("Local file opening cancelled".into());
                    }
                    Err(error) => return Err(error),
                };
                let result = (|| {
                    let metadata = std::fs::symlink_metadata(&snapshot)
                        .context("cannot inspect private open copy")?;
                    anyhow::ensure!(
                        metadata.is_file()
                            && !metadata.file_type().is_symlink()
                            && metadata.len() <= limit,
                        "private open copy is not a regular file within the saved limit"
                    );
                    if cancelled.load(Ordering::Acquire) {
                        return Ok(false);
                    }
                    opener(&snapshot)?;
                    Ok(true)
                })();
                if !matches!(result, Ok(true)) {
                    std::fs::remove_file(&snapshot)
                        .context("cannot remove unused private open copy")?;
                }
                if !result? {
                    return Ok("Local file opening cancelled".into());
                }
                Ok(format!("Opened local file {}", entry.name))
            })();
            Completion {
                generation,
                revision,
                result,
                cancelled: cancelled.load(Ordering::Acquire),
            }
        }));
        Ok(())
    }
}

pub(super) async fn next_job(
    task: &mut Option<JoinHandle<Completion>>,
) -> std::result::Result<Completion, tokio::task::JoinError> {
    match task {
        Some(task) => task.await,
        None => std::future::pending().await,
    }
}

pub(super) async fn deadline(at: Option<tokio::time::Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests;
