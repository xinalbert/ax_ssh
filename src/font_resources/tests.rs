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
