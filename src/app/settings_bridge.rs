use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

mod settings_search_catalog;

use settings_search_catalog::{SETTINGS_SEARCH_CATALOG, localized_catalog};

pub(super) fn wire_ui_language_options(ui: &AppWindow) {
    ui.on_ui_language_options(|system_label| {
        ModelRc::new(VecModel::from(
            std::iter::once(system_label)
                .chain(UiLanguage::bundled().map(|language| language.native_name().into()))
                .collect::<Vec<_>>(),
        ))
    });
}

fn settings_search_matches(query: &str, section: &str, title: &str, description: &str) -> bool {
    let query = query.trim();
    let query = query.to_lowercase();
    !query.is_empty()
        && [section, title, description]
            .into_iter()
            .any(|value| value.to_lowercase().contains(&query))
}

fn settings_search_results(query: &str, language: &str) -> Vec<SettingsSearchEntry> {
    let locale =
        UiLanguage::from_setting(language).resolved_locale(sys_locale::get_locale().as_deref());
    let localized = localized_catalog(locale);
    SETTINGS_SEARCH_CATALOG
        .iter()
        .enumerate()
        .filter(|(_, (_, title, _))| cfg!(target_os = "macos") || *title != "Software presentation")
        .filter_map(|(index, (section, title, description))| {
            let (display_section, display_title, display_description) = localized
                .and_then(|catalog| catalog.get(index))
                .copied()
                .unwrap_or((section, title, description));
            settings_search_matches(query, section, title, description)
                .then_some(())
                .or_else(|| {
                    settings_search_matches(
                        query,
                        display_section,
                        display_title,
                        display_description,
                    )
                    .then_some(())
                })?;
            Some(SettingsSearchEntry {
                section: (*section).into(),
                title: display_title.into(),
                description: display_description.into(),
            })
        })
        .collect()
}

pub(super) fn wire_settings(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    font_registry: Arc<Mutex<FontRegistry>>,
    window_router: WindowRouter,
    persistence: Arc<PersistenceCoordinator>,
) {
    wire_ui_language_options(ui);
    ui.on_system_icon_choices(|query| {
        ModelRc::new(VecModel::from(system_icon_choices(query.as_str())))
    });
    ui.on_system_icon_choice(|id| system_icon_choice(id.as_str()));
    let state_for_icon = state.clone();
    let ui_for_icon = ui.as_weak();
    let runtime_for_icon = runtime.clone();
    let persistence_for_icon = persistence.clone();
    let icon_revision = Arc::new(AtomicU64::new(0));
    ui.on_save_local_shell_icon(move |id| {
        let icon = ax_ssh::config::system_icon_id(id.as_str()).map(str::to_owned);
        let revision = icon_revision.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        let latest = icon_revision.clone();
        let state = state_for_icon.clone();
        let state_for_result = state.clone();
        let ui = ui_for_icon.clone();
        let persistence = persistence_for_icon.clone();
        runtime_for_icon.spawn(async move {
            let guard = persistence.gate.lock().await;
            let latest_for_save = latest.clone();
            let result = tokio::task::spawn_blocking(move || {
                save_local_shell_icon(&state, icon, revision, &latest_for_save)
            })
            .await;
            drop(guard);
            dispatch_ui(&ui, move |ui| {
                if latest.load(Ordering::Acquire) != revision {
                    return;
                }
                match result {
                    Ok(Ok(Some(settings))) => {
                        apply_settings_to_open_windows(ui, &settings);
                        refresh_workspace(&ui.as_weak(), &state_for_result);
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => {
                        ui.set_status(format!("Cannot save local shell icon: {error}").into())
                    }
                    Err(error) => {
                        ui.set_status(format!("Local shell icon task failed: {error}").into())
                    }
                }
            });
        });
    });
    ui.on_settings_search_results(|query, language| {
        ModelRc::new(VecModel::from(settings_search_results(
            query.as_str(),
            language.as_str(),
        )))
    });

    let ui_for_language = ui.as_weak();
    let state_for_language = state.clone();
    let runtime_for_language = runtime.clone();
    let persistence_for_language = persistence.clone();
    let language_revision = Arc::new(AtomicU64::new(0));
    ui.on_save_ui_language(move |index| {
        log_ui_action("settings.language.save");
        let language = UiLanguage::from_selector_index(index);
        let revision = language_revision
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let language_revision = language_revision.clone();
        let state = state_for_language.clone();
        let ui = ui_for_language.clone();
        let persistence = persistence_for_language.clone();
        runtime_for_language.spawn(async move {
            let _persistence_guard = persistence.gate.lock().await;
            let revision_for_save = language_revision.clone();
            let save_result = tokio::task::spawn_blocking(move || {
                save_ui_language(&state, language, revision, &revision_for_save)
            })
            .await;
            dispatch_ui(&ui, move |ui| {
                if language_revision.load(Ordering::Acquire) != revision {
                    return;
                }
                match save_result {
                    Ok(Ok(true)) => {
                        if let Err(error) = apply_ui_language_to_open_windows(ui, language) {
                            ui.set_status(
                                format!("Cannot apply interface language: {error}").into(),
                            );
                        } else {
                            ui.set_status("".into());
                        }
                    }
                    Ok(Ok(false)) => {}
                    Ok(Err(error)) => {
                        ui.set_status(format!("Cannot save interface language: {error}").into())
                    }
                    Err(error) => {
                        ui.set_status(format!("Interface language task failed: {error}").into())
                    }
                }
            });
        });
    });

    let ui_for_save = ui.as_weak();
    let font_registry_for_save = font_registry;
    let persistence_for_save = persistence;
    ui.on_save_settings(
        move |application_font_family,
              renderer_preference,
              terminal_drawing_preference,
              software_presentation,
              terminal_font_family,
              font_size,
              line_height_percent,
              terminal_software_block_rows,
              text_brightness,
              bright_bold_text,
              terminal_semantic_highlighting,
              terminal_compact_rendering,
              terminal_row_render_cache,
              terminal_cursor_blink,
              focused_terminal_refresh_fps,
              unfocused_terminal_refresh_fps,
              terminal_semantic_link_color,
              terminal_semantic_success_color,
              terminal_semantic_info_color,
              terminal_semantic_warning_color,
              terminal_semantic_error_color,
              theme_mode,
              theme_palette,
              theme_light_background,
              theme_light_panel,
              theme_light_panel_alt,
              theme_light_border,
              theme_light_text,
              theme_light_muted,
              theme_light_accent,
              theme_light_success,
              theme_light_danger,
              theme_light_overlay,
              theme_light_terminal_foreground,
              theme_light_terminal_background,
              theme_light_terminal_selection,
              theme_dark_background,
              theme_dark_panel,
              theme_dark_panel_alt,
              theme_dark_border,
              theme_dark_text,
              theme_dark_muted,
              theme_dark_accent,
              theme_dark_success,
              theme_dark_danger,
              theme_dark_overlay,
              theme_dark_terminal_foreground,
              theme_dark_terminal_background,
              theme_dark_terminal_selection,
              right_click_copy_or_paste,
              copy_selection_on_select,
              terminal_mouse_local_selection_priority,
              option_as_meta,
              alternate_scroll_to_home,
              osc52_clipboard,
              x11_server_provider,
              x11_server_app_path,
              x11_launch_on_connect,
              x11_allow_no_auth,
              local_shell,
              scrollback_lines,
              default_columns,
              default_rows,
              sidebar_width,
              tab_width,
              session_mask_character,
              collapsed_group_label_chars,
              open_settings_shortcut,
              new_session_shortcut,
              import_sessions_shortcut,
              export_selected_shortcut,
              toggle_sidebar_shortcut,
              copy_selection_shortcut,
              paste_shortcut,
              open_sftp_shortcut,
              credential_storage,
              sftp_filter_enabled,
              sftp_filter_use_platform_defaults,
              sftp_filter_custom_patterns,
              sftp_per_tab_transfers,
              sftp_global_uploads,
              sftp_rate_limit_mib_per_second,
              sftp_local_open_max_file_mib,
              sftp_remote_edit_max_file_mib,
              settings_tab_id,
              close_after_save| {
            let is_preview = !close_after_save && settings_tab_id.is_empty();
            log_ui_action(if is_preview {
                "settings.preview"
            } else {
                "settings.save"
            });
            if !close_after_save && !is_preview {
                set_status(&ui_for_save, "Invalid Settings preview request");
                return;
            }
            let close_tab_id = if close_after_save {
                let Some(id) = parse_uuid(settings_tab_id.as_str(), "settings tab", &ui_for_save)
                else {
                    return;
                };
                Some(id)
            } else {
                None
            };
            let shortcuts = ShortcutSettings {
                open_settings: open_settings_shortcut.as_str().to_owned(),
                new_session: new_session_shortcut.as_str().to_owned(),
                import_sessions: import_sessions_shortcut.as_str().to_owned(),
                export_selected: export_selected_shortcut.as_str().to_owned(),
                toggle_sidebar: toggle_sidebar_shortcut.as_str().to_owned(),
                copy_selection: copy_selection_shortcut.as_str().to_owned(),
                paste: paste_shortcut.as_str().to_owned(),
                open_sftp: open_sftp_shortcut.as_str().to_owned(),
            };
            if let Err(error) = shortcuts.validate() {
                if !is_preview {
                    set_status(&ui_for_save, &format!("Cannot save shortcuts: {error}"));
                }
                return;
            }
            let (known_shells, ui_language, system_icons_follow_theme) = match state.lock() {
                Ok(app) => (
                    app.sessions.settings.terminal.known_shells.clone(),
                    app.sessions.settings.ui_language,
                    app.sessions.settings.appearance.system_icons_follow_theme,
                ),
                Err(_) => {
                    set_status(&ui_for_save, "Cannot read local shell settings");
                    return;
                }
            };
            let settings = AppSettings::normalized(AppSettingsInput {
                appearance: AppearanceSettingsInput {
                    renderer_preference: renderer_preference.as_str(),
                    terminal_drawing_preference: terminal_drawing_preference.as_str(),
                    software_presentation: software_presentation.as_str(),
                    application_font_family: application_font_family.as_str(),
                    terminal_font_family: terminal_font_family.as_str(),
                    terminal_font_size: font_size,
                    terminal_line_height_percent: line_height_percent,
                    terminal_software_block_rows,
                    color_scheme: terminal_color_scheme_for_theme(
                        theme_mode.as_str(),
                        theme_palette.as_str(),
                    ),
                    text_brightness,
                    semantic_highlighting: terminal_semantic_highlighting,
                    terminal_compact_rendering,
                    terminal_row_render_cache,
                    terminal_cursor_blink,
                    system_icons_follow_theme,
                    focused_terminal_refresh_fps,
                    unfocused_terminal_refresh_fps,
                    terminal_semantic_colors: TerminalSemanticColorsInput {
                        link: terminal_semantic_link_color.as_str(),
                        success: terminal_semantic_success_color.as_str(),
                        info: terminal_semantic_info_color.as_str(),
                        warning: terminal_semantic_warning_color.as_str(),
                        error: terminal_semantic_error_color.as_str(),
                    },
                    bright_bold_text,
                    right_click_copy_or_paste,
                    copy_selection_on_select,
                    terminal_mouse_local_selection_priority,
                },
                terminal: TerminalSettingsInput {
                    scrollback_lines,
                    default_columns,
                    default_rows,
                    local_shell: local_shell.as_str(),
                    known_shells: &known_shells,
                    option_as_meta,
                    alternate_scroll_to_home,
                    osc52_clipboard,
                },
                workspace: WorkspaceSettingsInput {
                    sidebar_width,
                    tab_width,
                    session_mask_character: session_mask_character.as_str(),
                    collapsed_group_label_chars,
                },
                shortcuts,
                credential_storage: credential_storage.as_str(),
                ui_language: ui_language.as_setting(),
                sftp_per_tab_transfers,
                sftp_global_uploads,
                sftp_rate_limit_mib_per_second,
                sftp_local_open_max_file_mib,
                sftp_remote_edit_max_file_mib,
            });
            let mut settings = settings;
            if let Ok(app) = state.lock() {
                settings.audio = app.sessions.settings.audio.clone();
            }
            settings.sftp_transfer_filters = SftpTransferFilterSettings::normalized(
                sftp_filter_enabled,
                sftp_filter_use_platform_defaults,
                sftp_filter_custom_patterns.as_str(),
            );
            settings.x11 = X11Settings::normalized(
                x11_server_provider.as_str(),
                x11_server_app_path.as_str(),
                x11_launch_on_connect,
                x11_allow_no_auth,
            );
            settings.set_theme(ThemeSettings::normalized(
                theme_mode.as_str(),
                theme_palette.as_str(),
                ThemePalette {
                    background: theme_light_background.to_string(),
                    panel: theme_light_panel.to_string(),
                    panel_alt: theme_light_panel_alt.to_string(),
                    border: theme_light_border.to_string(),
                    text: theme_light_text.to_string(),
                    muted: theme_light_muted.to_string(),
                    accent: theme_light_accent.to_string(),
                    success: theme_light_success.to_string(),
                    danger: theme_light_danger.to_string(),
                    overlay: theme_light_overlay.to_string(),
                    terminal_foreground: theme_light_terminal_foreground.to_string(),
                    terminal_background: theme_light_terminal_background.to_string(),
                    terminal_selection: theme_light_terminal_selection.to_string(),
                },
                ThemePalette {
                    background: theme_dark_background.to_string(),
                    panel: theme_dark_panel.to_string(),
                    panel_alt: theme_dark_panel_alt.to_string(),
                    border: theme_dark_border.to_string(),
                    text: theme_dark_text.to_string(),
                    muted: theme_dark_muted.to_string(),
                    accent: theme_dark_accent.to_string(),
                    success: theme_dark_success.to_string(),
                    danger: theme_dark_danger.to_string(),
                    overlay: theme_dark_overlay.to_string(),
                    terminal_foreground: theme_dark_terminal_foreground.to_string(),
                    terminal_background: theme_dark_terminal_background.to_string(),
                    terminal_selection: theme_dark_terminal_selection.to_string(),
                },
            ));
            if is_preview {
                let families = match apply_preview_settings(&state, settings.clone()) {
                    Ok(families) => families,
                    Err(error) => {
                        set_status(
                            &ui_for_save,
                            &format!("Cannot preview workspace settings: {error}"),
                        );
                        return;
                    }
                };
                if let Some(ui) = ui_for_save.upgrade() {
                    apply_settings_to_open_windows(&ui, &settings);
                }
                apply_terminal_presentation_policy(&window_router, &settings);
                refresh_session_models(&ui_for_save, &state);
                // Tile grouping is part of the terminal snapshot model, so a
                // settings preview must rebuild visible terminal views once.
                refresh_workspace(&ui_for_save, &state);
                load_preview_bundled_fonts(
                    runtime.clone(),
                    state.clone(),
                    ui_for_save.clone(),
                    font_registry_for_save.clone(),
                    families,
                );
                return;
            }
            let state = state.clone();
            let ui = ui_for_save.clone();
            let window_router_for_save = window_router.clone();
            let font_registry = font_registry_for_save.clone();
            let runtime_for_save = runtime.clone();
            let persistence_for_save = persistence_for_save.clone();
            let runtime_for_close = runtime_for_save.clone();
            set_status(&ui_for_save, "Saving workspace settings...");
            runtime.spawn(async move {
                let families = vec![
                    settings.appearance.application_font_family.clone(),
                    settings.appearance.terminal_font_family.clone(),
                ];
                let font_load = match load_settings_fonts(&font_registry, families).await {
                    Ok(fonts) => fonts,
                    Err(error) => {
                        set_status(&ui, &format!("Cannot read font resources: {error:#}"));
                        return;
                    }
                };
                dispatch_ui(&ui, move |ui| {
                    if let Err(error) = register_settings_fonts(&font_registry, font_load) {
                        ui.set_status(format!("Cannot register font: {error}").into());
                        return;
                    }

                    let state_for_save = state.clone();
                    let state_for_refresh = state.clone();
                    let settings_for_save = settings.clone();
                    let ui_for_result = ui.as_weak();
                    let ui_for_refresh = ui_for_result.clone();
                    let ui_for_close = ui_for_result.clone();
                    let state_for_close = state_for_refresh.clone();
                    runtime_for_save.spawn(async move {
                        let _persistence_guard = persistence_for_save.gate.lock().await;
                        let save_result = tokio::task::spawn_blocking(move || {
                            save_workspace_settings(&state_for_save, settings_for_save)
                        })
                        .await;
                        match save_result {
                            Ok(Ok(saved_settings)) => dispatch_ui(&ui_for_result, move |ui| {
                                apply_settings_to_open_windows(ui, &saved_settings);
                                apply_terminal_presentation_policy(
                                    &window_router_for_save,
                                    &saved_settings,
                                );
                                refresh_session_models(&ui_for_refresh, &state_for_refresh);
                                ui.set_status("".into());
                                if let Some(tab_id) = close_tab_id {
                                    close_workspace_tab(
                                        tab_id,
                                        &state_for_close,
                                        &ui_for_close,
                                        &runtime_for_close,
                                    );
                                } else {
                                    refresh_workspace(&ui_for_refresh, &state_for_refresh);
                                }
                            }),
                            Ok(Err(error)) => set_status(
                                &ui_for_result,
                                &format!("Cannot save workspace settings: {error}"),
                            ),
                            Err(error) => set_status(
                                &ui_for_result,
                                &format!("Workspace settings task failed: {error}"),
                            ),
                        }
                    });
                    ui.set_status("Saving workspace settings...".into());
                });
            });
        },
    );
}

fn save_workspace_settings(
    state: &Arc<Mutex<AppState>>,
    mut settings: AppSettings,
) -> Result<AppSettings> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    settings.audio = app.sessions.settings.audio.clone();
    settings.ui_language = app.sessions.settings.ui_language;
    settings
        .appearance
        .local_shell_icon_override
        .clone_from(&app.sessions.settings.appearance.local_shell_icon_override);
    let mut candidate = app.sessions.clone();
    candidate.settings = settings.clone();
    app.config.save(&candidate)?;
    app.sessions = candidate;
    app.apply_scrollback_setting();
    ax_ssh::sftp::configure_global_upload_limit(usize::from(
        settings.sftp_transfers.global_uploads,
    ));
    Ok(settings)
}

// Presentation order shared by the server and Local Shell pickers.
const COMMON_SYSTEM_ICON_IDS: &[&str] = &[
    "ubuntu",
    "debian",
    "windows",
    "macos",
    "centos",
    "rocky-linux",
    "almalinux",
    "rhel",
    "fedora",
    "arch-linux",
    "alpine-linux",
    "opensuse",
    "openeuler",
    "anolis-os",
    "linux",
];

fn system_icon_choices(query: &str) -> Vec<SystemIconChoiceRow> {
    let query = query.chars().take(128).collect::<String>().to_lowercase();
    let mut choices = ax_ssh::config::system_icon_choices()
        .iter()
        .filter(|icon| {
            query.split_whitespace().all(|word| {
                icon.name.to_lowercase().contains(word)
                    || icon.id.contains(word)
                    || icon.family.contains(word)
                    || icon.aliases.iter().any(|alias| alias.contains(word))
            })
        })
        .collect::<Vec<_>>();
    choices.sort_by_cached_key(|icon| {
        (
            COMMON_SYSTEM_ICON_IDS
                .iter()
                .position(|id| *id == icon.id)
                .unwrap_or(COMMON_SYSTEM_ICON_IDS.len()),
            icon.name.to_lowercase(),
            icon.id,
        )
    });
    choices
        .into_iter()
        .map(|icon| SystemIconChoiceRow {
            id: icon.id.into(),
            name: icon.name.into(),
            family: icon.family.into(),
        })
        .collect()
}

fn system_icon_choice(value: &str) -> SystemIconChoiceRow {
    ax_ssh::config::system_icon_id(value)
        .and_then(|id| {
            ax_ssh::config::system_icon_choices()
                .iter()
                .find(|icon| icon.id == id)
        })
        .map(|icon| SystemIconChoiceRow {
            id: icon.id.into(),
            name: icon.name.into(),
            family: icon.family.into(),
        })
        .unwrap_or_default()
}

fn save_local_shell_icon(
    state: &Arc<Mutex<AppState>>,
    icon: Option<String>,
    revision: u64,
    latest_revision: &AtomicU64,
) -> Result<Option<AppSettings>> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    if latest_revision.load(Ordering::Acquire) != revision {
        return Ok(None);
    }
    let mut candidate = app.config.load()?;
    candidate.settings.appearance.local_shell_icon_override = icon.clone();
    app.config.save(&candidate)?;
    app.sessions.settings.appearance.local_shell_icon_override = icon;
    Ok(Some(app.sessions.settings.clone()))
}

fn save_ui_language(
    state: &Arc<Mutex<AppState>>,
    language: UiLanguage,
    revision: u64,
    latest_revision: &AtomicU64,
) -> Result<bool> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    if latest_revision.load(Ordering::Acquire) != revision {
        return Ok(false);
    }
    let mut candidate = app.config.load()?;
    candidate.settings.ui_language = language;
    app.config.save(&candidate)?;
    app.sessions.settings.ui_language = language;
    Ok(true)
}

fn apply_preview_settings(
    state: &Arc<Mutex<AppState>>,
    mut settings: AppSettings,
) -> Result<Vec<String>> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    settings.audio = app.sessions.settings.audio.clone();
    settings.ui_language = app.sessions.settings.ui_language;
    settings
        .appearance
        .local_shell_icon_override
        .clone_from(&app.sessions.settings.appearance.local_shell_icon_override);
    let font_families = changed_font_families(&app.sessions.settings, &settings);
    app.sessions.settings = settings;
    app.apply_scrollback_setting();
    ax_ssh::sftp::configure_global_upload_limit(usize::from(
        app.sessions.settings.sftp_transfers.global_uploads,
    ));
    Ok(font_families)
}

fn changed_font_families(previous: &AppSettings, current: &AppSettings) -> Vec<String> {
    let pairs = [
        (
            &previous.appearance.application_font_family,
            &current.appearance.application_font_family,
        ),
        (
            &previous.appearance.terminal_font_family,
            &current.appearance.terminal_font_family,
        ),
    ];
    let mut changed = Vec::new();
    for (previous, current) in pairs {
        if previous.eq_ignore_ascii_case(current)
            || changed
                .iter()
                .any(|family: &String| family.eq_ignore_ascii_case(current))
        {
            continue;
        }
        changed.push(current.clone());
    }
    changed
}

async fn load_settings_fonts(
    font_registry: &Arc<Mutex<FontRegistry>>,
    families: Vec<String>,
) -> Result<Vec<LoadedBundledFont>> {
    let resources = font_registry
        .lock()
        .map_err(|_| anyhow::anyhow!("font registry lock poisoned"))?
        .resources();
    tokio::task::spawn_blocking(move || resources.load_bundled_fonts(&families))
        .await
        .context("font loading task failed")?
}

fn register_settings_fonts(
    font_registry: &Arc<Mutex<FontRegistry>>,
    fonts: Vec<LoadedBundledFont>,
) -> Result<i32> {
    let mut registry = font_registry
        .lock()
        .map_err(|_| anyhow::anyhow!("font registry lock poisoned"))?;
    for font in fonts {
        registry.register_loaded_font(font)?;
    }
    Ok(registry.generation_as_slint_int())
}

fn load_preview_bundled_fonts(
    runtime: Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    font_registry: Arc<Mutex<FontRegistry>>,
    families: Vec<String>,
) {
    if families.is_empty() {
        return;
    }
    runtime.spawn(async move {
        let font_load = match load_settings_fonts(&font_registry, families).await {
            Ok(fonts) => fonts,
            Err(error) => {
                set_status(&ui, &format!("Cannot read font resources: {error:#}"));
                return;
            }
        };
        dispatch_ui(&ui, move |ui| {
            let generation = match register_settings_fonts(&font_registry, font_load) {
                Ok(generation) => generation,
                Err(error) => {
                    ui.set_status(format!("Cannot register font: {error}").into());
                    return;
                }
            };
            super::view::set_font_registry_generation(ui, generation);
            let settings = match state.lock() {
                Ok(app) => app.sessions.settings.clone(),
                Err(_) => {
                    ui.set_status("Cannot read workspace settings".into());
                    return;
                }
            };
            apply_settings_to_open_windows(ui, &settings);
            super::view::refresh_workspace(&ui.as_weak(), &state);
        });
    });
}

fn terminal_color_scheme_for_theme(mode: &str, palette: &str) -> &'static str {
    if mode.trim().eq_ignore_ascii_case("light") {
        "light"
    } else if palette.trim().eq_ignore_ascii_case("solarized") {
        "solarized-dark"
    } else if palette.trim().eq_ignore_ascii_case("arctic") {
        "arctic-dark"
    } else if palette.trim().eq_ignore_ascii_case("tokyo") {
        "tokyo-dark"
    } else if palette.trim().eq_ignore_ascii_case("ember") {
        "ember-dark"
    } else if palette.trim().eq_ignore_ascii_case("forest") {
        "forest-dark"
    } else {
        "dark"
    }
}

#[cfg(test)]
mod tests;
