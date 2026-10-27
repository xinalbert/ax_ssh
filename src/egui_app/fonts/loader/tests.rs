use super::*;

#[test]
fn bundled_faces_render_fixed_grid_with_real_bold_and_cjk_fallback() {
    let mut loader = Loader::bundled().unwrap();
    let ctx = eframe::egui::Context::default();
    for asset in font_resources::BUNDLED_FONTS {
        let selection = Selection {
            application: asset.family.into(),
            terminal: asset.family.into(),
        };
        let loaded = loader.load(&selection, &AtomicBool::new(false)).unwrap();
        assert!(
            loaded.warnings.is_empty(),
            "{}: {:?}",
            asset.family,
            loaded.warnings
        );
        assert_eq!(loaded.terminal, asset.family);
        assert_eq!(loaded.application, asset.family);
        let regular = &loaded.definitions.font_data["loaded-terminal-regular"];
        let bold = &loaded.definitions.font_data["loaded-terminal-bold"];
        if regular.variation_axes().is_empty() {
            assert_ne!(regular.bytes(), bold.bytes(), "{} bold face", asset.family);
        } else {
            assert_ne!(
                regular.tweak.coords, bold.tweak.coords,
                "variable weight must change"
            );
        }
        ctx.set_fonts(loaded.definitions);
        let output = ctx.run_ui(eframe::egui::RawInput::default(), |ui| {
            for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
                let font = super::super::font(15.0, bold, italic);
                ui.fonts_mut(|fonts| {
                    let width = fonts.glyph_width(&font, 'M');
                    assert!(width > 0.0);
                    for ch in ['i', 'W', '0', ' '] {
                        assert!(
                            (width - fonts.glyph_width(&font, ch)).abs() < 0.1,
                            "{} {ch}",
                            asset.family
                        );
                    }
                    let galley = fonts.layout_no_wrap(
                        "中文 e\u{301} →".into(),
                        font,
                        eframe::egui::Color32::WHITE,
                    );
                    assert!(galley.size().x > width * 3.0);
                });
            }
        });
        output.drop_without_applying_deltas();
        assert_eq!(selection.terminal, asset.family);
    }
}

#[test]
fn missing_or_proportional_terminal_font_falls_back_without_mutating_selection() {
    let mut system = fontdb::Database::new();
    let defaults = FontDefinitions::default();
    system.load_font_source(fontdb::Source::Binary(
        defaults.font_data["Ubuntu-Light"].font.clone(),
    ));
    let family = system.faces().next().unwrap().families[0].0.clone();
    let mut loader = Loader::new(system, resources()).unwrap();
    let selection = Selection {
        application: family.clone(),
        terminal: family.clone(),
    };
    let loaded = loader.load(&selection, &AtomicBool::new(false)).unwrap();
    assert_eq!(loaded.application, family);
    assert_eq!(loaded.terminal, BUNDLED_UI_FONT_FAMILY);
    assert_eq!(loaded.warnings.len(), 1);
    assert!(loaded.warnings[0].contains("proportional"));
    assert_eq!(selection.terminal, family);
    let missing = Selection {
        application: "Fixture Missing".into(),
        terminal: "Fixture Missing".into(),
    };
    let loaded = loader.load(&missing, &AtomicBool::new(false)).unwrap();
    assert_eq!(loaded.warnings.len(), 2);
    assert_eq!(loaded.application, BUNDLED_UI_FONT_FAMILY);
    assert_eq!(loaded.terminal, BUNDLED_UI_FONT_FAMILY);
    assert_eq!(missing.terminal, "Fixture Missing");
}

#[test]
fn font_reads_enforce_file_request_and_cancellation_limits() {
    let path = std::env::temp_dir().join(format!("axssh-font-limits-{}", uuid::Uuid::new_v4()));
    let file = File::create(&path).unwrap();
    file.set_len(MAX_BUNDLED_FONT_FILE_BYTES + 1).unwrap();
    let cancelled = AtomicBool::new(false);
    assert!(
        Reads::default()
            .file(&path, &cancelled)
            .err()
            .unwrap()
            .to_string()
            .contains("24 MiB")
    );
    file.set_len(1).unwrap();
    let mut reads = Reads {
        total: MAX_REQUEST_BYTES,
        ..Default::default()
    };
    assert!(
        reads
            .file(&path, &cancelled)
            .err()
            .unwrap()
            .to_string()
            .contains("96 MiB")
    );
    cancelled.store(true, std::sync::atomic::Ordering::Release);
    assert!(
        Reads::default()
            .file(&path, &cancelled)
            .err()
            .unwrap()
            .to_string()
            .contains("cancelled")
    );
    drop(file);
    std::fs::remove_file(path).unwrap();
}
