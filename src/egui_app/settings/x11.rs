//! X server draft controls and a read-only startup discovery snapshot; no I/O.

use std::sync::Arc;

use super::super::i18n::tr;
use ax_ssh::config::{X11ServerProvider, X11Settings};
use ax_ssh::x_server;
use eframe::egui;

fn locations_id() -> egui::Id {
    egui::Id::unique("axssh-x11-locations")
}

pub(super) fn set_locations(ctx: &egui::Context, locations: Vec<String>) {
    ctx.data_mut(|data| data.insert_temp(locations_id(), Arc::<[String]>::from(locations)));
}

pub(super) fn show(ui: &mut egui::Ui, settings: &mut X11Settings) {
    ui.heading(tr!("LOCAL X SERVER", ui.ctx()));
    let options = x_server::provider_options();
    let selected = x_server::provider_index(settings.provider) as usize;
    ui.label(tr!("Provider", ui.ctx()));
    ui.label(tr!(
        "X server used by forwarded graphical applications",
        ui.ctx()
    ));
    egui::ComboBox::from_id_salt("x11-provider")
        .selected_text(
            options
                .get(selected)
                .map_or("", |label| provider_label(label, ui.ctx())),
        )
        .show_ui(ui, |ui| {
            for label in &options {
                let provider = if label == "System DISPLAY" {
                    X11ServerProvider::System
                } else {
                    X11ServerProvider::from_setting(label)
                };
                ui.selectable_value(
                    &mut settings.provider,
                    provider,
                    provider_label(label, ui.ctx()),
                );
            }
        })
        .response
        .accessible_name(tr!("Local X server provider", ui.ctx()));
    ui.label(tr!("Detected locations", ui.ctx()));
    let locations = ui
        .ctx()
        .data(|data| data.get_temp::<Arc<[String]>>(locations_id()))
        .unwrap_or_default();
    if locations.is_empty() {
        ui.label(tr!("No known local X server was found", ui.ctx()));
    }
    for location in locations.iter() {
        ui.label(location);
    }
    if settings.provider == X11ServerProvider::Custom {
        ui.label(tr!("Custom executable", ui.ctx()));
        ui.label(tr!(
            "Choose Custom to launch this path without a command shell",
            ui.ctx()
        ));
        ui.add(
            egui::TextEdit::singleline(&mut settings.app_path)
                .char_limit(1024)
                .hint_text(tr!("Executable path", ui.ctx())),
        )
        .accessible_name(tr!("Local X server application path", ui.ctx()));
    }
    ui.add_enabled_ui(
        x_server::provider_for_current_platform(settings.provider) != X11ServerProvider::System,
        |ui| {
            ui.checkbox(
                &mut settings.launch_on_connect,
                tr!("Start for first X11 application", ui.ctx()),
            );
            ui.label(tr!(
                "Launch the selected X server only when a forwarded graphical app opens",
                ui.ctx()
            ));
        },
    );
    ui.separator();
    ui.heading(tr!("COMPATIBILITY", ui.ctx()));
    ui.checkbox(
        &mut settings.allow_no_auth,
        tr!("Allow local connections without X authority", ui.ctx()),
    );
    ui.label(tr!(
        "Loopback only; required by MacXServer and -ac Windows servers",
        ui.ctx()
    ));
}

fn provider_label<'a>(label: &'a str, ctx: &egui::Context) -> &'a str {
    if label == "Custom" {
        tr!("Custom", ctx)
    } else {
        label
    }
}

#[cfg(test)]
mod tests;
