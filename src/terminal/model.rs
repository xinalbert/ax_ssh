//! Terminal state, protocol handling, and viewport control.

use super::*;
use std::sync::mpsc::sync_channel;

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::term::{Config as TermConfig, TermMode};

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
            snapshot_lines: Vec::new(),
            snapshot_columns: 0,
            snapshot_display_offset: 0,
            next_line_revision: 0,
            viewport_detached: false,
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        let _ = self.process_with_responses(bytes);
    }

    /// Parse live output and return bounded protocol responses for the same transport.
    pub fn process_with_responses(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let was_alternate_screen = self.is_alternate_screen();
        self.processor.advance(&mut self.term, bytes);
        let is_alternate_screen = self.is_alternate_screen();
        if was_alternate_screen && !is_alternate_screen {
            self.term.scroll_display(Scroll::Bottom);
        }
        if is_alternate_screen
            || was_alternate_screen != is_alternate_screen
            || self.term.grid().display_offset() == 0
        {
            self.viewport_detached = false;
        }
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

    pub fn application_keypad(&self) -> bool {
        self.term.mode().contains(TermMode::APP_KEYPAD)
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

    pub fn mouse_button_reporting_active(&self) -> bool {
        self.mouse_reporting().enabled()
    }

    pub fn mouse_wheel_reporting_active(&self) -> bool {
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
                !is_wheel
                    && !matches!(event.button, TerminalMouseButton::None)
                    && (reporting.click || reporting.drag || reporting.motion)
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
        if matches!(event.kind, TerminalMouseEventKind::Release) && !reporting.sgr {
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

    pub fn resize(&mut self, columns: usize, rows: usize) -> bool {
        let dimensions = TerminalDimensions::from_size(TerminalSize::model(columns, rows));
        let grid = self.term.grid();
        if grid.columns() == dimensions.columns && grid.screen_lines() == dimensions.rows {
            return false;
        }
        self.term.resize(dimensions);
        true
    }

    /// Returns the model-normalized viewport size used by the terminal grid.
    pub fn size(&self) -> TerminalSize {
        let grid = self.term.grid();
        TerminalSize::model(grid.columns(), grid.screen_lines())
    }

    pub fn set_scrollback_lines(&mut self, scrollback_lines: usize) {
        if scrollback_lines == self.scrollback_lines {
            return;
        }

        self.term.set_options(terminal_config(scrollback_lines));
        self.scrollback_lines = scrollback_lines;
        if self.term.grid().display_offset() == 0 {
            self.viewport_detached = false;
        }
    }

    pub fn contents(&self) -> String {
        super::render::visible_contents(&self.term)
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
        if requested == 0 {
            self.viewport_detached = false;
        } else if requested > current {
            self.viewport_detached = true;
        }
        true
    }

    pub fn scroll_to_bottom(&mut self) -> bool {
        self.viewport_detached = false;
        if self.term.grid().display_offset() == 0 {
            return false;
        }
        self.term.scroll_display(Scroll::Bottom);
        true
    }

    pub fn viewport_mode(&self) -> TerminalViewportMode {
        if self.is_alternate_screen() {
            TerminalViewportMode::AlternateScreen
        } else if self.viewport_detached || self.term.grid().display_offset() > 0 {
            TerminalViewportMode::Detached
        } else {
            TerminalViewportMode::Follow
        }
    }

    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    fn is_alternate_screen(&self) -> bool {
        self.term.mode().contains(TermMode::ALT_SCREEN)
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
