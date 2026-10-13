//! Persistent appearance settings and normalization.

use super::*;

/// Selects the renderer used when the next application process starts.
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RendererPreference {
    #[default]
    Automatic,
    Gpu,
    Software,
}

/// Selects how the terminal grid is drawn inside the selected Slint backend.
///
/// `ItemTree` is the existing, fully supported renderer. `Canvas` reserves a
/// stable configuration boundary for the future batched glyph renderer while
/// preserving the current default and persisted-settings compatibility.
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalDrawingPreference {
    #[default]
    ItemTree,
    Canvas,
}

impl TerminalDrawingPreference {
    pub const fn as_setting(self) -> &'static str {
        match self {
            Self::ItemTree => "item-tree",
            Self::Canvas => "canvas",
        }
    }

    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "canvas" => Self::Canvas,
            _ => Self::ItemTree,
        }
    }
}

impl<'de> Deserialize<'de> for TerminalDrawingPreference {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::from_setting(&String::deserialize(deserializer)?))
    }
}

impl RendererPreference {
    pub const fn as_setting(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Gpu => "gpu",
            Self::Software => "software",
        }
    }

    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "gpu" => Self::Gpu,
            "software" => Self::Software,
            _ => Self::Automatic,
        }
    }
}

impl<'de> Deserialize<'de> for RendererPreference {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::from_setting(&String::deserialize(deserializer)?))
    }
}

/// Selects the macOS CPU presentation path used by newly created surfaces.
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SoftwarePresentationMode {
    LayerImages,
    #[default]
    DamageBackingStore,
}

impl SoftwarePresentationMode {
    pub const fn as_setting(self) -> &'static str {
        match self {
            Self::LayerImages => "layer-images",
            Self::DamageBackingStore => "damage-backing-store",
        }
    }

    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "layer-images" => Self::LayerImages,
            "damage-backing-store" => Self::DamageBackingStore,
            _ => Self::default(),
        }
    }
}

impl<'de> Deserialize<'de> for SoftwarePresentationMode {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::from_setting(&String::deserialize(deserializer)?))
    }
}

/// Raw appearance values supplied by an application settings surface.
#[derive(Clone, Copy, Debug)]
pub struct AppearanceSettingsInput<'a> {
    pub renderer_preference: &'a str,
    pub terminal_drawing_preference: &'a str,
    pub software_presentation: &'a str,
    pub application_font_family: &'a str,
    pub terminal_font_family: &'a str,
    pub terminal_font_size: i32,
    pub terminal_line_height_percent: i32,
    pub terminal_software_block_rows: i32,
    pub color_scheme: &'a str,
    pub text_brightness: f32,
    pub semantic_highlighting: bool,
    pub terminal_compact_rendering: bool,
    pub terminal_row_render_cache: bool,
    pub terminal_cursor_blink: bool,
    pub system_icons_follow_theme: bool,
    pub focused_terminal_refresh_fps: i32,
    pub unfocused_terminal_refresh_fps: i32,
    pub terminal_semantic_colors: TerminalSemanticColorsInput<'a>,
    pub bright_bold_text: bool,
    pub right_click_copy_or_paste: bool,
    pub copy_selection_on_select: bool,
    pub terminal_mouse_local_selection_priority: bool,
}

/// Raw Terminal semantic highlight colors supplied by an application settings surface.
#[derive(Clone, Copy, Debug)]
pub struct TerminalSemanticColorsInput<'a> {
    pub link: &'a str,
    pub success: &'a str,
    pub info: &'a str,
    pub warning: &'a str,
    pub error: &'a str,
}

/// Optional user colors for Terminal semantic highlights.
///
/// Empty values preserve the active terminal theme's ANSI-derived color. Any
/// non-empty persisted value is canonical, opaque `#RRGGBB` text so this
/// configuration boundary remains independent from Slint's `Color` type.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct TerminalSemanticColors {
    #[serde(default)]
    pub link: String,
    #[serde(default)]
    pub success: String,
    #[serde(default)]
    pub info: String,
    #[serde(default)]
    pub warning: String,
    #[serde(default)]
    pub error: String,
}

impl TerminalSemanticColors {
    pub fn normalized(input: TerminalSemanticColorsInput<'_>) -> Self {
        Self {
            link: normalize_terminal_semantic_color(input.link),
            success: normalize_terminal_semantic_color(input.success),
            info: normalize_terminal_semantic_color(input.info),
            warning: normalize_terminal_semantic_color(input.warning),
            error: normalize_terminal_semantic_color(input.error),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppearanceSettings {
    #[serde(default)]
    pub renderer_preference: RendererPreference,
    #[serde(default)]
    pub terminal_drawing_preference: TerminalDrawingPreference,
    #[serde(default)]
    pub software_presentation: SoftwarePresentationMode,
    #[serde(default = "default_application_font_family")]
    pub application_font_family: String,
    #[serde(default = "default_terminal_font_family")]
    pub terminal_font_family: String,
    #[serde(default = "default_terminal_font_size")]
    pub terminal_font_size: u16,
    #[serde(default = "default_terminal_line_height")]
    pub terminal_line_height_percent: u16,
    #[serde(default = "default_terminal_software_block_rows")]
    pub terminal_software_block_rows: u16,
    #[serde(default)]
    pub terminal_color_scheme: TerminalColorScheme,
    #[serde(default)]
    pub theme: ThemeSettings,
    #[serde(default = "default_terminal_text_brightness_percent")]
    pub terminal_text_brightness_percent: u16,
    #[serde(default)]
    pub terminal_semantic_highlighting: bool,
    #[serde(default = "default_true")]
    pub terminal_compact_rendering: bool,
    #[serde(default)]
    pub terminal_row_render_cache: bool,
    #[serde(default = "default_true")]
    pub terminal_cursor_blink: bool,
    /// Tint system icons with the UI theme. Disabled to preserve source colors.
    #[serde(default)]
    pub system_icons_follow_theme: bool,
    #[serde(
        default,
        deserialize_with = "crate::config::system_icons::deserialize_optional_icon"
    )]
    pub local_shell_icon_override: Option<String>,
    #[serde(default = "default_focused_terminal_refresh_fps")]
    pub focused_terminal_refresh_fps: u16,
    #[serde(default = "default_unfocused_terminal_refresh_fps")]
    pub unfocused_terminal_refresh_fps: u16,
    #[serde(default)]
    pub terminal_semantic_colors: TerminalSemanticColors,
    #[serde(default = "default_true")]
    pub bright_bold_text: bool,
    #[serde(default, alias = "right_click_copies_selection")]
    pub right_click_copy_or_paste: bool,
    /// Whether a completed terminal selection is copied to the clipboard.
    #[serde(default)]
    pub copy_selection_on_select: bool,
    /// Prefer local text selection while terminal mouse reporting is active.
    /// Disabled by default so reporting applications receive standard xterm input.
    #[serde(default)]
    pub terminal_mouse_local_selection_priority: bool,
}

fn default_terminal_software_block_rows() -> u16 {
    DEFAULT_TERMINAL_SOFTWARE_BLOCK_ROWS
}

impl AppearanceSettings {
    pub fn normalized(input: AppearanceSettingsInput<'_>) -> Self {
        let terminal_color_scheme = TerminalColorScheme::from_setting(input.color_scheme);
        let theme = ThemeSettings::from_terminal_color_scheme(terminal_color_scheme);
        Self::normalized_with_theme(input, theme)
    }

    fn normalized_with_theme(input: AppearanceSettingsInput<'_>, theme: ThemeSettings) -> Self {
        Self {
            renderer_preference: RendererPreference::from_setting(input.renderer_preference),
            terminal_drawing_preference: TerminalDrawingPreference::from_setting(
                input.terminal_drawing_preference,
            ),
            software_presentation: SoftwarePresentationMode::from_setting(
                input.software_presentation,
            ),
            application_font_family: normalize_font_family(
                input.application_font_family,
                default_application_font_family,
            ),
            terminal_font_family: normalize_font_family(
                input.terminal_font_family,
                default_terminal_font_family,
            ),
            terminal_font_size: input.terminal_font_size.clamp(
                i32::from(MIN_TERMINAL_FONT_SIZE),
                i32::from(MAX_TERMINAL_FONT_SIZE),
            ) as u16,
            terminal_line_height_percent: input.terminal_line_height_percent.clamp(
                i32::from(MIN_TERMINAL_LINE_HEIGHT),
                i32::from(MAX_TERMINAL_LINE_HEIGHT),
            ) as u16,
            terminal_software_block_rows: input.terminal_software_block_rows.clamp(
                i32::from(MIN_TERMINAL_SOFTWARE_BLOCK_ROWS),
                i32::from(MAX_TERMINAL_SOFTWARE_BLOCK_ROWS),
            ) as u16,
            terminal_color_scheme: theme.terminal_color_scheme(),
            theme,
            terminal_text_brightness_percent: normalize_terminal_text_brightness(
                input.text_brightness,
            ),
            terminal_semantic_highlighting: input.semantic_highlighting,
            terminal_compact_rendering: input.terminal_compact_rendering,
            terminal_row_render_cache: input.terminal_row_render_cache,
            terminal_cursor_blink: input.terminal_cursor_blink,
            system_icons_follow_theme: input.system_icons_follow_theme,
            local_shell_icon_override: None,
            focused_terminal_refresh_fps: normalize_terminal_refresh_fps(
                input.focused_terminal_refresh_fps,
                default_focused_terminal_refresh_fps,
            ),
            unfocused_terminal_refresh_fps: normalize_terminal_refresh_fps(
                input.unfocused_terminal_refresh_fps,
                default_unfocused_terminal_refresh_fps,
            ),
            terminal_semantic_colors: TerminalSemanticColors::normalized(
                input.terminal_semantic_colors,
            ),
            bright_bold_text: input.bright_bold_text,
            right_click_copy_or_paste: input.right_click_copy_or_paste,
            copy_selection_on_select: input.copy_selection_on_select,
            terminal_mouse_local_selection_priority: input.terminal_mouse_local_selection_priority,
        }
    }

    pub(in crate::config) fn normalize_in_place(&mut self) {
        let theme = ThemeSettings::normalized(
            self.theme.mode.as_setting(),
            self.theme.palette.as_setting(),
            self.theme.custom_light.clone(),
            self.theme.custom_dark.clone(),
        );
        let input = AppearanceSettingsInput {
            renderer_preference: self.renderer_preference.as_setting(),
            terminal_drawing_preference: self.terminal_drawing_preference.as_setting(),
            software_presentation: self.software_presentation.as_setting(),
            application_font_family: &self.application_font_family,
            terminal_font_family: &self.terminal_font_family,
            terminal_font_size: i32::from(self.terminal_font_size),
            terminal_line_height_percent: i32::from(self.terminal_line_height_percent),
            terminal_software_block_rows: i32::from(self.terminal_software_block_rows),
            color_scheme: self.terminal_color_scheme.as_setting(),
            text_brightness: f32::from(self.terminal_text_brightness_percent) / 100.0,
            semantic_highlighting: self.terminal_semantic_highlighting,
            terminal_compact_rendering: self.terminal_compact_rendering,
            terminal_row_render_cache: self.terminal_row_render_cache,
            terminal_cursor_blink: self.terminal_cursor_blink,
            system_icons_follow_theme: self.system_icons_follow_theme,
            focused_terminal_refresh_fps: i32::from(self.focused_terminal_refresh_fps),
            unfocused_terminal_refresh_fps: i32::from(self.unfocused_terminal_refresh_fps),
            terminal_semantic_colors: TerminalSemanticColorsInput {
                link: &self.terminal_semantic_colors.link,
                success: &self.terminal_semantic_colors.success,
                info: &self.terminal_semantic_colors.info,
                warning: &self.terminal_semantic_colors.warning,
                error: &self.terminal_semantic_colors.error,
            },
            bright_bold_text: self.bright_bold_text,
            right_click_copy_or_paste: self.right_click_copy_or_paste,
            copy_selection_on_select: self.copy_selection_on_select,
            terminal_mouse_local_selection_priority: self.terminal_mouse_local_selection_priority,
        };
        let icon = self
            .local_shell_icon_override
            .as_deref()
            .and_then(crate::config::system_icon_id)
            .map(str::to_owned);
        *self = Self::normalized_with_theme(input, theme);
        self.local_shell_icon_override = icon;
    }
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            renderer_preference: RendererPreference::Automatic,
            terminal_drawing_preference: TerminalDrawingPreference::ItemTree,
            software_presentation: SoftwarePresentationMode::default(),
            application_font_family: default_application_font_family(),
            terminal_font_family: default_terminal_font_family(),
            terminal_font_size: default_terminal_font_size(),
            terminal_line_height_percent: default_terminal_line_height(),
            terminal_software_block_rows: default_terminal_software_block_rows(),
            terminal_color_scheme: TerminalColorScheme::default(),
            theme: ThemeSettings::default(),
            terminal_text_brightness_percent: default_terminal_text_brightness_percent(),
            terminal_semantic_highlighting: false,
            terminal_compact_rendering: true,
            terminal_row_render_cache: false,
            terminal_cursor_blink: true,
            system_icons_follow_theme: false,
            local_shell_icon_override: None,
            focused_terminal_refresh_fps: default_focused_terminal_refresh_fps(),
            unfocused_terminal_refresh_fps: default_unfocused_terminal_refresh_fps(),
            terminal_semantic_colors: TerminalSemanticColors::default(),
            bright_bold_text: true,
            right_click_copy_or_paste: false,
            copy_selection_on_select: false,
            terminal_mouse_local_selection_priority: false,
        }
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

const fn default_terminal_text_brightness_percent() -> u16 {
    DEFAULT_TERMINAL_TEXT_BRIGHTNESS_PERCENT
}

const fn default_focused_terminal_refresh_fps() -> u16 {
    DEFAULT_FOCUSED_TERMINAL_REFRESH_FPS
}

const fn default_unfocused_terminal_refresh_fps() -> u16 {
    DEFAULT_UNFOCUSED_TERMINAL_REFRESH_FPS
}

fn normalize_terminal_refresh_fps(value: i32, fallback: fn() -> u16) -> u16 {
    value
        .clamp(
            i32::from(MIN_TERMINAL_REFRESH_FPS),
            i32::from(MAX_TERMINAL_REFRESH_FPS),
        )
        .try_into()
        .unwrap_or_else(|_| fallback())
}

fn normalize_terminal_text_brightness(value: f32) -> u16 {
    let percent = if value.is_finite() {
        (value * 100.0).round() as i32
    } else {
        i32::from(DEFAULT_TERMINAL_TEXT_BRIGHTNESS_PERCENT)
    };
    let percent = percent.clamp(
        i32::from(MIN_TERMINAL_TEXT_BRIGHTNESS_PERCENT),
        i32::from(MAX_TERMINAL_TEXT_BRIGHTNESS_PERCENT),
    );
    ((percent + 2) / 5 * 5) as u16
}

fn normalize_terminal_semantic_color(value: &str) -> String {
    let value = value.trim();
    let Some(value) = value.strip_prefix('#') else {
        return String::new();
    };
    if value.len() != 6 || !value.bytes().all(|digit| digit.is_ascii_hexdigit()) {
        return String::new();
    }
    format!("#{}", value.to_ascii_uppercase())
}
