use super::*;
use crate::app::state::PaneSessionSource;
use crate::app::terminal_targets::{
    TerminalTarget, terminal_target_at_cell, terminal_target_span_at_cell,
};
use ax_ssh::terminal::{
    TerminalMouseButton, TerminalMouseEvent, TerminalMouseEventKind, TerminalMouseModifiers,
};

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

pub(super) fn resume_existing_local_shell(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
) -> Result<()> {
    let events = {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        let shell = app.sessions.settings.terminal.local_shell.clone();
        let columns = u32::from(app.sessions.settings.terminal.default_columns);
        let rows = u32::from(app.sessions.settings.terminal.default_rows);
        let terminal = app
            .terminal_mut(tab_id)
            .context("restored local tab disappeared")?;
        if terminal.worker.is_some() {
            return Ok(());
        }
        let (worker, events) = LocalShellHandle::spawn(shell.clone(), columns, rows);
        terminal.worker = Some(TerminalWorker::Local(worker));
        terminal.worker_running = true;
        terminal.connected = false;
        terminal.status = "Restored; starting local shell...".to_owned();
        events
    };
    refresh_workspace(&ui, &state);
    spawn_local_shell_monitor(runtime, state, ui, tab_id, events);
    Ok(())
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
            let input_started_at = std::time::Instant::now();
            let mut state_lock_elapsed = None;
            let mut worker_request_elapsed = None;
            // Committed TextInput and pasted text are not physical key events, so
            // they must not inherit a still-held shortcut modifier such as Cmd+V.
            let mut modifiers =
                terminal_input_modifiers(alt, control, meta, shift, physical_key_event);
            let key = terminal_key_from_slint(text.as_str(), modifiers);
            log_terminal_input(&key, modifiers, physical_key_event);
            let state_lock_started_at = std::time::Instant::now();
            let result = state_for_key
                .lock()
                .map_err(|_| anyhow::anyhow!("state lock poisoned"))
                .and_then(|mut app| {
                    state_lock_elapsed = Some(state_lock_started_at.elapsed());
                    if !router_for_key.owns_terminal_pane(window_id, tab_id, &app) {
                        return Ok((true, false));
                    }
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
                        let worker_request_started_at = std::time::Instant::now();
                        let request_result = terminal
                            .worker
                            .as_ref()
                            .context("active terminal has no worker")?
                            .request_send(data);
                        worker_request_elapsed = Some(worker_request_started_at.elapsed());
                        request_result?;
                        viewport_changed
                    };
                    Ok((true, viewport_changed))
                });
            match result {
                Ok((handled, true)) => {
                    log_terminal_input_latency(
                        "handled-and-scrolled",
                        input_started_at.elapsed(),
                        state_lock_elapsed,
                        worker_request_elapsed,
                    );
                    log_ui_action_outcome("terminal.send-input", "handled-and-scrolled");
                    dispatch_active_snapshot(&ui_for_key, &state_for_key);
                    handled
                }
                Ok((handled, false)) => {
                    log_terminal_input_latency(
                        if handled { "handled" } else { "ignored" },
                        input_started_at.elapsed(),
                        state_lock_elapsed,
                        worker_request_elapsed,
                    );
                    log_ui_action_outcome(
                        "terminal.send-input",
                        if handled { "handled" } else { "ignored" },
                    );
                    handled
                }
                Err(error) => {
                    log_terminal_input_latency(
                        "error",
                        input_started_at.elapsed(),
                        state_lock_elapsed,
                        worker_request_elapsed,
                    );
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

    let ui_for_mouse = ui.as_weak();
    let state_for_mouse = state.clone();
    let router_for_mouse = window_router.clone();
    ui.on_mouse_event(
        move |tab_id, row, column, button, kind, shift, alt, control| {
            let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_mouse) else {
                return;
            };
            let button = match button {
                0 => TerminalMouseButton::Left,
                1 => TerminalMouseButton::Middle,
                2 => TerminalMouseButton::Right,
                3 => TerminalMouseButton::WheelUp,
                4 => TerminalMouseButton::WheelDown,
                5 => TerminalMouseButton::None,
                _ => return,
            };
            let kind = match kind {
                0 => TerminalMouseEventKind::Press,
                1 => TerminalMouseEventKind::Release,
                2 => TerminalMouseEventKind::Motion,
                _ => return,
            };
            let result = state_for_mouse
                .lock()
                .map_err(|_| anyhow::anyhow!("state lock poisoned"))
                .and_then(|mut app| {
                    if !router_for_mouse.owns_terminal_pane(window_id, tab_id, &app) {
                        anyhow::bail!("terminal pane is no longer visible in this window");
                    }
                    let terminal = app.terminal_mut(tab_id).context("terminal tab not found")?;
                    if !terminal.connected {
                        return Ok(true);
                    }
                    let model = terminal
                        .terminal
                        .as_ref()
                        .context("active tab has no terminal model")?;
                    let Some(data) = model.encode_mouse_event(TerminalMouseEvent {
                        kind,
                        button,
                        column: column.max(0) as usize,
                        row: row.max(0) as usize,
                        modifiers: TerminalMouseModifiers {
                            shift,
                            alt,
                            control,
                        },
                    }) else {
                        return Ok(true);
                    };
                    let worker = terminal
                        .worker
                        .as_ref()
                        .context("active terminal has no worker")?;
                    if kind == TerminalMouseEventKind::Motion {
                        worker.request_send_motion(data)
                    } else {
                        worker.request_send(data).map(|()| true)
                    }
                });
            match result {
                Ok(true) => {}
                Ok(false) => {
                    debug!(%tab_id, "terminal mouse motion dropped under worker backpressure");
                }
                Err(error) => {
                    debug!(%error, "terminal mouse event failed");
                    set_status(
                        &ui_for_mouse,
                        &format!("Cannot send terminal mouse event: {error}"),
                    );
                }
            }
        },
    );

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

    let ui_for_target_hover = ui.as_weak();
    let state_for_target_hover = state.clone();
    let router_for_target_hover = window_router.clone();
    ui.on_terminal_target_at_cell(move |tab_id, row, column, control, meta| {
        if !terminal_target_modifier_held(control, meta) {
            return TerminalTargetHighlight::default();
        }
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_target_hover) else {
            return TerminalTargetHighlight::default();
        };
        terminal_target_highlight_for_pane(
            &state_for_target_hover,
            &router_for_target_hover,
            window_id,
            tab_id,
            row,
            column,
        )
        .unwrap_or_default()
    });

    let ui_for_target_open = ui.as_weak();
    let state_for_target_open = state.clone();
    let runtime_for_target_open = runtime.clone();
    let font_registry_for_target_open = font_registry.clone();
    let terminal_font_started_for_target_open = terminal_font_started.clone();
    let router_for_target_open = window_router.clone();
    ui.on_activate_terminal_target(move |tab_id, row, column, control, meta| {
        if !terminal_target_modifier_held(control, meta) {
            return false;
        }
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_target_open) else {
            return false;
        };
        let Some(target) = terminal_target_for_pane(
            &state_for_target_open,
            &router_for_target_open,
            window_id,
            tab_id,
            row,
            column,
        ) else {
            return false;
        };

        log_ui_action("terminal.open-target");
        match target {
            TerminalTarget::Url(url) => {
                open_terminal_url(&runtime_for_target_open, ui_for_target_open.clone(), url);
                log_ui_action_outcome("terminal.open-target", "url");
            }
            TerminalTarget::RemotePath(path) => {
                open_terminal_remote_path(
                    &state_for_target_open,
                    &router_for_target_open,
                    window_id,
                    tab_id,
                    path,
                    &ui_for_target_open,
                    &runtime_for_target_open,
                    &font_registry_for_target_open,
                    &terminal_font_started_for_target_open,
                );
            }
        }
        true
    });

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
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_command) else {
            return false;
        };
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

fn close_terminal_notice_tab(
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

enum TerminalRetryRoute {
    Local,
    Profile {
        profile_id: Uuid,
        target: ConnectionTarget,
    },
}

#[allow(clippy::too_many_arguments)]
fn retry_terminal_notice_tab(
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

fn terminal_target_modifier_held(control: bool, _meta: bool) -> bool {
    // Slint normalizes the platform primary shortcut modifier into `control`:
    // Cmd on macOS and Ctrl elsewhere.
    control
}

fn terminal_target_for_pane(
    state: &Arc<Mutex<AppState>>,
    window_router: &WindowRouter,
    window_id: Uuid,
    tab_id: Uuid,
    row: i32,
    column: i32,
) -> Option<TerminalTarget> {
    let row = usize::try_from(row).ok()?;
    let column = usize::try_from(column).ok()?;
    let app = state.lock().ok()?;
    if !window_router.owns_terminal_pane(window_id, tab_id, &app) {
        return None;
    }
    let terminal = app.terminal(tab_id)?;
    if !terminal.connected {
        return None;
    }
    let (text, text_column) = terminal
        .terminal
        .as_ref()?
        .visible_row_text_at_cell(row, column)?;
    terminal_target_at_cell(&text, text_column)
}

fn terminal_target_highlight_for_pane(
    state: &Arc<Mutex<AppState>>,
    window_router: &WindowRouter,
    window_id: Uuid,
    tab_id: Uuid,
    row: i32,
    column: i32,
) -> Option<TerminalTargetHighlight> {
    let row = usize::try_from(row).ok()?;
    let column = usize::try_from(column).ok()?;
    let app = state.lock().ok()?;
    if !window_router.owns_terminal_pane(window_id, tab_id, &app) {
        return None;
    }
    let terminal = app.terminal(tab_id)?;
    if !terminal.connected {
        return None;
    }
    let terminal = terminal.terminal.as_ref()?;
    let (text, text_column) = terminal.visible_row_text_at_cell(row, column)?;
    let span = terminal_target_span_at_cell(&text, text_column)?;
    let (start_column, end_column) =
        terminal.visible_row_cell_span_for_characters(row, span.start, span.end)?;
    Some(TerminalTargetHighlight {
        active: true,
        row: i32::try_from(row).ok()?,
        start_column: i32::try_from(start_column).ok()?,
        end_column: i32::try_from(end_column).ok()?,
    })
}

#[allow(clippy::too_many_arguments)]
fn open_terminal_remote_path(
    state: &Arc<Mutex<AppState>>,
    window_router: &WindowRouter,
    window_id: Uuid,
    terminal_tab_id: Uuid,
    path: String,
    ui: &slint::Weak<AppWindow>,
    runtime: &Handle,
    font_registry: &Arc<Mutex<FontRegistry>>,
    terminal_font_started: &Arc<std::sync::atomic::AtomicBool>,
) {
    enum PathRoute {
        ExistingSftp(Uuid),
        NewSftp(Uuid),
    }

    let route = (|| -> Result<PathRoute> {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        if !window_router.owns_terminal_pane(window_id, terminal_tab_id, &app) {
            anyhow::bail!("terminal pane is no longer visible in this window");
        }
        let profile_id = {
            let terminal = app
                .terminal(terminal_tab_id)
                .context("terminal tab is no longer available")?;
            if !terminal.connected {
                anyhow::bail!("terminal session is not connected");
            }
            terminal
                .ssh_route()
                .map(|(profile_id, _)| profile_id)
                .context("remote paths require an SSH terminal")?
        };
        if let Some(sftp_tab_id) = app.sftp_companion_id(terminal_tab_id) {
            if !window_router
                .tab_ids(window_id, &app)
                .contains(&sftp_tab_id)
            {
                anyhow::bail!("SFTP companion is not in this window");
            }
            if !app.activate_tab(sftp_tab_id) {
                anyhow::bail!("SFTP companion is no longer available");
            }
            Ok(PathRoute::ExistingSftp(sftp_tab_id))
        } else {
            Ok(PathRoute::NewSftp(profile_id))
        }
    })();

    match route {
        Ok(PathRoute::ExistingSftp(sftp_tab_id)) => {
            window_router.set_active(window_id, sftp_tab_id);
            match navigate_sftp_tab_to_path(state, sftp_tab_id, path) {
                Ok(()) => {
                    log_ui_action_outcome("terminal.open-target", "sftp-existing");
                    refresh_workspace(ui, state);
                }
                Err(error) => {
                    log_ui_action_outcome("terminal.open-target", "sftp-unavailable");
                    set_status(ui, &format!("Cannot open SFTP location: {error}"));
                    refresh_workspace(ui, state);
                }
            }
        }
        Ok(PathRoute::NewSftp(profile_id)) => {
            let connection = ConnectionContext::new(
                ui.clone(),
                state.clone(),
                runtime.clone(),
                font_registry.clone(),
                terminal_font_started.clone(),
            );
            let _ = request_profile_connection(
                &connection,
                profile_id,
                ConnectionTarget::Sftp,
                Some(terminal_tab_id),
                Some(path),
                None,
                {
                    let router = window_router.clone();
                    move |new_tab_id, app| {
                        router.include_tab(window_id, new_tab_id)
                            && router.activate_tab(window_id, new_tab_id, app)
                    }
                },
            );
            log_ui_action_outcome("terminal.open-target", "sftp-new");
        }
        Err(error) => {
            log_ui_action_outcome("terminal.open-target", "sftp-rejected");
            set_status(ui, &format!("Cannot open SFTP location: {error}"));
        }
    }
}

fn open_terminal_url(runtime: &Handle, ui: slint::Weak<AppWindow>, url: String) {
    runtime.spawn(async move {
        let opened = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || open::that_detached(url)),
        )
        .await;
        if !matches!(opened, Ok(Ok(Ok(())))) {
            tracing::warn!(
                target: "ax_ssh::diagnostics",
                operation = "open-terminal-url",
                "failed to open terminal URL"
            );
            set_status(&ui, "Cannot open URL");
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
    let runtime_for_monitor = runtime.clone();
    runtime.spawn(async move {
        let mut terminal_event = false;
        let mut finished_worker = None;
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
                    let mut response_error = None;
                    if let Some(true) = mutate_local_terminal(&state, tab_id, |terminal| {
                        if let Err(error) = process_terminal_output(terminal, &data) {
                            response_error = Some(error);
                        }
                    }) {
                        dispatch_active_snapshot(&ui, &state);
                    }
                    if let Some(error) = response_error {
                        warn!(tab_id = %tab_id, %error, "failed to send local terminal protocol response");
                    }
                }
                // The UI updates its terminal snapshot as soon as this resize request is accepted.
                // Ignoring this later acknowledgement prevents a stale worker event from reverting it.
                LocalShellEvent::Resized { .. } => {}
                LocalShellEvent::Exited { status } => {
                    terminal_event = true;
                    if let Some(finished) = finish_local_terminal(
                        &state,
                        tab_id,
                        &format!("Local shell exited: {status}"),
                    ) {
                        finished_worker = finished.worker;
                        if !global_window_router().is_some_and(|router| {
                            close_terminal_child_pane(
                                &router,
                                None,
                                tab_id,
                                &state,
                                &ui,
                                &runtime_for_monitor,
                            )
                        }) {
                            refresh_workspace(&ui, &state);
                        }
                    }
                    break;
                }
                LocalShellEvent::Failed(message) => {
                    terminal_event = true;
                    warn!(tab_id = %tab_id, error = %message, "local shell worker failed");
                    if let Some(finished) = finish_local_terminal(
                        &state,
                        tab_id,
                        &format!("Local shell failed: {message}"),
                    ) {
                        finished_worker = finished.worker;
                        refresh_workspace(&ui, &state);
                    }
                    break;
                }
            }
        }
        if !terminal_event
            && let Some(finished) =
                finish_local_terminal(&state, tab_id, "Local shell worker stopped")
        {
            finished_worker = finished.worker;
            refresh_workspace(&ui, &state);
        }
        if let Some(worker) = finished_worker
            && let Err(error) = worker.shutdown().await
        {
            warn!(tab_id = %tab_id, %error, "failed to reclaim stopped local shell worker");
        }
        debug!(tab_id = %tab_id, "local shell event monitor stopped");
    });
}

pub(super) fn process_terminal_output(terminal: &mut TerminalTabState, data: &[u8]) -> Result<()> {
    let responses = terminal
        .terminal
        .as_mut()
        .context("terminal tab has no terminal model")?
        .process_with_responses(data);
    if responses.is_empty() {
        return Ok(());
    }
    let worker = terminal
        .worker
        .as_ref()
        .context("terminal protocol response has no transport worker")?;
    for response in responses {
        worker
            .request_send(response)
            .context("cannot queue terminal protocol response")?;
    }
    Ok(())
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

struct FinishedLocalTerminal {
    worker: Option<TerminalWorker>,
}

fn finish_local_terminal(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    status: &str,
) -> Option<FinishedLocalTerminal> {
    match state.lock() {
        Ok(mut app) if app.terminal(tab_id).is_some_and(TerminalTabState::is_local) => {
            let terminal = app.terminal_mut(tab_id)?;
            let worker = terminal.worker.take();
            terminal.connected = false;
            terminal.worker_running = false;
            terminal.status = status.to_owned();
            Some(FinishedLocalTerminal { worker })
        }
        Ok(_) | Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_target_uses_slint_primary_shortcut_modifier() {
        assert!(terminal_target_modifier_held(true, false));
        assert!(!terminal_target_modifier_held(false, true));
    }

    #[cfg(not(windows))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn finishing_local_terminal_returns_its_worker_for_explicit_shutdown() {
        let state = Arc::new(Mutex::new(AppState::new(
            ConfigStore::new(
                std::env::temp_dir().join(format!("axssh-local-finish-{}.json", Uuid::new_v4())),
            ),
            SessionStore::default(),
        )));
        let (tab_id, mut events) = {
            let mut app = state.lock().expect("state should lock");
            let tab_id = app.open_local_shell_tab();
            let (worker, events) =
                LocalShellHandle::spawn(ax_ssh::local_shell::SYSTEM_SHELL.into(), 80, 24);
            app.terminal_mut(tab_id)
                .expect("local terminal should exist")
                .worker = Some(TerminalWorker::Local(worker));
            (tab_id, events)
        };

        let started = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("local shell should report startup");
        assert!(matches!(started, Some(LocalShellEvent::Started { .. })));

        let finished = finish_local_terminal(&state, tab_id, "finished")
            .expect("local terminal should transition to finished");
        let worker = finished
            .worker
            .expect("finished transition must preserve the worker owner");
        {
            let app = state.lock().expect("state should lock");
            let terminal = app.terminal(tab_id).expect("local terminal should remain");
            assert!(terminal.worker.is_none());
            assert!(!terminal.connected);
            assert!(!terminal.worker_running);
            assert_eq!(terminal.status, "finished");
        }
        tokio::time::timeout(Duration::from_secs(5), worker.shutdown())
            .await
            .expect("worker shutdown must remain bounded")
            .expect("finished local worker should shut down cleanly");
    }
}
