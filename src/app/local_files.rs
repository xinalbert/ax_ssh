use std::fs::{self, File};
use std::path::{Path, PathBuf};
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
    fingerprint: Option<LocalFileFingerprint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LocalFileFingerprint {
    length: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    volume_serial: u32,
    #[cfg(windows)]
    file_index: u64,
}

#[derive(Debug)]
pub(super) struct ValidatedLocalFile {
    pub(super) file: File,
    pub(super) name: String,
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
        let metadata = match item.metadata() {
            Ok(metadata) => metadata,
            Err(_) => {
                truncated = true;
                continue;
            }
        };
        let fingerprint = if file_type.is_file() {
            let file = match File::open(&path) {
                Ok(file) => file,
                Err(_) => {
                    truncated = true;
                    continue;
                }
            };
            match local_file_fingerprint(&file) {
                Ok(fingerprint) => Some(fingerprint),
                Err(_) => {
                    truncated = true;
                    continue;
                }
            }
        } else {
            None
        };

        name_budget = name_budget.saturating_add(name_len);
        listed.push(LocalDirectoryEntry {
            name,
            path,
            is_dir: file_type.is_dir(),
            is_symlink: file_type.is_symlink(),
            size: metadata.len(),
            modified: metadata.modified().ok(),
            fingerprint,
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

pub(super) fn validate_local_file_for_open(
    directory: &str,
    entry: &LocalDirectoryEntry,
) -> Result<ValidatedLocalFile> {
    if entry.is_dir {
        bail!("local entry is a directory");
    }
    if entry.is_symlink {
        bail!("symbolic links cannot be opened from SFTP in this version");
    }

    let directory_path = Path::new(directory);
    let directory_metadata = fs::symlink_metadata(directory_path)
        .with_context(|| format!("cannot inspect local directory {directory}"))?;
    if directory_metadata.file_type().is_symlink() || !directory_metadata.is_dir() {
        bail!("local directory changed after it was listed");
    }
    let canonical_directory = fs::canonicalize(directory_path)
        .with_context(|| format!("cannot resolve local directory {directory}"))?;

    let entry_path = Path::new(&entry.path);
    if entry_path.parent() != Some(directory_path) {
        bail!("local entry is outside the current directory snapshot");
    }
    let entry_metadata = fs::symlink_metadata(entry_path)
        .with_context(|| format!("cannot inspect local file {}", entry.path))?;
    if entry_metadata.file_type().is_symlink() {
        bail!("local file became a symbolic link after it was listed");
    }
    if !entry_metadata.is_file() {
        bail!("local entry is no longer a regular file");
    }

    let file =
        File::open(entry_path).with_context(|| format!("cannot open local file {}", entry.path))?;
    let opened_metadata = file
        .metadata()
        .with_context(|| format!("cannot inspect opened local file {}", entry.path))?;
    let expected_fingerprint = entry
        .fingerprint
        .context("local file fingerprint was unavailable in the directory snapshot")?;
    if !opened_metadata.is_file() || local_file_fingerprint(&file)? != expected_fingerprint {
        bail!("local file changed after it was listed");
    }

    let canonical_entry = fs::canonicalize(entry_path)
        .with_context(|| format!("cannot resolve local file {}", entry.path))?;
    if canonical_entry.parent() != Some(canonical_directory.as_path()) {
        bail!("local file resolved outside the current directory snapshot");
    }
    Ok(ValidatedLocalFile {
        file,
        name: entry.name.clone(),
    })
}

fn local_file_fingerprint(file: &File) -> Result<LocalFileFingerprint> {
    let metadata = file
        .metadata()
        .context("cannot inspect local file metadata")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(LocalFileFingerprint {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle as _;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };

        let mut information = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: `file` owns a valid Windows file handle for this call, and
        // `information` is initialized writable storage of the documented type.
        let succeeded =
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), &raw mut information) != 0 };
        if !succeeded {
            return Err(std::io::Error::last_os_error())
                .context("cannot inspect Windows local file identity");
        }
        Ok(LocalFileFingerprint {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            volume_serial: information.dwVolumeSerialNumber,
            file_index: (u64::from(information.nFileIndexHigh) << 32)
                | u64::from(information.nFileIndexLow),
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(LocalFileFingerprint {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
        })
    }
}

fn is_safe_display_text(value: &str) -> bool {
    !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    fn fixture_entry(path: &Path, name: &str) -> LocalDirectoryEntry {
        let metadata = fs::metadata(path).expect("fixture metadata should be readable");
        let file = File::open(path).expect("fixture file should be openable");
        LocalDirectoryEntry {
            name: name.to_owned(),
            path: path.display().to_string(),
            is_dir: false,
            is_symlink: false,
            size: metadata.len(),
            modified: metadata.modified().ok(),
            fingerprint: Some(
                local_file_fingerprint(&file).expect("fixture fingerprint should be readable"),
            ),
        }
    }

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

    #[test]
    fn local_open_validation_accepts_only_snapshot_regular_files() {
        let directory =
            std::env::temp_dir().join(format!("ax-ssh-local-open-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).expect("test directory should be created");
        let file_path = directory.join("notes.txt");
        fs::write(&file_path, b"notes").expect("test file should be written");
        let entry = fixture_entry(&file_path, "notes.txt");

        let mut validated = validate_local_file_for_open(&directory.display().to_string(), &entry)
            .expect("snapshot regular file should be accepted");
        let mut contents = String::new();
        validated
            .file
            .read_to_string(&mut contents)
            .expect("validated file should remain readable");
        assert_eq!(validated.name, "notes.txt");
        assert_eq!(contents, "notes");

        fs::remove_dir_all(&directory).expect("test directory should be removed");
    }

    #[test]
    fn local_open_validation_rejects_entries_outside_snapshot() {
        let root = std::env::temp_dir().join(format!("ax-ssh-local-open-{}", uuid::Uuid::new_v4()));
        let directory = root.join("listed");
        let outside = root.join("outside.txt");
        fs::create_dir_all(&directory).expect("test directory should be created");
        fs::write(&outside, b"outside").expect("outside file should be written");
        let entry = fixture_entry(&outside, "outside.txt");

        let error = validate_local_file_for_open(&directory.display().to_string(), &entry)
            .expect_err("outside file should be rejected");
        assert!(error.to_string().contains("outside the current directory"));

        fs::remove_dir_all(&root).expect("test directory should be removed");
    }

    #[cfg(unix)]
    #[test]
    fn local_open_validation_rejects_replaced_symlink() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("ax-ssh-local-open-{}", uuid::Uuid::new_v4()));
        let directory = root.join("listed");
        let target = root.join("target.txt");
        let link = directory.join("notes.txt");
        fs::create_dir_all(&directory).expect("test directory should be created");
        fs::write(&target, b"target").expect("target file should be written");
        symlink(&target, &link).expect("test symlink should be created");
        let entry = fixture_entry(&link, "notes.txt");

        let error = validate_local_file_for_open(&directory.display().to_string(), &entry)
            .expect_err("replacement symlink should be rejected");
        assert!(error.to_string().contains("became a symbolic link"));

        fs::remove_dir_all(&root).expect("test directory should be removed");
    }

    #[test]
    fn local_open_validation_rejects_a_replaced_regular_file() {
        let directory =
            std::env::temp_dir().join(format!("ax-ssh-local-open-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).expect("test directory should be created");
        let file_path = directory.join("notes.txt");
        fs::write(&file_path, b"trusted").expect("test file should be written");
        let entry = fixture_entry(&file_path, "notes.txt");
        fs::remove_file(&file_path).expect("listed fixture should be removed");
        fs::write(&file_path, b"replaced").expect("replacement fixture should be written");

        let error = validate_local_file_for_open(&directory.display().to_string(), &entry)
            .expect_err("replacement with a different file identity should be rejected");
        assert!(error.to_string().contains("changed after it was listed"));

        fs::remove_dir_all(&directory).expect("test directory should be removed");
    }

    #[test]
    fn local_open_validation_rejects_an_in_place_length_change() {
        let directory =
            std::env::temp_dir().join(format!("ax-ssh-local-open-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).expect("test directory should be created");
        let file_path = directory.join("notes.txt");
        fs::write(&file_path, b"trusted").expect("test file should be written");
        let entry = fixture_entry(&file_path, "notes.txt");
        fs::write(&file_path, b"changed-content").expect("fixture should be changed in place");

        let error = validate_local_file_for_open(&directory.display().to_string(), &entry)
            .expect_err("an in-place length change should invalidate the listing fingerprint");
        assert!(error.to_string().contains("changed after it was listed"));

        fs::remove_dir_all(&directory).expect("test directory should be removed");
    }
}
