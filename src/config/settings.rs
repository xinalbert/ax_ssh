//! Settings facade and aggregate application configuration.

mod appearance;
mod audio;
mod sftp;
mod shortcuts;
mod terminal;
mod workspace;
mod x11;

pub use self::appearance::{
    AppearanceSettings, AppearanceSettingsInput, RendererPreference, SoftwarePresentationMode,
    TerminalDrawingPreference, TerminalSemanticColors, TerminalSemanticColorsInput,
};
pub use self::audio::AudioSettings;
pub use self::sftp::{SftpTransferFilterSettings, SftpTransferPolicy, SftpTransferSettings};
pub use self::shortcuts::ShortcutSettings;
pub use self::terminal::{TerminalSettings, TerminalSettingsInput};
pub use self::workspace::{WorkspaceSettings, WorkspaceSettingsInput};
pub use self::x11::{X11ServerProvider, X11Settings};

#[cfg(test)]
pub(super) use self::shortcuts::{default_copy_selection_shortcut, default_paste_shortcut};
pub(super) use self::shortcuts::{
    default_toggle_sidebar_shortcut, previous_toggle_sidebar_shortcut,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{
    CredentialStorage, DEFAULT_APPLICATION_FONT_FAMILY, DEFAULT_COLLAPSED_GROUP_LABEL_CHARS,
    DEFAULT_FOCUSED_TERMINAL_REFRESH_FPS, DEFAULT_SCROLLBACK_LINES, DEFAULT_SESSION_MASK_CHARACTER,
    DEFAULT_SFTP_GLOBAL_UPLOADS, DEFAULT_SFTP_LOCAL_OPEN_MAX_FILE_MIB, DEFAULT_SFTP_MAX_FILE_GIB,
    DEFAULT_SFTP_PER_TAB_TRANSFERS, DEFAULT_SFTP_REMOTE_EDIT_MAX_FILE_MIB, DEFAULT_SIDEBAR_WIDTH,
    DEFAULT_TAB_WIDTH, DEFAULT_TERMINAL_COLUMNS, DEFAULT_TERMINAL_FONT_FAMILY,
    DEFAULT_TERMINAL_FONT_SIZE, DEFAULT_TERMINAL_LINE_HEIGHT, DEFAULT_TERMINAL_ROWS,
    DEFAULT_TERMINAL_SOFTWARE_BLOCK_ROWS, DEFAULT_TERMINAL_TEXT_BRIGHTNESS_PERCENT,
    DEFAULT_UNFOCUSED_TERMINAL_REFRESH_FPS, MAX_COLLAPSED_GROUP_LABEL_CHARS, MAX_FONT_FAMILY_CHARS,
    MAX_KNOWN_SHELLS, MAX_SCROLLBACK_LINES, MAX_SFTP_GLOBAL_UPLOADS,
    MAX_SFTP_LOCAL_OPEN_MAX_FILE_MIB, MAX_SFTP_MAX_FILE_GIB, MAX_SFTP_PER_TAB_TRANSFERS,
    MAX_SFTP_RATE_LIMIT_MIB_PER_SECOND, MAX_SFTP_REMOTE_EDIT_MAX_FILE_MIB, MAX_SHELL_NAME_CHARS,
    MAX_SHORTCUT_CHARS, MAX_SIDEBAR_WIDTH, MAX_TAB_WIDTH, MAX_TERMINAL_COLUMNS,
    MAX_TERMINAL_FONT_SIZE, MAX_TERMINAL_LINE_HEIGHT, MAX_TERMINAL_REFRESH_FPS, MAX_TERMINAL_ROWS,
    MAX_TERMINAL_SOFTWARE_BLOCK_ROWS, MAX_TERMINAL_TEXT_BRIGHTNESS_PERCENT,
    MIN_COLLAPSED_GROUP_LABEL_CHARS, MIN_SCROLLBACK_LINES, MIN_SFTP_GLOBAL_UPLOADS,
    MIN_SFTP_PER_TAB_TRANSFERS, MIN_SIDEBAR_WIDTH, MIN_TAB_WIDTH, MIN_TERMINAL_COLUMNS,
    MIN_TERMINAL_FONT_SIZE, MIN_TERMINAL_LINE_HEIGHT, MIN_TERMINAL_REFRESH_FPS, MIN_TERMINAL_ROWS,
    MIN_TERMINAL_SOFTWARE_BLOCK_ROWS, MIN_TERMINAL_TEXT_BRIGHTNESS_PERCENT, SYSTEM_DEFAULT_SHELL,
    TerminalColorScheme, ThemeSettings, UiLanguage,
};

/// Raw application settings grouped by their persistent ownership domains.
#[derive(Clone, Debug)]
pub struct AppSettingsInput<'a> {
    pub appearance: AppearanceSettingsInput<'a>,
    pub terminal: TerminalSettingsInput<'a>,
    pub workspace: WorkspaceSettingsInput<'a>,
    pub shortcuts: ShortcutSettings,
    pub credential_storage: &'a str,
    pub ui_language: &'a str,
    pub sftp_per_tab_transfers: i32,
    pub sftp_global_uploads: i32,
    pub sftp_max_file_gib: i32,
    pub sftp_rate_limit_mib_per_second: i32,
    pub sftp_local_open_max_file_mib: i32,
    pub sftp_remote_edit_max_file_mib: i32,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppSettings {
    #[serde(default)]
    pub audio: AudioSettings,
    #[serde(default)]
    pub ui_language: UiLanguage,
    #[serde(default)]
    pub appearance: AppearanceSettings,
    #[serde(default)]
    pub terminal: TerminalSettings,
    #[serde(default)]
    pub workspace: WorkspaceSettings,
    #[serde(default)]
    pub shortcuts: ShortcutSettings,
    #[serde(default)]
    pub x11: X11Settings,
    /// Backend used when the user chooses to remember a password after login.
    #[serde(default)]
    pub credential_storage: CredentialStorage,
    #[serde(default)]
    pub sftp_transfer_filters: SftpTransferFilterSettings,
    #[serde(default)]
    pub sftp_transfers: SftpTransferSettings,
}

impl AppSettings {
    pub fn normalized(input: AppSettingsInput<'_>) -> Self {
        Self {
            audio: AudioSettings::default(),
            ui_language: UiLanguage::from_setting(input.ui_language),
            appearance: AppearanceSettings::normalized(input.appearance),
            terminal: TerminalSettings::normalized(input.terminal),
            workspace: WorkspaceSettings::normalized(input.workspace),
            shortcuts: ShortcutSettings::normalized(input.shortcuts),
            x11: X11Settings::default(),
            credential_storage: CredentialStorage::from_setting(input.credential_storage),
            sftp_transfer_filters: SftpTransferFilterSettings::default(),
            sftp_transfers: SftpTransferSettings::normalized(
                input.sftp_per_tab_transfers,
                input.sftp_global_uploads,
                input.sftp_max_file_gib,
                input.sftp_rate_limit_mib_per_second,
                input.sftp_local_open_max_file_mib,
                input.sftp_remote_edit_max_file_mib,
            ),
        }
    }

    pub fn set_theme(&mut self, theme: ThemeSettings) {
        let theme = ThemeSettings::normalized(
            theme.mode.as_setting(),
            theme.palette.as_setting(),
            theme.custom_light,
            theme.custom_dark,
        );
        self.appearance.terminal_color_scheme = theme.terminal_color_scheme();
        self.appearance.theme = theme;
    }

    pub(super) fn normalize_in_place(&mut self) {
        let format = self.audio.format();
        self.audio.sample_rate = format.sample_rate;
        self.audio.channels = format.channels;
        self.ui_language = UiLanguage::from_setting(self.ui_language.as_setting());
        self.appearance.normalize_in_place();
        self.terminal.normalize_in_place();
        self.workspace.normalize_in_place();
        self.shortcuts = ShortcutSettings::normalized(self.shortcuts.clone());
        self.x11.normalize_in_place();
        self.credential_storage =
            CredentialStorage::from_setting(self.credential_storage.as_setting());
        self.sftp_transfer_filters.normalize_in_place();
        self.sftp_transfers = SftpTransferSettings::normalized(
            i32::from(self.sftp_transfers.per_tab_transfers),
            i32::from(self.sftp_transfers.global_uploads),
            i32::try_from(self.sftp_transfers.max_file_gib).unwrap_or(i32::MAX),
            i32::try_from(self.sftp_transfers.rate_limit_mib_per_second).unwrap_or(i32::MAX),
            i32::try_from(self.sftp_transfers.local_open_max_file_mib).unwrap_or(i32::MAX),
            i32::try_from(self.sftp_transfers.remote_edit_max_file_mib).unwrap_or(i32::MAX),
        );
    }
}

const fn default_true() -> bool {
    true
}
