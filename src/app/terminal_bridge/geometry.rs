//! Terminal geometry application boundary.

use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct TerminalGeometrySample {
    pane_x: f32,
    pane_y: f32,
    pane_width: f32,
    pane_height: f32,
    grid_x: f32,
    grid_y: f32,
    grid_width: f32,
    grid_height: f32,
    cell_width: f32,
    cell_height: f32,
    columns: i32,
    rows: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TerminalGeometrySignature {
    window_physical_width: u32,
    window_physical_height: u32,
    winit_physical_width: u32,
    winit_physical_height: u32,
    scale_milli: u32,
    pane_x: i32,
    pane_y: i32,
    pane_width: i32,
    pane_height: i32,
    grid_x: i32,
    grid_y: i32,
    grid_width: i32,
    grid_height: i32,
    cell_width: i32,
    cell_height: i32,
    columns: i32,
    rows: i32,
    detached_window: bool,
    software_presentation_enabled: bool,
}

#[derive(Default)]
pub(super) struct TerminalGeometryDiagnostics {
    last: HashMap<String, TerminalGeometrySignature>,
}

impl TerminalGeometryDiagnostics {
    fn record(
        &mut self,
        ui: &AppWindow,
        window_id: Uuid,
        terminal_id: &str,
        sample: TerminalGeometrySample,
    ) {
        let window_size = ui.window().size();
        let scale_factor = f64::from(ui.window().scale_factor()).max(0.01);
        let (winit_physical_width, winit_physical_height) = ui
            .window()
            .with_winit_window(|window| {
                let size = window.inner_size();
                (size.width, size.height)
            })
            .unwrap_or((0, 0));
        let detached_window = ui.get_detached_window();
        let software_presentation_enabled = ui.get_software_presentation_enabled();
        let signature = TerminalGeometrySignature {
            window_physical_width: window_size.width,
            window_physical_height: window_size.height,
            winit_physical_width,
            winit_physical_height,
            scale_milli: quantize_scale(scale_factor),
            pane_x: quantize_logical(sample.pane_x),
            pane_y: quantize_logical(sample.pane_y),
            pane_width: quantize_logical(sample.pane_width),
            pane_height: quantize_logical(sample.pane_height),
            grid_x: quantize_logical(sample.grid_x),
            grid_y: quantize_logical(sample.grid_y),
            grid_width: quantize_logical(sample.grid_width),
            grid_height: quantize_logical(sample.grid_height),
            cell_width: quantize_logical(sample.cell_width),
            cell_height: quantize_logical(sample.cell_height),
            columns: sample.columns,
            rows: sample.rows,
            detached_window,
            software_presentation_enabled,
        };
        let key = format!("{window_id}:{terminal_id}");
        if self
            .last
            .get(&key)
            .is_some_and(|previous| *previous == signature)
        {
            return;
        }
        if !self.last.contains_key(&key) && self.last.len() >= 256 {
            self.last.clear();
        }
        self.last.insert(key, signature);

        let active_tab_kind = ui.get_active_tab_kind();
        let renderer_preference = ui.get_renderer_preference();
        let window_logical_width = f64::from(window_size.width) / scale_factor;
        let window_logical_height = f64::from(window_size.height) / scale_factor;
        let pane_right_gap =
            window_logical_width - f64::from(sample.pane_x) - f64::from(sample.pane_width);
        let pane_bottom_gap =
            window_logical_height - f64::from(sample.pane_y) - f64::from(sample.pane_height);
        let grid_right_gap = f64::from(sample.pane_width)
            - (f64::from(sample.grid_x) - f64::from(sample.pane_x))
            - f64::from(sample.grid_width);
        let grid_bottom_gap = f64::from(sample.pane_height)
            - (f64::from(sample.grid_y) - f64::from(sample.pane_y))
            - f64::from(sample.grid_height);
        let terminal_columns = sample.columns.max(0);
        let terminal_rows = sample.rows.max(0);
        let cell_remainder_width = f64::from(sample.grid_width)
            - f64::from(sample.cell_width) * f64::from(terminal_columns);
        let cell_remainder_height = f64::from(sample.grid_height)
            - f64::from(sample.cell_height) * f64::from(terminal_rows);

        tracing::debug!(
            target: "ax_ssh::diagnostics",
            event = "terminal-geometry",
            window_id = %window_id,
            terminal_id,
            active_tab_kind = active_tab_kind.as_str(),
            detached_window,
            renderer_preference = renderer_preference.as_str(),
            software_presentation_enabled,
            scale_factor,
            window_physical_width = window_size.width,
            window_physical_height = window_size.height,
            winit_physical_width,
            winit_physical_height,
            window_logical_width,
            window_logical_height,
            pane_x = sample.pane_x,
            pane_y = sample.pane_y,
            pane_width = sample.pane_width,
            pane_height = sample.pane_height,
            pane_right_gap,
            pane_bottom_gap,
            grid_x = sample.grid_x,
            grid_y = sample.grid_y,
            grid_width = sample.grid_width,
            grid_height = sample.grid_height,
            grid_right_gap,
            grid_bottom_gap,
            cell_width = sample.cell_width,
            cell_height = sample.cell_height,
            terminal_columns = sample.columns,
            terminal_rows = sample.rows,
            cell_remainder_width,
            cell_remainder_height,
            "terminal geometry changed"
        );
    }
}

pub(super) fn quantize_logical(value: f32) -> i32 {
    if !value.is_finite() {
        return i32::MIN;
    }
    (f64::from(value) * 10.0)
        .round()
        .clamp(f64::from(i32::MIN + 1), f64::from(i32::MAX)) as i32
}

pub(super) fn quantize_scale(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    (value * 1000.0).round().clamp(0.0, f64::from(u32::MAX)) as u32
}

pub(super) fn wire_geometry(ui: &AppWindow, window_id: Uuid) {
    let geometry_diagnostics = Rc::new(RefCell::new(TerminalGeometryDiagnostics::default()));
    let geometry_diagnostics_for_callback = geometry_diagnostics.clone();
    let ui_for_geometry = ui.as_weak();
    ui.on_terminal_geometry(
        move |terminal_id,
              pane_x,
              pane_y,
              pane_width,
              pane_height,
              grid_x,
              grid_y,
              grid_width,
              grid_height,
              cell_width,
              cell_height,
              columns,
              rows| {
            if let Some(ui) = ui_for_geometry.upgrade() {
                geometry_diagnostics_for_callback.borrow_mut().record(
                    &ui,
                    window_id,
                    terminal_id.as_str(),
                    TerminalGeometrySample {
                        pane_x,
                        pane_y,
                        pane_width,
                        pane_height,
                        grid_x,
                        grid_y,
                        grid_width,
                        grid_height,
                        cell_width,
                        cell_height,
                        columns,
                        rows,
                    },
                );
            }
        },
    );

    #[cfg(target_os = "macos")]
    {
        let ui_for_presentation = ui.as_weak();
        ui.on_terminal_presentation_layout(move |terminal_id, x, y, width, height, row_height| {
            if let Some(ui) = ui_for_presentation.upgrade() {
                software_presentation::update_region(
                    &ui,
                    window_id,
                    terminal_id.as_str(),
                    software_presentation::LogicalRegion {
                        x,
                        y,
                        width,
                        height,
                        row_height,
                    },
                );
            }
        });
        let ui_for_presentation_reset = ui.as_weak();
        ui.on_terminal_presentation_layout_reset(move || {
            if let Some(ui) = ui_for_presentation_reset.upgrade() {
                software_presentation::clear_layout(&ui, window_id);
            }
        });
    }
}
