//! Bounded terminal grid with primary-screen reflow on resize.

pub use self::input::{TerminalKey, TerminalModifiers, encode_key};

mod input;
mod model;
mod render;
mod selection;
#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::Processor;

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
    snapshot_lines: Vec<Arc<TerminalStyledLine>>,
    snapshot_columns: usize,
    snapshot_display_offset: usize,
    next_line_revision: u64,
    viewport_detached: bool,
}
