//! Terminal local_shell application boundary.

use super::*;

pub(in crate::app) fn start_local_shell(
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

pub(in crate::app) fn resume_existing_local_shell(
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
        let mut presentation =
            crate::app::terminal_presentation::TerminalPresentation::new();
        loop {
            let event = if let Some(event) = presentation.try_recv_before_present(&mut events).await {
                event
            } else {
                tokio::select! {
                    biased;
                    _ = presentation.wait_until_ready(tab_id), if presentation.has_pending_output() => {
                        if prepare_terminal_output_snapshot(&state, tab_id) {
                            dispatch_terminal_snapshot(&ui, &state, tab_id);
                        }
                        continue;
                    }
                    event = events.recv() => {
                        let Some(event) = event else {
                            break;
                        };
                        event
                    }
                }
            };
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
                LocalShellEvent::Output(output) => {
                    let data = &output.data;
                    super::diagnostics::log_terminal_output_chunk(
                        "local",
                        data.len(),
                        output.received_at,
                    );
                    let mut response_error = None;
                    let mut presentation_hold = None;
                    let mut output_effects = TerminalOutputEffects::default();
                    if mutate_local_terminal(&state, tab_id, |terminal| {
                        match process_terminal_output(terminal, data) {
                            Ok(effects) => {
                                presentation_hold = effects.presentation_hold;
                                output_effects = effects;
                            }
                            Err(error) => response_error = Some(error),
                        }
                    })
                    .is_some()
                        && !data.is_empty()
                    {
                        presentation.record_output(Some(output.received_at), presentation_hold);
                    }
                    apply_terminal_output_effects(&state, &ui, tab_id, output_effects);
                    if let Some(error) = response_error {
                        warn!(tab_id = %tab_id, %error, "failed to send local terminal protocol response");
                    }
                }
                // The UI updates its terminal snapshot as soon as this resize request is accepted.
                // Ignoring this later acknowledgement prevents a stale worker event from reverting it.
                LocalShellEvent::Resized { .. } => {}
                LocalShellEvent::Exited { status } => {
                    terminal_event = true;
                    presentation.clear_pending_output();
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
                            close_workspace_tab(
                                tab_id,
                                &state,
                                &ui,
                                &runtime_for_monitor,
                            );
                        }
                    }
                    break;
                }
                LocalShellEvent::Failed(message) => {
                    terminal_event = true;
                    presentation.clear_pending_output();
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
        presentation.clear_pending_output();
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

pub(super) struct FinishedLocalTerminal {
    pub(super) worker: Option<TerminalWorker>,
}

pub(super) fn finish_local_terminal(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    status: &str,
) -> Option<FinishedLocalTerminal> {
    match state.lock() {
        Ok(mut app) if app.terminal(tab_id).is_some_and(TerminalTabState::is_local) => {
            let terminal = app.terminal_mut(tab_id)?;
            let worker = terminal.worker.take();
            terminal.clear_pending_clipboard_read();
            terminal.media.cancel();
            terminal.connected = false;
            terminal.worker_running = false;
            terminal.status = status.to_owned();
            Some(FinishedLocalTerminal { worker })
        }
        Ok(_) | Err(_) => None,
    }
}
