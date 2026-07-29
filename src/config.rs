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
pub const MIN_TERMINAL_LINE_HEIGHT: u16 = 100;
pub const MAX_TERMINAL_LINE_HEIGHT: u16 = 200;
pub const MIN_TERMINAL_BRIGHTNESS: u16 = 60;
pub const MAX_TERMINAL_BRIGHTNESS: u16 = 140;
pub const MIN_SCROLLBACK_LINES: u32 = 100;
pub const MAX_SCROLLBACK_LINES: u32 = 50_000;
pub const MIN_TERMINAL_COLUMNS: u16 = 20;
pub const MAX_TERMINAL_COLUMNS: u16 = 300;
pub const MIN_TERMINAL_ROWS: u16 = 5;
pub const MAX_TERMINAL_ROWS: u16 = 100;
pub const MIN_SIDEBAR_WIDTH: u16 = 180;
pub const MAX_SIDEBAR_WIDTH: u16 = 420;
pub const MIN_TAB_WIDTH: u16 = 120;
pub const MAX_TAB_WIDTH: u16 = 260;
pub const SYSTEM_DEFAULT_SHELL: &str = "System default";
const DEFAULT_TERMINAL_FONT_SIZE: u16 = 14;
const DEFAULT_TERMINAL_LINE_HEIGHT: u16 = 120;
const DEFAULT_TERMINAL_BRIGHTNESS: u16 = 100;
const DEFAULT_SCROLLBACK_LINES: u32 = 2_000;
const DEFAULT_TERMINAL_COLUMNS: u16 = 120;
const DEFAULT_TERMINAL_ROWS: u16 = 36;
const DEFAULT_SIDEBAR_WIDTH: u16 = 220;
const PREVIOUS_DEFAULT_SIDEBAR_WIDTH: u16 = 260;
const DEFAULT_TAB_WIDTH: u16 = 172;
const CURRENT_SCHEMA_VERSION: u32 = 7;
const PLATFORM_SHORTCUT_SCHEMA_VERSION: u32 = 6;
const WORKSPACE_DENSITY_SCHEMA_VERSION: u32 = 7;
const MAX_FONT_FAMILY_CHARS: usize = 128;
const MAX_SHORTCUT_CHARS: usize = 64;
const MAX_KNOWN_SHELLS: usize = 32;
const MAX_SHELL_NAME_CHARS: usize = 256;

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

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalColorScheme {
    #[default]
    Dark,
    Light,
    SolarizedDark,
}

impl TerminalColorScheme {
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "light" => Self::Light,
            "solarized-dark" | "solarized dark" => Self::SolarizedDark,
            _ => Self::Dark,
        }
    }

    pub const fn as_setting(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::SolarizedDark => "solarized-dark",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppearanceSettings {
    #[serde(default = "default_terminal_font_family")]
    pub terminal_font_family: String,
    #[serde(default = "default_terminal_font_size")]
    pub terminal_font_size: u16,
    #[serde(default = "default_terminal_line_height")]
    pub terminal_line_height_percent: u16,
    #[serde(default)]
    pub terminal_color_scheme: TerminalColorScheme,
    #[serde(default = "default_terminal_brightness")]
    pub terminal_brightness_percent: u16,
    #[serde(default = "default_true")]
    pub bright_bold_text: bool,
    #[serde(default, alias = "right_click_copies_selection")]
    pub right_click_copy_or_paste: bool,
}

impl AppearanceSettings {
    pub fn normalized(
        font_family: &str,
        terminal_font_size: i32,
        terminal_line_height_percent: i32,
        color_scheme: &str,
        brightness_percent: i32,
        bright_bold_text: bool,
        right_click_copy_or_paste: bool,
    ) -> Self {
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
            terminal_line_height_percent: terminal_line_height_percent.clamp(
                i32::from(MIN_TERMINAL_LINE_HEIGHT),
                i32::from(MAX_TERMINAL_LINE_HEIGHT),
            ) as u16,
            terminal_color_scheme: TerminalColorScheme::from_setting(color_scheme),
            terminal_brightness_percent: brightness_percent.clamp(
                i32::from(MIN_TERMINAL_BRIGHTNESS),
                i32::from(MAX_TERMINAL_BRIGHTNESS),
            ) as u16,
            bright_bold_text,
            right_click_copy_or_paste,
        }
    }
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            terminal_font_family: default_terminal_font_family(),
            terminal_font_size: default_terminal_font_size(),
            terminal_line_height_percent: default_terminal_line_height(),
            terminal_color_scheme: TerminalColorScheme::default(),
            terminal_brightness_percent: default_terminal_brightness(),
            bright_bold_text: true,
            right_click_copy_or_paste: false,
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
    #[serde(default = "default_local_shell")]
    pub local_shell: String,
    #[serde(default = "default_known_shells")]
    pub known_shells: Vec<String>,
}

impl TerminalSettings {
    pub fn normalized(
        scrollback_lines: i32,
        default_columns: i32,
        default_rows: i32,
        local_shell: &str,
        known_shells: &[String],
    ) -> Self {
        let (local_shell, known_shells) = normalize_shell_settings(local_shell, known_shells);
        Self {
            scrollback_lines: scrollback_lines
                .clamp(MIN_SCROLLBACK_LINES as i32, MAX_SCROLLBACK_LINES as i32)
                as u32,
            default_columns: default_columns
                .clamp(MIN_TERMINAL_COLUMNS.into(), MAX_TERMINAL_COLUMNS.into())
                as u16,
            default_rows: default_rows.clamp(MIN_TERMINAL_ROWS.into(), MAX_TERMINAL_ROWS.into())
                as u16,
            local_shell,
            known_shells,
        }
    }

    fn normalize_in_place(&mut self) {
        *self = Self::normalized(
            i32::try_from(self.scrollback_lines).unwrap_or(i32::MAX),
            i32::from(self.default_columns),
            i32::from(self.default_rows),
            &self.local_shell,
            &self.known_shells,
        );
    }

    pub fn merge_known_shells(&mut self, discovered: impl IntoIterator<Item = String>) -> bool {
        let previous = self.known_shells.clone();
        let mut merged = previous.clone();
        merged.extend(discovered);
        let (local_shell, known_shells) = normalize_shell_settings(&self.local_shell, &merged);
        self.local_shell = local_shell;
        self.known_shells = known_shells;
        self.known_shells != previous
    }
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            scrollback_lines: default_scrollback_lines(),
            default_columns: default_terminal_columns(),
            default_rows: default_terminal_rows(),
            local_shell: default_local_shell(),
            known_shells: default_known_shells(),
        }
    }
}

fn normalize_shell_settings(local_shell: &str, known_shells: &[String]) -> (String, Vec<String>) {
    let local_shell =
        normalize_shell_name(local_shell).unwrap_or_else(|| SYSTEM_DEFAULT_SHELL.to_owned());
    let mut normalized = Vec::with_capacity(known_shells.len().min(MAX_KNOWN_SHELLS) + 2);
    normalized.push(SYSTEM_DEFAULT_SHELL.to_owned());
    for shell in known_shells {
        let Some(shell) = normalize_shell_name(shell) else {
            continue;
        };
        if normalized
            .iter()
            .any(|known| known.eq_ignore_ascii_case(&shell))
        {
            continue;
        }
        normalized.push(shell);
        if normalized.len() >= MAX_KNOWN_SHELLS {
            break;
        }
    }
    if !normalized
        .iter()
        .any(|known| known.eq_ignore_ascii_case(&local_shell))
        && normalized.len() < MAX_KNOWN_SHELLS
    {
        normalized.push(local_shell.clone());
    }
    (local_shell, normalized)
}

fn normalize_shell_name(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && value.chars().count() <= MAX_SHELL_NAME_CHARS
        && !value.chars().any(char::is_control))
    .then(|| value.to_owned())
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ShortcutSettings {
    #[serde(default = "default_open_settings_shortcut")]
    pub open_settings: String,
    #[serde(default = "default_toggle_sidebar_shortcut")]
    pub toggle_sidebar: String,
    #[serde(default = "default_copy_selection_shortcut")]
    pub copy_selection: String,
    #[serde(default = "default_paste_shortcut")]
    pub paste: String,
}

impl ShortcutSettings {
    pub fn normalized(
        open_settings: &str,
        toggle_sidebar: &str,
        copy_selection: &str,
        paste: &str,
    ) -> Self {
        let candidate = Self {
            open_settings: open_settings.trim().to_owned(),
            toggle_sidebar: toggle_sidebar.trim().to_owned(),
            copy_selection: copy_selection.trim().to_owned(),
            paste: paste.trim().to_owned(),
        };
        if candidate.validate().is_ok() {
            candidate
        } else {
            Self::default()
        }
    }

    pub fn validate(&self) -> Result<()> {
        let shortcuts = [
            ("open settings", self.open_settings.as_str()),
            ("toggle sidebar", self.toggle_sidebar.as_str()),
            ("copy selection", self.copy_selection.as_str()),
            ("paste", self.paste.as_str()),
        ];
        for (label, shortcut) in shortcuts {
            validate_shortcut(label, shortcut)?;
        }
        for (index, (label, shortcut)) in shortcuts.iter().enumerate() {
            if let Some((other_label, _)) = shortcuts[index + 1..]
                .iter()
                .find(|(_, other)| other.eq_ignore_ascii_case(shortcut))
            {
                anyhow::bail!("{label} and {other_label} cannot use the same shortcut");
            }
        }
        Ok(())
    }
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            open_settings: default_open_settings_shortcut(),
            toggle_sidebar: default_toggle_sidebar_shortcut(),
            copy_selection: default_copy_selection_shortcut(),
            paste: default_paste_shortcut(),
        }
    }
}

fn validate_shortcut(label: &str, shortcut: &str) -> Result<()> {
    let shortcut = shortcut.trim();
    if shortcut.is_empty() {
        anyhow::bail!("{label} shortcut cannot be empty");
    }
    if shortcut.chars().count() > MAX_SHORTCUT_CHARS || shortcut.chars().any(char::is_control) {
        anyhow::bail!("{label} shortcut is invalid");
    }
    let Some((modifiers, key)) = shortcut.rsplit_once('+') else {
        anyhow::bail!("{label} shortcut must include a modifier");
    };
    if key.is_empty() || key.chars().any(char::is_whitespace) {
        anyhow::bail!("{label} shortcut key is invalid");
    }
    let mut seen = Vec::new();
    for modifier in modifiers.split('+') {
        if !matches!(modifier, "Cmd" | "Meta" | "Ctrl" | "Alt" | "Shift") {
            anyhow::bail!("{label} shortcut contains an unknown modifier");
        }
        if seen.contains(&modifier) {
            anyhow::bail!("{label} shortcut contains a repeated modifier");
        }
        seen.push(modifier);
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppSettings {
    #[serde(default)]
    pub appearance: AppearanceSettings,
    #[serde(default)]
    pub terminal: TerminalSettings,
    #[serde(default)]
    pub workspace: WorkspaceSettings,
    #[serde(default)]
    pub shortcuts: ShortcutSettings,
}

impl AppSettings {
    pub fn normalized(
        font_family: &str,
        font_size: i32,
        line_height_percent: i32,
        color_scheme: &str,
        brightness_percent: i32,
        bright_bold_text: bool,
        right_click_copy_or_paste: bool,
        scrollback_lines: i32,
        default_columns: i32,
        default_rows: i32,
        local_shell: &str,
        known_shells: &[String],
        sidebar_width: i32,
        tab_width: i32,
        open_settings_shortcut: &str,
        toggle_sidebar_shortcut: &str,
        copy_selection_shortcut: &str,
        paste_shortcut: &str,
    ) -> Self {
        Self {
            appearance: AppearanceSettings::normalized(
                font_family,
                font_size,
                line_height_percent,
                color_scheme,
                brightness_percent,
                bright_bold_text,
                right_click_copy_or_paste,
            ),
            terminal: TerminalSettings::normalized(
                scrollback_lines,
                default_columns,
                default_rows,
                local_shell,
                known_shells,
            ),
            workspace: WorkspaceSettings::normalized(sidebar_width, tab_width),
            shortcuts: ShortcutSettings::normalized(
                open_settings_shortcut,
                toggle_sidebar_shortcut,
                copy_selection_shortcut,
                paste_shortcut,
            ),
        }
    }

    fn normalize_in_place(&mut self) {
        self.appearance = AppearanceSettings::normalized(
            &self.appearance.terminal_font_family,
            i32::from(self.appearance.terminal_font_size),
            i32::from(self.appearance.terminal_line_height_percent),
            self.appearance.terminal_color_scheme.as_setting(),
            i32::from(self.appearance.terminal_brightness_percent),
            self.appearance.bright_bold_text,
            self.appearance.right_click_copy_or_paste,
        );
        self.terminal.normalize_in_place();
        self.workspace.normalize_in_place();
        self.shortcuts = ShortcutSettings::normalized(
            &self.shortcuts.open_settings,
            &self.shortcuts.toggle_sidebar,
            &self.shortcuts.copy_selection,
            &self.shortcuts.paste,
        );
    }
}

fn default_terminal_font_family() -> String {
    DEFAULT_TERMINAL_FONT_FAMILY.to_owned()
}

const fn default_terminal_font_size() -> u16 {
    DEFAULT_TERMINAL_FONT_SIZE
}

const fn default_terminal_line_height() -> u16 {
    DEFAULT_TERMINAL_LINE_HEIGHT
}

const fn default_terminal_brightness() -> u16 {
    DEFAULT_TERMINAL_BRIGHTNESS
}

const fn default_true() -> bool {
    true
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

fn default_local_shell() -> String {
    SYSTEM_DEFAULT_SHELL.to_owned()
}

fn default_known_shells() -> Vec<String> {
    vec![default_local_shell()]
}

const fn default_sidebar_width() -> u16 {
    DEFAULT_SIDEBAR_WIDTH
}

const fn default_tab_width() -> u16 {
    DEFAULT_TAB_WIDTH
}

fn default_open_settings_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+,".to_owned()
    } else {
        "Ctrl+,".to_owned()
    }
}

fn default_toggle_sidebar_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+S".to_owned()
    } else {
        "Ctrl+S".to_owned()
    }
}

fn previous_toggle_sidebar_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+B".to_owned()
    } else {
        "Ctrl+Shift+B".to_owned()
    }
}

fn default_copy_selection_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+C".to_owned()
    } else {
        "Ctrl+Shift+C".to_owned()
    }
}

fn default_paste_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+V".to_owned()
    } else {
        "Ctrl+Shift+V".to_owned()
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
        if wire.version < PLATFORM_SHORTCUT_SCHEMA_VERSION {
            if settings
                .shortcuts
                .toggle_sidebar
                .eq_ignore_ascii_case(&previous_toggle_sidebar_shortcut())
            {
                settings.shortcuts.toggle_sidebar = default_toggle_sidebar_shortcut();
            }
        }
        if wire.version < WORKSPACE_DENSITY_SCHEMA_VERSION
            && settings.workspace.sidebar_width == PREVIOUS_DEFAULT_SIDEBAR_WIDTH
        {
            settings.workspace.sidebar_width = DEFAULT_SIDEBAR_WIDTH;
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
        // ReplaceFileW requires the replacement file handle to be closed first.
        {
            let mut file = options
                .open(&temporary)
                .with_context(|| format!("failed to open {}", temporary.display()))?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .with_context(|| format!("failed to write {}", temporary.display()))?;
        }
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
            AppearanceSettings::normalized("  Menlo  ", 18, 135, "light", 115, false, true),
            AppearanceSettings {
                terminal_font_family: "Menlo".into(),
                terminal_font_size: 18,
                terminal_line_height_percent: 135,
                terminal_color_scheme: TerminalColorScheme::Light,
                terminal_brightness_percent: 115,
                bright_bold_text: false,
                right_click_copy_or_paste: true,
            }
        );
        assert_eq!(
            AppearanceSettings::normalized("", 100, 1_000, "unknown", 1_000, true, false),
            AppearanceSettings {
                terminal_font_family: DEFAULT_TERMINAL_FONT_FAMILY.into(),
                terminal_font_size: MAX_TERMINAL_FONT_SIZE,
                terminal_line_height_percent: MAX_TERMINAL_LINE_HEIGHT,
                terminal_color_scheme: TerminalColorScheme::Dark,
                terminal_brightness_percent: MAX_TERMINAL_BRIGHTNESS,
                bright_bold_text: true,
                right_click_copy_or_paste: false,
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
        assert_eq!(
            store.settings.appearance.terminal_line_height_percent,
            DEFAULT_TERMINAL_LINE_HEIGHT
        );
        assert_eq!(
            store.settings.appearance.terminal_color_scheme,
            TerminalColorScheme::Dark
        );
        assert_eq!(
            store.settings.appearance.terminal_brightness_percent,
            DEFAULT_TERMINAL_BRIGHTNESS
        );
        assert!(store.settings.appearance.bright_bold_text);
        assert!(!store.settings.appearance.right_click_copy_or_paste);
        assert_eq!(store.settings.terminal, TerminalSettings::default());
        assert_eq!(store.settings.shortcuts, ShortcutSettings::default());
        let serialized = serde_json::to_value(store).expect("settings should serialize");
        assert!(serialized.get("settings").is_some());
        assert!(serialized.get("appearance").is_none());
        assert_eq!(
            serialized["settings"]["appearance"]["terminal_line_height_percent"],
            DEFAULT_TERMINAL_LINE_HEIGHT
        );
    }

    #[test]
    fn legacy_right_click_setting_uses_the_copy_or_paste_parameter() {
        let json = r#"{
            "version": 5,
            "settings": {
                "appearance": {
                    "right_click_copies_selection": true
                }
            }
        }"#;
        let store: SessionStore =
            serde_json::from_str(json).expect("legacy right-click setting should load");
        assert!(store.settings.appearance.right_click_copy_or_paste);

        let serialized = serde_json::to_value(store).expect("settings should serialize");
        let appearance = &serialized["settings"]["appearance"];
        assert_eq!(appearance["right_click_copy_or_paste"], true);
        assert!(appearance.get("right_click_copies_selection").is_none());
    }

    #[test]
    fn app_settings_clamp_all_persisted_dimensions() {
        let settings = AppSettings::normalized(
            "",
            100,
            -1,
            "solarized-dark",
            -1,
            false,
            true,
            -1,
            1,
            1_000,
            "zsh",
            &[SYSTEM_DEFAULT_SHELL.into(), "zsh".into()],
            20,
            9_000,
            "Ctrl+,",
            "Ctrl+Shift+B",
            "Ctrl+Shift+C",
            "Ctrl+Shift+V",
        );

        assert_eq!(
            settings.appearance.terminal_font_family,
            DEFAULT_TERMINAL_FONT_FAMILY
        );
        assert_eq!(
            settings.appearance.terminal_font_size,
            MAX_TERMINAL_FONT_SIZE
        );
        assert_eq!(
            settings.appearance.terminal_line_height_percent,
            MIN_TERMINAL_LINE_HEIGHT
        );
        assert_eq!(
            settings.appearance.terminal_color_scheme,
            TerminalColorScheme::SolarizedDark
        );
        assert_eq!(
            settings.appearance.terminal_brightness_percent,
            MIN_TERMINAL_BRIGHTNESS
        );
        assert!(!settings.appearance.bright_bold_text);
        assert!(settings.appearance.right_click_copy_or_paste);
        assert_eq!(settings.terminal.scrollback_lines, MIN_SCROLLBACK_LINES);
        assert_eq!(settings.terminal.default_columns, MIN_TERMINAL_COLUMNS);
        assert_eq!(settings.terminal.default_rows, MAX_TERMINAL_ROWS);
        assert_eq!(settings.terminal.local_shell, "zsh");
        assert_eq!(
            settings.terminal.known_shells,
            [SYSTEM_DEFAULT_SHELL, "zsh"]
        );
        assert_eq!(settings.workspace.sidebar_width, MIN_SIDEBAR_WIDTH);
        assert_eq!(settings.workspace.tab_width, MAX_TAB_WIDTH);
        assert_eq!(settings.shortcuts.open_settings, "Ctrl+,");
    }

    #[test]
    fn shortcut_settings_validate_modifiers_and_conflicts() {
        let defaults = ShortcutSettings::default();
        assert!(defaults.validate().is_ok());
        assert_eq!(
            defaults.toggle_sidebar,
            if cfg!(target_os = "macos") {
                "Cmd+S"
            } else {
                "Ctrl+S"
            }
        );
        assert_eq!(
            defaults.copy_selection,
            if cfg!(target_os = "macos") {
                "Cmd+C"
            } else {
                "Ctrl+Shift+C"
            }
        );
        assert_eq!(
            defaults.paste,
            if cfg!(target_os = "macos") {
                "Cmd+V"
            } else {
                "Ctrl+Shift+V"
            }
        );
        let conflicting = ShortcutSettings {
            open_settings: "Ctrl+,".into(),
            toggle_sidebar: "Ctrl+,".into(),
            copy_selection: "Ctrl+Shift+C".into(),
            paste: "Ctrl+Shift+V".into(),
        };
        assert!(conflicting.validate().is_err());

        assert_eq!(
            ShortcutSettings::normalized("B", "Ctrl+Shift+B", "Ctrl+Shift+C", "Ctrl+Shift+V"),
            ShortcutSettings::default()
        );
    }

    #[test]
    fn previous_sidebar_default_migrates_without_overwriting_custom_values() {
        let previous = previous_toggle_sidebar_shortcut();
        let copy = default_copy_selection_shortcut();
        let paste = default_paste_shortcut();
        let json = format!(
            r#"{{
                "version": 5,
                "settings": {{
                    "shortcuts": {{
                        "open_settings": "Ctrl+,",
                        "toggle_sidebar": "{previous}",
                        "copy_selection": "{copy}",
                        "paste": "{paste}"
                    }}
                }}
            }}"#
        );
        let migrated: SessionStore =
            serde_json::from_str(&json).expect("previous shortcuts should migrate");
        assert_eq!(
            migrated.settings.shortcuts.toggle_sidebar,
            default_toggle_sidebar_shortcut()
        );
        assert_eq!(migrated.settings.shortcuts.copy_selection, copy);
        assert_eq!(migrated.settings.shortcuts.paste, paste);

        let custom = json.replace(&previous, "Alt+S");
        let migrated: SessionStore =
            serde_json::from_str(&custom).expect("custom shortcut should load");
        assert_eq!(migrated.settings.shortcuts.toggle_sidebar, "Alt+S");
    }

    #[test]
    fn previous_workspace_width_migrates_without_overwriting_custom_values() {
        let json = r#"{
            "version": 6,
            "settings": {
                "workspace": {
                    "sidebar_width": 260,
                    "tab_width": 172
                }
            }
        }"#;
        let migrated: SessionStore =
            serde_json::from_str(json).expect("previous workspace width should migrate");
        assert_eq!(migrated.version, CURRENT_SCHEMA_VERSION);
        assert_eq!(
            migrated.settings.workspace.sidebar_width,
            DEFAULT_SIDEBAR_WIDTH
        );

        let custom = json.replace("\"sidebar_width\": 260", "\"sidebar_width\": 300");
        let migrated: SessionStore =
            serde_json::from_str(&custom).expect("custom workspace width should load");
        assert_eq!(migrated.settings.workspace.sidebar_width, 300);
    }

    #[test]
    fn terminal_shell_cache_is_normalized_and_only_adds_discoveries() {
        let mut settings = TerminalSettings::normalized(
            2_000,
            120,
            36,
            " zsh ",
            &["zsh".into(), "ZSH".into(), "bad\nshell".into()],
        );
        assert_eq!(settings.local_shell, "zsh");
        assert_eq!(settings.known_shells, [SYSTEM_DEFAULT_SHELL, "zsh"]);
        assert!(!settings.merge_known_shells(["ZSH".into()]));
        assert!(settings.merge_known_shells(["bash".into()]));
        assert_eq!(settings.known_shells, [SYSTEM_DEFAULT_SHELL, "zsh", "bash"]);
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
