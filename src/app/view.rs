use std::collections::BTreeSet;

use super::*;

pub(super) fn session_rows(
    sessions: &SessionStore,
    expanded_groups: &BTreeSet<String>,
) -> Vec<SessionRow> {
    let mut rows = Vec::new();
    for group in session_groups(sessions) {
        let group_name = group.name;
        let display_name = if group_name.is_empty() {
            "Ungrouped".to_owned()
        } else {
            group_name.clone()
        };
        let profiles = group.profiles;
        let expanded = expanded_groups.contains(&group_name);
        rows.push(SessionRow {
            id: "".into(),
            group_name: group_name.clone().into(),
            name: display_name.clone().into(),
            endpoint: profiles.len().to_string().into(),
            icon: compact_label(&display_name, "Un").into(),
            is_group: true,
            expanded,
        });
        if expanded {
            rows.extend(profiles.into_iter().map(|profile| {
                SessionRow {
                    id: profile.id.to_string().into(),
                    group_name: group_name.clone().into(),
                    name: profile.name.clone().into(),
                    endpoint: profile_sidebar_endpoint(
                        profile,
                        &sessions.settings.workspace.session_mask_character,
                    )
                    .into(),
                    icon: compact_label(&profile.name, "--").into(),
                    is_group: false,
                    expanded: false,
                }
            }));
        }
    }
    rows
}

pub(super) fn connection_option_rows(sessions: &SessionStore) -> Vec<ConnectableSessionRow> {
    sessions
        .sessions
        .iter()
        .map(|profile| ConnectableSessionRow {
            id: profile.id.to_string().into(),
            name: profile.name.clone().into(),
            endpoint: profile_sidebar_endpoint(
                profile,
                &sessions.settings.workspace.session_mask_character,
            )
            .into(),
        })
        .collect()
}

pub(super) fn group_option_rows(sessions: &SessionStore) -> Vec<SharedString> {
    group_options(sessions)
        .into_iter()
        .map(SharedString::from)
        .collect()
}

pub(super) fn shell_option_rows(settings: &AppSettings) -> Vec<SharedString> {
    settings
        .terminal
        .known_shells
        .iter()
        .cloned()
        .map(SharedString::from)
        .collect()
}

pub(super) fn refresh_session_models(ui: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let (rows, groups, options) = match state.lock() {
        Ok(app) => (
            session_rows(&app.sessions, &app.expanded_groups),
            group_option_rows(&app.sessions),
            connection_option_rows(&app.sessions),
        ),
        Err(_) => {
            set_status(ui, "State lock poisoned");
            return;
        }
    };
    dispatch_ui(ui, move |ui| {
        ui.set_sessions(ModelRc::new(VecModel::from(rows)));
        ui.set_group_options(ModelRc::new(VecModel::from(groups)));
        ui.set_connection_options(ModelRc::new(VecModel::from(options)));
    });
}

pub(super) fn refresh_workspace(ui: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let state = Arc::clone(state);
    dispatch_ui(ui, move |ui| {
        let (tabs, snapshot) = match state.lock() {
            Ok(app) => (
                visible_workspace_tab_rows(app.tab_summaries()),
                app.active_snapshot(),
            ),
            Err(_) => {
                ui.set_status("State lock poisoned".into());
                return;
            }
        };
        ui.set_workspace_tabs(ModelRc::new(VecModel::from(tabs)));
        apply_active_snapshot(ui, snapshot);
    });
}

pub(super) fn visible_workspace_tab_rows(tabs: Vec<WorkspaceTabSummary>) -> Vec<WorkspaceTabRow> {
    tabs.into_iter()
        .map(|tab| WorkspaceTabRow {
            id: tab.id.to_string().into(),
            title: tab.title.into(),
            kind: tab.kind.into(),
            connected: tab.connected,
        })
        .collect()
}

pub(super) fn set_tab_status(
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    tab_id: Uuid,
    message: &str,
) {
    let active = match state.lock() {
        Ok(mut app) => {
            let Some(terminal) = app.terminal_mut(tab_id) else {
                return;
            };
            terminal.status = message.to_owned();
            app.active_tab_id() == Some(tab_id)
        }
        Err(_) => {
            set_status(ui, "State lock poisoned");
            return;
        }
    };
    if active {
        dispatch_active_snapshot(ui, state);
    }
}

pub(super) fn dispatch_active_snapshot(ui: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let state = Arc::clone(state);
    dispatch_ui(ui, move |ui| {
        // Worker output and resize events can queue faster than the UI event loop runs.
        // Resolve the snapshot here so an older queued event cannot restore stale dimensions.
        let snapshot = match state.lock() {
            Ok(app) => app.active_snapshot(),
            Err(_) => {
                ui.set_status("State lock poisoned".into());
                return;
            }
        };
        apply_active_snapshot(ui, snapshot);
    });
}

pub(super) fn apply_active_snapshot(ui: &AppWindow, snapshot: ActiveTabSnapshot) {
    let active_tab_id = snapshot.id.map(|id| id.to_string()).unwrap_or_default();
    ui.set_active_tab_id(active_tab_id.into());
    ui.set_active_tab_kind(snapshot.kind.into());
    ui.set_active_tab_title(snapshot.title.into());
    ui.set_active_tab_status(snapshot.status.into());
    let terminal = snapshot.terminal.unwrap_or_else(empty_terminal_snapshot);
    let rendered = render_terminal(
        terminal,
        TerminalRenderSettings {
            color_scheme: TerminalColorScheme::from_setting(
                ui.get_terminal_color_scheme().as_str(),
            ),
            default_foreground: to_rgb_color(ui.get_theme_terminal_foreground()),
            default_background: to_rgb_color(ui.get_theme_terminal_background()),
            selection_background: to_rgb_color(ui.get_theme_terminal_selection()),
            brightness_percent: ui.get_terminal_brightness_percent().clamp(60, 140) as u16,
            bright_bold_text: ui.get_bright_bold_text(),
        },
    );
    apply_rendered_terminal(ui, rendered);
    ui.set_connected(snapshot.connected);
    ui.set_worker_running(snapshot.worker_running);
}

pub(super) fn apply_settings(ui: &slint::Weak<AppWindow>, settings: AppSettings) {
    dispatch_ui(ui, move |ui| apply_settings_to_component(ui, &settings));
}

pub(super) fn apply_settings_to_component(ui: &AppWindow, settings: &AppSettings) {
    apply_theme_to_component(ui, settings);
    ui.set_terminal_font_family(settings.appearance.terminal_font_family.clone().into());
    ui.set_terminal_font_size(i32::from(settings.appearance.terminal_font_size));
    ui.set_terminal_line_height_percent(i32::from(
        settings.appearance.terminal_line_height_percent,
    ));
    ui.set_terminal_brightness_percent(i32::from(settings.appearance.terminal_brightness_percent));
    ui.set_bright_bold_text(settings.appearance.bright_bold_text);
    ui.set_right_click_copy_or_paste(settings.appearance.right_click_copy_or_paste);
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
    ui.set_sidebar_width(i32::from(settings.workspace.sidebar_width));
    ui.set_tab_width(i32::from(settings.workspace.tab_width));
    ui.set_session_mask_character(settings.workspace.session_mask_character.clone().into());
    ui.set_open_settings_shortcut(settings.shortcuts.open_settings.clone().into());
    ui.set_toggle_sidebar_shortcut(settings.shortcuts.toggle_sidebar.clone().into());
    ui.set_copy_selection_shortcut(settings.shortcuts.copy_selection.clone().into());
    ui.set_paste_shortcut(settings.shortcuts.paste.clone().into());
}

fn apply_theme_to_component(ui: &AppWindow, settings: &AppSettings) {
    let light = settings.appearance.theme.light_palette();
    let dark = settings.appearance.theme.dark_palette();
    let theme = ui.global::<Theme>();
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

fn set_theme_palette(theme: &Theme, palette: &ThemePalette, light: bool) {
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

fn set_ui_theme_palette(ui: &AppWindow, palette: &ThemePalette, light: bool) {
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

pub(super) fn empty_terminal_snapshot() -> TerminalSnapshot {
    TerminalSnapshot {
        text: String::new(),
        lines: vec![Default::default()],
        max_columns: 0,
        cursor_row: 0,
        cursor_column: 0,
        cursor_visible: false,
        cursor_text: " ".to_owned(),
    }
}

pub(super) fn apply_rendered_terminal(ui: &AppWindow, rendered: terminal_render::RenderedTerminal) {
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

pub(super) fn terminal_render_line(line: RenderedTerminalLine) -> TerminalRenderLine {
    let runs = line
        .runs
        .into_iter()
        .map(terminal_render_run)
        .collect::<Vec<_>>();
    TerminalRenderLine {
        runs: ModelRc::new(VecModel::from(runs)),
    }
}

pub(super) fn terminal_render_run(run: RenderedTerminalRun) -> TerminalRenderRun {
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

pub(super) fn to_slint_color(color: RgbColor) -> Color {
    Color::from_rgb_u8(color.red, color.green, color.blue)
}

fn to_rgb_color(color: Color) -> RgbColor {
    let rgba = color.to_argb_u8();
    RgbColor::new(rgba.red, rgba.green, rgba.blue)
}

fn theme_color(value: &str) -> Color {
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

fn hex_byte(high: u8, low: u8) -> Option<u8> {
    let high = hex_digit(high)?;
    let low = hex_digit(low)?;
    Some(high * 16 + low)
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[derive(Clone, Copy)]
pub(super) enum Dialog {
    HostKey,
    Password,
}

pub(super) fn set_dialog_open(ui: &slint::Weak<AppWindow>, dialog: Dialog, open: bool) {
    dispatch_ui(ui, move |ui| match dialog {
        Dialog::HostKey => ui.set_host_key_dialog_open(open),
        Dialog::Password => ui.set_password_dialog_open(open),
    });
}

pub(super) fn show_host_key_prompt(ui: &slint::Weak<AppWindow>, prompt: &PendingHostKey) {
    let endpoint = format!("{}:{}", prompt.host, prompt.port);
    let fingerprint = prompt.fingerprint.clone();
    let changed = prompt.changed;
    dispatch_ui(ui, move |ui| {
        ui.set_host_key_endpoint(endpoint.into());
        ui.set_host_key_fingerprint(fingerprint.into());
        ui.set_host_key_changed(changed);
        ui.set_host_key_dialog_open(true);
    });
}

pub(super) fn show_auth_prompt(
    ui: &slint::Weak<AppWindow>,
    profile: &SessionProfile,
    remember_password: bool,
) {
    let endpoint = profile_endpoint(profile);
    let (private_key, key_path) = match &profile.auth {
        AuthMethod::Password => (false, String::new()),
        AuthMethod::PrivateKey { path } => (true, path.display().to_string()),
    };
    dispatch_ui(ui, move |ui| {
        ui.set_password_endpoint(endpoint.into());
        ui.set_password_remember_default(!private_key && remember_password);
        ui.set_password_private_key(private_key);
        ui.set_password_key_path(key_path.into());
        ui.set_password_dialog_open(true);
    });
}

pub(super) fn load_private_key_options(runtime: &Handle, ui: slint::Weak<AppWindow>) {
    runtime.spawn(async move {
        let result = tokio::task::spawn_blocking(discover_private_keys).await;
        match result {
            Ok(Ok(paths)) => {
                let options = paths
                    .into_iter()
                    .map(|path| SharedString::from(path.display().to_string()))
                    .collect::<Vec<_>>();
                dispatch_ui(&ui, move |ui| {
                    ui.set_private_key_options(ModelRc::new(VecModel::from(options)));
                });
            }
            Ok(Err(error)) => warn!(%error, "failed to discover local SSH private keys"),
            Err(error) => warn!(%error, "private-key discovery task failed"),
        }
    });
}

pub(super) fn parse_uuid(value: &str, label: &str, ui: &slint::Weak<AppWindow>) -> Option<Uuid> {
    match value.parse::<Uuid>() {
        Ok(id) => Some(id),
        Err(error) => {
            set_status(ui, &format!("Invalid {label} id: {error}"));
            None
        }
    }
}

pub(super) fn set_status(ui: &slint::Weak<AppWindow>, message: &str) {
    let message = message.to_owned();
    dispatch_ui(ui, move |ui| ui.set_status(message.into()));
}

pub(super) fn dispatch_ui(
    ui: &slint::Weak<AppWindow>,
    action: impl FnOnce(&AppWindow) + Send + 'static,
) {
    let ui = ui.clone();
    if slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui.upgrade() {
            action(&ui);
        }
    })
    .is_err()
    {
        debug!("Slint event loop is no longer available for UI update");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_workbench_is_exposed_as_a_workspace_tab() {
        let rows = visible_workspace_tab_rows(vec![
            WorkspaceTabSummary {
                id: Uuid::new_v4(),
                title: "Settings".to_owned(),
                kind: "settings",
                connected: false,
            },
            WorkspaceTabSummary {
                id: Uuid::new_v4(),
                title: "New session".to_owned(),
                kind: "session-editor",
                connected: false,
            },
        ]);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind.as_str(), "settings");
        assert_eq!(rows[1].kind.as_str(), "session-editor");
    }

    #[test]
    fn session_rows_group_profiles_and_respect_expansion() {
        let mut production_a = SessionProfile::new("prod-a", "a.example", "alice");
        production_a.group_name = " Production ".into();
        let mut production_b = SessionProfile::new("prod-b", "192.168.1.202", "zhushixin");
        production_b.group_name = "Production".into();
        let ungrouped = SessionProfile::new("local", "local.example", "carol");
        let sessions = SessionStore {
            sessions: vec![production_a, production_b, ungrouped],
            ..SessionStore::default()
        };
        let expanded_groups = BTreeSet::from(["Production".to_owned()]);
        let rows = session_rows(&sessions, &expanded_groups);

        assert_eq!(rows.len(), 4);
        assert!(rows[0].is_group);
        assert!(rows[0].expanded);
        assert_eq!(rows[0].name.as_str(), "Production");
        assert_eq!(rows[0].icon.as_str(), "Pr");
        assert_eq!(rows[0].endpoint.as_str(), "2");

        assert!(!rows[1].is_group);
        assert_eq!(rows[1].name.as_str(), "prod-a");
        assert_eq!(rows[1].endpoint.as_str(), "al*ce@a.example:22");
        assert!(!rows[2].is_group);
        assert_eq!(rows[2].name.as_str(), "prod-b");
        assert_eq!(rows[2].endpoint.as_str(), "zh*in@192.*.202:22");

        assert!(rows[3].is_group);
        assert!(!rows[3].expanded);
        assert_eq!(rows[3].name.as_str(), "Ungrouped");
        assert_eq!(rows[3].icon.as_str(), "Un");
        assert_eq!(rows[3].endpoint.as_str(), "1");
    }

    #[test]
    fn connection_options_include_collapsed_profiles_with_masked_endpoints() {
        let visible = SessionProfile::new("visible", "server.example", "alice");
        let hidden = SessionProfile::new("hidden", "192.168.1.202", "zhushixin");
        let sessions = SessionStore {
            sessions: vec![visible.clone(), hidden.clone()],
            ..SessionStore::default()
        };

        let options = connection_option_rows(&sessions);

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].id.as_str(), visible.id.to_string());
        assert_eq!(options[0].name.as_str(), "visible");
        assert_eq!(options[0].endpoint.as_str(), "al*ce@server.example:22");
        assert_eq!(options[1].id.as_str(), hidden.id.to_string());
        assert_eq!(options[1].name.as_str(), "hidden");
        assert_eq!(options[1].endpoint.as_str(), "zh*in@192.*.202:22");
    }
}
