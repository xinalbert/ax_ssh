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

pub const DEFAULT_TERMINAL_FONT_FAMILY: &str = "JetBrains Mono";
pub const MIN_TERMINAL_FONT_SIZE: u16 = 9;
pub const MAX_TERMINAL_FONT_SIZE: u16 = 32;
pub const MIN_SCROLLBACK_LINES: u32 = 100;
pub const MAX_SCROLLBACK_LINES: u32 = 50_000;
pub const MIN_TERMINAL_COLUMNS: u16 = 20;
pub const MAX_TERMINAL_COLUMNS: u16 = 300;
pub const MIN_TERMINAL_ROWS: u16 = 5;
pub const MAX_TERMINAL_ROWS: u16 = 100;
pub const MIN_SIDEBAR_WIDTH: u16 = 220;
pub const MAX_SIDEBAR_WIDTH: u16 = 420;
pub const MIN_TAB_WIDTH: u16 = 120;
pub const MAX_TAB_WIDTH: u16 = 260;
const DEFAULT_TERMINAL_FONT_SIZE: u16 = 14;
const DEFAULT_SCROLLBACK_LINES: u32 = 2_000;
const DEFAULT_TERMINAL_COLUMNS: u16 = 120;
const DEFAULT_TERMINAL_ROWS: u16 = 36;
const DEFAULT_SIDEBAR_WIDTH: u16 = 260;
const DEFAULT_TAB_WIDTH: u16 = 172;
const CURRENT_SCHEMA_VERSION: u32 = 1;
const MAX_FONT_FAMILY_CHARS: usize = 128;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SessionProfile {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub group_name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    /// Whether a password is expected in the platform credential store.
    /// The credential itself is never serialized in this profile.
    #[serde(default)]
    pub credential_stored: bool,
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppearanceSettings {
    #[serde(default = "default_terminal_font_family")]
    pub terminal_font_family: String,
    #[serde(default = "default_terminal_font_size")]
    pub terminal_font_size: u16,
}

impl AppearanceSettings {
    pub fn normalized(font_family: &str, terminal_font_size: i32) -> Self {
        let font_family = font_family.trim();
        let terminal_font_family = if font_family.is_empty()
            || font_family.chars().count() > MAX_FONT_FAMILY_CHARS
            || font_family.chars().any(char::is_control)
        {
            default_terminal_font_family()
        } else {
            font_family.to_owned()
        };
        Self {
            terminal_font_family,
            terminal_font_size: terminal_font_size.clamp(
                i32::from(MIN_TERMINAL_FONT_SIZE),
                i32::from(MAX_TERMINAL_FONT_SIZE),
            ) as u16,
        }
    }
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            terminal_font_family: default_terminal_font_family(),
            terminal_font_size: default_terminal_font_size(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TerminalSettings {
    #[serde(default = "default_scrollback_lines")]
    pub scrollback_lines: u32,
    #[serde(default = "default_terminal_columns")]
    pub default_columns: u16,
    #[serde(default = "default_terminal_rows")]
    pub default_rows: u16,
}

impl TerminalSettings {
    pub fn normalized(scrollback_lines: i32, default_columns: i32, default_rows: i32) -> Self {
        Self {
            scrollback_lines: scrollback_lines
                .clamp(MIN_SCROLLBACK_LINES as i32, MAX_SCROLLBACK_LINES as i32)
                as u32,
            default_columns: default_columns
                .clamp(MIN_TERMINAL_COLUMNS.into(), MAX_TERMINAL_COLUMNS.into())
                as u16,
            default_rows: default_rows.clamp(MIN_TERMINAL_ROWS.into(), MAX_TERMINAL_ROWS.into())
                as u16,
        }
    }

    fn normalize_in_place(&mut self) {
        self.scrollback_lines = self
            .scrollback_lines
            .clamp(MIN_SCROLLBACK_LINES, MAX_SCROLLBACK_LINES);
        self.default_columns = self
            .default_columns
            .clamp(MIN_TERMINAL_COLUMNS, MAX_TERMINAL_COLUMNS);
        self.default_rows = self
            .default_rows
            .clamp(MIN_TERMINAL_ROWS, MAX_TERMINAL_ROWS);
    }
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            scrollback_lines: default_scrollback_lines(),
            default_columns: default_terminal_columns(),
            default_rows: default_terminal_rows(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspaceSettings {
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u16,
    #[serde(default = "default_tab_width")]
    pub tab_width: u16,
}

impl WorkspaceSettings {
    pub fn normalized(sidebar_width: i32, tab_width: i32) -> Self {
        Self {
            sidebar_width: sidebar_width.clamp(MIN_SIDEBAR_WIDTH.into(), MAX_SIDEBAR_WIDTH.into())
                as u16,
            tab_width: tab_width.clamp(MIN_TAB_WIDTH.into(), MAX_TAB_WIDTH.into()) as u16,
        }
    }

    fn normalize_in_place(&mut self) {
        *self = Self::normalized(i32::from(self.sidebar_width), i32::from(self.tab_width));
    }
}

impl Default for WorkspaceSettings {
    fn default() -> Self {
        Self {
            sidebar_width: default_sidebar_width(),
            tab_width: default_tab_width(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppSettings {
    #[serde(default)]
    pub appearance: AppearanceSettings,
    #[serde(default)]
    pub terminal: TerminalSettings,
    #[serde(default)]
    pub workspace: WorkspaceSettings,
}

impl AppSettings {
    pub fn normalized(
        font_family: &str,
        font_size: i32,
        scrollback_lines: i32,
        default_columns: i32,
        default_rows: i32,
        sidebar_width: i32,
        tab_width: i32,
    ) -> Self {
        Self {
            appearance: AppearanceSettings::normalized(font_family, font_size),
            terminal: TerminalSettings::normalized(scrollback_lines, default_columns, default_rows),
            workspace: WorkspaceSettings::normalized(sidebar_width, tab_width),
        }
    }

    fn normalize_in_place(&mut self) {
        self.appearance = AppearanceSettings::normalized(
            &self.appearance.terminal_font_family,
            i32::from(self.appearance.terminal_font_size),
        );
        self.terminal.normalize_in_place();
        self.workspace.normalize_in_place();
    }
}

fn default_terminal_font_family() -> String {
    DEFAULT_TERMINAL_FONT_FAMILY.to_owned()
}

const fn default_terminal_font_size() -> u16 {
    DEFAULT_TERMINAL_FONT_SIZE
}

const fn default_scrollback_lines() -> u32 {
    DEFAULT_SCROLLBACK_LINES
}

const fn default_terminal_columns() -> u16 {
    DEFAULT_TERMINAL_COLUMNS
}

const fn default_terminal_rows() -> u16 {
    DEFAULT_TERMINAL_ROWS
}

const fn default_sidebar_width() -> u16 {
    DEFAULT_SIDEBAR_WIDTH
}

const fn default_tab_width() -> u16 {
    DEFAULT_TAB_WIDTH
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
            group_name: String::new(),
            host: host.into(),
            port: 22,
            username: username.into(),
            auth: AuthMethod::default(),
            credential_stored: false,
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
        if self.group_name.chars().count() > 64 {
            anyhow::bail!("group name cannot exceed 64 characters");
        }
        if self.group_name.chars().any(char::is_control) {
            anyhow::bail!("group name cannot contain control characters");
        }
        if let AuthMethod::PrivateKey { path } = &self.auth {
            if path.as_os_str().is_empty() {
                anyhow::bail!("private key path cannot be empty");
            }
            if self.credential_stored {
                anyhow::bail!("private-key profiles cannot store password credentials");
            }
        }
        Ok(())
    }
}

pub fn normalize_group_name(value: &str) -> String {
    value.trim().to_owned()
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SessionStore {
    pub version: u32,
    #[serde(default)]
    pub sessions: Vec<SessionProfile>,
    pub settings: AppSettings,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self {
            version: CURRENT_SCHEMA_VERSION,
            sessions: Vec::new(),
            settings: AppSettings::default(),
        }
    }
}

#[derive(Deserialize)]
struct SessionStoreWire {
    #[serde(default = "current_schema_version")]
    version: u32,
    #[serde(default)]
    sessions: Vec<SessionProfile>,
    #[serde(default)]
    settings: Option<AppSettings>,
    #[serde(default)]
    appearance: Option<AppearanceSettings>,
}

impl<'de> Deserialize<'de> for SessionStore {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = SessionStoreWire::deserialize(deserializer)?;
        let mut settings = wire.settings.unwrap_or_default();
        if let Some(appearance) = wire.appearance
            && settings.appearance == AppearanceSettings::default()
        {
            settings.appearance = appearance;
        }
        settings.normalize_in_place();
        Ok(Self {
            version: wire.version.max(CURRENT_SCHEMA_VERSION),
            sessions: wire.sessions,
            settings,
        })
    }
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

const fn current_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
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
        let mut profile = SessionProfile::new("demo", "host.example", "alice");
        profile.group_name = "Production".into();
        profile.credential_stored = true;
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

    #[test]
    fn legacy_profile_defaults_group_and_credential_marker() {
        let id = Uuid::new_v4();
        let json = format!(
            r#"{{"sessions":[{{"id":"{id}","name":"legacy","host":"host.example","port":22,"username":"alice","auth":"Password"}}]}}"#
        );

        let store: SessionStore =
            serde_json::from_str(&json).expect("legacy profile should deserialize");
        assert_eq!(store.sessions[0].group_name, "");
        assert!(!store.sessions[0].credential_stored);
        assert_eq!(store.settings, AppSettings::default());
    }

    #[test]
    fn appearance_settings_normalize_font_family_and_size() {
        assert_eq!(
            AppearanceSettings::normalized("  Menlo  ", 18),
            AppearanceSettings {
                terminal_font_family: "Menlo".into(),
                terminal_font_size: 18,
            }
        );
        assert_eq!(
            AppearanceSettings::normalized("", 100),
            AppearanceSettings {
                terminal_font_family: DEFAULT_TERMINAL_FONT_FAMILY.into(),
                terminal_font_size: MAX_TERMINAL_FONT_SIZE,
            }
        );
    }

    #[test]
    fn legacy_appearance_migrates_into_versioned_settings() {
        let json = r#"{
            "sessions": [],
            "appearance": {
                "terminal_font_family": "Menlo",
                "terminal_font_size": 17
            }
        }"#;

        let store: SessionStore = serde_json::from_str(json).expect("legacy settings should load");

        assert_eq!(store.version, CURRENT_SCHEMA_VERSION);
        assert_eq!(store.settings.appearance.terminal_font_family, "Menlo");
        assert_eq!(store.settings.appearance.terminal_font_size, 17);
        assert_eq!(store.settings.terminal, TerminalSettings::default());
        let serialized = serde_json::to_value(store).expect("settings should serialize");
        assert!(serialized.get("settings").is_some());
        assert!(serialized.get("appearance").is_none());
    }

    #[test]
    fn app_settings_clamp_all_persisted_dimensions() {
        let settings = AppSettings::normalized("", 100, -1, 1, 1_000, 20, 9_000);

        assert_eq!(
            settings.appearance.terminal_font_family,
            DEFAULT_TERMINAL_FONT_FAMILY
        );
        assert_eq!(
            settings.appearance.terminal_font_size,
            MAX_TERMINAL_FONT_SIZE
        );
        assert_eq!(settings.terminal.scrollback_lines, MIN_SCROLLBACK_LINES);
        assert_eq!(settings.terminal.default_columns, MIN_TERMINAL_COLUMNS);
        assert_eq!(settings.terminal.default_rows, MAX_TERMINAL_ROWS);
        assert_eq!(settings.workspace.sidebar_width, MIN_SIDEBAR_WIDTH);
        assert_eq!(settings.workspace.tab_width, MAX_TAB_WIDTH);
    }

    #[test]
    fn profile_json_contains_no_secret_fields() {
        let mut profile = SessionProfile::new("demo", "host.example", "alice");
        profile.credential_stored = true;
        let value = serde_json::to_value(profile).expect("profile should serialize");
        let object = value.as_object().expect("profile should be an object");

        assert!(!object.contains_key("password"));
        assert!(!object.contains_key("passphrase"));
        assert!(!object.contains_key("secret"));
        assert_eq!(
            object.get("credential_stored"),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn group_names_are_trimmed_and_bounded() {
        assert_eq!(normalize_group_name("  Production  "), "Production");
        let mut profile = SessionProfile::new("demo", "host.example", "alice");
        profile.group_name = "x".repeat(65);
        assert!(profile.validate().is_err());
    }

    #[test]
    fn private_key_profiles_require_a_path_and_never_store_password_markers() {
        let mut profile = SessionProfile::new("demo", "host.example", "alice");
        profile.auth = AuthMethod::PrivateKey {
            path: PathBuf::new(),
        };
        assert!(profile.validate().is_err());

        profile.auth = AuthMethod::PrivateKey {
            path: PathBuf::from("/tmp/id_ed25519"),
        };
        profile.credential_stored = true;
        assert!(profile.validate().is_err());

        profile.credential_stored = false;
        assert!(profile.validate().is_ok());
    }
}
