use std::fs;
use std::path::PathBuf;

use uuid::Uuid;

use super::*;

#[test]
fn profile_validation_rejects_missing_host() {
    let profile = SessionProfile::new("demo", "", "alice");
    assert!(profile.validate().is_err());
}

#[test]
fn store_round_trips_and_upserts() {
    let temp = std::env::temp_dir().join(format!("ax-ssh-{}", Uuid::new_v4()));
    let store = ConfigStore::new(&temp);
    let mut data = SessionStore::default();
    assert!(
        data.add_group(" Empty group ")
            .expect("group should be valid")
    );
    let mut profile = SessionProfile::new("demo", "host.example", "alice");
    profile.group_name = "Production".into();
    profile.credential_storage = Some(CredentialStorage::SystemKeyring);
    data.upsert(profile.clone());
    data.upsert(SessionProfile {
        name: "renamed".into(),
        ..profile.clone()
    });
    assert_eq!(data.sessions.len(), 1);
    assert_eq!(data.groups, ["Empty group", "Production"]);
    store.save(&data).expect("save should succeed");
    assert_eq!(store.load().expect("load should succeed"), data);
    data.sessions[0].name = "saved-again".into();
    store.save(&data).expect("replacement save should succeed");
    assert_eq!(store.load().expect("replacement load should succeed"), data);
    let _ = fs::remove_file(temp);
}

#[test]
fn legacy_profile_defaults_group_and_migrates_credential_marker() {
    let id = Uuid::new_v4();
    let json = format!(
        r#"{{"sessions":[{{"id":"{id}","name":"legacy","host":"host.example","port":22,"username":"alice","auth":"Password"}}]}}"#
    );

    let store: SessionStore =
        serde_json::from_str(&json).expect("legacy profile should deserialize");
    assert_eq!(store.sessions[0].group_name, "");
    assert!(store.groups.is_empty());
    assert_eq!(store.sessions[0].credential_storage, None);
    assert_eq!(store.settings, AppSettings::default());
}

#[test]
fn legacy_credential_marker_migrates_to_the_system_keyring() {
    let id = Uuid::new_v4();
    let json = format!(
        r#"{{"sessions":[{{"id":"{id}","name":"legacy","host":"host.example","port":22,"username":"alice","auth":"Password","credential_stored":true}}]}}"#
    );

    let store: SessionStore =
        serde_json::from_str(&json).expect("legacy profile should deserialize");

    assert_eq!(
        store.sessions[0].credential_storage,
        Some(CredentialStorage::SystemKeyring)
    );
    let encoded = serde_json::to_string(&store).expect("migrated store should serialize");
    assert!(encoded.contains("credential_storage"));
    assert!(!encoded.contains("credential_stored"));
}

#[test]
fn global_default_does_not_change_an_existing_credential_reference() {
    let mut store = SessionStore::default();
    let mut profile = SessionProfile::new("demo", "host.example", "alice");
    profile.credential_storage = Some(CredentialStorage::SystemKeyring);
    store.upsert(profile);

    store.settings.credential_storage = CredentialStorage::EncryptedVault;

    assert_eq!(
        store.sessions[0].credential_storage,
        Some(CredentialStorage::SystemKeyring)
    );
    assert_eq!(
        store.settings.credential_storage,
        CredentialStorage::EncryptedVault
    );
}

#[test]
fn appearance_settings_normalize_font_family_and_size() {
    assert_eq!(
        AppearanceSettings::normalized("  Menlo  ", 18, 135, "light", 115, false, true),
        AppearanceSettings {
            terminal_font_family: "Menlo".into(),
            terminal_font_size: 18,
            terminal_line_height_percent: 135,
            terminal_color_scheme: TerminalColorScheme::Light,
            theme: ThemeSettings {
                mode: ThemeMode::Light,
                palette: ThemePaletteKind::AxSsh,
                custom_light: ThemePalette::axssh_light(),
                custom_dark: ThemePalette::axssh_dark(),
            },
            terminal_brightness_percent: 115,
            bright_bold_text: false,
            right_click_copy_or_paste: true,
        }
    );
    assert_eq!(
        AppearanceSettings::normalized("", 100, 1_000, "unknown", 1_000, true, false),
        AppearanceSettings {
            terminal_font_family: DEFAULT_TERMINAL_FONT_FAMILY.into(),
            terminal_font_size: MAX_TERMINAL_FONT_SIZE,
            terminal_line_height_percent: MAX_TERMINAL_LINE_HEIGHT,
            terminal_color_scheme: TerminalColorScheme::Dark,
            theme: ThemeSettings::default(),
            terminal_brightness_percent: MAX_TERMINAL_BRIGHTNESS,
            bright_bold_text: true,
            right_click_copy_or_paste: false,
        }
    );
}

#[test]
fn legacy_appearance_migrates_into_versioned_settings() {
    let json = r#"{
            "sessions": [],
            "appearance": {
                "terminal_font_family": "Menlo",
                "terminal_font_size": 17
            }
        }"#;

    let store: SessionStore = serde_json::from_str(json).expect("legacy settings should load");

    assert_eq!(store.version, CURRENT_SCHEMA_VERSION);
    assert_eq!(store.settings.appearance.terminal_font_family, "Menlo");
    assert_eq!(store.settings.appearance.terminal_font_size, 17);
    assert_eq!(
        store.settings.appearance.terminal_line_height_percent,
        DEFAULT_TERMINAL_LINE_HEIGHT
    );
    assert_eq!(
        store.settings.appearance.terminal_color_scheme,
        TerminalColorScheme::Dark
    );
    assert_eq!(
        store.settings.appearance.terminal_brightness_percent,
        DEFAULT_TERMINAL_BRIGHTNESS
    );
    assert!(store.settings.appearance.bright_bold_text);
    assert!(!store.settings.appearance.right_click_copy_or_paste);
    assert_eq!(store.settings.terminal, TerminalSettings::default());
    assert_eq!(store.settings.shortcuts, ShortcutSettings::default());
    let serialized = serde_json::to_value(store).expect("settings should serialize");
    assert!(serialized.get("settings").is_some());
    assert!(serialized.get("appearance").is_none());
    assert_eq!(
        serialized["settings"]["appearance"]["terminal_line_height_percent"],
        DEFAULT_TERMINAL_LINE_HEIGHT
    );
}

#[test]
fn theme_mode_normalizes_aliases_and_unknown_values() {
    assert_eq!(ThemeMode::from_setting("follow-system"), ThemeMode::System);
    assert_eq!(ThemeMode::from_setting("AUTO"), ThemeMode::System);
    assert_eq!(ThemeMode::from_setting("Solarized Dark"), ThemeMode::Dark);
    assert_eq!(ThemeMode::from_setting("unexpected"), ThemeMode::Dark);
    assert_eq!(
        ThemePaletteKind::from_setting("Solarized Dark"),
        ThemePaletteKind::Solarized
    );
}

#[test]
fn persisted_theme_mode_accepts_system_aliases() {
    for alias in ["system", "follow-system", "auto"] {
        let json = format!(
            r#"{{"version":9,"settings":{{"appearance":{{"theme":{{"mode":"{alias}"}}}}}}}}"#
        );
        let store: SessionStore =
            serde_json::from_str(&json).expect("system mode alias should deserialize");

        assert_eq!(store.settings.appearance.theme.mode, ThemeMode::System);
    }
}

#[test]
fn custom_palette_normalizes_each_hex_value_independently() {
    let palette = ThemePalette::normalized(
        " #abc ",
        "#abcd",
        "#1a2b3c",
        "not-a-color",
        "#12345678",
        "#0A0B0C",
        "#112233",
        "#445566",
        "#778899",
        "#AABBCCDD",
        "#010203",
        "#040506",
        "#070809",
    );

    assert_eq!(palette.background, "#AABBCC");
    assert_eq!(palette.panel, "#AABBCCDD");
    assert_eq!(palette.panel_alt, "#1A2B3C");
    assert_eq!(palette.border, default_theme_border());
    assert_eq!(palette.text, "#12345678");
    assert_eq!(palette.terminal_selection, "#070809");
}

#[test]
fn version_eight_terminal_palette_migrates_to_theme_mode() {
    for (legacy_scheme, expected_mode, expected_palette) in [
        ("dark", ThemeMode::Dark, ThemePaletteKind::AxSsh),
        ("light", ThemeMode::Light, ThemePaletteKind::AxSsh),
        (
            "solarized-dark",
            ThemeMode::Dark,
            ThemePaletteKind::Solarized,
        ),
    ] {
        let json = format!(
            r#"{{"version":8,"settings":{{"appearance":{{"terminal_color_scheme":"{legacy_scheme}"}}}}}}"#
        );
        let store: SessionStore =
            serde_json::from_str(&json).expect("version eight settings should migrate");

        assert_eq!(store.version, CURRENT_SCHEMA_VERSION);
        assert_eq!(store.settings.appearance.theme.mode, expected_mode);
        assert_eq!(store.settings.appearance.theme.palette, expected_palette);
        assert_eq!(
            store.settings.appearance.terminal_color_scheme.as_setting(),
            legacy_scheme
        );
    }
}

#[test]
fn version_eleven_custom_theme_round_trips_without_secrets() {
    let mut settings = AppSettings::default();
    settings.set_theme(ThemeSettings::normalized(
        "dark",
        "custom",
        ThemePalette::axssh_light(),
        ThemePalette::normalized(
            "#102030",
            "#203040",
            "#304050",
            "#405060",
            "#506070",
            "#607080",
            "#708090",
            "#8090A0",
            "#90A0B0",
            "#00000080",
            "#A0B0C0",
            "#0A0B0C",
            "#102938",
        ),
    ));
    let store = SessionStore {
        version: CURRENT_SCHEMA_VERSION,
        groups: Vec::new(),
        sessions: Vec::new(),
        settings,
    };

    let encoded = serde_json::to_string(&store).expect("custom theme should serialize");
    let decoded: SessionStore =
        serde_json::from_str(&encoded).expect("custom theme should deserialize");

    assert_eq!(decoded, store);
    assert!(encoded.contains("\"theme\""));
    assert!(!encoded.contains("password"));
    assert!(!encoded.contains("passphrase"));
}

#[test]
fn version_ten_combined_theme_modes_migrate_without_changing_direction() {
    let solarized: SessionStore = serde_json::from_str(
        r##"{"version":10,"settings":{"appearance":{"theme":{"mode":"solarized-dark"}}}}"##,
    )
    .expect("legacy solarized theme should migrate");
    assert_eq!(solarized.settings.appearance.theme.mode, ThemeMode::Dark);
    assert_eq!(
        solarized.settings.appearance.theme.palette,
        ThemePaletteKind::Solarized
    );

    let custom: SessionStore = serde_json::from_str(
            r##"{"version":10,"settings":{"appearance":{"theme":{"mode":"custom","custom":{"background":"#F8F8F8","panel":"#FFFFFF","panel_alt":"#EEEEEE","border":"#555555","text":"#111111","muted":"#444444","accent":"#005F50","success":"#17633C","danger":"#982A25","overlay":"#00000099","terminal_foreground":"#222222","terminal_background":"#FFFFFF","terminal_selection":"#CDE4F8"}}}}}"##,
        )
        .expect("legacy custom theme should migrate");
    assert_eq!(custom.settings.appearance.theme.mode, ThemeMode::Light);
    assert_eq!(
        custom.settings.appearance.theme.palette,
        ThemePaletteKind::Custom
    );
    assert_eq!(
        custom.settings.appearance.theme.custom_light.background,
        "#F8F8F8"
    );
    assert_eq!(
        custom.settings.appearance.theme.custom_dark,
        ThemePalette::axssh_dark()
    );
}

#[test]
fn custom_palettes_repair_invisible_surfaces_and_semantic_roles() {
    let invisible = ThemePalette::from_hex([
        "#111111",
        "#111111",
        "#111111",
        "#111111",
        "#111111",
        "#111111",
        "#111111",
        "#111111",
        "#111111",
        "#00000000",
        "#111111",
        "#111111",
        "#111111",
    ]);
    let theme = ThemeSettings::normalized("light", "custom", invisible, ThemePalette::axssh_dark());
    let palette = theme.light_palette();
    let surfaces = [&palette.background, &palette.panel, &palette.panel_alt];

    assert!(role_meets_contrast(&palette.text, &surfaces, 4.5));
    assert!(role_meets_contrast(&palette.muted, &surfaces, 4.5));
    assert!(role_meets_contrast(&palette.accent, &surfaces, 4.5));
    assert!(role_meets_contrast(&palette.success, &surfaces, 4.5));
    assert!(role_meets_contrast(&palette.danger, &surfaces, 4.5));
    assert!(role_meets_contrast(&palette.border, &surfaces, 3.0));
    assert!(
        theme_contrast_ratio(&palette.terminal_foreground, &palette.terminal_background)
            .is_some_and(|ratio| ratio >= 4.5)
    );
    assert!(
        theme_contrast_ratio(&palette.terminal_foreground, &palette.terminal_selection)
            .is_some_and(|ratio| ratio >= 4.5)
    );
}

#[test]
fn fixed_palettes_keep_text_states_borders_and_terminal_selection_visible() {
    for palette in [
        ThemePalette::axssh_light(),
        ThemePalette::axssh_dark(),
        ThemePalette::solarized_light(),
        ThemePalette::solarized_dark(),
    ] {
        let surfaces = [&palette.background, &palette.panel, &palette.panel_alt];
        for role in [
            &palette.text,
            &palette.muted,
            &palette.accent,
            &palette.success,
            &palette.danger,
        ] {
            assert!(role_meets_contrast(role, &surfaces, 4.5));
        }
        assert!(role_meets_contrast(&palette.border, &surfaces, 3.0));
        assert!(
            theme_contrast_ratio(&palette.terminal_foreground, &palette.terminal_background)
                .is_some_and(|ratio| ratio >= 4.5)
        );
        assert!(
            theme_contrast_ratio(&palette.terminal_foreground, &palette.terminal_selection)
                .is_some_and(|ratio| ratio >= 4.5)
        );
        assert!(
            theme_contrast_ratio(&palette.terminal_selection, &palette.terminal_background)
                .is_some_and(|ratio| ratio >= 1.5)
        );
    }
}

#[test]
fn legacy_right_click_setting_uses_the_copy_or_paste_parameter() {
    let json = r#"{
            "version": 5,
            "settings": {
                "appearance": {
                    "right_click_copies_selection": true
                }
            }
        }"#;
    let store: SessionStore =
        serde_json::from_str(json).expect("legacy right-click setting should load");
    assert!(store.settings.appearance.right_click_copy_or_paste);

    let serialized = serde_json::to_value(store).expect("settings should serialize");
    let appearance = &serialized["settings"]["appearance"];
    assert_eq!(appearance["right_click_copy_or_paste"], true);
    assert!(appearance.get("right_click_copies_selection").is_none());
}

#[test]
fn app_settings_clamp_all_persisted_dimensions() {
    let settings = AppSettings::normalized(
        "",
        100,
        -1,
        "solarized-dark",
        -1,
        false,
        true,
        -1,
        1,
        1_000,
        "zsh",
        &[SYSTEM_DEFAULT_SHELL.into(), "zsh".into()],
        true,
        20,
        9_000,
        "  #  ",
        "Ctrl+,",
        "Ctrl+Shift+B",
        "Ctrl+Shift+C",
        "Ctrl+Shift+V",
        "encrypted-vault",
    );

    assert_eq!(
        settings.appearance.terminal_font_family,
        DEFAULT_TERMINAL_FONT_FAMILY
    );
    assert_eq!(
        settings.appearance.terminal_font_size,
        MAX_TERMINAL_FONT_SIZE
    );
    assert_eq!(
        settings.appearance.terminal_line_height_percent,
        MIN_TERMINAL_LINE_HEIGHT
    );
    assert_eq!(
        settings.appearance.terminal_color_scheme,
        TerminalColorScheme::SolarizedDark
    );
    assert_eq!(
        settings.appearance.terminal_brightness_percent,
        MIN_TERMINAL_BRIGHTNESS
    );
    assert!(!settings.appearance.bright_bold_text);
    assert!(settings.appearance.right_click_copy_or_paste);
    assert_eq!(settings.terminal.scrollback_lines, MIN_SCROLLBACK_LINES);
    assert_eq!(settings.terminal.default_columns, MIN_TERMINAL_COLUMNS);
    assert_eq!(settings.terminal.default_rows, MAX_TERMINAL_ROWS);
    assert_eq!(settings.terminal.local_shell, "zsh");
    assert!(settings.terminal.option_as_meta);
    assert_eq!(
        settings.terminal.known_shells,
        [SYSTEM_DEFAULT_SHELL, "zsh"]
    );
    assert_eq!(settings.workspace.sidebar_width, MIN_SIDEBAR_WIDTH);
    assert_eq!(settings.workspace.tab_width, MAX_TAB_WIDTH);
    assert_eq!(settings.workspace.session_mask_character, "#");
    assert_eq!(settings.shortcuts.open_settings, "Ctrl+,");
}

#[test]
fn legacy_terminal_option_meta_defaults_disabled_and_round_trips() {
    let json = r#"{
            "version": 12,
            "settings": {
                "terminal": {
                    "scrollback_lines": 4000
                }
            }
        }"#;
    let store: SessionStore =
        serde_json::from_str(json).expect("legacy terminal settings should deserialize");

    assert_eq!(store.version, CURRENT_SCHEMA_VERSION);
    assert!(!store.settings.terminal.option_as_meta);
    let serialized = serde_json::to_value(store).expect("settings should serialize");
    assert_eq!(serialized["settings"]["terminal"]["option_as_meta"], false);
}

#[test]
fn workspace_mask_character_is_one_visible_character() {
    assert_eq!(
        WorkspaceSettings::normalized(220, 172, "#").session_mask_character,
        "#"
    );
    assert_eq!(
        WorkspaceSettings::normalized(220, 172, "  $  ").session_mask_character,
        "$"
    );
    assert_eq!(
        WorkspaceSettings::normalized(220, 172, "").session_mask_character,
        DEFAULT_SESSION_MASK_CHARACTER
    );
    assert_eq!(
        WorkspaceSettings::normalized(220, 172, "**").session_mask_character,
        DEFAULT_SESSION_MASK_CHARACTER
    );
}

#[test]
fn shortcut_settings_validate_modifiers_and_conflicts() {
    let defaults = ShortcutSettings::default();
    assert!(defaults.validate().is_ok());
    assert_eq!(
        defaults.toggle_sidebar,
        if cfg!(target_os = "macos") {
            "Cmd+S"
        } else {
            "Ctrl+S"
        }
    );
    assert_eq!(
        defaults.copy_selection,
        if cfg!(target_os = "macos") {
            "Cmd+C"
        } else {
            "Ctrl+Shift+C"
        }
    );
    assert_eq!(
        defaults.paste,
        if cfg!(target_os = "macos") {
            "Cmd+V"
        } else {
            "Ctrl+Shift+V"
        }
    );
    let conflicting = ShortcutSettings {
        open_settings: "Ctrl+,".into(),
        toggle_sidebar: "Ctrl+,".into(),
        copy_selection: "Ctrl+Shift+C".into(),
        paste: "Ctrl+Shift+V".into(),
    };
    assert!(conflicting.validate().is_err());

    assert_eq!(
        ShortcutSettings::normalized("B", "Ctrl+Shift+B", "Ctrl+Shift+C", "Ctrl+Shift+V"),
        ShortcutSettings::default()
    );
}

#[test]
fn previous_sidebar_default_migrates_without_overwriting_custom_values() {
    let previous = previous_toggle_sidebar_shortcut();
    let copy = default_copy_selection_shortcut();
    let paste = default_paste_shortcut();
    let json = format!(
        r#"{{
                "version": 5,
                "settings": {{
                    "shortcuts": {{
                        "open_settings": "Ctrl+,",
                        "toggle_sidebar": "{previous}",
                        "copy_selection": "{copy}",
                        "paste": "{paste}"
                    }}
                }}
            }}"#
    );
    let migrated: SessionStore =
        serde_json::from_str(&json).expect("previous shortcuts should migrate");
    assert_eq!(
        migrated.settings.shortcuts.toggle_sidebar,
        default_toggle_sidebar_shortcut()
    );
    assert_eq!(migrated.settings.shortcuts.copy_selection, copy);
    assert_eq!(migrated.settings.shortcuts.paste, paste);

    let custom = json.replace(&previous, "Alt+S");
    let migrated: SessionStore =
        serde_json::from_str(&custom).expect("custom shortcut should load");
    assert_eq!(migrated.settings.shortcuts.toggle_sidebar, "Alt+S");
}

#[test]
fn previous_workspace_width_migrates_without_overwriting_custom_values() {
    let json = r#"{
            "version": 6,
            "settings": {
                "workspace": {
                    "sidebar_width": 260,
                    "tab_width": 172
                }
            }
        }"#;
    let migrated: SessionStore =
        serde_json::from_str(json).expect("previous workspace width should migrate");
    assert_eq!(migrated.version, CURRENT_SCHEMA_VERSION);
    assert_eq!(
        migrated.settings.workspace.sidebar_width,
        DEFAULT_SIDEBAR_WIDTH
    );

    let custom = json.replace("\"sidebar_width\": 260", "\"sidebar_width\": 300");
    let migrated: SessionStore =
        serde_json::from_str(&custom).expect("custom workspace width should load");
    assert_eq!(migrated.settings.workspace.sidebar_width, 300);
}

#[test]
fn terminal_shell_cache_is_normalized_and_only_adds_discoveries() {
    let mut settings = TerminalSettings::normalized(
        2_000,
        120,
        36,
        " zsh ",
        &["zsh".into(), "ZSH".into(), "bad\nshell".into()],
        true,
    );
    assert_eq!(settings.local_shell, "zsh");
    assert_eq!(settings.known_shells, [SYSTEM_DEFAULT_SHELL, "zsh"]);
    assert!(settings.option_as_meta);
    assert!(!settings.merge_known_shells(["ZSH".into()]));
    assert!(settings.merge_known_shells(["bash".into()]));
    assert_eq!(settings.known_shells, [SYSTEM_DEFAULT_SHELL, "zsh", "bash"]);
}

#[test]
fn profile_json_contains_no_secret_fields() {
    let mut profile = SessionProfile::new("demo", "host.example", "alice");
    profile.credential_storage = Some(CredentialStorage::EncryptedVault);
    let value = serde_json::to_value(profile).expect("profile should serialize");
    let object = value.as_object().expect("profile should be an object");

    assert!(!object.contains_key("password"));
    assert!(!object.contains_key("passphrase"));
    assert!(!object.contains_key("secret"));
    assert_eq!(
        object.get("credential_storage"),
        Some(&serde_json::json!("encrypted-vault"))
    );
    assert!(!object.contains_key("credential_stored"));
}

#[test]
fn group_names_are_trimmed_and_bounded() {
    assert_eq!(normalize_group_name("  Production  "), "Production");
    assert_eq!(normalize_group_name(" ungrouped "), "");
    let mut profile = SessionProfile::new("demo", "host.example", "alice");
    profile.group_name = "x".repeat(65);
    assert!(profile.validate().is_err());
}

#[test]
fn legacy_profile_groups_are_promoted_to_persistent_groups() {
    let id = Uuid::new_v4();
    let json = format!(
        r#"{{"version":9,"sessions":[{{"id":"{id}","name":"legacy","group_name":" Production ","host":"host.example","port":22,"username":"alice","auth":"Password"}}]}}"#
    );

    let store: SessionStore =
        serde_json::from_str(&json).expect("legacy profile should deserialize");

    assert_eq!(store.version, CURRENT_SCHEMA_VERSION);
    assert_eq!(store.groups, ["Production"]);
    assert_eq!(store.sessions[0].group_name, "Production");
}

#[test]
fn group_operations_preserve_profiles_without_implicit_deletion() {
    let mut store = SessionStore::default();
    assert!(
        store
            .add_group("Production")
            .expect("group should be added")
    );
    assert!(!store.add_group(" Production ").expect("duplicate is valid"));
    let mut profile = SessionProfile::new("demo", "host.example", "alice");
    profile.group_name = "Production".into();
    store.upsert(profile.clone());

    assert!(
        store
            .rename_group("Production", "Critical")
            .expect("group should be renamed")
    );
    assert_eq!(store.groups, ["Critical"]);
    assert_eq!(store.sessions[0].group_name, "Critical");
    assert!(store.remove_group("Critical"));
    assert!(store.groups.is_empty());
    assert_eq!(
        store.sessions,
        [SessionProfile {
            group_name: String::new(),
            ..profile
        }]
    );
    assert!(store.add_group("Ungrouped").is_err());
}

#[test]
fn private_key_profiles_require_a_path_and_never_store_password_references() {
    let mut profile = SessionProfile::new("demo", "host.example", "alice");
    profile.auth = AuthMethod::PrivateKey {
        path: PathBuf::new(),
    };
    assert!(profile.validate().is_err());

    profile.auth = AuthMethod::PrivateKey {
        path: PathBuf::from("/tmp/id_ed25519"),
    };
    profile.credential_storage = Some(CredentialStorage::SystemKeyring);
    assert!(profile.validate().is_err());

    profile.credential_storage = None;
    assert!(profile.validate().is_ok());
}

#[test]
fn persisted_private_key_profile_rejects_a_password_reference() {
    let id = Uuid::new_v4();
    let json = format!(
        r#"{{"id":"{id}","name":"demo","host":"host.example","port":22,"username":"alice","auth":{{"PrivateKey":{{"path":"/tmp/id_ed25519"}}}},"credential_storage":"system-keyring"}}"#
    );

    assert!(serde_json::from_str::<SessionProfile>(&json).is_err());
}
