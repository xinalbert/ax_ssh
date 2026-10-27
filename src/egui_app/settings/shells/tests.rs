use super::*;
use crate::egui_app::i18n::{self, Translator};

#[test]
fn choices_keep_current_and_saved_values_under_the_domain_limit_without_mutation() {
    let settings = TerminalSettings {
        local_shell: "/unavailable/custom shell".into(),
        known_shells: (0..80)
            .map(|index| format!("saved-shell-{index}"))
            .collect(),
        ..Default::default()
    };
    let before = settings.clone();
    let candidate = with_options(&settings, &["newly-discovered".into()]);
    assert_eq!(settings, before);
    assert_eq!(candidate.known_shells.len(), 32);
    assert_eq!(
        candidate.known_shells[0],
        ax_ssh::config::SYSTEM_DEFAULT_SHELL
    );
    assert_eq!(candidate.known_shells[1], settings.local_shell);
    assert_eq!(candidate.local_shell, settings.local_shell);
    assert_eq!(candidate.scrollback_lines, settings.scrollback_lines);
    let settings = TerminalSettings {
        local_shell: " ZSH ".into(),
        known_shells: vec!["zsh".into(), "bad\nname".into(), "bash".into()],
        ..Default::default()
    };
    let candidate = with_options(&settings, &["BASH".into(), "pwsh".into(), "x".repeat(257)]);
    assert_eq!(candidate.local_shell, "ZSH");
    assert_eq!(
        candidate.known_shells,
        [ax_ssh::config::SYSTEM_DEFAULT_SHELL, "ZSH", "bash", "pwsh"]
    );
    assert_eq!(settings.local_shell, " ZSH ");
}

#[test]
fn discovery_snapshots_are_context_local_and_normalized_without_launching_candidates() {
    let first = egui::Context::default();
    let second = egui::Context::default();
    set_discovered(
        &first,
        vec![
            "fixture-shell".into(),
            "FIXTURE-SHELL".into(),
            "\n".into(),
            "x".repeat(257),
        ],
    );
    assert_eq!(
        &*discovered(&first),
        &[ax_ssh::config::SYSTEM_DEFAULT_SHELL, "fixture-shell"]
    );
    assert!(discovered(&second).is_empty());
    set_discovered(
        &first,
        (0..100).map(|index| format!("shell-{index}")).collect(),
    );
    assert_eq!(discovered(&first).len(), 32);
}

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
fn localized_shell_picker_selects_cached_and_discovered_values_and_keeps_unicode_input_bounded() {
    let fonts = crate::egui_app::fonts::load().unwrap();
    let mut stable = None;
    for language in ax_ssh::config::UiLanguage::bundled() {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts.clone());
        Translator::new(language, None).apply(&ctx);
        set_discovered(
            &ctx,
            vec!["fixture-discovered-shell".into(), "Settings".into()],
        );
        let saved = TerminalSettings {
            known_shells: vec![
                ax_ssh::config::SYSTEM_DEFAULT_SHELL.into(),
                "fixture-cached-shell".into(),
            ],
            ..Default::default()
        };
        let mut draft = saved.clone();
        let draw = |draft: &mut TerminalSettings, events, focus| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 700.0),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    if focus {
                        ui.memory_mut(|memory| {
                            memory.request_focus(ui.make_persistent_id("local-shell-value"))
                        });
                    }
                    show(ui, draft, &discovered(&ctx));
                },
            )
        };
        let initial = draw(&mut draft, vec![], false);
        for label in [
            "Local shell",
            "Use System default for the platform shell; otherwise enter an executable path.",
        ] {
            assert!(
                texts(&initial)
                    .iter()
                    .any(|(text, _)| text == i18n::current(&ctx).text("", label)),
                "{}: {label}",
                language.as_setting()
            );
        }
        assert_eq!(draft, saved);
        initial.drop_without_applying_deltas();
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
        for wanted in [
            "fixture-cached-shell",
            "fixture-discovered-shell",
            "Settings",
        ] {
            let output = draw(&mut draft, vec![], false);
            let position = texts(&output)
                .iter()
                .find(|(text, _)| text == &draft.local_shell)
                .unwrap()
                .1
                .center();
            output.drop_without_applying_deltas();
            for pressed in [true, false] {
                draw(&mut draft, click(position, pressed), false).drop_without_applying_deltas();
            }
            let output = draw(&mut draft, vec![], false);
            let position = texts(&output)
                .iter()
                .find(|(text, _)| text == wanted)
                .unwrap()
                .1
                .center();
            output.drop_without_applying_deltas();
            for pressed in [true, false] {
                draw(&mut draft, click(position, pressed), false).drop_without_applying_deltas();
            }
            assert_eq!(draft.local_shell, wanted, "{}", language.as_setting());
            assert_eq!(draft.known_shells, saved.known_shells);
        }
        draft.local_shell.clear();
        let output = draw(
            &mut draft,
            vec![egui::Event::Text(format!("{}AB", "界".repeat(255)))],
            true,
        );
        output.drop_without_applying_deltas();
        assert_eq!(draft.local_shell.chars().count(), 256);
        assert_eq!(draft.local_shell, format!("{}A", "界".repeat(255)));
        let id = ctx.memory(|memory| memory.focused()).unwrap();
        assert_eq!(*stable.get_or_insert(id), id);
        assert_eq!(saved.local_shell, ax_ssh::config::SYSTEM_DEFAULT_SHELL);
    }
}
