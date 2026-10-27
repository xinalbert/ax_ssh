use super::*;

#[test]
fn locale_resolution_and_context_fallback_reuse_the_domain_policy() {
    let system = Translator::new(UiLanguage::SYSTEM, Some("zh-Hant-HK"));
    assert_eq!(system.text("", "Settings"), "設定");
    let british = Translator::new(UiLanguage::from_setting("en-GB"), None);
    assert_eq!(
        british.text("", "Source and licenses"),
        "Source and licences"
    );
    assert_eq!(system.text("menu", "Settings"), "Settings");
    assert_eq!(
        system.text("", "untranslated technical detail"),
        "untranslated technical detail"
    );
    assert_eq!(
        Translator::new(UiLanguage::SYSTEM, None),
        Translator::default()
    );
}

#[test]
fn every_bundled_table_is_complete_sorted_and_context_qualified() {
    let expected = catalog::messages("zh-CN");
    assert!(!expected.is_empty());
    for language in UiLanguage::bundled().filter(|language| language.as_setting() != "en") {
        let messages = catalog::messages(language.as_setting());
        assert_eq!(messages.len(), expected.len(), "{}", language.as_setting());
        assert!(
            messages
                .windows(2)
                .all(|pair| (pair[0].0, pair[0].1) < (pair[1].0, pair[1].1))
        );
        for ((context, source, text), expected) in messages.iter().zip(expected) {
            assert_eq!((*context, *source), (expected.0, expected.1));
            assert!(!text.trim().is_empty());
        }
    }
}

#[test]
fn language_is_local_to_the_context_and_keyboard_actions_are_consumed_once() {
    use crate::egui_app::menus::{self, Command};

    let first = egui::Context::default();
    let second = egui::Context::default();
    Translator::new(UiLanguage::from_setting("zh-CN"), None).apply(&first);
    Translator::new(UiLanguage::from_setting("ja"), None).apply(&second);
    assert_eq!(current(&first).text("", "Settings"), "设置");
    assert_eq!(current(&second).text("", "Settings"), "設定");
    let settings = ax_ssh::config::AppSettings::default();
    let shortcut = menus::parse_shortcut(&Command::Settings.shortcut(&settings).unwrap()).unwrap();
    for language in UiLanguage::bundled() {
        Translator::new(language, None).apply(&first);
        let events = [true, false].map(|pressed| egui::Event::Key {
            key: shortcut.logical_key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: shortcut.modifiers,
        });
        let output = first.run_ui(
            egui::RawInput {
                events: events.into(),
                ..Default::default()
            },
            |ui| {
                assert_eq!(
                    menus::keyboard(ui, &settings, false, |_| true),
                    Some(Command::Settings)
                );
                assert_eq!(menus::keyboard(ui, &settings, false, |_| true), None);
            },
        );
        output.drop_without_applying_deltas();
        assert_eq!(current(&second).text("", "Settings"), "設定");
    }
}
