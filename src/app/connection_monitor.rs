use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_session_monitor(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    profile: SessionProfile,
    attempt_id: Uuid,
    mut events: mpsc::Receiver<SshSessionEvent>,
    mut credential_to_store: Option<String>,
    used_stored_credential: bool,
) {
    let runtime_for_monitor = runtime.clone();
    runtime.spawn(async move {
        let mut terminal_event = false;
        while let Some(event) = events.recv().await {
            match event {
                SshSessionEvent::Connected => {
                    let Some(active) = mutate_terminal_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        |terminal| {
                            terminal.connected = true;
                            terminal.worker_running = true;
                            terminal.status = format!("Connected to {}", profile_endpoint(&profile));
                        },
                    ) else {
                        continue;
                    };
                    info!(tab_id = %tab_id, session_id = %profile.id, "SSH worker reported connected");
                    if active {
                        dispatch_active_snapshot(&ui, &state);
                    }
                    refresh_workspace(&ui, &state);
                    if let Some(secret) = credential_to_store.take() {
                        persist_authenticated_credential(
                            &runtime_for_monitor,
                            state.clone(),
                            ui.clone(),
                            tab_id,
                            profile.id,
                            attempt_id,
                            secret,
                        );
                    }
                }
                SshSessionEvent::Output(data) => {
                    if let Some(true) = mutate_terminal_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        |terminal| terminal.terminal.process(&data),
                    ) {
                        dispatch_active_snapshot(&ui, &state);
                    }
                }
                // The resize callback updates the active model immediately after its request is
                // accepted. A delayed worker acknowledgement must not restore an older grid.
                SshSessionEvent::Resized { .. } => {}
                SshSessionEvent::Disconnected => {
                    terminal_event = true;
                    if retire_session_attempt(&state, tab_id, profile.id, attempt_id) {
                        set_tab_status(&state, &ui, tab_id, "Disconnected");
                        refresh_workspace(&ui, &state);
                    }
                }
                SshSessionEvent::HostKeyRejected { expected, actual } => {
                    terminal_event = true;
                    warn!(
                        tab_id = %tab_id,
                        session_id = %profile.id,
                        expected = ?expected,
                        fingerprint = %actual,
                        "SSH worker rejected host key"
                    );
                    let prompt = PendingHostKey {
                        tab_id,
                        profile_id: profile.id,
                        host: profile.host.clone(),
                        port: profile.port,
                        fingerprint: actual,
                        changed: expected.is_some(),
                    };
                    match prepare_host_key_retry(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        prompt.clone(),
                    ) {
                        Ok(true) => {}
                        Ok(false) => {
                            debug!(tab_id = %tab_id, %attempt_id, "stale host-key rejection ignored");
                            continue;
                        }
                        Err(error) => {
                            error!(tab_id = %tab_id, %error, "cannot prepare host-key retry");
                            continue;
                        }
                    }
                    show_host_key_prompt(&ui, &prompt);
                    set_tab_status(
                        &state,
                        &ui,
                        tab_id,
                        "SSH host key changed; verify it before reconnecting",
                    );
                    refresh_workspace(&ui, &state);
                }
                SshSessionEvent::AuthenticationFailed => {
                    terminal_event = true;
                    let retry_current = match prepare_authentication_retry(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        used_stored_credential,
                    ) {
                        Ok(current) => current,
                        Err(error) => {
                            error!(tab_id = %tab_id, %attempt_id, %error, "failed to prepare authentication retry");
                            false
                        }
                    };
                    if !retry_current {
                        continue;
                    }
                    let remember_password =
                        used_stored_credential || credential_to_store.take().is_some();
                    if used_stored_credential {
                        let session_id = profile.id;
                        runtime_for_monitor.spawn(async move {
                            if let Err(error) = delete_stored_password(session_id).await {
                                warn!(session_id = %session_id, %error, "failed to remove rejected stored credential");
                            }
                        });
                    }
                    show_auth_prompt(&ui, &profile, remember_password);
                    set_tab_status(
                        &state,
                        &ui,
                        tab_id,
                        if matches!(profile.auth, AuthMethod::PrivateKey { .. }) {
                            "The server rejected this private key"
                        } else if used_stored_credential {
                            "Saved password was rejected; enter a new password"
                        } else {
                            "Authentication failed; check the password and try again"
                        },
                    );
                    refresh_workspace(&ui, &state);
                }
                SshSessionEvent::PrivateKeyFailed(message) => {
                    terminal_event = true;
                    let retry_current = prepare_authentication_retry(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        false,
                    )
                    .unwrap_or(false);
                    if retry_current {
                        show_auth_prompt(&ui, &profile, false);
                        set_tab_status(
                            &state,
                            &ui,
                            tab_id,
                            &format!("Private key could not be loaded: {message}"),
                        );
                        refresh_workspace(&ui, &state);
                    }
                }
                SshSessionEvent::Failed(message) => {
                    terminal_event = true;
                    if retire_session_attempt(&state, tab_id, profile.id, attempt_id) {
                        set_tab_status(
                            &state,
                            &ui,
                            tab_id,
                            &format!("Connection failed: {message}"),
                        );
                        refresh_workspace(&ui, &state);
                    }
                }
            }
        }

        let retired = retire_session_attempt(&state, tab_id, profile.id, attempt_id);
        if !terminal_event && retired {
            set_tab_status(&state, &ui, tab_id, "SSH worker stopped");
            refresh_workspace(&ui, &state);
        }
        debug!(tab_id = %tab_id, session_id = %profile.id, "SSH event monitor stopped");
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn persist_authenticated_credential(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    session_id: Uuid,
    attempt_id: Uuid,
    secret: String,
) {
    runtime.spawn(async move {
        if let Err(error) = save_stored_password(session_id, secret).await {
            warn!(session_id = %session_id, %error, "failed to save authenticated credential");
            if session_attempt_is_active(&state, tab_id, session_id, attempt_id) {
                set_tab_status(
                    &state,
                    &ui,
                    tab_id,
                    &format!("Connected, but password could not be saved: {error}"),
                );
            }
            return;
        }

        if let Err(error) = set_credential_marker(&state, session_id, true) {
            warn!(session_id = %session_id, %error, "failed to persist credential marker");
            if let Err(cleanup_error) = delete_stored_password(session_id).await {
                warn!(session_id = %session_id, %cleanup_error, "failed to roll back credential after marker save failure");
            }
            if session_attempt_is_active(&state, tab_id, session_id, attempt_id) {
                set_tab_status(
                    &state,
                    &ui,
                    tab_id,
                    &format!("Connected, but password preference could not be saved: {error}"),
                );
            }
            return;
        }

        info!(session_id = %session_id, "authenticated password stored in system credential store");
        if session_attempt_is_active(&state, tab_id, session_id, attempt_id) {
            set_tab_status(
                &state,
                &ui,
                tab_id,
                "Connected; password saved in system credential store",
            );
        }
    });
}

pub(super) fn mutate_terminal_attempt(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    profile_id: Uuid,
    attempt_id: Uuid,
    action: impl FnOnce(&mut TerminalTabState),
) -> Option<bool> {
    let mut app = state.lock().ok()?;
    let current = app
        .terminal(tab_id)
        .and_then(TerminalTabState::ssh_route)
        .is_some_and(|route| route == (profile_id, Some(attempt_id)));
    if !current {
        return None;
    }
    action(app.terminal_mut(tab_id)?);
    Some(app.active_tab_id() == Some(tab_id))
}
