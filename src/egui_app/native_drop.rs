//! Resolve the cursor at native drop delivery; path events have no coordinates.

use eframe::egui;
use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

pub(super) fn position(frame: &eframe::Frame, ctx: &egui::Context) -> Option<egui::Pos2> {
    if !ctx.input(|input| !input.raw.dropped_files.is_empty()) {
        return None;
    }
    let scale = f64::from(ctx.pixels_per_point());
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let handle = frame.window_handle().ok()?.as_raw();
    #[cfg(target_os = "macos")]
    if let RawWindowHandle::AppKit(handle) = handle {
        // SAFETY: eframe invokes this on its UI thread while Frame keeps the
        // native view alive; the shared reference is not retained after the call.
        let view = unsafe { handle.ns_view.cast::<objc2_app_kit::NSView>().as_ref() };
        let window = view.window()?;
        let point = view.convertPoint_fromView(window.mouseLocationOutsideOfEventStream(), None);
        let bounds = view.bounds();
        let x = point.x - bounds.origin.x;
        let y = if view.isFlipped() {
            point.y - bounds.origin.y
        } else {
            bounds.size.height - (point.y - bounds.origin.y)
        };
        let native = ctx
            .input(|input| input.viewport().native_pixels_per_point)
            .map(f64::from)?;
        return logical(x * native / scale, y * native / scale);
    }
    #[cfg(windows)]
    if let RawWindowHandle::Win32(handle) = handle {
        use windows_sys::Win32::{
            Foundation::{HWND, POINT},
            Graphics::Gdi::ScreenToClient,
            UI::WindowsAndMessaging::GetCursorPos,
        };
        let mut point = POINT::default();
        // SAFETY: Frame supplies a live HWND; both APIs write to the local POINT
        // synchronously and retain neither the pointer nor window handle.
        let ok = unsafe {
            GetCursorPos(&mut point) != 0
                && ScreenToClient(handle.hwnd.get() as HWND, &mut point) != 0
        };
        return ok
            .then(|| logical(f64::from(point.x) / scale, f64::from(point.y) / scale))
            .flatten();
    }
    #[cfg(target_os = "linux")]
    if let RawWindowHandle::Xlib(window) = handle {
        use raw_window_handle::{HasDisplayHandle as _, RawDisplayHandle};
        let RawDisplayHandle::Xlib(display) = frame.display_handle().ok()?.as_raw() else {
            return None;
        };
        let display = display.display?;
        let xlib = x11_dl::xlib::Xlib::open().ok()?;
        let mut root = 0;
        let mut child = 0;
        let mut rx = 0;
        let mut ry = 0;
        let mut x = 0;
        let mut y = 0;
        let mut mask = 0;
        // SAFETY: display/window are live Xlib handles from Frame, all ABI-sized
        // output values are local writable storage; XQueryPointer is synchronous.
        let ok = unsafe {
            (xlib.XQueryPointer)(
                display.as_ptr().cast(),
                window.window,
                &mut root,
                &mut child,
                &mut rx,
                &mut ry,
                &mut x,
                &mut y,
                &mut mask,
            ) != 0
        };
        return ok
            .then(|| logical(f64::from(x) / scale, f64::from(y) / scale))
            .flatten();
    }
    let _ = handle;
    // Wayland has no global pointer lookup. Accept only a PointerMoved delivered
    // with this frame, never a remembered active-tab/old-hover position.
    ctx.input(|input| {
        input.raw.events.iter().rev().find_map(|event| match event {
            egui::Event::PointerMoved(position) => Some(*position),
            _ => None,
        })
    })
}

fn logical(x: f64, y: f64) -> Option<egui::Pos2> {
    if !x.is_finite()
        || !y.is_finite()
        || x.abs() > f64::from(f32::MAX)
        || y.abs() > f64::from(f32::MAX)
    {
        return None;
    }
    Some(egui::pos2(x as f32, y as f32))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_native_coordinates_cannot_select_a_drop_target() {
        assert_eq!(logical(f64::NAN, 1.0), None);
        assert_eq!(logical(f64::INFINITY, 1.0), None);
        assert_eq!(logical(42.0, 24.0), Some(egui::pos2(42.0, 24.0)));
    }
}
