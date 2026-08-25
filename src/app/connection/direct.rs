use super::*;

const SERIAL_RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn start_telnet_connection(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    profile: SessionProfile,
) -> Result<()> {
    let attempt_id = Uuid::new_v4();
    let config = profile
        .telnet()
        .cloned()
        .context("Telnet worker requires a Telnet profile")?;
    let events = {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        let columns = u32::from(app.sessions.settings.terminal.default_columns);
        let rows = u32::from(app.sessions.settings.terminal.default_rows);
        let terminal = app
            .terminal_mut(tab_id)
            .context("Telnet terminal tab disappeared")?;
        if terminal.telnet_route() != Some((profile.id, None)) || terminal.worker.is_some() {
            anyhow::bail!("Telnet terminal is stale or already has a worker");
        }
        let (worker, events) =
            TelnetSessionHandle::spawn(runtime, profile.id, config, columns, rows);
        terminal.set_telnet_attempt(Some(attempt_id));
        terminal.enable_reconnect();
        terminal.worker = Some(TerminalWorker::Telnet(worker));
        terminal.worker_running = true;
        terminal.connected = false;
        terminal.status = format!("Connecting to {}...", profile_endpoint(&profile));
        events
    };
    refresh_workspace(&ui, &state);
    spawn_telnet_monitor(runtime, state, ui, tab_id, profile, attempt_id, events);
    Ok(())
}

pub(super) fn start_serial_connection(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    profile: SessionProfile,
) -> Result<()> {
    let attempt_id = Uuid::new_v4();
    let config = profile
        .serial()
        .cloned()
        .context("Serial worker requires a Serial profile")?;
    {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        let terminal = app
            .terminal_mut(tab_id)
            .context("Serial terminal tab disappeared")?;
        if terminal.serial_route() != Some((profile.id, None)) || terminal.worker.is_some() {
            anyhow::bail!("Serial terminal is stale or already has a worker");
        }
        terminal.set_serial_attempt(Some(attempt_id));
        terminal.enable_reconnect();
        terminal.worker_running = true;
        terminal.connected = false;
        terminal.status = format!("Locating {}...", config.port_name);
    }
    refresh_workspace(&ui, &state);

    let runtime_for_monitor = runtime.clone();
    runtime.spawn(async move {
        let discovery = tokio::task::spawn_blocking(discover_serial_ports);
        let ports = match tokio::time::timeout(SERIAL_RESOLVE_TIMEOUT, discovery).await {
            Ok(Ok(Ok(ports))) => ports,
            Ok(Ok(Err(error))) => {
                let finished = finish_direct_attempt(
                    &state,
                    tab_id,
                    profile.id,
                    attempt_id,
                    DirectProtocol::Serial,
                    &format!("Cannot list serial ports: {error}"),
                );
                if finished {
                    schedule_reconnect(
                        &runtime_for_monitor,
                        state.clone(),
                        ui.clone(),
                        tab_id,
                        profile.clone(),
                        ReconnectProtocol::Serial,
                        ConnectionTarget::Terminal,
                    );
                }
                refresh_workspace(&ui, &state);
                return;
            }
            Ok(Err(error)) => {
                let finished = finish_direct_attempt(
                    &state,
                    tab_id,
                    profile.id,
                    attempt_id,
                    DirectProtocol::Serial,
                    &format!("Serial port scan failed: {error}"),
                );
                if finished {
                    schedule_reconnect(
                        &runtime_for_monitor,
                        state.clone(),
                        ui.clone(),
                        tab_id,
                        profile.clone(),
                        ReconnectProtocol::Serial,
                        ConnectionTarget::Terminal,
                    );
                }
                refresh_workspace(&ui, &state);
                return;
            }
            Err(_) => {
                let finished = finish_direct_attempt(
                    &state,
                    tab_id,
                    profile.id,
                    attempt_id,
                    DirectProtocol::Serial,
                    "Serial port scan timed out",
                );
                if finished {
                    schedule_reconnect(
                        &runtime_for_monitor,
                        state.clone(),
                        ui.clone(),
                        tab_id,
                        profile.clone(),
                        ReconnectProtocol::Serial,
                        ConnectionTarget::Terminal,
                    );
                }
                refresh_workspace(&ui, &state);
                return;
            }
        };

        let mut resolved_config = config;
        let resolved = match resolve_serial_port(&resolved_config, &ports) {
            Ok(port) => port.clone(),
            Err(error) => {
                if let Ok(mut app) = state.lock() {
                    app.replace_serial_ports(ports);
                }
                let finished = finish_direct_attempt(
                    &state,
                    tab_id,
                    profile.id,
                    attempt_id,
                    DirectProtocol::Serial,
                    &format!("Cannot open serial session: {error}"),
                );
                if finished {
                    schedule_reconnect(
                        &runtime_for_monitor,
                        state.clone(),
                        ui.clone(),
                        tab_id,
                        profile.clone(),
                        ReconnectProtocol::Serial,
                        ConnectionTarget::Terminal,
                    );
                }
                refresh_workspace(&ui, &state);
                return;
            }
        };
        resolved.apply_identity_to(&mut resolved_config);
        let port_names = ports
            .iter()
            .map(|port| SharedString::from(port.port_name.clone()))
            .collect::<Vec<_>>();

        let events = {
            let mut app = match state.lock() {
                Ok(app) => app,
                Err(_) => {
                    set_status(&ui, "Cannot read session state");
                    return;
                }
            };
            app.replace_serial_ports(ports);
            let Some(terminal) = app.terminal_mut(tab_id) else {
                return;
            };
            if !direct_attempt_matches(terminal, profile.id, attempt_id, DirectProtocol::Serial)
                || terminal.worker.is_some()
            {
                return;
            }
            let (worker, events) =
                SerialSessionHandle::spawn(&runtime_for_monitor, profile.id, resolved_config);
            terminal.worker = Some(TerminalWorker::Serial(worker));
            terminal.status = format!("Opening serial port {}...", resolved.port_name);
            events
        };
        dispatch_ui(&ui, move |ui| {
            ui.set_serial_port_options(ModelRc::new(VecModel::from(port_names)));
        });
        refresh_workspace(&ui, &state);
        spawn_serial_monitor(
            &runtime_for_monitor,
            state,
            ui,
            tab_id,
            profile,
            attempt_id,
            events,
        );
    });
    Ok(())
}

fn spawn_telnet_monitor(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    profile: SessionProfile,
    attempt_id: Uuid,
    mut events: mpsc::Receiver<TelnetSessionEvent>,
) {
    let runtime_for_monitor = runtime.clone();
    runtime.spawn(async move {
        let mut terminal_event = false;
        let mut presentation = crate::app::terminal_presentation::TerminalPresentation::new();
        loop {
            let event = tokio::select! {
                event = events.recv() => {
                    let Some(event) = event else {
                        break;
                    };
                    event
                }
                _ = presentation.wait_until_ready(tab_id), if presentation.has_pending_output() => {
                    if prepare_terminal_output_snapshot(&state, tab_id) {
                        dispatch_terminal_snapshot(&ui, &state, tab_id);
                    }
                    continue;
                }
            };
            match event {
                TelnetSessionEvent::Connected => {
                    let Some(active) = mutate_direct_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        DirectProtocol::Telnet,
                        |terminal| {
                            terminal.connected = true;
                            terminal.worker_running = true;
                            let generation = terminal.reconnect_generation();
                            terminal.mark_reconnect_connected(generation);
                            terminal.status =
                                format!("Connected to {}", profile_endpoint(&profile));
                        },
                    ) else {
                        continue;
                    };
                    if active {
                        dispatch_active_snapshot(&ui, &state);
                    }
                    refresh_workspace(&ui, &state);
                }
                TelnetSessionEvent::Output(data) => {
                    let mut response_error = None;
                    if mutate_direct_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        DirectProtocol::Telnet,
                        |terminal| {
                            if let Err(error) = process_terminal_output(terminal, &data) {
                                response_error = Some(error);
                            }
                        },
                    )
                    .is_some()
                        && !data.is_empty()
                    {
                        presentation.record_output(None);
                    }
                    if let Some(error) = response_error {
                        warn!(
                            tab_id = %tab_id,
                            session_id = %profile.id,
                            %error,
                            "failed to send Telnet terminal protocol response"
                        );
                    }
                }
                TelnetSessionEvent::Disconnected => {
                    terminal_event = true;
                    presentation.clear_pending_output();
                    if finish_direct_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        DirectProtocol::Telnet,
                        "Disconnected",
                    ) {
                        schedule_reconnect(
                            &runtime_for_monitor,
                            state.clone(),
                            ui.clone(),
                            tab_id,
                            profile.clone(),
                            ReconnectProtocol::Telnet,
                            ConnectionTarget::Terminal,
                        );
                        refresh_workspace(&ui, &state);
                    }
                }
                TelnetSessionEvent::Failed(message) => {
                    terminal_event = true;
                    presentation.clear_pending_output();
                    if finish_direct_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        DirectProtocol::Telnet,
                        &format!("Telnet connection failed: {message}"),
                    ) {
                        schedule_reconnect(
                            &runtime_for_monitor,
                            state.clone(),
                            ui.clone(),
                            tab_id,
                            profile.clone(),
                            ReconnectProtocol::Telnet,
                            ConnectionTarget::Terminal,
                        );
                        refresh_workspace(&ui, &state);
                    }
                }
            }
        }
        presentation.clear_pending_output();
        if !terminal_event
            && finish_direct_attempt(
                &state,
                tab_id,
                profile.id,
                attempt_id,
                DirectProtocol::Telnet,
                "Telnet worker stopped",
            )
        {
            schedule_reconnect(
                &runtime_for_monitor,
                state.clone(),
                ui.clone(),
                tab_id,
                profile.clone(),
                ReconnectProtocol::Telnet,
                ConnectionTarget::Terminal,
            );
            refresh_workspace(&ui, &state);
        }
        debug!(tab_id = %tab_id, session_id = %profile.id, "Telnet event monitor stopped");
    });
}

fn spawn_serial_monitor(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    profile: SessionProfile,
    attempt_id: Uuid,
    mut events: mpsc::Receiver<SerialSessionEvent>,
) {
    let runtime_for_monitor = runtime.clone();
    runtime.spawn(async move {
        let mut terminal_event = false;
        let mut presentation = crate::app::terminal_presentation::TerminalPresentation::new();
        loop {
            let event = tokio::select! {
                event = events.recv() => {
                    let Some(event) = event else {
                        break;
                    };
                    event
                }
                _ = presentation.wait_until_ready(tab_id), if presentation.has_pending_output() => {
                    if prepare_terminal_output_snapshot(&state, tab_id) {
                        dispatch_terminal_snapshot(&ui, &state, tab_id);
                    }
                    continue;
                }
            };
            match event {
                SerialSessionEvent::Connected { port_name } => {
                    let Some(active) = mutate_direct_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        DirectProtocol::Serial,
                        |terminal| {
                            terminal.connected = true;
                            terminal.worker_running = true;
                            let generation = terminal.reconnect_generation();
                            terminal.mark_reconnect_connected(generation);
                            terminal.status = format!("Connected to serial port {port_name}");
                        },
                    ) else {
                        continue;
                    };
                    if active {
                        dispatch_active_snapshot(&ui, &state);
                    }
                    refresh_workspace(&ui, &state);
                }
                SerialSessionEvent::Output(data) => {
                    let mut response_error = None;
                    if mutate_direct_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        DirectProtocol::Serial,
                        |terminal| {
                            if let Err(error) = process_terminal_output(terminal, &data) {
                                response_error = Some(error);
                            }
                        },
                    )
                    .is_some()
                        && !data.is_empty()
                    {
                        presentation.record_output(None);
                    }
                    if let Some(error) = response_error {
                        warn!(
                            tab_id = %tab_id,
                            session_id = %profile.id,
                            %error,
                            "failed to send serial terminal protocol response"
                        );
                    }
                }
                SerialSessionEvent::Disconnected => {
                    terminal_event = true;
                    presentation.clear_pending_output();
                    if finish_direct_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        DirectProtocol::Serial,
                        "Serial port disconnected",
                    ) {
                        schedule_reconnect(
                            &runtime_for_monitor,
                            state.clone(),
                            ui.clone(),
                            tab_id,
                            profile.clone(),
                            ReconnectProtocol::Serial,
                            ConnectionTarget::Terminal,
                        );
                        refresh_workspace(&ui, &state);
                    }
                }
                SerialSessionEvent::Failed(message) => {
                    terminal_event = true;
                    presentation.clear_pending_output();
                    if finish_direct_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        DirectProtocol::Serial,
                        &format!("Serial connection failed: {message}"),
                    ) {
                        schedule_reconnect(
                            &runtime_for_monitor,
                            state.clone(),
                            ui.clone(),
                            tab_id,
                            profile.clone(),
                            ReconnectProtocol::Serial,
                            ConnectionTarget::Terminal,
                        );
                        refresh_workspace(&ui, &state);
                    }
                }
            }
        }
        presentation.clear_pending_output();
        if !terminal_event
            && finish_direct_attempt(
                &state,
                tab_id,
                profile.id,
                attempt_id,
                DirectProtocol::Serial,
                "Serial worker stopped",
            )
        {
            schedule_reconnect(
                &runtime_for_monitor,
                state.clone(),
                ui.clone(),
                tab_id,
                profile.clone(),
                ReconnectProtocol::Serial,
                ConnectionTarget::Terminal,
            );
            refresh_workspace(&ui, &state);
        }
        debug!(tab_id = %tab_id, session_id = %profile.id, "Serial event monitor stopped");
    });
}

#[derive(Clone, Copy)]
enum DirectProtocol {
    Telnet,
    Serial,
}

fn direct_attempt_matches(
    terminal: &TerminalTabState,
    profile_id: Uuid,
    attempt_id: Uuid,
    protocol: DirectProtocol,
) -> bool {
    (match protocol {
        DirectProtocol::Telnet => terminal.telnet_route(),
        DirectProtocol::Serial => terminal.serial_route(),
    }) == Some((profile_id, Some(attempt_id)))
}

fn mutate_direct_attempt(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    profile_id: Uuid,
    attempt_id: Uuid,
    protocol: DirectProtocol,
    action: impl FnOnce(&mut TerminalTabState),
) -> Option<bool> {
    let mut app = state.lock().ok()?;
    if !app
        .terminal(tab_id)
        .is_some_and(|terminal| direct_attempt_matches(terminal, profile_id, attempt_id, protocol))
    {
        return None;
    }
    action(app.terminal_mut(tab_id)?);
    Some(true)
}

fn finish_direct_attempt(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    profile_id: Uuid,
    attempt_id: Uuid,
    protocol: DirectProtocol,
    status: &str,
) -> bool {
    let Ok(mut app) = state.lock() else {
        return false;
    };
    if !app
        .terminal(tab_id)
        .is_some_and(|terminal| direct_attempt_matches(terminal, profile_id, attempt_id, protocol))
    {
        return false;
    }
    let Some(terminal) = app.terminal_mut(tab_id) else {
        return false;
    };
    let generation = terminal.reconnect_generation();
    terminal.finish_reconnect_attempt(generation);
    terminal.worker = None;
    match protocol {
        DirectProtocol::Telnet => terminal.set_telnet_attempt(None),
        DirectProtocol::Serial => terminal.set_serial_attempt(None),
    };
    terminal.connected = false;
    terminal.worker_running = false;
    terminal.status = status.to_owned();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_direct_attempt_cannot_mutate_a_duplicate_tab() {
        let profile = SessionProfile::new_telnet("console", "127.0.0.1");
        let mut app = AppState::new(
            ConfigStore::new(
                std::env::temp_dir().join(format!("ax-ssh-direct-{}.json", Uuid::new_v4())),
            ),
            SessionStore::default(),
        );
        let first_tab = app.open_terminal_tab(&profile);
        let second_tab = app.open_terminal_tab(&profile);
        let first_attempt = Uuid::new_v4();
        let second_attempt = Uuid::new_v4();
        app.terminal_mut(first_tab)
            .expect("first terminal should exist")
            .set_telnet_attempt(Some(first_attempt));
        app.terminal_mut(second_tab)
            .expect("second terminal should exist")
            .set_telnet_attempt(Some(second_attempt));
        app.close_tab(first_tab).expect("first tab should close");
        let state = Arc::new(Mutex::new(app));

        assert!(
            mutate_direct_attempt(
                &state,
                first_tab,
                profile.id,
                first_attempt,
                DirectProtocol::Telnet,
                |terminal| terminal.connected = true,
            )
            .is_none()
        );
        assert_eq!(
            state
                .lock()
                .expect("state should remain readable")
                .terminal(second_tab)
                .and_then(TerminalTabState::telnet_route),
            Some((profile.id, Some(second_attempt)))
        );
    }
}
