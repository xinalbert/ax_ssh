use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use uuid::Uuid;

use super::{MAX_CONFIG_FILE_BYTES, SessionStore};

/// Private persistence boundary for the versioned session store and vault files.
#[derive(Clone, Debug)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn default_path() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("com", "axsoft", "ax_ssh")
            .context("cannot determine the platform config directory")?;
        Ok(dirs.config_dir().join("sessions.json"))
    }

    pub fn load(&self) -> Result<SessionStore> {
        if !self.path.exists() {
            return Ok(SessionStore::default());
        }
        let file = File::open(&self.path)
            .with_context(|| format!("failed to open {}", self.path.display()))?;
        let metadata = file
            .metadata()
            .with_context(|| format!("failed to inspect {}", self.path.display()))?;
        if metadata.len() > MAX_CONFIG_FILE_BYTES as u64 {
            anyhow::bail!(
                "session store {} exceeds the {} byte limit",
                self.path.display(),
                MAX_CONFIG_FILE_BYTES
            );
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take((MAX_CONFIG_FILE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        if bytes.len() > MAX_CONFIG_FILE_BYTES {
            anyhow::bail!(
                "session store {} exceeds the {} byte limit",
                self.path.display(),
                MAX_CONFIG_FILE_BYTES
            );
        }
        serde_json::from_slice(&bytes)
            .with_context(|| format!("invalid session store {}", self.path.display()))
    }

    pub fn save(&self, store: &SessionStore) -> Result<()> {
        store
            .validate()
            .context("session store validation failed")?;
        let bytes = serde_json::to_vec_pretty(store).context("failed to encode session store")?;
        if bytes.len() > MAX_CONFIG_FILE_BYTES {
            anyhow::bail!("encoded session store exceeds the {MAX_CONFIG_FILE_BYTES} byte limit");
        }
        write_private_file_atomically(&self.path, &bytes)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub(crate) fn write_private_file_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
        set_private_directory_permissions(parent);
    }
    let (mut file, mut temporary) = create_private_temporary_file(path)?;
    // ReplaceFileW requires the replacement file handle to be closed first.
    let write_result = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    write_result.with_context(|| format!("failed to write {}", temporary.path().display()))?;
    replace_file_atomically(temporary.path(), path).with_context(|| {
        format!(
            "failed to atomically replace {} with {}",
            path.display(),
            temporary.path().display()
        )
    })?;
    temporary.disarm();
    set_private_file_permissions(path);
    if let Some(parent) = path.parent() {
        sync_parent_directory(parent);
    }
    Ok(())
}

fn create_private_temporary_file(path: &Path) -> Result<(File, TemporaryPath)> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .context("private persistence path must include a file name")?;
    for _ in 0..8 {
        let mut temporary_name = std::ffi::OsString::from(".");
        temporary_name.push(file_name);
        temporary_name.push(format!(".{}.tmp", Uuid::new_v4()));
        let temporary_path = parent.join(temporary_name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        match options.open(&temporary_path) {
            Ok(file) => {
                let metadata = match file.metadata() {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        drop(file);
                        let _ = fs::remove_file(&temporary_path);
                        return Err(error).with_context(|| {
                            format!("failed to inspect {}", temporary_path.display())
                        });
                    }
                };
                if !metadata.is_file() {
                    drop(file);
                    let _ = fs::remove_file(&temporary_path);
                    anyhow::bail!(
                        "private persistence temporary path is not a regular file: {}",
                        temporary_path.display()
                    );
                }
                return Ok((file, TemporaryPath::new(temporary_path)));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to create {}", temporary_path.display()));
            }
        }
    }
    anyhow::bail!("failed to allocate a unique private persistence temporary file")
}

struct TemporaryPath {
    path: PathBuf,
    armed: bool,
}

impl TemporaryPath {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TemporaryPath {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;

    if let Ok(mut permissions) = fs::metadata(path).map(|metadata| metadata.permissions()) {
        permissions.set_mode(0o700);
        let _ = fs::set_permissions(path, permissions);
    }
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_: &Path) {}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;

    if let Ok(mut permissions) = fs::metadata(path).map(|metadata| metadata.permissions()) {
        permissions.set_mode(0o600);
        let _ = fs::set_permissions(path, permissions);
    }
}

#[cfg(not(unix))]
fn set_private_file_permissions(_: &Path) {}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) {
    let _ = File::open(path).and_then(|directory| directory.sync_all());
}

#[cfg(not(unix))]
fn sync_parent_directory(_: &Path) {}

#[cfg(not(target_os = "windows"))]
fn replace_file_atomically(temporary: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(temporary, destination)
}

#[cfg(target_os = "windows")]
fn replace_file_atomically(temporary: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt as _;

    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, ReplaceFileW,
    };

    let temporary = temporary
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: both paths are NUL-terminated UTF-16 buffers that remain alive
    // for the native call; the API does not retain either pointer.
    let replaced = unsafe {
        if destination.exists() {
            ReplaceFileW(
                destination_wide.as_ptr(),
                temporary.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            ) != 0
        } else {
            MoveFileExW(
                temporary.as_ptr(),
                destination_wide.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            ) != 0
        }
    };
    if replaced {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn atomic_write_does_not_follow_the_legacy_tmp_symlink() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("ax-ssh-atomic-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("test directory should be created");
        let destination = root.join("sessions.json");
        let victim = root.join("victim.json");
        let legacy_temporary = destination.with_extension("tmp");
        fs::write(&victim, b"unchanged").expect("victim fixture should be written");
        symlink(&victim, &legacy_temporary).expect("legacy temporary symlink should be created");

        write_private_file_atomically(&destination, b"new session data")
            .expect("atomic write should use a unique temporary file");

        assert_eq!(
            fs::read(&victim).expect("victim should remain readable"),
            b"unchanged"
        );
        assert_eq!(
            fs::read(&destination).expect("destination should be written"),
            b"new session data"
        );
        assert!(
            fs::symlink_metadata(&legacy_temporary)
                .expect("legacy symlink should remain present")
                .file_type()
                .is_symlink()
        );

        fs::remove_dir_all(root).expect("test directory should be removed");
    }
}
