use super::*;

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
fn terminal_font_loading_includes_cjk_and_symbol_fallbacks() {
    assert_eq!(
        terminal_bundled_font_families("JetBrains Mono"),
        [
            "JetBrains Mono",
            TERMINAL_CJK_FALLBACK_FONT_FAMILY,
            TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
        ]
    );
    assert_eq!(
        terminal_bundled_font_families(TERMINAL_CJK_FALLBACK_FONT_FAMILY),
        [
            TERMINAL_CJK_FALLBACK_FONT_FAMILY,
            TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
        ]
    );
    assert_eq!(
        terminal_bundled_font_families(TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY),
        [
            TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
            TERMINAL_CJK_FALLBACK_FONT_FAMILY,
        ]
    );
    assert_eq!(
        terminal_bundled_font_families("System Monospace"),
        [
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
