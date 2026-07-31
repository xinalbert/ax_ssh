use serde::{Deserialize, Serialize};

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

    pub(super) fn from_hex(values: [&str; 13]) -> Self {
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

    pub(super) fn from_terminal_color_scheme(scheme: TerminalColorScheme) -> Self {
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

    pub(super) const fn terminal_color_scheme(&self) -> TerminalColorScheme {
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

fn default_theme_background() -> String {
    "#171918".to_owned()
}

fn default_theme_panel() -> String {
    "#202321".to_owned()
}

fn default_theme_panel_alt() -> String {
    "#292D2A".to_owned()
}

pub(super) fn default_theme_border() -> String {
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

pub(super) fn role_meets_contrast(value: &str, surfaces: &[&String], minimum: f64) -> bool {
    surfaces.iter().all(|surface| {
        theme_contrast_ratio(value, surface)
            .map(|ratio| ratio >= minimum)
            .unwrap_or(false)
    })
}

pub(super) fn theme_contrast_ratio(foreground: &str, background: &str) -> Option<f64> {
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
