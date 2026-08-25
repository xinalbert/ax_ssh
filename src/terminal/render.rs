//! Terminal snapshot and styled-cell rendering.

use super::*;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::TermDamage;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor};

impl TerminalModel {
    pub fn snapshot(&mut self) -> TerminalSnapshot {
        let damage = self.refresh_snapshot_lines();
        let content = self.term.renderable_content();
        let grid = self.term.grid();
        let columns = grid.columns();
        let (cursor, cursor_cells) = cursor_geometry(grid, content.cursor.point);
        let cursor_column = cursor.column.0;
        let cursor_visible =
            content.display_offset == 0 && !matches!(content.cursor.shape, CursorShape::Hidden);
        let cursor_text = cursor_visible
            .then(|| &grid[cursor])
            .filter(|cell| !is_wide_continuation(cell))
            .map(cell_text)
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| " ".to_owned());

        TerminalSnapshot {
            lines: self.snapshot_lines.clone(),
            dirty_rows: damage.dirty_rows,
            full_refresh: damage.full_refresh,
            max_columns: columns,
            cursor_row: cursor.line.0.max(0) as usize,
            cursor_column,
            cursor_cells,
            cursor_visible,
            cursor_text,
            display_offset: content.display_offset,
            viewport_mode: self.viewport_mode(),
            mouse_reporting: self.mouse_reporting(),
            mouse_button_reporting_active: self.mouse_button_reporting_active(),
            mouse_wheel_reporting_active: self.mouse_wheel_reporting_active(),
        }
    }

    fn refresh_snapshot_lines(&mut self) -> SnapshotDamage {
        let grid = self.term.grid();
        let rows = grid.screen_lines();
        let columns = grid.columns();
        let display_offset = grid.display_offset();
        let row_count_changed = self.snapshot_lines.len() != rows;
        let dimensions_changed = row_count_changed
            || self.snapshot_columns != columns
            || self.snapshot_display_offset != display_offset;
        let (damaged_rows, full_damage) = if dimensions_changed {
            (None, true)
        } else {
            match self.term.damage() {
                TermDamage::Full => (None, true),
                TermDamage::Partial(damage) => (
                    Some(damage.map(|bounds| bounds.line).collect::<Vec<_>>()),
                    false,
                ),
            }
        };

        let mut dirty_rows = Vec::new();
        if let Some(damaged_rows) = damaged_rows {
            for row in damaged_rows {
                if row >= rows {
                    continue;
                }
                let mut line = styled_line(&self.term, row, columns);
                if self
                    .snapshot_lines
                    .get(row)
                    .is_some_and(|current| current.runs == line.runs)
                {
                    continue;
                }
                line.revision = self.take_line_revision();
                self.snapshot_lines[row] = Arc::new(line);
                dirty_rows.push(row);
            }
        } else {
            let mut lines = Vec::with_capacity(rows);
            for row in 0..rows {
                let mut line = styled_line(&self.term, row, columns);
                if let Some(current) = self
                    .snapshot_lines
                    .get(row)
                    .filter(|current| current.runs == line.runs)
                {
                    lines.push(Arc::clone(current));
                } else {
                    line.revision = self.take_line_revision();
                    lines.push(Arc::new(line));
                    dirty_rows.push(row);
                }
            }
            self.snapshot_lines = lines;
        }
        self.snapshot_columns = columns;
        self.snapshot_display_offset = display_offset;
        self.term.reset_damage();
        dirty_rows.sort_unstable();
        dirty_rows.dedup();
        SnapshotDamage {
            dirty_rows,
            full_refresh: dimensions_changed || full_damage,
        }
    }

    fn take_line_revision(&mut self) -> u64 {
        self.next_line_revision = self.next_line_revision.wrapping_add(1).max(1);
        self.next_line_revision
    }
}

#[derive(Default)]
struct SnapshotDamage {
    dirty_rows: Vec<usize>,
    full_refresh: bool,
}

pub(super) fn visible_contents(term: &Term<TerminalEventListener>) -> String {
    let grid = term.grid();
    let mut contents = String::new();
    for row in 0..grid.screen_lines() {
        let line = Line(row as i32 - grid.display_offset() as i32);
        append_occupied_cells(
            &mut contents,
            grid,
            line,
            0,
            grid.columns().saturating_sub(1),
        );
        if !grid[line][Column(grid.columns().saturating_sub(1))]
            .flags
            .contains(Flags::WRAPLINE)
        {
            contents.push('\n');
        }
    }
    contents.truncate(contents.trim_end_matches('\n').len());
    contents
}

pub(super) fn append_occupied_cells(
    contents: &mut String,
    grid: &alacritty_terminal::Grid<Cell>,
    line: Line,
    start_column: usize,
    end_column: usize,
) {
    let Some(last_column) = occupied_end_column(grid, line, end_column) else {
        return;
    };
    if start_column > last_column {
        return;
    }
    for column in start_column..=last_column {
        let cell = &grid[line][Column(column)];
        if is_wide_continuation(cell) {
            continue;
        }
        append_cell_text(contents, cell);
    }
}

pub(super) fn occupied_end_column(
    grid: &alacritty_terminal::Grid<Cell>,
    line: Line,
    maximum: usize,
) -> Option<usize> {
    (0..=maximum)
        .rev()
        .find(|column| !is_default_blank(&grid[line][Column(*column)]))
}

fn is_default_blank(cell: &Cell) -> bool {
    cell.c == ' '
        && cell
            .zerowidth()
            .is_none_or(|characters| characters.is_empty())
        && cell.fg == Color::Named(NamedColor::Foreground)
        && cell.bg == Color::Named(NamedColor::Background)
        && cell.flags.is_empty()
}

fn terminal_color(color: Color) -> TerminalColor {
    match color {
        Color::Indexed(index) => TerminalColor::Indexed(index),
        Color::Spec(rgb) => TerminalColor::Rgb {
            red: rgb.r,
            green: rgb.g,
            blue: rgb.b,
        },
        Color::Named(named) if (named as usize) < 16 => TerminalColor::Indexed(named as u8),
        Color::Named(_) => TerminalColor::Default,
    }
}

fn terminal_style(cell: &Cell) -> TerminalStyle {
    TerminalStyle {
        foreground: terminal_color(cell.fg),
        background: terminal_color(cell.bg),
        bold: cell.flags.contains(Flags::BOLD),
        dim: cell.flags.contains(Flags::DIM),
        italic: cell.flags.contains(Flags::ITALIC),
        underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
        strikethrough: cell.flags.contains(Flags::STRIKEOUT),
        inverse: cell.flags.contains(Flags::INVERSE),
    }
}

pub(super) fn is_wide_continuation(cell: &Cell) -> bool {
    cell.flags
        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
}

pub(super) fn cursor_geometry(
    grid: &alacritty_terminal::Grid<Cell>,
    mut cursor: Point,
) -> (Point, usize) {
    let cell = &grid[cursor];
    if cell.flags.contains(Flags::WIDE_CHAR) {
        return (cursor, 2);
    }
    if cell.flags.contains(Flags::WIDE_CHAR_SPACER) && cursor.column.0 > 0 {
        cursor.column -= 1;
        return (cursor, 2);
    }
    if cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
        && cursor.line.0 + 1 < grid.screen_lines() as i32
    {
        let next_line = Line(cursor.line.0 + 1);
        if grid[next_line][Column(0)].flags.contains(Flags::WIDE_CHAR) {
            return (Point::new(next_line, Column(0)), 2);
        }
    }
    (cursor, 1)
}

pub(super) fn cell_text(cell: &Cell) -> String {
    let mut text = String::new();
    append_cell_text(&mut text, cell);
    text
}

pub(super) fn append_cell_text(text: &mut String, cell: &Cell) {
    if is_wide_continuation(cell) {
        return;
    }
    text.push(cell.c);
    for character in cell.zerowidth().into_iter().flatten() {
        text.push(*character);
    }
}

pub(super) fn cell_character_count(cell: &Cell) -> usize {
    if is_wide_continuation(cell) {
        return 0;
    }
    1usize.saturating_add(cell.zerowidth().map_or(0, |characters| characters.len()))
}

fn cell_contains_non_ascii(cell: &Cell) -> bool {
    !cell.c.is_ascii()
        || cell
            .zerowidth()
            .into_iter()
            .flatten()
            .any(|character| !character.is_ascii())
}

pub(super) fn styled_line(
    term: &Term<TerminalEventListener>,
    row: usize,
    columns: usize,
) -> TerminalStyledLine {
    let grid = term.grid();
    let line = Line(row as i32 - grid.display_offset() as i32);
    let mut runs = Vec::new();
    let mut column = 0;
    while column < columns {
        let cell = &grid[line][Column(column)];
        if is_wide_continuation(cell) {
            column += 1;
            continue;
        }
        let style = terminal_style(cell);
        let start_column = column;
        let is_wide = cell.flags.contains(Flags::WIDE_CHAR);
        let mut text = String::new();
        append_cell_text(&mut text, cell);
        let mut cells = if is_wide { 2 } else { 1 };
        column = column.saturating_add(cells).min(columns);

        if !is_wide {
            while column < columns {
                let next = &grid[line][Column(column)];
                if next.flags.contains(Flags::WIDE_CHAR)
                    || is_wide_continuation(next)
                    || terminal_style(next) != style
                    || cell_contains_non_ascii(cell)
                    || cell_contains_non_ascii(next)
                {
                    break;
                }
                append_cell_text(&mut text, next);
                cells += 1;
                column += 1;
            }
        }

        let invisible_default = text.chars().all(|character| character == ' ')
            && style.background == TerminalColor::Default
            && !style.underline
            && !style.strikethrough;
        if !invisible_default {
            runs.push(TerminalStyledRun {
                text,
                column: start_column,
                cells,
                style,
            });
        }
    }
    TerminalStyledLine { revision: 0, runs }
}
