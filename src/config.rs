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
pub const MIN_TERMINAL_COLUMNS: u16 = 10;
pub const MAX_TERMINAL_COLUMNS: u16 = 300;
pub const MIN_TERMINAL_ROWS: u16 = 3;
pub const MAX_TERMINAL_ROWS: u16 = 100;
pub const MIN_SIDEBAR_WIDTH: u16 = 180;
pub const MAX_SIDEBAR_WIDTH: u16 = 420;
pub const MIN_TAB_WIDTH: u16 = 120;
pub const MAX_TAB_WIDTH: u16 = 260;
pub const SYSTEM_DEFAULT_SHELL: &str = "System default";
pub const DEFAULT_SESSION_MASK_CHARACTER: &str = "*";
const DEFAULT_TERMINAL_FONT_SIZE: u16 = 14;
const DEFAULT_TERMINAL_LINE_HEIGHT: u16 = 120;
const DEFAULT_TERMINAL_BRIGHTNESS: u16 = 100;
const DEFAULT_SCROLLBACK_LINES: u32 = 2_000;
const DEFAULT_TERMINAL_COLUMNS: u16 = 120;
const DEFAULT_TERMINAL_ROWS: u16 = 36;
const DEFAULT_SIDEBAR_WIDTH: u16 = 220;
const PREVIOUS_DEFAULT_SIDEBAR_WIDTH: u16 = 260;
const DEFAULT_TAB_WIDTH: u16 = 172;
const CURRENT_SCHEMA_VERSION: u32 = 11;
const PLATFORM_SHORTCUT_SCHEMA_VERSION: u32 = 6;
const WORKSPACE_DENSITY_SCHEMA_VERSION: u32 = 7;
const THEME_SETTINGS_SCHEMA_VERSION: u32 = 9;
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

/// User-selectable application theme source.
///
/// The UI resolves `System` from Slint's current system color scheme; this
/// domain value deliberately has no dependency on a windowing toolkit.
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeMode {
    System,
    #[default]
    Dark,
    Light,
}

impl ThemeMode {
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "system" | "follow-system" | "auto" => Self::System,
            "light" => Self::Light,
            _ => Self::Dark,
        }
    }

    pub const fn as_setting(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

impl<'de> Deserialize<'de> for ThemeMode {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from_setting(&value))
    }
}

/// Color family applied independently from the system/light/dark strategy.
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePaletteKind {
    #[default]
    AxSsh,
    Solarized,
    Custom,
}

impl ThemePaletteKind {
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "solarized" | "solarized-dark" | "solarized dark" => Self::Solarized,
            "custom" => Self::Custom,
            _ => Self::AxSsh,
        }
    }

    pub const fn as_setting(self) -> &'static str {
        match self {
            Self::AxSsh => "axssh",
            Self::Solarized => "solarized",
            Self::Custom => "custom",
        }
    }
}

impl<'de> Deserialize<'de> for ThemePaletteKind {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from_setting(&value))
    }
}

/// Complete semantic color palette for a user-defined application theme.
///
/// Values are canonical CSS-like hex strings so persistence remains independent
/// from Slint's `Color` type. The overlay accepts alpha (`#RRGGBBAA`); all other
/// roles also accept it to keep a future visual token expansion compatible.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ThemePalette {
    #[serde(default = "default_theme_background")]
    pub background: String,
    #[serde(default = "default_theme_panel")]
    pub panel: String,
    #[serde(default = "default_theme_panel_alt")]
    pub panel_alt: String,
    #[serde(default = "default_theme_border")]
    pub border: String,
    #[serde(default = "default_theme_text")]
    pub text: String,
    #[serde(default = "default_theme_muted")]
    pub muted: String,
    #[serde(default = "default_theme_accent")]
    pub accent: String,
    #[serde(default = "default_theme_success")]
    pub success: String,
    #[serde(default = "default_theme_danger")]
    pub danger: String,
    #[serde(default = "default_theme_overlay")]
    pub overlay: String,
    #[serde(default = "default_theme_terminal_foreground")]
    pub terminal_foreground: String,
    #[serde(default = "default_theme_terminal_background")]
    pub terminal_background: String,
    #[serde(default = "default_theme_terminal_selection")]
    pub terminal_selection: String,
}

impl ThemePalette {
    pub fn normalized(
        background: &str,
        panel: &str,
        panel_alt: &str,
        border: &str,
        text: &str,
        muted: &str,
        accent: &str,
        success: &str,
        danger: &str,
        overlay: &str,
        terminal_foreground: &str,
        terminal_background: &str,
        terminal_selection: &str,
    ) -> Self {
        let defaults = Self::default();
        Self {
            background: normalize_theme_color(background, &defaults.background),
            panel: normalize_theme_color(panel, &defaults.panel),
            panel_alt: normalize_theme_color(panel_alt, &defaults.panel_alt),
            border: normalize_theme_color(border, &defaults.border),
            text: normalize_theme_color(text, &defaults.text),
            muted: normalize_theme_color(muted, &defaults.muted),
            accent: normalize_theme_color(accent, &defaults.accent),
            success: normalize_theme_color(success, &defaults.success),
            danger: normalize_theme_color(danger, &defaults.danger),
            overlay: normalize_theme_color(overlay, &defaults.overlay),
            terminal_foreground: normalize_theme_color(
                terminal_foreground,
                &defaults.terminal_foreground,
            ),
            terminal_background: normalize_theme_color(
                terminal_background,
                &defaults.terminal_background,
            ),
            terminal_selection: normalize_theme_color(
                terminal_selection,
                &defaults.terminal_selection,
            ),
        }
    }

    pub fn axssh_light() -> Self {
        Self::from_hex([
            "#F7F8F7",
            "#FFFFFF",
            "#EDF1EE",
            "#6D7972",
            "#1C2520",
            "#526058",
            "#116B54",
            "#126A43",
            "#A72C28",
            "#10201899",
            "#333333",
            "#FFFFFF",
            "#ADD6FF",
        ])
    }

    pub fn axssh_dark() -> Self {
        Self::from_hex([
            "#171918",
            "#202321",
            "#292D2A",
            "#727C76",
            "#EDF1EE",
            "#A8B2AC",
            "#52C7A5",
            "#63D6A9",
            "#FF8E88",
            "#00000099",
            "#CCCCCC",
            "#1E1E1E",
            "#264F78",
        ])
    }

    pub fn solarized_light() -> Self {
        Self::from_hex([
            "#FDF6E3",
            "#FFFCF2",
            "#EEE8D5",
            "#817A67",
            "#25363B",
            "#4B6066",
            "#006B61",
            "#5E6D00",
            "#A62F2C",
            "#3B352699",
            "#3B5056",
            "#FDF6E3",
            "#B7C8CA",
        ])
    }

    pub fn solarized_dark() -> Self {
        Self::from_hex([
            "#002B36",
            "#073642",
            "#0B4652",
            "#829294",
            "#EEE8D5",
            "#B0BABA",
            "#3FC8BE",
            "#B8C84A",
            "#FF8D86",
            "#001E26A8",
            "#BCC6C6",
            "#002B36",
            "#0F5362",
        ])
    }

    fn from_hex(values: [&str; 13]) -> Self {
        Self {
            background: values[0].to_owned(),
            panel: values[1].to_owned(),
            panel_alt: values[2].to_owned(),
            border: values[3].to_owned(),
            text: values[4].to_owned(),
            muted: values[5].to_owned(),
            accent: values[6].to_owned(),
            success: values[7].to_owned(),
            danger: values[8].to_owned(),
            overlay: values[9].to_owned(),
            terminal_foreground: values[10].to_owned(),
            terminal_background: values[11].to_owned(),
            terminal_selection: values[12].to_owned(),
        }
    }

    fn normalize_for_brightness(&mut self, brightness: ThemeBrightness) {
        let fallback = match brightness {
            ThemeBrightness::Light => Self::axssh_light(),
            ThemeBrightness::Dark => Self::axssh_dark(),
        };
        self.normalize_with_fallback(&fallback);
        self.protect_contrast(brightness, &fallback);
    }

    fn normalize_with_fallback(&mut self, fallback: &Self) {
        self.background = normalize_theme_color(&self.background, &fallback.background);
        self.panel = normalize_theme_color(&self.panel, &fallback.panel);
        self.panel_alt = normalize_theme_color(&self.panel_alt, &fallback.panel_alt);
        self.border = normalize_theme_color(&self.border, &fallback.border);
        self.text = normalize_theme_color(&self.text, &fallback.text);
        self.muted = normalize_theme_color(&self.muted, &fallback.muted);
        self.accent = normalize_theme_color(&self.accent, &fallback.accent);
        self.success = normalize_theme_color(&self.success, &fallback.success);
        self.danger = normalize_theme_color(&self.danger, &fallback.danger);
        self.overlay = normalize_theme_color(&self.overlay, &fallback.overlay);
        self.terminal_foreground =
            normalize_theme_color(&self.terminal_foreground, &fallback.terminal_foreground);
        self.terminal_background =
            normalize_theme_color(&self.terminal_background, &fallback.terminal_background);
        self.terminal_selection =
            normalize_theme_color(&self.terminal_selection, &fallback.terminal_selection);
    }

    fn protect_contrast(&mut self, brightness: ThemeBrightness, fallback: &Self) {
        let reference = match brightness {
            ThemeBrightness::Light => "#000000",
            ThemeBrightness::Dark => "#FFFFFF",
        };
        ensure_surface_direction(&mut self.background, &fallback.background, reference);
        ensure_surface_direction(&mut self.panel, &fallback.panel, reference);
        ensure_surface_direction(&mut self.panel_alt, &fallback.panel_alt, reference);

        let surfaces = [&self.background, &self.panel, &self.panel_alt];
        ensure_role_contrast(
            &mut self.border,
            &fallback.border,
            &surfaces,
            3.0,
            reference,
        );
        ensure_role_contrast(&mut self.text, &fallback.text, &surfaces, 4.5, reference);
        ensure_role_contrast(&mut self.muted, &fallback.muted, &surfaces, 4.5, reference);
        ensure_role_contrast(
            &mut self.accent,
            &fallback.accent,
            &surfaces,
            4.5,
            reference,
        );
        ensure_role_contrast(
            &mut self.success,
            &fallback.success,
            &surfaces,
            4.5,
            reference,
        );
        ensure_role_contrast(
            &mut self.danger,
            &fallback.danger,
            &surfaces,
            4.5,
            reference,
        );

        ensure_surface_direction(
            &mut self.terminal_background,
            &fallback.terminal_background,
            reference,
        );
        let terminal_surfaces = [&self.terminal_background];
        ensure_role_contrast(
            &mut self.terminal_foreground,
            &fallback.terminal_foreground,
            &terminal_surfaces,
            4.5,
            reference,
        );
        if theme_contrast_ratio(&self.terminal_selection, &self.terminal_background)
            .unwrap_or_default()
            < 1.5
            || theme_contrast_ratio(&self.terminal_foreground, &self.terminal_selection)
                .unwrap_or_default()
                < 4.5
        {
            self.terminal_selection = fallback.terminal_selection.clone();
        }
    }
}

impl Default for ThemePalette {
    fn default() -> Self {
        Self::axssh_dark()
    }
}

#[derive(Clone, Copy)]
enum ThemeBrightness {
    Light,
    Dark,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ThemeSettings {
    pub mode: ThemeMode,
    pub palette: ThemePaletteKind,
    pub custom_light: ThemePalette,
    pub custom_dark: ThemePalette,
}

impl ThemeSettings {
    pub fn normalized(
        mode: &str,
        palette: &str,
        mut custom_light: ThemePalette,
        mut custom_dark: ThemePalette,
    ) -> Self {
        custom_light.normalize_for_brightness(ThemeBrightness::Light);
        custom_dark.normalize_for_brightness(ThemeBrightness::Dark);
        Self {
            mode: ThemeMode::from_setting(mode),
            palette: ThemePaletteKind::from_setting(palette),
            custom_light,
            custom_dark,
        }
    }

    fn from_terminal_color_scheme(scheme: TerminalColorScheme) -> Self {
        let (mode, palette) = match scheme {
            TerminalColorScheme::Dark => (ThemeMode::Dark, ThemePaletteKind::AxSsh),
            TerminalColorScheme::Light => (ThemeMode::Light, ThemePaletteKind::AxSsh),
            TerminalColorScheme::SolarizedDark => (ThemeMode::Dark, ThemePaletteKind::Solarized),
        };
        Self {
            mode,
            palette,
            ..Self::default()
        }
    }

    pub fn light_palette(&self) -> ThemePalette {
        match self.palette {
            ThemePaletteKind::AxSsh => ThemePalette::axssh_light(),
            ThemePaletteKind::Solarized => ThemePalette::solarized_light(),
            ThemePaletteKind::Custom => self.custom_light.clone(),
        }
    }

    pub fn dark_palette(&self) -> ThemePalette {
        match self.palette {
            ThemePaletteKind::AxSsh => ThemePalette::axssh_dark(),
            ThemePaletteKind::Solarized => ThemePalette::solarized_dark(),
            ThemePaletteKind::Custom => self.custom_dark.clone(),
        }
    }

    const fn terminal_color_scheme(&self) -> TerminalColorScheme {
        match (self.mode, self.palette) {
            (ThemeMode::Light, _) => TerminalColorScheme::Light,
            (ThemeMode::Dark, ThemePaletteKind::Solarized) => TerminalColorScheme::SolarizedDark,
            (ThemeMode::System | ThemeMode::Dark, _) => TerminalColorScheme::Dark,
        }
    }
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            // Preserve the historical AxSSH appearance for both upgrades and
            // clean installs. Following the system remains an explicit choice.
            mode: ThemeMode::Dark,
            palette: ThemePaletteKind::AxSsh,
            custom_light: ThemePalette::axssh_light(),
            custom_dark: ThemePalette::axssh_dark(),
        }
    }
}

#[derive(Deserialize)]
struct ThemeSettingsWire {
    #[serde(default)]
    mode: String,
    #[serde(default)]
    palette: Option<ThemePaletteKind>,
    #[serde(default)]
    custom_light: Option<ThemePalette>,
    #[serde(default)]
    custom_dark: Option<ThemePalette>,
    #[serde(default)]
    custom: Option<ThemePalette>,
}

impl<'de> Deserialize<'de> for ThemeSettings {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = ThemeSettingsWire::deserialize(deserializer)?;
        let legacy_mode = wire.mode.trim().to_ascii_lowercase();
        let palette = wire
            .palette
            .unwrap_or_else(|| ThemePaletteKind::from_setting(&legacy_mode));
        let mut resolved_mode = legacy_mode.clone();
        let mut custom_light = wire.custom_light.unwrap_or_else(ThemePalette::axssh_light);
        let mut custom_dark = wire.custom_dark.unwrap_or_else(ThemePalette::axssh_dark);
        if let Some(custom) = wire.custom {
            if theme_color_is_dark(&custom.background) {
                custom_dark = custom;
                if legacy_mode == "custom" {
                    resolved_mode = "dark".to_owned();
                }
            } else {
                custom_light = custom;
                if legacy_mode == "custom" {
                    resolved_mode = "light".to_owned();
                }
            }
        }
        Ok(Self::normalized(
            &resolved_mode,
            palette.as_setting(),
            custom_light,
            custom_dark,
        ))
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
        font_family: &str,
        terminal_font_size: i32,
        terminal_line_height_percent: i32,
        color_scheme: &str,
        brightness_percent: i32,
        bright_bold_text: bool,
        right_click_copy_or_paste: bool,
    ) -> Self {
        let terminal_color_scheme = TerminalColorScheme::from_setting(color_scheme);
        Self::normalized_with_theme(
            font_family,
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
        font_family: &str,
        terminal_font_size: i32,
        terminal_line_height_percent: i32,
        terminal_color_scheme: TerminalColorScheme,
        brightness_percent: i32,
        bright_bold_text: bool,
        right_click_copy_or_paste: bool,
        theme: ThemeSettings,
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

    fn normalize_in_place(&mut self) {
        let theme = ThemeSettings::normalized(
            self.theme.mode.as_setting(),
            self.theme.palette.as_setting(),
            self.theme.custom_light.clone(),
            self.theme.custom_dark.clone(),
        );
        *self = Self::normalized_with_theme(
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
    #[serde(default = "default_session_mask_character")]
    pub session_mask_character: String,
}

impl WorkspaceSettings {
    pub fn normalized(sidebar_width: i32, tab_width: i32, session_mask_character: &str) -> Self {
        Self {
            sidebar_width: sidebar_width.clamp(MIN_SIDEBAR_WIDTH.into(), MAX_SIDEBAR_WIDTH.into())
                as u16,
            tab_width: tab_width.clamp(MIN_TAB_WIDTH.into(), MAX_TAB_WIDTH.into()) as u16,
            session_mask_character: normalize_session_mask_character(session_mask_character),
        }
    }

    fn normalize_in_place(&mut self) {
        *self = Self::normalized(
            i32::from(self.sidebar_width),
            i32::from(self.tab_width),
            &self.session_mask_character,
        );
    }
}

impl Default for WorkspaceSettings {
    fn default() -> Self {
        Self {
            sidebar_width: default_sidebar_width(),
            tab_width: default_tab_width(),
            session_mask_character: default_session_mask_character(),
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
        session_mask_character: &str,
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
            workspace: WorkspaceSettings::normalized(
                sidebar_width,
                tab_width,
                session_mask_character,
            ),
            shortcuts: ShortcutSettings::normalized(
                open_settings_shortcut,
                toggle_sidebar_shortcut,
                copy_selection_shortcut,
                paste_shortcut,
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

    fn normalize_in_place(&mut self) {
        self.appearance.normalize_in_place();
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

fn default_theme_background() -> String {
    "#171918".to_owned()
}

fn default_theme_panel() -> String {
    "#202321".to_owned()
}

fn default_theme_panel_alt() -> String {
    "#292D2A".to_owned()
}

fn default_theme_border() -> String {
    "#727C76".to_owned()
}

fn default_theme_text() -> String {
    "#EDF1EE".to_owned()
}

fn default_theme_muted() -> String {
    "#A8B2AC".to_owned()
}

fn default_theme_accent() -> String {
    "#52C7A5".to_owned()
}

fn default_theme_success() -> String {
    "#63D6A9".to_owned()
}

fn default_theme_danger() -> String {
    "#FF8E88".to_owned()
}

fn default_theme_overlay() -> String {
    "#00000099".to_owned()
}

fn default_theme_terminal_foreground() -> String {
    "#CCCCCC".to_owned()
}

fn default_theme_terminal_background() -> String {
    "#1E1E1E".to_owned()
}

fn default_theme_terminal_selection() -> String {
    "#264F78".to_owned()
}

fn normalize_theme_color(value: &str, fallback: &str) -> String {
    let value = value.trim().trim_start_matches('#');
    let valid_length = matches!(value.len(), 3 | 4 | 6 | 8);
    if !valid_length || !value.bytes().all(|digit| digit.is_ascii_hexdigit()) {
        return fallback.to_owned();
    }

    let mut normalized = String::with_capacity(9);
    normalized.push('#');
    if value.len() <= 4 {
        for digit in value.bytes() {
            let digit = (digit as char).to_ascii_uppercase();
            normalized.push(digit);
            normalized.push(digit);
        }
    } else {
        normalized.extend(
            value
                .bytes()
                .map(|digit| (digit as char).to_ascii_uppercase()),
        );
    }
    normalized
}

fn theme_color_is_dark(value: &str) -> bool {
    let normalized = normalize_theme_color(value, "#171918");
    parse_theme_rgba(&normalized)
        .map(|(red, green, blue, _)| relative_luminance(red, green, blue) < 0.18)
        .unwrap_or(true)
}

fn ensure_surface_direction(value: &mut String, fallback: &str, reference: &str) {
    let opaque = parse_theme_rgba(value)
        .map(|(_, _, _, alpha)| alpha == u8::MAX)
        .unwrap_or(false);
    if !opaque || theme_contrast_ratio(reference, value).unwrap_or_default() < 4.5 {
        *value = fallback.to_owned();
    }
}

fn ensure_role_contrast(
    value: &mut String,
    fallback: &str,
    surfaces: &[&String],
    minimum: f64,
    reference: &str,
) {
    if role_meets_contrast(value, surfaces, minimum) {
        return;
    }
    if role_meets_contrast(fallback, surfaces, minimum) {
        *value = fallback.to_owned();
    } else {
        *value = reference.to_owned();
    }
}

fn role_meets_contrast(value: &str, surfaces: &[&String], minimum: f64) -> bool {
    surfaces.iter().all(|surface| {
        theme_contrast_ratio(value, surface)
            .map(|ratio| ratio >= minimum)
            .unwrap_or(false)
    })
}

fn theme_contrast_ratio(foreground: &str, background: &str) -> Option<f64> {
    let (foreground_red, foreground_green, foreground_blue, foreground_alpha) =
        parse_theme_rgba(foreground)?;
    let (background_red, background_green, background_blue, _) = parse_theme_rgba(background)?;
    let alpha = f64::from(foreground_alpha) / 255.0;
    let composite = |foreground: u8, background: u8| {
        ((f64::from(foreground) * alpha) + (f64::from(background) * (1.0 - alpha))).round() as u8
    };
    let foreground_luminance = relative_luminance(
        composite(foreground_red, background_red),
        composite(foreground_green, background_green),
        composite(foreground_blue, background_blue),
    );
    let background_luminance =
        relative_luminance(background_red, background_green, background_blue);
    let lighter = foreground_luminance.max(background_luminance);
    let darker = foreground_luminance.min(background_luminance);
    Some((lighter + 0.05) / (darker + 0.05))
}

fn parse_theme_rgba(value: &str) -> Option<(u8, u8, u8, u8)> {
    let value = value.strip_prefix('#')?;
    if !matches!(value.len(), 6 | 8) {
        return None;
    }
    let red = u8::from_str_radix(&value[0..2], 16).ok()?;
    let green = u8::from_str_radix(&value[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&value[4..6], 16).ok()?;
    let alpha = if value.len() == 8 {
        u8::from_str_radix(&value[6..8], 16).ok()?
    } else {
        u8::MAX
    };
    Some((red, green, blue, alpha))
}

fn relative_luminance(red: u8, green: u8, blue: u8) -> f64 {
    let linear = |channel: u8| {
        let channel = f64::from(channel) / 255.0;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    (0.2126 * linear(red)) + (0.7152 * linear(green)) + (0.0722 * linear(blue))
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
    #[serde(default)]
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
        if wire.version < THEME_SETTINGS_SCHEMA_VERSION {
            settings.appearance.theme = ThemeSettings::from_terminal_color_scheme(
                settings.appearance.terminal_color_scheme,
            );
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
                theme: ThemeSettings {
                    mode: ThemeMode::Light,
                    palette: ThemePaletteKind::AxSsh,
                    custom_light: ThemePalette::axssh_light(),
                    custom_dark: ThemePalette::axssh_dark(),
                },
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
                theme: ThemeSettings::default(),
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
    fn theme_mode_normalizes_aliases_and_unknown_values() {
        assert_eq!(ThemeMode::from_setting("follow-system"), ThemeMode::System);
        assert_eq!(ThemeMode::from_setting("AUTO"), ThemeMode::System);
        assert_eq!(ThemeMode::from_setting("Solarized Dark"), ThemeMode::Dark);
        assert_eq!(ThemeMode::from_setting("unexpected"), ThemeMode::Dark);
        assert_eq!(
            ThemePaletteKind::from_setting("Solarized Dark"),
            ThemePaletteKind::Solarized
        );
    }

    #[test]
    fn persisted_theme_mode_accepts_system_aliases() {
        for alias in ["system", "follow-system", "auto"] {
            let json = format!(
                r#"{{"version":9,"settings":{{"appearance":{{"theme":{{"mode":"{alias}"}}}}}}}}"#
            );
            let store: SessionStore =
                serde_json::from_str(&json).expect("system mode alias should deserialize");

            assert_eq!(store.settings.appearance.theme.mode, ThemeMode::System);
        }
    }

    #[test]
    fn custom_palette_normalizes_each_hex_value_independently() {
        let palette = ThemePalette::normalized(
            " #abc ",
            "#abcd",
            "#1a2b3c",
            "not-a-color",
            "#12345678",
            "#0A0B0C",
            "#112233",
            "#445566",
            "#778899",
            "#AABBCCDD",
            "#010203",
            "#040506",
            "#070809",
        );

        assert_eq!(palette.background, "#AABBCC");
        assert_eq!(palette.panel, "#AABBCCDD");
        assert_eq!(palette.panel_alt, "#1A2B3C");
        assert_eq!(palette.border, default_theme_border());
        assert_eq!(palette.text, "#12345678");
        assert_eq!(palette.terminal_selection, "#070809");
    }

    #[test]
    fn version_eight_terminal_palette_migrates_to_theme_mode() {
        for (legacy_scheme, expected_mode, expected_palette) in [
            ("dark", ThemeMode::Dark, ThemePaletteKind::AxSsh),
            ("light", ThemeMode::Light, ThemePaletteKind::AxSsh),
            (
                "solarized-dark",
                ThemeMode::Dark,
                ThemePaletteKind::Solarized,
            ),
        ] {
            let json = format!(
                r#"{{"version":8,"settings":{{"appearance":{{"terminal_color_scheme":"{legacy_scheme}"}}}}}}"#
            );
            let store: SessionStore =
                serde_json::from_str(&json).expect("version eight settings should migrate");

            assert_eq!(store.version, CURRENT_SCHEMA_VERSION);
            assert_eq!(store.settings.appearance.theme.mode, expected_mode);
            assert_eq!(store.settings.appearance.theme.palette, expected_palette);
            assert_eq!(
                store.settings.appearance.terminal_color_scheme.as_setting(),
                legacy_scheme
            );
        }
    }

    #[test]
    fn version_eleven_custom_theme_round_trips_without_secrets() {
        let mut settings = AppSettings::default();
        settings.set_theme(ThemeSettings::normalized(
            "dark",
            "custom",
            ThemePalette::axssh_light(),
            ThemePalette::normalized(
                "#102030",
                "#203040",
                "#304050",
                "#405060",
                "#506070",
                "#607080",
                "#708090",
                "#8090A0",
                "#90A0B0",
                "#00000080",
                "#A0B0C0",
                "#0A0B0C",
                "#102938",
            ),
        ));
        let store = SessionStore {
            version: CURRENT_SCHEMA_VERSION,
            sessions: Vec::new(),
            settings,
        };

        let encoded = serde_json::to_string(&store).expect("custom theme should serialize");
        let decoded: SessionStore =
            serde_json::from_str(&encoded).expect("custom theme should deserialize");

        assert_eq!(decoded, store);
        assert!(encoded.contains("\"theme\""));
        assert!(!encoded.contains("password"));
        assert!(!encoded.contains("passphrase"));
    }

    #[test]
    fn version_ten_combined_theme_modes_migrate_without_changing_direction() {
        let solarized: SessionStore = serde_json::from_str(
            r##"{"version":10,"settings":{"appearance":{"theme":{"mode":"solarized-dark"}}}}"##,
        )
        .expect("legacy solarized theme should migrate");
        assert_eq!(solarized.settings.appearance.theme.mode, ThemeMode::Dark);
        assert_eq!(
            solarized.settings.appearance.theme.palette,
            ThemePaletteKind::Solarized
        );

        let custom: SessionStore = serde_json::from_str(
            r##"{"version":10,"settings":{"appearance":{"theme":{"mode":"custom","custom":{"background":"#F8F8F8","panel":"#FFFFFF","panel_alt":"#EEEEEE","border":"#555555","text":"#111111","muted":"#444444","accent":"#005F50","success":"#17633C","danger":"#982A25","overlay":"#00000099","terminal_foreground":"#222222","terminal_background":"#FFFFFF","terminal_selection":"#CDE4F8"}}}}}"##,
        )
        .expect("legacy custom theme should migrate");
        assert_eq!(custom.settings.appearance.theme.mode, ThemeMode::Light);
        assert_eq!(
            custom.settings.appearance.theme.palette,
            ThemePaletteKind::Custom
        );
        assert_eq!(
            custom.settings.appearance.theme.custom_light.background,
            "#F8F8F8"
        );
        assert_eq!(
            custom.settings.appearance.theme.custom_dark,
            ThemePalette::axssh_dark()
        );
    }

    #[test]
    fn custom_palettes_repair_invisible_surfaces_and_semantic_roles() {
        let invisible = ThemePalette::from_hex([
            "#111111",
            "#111111",
            "#111111",
            "#111111",
            "#111111",
            "#111111",
            "#111111",
            "#111111",
            "#111111",
            "#00000000",
            "#111111",
            "#111111",
            "#111111",
        ]);
        let theme =
            ThemeSettings::normalized("light", "custom", invisible, ThemePalette::axssh_dark());
        let palette = theme.light_palette();
        let surfaces = [&palette.background, &palette.panel, &palette.panel_alt];

        assert!(role_meets_contrast(&palette.text, &surfaces, 4.5));
        assert!(role_meets_contrast(&palette.muted, &surfaces, 4.5));
        assert!(role_meets_contrast(&palette.accent, &surfaces, 4.5));
        assert!(role_meets_contrast(&palette.success, &surfaces, 4.5));
        assert!(role_meets_contrast(&palette.danger, &surfaces, 4.5));
        assert!(role_meets_contrast(&palette.border, &surfaces, 3.0));
        assert!(
            theme_contrast_ratio(&palette.terminal_foreground, &palette.terminal_background)
                .is_some_and(|ratio| ratio >= 4.5)
        );
        assert!(
            theme_contrast_ratio(&palette.terminal_foreground, &palette.terminal_selection)
                .is_some_and(|ratio| ratio >= 4.5)
        );
    }

    #[test]
    fn fixed_palettes_keep_text_states_borders_and_terminal_selection_visible() {
        for palette in [
            ThemePalette::axssh_light(),
            ThemePalette::axssh_dark(),
            ThemePalette::solarized_light(),
            ThemePalette::solarized_dark(),
        ] {
            let surfaces = [&palette.background, &palette.panel, &palette.panel_alt];
            for role in [
                &palette.text,
                &palette.muted,
                &palette.accent,
                &palette.success,
                &palette.danger,
            ] {
                assert!(role_meets_contrast(role, &surfaces, 4.5));
            }
            assert!(role_meets_contrast(&palette.border, &surfaces, 3.0));
            assert!(
                theme_contrast_ratio(&palette.terminal_foreground, &palette.terminal_background)
                    .is_some_and(|ratio| ratio >= 4.5)
            );
            assert!(
                theme_contrast_ratio(&palette.terminal_foreground, &palette.terminal_selection)
                    .is_some_and(|ratio| ratio >= 4.5)
            );
            assert!(
                theme_contrast_ratio(&palette.terminal_selection, &palette.terminal_background)
                    .is_some_and(|ratio| ratio >= 1.5)
            );
        }
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
            "  #  ",
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
        assert_eq!(settings.workspace.session_mask_character, "#");
        assert_eq!(settings.shortcuts.open_settings, "Ctrl+,");
    }

    #[test]
    fn workspace_mask_character_is_one_visible_character() {
        assert_eq!(
            WorkspaceSettings::normalized(220, 172, "#").session_mask_character,
            "#"
        );
        assert_eq!(
            WorkspaceSettings::normalized(220, 172, "  $  ").session_mask_character,
            "$"
        );
        assert_eq!(
            WorkspaceSettings::normalized(220, 172, "").session_mask_character,
            DEFAULT_SESSION_MASK_CHARACTER
        );
        assert_eq!(
            WorkspaceSettings::normalized(220, 172, "**").session_mask_character,
            DEFAULT_SESSION_MASK_CHARACTER
        );
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
