//! Exercise the patched backend's real resize helpers through the application
//! dependency graph. This avoids the upstream backend's GUI-only test harness
//! and runs the Windows policy cases on every native CI runner.

use slint::winit_030::winit;

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/vendor/i-slint-backend-winit/drag_resize_window.rs"
));
