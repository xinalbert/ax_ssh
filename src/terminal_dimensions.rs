//! Shared terminal dimension limits used by the model, settings, and workers.

use anyhow::{Result, bail};

/// Minimum dimensions exposed by the terminal model and settings UI.
pub const MIN_TERMINAL_COLUMNS: u16 = 10;
pub const MIN_TERMINAL_ROWS: u16 = 3;

/// Maximum dimensions accepted by all terminal backends.
pub const MAX_TERMINAL_COLUMNS: u16 = 300;
pub const MAX_TERMINAL_ROWS: u16 = 100;

/// One normalized terminal size shared by the model and backend workers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSize {
    columns: u32,
    rows: u32,
}

impl TerminalSize {
    pub fn model(columns: usize, rows: usize) -> Self {
        let columns = u32::try_from(columns).unwrap_or(u32::MAX);
        let rows = u32::try_from(rows).unwrap_or(u32::MAX);
        Self {
            columns: columns.clamp(MIN_TERMINAL_COLUMNS as u32, MAX_TERMINAL_COLUMNS as u32),
            rows: rows.clamp(MIN_TERMINAL_ROWS as u32, MAX_TERMINAL_ROWS as u32),
        }
    }

    pub fn backend(columns: u32, rows: u32) -> Self {
        Self {
            columns: columns.clamp(1, MAX_TERMINAL_COLUMNS as u32),
            rows: rows.clamp(1, MAX_TERMINAL_ROWS as u32),
        }
    }

    pub const fn columns(self) -> u32 {
        self.columns
    }

    pub const fn rows(self) -> u32 {
        self.rows
    }
}

pub fn validate_backend_size(columns: u32, rows: u32) -> Result<()> {
    if columns == 0 || rows == 0 {
        bail!("terminal dimensions must be greater than zero");
    }
    if columns > u32::from(MAX_TERMINAL_COLUMNS) || rows > u32::from(MAX_TERMINAL_ROWS) {
        bail!("terminal dimensions cannot exceed {MAX_TERMINAL_COLUMNS}x{MAX_TERMINAL_ROWS}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrors the Slint complete-row bottom anchor for geometry tests.
    const fn grid_top_offset(height: i32, rows: i32, cell_height: i32) -> i32 {
        height.saturating_sub(rows.saturating_mul(cell_height))
    }

    fn visible_rows(height: i32, cell_height: i32) -> i32 {
        height
            .saturating_div(cell_height)
            .clamp(i32::from(MIN_TERMINAL_ROWS), i32::from(MAX_TERMINAL_ROWS))
    }

    #[test]
    fn shared_limits_match_the_terminal_contract() {
        assert_eq!((MIN_TERMINAL_COLUMNS, MIN_TERMINAL_ROWS), (10, 3));
        assert_eq!((MAX_TERMINAL_COLUMNS, MAX_TERMINAL_ROWS), (300, 100));
    }

    #[test]
    fn model_and_backend_sizes_share_the_same_upper_bounds() {
        assert_eq!(
            TerminalSize::model(usize::MAX, usize::MAX),
            TerminalSize::backend(300, 100)
        );
        assert_eq!(TerminalSize::backend(0, 0), TerminalSize::backend(1, 1));
    }

    #[test]
    fn bottom_anchored_grid_keeps_normal_rows_complete() {
        assert_eq!(grid_top_offset(60, 3, 20), 0);
        assert_eq!(visible_rows(61, 20), 3);
        assert_eq!(grid_top_offset(61, 3, 20), 1);
        assert_eq!(grid_top_offset(61, 3, 20) + 3 * 20, 61);

        assert_eq!(visible_rows(59, 20), 3);
        assert_eq!(grid_top_offset(59, 3, 20), -1);

        assert_eq!(visible_rows(2_001, 20), i32::from(MAX_TERMINAL_ROWS));
        assert_eq!(grid_top_offset(2_001, 100, 20), 1);
        assert_eq!(grid_top_offset(2_001, 100, 20) + 100 * 20, 2_001);
    }
}
