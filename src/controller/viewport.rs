//! Shared viewport transitions, selection identity, and admitted frames.

use crate::terminal::{TerminalModel, TerminalSnapshot};
use crate::terminal_dimensions::TerminalSize;

use super::TerminalFrames;

/// Per-terminal presentation state, without a UI or transport owner.
///
/// A viewport change invalidates both the pending frame and any selection made
/// against its old coordinates. Ordinary output preserves selection identity.
#[derive(Default)]
pub struct TerminalViewport {
    frames: TerminalFrames,
    selection_revision: i32,
}

impl TerminalViewport {
    /// Nonnegative identity for selections in the current viewport.
    pub fn selection_revision(&self) -> i32 {
        self.selection_revision
    }

    /// Replaces a connection's model without reusing old frames or selections.
    pub fn reset(&mut self) {
        self.changed(true);
        self.frames = TerminalFrames::default();
    }

    /// Applies cell dimensions and invalidates old coordinates only on change.
    /// The caller queues transport resize before updating the model.
    pub fn resize(&mut self, model: &mut TerminalModel, size: TerminalSize) -> bool {
        let changed = model.resize(size.columns() as usize, size.rows() as usize);
        self.changed(changed)
    }

    /// Scrolls history and invalidates selections only when the viewport moves.
    pub fn scroll(&mut self, model: &mut TerminalModel, lines: i32) -> bool {
        let changed = model.scroll(lines);
        self.changed(changed)
    }

    /// Resumes following live output after successfully queued input.
    pub fn scroll_to_bottom(&mut self, model: &mut TerminalModel) -> bool {
        let changed = model.scroll_to_bottom();
        self.changed(changed)
    }

    fn changed(&mut self, changed: bool) -> bool {
        if changed {
            self.frames.discard_pending();
            self.selection_revision = if self.selection_revision == i32::MAX {
                1
            } else {
                self.selection_revision + 1
            };
        }
        changed
    }

    /// Defers output until the owner's next presentation deadline.
    pub fn defer_output(&mut self) {
        self.frames.defer_output();
    }

    /// Admits output without ending a pending synchronized update early.
    pub fn prepare(&mut self, model: Option<&mut TerminalModel>) -> bool {
        self.frames.prepare(model)
    }

    /// Returns the admitted frame, retaining dirty rows across skipped frames.
    pub fn snapshot(&mut self, model: Option<&mut TerminalModel>) -> Option<TerminalSnapshot> {
        self.frames.snapshot(model)
    }

    /// Invalidates presentation after settings changes without moving a selection.
    pub fn discard_pending(&mut self) {
        self.frames.discard_pending();
    }
}

#[cfg(test)]
mod tests;
