use super::*;

pub(super) fn wire_connection_request(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
) {
    let ui_for_connect = ui.as_weak();
    ui.on_connect_session(move |id| {
        let profile_id = match parse_uuid(id.as_str(), "session", &ui_for_connect) {
            Some(id) => id,
            None => return,
        };
        let start = {
            let mut app = match state.lock() {
                Ok(app) => app,
                Err(_) => {
                    set_status(&ui_for_connect, "Cannot read session state");
                    return;
                }
            };
            if app.prompt_flow_busy() {
                set_status(
                    &ui_for_connect,
                    "Finish or cancel the current security prompt first",
                );
                return;
            }
            let Some(profile) = app
                .sessions
                .sessions
                .iter()
                .find(|profile| profile.id == profile_id)
                .cloned()
            else {
                set_status(&ui_for_connect, "Session not found");
                return;
            };
            let tab_id = app.open_terminal_tab(&profile);
            if profile.host_key_fingerprint.is_some() {
                app.pending_auth = Some(PendingAuth {
                    tab_id,
                    profile_id: profile.id,
                });
                ConnectionStart::Authenticate { tab_id, profile }
            } else {
                let (cancel, cancelled) = oneshot::channel();
                app.pending_probe = Some(PendingProbe {
                    tab_id,
                    profile_id: profile.id,
                    cancel,
                });
                ConnectionStart::Probe {
                    tab_id,
                    profile,
                    cancelled,
                }
            }
        };
        refresh_workspace(&ui_for_connect, &state);

        let (tab_id, profile, cancelled) = match start {
            ConnectionStart::Authenticate { tab_id, profile } => {
                begin_authentication(
                    &runtime,
                    state.clone(),
                    ui_for_connect.clone(),
                    tab_id,
                    profile,
                );
                return;
            }
            ConnectionStart::Probe {
                tab_id,
                profile,
                cancelled,
            } => (tab_id, profile, cancelled),
        };

        set_tab_status(
            &state,
            &ui_for_connect,
            tab_id,
            "Checking SSH host key...",
        );
        info!(
            tab_id = %tab_id,
            session_id = %profile.id,
            host = %profile.host,
            port = profile.port,
            "probing unknown SSH host key"
        );
        let state_for_probe = state.clone();
        let ui_for_probe = ui_for_connect.clone();
        runtime.spawn(async move {
            let result = tokio::select! {
                _ = cancelled => None,
                result = probe_host_key(&profile) => Some(result),
            };
            let prompt = match state_for_probe.lock() {
                Ok(mut app)
                    if app.pending_probe.as_ref().is_some_and(|probe| {
                        probe.tab_id == tab_id && probe.profile_id == profile.id
                    }) =>
                {
                    app.pending_probe = None;
                    match result {
                        Some(Ok(fingerprint)) => {
                            let prompt = PendingHostKey {
                                tab_id,
                                profile_id: profile.id,
                                host: profile.host.clone(),
                                port: profile.port,
                                fingerprint,
                                changed: false,
                            };
                            app.pending_trust = Some(prompt.clone());
                            Some(Ok(prompt))
                        }
                        Some(Err(error)) => Some(Err(error)),
                        None => None,
                    }
                }
                Ok(_) => None,
                Err(_) => Some(Err(anyhow::anyhow!("state lock poisoned"))),
            };
            match prompt {
                Some(Ok(prompt)) => {
                    show_host_key_prompt(&ui_for_probe, &prompt);
                    set_tab_status(
                        &state_for_probe,
                        &ui_for_probe,
                        tab_id,
                        "Verify the SSH host key before connecting",
                    );
                }
                Some(Err(error)) => {
                    warn!(tab_id = %tab_id, session_id = %profile.id, %error, "SSH host-key probe failed");
                    set_tab_status(
                        &state_for_probe,
                        &ui_for_probe,
                        tab_id,
                        &format!("Host-key check failed: {error}"),
                    );
                }
                None => debug!(tab_id = %tab_id, "cancelled or stale host-key probe result ignored"),
            }
        });
    });
}

pub(super) fn wire_host_key_confirmation(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
) {
    let ui_for_confirm = ui.as_weak();
    let state_for_confirm = state.clone();
    ui.on_confirm_host_key(move || {
        let (tab_id, profile) = {
            let mut app = match state_for_confirm.lock() {
                Ok(app) => app,
                Err(_) => {
                    set_status(&ui_for_confirm, "Cannot update session state");
                    return;
                }
            };
            let Some(pending) = app.pending_trust.clone() else {
                set_status(&ui_for_confirm, "No host key is awaiting confirmation");
                return;
            };
            let mut candidate = app.sessions.clone();
            let Some(profile) = candidate
                .sessions
                .iter_mut()
                .find(|profile| profile.id == pending.profile_id)
            else {
                set_status(&ui_for_confirm, "Session not found");
                return;
            };
            if app.terminal(pending.tab_id).is_none()
                || profile.host != pending.host
                || profile.port != pending.port
            {
                app.pending_trust = None;
                set_dialog_open(&ui_for_confirm, Dialog::HostKey, false);
                set_status(
                    &ui_for_confirm,
                    "Session endpoint or tab changed; check the host key again",
                );
                return;
            }
            profile.host_key_fingerprint = Some(pending.fingerprint.clone());
            let profile = profile.clone();
            if let Err(error) = app.config.save(&candidate) {
                set_status(&ui_for_confirm, &format!("Cannot trust host key: {error}"));
                return;
            }
            app.sessions = candidate;
            app.pending_trust = None;
            app.pending_auth = Some(PendingAuth {
                tab_id: pending.tab_id,
                profile_id: profile.id,
            });
            (pending.tab_id, profile)
        };

        info!(
            tab_id = %tab_id,
            session_id = %profile.id,
            fingerprint = ?profile.host_key_fingerprint,
            "SSH host key trusted by user"
        );
        set_dialog_open(&ui_for_confirm, Dialog::HostKey, false);
        begin_authentication(
            &runtime,
            state_for_confirm.clone(),
            ui_for_confirm.clone(),
            tab_id,
            profile,
        );
    });

    let ui_for_reject = ui.as_weak();
    ui.on_reject_host_key(move || {
        let pending = match state.lock() {
            Ok(mut app) => app.pending_trust.take(),
            Err(_) => {
                set_status(&ui_for_reject, "Cannot update session state");
                return;
            }
        };
        set_dialog_open(&ui_for_reject, Dialog::HostKey, false);
        if let Some(pending) = pending {
            set_tab_status(
                &state,
                &ui_for_reject,
                pending.tab_id,
                "Connection cancelled; host key was not trusted",
            );
        }
    });
}

pub(super) fn begin_authentication(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    profile: SessionProfile,
) {
    if matches!(profile.auth, AuthMethod::PrivateKey { .. }) {
        set_tab_status(&state, &ui, tab_id, "Loading private key...");
        if let Err(error) = start_session_worker(
            runtime,
            state,
            ui.clone(),
            tab_id,
            profile.id,
            String::new(),
            false,
            false,
        ) {
            show_auth_prompt(&ui, &profile, false);
            set_status(
                &ui,
                &format!("Cannot start private-key connection: {error}"),
            );
        }
        return;
    }
    if !profile.credential_stored {
        show_auth_prompt(&ui, &profile, false);
        set_tab_status(&state, &ui, tab_id, "Password required");
        return;
    }

    let runtime_for_lookup = runtime.clone();
    set_tab_status(
        &state,
        &ui,
        tab_id,
        "Loading password from system credential store...",
    );
    runtime.spawn(async move {
        let result = load_stored_password(profile.id).await;
        let current = match state.lock() {
            Ok(app) => {
                app.pending_auth
                    == Some(PendingAuth {
                        tab_id,
                        profile_id: profile.id,
                    })
                    && app.terminal(tab_id).is_some()
            }
            Err(_) => {
                set_status(&ui, "Cannot read session state");
                return;
            }
        };
        if !current {
            debug!(tab_id = %tab_id, session_id = %profile.id, "stale credential lookup ignored");
            return;
        }

        match result {
            Ok(Some(secret)) => {
                if let Err(error) = start_session_worker(
                    &runtime_for_lookup,
                    state,
                    ui.clone(),
                    tab_id,
                    profile.id,
                    secret,
                    false,
                    true,
                ) {
                    set_status(&ui, &format!("Cannot start connection: {error}"));
                }
            }
            Ok(None) => {
                if let Err(error) = set_credential_marker(&state, profile.id, false) {
                    warn!(session_id = %profile.id, %error, "failed to clear missing credential marker");
                }
                show_auth_prompt(&ui, &profile, false);
                set_tab_status(
                    &state,
                    &ui,
                    tab_id,
                    "Saved password was not found; enter it again",
                );
            }
            Err(error) => {
                warn!(session_id = %profile.id, %error, "system credential lookup failed");
                show_auth_prompt(&ui, &profile, true);
                set_tab_status(
                    &state,
                    &ui,
                    tab_id,
                    &format!("System credential unavailable; enter password: {error}"),
                );
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn start_session_worker(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    profile_id: Uuid,
    secret: String,
    remember_after_connect: bool,
    used_stored_credential: bool,
) -> Result<()> {
    let attempt_id = Uuid::new_v4();
    let (profile, events, secret_to_store) = {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        if app.pending_auth != Some(PendingAuth { tab_id, profile_id }) {
            anyhow::bail!("terminal tab is not awaiting authentication");
        }
        let profile = app
            .sessions
            .sessions
            .iter()
            .find(|profile| profile.id == profile_id)
            .cloned()
            .context("session not found")?;
        if profile.host_key_fingerprint.is_none() {
            anyhow::bail!("verify the SSH host key first");
        }
        if app
            .terminal(tab_id)
            .is_none_or(|terminal| terminal.worker.is_some())
        {
            anyhow::bail!("terminal tab is missing or already has a worker");
        }
        let columns = u32::from(app.sessions.settings.terminal.default_columns);
        let rows = u32::from(app.sessions.settings.terminal.default_rows);
        let password_auth = matches!(profile.auth, AuthMethod::Password);
        let secret_to_store = (password_auth && remember_after_connect).then(|| secret.clone());
        let (worker, events) =
            SshSessionHandle::spawn(runtime, tab_id, profile.clone(), secret, columns, rows);
        let terminal = app
            .terminal_mut(tab_id)
            .context("terminal tab disappeared while starting worker")?;
        if !terminal.set_ssh_attempt(Some(attempt_id)) {
            anyhow::bail!("terminal tab is not an SSH terminal");
        }
        terminal.worker = Some(TerminalWorker::Ssh(worker));
        terminal.worker_running = true;
        terminal.connected = false;
        terminal.status = format!("Connecting to {}...", profile_endpoint(&profile));
        app.pending_auth = None;
        (profile, events, secret_to_store)
    };

    set_dialog_open(&ui, Dialog::Password, false);
    refresh_workspace(&ui, &state);
    spawn_session_monitor(
        runtime,
        state,
        ui,
        tab_id,
        profile,
        attempt_id,
        events,
        secret_to_store,
        used_stored_credential,
    );
    Ok(())
}

pub(super) fn wire_authentication(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    let ui_for_auth = ui.as_weak();
    let state_for_auth = state.clone();
    let runtime_for_auth = runtime.clone();
    ui.on_authenticate_session(move |password, remember_password| {
        let (pending, password_auth) = match state_for_auth.lock() {
            Ok(app) => {
                let pending = app.pending_auth;
                let password_auth = pending
                    .and_then(|pending| {
                        app.sessions
                            .sessions
                            .iter()
                            .find(|profile| profile.id == pending.profile_id)
                    })
                    .is_some_and(|profile| matches!(profile.auth, AuthMethod::Password));
                (pending, password_auth)
            }
            Err(_) => {
                set_status(&ui_for_auth, "Cannot read session state");
                return;
            }
        };
        let Some(pending) = pending else {
            set_status(&ui_for_auth, "No terminal tab is awaiting authentication");
            return;
        };
        if password_auth && password.is_empty() {
            set_status(&ui_for_auth, "Password cannot be empty");
            return;
        }
        if let Err(error) = start_session_worker(
            &runtime_for_auth,
            state_for_auth.clone(),
            ui_for_auth.clone(),
            pending.tab_id,
            pending.profile_id,
            password.as_str().to_owned(),
            password_auth && remember_password,
            false,
        ) {
            set_status(&ui_for_auth, &format!("Cannot start connection: {error}"));
        }
    });

    let ui_for_cancel = ui.as_weak();
    let state_for_cancel = state.clone();
    ui.on_cancel_password_dialog(move || {
        let pending = match state_for_cancel.lock() {
            Ok(mut app) => app.pending_auth.take(),
            Err(_) => {
                set_status(&ui_for_cancel, "Cannot update session state");
                return;
            }
        };
        set_dialog_open(&ui_for_cancel, Dialog::Password, false);
        if let Some(pending) = pending {
            set_tab_status(
                &state_for_cancel,
                &ui_for_cancel,
                pending.tab_id,
                "Authentication cancelled",
            );
        }
    });

    let ui_for_disconnect = ui.as_weak();
    ui.on_disconnect_session(move || {
        let result = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))
            .and_then(|app| {
                app.active_terminal()
                    .context("no active terminal")?
                    .worker
                    .as_ref()
                    .context("active terminal has no worker")?
                    .request_disconnect()
            });
        match result {
            Ok(()) => set_status(&ui_for_disconnect, "Disconnecting..."),
            Err(error) => set_status(
                &ui_for_disconnect,
                &format!("Cannot disconnect session: {error}"),
            ),
        }
    });
}
