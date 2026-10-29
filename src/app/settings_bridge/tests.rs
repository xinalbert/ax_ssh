use super::*;

#[test]
fn preview_only_loads_new_font_families_once() {
    let previous = AppSettings::default();
    let mut current = previous.clone();
    current.appearance.application_font_family = "Maple Mono NF CN".to_owned();
    current.appearance.terminal_font_family = "Maple Mono NF CN".to_owned();

    assert_eq!(
        changed_font_families(&previous, &current),
        ["Maple Mono NF CN"]
    );
}

#[test]
fn preview_replaces_memory_without_persisting() {
    let path = std::env::temp_dir()
        .join(format!("axssh-settings-preview-{}", Uuid::new_v4()))
        .join("sessions.json");
    let state = Arc::new(Mutex::new(AppState::new(
        ConfigStore::new(path.clone()),
        SessionStore::default(),
    )));
    let mut settings = AppSettings::default();
    settings.terminal.scrollback_lines = 321;
    settings.terminal.alternate_scroll_to_home = true;
    settings.ui_language = UiLanguage::from_setting("zh-CN");

    let expected_language = state.lock().unwrap().sessions.settings.ui_language;

    let loaded = apply_preview_settings(&state, settings.clone()).unwrap();

    assert!(loaded.is_empty());
    assert_eq!(
        state.lock().unwrap().sessions.settings.ui_language,
        expected_language
    );
    settings.ui_language = expected_language;
    assert_eq!(state.lock().unwrap().sessions.settings, settings);
    assert!(!path.exists());
}

#[test]
fn language_save_persists_only_after_success() {
    let path = std::env::temp_dir()
        .join(format!("axssh-language-save-{}", Uuid::new_v4()))
        .join("sessions.json");
    let state = Arc::new(Mutex::new(AppState::new(
        ConfigStore::new(path.clone()),
        SessionStore::default(),
    )));

    let revision = AtomicU64::new(1);
    assert!(save_ui_language(&state, UiLanguage::from_setting("zh-CN"), 1, &revision).unwrap());

    assert_eq!(
        state.lock().unwrap().sessions.settings.ui_language,
        UiLanguage::from_setting("zh-CN")
    );
    let persisted = ConfigStore::new(path).load().unwrap();
    assert_eq!(
        persisted.settings.ui_language,
        UiLanguage::from_setting("zh-CN")
    );
}

#[test]
fn language_save_keeps_other_preview_settings_unpersisted() {
    let path = std::env::temp_dir()
        .join(format!("axssh-language-preview-{}", Uuid::new_v4()))
        .join("sessions.json");
    let state = Arc::new(Mutex::new(AppState::new(
        ConfigStore::new(path.clone()),
        SessionStore::default(),
    )));
    let mut preview = AppSettings::default();
    preview.terminal.scrollback_lines = 321;
    apply_preview_settings(&state, preview).expect("preview should update memory");

    let revision = AtomicU64::new(1);
    assert!(save_ui_language(&state, UiLanguage::from_setting("zh-CN"), 1, &revision).unwrap());

    assert_eq!(
        state
            .lock()
            .unwrap()
            .sessions
            .settings
            .terminal
            .scrollback_lines,
        321
    );
    let persisted = ConfigStore::new(path).load().unwrap();
    assert_eq!(
        persisted.settings.terminal.scrollback_lines,
        AppSettings::default().terminal.scrollback_lines
    );
    assert_eq!(
        persisted.settings.ui_language,
        UiLanguage::from_setting("zh-CN")
    );
}

#[test]
fn stale_language_save_cannot_overwrite_latest_request() {
    let path = std::env::temp_dir()
        .join(format!("axssh-language-revision-{}", Uuid::new_v4()))
        .join("sessions.json");
    let state = Arc::new(Mutex::new(AppState::new(
        ConfigStore::new(path.clone()),
        SessionStore::default(),
    )));
    let latest_revision = AtomicU64::new(2);

    assert!(
        save_ui_language(
            &state,
            UiLanguage::from_setting("zh-CN"),
            2,
            &latest_revision,
        )
        .unwrap()
    );
    assert!(
        !save_ui_language(&state, UiLanguage::from_setting("en"), 1, &latest_revision).unwrap()
    );

    assert_eq!(
        state.lock().unwrap().sessions.settings.ui_language,
        UiLanguage::from_setting("zh-CN")
    );
    let persisted = ConfigStore::new(path).load().unwrap();
    assert_eq!(
        persisted.settings.ui_language,
        UiLanguage::from_setting("zh-CN")
    );
}

#[test]
fn settings_search_matches_titles_descriptions_and_sections_case_insensitively() {
    let title_matches = settings_search_results("FONT", "english");
    assert!(
        title_matches
            .iter()
            .any(|entry| { entry.section == "Appearance" && entry.title == "Font family" })
    );
    assert!(
        title_matches
            .iter()
            .any(|entry| { entry.section == "Terminal" && entry.title == "Font family" })
    );

    let description_matches = settings_search_results("forwarded graphical", "english");
    assert_eq!(description_matches.len(), 1);
    assert_eq!(description_matches[0].section, "X11");
    assert_eq!(description_matches[0].title, "Provider");

    let section_matches = settings_search_results("workspace", "english");
    assert!(
        section_matches.iter().any(|entry| {
            entry.section == "Workspace" && entry.title == "Session sidebar width"
        })
    );

    let renderer_matches = settings_search_results("restarting", "english");
    assert_eq!(
        renderer_matches.len(),
        if cfg!(target_os = "macos") { 2 } else { 1 }
    );
    assert!(
        renderer_matches
            .iter()
            .all(|entry| entry.section == "Appearance")
    );
    assert!(
        renderer_matches
            .iter()
            .any(|entry| entry.title == "Renderer")
    );
    assert_eq!(
        renderer_matches
            .iter()
            .any(|entry| entry.title == "Software presentation"),
        cfg!(target_os = "macos")
    );

    let cache_matches = settings_search_results("graphics memory", "english");
    assert_eq!(cache_matches.len(), 1);
    assert_eq!(cache_matches[0].section, "Appearance");
    assert_eq!(cache_matches[0].title, "Cache unchanged terminal rows");

    let blink_matches = settings_search_results("blinking cursor", "english");
    assert_eq!(blink_matches.len(), 1);
    assert_eq!(blink_matches[0].section, "Appearance");
    assert_eq!(blink_matches[0].title, "Blink terminal cursor");

    let home_matches = settings_search_results("line start", "english");
    assert_eq!(home_matches.len(), 1);
    assert_eq!(home_matches[0].section, "Terminal");
    assert_eq!(
        home_matches[0].title,
        "Keep cursor at line start while scrolling"
    );

    let edit_matches = settings_search_results("remote edit file", "english");
    assert_eq!(edit_matches.len(), 1);
    assert_eq!(edit_matches[0].section, "SFTP");
    assert_eq!(edit_matches[0].title, "Remote edit file limit (MiB)");
    let sftp_matches = settings_search_results("bandwidth limit", "english");
    assert_eq!(sftp_matches.len(), 1);
    assert_eq!(sftp_matches[0].section, "SFTP");

    for (query, title) in [
        ("select all", "Select All"),
        ("previous tab", "Previous Tab"),
        ("next tab", "Next Tab"),
    ] {
        let fixed_shortcut_matches = settings_search_results(query, "english");
        assert_eq!(fixed_shortcut_matches.len(), 1);
        assert_eq!(fixed_shortcut_matches[0].section, "Shortcuts");
        assert_eq!(fixed_shortcut_matches[0].title, title);
    }
}

#[test]
fn settings_search_ignores_empty_and_unknown_queries() {
    assert!(settings_search_results("   ", "english").is_empty());
    assert!(settings_search_results("not-a-setting", "english").is_empty());
}

#[test]
fn settings_search_matches_japanese_and_english_with_stable_routes() {
    let matches = settings_search_results("フォント", "japanese");
    assert!(matches.iter().any(|entry| {
        entry.section == "Appearance"
            && entry.title == "フォント"
            && entry.description == "アプリケーション画面のフォント"
    }));
    assert!(matches.iter().any(|entry| {
        entry.section == "Terminal" && entry.description == "端末の文字セル用フォント"
    }));
    let language = settings_search_results("表示に使用する言語", "japanese");
    assert_eq!(language.len(), 1);
    assert_eq!(language[0].section, "General");
    assert_eq!(language[0].title, "言語");
    let english = settings_search_results("renderer", "japanese");
    assert_eq!(english.len(), 1);
    assert_eq!(english[0].title, "レンダラー");
    assert_eq!(english[0].section, "Appearance");
    let restarts = settings_search_results("再起動", "japanese");
    assert_eq!(
        restarts.len(),
        if cfg!(target_os = "macos") { 2 } else { 1 }
    );
    for (query, title) in [("すべて選択", "すべて選択"), ("次のタブ", "次のタブ")]
    {
        let matches = settings_search_results(query, "ja");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].title, title);
        assert_eq!(matches[0].section, "Shortcuts");
    }
}

#[test]
fn settings_search_generated_locale_tables_match_source_rows() {
    for language in UiLanguage::bundled().filter(|language| language.as_setting() != "en") {
        let locale = language.as_setting();
        let rows = localized_catalog(locale).expect("bundled locale has a search table");
        assert_eq!(rows.len(), SETTINGS_SEARCH_CATALOG.len());
        for (section, title, description) in rows {
            assert!(!section.is_empty() && !title.is_empty() && !description.is_empty());
        }
    }
    assert!(localized_catalog("en").is_none());
    assert!(localized_catalog("unsupported").is_none());
}

#[test]
fn common_language_search_preserves_routes_and_localized_titles() {
    for (locale, query, title) in [
        ("de", "Schriftfamilie", "Schriftfamilie"),
        ("es", "Familia de fuentes", "Familia de fuentes"),
        ("fr", "Famille de polices", "Famille de polices"),
        ("ko", "글꼴 모음", "글꼴 모음"),
        ("pt-BR", "Família de fontes", "Família de fontes"),
        ("zh-TW", "字型系列", "字型系列"),
        ("ru", "Семейство шрифтов", "Семейство шрифтов"),
        ("uk", "Родина шрифтів", "Родина шрифтів"),
        ("en-GB", "Font family", "Font family"),
    ] {
        let matches = settings_search_results(query, locale);
        assert!(
            matches
                .iter()
                .any(|entry| { entry.section == "Appearance" && entry.title == title }),
            "{locale} search must keep the English route with translated text"
        );
        assert!(
            settings_search_results("renderer", locale)
                .iter()
                .any(|entry| entry.section == "Appearance")
        );
    }
}

#[test]
fn british_english_search_matches_localized_and_source_spellings() {
    for query in ["colour", "color"] {
        let matches = settings_search_results(query, "en-GB");
        assert!(
            matches
                .iter()
                .any(|entry| { entry.section == "Appearance" && entry.title == "Colour palette" })
        );
    }
    assert!(
        settings_search_results("licence", "en-GB")
            .iter()
            .any(|entry| entry.section == "About")
    );
}

#[test]
fn settings_search_matches_chinese_without_changing_route_ids() {
    let matches = settings_search_results("界面字体", "simplified-chinese");

    assert!(matches.iter().any(|entry| {
        entry.section == "Appearance"
            && entry.title == "字体系列"
            && entry.description == "应用界面字体"
    }));

    let renderer_matches = settings_search_results("重启", "simplified-chinese");
    assert_eq!(
        renderer_matches.len(),
        if cfg!(target_os = "macos") { 2 } else { 1 }
    );
    assert!(
        renderer_matches
            .iter()
            .all(|entry| entry.section == "Appearance")
    );
    assert!(renderer_matches.iter().any(|entry| entry.title == "渲染器"));
    assert_eq!(
        renderer_matches
            .iter()
            .any(|entry| entry.title == "软件呈现方式"),
        cfg!(target_os = "macos")
    );

    let compact_matches = settings_search_results("省略默认背景", "simplified-chinese");
    assert_eq!(compact_matches.len(), 1);
    assert_eq!(compact_matches[0].section, "Appearance");
    assert_eq!(compact_matches[0].title, "紧凑终端渲染");

    let home_matches = settings_search_results("行首", "simplified-chinese");
    assert_eq!(home_matches.len(), 1);
    assert_eq!(home_matches[0].section, "Terminal");
    assert_eq!(home_matches[0].title, "滚轮时保持光标在行首");

    let blink_matches = settings_search_results("终端光标", "simplified-chinese");
    assert_eq!(blink_matches.len(), 1);
    assert_eq!(blink_matches[0].section, "Appearance");
    assert_eq!(blink_matches[0].title, "终端光标闪烁");

    let edit_matches = settings_search_results("远程编辑", "simplified-chinese");
    assert_eq!(edit_matches.len(), 1);
    assert_eq!(edit_matches[0].section, "SFTP");
    assert_eq!(edit_matches[0].title, "远程编辑文件大小上限（MiB）");
    for (query, title) in [
        ("全选", "全选"),
        ("上一个标签页", "上一个标签页"),
        ("下一个标签页", "下一个标签页"),
    ] {
        let fixed_shortcut_matches = settings_search_results(query, "simplified-chinese");
        assert_eq!(fixed_shortcut_matches.len(), 1);
        assert_eq!(fixed_shortcut_matches[0].section, "Shortcuts");
        assert_eq!(fixed_shortcut_matches[0].title, title);
    }
}

#[test]
fn local_icon_save_preserves_preview_and_ignores_superseded_choices() {
    let directory = std::env::temp_dir().join(format!("axssh-local-icon-{}", Uuid::new_v4()));
    let config = ConfigStore::new(directory.join("sessions.json"));
    let state = Arc::new(Mutex::new(AppState::new(
        config.clone(),
        SessionStore::default(),
    )));
    let mut preview = AppSettings::default();
    preview.terminal.scrollback_lines = 321;
    apply_preview_settings(&state, preview.clone()).unwrap();
    let latest = AtomicU64::new(2);
    assert!(
        save_local_shell_icon(&state, Some("windows".into()), 1, &latest)
            .unwrap()
            .is_none()
    );
    assert!(
        save_local_shell_icon(&state, Some("debian".into()), 2, &latest)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        config.load().unwrap().settings.terminal.scrollback_lines,
        AppSettings::default().terminal.scrollback_lines
    );
    apply_preview_settings(&state, preview.clone()).unwrap();
    let saved = save_workspace_settings(&state, preview).unwrap();
    assert_eq!(
        saved.appearance.local_shell_icon_override.as_deref(),
        Some("debian")
    );
    assert!(
        save_local_shell_icon(&state, None, 2, &latest)
            .unwrap()
            .is_some()
    );
    assert!(
        config
            .load()
            .unwrap()
            .settings
            .appearance
            .local_shell_icon_override
            .is_none()
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn icon_picker_search_uses_shared_names_ids_and_aliases() {
    assert_eq!(
        system_icon_choices("").len(),
        ax_ssh::config::system_icon_choices().len()
    );
    assert!(
        system_icon_choices("RED HAT")
            .iter()
            .any(|icon| icon.id == "rhel")
    );
    assert_eq!(system_icon_choices("microsoft")[0].id, "windows");
    assert!(system_icon_choices("absent-distribution").is_empty());
    assert_eq!(
        settings_search_results("本地终端图标", "simplified-chinese")[0].section,
        "General"
    );
}

#[test]
fn icon_picker_current_name_uses_exact_identity_instead_of_search_matches() {
    assert_eq!(system_icon_choice("linux").name, "Linux");
    assert_eq!(system_icon_choice("ubuntu").name, "Ubuntu");
    let rocky = system_icon_choice("Rocky Linux");
    assert_eq!(rocky.id, "rocky-linux");
    assert_eq!(rocky.name, "Rocky Linux");
    assert_eq!(system_icon_choice("microsoft").id, "windows");
    assert_eq!(system_icon_choice(""), SystemIconChoiceRow::default());
    assert_eq!(
        system_icon_choice("unknown-system"),
        SystemIconChoiceRow::default()
    );
}
