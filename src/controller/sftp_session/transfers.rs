//! Snapshot-bound drag transfers and retained blocking upload preparation.

use super::*;
use crate::sftp::SftpUploadRequest;
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_ROOTS: usize = 64;
const MAX_PATH_TEXT: usize = 256 * 1024;

#[derive(Default)]
pub(super) struct Preparation {
    pub(super) task: Option<JoinHandle<Prepared>>,
    cancelled: Option<Arc<AtomicBool>>,
}

pub(super) struct Prepared {
    generation: u64,
    request_id: u64,
    directory: String,
    local_source: Option<(String, u64)>,
    cancelled: Arc<AtomicBool>,
    result: Result<Vec<SftpUploadRequest>>,
}

impl Preparation {
    pub(super) fn active(&self) -> bool {
        self.task.is_some()
    }
    pub(super) fn cancel(&self) {
        if let Some(cancelled) = &self.cancelled {
            cancelled.store(true, Ordering::Release);
        }
    }
    pub(super) async fn finish(&mut self) -> Result<()> {
        self.cancel();
        if let Some(task) = self.task.take() {
            task.await.context("upload preparation cleanup failed")?;
        }
        self.cancelled = None;
        Ok(())
    }
}

pub(super) fn validate_paths(paths: &[PathBuf]) -> Result<()> {
    anyhow::ensure!(
        !paths.is_empty() && paths.len() <= MAX_ROOTS,
        "drop must contain 1–64 local paths"
    );
    let mut bytes = 0usize;
    for path in paths {
        let text = path.to_str().context("upload path must be UTF-8")?;
        anyhow::ensure!(
            !text.is_empty()
                && text.len() <= MAX_LOCAL_PATH_BYTES
                && !text.chars().any(char::is_control),
            "invalid upload path"
        );
        bytes = bytes.saturating_add(text.len());
    }
    anyhow::ensure!(bytes <= MAX_PATH_TEXT, "drop path text exceeds the limit");
    Ok(())
}

impl Worker {
    fn upload_target_current(
        &self,
        generation: u64,
        request_id: u64,
        directory: &str,
        source: Option<&(String, u64)>,
    ) -> bool {
        self.connected
            && generation == self.generation
            && !self.loading
            && self.request_id == request_id
            && !directory.is_empty()
            && self.path == directory
            && source.is_none_or(|(path, revision)| {
                !self.local_loading
                    && self.local_revision == *revision
                    && self.local_entries.iter().any(|entry| &entry.path == path)
            })
    }

    pub(super) fn prepare_upload(
        &mut self,
        paths: Vec<PathBuf>,
        directory: String,
        request_id: u64,
        local_source: Option<(String, u64)>,
        explicit_target: Option<String>,
    ) -> Result<()> {
        validate_paths(&paths)?;
        anyhow::ensure!(
            self.upload_target_current(
                self.generation,
                request_id,
                &directory,
                local_source.as_ref()
            ),
            "upload directory or source snapshot changed"
        );
        anyhow::ensure!(
            !self.preparation.active(),
            "another upload is being prepared"
        );
        let worker = self.transport.as_ref().context("SFTP is disconnected")?;
        let max_file_bytes = worker.sftp_max_file_bytes();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation = cancelled.clone();
        let generation = self.generation;
        self.preparation.cancelled = Some(cancelled.clone());
        self.preparation.task = Some(self.runtime.spawn_blocking(move || {
            let result = (|| {
                let filters =
                    crate::config::SftpTransferFilterSettings::default().effective_patterns();
                let mut files = crate::local_upload::discover_local_uploads_cancellable(
                    paths,
                    &directory,
                    &filters,
                    max_file_bytes,
                    &cancellation,
                )?;
                if let Some(target) = explicit_target {
                    anyhow::ensure!(
                        files.len() == 1 && files[0].remote_directories.is_empty(),
                        "explicit upload target requires one regular file"
                    );
                    crate::sftp::validate_remote_path(&target)?;
                    files[0].remote_path = target;
                }
                anyhow::ensure!(
                    files.len() <= MAX_TRANSFERS,
                    "upload batch exceeds the 64-transfer companion limit"
                );
                let batch = Uuid::new_v4();
                files
                    .into_iter()
                    .map(|file| {
                        anyhow::ensure!(
                            !cancellation.load(Ordering::Acquire),
                            "upload preparation cancelled"
                        );
                        SftpUploadRequest::from_local_file(
                            file.id,
                            batch,
                            file.remote_path,
                            file.local_path,
                            file.size,
                            file.remote_directories,
                        )
                    })
                    .collect()
            })();
            Prepared {
                generation,
                request_id,
                directory,
                local_source,
                cancelled,
                result,
            }
        }));
        self.status = "Preparing upload…".into();
        Ok(())
    }

    pub(super) fn complete_preparation(&mut self, prepared: Prepared) -> Result<()> {
        self.preparation.cancelled = None;
        if prepared.cancelled.load(Ordering::Acquire) {
            return Ok(());
        }
        anyhow::ensure!(
            self.upload_target_current(
                prepared.generation,
                prepared.request_id,
                &prepared.directory,
                prepared.local_source.as_ref()
            ),
            "upload preparation discarded: directory or connection changed"
        );
        let requests = prepared.result?;
        anyhow::ensure!(
            self.transfers.len().saturating_add(requests.len()) <= MAX_TRANSFERS,
            "SFTP transfer history is full"
        );
        let items: Vec<_> = requests
            .iter()
            .map(|request| SftpTransferView {
                id: request.transfer_id(),
                name: request.name().into(),
                status: "Queued upload".into(),
                downloaded_bytes: 0,
                total_bytes: request.total_bytes(),
                conflict: false,
            })
            .collect();
        self.transport
            .as_ref()
            .context("SFTP is disconnected")?
            .request_prepared_sftp_uploads(requests)?;
        for item in items {
            self.transfers.insert(item.id, item);
        }
        self.status = "Upload batch queued".into();
        Ok(())
    }

    pub(super) fn validate_download_drop(
        &self,
        path: &str,
        request_id: u64,
        directory: &str,
        revision: u64,
    ) -> Result<()> {
        anyhow::ensure!(
            self.connected && !self.loading && self.request_id == request_id,
            "remote drag snapshot changed"
        );
        anyhow::ensure!(
            !self.local_loading
                && !directory.is_empty()
                && self.local_path == directory
                && self.local_revision == revision,
            "download target directory changed"
        );
        anyhow::ensure!(
            self.entries
                .iter()
                .any(|entry| entry.path == path && !entry.is_symlink),
            "remote drag entry is no longer available"
        );
        Ok(())
    }

    pub(super) fn queue_download(&mut self, path: String, local_directory: PathBuf) -> Result<()> {
        anyhow::ensure!(
            self.connected && !self.loading,
            "SFTP directory is not ready"
        );
        anyhow::ensure!(
            self.transfers.len() < MAX_TRANSFERS,
            "SFTP transfer history is full"
        );
        let id = Uuid::new_v4();
        self.transport
            .as_ref()
            .context("SFTP is disconnected")?
            .request_open_sftp_file(
                id,
                path.clone(),
                local_directory,
                crate::config::SftpTransferFilterSettings::default().effective_patterns(),
            )?;
        self.transfers.insert(
            id,
            SftpTransferView {
                id,
                name: path.rsplit('/').next().unwrap_or("download").into(),
                status: "Queued download".into(),
                downloaded_bytes: 0,
                total_bytes: 0,
                conflict: false,
            },
        );
        Ok(())
    }
}

pub(super) async fn next_preparation(
    task: &mut Option<JoinHandle<Prepared>>,
) -> std::result::Result<Prepared, tokio::task::JoinError> {
    match task {
        Some(task) => task.await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn shutdown_revokes_and_joins_an_inflight_preparation() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let task = tokio::task::spawn_blocking(move || {
            receiver.recv().unwrap();
            assert!(worker_cancelled.load(Ordering::Acquire));
            Prepared {
                generation: 1,
                request_id: 1,
                directory: "/remote".into(),
                local_source: None,
                cancelled: worker_cancelled,
                result: Ok(Vec::new()),
            }
        });
        let mut preparation = Preparation {
            task: Some(task),
            cancelled: Some(cancelled.clone()),
        };
        let finish = tokio::spawn(async move { preparation.finish().await });
        tokio::task::yield_now().await;
        assert!(cancelled.load(Ordering::Acquire));
        assert!(!finish.is_finished());
        sender.send(()).unwrap();
        finish.await.unwrap().unwrap();
    }

    #[test]
    fn drop_inputs_are_bounded_before_entering_the_command_queue() {
        assert!(validate_paths(&[]).is_err());
        assert!(validate_paths(&vec![PathBuf::from("/fixture/file"); 65]).is_err());
        assert!(validate_paths(&[PathBuf::from("/fixture/bad\npath")]).is_err());
        assert!(validate_paths(&[PathBuf::from("x".repeat(4097))]).is_err());
        assert!(validate_paths(&[PathBuf::from("/fixture/normal file")]).is_ok());
    }
}
