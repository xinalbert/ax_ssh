use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{
    CredentialStorage, DEFAULT_APPLICATION_FONT_FAMILY, DEFAULT_COLLAPSED_GROUP_LABEL_CHARS,
    DEFAULT_SCROLLBACK_LINES, DEFAULT_SESSION_MASK_CHARACTER, DEFAULT_SIDEBAR_WIDTH,
    DEFAULT_TAB_WIDTH, DEFAULT_TERMINAL_BRIGHTNESS, DEFAULT_TERMINAL_COLUMNS,
    DEFAULT_TERMINAL_FONT_FAMILY, DEFAULT_TERMINAL_FONT_SIZE, DEFAULT_TERMINAL_LINE_HEIGHT,
    DEFAULT_TERMINAL_ROWS, MAX_COLLAPSED_GROUP_LABEL_CHARS, MAX_FONT_FAMILY_CHARS,
    MAX_KNOWN_SHELLS, MAX_SCROLLBACK_LINES, MAX_SHELL_NAME_CHARS, MAX_SHORTCUT_CHARS,
    MAX_SIDEBAR_WIDTH, MAX_TAB_WIDTH, MAX_TERMINAL_BRIGHTNESS, MAX_TERMINAL_COLUMNS,
    MAX_TERMINAL_FONT_SIZE, MAX_TERMINAL_LINE_HEIGHT, MAX_TERMINAL_ROWS,
    MIN_COLLAPSED_GROUP_LABEL_CHARS, MIN_SCROLLBACK_LINES, MIN_SIDEBAR_WIDTH, MIN_TAB_WIDTH,
    MIN_TERMINAL_BRIGHTNESS, MIN_TERMINAL_COLUMNS, MIN_TERMINAL_FONT_SIZE,
    MIN_TERMINAL_LINE_HEIGHT, MIN_TERMINAL_ROWS, SYSTEM_DEFAULT_SHELL, TerminalColorScheme,
    ThemeSettings,
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppearanceSettings {
    #[serde(default = "default_application_font_family")]
    pub application_font_family: String,
    #[serde(default = "default_terminal_font_family")]
    pub terminal_font_family: String,
    #[serde(default = "default_terminal_font_size")]
    pub terminal_font_size: u16,
    #[serde(default = "default_terminal_line_height")]
    pub terminal_line_height_percent: u16,
    #[serde(default)]
    pub terminal_color_scheme: TerminalColorScheme,
    #[serde(default)]
    pub theme: ThemeSettings,
    #[serde(default = "default_terminal_brightness")]
    pub terminal_brightness_percent: u16,
    #[serde(default = "default_true")]
    pub bright_bold_text: bool,
    #[serde(default, alias = "right_click_copies_selection")]
    pub right_click_copy_or_paste: bool,
}

impl AppearanceSettings {
    pub fn normalized(
        application_font_family: &str,
        terminal_font_family: &str,
        terminal_font_size: i32,
        terminal_line_height_percent: i32,
        color_scheme: &str,
        brightness_percent: i32,
        bright_bold_text: bool,
        right_click_copy_or_paste: bool,
    ) -> Self {
        let terminal_color_scheme = TerminalColorScheme::from_setting(color_scheme);
        Self::normalized_with_theme(
            application_font_family,
            terminal_font_family,
            terminal_font_size,
            terminal_line_height_percent,
            terminal_color_scheme,
            brightness_percent,
            bright_bold_text,
            right_click_copy_or_paste,
            ThemeSettings::from_terminal_color_scheme(terminal_color_scheme),
        )
    }

    fn normalized_with_theme(
        application_font_family: &str,
        terminal_font_family: &str,
        terminal_font_size: i32,
        terminal_line_height_percent: i32,
        terminal_color_scheme: TerminalColorScheme,
        brightness_percent: i32,
        bright_bold_text: bool,
        right_click_copy_or_paste: bool,
        theme: ThemeSettings,
    ) -> Self {
        Self {
            application_font_family: normalize_font_family(
                application_font_family,
                default_application_font_family,
            ),
            terminal_font_family: normalize_font_family(
                terminal_font_family,
                default_terminal_font_family,
            ),
            terminal_font_size: terminal_font_size.clamp(
                i32::from(MIN_TERMINAL_FONT_SIZE),
                i32::from(MAX_TERMINAL_FONT_SIZE),
            ) as u16,
            terminal_line_height_percent: terminal_line_height_percent.clamp(
                i32::from(MIN_TERMINAL_LINE_HEIGHT),
                i32::from(MAX_TERMINAL_LINE_HEIGHT),
            ) as u16,
            terminal_color_scheme,
            theme,
            terminal_brightness_percent: brightness_percent.clamp(
                i32::from(MIN_TERMINAL_BRIGHTNESS),
                i32::from(MAX_TERMINAL_BRIGHTNESS),
            ) as u16,
            bright_bold_text,
            right_click_copy_or_paste,
        }
    }

    pub(super) fn normalize_in_place(&mut self) {
        let theme = ThemeSettings::normalized(
            self.theme.mode.as_setting(),
            self.theme.palette.as_setting(),
            self.theme.custom_light.clone(),
            self.theme.custom_dark.clone(),
        );
        *self = Self::normalized_with_theme(
            &self.application_font_family,
            &self.terminal_font_family,
            i32::from(self.terminal_font_size),
            i32::from(self.terminal_line_height_percent),
            theme.terminal_color_scheme(),
            i32::from(self.terminal_brightness_percent),
            self.bright_bold_text,
            self.right_click_copy_or_paste,
            theme,
        );
    }
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            application_font_family: default_application_font_family(),
            terminal_font_family: default_terminal_font_family(),
            terminal_font_size: default_terminal_font_size(),
            terminal_line_height_percent: default_terminal_line_height(),
            terminal_color_scheme: TerminalColorScheme::default(),
            theme: ThemeSettings::default(),
            terminal_brightness_percent: default_terminal_brightness(),
            bright_bold_text: true,
            right_click_copy_or_paste: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum X11ServerProvider {
    #[default]
    Auto,
    System,
    XQuartz,
    MacXServer,
    VcXsrv,
    Xming,
    Custom,
}

impl X11ServerProvider {
    pub const fn as_setting(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::System => "system",
            Self::XQuartz => "xquartz",
            Self::MacXServer => "macxserver",
            Self::VcXsrv => "vcxsrv",
            Self::Xming => "xming",
            Self::Custom => "custom",
        }
    }

    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "system" => Self::System,
            "xquartz" => Self::XQuartz,
            "macxserver" | "mac-x-server" => Self::MacXServer,
            "vcxsrv" => Self::VcXsrv,
            "xming" => Self::Xming,
            "custom" => Self::Custom,
            _ => Self::Auto,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct X11Settings {
    #[serde(default)]
    pub provider: X11ServerProvider,
    #[serde(default)]
    pub app_path: String,
    #[serde(default = "default_true")]
    pub launch_on_connect: bool,
    #[serde(default)]
    pub allow_no_auth: bool,
}

impl X11Settings {
    pub fn normalized(
        provider: &str,
        app_path: &str,
        launch_on_connect: bool,
        allow_no_auth: bool,
    ) -> Self {
        Self {
            provider: X11ServerProvider::from_setting(provider),
            app_path: normalize_x11_app_path(app_path),
            launch_on_connect,
            allow_no_auth,
        }
    }

    fn normalize_in_place(&mut self) {
        *self = Self::normalized(
            self.provider.as_setting(),
            &self.app_path,
            self.launch_on_connect,
            self.allow_no_auth,
        );
    }
}

impl Default for X11Settings {
    fn default() -> Self {
        Self {
            provider: X11ServerProvider::Auto,
            app_path: String::new(),
            launch_on_connect: true,
            allow_no_auth: false,
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
    /// Whether macOS Option-modified keys should be encoded as terminal Meta.
    #[serde(default)]
    pub option_as_meta: bool,
}

impl TerminalSettings {
    pub fn normalized(
        scrollback_lines: i32,
        default_columns: i32,
        default_rows: i32,
        local_shell: &str,
        known_shells: &[String],
        option_as_meta: bool,
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
            option_as_meta,
        }
    }

    fn normalize_in_place(&mut self) {
        *self = Self::normalized(
            i32::try_from(self.scrollback_lines).unwrap_or(i32::MAX),
            i32::from(self.default_columns),
            i32::from(self.default_rows),
            &self.local_shell,
            &self.known_shells,
            self.option_as_meta,
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
            option_as_meta: false,
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
    #[serde(default = "default_session_mask_character")]
    pub session_mask_character: String,
    /// `0` keeps the entire group name in the widened collapsed sidebar.
    #[serde(default = "default_collapsed_group_label_chars")]
    pub collapsed_group_label_chars: u8,
}

impl WorkspaceSettings {
    pub fn normalized(
        sidebar_width: i32,
        tab_width: i32,
        session_mask_character: &str,
        collapsed_group_label_chars: i32,
    ) -> Self {
        Self {
            sidebar_width: sidebar_width.clamp(MIN_SIDEBAR_WIDTH.into(), MAX_SIDEBAR_WIDTH.into())
                as u16,
            tab_width: tab_width.clamp(MIN_TAB_WIDTH.into(), MAX_TAB_WIDTH.into()) as u16,
            session_mask_character: normalize_session_mask_character(session_mask_character),
            collapsed_group_label_chars: collapsed_group_label_chars.clamp(
                i32::from(MIN_COLLAPSED_GROUP_LABEL_CHARS),
                i32::from(MAX_COLLAPSED_GROUP_LABEL_CHARS),
            ) as u8,
        }
    }

    fn normalize_in_place(&mut self) {
        *self = Self::normalized(
            i32::from(self.sidebar_width),
            i32::from(self.tab_width),
            &self.session_mask_character,
            i32::from(self.collapsed_group_label_chars),
        );
    }
}

impl Default for WorkspaceSettings {
    fn default() -> Self {
        Self {
            sidebar_width: default_sidebar_width(),
            tab_width: default_tab_width(),
            session_mask_character: default_session_mask_character(),
            collapsed_group_label_chars: default_collapsed_group_label_chars(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ShortcutSettings {
    #[serde(default = "default_open_settings_shortcut")]
    pub open_settings: String,
    #[serde(default = "default_new_session_shortcut")]
    pub new_session: String,
    #[serde(default = "default_import_sessions_shortcut")]
    pub import_sessions: String,
    #[serde(default = "default_export_selected_shortcut")]
    pub export_selected: String,
    #[serde(default = "default_toggle_sidebar_shortcut")]
    pub toggle_sidebar: String,
    #[serde(default = "default_copy_selection_shortcut")]
    pub copy_selection: String,
    #[serde(default = "default_paste_shortcut")]
    pub paste: String,
    #[serde(default = "default_open_sftp_shortcut")]
    pub open_sftp: String,
}

impl ShortcutSettings {
    pub fn normalized(
        open_settings: &str,
        new_session: &str,
        import_sessions: &str,
        export_selected: &str,
        toggle_sidebar: &str,
        copy_selection: &str,
        paste: &str,
        open_sftp: &str,
    ) -> Self {
        let candidate = Self {
            open_settings: open_settings.trim().to_owned(),
            new_session: new_session.trim().to_owned(),
            import_sessions: import_sessions.trim().to_owned(),
            export_selected: export_selected.trim().to_owned(),
            toggle_sidebar: toggle_sidebar.trim().to_owned(),
            copy_selection: copy_selection.trim().to_owned(),
            paste: paste.trim().to_owned(),
            open_sftp: open_sftp.trim().to_owned(),
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
            ("new server", self.new_session.as_str()),
            ("import sessions", self.import_sessions.as_str()),
            ("export selected", self.export_selected.as_str()),
            ("toggle sidebar", self.toggle_sidebar.as_str()),
            ("copy selection", self.copy_selection.as_str()),
            ("paste", self.paste.as_str()),
            ("open SFTP", self.open_sftp.as_str()),
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
            new_session: default_new_session_shortcut(),
            import_sessions: default_import_sessions_shortcut(),
            export_selected: default_export_selected_shortcut(),
            toggle_sidebar: default_toggle_sidebar_shortcut(),
            copy_selection: default_copy_selection_shortcut(),
            paste: default_paste_shortcut(),
            open_sftp: default_open_sftp_shortcut(),
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
    #[serde(default)]
    pub x11: X11Settings,
    /// Backend used when the user chooses to remember a password after login.
    #[serde(default)]
    pub credential_storage: CredentialStorage,
}

impl AppSettings {
    pub fn normalized(
        application_font_family: &str,
        terminal_font_family: &str,
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
        option_as_meta: bool,
        sidebar_width: i32,
        tab_width: i32,
        session_mask_character: &str,
        collapsed_group_label_chars: i32,
        open_settings_shortcut: &str,
        new_session_shortcut: &str,
        import_sessions_shortcut: &str,
        export_selected_shortcut: &str,
        toggle_sidebar_shortcut: &str,
        copy_selection_shortcut: &str,
        paste_shortcut: &str,
        open_sftp_shortcut: &str,
        credential_storage: &str,
    ) -> Self {
        Self {
            appearance: AppearanceSettings::normalized(
                application_font_family,
                terminal_font_family,
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
                option_as_meta,
            ),
            workspace: WorkspaceSettings::normalized(
                sidebar_width,
                tab_width,
                session_mask_character,
                collapsed_group_label_chars,
            ),
            shortcuts: ShortcutSettings::normalized(
                open_settings_shortcut,
                new_session_shortcut,
                import_sessions_shortcut,
                export_selected_shortcut,
                toggle_sidebar_shortcut,
                copy_selection_shortcut,
                paste_shortcut,
                open_sftp_shortcut,
            ),
            x11: X11Settings::default(),
            credential_storage: CredentialStorage::from_setting(credential_storage),
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
        self.appearance.normalize_in_place();
        self.terminal.normalize_in_place();
        self.workspace.normalize_in_place();
        self.shortcuts = ShortcutSettings::normalized(
            &self.shortcuts.open_settings,
            &self.shortcuts.new_session,
            &self.shortcuts.import_sessions,
            &self.shortcuts.export_selected,
            &self.shortcuts.toggle_sidebar,
            &self.shortcuts.copy_selection,
            &self.shortcuts.paste,
            &self.shortcuts.open_sftp,
        );
        self.x11.normalize_in_place();
        self.credential_storage =
            CredentialStorage::from_setting(self.credential_storage.as_setting());
    }
}

fn normalize_font_family(value: &str, default: fn() -> String) -> String {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_FONT_FAMILY_CHARS
        || value.chars().any(char::is_control)
    {
        default()
    } else {
        value.to_owned()
    }
}

fn normalize_x11_app_path(value: &str) -> String {
    let value = value.trim();
    if value.chars().count() <= super::MAX_X11_APP_PATH_CHARS
        && !value.chars().any(char::is_control)
    {
        value.to_owned()
    } else {
        String::new()
    }
}

fn default_application_font_family() -> String {
    DEFAULT_APPLICATION_FONT_FAMILY.to_owned()
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

fn default_session_mask_character() -> String {
    DEFAULT_SESSION_MASK_CHARACTER.to_owned()
}

const fn default_collapsed_group_label_chars() -> u8 {
    DEFAULT_COLLAPSED_GROUP_LABEL_CHARS
}

fn normalize_session_mask_character(value: &str) -> String {
    let value = value.trim();
    if value.chars().count() == 1
        && !value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        value.to_owned()
    } else {
        default_session_mask_character()
    }
}

fn default_open_settings_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+,".to_owned()
    } else {
        "Ctrl+,".to_owned()
    }
}

fn default_new_session_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+N".to_owned()
    } else {
        "Ctrl+N".to_owned()
    }
}

fn default_import_sessions_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+Shift+I".to_owned()
    } else {
        "Ctrl+Shift+I".to_owned()
    }
}

fn default_export_selected_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+Shift+E".to_owned()
    } else {
        "Ctrl+Shift+E".to_owned()
    }
}

pub(super) fn default_toggle_sidebar_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+S".to_owned()
    } else {
        "Ctrl+S".to_owned()
    }
}

pub(super) fn previous_toggle_sidebar_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+B".to_owned()
    } else {
        "Ctrl+Shift+B".to_owned()
    }
}

pub(super) fn default_copy_selection_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+C".to_owned()
    } else {
        "Ctrl+Shift+C".to_owned()
    }
}

pub(super) fn default_paste_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+V".to_owned()
    } else {
        "Ctrl+Shift+V".to_owned()
    }
}

fn default_open_sftp_shortcut() -> String {
    "Ctrl+M".to_owned()
}
