//! Window-local software presentation layout registration.
//!
//! Slint reports terminal geometry in logical pixels, while the macOS
//! softbuffer backend partitions the physical framebuffer. This module owns
//! that conversion and keeps one bounded layout snapshot per application
//! window. Sidebar and tab geometry is intentionally absent; the backend uses
//! its normal fallback partition for those areas.

use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
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

#[derive(Clone)]
struct WindowLayout {
    rows_per_block: u32,
    regions: HashMap<String, LogicalRegion>,
}

static WINDOW_LAYOUTS: OnceLock<Mutex<HashMap<Uuid, WindowLayout>>> = OnceLock::new();
static SOFTWARE_PRESENTATION_ENABLED: AtomicBool = AtomicBool::new(false);

pub(super) fn set_enabled(enabled: bool) {
    SOFTWARE_PRESENTATION_ENABLED.store(enabled && cfg!(target_os = "macos"), Ordering::Release);
}

pub(super) fn is_enabled() -> bool {
    SOFTWARE_PRESENTATION_ENABLED.load(Ordering::Acquire)
}

fn layouts() -> &'static Mutex<HashMap<Uuid, WindowLayout>> {
    WINDOW_LAYOUTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn insert_layout(
    layouts: &mut HashMap<Uuid, WindowLayout>,
    window_id: Uuid,
    rows_per_block: u16,
) -> &mut WindowLayout {
    if !layouts.contains_key(&window_id) && layouts.len() >= 32 {
        let evicted = layouts
            .keys()
            .copied()
            .find(|candidate| *candidate != MAIN_WINDOW_ID);
        if let Some(evicted) = evicted {
            layouts.remove(&evicted);
        }
    }
    layouts.entry(window_id).or_insert_with(|| WindowLayout {
        rows_per_block: u32::from(rows_per_block),
        regions: HashMap::new(),
    })
}

pub(super) fn set_rows(ui: &AppWindow, window_id: Uuid, rows_per_block: u16) {
    if !is_enabled() {
        return;
    }
    let Ok(mut layouts) = layouts().lock() else {
        return;
    };
    let layout = insert_layout(&mut layouts, window_id, rows_per_block);
    layout.rows_per_block = u32::from(rows_per_block);
    let layout = layout.clone();
    drop(layouts);
    publish_layout(ui, &layout);
}

pub(super) fn update_region(
    ui: &AppWindow,
    window_id: Uuid,
    terminal_id: &str,
    region: LogicalRegion,
) {
    if !is_enabled() {
        return;
    }
    let Ok(mut layouts) = layouts().lock() else {
        return;
    };
    let Some(layout) = layouts.get_mut(&window_id) else {
        return;
    };
    if !layout.regions.contains_key(terminal_id) && layout.regions.len() >= 64 {
        return;
    }
    layout.regions.insert(terminal_id.to_owned(), region);
    let layout = layout.clone();
    drop(layouts);
    publish_layout(ui, &layout);
}

pub(super) fn clear_layout(ui: &AppWindow, window_id: Uuid) {
    if !is_enabled() {
        return;
    }
    let Ok(mut layouts) = layouts().lock() else {
        return;
    };
    let Some(layout) = layouts.get_mut(&window_id) else {
        return;
    };
    layout.regions.clear();
    let layout = layout.clone();
    drop(layouts);
    publish_layout(ui, &layout);
}

pub(super) fn refresh_layout(ui: &AppWindow, window_id: Uuid) {
    if !is_enabled() {
        return;
    }
    let Ok(layouts) = layouts().lock() else {
        return;
    };
    let Some(layout) = layouts.get(&window_id).cloned() else {
        return;
    };
    drop(layouts);
    publish_layout(ui, &layout);
}

pub(super) fn remove_layout(ui: &AppWindow, window_id: Uuid) {
    if !is_enabled() {
        return;
    }
    if let Ok(mut layouts) = layouts().lock() {
        layouts.remove(&window_id);
    }
    #[cfg(target_os = "macos")]
    softbuffer::remove_presentation_layout(window_key(ui));
    #[cfg(not(target_os = "macos"))]
    let _ = ui;
}

#[cfg(target_os = "macos")]
fn publish_layout(ui: &AppWindow, layout: &WindowLayout) {
    let scale = f64::from(ui.window().scale_factor()).max(1.0);
    let key = window_key(ui);
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
    softbuffer::set_presentation_layout(key, layout.rows_per_block, &regions);
}

#[cfg(target_os = "macos")]
fn window_key(ui: &AppWindow) -> u64 {
    use slint::winit_030::WinitWindowAccessor;

    ui.window()
        .with_winit_window(|window| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            window.id().hash(&mut hasher);
            hasher.finish()
        })
        .unwrap_or(0)
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn logical_region_edges_are_recomputed_at_retina_scale() {
        assert_eq!(physical_start(10.25, 2.0), Some(21));
        assert_eq!(physical_end(10.25, 100.5, 2.0), Some(222));
        assert_eq!(physical_length(17.25, 2.0), Some(35));
    }
}
