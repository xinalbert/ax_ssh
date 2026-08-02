use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use directories::UserDirs;

const LOCAL_DIRECTORY_ENTRY_LIMIT: usize = 250;
const LOCAL_DIRECTORY_NAME_LIMIT: usize = 256;
const LOCAL_DIRECTORY_NAME_BUDGET: usize = 64 * 1024;
pub(super) const LOCAL_DIRECTORY_PATH_LIMIT: usize = 4 * 1024;

#[derive(Clone, Debug)]
pub(super) struct LocalDirectoryEntry {
    pub(super) name: String,
    pub(super) path: String,
    pub(super) is_dir: bool,
    pub(super) is_symlink: bool,
    pub(super) size: u64,
    pub(super) modified: Option<SystemTime>,
}

pub(super) struct LocalDirectoryListing {
    pub(super) path: String,
    pub(super) entries: Vec<LocalDirectoryEntry>,
    pub(super) truncated: bool,
}

pub(super) fn default_local_directory() -> String {
    if let Some(directories) = UserDirs::new() {
        return directories.home_dir().display().to_string();
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("/"))
        .display()
        .to_string()
}

pub(super) fn read_local_directory(path: &str) -> Result<LocalDirectoryListing> {
    let path = path.trim();
    if path.is_empty() || !is_safe_display_text(path) {
        bail!("local path is empty or contains a control character");
    }
    if path.len() > LOCAL_DIRECTORY_PATH_LIMIT {
        bail!("local path is too long");
    }

    let resolved =
        fs::canonicalize(path).with_context(|| format!("cannot resolve local directory {path}"))?;
    if !resolved.is_dir() {
        bail!("local path is not a directory");
    }

    let entries = fs::read_dir(&resolved)
        .with_context(|| format!("cannot read local directory {}", resolved.display()))?;
    let mut listed = Vec::new();
    let mut name_budget = 0usize;
    let mut truncated = false;

    for item in entries {
        if listed.len() == LOCAL_DIRECTORY_ENTRY_LIMIT {
            truncated = true;
            break;
        }
        let Ok(item) = item else {
            truncated = true;
            continue;
        };
        let name = item.file_name().to_string_lossy().to_string();
        let name_len = name.chars().count();
        if name_len == 0
            || !is_safe_display_text(&name)
            || name_len > LOCAL_DIRECTORY_NAME_LIMIT
            || name_budget.saturating_add(name_len) > LOCAL_DIRECTORY_NAME_BUDGET
        {
            truncated = true;
            continue;
        }
        let path = item.path().display().to_string();
        if !is_safe_display_text(&path) || path.len() > LOCAL_DIRECTORY_PATH_LIMIT {
            truncated = true;
            continue;
        }
        let file_type = match item.file_type() {
            Ok(file_type) => file_type,
            Err(_) => {
                truncated = true;
                continue;
            }
        };
        let metadata = item.metadata().ok();
        name_budget = name_budget.saturating_add(name_len);
        listed.push(LocalDirectoryEntry {
            name,
            path,
            is_dir: file_type.is_dir(),
            is_symlink: file_type.is_symlink(),
            size: metadata.as_ref().map_or(0, fs::Metadata::len),
            modified: metadata.and_then(|metadata| metadata.modified().ok()),
        });
    }

    listed.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(LocalDirectoryListing {
        path: resolved.display().to_string(),
        entries: listed,
        truncated,
    })
}

fn is_safe_display_text(value: &str) -> bool {
    !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_directory_listing_is_sorted_and_keeps_metadata() {
        let directory =
            std::env::temp_dir().join(format!("ax-ssh-local-files-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).expect("test directory should be created");
        fs::create_dir(directory.join("folder")).expect("test folder should be created");
        fs::write(directory.join("notes.txt"), b"notes").expect("test file should be written");

        let listing = read_local_directory(&directory.display().to_string())
            .expect("test directory should be listed");

        assert_eq!(listing.entries.len(), 2);
        assert_eq!(listing.entries[0].name, "folder");
        assert!(listing.entries[0].is_dir);
        assert_eq!(listing.entries[1].name, "notes.txt");
        assert_eq!(listing.entries[1].size, 5);
        assert!(listing.entries[1].modified.is_some());

        fs::remove_dir_all(&directory).expect("test directory should be removed");
    }

    #[test]
    fn local_directory_text_rejects_control_characters() {
        assert!(!is_safe_display_text("bad\nname"));
        assert!(!is_safe_display_text("bad\u{1b}name"));
        assert!(is_safe_display_text("normal-name"));
    }
}
