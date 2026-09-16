//! Bounded terminal grid with primary-screen reflow on resize.

pub use self::input::{
    TerminalKey, TerminalKeypadKey, TerminalModifiers, encode_key, encode_key_with_modes,
};

mod input;
mod model;
mod render;
mod selection;
#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::time::Duration;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::Processor;

use crate::terminal_dimensions::TerminalSize;

const PROTOCOL_RESPONSE_CAPACITY: usize = 16;
const MAX_PROTOCOL_RESPONSE_BYTES: usize = 4 * 1024;
/// A cursor-hidden redraw is normally emitted as several small PTY writes.
/// Bound the time that its intermediate frames may remain unpublished.
const OUTPUT_FRAME_HOLD_MAX: Duration = Duration::from_millis(250);

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
    pub cursor_text: String,
    pub display_offset: usize,
    pub viewport_mode: TerminalViewportMode,
    pub mouse_reporting: TerminalMouseReporting,
    pub mouse_button_reporting_active: bool,
    pub mouse_wheel_reporting_active: bool,
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
    pub utf8: bool,
    pub urxvt: bool,
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
    protocol_responses: Receiver<Vec<u8>>,
    scrollback_lines: usize,
    snapshot_lines: Vec<Arc<TerminalStyledLine>>,
    snapshot_columns: usize,
    snapshot_display_offset: usize,
    next_line_revision: u64,
    viewport_detached: bool,
    output_frame_hold: OutputFrameHold,
    mouse_encoding: MouseEncodingTracker,
}

/// The three extended mouse coordinate encodings selected through DEC private
/// modes. `1015` is not exposed by the locked terminal parser, so AxSSH tracks
/// all three raw mode changes to preserve their mutually-exclusive contract.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum MouseEncoding {
    #[default]
    Default,
    Utf8,
    Sgr,
    Urxvt,
}

#[derive(Default)]
struct MouseEncodingTracker {
    encoding: MouseEncoding,
    parser_state: u8,
    parameter: u16,
    has_parameter: bool,
}

/// Tracks DEC cursor-visibility sequences without retaining terminal output.
///
/// Full-screen progress applications often hide the cursor, rewrite several
/// rows across separate transport reads, then show it again. Publishing those
/// partial grids makes the cursor appear to jump between the rows being
/// rewritten. This only controls presentation timing; the terminal parser
/// continues to consume every byte immediately.
#[derive(Default)]
struct OutputFrameHold {
    active: bool,
    started_at: Option<std::time::Instant>,
    cursor_visibility_prefix: u8,
}
