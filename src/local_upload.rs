//! Bounded local upload-tree discovery without UI or worker state.

use anyhow::{Context, Result};
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const MAX_LOCAL_UPLOAD_FILES: usize = 512;
const MAX_UPLOAD_SCAN_ENTRIES: usize = 4096;
const MAX_UPLOAD_DIRECTORIES: usize = 256;
const MAX_UPLOAD_DEPTH: usize = 16;
const MAX_UPLOAD_TEXT_BYTES: usize = 512 * 1024;

fn join_remote_upload_path(directory: &str, name: &str) -> String {
    if directory == "/" || directory.is_empty() {
        format!("/{name}")
    } else {
        format!("{}/{}", directory.trim_end_matches('/'), name)
    }
}

pub struct PreparedLocalUpload {
    pub id: Uuid,
    pub local_path: PathBuf,
    pub remote_path: String,
    pub remote_directories: Vec<String>,
    pub size: u64,
}

pub fn discover_local_uploads(
    paths: Vec<PathBuf>,
    remote_directory: &str,
    filters: &[String],
    max_file_bytes: u64,
) -> Result<Vec<PreparedLocalUpload>> {
    discover_local_uploads_cancellable(
        paths,
        remote_directory,
        filters,
        max_file_bytes,
        &std::sync::atomic::AtomicBool::new(false),
    )
}

pub(crate) fn discover_local_uploads_cancellable(
    paths: Vec<PathBuf>,
    remote_directory: &str,
    filters: &[String],
    max_file_bytes: u64,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<Vec<PreparedLocalUpload>> {
    crate::sftp::validate_remote_path(remote_directory)?;
    anyhow::ensure!(
        !paths.is_empty() && paths.len() <= MAX_LOCAL_UPLOAD_FILES,
        "upload root count exceeds the limit"
    );
    let mut pending = VecDeque::new();
    let mut root_names = HashSet::new();
    let mut files = Vec::new();
    let mut scanned = 0_usize;
    let mut directories = 0_usize;
    let mut text_bytes = 0_usize;
    for path in paths {
        let name = upload_local_name(&path)?;
        if !root_names.insert(name.clone()) {
            anyhow::bail!("selected upload roots contain duplicate names");
        }
        pending.push_back((path, Vec::<String>::new(), 0_usize));
    }
    while let Some((path, parents, depth)) = pending.pop_front() {
        anyhow::ensure!(
            !cancelled.load(std::sync::atomic::Ordering::Acquire),
            "upload preparation cancelled"
        );
        scanned += 1;
        if scanned > MAX_UPLOAD_SCAN_ENTRIES {
            anyhow::bail!(
                "local upload tree exceeds the {MAX_UPLOAD_SCAN_ENTRIES}-entry scan limit"
            );
        }
        let metadata = std::fs::symlink_metadata(&path)
            .with_context(|| format!("cannot inspect local upload path {path:?}"))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let name = upload_local_name(&path)?;
        if crate::sftp::transfer_name_matches_filter(&name, filters) {
            continue;
        }
        text_bytes = text_bytes
            .saturating_add(path.as_os_str().len())
            .saturating_add(name.len());
        if text_bytes > MAX_UPLOAD_TEXT_BYTES {
            anyhow::bail!("local upload tree exceeds the {MAX_UPLOAD_TEXT_BYTES}-byte path budget");
        }
        let mut remote = remote_directory.to_owned();
        for parent in &parents {
            remote = join_remote_upload_path(&remote, parent);
        }
        if metadata.is_dir() {
            if depth >= MAX_UPLOAD_DEPTH || directories >= MAX_UPLOAD_DIRECTORIES {
                anyhow::bail!("local upload tree exceeds the directory depth or count limit");
            }
            directories += 1;
            let mut children = Vec::new();
            for item in std::fs::read_dir(&path)
                .with_context(|| format!("cannot read local upload directory {path:?}"))?
            {
                anyhow::ensure!(
                    !cancelled.load(std::sync::atomic::Ordering::Acquire),
                    "upload preparation cancelled"
                );
                let entry =
                    item.with_context(|| format!("cannot read local upload directory {path:?}"))?;
                if scanned
                    .saturating_add(pending.len())
                    .saturating_add(children.len())
                    >= MAX_UPLOAD_SCAN_ENTRIES
                {
                    anyhow::bail!(
                        "local upload tree exceeds the {MAX_UPLOAD_SCAN_ENTRIES}-entry scan limit"
                    );
                }
                children.push(entry.path());
            }
            children.sort();
            let mut child_parents = parents;
            child_parents.push(name);
            for child in children {
                pending.push_back((child, child_parents.clone(), depth + 1));
            }
        } else if metadata.is_file() {
            if metadata.len() > max_file_bytes || files.len() >= MAX_LOCAL_UPLOAD_FILES {
                anyhow::bail!("local upload tree exceeds the file size or count limit");
            }
            let mut remote_directories = Vec::new();
            let mut directory = remote_directory.to_owned();
            for parent in &parents {
                directory = join_remote_upload_path(&directory, parent);
                remote_directories.push(directory.clone());
            }
            files.push(PreparedLocalUpload {
                id: Uuid::new_v4(),
                local_path: path,
                remote_path: join_remote_upload_path(&remote, &name),
                remote_directories,
                size: metadata.len(),
            });
        }
    }
    if files.is_empty() {
        anyhow::bail!("selected upload contains no eligible regular files");
    }
    Ok(files)
}

fn upload_local_name(path: &Path) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty() && *value != "." && *value != "..")
        .context("local upload path has no valid UTF-8 name")?;
    if name.chars().any(char::is_control)
        || name.contains(['/', '\\'])
        || name.chars().count() > 512
    {
        anyhow::bail!("local upload name is invalid or too long");
    }
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests;
