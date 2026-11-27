//! AxSSH protocol behavior at the VTE dispatch boundary.

use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{
    Attr, CharsetIndex, ClearMode, CursorShape, CursorStyle, Handler, Hyperlink, KeyboardModes,
    KeyboardModesApplyBehavior, LineClearMode, Mode, NamedPrivateMode, PrivateMode, Rgb,
    StandardCharset, TabulationClearMode,
};

use super::{
    Event, EventListener, GridSizeQuery, MAX_HYPERLINK_URI_BYTES, MAX_OSC52_CLIPBOARD_BYTES,
    MouseCoordinateEncoding, TerminalEventListener, TerminalProtocolEvent, TerminalQueryColor,
    TerminalQueryPalette,
};

const MAX_HYPERLINK_ID_BYTES: usize = 256;
const MAX_OSC52_ENCODED_BYTES: usize = MAX_OSC52_CLIPBOARD_BYTES.div_ceil(3) * 4;

#[derive(Default)]
pub(super) struct ExtendedMouseModes {
    urxvt: bool,
    sgr_pixels: bool,
}

impl ExtendedMouseModes {
    pub(super) fn coordinate_encoding(&self, mode: TermMode) -> MouseCoordinateEncoding {
        if mode.contains(TermMode::SGR_MOUSE) {
            if self.sgr_pixels {
                MouseCoordinateEncoding::SgrPixels
            } else {
                MouseCoordinateEncoding::Sgr
            }
        } else if self.urxvt {
            MouseCoordinateEncoding::Urxvt
        } else if mode.contains(TermMode::UTF8_MOUSE) {
            MouseCoordinateEncoding::Utf8
        } else {
            MouseCoordinateEncoding::Default
        }
    }
}

pub(super) struct ProtocolHandler<'a> {
    pub(super) term: &'a mut Term<TerminalEventListener>,
    pub(super) events: &'a TerminalEventListener,
    pub(super) mouse_modes: &'a mut ExtendedMouseModes,
    pub(super) query_palette: &'a TerminalQueryPalette,
}

// Forward the terminal actions implemented by the locked Alacritty core.
// Keep the list in sync with its Handler implementation on dependency upgrades.
macro_rules! forward_terminal_actions {
    ($(fn $method:ident($($argument:ident: $kind:ty),*);)*) => {
        $(fn $method(&mut self, $($argument: $kind),*) {
            self.term.$method($($argument),*);
        })*
    };
}

impl Handler for ProtocolHandler<'_> {
    forward_terminal_actions! {
        fn set_title(title: Option<String>);
        fn set_cursor_style(style: Option<CursorStyle>);
        fn set_cursor_shape(shape: CursorShape);
        fn input(character: char);
        fn goto(line: i32, column: usize);
        fn goto_line(line: i32);
        fn goto_col(column: usize);
        fn insert_blank(count: usize);
        fn move_up(rows: usize);
        fn move_down(rows: usize);
        fn identify_terminal(intermediate: Option<char>);
        fn device_status(status: usize);
        fn move_forward(columns: usize);
        fn move_backward(columns: usize);
        fn move_down_and_cr(rows: usize);
        fn move_up_and_cr(rows: usize);
        fn put_tab(count: u16);
        fn backspace();
        fn carriage_return();
        fn linefeed();
        fn bell();
        fn substitute();
        fn newline();
        fn set_horizontal_tabstop();
        fn scroll_up(rows: usize);
        fn scroll_down(rows: usize);
        fn insert_blank_lines(count: usize);
        fn delete_lines(count: usize);
        fn erase_chars(count: usize);
        fn delete_chars(count: usize);
        fn move_backward_tabs(count: u16);
        fn move_forward_tabs(count: u16);
        fn save_cursor_position();
        fn restore_cursor_position();
        fn clear_line(mode: LineClearMode);
        fn clear_screen(mode: ClearMode);
        fn clear_tabs(mode: TabulationClearMode);
        fn reverse_index();
        fn terminal_attribute(attr: Attr);
        fn set_mode(mode: Mode);
        fn unset_mode(mode: Mode);
        fn report_mode(mode: Mode);
        fn set_scrolling_region(top: usize, bottom: Option<usize>);
        fn set_keypad_application_mode();
        fn unset_keypad_application_mode();
        fn set_active_charset(index: CharsetIndex);
        fn configure_charset(index: CharsetIndex, charset: StandardCharset);
        fn set_color(index: usize, color: Rgb);
        fn reset_color(index: usize);
        fn decaln();
        fn push_title();
        fn pop_title();
        fn text_area_size_pixels();
        fn text_area_size_chars();
        fn report_keyboard_mode();
        fn push_keyboard_mode(mode: KeyboardModes);
        fn pop_keyboard_modes(count: u16);
        fn set_keyboard_mode(mode: KeyboardModes, behavior: KeyboardModesApplyBehavior);
    }

    fn dynamic_color_sequence(&mut self, prefix: String, index: usize, terminator: &str) {
        let Rgb { r, g, b } = self.term.colors()[index]
            .unwrap_or_else(|| default_query_color(index, *self.query_palette));
        self.events.send_event(Event::PtyWrite(format!(
            "\x1b]{prefix};rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}{terminator}"
        )));
    }

    fn cell_size_pixels(&mut self) {
        self.events
            .send_protocol_event(TerminalProtocolEvent::GridSizeRequest(GridSizeQuery::Cell));
    }

    fn clipboard_selection(&mut self, target: &[u8], data: &[u8], terminator: &str) {
        if !matches!(target, b"c" | b"") {
            return;
        }
        match data {
            b"?" => self.term.clipboard_load(b'c', terminator),
            encoded if encoded.len() <= MAX_OSC52_ENCODED_BYTES => {
                self.term.clipboard_store(b'c', encoded);
            }
            _ => (),
        }
    }

    fn set_hyperlink(&mut self, hyperlink: Option<Hyperlink>) {
        let bounded = hyperlink
            .filter(|link| link.uri.len() <= MAX_HYPERLINK_URI_BYTES)
            .map(|link| Hyperlink {
                id: link.id.filter(|id| id.len() <= MAX_HYPERLINK_ID_BYTES),
                uri: link.uri,
            });
        self.term.set_hyperlink(bounded);
    }

    fn reset_state(&mut self) {
        *self.mouse_modes = ExtendedMouseModes::default();
        self.term.reset_state();
    }

    fn set_private_mode(&mut self, mode: PrivateMode) {
        match mode {
            PrivateMode::Unknown(1048) => self.term.save_cursor_position(),
            PrivateMode::Unknown(1015) => {
                self.term
                    .unset_private_mode(NamedPrivateMode::Utf8Mouse.into());
                self.term
                    .unset_private_mode(NamedPrivateMode::SgrMouse.into());
                *self.mouse_modes = ExtendedMouseModes {
                    urxvt: true,
                    sgr_pixels: false,
                };
            }
            PrivateMode::Unknown(1016) => self.mouse_modes.sgr_pixels = true,
            PrivateMode::Named(NamedPrivateMode::Utf8Mouse) => {
                *self.mouse_modes = ExtendedMouseModes::default();
                self.term.set_private_mode(mode);
            }
            PrivateMode::Named(NamedPrivateMode::SgrMouse) => {
                self.mouse_modes.urxvt = false;
                self.term.set_private_mode(mode);
            }
            _ => self.term.set_private_mode(mode),
        }
    }

    fn unset_private_mode(&mut self, mode: PrivateMode) {
        match mode {
            PrivateMode::Unknown(1048) => self.term.restore_cursor_position(),
            PrivateMode::Unknown(1015) => self.mouse_modes.urxvt = false,
            PrivateMode::Unknown(1016) => self.mouse_modes.sgr_pixels = false,
            _ => self.term.unset_private_mode(mode),
        }
    }

    fn report_private_mode(&mut self, mode: PrivateMode) {
        let enabled = match mode {
            PrivateMode::Unknown(1015) => self.mouse_modes.urxvt,
            PrivateMode::Unknown(1016) => self.mouse_modes.sgr_pixels,
            _ => return self.term.report_private_mode(mode),
        };
        let status = if enabled { 1 } else { 2 };
        self.events
            .send_event(Event::PtyWrite(format!("\x1b[?{};{status}$y", mode.raw())));
    }
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
