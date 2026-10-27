//! Snapshot-bound root admission. Recursive discovery and file I/O stay in the worker.
use super::*;

pub const MAX_SFTP_DOWNLOAD_ROOTS: usize = 64;
const MAX_PATH_TEXT: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SftpDownloadFailure {
    pub path: String,
    pub message: String,
}

/// Latest root enqueue summary, never a file-completion receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SftpDownloadBatchResult {
    pub requested: usize,
    pub queued: usize,
    pub skipped: usize,
    pub failures: Vec<SftpDownloadFailure>,
}

fn validate(paths: &[String]) -> Result<()> {
    anyhow::ensure!(
        !paths.is_empty() && paths.len() <= MAX_SFTP_DOWNLOAD_ROOTS,
        "download requires 1–64 remote paths"
    );
    let mut unique = HashSet::new();
    let mut bytes = 0usize;
    for path in paths {
        crate::sftp::validate_remote_path(path)?;
        let name = path.rsplit('/').next().unwrap_or_default();
        anyhow::ensure!(
            !matches!(name, "" | "." | "..") && unique.insert(path),
            "invalid or duplicate remote download path"
        );
        bytes = bytes.saturating_add(path.len());
    }
    anyhow::ensure!(bytes <= MAX_PATH_TEXT, "download path text exceeds 256 KiB");
    Ok(())
}

impl SftpClient {
    pub fn download_selected(
        &self,
        paths: Vec<String>,
        request_id: u64,
        local_directory: String,
        local_revision: u64,
    ) -> Result<()> {
        validate(&paths)?;
        validate_local_directory_input(Path::new(&local_directory))?;
        self.send(Command::DownloadSelected {
            paths,
            request_id,
            local_directory,
            local_revision,
        })
    }
}

fn admit(
    paths: &[String],
    mut current: impl FnMut(&str) -> bool,
    mut send: impl FnMut(&str) -> Result<()>,
) -> SftpDownloadBatchResult {
    let mut result = SftpDownloadBatchResult {
        requested: paths.len(),
        queued: 0,
        skipped: 0,
        failures: vec![],
    };
    for path in paths {
        if !current(path) {
            result.skipped += 1;
            continue;
        }
        match send(path) {
            Ok(()) => result.queued += 1,
            Err(error) => result.failures.push(SftpDownloadFailure {
                path: path.clone(),
                message: format!("{error:#}").chars().take(512).collect(),
            }),
        }
    }
    result
}

impl Worker {
    pub(super) fn download_selected(
        &mut self,
        paths: Vec<String>,
        request_id: u64,
        directory: String,
        revision: u64,
    ) -> Result<()> {
        validate(&paths)?;
        anyhow::ensure!(
            self.connected
                && !self.loading
                && self.prompt.is_none()
                && self.request_id == request_id,
            "remote selection snapshot changed"
        );
        anyhow::ensure!(
            !self.local_loading
                && !directory.is_empty()
                && self.local_path == directory
                && self.local_revision == revision,
            "download target directory changed"
        );
        let filters = self.settings.filters.effective_patterns();
        let allowed: HashSet<_> = self
            .entries
            .iter()
            .filter(|entry| {
                !entry.is_symlink
                    && !crate::sftp::transfer_name_matches_filter(&entry.name, &filters)
            })
            .map(|entry| entry.path.clone())
            .collect();
        let result = admit(
            &paths,
            |path| allowed.contains(path),
            |path| self.queue_download(path.into(), PathBuf::from(&directory)),
        );
        self.status = format!(
            "Download roots: queued={}, skipped={}, failed={}",
            result.queued,
            result.skipped,
            result.failures.len()
        );
        self.last_download_batch = Some(result);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
