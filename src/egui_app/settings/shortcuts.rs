//! Editable commands only; validate all persisted chords, including future actions.
use super::super::{
    i18n::{self, mark, tr},
    menus::{Command, parse_shortcut},
};
use anyhow::Result;
use ax_ssh::config::{AppSettings, ShortcutSettings};
use eframe::egui;

pub(super) fn show(ui: &mut egui::Ui, shortcuts: &mut ShortcutSettings) {
    for (index, (label, text)) in [
        (mark!("Open Settings"), &mut shortcuts.open_settings),
        (mark!("New Server"), &mut shortcuts.new_session),
        (
            mark!("Toggle Session Sidebar"),
            &mut shortcuts.toggle_sidebar,
        ),
        (
            mark!("Copy Terminal Selection"),
            &mut shortcuts.copy_selection,
        ),
        (mark!("Paste Into Terminal"), &mut shortcuts.paste),
        (mark!("SFTP"), &mut shortcuts.open_sftp),
        (
            mark!("Import from Clipboard"),
            &mut shortcuts.import_sessions,
        ),
        (mark!("Export Selected"), &mut shortcuts.export_selected),
    ]
    .into_iter()
    .enumerate()
    {
        ui.push_id(index, |ui| {
            ui.horizontal_wrapped(|ui| {
                let label = i18n::current(ui.ctx()).text("", label);
                let label = ui.label(label);
                ui.add(egui::TextEdit::singleline(text).char_limit(64))
                    .labelled_by(label.id);
            });
        });
    }
    ui.label(tr!("Modifier keys", ui.ctx()));
    ui.monospace("Ctrl, Cmd/Meta, Alt, Shift · Ctrl+N, Cmd+,");
    for (label, keys) in [
        (mark!("Previous Tab"), "Ctrl+Shift+Tab"),
        (mark!("Next Tab"), "Ctrl+Tab"),
        (mark!("Close Current Tab"), "Cmd/Ctrl+W"),
        (mark!("Split Columns"), "Cmd/Ctrl+D"),
        (mark!("Split Rows"), "Cmd/Ctrl+Shift+D"),
    ] {
        ui.label(format!(
            "{}: {keys}",
            i18n::current(ui.ctx()).text("", label)
        ));
    }
}

pub(super) fn validate(settings: &AppSettings) -> Result<()> {
    let shortcuts = &settings.shortcuts;
    shortcuts.validate()?;
    let mut keys = Vec::new();
    for text in [
        &shortcuts.open_settings,
        &shortcuts.new_session,
        &shortcuts.import_sessions,
        &shortcuts.export_selected,
        &shortcuts.toggle_sidebar,
        &shortcuts.copy_selection,
        &shortcuts.paste,
        &shortcuts.open_sftp,
    ] {
        let key = validation_key(text, cfg!(target_os = "macos"))?;
        anyhow::ensure!(!keys.contains(&key), "shortcuts resolve to the same key");
        for command in [
            Command::SplitRight,
            Command::SplitDown,
            Command::PreviousTab,
            Command::NextTab,
            Command::CloseTab,
            Command::Quit,
        ] {
            let reserved = command
                .shortcut(settings)
                .and_then(|key| validation_key(&key, cfg!(target_os = "macos")).ok());
            anyhow::ensure!(
                reserved != Some(key),
                "shortcut is reserved by a pane, tab or window command"
            );
        }
        keys.push(key);
    }
    Ok(())
}

fn validation_key(text: &str, macos: bool) -> Result<egui::KeyboardShortcut> {
    let mut key = parse_shortcut(text)?;
    // egui maps command to Ctrl on Windows/Linux, so the saved aliases collide.
    if !macos && key.modifiers.command {
        key.modifiers.ctrl = true;
        key.modifiers.command = false;
    }
    Ok(key)
}

#[cfg(test)]
mod tests;
