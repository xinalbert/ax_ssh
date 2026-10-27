//! Private selection snapshots; filesystem checks run only in upload preparation.
use super::*;
use crate::local_files::{LocalDirectoryEntry, validate_local_file_for_open};

pub(in crate::controller::sftp_session) enum LocalSource {
    Entry(String, u64),
    Selection {
        directory: String,
        revision: u64,
        entries: Vec<LocalDirectoryEntry>,
    },
}

impl LocalSource {
    pub(super) fn current(&self, worker: &Worker) -> bool {
        match self {
            Self::Entry(path, revision) => {
                !worker.local_loading
                    && worker.local_revision == *revision
                    && worker.local_entries.iter().any(|entry| &entry.path == path)
            }
            Self::Selection {
                directory,
                revision,
                entries,
            } => {
                !worker.local_loading
                    && worker.local_revision == *revision
                    && worker.local_path == *directory
                    && entries
                        .iter()
                        .all(|entry| worker.local_files.get(&entry.path) == Some(entry))
            }
        }
    }

    pub(super) fn validate(&self, cancelled: &AtomicBool) -> Result<()> {
        let Self::Selection {
            directory, entries, ..
        } = self
        else {
            return Ok(());
        };
        let metadata = std::fs::symlink_metadata(directory)
            .context("cannot inspect selected upload directory")?;
        anyhow::ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "selected upload directory changed"
        );
        let canonical =
            std::fs::canonicalize(directory).context("cannot resolve selected upload directory")?;
        for entry in entries {
            anyhow::ensure!(
                !cancelled.load(Ordering::Acquire),
                "upload preparation cancelled"
            );
            if !entry.is_dir {
                validate_local_file_for_open(directory, entry)?;
                continue;
            }
            let path = Path::new(&entry.path);
            anyhow::ensure!(
                path.parent() == Some(Path::new(directory)),
                "selected upload root is outside the directory snapshot"
            );
            let metadata =
                std::fs::symlink_metadata(path).context("cannot inspect selected upload root")?;
            anyhow::ensure!(
                metadata.is_dir()
                    && !metadata.file_type().is_symlink()
                    && metadata.len() == entry.size
                    && metadata.modified().ok() == entry.modified,
                "selected upload directory changed after listing"
            );
            let resolved =
                std::fs::canonicalize(path).context("cannot resolve selected upload root")?;
            anyhow::ensure!(
                resolved.parent() == Some(canonical.as_path()),
                "selected upload directory resolved outside the snapshot"
            );
        }
        Ok(())
    }
}

fn validate_paths(paths: &[String]) -> Result<()> {
    anyhow::ensure!(
        !paths.is_empty() && paths.len() <= MAX_SFTP_UPLOAD_ROOTS,
        "selected upload requires 1–64 paths"
    );
    super::validate_paths(&paths.iter().map(PathBuf::from).collect::<Vec<_>>())?;
    anyhow::ensure!(
        paths.iter().collect::<HashSet<_>>().len() == paths.len(),
        "selected uploads contain duplicate paths"
    );
    Ok(())
}

impl SftpClient {
    pub fn upload_selected(
        &self,
        paths: Vec<String>,
        local_revision: u64,
        remote_directory: String,
        request_id: u64,
    ) -> Result<()> {
        validate_paths(&paths)?;
        crate::sftp::validate_remote_path(&remote_directory)?;
        self.send(Command::UploadSelected {
            paths,
            local_revision,
            remote_directory,
            request_id,
        })
    }
}

impl Worker {
    pub(in crate::controller::sftp_session) fn prepare_selected_upload(
        &mut self,
        paths: Vec<String>,
        local_revision: u64,
        remote_directory: String,
        request_id: u64,
    ) -> Result<()> {
        validate_paths(&paths)?;
        anyhow::ensure!(
            !self.local_loading
                && self.local_revision == local_revision
                && !self.local_path.is_empty(),
            "selected upload directory snapshot changed"
        );
        let entries = paths
            .iter()
            .map(|path| {
                self.local_files
                    .get(path)
                    .filter(|entry| !entry.is_symlink && (entry.is_dir || entry.is_regular_file()))
                    .cloned()
                    .context("selected upload entry is no longer available")
            })
            .collect::<Result<Vec<_>>>()?;
        let source = LocalSource::Selection {
            directory: self.local_path.clone(),
            revision: local_revision,
            entries,
        };
        self.prepare_upload(
            paths.into_iter().map(PathBuf::from).collect(),
            remote_directory,
            request_id,
            Some(source),
            None,
        )
    }
}

#[cfg(test)]
mod tests;
