//! Cell-based text extraction and selection boundaries.

use super::*;

use super::render::{
    append_cell_text, append_occupied_cells, cell_character_count, is_wide_continuation,
    occupied_end_column,
};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;

impl TerminalModel {
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

    fn selection_range(
        &self,
        row: usize,
        column: usize,
        selection_type: SelectionType,
    ) -> Option<TerminalSelectionRange> {
        let grid = self.term.grid();
        let rows = grid.screen_lines();
        let columns = grid.columns();
        if rows == 0 || columns == 0 || row >= rows || column >= columns {
            return None;
        }

        let display_offset = grid.display_offset() as i32;
        let line = Line(row as i32 - display_offset);
        let point = Point::new(line, Column(column));
        let selection = Selection::new(selection_type, point, Side::Left);
        let range = selection.to_range(&self.term)?;

        let visible_top = -display_offset;
        let visible_bottom = visible_top + rows as i32 - 1;
        let start_line = range.start.line.0.max(visible_top);
        let end_line = range.end.line.0.min(visible_bottom);
        if start_line > end_line {
            return None;
        }

        let last_column = columns - 1;
        let start_column = if range.start.line.0 < visible_top {
            0
        } else {
            range.start.column.0.min(last_column)
        };
        let end_column = if range.end.line.0 > visible_bottom {
            last_column
        } else {
            range.end.column.0.min(last_column)
        };

        Some(TerminalSelectionRange {
            start_row: (start_line - visible_top) as usize,
            start_column,
            end_row: (end_line - visible_top) as usize,
            end_column,
        })
    }

    /// Returns the visible cell range for a semantic double-click selection.
    ///
    /// The temporary alacritty selection is used only for its boundary
    /// semantics. It is never stored in the terminal model, so local Slint
    /// selection state remains the sole owner of highlighting and copying.
    pub fn semantic_selection_range(
        &self,
        row: usize,
        column: usize,
    ) -> Option<TerminalSelectionRange> {
        self.selection_range(row, column, SelectionType::Semantic)
    }

    /// Returns the visible cell range for a triple-click line selection.
    ///
    /// Alacritty's line search follows logical lines across soft wraps and
    /// keeps hard line boundaries intact. The range is clipped to the current
    /// viewport before it crosses the UI boundary.
    pub fn line_selection_range(
        &self,
        row: usize,
        column: usize,
    ) -> Option<TerminalSelectionRange> {
        self.selection_range(row, column, SelectionType::Lines)
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
        let mut character = 0usize;
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
                target_character = Some(character);
            }
            append_cell_text(&mut contents, cell);
            character = character.saturating_add(cell_character_count(cell));
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
            let cell_characters = cell_character_count(cell);
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
