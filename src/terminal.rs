//! Bounded terminal grid backed by the `vt100` emulator.

pub use self::input::{TerminalKey, TerminalModifiers, encode_key};

mod input;

const MIN_COLUMNS: usize = 20;
const MAX_COLUMNS: usize = 300;
const MIN_ROWS: usize = 5;
const MAX_ROWS: usize = 100;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerminalColor {
    #[default]
    Default,
    Indexed(u8),
    Rgb {
        red: u8,
        green: u8,
        blue: u8,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalStyle {
    pub foreground: TerminalColor,
    pub background: TerminalColor,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub inverse: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalStyledRun {
    pub text: String,
    pub column: usize,
    pub cells: usize,
    pub style: TerminalStyle,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalStyledLine {
    pub runs: Vec<TerminalStyledRun>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSnapshot {
    pub text: String,
    pub lines: Vec<TerminalStyledLine>,
    pub max_columns: usize,
    pub cursor_row: usize,
    pub cursor_column: usize,
    pub cursor_visible: bool,
    pub cursor_text: String,
}

pub struct TerminalModel {
    parser: vt100::Parser,
    scrollback_lines: usize,
}

impl TerminalModel {
    pub fn new(columns: usize, rows: usize, scrollback_lines: usize) -> Self {
        let columns = clamped_dimension(columns, MIN_COLUMNS, MAX_COLUMNS);
        let rows = clamped_dimension(rows, MIN_ROWS, MAX_ROWS);
        Self {
            parser: vt100::Parser::new(rows, columns, scrollback_lines),
            scrollback_lines,
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    pub fn application_cursor(&self) -> bool {
        self.parser.screen().application_cursor()
    }

    pub fn resize(&mut self, columns: usize, rows: usize) {
        let columns = clamped_dimension(columns, MIN_COLUMNS, MAX_COLUMNS);
        let rows = clamped_dimension(rows, MIN_ROWS, MAX_ROWS);
        self.parser.screen_mut().set_size(rows, columns);
    }

    pub fn set_scrollback_lines(&mut self, scrollback_lines: usize) {
        if scrollback_lines == self.scrollback_lines {
            return;
        }

        self.parser.screen_mut().set_scrollback(0);
        let (rows, columns) = self.parser.screen().size();
        let visible_state = self.parser.screen().state_formatted();
        let mut parser = vt100::Parser::new(rows, columns, scrollback_lines);
        parser.process(&visible_state);
        self.parser = parser;
        self.scrollback_lines = scrollback_lines;
    }

    pub fn contents(&self) -> String {
        self.parser.screen().contents()
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        let screen = self.parser.screen();
        let (rows, columns) = screen.size();
        let (cursor_row, cursor_column) = screen.cursor_position();
        let cursor_visible = screen.scrollback() == 0 && !screen.hide_cursor();
        let cursor_text = cursor_visible
            .then(|| screen.cell(cursor_row, cursor_column))
            .flatten()
            .filter(|cell| !cell.is_wide_continuation())
            .map(|cell| cell.contents().to_owned())
            .filter(|contents| !contents.is_empty())
            .unwrap_or_else(|| " ".to_owned());

        TerminalSnapshot {
            text: screen.contents(),
            lines: (0..rows)
                .map(|row| styled_line(screen, row, columns))
                .collect(),
            max_columns: usize::from(columns),
            cursor_row: usize::from(cursor_row),
            cursor_column: usize::from(cursor_column),
            cursor_visible,
            cursor_text,
        }
    }

    /// Moves the visible terminal viewport. Positive values reveal older rows.
    pub fn scroll(&mut self, delta_lines: i32) -> bool {
        if self.parser.screen().alternate_screen() || delta_lines == 0 {
            return false;
        }
        let current = self.parser.screen().scrollback();
        let requested = if delta_lines > 0 {
            current.saturating_add(delta_lines as usize)
        } else {
            current.saturating_sub(delta_lines.unsigned_abs() as usize)
        };
        self.parser.screen_mut().set_scrollback(requested);
        self.parser.screen().scrollback() != current
    }

    pub fn scroll_to_bottom(&mut self) -> bool {
        let was_scrolled = self.parser.screen().scrollback() != 0;
        self.parser.screen_mut().set_scrollback(0);
        was_scrolled
    }

    /// Returns text for an inclusive, viewport-relative cell selection.
    pub fn selection_text(
        &self,
        anchor_row: usize,
        anchor_column: usize,
        focus_row: usize,
        focus_column: usize,
    ) -> String {
        let screen = self.parser.screen();
        let (rows, columns) = screen.size();
        let last_row = rows.saturating_sub(1);
        let last_column = columns.saturating_sub(1);
        let mut start = (
            usize_to_u16(anchor_row).min(last_row),
            usize_to_u16(anchor_column).min(last_column),
        );
        let mut end = (
            usize_to_u16(focus_row).min(last_row),
            usize_to_u16(focus_column).min(last_column),
        );
        if start > end {
            std::mem::swap(&mut start, &mut end);
        }
        screen.contents_between(
            start.0,
            start.1,
            end.0,
            end.1.saturating_add(1).min(columns),
        )
    }
}

fn clamped_dimension(value: usize, minimum: usize, maximum: usize) -> u16 {
    usize_to_u16(value.clamp(minimum, maximum))
}

fn usize_to_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

fn terminal_color(color: vt100::Color) -> TerminalColor {
    match color {
        vt100::Color::Default => TerminalColor::Default,
        vt100::Color::Idx(index) => TerminalColor::Indexed(index),
        vt100::Color::Rgb(red, green, blue) => TerminalColor::Rgb { red, green, blue },
    }
}

fn terminal_style(cell: &vt100::Cell) -> TerminalStyle {
    TerminalStyle {
        foreground: terminal_color(cell.fgcolor()),
        background: terminal_color(cell.bgcolor()),
        bold: cell.bold(),
        dim: cell.dim(),
        italic: cell.italic(),
        underline: cell.underline(),
        strikethrough: false,
        inverse: cell.inverse(),
    }
}

fn cell_text(cell: Option<&vt100::Cell>) -> String {
    match cell {
        Some(cell) if cell.is_wide_continuation() => String::new(),
        Some(cell) if cell.has_contents() => cell.contents().to_owned(),
        Some(_) | None => " ".to_owned(),
    }
}

fn styled_line(screen: &vt100::Screen, row: u16, columns: u16) -> TerminalStyledLine {
    let mut runs = Vec::new();
    let mut column = 0;
    while column < columns {
        let cell = screen.cell(row, column);
        if cell.is_some_and(vt100::Cell::is_wide_continuation) {
            column += 1;
            continue;
        }
        let style = cell.map(terminal_style).unwrap_or_default();
        let start_column = column;
        let is_wide = cell.is_some_and(vt100::Cell::is_wide);
        let mut text = cell_text(cell);
        let mut cells = if is_wide { 2 } else { 1 };
        column = column.saturating_add(cells).min(columns);

        if !is_wide {
            while column < columns {
                let next = screen.cell(row, column);
                if next.is_some_and(|cell| cell.is_wide() || cell.is_wide_continuation())
                    || next.map(terminal_style).unwrap_or_default() != style
                {
                    break;
                }
                text.push_str(&cell_text(next));
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
                column: usize::from(start_column),
                cells: usize::from(cells),
                style,
            });
        }
    }
    TerminalStyledLine { runs }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_colored_output_and_carriage_return_updates() {
        let mut terminal = TerminalModel::new(80, 24, 10);
        terminal.process(b"\x1b[32mready\x1b[0m\rbusy\r\nnext");
        let snapshot = terminal.snapshot();

        assert_eq!(snapshot.text, "busyy\nnext");
        assert_eq!((snapshot.cursor_row, snapshot.cursor_column), (1, 4));
        assert_eq!(snapshot.lines[0].runs.len(), 2);
        assert_eq!(snapshot.lines[0].runs[0].text, "busy");
        assert_eq!(snapshot.lines[0].runs[0].column, 0);
        assert_eq!(
            snapshot.lines[0].runs[1].style.foreground,
            TerminalColor::Indexed(2)
        );
    }

    #[test]
    fn parses_standard_extended_truecolor_and_attributes() {
        let mut terminal = TerminalModel::new(80, 24, 10);
        terminal.process(b"\x1b[1;3;4;31mred\x1b[22;23;24;38;5;208mindex\x1b[48;2;1;2;3;7mflip");
        let runs = terminal.snapshot().lines.remove(0).runs;

        assert_eq!(runs.len(), 3);
        assert!(runs[0].style.bold);
        assert!(runs[0].style.italic);
        assert!(runs[0].style.underline);
        assert_eq!(runs[0].style.foreground, TerminalColor::Indexed(1));
        assert_eq!(runs[1].style.foreground, TerminalColor::Indexed(208));
        assert!(!runs[1].style.bold);
        assert_eq!(
            runs[2].style.background,
            TerminalColor::Rgb {
                red: 1,
                green: 2,
                blue: 3,
            }
        );
        assert!(runs[2].style.inverse);
    }

    #[test]
    fn wide_characters_occupy_two_grid_cells() {
        let mut terminal = TerminalModel::new(80, 24, 10);
        terminal.process("A中B".as_bytes());
        let snapshot = terminal.snapshot();

        assert_eq!(snapshot.text, "A中B");
        assert_eq!(snapshot.lines[0].runs.len(), 3);
        assert_eq!(snapshot.lines[0].runs[1].text, "中");
        assert_eq!(snapshot.lines[0].runs[1].column, 1);
        assert_eq!(snapshot.lines[0].runs[1].cells, 2);
        assert_eq!(snapshot.lines[0].runs[2].column, 3);
        assert_eq!(snapshot.cursor_column, 4);
    }

    #[test]
    fn cursor_visibility_and_cell_text_follow_terminal_state() {
        let mut terminal = TerminalModel::new(80, 24, 10);
        terminal.process(b"prompt\x1b[2D");
        let snapshot = terminal.snapshot();
        assert!(snapshot.cursor_visible);
        assert_eq!(snapshot.cursor_text, "p");

        terminal.process(b"\x1b[?25l");
        assert!(!terminal.snapshot().cursor_visible);
    }

    #[test]
    fn tracks_application_cursor_mode() {
        let mut terminal = TerminalModel::new(80, 24, 10);
        assert!(!terminal.application_cursor());

        terminal.process(b"\x1b[?1h");
        assert!(terminal.application_cursor());

        terminal.process(b"\x1b[?1l");
        assert!(!terminal.application_cursor());
    }

    #[test]
    fn selection_uses_cell_coordinates_and_ignores_wide_continuations() {
        let mut terminal = TerminalModel::new(80, 24, 10);
        terminal.process("one\r\nt中ree".as_bytes());

        assert_eq!(terminal.selection_text(0, 1, 1, 2), "ne\nt中");
    }

    #[test]
    fn scrollback_is_bounded_and_scrollable() {
        let mut terminal = TerminalModel::new(80, 5, 2);
        for index in 0..12 {
            terminal.process(format!("line-{index}\r\n").as_bytes());
        }
        let live = terminal.contents();
        assert!(!live.contains("line-0"));
        assert!(live.contains("line-11"));
        assert!(terminal.scroll(10));
        assert!(terminal.contents().contains("line-8"));
        assert!(!terminal.snapshot().cursor_visible);
        assert!(terminal.scroll_to_bottom());
    }

    #[test]
    fn changing_scrollback_preserves_the_visible_grid() {
        let mut terminal = TerminalModel::new(80, 24, 10);
        terminal.process(b"prompt");
        terminal.set_scrollback_lines(20);

        assert_eq!(terminal.contents(), "prompt");
        assert_eq!(terminal.snapshot().cursor_column, 6);
    }
}
