//! Terminal panes application boundary.

use super::*;

pub(super) fn close_terminal_notice_tab(
    router: &WindowRouter,
    window_id: Uuid,
    tab_id: Uuid,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    runtime: &Handle,
) -> bool {
    if close_terminal_child_pane(router, Some(window_id), tab_id, state, ui, runtime) {
        return true;
    }
    if !state.lock().ok().is_some_and(|app| {
        router.owns_terminal_pane(window_id, tab_id, &app)
            || router.tab_ids(window_id, &app).contains(&tab_id)
    }) {
        return false;
    }
    close_workspace_tab(tab_id, state, ui, runtime);
    true
}

pub(super) enum TerminalRetryRoute {
    Local,
    Profile {
        profile_id: Uuid,
        target: ConnectionTarget,
    },
}

#[allow(clippy::too_many_arguments)]
pub(super) fn retry_terminal_notice_tab(
    router: &WindowRouter,
    window_id: Uuid,
    tab_id: Uuid,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    runtime: &Handle,
    font_registry: &Arc<Mutex<FontRegistry>>,
    terminal_font_started: &Arc<std::sync::atomic::AtomicBool>,
) -> bool {
    let route = match state.lock() {
        Ok(mut app) => {
            if !router.owns_terminal_pane(window_id, tab_id, &app) {
                return false;
            }
            let Some(terminal) = app.terminal_mut(tab_id) else {
                return false;
            };
            if terminal.worker.is_some() || terminal.worker_running {
                return false;
            }
            terminal.prepare_manual_retry();
            if terminal.is_local() {
                TerminalRetryRoute::Local
            } else if let Some(profile_id) = terminal.profile_id() {
                TerminalRetryRoute::Profile {
                    profile_id,
                    target: terminal.connection_target(),
                }
            } else {
                return false;
            }
        }
        Err(_) => {
            set_status(ui, "Cannot read workspace state");
            return false;
        }
    };

    match route {
        TerminalRetryRoute::Local => {
            if let Err(error) =
                resume_existing_local_shell(runtime, state.clone(), ui.clone(), tab_id)
            {
                set_tab_status(
                    state,
                    ui,
                    tab_id,
                    &format!("Cannot restart terminal: {error}"),
                );
                return false;
            }
        }
        TerminalRetryRoute::Profile { profile_id, target } => {
            let connection = ConnectionContext::new(
                ui.clone(),
                state.clone(),
                runtime.clone(),
                font_registry.clone(),
                terminal_font_started.clone(),
            );
            resume_existing_connection(&connection, tab_id, profile_id, target);
        }
    }
    true
}

pub(super) fn wire_panes(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    font_registry: Arc<Mutex<FontRegistry>>,
    terminal_font_started: Arc<std::sync::atomic::AtomicBool>,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let ui_for_focus = ui.as_weak();
    let state_for_focus = state.clone();
    let router_for_focus = window_router.clone();
    ui.on_terminal_pane_focus(move |tab_id| {
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_focus) else {
            return;
        };
        let layout = state_for_focus
            .lock()
            .ok()
            .and_then(|mut app| router_for_focus.focus_terminal_pane(window_id, tab_id, &mut app));
        if let Some(layout) = layout {
            let applied_in_place = ui_for_focus
                .upgrade()
                .is_some_and(|ui| apply_terminal_pane_layout(&ui, layout));
            if !applied_in_place {
                refresh_workspace(&ui_for_focus, &state_for_focus);
            }
        }
    });

    let ui_for_divider = ui.as_weak();
    let state_for_divider = state.clone();
    let router_for_divider = window_router.clone();
    ui.on_resize_terminal_divider(move |divider_id, ratio| {
        let Some(layout) = router_for_divider.resize_terminal_divider(window_id, divider_id, ratio)
        else {
            return false;
        };
        let applied_in_place = ui_for_divider
            .upgrade()
            .is_some_and(|ui| apply_terminal_pane_layout(&ui, layout));
        if !applied_in_place {
            refresh_workspace(&ui_for_divider, &state_for_divider);
        }
        true
    });

    let ui_for_command = ui.as_weak();
    let state_for_command = state.clone();
    let runtime_for_command = runtime;
    let font_registry_for_command = font_registry;
    let terminal_font_started_for_command = terminal_font_started;
    let router_for_command = window_router;
    ui.on_terminal_pane_command(move |tab_id, command| {
        if state_for_command
            .lock()
            .is_ok_and(|app| router_for_command.workspace_actions_locked(window_id, &app))
        {
            return false;
        }
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_command) else {
            return false;
        };
        if matches!(
            command.as_str(),
            "paste-clipboard" | "microphone" | "dictation" | "discard-transcript"
        ) || command.as_str().starts_with("insert-transcript\n")
        {
            return super::terminal_media::handle_command(
                tab_id,
                command.as_str(),
                &state_for_command,
                &ui_for_command,
                &runtime_for_command,
                &router_for_command,
                window_id,
            );
        }
        if command.as_str() == "close" {
            return close_terminal_child_pane(
                &router_for_command,
                Some(window_id),
                tab_id,
                &state_for_command,
                &ui_for_command,
                &runtime_for_command,
            );
        }
        if command.as_str() == "close-tab" {
            return close_terminal_notice_tab(
                &router_for_command,
                window_id,
                tab_id,
                &state_for_command,
                &ui_for_command,
                &runtime_for_command,
            );
        }
        if command.as_str() == "retry" {
            return retry_terminal_notice_tab(
                &router_for_command,
                window_id,
                tab_id,
                &state_for_command,
                &ui_for_command,
                &runtime_for_command,
                &font_registry_for_command,
                &terminal_font_started_for_command,
            );
        }
        if command.as_str() == "allow-osc52-clipboard-read" {
            return handle_osc52_clipboard_read(
                &router_for_command,
                window_id,
                tab_id,
                &state_for_command,
                &ui_for_command,
            );
        }
        if command.as_str() == "deny-osc52-clipboard-read" {
            return handle_osc52_clipboard_deny(
                &router_for_command,
                window_id,
                tab_id,
                &state_for_command,
                &ui_for_command,
            );
        }
        let Some((direction, action)) = PaneDirection::from_command(command.as_str()) else {
            return false;
        };
        match action {
            PaneCommand::Focus => {
                let layout = state_for_command.lock().ok().and_then(|mut app| {
                    router_for_command
                        .focus_terminal_pane(window_id, tab_id, &mut app)
                        .and_then(|_| {
                            router_for_command.focus_pane_direction(window_id, direction, &mut app)
                        })
                });
                let Some(layout) = layout else {
                    return false;
                };
                let applied_in_place = ui_for_command
                    .upgrade()
                    .is_some_and(|ui| apply_terminal_pane_layout(&ui, layout));
                if !applied_in_place {
                    refresh_workspace(&ui_for_command, &state_for_command);
                }
                true
            }
            PaneCommand::Split => {
                let source = match state_for_command.lock() {
                    Ok(mut app) => {
                        if router_for_command.prepare_pane_split(window_id, tab_id, &mut app) {
                            app.pane_session_source(tab_id)
                        } else {
                            None
                        }
                    }
                    Err(_) => {
                        set_status(&ui_for_command, "Cannot read workspace state");
                        None
                    }
                };
                let Some(source) = source else {
                    return false;
                };
                let new_tab_id = match source {
                    PaneSessionSource::LocalShell => {
                        load_terminal_font_on_demand(
                            &runtime_for_command,
                            ui_for_command.clone(),
                            state_for_command.clone(),
                            font_registry_for_command.clone(),
                            terminal_font_started_for_command.clone(),
                        );
                        match start_local_shell(
                            &runtime_for_command,
                            state_for_command.clone(),
                            ui_for_command.clone(),
                            {
                                let router = router_for_command.clone();
                                move |new_tab_id, app| {
                                    router.complete_pane_split(
                                        window_id, tab_id, direction, new_tab_id, app,
                                    )
                                }
                            },
                        ) {
                            Ok(tab_id) => Some(tab_id),
                            Err(error) => {
                                set_status(
                                    &ui_for_command,
                                    &format!("Cannot create terminal pane: {error}"),
                                );
                                None
                            }
                        }
                    }
                    PaneSessionSource::ProfileConnection(profile_id) => {
                        let connection = ConnectionContext::new(
                            ui_for_command.clone(),
                            state_for_command.clone(),
                            runtime_for_command.clone(),
                            font_registry_for_command.clone(),
                            terminal_font_started_for_command.clone(),
                        );
                        request_profile_connection(
                            &connection,
                            profile_id,
                            ConnectionTarget::Terminal,
                            None,
                            None,
                            None,
                            {
                                let router = router_for_command.clone();
                                move |new_tab_id, app| {
                                    router.complete_pane_split(
                                        window_id, tab_id, direction, new_tab_id, app,
                                    )
                                }
                            },
                        )
                    }
                };
                let Some(_) = new_tab_id else {
                    return false;
                };
                true
            }
        }
    });
}
