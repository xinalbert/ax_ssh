use super::*;

pub(in crate::app) fn apply_settings_to_component(ui: &AppWindow, settings: &AppSettings) {
    ui.set_ui_language(settings.ui_language.as_setting().into());
    apply_theme_to_component(ui, settings);
    ui.set_application_font_family(settings.appearance.application_font_family.clone().into());
    ui.set_application_font_index(font_option_index(
        &ui.get_application_font_options(),
        &settings.appearance.application_font_family,
    ));
    ui.set_terminal_font_family(settings.appearance.terminal_font_family.clone().into());
    ui.set_terminal_font_index(font_option_index(
        &ui.get_terminal_font_options(),
        &settings.appearance.terminal_font_family,
    ));
    ui.set_terminal_font_size(i32::from(settings.appearance.terminal_font_size));
    ui.set_terminal_line_height_percent(i32::from(
        settings.appearance.terminal_line_height_percent,
    ));
    ui.set_terminal_minimum_contrast_ratio(
        f32::from(settings.appearance.terminal_minimum_contrast_ratio_tenths) / 10.0,
    );
    ui.set_bright_bold_text(settings.appearance.bright_bold_text);
    ui.set_terminal_semantic_link_color(
        settings
            .appearance
            .terminal_semantic_colors
            .link
            .clone()
            .into(),
    );
    ui.set_terminal_semantic_success_color(
        settings
            .appearance
            .terminal_semantic_colors
            .success
            .clone()
            .into(),
    );
    ui.set_terminal_semantic_info_color(
        settings
            .appearance
            .terminal_semantic_colors
            .info
            .clone()
            .into(),
    );
    ui.set_terminal_semantic_warning_color(
        settings
            .appearance
            .terminal_semantic_colors
            .warning
            .clone()
            .into(),
    );
    ui.set_terminal_semantic_error_color(
        settings
            .appearance
            .terminal_semantic_colors
            .error
            .clone()
            .into(),
    );
    ui.set_right_click_copy_or_paste(settings.appearance.right_click_copy_or_paste);
    ui.set_copy_selection_on_select(settings.appearance.copy_selection_on_select);
    ui.set_option_as_meta(settings.terminal.option_as_meta);
    ui.set_x11_server_provider(
        ax_ssh::x_server::provider_for_current_platform(settings.x11.provider)
            .as_setting()
            .into(),
    );
    ui.set_x11_server_provider_index(ax_ssh::x_server::provider_index(settings.x11.provider));
    ui.set_x11_server_app_path(settings.x11.app_path.clone().into());
    ui.set_x11_launch_on_connect(settings.x11.launch_on_connect);
    ui.set_x11_allow_no_auth(settings.x11.allow_no_auth);
    ui.set_scrollback_lines(settings.terminal.scrollback_lines as i32);
    ui.set_default_terminal_columns(i32::from(settings.terminal.default_columns));
    ui.set_default_terminal_rows(i32::from(settings.terminal.default_rows));
    ui.set_local_shell(settings.terminal.local_shell.clone().into());
    let local_shell_index = settings
        .terminal
        .known_shells
        .iter()
        .position(|shell| shell.eq_ignore_ascii_case(&settings.terminal.local_shell))
        .unwrap_or(0);
    ui.set_local_shell_index(local_shell_index.min(i32::MAX as usize) as i32);
    ui.set_credential_storage(settings.credential_storage.as_setting().into());
    ui.set_sidebar_width(i32::from(settings.workspace.sidebar_width));
    ui.set_tab_width(i32::from(settings.workspace.tab_width));
    ui.set_session_mask_character(settings.workspace.session_mask_character.clone().into());
    ui.set_collapsed_group_label_chars(i32::from(settings.workspace.collapsed_group_label_chars));
    ui.set_open_settings_shortcut(settings.shortcuts.open_settings.clone().into());
    ui.set_new_session_shortcut(settings.shortcuts.new_session.clone().into());
    ui.set_import_sessions_shortcut(settings.shortcuts.import_sessions.clone().into());
    ui.set_export_selected_shortcut(settings.shortcuts.export_selected.clone().into());
    ui.set_toggle_sidebar_shortcut(settings.shortcuts.toggle_sidebar.clone().into());
    ui.set_copy_selection_shortcut(settings.shortcuts.copy_selection.clone().into());
    ui.set_paste_shortcut(settings.shortcuts.paste.clone().into());
    let select_all_shortcut = terminal_select_all_shortcut_for_platform(cfg!(target_os = "macos"));
    ui.set_select_all_shortcut(select_all_shortcut.into());
    ui.set_open_sftp_shortcut(settings.shortcuts.open_sftp.clone().into());
    ui.set_open_settings_menu_shortcut(menu_shortcut_keys(
        "open-settings",
        &settings.shortcuts.open_settings,
    ));
    ui.set_new_session_menu_shortcut(menu_shortcut_keys(
        "new-session",
        &settings.shortcuts.new_session,
    ));
    ui.set_import_sessions_menu_shortcut(menu_shortcut_keys(
        "import-sessions",
        &settings.shortcuts.import_sessions,
    ));
    ui.set_export_selected_menu_shortcut(menu_shortcut_keys(
        "export-selected",
        &settings.shortcuts.export_selected,
    ));
    ui.set_toggle_sidebar_menu_shortcut(menu_shortcut_keys(
        "toggle-sidebar",
        &settings.shortcuts.toggle_sidebar,
    ));
    ui.set_copy_selection_menu_shortcut(menu_shortcut_keys(
        "copy-terminal",
        &settings.shortcuts.copy_selection,
    ));
    ui.set_paste_menu_shortcut(menu_shortcut_keys(
        "paste-terminal",
        &settings.shortcuts.paste,
    ));
    ui.set_select_all_menu_shortcut(menu_shortcut_keys(
        "select-all-terminal",
        select_all_shortcut,
    ));
    ui.set_open_sftp_menu_shortcut(menu_shortcut_keys(
        "open-sftp",
        &settings.shortcuts.open_sftp,
    ));
    let defaults = ShortcutSettings::default();
    ui.set_default_open_settings_shortcut(defaults.open_settings.into());
    ui.set_default_new_session_shortcut(defaults.new_session.into());
    ui.set_default_import_sessions_shortcut(defaults.import_sessions.into());
    ui.set_default_export_selected_shortcut(defaults.export_selected.into());
    ui.set_default_toggle_sidebar_shortcut(defaults.toggle_sidebar.into());
    ui.set_default_copy_selection_shortcut(defaults.copy_selection.into());
    ui.set_default_paste_shortcut(defaults.paste.into());
    ui.set_default_open_sftp_shortcut(defaults.open_sftp.into());
    #[cfg(target_os = "macos")]
    schedule_macos_application_menu_configuration(ui);
}

pub(in crate::app) fn select_ui_language(language: UiLanguage) -> Result<()> {
    let locale = language.resolved_locale(sys_locale::get_locale().as_deref());
    slint::select_bundled_translation(locale)
        .with_context(|| format!("failed to select bundled UI locale {locale}"))
}

pub(in crate::app) fn apply_ui_language_to_open_windows(
    ui: &AppWindow,
    language: UiLanguage,
) -> Result<()> {
    select_ui_language(language)?;
    ui.set_ui_language(language.as_setting().into());
    if let Some(router) = global_window_router() {
        for detached_ui in router.detached_uis() {
            if let Some(detached_ui) = detached_ui.upgrade() {
                detached_ui.set_ui_language(language.as_setting().into());
            }
        }
    }
    #[cfg(target_os = "macos")]
    schedule_macos_application_menu_configuration(ui);
    Ok(())
}

pub(in crate::app) fn apply_settings_to_open_windows(ui: &AppWindow, settings: &AppSettings) {
    apply_settings_to_component(ui, settings);
    let Some(router) = global_window_router() else {
        return;
    };
    for detached_ui in router.detached_uis() {
        let Some(detached_ui) = detached_ui.upgrade() else {
            continue;
        };
        apply_settings_to_component(&detached_ui, settings);
        #[cfg(target_os = "macos")]
        if let Err(error) = macos_window::update_detached_titlebar_background(
            detached_ui.window(),
            detached_titlebar_background(&detached_ui),
        ) {
            warn!(%error, "failed to update detached macOS title-bar background");
        }
    }
}

pub(super) fn menu_shortcut_keys(action: &'static str, setting: &str) -> slint::Keys {
    match menu_shortcut_from_setting(setting) {
        Ok(shortcut) => shortcut.keys,
        Err(error) => {
            warn!(action, %error, "cannot bind configured native menu shortcut");
            slint::Keys::default()
        }
    }
}

pub(super) fn apply_theme_to_component(ui: &AppWindow, settings: &AppSettings) {
    let light = settings.appearance.theme.light_palette();
    let dark = settings.appearance.theme.dark_palette();
    let theme = ui.global::<Theme>();
    theme.set_application_font_family(settings.appearance.application_font_family.clone().into());
    theme.set_mode(settings.appearance.theme.mode.as_setting().into());
    theme.set_palette(settings.appearance.theme.palette.as_setting().into());
    set_theme_palette(&theme, &light, true);
    set_theme_palette(&theme, &dark, false);

    ui.set_theme_mode(settings.appearance.theme.mode.as_setting().into());
    ui.set_theme_palette(settings.appearance.theme.palette.as_setting().into());
    set_ui_theme_palette(ui, &settings.appearance.theme.custom_light, true);
    set_ui_theme_palette(ui, &settings.appearance.theme.custom_dark, false);
    ui.set_theme_revision(ui.get_theme_revision().wrapping_add(1));
}

pub(super) fn set_theme_palette(theme: &Theme, palette: &ThemePalette, light: bool) {
    if light {
        theme.set_light_background(theme_color(&palette.background));
        theme.set_light_panel(theme_color(&palette.panel));
        theme.set_light_panel_alt(theme_color(&palette.panel_alt));
        theme.set_light_border(theme_color(&palette.border));
        theme.set_light_text(theme_color(&palette.text));
        theme.set_light_muted(theme_color(&palette.muted));
        theme.set_light_accent(theme_color(&palette.accent));
        theme.set_light_success(theme_color(&palette.success));
        theme.set_light_danger(theme_color(&palette.danger));
        theme.set_light_overlay(theme_color(&palette.overlay));
        theme.set_light_terminal_foreground(theme_color(&palette.terminal_foreground));
        theme.set_light_terminal_background(theme_color(&palette.terminal_background));
        theme.set_light_terminal_selection(theme_color(&palette.terminal_selection));
    } else {
        theme.set_dark_background(theme_color(&palette.background));
        theme.set_dark_panel(theme_color(&palette.panel));
        theme.set_dark_panel_alt(theme_color(&palette.panel_alt));
        theme.set_dark_border(theme_color(&palette.border));
        theme.set_dark_text(theme_color(&palette.text));
        theme.set_dark_muted(theme_color(&palette.muted));
        theme.set_dark_accent(theme_color(&palette.accent));
        theme.set_dark_success(theme_color(&palette.success));
        theme.set_dark_danger(theme_color(&palette.danger));
        theme.set_dark_overlay(theme_color(&palette.overlay));
        theme.set_dark_terminal_foreground(theme_color(&palette.terminal_foreground));
        theme.set_dark_terminal_background(theme_color(&palette.terminal_background));
        theme.set_dark_terminal_selection(theme_color(&palette.terminal_selection));
    }
}

pub(super) fn set_ui_theme_palette(ui: &AppWindow, palette: &ThemePalette, light: bool) {
    if light {
        ui.set_theme_light_background(palette.background.clone().into());
        ui.set_theme_light_panel(palette.panel.clone().into());
        ui.set_theme_light_panel_alt(palette.panel_alt.clone().into());
        ui.set_theme_light_border(palette.border.clone().into());
        ui.set_theme_light_text(palette.text.clone().into());
        ui.set_theme_light_muted(palette.muted.clone().into());
        ui.set_theme_light_accent(palette.accent.clone().into());
        ui.set_theme_light_success(palette.success.clone().into());
        ui.set_theme_light_danger(palette.danger.clone().into());
        ui.set_theme_light_overlay(palette.overlay.clone().into());
        ui.set_theme_light_terminal_foreground(palette.terminal_foreground.clone().into());
        ui.set_theme_light_terminal_background(palette.terminal_background.clone().into());
        ui.set_theme_light_terminal_selection(palette.terminal_selection.clone().into());
    } else {
        ui.set_theme_dark_background(palette.background.clone().into());
        ui.set_theme_dark_panel(palette.panel.clone().into());
        ui.set_theme_dark_panel_alt(palette.panel_alt.clone().into());
        ui.set_theme_dark_border(palette.border.clone().into());
        ui.set_theme_dark_text(palette.text.clone().into());
        ui.set_theme_dark_muted(palette.muted.clone().into());
        ui.set_theme_dark_accent(palette.accent.clone().into());
        ui.set_theme_dark_success(palette.success.clone().into());
        ui.set_theme_dark_danger(palette.danger.clone().into());
        ui.set_theme_dark_overlay(palette.overlay.clone().into());
        ui.set_theme_dark_terminal_foreground(palette.terminal_foreground.clone().into());
        ui.set_theme_dark_terminal_background(palette.terminal_background.clone().into());
        ui.set_theme_dark_terminal_selection(palette.terminal_selection.clone().into());
    }
}

pub(in crate::app) fn empty_terminal_snapshot() -> TerminalSnapshot {
    TerminalSnapshot {
        text: String::new(),
        lines: vec![Default::default()],
        max_columns: 0,
        cursor_row: 0,
        cursor_column: 0,
        cursor_visible: false,
        cursor_text: " ".to_owned(),
        mouse_reporting: Default::default(),
        mouse_reporting_active: false,
    }
}

pub(in crate::app) fn apply_rendered_terminal(
    ui: &AppWindow,
    rendered: terminal_render::RenderedTerminal,
) {
    ui.set_terminal_content_columns(rendered.max_columns.min(i32::MAX as usize) as i32);
    ui.set_terminal_cursor_row(rendered.cursor_row.min(i32::MAX as usize) as i32);
    ui.set_terminal_cursor_column(rendered.cursor_column.min(i32::MAX as usize) as i32);
    ui.set_terminal_cursor_visible(rendered.cursor_visible);
    ui.set_terminal_cursor_text(rendered.cursor_text.into());
    ui.set_terminal_render_foreground(to_slint_color(rendered.foreground));
    ui.set_terminal_render_background(to_slint_color(rendered.background));
    ui.set_terminal_render_selection_background(to_slint_color(rendered.selection_background));
    let lines = rendered
        .lines
        .into_iter()
        .map(terminal_render_line)
        .collect::<Vec<_>>();
    ui.set_terminal_render_lines(ModelRc::new(VecModel::from(lines)));
}

pub(in crate::app) fn terminal_render_line(line: RenderedTerminalLine) -> TerminalRenderLine {
    let runs = line
        .runs
        .into_iter()
        .map(terminal_render_run)
        .collect::<Vec<_>>();
    TerminalRenderLine {
        runs: ModelRc::new(VecModel::from(runs)),
    }
}

pub(in crate::app) fn terminal_render_run(run: RenderedTerminalRun) -> TerminalRenderRun {
    TerminalRenderRun {
        text: run.text.into(),
        column: run.column.min(i32::MAX as usize) as i32,
        cells: run.cells.min(i32::MAX as usize) as i32,
        foreground: to_slint_color(run.foreground),
        background: to_slint_color(run.background),
        bold: run.bold,
        italic: run.italic,
        underline: run.underline,
        strikethrough: run.strikethrough,
    }
}

pub(in crate::app) fn to_slint_color(color: RgbColor) -> Color {
    Color::from_rgb_u8(color.red, color.green, color.blue)
}

pub(super) fn to_rgb_color(color: Color) -> RgbColor {
    let rgba = color.to_argb_u8();
    RgbColor::new(rgba.red, rgba.green, rgba.blue)
}

pub(super) fn theme_color(value: &str) -> Color {
    let value = value.trim().trim_start_matches('#');
    let fallback = Color::from_rgb_u8(23, 25, 24);
    let (red, green, blue, alpha) = match value.as_bytes() {
        [red_a, red_b, green_a, green_b, blue_a, blue_b] => (
            hex_byte(*red_a, *red_b),
            hex_byte(*green_a, *green_b),
            hex_byte(*blue_a, *blue_b),
            Some(255),
        ),
        [
            red_a,
            red_b,
            green_a,
            green_b,
            blue_a,
            blue_b,
            alpha_a,
            alpha_b,
        ] => (
            hex_byte(*red_a, *red_b),
            hex_byte(*green_a, *green_b),
            hex_byte(*blue_a, *blue_b),
            hex_byte(*alpha_a, *alpha_b),
        ),
        _ => return fallback,
    };
    match (red, green, blue, alpha) {
        (Some(red), Some(green), Some(blue), Some(alpha)) => {
            Color::from_argb_u8(alpha, red, green, blue)
        }
        _ => fallback,
    }
}

pub(super) fn hex_byte(high: u8, low: u8) -> Option<u8> {
    let high = hex_digit(high)?;
    let low = hex_digit(low)?;
    Some(high * 16 + low)
}

pub(super) fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
