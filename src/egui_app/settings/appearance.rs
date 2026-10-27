//! Presentation drafts only; workspace/config retain save and normalization.

use super::tr;
use ax_ssh::config::{self, AppearanceSettings, ThemePalette};
use eframe::egui;

pub(super) fn refresh_rates(ui: &mut egui::Ui, draft: &mut AppearanceSettings) {
    for (id, label, description, accessible, value) in [
        (
            "focused",
            tr!("Focused refresh rate", ui.ctx()),
            tr!("Terminal updates while the pane is focused", ui.ctx()),
            tr!("Focused terminal refresh rate", ui.ctx()),
            &mut draft.focused_terminal_refresh_fps,
        ),
        (
            "unfocused",
            tr!("Unfocused refresh rate", ui.ctx()),
            tr!(
                "Visible terminal updates while another pane is focused",
                ui.ctx()
            ),
            tr!("Unfocused terminal refresh rate", ui.ctx()),
            &mut draft.unfocused_terminal_refresh_fps,
        ),
    ] {
        ui.push_id(("terminal-refresh-rate", id), |ui| {
            ui.add(
                egui::Slider::new(value, 1..=120)
                    .text(label)
                    .suffix(format!(" {}", tr!("FPS", ui.ctx()))),
            )
            .accessible_name(accessible)
            .on_hover_text(description);
        });
    }
}

pub(super) fn theme_palette(ui: &mut egui::Ui, mode: &'static str, palette: &mut ThemePalette) {
    let title = if mode == "light" {
        tr!("CUSTOM LIGHT PALETTE", ui.ctx())
    } else {
        tr!("CUSTOM DARK PALETTE", ui.ctx())
    };
    ui.heading(title);
    ui.label(tr!(
        "Enter #RRGGBB or #RRGGBBAA values. Unsafe contrast falls back to readable colors when saved.",
        ui.ctx()
    ));
    for (id, label, description, value) in [
        (
            "background",
            tr!("Background", ui.ctx()),
            tr!("Primary workspace surface", ui.ctx()),
            &mut palette.background,
        ),
        (
            "panel",
            tr!("Panel", ui.ctx()),
            tr!("Navigation and title surfaces", ui.ctx()),
            &mut palette.panel,
        ),
        (
            "panel-alt",
            tr!("Panel alternative", ui.ctx()),
            tr!("Hover and secondary surfaces", ui.ctx()),
            &mut palette.panel_alt,
        ),
        (
            "border",
            tr!("Border", ui.ctx()),
            tr!("Dividers and control outlines", ui.ctx()),
            &mut palette.border,
        ),
        (
            "text",
            tr!("Text", ui.ctx()),
            tr!("Primary readable text", ui.ctx()),
            &mut palette.text,
        ),
        (
            "muted",
            tr!("Muted text", ui.ctx()),
            tr!("Secondary labels and metadata", ui.ctx()),
            &mut palette.muted,
        ),
        (
            "accent",
            tr!("Accent / focus", ui.ctx()),
            tr!("Focus rings, selection, and active indicators", ui.ctx()),
            &mut palette.accent,
        ),
        (
            "success",
            tr!("Success", ui.ctx()),
            tr!("Positive status indicators", ui.ctx()),
            &mut palette.success,
        ),
        (
            "danger",
            tr!("Danger", ui.ctx()),
            tr!("Error and destructive indicators", ui.ctx()),
            &mut palette.danger,
        ),
        (
            "overlay",
            tr!("Overlay", ui.ctx()),
            tr!("Dialog and transient overlay tint", ui.ctx()),
            &mut palette.overlay,
        ),
        (
            "terminal-foreground",
            tr!("Foreground", ui.ctx()),
            tr!("Default terminal text color", ui.ctx()),
            &mut palette.terminal_foreground,
        ),
        (
            "terminal-background",
            tr!("Background", ui.ctx()),
            tr!("Default terminal background color", ui.ctx()),
            &mut palette.terminal_background,
        ),
        (
            "terminal-selection",
            tr!("Selection", ui.ctx()),
            tr!("Terminal text selection background", ui.ctx()),
            &mut palette.terminal_selection,
        ),
    ] {
        let widget_id = ui.make_persistent_id(("theme-palette-color", mode, id));
        let accessible = format!("{title} {label}");
        ui.horizontal_wrapped(|ui| {
            ui.label(label);
            ui.add(
                egui::TextEdit::singleline(value)
                    .id(widget_id)
                    .char_limit(16)
                    .desired_width(110.0)
                    .hint_text("#RRGGBB"),
            )
            .accessible_name(accessible);
            ui.label(description);
        });
    }
}

pub(super) fn text_colors(ui: &mut egui::Ui, draft: &mut AppearanceSettings) {
    ui.add(
        egui::Slider::new(
            &mut draft.terminal_text_brightness_percent,
            config::MIN_TERMINAL_TEXT_BRIGHTNESS_PERCENT
                ..=config::MAX_TERMINAL_TEXT_BRIGHTNESS_PERCENT,
        )
        .step_by(5.0)
        .suffix("%")
        .text(tr!("Text brightness", ui.ctx())),
    )
    .accessible_name(tr!("Terminal text brightness", ui.ctx()));
    ui.label(tr!(
        "60-120 percent; 100 percent preserves terminal colors",
        ui.ctx()
    ));
    ui.checkbox(
        &mut draft.bright_bold_text,
        tr!("Bright colors for bold text", ui.ctx()),
    )
    .on_hover_text(tr!("Use the bright ANSI palette for bold text", ui.ctx()));
}

pub(super) fn semantic(ui: &mut egui::Ui, draft: &mut AppearanceSettings) {
    ui.checkbox(
        &mut draft.terminal_semantic_highlighting,
        tr!("Enable semantic highlighting", ui.ctx()),
    );
    ui.label(tr!("Log levels and HTTP status codes", ui.ctx()));
    if !draft.terminal_semantic_highlighting {
        return;
    }
    let colors = &mut draft.terminal_semantic_colors;
    for (id, label, description, accessible, value) in [
        (
            "success",
            tr!("Success", ui.ctx()),
            tr!("HTTP 2xx responses", ui.ctx()),
            tr!("Terminal success highlight color", ui.ctx()),
            &mut colors.success,
        ),
        (
            "info",
            tr!("Information", ui.ctx()),
            tr!("Log levels and HTTP 1xx/3xx", ui.ctx()),
            tr!("Terminal information highlight color", ui.ctx()),
            &mut colors.info,
        ),
        (
            "warning",
            tr!("Warning", ui.ctx()),
            tr!("Warning levels and HTTP 4xx", ui.ctx()),
            tr!("Terminal warning highlight color", ui.ctx()),
            &mut colors.warning,
        ),
        (
            "error",
            tr!("Error", ui.ctx()),
            tr!("Error levels and HTTP 5xx", ui.ctx()),
            tr!("Terminal error highlight color", ui.ctx()),
            &mut colors.error,
        ),
    ] {
        let id = ui.make_persistent_id(("semantic-color", id));
        ui.horizontal_wrapped(|ui| {
            ui.label(label);
            ui.add(
                egui::TextEdit::singleline(value)
                    .id(id)
                    .char_limit(32)
                    .desired_width(100.0)
                    .hint_text(tr!("Theme default", ui.ctx())),
            )
            .accessible_name(accessible);
            ui.label(description);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egui_app::i18n::{self, Translator};

    #[test]
    fn localized_brightness_controls_toggle_only_the_draft() {
        fn texts(shape: &egui::epaint::Shape, found: &mut Vec<(String, egui::Rect)>) {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    found.push((text.galley.job.text.clone(), text.visual_bounding_rect()))
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        texts(shape, found);
                    }
                }
                _ => {}
            }
        }
        let fonts = crate::egui_app::fonts::load().unwrap();
        for language in config::UiLanguage::bundled() {
            let ctx = egui::Context::default();
            ctx.set_fonts(fonts.clone());
            Translator::new(language, None).apply(&ctx);
            let saved = AppearanceSettings::default();
            let mut draft = saved.clone();
            let draw = |draft: &mut AppearanceSettings, events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1400.0, 900.0),
                        )),
                        events,
                        focused: true,
                        ..Default::default()
                    },
                    |ui| text_colors(ui, draft),
                )
            };
            let output = draw(&mut draft, vec![]);
            let mut found = Vec::new();
            for shape in &output.shapes {
                texts(&shape.shape, &mut found);
            }
            let label = i18n::current(&ctx).text("", "Bright colors for bold text");
            let position = found
                .iter()
                .find(|(text, _)| text == label)
                .unwrap()
                .1
                .center();
            for source in [
                "Text brightness",
                "60-120 percent; 100 percent preserves terminal colors",
            ] {
                assert!(
                    found
                        .iter()
                        .any(|(text, _)| text == i18n::current(&ctx).text("", source)),
                    "{}: {source}",
                    language.as_setting()
                );
            }
            output.drop_without_applying_deltas();
            let key_events = |key, pressed| {
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]
            };
            for pressed in [true, false] {
                draw(&mut draft, key_events(egui::Key::Tab, pressed))
                    .drop_without_applying_deltas();
            }
            for (key, expected) in [105, 110, 115, 120, 120]
                .into_iter()
                .map(|n| (egui::Key::ArrowRight, n))
                .chain(
                    (60..120)
                        .step_by(5)
                        .rev()
                        .map(|n| (egui::Key::ArrowLeft, n)),
                )
            {
                for pressed in [true, false] {
                    draw(&mut draft, key_events(key, pressed)).drop_without_applying_deltas();
                }
                assert_eq!(
                    draft.terminal_text_brightness_percent,
                    expected,
                    "{}",
                    language.as_setting()
                );
            }
            for pressed in [true, false] {
                draw(
                    &mut draft,
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
            assert!(!draft.bright_bold_text);
            assert!(saved.bright_bold_text);
            draft.terminal_text_brightness_percent = 0;
            draw(&mut draft, vec![]).drop_without_applying_deltas();
            assert_eq!(draft.terminal_text_brightness_percent, 60);
        }
    }
}

#[cfg(test)]
mod semantic_tests {
    use super::*;
    use crate::egui_app::i18n::Translator;

    #[test]
    fn localized_semantic_fields_keep_distinct_stable_ids_and_preserve_other_drafts() {
        let fonts = crate::egui_app::fonts::load().unwrap();
        let mut stable = None;
        for language in config::UiLanguage::bundled() {
            let ctx = egui::Context::default();
            ctx.set_fonts(fonts.clone());
            Translator::new(language, None).apply(&ctx);
            let mut draft = AppearanceSettings {
                terminal_semantic_highlighting: true,
                ..Default::default()
            };
            draft.terminal_semantic_colors.link = "#123456".into();
            let mut ids = Vec::new();
            for (slot, key) in ["success", "info", "warning", "error"]
                .into_iter()
                .enumerate()
            {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1600.0, 1000.0),
                        )),
                        focused: true,
                        events: vec![egui::Event::Text("#aabbcc".into())],
                        ..Default::default()
                    },
                    |ui| {
                        let id = ui.make_persistent_id(("semantic-color", key));
                        ids.push(id);
                        ui.memory_mut(|memory| memory.request_focus(id));
                        semantic(ui, &mut draft);
                    },
                );
                output.drop_without_applying_deltas();
                let colors = &draft.terminal_semantic_colors;
                let values = [
                    &colors.success,
                    &colors.info,
                    &colors.warning,
                    &colors.error,
                ];
                for (index, value) in values.into_iter().enumerate() {
                    assert_eq!(
                        value.as_str(),
                        if index <= slot { "#aabbcc" } else { "" },
                        "{}: {key}",
                        language.as_setting()
                    );
                }
            }
            assert_eq!(*stable.get_or_insert(ids.clone()), ids);
            for (index, id) in ids.iter().enumerate() {
                assert!(!ids[index + 1..].contains(id));
            }
            assert_eq!(draft.terminal_semantic_colors.link, "#123456");
            let before = draft.clone();
            draft.terminal_semantic_highlighting = false;
            ctx.run_ui(egui::RawInput::default(), |ui| semantic(ui, &mut draft))
                .drop_without_applying_deltas();
            assert_eq!(
                draft.terminal_semantic_colors,
                before.terminal_semantic_colors
            );
        }
    }
}

#[cfg(test)]
mod theme_palette_tests {
    use super::*;
    use crate::egui_app::i18n::Translator;

    #[test]
    fn light_and_dark_palette_editors_keep_distinct_fields_and_stable_ids() {
        let fonts = crate::egui_app::fonts::load().unwrap();
        let mut stable_ids = None;
        for language in config::UiLanguage::bundled() {
            let ctx = egui::Context::default();
            ctx.set_fonts(fonts.clone());
            Translator::new(language, None).apply(&ctx);
            let mut light = ThemePalette::axssh_light();
            let mut dark = ThemePalette::axssh_dark();
            light.background.clear();
            let focus = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600.0, 1400.0),
                    )),
                    focused: true,
                    events: Vec::new(),
                    ..Default::default()
                },
                |ui| {
                    let first_id =
                        ui.make_persistent_id(("theme-palette-color", "light", "background"));
                    ui.memory_mut(|memory| memory.request_focus(first_id));
                    theme_palette(ui, "light", &mut light);
                    theme_palette(ui, "dark", &mut dark);
                },
            );
            focus.drop_without_applying_deltas();
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600.0, 1400.0),
                    )),
                    focused: true,
                    events: vec![egui::Event::Text("#112233".into())],
                    ..Default::default()
                },
                |ui| {
                    theme_palette(ui, "light", &mut light);
                    theme_palette(ui, "dark", &mut dark);
                },
            );
            output.drop_without_applying_deltas();
            assert_eq!(light.background, "#112233", "{}", language.as_setting());
            assert_eq!(dark.background, ThemePalette::axssh_dark().background);
            let ids = ["light", "dark"]
                .into_iter()
                .flat_map(|mode| {
                    [
                        "background",
                        "panel",
                        "panel-alt",
                        "border",
                        "text",
                        "muted",
                        "accent",
                        "success",
                        "danger",
                        "overlay",
                        "terminal-foreground",
                        "terminal-background",
                        "terminal-selection",
                    ]
                    .into_iter()
                    .map(move |role| (mode, role))
                })
                .map(|(mode, role)| egui::Id::unique(("theme-palette-color", mode, role)))
                .collect::<Vec<_>>();
            assert_eq!(ids.len(), 26);
            for (index, id) in ids.iter().enumerate() {
                assert!(!ids[index + 1..].contains(id));
            }
            assert_eq!(*stable_ids.get_or_insert(ids.clone()), ids);
        }
    }
}

#[cfg(test)]
mod refresh_tests {
    use super::*;
    use crate::egui_app::i18n::{self, Translator};

    #[test]
    fn localized_refresh_controls_support_keyboard_edits_bounds_and_stable_focus() {
        let fonts = crate::egui_app::fonts::load().unwrap();
        let mut stable = None;
        for language in config::UiLanguage::bundled() {
            let ctx = egui::Context::default();
            ctx.set_fonts(fonts.clone());
            Translator::new(language, None).apply(&ctx);
            let saved = AppearanceSettings::default();
            let mut draft = saved.clone();
            draft.focused_terminal_refresh_fps = 119;
            draft.unfocused_terminal_refresh_fps = 2;
            let draw = |draft: &mut AppearanceSettings, events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1600.0, 600.0),
                        )),
                        focused: true,
                        events,
                        ..Default::default()
                    },
                    |ui| refresh_rates(ui, draft),
                )
            };
            let output = draw(&mut draft, vec![]);
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let egui::epaint::Shape::Text(text) = &shape.shape {
                        Some(text.galley.job.text.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            for label in ["Focused refresh rate", "Unfocused refresh rate"] {
                assert!(
                    labels.contains(&i18n::current(&ctx).text("", label)),
                    "{}: {label}",
                    language.as_setting()
                );
            }
            output.drop_without_applying_deltas();
            let key = |draft: &mut AppearanceSettings, key| {
                for pressed in [true, false] {
                    draw(
                        draft,
                        vec![egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        }],
                    )
                    .drop_without_applying_deltas();
                }
            };
            key(&mut draft, egui::Key::Tab);
            let focused = ctx.memory(|memory| memory.focused()).unwrap();
            assert_eq!(*stable.get_or_insert(focused), focused);
            key(&mut draft, egui::Key::ArrowRight);
            key(&mut draft, egui::Key::ArrowRight);
            assert_eq!(draft.focused_terminal_refresh_fps, 120);
            assert_eq!(draft.unfocused_terminal_refresh_fps, 2);
            key(&mut draft, egui::Key::ArrowLeft);
            assert_eq!(draft.focused_terminal_refresh_fps, 119);
            // Each slider and its numeric editor are focusable.
            key(&mut draft, egui::Key::Tab);
            key(&mut draft, egui::Key::Tab);
            key(&mut draft, egui::Key::ArrowLeft);
            key(&mut draft, egui::Key::ArrowLeft);
            assert_eq!(draft.unfocused_terminal_refresh_fps, 1);
            assert_eq!(draft.focused_terminal_refresh_fps, 119);
            draft.focused_terminal_refresh_fps = 0;
            draft.unfocused_terminal_refresh_fps = u16::MAX;
            draw(&mut draft, vec![]).drop_without_applying_deltas();
            assert_eq!(draft.focused_terminal_refresh_fps, 1);
            assert_eq!(draft.unfocused_terminal_refresh_fps, 120);
            assert_eq!(saved.focused_terminal_refresh_fps, 60);
            assert_eq!(saved.unfocused_terminal_refresh_fps, 4);
        }
    }
}
