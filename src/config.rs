//! Persistent session profiles.
//!
//! The config layer is deliberately independent from Slint and russh. It owns
//! the on-disk schema and can therefore be tested without creating a window or
//! opening a network connection.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SessionProfile {
    pub id: Uuid,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    /// A SHA-256 SSH public-key fingerprint. The empty value means unknown;
    /// the SSH layer must refuse the connection until it is trusted.
    #[serde(default)]
    pub host_key_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum AuthMethod {
    Password,
    PrivateKey { path: PathBuf },
}

impl Default for AuthMethod {
    fn default() -> Self {
        Self::Password
    }
}

impl SessionProfile {
    pub fn new(
        name: impl Into<String>,
        host: impl Into<String>,
        username: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            host: host.into(),
            port: 22,
            username: username.into(),
            auth: AuthMethod::default(),
            host_key_fingerprint: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            anyhow::bail!("session name cannot be empty");
        }
        if self.host.trim().is_empty() {
            anyhow::bail!("host cannot be empty");
        }
        if self.username.trim().is_empty() {
            anyhow::bail!("username cannot be empty");
        }
        if self.port == 0 {
            anyhow::bail!("port must be between 1 and 65535");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct SessionStore {
    #[serde(default)]
    pub sessions: Vec<SessionProfile>,
}

impl SessionStore {
    pub fn upsert(&mut self, profile: SessionProfile) {
        if let Some(existing) = self.sessions.iter_mut().find(|item| item.id == profile.id) {
            *existing = profile;
        } else {
            self.sessions.push(profile);
        }
    }

    pub fn remove(&mut self, id: Uuid) -> bool {
        let before = self.sessions.len();
        self.sessions.retain(|item| item.id != id);
        before != self.sessions.len()
    }
}

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
        let bytes = fs::read(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        serde_json::from_slice(&bytes)
            .with_context(|| format!("invalid session store {}", self.path.display()))
    }

    pub fn save(&self, store: &SessionStore) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
            set_private_directory_permissions(parent);
        }
        let bytes = serde_json::to_vec_pretty(store).context("failed to encode session store")?;
        let temporary = self.path.with_extension("json.tmp");
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .with_context(|| format!("failed to open {}", temporary.display()))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .with_context(|| format!("failed to write {}", temporary.display()))?;
        replace_file_atomically(&temporary, &self.path).with_context(|| {
            format!(
                "failed to atomically replace {} with {}",
                self.path.display(),
                temporary.display()
            )
        })?;
        set_private_file_permissions(&self.path);
        if let Some(parent) = self.path.parent() {
            sync_parent_directory(parent);
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
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

    #[test]
    fn profile_validation_rejects_missing_host() {
        let profile = SessionProfile::new("demo", "", "alice");
        assert!(profile.validate().is_err());
    }

    #[test]
    fn store_round_trips_and_upserts() {
        let temp = std::env::temp_dir().join(format!("ax-ssh-{}", Uuid::new_v4()));
        let store = ConfigStore::new(&temp);
        let mut data = SessionStore::default();
        let profile = SessionProfile::new("demo", "host.example", "alice");
        data.upsert(profile.clone());
        data.upsert(SessionProfile {
            name: "renamed".into(),
            ..profile.clone()
        });
        assert_eq!(data.sessions.len(), 1);
        store.save(&data).expect("save should succeed");
        assert_eq!(store.load().expect("load should succeed"), data);
        data.sessions[0].name = "saved-again".into();
        store.save(&data).expect("replacement save should succeed");
        assert_eq!(store.load().expect("replacement load should succeed"), data);
        let _ = fs::remove_file(temp);
    }
}
