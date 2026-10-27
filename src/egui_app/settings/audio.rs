//! Saved remote PCM format only; device ownership remains in the SSH/audio worker.

use super::super::i18n::tr;
use ax_ssh::config::AudioSettings;
use eframe::egui;

pub(super) fn show(ui: &mut egui::Ui, settings: &mut AudioSettings) {
    ui.separator();
    ui.heading(tr!("MICROPHONE", ui.ctx()));
    ui.label(tr!(
        "PCM audio forwarded to the Linux SSH session",
        ui.ctx()
    ));
    ui.label(tr!("Remote sample rate", ui.ctx()));
    egui::ComboBox::from_id_salt("remote-audio-rate")
        .selected_text(format!("{} Hz", settings.sample_rate))
        .show_ui(ui, |ui| {
            for rate in [
                8000, 16000, 22050, 24000, 32000, 44100, 48000, 96000, 192000,
            ] {
                ui.selectable_value(&mut settings.sample_rate, rate, format!("{rate} Hz"));
            }
        })
        .response
        .accessible_name(tr!("Remote sample rate", ui.ctx()));
    ui.label(tr!("Remote channels", ui.ctx()));
    egui::ComboBox::from_id_salt("remote-audio-channels")
        .selected_text(channel_label(settings.channels, ui.ctx()))
        .show_ui(ui, |ui| {
            for channels in [1, 2, 4, 6, 8] {
                ui.selectable_value(
                    &mut settings.channels,
                    channels,
                    channel_label(channels, ui.ctx()),
                );
            }
        })
        .response
        .accessible_name(tr!("Remote channels", ui.ctx()));
    ui.label(tr!(
        "Mono, stereo, or multi-channel; adapted from the local device",
        ui.ctx()
    ));
}

fn channel_label(channels: u16, ctx: &egui::Context) -> String {
    match channels {
        1 => tr!("Mono (1)", ctx).into(),
        2 => tr!("Stereo (2)", ctx).into(),
        _ => channels.to_string(),
    }
}

#[cfg(test)]
mod tests;
