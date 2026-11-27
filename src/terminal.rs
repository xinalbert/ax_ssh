//! Bounded terminal grid with primary-screen reflow on resize.

pub use self::input::{TerminalKey, TerminalModifiers, encode_key};

mod input;
mod model;
mod protocol;
mod render;
mod selection;
#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::term::{ClipboardType, Term};
use alacritty_terminal::vte::ansi::Processor;

use crate::terminal_dimensions::{TerminalGridMetrics, TerminalSize};
use protocol::ExtendedMouseModes;

const PROTOCOL_RESPONSE_CAPACITY: usize = 16;
const MAX_PROTOCOL_RESPONSE_BYTES: usize = 4 * 1024;
const MAX_TERMINAL_TITLE_BYTES: usize = 512;
const MAX_HYPERLINK_URI_BYTES: usize = 2 * 1024;
pub const MAX_OSC52_CLIPBOARD_BYTES: usize = 64 * 1024;

pub type ClipboardLoadFormatter = Arc<dyn Fn(&str) -> String + Send + Sync + 'static>;

/// One bounded output batch crossing a transport/application boundary.
///
/// `received_at` is captured by the transport as close as practical to the
/// underlying read. The application layer may use it for presentation latency
/// and diagnostics without coupling the UI to a worker or runtime handle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalOutputChunk {
    pub data: Vec<u8>,
    pub received_at: Instant,
}

impl TerminalOutputChunk {
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            received_at: Instant::now(),
        }
    }

    pub fn with_received_at(data: Vec<u8>, received_at: Instant) -> Self {
        Self { data, received_at }
    }
}

enum TerminalProtocolEvent {
    PtyWrite(Vec<u8>),
    GridSizeRequest(GridSizeQuery),
    Title(String),
    ResetTitle,
    Bell,
    ClipboardStore(String),
    ClipboardLoad(ClipboardLoadFormatter),
}

enum GridSizeQuery {
    TextArea,
    Cell,
}

#[derive(Clone)]
struct TerminalEventListener {
    protocol_events: SyncSender<TerminalProtocolEvent>,
}

impl EventListener for TerminalEventListener {
    fn send_event(&self, event: Event) {
        let event = match event {
            Event::PtyWrite(response) => TerminalProtocolEvent::PtyWrite(response.into_bytes()),
            Event::TextAreaSizeRequest(_) => {
                TerminalProtocolEvent::GridSizeRequest(GridSizeQuery::TextArea)
            }
            Event::Title(title) => {
                TerminalProtocolEvent::Title(bound_utf8(title, MAX_TERMINAL_TITLE_BYTES))
            }
            Event::ResetTitle => TerminalProtocolEvent::ResetTitle,
            Event::Bell => TerminalProtocolEvent::Bell,
            Event::ClipboardStore(ClipboardType::Clipboard, text)
                if text.len() <= MAX_OSC52_CLIPBOARD_BYTES =>
            {
                TerminalProtocolEvent::ClipboardStore(text)
            }
            Event::ClipboardLoad(ClipboardType::Clipboard, formatter) => {
                TerminalProtocolEvent::ClipboardLoad(formatter)
            }
            _ => return,
        };
        self.send_protocol_event(event);
    }
}

impl TerminalEventListener {
    fn send_protocol_event(&self, event: TerminalProtocolEvent) {
        // Parser callbacks run synchronously while a bounded output batch is
        // being consumed. A full queue means the model cannot preserve the
        // response ordering contract, so surface that condition as a bounded
        // diagnostic instead of silently dropping a query response.
        if let Err(error) = self.protocol_events.try_send(event) {
            match error {
                TrySendError::Full(_) => {
                    tracing::warn!(
                        target: "ax_ssh::diagnostics",
                        event = "terminal-protocol-event-dropped",
                        reason = "bounded protocol event queue full",
                        "terminal protocol response was dropped"
                    );
                }
                TrySendError::Disconnected(_) => {}
            }
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
pub struct TerminalQueryColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalQueryPalette {
    pub ansi: [TerminalQueryColor; 16],
    pub foreground: TerminalQueryColor,
    pub background: TerminalQueryColor,
    pub cursor: TerminalQueryColor,
}

impl Default for TerminalQueryPalette {
    fn default() -> Self {
        let ansi = [
            TerminalQueryColor {
                red: 0,
                green: 0,
                blue: 0,
            },
            TerminalQueryColor {
                red: 205,
                green: 0,
                blue: 0,
            },
            TerminalQueryColor {
                red: 0,
                green: 205,
                blue: 0,
            },
            TerminalQueryColor {
                red: 205,
                green: 205,
                blue: 0,
            },
            TerminalQueryColor {
                red: 0,
                green: 0,
                blue: 238,
            },
            TerminalQueryColor {
                red: 205,
                green: 0,
                blue: 205,
            },
            TerminalQueryColor {
                red: 0,
                green: 205,
                blue: 205,
            },
            TerminalQueryColor {
                red: 229,
                green: 229,
                blue: 229,
            },
            TerminalQueryColor {
                red: 127,
                green: 127,
                blue: 127,
            },
            TerminalQueryColor {
                red: 255,
                green: 0,
                blue: 0,
            },
            TerminalQueryColor {
                red: 0,
                green: 255,
                blue: 0,
            },
            TerminalQueryColor {
                red: 255,
                green: 255,
                blue: 0,
            },
            TerminalQueryColor {
                red: 92,
                green: 92,
                blue: 255,
            },
            TerminalQueryColor {
                red: 255,
                green: 0,
                blue: 255,
            },
            TerminalQueryColor {
                red: 0,
                green: 255,
                blue: 255,
            },
            TerminalQueryColor {
                red: 255,
                green: 255,
                blue: 255,
            },
        ];
        Self {
            ansi,
            foreground: ansi[7],
            background: ansi[0],
            cursor: ansi[7],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerminalUnderlineStyle {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerminalCursorShape {
    #[default]
    Block,
    HollowBlock,
    Underline,
    Beam,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalStyle {
    pub foreground: TerminalColor,
    pub background: TerminalColor,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub underline_style: TerminalUnderlineStyle,
    pub underline_color: Option<TerminalColor>,
    pub strikethrough: bool,
    pub inverse: bool,
    pub hidden: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalStyledRun {
    pub text: String,
    pub column: usize,
    pub cells: usize,
    pub style: TerminalStyle,
    /// URI carried by OSC 8 for this cell span. It is bounded at the parser
    /// boundary and is never persisted or logged.
    pub hyperlink: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalStyledLine {
    pub revision: u64,
    pub runs: Vec<TerminalStyledRun>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerminalViewportMode {
    #[default]
    Follow,
    Detached,
    AlternateScreen,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSnapshot {
    pub lines: Vec<Arc<TerminalStyledLine>>,
    /// Rows whose styled content changed since the previous snapshot. The UI
    /// uses this bounded list for incremental model updates; the full `lines`
    /// vector remains available for callers that need a complete snapshot.
    pub dirty_rows: Vec<usize>,
    /// True when the visible row model must be rebuilt (first frame or geometry change).
    pub full_refresh: bool,
    pub max_columns: usize,
    pub cursor_row: usize,
    pub cursor_column: usize,
    pub cursor_cells: usize,
    pub cursor_visible: bool,
    pub cursor_shape: TerminalCursorShape,
    pub cursor_blinking: bool,
    /// Resolved terminal-local defaults after OSC palette overrides.
    pub foreground_color: TerminalColor,
    pub background_color: TerminalColor,
    /// Resolved cursor color after terminal-local OSC palette overrides.
    pub cursor_color: TerminalColor,
    pub cursor_text: String,
    pub display_offset: usize,
    pub viewport_mode: TerminalViewportMode,
    pub mouse_reporting: TerminalMouseReporting,
    pub mouse_button_reporting_active: bool,
    pub mouse_wheel_reporting_active: bool,
    pub bell_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSelectionRange {
    pub start_row: usize,
    pub start_column: usize,
    pub end_row: usize,
    pub end_column: usize,
}

/// One bounded physical row participating in a logical terminal line used
/// for short-lived target recognition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalTargetRow {
    pub row: usize,
    pub text: String,
}

/// Bounded logical-line text around a clicked terminal cell.
///
/// Consecutive rows are joined without inserting a newline when the terminal
/// marked them with `WRAPLINE`. The context is clipped to the visible
/// viewport and does not retain or expose the terminal grid itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalTargetContext {
    pub rows: Vec<TerminalTargetRow>,
    pub clicked_row: usize,
    pub clicked_character: usize,
    pub starts_mid_logical_line: bool,
    pub ends_mid_logical_line: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalMouseReporting {
    pub click: bool,
    pub drag: bool,
    pub motion: bool,
    pub sgr: bool,
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
    WheelLeft,
    WheelRight,
    Auxiliary8,
    Auxiliary9,
    Auxiliary10,
    Auxiliary11,
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
    protocol_events: Receiver<TerminalProtocolEvent>,
    protocol_listener: TerminalEventListener,
    query_palette: TerminalQueryPalette,
    grid_metrics: Option<TerminalGridMetrics>,
    pending_grid_queries: Vec<GridSizeQuery>,
    scrollback_lines: usize,
    snapshot_lines: Vec<Arc<TerminalStyledLine>>,
    snapshot_columns: usize,
    snapshot_display_offset: usize,
    next_line_revision: u64,
    viewport_detached: bool,
    extended_mouse_modes: ExtendedMouseModes,
    pending_title_update: Option<Option<String>>,
    bell_revision: u64,
    pending_bell: bool,
    pending_clipboard_store: Option<String>,
    pending_clipboard_load: Option<ClipboardLoadFormatter>,
    osc52_clipboard_enabled: bool,
    osc52_permission_generation: u64,
}

fn bound_utf8(mut value: String, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes.min(value.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value
}

fn is_safe_hyperlink_uri(uri: &str) -> bool {
    let Some((scheme, authority)) = uri.split_once("://") else {
        return false;
    };
    matches!(scheme, "http" | "https")
        && !authority.is_empty()
        && !uri
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
}

#[derive(Clone, Copy)]
enum MouseCoordinateEncoding {
    Default,
    Utf8,
    Urxvt,
    Sgr,
    SgrPixels,
}
