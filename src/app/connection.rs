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
                let Some(terminal) = app.terminal_mut(tab_id) else {
                    set_status(&ui_for_connect, "Cannot prepare SSH terminal tab");
                    return;
                };
                terminal.set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
                    vault_unlock_only: false,
                });
                ConnectionStart::Authenticate { tab_id, profile }
            } else {
                let (cancel, cancelled) = oneshot::channel();
                let probe = PendingProbe {
                    tab_id,
                    profile_id: profile.id,
                    cancel,
                };
                let Some(terminal) = app.terminal_mut(tab_id) else {
                    set_status(&ui_for_connect, "Cannot prepare SSH terminal tab");
                    return;
                };
                terminal.set_ssh_phase(SshConnectionPhase::Probing(probe));
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
            let outcome = match state_for_probe.lock() {
                Ok(mut app) => {
                    let Some(terminal) = app.terminal_mut(tab_id) else {
                        return;
                    };
                    let current = terminal.ssh_route().is_some_and(|route| route.0 == profile.id)
                        && matches!(
                            terminal.ssh_phase(),
                            Some(SshConnectionPhase::Probing(probe))
                                if probe.tab_id == tab_id && probe.profile_id == profile.id
                        );
                    if !current {
                        None
                    } else {
                        match result {
                            Some(Ok(fingerprint)) => {
                                terminal.set_ssh_phase(SshConnectionPhase::AwaitingHostKey(
                                    PendingHostKey {
                                        tab_id,
                                        profile_id: profile.id,
                                        host: profile.host.clone(),
                                        port: profile.port,
                                        fingerprint,
                                        changed: false,
                                    },
                                ));
                                Some(Ok(()))
                            }
                            Some(Err(error)) => {
                                terminal.set_ssh_phase(SshConnectionPhase::Idle);
                                Some(Err(error))
                            }
                            None => None,
                        }
                    }
                }
                Err(_) => Some(Err(anyhow::anyhow!("state lock poisoned"))),
            };
            match outcome {
                Some(Ok(())) => {
                    set_tab_status(
                        &state_for_probe,
                        &ui_for_probe,
                        tab_id,
                        "Verify the SSH host key before connecting",
                    );
                    refresh_workspace(&ui_for_probe, &state_for_probe);
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
            let Some(active_tab_id) = app.active_tab_id() else {
                set_status(&ui_for_confirm, "No host key is awaiting confirmation");
                return;
            };
            let Some(pending) = app
                .terminal(active_tab_id)
                .and_then(TerminalTabState::ssh_phase)
                .and_then(|phase| match phase {
                    SshConnectionPhase::AwaitingHostKey(prompt)
                        if app
                            .terminal(active_tab_id)
                            .and_then(TerminalTabState::ssh_route)
                            .is_some_and(|route| {
                                route.0 == prompt.profile_id && prompt.tab_id == active_tab_id
                            }) =>
                    {
                        Some(prompt.clone())
                    }
                    SshConnectionPhase::AwaitingHostKey(_) => None,
                    SshConnectionPhase::Idle
                    | SshConnectionPhase::Probing(_)
                    | SshConnectionPhase::AwaitingAuthentication { .. }
                    | SshConnectionPhase::LoadingStoredCredential => None,
                })
            else {
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
            if pending.tab_id != active_tab_id
                || app
                    .terminal(pending.tab_id)
                    .and_then(TerminalTabState::ssh_route)
                    .is_none_or(|route| route.0 != pending.profile_id)
                || profile.host != pending.host
                || profile.port != pending.port
            {
                if let Some(terminal) = app.terminal_mut(pending.tab_id) {
                    terminal.set_ssh_phase(SshConnectionPhase::Idle);
                }
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
            let Some(terminal) = app.terminal_mut(pending.tab_id) else {
                set_status(&ui_for_confirm, "SSH terminal tab is no longer available");
                return;
            };
            let still_awaiting_this_key = matches!(
                terminal.ssh_phase(),
                Some(SshConnectionPhase::AwaitingHostKey(current))
                    if current.profile_id == pending.profile_id
                        && current.host == pending.host
                        && current.port == pending.port
                        && current.fingerprint == pending.fingerprint
            );
            if !still_awaiting_this_key {
                set_status(
                    &ui_for_confirm,
                    "Host-key confirmation is no longer current",
                );
                return;
            }
            terminal.set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
                vault_unlock_only: false,
            });
            (pending.tab_id, profile)
        };

        info!(
            tab_id = %tab_id,
            session_id = %profile.id,
            fingerprint = ?profile.host_key_fingerprint,
            "SSH host key trusted by user"
        );
        refresh_workspace(&ui_for_confirm, &state_for_confirm);
        begin_authentication(
            &runtime,
            state_for_confirm.clone(),
            ui_for_confirm.clone(),
            tab_id,
            profile,
        );
    });

    let ui_for_reject = ui.as_weak();
    let state_for_reject = state.clone();
    ui.on_reject_host_key(move || {
        let pending = match state_for_reject.lock() {
            Ok(mut app) => {
                let active_tab_id = app.active_tab_id();
                let pending = active_tab_id
                    .and_then(|tab_id| app.terminal(tab_id).map(|terminal| (tab_id, terminal)))
                    .and_then(|(tab_id, terminal)| match terminal.ssh_phase() {
                        Some(SshConnectionPhase::AwaitingHostKey(prompt))
                            if prompt.tab_id == tab_id =>
                        {
                            Some(prompt.clone())
                        }
                        Some(SshConnectionPhase::Idle)
                        | Some(SshConnectionPhase::Probing(_))
                        | Some(SshConnectionPhase::AwaitingAuthentication { .. })
                        | Some(SshConnectionPhase::LoadingStoredCredential)
                        | Some(SshConnectionPhase::AwaitingHostKey(_))
                        | None => None,
                    });
                if let Some(pending) = &pending
                    && let Some(terminal) = app.terminal_mut(pending.tab_id)
                {
                    terminal.set_ssh_phase(SshConnectionPhase::Idle);
                }
                pending
            }
            Err(_) => {
                set_status(&ui_for_reject, "Cannot update session state");
                return;
            }
        };
        if let Some(pending) = pending {
            set_tab_status(
                &state_for_reject,
                &ui_for_reject,
                pending.tab_id,
                "Connection cancelled; host key was not trusted",
            );
            refresh_workspace(&ui_for_reject, &state_for_reject);
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
            state.clone(),
            ui.clone(),
            tab_id,
            profile.id,
            zeroize::Zeroizing::new(String::new()),
            None,
            false,
            AuthenticationStart::Prompt,
        ) {
            set_awaiting_authentication(&state, tab_id, profile.id, false);
            set_tab_status(
                &state,
                &ui,
                tab_id,
                &format!("Cannot start private-key connection: {error}"),
            );
            refresh_workspace(&ui, &state);
        }
        return;
    }
    let Some(storage) = profile.credential_storage else {
        set_awaiting_authentication(&state, tab_id, profile.id, false);
        set_tab_status(&state, &ui, tab_id, "Password required");
        refresh_workspace(&ui, &state);
        return;
    };
    if storage == CredentialStorage::EncryptedVault {
        set_awaiting_authentication(&state, tab_id, profile.id, true);
        set_tab_status(
            &state,
            &ui,
            tab_id,
            "Unlocking the saved password requires the vault password",
        );
        refresh_workspace(&ui, &state);
        return;
    }

    let runtime_for_lookup = runtime.clone();
    if !set_loading_stored_credential(&state, tab_id, profile.id) {
        debug!(tab_id = %tab_id, session_id = %profile.id, "stale authentication start ignored");
        return;
    }
    set_tab_status(
        &state,
        &ui,
        tab_id,
        "Loading password from system credential store...",
    );
    refresh_workspace(&ui, &state);
    runtime.spawn(async move {
        let result = load_system_password(profile.id).await;
        let current = match state.lock() {
            Ok(app) => terminal_has_phase(
                &app,
                tab_id,
                profile.id,
                |phase| matches!(phase, SshConnectionPhase::LoadingStoredCredential),
            ),
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
                    None,
                    true,
                    AuthenticationStart::StoredCredential,
                ) {
                    set_status(&ui, &format!("Cannot start connection: {error}"));
                }
            }
            Ok(None) => {
                match set_credential_storage_while_loading(&state, tab_id, profile.id, None) {
                    Ok(true) => {}
                    Ok(false) => {
                        debug!(tab_id = %tab_id, session_id = %profile.id, "stale missing credential result ignored");
                        return;
                    }
                    Err(error) => {
                        warn!(session_id = %profile.id, %error, "failed to clear missing credential storage policy");
                    }
                }
                if !set_awaiting_authentication(&state, tab_id, profile.id, false) {
                    return;
                }
                set_tab_status(
                    &state,
                    &ui,
                    tab_id,
                    "Saved password was not found; enter it again",
                );
                refresh_workspace(&ui, &state);
            }
            Err(error) => {
                warn!(session_id = %profile.id, %error, "system credential lookup failed");
                if !set_awaiting_authentication(&state, tab_id, profile.id, false) {
                    return;
                }
                set_tab_status(
                    &state,
                    &ui,
                    tab_id,
                    &format!("System credential unavailable; enter password: {error}"),
                );
                refresh_workspace(&ui, &state);
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
    secret: zeroize::Zeroizing<String>,
    credential_to_store: Option<PendingCredentialStore>,
    used_stored_credential: bool,
    source: AuthenticationStart,
) -> Result<()> {
    let attempt_id = Uuid::new_v4();
    let (profile, events, credential_to_store) = {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        if !terminal_has_phase(&app, tab_id, profile_id, |phase| match source {
            AuthenticationStart::Prompt => {
                matches!(phase, SshConnectionPhase::AwaitingAuthentication { .. })
            }
            AuthenticationStart::StoredCredential => {
                matches!(phase, SshConnectionPhase::LoadingStoredCredential)
            }
        }) {
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
        terminal.set_ssh_phase(SshConnectionPhase::Idle);
        (profile, events, credential_to_store)
    };

    refresh_workspace(&ui, &state);
    spawn_session_monitor(
        runtime,
        state,
        ui,
        tab_id,
        profile,
        attempt_id,
        events,
        credential_to_store,
        used_stored_credential,
    );
    Ok(())
}

pub(super) fn wire_authentication(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    let ui_for_auth = ui.as_weak();
    let state_for_auth = state.clone();
    let runtime_for_auth = runtime.clone();
    ui.on_authenticate_session(move |password, vault_password, remember_password| {
        let password = zeroize::Zeroizing::new(password.as_str().to_owned());
        let vault_password = zeroize::Zeroizing::new(vault_password.as_str().to_owned());
        let (target, default_storage) = match state_for_auth.lock() {
            Ok(app) => {
                let target = app.active_tab_id().and_then(|tab_id| {
                    let terminal = app.terminal(tab_id)?;
                    let (profile_id, _) = terminal.ssh_route()?;
                    let vault_unlock_only = match terminal.ssh_phase()? {
                        SshConnectionPhase::AwaitingAuthentication { vault_unlock_only } => {
                            *vault_unlock_only
                        }
                        SshConnectionPhase::Idle
                        | SshConnectionPhase::Probing(_)
                        | SshConnectionPhase::AwaitingHostKey(_)
                        | SshConnectionPhase::LoadingStoredCredential => return None,
                    };
                    let profile = app
                        .sessions
                        .sessions
                        .iter()
                        .find(|profile| profile.id == profile_id)
                        .cloned()?;
                    Some((tab_id, profile, vault_unlock_only))
                });
                (target, app.sessions.settings.credential_storage)
            }
            Err(_) => {
                set_status(&ui_for_auth, "Cannot read session state");
                return false;
            }
        };
        let Some((tab_id, profile, vault_unlock_only)) = target else {
            set_status(&ui_for_auth, "No terminal tab is awaiting authentication");
            return false;
        };
        let password_auth = matches!(profile.auth, AuthMethod::Password);
        if vault_unlock_only {
            if vault_password.is_empty() {
                set_status(&ui_for_auth, "Enter the vault password to unlock the saved SSH password");
                return false;
            }
            if !set_loading_stored_credential(&state_for_auth, tab_id, profile.id) {
                set_status(&ui_for_auth, "Authentication request is no longer current");
                return false;
            }
            refresh_workspace(&ui_for_auth, &state_for_auth);
            let state = state_for_auth.clone();
            let ui = ui_for_auth.clone();
            let runtime = runtime_for_auth.clone();
            runtime_for_auth.spawn(async move {
                match load_vault_password(profile.id, vault_password).await {
                    Ok(Some(secret)) => {
                        if let Err(error) = start_session_worker(
                            &runtime,
                            state,
                            ui.clone(),
                            tab_id,
                            profile.id,
                            secret,
                            None,
                            true,
                            AuthenticationStart::StoredCredential,
                        ) {
                            set_status(&ui, &format!("Cannot start connection: {error}"));
                        }
                    }
                    Ok(None) => {
                        match set_credential_storage_while_loading(&state, tab_id, profile.id, None) {
                            Ok(true) => {}
                            Ok(false) => {
                                debug!(tab_id = %tab_id, session_id = %profile.id, "stale missing vault credential result ignored");
                                return;
                            }
                            Err(error) => {
                                warn!(session_id = %profile.id, %error, "failed to clear missing vault credential storage reference");
                            }
                        }
                        if !set_awaiting_authentication(&state, tab_id, profile.id, false) {
                            return;
                        }
                        set_tab_status(
                            &state,
                            &ui,
                            tab_id,
                            "Saved vault password was not found; enter the SSH password",
                        );
                        refresh_workspace(&ui, &state);
                    }
                    Err(error) => {
                        if !set_awaiting_authentication(&state, tab_id, profile.id, false) {
                            return;
                        }
                        set_tab_status(
                            &state,
                            &ui,
                            tab_id,
                            &format!(
                                "Cannot unlock saved password; enter the SSH password to continue: {error}"
                            ),
                        );
                        refresh_workspace(&ui, &state);
                    }
                }
            });
            return true;
        }
        if password_auth && password.is_empty() {
            set_status(&ui_for_auth, "Password cannot be empty");
            return false;
        }
        let credential_to_store = (password_auth && remember_password).then(|| PendingCredentialStore {
            storage: default_storage,
            previous_storage: profile.credential_storage,
            vault_password: (default_storage == CredentialStorage::EncryptedVault)
                .then(|| vault_password.clone()),
            secret: password.clone(),
        });
        if credential_to_store.as_ref().is_some_and(|store| {
            store.storage == CredentialStorage::EncryptedVault
                && store.vault_password.as_deref().is_none_or(String::is_empty)
        }) {
            set_status(&ui_for_auth, "Enter a vault password before remembering this SSH password");
            return false;
        }
        match start_session_worker(
            &runtime_for_auth,
            state_for_auth.clone(),
            ui_for_auth.clone(),
            tab_id,
            profile.id,
            password,
            credential_to_store,
            false,
            AuthenticationStart::Prompt,
        ) {
            Ok(()) => true,
            Err(error) => {
                set_status(&ui_for_auth, &format!("Cannot start connection: {error}"));
                false
            }
        }
    });

    let ui_for_cancel = ui.as_weak();
    let state_for_cancel = state.clone();
    ui.on_cancel_password_dialog(move || {
        let pending = match state_for_cancel.lock() {
            Ok(mut app) => {
                let tab_id = app.active_tab_id();
                let pending = tab_id.and_then(|tab_id| {
                    let terminal = app.terminal(tab_id)?;
                    let (profile_id, _) = terminal.ssh_route()?;
                    matches!(
                        terminal.ssh_phase(),
                        Some(SshConnectionPhase::AwaitingAuthentication { .. })
                    )
                    .then_some((tab_id, profile_id))
                });
                if let Some((tab_id, _)) = pending
                    && let Some(terminal) = app.terminal_mut(tab_id)
                {
                    terminal.set_ssh_phase(SshConnectionPhase::Idle);
                }
                pending
            }
            Err(_) => {
                set_status(&ui_for_cancel, "Cannot update session state");
                return;
            }
        };
        if let Some((tab_id, _)) = pending {
            set_tab_status(
                &state_for_cancel,
                &ui_for_cancel,
                tab_id,
                "Authentication cancelled",
            );
            refresh_workspace(&ui_for_cancel, &state_for_cancel);
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

#[derive(Clone, Copy)]
pub(super) enum AuthenticationStart {
    Prompt,
    StoredCredential,
}

fn terminal_has_phase(
    app: &AppState,
    tab_id: Uuid,
    profile_id: Uuid,
    predicate: impl FnOnce(&SshConnectionPhase) -> bool,
) -> bool {
    app.terminal(tab_id).is_some_and(|terminal| {
        terminal
            .ssh_route()
            .is_some_and(|route| route.0 == profile_id)
    }) && app
        .terminal(tab_id)
        .and_then(TerminalTabState::ssh_phase)
        .is_some_and(predicate)
}

fn set_loading_stored_credential(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    profile_id: Uuid,
) -> bool {
    let Ok(mut app) = state.lock() else {
        return false;
    };
    if !terminal_has_phase(&app, tab_id, profile_id, |phase| {
        matches!(phase, SshConnectionPhase::AwaitingAuthentication { .. })
    }) {
        return false;
    }
    app.terminal_mut(tab_id)
        .is_some_and(|terminal| terminal.set_ssh_phase(SshConnectionPhase::LoadingStoredCredential))
}

fn set_awaiting_authentication(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    profile_id: Uuid,
    vault_unlock_only: bool,
) -> bool {
    let Ok(mut app) = state.lock() else {
        return false;
    };
    if !terminal_has_phase(&app, tab_id, profile_id, |phase| {
        matches!(
            phase,
            SshConnectionPhase::AwaitingAuthentication { .. }
                | SshConnectionPhase::LoadingStoredCredential
        )
    }) {
        return false;
    }
    app.terminal_mut(tab_id).is_some_and(|terminal| {
        terminal.set_ssh_phase(SshConnectionPhase::AwaitingAuthentication { vault_unlock_only })
    })
}
