use super::*;

pub(in crate::app) fn wire_workspace_tabs(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    font_registry: Arc<Mutex<FontRegistry>>,
    terminal_font_started: Arc<std::sync::atomic::AtomicBool>,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let ui_for_settings = ui.as_weak();
    let state_for_settings = state.clone();
    let runtime_for_settings = runtime.clone();
    let router_for_settings = window_router.clone();
    ui.on_open_settings(move || {
        log_ui_action("workspace.open-settings");
        let load_options = match state_for_settings.lock() {
            Ok(mut app) => {
                if router_for_settings.workspace_actions_locked(window_id, &app) {
                    return;
                }
                let load_options = !app.has_settings_tab();
                let tab_id = app.open_settings_tab();
                let _ = router_for_settings.activate_tab(window_id, tab_id, &mut app);
                load_options
            }
            Err(_) => {
                set_status(&ui_for_settings, "Cannot update workspace tabs");
                return;
            }
        };
        refresh_workspace(&ui_for_settings, &state_for_settings);
        if load_options {
            load_settings_option_models(
                &runtime_for_settings,
                state_for_settings.clone(),
                ui_for_settings.clone(),
            );
        }
    });

    let ui_for_new = ui.as_weak();
    let state_for_new = state.clone();
    let router_for_new = window_router.clone();
    ui.on_new_session(move || {
        log_ui_action("workspace.new-session");
        match state_for_new.lock() {
            Ok(mut app) => {
                if router_for_new.workspace_actions_locked(window_id, &app) {
                    return;
                }
                let tab_id = app.open_session_editor_tab();
                let _ = router_for_new.activate_tab(window_id, tab_id, &mut app);
            }
            Err(_) => {
                set_status(&ui_for_new, "Cannot update workspace tabs");
                return;
            }
        }
        refresh_workspace(&ui_for_new, &state_for_new);
    });

    let ui_for_new_in_group = ui.as_weak();
    let state_for_new_in_group = state.clone();
    let router_for_new_in_group = window_router.clone();
    ui.on_new_session_in_group(move |group_name| {
        log_ui_action("workspace.new-session-in-group");
        match state_for_new_in_group.lock() {
            Ok(mut app) => {
                if router_for_new_in_group.workspace_actions_locked(window_id, &app) {
                    return;
                }
                let tab_id = app.open_session_editor_for_group(group_name.as_str());
                let _ = router_for_new_in_group.activate_tab(window_id, tab_id, &mut app);
            }
            Err(_) => {
                set_status(&ui_for_new_in_group, "Cannot update workspace tabs");
                return;
            }
        }
        refresh_workspace(&ui_for_new_in_group, &state_for_new_in_group);
    });

    let ui_for_edit = ui.as_weak();
    let state_for_edit = state.clone();
    let router_for_edit = window_router.clone();
    ui.on_edit_session(move |id| {
        log_ui_action("workspace.edit-session");
        let id = match parse_uuid(id.as_str(), "session", &ui_for_edit) {
            Some(id) => id,
            None => return,
        };
        let opened = state_for_edit.lock().is_ok_and(|mut app| {
            if router_for_edit.workspace_actions_locked(window_id, &app) {
                return false;
            }
            if !app.open_session_editor_for_profile(id) {
                return false;
            }
            let Some(tab_id) = app.active_tab_id() else {
                return false;
            };
            router_for_edit.activate_tab(window_id, tab_id, &mut app)
        });
        if !opened {
            set_status(&ui_for_edit, "Session not found");
            return;
        }
        refresh_workspace(&ui_for_edit, &state_for_edit);
    });

    let ui_for_local = ui.as_weak();
    let state_for_local = state.clone();
    let runtime_for_local = runtime.clone();
    let font_registry_for_local = font_registry.clone();
    let terminal_font_started_for_local = terminal_font_started.clone();
    let router_for_local = window_router.clone();
    ui.on_open_local_shell(move || {
        log_ui_action("workspace.open-local-shell");
        if state_for_local
            .lock()
            .is_ok_and(|app| router_for_local.workspace_actions_locked(window_id, &app))
        {
            return;
        }
        load_terminal_font_on_demand(
            &runtime_for_local,
            ui_for_local.clone(),
            state_for_local.clone(),
            font_registry_for_local.clone(),
            terminal_font_started_for_local.clone(),
        );
        match start_local_shell(
            &runtime_for_local,
            state_for_local.clone(),
            ui_for_local.clone(),
            {
                let router = router_for_local.clone();
                move |tab_id, app| router.activate_tab(window_id, tab_id, app)
            },
        ) {
            Ok(_) => {}
            Err(error) => {
                set_status(&ui_for_local, &format!("Cannot open local shell: {error}"));
            }
        }
    });

    let ui_for_activate = ui.as_weak();
    let state_for_activate = state.clone();
    let router_for_activate = window_router.clone();
    ui.on_activate_tab(move |id| {
        log_ui_action("workspace.activate-tab");
        let id = match parse_uuid(id.as_str(), "tab", &ui_for_activate) {
            Some(id) => id,
            None => return,
        };
        let activated = state_for_activate
            .lock()
            .is_ok_and(|mut app| router_for_activate.activate_tab(window_id, id, &mut app));
        if !activated {
            set_status(&ui_for_activate, "Tab not found");
            return;
        }
        refresh_workspace(&ui_for_activate, &state_for_activate);
    });

    let ui_for_cycle = ui.as_weak();
    let state_for_cycle = state.clone();
    let router_for_cycle = window_router.clone();
    ui.on_cycle_tab(move |next| {
        log_ui_action(if next {
            "workspace.next-tab"
        } else {
            "workspace.previous-tab"
        });
        let cycled = match state_for_cycle.lock() {
            Ok(mut app) => router_for_cycle.cycle_tab(window_id, next, &mut app),
            Err(_) => {
                set_status(&ui_for_cycle, "Cannot update workspace tabs");
                return;
            }
        };
        if cycled {
            refresh_workspace(&ui_for_cycle, &state_for_cycle);
        }
    });

    let ui_for_move = ui.as_weak();
    let state_for_move = state.clone();
    let router_for_move = window_router.clone();
    ui.on_move_tab(move |id, target_index| {
        log_ui_action("workspace.move-tab");
        let id = match parse_uuid(id.as_str(), "tab", &ui_for_move) {
            Some(id) => id,
            None => return,
        };
        let moved = state_for_move.lock().is_ok_and(|mut app| {
            if router_for_move.workspace_actions_locked(window_id, &app) {
                return false;
            }
            let tab_ids = router_for_move.tab_ids(window_id, &app);
            app.move_tab_for(id, target_index.max(0) as usize, &tab_ids)
        });
        if !moved {
            set_status(&ui_for_move, "Tab not found");
            return;
        }
        refresh_workspace(&ui_for_move, &state_for_move);
    });

    let ui_for_close = ui.as_weak();
    let state_for_close = state.clone();
    let runtime_for_close = runtime.clone();
    let router_for_close = window_router.clone();
    ui.on_close_tab(move |id| {
        log_ui_action("workspace.close-tab");
        let id = match parse_uuid(id.as_str(), "tab", &ui_for_close) {
            Some(id) => id,
            None => return,
        };
        if state_for_close
            .lock()
            .is_ok_and(|app| router_for_close.workspace_actions_locked(window_id, &app))
        {
            return;
        }
        close_workspace_tab(id, &state_for_close, &ui_for_close, &runtime_for_close);
    });

    let ui_for_cancel_editor = ui.as_weak();
    let state_for_cancel_editor = state;
    ui.on_cancel_session_dialog(move || {
        log_ui_action("session-editor.cancel");
        let active_id = state_for_cancel_editor
            .lock()
            .ok()
            .and_then(|app| app.active_tab_id());
        if let Some(active_id) = active_id {
            close_workspace_tab(
                active_id,
                &state_for_cancel_editor,
                &ui_for_cancel_editor,
                &runtime,
            );
        }
    });
}

pub(in crate::app) fn close_workspace_tab(
    tab_id: Uuid,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    runtime: &Handle,
) {
    let tab_ids = global_window_router()
        .map(|router| router.take_workspace_tab_ids(tab_id))
        .unwrap_or_else(|| vec![tab_id]);
    let closed_tabs = match state.lock() {
        Ok(mut app) => tab_ids
            .into_iter()
            .filter_map(|closed_tab_id| {
                app.close_tab(closed_tab_id)
                    .map(|closed| (closed_tab_id, closed))
            })
            .collect::<Vec<_>>(),
        Err(_) => {
            set_status(ui, "Cannot update workspace tabs");
            return;
        }
    };
    if closed_tabs.is_empty() {
        set_status(ui, "Tab not found");
        return;
    }
    release_closed_tabs(closed_tabs, state, ui, runtime);
    refresh_workspace(ui, state);
}

pub(in crate::app) fn close_terminal_child_pane(
    router: &WindowRouter,
    window_id: Option<Uuid>,
    tab_id: Uuid,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    runtime: &Handle,
) -> bool {
    let closed = match state.lock() {
        Ok(mut app) => router.remove_terminal_child_pane(window_id, tab_id, &mut app),
        Err(_) => {
            set_status(ui, "Cannot update terminal panes");
            return false;
        }
    };
    let Some(closed) = closed else {
        return false;
    };
    release_closed_tabs(vec![(tab_id, closed)], state, ui, runtime);
    refresh_workspace(ui, state);
    true
}

fn release_closed_tabs(
    closed_tabs: Vec<(Uuid, ClosedTab)>,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    runtime: &Handle,
) {
    for (closed_tab_id, closed) in closed_tabs {
        match closed.kind {
            ClosedTabKind::Settings => clear_settings_option_models(ui, state),
            ClosedTabKind::SessionEditor => clear_session_editor_resources(ui),
            ClosedTabKind::Terminal {
                release_file_icon_cache: true,
            } => clear_file_icon_cache(),
            ClosedTabKind::Terminal {
                release_file_icon_cache: false,
            } => {}
        }
        if let Some(probe) = closed.pending_probe
            && probe.cancel.send(()).is_err()
        {
            debug!(tab_id = %closed_tab_id, "host-key probe already stopped while closing tab");
        }
        if let Some(worker) = closed.worker {
            let ui = ui.clone();
            runtime.spawn(async move {
                if let Err(error) = worker.shutdown().await {
                    warn!(tab_id = %closed_tab_id, %error, "failed to shut down closed tab worker");
                    set_status(
                        &ui,
                        &format!("Cannot close terminal worker cleanly: {error}"),
                    );
                }
            });
        }
    }
}

pub(super) fn clear_session_editor_resources(ui: &slint::Weak<AppWindow>) {
    invalidate_serial_port_discovery();
    clear_session_editor_option_models(ui);
}

fn load_settings_option_models(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
) {
    load_local_shell_options(runtime, state.clone(), ui.clone());
    load_font_options(runtime, state.clone(), ui.clone());
    load_x11_server_installations(runtime, state, ui);
}
