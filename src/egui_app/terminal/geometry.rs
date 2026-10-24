use ax_ssh::terminal_dimensions::TerminalSize;
use eframe::egui::{Pos2, Rect, Vec2};

#[derive(Clone, Copy)]
pub(super) struct Grid {
    pub(super) size: TerminalSize,
    pub(super) origin: Pos2,
    pub(super) cell: Vec2,
}

impl Grid {
    pub(super) fn new(rect: Rect, cell: Vec2, ppp: f32) -> Self {
        let columns = (rect.width() / cell.x).floor().max(0.0) as usize;
        let rows = (rect.height() / cell.y).floor().max(0.0) as usize;
        let normalized = TerminalSize::model(columns, rows);
        let size = TerminalSize::model_with_pixels(
            columns,
            rows,
            (normalized.columns() as f32 * cell.x * ppp).round() as u32,
            (normalized.rows() as f32 * cell.y * ppp).round() as u32,
        );
        let origin = Pos2::new(rect.left(), rect.bottom() - size.rows() as f32 * cell.y);
        Self { size, origin, cell }
    }

    pub(super) fn span(&self, row: usize, column: usize, cells: usize) -> Rect {
        Rect::from_min_size(
            self.origin + Vec2::new(column as f32 * self.cell.x, row as f32 * self.cell.y),
            Vec2::new(cells as f32 * self.cell.x, self.cell.y),
        )
    }

    pub(super) fn hit(&self, pos: Pos2) -> (usize, usize) {
        let offset = pos - self.origin;
        (
            (offset.y / self.cell.y)
                .floor()
                .clamp(0.0, self.size.rows().saturating_sub(1) as f32) as usize,
            (offset.x / self.cell.x)
                .floor()
                .clamp(0.0, self.size.columns().saturating_sub(1) as f32) as usize,
        )
    }

    pub(super) fn pixel_hit(&self, pos: Pos2, pixels_per_point: f32) -> (usize, usize) {
        let offset = pos - self.origin;
        let width = self.size.pixel_width().max(1) as f32;
        let height = self.size.pixel_height().max(1) as f32;
        let x = ((offset.x * pixels_per_point).floor() + 1.0).clamp(1.0, width) as usize;
        let y = ((offset.y * pixels_per_point).floor() + 1.0).clamp(1.0, height) as usize;
        (x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn residual_height_is_above_the_grid_and_hit_testing_uses_the_same_origin() {
        let grid = Grid::new(
            Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 487.0)),
            Vec2::new(10.0, 20.0),
            2.0,
        );
        assert_eq!((grid.size.columns(), grid.size.rows()), (80, 24));
        assert_eq!(grid.origin.y, 7.0);
        assert_eq!(grid.hit(Pos2::new(15.0, 28.0)), (1, 1));
        assert_eq!(grid.span(23, 0, 1).bottom(), 487.0);
        assert_eq!(
            (grid.size.pixel_width(), grid.size.pixel_height()),
            (1600, 960)
        );
        assert_eq!(grid.pixel_hit(Pos2::new(15.0, 28.0), 2.0), (31, 43));
        assert_eq!(grid.pixel_hit(Pos2::new(-50.0, -50.0), 2.0), (1, 1));
        assert_eq!(
            grid.pixel_hit(Pos2::new(10_000.0, 10_000.0), 2.0),
            (1600, 960)
        );
    }

    #[test]
    fn tiny_and_huge_windows_keep_shared_model_limits() {
        let tiny = Grid::new(
            Rect::from_min_size(Pos2::ZERO, Vec2::new(40.0, 15.0)),
            Vec2::new(10.0, 20.0),
            1.0,
        );
        assert_eq!((tiny.size.columns(), tiny.size.rows()), (10, 3));
        assert_eq!(tiny.origin.y, -45.0);
        let large = Grid::new(
            Rect::from_min_size(Pos2::ZERO, Vec2::splat(10_000.0)),
            Vec2::new(10.0, 20.0),
            1.0,
        );
        assert_eq!((large.size.columns(), large.size.rows()), (300, 100));
    }
}
