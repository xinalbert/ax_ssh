use super::*;

#[test]
fn import_and_export_shortcut_editors_keep_distinct_ids_in_all_locales() {
    fn find(shape: &egui::epaint::Shape, text: &str) -> Option<egui::Rect> {
        match shape {
            egui::epaint::Shape::Text(shape) if shape.galley.job.text == text => {
                Some(shape.visual_bounding_rect())
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, text)),
            _ => None,
        }
    }
    let fonts = crate::egui_app::fonts::load().unwrap();
    let mut stable = [None, None];
    for language in ax_ssh::config::UiLanguage::bundled() {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts.clone());
        i18n::Translator::new(language, None).apply(&ctx);
        let mut shortcuts = ShortcutSettings::default();
        let original = shortcuts.clone();
        for (index, value) in [
            original.import_sessions.clone(),
            original.export_selected.clone(),
        ]
        .into_iter()
        .enumerate()
        {
            let draw = |shortcuts: &mut ShortcutSettings, events| {
                ctx.run_ui(
                    egui::RawInput {
                        focused: true,
                        events,
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1400.0, 1000.0),
                        )),
                        ..Default::default()
                    },
                    |ui| show(ui, shortcuts),
                )
            };
            let output = draw(&mut shortcuts, vec![]);
            let position = output
                .shapes
                .iter()
                .find_map(|shape| find(&shape.shape, &value))
                .unwrap()
                .center();
            output.drop_without_applying_deltas();
            for pressed in [true, false] {
                draw(
                    &mut shortcuts,
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                )
                .drop_without_applying_deltas();
            }
            let focus = ctx.memory(|memory| memory.focused()).unwrap();
            assert_eq!(*stable[index].get_or_insert(focus), focus);
            draw(&mut shortcuts, vec![egui::Event::Text("x".into())])
                .drop_without_applying_deltas();
            let changed = if index == 0 {
                &shortcuts.import_sessions
            } else {
                &shortcuts.export_selected
            };
            assert!(changed.contains('x'));
            shortcuts = original.clone();
        }
        assert_ne!(stable[0], stable[1]);
        assert_eq!(shortcuts, original);
    }
}

#[test]
fn all_saved_chords_reject_alias_order_and_reserved_conflicts() {
    assert_eq!(
        validation_key("Meta+Alt+M", false).unwrap(),
        validation_key("Alt+Ctrl+M", false).unwrap()
    );
    assert_eq!(
        validation_key("Cmd+Ctrl+M", false).unwrap(),
        validation_key("Ctrl+M", false).unwrap()
    );
    assert_ne!(
        validation_key("Meta+M", true).unwrap(),
        validation_key("Ctrl+M", true).unwrap()
    );
    let defaults = AppSettings::default();
    validate(&defaults).unwrap();
    for (import, export, sftp) in [
        ("Ctrl+Shift+I", "Ctrl+Shift+E", "Shift+Ctrl+I"),
        ("Ctrl+Shift+I", "Alt+Meta+E", "Cmd+Alt+E"),
        ("Cmd+Alt+I", "Alt+Meta+I", "Ctrl+M"),
        ("Ctrl+Shift+I", "Ctrl+Shift+E", "Ctrl+Tab"),
    ] {
        let mut settings = defaults.clone();
        settings.shortcuts.import_sessions = import.into();
        settings.shortcuts.export_selected = export.into();
        settings.shortcuts.open_sftp = sftp.into();
        assert!(validate(&settings).is_err(), "{import}, {export}, {sftp}");
    }
    let mut settings = defaults;
    settings.shortcuts.import_sessions = "Ctrl+NotAKey".into();
    assert!(validate(&settings).is_err());
}

#[test]
fn sftp_editor_accepts_bounded_input_with_stable_focus_in_every_locale() {
    fn find(shape: &egui::epaint::Shape) -> Option<egui::Rect> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.job.text == "Ctrl+M" => {
                Some(text.visual_bounding_rect())
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(find),
            _ => None,
        }
    }
    let fonts = crate::egui_app::fonts::load().unwrap();
    let mut stable = None;
    for language in ax_ssh::config::UiLanguage::bundled() {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts.clone());
        i18n::Translator::new(language, None).apply(&ctx);
        let mut shortcuts = ShortcutSettings::default();
        let before = shortcuts.clone();
        let draw = |shortcuts: &mut ShortcutSettings, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| show(ui, shortcuts),
            )
        };
        let output = draw(&mut shortcuts, vec![]);
        let position = output
            .shapes
            .iter()
            .find_map(|shape| find(&shape.shape))
            .unwrap()
            .center();
        output.drop_without_applying_deltas();
        for pressed in [true, false] {
            draw(
                &mut shortcuts,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            )
            .drop_without_applying_deltas();
        }
        let focused = ctx.memory(|memory| memory.focused()).unwrap();
        assert_eq!(*stable.get_or_insert(focused), focused);
        draw(&mut shortcuts, vec![egui::Event::Text("x".repeat(100))])
            .drop_without_applying_deltas();
        assert_eq!(shortcuts.open_sftp.chars().count(), 64);
        assert!(shortcuts.open_sftp.contains('x'));
        shortcuts.open_sftp = before.open_sftp.clone();
        assert_eq!(shortcuts, before);
    }
}
