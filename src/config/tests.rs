use std::fs;
use std::path::PathBuf;

use uuid::Uuid;

use super::*;

fn ssh(profile: &SessionProfile) -> &SshConfig {
    profile.ssh().expect("test profile should use SSH")
}

fn ssh_mut(profile: &mut SessionProfile) -> &mut SshConfig {
    profile.ssh_mut().expect("test profile should use SSH")
}

fn normalized_workspace(
    session_mask_character: &str,
    collapsed_group_label_chars: i32,
) -> WorkspaceSettings {
    WorkspaceSettings::normalized(WorkspaceSettingsInput {
        sidebar_width: 220,
        tab_width: 172,
        session_mask_character,
        collapsed_group_label_chars,
    })
}

#[test]
fn profile_validation_rejects_missing_host() {
    let profile = SessionProfile::new("demo", "", "alice");
    assert!(profile.validate().is_err());
}

#[test]
fn profile_validation_enforces_shared_text_limits() {
    let mut profile = SessionProfile::new(
        "x".repeat(MAX_SESSION_NAME_CHARS + 1),
        "host.example",
        "alice",
    );
    assert!(profile.validate().is_err());

    profile.name = "demo".to_owned();
    ssh_mut(&mut profile).host = "bad\nhost".to_owned();
    assert!(profile.validate().is_err());

    ssh_mut(&mut profile).host = "host.example".to_owned();
    ssh_mut(&mut profile).username = "x".repeat(MAX_USERNAME_CHARS + 1);
    assert!(profile.validate().is_err());

    ssh_mut(&mut profile).username = "alice".to_owned();
    ssh_mut(&mut profile).auth = AuthMethod::PrivateKey {
        path: PathBuf::from("x".repeat(MAX_PRIVATE_KEY_PATH_CHARS + 1)),
    };
    assert!(profile.validate().is_err());

    ssh_mut(&mut profile).auth = AuthMethod::Password;
    ssh_mut(&mut profile).sftp_remote_path = "bad\npath".to_owned();
    assert!(profile.validate().is_err());

    ssh_mut(&mut profile).sftp_remote_path = "~".to_owned();
    ssh_mut(&mut profile).sftp_local_path = "x".repeat(4_097);
    assert!(profile.validate().is_err());
}

#[test]
fn profile_deserialization_rejects_invalid_normal_fields() {
    let profile = SessionProfile::new("bad\nname", "host.example", "alice");
    let json = serde_json::to_string(&profile).expect("invalid fixture should still serialize");

    assert!(serde_json::from_str::<SessionProfile>(&json).is_err());
}

#[test]
fn store_validation_enforces_group_and_profile_count_limits() {
    let groups = SessionStore {
        groups: (0..=MAX_GROUPS)
            .map(|index| format!("group-{index}"))
            .collect(),
        ..SessionStore::default()
    };
    assert!(groups.validate().is_err());

    let template = SessionProfile::new("demo", "host.example", "alice");
    let profiles = SessionStore {
        sessions: (0..=MAX_SESSION_PROFILES)
            .map(|_| SessionProfile {
                id: Uuid::new_v4(),
                ..template.clone()
            })
            .collect(),
        ..SessionStore::default()
    };
    assert!(profiles.validate().is_err());
}

#[test]
fn config_load_rejects_oversized_files_before_deserialization() {
    let path = std::env::temp_dir().join(format!("ax-ssh-oversized-{}.json", Uuid::new_v4()));
    fs::write(&path, vec![b' '; MAX_CONFIG_FILE_BYTES + 1])
        .expect("oversized config fixture should be written");

    let error = ConfigStore::new(&path)
        .load()
        .expect_err("oversized config should be rejected");
    assert!(error.to_string().contains("exceeds"));
    let _ = fs::remove_file(path);
}

#[test]
fn workspace_snapshot_round_trips_separately_from_session_store() {
    let path = std::env::temp_dir().join(format!("ax-ssh-workspace-{}.json", Uuid::new_v4()));
    let store = ConfigStore::new(&path);
    let tab_id = Uuid::new_v4();
    let snapshot = WorkspaceSnapshot {
        version: WORKSPACE_SNAPSHOT_VERSION,
        tabs: vec![WorkspaceTabSnapshot {
            id: tab_id,
            title: "Shell".to_owned(),
            kind: "terminal".to_owned(),
            terminal_text: "hello\n".to_owned(),
            sftp_remote_path: "/tmp".to_owned(),
            sftp_local_path: "/tmp/local".to_owned(),
            ..WorkspaceTabSnapshot::default()
        }],
        active_tab_id: Some(tab_id),
        windows: vec![WorkspaceWindowSnapshot {
            id: Uuid::nil(),
            tab_ids: vec![tab_id],
            active_tab_id: Some(tab_id),
            focused_tab_id: Some(tab_id),
            panes: vec![PaneNodeSnapshot::Leaf(tab_id)],
        }],
    };
    store
        .save_workspace(&snapshot)
        .expect("workspace save should succeed");
    assert_eq!(
        store
            .load_workspace()
            .expect("workspace load should succeed"),
        Some(snapshot)
    );
    assert_ne!(store.path(), &store.workspace_path());
    assert!(store.workspace_path().exists());
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(store.workspace_path());
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
    ssh_mut(&mut profile).credential_storage = Some(CredentialStorage::SystemKeyring);
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
    assert_eq!(ssh(&store.sessions[0]).credential_storage, None);
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
        ssh(&store.sessions[0]).credential_storage,
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
    ssh_mut(&mut profile).credential_storage = Some(CredentialStorage::SystemKeyring);
    store.upsert(profile);

    store.settings.credential_storage = CredentialStorage::EncryptedVault;

    assert_eq!(
        ssh(&store.sessions[0]).credential_storage,
        Some(CredentialStorage::SystemKeyring)
    );
    assert_eq!(
        store.settings.credential_storage,
        CredentialStorage::EncryptedVault
    );
}

#[test]
fn appearance_settings_normalize_application_and_terminal_fonts() {
    assert_eq!(
        AppearanceSettings::normalized(AppearanceSettingsInput {
            renderer_preference: "gpu",
            application_font_family: "  JetBrains Mono  ",
            terminal_font_family: "  Menlo  ",
            terminal_font_size: 18,
            terminal_line_height_percent: 135,
            color_scheme: "light",
            text_brightness: 1.13,
            semantic_highlighting: true,
            terminal_compact_rendering: false,
            terminal_row_render_cache: true,
            terminal_partition_strategy: "tile-16",
            terminal_cursor_blink: false,
            focused_terminal_refresh_fps: 60,
            unfocused_terminal_refresh_fps: 4,
            terminal_semantic_colors: TerminalSemanticColorsInput {
                link: "#17a8cd",
                success: " #21cd8b ",
                info: "#3b8eea",
                warning: "#f5f543",
                error: "#f14c4c",
            },
            bright_bold_text: false,
            right_click_copy_or_paste: true,
            copy_selection_on_select: true,
            terminal_mouse_local_selection_priority: true,
        }),
        AppearanceSettings {
            renderer_preference: RendererPreference::Gpu,
            application_font_family: "JetBrains Mono".into(),
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
            terminal_text_brightness_percent: 115,
            terminal_semantic_highlighting: true,
            terminal_compact_rendering: false,
            terminal_row_render_cache: true,
            terminal_partition_strategy: TerminalPartitionStrategy::Tile16,
            terminal_cursor_blink: false,
            focused_terminal_refresh_fps: 60,
            unfocused_terminal_refresh_fps: 4,
            terminal_semantic_colors: TerminalSemanticColors {
                link: "#17A8CD".into(),
                success: "#21CD8B".into(),
                info: "#3B8EEA".into(),
                warning: "#F5F543".into(),
                error: "#F14C4C".into(),
            },
            bright_bold_text: false,
            right_click_copy_or_paste: true,
            copy_selection_on_select: true,
            terminal_mouse_local_selection_priority: true,
        }
    );
    assert_eq!(
        AppearanceSettings::normalized(AppearanceSettingsInput {
            renderer_preference: "unsupported",
            application_font_family: "",
            terminal_font_family: "",
            terminal_font_size: 100,
            terminal_line_height_percent: 1_000,
            color_scheme: "unknown",
            text_brightness: 1_000.0,
            semantic_highlighting: false,
            terminal_compact_rendering: true,
            terminal_row_render_cache: false,
            terminal_partition_strategy: "unsupported",
            terminal_cursor_blink: true,
            focused_terminal_refresh_fps: 60,
            unfocused_terminal_refresh_fps: 4,
            terminal_semantic_colors: TerminalSemanticColorsInput {
                link: "blue",
                success: "#FFF",
                info: "#12345678",
                warning: "#12XZ56",
                error: "",
            },
            bright_bold_text: true,
            right_click_copy_or_paste: false,
            copy_selection_on_select: false,
            terminal_mouse_local_selection_priority: false,
        }),
        AppearanceSettings {
            renderer_preference: RendererPreference::Automatic,
            application_font_family: DEFAULT_APPLICATION_FONT_FAMILY.into(),
            terminal_font_family: DEFAULT_TERMINAL_FONT_FAMILY.into(),
            terminal_font_size: MAX_TERMINAL_FONT_SIZE,
            terminal_line_height_percent: MAX_TERMINAL_LINE_HEIGHT,
            terminal_color_scheme: TerminalColorScheme::Dark,
            theme: ThemeSettings::default(),
            terminal_text_brightness_percent: MAX_TERMINAL_TEXT_BRIGHTNESS_PERCENT,
            terminal_semantic_highlighting: false,
            terminal_compact_rendering: true,
            terminal_row_render_cache: false,
            terminal_partition_strategy: TerminalPartitionStrategy::Tile8,
            terminal_cursor_blink: true,
            focused_terminal_refresh_fps: 60,
            unfocused_terminal_refresh_fps: 4,
            terminal_semantic_colors: TerminalSemanticColors::default(),
            bright_bold_text: true,
            right_click_copy_or_paste: false,
            copy_selection_on_select: false,
            terminal_mouse_local_selection_priority: false,
        }
    );
}

#[test]
fn terminal_refresh_rates_are_clamped_to_supported_fps_range() {
    let known_shells = [SYSTEM_DEFAULT_SHELL.to_owned()];
    let settings = AppSettings::normalized(AppSettingsInput {
        appearance: AppearanceSettingsInput {
            renderer_preference: "automatic",
            application_font_family: "",
            terminal_font_family: "",
            terminal_font_size: 14,
            terminal_line_height_percent: 120,
            color_scheme: "dark",
            text_brightness: 1.0,
            semantic_highlighting: false,
            terminal_compact_rendering: true,
            terminal_row_render_cache: false,
            terminal_partition_strategy: "tile-8",
            terminal_cursor_blink: true,
            focused_terminal_refresh_fps: -10,
            unfocused_terminal_refresh_fps: 999,
            terminal_semantic_colors: TerminalSemanticColorsInput {
                link: "",
                success: "",
                info: "",
                warning: "",
                error: "",
            },
            bright_bold_text: true,
            right_click_copy_or_paste: false,
            copy_selection_on_select: false,
            terminal_mouse_local_selection_priority: true,
        },
        terminal: TerminalSettingsInput {
            scrollback_lines: 2_000,
            default_columns: 120,
            default_rows: 36,
            local_shell: SYSTEM_DEFAULT_SHELL,
            known_shells: &known_shells,
            option_as_meta: false,
        },
        workspace: WorkspaceSettingsInput {
            sidebar_width: 220,
            tab_width: 172,
            session_mask_character: "*",
            collapsed_group_label_chars: 2,
        },
        shortcuts: ShortcutSettings::default(),
        credential_storage: "system-keyring",
        ui_language: "english",
    });

    assert_eq!(
        settings.appearance.focused_terminal_refresh_fps,
        MIN_TERMINAL_REFRESH_FPS
    );
    assert_eq!(
        settings.appearance.unfocused_terminal_refresh_fps,
        MAX_TERMINAL_REFRESH_FPS
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
    assert_eq!(
        store.settings.appearance.application_font_family,
        DEFAULT_APPLICATION_FONT_FAMILY
    );
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
        store.settings.appearance.terminal_text_brightness_percent,
        DEFAULT_TERMINAL_TEXT_BRIGHTNESS_PERCENT
    );
    assert!(!store.settings.appearance.terminal_semantic_highlighting);
    assert!(store.settings.appearance.terminal_compact_rendering);
    assert!(!store.settings.appearance.terminal_row_render_cache);
    assert!(store.settings.appearance.terminal_cursor_blink);
    assert!(store.settings.appearance.bright_bold_text);
    assert!(!store.settings.appearance.right_click_copy_or_paste);
    assert!(!store.settings.appearance.copy_selection_on_select);
    assert_eq!(
        store.settings.appearance.terminal_semantic_colors,
        TerminalSemanticColors::default()
    );
    assert_eq!(store.settings.terminal, TerminalSettings::default());
    assert_eq!(store.settings.shortcuts, ShortcutSettings::default());
    let serialized = serde_json::to_value(store).expect("settings should serialize");
    assert!(serialized.get("settings").is_some());
    assert!(serialized.get("appearance").is_none());
    assert_eq!(
        serialized["settings"]["appearance"]["application_font_family"],
        DEFAULT_APPLICATION_FONT_FAMILY
    );
    assert_eq!(
        serialized["settings"]["appearance"]["terminal_line_height_percent"],
        DEFAULT_TERMINAL_LINE_HEIGHT
    );
}

#[test]
fn version_twenty_one_contrast_migrates_to_default_text_brightness() {
    let json = r#"{
        "version": 21,
        "settings": {
            "appearance": {
                "terminal_minimum_contrast_ratio_tenths": 210,
                "terminal_semantic_highlighting": true
            }
        }
    }"#;

    let store: SessionStore =
        serde_json::from_str(json).expect("version twenty-one should migrate");

    assert_eq!(store.version, CURRENT_SCHEMA_VERSION);
    assert_eq!(
        store.settings.appearance.terminal_text_brightness_percent,
        DEFAULT_TERMINAL_TEXT_BRIGHTNESS_PERCENT
    );
    assert!(!store.settings.appearance.terminal_semantic_highlighting);
    let serialized = serde_json::to_value(store).expect("migrated settings should serialize");
    let appearance = &serialized["settings"]["appearance"];
    assert_eq!(
        appearance["terminal_text_brightness_percent"],
        DEFAULT_TERMINAL_TEXT_BRIGHTNESS_PERCENT
    );
    assert!(
        appearance
            .get("terminal_minimum_contrast_ratio_tenths")
            .is_none()
    );
}

#[test]
fn terminal_render_preferences_round_trip() {
    let mut store = SessionStore::default();
    store.settings.appearance.terminal_text_brightness_percent = 115;
    store.settings.appearance.terminal_semantic_highlighting = true;
    store.settings.appearance.terminal_compact_rendering = false;
    store.settings.appearance.terminal_row_render_cache = true;
    store.settings.appearance.terminal_cursor_blink = false;

    let encoded = serde_json::to_string(&store).expect("settings should serialize");
    let decoded: SessionStore =
        serde_json::from_str(&encoded).expect("settings should deserialize");

    assert_eq!(
        decoded.settings.appearance.terminal_text_brightness_percent,
        115
    );
    assert!(decoded.settings.appearance.terminal_semantic_highlighting);
    assert!(!decoded.settings.appearance.terminal_compact_rendering);
    assert!(decoded.settings.appearance.terminal_row_render_cache);
    assert!(!decoded.settings.appearance.terminal_cursor_blink);
}

#[test]
fn terminal_semantic_colors_round_trip_and_reject_invalid_values() {
    let json = r##"{
        "version": 19,
        "settings": {
            "appearance": {
                "terminal_semantic_colors": {
                    "link": "#17a8cd",
                    "success": "#21cd8b",
                    "info": "#12345678",
                    "warning": "#F5F543",
                    "error": "red"
                }
            }
        }
    }"##;

    let store: SessionStore = serde_json::from_str(json).expect("settings should deserialize");

    assert_eq!(store.version, CURRENT_SCHEMA_VERSION);
    assert_eq!(
        store.settings.appearance.terminal_semantic_colors,
        TerminalSemanticColors {
            link: "#17A8CD".into(),
            success: "#21CD8B".into(),
            info: String::new(),
            warning: "#F5F543".into(),
            error: String::new(),
        }
    );
    let encoded = serde_json::to_value(store).expect("settings should serialize");
    assert_eq!(
        encoded["settings"]["appearance"]["terminal_semantic_colors"]["link"],
        "#17A8CD"
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
    for (value, expected) in [
        ("arctic-dark", ThemePaletteKind::Arctic),
        ("Tokyo Dark", ThemePaletteKind::Tokyo),
        ("ember", ThemePaletteKind::Ember),
        ("FOREST", ThemePaletteKind::Forest),
    ] {
        assert_eq!(ThemePaletteKind::from_setting(value), expected);
    }
}

#[test]
fn fixed_palette_terminal_schemes_follow_dark_mode() {
    for (palette, expected) in [
        ("solarized", TerminalColorScheme::SolarizedDark),
        ("arctic", TerminalColorScheme::ArcticDark),
        ("tokyo", TerminalColorScheme::TokyoDark),
        ("ember", TerminalColorScheme::EmberDark),
        ("forest", TerminalColorScheme::ForestDark),
    ] {
        let dark = ThemeSettings::normalized(
            "dark",
            palette,
            ThemePalette::axssh_light(),
            ThemePalette::axssh_dark(),
        );
        let light = ThemeSettings::normalized(
            "light",
            palette,
            ThemePalette::axssh_light(),
            ThemePalette::axssh_dark(),
        );
        let system = ThemeSettings::normalized(
            "system",
            palette,
            ThemePalette::axssh_light(),
            ThemePalette::axssh_dark(),
        );

        assert_eq!(dark.terminal_color_scheme(), expected);
        assert_eq!(light.terminal_color_scheme(), TerminalColorScheme::Light);
        assert_eq!(system.terminal_color_scheme(), expected);
    }
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
    let palette = ThemePalette::normalized(ThemePalette {
        background: " #abc ".into(),
        panel: "#abcd".into(),
        panel_alt: "#1a2b3c".into(),
        border: "not-a-color".into(),
        text: "#12345678".into(),
        muted: "#0A0B0C".into(),
        accent: "#112233".into(),
        success: "#445566".into(),
        danger: "#778899".into(),
        overlay: "#AABBCCDD".into(),
        terminal_foreground: "#010203".into(),
        terminal_background: "#040506".into(),
        terminal_selection: "#070809".into(),
    });

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
        ThemePalette::normalized(ThemePalette {
            background: "#102030".into(),
            panel: "#203040".into(),
            panel_alt: "#304050".into(),
            border: "#405060".into(),
            text: "#506070".into(),
            muted: "#607080".into(),
            accent: "#708090".into(),
            success: "#8090A0".into(),
            danger: "#90A0B0".into(),
            overlay: "#00000080".into(),
            terminal_foreground: "#A0B0C0".into(),
            terminal_background: "#0A0B0C".into(),
            terminal_selection: "#102938".into(),
        }),
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
        ThemePalette::arctic_light(),
        ThemePalette::arctic_dark(),
        ThemePalette::tokyo_light(),
        ThemePalette::tokyo_dark(),
        ThemePalette::ember_light(),
        ThemePalette::ember_dark(),
        ThemePalette::forest_light(),
        ThemePalette::forest_dark(),
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
fn copy_selection_on_select_defaults_disabled_and_round_trips() {
    let legacy: SessionStore = serde_json::from_str(
        r#"{"version":17,"settings":{"appearance":{"right_click_copy_or_paste":true}}}"#,
    )
    .expect("previous settings should deserialize");
    assert_eq!(legacy.version, CURRENT_SCHEMA_VERSION);
    assert!(legacy.settings.appearance.right_click_copy_or_paste);
    assert!(!legacy.settings.appearance.copy_selection_on_select);
    assert!(
        legacy
            .settings
            .appearance
            .terminal_mouse_local_selection_priority
    );

    let mut current = SessionStore::default();
    current.settings.appearance.copy_selection_on_select = true;
    current
        .settings
        .appearance
        .terminal_mouse_local_selection_priority = false;
    let encoded = serde_json::to_string(&current).expect("settings should serialize");
    let decoded: SessionStore =
        serde_json::from_str(&encoded).expect("settings should deserialize");
    assert!(decoded.settings.appearance.copy_selection_on_select);
    assert!(
        !decoded
            .settings
            .appearance
            .terminal_mouse_local_selection_priority
    );
}

#[test]
fn app_settings_clamp_all_persisted_dimensions() {
    let known_shells = [SYSTEM_DEFAULT_SHELL.into(), "zsh".into()];
    let settings = AppSettings::normalized(AppSettingsInput {
        appearance: AppearanceSettingsInput {
            renderer_preference: "software",
            application_font_family: "Maple Mono NF CN",
            terminal_font_family: "",
            terminal_font_size: 100,
            terminal_line_height_percent: -1,
            color_scheme: "solarized-dark",
            text_brightness: -1.0,
            semantic_highlighting: true,
            terminal_compact_rendering: false,
            terminal_row_render_cache: true,
            terminal_partition_strategy: "tile-16",
            terminal_cursor_blink: false,
            focused_terminal_refresh_fps: 60,
            unfocused_terminal_refresh_fps: 4,
            terminal_semantic_colors: TerminalSemanticColorsInput {
                link: "#17A8CD",
                success: "#21CD8B",
                info: "#3B8EEA",
                warning: "#F5F543",
                error: "#F14C4C",
            },
            bright_bold_text: false,
            right_click_copy_or_paste: true,
            copy_selection_on_select: true,
            terminal_mouse_local_selection_priority: true,
        },
        terminal: TerminalSettingsInput {
            scrollback_lines: -1,
            default_columns: 1,
            default_rows: 1_000,
            local_shell: "zsh",
            known_shells: &known_shells,
            option_as_meta: true,
        },
        workspace: WorkspaceSettingsInput {
            sidebar_width: 20,
            tab_width: 9_000,
            session_mask_character: "  #  ",
            collapsed_group_label_chars: 9,
        },
        shortcuts: ShortcutSettings {
            open_settings: "Ctrl+,".into(),
            new_session: "Ctrl+N".into(),
            import_sessions: "Ctrl+Shift+I".into(),
            export_selected: "Ctrl+Shift+E".into(),
            toggle_sidebar: "Ctrl+Shift+B".into(),
            copy_selection: "Ctrl+Shift+C".into(),
            paste: "Ctrl+Shift+V".into(),
            open_sftp: "Ctrl+Shift+F".into(),
        },
        credential_storage: "encrypted-vault",
        ui_language: "simplified-chinese",
    });

    assert_eq!(settings.ui_language, UiLanguage::SimplifiedChinese);
    assert_eq!(
        settings.appearance.renderer_preference,
        RendererPreference::Software
    );

    assert_eq!(
        settings.appearance.application_font_family,
        "Maple Mono NF CN"
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
        settings.appearance.terminal_text_brightness_percent,
        MIN_TERMINAL_TEXT_BRIGHTNESS_PERCENT
    );
    assert!(settings.appearance.terminal_semantic_highlighting);
    assert!(!settings.appearance.terminal_compact_rendering);
    assert!(settings.appearance.terminal_row_render_cache);
    assert!(!settings.appearance.bright_bold_text);
    assert!(settings.appearance.right_click_copy_or_paste);
    assert!(settings.appearance.copy_selection_on_select);
    assert_eq!(settings.appearance.terminal_semantic_colors.link, "#17A8CD");
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
    assert_eq!(
        settings.workspace.collapsed_group_label_chars,
        MAX_COLLAPSED_GROUP_LABEL_CHARS
    );
    assert_eq!(settings.shortcuts.open_settings, "Ctrl+,");
}

#[test]
fn ui_language_defaults_to_system_and_serializes_stable_values() {
    let legacy: SessionStore = serde_json::from_str(r#"{"version":20,"settings":{}}"#)
        .expect("version twenty settings should migrate");
    assert_eq!(legacy.version, CURRENT_SCHEMA_VERSION);
    assert_eq!(legacy.settings.ui_language, UiLanguage::System);

    let invalid: AppSettings = serde_json::from_str(r#"{"ui_language":"unsupported"}"#)
        .expect("unknown language should normalize");
    assert_eq!(invalid.ui_language, UiLanguage::System);

    assert_eq!(UiLanguage::from_selector_index(0), UiLanguage::System);
    assert_eq!(UiLanguage::from_selector_index(1), UiLanguage::English);
    assert_eq!(
        UiLanguage::from_selector_index(2),
        UiLanguage::SimplifiedChinese
    );
    assert_eq!(UiLanguage::from_selector_index(99), UiLanguage::System);
    assert_eq!(UiLanguage::SimplifiedChinese.selector_index(), 2);

    assert_eq!(
        serde_json::to_value(UiLanguage::System).expect("system language should serialize"),
        "system"
    );
    assert_eq!(
        serde_json::to_value(UiLanguage::SimplifiedChinese)
            .expect("Chinese language should serialize"),
        "simplified-chinese"
    );
}

#[test]
fn renderer_preference_defaults_and_serializes_stable_values() {
    let legacy: SessionStore = serde_json::from_str(r#"{"version":23,"settings":{}}"#)
        .expect("version twenty-three settings should migrate");
    assert_eq!(legacy.version, CURRENT_SCHEMA_VERSION);
    assert_eq!(
        legacy.settings.appearance.renderer_preference,
        RendererPreference::Automatic
    );

    let invalid: AppearanceSettings =
        serde_json::from_str(r#"{"renderer_preference":"unsupported"}"#)
            .expect("unknown renderer preference should normalize");
    assert_eq!(invalid.renderer_preference, RendererPreference::Automatic);
    assert_eq!(
        serde_json::to_value(RendererPreference::Automatic)
            .expect("automatic renderer preference should serialize"),
        "automatic"
    );
    assert_eq!(
        serde_json::to_value(RendererPreference::Gpu)
            .expect("GPU renderer preference should serialize"),
        "gpu"
    );
    assert_eq!(
        serde_json::to_value(RendererPreference::Software)
            .expect("software renderer preference should serialize"),
        "software"
    );
}

#[test]
fn terminal_partition_strategy_defaults_and_serializes_stable_values() {
    assert_eq!(
        TerminalPartitionStrategy::default(),
        TerminalPartitionStrategy::Tile8
    );
    assert_eq!(
        TerminalPartitionStrategy::from_setting("rows"),
        TerminalPartitionStrategy::Rows
    );
    assert_eq!(
        TerminalPartitionStrategy::from_setting("tile-16"),
        TerminalPartitionStrategy::Tile16
    );
    assert_eq!(
        TerminalPartitionStrategy::from_setting("invalid"),
        TerminalPartitionStrategy::Tile8
    );
    assert_eq!(TerminalPartitionStrategy::Rows.as_setting(), "rows");
    assert_eq!(TerminalPartitionStrategy::Tile8.as_setting(), "tile-8");
    assert_eq!(TerminalPartitionStrategy::Tile16.as_setting(), "tile-16");
    assert_eq!(
        serde_json::to_value(TerminalPartitionStrategy::Tile16).unwrap(),
        "tile-16"
    );
    let legacy: AppearanceSettings =
        serde_json::from_str("{}").expect("missing strategy should default");
    assert_eq!(
        legacy.terminal_partition_strategy,
        TerminalPartitionStrategy::Tile8
    );
}

#[test]
fn ui_language_resolves_only_bundled_system_locales() {
    assert_eq!(UiLanguage::System.resolved_locale(Some("zh-CN")), "zh-CN");
    assert_eq!(UiLanguage::System.resolved_locale(Some("zh_Hans")), "zh-CN");
    assert_eq!(UiLanguage::System.resolved_locale(Some("en-US")), "en");
    assert_eq!(UiLanguage::System.resolved_locale(Some("ja-JP")), "en");
    assert_eq!(UiLanguage::System.resolved_locale(None), "en");
    assert_eq!(UiLanguage::English.resolved_locale(Some("zh-CN")), "en");
    assert_eq!(UiLanguage::SimplifiedChinese.resolved_locale(None), "zh-CN");
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
    assert_eq!(
        store.settings.workspace.collapsed_group_label_chars,
        DEFAULT_COLLAPSED_GROUP_LABEL_CHARS
    );
    let serialized = serde_json::to_value(store).expect("settings should serialize");
    assert_eq!(serialized["settings"]["terminal"]["option_as_meta"], false);
    assert_eq!(
        serialized["settings"]["workspace"]["collapsed_group_label_chars"],
        DEFAULT_COLLAPSED_GROUP_LABEL_CHARS
    );
}

#[test]
fn workspace_mask_character_is_one_visible_character() {
    assert_eq!(normalized_workspace("#", 2).session_mask_character, "#");
    assert_eq!(normalized_workspace("  $  ", 2).session_mask_character, "$");
    assert_eq!(
        normalized_workspace("", 2).session_mask_character,
        DEFAULT_SESSION_MASK_CHARACTER
    );
    assert_eq!(
        normalized_workspace("**", 2).session_mask_character,
        DEFAULT_SESSION_MASK_CHARACTER
    );
}

#[test]
fn collapsed_group_label_chars_support_full_or_clamped_compact_labels() {
    assert_eq!(normalized_workspace("*", 0).collapsed_group_label_chars, 0);
    assert_eq!(normalized_workspace("*", 2).collapsed_group_label_chars, 2);
    assert_eq!(
        normalized_workspace("*", 9).collapsed_group_label_chars,
        MAX_COLLAPSED_GROUP_LABEL_CHARS
    );
}

#[test]
fn shortcut_settings_validate_modifiers_and_conflicts() {
    let defaults = ShortcutSettings::default();
    assert!(defaults.validate().is_ok());
    assert_eq!(
        defaults.new_session,
        if cfg!(target_os = "macos") {
            "Cmd+N"
        } else {
            "Ctrl+N"
        }
    );
    assert_eq!(defaults.open_sftp, "Ctrl+M");
    assert_eq!(
        defaults.import_sessions,
        if cfg!(target_os = "macos") {
            "Cmd+Shift+I"
        } else {
            "Ctrl+Shift+I"
        }
    );
    assert_eq!(
        defaults.export_selected,
        if cfg!(target_os = "macos") {
            "Cmd+Shift+E"
        } else {
            "Ctrl+Shift+E"
        }
    );
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
        new_session: "Ctrl+N".into(),
        import_sessions: "Ctrl+Shift+I".into(),
        export_selected: "Ctrl+Shift+E".into(),
        toggle_sidebar: "Ctrl+,".into(),
        copy_selection: "Ctrl+Shift+C".into(),
        paste: "Ctrl+Shift+V".into(),
        open_sftp: "Ctrl+Shift+F".into(),
    };
    assert!(conflicting.validate().is_err());

    assert_eq!(
        ShortcutSettings::normalized(ShortcutSettings {
            open_settings: "B".into(),
            new_session: "Ctrl+N".into(),
            import_sessions: "Ctrl+Shift+I".into(),
            export_selected: "Ctrl+Shift+E".into(),
            toggle_sidebar: "Ctrl+Shift+B".into(),
            copy_selection: "Ctrl+Shift+C".into(),
            paste: "Ctrl+Shift+V".into(),
            open_sftp: "Ctrl+Shift+F".into(),
        }),
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
    let known_shells = ["zsh".into(), "ZSH".into(), "bad\nshell".into()];
    let mut settings = TerminalSettings::normalized(TerminalSettingsInput {
        scrollback_lines: 2_000,
        default_columns: 120,
        default_rows: 36,
        local_shell: " zsh ",
        known_shells: &known_shells,
        option_as_meta: true,
    });
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
    ssh_mut(&mut profile).credential_storage = Some(CredentialStorage::EncryptedVault);
    let value = serde_json::to_value(profile).expect("profile should serialize");
    let object = value.as_object().expect("profile should be an object");

    assert!(!object.contains_key("password"));
    assert!(!object.contains_key("passphrase"));
    assert!(!object.contains_key("secret"));
    assert_eq!(object["connection"]["protocol"], "ssh");
    assert_eq!(
        object["connection"]["config"]["credential_storage"],
        "encrypted-vault"
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
    ssh_mut(&mut profile).auth = AuthMethod::PrivateKey {
        path: PathBuf::new(),
    };
    assert!(profile.validate().is_err());

    ssh_mut(&mut profile).auth = AuthMethod::PrivateKey {
        path: PathBuf::from("/tmp/id_ed25519"),
    };
    ssh_mut(&mut profile).credential_storage = Some(CredentialStorage::SystemKeyring);
    assert!(profile.validate().is_err());

    ssh_mut(&mut profile).credential_storage = None;
    assert!(profile.validate().is_ok());
}

#[test]
fn ssh_agent_profiles_round_trip_without_runtime_or_credential_state() {
    let mut profile = SessionProfile::new("agent", "host.example", "alice");
    ssh_mut(&mut profile).auth = AuthMethod::SshAgent;
    assert!(profile.validate().is_ok());

    let encoded = serde_json::to_string(&profile).expect("agent profile should serialize");
    assert!(encoded.contains("SshAgent"));
    assert!(!encoded.contains("SSH_AUTH_SOCK"));
    assert!(!encoded.contains("credential_storage"));
    let decoded: SessionProfile =
        serde_json::from_str(&encoded).expect("agent profile should deserialize");
    assert_eq!(decoded, profile);

    ssh_mut(&mut profile).credential_storage = Some(CredentialStorage::SystemKeyring);
    assert!(profile.validate().is_err());
}

#[test]
fn persisted_private_key_profile_rejects_a_password_reference() {
    let id = Uuid::new_v4();
    let json = format!(
        r#"{{"id":"{id}","name":"demo","host":"host.example","port":22,"username":"alice","auth":{{"PrivateKey":{{"path":"/tmp/id_ed25519"}}}},"credential_storage":"system-keyring"}}"#
    );

    assert!(serde_json::from_str::<SessionProfile>(&json).is_err());
}

#[test]
fn legacy_ssh_profile_migrates_to_explicit_protocol_config() {
    let id = Uuid::new_v4();
    let json = format!(
        r#"{{"id":"{id}","name":"legacy","host":"host.example","port":22,"username":"alice","auth":"Password"}}"#
    );
    let profile: SessionProfile =
        serde_json::from_str(&json).expect("legacy SSH profile should migrate");

    assert_eq!(profile.protocol(), SessionProtocol::Ssh);
    assert!(ssh(&profile).x11_forwarding);
    assert_eq!(ssh(&profile).sftp_remote_path, "~");
    assert!(ssh(&profile).sftp_local_path.is_empty());
    let serialized = serde_json::to_value(profile).expect("profile should serialize");
    assert_eq!(serialized["connection"]["protocol"], "ssh");
    assert!(serialized.get("host").is_none());
}

#[test]
fn sftp_default_paths_round_trip_without_secrets() {
    let mut profile = SessionProfile::new("files", "host.example", "alice");
    ssh_mut(&mut profile).sftp_remote_path = "/srv/releases".into();
    ssh_mut(&mut profile).sftp_local_path = "/Users/alice/Downloads".into();

    profile.validate().expect("SFTP paths should be valid");
    let encoded = serde_json::to_string(&profile).expect("profile should serialize");
    assert!(encoded.contains("sftp_remote_path"));
    assert!(encoded.contains("sftp_local_path"));
    assert!(!encoded.contains("password"));
    let decoded: SessionProfile =
        serde_json::from_str(&encoded).expect("profile should deserialize");
    assert_eq!(decoded, profile);
}

#[test]
fn x11_forwarding_defaults_on_and_round_trips_without_becoming_a_secret() {
    let mut profile = SessionProfile::new("x11", "host.example", "alice");
    assert!(ssh(&profile).x11_forwarding);
    let mut legacy_value = serde_json::to_value(&profile).expect("profile should serialize");
    legacy_value["connection"]["config"]
        .as_object_mut()
        .expect("SSH config should serialize as an object")
        .remove("x11_forwarding");
    let legacy_profile: SessionProfile =
        serde_json::from_value(legacy_value).expect("old SSH config should default X11 on");
    assert!(ssh(&legacy_profile).x11_forwarding);
    ssh_mut(&mut profile).x11_forwarding = false;
    let encoded = serde_json::to_string(&profile).expect("SSH profile should serialize");
    assert!(encoded.contains("x11_forwarding"));
    let decoded: SessionProfile =
        serde_json::from_str(&encoded).expect("SSH profile should deserialize");
    assert!(!ssh(&decoded).x11_forwarding);
    assert!(!encoded.contains("MAGIC-COOKIE"));
}

#[test]
fn x11_settings_normalize_and_round_trip_without_secret_material() {
    let settings =
        X11Settings::normalized("MacXServer", "  /Applications/MacXServer.app  ", true, true);
    assert_eq!(settings.provider, X11ServerProvider::MacXServer);
    assert_eq!(settings.app_path, "/Applications/MacXServer.app");
    assert!(settings.launch_on_connect);
    assert!(settings.allow_no_auth);

    let encoded = serde_json::to_string(&settings).expect("X11 settings should serialize");
    assert!(!encoded.contains("MAGIC-COOKIE"));
    let decoded: X11Settings =
        serde_json::from_str(&encoded).expect("X11 settings should deserialize");
    assert_eq!(decoded, settings);

    let defaults: X11Settings = serde_json::from_str("{}").expect("defaults should load");
    assert_eq!(defaults, X11Settings::default());
}

#[test]
fn telnet_and_serial_profiles_round_trip_without_ssh_security_fields() {
    let telnet = SessionProfile::new_telnet("console", "router.example");
    let mut serial = SessionProfile::new_serial("switch", "/dev/cu.usbserial-01");
    if let ConnectionProfile::Serial(config) = &mut serial.connection {
        config.usb_vendor_id = Some(0x0403);
        config.usb_product_id = Some(0x6001);
        config.usb_serial_number = Some("FT123".into());
    }

    for profile in [telnet, serial] {
        profile.validate().expect("profile should be valid");
        let encoded = serde_json::to_string(&profile).expect("profile should serialize");
        assert!(!encoded.contains("credential_storage"));
        assert!(!encoded.contains("host_key_fingerprint"));
        let decoded: SessionProfile =
            serde_json::from_str(&encoded).expect("profile should deserialize");
        assert_eq!(decoded, profile);
    }
}
