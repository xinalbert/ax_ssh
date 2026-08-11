use super::*;

pub(in crate::app) fn load_private_key_options(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
) {
    let generation = PRIVATE_KEY_OPTION_GENERATION
        .fetch_add(1, Ordering::AcqRel)
        .wrapping_add(1);
    runtime.spawn(async move {
        let result = tokio::task::spawn_blocking(discover_private_keys).await;
        match result {
            Ok(Ok(paths)) => {
                let options = paths
                    .into_iter()
                    .map(|path| SharedString::from(path.display().to_string()))
                    .collect::<Vec<_>>();
                dispatch_ui(&ui, move |ui| {
                    if PRIVATE_KEY_OPTION_GENERATION.load(Ordering::Acquire) != generation
                        || !state.lock().is_ok_and(|app| app.has_session_editor_tab())
                    {
                        return;
                    }
                    ui.set_private_key_options(ModelRc::new(VecModel::from(options)));
                });
            }
            Ok(Err(error)) => warn!(%error, "failed to discover local SSH private keys"),
            Err(error) => warn!(%error, "private-key discovery task failed"),
        }
    });
}

pub(in crate::app) fn load_font_options(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
) {
    runtime.spawn(async move {
        let discovery = tokio::task::spawn_blocking(discover_system_monospace_families).await;
        match discovery {
            Ok(system_families) => dispatch_ui(&ui, move |ui| {
                if !state.lock().is_ok_and(|app| app.has_settings_tab()) {
                    return;
                }
                let application_font = ui.get_application_font_family().to_string();
                let application_options = font_option_rows(&application_font, &system_families);
                let application_index =
                    font_option_index_in_slice(&application_options, &application_font);
                ui.set_application_font_options(ModelRc::new(VecModel::from(application_options)));
                ui.set_application_font_index(application_index);

                let terminal_font = ui.get_terminal_font_family().to_string();
                let terminal_options = font_option_rows(&terminal_font, &system_families);
                let terminal_index = font_option_index_in_slice(&terminal_options, &terminal_font);
                ui.set_terminal_font_options(ModelRc::new(VecModel::from(terminal_options)));
                ui.set_terminal_font_index(terminal_index);
            }),
            Err(error) => warn!(%error, "system monospace font discovery task failed"),
        }
    });
}

pub(in crate::app) fn load_x11_server_installations(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
) {
    runtime.spawn(async move {
        let discovery =
            tokio::task::spawn_blocking(ax_ssh::x_server::discovered_provider_locations);
        match tokio::time::timeout(std::time::Duration::from_secs(3), discovery).await {
            Ok(Ok(locations)) => dispatch_ui(&ui, move |ui| {
                if !state.lock().is_ok_and(|app| app.has_settings_tab()) {
                    return;
                }
                ui.set_x11_server_installations(ModelRc::new(VecModel::from(
                    locations
                        .into_iter()
                        .map(SharedString::from)
                        .collect::<Vec<_>>(),
                )));
            }),
            Ok(Err(error)) => warn!(%error, "X server location discovery task failed"),
            Err(_) => warn!("X server location discovery timed out"),
        }
    });
}

pub(in crate::app) fn load_local_shell_options(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
) {
    runtime.spawn(async move {
        let shells = match tokio::task::spawn_blocking(discover_shells).await {
            Ok(shells) => shells,
            Err(error) => {
                warn!(%error, "local-shell discovery task failed");
                return;
            }
        };
        let options = match state.lock() {
            Ok(mut app) if app.has_settings_tab() => {
                app.sessions.settings.terminal.merge_known_shells(shells);
                shell_option_rows(&app.sessions.settings)
            }
            Ok(_) => return,
            Err(_) => {
                set_status(&ui, "Cannot update local shell options");
                return;
            }
        };
        dispatch_ui(&ui, move |ui| {
            if !state.lock().is_ok_and(|app| app.has_settings_tab()) {
                return;
            }
            ui.set_local_shell_options(ModelRc::new(VecModel::from(options)));
            let selected = ui.get_local_shell().to_string();
            let index = ui
                .get_local_shell_options()
                .iter()
                .position(|shell| shell.as_str().eq_ignore_ascii_case(&selected))
                .unwrap_or(0);
            ui.set_local_shell_index(index.min(i32::MAX as usize) as i32);
        });
    });
}

pub(in crate::app) fn clear_settings_option_models(
    ui: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
) {
    let (application_font, terminal_font, shell_options) = match state.lock() {
        Ok(app) => (
            app.sessions
                .settings
                .appearance
                .application_font_family
                .clone(),
            app.sessions
                .settings
                .appearance
                .terminal_font_family
                .clone(),
            shell_option_rows(&app.sessions.settings),
        ),
        Err(_) => return,
    };
    dispatch_ui(ui, move |ui| {
        let application_options = font_option_rows(&application_font, &[]);
        let application_index = font_option_index_in_slice(&application_options, &application_font);
        ui.set_application_font_options(ModelRc::new(VecModel::from(application_options)));
        ui.set_application_font_index(application_index);

        let terminal_options = font_option_rows(&terminal_font, &[]);
        let terminal_index = font_option_index_in_slice(&terminal_options, &terminal_font);
        ui.set_terminal_font_options(ModelRc::new(VecModel::from(terminal_options)));
        ui.set_terminal_font_index(terminal_index);
        ui.set_local_shell_options(ModelRc::new(VecModel::from(shell_options)));
        ui.set_x11_server_installations(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    });
}

pub(in crate::app) fn clear_session_editor_option_models(ui: &slint::Weak<AppWindow>) {
    invalidate_private_key_option_load();
    dispatch_ui(ui, move |ui| {
        ui.set_private_key_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
        ui.set_serial_port_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    });
}

pub(in crate::app) fn clear_private_key_option_model(ui: &slint::Weak<AppWindow>) {
    invalidate_private_key_option_load();
    dispatch_ui(ui, move |ui| {
        ui.set_private_key_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    });
}

pub(super) fn invalidate_private_key_option_load() {
    PRIVATE_KEY_OPTION_GENERATION.fetch_add(1, Ordering::AcqRel);
}

pub(super) fn font_option_index(options: &ModelRc<SharedString>, selected: &str) -> i32 {
    font_option_index_in_slice(&options.iter().collect::<Vec<_>>(), selected)
}

pub(super) fn font_option_index_in_slice(options: &[SharedString], selected: &str) -> i32 {
    options
        .iter()
        .position(|font| font.as_str().eq_ignore_ascii_case(selected))
        .unwrap_or(0)
        .min(i32::MAX as usize) as i32
}

pub(in crate::app) fn parse_uuid(
    value: &str,
    label: &str,
    ui: &slint::Weak<AppWindow>,
) -> Option<Uuid> {
    match value.parse::<Uuid>() {
        Ok(id) => Some(id),
        Err(error) => {
            set_status(ui, &format!("Invalid {label} id: {error}"));
            None
        }
    }
}

pub(in crate::app) fn set_status(ui: &slint::Weak<AppWindow>, message: &str) {
    let message = message.to_owned();
    dispatch_ui(ui, move |ui| ui.set_status(message.into()));
}

pub(in crate::app) fn dispatch_ui(
    ui: &slint::Weak<AppWindow>,
    action: impl FnOnce(&AppWindow) + Send + 'static,
) {
    let _ = dispatch_ui_result(ui, action);
}

pub(in crate::app) fn dispatch_ui_result(
    ui: &slint::Weak<AppWindow>,
    action: impl FnOnce(&AppWindow) + Send + 'static,
) -> bool {
    let ui = ui.clone();
    if slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui.upgrade() {
            action(&ui);
        }
    })
    .is_err()
    {
        debug!("Slint event loop is no longer available for UI update");
        false
    } else {
        true
    }
}
