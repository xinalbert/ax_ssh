//! Icon drafts follow the same preview and atomic Save flow as appearance.

use super::tr;
use ax_ssh::config::{AppearanceSettings, system_icon_choices, system_icon_id};
use eframe::egui;

pub(super) fn show(
    ui: &mut egui::Ui,
    draft: &mut AppearanceSettings,
    icons: &mut super::super::system_icons::Icons,
) {
    ui.checkbox(
        &mut draft.system_icons_follow_theme,
        tr!("System icons follow theme", ui.ctx()),
    );
    ui.horizontal_wrapped(|ui| {
        let label = tr!("Local shell icon", ui.ctx());
        ui.label(label);
        let selected = draft
            .local_shell_icon_override
            .as_deref()
            .and_then(system_icon_id);
        let name = system_icon_choices()
            .iter()
            .find(|choice| Some(choice.id) == selected)
            .map_or_else(|| tr!("Automatic", ui.ctx()), |choice| choice.name);
        if let Some(image) = icons.image(
            ui,
            super::super::system_icons::local_icon(draft),
            draft.system_icons_follow_theme,
        ) {
            ui.add(image);
        }
        egui::ComboBox::from_id_salt("local-shell-icon")
            .selected_text(name)
            .height(240.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut draft.local_shell_icon_override,
                    None,
                    tr!("Automatic", ui.ctx()),
                );
                for choice in system_icon_choices() {
                    ui.push_id(choice.id, |ui| {
                        if ui
                            .selectable_label(Some(choice.id) == selected, choice.name)
                            .clicked()
                        {
                            draft.local_shell_icon_override = Some(choice.id.into());
                        }
                    });
                }
            })
            .response
            .accessible_name(label);
    });
    ui.label(tr!(
        "Uses the host system by default. Save applies the selection.",
        ui.ctx()
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egui_app::i18n::{self, Translator};

    fn texts(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
        fn visit(shape: &egui::epaint::Shape, found: &mut Vec<(String, egui::Rect)>) {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    found.push((text.galley.job.text.clone(), text.visual_bounding_rect()))
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        visit(shape, found);
                    }
                }
                _ => {}
            }
        }
        let mut found = Vec::new();
        for shape in &output.shapes {
            visit(&shape.shape, &mut found);
        }
        found
    }

    #[test]
    fn localized_icon_controls_toggle_select_and_restore_automatic_without_mutating_saved_values() {
        let fonts = crate::egui_app::fonts::load().unwrap();
        for language in ax_ssh::config::UiLanguage::bundled() {
            let ctx = egui::Context::default();
            ctx.set_fonts(fonts.clone());
            Translator::new(language, None).apply(&ctx);
            let saved = AppearanceSettings::default();
            let mut draft = saved.clone();
            let mut icons = super::super::super::system_icons::Icons::default();
            let mut draw = |draft: &mut AppearanceSettings, events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1600.0, 700.0),
                        )),
                        focused: true,
                        events,
                        ..Default::default()
                    },
                    |ui| show(ui, draft, &mut icons),
                )
            };
            let click = |position, pressed| {
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        pressed,
                        button: egui::PointerButton::Primary,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            let initial = draw(&mut draft, vec![]);
            let found = texts(&initial);
            let checkbox = found
                .iter()
                .find(|(text, _)| text == i18n::current(&ctx).text("", "System icons follow theme"))
                .unwrap()
                .1
                .center();
            for label in [
                "Local shell icon",
                "Uses the host system by default. Save applies the selection.",
            ] {
                assert!(
                    found
                        .iter()
                        .any(|(text, _)| text == i18n::current(&ctx).text("", label)),
                    "{}: {label}",
                    language.as_setting()
                );
            }
            initial.drop_without_applying_deltas();
            for pressed in [true, false] {
                draw(&mut draft, click(checkbox, pressed)).drop_without_applying_deltas();
            }
            assert!(draft.system_icons_follow_theme);
            for wanted in ["Absolute Linux", "Automatic"] {
                let selected = draft
                    .local_shell_icon_override
                    .as_deref()
                    .and_then(system_icon_id)
                    .and_then(|id| system_icon_choices().iter().find(|choice| choice.id == id))
                    .map_or_else(
                        || i18n::current(&ctx).text("", "Automatic"),
                        |choice| choice.name,
                    );
                let output = draw(&mut draft, vec![]);
                let position = texts(&output)
                    .iter()
                    .find(|(text, _)| text == selected)
                    .unwrap()
                    .1
                    .center();
                output.drop_without_applying_deltas();
                for pressed in [true, false] {
                    draw(&mut draft, click(position, pressed)).drop_without_applying_deltas();
                }
                let output = draw(&mut draft, vec![]);
                let wanted = if wanted == "Automatic" {
                    i18n::current(&ctx).text("", wanted)
                } else {
                    wanted
                };
                let position = texts(&output)
                    .iter()
                    .find(|(text, _)| text == wanted)
                    .unwrap()
                    .1
                    .center();
                output.drop_without_applying_deltas();
                for pressed in [true, false] {
                    draw(&mut draft, click(position, pressed)).drop_without_applying_deltas();
                }
                assert_eq!(
                    draft.local_shell_icon_override.as_deref(),
                    if wanted == "Absolute Linux" {
                        Some("absolute")
                    } else {
                        None
                    },
                    "{}",
                    language.as_setting()
                );
            }
            assert!(!saved.system_icons_follow_theme);
            assert!(saved.local_shell_icon_override.is_none());
        }
    }
}
