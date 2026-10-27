use super::*;
use crate::egui_app::i18n::{Translator, current};

fn texts(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
    fn collect(shape: &egui::epaint::Shape, result: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                result.push((text.galley.job.text.clone(), text.visual_bounding_rect()))
            }
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, result);
                }
            }
            _ => {}
        }
    }
    let mut result = Vec::new();
    for shape in &output.shapes {
        collect(&shape.shape, &mut result);
    }
    result
}

#[test]
fn localized_x11_controls_preserve_paths_and_only_edit_the_draft() {
    let fonts = crate::egui_app::fonts::load().unwrap();
    for language in ax_ssh::config::UiLanguage::bundled() {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts.clone());
        Translator::new(language, None).apply(&ctx);
        let location = "/fixture/Settings XServer.app";
        set_locations(&ctx, vec![location.into()]);
        let mut draft = X11Settings {
            provider: X11ServerProvider::Custom,
            app_path: "/fixture/Untrusted (-X)".into(),
            launch_on_connect: false,
            allow_no_auth: false,
        };
        let saved = draft.clone();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1200.0),
                )),
                ..Default::default()
            },
            |ui| show(ui, &mut draft),
        );
        let painted = texts(&output);
        for label in [
            "LOCAL X SERVER",
            "Detected locations",
            "Custom executable",
            "Allow local connections without X authority",
        ] {
            assert!(
                painted
                    .iter()
                    .any(|(text, _)| text == current(&ctx).text("", label)),
                "{}: {label}",
                language.as_setting()
            );
        }
        assert!(painted.iter().any(|(text, _)| text == location));
        assert!(painted.iter().any(|(text, _)| text == &draft.app_path));
        assert_eq!(draft, saved);
        let position = painted
            .iter()
            .find(|(text, _)| {
                text == current(&ctx).text("", "Allow local connections without X authority")
            })
            .unwrap()
            .1
            .center();
        output.drop_without_applying_deltas();
        for pressed in [true, false] {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 1200.0),
                    )),
                    events: vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| show(ui, &mut draft),
            );
            output.drop_without_applying_deltas();
        }
        assert!(draft.allow_no_auth);
        assert!(!saved.allow_no_auth);
        assert_eq!(draft.app_path, saved.app_path);
        assert!(!draft.launch_on_connect);
    }
}
