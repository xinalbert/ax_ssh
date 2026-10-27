//! macOS Option interpretation belongs to the native window, before Text events.
use eframe::egui;
use winit::platform::macos::{OptionAsAlt, WindowExtMacOS};

fn id() -> egui::Id {
    egui::Id::unique("terminal-native-option-meta")
}

pub(super) fn active(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(id()).unwrap_or(false))
}

pub(super) fn update(ctx: &egui::Context, frame: &eframe::Frame, enabled: bool) {
    let enabled = enabled && ctx.input(|input| input.focused);
    let applied = if let Some(window) = frame.winit_window() {
        let policy = if enabled {
            OptionAsAlt::Both
        } else {
            OptionAsAlt::None
        };
        if window.option_as_alt() != policy {
            window.set_option_as_alt(policy);
        }
        enabled
    } else {
        false
    };
    ctx.data_mut(|data| data.insert_temp(id(), applied));
}

#[cfg(test)]
pub(super) fn emulate(ctx: &egui::Context, enabled: bool) {
    ctx.data_mut(|data| data.insert_temp(id(), enabled));
}
