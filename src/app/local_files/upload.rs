//! Bounded local upload-tree discovery without UI or worker state.

use super::*;
use std::fs::ReadDir;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

fn join_remote_upload_path(directory: &str, name: &str) -> String {
    if directory == "/" || directory.is_empty() {
        format!("/{name}")
    } else {
        format!("{}/{}", directory.trim_end_matches('/'), name)
    }
}

#[derive(Clone)]
pub(in crate::app) struct PreparedLocalUpload {
    pub(in crate::app) id: Uuid,
    pub(in crate::app) local_path: PathBuf,
    pub(in crate::app) remote_path: String,
    pub(in crate::app) remote_directories: Vec<String>,
    pub(in crate::app) size: u64,
}

pub(in crate::app) struct UploadDiscoverySummary {
    pub(in crate::app) discovered_files: usize,
    pub(in crate::app) skipped_entries: usize,
    pub(in crate::app) first_issue: Option<String>,
}

fn record_upload_discovery_issue(
    skipped_entries: &mut usize,
    first_issue: &mut Option<String>,
    error: impl std::fmt::Display,
) {
    *skipped_entries = (*skipped_entries).saturating_add(1);
    if first_issue.is_none() {
        *first_issue = Some(error.to_string());
    }
}

struct UploadDirectoryNode {
    remote_path: String,
    parent: Option<Arc<UploadDirectoryNode>>,
}

enum UploadWalkFrame {
    Visit(PathBuf, Option<Arc<UploadDirectoryNode>>),
    ReadDirectory(Box<ReadDir>, Arc<UploadDirectoryNode>),
}

pub(in crate::app) fn discover_local_uploads_streaming(
    paths: Vec<PathBuf>,
    remote_directory: &str,
    filters: &[String],
    output: &mpsc::Sender<PreparedLocalUpload>,
) -> Result<UploadDiscoverySummary> {
    let mut stack = Vec::with_capacity(paths.len());
    let mut discovered_files = 0_usize;
    let mut skipped_entries = 0_usize;
    let mut first_issue = None;
    for path in paths.into_iter().rev() {
        if let Err(error) = upload_local_name(&path) {
            let error = error;
            record_upload_discovery_issue(
                &mut skipped_entries,
                &mut first_issue,
                error.context(format!("invalid local upload root {path:?}")),
            );
            continue;
        }
        stack.push(UploadWalkFrame::Visit(path, None));
    }
    while let Some(frame) = stack.pop() {
        match frame {
            UploadWalkFrame::ReadDirectory(mut entries, parent) => match entries.next() {
                Some(Ok(entry)) => {
                    stack.push(UploadWalkFrame::ReadDirectory(entries, parent.clone()));
                    stack.push(UploadWalkFrame::Visit(entry.path(), Some(parent)));
                }
                Some(Err(error)) => {
                    record_upload_discovery_issue(
                        &mut skipped_entries,
                        &mut first_issue,
                        format!("cannot read local upload directory: {error}"),
                    );
                    stack.push(UploadWalkFrame::ReadDirectory(entries, parent));
                }
                None => {}
            },
            UploadWalkFrame::Visit(path, parent) => {
                let metadata = match std::fs::symlink_metadata(&path)
                    .with_context(|| format!("cannot inspect local upload path {path:?}"))
                {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        record_upload_discovery_issue(
                            &mut skipped_entries,
                            &mut first_issue,
                            error,
                        );
                        continue;
                    }
                };
                if metadata.file_type().is_symlink() {
                    record_upload_discovery_issue(
                        &mut skipped_entries,
                        &mut first_issue,
                        format!("local upload tree contains a symbolic link: {path:?}"),
                    );
                    continue;
                }
                let name = match upload_local_name(&path) {
                    Ok(name) => name,
                    Err(error) => {
                        record_upload_discovery_issue(
                            &mut skipped_entries,
                            &mut first_issue,
                            error.context(format!("invalid local upload path {path:?}")),
                        );
                        continue;
                    }
                };
                if ax_ssh::sftp::transfer_name_matches_filter(&name, filters) {
                    record_upload_discovery_issue(
                        &mut skipped_entries,
                        &mut first_issue,
                        format!("Excluded by transfer filter: {path:?}"),
                    );
                    continue;
                }
                if metadata.is_dir() {
                    let remote_path = match &parent {
                        Some(parent) => join_remote_upload_path(&parent.remote_path, &name),
                        None => join_remote_upload_path(remote_directory, &name),
                    };
                    let directory = Arc::new(UploadDirectoryNode {
                        remote_path,
                        parent,
                    });
                    match std::fs::read_dir(&path)
                        .with_context(|| format!("cannot read local upload directory {path:?}"))
                    {
                        Ok(entries) => {
                            stack.push(UploadWalkFrame::ReadDirectory(Box::new(entries), directory))
                        }
                        Err(error) => record_upload_discovery_issue(
                            &mut skipped_entries,
                            &mut first_issue,
                            error,
                        ),
                    }
                } else if metadata.is_file() {
                    let directory = parent
                        .as_ref()
                        .map(|node| node.remote_path.as_str())
                        .unwrap_or(remote_directory);
                    let mut remote_directories = Vec::new();
                    let mut current = parent.as_deref();
                    while let Some(node) = current {
                        remote_directories.push(node.remote_path.clone());
                        current = node.parent.as_deref();
                    }
                    remote_directories.reverse();
                    let file = PreparedLocalUpload {
                        id: Uuid::new_v4(),
                        local_path: path,
                        remote_path: join_remote_upload_path(directory, &name),
                        remote_directories,
                        size: metadata.len(),
                    };
                    output
                        .blocking_send(file)
                        .context("SFTP upload queue stopped during local discovery")?;
                    discovered_files = discovered_files.saturating_add(1);
                } else {
                    record_upload_discovery_issue(
                        &mut skipped_entries,
                        &mut first_issue,
                        format!("local upload tree contains an unsupported file type: {path:?}"),
                    );
                }
            }
        }
    }
    Ok(UploadDiscoverySummary {
        discovered_files,
        skipped_entries,
        first_issue,
    })
}

fn upload_local_name(path: &Path) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty() && *value != "." && *value != "..")
        .context("local upload path has no valid UTF-8 name")?;
    if name.chars().any(char::is_control) || name.contains(['/', '\\']) {
        anyhow::bail!("local upload name is invalid");
    }
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests;
