//! UI-independent terminal palette resolution.

use ax_ssh::config::TerminalColorScheme;
use ax_ssh::terminal::{
    TerminalColor, TerminalSnapshot, TerminalStyle, TerminalStyledLine, TerminalStyledRun,
};

use super::terminal_targets::terminal_target_span_at_cell;

const MAX_SEMANTIC_HIGHLIGHT_CHARS: usize = 512;
const SEMANTIC_HIGHLIGHT_MINIMUM_CONTRAST_RATIO: f64 = 4.5;

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
    Link,
    Success,
    Info,
    Warning,
    Error,
}

struct SemanticPalette {
    link: RgbColor,
    success: RgbColor,
    info: RgbColor,
    warning: RgbColor,
    error: RgbColor,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SemanticColorOverrides {
    pub(super) link: Option<RgbColor>,
    pub(super) success: Option<RgbColor>,
    pub(super) info: Option<RgbColor>,
    pub(super) warning: Option<RgbColor>,
    pub(super) error: Option<RgbColor>,
}

impl SemanticPalette {
    fn for_terminal(palette: &TerminalPalette, overrides: SemanticColorOverrides) -> Self {
        Self {
            link: semantic_color(
                overrides.link.unwrap_or(palette.ansi[14]),
                palette.background,
            ),
            success: semantic_color(
                overrides.success.unwrap_or(palette.ansi[10]),
                palette.background,
            ),
            info: semantic_color(
                overrides.info.unwrap_or(palette.ansi[12]),
                palette.background,
            ),
            warning: semantic_color(
                overrides.warning.unwrap_or(palette.ansi[11]),
                palette.background,
            ),
            error: semantic_color(
                overrides.error.unwrap_or(palette.ansi[9]),
                palette.background,
            ),
        }
    }

    fn color_for(&self, highlight: SemanticHighlight) -> RgbColor {
        match highlight {
            SemanticHighlight::Link => self.link,
            SemanticHighlight::Success => self.success,
            SemanticHighlight::Info => self.info,
            SemanticHighlight::Warning => self.warning,
            SemanticHighlight::Error => self.error,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct TerminalRenderSettings {
    pub(super) color_scheme: TerminalColorScheme,
    pub(super) default_foreground: RgbColor,
    pub(super) default_background: RgbColor,
    pub(super) selection_background: RgbColor,
    pub(super) minimum_contrast_ratio: f64,
    pub(super) bright_bold_text: bool,
    pub(super) semantic_colors: SemanticColorOverrides,
}

pub(super) struct RenderedTerminal {
    pub(super) lines: Vec<RenderedTerminalLine>,
    pub(super) max_columns: usize,
    pub(super) cursor_row: usize,
    pub(super) cursor_column: usize,
    pub(super) cursor_visible: bool,
    pub(super) cursor_text: String,
    pub(super) foreground: RgbColor,
    pub(super) background: RgbColor,
    pub(super) selection_background: RgbColor,
}

pub(super) struct RenderedTerminalLine {
    pub(super) runs: Vec<RenderedTerminalRun>,
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
}

pub(super) fn render_terminal(
    snapshot: TerminalSnapshot,
    settings: TerminalRenderSettings,
) -> RenderedTerminal {
    let mut palette = TerminalPalette::for_scheme(settings.color_scheme);
    palette.foreground = settings.default_foreground;
    palette.background = settings.default_background;
    palette.selection_background = settings.selection_background;
    let foreground = ensure_contrast_ratio(
        palette.foreground,
        palette.background,
        settings.minimum_contrast_ratio,
    );
    let lines = snapshot
        .lines
        .into_iter()
        .map(|line| render_line(line, &palette, &settings))
        .collect();
    RenderedTerminal {
        lines,
        max_columns: snapshot.max_columns,
        cursor_row: snapshot.cursor_row,
        cursor_column: snapshot.cursor_column,
        cursor_visible: snapshot.cursor_visible,
        cursor_text: snapshot.cursor_text,
        foreground,
        background: palette.background,
        selection_background: palette.selection_background,
    }
}

fn render_line(
    line: TerminalStyledLine,
    palette: &TerminalPalette,
    settings: &TerminalRenderSettings,
) -> RenderedTerminalLine {
    let semantic_palette = SemanticPalette::for_terminal(palette, settings.semantic_colors);
    let runs = line
        .runs
        .into_iter()
        .flat_map(|run| render_run(run, palette, settings, &semantic_palette))
        .collect();
    RenderedTerminalLine { runs }
}

fn render_run(
    run: TerminalStyledRun,
    palette: &TerminalPalette,
    settings: &TerminalRenderSettings,
    semantic_palette: &SemanticPalette,
) -> Vec<RenderedTerminalRun> {
    let TerminalStyledRun {
        text,
        column,
        cells,
        style,
    } = run;
    let highlights = semantic_highlights(&text, cells, style);
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
    };
    let Some(highlights) = highlights else {
        return vec![rendered];
    };
    split_semantic_run(rendered, highlights, semantic_palette)
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
    highlight_terminal_targets(text, &mut highlights);
    highlight_http_statuses(text, &mut highlights);
    highlight_keywords(
        text,
        &mut highlights,
        SemanticHighlight::Error,
        &[
            "error",
            "err",
            "fatal",
            "panic",
            "fail",
            "failed",
            "failure",
            "exception",
            "traceback",
            "critical",
            "crash",
            "crashed",
            "sigsegv",
        ],
    );
    highlight_keywords(
        text,
        &mut highlights,
        SemanticHighlight::Warning,
        &[
            "warn",
            "warning",
            "deprecated",
            "todo",
            "fixme",
            "timeout",
            "timed out",
            "refused",
            "denied",
            "rejected",
            "unreachable",
            "offline",
            "pending",
            "waiting",
            "processing",
        ],
    );
    highlight_keywords(
        text,
        &mut highlights,
        SemanticHighlight::Success,
        &[
            "ok",
            "success",
            "pass",
            "passed",
            "done",
            "completed",
            "ready",
            "connected",
            "online",
            "up",
            "running",
            "deployed",
            "authenticated",
            "authorized",
        ],
    );
    highlight_keywords(
        text,
        &mut highlights,
        SemanticHighlight::Info,
        &[
            "info",
            "notice",
            "debug",
            "trace",
            "dbg",
            "ssh",
            "ssl",
            "tls",
            "certificate",
            "auth",
            "login",
            "start",
            "started",
            "starting",
            "boot",
            "restart",
            "restarting",
            "deploy",
            "deploying",
            "active",
            "executing",
        ],
    );
    highlights.iter().any(Option::is_some).then_some(highlights)
}

fn highlight_terminal_targets(text: &str, highlights: &mut [Option<SemanticHighlight>]) {
    let bytes = text.as_bytes();
    let mut column = 0;
    while column < bytes.len() {
        if matches!(bytes[column], b'h' | b'/' | b'.')
            && let Some(span) = terminal_target_span_at_cell(text, column)
        {
            mark_highlight(highlights, span.start, span.end, SemanticHighlight::Link);
            column = span.end.max(column + 1);
            continue;
        }
        column += 1;
    }
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
            300..=399 => SemanticHighlight::Link,
            400..=499 => SemanticHighlight::Warning,
            500..=599 => SemanticHighlight::Error,
            _ => continue,
        };
        mark_highlight(highlights, start, end, highlight);
    }
}

fn highlight_keywords(
    text: &str,
    highlights: &mut [Option<SemanticHighlight>],
    highlight: SemanticHighlight,
    keywords: &[&str],
) {
    let bytes = text.as_bytes();
    for keyword in keywords {
        let keyword = keyword.as_bytes();
        if keyword.len() > bytes.len() {
            continue;
        }
        for start in 0..=bytes.len() - keyword.len() {
            let end = start + keyword.len();
            if bytes[start..end].eq_ignore_ascii_case(keyword)
                && semantic_token_boundaries(bytes, start, end)
            {
                mark_highlight(highlights, start, end, highlight);
            }
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
        });
        start = end;
    }
    runs
}

fn semantic_color(color: RgbColor, background: RgbColor) -> RgbColor {
    ensure_contrast_ratio(color, background, SEMANTIC_HIGHLIGHT_MINIMUM_CONTRAST_RATIO)
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
    if style.dim {
        foreground = blend(foreground, background, 55);
        foreground = ensure_contrast_ratio(
            foreground,
            background,
            settings.minimum_contrast_ratio / 2.0,
        );
    } else {
        foreground = ensure_contrast_ratio(foreground, background, settings.minimum_contrast_ratio);
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

fn ensure_contrast_ratio(color: RgbColor, background: RgbColor, ratio: f64) -> RgbColor {
    let ratio = ratio.clamp(1.0, 21.0);
    if contrast_ratio(color, background) >= ratio {
        return color;
    }

    let white = adjust_toward(color, background, RgbColor::new(255, 255, 255), ratio);
    let black = adjust_toward(color, background, RgbColor::new(0, 0, 0), ratio);
    match (white, black) {
        (Some((white, white_steps)), Some((black, black_steps))) => {
            if white_steps <= black_steps {
                white
            } else {
                black
            }
        }
        (Some((white, _)), None) => white,
        (None, Some((black, _))) => black,
        (None, None) => color,
    }
}

fn adjust_toward(
    color: RgbColor,
    background: RgbColor,
    target: RgbColor,
    ratio: f64,
) -> Option<(RgbColor, u16)> {
    if contrast_ratio(target, background) < ratio {
        return None;
    }

    let mut low = 1u16;
    let mut high = 255u16;
    while low < high {
        let middle = low + (high - low) / 2;
        let candidate = blend_steps(color, target, middle);
        if contrast_ratio(candidate, background) >= ratio {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    Some((blend_steps(color, target, low), low))
}

fn blend_steps(from: RgbColor, to: RgbColor, steps: u16) -> RgbColor {
    let channel = |from: u8, to: u8| {
        ((u16::from(from) * (255 - steps) + u16::from(to) * steps + 127) / 255) as u8
    };
    RgbColor::new(
        channel(from.red, to.red),
        channel(from.green, to.green),
        channel(from.blue, to.blue),
    )
}

fn blend(from: RgbColor, to: RgbColor, to_percent: u8) -> RgbColor {
    let to_weight = u16::from(to_percent.min(100));
    let from_weight = 100 - to_weight;
    let channel = |from: u8, to: u8| {
        ((u16::from(from) * from_weight + u16::from(to) * to_weight) / 100) as u8
    };
    RgbColor::new(
        channel(from.red, to.red),
        channel(from.green, to.green),
        channel(from.blue, to.blue),
    )
}

fn contrast_ratio(first: RgbColor, second: RgbColor) -> f64 {
    let first = relative_luminance(first);
    let second = relative_luminance(second);
    let lighter = first.max(second);
    let darker = first.min(second);
    (lighter + 0.05) / (darker + 0.05)
}

fn relative_luminance(color: RgbColor) -> f64 {
    let linear = |channel: u8| {
        let channel = f64::from(channel) / 255.0;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    linear(color.red) * 0.2126 + linear(color.green) * 0.7152 + linear(color.blue) * 0.0722
}

struct TerminalPalette {
    foreground: RgbColor,
    background: RgbColor,
    selection_background: RgbColor,
    ansi: [RgbColor; 16],
}

impl TerminalPalette {
    fn for_scheme(scheme: TerminalColorScheme) -> Self {
        match scheme {
            TerminalColorScheme::Dark => Self {
                foreground: RgbColor::new(204, 204, 204),
                background: RgbColor::new(30, 30, 30),
                selection_background: RgbColor::new(38, 79, 120),
                ansi: [
                    RgbColor::new(0, 0, 0),
                    RgbColor::new(205, 49, 49),
                    RgbColor::new(13, 188, 121),
                    RgbColor::new(229, 229, 16),
                    RgbColor::new(36, 114, 200),
                    RgbColor::new(188, 63, 188),
                    RgbColor::new(17, 168, 205),
                    RgbColor::new(229, 229, 229),
                    RgbColor::new(102, 102, 102),
                    RgbColor::new(241, 76, 76),
                    RgbColor::new(35, 209, 139),
                    RgbColor::new(245, 245, 67),
                    RgbColor::new(59, 142, 234),
                    RgbColor::new(214, 112, 214),
                    RgbColor::new(41, 184, 219),
                    RgbColor::new(255, 255, 255),
                ],
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
                ansi: [
                    RgbColor::new(7, 54, 66),
                    RgbColor::new(220, 50, 47),
                    RgbColor::new(133, 153, 0),
                    RgbColor::new(181, 137, 0),
                    RgbColor::new(38, 139, 210),
                    RgbColor::new(211, 54, 130),
                    RgbColor::new(42, 161, 152),
                    RgbColor::new(238, 232, 213),
                    RgbColor::new(0, 43, 54),
                    RgbColor::new(203, 75, 22),
                    RgbColor::new(88, 110, 117),
                    RgbColor::new(101, 123, 131),
                    RgbColor::new(131, 148, 150),
                    RgbColor::new(108, 113, 196),
                    RgbColor::new(147, 161, 161),
                    RgbColor::new(253, 246, 227),
                ],
            },
            TerminalColorScheme::ArcticDark => Self {
                foreground: RgbColor::new(213, 226, 232),
                background: RgbColor::new(17, 28, 37),
                selection_background: RgbColor::new(39, 86, 107),
                ansi: [
                    RgbColor::new(20, 30, 40),
                    RgbColor::new(195, 76, 92),
                    RgbColor::new(73, 151, 115),
                    RgbColor::new(183, 137, 55),
                    RgbColor::new(77, 136, 191),
                    RgbColor::new(145, 105, 184),
                    RgbColor::new(63, 151, 178),
                    RgbColor::new(210, 222, 230),
                    RgbColor::new(91, 111, 126),
                    RgbColor::new(231, 105, 119),
                    RgbColor::new(105, 194, 151),
                    RgbColor::new(218, 177, 87),
                    RgbColor::new(111, 174, 232),
                    RgbColor::new(182, 142, 224),
                    RgbColor::new(100, 198, 220),
                    RgbColor::new(241, 247, 250),
                ],
            },
            TerminalColorScheme::TokyoDark => Self {
                foreground: RgbColor::new(200, 211, 245),
                background: RgbColor::new(16, 19, 35),
                selection_background: RgbColor::new(51, 70, 124),
                ansi: [
                    RgbColor::new(26, 27, 38),
                    RgbColor::new(211, 97, 111),
                    RgbColor::new(95, 188, 142),
                    RgbColor::new(224, 175, 104),
                    RgbColor::new(122, 162, 247),
                    RgbColor::new(187, 154, 247),
                    RgbColor::new(125, 207, 255),
                    RgbColor::new(192, 202, 245),
                    RgbColor::new(76, 82, 112),
                    RgbColor::new(245, 118, 135),
                    RgbColor::new(133, 211, 162),
                    RgbColor::new(242, 196, 124),
                    RgbColor::new(141, 176, 255),
                    RgbColor::new(205, 178, 255),
                    RgbColor::new(147, 218, 255),
                    RgbColor::new(232, 236, 255),
                ],
            },
            TerminalColorScheme::EmberDark => Self {
                foreground: RgbColor::new(231, 214, 207),
                background: RgbColor::new(26, 18, 16),
                selection_background: RgbColor::new(112, 65, 45),
                ansi: [
                    RgbColor::new(34, 23, 20),
                    RgbColor::new(210, 83, 75),
                    RgbColor::new(117, 174, 109),
                    RgbColor::new(224, 170, 91),
                    RgbColor::new(215, 121, 77),
                    RgbColor::new(198, 112, 154),
                    RgbColor::new(98, 182, 181),
                    RgbColor::new(235, 216, 207),
                    RgbColor::new(112, 79, 68),
                    RgbColor::new(239, 116, 106),
                    RgbColor::new(143, 207, 133),
                    RgbColor::new(245, 197, 116),
                    RgbColor::new(243, 151, 98),
                    RgbColor::new(227, 144, 183),
                    RgbColor::new(121, 207, 206),
                    RgbColor::new(255, 244, 238),
                ],
            },
            TerminalColorScheme::ForestDark => Self {
                foreground: RgbColor::new(209, 230, 214),
                background: RgbColor::new(14, 25, 18),
                selection_background: RgbColor::new(40, 94, 59),
                ansi: [
                    RgbColor::new(16, 28, 20),
                    RgbColor::new(202, 80, 92),
                    RgbColor::new(103, 188, 128),
                    RgbColor::new(201, 174, 93),
                    RgbColor::new(101, 161, 221),
                    RgbColor::new(177, 128, 205),
                    RgbColor::new(91, 182, 177),
                    RgbColor::new(215, 233, 220),
                    RgbColor::new(77, 106, 84),
                    RgbColor::new(236, 105, 116),
                    RgbColor::new(129, 213, 153),
                    RgbColor::new(229, 205, 120),
                    RgbColor::new(132, 188, 240),
                    RgbColor::new(203, 153, 231),
                    RgbColor::new(117, 211, 203),
                    RgbColor::new(242, 250, 244),
                ],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(style: TerminalStyle) -> TerminalSnapshot {
        TerminalSnapshot {
            text: "x".into(),
            lines: vec![TerminalStyledLine {
                runs: vec![TerminalStyledRun {
                    text: "x".into(),
                    column: 0,
                    cells: 1,
                    style,
                }],
            }],
            max_columns: 1,
            cursor_row: 0,
            cursor_column: 1,
            cursor_visible: true,
            cursor_text: " ".into(),
        }
    }

    fn settings() -> TerminalRenderSettings {
        TerminalRenderSettings {
            color_scheme: TerminalColorScheme::Dark,
            default_foreground: RgbColor::new(204, 204, 204),
            default_background: RgbColor::new(30, 30, 30),
            selection_background: RgbColor::new(38, 79, 120),
            minimum_contrast_ratio: 4.5,
            bright_bold_text: true,
            semantic_colors: SemanticColorOverrides::default(),
        }
    }

    fn snapshot_line(runs: Vec<TerminalStyledRun>) -> TerminalSnapshot {
        let text = runs.iter().map(|run| run.text.as_str()).collect::<String>();
        TerminalSnapshot {
            text,
            lines: vec![TerminalStyledLine { runs }],
            max_columns: 128,
            cursor_row: 0,
            cursor_column: 0,
            cursor_visible: false,
            cursor_text: String::new(),
        }
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
    fn semantic_highlights_cover_targets_statuses_and_bounded_keywords() {
        let text = "INFO 200 OK https://example.test 404 WARN 503 ERROR /srv/log";
        let rendered = render_terminal(snapshot_line(vec![plain_run(text, 0)]), settings());
        let runs = &rendered.lines[0].runs;
        let run_for = |text: &str| {
            runs.iter()
                .find(|run| run.text == text)
                .unwrap_or_else(|| panic!("missing semantic run {text:?}"))
        };
        let palette = SemanticPalette::for_terminal(
            &TerminalPalette::for_scheme(TerminalColorScheme::Dark),
            SemanticColorOverrides::default(),
        );

        assert_eq!(run_for("https://example.test").foreground, palette.link);
        assert_eq!(run_for("/srv/log").foreground, palette.link);
        assert_eq!(run_for("INFO").foreground, palette.info);
        assert_eq!(run_for("200").foreground, palette.success);
        assert_eq!(run_for("OK").foreground, palette.success);
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

        let rendered = render_terminal(snapshot, settings());
        let runs = &rendered.lines[0].runs;
        let run_for = |text: &str| {
            runs.iter()
                .find(|run| run.text == text)
                .unwrap_or_else(|| panic!("missing rendered run {text:?}"))
        };
        let palette = SemanticPalette::for_terminal(
            &TerminalPalette::for_scheme(TerminalColorScheme::Dark),
            SemanticColorOverrides::default(),
        );

        assert_eq!(run_for("INFO").foreground, palette.info);
        assert_eq!(run_for("200").foreground, palette.success);
        assert_eq!(run_for("OK").foreground, palette.success);
        assert_eq!(run_for("ERROR").foreground, palette.error);
        assert_eq!(run_for("https://example.test").foreground, palette.link);
        assert_eq!(run_for("/srv/log").foreground, palette.link);
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
            settings(),
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
    fn semantic_colors_keep_contrast_for_terminal_schemes_and_custom_surfaces() {
        let schemes = [
            TerminalColorScheme::Dark,
            TerminalColorScheme::Light,
            TerminalColorScheme::SolarizedDark,
            TerminalColorScheme::ArcticDark,
            TerminalColorScheme::TokyoDark,
            TerminalColorScheme::EmberDark,
            TerminalColorScheme::ForestDark,
        ];
        for scheme in schemes {
            let palette = TerminalPalette::for_scheme(scheme);
            let semantic =
                SemanticPalette::for_terminal(&palette, SemanticColorOverrides::default());
            for color in [
                semantic.link,
                semantic.success,
                semantic.info,
                semantic.warning,
                semantic.error,
            ] {
                assert!(contrast_ratio(color, palette.background) >= 4.5);
            }
        }

        let mut palette = TerminalPalette::for_scheme(TerminalColorScheme::Dark);
        palette.background = RgbColor::new(245, 242, 235);
        let semantic = SemanticPalette::for_terminal(&palette, SemanticColorOverrides::default());
        for color in [
            semantic.link,
            semantic.success,
            semantic.info,
            semantic.warning,
            semantic.error,
        ] {
            assert!(contrast_ratio(color, palette.background) >= 4.5);
        }
    }

    #[test]
    fn configured_semantic_colors_override_theme_defaults_and_keep_contrast() {
        let overrides = SemanticColorOverrides {
            link: Some(RgbColor::new(28, 202, 238)),
            success: Some(RgbColor::new(37, 211, 139)),
            info: Some(RgbColor::new(116, 177, 255)),
            warning: Some(RgbColor::new(255, 210, 77)),
            error: Some(RgbColor::new(255, 114, 114)),
        };
        let rendered = render_terminal(
            snapshot_line(vec![plain_run(
                "INFO 200 OK https://example.test 404 WARN 503 ERROR",
                0,
            )]),
            TerminalRenderSettings {
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

        assert_eq!(
            run_for("https://example.test").foreground,
            overrides.link.unwrap()
        );
        assert_eq!(run_for("INFO").foreground, overrides.info.unwrap());
        assert_eq!(run_for("200").foreground, overrides.success.unwrap());
        assert_eq!(run_for("WARN").foreground, overrides.warning.unwrap());
        assert_eq!(run_for("ERROR").foreground, overrides.error.unwrap());

        let mut light_palette = TerminalPalette::for_scheme(TerminalColorScheme::Light);
        light_palette.background = RgbColor::new(245, 242, 235);
        let low_contrast = SemanticColorOverrides {
            link: Some(RgbColor::new(250, 250, 250)),
            success: Some(RgbColor::new(250, 250, 250)),
            info: Some(RgbColor::new(250, 250, 250)),
            warning: Some(RgbColor::new(250, 250, 250)),
            error: Some(RgbColor::new(250, 250, 250)),
        };
        let corrected = SemanticPalette::for_terminal(&light_palette, low_contrast);
        for color in [
            corrected.link,
            corrected.success,
            corrected.info,
            corrected.warning,
            corrected.error,
        ] {
            assert_ne!(color, low_contrast.link.unwrap());
            assert!(contrast_ratio(color, light_palette.background) >= 4.5);
        }
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
            RgbColor::new(241, 76, 76)
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

        assert!(
            contrast_ratio(
                rendered.lines[0].runs[0].foreground,
                rendered.lines[0].runs[0].background,
            ) >= 4.5
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
        assert!(
            contrast_ratio(
                indexed_rendered.lines[0].runs[0].foreground,
                themed.default_background,
            ) >= 4.5
        );
    }

    #[test]
    fn fixed_dark_theme_schemes_keep_distinct_ansi_blue() {
        for (scheme, blue) in [
            (TerminalColorScheme::ArcticDark, RgbColor::new(77, 136, 191)),
            (TerminalColorScheme::TokyoDark, RgbColor::new(122, 162, 247)),
            (TerminalColorScheme::EmberDark, RgbColor::new(215, 121, 77)),
            (
                TerminalColorScheme::ForestDark,
                RgbColor::new(101, 161, 221),
            ),
        ] {
            assert_eq!(TerminalPalette::for_scheme(scheme).ansi[4], blue);
        }
    }

    #[test]
    fn minimum_contrast_preserves_readable_colors_and_backgrounds() {
        let palette = TerminalPalette::for_scheme(TerminalColorScheme::Dark);
        let color = RgbColor::new(100, 120, 140);
        let background = RgbColor::new(220, 230, 240);
        let rendered = render_terminal(
            snapshot(TerminalStyle {
                foreground: TerminalColor::Rgb {
                    red: color.red,
                    green: color.green,
                    blue: color.blue,
                },
                background: TerminalColor::Rgb {
                    red: background.red,
                    green: background.green,
                    blue: background.blue,
                },
                ..TerminalStyle::default()
            }),
            TerminalRenderSettings {
                minimum_contrast_ratio: 4.5,
                ..settings()
            },
        );
        let run = &rendered.lines[0].runs[0];

        assert_eq!(run.background, background);
        assert_ne!(run.foreground, color);
        assert!(contrast_ratio(run.foreground, background) >= 4.5);
        assert_eq!(
            render_terminal(
                snapshot(TerminalStyle {
                    foreground: TerminalColor::Rgb {
                        red: color.red,
                        green: color.green,
                        blue: color.blue,
                    },
                    ..TerminalStyle::default()
                }),
                TerminalRenderSettings {
                    minimum_contrast_ratio: 1.0,
                    ..settings()
                },
            )
            .lines[0]
                .runs[0]
                .foreground,
            color
        );
        assert_eq!(palette.background, RgbColor::new(30, 30, 30));
    }
}
