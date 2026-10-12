use super::*;

#[test]
fn registry_choices_round_trip_without_persisting_indices() {
    for (index, language) in UiLanguage::bundled().enumerate() {
        let selector = i32::try_from(index + 1).expect("bounded language registry");
        assert_eq!(language.selector_index(), selector);
        assert_eq!(UiLanguage::from_selector_index(selector), language);
        assert_eq!(UiLanguage::from_setting(language.as_setting()), language);
        assert!(!language.native_name().is_empty());
        let serialized = serde_json::to_string(&language).expect("serialize locale");
        assert_eq!(
            serde_json::from_str::<UiLanguage>(&serialized).expect("deserialize locale"),
            language
        );
    }
    for index in [-1, i32::MIN, i32::MAX] {
        assert_eq!(UiLanguage::from_selector_index(index), UiLanguage::SYSTEM);
    }
}

#[test]
fn traditional_system_locales_select_the_bundled_traditional_catalog() {
    for locale in ["zh-TW", "zh_HK.UTF-8", "zh-MO", "zh-Hant", "zh-Hant-CN"] {
        assert_eq!(
            UiLanguage::SYSTEM.resolved_locale(Some(locale)),
            "zh-TW",
            "{locale}"
        );
    }
    assert_eq!(UiLanguage::SYSTEM.resolved_locale(Some("zh-Latn")), "en");
    for locale in [
        "zh",
        "zh-CN",
        "zh-SG",
        "zh-Hans",
        "zh-Hans-TW",
        "zh-CN-u-ca-chinese",
    ] {
        assert_eq!(
            UiLanguage::SYSTEM.resolved_locale(Some(locale)),
            "zh-CN",
            "{locale}"
        );
    }
}

#[test]
fn common_system_languages_resolve_and_persist_canonical_locales() {
    for (system, expected) in [
        ("de_DE.UTF-8", "de"),
        ("de-AT", "de"),
        ("es-ES", "es"),
        ("es-MX", "es"),
        ("fr-FR", "fr"),
        ("fr_CA.UTF-8", "fr"),
        ("ko_KR.UTF-8", "ko"),
        ("pt-BR", "pt-BR"),
        ("pt-PT", "pt-BR"),
        ("zh-Hant-TW", "zh-TW"),
        ("ru_RU.UTF-8", "ru"),
        ("ru-KZ", "ru"),
        ("uk_UA.UTF-8", "uk"),
        ("uk", "uk"),
        ("en_GB.UTF-8", "en-GB"),
        ("en-UK", "en-GB"),
    ] {
        assert_eq!(UiLanguage::SYSTEM.resolved_locale(Some(system)), expected);
        let language = UiLanguage::from_setting(expected);
        assert_ne!(language, UiLanguage::SYSTEM);
        assert_eq!(language.resolved_locale(Some("en-US")), expected);
        assert_eq!(
            serde_json::to_value(language).expect("locale serialization"),
            expected
        );
    }
    for (alias, expected) in [
        (" DE ", "de"),
        ("de-DE", "de"),
        ("es-ES", "es"),
        ("fr-FR", "fr"),
        ("ko-KR", "ko"),
        ("pt", "pt-BR"),
        ("PT_br", "pt-BR"),
        ("zh-Hant", "zh-TW"),
        ("zh-HK", "zh-TW"),
        ("RU_ru", "ru"),
        ("UK_ua", "uk"),
        ("EN_gb", "en-GB"),
        ("en-UK", "en-GB"),
    ] {
        assert_eq!(UiLanguage::from_setting(alias).as_setting(), expected);
    }
}

#[test]
fn british_english_preserves_other_english_region_fallbacks() {
    for (locale, expected) in [
        ("en-GB", "en-GB"),
        ("en_GB.UTF-8", "en-GB"),
        ("en-GB-x-private", "en-GB"),
        ("en-Latn-GB", "en-GB"),
        ("en-Latn-GB-u-ca-gregory", "en-GB"),
        ("en-Latn-UK", "en-GB"),
        ("en", "en"),
        ("en-US", "en"),
        ("en-AU", "en"),
        ("en-CA", "en"),
        ("en-Latn-US", "en"),
        ("en-Latn-AU-u-ca-gregory", "en"),
        ("ru-Latn", "en"),
        ("uk-Latn", "en"),
    ] {
        assert_eq!(
            UiLanguage::SYSTEM.resolved_locale(Some(locale)),
            expected,
            "{locale}"
        );
    }
    assert_eq!(UiLanguage::from_setting("uk").as_setting(), "uk");
    assert_ne!(
        UiLanguage::from_setting("uk"),
        UiLanguage::from_setting("en-UK")
    );
    assert_eq!(UiLanguage::from_setting("english").as_setting(), "en");
}

#[test]
fn unavailable_script_still_falls_back_to_english() {
    let languages = [LanguageDefinition {
        locale: "zh-CN",
        native_name: "简体中文",
        aliases: &["zh-Hans"],
        setting_aliases: &[],
        script: Some("Hans"),
        language_default: true,
        region_scripts: &[("TW", "Hant"), ("HK", "Hant"), ("MO", "Hant")],
    }];
    for locale in ["zh-TW", "zh-HK", "zh-MO", "zh-Hant", "zh-Hant-CN"] {
        assert_eq!(resolve_locale_in(Some(locale), &languages), "en");
    }
}

#[test]
fn future_catalogs_match_exact_script_region_and_language_in_order() {
    let languages = [
        LanguageDefinition {
            locale: "en",
            native_name: "English",
            aliases: &[],
            setting_aliases: &[],
            script: None,
            language_default: true,
            region_scripts: &[],
        },
        LanguageDefinition {
            locale: "zh-CN",
            native_name: "简体中文",
            aliases: &["zh-Hans", "zh-SG"],
            setting_aliases: &[],
            script: Some("Hans"),
            language_default: true,
            region_scripts: &[
                ("CN", "Hans"),
                ("SG", "Hans"),
                ("TW", "Hant"),
                ("HK", "Hant"),
                ("MO", "Hant"),
            ],
        },
        LanguageDefinition {
            locale: "zh-TW",
            native_name: "繁體中文",
            aliases: &["zh-Hant", "zh-HK", "zh-MO"],
            setting_aliases: &[],
            script: Some("Hant"),
            language_default: false,
            region_scripts: &[],
        },
        LanguageDefinition {
            locale: "fr",
            native_name: "Français",
            aliases: &[],
            setting_aliases: &[],
            script: None,
            language_default: true,
            region_scripts: &[],
        },
        LanguageDefinition {
            locale: "fr-CA",
            native_name: "Français (Canada)",
            aliases: &[],
            setting_aliases: &[],
            script: None,
            language_default: false,
            region_scripts: &[],
        },
    ];
    for (locale, expected) in [
        ("zh-TW", "zh-TW"),
        ("zh-Hant-CN", "zh-TW"),
        ("zh-Hans-TW", "zh-CN"),
        ("zh_HK.UTF-8", "zh-TW"),
        ("zh-Hant-HK", "zh-TW"),
        ("zh-MO", "zh-TW"),
        ("zh-CN", "zh-CN"),
        ("zh", "zh-CN"),
        ("zh-Latn", "en"),
        ("fr-CA", "fr-CA"),
        ("fr-CA-variant", "fr-CA"),
        ("fr-FR", "fr"),
        ("fr-BE", "fr"),
        ("de-DE", "en"),
        ("en-u-rg-cnzzzz", "en"),
    ] {
        assert_eq!(
            resolve_locale_in(Some(locale), &languages),
            expected,
            "{locale}"
        );
    }
}
