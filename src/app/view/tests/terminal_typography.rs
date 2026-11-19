use super::*;
use crate::app::panes::PanePlacement;
use crate::app::window_router::WindowTerminalPane;
use ax_ssh::terminal::TerminalModel;
use i_slint_core::item_tree::ItemRc;
use i_slint_core::items::{ComplexText, TextWrap};
use i_slint_core::window::WindowInner;
use slint::platform::WindowAdapter;
use std::ops::ControlFlow;
use std::sync::Arc;

fn publish(ui: &AppWindow, terminal: &mut TerminalModel) {
    let id = Uuid::from_u128(1);
    apply_terminal_panes(
        ui,
        vec![WindowTerminalPane {
            placement: PanePlacement {
                tab_id: id,
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                focused: true,
            },
            snapshot: ActiveTabSnapshot {
                id: Some(id),
                kind: "terminal",
                connected: true,
                terminal: Some(terminal.snapshot()),
                ..Default::default()
            },
            closable: false,
        }],
        vec![],
    );
    ui.window().request_redraw();
}

fn text_item(ui: &AppWindow, expected: &str) -> ItemRc {
    ItemRc::new_root(WindowInner::from_pub(ui.window()).component())
        .visit_descendants(|item| {
            if item.is_visible()
                && let Some(text) = item.downcast::<ComplexText>()
                && ComplexText::FIELD_OFFSETS
                    .text()
                    .apply_pin(text.as_pin_ref())
                    .get()
                    .trim_end()
                    == expected
            {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
        .expect("visible terminal text")
}

fn text_family(item: &ItemRc) -> SharedString {
    let text = item.downcast::<ComplexText>().unwrap();
    ComplexText::FIELD_OFFSETS
        .font_family()
        .apply_pin(text.as_pin_ref())
        .get()
}

fn assert_probe_family(ui: &AppWindow, family: &str) {
    let mut count = 0;
    ItemRc::new_root(WindowInner::from_pub(ui.window()).component()).visit_descendants(|item| {
        if let Some(text) = item.downcast::<ComplexText>()
            && ComplexText::FIELD_OFFSETS
                .text()
                .apply_pin(text.as_pin_ref())
                .get()
                .chars()
                .count()
                == 1001
        {
            assert_eq!(text_family(item), family);
            count += 1;
        }
        ControlFlow::<()>::Continue(())
    });
    assert_eq!(
        count, 5,
        "all actual advance probes must follow the selection"
    );
}

fn setup(harness: &LayoutTestHarness, detached: bool, terminal: &mut TerminalModel) {
    let ui = &harness.ui;
    apply_settings_to_component(ui, &AppSettings::default());
    let mut fonts = font_bridge::FontRegistry::new();
    for family in ["Maple Mono NF CN", "Iosevka Term"] {
        let font = fonts
            .resources()
            .load_bundled_font(family)
            .unwrap()
            .unwrap();
        fonts.register_loaded_font(font).unwrap();
    }
    set_font_registry_generation(ui, fonts.generation_as_slint_int());
    ui.set_detached_window(detached);
    ui.set_active_tab_kind("terminal".into());
    ui.set_active_tab_id(Uuid::from_u128(1).to_string().into());
    publish(ui, terminal);
    ui.show().unwrap();
    harness.window.set_size(slint::PhysicalSize::new(1100, 720));
    harness.settle();
}

#[test]
fn main_and_detached_terminals_render_symbol_batches_on_the_fixed_grid() {
    for detached in [false, true] {
        std::thread::spawn(move || {
            let harness = LayoutTestHarness::new();
            let ui = &harness.ui;
            let mut terminal = TerminalModel::new(80, 24, 10);
            terminal
                .process("←↑→\x1b[31m⏺\x1b[0m⏺\u{fe0f}中\r\n┌──┐\r\n█\u{e0b0}\x1b[1;4H".as_bytes());
            setup(&harness, detached, &mut terminal);
            for compact in [false, true] {
                ui.set_terminal_compact_rendering(compact);
                publish(ui, &mut terminal);
                harness.settle();
                let arrows = text_item(ui, "←↑→");
                assert_eq!(text_family(&arrows), "Iosevka Term");
                let text = arrows.downcast::<ComplexText>().unwrap();
                let measured = harness
                    .window
                    .renderer()
                    .text_size(text.as_pin_ref(), &arrows, None, TextWrap::NoWrap)
                    .width;
                assert!(
                    (measured - arrows.geometry().size.width).abs() <= 1.1,
                    "batched symbols must match their allocated cells: {measured} vs {:?}",
                    arrows.geometry()
                );
                let record = text_item(ui, "⏺");
                assert_eq!(text_family(&record), "Iosevka Term");
                let mut record_count = 0;
                ItemRc::new_root(WindowInner::from_pub(ui.window()).component()).visit_descendants(
                    |item| {
                        if item.is_visible()
                            && let Some(text) = item.downcast::<ComplexText>()
                            && ComplexText::FIELD_OFFSETS
                                .text()
                                .apply_pin(text.as_pin_ref())
                                .get()
                                == "⏺"
                        {
                            assert_eq!(text_family(item), "Iosevka Term");
                            record_count += 1;
                        }
                        ControlFlow::<()>::Continue(())
                    },
                );
                assert_eq!(
                    record_count, 2,
                    "the block cursor must use the same symbol font"
                );
                let emoji = text_item(ui, "⏺\u{fe0f}");
                let family = font_bridge::terminal_emoji_font_family();
                if !family.is_empty() {
                    assert_eq!(text_family(&emoji), family);
                }
                assert_eq!(text_family(&text_item(ui, "中")), "JetBrains Mono");
                let pane = ui.get_terminal_panes().row_data(0).unwrap();
                let border = pane.terminal.render_lines.row_data(1).unwrap();
                assert!(border.runs.iter().any(|run| !run.light_path.is_empty()));
                let blocks = pane.terminal.render_lines.row_data(2).unwrap();
                assert!(blocks.runs.iter().any(|run| !run.fill_path.is_empty()));
            }
            let before = ui.get_terminal_panes().row_data(0).unwrap();
            let lines = before.terminal.render_lines;
            let runs = lines.row_data(0).unwrap().runs;
            let focus = WindowInner::from_pub(ui.window())
                .focus_item
                .borrow()
                .upgrade();
            terminal.process("\x1b[1;1H\x1b[2K\x1b[32m┏━┓\x1b[0m".as_bytes());
            publish(ui, &mut terminal);
            harness.settle();
            let updated = ui.get_terminal_panes().row_data(0).unwrap();
            assert_eq!(updated.terminal.render_lines, lines);
            assert_eq!(lines.row_data(0).unwrap().runs, runs);
            assert!(runs.iter().any(|run| !run.heavy_path.is_empty()));
            terminal.process(b"\x1b[1;1H\x1b[2Kplain");
            publish(ui, &mut terminal);
            harness.settle();
            assert!(runs.iter().all(|run| run.light_path.is_empty()
                && run.fill_path.is_empty()
                && run.heavy_path.is_empty()));
            assert!(text_item(ui, "plain").is_visible());
            assert_eq!(
                focus,
                WindowInner::from_pub(ui.window())
                    .focus_item
                    .borrow()
                    .upgrade()
            );
            ui.hide().unwrap();
        })
        .join()
        .expect("terminal typography regression");
    }
}

#[test]
fn braille_spinner_frames_keep_the_symbol_font_in_main_and_detached_terminals() {
    for detached in [false, true] {
        std::thread::spawn(move || {
            let harness = LayoutTestHarness::new();
            let ui = &harness.ui;
            let mut terminal = TerminalModel::new(80, 24, 10);
            terminal.process(b"\x1b[2 q");
            setup(&harness, detached, &mut terminal);
            for compact in [false, true] {
                ui.set_terminal_compact_rendering(compact);
                for frame in "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏\u{2800}".chars() {
                    terminal
                        .process(format!("\r\x1b[38;2;30;210;80m{frame}\x1b[0m A\r").as_bytes());
                    publish(ui, &mut terminal);
                    harness.settle();
                    let frame = frame.to_string();
                    let mut count = 0;
                    ItemRc::new_root(WindowInner::from_pub(ui.window()).component())
                        .visit_descendants(|item| {
                            if item.is_visible()
                                && let Some(text) = item.downcast::<ComplexText>()
                                && ComplexText::FIELD_OFFSETS
                                    .text()
                                    .apply_pin(text.as_pin_ref())
                                    .get()
                                    == frame
                            {
                                assert_eq!(text_family(item), "Iosevka Term");
                                count += 1;
                            }
                            ControlFlow::<()>::Continue(())
                        });
                    assert_eq!(count, 2, "spinner and block cursor must share the font");
                    let pane = ui.get_terminal_panes().row_data(0).unwrap();
                    let runs = pane.terminal.render_lines.row_data(0).unwrap().runs;
                    let spinner = runs.row_data(0).unwrap();
                    assert_eq!(spinner.text, frame);
                    assert_eq!((spinner.column, spinner.cells), (0, 1));
                    assert_eq!(runs.row_data(1).unwrap().column, 1);
                    assert_eq!(terminal.selection_text(0, 0, 0, 3), format!("{frame} A"));
                }
            }
            ui.hide().unwrap();
        })
        .join()
        .expect("braille spinner typography regression");
    }
}

fn register_advance_fixture(family: &str, character: char, extra_advance: u16) {
    use slint::fontique_011::fontique;
    let mut collection = slint::fontique_011::shared_collection();
    let mut cache = fontique::SourceCache::default();
    let mut glyph = None;
    {
        let mut query = collection.query(&mut cache);
        query.set_families(["JetBrains Mono"]);
        query.matches_with(|font| {
            glyph = font.charmap().and_then(|map| map.map(character as u32));
            fontique::QueryStatus::Stop
        });
    }
    let mut data = include_bytes!("../../../../assets/fonts/JetBrainsMono-Regular.ttf").to_vec();
    let u16_at =
        |data: &[u8], offset| u16::from_be_bytes(data[offset..offset + 2].try_into().unwrap());
    let u32_at =
        |data: &[u8], offset| u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap());
    let table = |tag: &[u8]| {
        (0..usize::from(u16_at(&data, 4)))
            .find_map(|index| {
                let offset = 12 + index * 16;
                (&data[offset..offset + 4] == tag).then(|| u32_at(&data, offset + 8) as usize)
            })
            .unwrap()
    };
    let metrics = table(b"hmtx");
    let count = usize::from(u16_at(&data, table(b"hhea") + 34));
    let glyph = glyph.unwrap() as usize;
    assert!(glyph < count);
    let offset = metrics + 4 * glyph;
    let advance = u16_at(&data, offset) + extra_advance;
    data[offset..offset + 2].copy_from_slice(&advance.to_be_bytes());
    // This fixture retains the monospace metadata but has a deliberately
    // different real advance; the UI must measure rather than trust the flag.
    collection.register_fonts(
        fontique::Blob::new(Arc::new(data)),
        Some(fontique::FontInfoOverride {
            family_name: Some(family),
            ..Default::default()
        }),
    );
}

#[test]
fn proportional_terminal_fonts_fall_back_without_mutating_the_selected_family() {
    for detached in [false, true] {
        std::thread::spawn(move || {
            let harness = LayoutTestHarness::new();
            let ui = &harness.ui;
            let mut terminal = TerminalModel::new(80, 24, 10);
            terminal.process(b"monospace-fixture");
            setup(&harness, detached, &mut terminal);
            assert_eq!(
                text_family(&text_item(ui, "monospace-fixture")),
                "JetBrains Mono"
            );
            register_advance_fixture("AxSSH Proportional Fixture", 'i', 200);
            ui.set_terminal_font_family("AxSSH Proportional Fixture".into());
            publish(ui, &mut terminal);
            harness.settle();
            assert_eq!(ui.get_terminal_font_family(), "AxSSH Proportional Fixture");
            assert_probe_family(ui, "AxSSH Proportional Fixture");
            assert_eq!(
                text_family(&text_item(ui, "monospace-fixture")),
                "JetBrains Mono"
            );
            ui.set_terminal_font_family("Maple Mono NF CN".into());
            publish(ui, &mut terminal);
            harness.settle();
            assert_probe_family(ui, "Maple Mono NF CN");
            assert_eq!(
                ui.get_terminal_panes()
                    .row_data(0)
                    .unwrap()
                    .terminal
                    .font_family,
                "Maple Mono NF CN"
            );
            assert_eq!(
                text_family(&text_item(ui, "monospace-fixture")),
                "Maple Mono NF CN"
            );
            ui.hide().unwrap();
        })
        .join()
        .expect("actual monospace measurement regression");
    }
}

// Shape the item's actual properties in its real Slint font context and inspect
// font bytes, collection index and glyph IDs, not just the requested family.
fn assert_shortcut_shaping(ui: &AppWindow, item: &ItemRc, expected: &str, size: i32) {
    use i_slint_core::textlayout::sharedparley::parley;
    use slint::fontique_011::fontique;
    let text = item.downcast::<ComplexText>().unwrap();
    let text = text.as_pin_ref();
    let value = ComplexText::FIELD_OFFSETS.text().apply_pin(text).get();
    let pixel_size = ComplexText::FIELD_OFFSETS.font_size().apply_pin(text).get();
    assert!(
        (pixel_size.get() - size as f32).abs() < 0.001,
        "no shortcut shrink"
    );
    let weight = ComplexText::FIELD_OFFSETS
        .font_weight()
        .apply_pin(text)
        .get();
    let italic = ComplexText::FIELD_OFFSETS
        .font_italic()
        .apply_pin(text)
        .get();
    let ctx = WindowInner::from_pub(ui.window()).context();
    let mut fonts = ctx.font_context().borrow_mut();
    let mut expected_font = None;
    {
        let fonts = &mut fonts.inner;
        let mut query = fonts.collection.query(&mut fonts.source_cache);
        query.set_families([expected]);
        query.set_attributes(fontique::Attributes {
            weight: fontique::FontWeight::new(weight as f32),
            style: if italic {
                fontique::FontStyle::Italic
            } else {
                fontique::FontStyle::Normal
            },
            ..Default::default()
        });
        query.matches_with(|font| {
            expected_font = Some(font.clone());
            fontique::QueryStatus::Stop
        });
    }
    let expected_font = expected_font.unwrap();
    let mut layouts = parley::LayoutContext::<()>::new();
    let family = text_family(item);
    let mut builder = layouts.ranged_builder(&mut fonts.inner, value.as_str(), 1.0, false);
    builder.push_default(parley::StyleProperty::FontFamily(
        parley::FontFamily::Single(parley::FontFamilyName::named(family.as_str())),
    ));
    builder.push_default(parley::StyleProperty::FontSize(pixel_size.get()));
    builder.push_default(parley::StyleProperty::FontWeight(parley::FontWeight::new(
        weight as f32,
    )));
    builder.push_default(parley::StyleProperty::FontStyle(if italic {
        parley::FontStyle::Italic
    } else {
        parley::FontStyle::Normal
    }));
    let mut layout = builder.build(value.as_str());
    layout.break_all_lines(None);
    let runs = layout
        .lines()
        .flat_map(|line| line.runs())
        .collect::<Vec<_>>();
    assert_eq!(runs.len(), 1);
    assert!(
        runs[0].font().data.as_ref() == expected_font.blob.as_ref(),
        "actual shortcut font must be {expected}, requested {family}"
    );
    assert_eq!(runs[0].font().index, expected_font.index);
    assert!(
        runs[0]
            .clusters()
            .all(|cluster| cluster.glyphs().all(|glyph| glyph.id != 0))
    );
}

#[test]
fn shortcut_modifiers_use_selected_fonts_at_full_size_without_changing_cells() {
    for detached in [false, true] {
        std::thread::spawn(move || {
            let harness = LayoutTestHarness::new();
            let ui = &harness.ui;
            let mut terminal = TerminalModel::new(80, 24, 10);
            setup(&harness, detached, &mut terminal);
            register_advance_fixture("AxSSH Selected Shortcut Fixture", '⌘', 600);
            for (family, cell_advance) in [("JetBrains Mono", 0.6), ("AxSSH Selected Shortcut Fixture", 0.6)] {
                ui.set_terminal_font_family(family.into());
                for size in [12, 24] {
                    ui.set_terminal_font_size(size);
                    for compact in [false, true] {
                        ui.set_terminal_compact_rendering(compact);
                        for sgr in ["0", "1", "3", "1;3"] {
                            terminal.process(format!("\x1b[H\x1b[{sgr}m⌘S\x1b[0m\r\n\x1b[{sgr}m⌃g\x1b[0m\x1b[2 q\x1b[2;1H").as_bytes());
                            publish(ui, &mut terminal);
                            harness.settle();
                            let cell_width = size as f32 * cell_advance;
                            for (glyph, letter) in [("⌘", "S"), ("⌃", "g")] {
                                let item = text_item(ui, glyph);
                                assert_shortcut_shaping(ui, &item, family, size);
                                let text = item.downcast::<ComplexText>().unwrap();
                                let advance = harness.window.renderer().text_size(
                                    text.as_pin_ref(), &item, None, TextWrap::NoWrap,
                                ).width;
                                assert!(item.geometry().size.width + 0.02 >= advance, "native advance must not be clipped to one cell");
                                let next = text_item(ui, letter).map_to_window(Default::default()).x;
                                assert!((next - item.map_to_window(Default::default()).x - cell_width).abs() < 0.02);
                            }
                            let mut cursors = 0;
                            ItemRc::new_root(WindowInner::from_pub(ui.window()).component()).visit_descendants(|item| {
                                if item.is_visible()
                                    && let Some(text) = item.downcast::<ComplexText>()
                                    && ComplexText::FIELD_OFFSETS.text().apply_pin(text.as_pin_ref()).get() == "⌃"
                                {
                                    assert_shortcut_shaping(ui, item, family, size);
                                    cursors += 1;
                                }
                                ControlFlow::<()>::Continue(())
                            });
                            assert_eq!(cursors, 2, "row and block cursor");
                            let pane = ui.get_terminal_panes().row_data(0).unwrap();
                            for row in [0, 1] {
                                let run = pane.terminal.render_lines.row_data(row).unwrap().runs.row_data(0).unwrap();
                                assert_eq!((run.column, run.cells), (0, 1));
                                assert!(run.fill_path.is_empty() && run.light_path.is_empty() && run.heavy_path.is_empty());
                            }
                            assert_eq!(terminal.selection_text(0, 0, 1, 1), "⌘S\n⌃g");
                            assert_eq!(terminal.snapshot().cursor_column, 0);
                        }
                    }
                }
            }
            ui.hide().unwrap();
        }).join().expect("selected shortcut typography regression");
    }
}

#[test]
fn shortcut_font_choices_refresh_after_registration_without_replacing_text_models() {
    for detached in [false, true] {
        std::thread::spawn(move || {
            let harness = LayoutTestHarness::new();
            let ui = &harness.ui;
            let mut terminal = TerminalModel::new(80, 24, 10);
            terminal.process("⌘S\r\n⌃g\x1b[2 q\x1b[2;1H".as_bytes());
            setup(&harness, detached, &mut terminal);
            ui.set_terminal_font_family("AxSSH Late Shortcut Fixture".into());
            ui.set_terminal_font_size(24);
            for compact in [false, true] {
                ui.set_terminal_compact_rendering(compact);
                publish(ui, &mut terminal);
                harness.settle();
                assert_ne!(
                    text_family(&text_item(ui, "⌘")),
                    "AxSSH Late Shortcut Fixture"
                );
            }
            let before = ui
                .get_terminal_panes()
                .row_data(0)
                .unwrap()
                .terminal
                .render_lines;
            register_advance_fixture("AxSSH Late Shortcut Fixture", '⌘', 600);
            set_font_registry_generation(ui, ui.get_font_registry_generation() + 1);
            publish(ui, &mut terminal);
            for size in [12, 24] {
                ui.set_terminal_font_size(size);
                for compact in [false, true] {
                    ui.set_terminal_compact_rendering(compact);
                    publish(ui, &mut terminal);
                    harness.settle();
                    assert_shortcut_shaping(
                        ui,
                        &text_item(ui, "⌘"),
                        "AxSSH Late Shortcut Fixture",
                        size,
                    );
                    assert_shortcut_shaping(
                        ui,
                        &text_item(ui, "⌃"),
                        "AxSSH Late Shortcut Fixture",
                        size,
                    );
                    assert_eq!(
                        ui.get_terminal_panes()
                            .row_data(0)
                            .unwrap()
                            .terminal
                            .render_lines,
                        before
                    );
                }
            }
            assert_eq!(terminal.selection_text(0, 0, 1, 1), "⌘S\n⌃g");
            ui.hide().unwrap();
        })
        .join()
        .expect("shortcut registration invalidation");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn shortcut_native_fonts_keep_selected_menlo_and_use_platform_fallbacks_in_both_windows() {
    for detached in [false, true] {
        std::thread::spawn(move || {
            use slint::fontique_011::fontique;
            // Inspect an untouched system collection, not AxSSH's overridden
            // Common/Han list. Fontique's native generic is not always Menlo.
            let mut native = fontique::Collection::new(fontique::CollectionOptions {
                shared: false,
                system_fonts: true,
            });
            let mut ids = native
                .generic_families(fontique::GenericFamily::Monospace)
                .collect::<Vec<_>>();
            for script in [*b"Zyyy", *b"Hani"] {
                ids.extend(native.fallback_families(fontique::FallbackKey::new(
                    fontique::Script::from_bytes(script),
                    None,
                )));
            }
            let mut cache = fontique::SourceCache::default();
            let mut expected = Vec::new();
            for character in ['⌃', '⌘'] {
                let mut family_id = None;
                {
                    let mut query = native.query(&mut cache);
                    query.set_families(ids.iter().copied());
                    query.matches_with(|font| {
                        if font
                            .charmap()
                            .and_then(|map| map.map(character as u32))
                            .is_some_and(|glyph| glyph != 0)
                        {
                            family_id = Some(font.family.0);
                            fontique::QueryStatus::Stop
                        } else {
                            fontique::QueryStatus::Continue
                        }
                    });
                }
                expected.push(
                    family_id
                        .and_then(|id| native.family_name(id).map(str::to_owned))
                        .unwrap_or_else(|| "JetBrains Mono".to_owned()),
                );
            }
            println!("shortcut native fallback before background discovery: {expected:?}");
            let harness = LayoutTestHarness::new();
            let ui = &harness.ui;
            let mut terminal = TerminalModel::new(80, 24, 10);
            terminal.process("⌘S\r\n⌃g\x1b[2 q\x1b[2;1H".as_bytes());
            setup(&harness, detached, &mut terminal);
            for family in ["Menlo", "Iosevka Term"] {
                ui.set_terminal_font_family(family.into());
                for size in [12, 24] {
                    ui.set_terminal_font_size(size);
                    for compact in [false, true] {
                        ui.set_terminal_compact_rendering(compact);
                        publish(ui, &mut terminal);
                        harness.settle();
                        for (index, glyph) in ["⌃", "⌘"].iter().enumerate() {
                            let expected_family = if family == "Menlo" {
                                "Menlo"
                            } else {
                                &expected[index]
                            };
                            assert_shortcut_shaping(
                                ui,
                                &text_item(ui, glyph),
                                expected_family,
                                size,
                            );
                        }
                    }
                }
            }
            ui.hide().unwrap();
        })
        .join()
        .expect("native shortcut fallback in main/detached windows");
    }
}

#[test]
fn fallback_glyph_paint_bounds_follow_shaping_without_changing_terminal_columns() {
    for detached in [false, true] {
        std::thread::spawn(move || {
            let harness = LayoutTestHarness::new();
            let ui = &harness.ui;
            let mut terminal = TerminalModel::new(80, 24, 10);
            terminal.process("🖥  A\r\n⚠  B\r\nΩ  C\r\n!".as_bytes());
            setup(&harness, detached, &mut terminal);
            // Ω is a primary-font glyph with no production special case. Its
            // doubled advance exercises the same generic path on every OS,
            // even when a CI image has no system emoji font installed.
            register_advance_fixture("AxSSH Wide Glyph Fixture", 'Ω', 600);
            let copy = terminal.selection_text(0, 0, 3, 3);
            let cursor = terminal.snapshot();
            let mut exercised_wide_fixture = false;
            for family in ["JetBrains Mono", "AxSSH Wide Glyph Fixture"] {
                ui.set_terminal_font_family(family.into());
                for size in [12, 24] {
                    ui.set_terminal_font_size(size);
                    for compact in [false, true] {
                        ui.set_terminal_compact_rendering(compact);
                        publish(ui, &mut terminal);
                        harness.settle();
                        let pane = ui.get_terminal_panes().row_data(0).unwrap();
                        for (row, glyph, marker) in
                            [(0, "🖥", "  A"), (1, "⚠", "  B"), (2, "Ω", "  C")]
                        {
                            let item = text_item(ui, glyph);
                            let text = item.downcast::<ComplexText>().unwrap();
                            let advance = harness.window.renderer()
                                .text_size(text.as_pin_ref(), &item, None, TextWrap::NoWrap).width;
                            let allocated = item.geometry().size.width;
                            let marker_item = text_item(ui, marker);
                            let cell_width = marker_item.map_to_window(Default::default()).x
                                - item.map_to_window(Default::default()).x;
                            assert!((cell_width - size as f32 * 0.6).abs() < 0.02,
                                "following text must keep its protocol column");
                            let run = pane.terminal.render_lines.row_data(row).unwrap()
                                .runs.row_data(0).unwrap();
                            assert_eq!((run.column, run.cells), (0, 1), "{glyph}");
                            assert!(allocated + 0.01 >= advance,
                                "{glyph}: glyph advance {advance} exceeds paint width {allocated}; \
                                 logical cell width {cell_width}, family {family}, size {size}, compact {compact}");
                            assert!(allocated <= cell_width + advance.ceil() + 0.01);
                            if family == "AxSSH Wide Glyph Fixture" && glyph == "Ω" {
                                assert_eq!(text_family(&item), family);
                                assert!(advance > 1.9 * cell_width);
                                exercised_wide_fixture = true;
                            }
                        }
                        let after = terminal.snapshot();
                        assert_eq!((after.cursor_row, after.cursor_column),
                            (cursor.cursor_row, cursor.cursor_column));
                        assert_eq!(terminal.selection_text(0, 0, 3, 3), copy);
                    }
                }
            }
            assert!(exercised_wide_fixture);
            ui.hide().unwrap();
        }).join().expect("generic fallback paint bounds regression");
    }
}

#[test]
fn late_font_registration_refreshes_existing_text_metrics_and_paint_bounds() {
    for detached in [false, true] {
        std::thread::spawn(move || {
            let harness = LayoutTestHarness::new();
            let ui = &harness.ui;
            let mut terminal = TerminalModel::new(80, 24, 10);
            terminal.process("Ω  A".as_bytes());
            setup(&harness, detached, &mut terminal);
            ui.set_terminal_font_family("AxSSH Late Wide Fixture".into());
            ui.set_terminal_font_size(12);
            for compact in [false, true] {
                ui.set_terminal_compact_rendering(compact);
                publish(ui, &mut terminal);
                harness.settle();
                // Populate both the renderer's shaping cache and the item's
                // preferred-width binding before the requested face exists.
                let item = text_item(ui, "Ω");
                let text = item.downcast::<ComplexText>().unwrap();
                let measured = harness.window.renderer().text_size(
                    text.as_pin_ref(),
                    &item,
                    None,
                    TextWrap::NoWrap,
                );
                assert!(measured.width > 0.0);
            }
            let generation = ui.get_font_registry_generation();
            register_advance_fixture("AxSSH Late Wide Fixture", 'Ω', 600);
            set_font_registry_generation(ui, generation + 1);
            // Match the loader's completion refresh; the snapshot text is
            // unchanged and stable run models remain reusable.
            publish(ui, &mut terminal);
            for compact in [false, true] {
                ui.set_terminal_compact_rendering(compact);
                harness.settle();
                let item = text_item(ui, "Ω");
                assert_eq!(text_family(&item), "AxSSH Late Wide Fixture");
                let text = item.downcast::<ComplexText>().unwrap();
                let width = harness
                    .window
                    .renderer()
                    .text_size(text.as_pin_ref(), &item, None, TextWrap::NoWrap)
                    .width;
                assert!((width - 14.4).abs() < 0.02, "cached advance: {width}");
                assert!(item.geometry().size.width + 0.01 >= width);
            }
            ui.hide().unwrap();
        })
        .join()
        .expect("late font registration must refresh existing items");
    }
}
