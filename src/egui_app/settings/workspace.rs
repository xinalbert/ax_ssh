//! Workspace presentation drafts; config owns normalization and persistence.

use super::tr;
use ax_ssh::config::{self, WorkspaceSettings};
use eframe::egui;

pub(super) fn show(ui: &mut egui::Ui, draft: &mut WorkspaceSettings) {
    ui.add(
        egui::Slider::new(
            &mut draft.sidebar_width,
            config::MIN_SIDEBAR_WIDTH..=config::MAX_SIDEBAR_WIDTH,
        )
        .text(tr!("Session sidebar width", ui.ctx())),
    );
    ui.add(
        egui::Slider::new(
            &mut draft.tab_width,
            config::MIN_TAB_WIDTH..=config::MAX_TAB_WIDTH,
        )
        .text(tr!("Workspace tab width", ui.ctx())),
    );
    let mask_id = ui.make_persistent_id("session-mask-character");
    ui.horizontal_wrapped(|ui| {
        let label = tr!("Session mask character", ui.ctx());
        ui.label(label);
        ui.add(
            egui::TextEdit::singleline(&mut draft.session_mask_character)
                .id(mask_id)
                .char_limit(1)
                .desired_width(48.0),
        )
        .accessible_name(label);
    });
    ui.label(tr!(
        "Masks usernames and the middle IPv4 octets in the session sidebar",
        ui.ctx()
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egui_app::i18n::{self, Translator};
    use ax_ssh::config::UiLanguage;

    #[test]
    fn translated_mask_control_keeps_unicode_input_to_one_character() {
        fn contains(shape: &egui::epaint::Shape, expected: &str) -> bool {
            match shape {
                egui::epaint::Shape::Text(text) => text.galley.job.text == expected,
                egui::epaint::Shape::Vec(shapes) => {
                    shapes.iter().any(|shape| contains(shape, expected))
                }
                _ => false,
            }
        }
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::egui_app::fonts::load().unwrap());
        let mut stable_id = None;
        for language in UiLanguage::bundled() {
            Translator::new(language, None).apply(&ctx);
            let mut draft = WorkspaceSettings {
                session_mask_character: String::new(),
                ..Default::default()
            };
            let output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    events: vec![egui::Event::Text("•#".into())],
                    ..Default::default()
                },
                |ui| {
                    let id = ui.make_persistent_id("session-mask-character");
                    assert_eq!(*stable_id.get_or_insert(id), id);
                    ui.memory_mut(|memory| memory.request_focus(id));
                    show(ui, &mut draft);
                },
            );
            assert_eq!(
                draft.session_mask_character,
                "•",
                "{}",
                language.as_setting()
            );
            for label in [
                "Session mask character",
                "Masks usernames and the middle IPv4 octets in the session sidebar",
            ] {
                assert!(
                    output
                        .shapes
                        .iter()
                        .any(|shape| contains(&shape.shape, i18n::current(&ctx).text("", label))),
                    "{}: {label}",
                    language.as_setting()
                );
            }
            output.drop_without_applying_deltas();
        }
    }
}
