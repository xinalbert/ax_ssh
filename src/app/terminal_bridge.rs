use super::*;
use crate::app::state::PaneSessionSource;

pub(super) fn start_local_shell(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    register_tab: impl FnOnce(Uuid, &mut AppState) -> bool,
) -> Result<Uuid> {
    let (tab_id, events) = {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        let shell = app.sessions.settings.terminal.local_shell.clone();
        let columns = u32::from(app.sessions.settings.terminal.default_columns);
        let rows = u32::from(app.sessions.settings.terminal.default_rows);
        let tab_id = app.open_local_shell_tab();
        if !register_tab(tab_id, &mut app) {
            let _ = app.close_tab(tab_id);
            anyhow::bail!("cannot attach local shell to the requested terminal pane");
        }
        let (worker, events) = LocalShellHandle::spawn(shell, columns, rows);
        let terminal = app
            .terminal_mut(tab_id)
            .context("local terminal tab disappeared while starting worker")?;
        terminal.worker = Some(TerminalWorker::Local(worker));
        (tab_id, events)
    };
    refresh_workspace(&ui, &state);
    spawn_local_shell_monitor(runtime, state, ui, tab_id, events);
    Ok(tab_id)
}

pub(super) fn wire_terminal(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    font_registry: Arc<Mutex<FontRegistry>>,
    terminal_font_started: Arc<std::sync::atomic::AtomicBool>,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let ui_for_theme = ui.as_weak();
    let state_for_theme = state.clone();
    ui.on_refresh_terminal_appearance(move || {
        log_ui_action("terminal.refresh-appearance");
        // A theme change only changes the visual snapshot; it must not resize or
        // otherwise disturb the PTY worker that owns the active terminal.
        dispatch_active_snapshot(&ui_for_theme, &state_for_theme);
    });

    let ui_for_key = ui.as_weak();
    let state_for_key = state.clone();
    let router_for_key = window_router.clone();
    ui.on_terminal_key(
        move |tab_id, text, alt, control, meta, shift, physical_key_event| {
            let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_key) else {
                return true;
            };
            let belongs_to_pane = state_for_key
                .lock()
                .is_ok_and(|app| router_for_key.owns_terminal_pane(window_id, tab_id, &app));
            if !belongs_to_pane {
                return true;
            }
            let input_started_at = std::time::Instant::now();
            // Committed TextInput and pasted text are not physical key events, so
            // they must not inherit a still-held shortcut modifier such as Cmd+V.
            let mut modifiers =
                terminal_input_modifiers(alt, control, meta, shift, physical_key_event);
            let key = terminal_key_from_slint(text.as_str(), modifiers);
            log_terminal_input(&key, modifiers, physical_key_event);
            let result = state_for_key
                .lock()
                .map_err(|_| anyhow::anyhow!("state lock poisoned"))
                .and_then(|mut app| {
                    if cfg!(target_os = "macos") && !app.sessions.settings.terminal.option_as_meta {
                        modifiers.alt = false;
                    }
                    let terminal = app.terminal(tab_id).context("terminal tab not found")?;
                    if !terminal.connected {
                        return Ok((false, false));
                    }
                    let application_cursor = terminal
                        .terminal
                        .as_ref()
                        .context("active tab has no terminal model")?
                        .application_cursor();
                    let Some(data) = encode_terminal_key(&key, modifiers, application_cursor)
                    else {
                        return Ok((false, false));
                    };
                    let viewport_changed = {
                        let terminal =
                            app.terminal_mut(tab_id).context("terminal tab not found")?;
                        let viewport_changed = terminal
                            .terminal
                            .as_mut()
                            .context("active tab has no terminal model")?
                            .scroll_to_bottom();
                        terminal
                            .worker
                            .as_ref()
                            .context("active terminal has no worker")?
                            .request_send(data)?;
                        viewport_changed
                    };
                    Ok((true, viewport_changed))
                });
            match result {
                Ok((handled, true)) => {
                    log_terminal_input_latency("handled-and-scrolled", input_started_at.elapsed());
                    log_ui_action_outcome("terminal.send-input", "handled-and-scrolled");
                    dispatch_active_snapshot(&ui_for_key, &state_for_key);
                    handled
                }
                Ok((handled, false)) => {
                    log_terminal_input_latency(
                        if handled { "handled" } else { "ignored" },
                        input_started_at.elapsed(),
                    );
                    log_ui_action_outcome(
                        "terminal.send-input",
                        if handled { "handled" } else { "ignored" },
                    );
                    handled
                }
                Err(error) => {
                    log_terminal_input_latency("error", input_started_at.elapsed());
                    log_ui_action_outcome("terminal.send-input", "error");
                    debug!(%error, "terminal input failed");
                    set_status(&ui_for_key, &format!("Cannot send terminal input: {error}"));
                    true
                }
            }
        },
    );

    let ui_for_resize = ui.as_weak();
    let state_for_resize = state.clone();
    let router_for_resize = window_router.clone();
    ui.on_resize_terminal(move |tab_id, columns, rows| {
        log_ui_action("terminal.resize");
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_resize) else {
            return;
        };
        let result = state_for_resize
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))
            .and_then(|mut app| {
                if !router_for_resize.owns_terminal_pane(window_id, tab_id, &app) {
                    anyhow::bail!("terminal pane is no longer visible in this window");
                }
                let columns = columns.max(1) as u32;
                let rows = rows.max(1) as u32;
                app.resize_terminal(tab_id, columns, rows)
            });
        match result {
            Ok(()) => {
                log_ui_action_outcome("terminal.resize", "accepted");
                dispatch_active_snapshot(&ui_for_resize, &state_for_resize);
            }
            Err(error) => {
                log_ui_action_outcome("terminal.resize", "ignored");
                debug!(%error, "terminal resize ignored");
                set_status(&ui_for_resize, &format!("Cannot resize terminal: {error}"));
            }
        }
    });

    let ui_for_scroll = ui.as_weak();
    let state_for_scroll = state.clone();
    let router_for_scroll = window_router.clone();
    ui.on_scroll_terminal(move |tab_id, lines| {
        log_ui_action("terminal.scroll");
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_scroll) else {
            return;
        };
        let changed = state_for_scroll
            .lock()
            .ok()
            .and_then(|mut app| {
                if !router_for_scroll.owns_terminal_pane(window_id, tab_id, &app) {
                    return None;
                }
                app.terminal_mut(tab_id)
                    .and_then(|terminal| terminal.terminal.as_mut())
                    .map(|terminal| terminal.scroll(lines))
            })
            .unwrap_or(false);
        if changed {
            log_ui_action_outcome("terminal.scroll", "changed");
            dispatch_active_snapshot(&ui_for_scroll, &state_for_scroll);
        } else {
            log_ui_action_outcome("terminal.scroll", "unchanged");
        }
    });

    let ui_for_selection = ui.as_weak();
    let state_for_selection = state.clone();
    let router_for_selection = window_router.clone();
    ui.on_terminal_selection_text(
        move |tab_id, anchor_row, anchor_column, focus_row, focus_column| {
            log_ui_action("terminal.selection-read");
            let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_selection) else {
                return SharedString::default();
            };
            state_for_selection
                .lock()
                .ok()
                .and_then(|app| {
                    if !router_for_selection.owns_terminal_pane(window_id, tab_id, &app) {
                        return None;
                    }
                    app.terminal(tab_id)
                        .and_then(|terminal| terminal.terminal.as_ref())
                        .map(|terminal| {
                            terminal.selection_text(
                                anchor_row.max(0) as usize,
                                anchor_column.max(0) as usize,
                                focus_row.max(0) as usize,
                                focus_column.max(0) as usize,
                            )
                        })
                })
                .unwrap_or_default()
                .into()
        },
    );

    let ui_for_focus = ui.as_weak();
    let state_for_focus = state.clone();
    let router_for_focus = window_router.clone();
    ui.on_terminal_pane_focus(move |tab_id| {
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_focus) else {
            return;
        };
        let focused = state_for_focus
            .lock()
            .is_ok_and(|mut app| router_for_focus.focus_terminal_pane(window_id, tab_id, &mut app));
        if focused {
            refresh_workspace(&ui_for_focus, &state_for_focus);
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
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_command) else {
            return false;
        };
        let Some((direction, action)) = PaneDirection::from_command(command.as_str()) else {
            return false;
        };
        match action {
            PaneCommand::Focus => {
                let focused = state_for_command.lock().is_ok_and(|mut app| {
                    router_for_command.focus_terminal_pane(window_id, tab_id, &mut app)
                        && router_for_command.focus_pane_direction(window_id, direction, &mut app)
                });
                if focused {
                    refresh_workspace(&ui_for_command, &state_for_command);
                }
                focused
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
                    PaneSessionSource::ProfileConnection(profile_id) => request_profile_connection(
                        &ui_for_command,
                        &state_for_command,
                        &runtime_for_command,
                        font_registry_for_command.clone(),
                        terminal_font_started_for_command.clone(),
                        profile_id,
                        ConnectionTarget::Terminal,
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
                    ),
                };
                let Some(_) = new_tab_id else {
                    return false;
                };
                true
            }
        }
    });
}

pub(super) fn spawn_local_shell_monitor(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    mut events: mpsc::Receiver<LocalShellEvent>,
) {
    runtime.spawn(async move {
        let mut terminal_event = false;
        while let Some(event) = events.recv().await {
            match event {
                LocalShellEvent::Started { shell } => {
                    let Some(active) = mutate_local_terminal(&state, tab_id, |terminal| {
                        terminal.connected = true;
                        terminal.worker_running = true;
                        terminal.status = format!("Local shell: {shell}");
                    }) else {
                        continue;
                    };
                    info!(tab_id = %tab_id, shell = %shell, "local shell started");
                    if active {
                        dispatch_active_snapshot(&ui, &state);
                    }
                    refresh_workspace(&ui, &state);
                }
                LocalShellEvent::Output(data) => {
                    if let Some(true) = mutate_local_terminal(&state, tab_id, |terminal| {
                        if let Some(model) = terminal.terminal.as_mut() {
                            model.process(&data);
                        }
                    }) {
                        dispatch_active_snapshot(&ui, &state);
                    }
                }
                // The UI updates its terminal snapshot as soon as this resize request is accepted.
                // Ignoring this later acknowledgement prevents a stale worker event from reverting it.
                LocalShellEvent::Resized { .. } => {}
                LocalShellEvent::Exited { status } => {
                    terminal_event = true;
                    if finish_local_terminal(
                        &state,
                        tab_id,
                        &format!("Local shell exited: {status}"),
                    ) {
                        refresh_workspace(&ui, &state);
                    }
                }
                LocalShellEvent::Failed(message) => {
                    terminal_event = true;
                    if finish_local_terminal(
                        &state,
                        tab_id,
                        &format!("Local shell failed: {message}"),
                    ) {
                        refresh_workspace(&ui, &state);
                    }
                }
            }
        }
        if !terminal_event && finish_local_terminal(&state, tab_id, "Local shell worker stopped") {
            refresh_workspace(&ui, &state);
        }
        debug!(tab_id = %tab_id, "local shell event monitor stopped");
    });
}

pub(super) fn mutate_local_terminal(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    action: impl FnOnce(&mut TerminalTabState),
) -> Option<bool> {
    let mut app = state.lock().ok()?;
    if !app.terminal(tab_id).is_some_and(TerminalTabState::is_local) {
        return None;
    }
    action(app.terminal_mut(tab_id)?);
    Some(true)
}

pub(super) fn finish_local_terminal(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    status: &str,
) -> bool {
    match state.lock() {
        Ok(mut app) if app.terminal(tab_id).is_some_and(TerminalTabState::is_local) => {
            if let Some(terminal) = app.terminal_mut(tab_id) {
                terminal.worker = None;
                terminal.connected = false;
                terminal.worker_running = false;
                terminal.status = status.to_owned();
            }
            true
        }
        Ok(_) | Err(_) => false,
    }
}
