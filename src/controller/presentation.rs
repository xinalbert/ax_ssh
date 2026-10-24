//! Latest admitted terminal frames, independent of any frontend event loop.

use crate::terminal::{TerminalModel, TerminalSnapshot};

/// Bounded pending/published snapshots and visibility state for one terminal.
/// The owner decides presentation deadlines; this gate preserves synchronized
/// output and unions row damage when updates arrive before UI consumption.
#[derive(Default)]
pub(super) struct TerminalFrames {
    output_frame_pending: bool,
    pending_terminal_snapshot: Option<TerminalSnapshot>,
    published_terminal_snapshot: Option<TerminalSnapshot>,
    published_terminal_state: Option<TerminalVisibleState>,
}

impl TerminalFrames {
    /// Keeps new output private until the owner admits its next frame.
    pub(super) fn defer_output(&mut self) {
        self.output_frame_pending = true;
    }

    /// Admit one bounded snapshot at the output frame deadline. Later output
    /// remains private until its own deadline, including during unrelated UI
    /// refreshes. Control-only writes still do not enter the UI queue.
    pub(super) fn prepare(&mut self, mut model: Option<&mut TerminalModel>) -> bool {
        if Self::synchronized_output_pending(model.as_deref_mut()) {
            return false;
        }
        self.output_frame_pending = false;
        if self.pending_terminal_snapshot.is_some() {
            return true;
        }
        let Some(snapshot) = model.map(TerminalModel::snapshot) else {
            return false;
        };
        let visible_state = TerminalVisibleState::from(&snapshot);
        if snapshot.dirty_rows.is_empty()
            && self
                .published_terminal_state
                .as_ref()
                .is_some_and(|current| current == &visible_state)
        {
            return false;
        }
        self.pending_terminal_snapshot = Some(snapshot);
        true
    }

    /// Returns the admitted frame, retaining the previous frame during sync output.
    pub(super) fn snapshot(
        &mut self,
        mut model: Option<&mut TerminalModel>,
    ) -> Option<TerminalSnapshot> {
        if Self::synchronized_output_pending(model.as_deref_mut()) {
            return self
                .published_terminal_snapshot
                .clone()
                .or_else(|| self.pending_terminal_snapshot.clone());
        }
        let current_snapshot = if self.output_frame_pending {
            None
        } else {
            model.map(TerminalModel::snapshot)
        };
        let snapshot = match (self.pending_terminal_snapshot.take(), current_snapshot) {
            (Some(pending), Some(current)) => merge_terminal_snapshots(pending, current),
            (Some(pending), None) => pending,
            (None, Some(current)) => current,
            (None, None) => return self.published_terminal_snapshot.clone(),
        };
        self.published_terminal_state = Some(TerminalVisibleState::from(&snapshot));
        self.published_terminal_snapshot = Some(snapshot.clone());
        Some(snapshot)
    }

    fn synchronized_output_pending(model: Option<&mut TerminalModel>) -> bool {
        let Some(model) = model else {
            return false;
        };
        !model.flush_synchronized_output_if_due() && model.synchronized_output_remaining().is_some()
    }

    /// Invalidates an admitted frame after an explicit viewport or size change.
    pub(super) fn discard_pending(&mut self) {
        self.pending_terminal_snapshot = None;
        self.output_frame_pending = false;
    }
}

fn merge_terminal_snapshots(
    pending: TerminalSnapshot,
    mut current: TerminalSnapshot,
) -> TerminalSnapshot {
    if pending.full_refresh
        || current.full_refresh
        || pending.lines.len() != current.lines.len()
        || pending.max_columns != current.max_columns
    {
        current.full_refresh = true;
        current.dirty_rows = (0..current.lines.len()).collect();
        return current;
    }

    current.dirty_rows.extend(pending.dirty_rows);
    current.dirty_rows.sort_unstable();
    current.dirty_rows.dedup();
    current
}

/// The bounded terminal state that is visible outside the retained row models.
/// Row mutations are represented by `TerminalSnapshot::dirty_rows`; this value
/// covers cursor, viewport, and protocol-mode changes that do not necessarily
/// alter a styled row.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalVisibleState {
    line_count: usize,
    max_columns: usize,
    cursor_row: usize,
    cursor_column: usize,
    cursor_cells: usize,
    cursor_visible: bool,
    cursor_shape: crate::terminal::TerminalCursorShape,
    cursor_blinking: bool,
    foreground_color: crate::terminal::TerminalColor,
    background_color: crate::terminal::TerminalColor,
    cursor_color: crate::terminal::TerminalColor,
    cursor_text: String,
    display_offset: usize,
    viewport_mode: crate::terminal::TerminalViewportMode,
    mouse_reporting: crate::terminal::TerminalMouseReporting,
    mouse_button_reporting_active: bool,
    mouse_wheel_reporting_active: bool,
    bell_revision: u64,
}

impl From<&TerminalSnapshot> for TerminalVisibleState {
    fn from(snapshot: &TerminalSnapshot) -> Self {
        Self {
            line_count: snapshot.lines.len(),
            max_columns: snapshot.max_columns,
            cursor_row: snapshot.cursor_row,
            cursor_column: snapshot.cursor_column,
            cursor_cells: snapshot.cursor_cells,
            cursor_visible: snapshot.cursor_visible,
            cursor_shape: snapshot.cursor_shape,
            cursor_blinking: snapshot.cursor_blinking,
            foreground_color: snapshot.foreground_color,
            background_color: snapshot.background_color,
            cursor_color: snapshot.cursor_color,
            cursor_text: snapshot.cursor_text.clone(),
            display_offset: snapshot.display_offset,
            viewport_mode: snapshot.viewport_mode,
            mouse_reporting: snapshot.mouse_reporting,
            mouse_button_reporting_active: snapshot.mouse_button_reporting_active,
            mouse_wheel_reporting_active: snapshot.mouse_wheel_reporting_active,
            bell_revision: snapshot.bell_revision,
        }
    }
}

#[cfg(test)]
mod tests;
