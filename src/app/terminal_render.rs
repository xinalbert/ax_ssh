//! UI-independent terminal palette resolution.

use std::ops::Range;

use ax_ssh::config::TerminalColorScheme;
#[cfg(test)]
use ax_ssh::terminal::TerminalSnapshot;
use ax_ssh::terminal::{
    TerminalColor, TerminalQueryColor, TerminalQueryPalette, TerminalStyle, TerminalStyledLine,
    TerminalStyledRun, TerminalUnderlineStyle,
};

mod highlighting;

use self::highlighting::HighlightSpan;

const MAX_SEMANTIC_HIGHLIGHT_BYTES: usize = 512;
const MIN_TEXT_BRIGHTNESS: f64 = 0.60;
const MAX_TEXT_BRIGHTNESS: f64 = 1.20;
const DIM_TEXT_BRIGHTNESS_FACTOR: f64 = 0.70;

pub(super) fn prepare_semantic_highlighting() {
    highlighting::prepare();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RgbColor {
    pub(super) red: u8,
    pub(super) green: u8,
    pub(super) blue: u8,
}

impl RgbColor {
    pub(super) const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SemanticHighlight {
    Success,
    Info,
    Warning,
    Error,
}

struct SemanticPalette {
    success: RgbColor,
    info: RgbColor,
    warning: RgbColor,
    error: RgbColor,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SemanticColorOverrides {
    pub(super) success: Option<RgbColor>,
    pub(super) info: Option<RgbColor>,
    pub(super) warning: Option<RgbColor>,
    pub(super) error: Option<RgbColor>,
}

impl SemanticPalette {
    fn for_terminal(palette: &TerminalPalette, overrides: SemanticColorOverrides) -> Self {
        Self {
            success: overrides.success.unwrap_or(palette.ansi[10]),
            info: overrides.info.unwrap_or(palette.ansi[12]),
            warning: overrides.warning.unwrap_or(palette.ansi[11]),
            error: overrides.error.unwrap_or(palette.ansi[9]),
        }
    }

    fn color_for(&self, highlight: SemanticHighlight) -> RgbColor {
        match highlight {
            SemanticHighlight::Success => self.success,
            SemanticHighlight::Info => self.info,
            SemanticHighlight::Warning => self.warning,
            SemanticHighlight::Error => self.error,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct TerminalRenderSettings {
    pub(super) color_scheme: TerminalColorScheme,
    pub(super) default_foreground: RgbColor,
    pub(super) default_background: RgbColor,
    pub(super) selection_background: RgbColor,
    pub(super) text_brightness: f64,
    pub(super) bright_bold_text: bool,
    pub(super) semantic_highlighting: bool,
    pub(super) semantic_colors: SemanticColorOverrides,
}

#[cfg(test)]
pub(super) struct RenderedTerminal {
    pub(super) lines: Vec<RenderedTerminalLine>,
    pub(super) foreground: RgbColor,
    pub(super) background: RgbColor,
    pub(super) selection_background: RgbColor,
}

pub(super) struct RenderedTerminalLine {
    pub(super) source_revision: u64,
    pub(super) render_cache_key: u64,
    pub(super) backgrounds: Vec<RenderedTerminalBackgroundRun>,
    pub(super) decorations: Vec<RenderedTerminalDecorationRun>,
    pub(super) runs: Vec<RenderedTerminalRun>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RenderedTerminalBackgroundRun {
    pub(super) column: usize,
    pub(super) cells: usize,
    pub(super) background: RgbColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RenderedTerminalDecorationRun {
    pub(super) column: usize,
    pub(super) cells: usize,
    pub(super) foreground: RgbColor,
    pub(super) underline_style: TerminalUnderlineStyle,
    pub(super) strikethrough: bool,
}

pub(super) struct RenderedTerminalRun {
    pub(super) text: String,
    pub(super) hyperlink: Option<String>,
    pub(super) column: usize,
    pub(super) cells: usize,
    pub(super) foreground: RgbColor,
    pub(super) background: RgbColor,
    pub(super) decoration_foreground: RgbColor,
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) underline: bool,
    pub(super) underline_style: TerminalUnderlineStyle,
    pub(super) strikethrough: bool,
}

pub(super) struct TerminalRenderer {
    palette: TerminalPalette,
    settings: TerminalRenderSettings,
    semantic_palette: Option<SemanticPalette>,
    cache_key: u64,
}

impl TerminalRenderer {
    pub(super) fn new(settings: TerminalRenderSettings) -> Self {
        Self::with_terminal_defaults(settings, TerminalColor::Default, TerminalColor::Default)
    }

    pub(super) fn with_terminal_defaults(
        settings: TerminalRenderSettings,
        foreground: TerminalColor,
        background: TerminalColor,
    ) -> Self {
        let mut palette = TerminalPalette::for_scheme(settings.color_scheme);
        palette.foreground = settings.default_foreground;
        palette.background = settings.default_background;
        palette.selection_background = settings.selection_background;
        palette.foreground = resolve_color(foreground, palette.foreground, &palette);
        palette.background = resolve_color(background, palette.background, &palette);
        let semantic_palette = settings
            .semantic_highlighting
            .then(|| SemanticPalette::for_terminal(&palette, settings.semantic_colors));
        let cache_key = terminal_render_cache_key(settings, foreground, background);
        Self {
            palette,
            settings,
            semantic_palette,
            cache_key,
        }
    }

    pub(super) fn render_line(&self, line: &TerminalStyledLine) -> RenderedTerminalLine {
        render_line(
            line,
            &self.palette,
            &self.settings,
            self.semantic_palette.as_ref(),
            self.cache_key,
        )
    }

    pub(super) fn cache_key(&self) -> u64 {
        self.cache_key
    }

    pub(super) fn foreground(&self) -> RgbColor {
        self.palette.foreground
    }

    pub(super) fn background(&self) -> RgbColor {
        self.palette.background
    }

    pub(super) fn selection_background(&self) -> RgbColor {
        self.palette.selection_background
    }

    pub(super) fn cursor_color(&self, color: TerminalColor) -> RgbColor {
        resolve_color(color, self.palette.foreground, &self.palette)
    }

    pub(super) fn query_palette(&self) -> TerminalQueryPalette {
        let color = |color: RgbColor| TerminalQueryColor {
            red: color.red,
            green: color.green,
            blue: color.blue,
        };
        TerminalQueryPalette {
            ansi: self.palette.ansi.map(color),
            foreground: color(self.palette.foreground),
            background: color(self.palette.background),
            cursor: color(self.palette.foreground),
        }
    }
}

#[cfg(test)]
pub(super) fn render_terminal(
    snapshot: TerminalSnapshot,
    settings: TerminalRenderSettings,
) -> RenderedTerminal {
    let renderer = TerminalRenderer::with_terminal_defaults(
        settings,
        snapshot.foreground_color,
        snapshot.background_color,
    );
    let lines = snapshot
        .lines
        .iter()
        .map(|line| renderer.render_line(line))
        .collect();
    RenderedTerminal {
        lines,
        foreground: renderer.foreground(),
        background: renderer.background(),
        selection_background: renderer.selection_background(),
    }
}

fn render_line(
    line: &TerminalStyledLine,
    palette: &TerminalPalette,
    settings: &TerminalRenderSettings,
    semantic_palette: Option<&SemanticPalette>,
    render_cache_key: u64,
) -> RenderedTerminalLine {
    let runs = line
        .runs
        .iter()
        .flat_map(|run| render_run(run, palette, settings, semantic_palette))
        .collect::<Vec<_>>();
    let backgrounds = merged_background_runs(&runs, palette.background);
    let decorations = compact_decoration_runs(&runs);
    RenderedTerminalLine {
        source_revision: line.revision,
        render_cache_key,
        backgrounds,
        decorations,
        runs,
    }
}

fn compact_decoration_runs(runs: &[RenderedTerminalRun]) -> Vec<RenderedTerminalDecorationRun> {
    let mut decorations: Vec<RenderedTerminalDecorationRun> = Vec::new();
    for run in runs.iter().filter(|run| run.cells > 0) {
        if run.underline_style != TerminalUnderlineStyle::None {
            push_decoration_run(&mut decorations, run, run.underline_style, false);
        }
    }
    for run in runs.iter().filter(|run| run.cells > 0 && run.strikethrough) {
        push_decoration_run(&mut decorations, run, TerminalUnderlineStyle::None, true);
    }
    decorations
}

fn push_decoration_run(
    decorations: &mut Vec<RenderedTerminalDecorationRun>,
    run: &RenderedTerminalRun,
    underline_style: TerminalUnderlineStyle,
    strikethrough: bool,
) {
    if let Some(previous) = decorations.last_mut()
        && previous.strikethrough == strikethrough
        && previous.underline_style == underline_style
        && previous.foreground == run.decoration_foreground
        && previous.column.saturating_add(previous.cells) == run.column
    {
        previous.cells = previous.cells.saturating_add(run.cells);
        return;
    }
    decorations.push(RenderedTerminalDecorationRun {
        column: run.column,
        cells: run.cells,
        foreground: run.decoration_foreground,
        underline_style,
        strikethrough,
    });
}

fn merged_background_runs(
    runs: &[RenderedTerminalRun],
    default_background: RgbColor,
) -> Vec<RenderedTerminalBackgroundRun> {
    let mut backgrounds: Vec<RenderedTerminalBackgroundRun> = Vec::new();
    for run in runs
        .iter()
        .filter(|run| run.cells > 0 && run.background != default_background)
    {
        if let Some(previous) = backgrounds.last_mut()
            && previous.background == run.background
            && previous.column.saturating_add(previous.cells) == run.column
        {
            previous.cells = previous.cells.saturating_add(run.cells);
            continue;
        }
        backgrounds.push(RenderedTerminalBackgroundRun {
            column: run.column,
            cells: run.cells,
            background: run.background,
        });
    }
    backgrounds
}

fn render_run(
    run: &TerminalStyledRun,
    palette: &TerminalPalette,
    settings: &TerminalRenderSettings,
    semantic_palette: Option<&SemanticPalette>,
) -> Vec<RenderedTerminalRun> {
    let text = run.text.clone();
    let column = run.column;
    let cells = run.cells;
    let style = run.style;
    let highlights = semantic_palette
        .map(|_| semantic_highlight_spans(&text, cells, style))
        .unwrap_or_default();
    let (foreground, background) = resolve_style_colors(style, palette, settings);
    let hyperlink = run.hyperlink.clone();
    let hyperlink_underline = hyperlink.is_some()
        && style.underline_style == TerminalUnderlineStyle::None
        && !style.underline;
    let decoration_foreground = style
        .underline_color
        .map(|color| resolve_color(color, foreground, palette))
        .unwrap_or(foreground);
    let rendered = RenderedTerminalRun {
        text,
        hyperlink,
        column,
        cells,
        foreground,
        background,
        decoration_foreground,
        bold: style.bold,
        italic: style.italic,
        underline: style.underline || hyperlink_underline,
        underline_style: if hyperlink_underline {
            TerminalUnderlineStyle::Single
        } else {
            style.underline_style
        },
        strikethrough: style.strikethrough,
    };
    let mut rendered_runs =
        if let Some(semantic_palette) = semantic_palette.filter(|_| !highlights.is_empty()) {
            split_semantic_run(rendered, highlights, semantic_palette)
        } else {
            vec![rendered]
        };
    for rendered_run in &mut rendered_runs {
        rendered_run.foreground =
            adjust_text_foreground(rendered_run.foreground, settings.text_brightness, style.dim);
        rendered_run.decoration_foreground = adjust_text_foreground(
            rendered_run.decoration_foreground,
            settings.text_brightness,
            style.dim,
        );
        if style.hidden {
            rendered_run.foreground = rendered_run.background;
            rendered_run.decoration_foreground = rendered_run.background;
        }
    }
    rendered_runs
}

fn terminal_render_cache_key(
    settings: TerminalRenderSettings,
    foreground: TerminalColor,
    background: TerminalColor,
) -> u64 {
    let mut hash = 14_695_981_039_346_656_037_u64;
    let mut mix = |byte: u8| {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(1_099_511_628_211);
    };
    for byte in settings.color_scheme.as_setting().bytes() {
        mix(byte);
    }
    for color in [
        settings.default_foreground,
        settings.default_background,
        settings.selection_background,
    ] {
        mix(color.red);
        mix(color.green);
        mix(color.blue);
    }
    for color in [foreground, background] {
        match color {
            TerminalColor::Default => mix(0),
            TerminalColor::Indexed(index) => {
                mix(1);
                mix(index);
            }
            TerminalColor::Rgb { red, green, blue } => {
                mix(2);
                mix(red);
                mix(green);
                mix(blue);
            }
        }
    }
    for byte in settings.text_brightness.to_bits().to_le_bytes() {
        mix(byte);
    }
    mix(u8::from(settings.bright_bold_text));
    mix(u8::from(settings.semantic_highlighting));
    for color in [
        settings.semantic_colors.success,
        settings.semantic_colors.info,
        settings.semantic_colors.warning,
        settings.semantic_colors.error,
    ] {
        match color {
            Some(color) => {
                mix(1);
                mix(color.red);
                mix(color.green);
                mix(color.blue);
            }
            None => mix(0),
        }
    }
    hash
}

fn semantic_highlight_spans(text: &str, cells: usize, style: TerminalStyle) -> Vec<HighlightSpan> {
    // An ASCII run with one terminal cell per byte can be split without
    // changing its cell geometry. Wide and combining text keeps its original
    // terminal styling; Cmd/Ctrl target feedback remains available for it.
    if !text.is_ascii()
        || text.is_empty()
        || text.len() > MAX_SEMANTIC_HIGHLIGHT_BYTES
        || cells != text.len()
        || style.foreground != TerminalColor::Default
        || style.background != TerminalColor::Default
        || style.inverse
        || style.dim
        || style.hidden
    {
        return Vec::new();
    }

    highlighting::spans(text)
}

fn split_semantic_run(
    run: RenderedTerminalRun,
    highlights: Vec<HighlightSpan>,
    palette: &SemanticPalette,
) -> Vec<RenderedTerminalRun> {
    let slice = |range: Range<usize>, highlight: Option<SemanticHighlight>| RenderedTerminalRun {
        text: run.text[range.clone()].to_owned(),
        hyperlink: run.hyperlink.clone(),
        column: run.column + range.start,
        cells: range.len(),
        foreground: highlight.map_or(run.foreground, |value| palette.color_for(value)),
        background: run.background,
        decoration_foreground: run.decoration_foreground,
        bold: run.bold,
        italic: run.italic,
        underline: run.underline,
        underline_style: run.underline_style,
        strikethrough: run.strikethrough,
    };
    let mut runs = Vec::with_capacity(highlights.len() * 2 + 1);
    let mut start = 0;
    // Regex captures are ordered, disjoint byte ranges; eligibility above
    // guarantees that each byte is one terminal cell.
    for span in highlights {
        if start < span.range.start {
            runs.push(slice(start..span.range.start, None));
        }
        let end = span.range.end;
        runs.push(slice(span.range, Some(span.highlight)));
        start = end;
    }
    if start < run.text.len() {
        runs.push(slice(start..run.text.len(), None));
    }
    runs
}

fn resolve_style_colors(
    style: TerminalStyle,
    palette: &TerminalPalette,
    settings: &TerminalRenderSettings,
) -> (RgbColor, RgbColor) {
    let foreground_color = if settings.bright_bold_text && style.bold {
        match style.foreground {
            TerminalColor::Indexed(index @ 0..=7) => TerminalColor::Indexed(index + 8),
            color => color,
        }
    } else {
        style.foreground
    };
    let mut foreground = resolve_color(foreground_color, palette.foreground, palette);
    let mut background = resolve_color(style.background, palette.background, palette);
    if style.inverse {
        std::mem::swap(&mut foreground, &mut background);
    }
    (foreground, background)
}

fn resolve_color(color: TerminalColor, default: RgbColor, palette: &TerminalPalette) -> RgbColor {
    match color {
        TerminalColor::Default => default,
        TerminalColor::Indexed(index) => indexed_color(index, palette),
        TerminalColor::Rgb { red, green, blue } => RgbColor::new(red, green, blue),
    }
}

fn indexed_color(index: u8, palette: &TerminalPalette) -> RgbColor {
    match index {
        0..=15 => palette.ansi[usize::from(index)],
        16..=231 => {
            const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
            let value = index - 16;
            RgbColor::new(
                LEVELS[usize::from(value / 36)],
                LEVELS[usize::from((value % 36) / 6)],
                LEVELS[usize::from(value % 6)],
            )
        }
        232..=255 => {
            let gray = 8 + (index - 232) * 10;
            RgbColor::new(gray, gray, gray)
        }
    }
}

fn adjust_text_foreground(color: RgbColor, brightness: f64, dim: bool) -> RgbColor {
    let brightness = if brightness.is_finite() {
        brightness.clamp(MIN_TEXT_BRIGHTNESS, MAX_TEXT_BRIGHTNESS)
    } else {
        1.0
    };
    let factor = if dim {
        brightness * DIM_TEXT_BRIGHTNESS_FACTOR
    } else {
        brightness
    };
    if factor == 1.0 {
        return color;
    }

    let red = f64::from(color.red) / 255.0;
    let green = f64::from(color.green) / 255.0;
    let blue = f64::from(color.blue) / 255.0;
    let maximum = red.max(green).max(blue);
    let minimum = red.min(green).min(blue);
    let lightness = (maximum + minimum) / 2.0;
    let delta = maximum - minimum;
    let (hue, saturation) = if delta == 0.0 {
        (0.0, 0.0)
    } else {
        let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
        let hue = if maximum == red {
            ((green - blue) / delta).rem_euclid(6.0)
        } else if maximum == green {
            (blue - red) / delta + 2.0
        } else {
            (red - green) / delta + 4.0
        } / 6.0;
        (hue, saturation)
    };
    hsl_to_rgb(hue, saturation, (lightness * factor).clamp(0.0, 1.0))
}

fn hsl_to_rgb(hue: f64, saturation: f64, lightness: f64) -> RgbColor {
    let channel = |offset: f64| {
        let value = if saturation == 0.0 {
            lightness
        } else {
            let q = if lightness < 0.5 {
                lightness * (1.0 + saturation)
            } else {
                lightness + saturation - lightness * saturation
            };
            let p = 2.0 * lightness - q;
            hue_channel(p, q, hue + offset)
        };
        (value * 255.0).round().clamp(0.0, 255.0) as u8
    };
    RgbColor::new(channel(1.0 / 3.0), channel(0.0), channel(-1.0 / 3.0))
}

fn hue_channel(p: f64, q: f64, hue: f64) -> f64 {
    let hue = hue.rem_euclid(1.0);
    if hue < 1.0 / 6.0 {
        p + (q - p) * 6.0 * hue
    } else if hue < 0.5 {
        q
    } else if hue < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - hue) * 6.0
    } else {
        p
    }
}

struct TerminalPalette {
    foreground: RgbColor,
    background: RgbColor,
    selection_background: RgbColor,
    ansi: [RgbColor; 16],
}

// Keep dark terminal ANSI colors aligned with axshell while allowing each
// AxSSH theme to retain its own default foreground, background, and selection.
const AXSHELL_DARK_ANSI: [RgbColor; 16] = [
    RgbColor::new(31, 36, 48),
    RgbColor::new(255, 92, 87),
    RgbColor::new(90, 247, 142),
    RgbColor::new(243, 249, 157),
    RgbColor::new(87, 199, 255),
    RgbColor::new(255, 106, 193),
    RgbColor::new(154, 237, 254),
    RgbColor::new(241, 241, 240),
    RgbColor::new(104, 104, 104),
    RgbColor::new(255, 92, 87),
    RgbColor::new(90, 247, 142),
    RgbColor::new(243, 249, 157),
    RgbColor::new(87, 199, 255),
    RgbColor::new(255, 106, 193),
    RgbColor::new(154, 237, 254),
    RgbColor::new(255, 255, 255),
];

impl TerminalPalette {
    fn for_scheme(scheme: TerminalColorScheme) -> Self {
        match scheme {
            TerminalColorScheme::Dark => Self {
                foreground: RgbColor::new(204, 204, 204),
                background: RgbColor::new(30, 30, 30),
                selection_background: RgbColor::new(38, 79, 120),
                ansi: AXSHELL_DARK_ANSI,
            },
            TerminalColorScheme::Light => Self {
                foreground: RgbColor::new(51, 51, 51),
                background: RgbColor::new(255, 255, 255),
                selection_background: RgbColor::new(173, 214, 255),
                ansi: [
                    RgbColor::new(0, 0, 0),
                    RgbColor::new(205, 49, 49),
                    RgbColor::new(0, 128, 0),
                    RgbColor::new(148, 108, 0),
                    RgbColor::new(4, 81, 165),
                    RgbColor::new(175, 0, 219),
                    RgbColor::new(5, 139, 164),
                    RgbColor::new(85, 85, 85),
                    RgbColor::new(102, 102, 102),
                    RgbColor::new(241, 76, 76),
                    RgbColor::new(20, 164, 20),
                    RgbColor::new(181, 137, 0),
                    RgbColor::new(0, 122, 204),
                    RgbColor::new(188, 63, 188),
                    RgbColor::new(49, 154, 188),
                    RgbColor::new(0, 0, 0),
                ],
            },
            TerminalColorScheme::SolarizedDark => Self {
                foreground: RgbColor::new(131, 148, 150),
                background: RgbColor::new(0, 43, 54),
                selection_background: RgbColor::new(7, 54, 66),
                ansi: AXSHELL_DARK_ANSI,
            },
            TerminalColorScheme::ArcticDark => Self {
                foreground: RgbColor::new(213, 226, 232),
                background: RgbColor::new(17, 28, 37),
                selection_background: RgbColor::new(39, 86, 107),
                ansi: AXSHELL_DARK_ANSI,
            },
            TerminalColorScheme::TokyoDark => Self {
                foreground: RgbColor::new(200, 211, 245),
                background: RgbColor::new(16, 19, 35),
                selection_background: RgbColor::new(51, 70, 124),
                ansi: AXSHELL_DARK_ANSI,
            },
            TerminalColorScheme::EmberDark => Self {
                foreground: RgbColor::new(231, 214, 207),
                background: RgbColor::new(26, 18, 16),
                selection_background: RgbColor::new(112, 65, 45),
                ansi: AXSHELL_DARK_ANSI,
            },
            TerminalColorScheme::ForestDark => Self {
                foreground: RgbColor::new(209, 230, 214),
                background: RgbColor::new(14, 25, 18),
                selection_background: RgbColor::new(40, 94, 59),
                ansi: AXSHELL_DARK_ANSI,
            },
        }
    }
}

#[cfg(test)]
mod tests;
