//! Platform cursor lookup used when routing native file-manager drops.
//!
//! Winit's `DroppedFile` event carries a path but no pointer position. Some
//! window systems suppress `CursorMoved` while an external drag is over the
//! window, so the drop route asks the platform for the position at delivery
//! time when that API is available. The caller still supplies the most recent
//! hover position as a bounded fallback for platforms without such an API.

use slint::winit_030::winit::window::Window;

#[cfg(target_os = "linux")]
use slint::winit_030::winit::raw_window_handle::{
    HasDisplayHandle as _, HasWindowHandle as _, RawDisplayHandle, RawWindowHandle,
};
#[cfg(target_os = "windows")]
use slint::winit_030::winit::raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

/// Resolve the best available window-relative logical cursor position.
pub(super) fn logical_position(
    _window: &Window,
    scale_factor: f64,
    hover_position: Option<(f32, f32)>,
) -> Option<(f32, f32)> {
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return None;
    }

    #[cfg(target_os = "windows")]
    if let Some(position) = windows_physical_position(_window) {
        return physical_to_logical(position, scale_factor);
    }

    #[cfg(target_os = "linux")]
    if let Some(position) = x11_physical_position(_window) {
        return physical_to_logical(position, scale_factor);
    }

    hover_position
}

fn physical_to_logical((x, y): (f64, f64), scale_factor: f64) -> Option<(f32, f32)> {
    let x = x / scale_factor;
    let y = y / scale_factor;
    if !x.is_finite()
        || !y.is_finite()
        || x < f64::from(f32::MIN)
        || x > f64::from(f32::MAX)
        || y < f64::from(f32::MIN)
        || y > f64::from(f32::MAX)
    {
        return None;
    }
    Some((x as f32, y as f32))
}

#[cfg(target_os = "windows")]
fn windows_physical_position(window: &Window) -> Option<(f64, f64)> {
    use windows_sys::Win32::Foundation::{HWND, POINT};
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let handle = window.window_handle().ok()?.as_raw();
    let RawWindowHandle::Win32(handle) = handle else {
        return None;
    };
    let hwnd = handle.hwnd.get() as HWND;
    let mut point = POINT::default();
    // SAFETY: `hwnd` is the live Winit window handle and `point` is a valid
    // writable Win32 POINT for both calls. The calls do not retain pointers.
    let ok = unsafe { GetCursorPos(&mut point) != 0 && ScreenToClient(hwnd, &mut point) != 0 };
    ok.then_some((f64::from(point.x), f64::from(point.y)))
}

#[cfg(target_os = "linux")]
fn x11_physical_position(window: &Window) -> Option<(f64, f64)> {
    use std::os::raw::{c_int, c_uint, c_ulong};

    let window_handle = window.window_handle().ok()?.as_raw();
    let display_handle = window.display_handle().ok()?.as_raw();
    let (display, x11_window) = match (display_handle, window_handle) {
        (RawDisplayHandle::Xlib(display), RawWindowHandle::Xlib(window)) => {
            (display.display?.as_ptr().cast(), window.window)
        }
        _ => return None,
    };

    let xlib = x11_dl::xlib::Xlib::open().ok()?;
    let mut root: c_ulong = 0;
    let mut child: c_ulong = 0;
    let mut root_x: c_int = 0;
    let mut root_y: c_int = 0;
    let mut window_x: c_int = 0;
    let mut window_y: c_int = 0;
    let mut mask: c_uint = 0;
    // SAFETY: The display and window come from the live Winit X11 handles.
    // Every output pointer refers to a local value with the ABI types required
    // by XQueryPointer. The call is synchronous and retains no pointer.
    let found = unsafe {
        (xlib.XQueryPointer)(
            display,
            x11_window,
            &mut root,
            &mut child,
            &mut root_x,
            &mut root_y,
            &mut window_x,
            &mut window_y,
            &mut mask,
        ) != 0
    };
    found.then_some((f64::from(window_x), f64::from(window_y)))
}

#[cfg(test)]
mod tests {
    use super::physical_to_logical;

    #[test]
    fn physical_position_uses_window_scale() {
        assert_eq!(physical_to_logical((80.0, 48.0), 2.0), Some((40.0, 24.0)));
    }

    #[test]
    fn physical_position_rejects_invalid_values() {
        assert_eq!(physical_to_logical((80.0, 48.0), 0.0), None);
        assert_eq!(physical_to_logical((f64::NAN, 48.0), 2.0), None);
    }
}
