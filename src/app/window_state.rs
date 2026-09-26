//! UI-thread native geometry capture and display-aware restoration.

use ax_ssh::config::{WindowPlacement, WindowPosition};
use slint::ComponentHandle;
use slint::winit_030::{WinitWindowAccessor, winit::window::Window};

use super::{AppWindow, WindowRouter};
use uuid::Uuid;

const DEFAULT_WIDTH: u32 = 1180;
const DEFAULT_HEIGHT: u32 = 740;
// Mirrors the application window constraints in ui/theme.slint.
const MIN_WIDTH: u32 = 520;
const MIN_HEIGHT: u32 = 360;

pub(super) fn capture(
    window: &Window,
    previous: Option<WindowPlacement>,
) -> Option<WindowPlacement> {
    let size = window.inner_size();
    update_placement(
        previous,
        size.width,
        size.height,
        window.scale_factor(),
        window
            .outer_position()
            .ok()
            .map(|p| WindowPosition { x: p.x, y: p.y }),
        window.is_maximized(),
        window.is_minimized() == Some(true) || window.fullscreen().is_some(),
    )
}

#[allow(clippy::too_many_arguments)]
fn update_placement(
    previous: Option<WindowPlacement>,
    width: u32,
    height: u32,
    scale: f64,
    position: Option<WindowPosition>,
    maximized: bool,
    transient: bool,
) -> Option<WindowPlacement> {
    if transient || width == 0 || height == 0 || !scale.is_finite() || scale <= 0.0 {
        return previous;
    }
    let placement = if maximized {
        WindowPlacement {
            maximized: true,
            ..previous.unwrap_or(WindowPlacement {
                width: DEFAULT_WIDTH,
                height: DEFAULT_HEIGHT,
                position: None,
                maximized: false,
            })
        }
    } else {
        WindowPlacement {
            width: (f64::from(width) / scale).round().max(1.0) as u32,
            height: (f64::from(height) / scale).round().max(1.0) as u32,
            position,
            maximized: false,
        }
    };
    if placement.validate().is_ok() {
        Some(placement)
    } else {
        previous
    }
}

/// Use a bounded provisional size before native monitor enumeration is available.
pub(super) fn prepare(ui: &AppWindow, placement: Option<WindowPlacement>) {
    if let Some(p) = placement.filter(|p| p.validate().is_ok()) {
        ui.window().set_size(slint::LogicalSize::new(
            p.width.clamp(MIN_WIDTH, DEFAULT_WIDTH) as f32,
            p.height.clamp(MIN_HEIGHT, DEFAULT_HEIGHT) as f32,
        ));
    }
}

#[derive(Clone, Copy)]
struct Screen {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
}

/// Native monitor enumeration is only available once the window is shown.
/// Unsupported absolute positioning (Wayland) is left to the compositor.
pub(super) fn restore(
    ui: &AppWindow,
    placement: Option<WindowPlacement>,
    router: &WindowRouter,
    window_id: Uuid,
) {
    let Some(p) = placement.filter(|p| p.validate().is_ok()) else {
        return;
    };
    if ui.window().has_winit_window() {
        let adjusted = restore_shown(ui, p);
        router.set_placement(window_id, Some(adjusted));
    } else {
        let ui = ui.as_weak();
        let router = router.clone();
        slint::Timer::single_shot(std::time::Duration::ZERO, move || {
            if let Some(ui) = ui.upgrade() {
                let adjusted = restore_shown(&ui, p);
                router.set_placement(window_id, Some(adjusted));
            }
        });
    }
}

fn restore_shown(ui: &AppWindow, p: WindowPlacement) -> WindowPlacement {
    let adjusted = ui
        .window()
        .with_winit_window(|window| {
            let monitors = window.available_monitors().collect::<Vec<_>>();
            let preferred = window
                .primary_monitor()
                .or_else(|| window.current_monitor());
            let preferred =
                preferred.and_then(|monitor| monitors.iter().position(|m| *m == monitor));
            let screens = monitors
                .iter()
                .map(|monitor| {
                    let position = monitor.position();
                    let size = monitor.size();
                    Screen {
                        x: position.x,
                        y: position.y,
                        width: size.width,
                        height: size.height,
                        scale: monitor.scale_factor(),
                    }
                })
                .collect::<Vec<_>>();
            let can_position = window.outer_position().is_ok();
            fit_to_screens(p, &screens, preferred.unwrap_or(0), can_position)
        })
        .unwrap_or_else(|| fit_to_screens(p, &[], 0, false));
    ui.window().set_fullscreen(false);
    ui.window().set_minimized(false);
    ui.window().set_maximized(false);
    // Slint properties are applied lazily. Reset native zoom before changing
    // normal bounds when opening a workspace in an already-maximized window.
    ui.window().with_winit_window(|window| {
        if window.is_maximized() {
            window.set_maximized(false);
        }
    });
    ui.window().set_size(slint::LogicalSize::new(
        adjusted.width as f32,
        adjusted.height as f32,
    ));
    if let Some(position) = adjusted.position {
        ui.window()
            .set_position(slint::PhysicalPosition::new(position.x, position.y));
    }
    ui.window().set_maximized(adjusted.maximized);
    // An already-maximized Slint property can coalesce false -> true above.
    // Restore native zoom explicitly as well, keeping its normal restore frame.
    ui.window()
        .with_winit_window(|window| window.set_maximized(adjusted.maximized));
    // Seed fitted normal bounds after native reentrant events and before zoom
    // takes effect, so a provisional startup size cannot become the saved size.
    adjusted
}

fn fit_to_screens(
    mut placement: WindowPlacement,
    screens: &[Screen],
    fallback: usize,
    can_position: bool,
) -> WindowPlacement {
    placement.width = placement.width.max(MIN_WIDTH);
    placement.height = placement.height.max(MIN_HEIGHT);
    if !can_position {
        placement.position = None;
    }
    let screen = placement
        .position
        .and_then(|p| {
            screens.iter().find(|screen| {
                i64::from(p.x) >= i64::from(screen.x)
                    && i64::from(p.x) < i64::from(screen.x) + i64::from(screen.width)
                    && i64::from(p.y) >= i64::from(screen.y)
                    && i64::from(p.y) < i64::from(screen.y) + i64::from(screen.height)
            })
        })
        .or_else(|| screens.get(fallback))
        .or_else(|| screens.first());
    let Some(screen) =
        screen.filter(|s| s.scale.is_finite() && s.scale > 0.0 && s.width > 0 && s.height > 0)
    else {
        // Without display information, let the window manager choose a visible position.
        placement.position = None;
        placement.width = placement.width.min(DEFAULT_WIDTH);
        placement.height = placement.height.min(DEFAULT_HEIGHT);
        return placement;
    };
    // Winit exposes monitor bounds, not the portable work area. Leave room for
    // native decorations, menu bars and the taskbar/Dock on a smaller display.
    let available_width = ((f64::from(screen.width) / screen.scale) as u32).saturating_sub(16);
    let available_height = ((f64::from(screen.height) / screen.scale) as u32).saturating_sub(80);
    placement.width = placement.width.min(available_width.max(MIN_WIDTH));
    placement.height = placement.height.min(available_height.max(MIN_HEIGHT));
    if can_position && let Some(p) = placement.position {
        let left = i64::from(screen.x);
        let top = i64::from(screen.y) + (32.0 * screen.scale).round() as i64;
        let right = (left + i64::from(screen.width)
            - (f64::from(placement.width) * screen.scale).round() as i64
            - (8.0 * screen.scale).round() as i64)
            .max(left);
        let bottom = (i64::from(screen.y) + i64::from(screen.height)
            - (f64::from(placement.height) * screen.scale).round() as i64
            - (48.0 * screen.scale).round() as i64)
            .max(top);
        placement.position = Some(WindowPosition {
            x: i64::from(p.x)
                .clamp(left, right)
                .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            y: i64::from(p.y)
                .clamp(top, bottom)
                .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        });
    }
    placement
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal() -> WindowPlacement {
        WindowPlacement {
            width: 900,
            height: 600,
            position: Some(WindowPosition { x: -1600, y: 100 }),
            maximized: false,
        }
    }

    #[test]
    fn maximized_fullscreen_and_minimized_keep_normal_bounds() {
        let normal = normal();
        let maximized = update_placement(Some(normal), 3840, 2160, 2.0, None, true, false).unwrap();
        assert_eq!(
            maximized,
            WindowPlacement {
                maximized: true,
                ..normal
            }
        );
        assert_eq!(
            update_placement(Some(maximized), 3840, 2160, 2.0, None, false, true),
            Some(maximized)
        );
        assert_eq!(
            update_placement(Some(normal), 0, 0, 2.0, None, false, true),
            Some(normal)
        );
        let restored = update_placement(
            Some(maximized),
            1800,
            1200,
            2.0,
            normal.position,
            false,
            false,
        )
        .unwrap();
        assert_eq!(restored, normal);
    }

    #[test]
    fn secondary_monitor_negative_coordinates_and_dpi_are_preserved() {
        let secondary = Screen {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
            scale: 1.0,
        };
        let primary = Screen {
            x: 0,
            y: 0,
            width: 3840,
            height: 2160,
            scale: 2.0,
        };
        assert_eq!(
            fit_to_screens(normal(), &[primary, secondary], 0, true),
            normal()
        );
        let retina = WindowPlacement {
            position: Some(WindowPosition { x: 500, y: 200 }),
            ..normal()
        };
        assert_eq!(fit_to_screens(retina, &[primary], 0, true), retina);
    }

    #[test]
    fn unplugged_monitor_and_oversized_window_are_clamped() {
        let screen = Screen {
            x: 0,
            y: 0,
            width: 1280,
            height: 800,
            scale: 1.0,
        };
        let p = fit_to_screens(
            WindowPlacement {
                width: 4000,
                height: 2000,
                ..normal()
            },
            &[screen],
            0,
            true,
        );
        assert_eq!((p.width, p.height), (1264, 720));
        assert_eq!(p.position, Some(WindowPosition { x: 0, y: 32 }));
        let far = WindowPlacement {
            position: Some(WindowPosition {
                x: 900_000,
                y: 900_000,
            }),
            ..normal()
        };
        let p = fit_to_screens(far, &[screen], 0, true);
        assert_eq!(p.position, Some(WindowPosition { x: 372, y: 152 }));
    }

    #[test]
    fn unsupported_position_or_missing_monitors_uses_window_manager() {
        let screen = Screen {
            x: 0,
            y: 0,
            width: 1280,
            height: 800,
            scale: 1.0,
        };
        assert_eq!(fit_to_screens(normal(), &[screen], 0, false).position, None);
        assert_eq!(fit_to_screens(normal(), &[], 0, true).position, None);
    }
}
