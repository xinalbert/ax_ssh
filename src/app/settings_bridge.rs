use super::*;

pub(super) fn wire_settings(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    let ui_for_cancel = ui.as_weak();
    let state_for_cancel = state.clone();
    let runtime_for_cancel = runtime.clone();
    ui.on_cancel_settings(move || {
        let active_id = state_for_cancel
            .lock()
            .ok()
            .and_then(|app| app.active_tab_id());
        if let Some(active_id) = active_id {
            close_workspace_tab(
                active_id,
                &state_for_cancel,
                &ui_for_cancel,
                &runtime_for_cancel,
            );
        }
    });

    let ui_for_save = ui.as_weak();
    ui.on_save_settings(
        move |font_family,
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
              local_shell,
              scrollback_lines,
              default_columns,
              default_rows,
              sidebar_width,
              tab_width,
              session_mask_character,
              open_settings_shortcut,
              toggle_sidebar_shortcut,
              copy_selection_shortcut,
              paste_shortcut| {
            let shortcuts = ShortcutSettings {
                open_settings: open_settings_shortcut.as_str().to_owned(),
                toggle_sidebar: toggle_sidebar_shortcut.as_str().to_owned(),
                copy_selection: copy_selection_shortcut.as_str().to_owned(),
                paste: paste_shortcut.as_str().to_owned(),
            };
            if let Err(error) = shortcuts.validate() {
                set_status(&ui_for_save, &format!("Cannot save shortcuts: {error}"));
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
                font_family.as_str(),
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
                sidebar_width,
                tab_width,
                session_mask_character.as_str(),
                &shortcuts.open_settings,
                &shortcuts.toggle_sidebar,
                &shortcuts.copy_selection,
                &shortcuts.paste,
            );
            let mut settings = settings;
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
            let state = state.clone();
            let ui = ui_for_save.clone();
            set_status(&ui_for_save, "Saving workspace settings...");
            runtime.spawn(async move {
                let save_result = (|| -> Result<()> {
                    let mut app = state
                        .lock()
                        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
                    let mut candidate = app.sessions.clone();
                    candidate.settings = settings.clone();
                    app.config.save(&candidate)?;
                    app.sessions = candidate;
                    app.apply_scrollback_setting();
                    Ok(())
                })();
                match save_result {
                    Ok(()) => {
                        apply_settings(&ui, settings);
                        refresh_session_models(&ui, &state);
                        refresh_workspace(&ui, &state);
                        set_status(&ui, "Workspace settings saved");
                    }
                    Err(error) => {
                        set_status(&ui, &format!("Cannot save workspace settings: {error}"));
                    }
                }
            });
        },
    );
}

fn terminal_color_scheme_for_theme(mode: &str, palette: &str) -> &'static str {
    if mode.trim().eq_ignore_ascii_case("light") {
        "light"
    } else if palette.trim().eq_ignore_ascii_case("solarized") {
        "solarized-dark"
    } else {
        "dark"
    }
}
