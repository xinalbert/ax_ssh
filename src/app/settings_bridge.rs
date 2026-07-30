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
              color_scheme,
              brightness_percent,
              bright_bold_text,
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
                color_scheme.as_str(),
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
