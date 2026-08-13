//! Bounded terminal grid with primary-screen reflow on resize.

pub use self::input::{TerminalKey, TerminalModifiers, encode_key};

mod input;

use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor};

use crate::terminal_dimensions::TerminalSize;

const PROTOCOL_RESPONSE_CAPACITY: usize = 16;
const MAX_PROTOCOL_RESPONSE_BYTES: usize = 4 * 1024;

#[derive(Clone)]
struct TerminalEventListener {
    protocol_responses: SyncSender<Vec<u8>>,
}

impl EventListener for TerminalEventListener {
    fn send_event(&self, event: Event) {
        let Event::PtyWrite(response) = event else {
            return;
        };
        let response = response.into_bytes();
        if response.is_empty() || response.len() > MAX_PROTOCOL_RESPONSE_BYTES {
            return;
        }
        match self.protocol_responses.try_send(response) {
            Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }
}

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
    pub mouse_reporting: TerminalMouseReporting,
    pub mouse_reporting_active: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalMouseReporting {
    pub click: bool,
    pub drag: bool,
    pub motion: bool,
    pub sgr: bool,
    pub utf8: bool,
    pub alternate_scroll: bool,
}

impl TerminalMouseReporting {
    pub const fn enabled(self) -> bool {
        self.click || self.drag || self.motion
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalMouseButton {
    None,
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalMouseEventKind {
    Press,
    Release,
    Motion,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalMouseModifiers {
    pub shift: bool,
    pub alt: bool,
    pub control: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalMouseEvent {
    pub kind: TerminalMouseEventKind,
    pub button: TerminalMouseButton,
    pub column: usize,
    pub row: usize,
    pub modifiers: TerminalMouseModifiers,
}

pub struct TerminalModel {
    term: Term<TerminalEventListener>,
    processor: Processor,
    protocol_responses: Receiver<Vec<u8>>,
    scrollback_lines: usize,
}

impl TerminalModel {
    pub fn new(columns: usize, rows: usize, scrollback_lines: usize) -> Self {
        let dimensions = TerminalDimensions::new(columns, rows);
        let config = terminal_config(scrollback_lines);
        let (protocol_response_tx, protocol_responses) = sync_channel(PROTOCOL_RESPONSE_CAPACITY);
        Self {
            term: Term::new(
                config,
                &dimensions,
                TerminalEventListener {
                    protocol_responses: protocol_response_tx,
                },
            ),
            processor: Processor::new(),
            protocol_responses,
            scrollback_lines,
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        let _ = self.process_with_responses(bytes);
    }

    /// Parse live output and return bounded protocol responses for the same transport.
    pub fn process_with_responses(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.processor.advance(&mut self.term, bytes);
        self.protocol_responses.try_iter().collect()
    }

    /// Rebuild a bounded text-only view from a workspace snapshot.
    /// Process state, alternate-screen mode, and ANSI cursor state are not persisted.
    pub fn from_text(text: &str, columns: usize, rows: usize, scrollback_lines: usize) -> Self {
        let mut terminal = Self::new(columns, rows, scrollback_lines);
        terminal.process(text.as_bytes());
        terminal
    }

    pub fn application_cursor(&self) -> bool {
        self.term.mode().contains(TermMode::APP_CURSOR)
    }

    pub fn mouse_reporting(&self) -> TerminalMouseReporting {
        let mode = self.term.mode();
        TerminalMouseReporting {
            click: mode.contains(TermMode::MOUSE_REPORT_CLICK),
            drag: mode.contains(TermMode::MOUSE_DRAG),
            motion: mode.contains(TermMode::MOUSE_MOTION),
            sgr: mode.contains(TermMode::SGR_MOUSE),
            utf8: mode.contains(TermMode::UTF8_MOUSE),
            alternate_scroll: mode.contains(TermMode::ALTERNATE_SCROLL),
        }
    }

    pub fn mouse_reporting_active(&self) -> bool {
        let reporting = self.mouse_reporting();
        reporting.enabled()
            || (reporting.alternate_scroll && self.term.mode().contains(TermMode::ALT_SCREEN))
    }

    /// Encode one bounded terminal mouse event according to the active private modes.
    pub fn encode_mouse_event(&self, event: TerminalMouseEvent) -> Option<Vec<u8>> {
        let reporting = self.mouse_reporting();
        let is_wheel = matches!(
            event.button,
            TerminalMouseButton::WheelUp | TerminalMouseButton::WheelDown
        );
        let alternate_scroll = reporting.alternate_scroll
            && self.term.mode().contains(TermMode::ALT_SCREEN)
            && !reporting.enabled();
        let allowed = match event.kind {
            TerminalMouseEventKind::Press => {
                !matches!(event.button, TerminalMouseButton::None)
                    && (is_wheel || reporting.click || reporting.drag || reporting.motion)
            }
            TerminalMouseEventKind::Release => {
                !is_wheel && (reporting.click || reporting.drag || reporting.motion)
            }
            TerminalMouseEventKind::Motion => {
                (reporting.motion
                    || (reporting.drag && !matches!(event.button, TerminalMouseButton::None)))
                    && !is_wheel
            }
        };
        if !reporting.enabled()
            && !(reporting.alternate_scroll && self.term.mode().contains(TermMode::ALT_SCREEN))
        {
            return None;
        }
        if !allowed {
            return None;
        }
        if alternate_scroll && is_wheel {
            let application_cursor = self.term.mode().contains(TermMode::APP_CURSOR);
            let direction = match event.button {
                TerminalMouseButton::WheelUp => b'A',
                TerminalMouseButton::WheelDown => b'B',
                _ => return None,
            };
            return Some(vec![
                0x1b,
                if application_cursor { b'O' } else { b'[' },
                direction,
            ]);
        }
        let columns = self.term.grid().columns();
        let rows = self.term.grid().screen_lines();
        if columns == 0 || rows == 0 {
            return None;
        }
        let column = event.column.min(columns - 1) + 1;
        let row = event.row.min(rows - 1) + 1;
        let mut code = match event.button {
            TerminalMouseButton::None => 3,
            TerminalMouseButton::Left => 0,
            TerminalMouseButton::Middle => 1,
            TerminalMouseButton::Right => 2,
            TerminalMouseButton::WheelUp => 64,
            TerminalMouseButton::WheelDown => 65,
        };
        if matches!(event.kind, TerminalMouseEventKind::Release) {
            code = 3;
        } else if matches!(event.kind, TerminalMouseEventKind::Motion) {
            code |= 32;
        }
        if event.modifiers.shift {
            code |= 4;
        }
        if event.modifiers.alt {
            code |= 8;
        }
        if event.modifiers.control {
            code |= 16;
        }
        if reporting.sgr {
            let suffix = if matches!(event.kind, TerminalMouseEventKind::Release) {
                'm'
            } else {
                'M'
            };
            return Some(format!("\x1b[<{};{};{}{}", code, column, row, suffix).into_bytes());
        }
        let encode = |value: usize| -> Option<Vec<u8>> {
            let value = value + 32;
            if reporting.utf8 {
                let mut output = String::new();
                char::from_u32(value as u32).map(|ch| {
                    output.push(ch);
                    output.into_bytes()
                })
            } else {
                (value <= u8::MAX as usize).then_some(vec![value as u8])
            }
        };
        let mut output = vec![0x1b, b'[', b'M'];
        output.extend(encode(code)?);
        output.extend(encode(column)?);
        output.extend(encode(row)?);
        Some(output)
    }

    pub fn resize(&mut self, columns: usize, rows: usize) {
        let dimensions = TerminalDimensions::from_size(TerminalSize::model(columns, rows));
        self.term.resize(dimensions);
    }

    pub fn set_scrollback_lines(&mut self, scrollback_lines: usize) {
        if scrollback_lines == self.scrollback_lines {
            return;
        }

        self.term.set_options(terminal_config(scrollback_lines));
        self.scrollback_lines = scrollback_lines;
    }

    pub fn contents(&self) -> String {
        visible_contents(&self.term)
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        let grid = self.term.grid();
        let rows = grid.screen_lines();
        let columns = grid.columns();
        let cursor = grid.cursor.point;
        let cursor_visible =
            grid.display_offset() == 0 && self.term.mode().contains(TermMode::SHOW_CURSOR);
        let cursor_text = cursor_visible
            .then(|| &grid[cursor])
            .filter(|cell| !is_wide_continuation(cell))
            .map(cell_text)
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| " ".to_owned());

        TerminalSnapshot {
            text: visible_contents(&self.term),
            lines: (0..rows)
                .map(|row| styled_line(&self.term, row, columns))
                .collect(),
            max_columns: columns,
            cursor_row: cursor.line.0.max(0) as usize,
            cursor_column: cursor.column.0,
            cursor_visible,
            cursor_text,
            mouse_reporting: self.mouse_reporting(),
            mouse_reporting_active: self.mouse_reporting_active(),
        }
    }

    /// Moves the visible terminal viewport. Positive values reveal older rows.
    pub fn scroll(&mut self, delta_lines: i32) -> bool {
        if self.term.mode().contains(TermMode::ALT_SCREEN) || delta_lines == 0 {
            return false;
        }

        let (current, requested) = {
            let grid = self.term.grid();
            let history_size = grid.total_lines().saturating_sub(grid.screen_lines());
            let current = grid.display_offset();
            let requested = if delta_lines > 0 {
                current
                    .saturating_add(delta_lines as usize)
                    .min(history_size)
            } else {
                current.saturating_sub(delta_lines.unsigned_abs() as usize)
            };
            (current, requested)
        };
        if requested == current {
            return false;
        }
        self.term
            .scroll_display(Scroll::Delta(requested as i32 - current as i32));
        true
    }

    pub fn scroll_to_bottom(&mut self) -> bool {
        if self.term.grid().display_offset() == 0 {
            return false;
        }
        self.term.scroll_display(Scroll::Bottom);
        true
    }

    /// Returns text for an inclusive, viewport-relative cell selection.
    pub fn selection_text(
        &self,
        anchor_row: usize,
        anchor_column: usize,
        focus_row: usize,
        focus_column: usize,
    ) -> String {
        let grid = self.term.grid();
        let last_row = grid.screen_lines().saturating_sub(1);
        let last_column = grid.columns().saturating_sub(1);
        let mut start = (anchor_row.min(last_row), anchor_column.min(last_column));
        let mut end = (focus_row.min(last_row), focus_column.min(last_column));
        if start > end {
            std::mem::swap(&mut start, &mut end);
        }

        let display_offset = grid.display_offset() as i32;
        let mut contents = String::new();
        for row in start.0..=end.0 {
            let line = Line(row as i32 - display_offset);
            let start_column = if row == start.0 { start.1 } else { 0 };
            let end_column = if row == end.0 { end.1 } else { last_column };
            append_occupied_cells(&mut contents, grid, line, start_column, end_column);
            if row != end.0
                && !grid[line][Column(last_column)]
                    .flags
                    .contains(Flags::WRAPLINE)
            {
                contents.push('\n');
            }
        }
        contents
    }

    /// Returns a bounded visible row and the text position at a terminal cell.
    ///
    /// Terminal columns differ from Unicode character positions when a row
    /// contains a wide character. The returned index preserves that mapping
    /// for short-lived pointer intents without exposing the terminal grid.
    pub fn visible_row_text_at_cell(&self, row: usize, column: usize) -> Option<(String, usize)> {
        let grid = self.term.grid();
        if row >= grid.screen_lines() || column >= grid.columns() {
            return None;
        }
        let line = Line(row as i32 - grid.display_offset() as i32);
        let last_column = occupied_end_column(grid, line, grid.columns().saturating_sub(1))?;
        if column > last_column {
            return None;
        }
        let mut contents = String::new();
        let mut target_character = None;
        for cell_column in 0..=last_column {
            let cell = &grid[line][Column(cell_column)];
            if is_wide_continuation(cell) {
                if cell_column == column {
                    return None;
                }
                continue;
            }
            if cell_column == column {
                target_character = Some(contents.chars().count());
            }
            contents.push_str(&cell_text(cell));
        }
        target_character.map(|target_character| (contents, target_character))
    }

    /// Converts a bounded character range in a visible row back to terminal cells.
    pub fn visible_row_cell_span_for_characters(
        &self,
        row: usize,
        start: usize,
        end: usize,
    ) -> Option<(usize, usize)> {
        if start >= end {
            return None;
        }
        let grid = self.term.grid();
        if row >= grid.screen_lines() {
            return None;
        }
        let line = Line(row as i32 - grid.display_offset() as i32);
        let last_column = occupied_end_column(grid, line, grid.columns().saturating_sub(1))?;
        let mut character = 0usize;
        let mut start_column = None;
        let mut end_column = None;
        for cell_column in 0..=last_column {
            let cell = &grid[line][Column(cell_column)];
            if is_wide_continuation(cell) {
                continue;
            }
            let cell_characters = cell_text(cell).chars().count();
            let next_character = character.saturating_add(cell_characters);
            if start_column.is_none() && start >= character && start < next_character {
                start_column = Some(cell_column);
            }
            if end > character && end <= next_character {
                end_column = Some(
                    cell_column
                        .saturating_add(1)
                        .saturating_add(usize::from(cell.flags.contains(Flags::WIDE_CHAR))),
                );
                break;
            }
            character = next_character;
        }
        Some((start_column?, end_column?))
    }
}

#[derive(Clone, Copy)]
struct TerminalDimensions {
    columns: usize,
    rows: usize,
}

impl TerminalDimensions {
    fn new(columns: usize, rows: usize) -> Self {
        Self::from_size(TerminalSize::model(columns, rows))
    }

    fn from_size(size: TerminalSize) -> Self {
        Self {
            columns: size.columns() as usize,
            rows: size.rows() as usize,
        }
    }
}

impl Dimensions for TerminalDimensions {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

fn terminal_config(scrollback_lines: usize) -> TermConfig {
    TermConfig {
        scrolling_history: scrollback_lines,
        ..TermConfig::default()
    }
}

fn visible_contents(term: &Term<TerminalEventListener>) -> String {
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

fn append_occupied_cells(
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
        contents.push_str(&cell_text(cell));
    }
}

fn occupied_end_column(
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

fn is_wide_continuation(cell: &Cell) -> bool {
    cell.flags
        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
}

fn cell_text(cell: &Cell) -> String {
    if is_wide_continuation(cell) {
        return String::new();
    }
    let mut text = String::new();
    text.push(cell.c);
    for character in cell.zerowidth().into_iter().flatten() {
        text.push(*character);
    }
    text
}

fn styled_line(
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
        let mut text = cell_text(cell);
        let mut cells = if is_wide { 2 } else { 1 };
        column = column.saturating_add(cells).min(columns);

        if !is_wide {
            while column < columns {
                let next = &grid[line][Column(column)];
                if next.flags.contains(Flags::WIDE_CHAR)
                    || is_wide_continuation(next)
                    || terminal_style(next) != style
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
                column: start_column,
                cells,
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
    fn terminal_protocol_queries_return_bounded_transport_responses() {
        let mut terminal = TerminalModel::new(80, 24, 10);

        assert_eq!(
            terminal.process_with_responses(b"\x1b[6n"),
            vec![b"\x1b[1;1R".to_vec()]
        );

        let repeated_query = b"\x1b[5n".repeat(PROTOCOL_RESPONSE_CAPACITY + 4);
        let responses = terminal.process_with_responses(&repeated_query);
        assert_eq!(responses.len(), PROTOCOL_RESPONSE_CAPACITY);
        assert!(responses.iter().all(|response| response == b"\x1b[0n"));
        assert_eq!(
            terminal.process_with_responses(b"\x1b[5n"),
            vec![b"\x1b[0n".to_vec()]
        );
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
    fn visible_row_target_text_preserves_cell_columns_after_wide_characters() {
        let mut terminal = TerminalModel::new(80, 3, 10);
        terminal.process("\u{4e2d} https://example.test".as_bytes());

        assert_eq!(
            terminal.visible_row_text_at_cell(0, 3),
            Some(("\u{4e2d} https://example.test".to_owned(), 2))
        );
        assert_eq!(terminal.visible_row_text_at_cell(0, 1), None);
    }

    #[test]
    fn visible_row_cell_span_maps_target_characters_after_a_wide_prefix() {
        let mut terminal = TerminalModel::new(80, 3, 10);
        terminal.process("中 https://example.test".as_bytes());

        assert_eq!(
            terminal.visible_row_cell_span_for_characters(0, 2, 22),
            Some((3, 23))
        );
    }

    #[test]
    fn terminal_grid_clamps_to_the_small_screen_floor() {
        let mut terminal = TerminalModel::new(1, 1, 10);
        let snapshot = terminal.snapshot();
        assert_eq!(
            (snapshot.max_columns, snapshot.lines.len()),
            (
                usize::from(crate::terminal_dimensions::MIN_TERMINAL_COLUMNS),
                usize::from(crate::terminal_dimensions::MIN_TERMINAL_ROWS),
            )
        );

        terminal.resize(0, 0);
        let snapshot = terminal.snapshot();
        assert_eq!(
            (snapshot.max_columns, snapshot.lines.len()),
            (
                usize::from(crate::terminal_dimensions::MIN_TERMINAL_COLUMNS),
                usize::from(crate::terminal_dimensions::MIN_TERMINAL_ROWS),
            )
        );
    }

    #[test]
    fn growing_a_primary_terminal_without_scrollback_keeps_content_at_the_top() {
        let mut terminal = TerminalModel::new(10, 3, 10);
        terminal.process(b"one\r\ntwo\r\nthree");
        assert_eq!(terminal.snapshot().cursor_row, 2);

        terminal.resize(10, 5);
        let snapshot = terminal.snapshot();
        assert_eq!(snapshot.cursor_row, 2);
        assert_eq!(snapshot_line_text(&snapshot, 0), "one");
        assert_eq!(snapshot_line_text(&snapshot, 1), "two");
        assert_eq!(snapshot_line_text(&snapshot, 2), "three");
        assert_eq!(snapshot_line_text(&snapshot, 3), "");
        assert_eq!(snapshot_line_text(&snapshot, 4), "");

        terminal.process(b"!");
        assert_eq!(snapshot_line_text(&terminal.snapshot(), 2), "three!");
    }

    #[test]
    fn growing_a_primary_terminal_away_from_the_bottom_keeps_its_cursor_row() {
        let mut terminal = TerminalModel::new(10, 3, 10);
        terminal.process(b"one\r\ntwo");
        assert_eq!(terminal.snapshot().cursor_row, 1);

        terminal.resize(10, 5);
        let snapshot = terminal.snapshot();
        assert_eq!(snapshot.cursor_row, 1);
        assert_eq!(snapshot_line_text(&snapshot, 0), "one");
        assert_eq!(snapshot_line_text(&snapshot, 1), "two");
    }

    #[test]
    fn growing_an_alternate_terminal_keeps_standard_resize_behavior() {
        let mut terminal = TerminalModel::new(10, 3, 10);
        terminal.process(b"\x1b[?1049hone\r\ntwo\r\nthree");
        assert_eq!(terminal.snapshot().cursor_row, 2);

        terminal.resize(10, 5);
        let snapshot = terminal.snapshot();
        assert_eq!(snapshot.cursor_row, 2);
        assert_eq!(snapshot_line_text(&snapshot, 0), "one");
        assert_eq!(snapshot_line_text(&snapshot, 2), "three");
    }

    #[test]
    fn growing_a_live_terminal_restores_recent_scrollback_above_the_viewport() {
        let mut terminal = TerminalModel::new(10, 3, 10);
        terminal.process(b"one\r\ntwo\r\nthree\r\nfour");

        terminal.resize(10, 4);
        let snapshot = terminal.snapshot();
        assert_eq!(snapshot.cursor_row, 3);
        assert_eq!(snapshot_line_text(&snapshot, 0), "one");
        assert_eq!(snapshot_line_text(&snapshot, 1), "two");
        assert_eq!(snapshot_line_text(&snapshot, 3), "four");
    }

    #[test]
    fn repeated_primary_terminal_resizes_do_not_create_top_blank_rows() {
        let mut terminal = TerminalModel::new(10, 3, 10);
        terminal.process(b"one\r\ntwo\r\nthree");

        terminal.resize(10, 5);
        terminal.resize(10, 3);
        let snapshot = terminal.snapshot();
        assert_eq!(snapshot.cursor_row, 2);
        assert_eq!(snapshot_line_text(&snapshot, 0), "one");
        assert_eq!(snapshot_line_text(&snapshot, 1), "two");
        assert_eq!(snapshot_line_text(&snapshot, 2), "three");

        terminal.resize(10, 5);
        let snapshot = terminal.snapshot();
        assert_eq!(snapshot.cursor_row, 2);
        assert_eq!(snapshot_line_text(&snapshot, 0), "one");
        assert_eq!(snapshot_line_text(&snapshot, 1), "two");
        assert_eq!(snapshot_line_text(&snapshot, 2), "three");
        assert_eq!(snapshot_line_text(&snapshot, 3), "");
        assert_eq!(snapshot_line_text(&snapshot, 4), "");
    }

    #[test]
    fn growing_while_viewing_scrollback_keeps_the_standard_cursor_position() {
        let mut terminal = TerminalModel::new(10, 3, 10);
        terminal.process(b"one\r\ntwo\r\nthree\r\nfour");
        assert!(terminal.scroll(1));

        terminal.resize(10, 5);
        let snapshot = terminal.snapshot();
        assert_eq!(snapshot.cursor_row, 3);
        assert_eq!(snapshot_line_text(&snapshot, 0), "one");
        assert_eq!(snapshot_line_text(&snapshot, 3), "four");
    }

    #[test]
    fn narrowing_and_widening_a_primary_terminal_reflows_soft_wrapped_content() {
        let mut terminal = TerminalModel::new(20, 5, 20);
        terminal.process(b"\x1b[5;1H0123456789abcdefghij");

        terminal.resize(10, 5);
        let narrow = terminal.snapshot();
        assert_eq!(narrow.cursor_row, 4);
        assert_eq!(snapshot_line_text(&narrow, 3), "0123456789");
        assert_eq!(snapshot_line_text(&narrow, 4), "abcdefghij");

        terminal.resize(20, 5);
        let wide = terminal.snapshot();
        assert_eq!(wide.cursor_row, 4);
        assert_eq!(snapshot_line_text(&wide, 4), "0123456789abcdefghij");
    }

    #[test]
    fn primary_reflow_preserves_hard_breaks_and_wide_characters() {
        let mut terminal = TerminalModel::new(20, 5, 20);
        terminal.process("first中line\r\nsecond中line".as_bytes());

        terminal.resize(10, 5);
        terminal.resize(20, 5);

        assert_eq!(terminal.snapshot().text, "first中line\nsecond中line");
    }

    #[test]
    fn alternate_screen_resize_does_not_reflow_existing_rows() {
        let mut terminal = TerminalModel::new(20, 5, 20);
        terminal.process(b"\x1b[?1049h0123456789abcdefghij");

        terminal.resize(10, 5);
        terminal.resize(20, 5);

        assert_ne!(terminal.snapshot().text, "0123456789abcdefghij");
    }

    #[test]
    fn repeated_resize_preserves_hard_break_columns() {
        let mut terminal = TerminalModel::new(80, 24, 100);
        terminal.process(
            b"2026-08-13 21:33:54\r\n$:\r\nzhushixin@compute-0-0 :\r\n~\r\n2026-08-13 21:33:54\r\n$:\r\nzhushixin@compute-0-0 :\r\n~\r\n",
        );
        for (columns, rows) in [(160, 40), (200, 50), (120, 30), (180, 45), (80, 24)] {
            terminal.resize(columns, rows);
            let snapshot = terminal.snapshot();
            let occupied_columns = snapshot
                .lines
                .iter()
                .filter_map(|line| line.runs.first())
                .map(|run| run.column)
                .collect::<Vec<_>>();
            assert_eq!(occupied_columns, vec![0; 8], "resize {columns}x{rows}");
        }
    }

    fn snapshot_line_text(snapshot: &TerminalSnapshot, row: usize) -> String {
        let text = snapshot.lines[row]
            .runs
            .iter()
            .map(|run| run.text.as_str())
            .collect::<String>();
        text.trim_end().to_owned()
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
    fn encodes_sgr_click_release_wheel_drag_and_modifiers() {
        let mut terminal = TerminalModel::new(80, 24, 10);
        terminal.process(b"\x1b[?1000h\x1b[?1006h");
        assert_eq!(
            terminal.encode_mouse_event(TerminalMouseEvent {
                kind: TerminalMouseEventKind::Press,
                button: TerminalMouseButton::Left,
                column: 2,
                row: 3,
                modifiers: TerminalMouseModifiers {
                    shift: true,
                    alt: false,
                    control: true
                },
            }),
            Some(b"\x1b[<20;3;4M".to_vec())
        );
        assert_eq!(
            terminal.encode_mouse_event(TerminalMouseEvent {
                kind: TerminalMouseEventKind::Release,
                button: TerminalMouseButton::Left,
                column: 2,
                row: 3,
                modifiers: TerminalMouseModifiers::default(),
            }),
            Some(b"\x1b[<3;3;4m".to_vec())
        );
        assert_eq!(
            terminal.encode_mouse_event(TerminalMouseEvent {
                kind: TerminalMouseEventKind::Press,
                button: TerminalMouseButton::WheelDown,
                column: 0,
                row: 0,
                modifiers: TerminalMouseModifiers::default(),
            }),
            Some(b"\x1b[<65;1;1M".to_vec())
        );
    }

    #[test]
    fn encodes_x10_and_utf8_coordinates_with_bounds() {
        let mut terminal = TerminalModel::new(300, 100, 10);
        terminal.process(b"\x1b[?1000h");
        let event = TerminalMouseEvent {
            kind: TerminalMouseEventKind::Press,
            button: TerminalMouseButton::Right,
            column: 299,
            row: 99,
            modifiers: TerminalMouseModifiers::default(),
        };
        assert_eq!(terminal.encode_mouse_event(event), None);
        terminal.process(b"\x1b[?1005h");
        assert_eq!(
            terminal.encode_mouse_event(event),
            Some(vec![27, 91, 77, 34, 197, 140, 194, 132])
        );
    }

    #[test]
    fn reports_drag_and_motion_only_when_enabled() {
        let mut terminal = TerminalModel::new(80, 24, 10);
        let event = TerminalMouseEvent {
            kind: TerminalMouseEventKind::Motion,
            button: TerminalMouseButton::Left,
            column: 1,
            row: 1,
            modifiers: TerminalMouseModifiers::default(),
        };
        assert_eq!(terminal.encode_mouse_event(event), None);
        terminal.process(b"\x1b[?1002h");
        assert!(terminal.encode_mouse_event(event).is_some());
        terminal.process(b"\x1b[?1002l\x1b[?1003h");
        assert!(terminal.encode_mouse_event(event).is_some());
    }

    #[test]
    fn alternate_screen_scroll_uses_application_cursor_sequences() {
        let mut terminal = TerminalModel::new(80, 24, 10);
        terminal.process(b"\x1b[?1049h\x1b[?1007h");
        assert_eq!(
            terminal.encode_mouse_event(TerminalMouseEvent {
                kind: TerminalMouseEventKind::Press,
                button: TerminalMouseButton::WheelUp,
                column: 0,
                row: 0,
                modifiers: TerminalMouseModifiers::default(),
            }),
            Some(b"\x1b[A".to_vec())
        );
        terminal.process(b"\x1b[?1h");
        assert_eq!(
            terminal.encode_mouse_event(TerminalMouseEvent {
                kind: TerminalMouseEventKind::Press,
                button: TerminalMouseButton::WheelDown,
                column: 0,
                row: 0,
                modifiers: TerminalMouseModifiers::default(),
            }),
            Some(b"\x1bOB".to_vec())
        );
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
