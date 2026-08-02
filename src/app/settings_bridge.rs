use super::*;

pub(super) fn wire_settings(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    font_registry: Arc<Mutex<FontRegistry>>,
) {
    let ui_for_save = ui.as_weak();
    let font_registry_for_save = font_registry;
    ui.on_save_settings(
        move |application_font_family,
              terminal_font_family,
              font_size,
              line_height_percent,
              brightness_percent,
              bright_bold_text,
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
              option_as_meta,
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
            let known_shells = match state.lock() {
                Ok(app) => app.sessions.settings.terminal.known_shells.clone(),
                Err(_) => {
                    set_status(&ui_for_save, "Cannot read local shell settings");
                    return;
                }
            };
            let settings = AppSettings::normalized(
                application_font_family.as_str(),
                terminal_font_family.as_str(),
                font_size,
                line_height_percent,
                terminal_color_scheme_for_theme(theme_mode.as_str(), theme_palette.as_str()),
                brightness_percent,
                bright_bold_text,
                right_click_copy_or_paste,
                scrollback_lines,
                default_columns,
                default_rows,
                local_shell.as_str(),
                &known_shells,
                option_as_meta,
                sidebar_width,
                tab_width,
                session_mask_character.as_str(),
                collapsed_group_label_chars,
                &shortcuts.open_settings,
                &shortcuts.new_session,
                &shortcuts.import_sessions,
                &shortcuts.export_selected,
                &shortcuts.toggle_sidebar,
                &shortcuts.copy_selection,
                &shortcuts.paste,
                &shortcuts.open_sftp,
                credential_storage.as_str(),
            );
            let mut settings = settings;
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
                    apply_settings_to_component(&ui, &settings);
                }
                refresh_session_models(&ui_for_save, &state);
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
            let font_registry = font_registry_for_save.clone();
            let runtime_for_save = runtime.clone();
            let runtime_for_close = runtime_for_save.clone();
            set_status(&ui_for_save, "Saving workspace settings...");
            runtime.spawn(async move {
                let resources = match font_registry.lock() {
                    Ok(registry) => registry.resources(),
                    Err(_) => {
                        set_status(&ui, "Cannot access font resources");
                        return;
                    }
                };
                let families = vec![
                    settings.appearance.application_font_family.clone(),
                    settings.appearance.terminal_font_family.clone(),
                ];
                let font_load = match tokio::task::spawn_blocking(move || {
                    resources.load_bundled_fonts(&families)
                })
                .await
                {
                    Ok(Ok(fonts)) => fonts,
                    Ok(Err(error)) => {
                        set_status(&ui, &format!("Cannot read font resources: {error}"));
                        return;
                    }
                    Err(error) => {
                        set_status(&ui, &format!("Font loading task failed: {error}"));
                        return;
                    }
                };
                dispatch_ui(&ui, move |ui| {
                    for font in font_load {
                        let registration = font_registry
                            .lock()
                            .map_err(|_| anyhow::anyhow!("font registry lock poisoned"))
                            .and_then(|mut registry| registry.register_loaded_font(font));
                        if let Err(error) = registration {
                            ui.set_status(format!("Cannot register font: {error}").into());
                            return;
                        }
                    }

                    let state_for_save = state.clone();
                    let state_for_refresh = state.clone();
                    let settings_for_save = settings.clone();
                    let settings_for_apply = settings.clone();
                    let ui_for_result = ui.as_weak();
                    let ui_for_refresh = ui_for_result.clone();
                    let ui_for_close = ui_for_result.clone();
                    let state_for_close = state_for_refresh.clone();
                    runtime_for_save.spawn(async move {
                        let save_result = tokio::task::spawn_blocking(move || {
                            save_workspace_settings(&state_for_save, settings_for_save)
                        })
                        .await;
                        match save_result {
                            Ok(Ok(())) => dispatch_ui(&ui_for_result, move |ui| {
                                apply_settings_to_component(ui, &settings_for_apply);
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

fn save_workspace_settings(state: &Arc<Mutex<AppState>>, settings: AppSettings) -> Result<()> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut candidate = app.sessions.clone();
    candidate.settings = settings;
    app.config.save(&candidate)?;
    app.sessions = candidate;
    app.apply_scrollback_setting();
    Ok(())
}

fn apply_preview_settings(
    state: &Arc<Mutex<AppState>>,
    settings: AppSettings,
) -> Result<Vec<String>> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let font_families = changed_font_families(&app.sessions.settings, &settings);
    app.sessions.settings = settings;
    app.apply_scrollback_setting();
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
    let resources = match font_registry.lock() {
        Ok(registry) => registry.resources(),
        Err(_) => {
            set_status(&ui, "Cannot access font resources");
            return;
        }
    };
    runtime.spawn(async move {
        let font_load = match tokio::task::spawn_blocking(move || {
            resources.load_bundled_fonts(&families)
        })
        .await
        {
            Ok(Ok(fonts)) => fonts,
            Ok(Err(error)) => {
                set_status(&ui, &format!("Cannot read font resources: {error}"));
                return;
            }
            Err(error) => {
                set_status(&ui, &format!("Font loading task failed: {error}"));
                return;
            }
        };
        dispatch_ui(&ui, move |ui| {
            for font in font_load {
                let registration = font_registry
                    .lock()
                    .map_err(|_| anyhow::anyhow!("font registry lock poisoned"))
                    .and_then(|mut registry| registry.register_loaded_font(font));
                if let Err(error) = registration {
                    ui.set_status(format!("Cannot register font: {error}").into());
                    return;
                }
            }
            let settings = match state.lock() {
                Ok(app) => app.sessions.settings.clone(),
                Err(_) => {
                    ui.set_status("Cannot read workspace settings".into());
                    return;
                }
            };
            apply_settings_to_component(ui, &settings);
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
mod tests {
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

        let loaded = apply_preview_settings(&state, settings.clone()).unwrap();

        assert!(loaded.is_empty());
        assert_eq!(state.lock().unwrap().sessions.settings, settings);
        assert!(!path.exists());
    }
}
