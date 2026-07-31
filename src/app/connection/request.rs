use super::*;

pub(in crate::app) fn wire_connection_request(
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
