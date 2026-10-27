use super::*;
use crate::egui_app::i18n::Translator;

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
fn localized_audio_drafts_select_formats_and_preserve_legacy_engine_paths() {
    let fonts = crate::egui_app::fonts::load().unwrap();
    for language in ax_ssh::config::UiLanguage::bundled() {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts.clone());
        Translator::new(language, None).apply(&ctx);
        let mut draft = AudioSettings {
            whisper_executable: "/fixture/Settings".into(),
            whisper_model: "/fixture/Mono (1)".into(),
            ..Default::default()
        };
        let saved = draft.clone();
        let draw = |draft: &mut AudioSettings, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 1000.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| show(ui, draft),
            )
        };
        let click = |position, pressed| {
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let rates = [
            8000, 16000, 22050, 24000, 32000, 44100, 48000, 96000, 192000,
        ];
        let channels = [1, 2, 4, 6, 8];
        for (rate, wanted) in rates
            .into_iter()
            .map(|rate| (true, format!("{rate} Hz")))
            .chain(
                channels
                    .into_iter()
                    .map(|channels| (false, channel_label(channels, &ctx))),
            )
        {
            let selected = if rate {
                format!("{} Hz", draft.sample_rate)
            } else {
                channel_label(draft.channels, &ctx)
            };
            let output = draw(&mut draft, vec![]);
            let position = texts(&output)
                .into_iter()
                .find(|(text, _)| text == &selected)
                .unwrap()
                .1
                .center();
            output.drop_without_applying_deltas();
            for pressed in [true, false] {
                draw(&mut draft, click(position, pressed)).drop_without_applying_deltas();
            }
            if wanted == "192000 Hz" {
                // The final rate is below the popup's default scroll viewport.
                draw(
                    &mut draft,
                    vec![
                        egui::Event::PointerMoved(position + egui::vec2(0.0, 40.0)),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, -120.0),
                            phase: egui::TouchPhase::Move,
                            source: egui::MouseWheelSource::Unknown,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                )
                .drop_without_applying_deltas();
                for _ in 0..20 {
                    draw(&mut draft, vec![]).drop_without_applying_deltas();
                }
            }
            let output = draw(&mut draft, vec![]);
            let position = texts(&output)
                .into_iter()
                .find(|(text, _)| text == &wanted)
                .unwrap()
                .1
                .center();
            output.drop_without_applying_deltas();
            for pressed in [true, false] {
                draw(&mut draft, click(position, pressed)).drop_without_applying_deltas();
            }
            assert_eq!(
                if rate {
                    format!("{} Hz", draft.sample_rate)
                } else {
                    channel_label(draft.channels, &ctx)
                },
                wanted,
                "{}",
                language.as_setting()
            );
            assert!(draft.format().validate().is_ok());
        }
        assert_eq!((draft.sample_rate, draft.channels), (192000, 8));
        assert_eq!(draft.whisper_executable, saved.whisper_executable);
        assert_eq!(draft.whisper_model, saved.whisper_model);
        assert_eq!((saved.sample_rate, saved.channels), (48000, 2));
    }
}
