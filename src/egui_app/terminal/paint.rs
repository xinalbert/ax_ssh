use ax_ssh::config::AppearanceSettings;
use ax_ssh::controller::semantic::{HighlightSpan, SemanticHighlight};
use ax_ssh::controller::{TerminalView, adjust_terminal_text_rgb};
use ax_ssh::terminal::{TerminalColor, TerminalQueryPalette, TerminalStyle, TerminalStyledRun};
use eframe::egui::{self, Color32, Painter, Stroke};

use super::geometry::Grid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Colors {
    palette: [Color32; 2],
    brightness_percent: u16,
    bright_bold: bool,
    semantic_enabled: bool,
    semantic_colors: [Option<Color32>; 4],
}

impl Colors {
    pub(super) fn new(palette: [Color32; 2], appearance: Option<&AppearanceSettings>) -> Self {
        Self {
            palette,
            brightness_percent: appearance.map_or(100, |a| a.terminal_text_brightness_percent),
            bright_bold: appearance.is_none_or(|a| a.bright_bold_text),
            semantic_enabled: appearance.is_some_and(|a| a.terminal_semantic_highlighting),
            semantic_colors: appearance.map_or([None; 4], |a| {
                let colors = &a.terminal_semantic_colors;
                [
                    &colors.success,
                    &colors.info,
                    &colors.warning,
                    &colors.error,
                ]
                .map(|value| {
                    let value = value.trim();
                    (value.len() == 7
                        && value.starts_with('#')
                        && value[1..].bytes().all(|b| b.is_ascii_hexdigit()))
                    .then(|| Color32::from_hex(value).ok())
                    .flatten()
                })
            }),
        }
    }

    fn semantic(self, highlight: SemanticHighlight) -> Color32 {
        let (slot, ansi) = match highlight {
            SemanticHighlight::Success => (0, 10),
            SemanticHighlight::Info => (1, 12),
            SemanticHighlight::Warning => (2, 11),
            SemanticHighlight::Error => (3, 9),
        };
        self.adjust(
            self.semantic_colors[slot]
                .unwrap_or_else(|| resolve(TerminalColor::Indexed(ansi), self.palette[0])),
            false,
        )
    }

    fn adjust(self, color: Color32, dim: bool) -> Color32 {
        if self.brightness_percent == 100 && !dim {
            return color;
        }
        let [r, g, b, alpha] = color.to_srgba_unmultiplied();
        let [r, g, b] =
            adjust_terminal_text_rgb([r, g, b], f64::from(self.brightness_percent) / 100.0, dim);
        Color32::from_rgba_unmultiplied(r, g, b, alpha)
    }
}

struct Run {
    column: usize,
    cells: usize,
    background: Color32,
    text: egui::text::LayoutJob,
}

#[derive(Default)]
pub(super) struct CachedRow {
    revision: u64,
    scale: u32,
    cell_width: u32,
    cell_height: u32,
    font_size: u32,
    foreground: Color32,
    background: Color32,
    colors: Option<Colors>,
    source: Option<std::sync::Arc<ax_ssh::terminal::TerminalStyledLine>>,
    semantic: Option<std::sync::Arc<ax_ssh::controller::semantic::SemanticLine>>,
    runs: Vec<Run>,
}

pub(super) fn rows(
    painter: &Painter,
    view: &TerminalView,
    grid: &Grid,
    cache: &mut Vec<CachedRow>,
    size: f32,
    ppp: f32,
    settings: Colors,
) {
    cache.resize_with(view.terminal.lines.len(), CachedRow::default);
    let foreground = resolve(view.terminal.foreground_color, settings.palette[0]);
    let background = resolve(view.terminal.background_color, settings.palette[1]);
    let synthetic_italic = super::super::fonts::synthetic_italic(painter.ctx());
    for (row, (source, cached)) in view.terminal.lines.iter().zip(cache.iter_mut()).enumerate() {
        if cached
            .source
            .as_ref()
            .is_none_or(|previous| !std::sync::Arc::ptr_eq(previous, source))
            || cached.semantic.as_ref().map(std::sync::Arc::as_ptr)
                != view.semantic.get(row).map(std::sync::Arc::as_ptr)
            || cached.revision != source.revision
            || cached.scale != ppp.to_bits()
            || cached.cell_width != grid.cell.x.to_bits()
            || cached.cell_height != grid.cell.y.to_bits()
            || cached.font_size != size.to_bits()
            || cached.foreground != foreground
            || cached.background != background
            || cached.colors != Some(settings)
        {
            cached.runs.clear();
            for (run_index, run) in source.runs.iter().enumerate() {
                let (fg, bg, decoration) = colors(run.style, foreground, background, settings);
                let font_id = super::super::fonts::font(size, run.style.bold, run.style.italic);
                let format = egui::TextFormat {
                    font_id,
                    italics: run.style.italic && synthetic_italic[usize::from(run.style.bold)],
                    color: fg,
                    line_height: Some(grid.cell.y),
                    underline: if run.style.underline {
                        Stroke::new(1.0 / ppp, decoration)
                    } else {
                        Stroke::NONE
                    },
                    strikethrough: if run.style.strikethrough {
                        Stroke::new(1.0 / ppp, fg)
                    } else {
                        Stroke::NONE
                    },
                    ..Default::default()
                };
                let mut text = text_job(painter, run, format, grid.cell.x);
                if settings.semantic_enabled {
                    let spans = view
                        .semantic
                        .get(row)
                        .map_or(&[][..], |line| line.spans(source, run_index));
                    apply_semantic(&mut text, spans, settings);
                }
                cached.runs.push(Run {
                    column: run.column,
                    cells: run.cells,
                    background: bg,
                    text,
                });
            }
            cached.revision = source.revision;
            cached.scale = ppp.to_bits();
            cached.cell_width = grid.cell.x.to_bits();
            cached.cell_height = grid.cell.y.to_bits();
            cached.font_size = size.to_bits();
            cached.foreground = foreground;
            cached.background = background;
            cached.colors = Some(settings);
            cached.source = Some(source.clone());
            cached.semantic = view.semantic.get(row).cloned();
        }
        for run in &cached.runs {
            painter.rect_filled(grid.span(row, run.column, run.cells), 0.0, run.background);
        }
        for run in &cached.runs {
            painter.galley(
                grid.span(row, run.column, run.cells).min,
                // Ask egui's memoized layout cache every pass. Retaining a
                // Galley ourselves would retain stale atlas UVs after egui
                // clears its glyph atlas or replaces a discovered fallback.
                painter.layout_job(run.text.clone()),
                foreground,
            );
        }
    }
}

fn apply_semantic(job: &mut egui::text::LayoutJob, spans: &[HighlightSpan], colors: Colors) {
    let mut spans = spans.iter().peekable();
    // Eligible ASCII runs already have one section per cell. Only foreground
    // changes: offsets, program decorations and font styles remain intact.
    for section in &mut job.sections {
        while spans
            .peek()
            .is_some_and(|span| span.range.end <= section.byte_range.start.0)
        {
            spans.next();
        }
        if let Some(span) = spans.peek()
            && span.range.start <= section.byte_range.start.0
            && section.byte_range.end.0 <= span.range.end
        {
            section.format.color = colors.semantic(span.highlight);
        }
    }
}

fn text_job(
    painter: &Painter,
    run: &TerminalStyledRun,
    format: egui::TextFormat,
    cell_width: f32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::single_section(run.text.clone(), format.clone());
    job.wrap.max_width = f32::INFINITY;
    // These model batches contain one character per cell. Give each its exact
    // column, avoiding both ligatures and accumulated font-advance rounding.
    // Other runs represent one model cell (possibly wide or multi-scalar), and
    // stay intact so combining marks and emoji can be shaped together.
    if (run.text.is_ascii()
        || run
            .text
            .chars()
            .all(|c| ('\u{2500}'..='\u{257f}').contains(&c)))
        && run.text.chars().count() == run.cells
        && run.cells > 1
    {
        job.sections.clear();
        let mut end = 0.0;
        for (column, (start, character)) in run.text.char_indices().enumerate() {
            job.sections.push(egui::text::LayoutSection {
                leading_space: column as f32 * cell_width - end,
                byte_range: egui::text::ByteIndex(start)
                    ..egui::text::ByteIndex(start + character.len_utf8()),
                format: format.clone(),
            });
            let character_job =
                egui::text::LayoutJob::single_section(character.to_string(), format.clone());
            end = column as f32 * cell_width + painter.layout_job(character_job).size().x;
        }
    }
    job
}

fn colors(
    style: TerminalStyle,
    foreground: Color32,
    background: Color32,
    settings: Colors,
) -> (Color32, Color32, Color32) {
    let foreground_color = match style.foreground {
        TerminalColor::Indexed(index @ 0..=7) if style.bold && settings.bright_bold => {
            TerminalColor::Indexed(index + 8)
        }
        color => color,
    };
    let mut fg = resolve(foreground_color, foreground);
    let mut bg = resolve(style.background, background);
    if style.inverse {
        std::mem::swap(&mut fg, &mut bg);
    }
    let mut decoration = style.underline_color.map_or(fg, |color| resolve(color, fg));
    fg = settings.adjust(fg, style.dim);
    decoration = settings.adjust(decoration, style.dim);
    if style.hidden {
        fg = bg;
        decoration = bg;
    }
    (fg, bg, decoration)
}

pub(super) fn resolve(color: TerminalColor, fallback: Color32) -> Color32 {
    match color {
        TerminalColor::Default => fallback,
        TerminalColor::Rgb { red, green, blue } => Color32::from_rgb(red, green, blue),
        TerminalColor::Indexed(index) if index < 16 => {
            let color = TerminalQueryPalette::default().ansi[index as usize];
            Color32::from_rgb(color.red, color.green, color.blue)
        }
        TerminalColor::Indexed(index) if index >= 232 => Color32::from_gray(8 + (index - 232) * 10),
        TerminalColor::Indexed(index) => {
            let index = index - 16;
            let component = |n: u8| if n == 0 { 0 } else { 55 + n * 40 };
            Color32::from_rgb(
                component(index / 36),
                component(index / 6 % 6),
                component(index % 6),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_batches_place_each_glyph_on_its_column_at_fractional_scales() {
        for ppp in [1.0, 1.25, 1.5, 2.0] {
            let ctx = egui::Context::default();
            ctx.set_fonts(super::super::super::fonts::load().unwrap());
            ctx.set_pixels_per_point(ppp);
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let font = super::super::super::fonts::font(15.0, false, false);
                let width = ui.fonts_mut(|fonts| fonts.glyph_width(&font, 'M'));
                let cell_width = (width * ppp).round().max(1.0) / ppp;
                for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
                    for text in [
                        "fi -> <= != iMW 0123456789".repeat(12),
                        "┌─┬─┐│└─┴─┘".repeat(24),
                    ] {
                        let run = TerminalStyledRun {
                            text: text.clone(),
                            column: 0,
                            cells: text.chars().count(),
                            style: TerminalStyle::default(),
                            hyperlink: None,
                        };
                        let format = egui::TextFormat {
                            font_id: super::super::super::fonts::font(15.0, bold, italic),
                            ..Default::default()
                        };
                        let galley = ui.painter().layout_job(text_job(
                            ui.painter(),
                            &run,
                            format,
                            cell_width,
                        ));
                        assert_eq!(galley.rows.len(), 1);
                        assert_eq!(galley.rows[0].glyphs.len(), run.cells);
                        for (column, glyph) in galley.rows[0].glyphs.iter().enumerate() {
                            assert!(
                                (glyph.pos.x - column as f32 * cell_width).abs() <= 1.0 / ppp,
                                "{text:?}, scale {ppp}, column {column}: {}",
                                glyph.pos.x
                            );
                        }
                    }
                }
            });
            output.drop_without_applying_deltas();
        }
    }

    #[test]
    fn indexed_palette_preserves_cube_and_grayscale_endpoints() {
        assert_eq!(
            resolve(TerminalColor::Indexed(16), Color32::WHITE),
            Color32::BLACK
        );
        assert_eq!(
            resolve(TerminalColor::Indexed(231), Color32::BLACK),
            Color32::WHITE
        );
        assert_eq!(
            resolve(TerminalColor::Indexed(255), Color32::BLACK),
            Color32::from_gray(238)
        );
    }

    #[test]
    fn inverse_and_hidden_apply_after_resolving_defaults() {
        let style = TerminalStyle {
            inverse: true,
            hidden: true,
            ..Default::default()
        };
        assert_eq!(
            colors(
                style,
                Color32::WHITE,
                Color32::BLACK,
                Colors::new([Color32::WHITE, Color32::BLACK], None)
            ),
            (Color32::WHITE, Color32::WHITE, Color32::WHITE)
        );
    }
}

#[cfg(test)]
mod color_tests {
    use super::*;
    use ax_ssh::controller::ConnectionIdentity;
    use ax_ssh::terminal::TerminalModel;

    #[test]
    fn bold_only_promotes_low_ansi_and_faint_preserves_alpha() {
        let palette = [Color32::LIGHT_GRAY, Color32::BLACK];
        for foreground in [
            TerminalColor::Default,
            TerminalColor::Indexed(1),
            TerminalColor::Indexed(9),
            TerminalColor::Indexed(208),
            TerminalColor::Rgb {
                red: 80,
                green: 120,
                blue: 160,
            },
        ] {
            let style = TerminalStyle {
                foreground,
                bold: true,
                ..Default::default()
            };
            let enabled = Colors::new(palette, None);
            let disabled = Colors {
                bright_bold: false,
                ..enabled
            };
            let (bright, _, _) = colors(style, palette[0], palette[1], enabled);
            let (normal, _, _) = colors(style, palette[0], palette[1], disabled);
            let expected = match foreground {
                TerminalColor::Indexed(1) => resolve(TerminalColor::Indexed(9), palette[0]),
                _ => normal,
            };
            assert_eq!(bright, expected);
            let (dim, _, _) = colors(
                TerminalStyle { dim: true, ..style },
                palette[0],
                palette[1],
                enabled,
            );
            assert_eq!(dim.a(), 255);
        }
        let transparent = Color32::from_rgba_unmultiplied(100, 120, 140, 80);
        assert_eq!(Colors::new(palette, None).adjust(transparent, true).a(), 80);
    }

    #[test]
    fn parsed_rows_repaint_settings_without_output_and_keep_surfaces_hidden_and_decorations() {
        let mut model = TerminalModel::new(80, 3, 100);
        model.process(b"\x1b[1;30;4;9mB\x1b[0m\x1b[2;4;38;2;100;120;140mD\x1b[0m\x1b[7;4;38;2;40;60;80;48;2;100;120;140mI\x1b[0m\x1b[8;4;9;58;2;255;0;0mH\x1b[0m\x1b[4;58;2;100;100;100mU");
        let view = TerminalView {
            semantic: Vec::new(),
            terminal: model.snapshot(),
            status: String::new(),
            connected: false,
            selection_revision: 0,
            generation: 0,
            identity: ConnectionIdentity::Local,
            prompt: None,
        };
        let revision = view.terminal.lines[0].revision;
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::egui_app::fonts::load().unwrap());
        let palette = [Color32::from_gray(200), Color32::from_gray(20)];
        let original = Colors::new(palette, None);
        let mut cache = Vec::new();
        let mut draw = |settings| {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let grid = Grid::new(ui.max_rect(), egui::vec2(10.0, 20.0), 1.0);
                rows(ui.painter(), &view, &grid, &mut cache, 15.0, 1.0, settings);
            });
            output.drop_without_applying_deltas();
            assert_eq!(cache[0].revision, revision);
            ["B", "D", "I", "H", "U"].map(|text| {
                let run = cache[0]
                    .runs
                    .iter()
                    .find(|run| run.text.text == text)
                    .unwrap();
                (run.text.sections[0].format.clone(), run.background)
            })
        };
        let baseline = draw(original);
        for brightness_percent in [60, 120] {
            let updated = draw(Colors {
                brightness_percent,
                ..original
            });
            for index in [0, 1, 2, 4] {
                assert_ne!(updated[index].0.color, baseline[index].0.color);
                assert_eq!(updated[index].1, baseline[index].1);
                assert_eq!(updated[index].0.color.a(), 255);
            }
            assert_eq!(updated[2].0.underline.color, updated[2].0.color);
            assert_eq!(updated[2].1, Color32::from_rgb(40, 60, 80));
            assert_eq!(updated[3].0.color, updated[3].1);
            assert_eq!(updated[3].0.underline.color, updated[3].1);
            assert_eq!(updated[3].0.strikethrough.color, updated[3].1);
            assert_eq!(
                updated[4].0.underline.color,
                Color32::from_gray(brightness_percent as u8)
            );
            assert_eq!(updated[0].0.strikethrough.color, updated[0].0.color);
        }
        let normal_bold = draw(Colors {
            bright_bold: false,
            ..original
        });
        assert_ne!(normal_bold[0].0.color, baseline[0].0.color);
        assert_eq!(normal_bold[0].0.font_id, baseline[0].0.font_id);
        let restored = draw(original);
        for index in 0..5 {
            assert_eq!(restored[index].0, baseline[index].0);
            assert_eq!(restored[index].1, baseline[index].1);
        }
    }
}

#[cfg(all(test, unix))]
mod semantic_tests;
