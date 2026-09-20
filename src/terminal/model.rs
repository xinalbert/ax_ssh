//! Terminal state, protocol handling, and viewport control.

use super::*;
use std::sync::mpsc::sync_channel;
use std::time::Instant;

use alacritty_terminal::event::WindowSize;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::term::{Config as TermConfig, Osc52, TermMode};
use alacritty_terminal::vte::ansi::Rgb;

impl TerminalModel {
    pub fn new(columns: usize, rows: usize, scrollback_lines: usize) -> Self {
        Self::new_with_osc52_clipboard(columns, rows, scrollback_lines, false)
    }

    pub fn new_with_osc52_clipboard(
        columns: usize,
        rows: usize,
        scrollback_lines: usize,
        osc52_clipboard_enabled: bool,
    ) -> Self {
        let dimensions = TerminalDimensions::new(columns, rows);
        let config = terminal_config(scrollback_lines, osc52_clipboard_enabled);
        let (protocol_events_tx, protocol_events) = sync_channel(PROTOCOL_RESPONSE_CAPACITY);
        Self {
            term: Term::new(
                config,
                &dimensions,
                TerminalEventListener {
                    protocol_events: protocol_events_tx,
                },
            ),
            processor: Processor::new(),
            protocol_events,
            query_palette: TerminalQueryPalette::default(),
            window_size: None,
            pending_text_area_requests: Vec::new(),
            pending_cell_size_requests: 0,
            scrollback_lines,
            snapshot_lines: Vec::new(),
            snapshot_columns: 0,
            snapshot_display_offset: 0,
            next_line_revision: 0,
            viewport_detached: false,
            mouse_encoding: MouseEncodingTracker::default(),
            window_operation_queries: WindowOperationQueryTracker::default(),
            pending_title_update: None,
            bell_revision: 0,
            pending_bell: false,
            pending_clipboard_store: None,
            pending_clipboard_load: None,
            osc52_clipboard_enabled,
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        let _ = self.process_with_responses(bytes);
    }

    /// Parse live output and return bounded protocol responses for the same transport.
    pub fn process_with_responses(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.mouse_encoding.observe(bytes);
        let cell_size_requests = self.window_operation_queries.observe(bytes);
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
        let mut responses = self.drain_protocol_events();
        self.handle_cell_size_requests(cell_size_requests, &mut responses);
        responses
    }

    /// Takes the latest OSC 0/2 title update. `Some(None)` means the terminal
    /// requested a reset to the application-provided default title.
    pub fn take_title_update(&mut self) -> Option<Option<String>> {
        self.pending_title_update.take()
    }

    /// Takes one or more Bell events coalesced during the last parser pass.
    pub fn take_bell(&mut self) -> bool {
        std::mem::take(&mut self.pending_bell)
    }

    /// Takes the latest bounded OSC 52 write request for the default clipboard.
    pub fn take_clipboard_store(&mut self) -> Option<String> {
        self.pending_clipboard_store.take()
    }

    /// Takes the latest OSC 52 read request for the default clipboard.
    pub fn take_clipboard_load(&mut self) -> Option<ClipboardLoadFormatter> {
        self.pending_clipboard_load.take()
    }

    /// Returns the remaining standard synchronized-output interval, if any.
    pub fn synchronized_output_remaining(&self) -> Option<std::time::Duration> {
        self.processor
            .sync_timeout()
            .sync_timeout()
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }

    /// Flushes an expired `CSI ?2026h` synchronized update. Cursor visibility
    /// remains a pure display state and never controls frame batching.
    pub fn flush_synchronized_output_if_due(&mut self) -> bool {
        let Some(deadline) = self.processor.sync_timeout().sync_timeout() else {
            return false;
        };
        if deadline > Instant::now() {
            return false;
        }
        self.processor.stop_sync(&mut self.term);
        true
    }

    #[cfg(test)]
    pub(super) fn flush_synchronized_output_for_test(&mut self) {
        self.processor.stop_sync(&mut self.term);
    }

    /// Records measured physical cell dimensions and returns responses for
    /// bounded `CSI 14 t` and `CSI 16 t` requests deferred until the first
    /// layout completed.
    pub fn set_window_size(
        &mut self,
        columns: u32,
        rows: u32,
        cell_width: u32,
        cell_height: u32,
    ) -> Vec<Vec<u8>> {
        let window_size = WindowSize {
            num_lines: rows.min(u32::from(u16::MAX)) as u16,
            num_cols: columns.min(u32::from(u16::MAX)) as u16,
            cell_width: cell_width.min(u32::from(u16::MAX)) as u16,
            cell_height: cell_height.min(u32::from(u16::MAX)) as u16,
        };
        if self.window_size.is_some_and(|current| {
            current.num_lines == window_size.num_lines
                && current.num_cols == window_size.num_cols
                && current.cell_width == window_size.cell_width
                && current.cell_height == window_size.cell_height
        }) {
            return Vec::new();
        }
        self.window_size = Some(window_size);
        let mut responses = self
            .pending_text_area_requests
            .drain(..)
            .filter_map(|formatter| protocol_response(formatter(window_size).into_bytes()))
            .collect::<Vec<_>>();
        let remaining = PROTOCOL_RESPONSE_CAPACITY.saturating_sub(responses.len());
        let cell_size_requests = std::mem::take(&mut self.pending_cell_size_requests);
        responses.extend(
            std::iter::repeat_with(|| cell_size_response(window_size))
                .take(cell_size_requests.min(remaining))
                .flatten(),
        );
        responses
    }

    /// Updates the configured base colors used to answer OSC color queries.
    /// Explicit colors set by the terminal application continue to take
    /// precedence through Alacritty's dynamic color table.
    pub fn set_query_palette(&mut self, palette: TerminalQueryPalette) {
        self.query_palette = palette;
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
            sgr: mode.contains(TermMode::SGR_MOUSE),
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
        self.encode_mouse_event_with_pixels(
            event,
            event.column.saturating_add(1),
            event.row.saturating_add(1),
        )
    }

    /// Encode one mouse event with the physical text-area position used by
    /// xterm's SGR pixel-coordinate extension (`?1016`).
    pub fn encode_mouse_event_with_pixels(
        &self,
        event: TerminalMouseEvent,
        pixel_x: usize,
        pixel_y: usize,
    ) -> Option<Vec<u8>> {
        let reporting = self.mouse_reporting();
        let coordinate_encoding = self.mouse_encoding.coordinate_encoding(reporting.sgr);
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
        if matches!(
            coordinate_encoding,
            MouseCoordinateEncoding::Sgr | MouseCoordinateEncoding::SgrPixels
        ) {
            let suffix = if matches!(event.kind, TerminalMouseEventKind::Release) {
                'm'
            } else {
                'M'
            };
            let (x, y) = if matches!(coordinate_encoding, MouseCoordinateEncoding::SgrPixels) {
                let (max_x, max_y) = self
                    .window_size
                    .map(|size| {
                        (
                            usize::from(size.num_cols).saturating_mul(usize::from(size.cell_width)),
                            usize::from(size.num_lines)
                                .saturating_mul(usize::from(size.cell_height)),
                        )
                    })
                    .unwrap_or((usize::MAX, usize::MAX));
                (
                    pixel_x.clamp(1, max_x.max(1)),
                    pixel_y.clamp(1, max_y.max(1)),
                )
            } else {
                (column, row)
            };
            return Some(format!("\x1b[<{};{};{}{}", code, x, y, suffix).into_bytes());
        }
        if matches!(coordinate_encoding, MouseCoordinateEncoding::Urxvt) {
            return Some(format!("\x1b[{};{};{}M", code + 32, column, row).into_bytes());
        }
        if matches!(coordinate_encoding, MouseCoordinateEncoding::Utf8) {
            let encode = |value: usize| {
                char::from_u32((value + 32) as u32).map(|value| {
                    let mut output = [0; 4];
                    value.encode_utf8(&mut output).as_bytes().to_vec()
                })
            };
            let mut output = vec![0x1b, b'[', b'M'];
            output.extend(encode(code)?);
            output.extend(encode(column)?);
            output.extend(encode(row)?);
            return Some(output);
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

        self.term.set_options(terminal_config(
            scrollback_lines,
            self.osc52_clipboard_enabled,
        ));
        self.scrollback_lines = scrollback_lines;
        if self.term.grid().display_offset() == 0 {
            self.viewport_detached = false;
        }
    }

    pub fn set_osc52_clipboard_enabled(&mut self, enabled: bool) {
        if self.osc52_clipboard_enabled == enabled {
            return;
        }
        self.osc52_clipboard_enabled = enabled;
        self.term
            .set_options(terminal_config(self.scrollback_lines, enabled));
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

    fn drain_protocol_events(&mut self) -> Vec<Vec<u8>> {
        let mut responses = Vec::new();
        while let Ok(event) = self.protocol_events.try_recv() {
            match event {
                TerminalProtocolEvent::PtyWrite(response) => {
                    if let Some(response) = protocol_response(response) {
                        responses.push(response);
                    }
                }
                TerminalProtocolEvent::ColorRequest(index, formatter) => {
                    let color = self.term.colors()[index]
                        .unwrap_or_else(|| default_query_color(index, self.query_palette));
                    if let Some(response) = protocol_response(formatter(color).into_bytes()) {
                        responses.push(response);
                    }
                }
                TerminalProtocolEvent::TextAreaSizeRequest(formatter) => {
                    if let Some(window_size) = self.window_size {
                        if let Some(response) =
                            protocol_response(formatter(window_size).into_bytes())
                        {
                            responses.push(response);
                        }
                    } else if self.pending_text_area_requests.len() < PROTOCOL_RESPONSE_CAPACITY {
                        self.pending_text_area_requests.push(formatter);
                    }
                }
                TerminalProtocolEvent::Title(title) => {
                    self.pending_title_update = Some(Some(title));
                }
                TerminalProtocolEvent::ResetTitle => {
                    self.pending_title_update = Some(None);
                }
                TerminalProtocolEvent::Bell => {
                    self.bell_revision = self.bell_revision.wrapping_add(1).max(1);
                    self.pending_bell = true;
                }
                TerminalProtocolEvent::ClipboardStore(text) => {
                    self.pending_clipboard_store = Some(text);
                }
                TerminalProtocolEvent::ClipboardLoad(formatter) => {
                    self.pending_clipboard_load = Some(formatter);
                }
            }
        }
        responses
    }

    fn handle_cell_size_requests(&mut self, count: usize, responses: &mut Vec<Vec<u8>>) {
        if count == 0 {
            return;
        }
        if let Some(window_size) = self.window_size {
            let remaining = PROTOCOL_RESPONSE_CAPACITY.saturating_sub(responses.len());
            responses.extend(
                std::iter::repeat_with(|| cell_size_response(window_size))
                    .take(count.min(remaining))
                    .flatten(),
            );
        } else {
            let occupied = self
                .pending_text_area_requests
                .len()
                .saturating_add(self.pending_cell_size_requests);
            self.pending_cell_size_requests = self
                .pending_cell_size_requests
                .saturating_add(count.min(PROTOCOL_RESPONSE_CAPACITY.saturating_sub(occupied)));
        }
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

fn terminal_config(scrollback_lines: usize, osc52_clipboard_enabled: bool) -> TermConfig {
    TermConfig {
        scrolling_history: scrollback_lines,
        osc52: if osc52_clipboard_enabled {
            Osc52::CopyPaste
        } else {
            Osc52::Disabled
        },
        ..TermConfig::default()
    }
}

fn protocol_response(response: Vec<u8>) -> Option<Vec<u8>> {
    (!response.is_empty() && response.len() <= MAX_PROTOCOL_RESPONSE_BYTES).then_some(response)
}

fn cell_size_response(window_size: WindowSize) -> Option<Vec<u8>> {
    protocol_response(
        format!(
            "\x1b[6;{};{}t",
            window_size.cell_height, window_size.cell_width
        )
        .into_bytes(),
    )
}

fn default_query_color(index: usize, palette: TerminalQueryPalette) -> Rgb {
    let from_query_color = |color: TerminalQueryColor| Rgb {
        r: color.red,
        g: color.green,
        b: color.blue,
    };
    match index {
        0..=15 => from_query_color(palette.ansi[index]),
        16..=231 => {
            let value = index - 16;
            let component = |part: usize| [0, 95, 135, 175, 215, 255][part];
            Rgb {
                r: component(value / 36),
                g: component((value / 6) % 6),
                b: component(value % 6),
            }
        }
        232..=255 => {
            let shade = (8 + (index - 232) * 10) as u8;
            Rgb {
                r: shade,
                g: shade,
                b: shade,
            }
        }
        256 => from_query_color(palette.foreground),
        257 => from_query_color(palette.background),
        258 => from_query_color(palette.cursor),
        _ => from_query_color(palette.foreground),
    }
}

impl MouseEncodingTracker {
    fn coordinate_encoding(&self, sgr: bool) -> MouseCoordinateEncoding {
        if self.pixel_coordinates && sgr {
            MouseCoordinateEncoding::SgrPixels
        } else if sgr {
            MouseCoordinateEncoding::Sgr
        } else if self.urxvt_coordinates {
            MouseCoordinateEncoding::Urxvt
        } else if self.utf8_coordinates {
            MouseCoordinateEncoding::Utf8
        } else {
            MouseCoordinateEncoding::Default
        }
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
                    self.parameters.clear();
                }
                3 if byte.is_ascii_digit() => {
                    self.parameter = self
                        .parameter
                        .saturating_mul(10)
                        .saturating_add(u16::from(byte - b'0'));
                    self.has_parameter = true;
                }
                3 if byte == b';' => self.finish_parameter(),
                3 if matches!(byte, b'h' | b'l') => {
                    self.finish_parameter();
                    self.apply_parameters(byte == b'h');
                    self.parser_state = 0;
                }
                _ => self.parser_state = u8::from(byte == 0x1b),
            }
        }
    }

    fn finish_parameter(&mut self) {
        let parameter = self.has_parameter.then_some(self.parameter);
        self.parameter = 0;
        self.has_parameter = false;
        if let Some(parameter) = parameter {
            self.parameters.push(parameter);
        }
    }

    fn apply_parameters(&mut self, enabled: bool) {
        for parameter in self.parameters.drain(..) {
            match parameter {
                1005 => {
                    self.utf8_coordinates = enabled;
                    if enabled {
                        self.urxvt_coordinates = false;
                        self.pixel_coordinates = false;
                    }
                }
                1015 => {
                    self.urxvt_coordinates = enabled;
                    if enabled {
                        self.utf8_coordinates = false;
                        self.pixel_coordinates = false;
                    }
                }
                1006 if enabled => {
                    // xterm mouse encodings are mutually exclusive. SGR pixel
                    // mode (1016) remains an SGR extension and is checked
                    // separately by `coordinate_encoding`.
                    self.utf8_coordinates = false;
                    self.urxvt_coordinates = false;
                }
                1016 => self.pixel_coordinates = enabled,
                _ => {}
            }
        }
    }
}

impl WindowOperationQueryTracker {
    fn observe(&mut self, bytes: &[u8]) -> usize {
        let mut requests: usize = 0;
        for &byte in bytes {
            match self.parser_state {
                0 if byte == 0x1b => self.parser_state = 1,
                1 if byte == b'[' => {
                    self.parser_state = 2;
                    self.parameter = 0;
                    self.has_parameter = false;
                }
                2 if byte.is_ascii_digit() => {
                    self.parameter = self
                        .parameter
                        .saturating_mul(10)
                        .saturating_add(u16::from(byte - b'0'));
                    self.has_parameter = true;
                }
                2 if byte == b't' && self.has_parameter && self.parameter == 16 => {
                    requests = requests.saturating_add(1);
                    self.parser_state = 0;
                }
                _ => self.parser_state = u8::from(byte == 0x1b),
            }
        }
        requests
    }
}
