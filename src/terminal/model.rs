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
            output_frame_hold: OutputFrameHold::default(),
            mouse_encoding: MouseEncodingTracker::default(),
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        let _ = self.process_with_responses(bytes);
    }

    /// Parse live output and return bounded protocol responses for the same transport.
    pub fn process_with_responses(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.output_frame_hold
            .observe(bytes, std::time::Instant::now());
        self.mouse_encoding.observe(bytes);
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

    /// Returns the remaining presentation hold for a cursor-hidden redraw.
    ///
    /// The hold is intentionally short and is released even if a program does
    /// not send the matching cursor-show sequence.
    pub fn output_frame_hold_remaining(&mut self) -> Option<std::time::Duration> {
        self.output_frame_hold.remaining(std::time::Instant::now())
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

    pub fn encode_paste(&self, text: &str) -> Option<Vec<u8>> {
        super::input::encode_paste(text, self.term.mode().contains(TermMode::BRACKETED_PASTE))
    }

    /// Returns whether the active terminal application requested xterm focus reports.
    pub fn focus_reporting_active(&self) -> bool {
        self.term.mode().contains(TermMode::FOCUS_IN_OUT)
    }

    /// Encodes one xterm FocusIn or FocusOut report when that mode is active.
    pub fn encode_focus_event(&self, focused: bool) -> Option<Vec<u8>> {
        self.focus_reporting_active()
            .then(|| (if focused { b"\x1b[I" } else { b"\x1b[O" }).to_vec())
    }

    pub fn mouse_reporting(&self) -> TerminalMouseReporting {
        let mode = self.term.mode();
        TerminalMouseReporting {
            click: mode.contains(TermMode::MOUSE_REPORT_CLICK),
            drag: mode.contains(TermMode::MOUSE_DRAG),
            motion: mode.contains(TermMode::MOUSE_MOTION),
            sgr: mode.contains(TermMode::SGR_MOUSE) && !self.mouse_encoding.suppresses_reports(),
            alternate_scroll: mode.contains(TermMode::ALTERNATE_SCROLL),
        }
    }

    pub fn mouse_button_reporting_active(&self) -> bool {
        self.mouse_reporting().enabled() && !self.mouse_encoding.suppresses_reports()
    }

    pub fn mouse_wheel_reporting_active(&self) -> bool {
        let reporting = self.mouse_reporting();
        (reporting.enabled() && !self.mouse_encoding.suppresses_reports())
            || (reporting.alternate_scroll && self.term.mode().contains(TermMode::ALT_SCREEN))
    }

    /// Encode one bounded terminal mouse event according to the active private modes.
    pub fn encode_mouse_event(&self, event: TerminalMouseEvent) -> Option<Vec<u8>> {
        if self.mouse_encoding.suppresses_reports() {
            return None;
        }
        let reporting = self.mouse_reporting();
        let is_wheel = matches!(
            event.button,
            TerminalMouseButton::WheelUp
                | TerminalMouseButton::WheelDown
                | TerminalMouseButton::WheelLeft
                | TerminalMouseButton::WheelRight
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
            TerminalMouseButton::WheelLeft => 66,
            TerminalMouseButton::WheelRight => 67,
            TerminalMouseButton::Auxiliary8 => 128,
            TerminalMouseButton::Auxiliary9 => 129,
            TerminalMouseButton::Auxiliary10 => 130,
            TerminalMouseButton::Auxiliary11 => 131,
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
            (value <= u8::MAX as usize).then_some(vec![value as u8])
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
        let previous_display_offset = grid.display_offset();
        let preserve_viewport =
            !self.is_alternate_screen() && (self.viewport_detached || previous_display_offset > 0);
        self.term.resize(dimensions);
        if preserve_viewport {
            let grid = self.term.grid();
            let target_display_offset =
                previous_display_offset.min(grid.total_lines().saturating_sub(grid.screen_lines()));
            let current_display_offset = grid.display_offset();
            let target = target_display_offset.min(i32::MAX as usize) as i32;
            let current = current_display_offset.min(i32::MAX as usize) as i32;
            if target != current {
                self.term.scroll_display(Scroll::Delta(target - current));
            }
            self.viewport_detached = target_display_offset > 0;
        } else if self.term.grid().display_offset() == 0 {
            self.viewport_detached = false;
        }
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

impl OutputFrameHold {
    pub(super) fn observe(&mut self, bytes: &[u8], now: std::time::Instant) {
        self.expire(now);
        for &byte in bytes {
            match self.cursor_visibility_prefix {
                0 if byte == 0x1b => self.cursor_visibility_prefix = 1,
                1 if byte == b'[' => self.cursor_visibility_prefix = 2,
                2 if byte == b'?' => self.cursor_visibility_prefix = 3,
                3 if byte == b'2' => self.cursor_visibility_prefix = 4,
                4 if byte == b'5' => self.cursor_visibility_prefix = 5,
                5 if byte == b'l' => {
                    self.active = true;
                    self.started_at = Some(now);
                    self.cursor_visibility_prefix = 0;
                }
                5 if byte == b'h' => {
                    self.active = false;
                    self.started_at = None;
                    self.cursor_visibility_prefix = 0;
                }
                _ => self.cursor_visibility_prefix = u8::from(byte == 0x1b),
            }
        }
    }

    pub(super) fn remaining(&mut self, now: std::time::Instant) -> Option<std::time::Duration> {
        self.expire(now);
        if !self.active {
            return None;
        }
        self.started_at.map(|started_at| {
            OUTPUT_FRAME_HOLD_MAX.saturating_sub(now.saturating_duration_since(started_at))
        })
    }

    fn expire(&mut self, now: std::time::Instant) {
        if self.started_at.is_some_and(|started_at| {
            now.saturating_duration_since(started_at) >= OUTPUT_FRAME_HOLD_MAX
        }) {
            self.active = false;
            self.started_at = None;
        }
    }
}

impl MouseEncodingTracker {
    fn suppresses_reports(&self) -> bool {
        self.utf8_coordinates || self.urxvt_coordinates || self.pixel_coordinates
    }

    fn observe(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            match self.parser_state {
                0 if byte == 0x1b => self.parser_state = 1,
                1 if byte == b'[' => self.parser_state = 2,
                1 if byte == b'c' => {
                    self.utf8_coordinates = false;
                    self.urxvt_coordinates = false;
                    self.pixel_coordinates = false;
                    self.parser_state = 0;
                }
                2 if byte == b'?' => {
                    self.parser_state = 3;
                    self.parameter = 0;
                    self.has_parameter = false;
                }
                3 if byte.is_ascii_digit() => {
                    self.parameter = self
                        .parameter
                        .saturating_mul(10)
                        .saturating_add(u16::from(byte - b'0'));
                    self.has_parameter = true;
                }
                3 if byte == b';' => self.finish_parameter(None),
                3 if matches!(byte, b'h' | b'l') => {
                    self.finish_parameter(Some(byte == b'h'));
                    self.parser_state = 0;
                }
                _ => self.parser_state = u8::from(byte == 0x1b),
            }
        }
    }

    fn finish_parameter(&mut self, enabled: Option<bool>) {
        let parameter = self.has_parameter.then_some(self.parameter);
        self.parameter = 0;
        self.has_parameter = false;
        let Some(parameter) = parameter else {
            return;
        };
        match (parameter, enabled) {
            (1005, Some(true)) => self.utf8_coordinates = true,
            (1005, Some(false)) => self.utf8_coordinates = false,
            (1015, Some(true)) => self.urxvt_coordinates = true,
            (1015, Some(false)) => self.urxvt_coordinates = false,
            // Alacritty owns the actual 1006 mode. An explicit 1006 request
            // selects the supported report format; 1016 intentionally remains
            // separate because it changes the coordinate unit to pixels.
            (1006, Some(true)) => {
                self.utf8_coordinates = false;
                self.urxvt_coordinates = false;
            }
            (1016, Some(true)) => self.pixel_coordinates = true,
            (1016, Some(false)) => self.pixel_coordinates = false,
            _ => {}
        }
    }
}
