use ax_ssh::controller::TerminalView;
use ax_ssh::terminal::{TerminalColor, TerminalQueryPalette, TerminalStyle, TerminalStyledRun};
use eframe::egui::{self, Color32, Painter, Stroke};

use super::geometry::Grid;

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
    runs: Vec<Run>,
}

pub(super) fn rows(
    painter: &Painter,
    view: &TerminalView,
    grid: &Grid,
    cache: &mut Vec<CachedRow>,
    size: f32,
    ppp: f32,
    palette: [Color32; 2],
) {
    cache.resize_with(view.terminal.lines.len(), CachedRow::default);
    let foreground = resolve(view.terminal.foreground_color, palette[0]);
    let background = resolve(view.terminal.background_color, palette[1]);
    for (row, (source, cached)) in view.terminal.lines.iter().zip(cache.iter_mut()).enumerate() {
        if cached.revision != source.revision
            || cached.scale != ppp.to_bits()
            || cached.cell_width != grid.cell.x.to_bits()
            || cached.cell_height != grid.cell.y.to_bits()
            || cached.font_size != size.to_bits()
            || cached.foreground != foreground
            || cached.background != background
        {
            cached.runs.clear();
            for run in &source.runs {
                let (fg, bg) = colors(run.style, foreground, background);
                let font_id = super::super::fonts::font(size, run.style.bold, run.style.italic);
                let format = egui::TextFormat {
                    font_id,
                    color: fg,
                    line_height: Some(grid.cell.y),
                    underline: if run.style.underline {
                        Stroke::new(
                            1.0 / ppp,
                            resolve(
                                run.style.underline_color.unwrap_or(run.style.foreground),
                                fg,
                            ),
                        )
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
                let text = text_job(painter, run, format, grid.cell.x);
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

fn colors(style: TerminalStyle, foreground: Color32, background: Color32) -> (Color32, Color32) {
    let mut fg = resolve(style.foreground, foreground);
    let mut bg = resolve(style.background, background);
    if style.inverse {
        std::mem::swap(&mut fg, &mut bg);
    }
    if style.dim {
        fg = fg.gamma_multiply(0.7);
    }
    if style.hidden {
        fg = bg;
    }
    (fg, bg)
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
            colors(style, Color32::WHITE, Color32::BLACK),
            (Color32::WHITE, Color32::WHITE)
        );
    }
}
