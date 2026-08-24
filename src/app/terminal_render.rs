//! UI-independent terminal palette resolution.

use ax_ssh::config::TerminalColorScheme;
#[cfg(test)]
use ax_ssh::terminal::TerminalSnapshot;
use ax_ssh::terminal::{TerminalColor, TerminalStyle, TerminalStyledLine, TerminalStyledRun};

const MAX_SEMANTIC_HIGHLIGHT_CHARS: usize = 512;
const MIN_TEXT_BRIGHTNESS: f64 = 0.60;
const MAX_TEXT_BRIGHTNESS: f64 = 1.20;
const DIM_TEXT_BRIGHTNESS_FACTOR: f64 = 0.70;
const ERROR_KEYWORDS: &[&[u8]] = &[
    b"error",
    b"err",
    b"fatal",
    b"panic",
    b"fail",
    b"failed",
    b"failure",
    b"exception",
    b"traceback",
    b"critical",
    b"crash",
    b"crashed",
    b"sigsegv",
];
const WARNING_KEYWORDS: &[&[u8]] = &[
    b"warn",
    b"warning",
    b"deprecated",
    b"todo",
    b"fixme",
    b"timeout",
    b"refused",
    b"denied",
    b"rejected",
    b"unreachable",
    b"offline",
    b"pending",
    b"waiting",
    b"processing",
];
const SUCCESS_KEYWORDS: &[&[u8]] = &[
    b"ok",
    b"success",
    b"pass",
    b"passed",
    b"done",
    b"completed",
    b"ready",
    b"connected",
    b"online",
    b"up",
    b"running",
    b"deployed",
    b"authenticated",
    b"authorized",
];
const INFO_KEYWORDS: &[&[u8]] = &[
    b"info",
    b"notice",
    b"debug",
    b"trace",
    b"dbg",
    b"ssh",
    b"ssl",
    b"tls",
    b"certificate",
    b"auth",
    b"login",
    b"start",
    b"started",
    b"starting",
    b"boot",
    b"restart",
    b"restarting",
    b"deploy",
    b"deploying",
    b"active",
    b"executing",
];

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
    pub(super) strikethrough: bool,
}

pub(super) struct RenderedTerminalRun {
    pub(super) text: String,
    pub(super) column: usize,
    pub(super) cells: usize,
    pub(super) foreground: RgbColor,
    pub(super) background: RgbColor,
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) underline: bool,
    pub(super) strikethrough: bool,
    pub(super) centered: bool,
}

pub(super) struct TerminalRenderer {
    palette: TerminalPalette,
    settings: TerminalRenderSettings,
    semantic_palette: Option<SemanticPalette>,
    cache_key: u64,
}

impl TerminalRenderer {
    pub(super) fn new(settings: TerminalRenderSettings) -> Self {
        let mut palette = TerminalPalette::for_scheme(settings.color_scheme);
        palette.foreground = settings.default_foreground;
        palette.background = settings.default_background;
        palette.selection_background = settings.selection_background;
        let semantic_palette = settings
            .semantic_highlighting
            .then(|| SemanticPalette::for_terminal(&palette, settings.semantic_colors));
        let cache_key = terminal_render_cache_key(settings);
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
}

#[cfg(test)]
pub(super) fn render_terminal(
    snapshot: TerminalSnapshot,
    settings: TerminalRenderSettings,
) -> RenderedTerminal {
    let renderer = TerminalRenderer::new(settings);
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
    let backgrounds = compact_background_runs(&runs, palette.background);
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
    for strikethrough in [false, true] {
        for run in runs.iter().filter(|run| {
            run.cells > 0
                && if strikethrough {
                    run.strikethrough
                } else {
                    run.underline
                }
        }) {
            if let Some(previous) = decorations.last_mut()
                && previous.strikethrough == strikethrough
                && previous.foreground == run.foreground
                && previous.column.saturating_add(previous.cells) == run.column
            {
                previous.cells = previous.cells.saturating_add(run.cells);
                continue;
            }
            decorations.push(RenderedTerminalDecorationRun {
                column: run.column,
                cells: run.cells,
                foreground: run.foreground,
                strikethrough,
            });
        }
    }
    decorations
}

fn compact_background_runs(
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
    let centered = !text.is_ascii();
    let highlights = semantic_palette.and_then(|_| semantic_highlights(&text, cells, style));
    let (foreground, background) = resolve_style_colors(style, palette, settings);
    let rendered = RenderedTerminalRun {
        text,
        column,
        cells,
        foreground,
        background,
        bold: style.bold,
        italic: style.italic,
        underline: style.underline,
        strikethrough: style.strikethrough,
        centered,
    };
    let mut rendered_runs =
        if let (Some(highlights), Some(semantic_palette)) = (highlights, semantic_palette) {
            split_semantic_run(rendered, highlights, semantic_palette)
        } else {
            vec![rendered]
        };
    for rendered_run in &mut rendered_runs {
        rendered_run.foreground =
            adjust_text_foreground(rendered_run.foreground, settings.text_brightness, style.dim);
    }
    rendered_runs
}

fn terminal_render_cache_key(settings: TerminalRenderSettings) -> u64 {
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

fn semantic_highlights(
    text: &str,
    cells: usize,
    style: TerminalStyle,
) -> Option<Vec<Option<SemanticHighlight>>> {
    // An ASCII run with one terminal cell per byte can be split without
    // changing its cell geometry. Wide and combining text keeps its original
    // terminal styling; Cmd/Ctrl target feedback remains available for it.
    if !text.is_ascii()
        || text.is_empty()
        || text.len() > MAX_SEMANTIC_HIGHLIGHT_CHARS
        || cells != text.len()
        || style.foreground != TerminalColor::Default
        || style.background != TerminalColor::Default
        || style.inverse
        || style.dim
    {
        return None;
    }

    let mut highlights = vec![None; text.len()];
    highlight_http_statuses(text, &mut highlights);
    highlight_keyword_tokens(text, &mut highlights);
    highlight_phrase(
        text,
        &mut highlights,
        SemanticHighlight::Warning,
        b"timed out",
    );
    highlights.iter().any(Option::is_some).then_some(highlights)
}

fn highlight_http_statuses(text: &str, highlights: &mut [Option<SemanticHighlight>]) {
    let bytes = text.as_bytes();
    for start in 0..bytes.len().saturating_sub(2) {
        let end = start + 3;
        if !bytes[start..end].iter().all(u8::is_ascii_digit)
            || (start > 0 && bytes[start - 1].is_ascii_digit())
            || (end < bytes.len() && bytes[end].is_ascii_digit())
        {
            continue;
        }
        let status = (u16::from(bytes[start] - b'0') * 100)
            + (u16::from(bytes[start + 1] - b'0') * 10)
            + u16::from(bytes[start + 2] - b'0');
        let highlight = match status {
            200..=299 => SemanticHighlight::Success,
            300..=399 => SemanticHighlight::Info,
            400..=499 => SemanticHighlight::Warning,
            500..=599 => SemanticHighlight::Error,
            _ => continue,
        };
        mark_highlight(highlights, start, end, highlight);
    }
}

fn highlight_keyword_tokens(text: &str, highlights: &mut [Option<SemanticHighlight>]) {
    let bytes = text.as_bytes();
    let mut start = 0usize;
    while start < bytes.len() {
        while start < bytes.len() && !is_semantic_token_byte(bytes[start]) {
            start += 1;
        }
        let mut end = start;
        while end < bytes.len() && is_semantic_token_byte(bytes[end]) {
            end += 1;
        }
        if start == end {
            break;
        }
        let token = &bytes[start..end];
        let highlight = if keyword_matches(token, ERROR_KEYWORDS) {
            Some(SemanticHighlight::Error)
        } else if keyword_matches(token, WARNING_KEYWORDS) {
            Some(SemanticHighlight::Warning)
        } else if keyword_matches(token, SUCCESS_KEYWORDS) {
            Some(SemanticHighlight::Success)
        } else if keyword_matches(token, INFO_KEYWORDS) {
            Some(SemanticHighlight::Info)
        } else {
            None
        };
        if let Some(highlight) = highlight {
            mark_highlight(highlights, start, end, highlight);
        }
        start = end;
    }
}

fn keyword_matches(token: &[u8], keywords: &[&[u8]]) -> bool {
    keywords
        .iter()
        .any(|keyword| token.eq_ignore_ascii_case(keyword))
}

fn highlight_phrase(
    text: &str,
    highlights: &mut [Option<SemanticHighlight>],
    highlight: SemanticHighlight,
    phrase: &[u8],
) {
    let bytes = text.as_bytes();
    if phrase.len() > bytes.len() {
        return;
    }
    for start in 0..=bytes.len() - phrase.len() {
        let end = start + phrase.len();
        if bytes[start..end].eq_ignore_ascii_case(phrase)
            && semantic_token_boundaries(bytes, start, end)
        {
            mark_highlight(highlights, start, end, highlight);
        }
    }
}

fn semantic_token_boundaries(text: &[u8], start: usize, end: usize) -> bool {
    (start == 0 || !is_semantic_token_byte(text[start - 1]))
        && (end == text.len() || !is_semantic_token_byte(text[end]))
}

fn is_semantic_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn mark_highlight(
    highlights: &mut [Option<SemanticHighlight>],
    start: usize,
    end: usize,
    highlight: SemanticHighlight,
) {
    if start >= end || end > highlights.len() {
        return;
    }
    for cell in &mut highlights[start..end] {
        if cell.is_none() {
            *cell = Some(highlight);
        }
    }
}

fn split_semantic_run(
    run: RenderedTerminalRun,
    highlights: Vec<Option<SemanticHighlight>>,
    palette: &SemanticPalette,
) -> Vec<RenderedTerminalRun> {
    let mut runs = Vec::new();
    let mut start = 0;
    while start < run.text.len() {
        let highlight = highlights[start];
        let mut end = start + 1;
        while end < run.text.len() && highlights[end] == highlight {
            end += 1;
        }
        runs.push(RenderedTerminalRun {
            text: run.text[start..end].to_owned(),
            column: run.column + start,
            cells: end - start,
            foreground: highlight.map_or(run.foreground, |value| palette.color_for(value)),
            background: run.background,
            bold: run.bold,
            italic: run.italic,
            underline: run.underline,
            strikethrough: run.strikethrough,
            centered: run.centered,
        });
        start = end;
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
mod tests {
    use std::sync::Arc;

    use super::*;

    fn snapshot(style: TerminalStyle) -> TerminalSnapshot {
        TerminalSnapshot {
            lines: vec![Arc::new(TerminalStyledLine {
                revision: 1,
                runs: vec![TerminalStyledRun {
                    text: "x".into(),
                    column: 0,
                    cells: 1,
                    style,
                }],
            })],
            max_columns: 1,
            cursor_row: 0,
            cursor_column: 1,
            cursor_cells: 1,
            cursor_visible: true,
            cursor_text: " ".into(),
            display_offset: 0,
            viewport_mode: ax_ssh::terminal::TerminalViewportMode::Follow,
            mouse_reporting: Default::default(),
            mouse_button_reporting_active: false,
            mouse_wheel_reporting_active: false,
        }
    }

    fn settings() -> TerminalRenderSettings {
        TerminalRenderSettings {
            color_scheme: TerminalColorScheme::Dark,
            default_foreground: RgbColor::new(204, 204, 204),
            default_background: RgbColor::new(30, 30, 30),
            selection_background: RgbColor::new(38, 79, 120),
            text_brightness: 1.0,
            bright_bold_text: true,
            semantic_highlighting: false,
            semantic_colors: SemanticColorOverrides::default(),
        }
    }

    fn semantic_settings() -> TerminalRenderSettings {
        TerminalRenderSettings {
            semantic_highlighting: true,
            ..settings()
        }
    }

    fn snapshot_line(runs: Vec<TerminalStyledRun>) -> TerminalSnapshot {
        TerminalSnapshot {
            lines: vec![Arc::new(TerminalStyledLine { revision: 1, runs })],
            max_columns: 128,
            cursor_row: 0,
            cursor_column: 0,
            cursor_cells: 1,
            cursor_visible: false,
            cursor_text: String::new(),
            display_offset: 0,
            viewport_mode: ax_ssh::terminal::TerminalViewportMode::Follow,
            mouse_reporting: Default::default(),
            mouse_button_reporting_active: false,
            mouse_wheel_reporting_active: false,
        }
    }

    #[test]
    fn non_ascii_runs_use_fixed_cell_alignment() {
        let rendered = render_terminal(
            snapshot_line(vec![
                TerminalStyledRun {
                    text: "中".into(),
                    column: 0,
                    cells: 2,
                    style: TerminalStyle::default(),
                },
                TerminalStyledRun {
                    text: "┌".into(),
                    column: 2,
                    cells: 1,
                    style: TerminalStyle::default(),
                },
                plain_run("A", 3),
            ]),
            settings(),
        );
        let runs = &rendered.lines[0].runs;

        assert!(runs[0].centered);
        assert!(runs[1].centered);
        assert!(!runs[2].centered);
    }

    #[test]
    fn compact_spans_omit_default_backgrounds_and_merge_adjacent_decorations() {
        let default_background = settings().default_background;
        let accent_background = RgbColor::new(80, 30, 20);
        let foreground = settings().default_foreground;
        let run = |column, background, underline, strikethrough| RenderedTerminalRun {
            text: "x".into(),
            column,
            cells: 1,
            foreground,
            background,
            bold: false,
            italic: false,
            underline,
            strikethrough,
            centered: false,
        };
        let runs = [
            run(0, default_background, false, false),
            run(1, accent_background, true, false),
            run(2, accent_background, true, false),
            run(4, accent_background, false, true),
        ];

        assert_eq!(
            compact_background_runs(&runs, default_background),
            vec![
                RenderedTerminalBackgroundRun {
                    column: 1,
                    cells: 2,
                    background: accent_background,
                },
                RenderedTerminalBackgroundRun {
                    column: 4,
                    cells: 1,
                    background: accent_background,
                },
            ]
        );
        assert_eq!(
            compact_decoration_runs(&runs),
            vec![
                RenderedTerminalDecorationRun {
                    column: 1,
                    cells: 2,
                    foreground,
                    strikethrough: false,
                },
                RenderedTerminalDecorationRun {
                    column: 4,
                    cells: 1,
                    foreground,
                    strikethrough: true,
                },
            ]
        );
    }

    fn plain_run(text: &str, column: usize) -> TerminalStyledRun {
        TerminalStyledRun {
            text: text.to_owned(),
            column,
            cells: text.len(),
            style: TerminalStyle::default(),
        }
    }

    #[test]
    fn semantic_highlights_cover_statuses_and_bounded_keywords() {
        let text = "INFO 200 OK https://example.test 302 404 WARN 503 ERROR /srv/log";
        let rendered =
            render_terminal(snapshot_line(vec![plain_run(text, 0)]), semantic_settings());
        let runs = &rendered.lines[0].runs;
        let run_for = |text: &str| {
            runs.iter()
                .find(|run| run.text == text)
                .unwrap_or_else(|| panic!("missing semantic run {text:?}"))
        };
        let run_containing = |text: &str| {
            runs.iter()
                .find(|run| run.text.contains(text))
                .unwrap_or_else(|| panic!("missing semantic text {text:?}"))
        };
        let palette = SemanticPalette::for_terminal(
            &TerminalPalette::for_scheme(TerminalColorScheme::Dark),
            SemanticColorOverrides::default(),
        );

        assert_eq!(
            run_containing("https://example.test").foreground,
            settings().default_foreground
        );
        assert_eq!(
            run_containing("/srv/log").foreground,
            settings().default_foreground
        );
        assert_eq!(run_for("INFO").foreground, palette.info);
        assert_eq!(run_for("200").foreground, palette.success);
        assert_eq!(run_for("OK").foreground, palette.success);
        assert_eq!(run_for("302").foreground, palette.info);
        assert_eq!(run_for("404").foreground, palette.warning);
        assert_eq!(run_for("WARN").foreground, palette.warning);
        assert_eq!(run_for("503").foreground, palette.error);
        assert_eq!(run_for("ERROR").foreground, palette.error);
    }

    #[test]
    fn semantic_highlights_apply_to_real_default_terminal_cells() {
        let mut terminal = ax_ssh::terminal::TerminalModel::new(128, 3, 10);
        terminal.process(b"INFO 200 OK ERROR https://example.test /srv/log");
        let snapshot = terminal.snapshot();
        let source_runs = &snapshot.lines[0].runs;
        assert_eq!(source_runs.len(), 1);
        assert_eq!(source_runs[0].style, TerminalStyle::default());

        let rendered = render_terminal(snapshot, semantic_settings());
        let runs = &rendered.lines[0].runs;
        let run_for = |text: &str| {
            runs.iter()
                .find(|run| run.text == text)
                .unwrap_or_else(|| panic!("missing rendered run {text:?}"))
        };
        let run_containing = |text: &str| {
            runs.iter()
                .find(|run| run.text.contains(text))
                .unwrap_or_else(|| panic!("missing rendered text {text:?}"))
        };
        let palette = SemanticPalette::for_terminal(
            &TerminalPalette::for_scheme(TerminalColorScheme::Dark),
            SemanticColorOverrides::default(),
        );

        assert_eq!(run_for("INFO").foreground, palette.info);
        assert_eq!(run_for("200").foreground, palette.success);
        assert_eq!(run_for("OK").foreground, palette.success);
        assert_eq!(run_for("ERROR").foreground, palette.error);
        assert_eq!(
            run_containing("https://example.test").foreground,
            settings().default_foreground
        );
        assert_eq!(
            run_containing("/srv/log").foreground,
            settings().default_foreground
        );
    }

    #[test]
    fn semantic_highlights_are_boundary_aware_and_preserve_ansi_runs() {
        let ansi_style = TerminalStyle {
            foreground: TerminalColor::Indexed(4),
            ..TerminalStyle::default()
        };
        let rendered = render_terminal(
            snapshot_line(vec![
                plain_run("terror error 2000 200", 0),
                TerminalStyledRun {
                    text: " ERROR".to_owned(),
                    column: 24,
                    cells: 6,
                    style: ansi_style,
                },
            ]),
            semantic_settings(),
        );
        let runs = &rendered.lines[0].runs;
        let terror = runs
            .iter()
            .find(|run| run.text.starts_with("terror"))
            .unwrap();
        let error = runs.iter().find(|run| run.text == "error").unwrap();
        let number = runs.iter().find(|run| run.text.contains("2000")).unwrap();
        let status = runs.iter().find(|run| run.text == "200").unwrap();
        let ansi = runs.iter().find(|run| run.text == " ERROR").unwrap();

        assert_eq!(terror.foreground, settings().default_foreground);
        assert_eq!(
            error.foreground,
            SemanticPalette::for_terminal(
                &TerminalPalette::for_scheme(TerminalColorScheme::Dark),
                SemanticColorOverrides::default(),
            )
            .error
        );
        assert_eq!(number.foreground, settings().default_foreground);
        assert_eq!(
            status.foreground,
            SemanticPalette::for_terminal(
                &TerminalPalette::for_scheme(TerminalColorScheme::Dark),
                SemanticColorOverrides::default(),
            )
            .success
        );
        assert_eq!(
            ansi.foreground,
            resolve_style_colors(
                ansi_style,
                &TerminalPalette::for_scheme(TerminalColorScheme::Dark),
                &settings(),
            )
            .0
        );
    }

    #[test]
    fn semantic_highlighting_is_disabled_by_default() {
        let rendered = render_terminal(
            snapshot_line(vec![plain_run("INFO ERROR https://example.test", 0)]),
            settings(),
        );

        assert_eq!(rendered.lines[0].runs.len(), 1);
        assert_eq!(
            rendered.lines[0].runs[0].foreground,
            settings().default_foreground
        );
    }

    #[test]
    fn configured_semantic_colors_are_brightened_after_selection() {
        let overrides = SemanticColorOverrides {
            success: Some(RgbColor::new(37, 211, 139)),
            info: Some(RgbColor::new(116, 177, 255)),
            warning: Some(RgbColor::new(255, 210, 77)),
            error: Some(RgbColor::new(255, 114, 114)),
        };
        let rendered = render_terminal(
            snapshot_line(vec![plain_run(
                "INFO 200 OK https://example.test 302 404 WARN 503 ERROR /srv/log",
                0,
            )]),
            TerminalRenderSettings {
                text_brightness: 1.20,
                semantic_highlighting: true,
                semantic_colors: overrides,
                ..settings()
            },
        );
        let runs = &rendered.lines[0].runs;
        let run_for = |text: &str| {
            runs.iter()
                .find(|run| run.text == text)
                .unwrap_or_else(|| panic!("missing semantic run {text:?}"))
        };
        let run_containing = |text: &str| {
            runs.iter()
                .find(|run| run.text.contains(text))
                .unwrap_or_else(|| panic!("missing semantic text {text:?}"))
        };

        assert_eq!(
            run_containing("https://example.test").foreground,
            adjust_text_foreground(settings().default_foreground, 1.20, false)
        );
        assert_eq!(
            run_for("INFO").foreground,
            adjust_text_foreground(overrides.info.unwrap(), 1.20, false)
        );
        assert_eq!(
            run_for("200").foreground,
            adjust_text_foreground(overrides.success.unwrap(), 1.20, false)
        );
        assert_eq!(
            run_for("302").foreground,
            adjust_text_foreground(overrides.info.unwrap(), 1.20, false)
        );
        assert_eq!(
            run_for("WARN").foreground,
            adjust_text_foreground(overrides.warning.unwrap(), 1.20, false)
        );
        assert_eq!(
            run_for("ERROR").foreground,
            adjust_text_foreground(overrides.error.unwrap(), 1.20, false)
        );
    }

    #[test]
    fn bold_standard_colors_use_bright_palette_when_enabled() {
        let rendered = render_terminal(
            snapshot(TerminalStyle {
                foreground: TerminalColor::Indexed(1),
                bold: true,
                ..TerminalStyle::default()
            }),
            settings(),
        );

        assert_eq!(
            rendered.lines[0].runs[0].foreground,
            RgbColor::new(255, 92, 87)
        );
    }

    #[test]
    fn resolves_256_color_cube_and_inverse_background() {
        let rendered = render_terminal(
            snapshot(TerminalStyle {
                foreground: TerminalColor::Indexed(208),
                background: TerminalColor::Indexed(21),
                inverse: true,
                ..TerminalStyle::default()
            }),
            settings(),
        );

        assert_eq!(
            rendered.lines[0].runs[0].foreground,
            RgbColor::new(0, 0, 255)
        );
        assert_eq!(
            rendered.lines[0].runs[0].background,
            RgbColor::new(255, 135, 0)
        );
    }

    #[test]
    fn theme_defaults_override_only_terminal_defaults() {
        let mut themed = settings();
        themed.default_foreground = RgbColor::new(170, 187, 204);
        themed.default_background = RgbColor::new(16, 32, 48);
        themed.selection_background = RgbColor::new(48, 64, 80);
        let default_rendered = render_terminal(snapshot(TerminalStyle::default()), themed);
        let indexed_rendered = render_terminal(
            snapshot(TerminalStyle {
                foreground: TerminalColor::Indexed(1),
                ..TerminalStyle::default()
            }),
            themed,
        );

        assert_eq!(default_rendered.foreground, RgbColor::new(170, 187, 204));
        assert_eq!(default_rendered.background, RgbColor::new(16, 32, 48));
        assert_eq!(
            default_rendered.selection_background,
            RgbColor::new(48, 64, 80)
        );
        assert_eq!(
            indexed_rendered.lines[0].runs[0].foreground,
            RgbColor::new(255, 92, 87)
        );
    }

    #[test]
    fn fixed_dark_theme_schemes_use_axshell_ansi() {
        for scheme in [
            TerminalColorScheme::Dark,
            TerminalColorScheme::SolarizedDark,
            TerminalColorScheme::ArcticDark,
            TerminalColorScheme::TokyoDark,
            TerminalColorScheme::EmberDark,
            TerminalColorScheme::ForestDark,
        ] {
            assert_eq!(TerminalPalette::for_scheme(scheme).ansi, AXSHELL_DARK_ANSI);
        }
    }

    #[test]
    fn neutral_brightness_preserves_all_foreground_sources_exactly() {
        for (color, expected) in [
            (TerminalColor::Default, RgbColor::new(204, 204, 204)),
            (TerminalColor::Indexed(1), RgbColor::new(255, 92, 87)),
            (TerminalColor::Indexed(208), RgbColor::new(255, 135, 0)),
            (
                TerminalColor::Rgb {
                    red: 100,
                    green: 120,
                    blue: 140,
                },
                RgbColor::new(100, 120, 140),
            ),
        ] {
            let rendered = render_terminal(
                snapshot(TerminalStyle {
                    foreground: color,
                    ..TerminalStyle::default()
                }),
                settings(),
            );
            assert_eq!(rendered.lines[0].runs[0].foreground, expected);
        }
    }

    #[test]
    fn brightness_adjusts_all_foreground_sources_but_not_surfaces_or_cursor() {
        let background = RgbColor::new(220, 230, 240);
        for color in [
            TerminalColor::Default,
            TerminalColor::Indexed(1),
            TerminalColor::Indexed(208),
            TerminalColor::Rgb {
                red: 100,
                green: 120,
                blue: 140,
            },
        ] {
            let rendered = render_terminal(
                snapshot(TerminalStyle {
                    foreground: color,
                    background: TerminalColor::Rgb {
                        red: background.red,
                        green: background.green,
                        blue: background.blue,
                    },
                    ..TerminalStyle::default()
                }),
                TerminalRenderSettings {
                    text_brightness: 0.60,
                    ..settings()
                },
            );
            assert_ne!(
                rendered.lines[0].runs[0].foreground,
                resolve_color(
                    color,
                    settings().default_foreground,
                    &TerminalPalette::for_scheme(TerminalColorScheme::Dark),
                )
            );
            assert_eq!(rendered.lines[0].runs[0].background, background);
            assert_eq!(rendered.background, settings().default_background);
            assert_eq!(
                rendered.selection_background,
                settings().selection_background
            );
            assert_eq!(rendered.foreground, settings().default_foreground);
        }
    }

    #[test]
    fn inverse_is_resolved_before_visible_foreground_brightness() {
        let rendered = render_terminal(
            snapshot(TerminalStyle {
                foreground: TerminalColor::Indexed(208),
                background: TerminalColor::Indexed(21),
                inverse: true,
                ..TerminalStyle::default()
            }),
            TerminalRenderSettings {
                text_brightness: 0.60,
                ..settings()
            },
        );
        let run = &rendered.lines[0].runs[0];

        assert_eq!(
            run.foreground,
            adjust_text_foreground(RgbColor::new(0, 0, 255), 0.60, false)
        );
        assert_eq!(run.background, RgbColor::new(255, 135, 0));
    }

    #[test]
    fn dim_combines_with_brightness_in_the_final_foreground_adjustment() {
        let source = RgbColor::new(100, 120, 140);
        let rendered = render_terminal(
            snapshot(TerminalStyle {
                foreground: TerminalColor::Rgb {
                    red: source.red,
                    green: source.green,
                    blue: source.blue,
                },
                dim: true,
                ..TerminalStyle::default()
            }),
            TerminalRenderSettings {
                text_brightness: 1.20,
                ..settings()
            },
        );

        assert_eq!(
            rendered.lines[0].runs[0].foreground,
            adjust_text_foreground(source, 1.20, true)
        );
        assert_ne!(
            rendered.lines[0].runs[0].foreground,
            adjust_text_foreground(source, 1.20, false)
        );
    }
}
