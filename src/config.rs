//! Persistent session profiles.
//!
//! The config layer is deliberately independent from Slint and russh. It owns
//! the on-disk schema and can therefore be tested without creating a window or
//! opening a network connection.

mod persistence;
mod session;
mod settings;
mod theme;
mod workspace;

#[cfg(test)]
mod tests;

pub use crate::terminal_dimensions::{
    MAX_TERMINAL_COLUMNS, MAX_TERMINAL_ROWS, MIN_TERMINAL_COLUMNS, MIN_TERMINAL_ROWS,
};

pub use self::persistence::ConfigStore;
pub(crate) use self::persistence::write_private_file_atomically;
pub use self::session::{
    AuthMethod, ConnectionProfile, CredentialStorage, MAX_HOST_CHARS, MAX_PRIVATE_KEY_PATH_CHARS,
    MAX_SESSION_NAME_CHARS, MAX_USERNAME_CHARS, SerialConfig, SerialDataBits, SerialFlowControl,
    SerialParity, SerialStopBits, SessionProfile, SessionProtocol, SessionStore, SshConfig,
    TelnetConfig, normalize_group_name,
};
#[cfg(test)]
use self::session::{MAX_GROUPS, MAX_SESSION_PROFILES};
pub use self::settings::{
    AppSettings, AppSettingsInput, AppearanceSettings, AppearanceSettingsInput, RendererPreference,
    ShortcutSettings, SoftwarePresentationMode, TerminalSemanticColors,
    TerminalSemanticColorsInput, TerminalSettings, TerminalSettingsInput, UiLanguage,
    WorkspaceSettings, WorkspaceSettingsInput, X11ServerProvider, X11Settings,
};
pub use self::theme::{
    TerminalColorScheme, ThemeMode, ThemePalette, ThemePaletteKind, ThemeSettings,
};
pub use self::workspace::{
    PaneNodeSnapshot, WORKSPACE_SNAPSHOT_VERSION, WorkspaceSnapshot, WorkspaceTabSnapshot,
    WorkspaceWindowSnapshot,
};

#[cfg(test)]
use self::settings::{
    default_copy_selection_shortcut, default_paste_shortcut, default_toggle_sidebar_shortcut,
    previous_toggle_sidebar_shortcut,
};
#[cfg(test)]
use self::theme::{default_theme_border, role_meets_contrast, theme_contrast_ratio};

pub const DEFAULT_APPLICATION_FONT_FAMILY: &str = "JetBrains Mono";
pub const DEFAULT_TERMINAL_FONT_FAMILY: &str = "JetBrains Mono";
pub const MIN_TERMINAL_FONT_SIZE: u16 = 9;
pub const MAX_TERMINAL_FONT_SIZE: u16 = 32;
pub const MIN_TERMINAL_LINE_HEIGHT: u16 = 100;
pub const MAX_TERMINAL_LINE_HEIGHT: u16 = 200;
pub const MIN_TERMINAL_SOFTWARE_BLOCK_ROWS: u16 = 1;
pub const MAX_TERMINAL_SOFTWARE_BLOCK_ROWS: u16 = 16;
pub const MIN_TERMINAL_TEXT_BRIGHTNESS_PERCENT: u16 = 60;
pub const MAX_TERMINAL_TEXT_BRIGHTNESS_PERCENT: u16 = 120;
pub const MIN_SCROLLBACK_LINES: u32 = 100;
pub const MAX_SCROLLBACK_LINES: u32 = 50_000;
pub const MIN_SIDEBAR_WIDTH: u16 = 180;
pub const MAX_SIDEBAR_WIDTH: u16 = 420;
pub const MIN_TAB_WIDTH: u16 = 120;
pub const MAX_TAB_WIDTH: u16 = 260;
pub const MIN_COLLAPSED_GROUP_LABEL_CHARS: u8 = 0;
pub const MAX_COLLAPSED_GROUP_LABEL_CHARS: u8 = 4;
pub const SYSTEM_DEFAULT_SHELL: &str = "System default";
pub const DEFAULT_SESSION_MASK_CHARACTER: &str = "*";
pub(crate) const MAX_CONFIG_FILE_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_TERMINAL_FONT_SIZE: u16 = 14;
const DEFAULT_TERMINAL_LINE_HEIGHT: u16 = 120;
const DEFAULT_TERMINAL_SOFTWARE_BLOCK_ROWS: u16 = 4;
const DEFAULT_TERMINAL_TEXT_BRIGHTNESS_PERCENT: u16 = 100;
const DEFAULT_FOCUSED_TERMINAL_REFRESH_FPS: u16 = 60;
const DEFAULT_UNFOCUSED_TERMINAL_REFRESH_FPS: u16 = 4;
const DEFAULT_SCROLLBACK_LINES: u32 = 2_000;
const DEFAULT_TERMINAL_COLUMNS: u16 = 120;
const DEFAULT_TERMINAL_ROWS: u16 = 36;
const DEFAULT_SIDEBAR_WIDTH: u16 = 220;
const PREVIOUS_DEFAULT_SIDEBAR_WIDTH: u16 = 260;
const DEFAULT_TAB_WIDTH: u16 = 172;
const DEFAULT_COLLAPSED_GROUP_LABEL_CHARS: u8 = 2;
const CURRENT_SCHEMA_VERSION: u32 = 28;
const TERMINAL_TEXT_BRIGHTNESS_SCHEMA_VERSION: u32 = 22;
const PLATFORM_SHORTCUT_SCHEMA_VERSION: u32 = 6;
const WORKSPACE_DENSITY_SCHEMA_VERSION: u32 = 7;
const THEME_SETTINGS_SCHEMA_VERSION: u32 = 9;
const MAX_FONT_FAMILY_CHARS: usize = 128;
const MAX_SHORTCUT_CHARS: usize = 64;
const MAX_KNOWN_SHELLS: usize = 32;
const MAX_SHELL_NAME_CHARS: usize = 256;
const MAX_X11_APP_PATH_CHARS: usize = 1_024;
const MIN_TERMINAL_REFRESH_FPS: u16 = 1;
const MAX_TERMINAL_REFRESH_FPS: u16 = 120;
