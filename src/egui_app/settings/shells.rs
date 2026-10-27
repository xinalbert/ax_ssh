//! Local shell choices are immutable UI metadata; config owns their normalization.

use std::sync::Arc;

use ax_ssh::config::TerminalSettings;
use eframe::egui;

use super::tr;

pub(super) fn set_discovered(ctx: &egui::Context, found: Vec<String>) {
    let mut settings = TerminalSettings::default();
    settings.merge_known_shells(found);
    let choices: Arc<[String]> = settings.known_shells.into();
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique("local-shell-discovery"), choices));
}

pub(super) fn discovered(ctx: &egui::Context) -> Arc<[String]> {
    ctx.data(|data| data.get_temp(egui::Id::unique("local-shell-discovery")))
        .unwrap_or_default()
}

pub(super) fn with_options(settings: &TerminalSettings, available: &[String]) -> TerminalSettings {
    let mut candidate = settings.clone();
    // The selected executable must remain available even when the cache is full.
    // The domain inserts System default, validates names, deduplicates and caps.
    candidate.known_shells = vec![settings.local_shell.clone()];
    candidate.merge_known_shells(settings.known_shells.iter().chain(available).cloned());
    candidate
}

pub(super) fn show(ui: &mut egui::Ui, settings: &mut TerminalSettings, available: &[String]) {
    let label = tr!("Local shell", ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt("local-shell-choices")
            .selected_text(settings.local_shell.clone())
            .wrap_mode(egui::TextWrapMode::Truncate)
            .height(240.0)
            .show_ui(ui, |ui| {
                for choice in with_options(settings, available).known_shells {
                    if ui
                        .selectable_label(
                            settings.local_shell.eq_ignore_ascii_case(&choice),
                            &choice,
                        )
                        .on_hover_text(&choice)
                        .clicked()
                    {
                        settings.local_shell = choice;
                    }
                }
            })
            .response
            .accessible_name(label)
            .on_hover_text(&settings.local_shell);
    });
    let id = ui.make_persistent_id("local-shell-value");
    ui.add(
        egui::TextEdit::singleline(&mut settings.local_shell)
            .id(id)
            .char_limit(256),
    )
    .accessible_name(label);
    ui.label(tr!(
        "Use System default for the platform shell; otherwise enter an executable path.",
        ui.ctx()
    ));
}

#[cfg(test)]
mod tests;
