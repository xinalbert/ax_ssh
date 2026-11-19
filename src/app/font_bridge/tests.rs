use super::*;

fn shortcut_test_fonts() -> i_slint_core::textlayout::sharedparley::parley::FontContext {
    let resources = FontResources {
        directories: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")],
    };
    let mut fonts = i_slint_core::textlayout::sharedparley::parley::FontContext {
        collection: fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
        source_cache: Default::default(),
    };
    for family in [
        BUNDLED_UI_FONT_FAMILY,
        TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
        TERMINAL_CJK_FALLBACK_FONT_FAMILY,
    ] {
        register_loaded_font_in_collection(
            &mut fonts.collection,
            resources.load_bundled_font(family).unwrap().unwrap(),
        )
        .unwrap();
    }
    fonts
}

fn assert_shortcut_font_bytes(
    fonts: &mut i_slint_core::textlayout::sharedparley::parley::FontContext,
    family: &str,
    character: char,
    style: usize,
    expected: &[u8],
) {
    use i_slint_core::textlayout::sharedparley::parley;
    let text = character.to_string();
    for size in [12.0, 24.0] {
        let mut layouts = parley::LayoutContext::<()>::new();
        let mut builder = layouts.ranged_builder(fonts, &text, 1.0, false);
        builder.push_default(parley::StyleProperty::FontFamily(
            parley::FontFamily::Single(parley::FontFamilyName::named(family)),
        ));
        builder.push_default(parley::StyleProperty::FontSize(size));
        builder.push_default(parley::StyleProperty::FontWeight(parley::FontWeight::new(
            if style & 1 != 0 { 700.0 } else { 400.0 },
        )));
        builder.push_default(parley::StyleProperty::FontStyle(if style & 2 != 0 {
            parley::FontStyle::Italic
        } else {
            parley::FontStyle::Normal
        }));
        let mut layout = builder.build(&text);
        layout.break_all_lines(None);
        let runs = layout
            .lines()
            .flat_map(|line| line.runs())
            .collect::<Vec<_>>();
        assert_eq!(runs.len(), 1);
        assert!(
            runs[0].font().data.as_ref() == expected,
            "actual font bytes for {family} {character}"
        );
        assert!(
            runs[0]
                .clusters()
                .all(|cluster| cluster.glyphs().all(|glyph| glyph.id != 0))
        );
        assert!(runs[0].advance() > 0.0);
        if family == "Menlo" {
            let glyphs = runs[0]
                .clusters()
                .flat_map(|cluster| cluster.glyphs())
                .map(|glyph| glyph.id)
                .collect::<Vec<_>>();
            println!(
                "Menlo {character} style={style} size={size} index={} glyphs={glyphs:?} shaped advance={}",
                runs[0].font().index,
                runs[0].advance()
            );
        }
    }
}

#[test]
fn shortcut_resolution_selects_coverage_in_selected_system_bundled_order() {
    let mut fonts = shortcut_test_fonts();
    let candidate = "AxSSH Native Shortcut Fixture";
    let system_bytes = include_bytes!("../../../assets/fonts/JetBrainsMono-Bold.ttf");
    fonts.collection.register_fonts(
        fontique::Blob::new(Arc::new(system_bytes.to_vec())),
        Some(fontique::FontInfoOverride {
            family_name: Some(candidate),
            ..Default::default()
        }),
    );
    let system = ["Missing system family".to_owned(), candidate.to_owned()];
    for character in ['⌃', '⌘'] {
        for (style, bytes) in EMBEDDED_UI_FONT_FILES.iter().enumerate() {
            let selected = resolve_shortcut_family(
                &mut fonts.collection,
                &mut fonts.source_cache,
                BUNDLED_UI_FONT_FAMILY,
                &system,
                character,
                style,
            );
            assert_eq!(selected, BUNDLED_UI_FONT_FAMILY);
            assert_shortcut_font_bytes(&mut fonts, &selected, character, style, bytes);
            let native = resolve_shortcut_family(
                &mut fonts.collection,
                &mut fonts.source_cache,
                TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
                &system,
                character,
                style,
            );
            assert_eq!(
                native, candidate,
                "missing selected glyph must use system before bundled"
            );
            assert_shortcut_font_bytes(&mut fonts, &native, character, style, system_bytes);
            let bundled = resolve_shortcut_family(
                &mut fonts.collection,
                &mut fonts.source_cache,
                TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
                &[],
                character,
                style,
            );
            assert_eq!(bundled, BUNDLED_UI_FONT_FAMILY);
            assert_shortcut_font_bytes(&mut fonts, &bundled, character, style, bytes);
        }
    }
}

#[test]
fn shortcut_native_candidates_prefer_platform_monospace_without_mutating_other_fallbacks() {
    let mut fonts = shortcut_test_fonts();
    let bytes = EMBEDDED_UI_FONT_FILES[0];
    for name in ["Native Mono Fixture", "Native Common Fixture"] {
        fonts.collection.register_fonts(
            fontique::Blob::new(Arc::new(bytes.to_vec())),
            Some(fontique::FontInfoOverride {
                family_name: Some(name),
                ..Default::default()
            }),
        );
    }
    let mono = fonts.collection.family_id("Native Mono Fixture").unwrap();
    let common = fonts.collection.family_id("Native Common Fixture").unwrap();
    fonts
        .collection
        .set_generic_families(fontique::GenericFamily::Monospace, [mono].into_iter());
    let key = fontique::FallbackKey::new(fontique::Script::from_bytes(*b"Zyyy"), None);
    fonts.collection.set_fallbacks(key, [common].into_iter());
    let before = fonts.collection.fallback_families(key).collect::<Vec<_>>();
    let system = native_shortcut_families(&mut fonts.collection);
    assert_eq!(
        &system[..2],
        ["Native Mono Fixture", "Native Common Fixture"]
    );
    for character in ['⌃', '⌘'] {
        assert_eq!(
            resolve_shortcut_family(
                &mut fonts.collection,
                &mut fonts.source_cache,
                TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
                &system,
                character,
                0
            ),
            "Native Mono Fixture"
        );
    }
    assert_eq!(
        fonts.collection.fallback_families(key).collect::<Vec<_>>(),
        before
    );
}

#[test]
fn shortcut_discovery_and_registration_invalidate_cached_choices_in_the_slint_collection() {
    std::thread::spawn(|| {
        use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
        use slint::platform::{Platform, WindowAdapter};
        struct TestPlatform;
        impl Platform for TestPlatform {
            fn create_window_adapter(
                &self,
            ) -> Result<std::rc::Rc<dyn WindowAdapter>, slint::PlatformError> {
                Ok(MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer))
            }
        }
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        // Generic-family overrides still append native system families. Keep
        // this cache-invalidation fixture independent of the runner's fonts,
        // while retaining the shared collection used by the real Slint context.
        let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        i_slint_core::window::WindowInner::from_pub(window.window())
            .context()
            .font_context()
            .borrow_mut()
            .inner = i_slint_core::textlayout::sharedparley::parley::FontContext {
            collection: fontique::Collection::new(fontique::CollectionOptions {
                shared: true,
                system_fonts: false,
            }),
            source_cache: Default::default(),
        };
        let mut collection = slint::fontique_011::shared_collection();
        let missing = "AxSSH Missing Native Fixture";
        collection.register_fonts(
            fontique::Blob::new(Arc::new(
                include_bytes!("../../../assets/fonts/IosevkaTerm-Regular.ttf").to_vec(),
            )),
            Some(fontique::FontInfoOverride {
                family_name: Some(missing),
                ..Default::default()
            }),
        );
        let id = collection.family_id(missing).unwrap();
        collection.set_generic_families(fontique::GenericFamily::Monospace, [id].into_iter());
        for script in [*b"Zyyy", *b"Hani"] {
            collection.set_fallbacks(
                fontique::FallbackKey::new(fontique::Script::from_bytes(script), None),
                [id].into_iter(),
            );
        }
        assert_eq!(native_shortcut_families(&mut collection), [missing]);
        let mut registry = FontRegistry::new();
        for family in [BUNDLED_UI_FONT_FAMILY, TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY] {
            registry
                .register_loaded_font(
                    registry
                        .resources()
                        .load_bundled_font(family)
                        .unwrap()
                        .unwrap(),
                )
                .unwrap();
        }
        let primary = "AxSSH Late Primary Fixture";
        let generation = registry.generation_as_slint_int();
        let before = terminal_shortcut_font_families(primary, generation);
        assert!(
            before.iter().all(|family| family == BUNDLED_UI_FONT_FAMILY),
            "initial shortcut families: {before:?}"
        );
        let candidate = "AxSSH Discovered Native Fixture";
        collection.register_fonts(
            fontique::Blob::new(Arc::new(EMBEDDED_UI_FONT_FILES[1].to_vec())),
            Some(fontique::FontInfoOverride {
                family_name: Some(candidate),
                ..Default::default()
            }),
        );
        registry
            .register_system_terminal_fallbacks(&[candidate.to_owned()])
            .unwrap();
        let discovered =
            terminal_shortcut_font_families(primary, registry.generation_as_slint_int());
        assert!(discovered.iter().all(|family| family == candidate));
        let mut fonts = i_slint_core::textlayout::sharedparley::parley::FontContext {
            collection: collection.clone(),
            source_cache: Default::default(),
        };
        for (index, family) in discovered.iter().enumerate() {
            assert_shortcut_font_bytes(
                &mut fonts,
                family,
                if index < 4 { '⌃' } else { '⌘' },
                index % 4,
                EMBEDDED_UI_FONT_FILES[1],
            );
        }
        collection.register_fonts(
            fontique::Blob::new(Arc::new(EMBEDDED_UI_FONT_FILES[0].to_vec())),
            Some(fontique::FontInfoOverride {
                family_name: Some(primary),
                ..Default::default()
            }),
        );
        let refreshed =
            terminal_shortcut_font_families(primary, registry.generation_as_slint_int() + 1);
        assert!(refreshed.iter().all(|family| family == primary));
        for (index, family) in refreshed.iter().enumerate() {
            assert_shortcut_font_bytes(
                &mut fonts,
                family,
                if index < 4 { '⌃' } else { '⌘' },
                index % 4,
                EMBEDDED_UI_FONT_FILES[0],
            );
        }
    })
    .join()
    .expect("shortcut discovery and registry refresh");
}

#[cfg(target_os = "macos")]
#[test]
fn native_menlo_shortcut_coverage_and_shaping_metrics_are_real() {
    use i_slint_core::textlayout::sharedparley::parley;
    let mut fonts = parley::FontContext {
        collection: fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: true,
        }),
        source_cache: Default::default(),
    };
    let system = native_shortcut_families(&mut fonts.collection);
    println!("native shortcut candidates: {system:?}");
    let resources = FontResources {
        directories: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")],
    };
    for family in [
        BUNDLED_UI_FONT_FAMILY,
        TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
        TERMINAL_CJK_FALLBACK_FONT_FAMILY,
    ] {
        register_loaded_font_in_collection(
            &mut fonts.collection,
            resources.load_bundled_font(family).unwrap().unwrap(),
        )
        .unwrap();
    }
    let system = system
        .into_iter()
        .chain(discover_system_monospace_families())
        .collect::<Vec<_>>();
    for character in ['⌃', '⌘'] {
        for style in 0..4 {
            let selected = resolve_shortcut_family(
                &mut fonts.collection,
                &mut fonts.source_cache,
                "Menlo",
                &system,
                character,
                style,
            );
            assert_eq!(selected, "Menlo");
            let fallback = resolve_shortcut_family(
                &mut fonts.collection,
                &mut fonts.source_cache,
                TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
                &system,
                character,
                style,
            );
            assert!(
                bundled_font(&fallback).is_none(),
                "installed system glyph must beat bundled: {fallback}"
            );
            for expected_family in [&selected, &fallback] {
                let mut query = fonts.collection.query(&mut fonts.source_cache);
                query.set_families([expected_family.as_str()]);
                query.set_attributes(fontique::Attributes {
                    weight: fontique::FontWeight::new(if style & 1 != 0 { 700.0 } else { 400.0 }),
                    style: if style & 2 != 0 {
                        fontique::FontStyle::Italic
                    } else {
                        fontique::FontStyle::Normal
                    },
                    ..Default::default()
                });
                let mut actual = None;
                query.matches_with(|font| {
                    if font
                        .charmap()
                        .and_then(|map| map.map(character as u32))
                        .is_some_and(|glyph| glyph != 0)
                    {
                        actual = Some(font.blob.clone());
                        fontique::QueryStatus::Stop
                    } else {
                        fontique::QueryStatus::Continue
                    }
                });
                drop(query);
                assert_shortcut_font_bytes(
                    &mut fonts,
                    expected_family,
                    character,
                    style,
                    actual.unwrap().as_ref(),
                );
            }
            println!(
                "Iosevka Term missing {character} style={style}: actual system fallback={fallback}"
            );
        }
    }
}

#[test]
fn real_parley_shaping_selects_the_explicit_symbol_face_before_emoji_fallback() {
    use i_slint_core::textlayout::sharedparley::parley;
    let resources = FontResources {
        directories: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")],
    };
    let mut fonts = parley::FontContext {
        collection: fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
        source_cache: Default::default(),
    };
    for family in [BUNDLED_UI_FONT_FAMILY, TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY] {
        let font = resources.load_bundled_font(family).unwrap().unwrap();
        register_loaded_font_in_collection(&mut fonts.collection, font).unwrap();
    }
    let braille = ('\u{2800}'..='\u{28ff}').collect::<String>();
    let mut layouts = parley::LayoutContext::<()>::new();
    for (face, weight, style) in [
        ("Regular", 400.0, parley::FontStyle::Normal),
        ("Bold", 700.0, parley::FontStyle::Normal),
        ("Italic", 400.0, parley::FontStyle::Italic),
        ("BoldItalic", 700.0, parley::FontStyle::Italic),
    ] {
        let symbol_bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(format!("assets/fonts/IosevkaTerm-{face}.ttf")),
        )
        .unwrap();
        for text in ["⏺", "⏺\u{fe0e}", "←↑→", "✔", &braille] {
            let mut builder = layouts.ranged_builder(&mut fonts, text, 1.0, false);
            builder.push_default(parley::StyleProperty::FontFamily(
                parley::FontFamily::Single(parley::FontFamilyName::named(
                    TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
                )),
            ));
            builder.push_default(parley::StyleProperty::FontSize(14.0));
            builder.push_default(parley::StyleProperty::FontWeight(parley::FontWeight::new(
                weight,
            )));
            builder.push_default(parley::StyleProperty::FontStyle(style));
            let mut layout = builder.build(text);
            layout.break_all_lines(None);
            let runs = layout
                .lines()
                .flat_map(|line| line.runs())
                .collect::<Vec<_>>();
            assert!(!runs.is_empty());
            assert!(
                runs.iter()
                    .all(|run| run.font().data.as_ref() == symbol_bytes.as_slice()),
                "{text:?} must shape from Iosevka {face}, not merely appear in a candidate list"
            );
            assert!(runs.iter().all(|run| {
                run.clusters()
                    .all(|cluster| cluster.glyphs().all(|glyph| glyph.id != 0))
            }));
        }
    }
}

#[test]
fn batchable_symbols_have_real_cell_advances_in_all_bundled_symbol_faces() {
    use i_slint_core::textlayout::sharedparley::parley;
    let mut fonts = parley::FontContext {
        collection: fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
        source_cache: Default::default(),
    };
    let resources = FontResources {
        directories: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")],
    };
    let font = resources
        .load_bundled_font(TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY)
        .unwrap()
        .unwrap();
    register_loaded_font_in_collection(&mut fonts.collection, font).unwrap();
    let text = (0..=0x2bff)
        .filter_map(char::from_u32)
        .filter(|&ch| super::super::terminal_render::batchable_symbol(ch))
        .collect::<String>();
    assert!(text.chars().count() > 800);
    let mut layouts = parley::LayoutContext::<()>::new();
    for (weight, italic) in [(400.0, false), (700.0, false), (400.0, true), (700.0, true)] {
        let mut builder = layouts.ranged_builder(&mut fonts, &text, 1.0, false);
        builder.push_default(parley::StyleProperty::FontFamily(
            parley::FontFamily::Single(parley::FontFamilyName::named(
                TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
            )),
        ));
        builder.push_default(parley::StyleProperty::FontSize(14.0));
        builder.push_default(parley::StyleProperty::FontWeight(parley::FontWeight::new(
            weight,
        )));
        builder.push_default(parley::StyleProperty::FontStyle(if italic {
            parley::FontStyle::Italic
        } else {
            parley::FontStyle::Normal
        }));
        let mut layout = builder.build(&text);
        layout.break_all_lines(None);
        let mut count = 0;
        for run in layout.lines().flat_map(|line| line.runs()) {
            for cluster in run.clusters() {
                assert!(
                    (cluster.advance() - 7.0).abs() < 0.01,
                    "unsafe symbol advance {} at {:?}, weight {weight}, italic {italic}",
                    cluster.advance(),
                    cluster.text_range()
                );
                assert!(cluster.glyphs().all(|glyph| glyph.id != 0));
                count += 1;
            }
        }
        assert_eq!(count, text.chars().count());
    }
}

#[test]
fn bundled_fonts_have_unique_valid_family_names() {
    let options = font_options("", &[]);
    assert_eq!(options.len(), BUNDLED_FONTS.len());
    assert!(
        options
            .iter()
            .all(|family| valid_font_family(family).is_some())
    );
}

#[test]
fn bundled_fonts_always_precede_system_fonts() {
    let options = font_options(
        "Saved Monospace",
        &[
            "Zed Mono".to_owned(),
            "jetbrains mono".to_owned(),
            "Another Mono".to_owned(),
        ],
    );
    let bundled = BUNDLED_FONTS
        .iter()
        .map(|font| font.family)
        .collect::<Vec<_>>();

    assert_eq!(&options[..bundled.len()], bundled);
    assert_eq!(
        options[bundled.len()..],
        ["Another Mono", "Saved Monospace", "Zed Mono"]
    );
}

#[test]
fn options_keep_the_selected_font_within_the_cap() {
    let selected = "Saved Monospace";
    let system_families = (0..MAX_FONT_OPTIONS + 20)
        .map(|index| format!("System Mono {index:03}"))
        .collect::<Vec<_>>();
    let options = font_options(selected, &system_families);

    assert_eq!(options.len(), MAX_FONT_OPTIONS);
    assert!(options.iter().any(|family| family == selected));
}

#[test]
fn options_are_case_insensitively_deduplicated() {
    let options = font_options(
        "JetBrains Mono",
        &["jetbrains mono".to_owned(), "Another Mono".to_owned()],
    );

    assert_eq!(
        options
            .iter()
            .filter(|family| family.eq_ignore_ascii_case("JetBrains Mono"))
            .count(),
        1
    );
}

#[test]
fn registry_matches_registered_bundled_families_case_insensitively() {
    let mut registry = FontRegistry::new();
    assert!(!registry.is_registered("jetbrains mono"));

    registry.registered_families.insert(BUNDLED_UI_FONT_FAMILY);

    assert!(registry.is_registered("jetbrains mono"));
    assert!(!registry.is_registered("System Monospace"));
}

#[test]
fn default_ui_font_is_available_without_external_resources() {
    let resources = FontResources {
        directories: Vec::new(),
    };
    let loaded = resources
        .load_bundled_font(BUNDLED_UI_FONT_FAMILY)
        .expect("embedded UI font should load")
        .expect("default UI font should be bundled");

    assert_eq!(loaded.family, BUNDLED_UI_FONT_FAMILY);
    assert!(matches!(loaded.source, BundledFontSource::Embedded));
}

#[test]
fn embedded_ui_font_registers_regular_bold_and_italic_faces() {
    let resources = FontResources {
        directories: Vec::new(),
    };
    let loaded = resources
        .load_bundled_font(BUNDLED_UI_FONT_FAMILY)
        .expect("embedded UI font should load")
        .expect("default UI font should be bundled");
    let mut collection = fontique::Collection::new(fontique::CollectionOptions {
        shared: false,
        system_fonts: false,
    });

    register_loaded_font_in_collection(&mut collection, loaded)
        .expect("embedded UI font faces should register");

    let family = collection
        .family_by_name(BUNDLED_UI_FONT_FAMILY)
        .expect("registered UI font family should exist");
    assert!(
        family
            .fonts()
            .iter()
            .any(|font| font.style() == fontique::FontStyle::Normal
                && font.weight() == fontique::FontWeight::NORMAL)
    );
    assert!(
        family
            .fonts()
            .iter()
            .any(|font| font.style() == fontique::FontStyle::Normal
                && font.weight() == fontique::FontWeight::BOLD)
    );
    assert!(
        family
            .fonts()
            .iter()
            .any(|font| font.style() == fontique::FontStyle::Italic
                && font.weight() == fontique::FontWeight::NORMAL)
    );
    assert!(
        family
            .fonts()
            .iter()
            .any(|font| font.style() == fontique::FontStyle::Italic
                && font.weight() == fontique::FontWeight::BOLD)
    );
}

#[test]
fn terminal_font_loading_includes_required_font_families_without_duplicates() {
    assert_eq!(
        terminal_bundled_font_families("JetBrains Mono"),
        [
            BUNDLED_UI_FONT_FAMILY,
            TERMINAL_CJK_FALLBACK_FONT_FAMILY,
            TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
        ]
    );
    assert_eq!(
        terminal_bundled_font_families(TERMINAL_CJK_FALLBACK_FONT_FAMILY),
        [
            TERMINAL_CJK_FALLBACK_FONT_FAMILY,
            BUNDLED_UI_FONT_FAMILY,
            TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
        ]
    );
    assert_eq!(
        terminal_bundled_font_families(TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY),
        [
            TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
            BUNDLED_UI_FONT_FAMILY,
            TERMINAL_CJK_FALLBACK_FONT_FAMILY,
        ]
    );
    assert_eq!(
        terminal_bundled_font_families("System Monospace"),
        [
            BUNDLED_UI_FONT_FAMILY,
            TERMINAL_CJK_FALLBACK_FONT_FAMILY,
            TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
        ]
    );
}

fn first_family_with_glyph(
    collection: &mut fontique::Collection,
    primary: &str,
    script: [u8; 4],
    character: char,
) -> Option<String> {
    let mut cache = fontique::SourceCache::default();
    let mut selected = None;
    {
        let mut query = collection.query(&mut cache);
        query.set_families([primary]);
        query.set_fallbacks(fontique::FallbackKey::new(
            fontique::Script::from_bytes(script),
            None,
        ));
        query.matches_with(|font| {
            if font
                .charmap()
                .and_then(|map| map.map(character as u32))
                .is_some_and(|glyph| glyph != 0)
            {
                selected = Some(font.family.0);
                fontique::QueryStatus::Stop
            } else {
                fontique::QueryStatus::Continue
            }
        });
    }
    selected.and_then(|family| collection.family_name(family).map(str::to_owned))
}

#[test]
fn bundled_registration_preserves_existing_script_fallbacks() {
    let resources = FontResources {
        directories: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")],
    };
    let mut collection = fontique::Collection::new(fontique::CollectionOptions {
        shared: false,
        system_fonts: false,
    });
    let font = resources
        .load_bundled_font(BUNDLED_UI_FONT_FAMILY)
        .unwrap()
        .unwrap();
    register_loaded_font_in_collection(&mut collection, font).unwrap();
    let existing = collection.family_id(BUNDLED_UI_FONT_FAMILY).unwrap();
    for script in [*b"Hani", *b"Zyyy"] {
        assert!(collection.set_fallbacks(
            fontique::FallbackKey::new(fontique::Script::from_bytes(script), None),
            [existing].into_iter(),
        ));
    }
    for family in [
        TERMINAL_CJK_FALLBACK_FONT_FAMILY,
        TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
    ] {
        let font = resources.load_bundled_font(family).unwrap().unwrap();
        register_loaded_font_in_collection(&mut collection, font).unwrap();
    }
    for script in [*b"Hani", *b"Zyyy"] {
        let fallbacks = collection
            .fallback_families(fontique::FallbackKey::new(
                fontique::Script::from_bytes(script),
                None,
            ))
            .collect::<Vec<_>>();
        assert!(
            fallbacks.contains(&existing),
            "must preserve native fallback for {script:?}"
        );
    }
}

#[test]
fn discovered_fallbacks_resolve_unknown_families_and_survive_later_registration() {
    use i_slint_core::textlayout::sharedparley::parley;
    let resources = FontResources {
        directories: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")],
    };
    let mut fonts = parley::FontContext {
        collection: fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
        source_cache: Default::default(),
    };
    let primary = resources
        .load_bundled_font(BUNDLED_UI_FONT_FAMILY)
        .unwrap()
        .unwrap();
    register_loaded_font_in_collection(&mut fonts.collection, primary).unwrap();
    let bytes = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts/IosevkaTerm-Regular.ttf"),
    )
    .unwrap();
    // Simulate a newly installed family that production code has never named.
    let candidate = "AxSSH Unseen Monospace Fixture";
    fonts.collection.register_fonts(
        fontique::Blob::new(Arc::new(bytes.clone())),
        Some(fontique::FontInfoOverride {
            family_name: Some(candidate),
            ..Default::default()
        }),
    );
    let candidate_id = fonts.collection.family_id(candidate).unwrap();
    assert_eq!(
        first_family_with_glyph(&mut fonts.collection, BUNDLED_UI_FONT_FAMILY, *b"Zyyy", '↳'),
        None
    );
    let mut over_budget = vec!["Missing family".to_owned(); MAX_FONT_OPTIONS];
    over_budget.push(candidate.to_owned());
    configure_terminal_fallbacks(&mut fonts.collection, &over_budget).unwrap();
    assert_eq!(
        first_family_with_glyph(&mut fonts.collection, BUNDLED_UI_FONT_FAMILY, *b"Zyyy", '↳'),
        None
    );
    let candidates = [
        "Missing family".to_owned(),
        candidate.to_owned(),
        candidate.to_owned(),
    ];
    configure_terminal_fallbacks(&mut fonts.collection, &candidates).unwrap();
    assert_eq!(
        first_family_with_glyph(&mut fonts.collection, BUNDLED_UI_FONT_FAMILY, *b"Zyyy", '↳'),
        Some(candidate.to_owned())
    );

    let mut layouts = parley::LayoutContext::<()>::new();
    let mut builder = layouts.ranged_builder(&mut fonts, "↳", 1.0, false);
    builder.push_default(parley::StyleProperty::FontFamily(
        parley::FontFamily::Single(parley::FontFamilyName::named(BUNDLED_UI_FONT_FAMILY)),
    ));
    let mut layout = builder.build("↳");
    layout.break_all_lines(None);
    let runs = layout
        .lines()
        .flat_map(|line| line.runs())
        .collect::<Vec<_>>();
    assert!(!runs.is_empty());
    assert!(
        runs.iter()
            .all(|run| run.font().data.as_ref() == bytes.as_slice())
    );
    assert!(runs.iter().all(|run| {
        run.clusters()
            .all(|cluster| cluster.glyphs().all(|glyph| glyph.id != 0))
    }));

    // A later bundled selection must retain the installed candidate, regardless
    // of registration order, and repeated configuration must not grow the list.
    for family in [
        TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
        TERMINAL_CJK_FALLBACK_FONT_FAMILY,
    ] {
        let font = resources.load_bundled_font(family).unwrap().unwrap();
        register_loaded_font_in_collection(&mut fonts.collection, font).unwrap();
    }
    configure_terminal_fallbacks(&mut fonts.collection, &candidates).unwrap();
    for (script, first) in [
        (*b"Hani", TERMINAL_CJK_FALLBACK_FONT_FAMILY),
        (*b"Zyyy", TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY),
    ] {
        let key = fontique::FallbackKey::new(fontique::Script::from_bytes(script), None);
        let before = fonts.collection.fallback_families(key).collect::<Vec<_>>();
        assert_eq!(before.len(), 3);
        assert_eq!(fonts.collection.family_name(before[0]), Some(first));
        assert_eq!(before.iter().filter(|&&id| id == candidate_id).count(), 1);
        configure_terminal_fallbacks(&mut fonts.collection, &candidates).unwrap();
        assert_eq!(
            fonts.collection.fallback_families(key).collect::<Vec<_>>(),
            before
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn native_system_fallbacks_shape_reported_symbols_in_all_styles() {
    use i_slint_core::textlayout::sharedparley::parley;
    let resources = FontResources {
        directories: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")],
    };
    let mut fonts = parley::FontContext {
        collection: fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: true,
        }),
        source_cache: Default::default(),
    };
    let key = fontique::FallbackKey::new(fontique::Script::from_bytes(*b"Hani"), None);
    let native = fonts.collection.fallback_families(key).collect::<Vec<_>>();
    for family in [
        BUNDLED_UI_FONT_FAMILY,
        TERMINAL_CJK_FALLBACK_FONT_FAMILY,
        TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
    ] {
        let font = resources.load_bundled_font(family).unwrap().unwrap();
        register_loaded_font_in_collection(&mut fonts.collection, font).unwrap();
    }
    configure_terminal_fallbacks(&mut fonts.collection, &discover_system_monospace_families())
        .unwrap();
    let merged = fonts.collection.fallback_families(key).collect::<Vec<_>>();
    assert!(native.iter().all(|id| merged.contains(id)));
    let mut layouts = parley::LayoutContext::<()>::new();
    for text in ["↳⎿", "↳✳✻·✻✽✻✶·", "⏺"] {
        for (weight, italic) in [(400.0, false), (700.0, false), (400.0, true), (700.0, true)] {
            let mut builder = layouts.ranged_builder(&mut fonts, text, 1.0, false);
            builder.push_default(parley::StyleProperty::FontFamily(
                parley::FontFamily::Single(parley::FontFamilyName::named(
                    TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
                )),
            ));
            builder.push_default(parley::StyleProperty::FontSize(14.0));
            builder.push_default(parley::StyleProperty::FontWeight(parley::FontWeight::new(
                weight,
            )));
            builder.push_default(parley::StyleProperty::FontStyle(if italic {
                parley::FontStyle::Italic
            } else {
                parley::FontStyle::Normal
            }));
            let mut layout = builder.build(text);
            layout.break_all_lines(None);
            let runs = layout
                .lines()
                .flat_map(|line| line.runs())
                .collect::<Vec<_>>();
            assert!(!runs.is_empty());
            assert!(
                runs.iter().all(|run| run.advance() > 0.0
                    && run
                        .clusters()
                        .all(|cluster| cluster.glyphs().all(|glyph| glyph.id != 0))),
                "{text:?}: weight={weight}, italic={italic}"
            );
        }
    }
}

#[test]
fn symbol_fallback_covers_return_arrow_without_replacing_han_or_box_glyphs() {
    let resources = FontResources {
        directories: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")],
    };
    let mut collection = fontique::Collection::new(fontique::CollectionOptions {
        shared: false,
        system_fonts: false,
    });
    for family in [BUNDLED_UI_FONT_FAMILY, TERMINAL_CJK_FALLBACK_FONT_FAMILY] {
        let font = resources
            .load_bundled_font(family)
            .expect("bundled font should load")
            .expect("font should be bundled");
        register_loaded_font_in_collection(&mut collection, font).expect("font should register");
    }
    assert_eq!(
        first_family_with_glyph(
            &mut collection,
            TERMINAL_CJK_FALLBACK_FONT_FAMILY,
            *b"Zyyy",
            '↴',
        ),
        None,
    );

    let symbol_font = resources
        .load_bundled_font(TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY)
        .expect("symbol font should load")
        .expect("symbol font should be bundled");
    register_loaded_font_in_collection(&mut collection, symbol_font)
        .expect("symbol font should register");

    let han_names = collection
        .fallback_families(fontique::FallbackKey::new(
            fontique::Script::from_bytes(*b"Hani"),
            None,
        ))
        .collect::<Vec<_>>()
        .into_iter()
        .filter_map(|id| collection.family_name(id).map(str::to_owned))
        .collect::<Vec<_>>();
    assert_eq!(
        han_names,
        [
            TERMINAL_CJK_FALLBACK_FONT_FAMILY,
            TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
        ]
    );
    assert_eq!(
        first_family_with_glyph(
            &mut collection,
            TERMINAL_CJK_FALLBACK_FONT_FAMILY,
            *b"Zyyy",
            '↴',
        ),
        Some(TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY.to_owned())
    );
    assert_eq!(
        first_family_with_glyph(&mut collection, BUNDLED_UI_FONT_FAMILY, *b"Latn", '↴'),
        Some(TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY.to_owned())
    );
    assert_eq!(
        first_family_with_glyph(&mut collection, BUNDLED_UI_FONT_FAMILY, *b"Hani", '中'),
        Some(TERMINAL_CJK_FALLBACK_FONT_FAMILY.to_owned())
    );
    assert_eq!(
        first_family_with_glyph(
            &mut collection,
            TERMINAL_CJK_FALLBACK_FONT_FAMILY,
            *b"Zyyy",
            '─',
        ),
        Some(TERMINAL_CJK_FALLBACK_FONT_FAMILY.to_owned())
    );
}

#[test]
fn maple_retains_han_priority_when_symbol_font_registers_first() {
    let resources = FontResources {
        directories: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")],
    };
    let mut collection = fontique::Collection::new(fontique::CollectionOptions {
        shared: false,
        system_fonts: false,
    });
    for family in [
        TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
        TERMINAL_CJK_FALLBACK_FONT_FAMILY,
    ] {
        let font = resources
            .load_bundled_font(family)
            .expect("bundled font should load")
            .expect("font should be bundled");
        register_loaded_font_in_collection(&mut collection, font).expect("font should register");
    }
    assert_eq!(
        first_family_with_glyph(&mut collection, "Unregistered primary", *b"Hani", '中'),
        Some(TERMINAL_CJK_FALLBACK_FONT_FAMILY.to_owned())
    );
    assert_eq!(
        first_family_with_glyph(&mut collection, "Unregistered primary", *b"Zyyy", '↴'),
        Some(TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY.to_owned())
    );
}

#[test]
fn maple_is_the_only_registered_han_fallback() {
    let resources = FontResources {
        directories: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")],
    };
    let maple = resources
        .load_bundled_font(TERMINAL_CJK_FALLBACK_FONT_FAMILY)
        .expect("Maple font resources should load")
        .expect("Maple should be a bundled font");
    assert!(matches!(&maple.source, BundledFontSource::Paths(_)));
    let mut collection = fontique::Collection::new(fontique::CollectionOptions {
        shared: false,
        system_fonts: false,
    });

    register_loaded_font_in_collection(&mut collection, maple)
        .expect("Maple should register as the Han fallback");

    let fallback_ids = collection
        .fallback_families(fontique::FallbackKey::new(
            fontique::Script::from_bytes(*b"Hani"),
            None,
        ))
        .collect::<Vec<_>>();
    assert_eq!(fallback_ids.len(), 1);
    assert_eq!(
        collection.family_name(fallback_ids[0]),
        Some(TERMINAL_CJK_FALLBACK_FONT_FAMILY)
    );
}
