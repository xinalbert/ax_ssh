use super::*;

pub(super) fn start_local_shell(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
) -> Result<()> {
    let (tab_id, events) = {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        let shell = app.sessions.settings.terminal.local_shell.clone();
        let columns = u32::from(app.sessions.settings.terminal.default_columns);
        let rows = u32::from(app.sessions.settings.terminal.default_rows);
        let tab_id = app.open_local_shell_tab();
        let (worker, events) = LocalShellHandle::spawn(shell, columns, rows);
        let terminal = app
            .terminal_mut(tab_id)
            .context("local terminal tab disappeared while starting worker")?;
        terminal.worker = Some(TerminalWorker::Local(worker));
        (tab_id, events)
    };
    refresh_workspace(&ui, &state);
    spawn_local_shell_monitor(runtime, state, ui, tab_id, events);
    Ok(())
}

pub(super) fn wire_terminal(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_for_theme = ui.as_weak();
    let state_for_theme = state.clone();
    ui.on_refresh_terminal_appearance(move || {
        // A theme change only changes the visual snapshot; it must not resize or
        // otherwise disturb the PTY worker that owns the active terminal.
        dispatch_active_snapshot(&ui_for_theme, &state_for_theme);
    });

    let ui_for_key = ui.as_weak();
    let state_for_key = state.clone();
    ui.on_terminal_key(move |text, alt, control, meta, shift| {
        let mut modifiers = normalize_slint_modifiers(alt, control, meta, shift);
        let key = terminal_key_from_slint(text.as_str(), modifiers);
        let result = state_for_key
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))
            .and_then(|mut app| {
                if cfg!(target_os = "macos") && !app.sessions.settings.terminal.option_as_meta {
                    modifiers.alt = false;
                }
                let application_cursor = app
                    .active_terminal()
                    .context("no active terminal")?
                    .terminal
                    .application_cursor();
                let Some(data) = encode_terminal_key(&key, modifiers, application_cursor) else {
                    return Ok((false, false));
                };
                let viewport_changed = {
                    let terminal = app.active_terminal_mut().context("no active terminal")?;
                    let viewport_changed = terminal.terminal.scroll_to_bottom();
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
                dispatch_active_snapshot(&ui_for_key, &state_for_key);
                handled
            }
            Ok((handled, false)) => handled,
            Err(error) => {
                set_status(&ui_for_key, &format!("Cannot send terminal input: {error}"));
                true
            }
        }
    });

    let ui_for_resize = ui.as_weak();
    let state_for_resize = state.clone();
    ui.on_resize_terminal(move |columns, rows| {
        let result = state_for_resize
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))
            .and_then(|mut app| {
                let columns = columns.max(1) as u32;
                let rows = rows.max(1) as u32;
                app.active_terminal()
                    .context("no active terminal")?
                    .worker
                    .as_ref()
                    .context("active terminal has no worker")?
                    .request_resize(columns, rows)?;
                app.resize_active_terminal_model(columns as usize, rows as usize)
                    .context("active terminal disappeared while resizing")?;
                Ok(())
            });
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_resize, &state_for_resize),
            Err(error) => {
                debug!(%error, "terminal resize ignored");
                set_status(&ui_for_resize, &format!("Cannot resize terminal: {error}"));
            }
        }
    });

    let ui_for_scroll = ui.as_weak();
    let state_for_scroll = state.clone();
    ui.on_scroll_terminal(move |lines| {
        let changed = state_for_scroll
            .lock()
            .ok()
            .and_then(|mut app| {
                app.active_terminal_mut()
                    .map(|terminal| terminal.terminal.scroll(lines))
            })
            .unwrap_or(false);
        if changed {
            dispatch_active_snapshot(&ui_for_scroll, &state_for_scroll);
        }
    });

    ui.on_terminal_selection_text(move |anchor_row, anchor_column, focus_row, focus_column| {
        state
            .lock()
            .ok()
            .and_then(|app| {
                app.active_terminal().map(|terminal| {
                    terminal.terminal.selection_text(
                        anchor_row.max(0) as usize,
                        anchor_column.max(0) as usize,
                        focus_row.max(0) as usize,
                        focus_column.max(0) as usize,
                    )
                })
            })
            .unwrap_or_default()
            .into()
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
                        terminal.terminal.process(&data);
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
    Some(app.active_tab_id() == Some(tab_id))
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
