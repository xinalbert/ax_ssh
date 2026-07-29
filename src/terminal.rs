//! Bounded terminal text model backed by the `vte` ANSI parser.

use std::collections::VecDeque;

use vte::{Params, Perform};

pub use self::input::{TerminalKey, TerminalModifiers, encode_key};

mod input;

const MIN_COLUMNS: usize = 20;
const MAX_COLUMNS: usize = 300;
const MIN_ROWS: usize = 5;
const MAX_ROWS: usize = 100;

pub struct TerminalModel {
    parser: vte::Parser,
    screen: TerminalScreen,
}

impl TerminalModel {
    pub fn new(columns: usize, rows: usize, scrollback_lines: usize) -> Self {
        Self {
            parser: vte::Parser::new(),
            screen: TerminalScreen::new(columns, rows, scrollback_lines),
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        let Self { parser, screen } = self;
        parser.advance(screen, bytes);
    }

    pub fn resize(&mut self, columns: usize, rows: usize) {
        self.screen.resize(columns, rows);
    }

    pub fn set_scrollback_lines(&mut self, scrollback_lines: usize) {
        self.screen.scrollback_lines = scrollback_lines;
        self.screen.enforce_capacity();
    }

    pub fn contents(&self) -> String {
        self.screen.contents()
    }
}

struct TerminalScreen {
    columns: usize,
    rows: usize,
    scrollback_lines: usize,
    lines: VecDeque<Vec<char>>,
    cursor_row: usize,
    cursor_column: usize,
}

impl TerminalScreen {
    fn new(columns: usize, rows: usize, scrollback_lines: usize) -> Self {
        let columns = columns.clamp(MIN_COLUMNS, MAX_COLUMNS);
        let rows = rows.clamp(MIN_ROWS, MAX_ROWS);
        let mut lines = VecDeque::new();
        lines.push_back(Vec::new());
        Self {
            columns,
            rows,
            scrollback_lines,
            lines,
            cursor_row: 0,
            cursor_column: 0,
        }
    }

    fn resize(&mut self, columns: usize, rows: usize) {
        self.columns = columns.clamp(MIN_COLUMNS, MAX_COLUMNS);
        self.rows = rows.clamp(MIN_ROWS, MAX_ROWS);
        for line in &mut self.lines {
            line.truncate(self.columns);
        }
        self.cursor_column = self.cursor_column.min(self.columns.saturating_sub(1));
        self.enforce_capacity();
    }

    fn contents(&self) -> String {
        let mut output = self
            .lines
            .iter()
            .map(|line| line.iter().collect::<String>().trim_end().to_owned())
            .collect::<Vec<_>>()
            .join("\n");
        while output.ends_with('\n') {
            output.pop();
        }
        output
    }

    fn write_char(&mut self, character: char) {
        if self.cursor_column >= self.columns {
            self.line_feed();
            self.cursor_column = 0;
        }
        self.ensure_cursor_line();
        let line = &mut self.lines[self.cursor_row];
        if line.len() < self.cursor_column {
            line.resize(self.cursor_column, ' ');
        }
        if self.cursor_column == line.len() {
            line.push(character);
        } else {
            line[self.cursor_column] = character;
        }
        self.cursor_column += 1;
    }

    fn line_feed(&mut self) {
        self.cursor_row += 1;
        self.ensure_cursor_line();
        self.enforce_capacity();
    }

    fn ensure_cursor_line(&mut self) {
        while self.lines.len() <= self.cursor_row {
            self.lines.push_back(Vec::new());
        }
    }

    fn enforce_capacity(&mut self) {
        let capacity = self.rows.saturating_add(self.scrollback_lines).max(1);
        while self.lines.len() > capacity {
            self.lines.pop_front();
            self.cursor_row = self.cursor_row.saturating_sub(1);
        }
    }

    fn move_cursor(&mut self, row: usize, column: usize) {
        self.cursor_row = row.min(self.lines.len().saturating_sub(1));
        self.cursor_column = column.min(self.columns.saturating_sub(1));
    }

    fn erase_line(&mut self, mode: u16) {
        self.ensure_cursor_line();
        let line = &mut self.lines[self.cursor_row];
        match mode {
            1 => {
                let end = self.cursor_column.min(line.len().saturating_sub(1));
                for character in line.iter_mut().take(end + 1) {
                    *character = ' ';
                }
            }
            2 => line.clear(),
            _ => line.truncate(self.cursor_column.min(line.len())),
        }
    }

    fn erase_display(&mut self, mode: u16) {
        match mode {
            2 | 3 => {
                self.lines.clear();
                self.lines.push_back(Vec::new());
                self.cursor_row = 0;
                self.cursor_column = 0;
            }
            1 => {
                for line in self.lines.iter_mut().take(self.cursor_row) {
                    line.clear();
                }
                self.erase_line(1);
            }
            _ => {
                self.erase_line(0);
                for line in self.lines.iter_mut().skip(self.cursor_row + 1) {
                    line.clear();
                }
            }
        }
    }
}

impl Perform for TerminalScreen {
    fn print(&mut self, character: char) {
        self.write_char(character);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' | 0x0B | 0x0C => self.line_feed(),
            b'\r' => self.cursor_column = 0,
            0x08 => self.cursor_column = self.cursor_column.saturating_sub(1),
            b'\t' => {
                let next_tab = ((self.cursor_column / 8) + 1) * 8;
                self.cursor_column = next_tab.min(self.columns.saturating_sub(1));
            }
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let values = params
            .iter()
            .map(|parameter| parameter.first().copied().unwrap_or(0))
            .collect::<Vec<_>>();
        let count = values
            .first()
            .copied()
            .filter(|value| *value > 0)
            .unwrap_or(1) as usize;
        match action {
            'A' => self.cursor_row = self.cursor_row.saturating_sub(count),
            'B' => self.move_cursor(self.cursor_row.saturating_add(count), self.cursor_column),
            'C' => {
                self.cursor_column = self
                    .cursor_column
                    .saturating_add(count)
                    .min(self.columns.saturating_sub(1));
            }
            'D' => self.cursor_column = self.cursor_column.saturating_sub(count),
            'G' => self.cursor_column = count.saturating_sub(1).min(self.columns - 1),
            'H' | 'f' => {
                let row = values.first().copied().unwrap_or(1).max(1) as usize - 1;
                let column = values.get(1).copied().unwrap_or(1).max(1) as usize - 1;
                self.move_cursor(row, column);
            }
            'J' => self.erase_display(values.first().copied().unwrap_or(0)),
            'K' => self.erase_line(values.first().copied().unwrap_or(0)),
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        if byte == b'c' {
            self.erase_display(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_colored_output_and_carriage_return_updates() {
        let mut terminal = TerminalModel::new(80, 24, 10);
        terminal.process(b"\x1b[32mready\x1b[0m\rbusy\r\nnext");
        assert_eq!(terminal.contents(), "busyy\nnext");
    }

    #[test]
    fn erase_sequences_update_visible_text() {
        let mut terminal = TerminalModel::new(80, 24, 10);
        terminal.process(b"first\nsecond\x1b[2K\rreplacement");
        assert_eq!(terminal.contents(), "first\nreplacement");
        terminal.process(b"\x1b[2Jdone");
        assert_eq!(terminal.contents(), "done");
    }

    #[test]
    fn scrollback_is_bounded() {
        let mut terminal = TerminalModel::new(80, 5, 2);
        for index in 0..12 {
            terminal.process(format!("line-{index}\n").as_bytes());
        }
        let contents = terminal.contents();
        assert!(!contents.contains("line-0"));
        assert!(contents.contains("line-11"));
        assert!(contents.lines().count() <= 7);
    }
}
