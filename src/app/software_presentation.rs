//! Window-local software presentation layout registration.
//!
//! Slint reports terminal geometry in logical pixels, while the macOS
//! softbuffer backend partitions the physical framebuffer. This module owns
//! that conversion and keeps one bounded layout snapshot per application
//! window. Sidebar and tab geometry is intentionally absent; the backend uses
//! its normal fallback partition for those areas.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use uuid::Uuid;

use super::*;

#[derive(Clone, Copy)]
pub(super) struct LogicalRegion {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) row_height: f32,
}

struct WindowLayout {
    rows_per_block: u32,
    regions: HashMap<String, LogicalRegion>,
}

impl Default for WindowLayout {
    fn default() -> Self {
        Self {
            rows_per_block: 4,
            regions: HashMap::new(),
        }
    }
}

static WINDOW_LAYOUTS: OnceLock<Mutex<HashMap<Uuid, WindowLayout>>> = OnceLock::new();

fn layouts() -> &'static Mutex<HashMap<Uuid, WindowLayout>> {
    WINDOW_LAYOUTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn layout_for(layouts: &mut HashMap<Uuid, WindowLayout>, window_id: Uuid) -> &mut WindowLayout {
    if !layouts.contains_key(&window_id) && layouts.len() >= 32 {
        let evicted = layouts
            .keys()
            .copied()
            .find(|candidate| *candidate != MAIN_WINDOW_ID);
        if let Some(evicted) = evicted {
            layouts.remove(&evicted);
        }
    }
    layouts.entry(window_id).or_default()
}

pub(super) fn set_rows(ui: &AppWindow, window_id: Uuid, rows_per_block: u16) {
    let Ok(mut layouts) = layouts().lock() else {
        return;
    };
    let layout = layout_for(&mut layouts, window_id);
    layout.rows_per_block = u32::from(rows_per_block.clamp(1, 16));
    publish_layout(ui, layout);
}

pub(super) fn update_region(
    ui: &AppWindow,
    window_id: Uuid,
    terminal_id: &str,
    region: LogicalRegion,
) {
    let Ok(mut layouts) = layouts().lock() else {
        return;
    };
    let layout = layout_for(&mut layouts, window_id);
    if !layout.regions.contains_key(terminal_id) && layout.regions.len() >= 64 {
        return;
    }
    layout.regions.insert(terminal_id.to_owned(), region);
    publish_layout(ui, layout);
}

pub(super) fn clear_layout(ui: &AppWindow, window_id: Uuid) {
    let Ok(mut layouts) = layouts().lock() else {
        return;
    };
    let layout = layout_for(&mut layouts, window_id);
    layout.regions.clear();
    publish_layout(ui, layout);
}

#[cfg(target_os = "macos")]
fn publish_layout(ui: &AppWindow, layout: &WindowLayout) {
    use slint::winit_030::WinitWindowAccessor;

    let scale = f64::from(ui.window().scale_factor()).max(1.0);
    let key = ui
        .window()
        .with_winit_window(|window| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            window.id().hash(&mut hasher);
            hasher.finish()
        })
        .unwrap_or(0);
    let regions = layout
        .regions
        .values()
        .filter_map(|region| {
            let x = physical_start(region.x, scale)?;
            let y = physical_start(region.y, scale)?;
            let end_x = physical_end(region.x, region.width, scale)?;
            let end_y = physical_end(region.y, region.height, scale)?;
            let row_height = physical_length(region.row_height, scale)?;
            (end_x > x && end_y > y && row_height > 0).then_some(softbuffer::PresentationRegion {
                x,
                y,
                width: end_x - x,
                height: end_y - y,
                row_height,
            })
        })
        .collect::<Vec<_>>();
    softbuffer::set_presentation_layout(key, layout.rows_per_block.max(1), &regions);
}

#[cfg(not(target_os = "macos"))]
fn publish_layout(_ui: &AppWindow, _layout: &WindowLayout) {}

#[cfg(target_os = "macos")]
fn physical_start(value: f32, scale: f64) -> Option<u32> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    u32::try_from((f64::from(value) * scale).round() as u64).ok()
}

#[cfg(target_os = "macos")]
fn physical_end(origin: f32, length: f32, scale: f64) -> Option<u32> {
    if !origin.is_finite() || !length.is_finite() || origin < 0.0 || length <= 0.0 {
        return None;
    }
    u32::try_from((f64::from(origin + length) * scale).round() as u64).ok()
}

#[cfg(target_os = "macos")]
fn physical_length(value: f32, scale: f64) -> Option<u32> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    u32::try_from((f64::from(value) * scale).round().max(1.0) as u64).ok()
}
