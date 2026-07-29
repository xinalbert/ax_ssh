//! Native macOS title-bar integration for the Slint window.

use anyhow::{Context, Result};
use objc2_app_kit::{NSView, NSWindowStyleMask, NSWindowTitleVisibility};
use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

pub(super) fn configure(window: &slint::Window) -> Result<()> {
    let handle = window.window_handle();
    let raw = handle
        .window_handle()
        .context("macOS window handle is not available")?
        .as_raw();
    let RawWindowHandle::AppKit(appkit) = raw else {
        anyhow::bail!("Slint did not create an AppKit window");
    };

    // SAFETY: raw-window-handle guarantees that `ns_view` points to the live
    // NSView owned by this window for the lifetime of the borrowed handle.
    let view = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
    let native_window = view.window().context("AppKit view has no NSWindow")?;
    native_window.setStyleMask(native_window.styleMask() | NSWindowStyleMask::FullSizeContentView);
    native_window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    native_window.setTitlebarAppearsTransparent(true);
    native_window.setMovableByWindowBackground(true);
    Ok(())
}
