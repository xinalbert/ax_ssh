use super::*;

enum ConnectionRequest {
    Ssh(ConnectionStart),
    Telnet {
        tab_id: Uuid,
        profile: SessionProfile,
    },
    Serial {
        tab_id: Uuid,
        profile: SessionProfile,
    },
}

pub(in crate::app) fn wire_connection_request(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
) {
    let ui_for_connect = ui.as_weak();
    let state_for_connect = state.clone();
    let runtime_for_connect = runtime.clone();
    ui.on_connect_session(move |id| {
        log_ui_action("connection.open-terminal");
        request_connection(
            &ui_for_connect,
            &state_for_connect,
            &runtime_for_connect,
            id.as_str(),
            ConnectionTarget::Terminal,
            None,
        );
    });

    let ui_for_sftp = ui.as_weak();
    let state_for_sftp = state.clone();
    let runtime_for_sftp = runtime.clone();
    ui.on_open_sftp_session(move |id| {
        log_ui_action("connection.open-sftp-session");
        request_connection(
            &ui_for_sftp,
            &state_for_sftp,
            &runtime_for_sftp,
            id.as_str(),
            ConnectionTarget::Sftp,
            None,
        );
    });

    let ui_for_active_sftp = ui.as_weak();
    let state_for_active_sftp = state;
    ui.on_open_sftp(move || {
        log_ui_action("connection.switch-ssh-sftp");
        let navigation = match state_for_active_sftp.lock() {
            Ok(mut app) => app.switch_ssh_sftp_tab(),
            Err(_) => {
                set_status(&ui_for_active_sftp, "Cannot read active SSH session");
                return;
            }
        };
        let Some(navigation) = navigation else {
            set_status(&ui_for_active_sftp, "Select an SSH or SFTP tab first");
            return;
        };
        match navigation {
            SshSftpNavigation::Activated(_) => {
                refresh_workspace(&ui_for_active_sftp, &state_for_active_sftp);
            }
            SshSftpNavigation::Connect {
                profile_id,
                target,
                companion_tab_id,
            } => request_profile_connection(
                &ui_for_active_sftp,
                &state_for_active_sftp,
                &runtime,
                profile_id,
                target,
                Some(companion_tab_id),
                None,
            ),
        }
    });
}

fn request_connection(
    ui: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    runtime: &Handle,
    id: &str,
    target: ConnectionTarget,
    companion_tab_id: Option<Uuid>,
) {
    let profile_id = match parse_uuid(id, "session", ui) {
        Some(id) => id,
        None => return,
    };
    request_profile_connection(
        ui,
        state,
        runtime,
        profile_id,
        target,
        companion_tab_id,
        None,
    );
}

pub(in crate::app) fn request_profile_connection(
    ui: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    runtime: &Handle,
    profile_id: Uuid,
    target: ConnectionTarget,
    companion_tab_id: Option<Uuid>,
    one_time_password: Option<zeroize::Zeroizing<String>>,
) {
    let start = {
        let mut app = match state.lock() {
            Ok(app) => app,
            Err(_) => {
                set_status(ui, "Cannot read session state");
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
            set_status(ui, "Session not found");
            return;
        };
        if target == ConnectionTarget::Sftp && profile.ssh().is_none() {
            set_status(ui, "SFTP is available only for SSH servers");
            return;
        }
        let tab_id = match target {
            ConnectionTarget::Terminal => {
                app.open_terminal_tab_with_companion(&profile, companion_tab_id)
            }
            ConnectionTarget::Sftp => app.open_sftp_tab_with_companion(&profile, companion_tab_id),
        };
        if let Some(password) = one_time_password
            && profile.ssh().is_some()
            && let Some(terminal) = app.terminal_mut(tab_id)
        {
            terminal.set_pending_auth_secret(password);
        }
        match &profile.connection {
            ConnectionProfile::Ssh(ssh) if ssh.host_key_fingerprint.is_some() => {
                let Some(terminal) = app.terminal_mut(tab_id) else {
                    set_status(ui, "Cannot prepare SSH tab");
                    return;
                };
                terminal.set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
                    vault_unlock_only: false,
                });
                ConnectionRequest::Ssh(ConnectionStart::Authenticate {
                    tab_id,
                    profile,
                    target,
                })
            }
            ConnectionProfile::Ssh(_) => {
                let (cancel, cancelled) = oneshot::channel();
                let probe = PendingProbe {
                    tab_id,
                    profile_id: profile.id,
                    cancel,
                };
                let Some(terminal) = app.terminal_mut(tab_id) else {
                    set_status(ui, "Cannot prepare SSH tab");
                    return;
                };
                terminal.set_ssh_phase(SshConnectionPhase::Probing(probe));
                ConnectionRequest::Ssh(ConnectionStart::Probe {
                    tab_id,
                    profile,
                    cancelled,
                    target,
                })
            }
            ConnectionProfile::Telnet(_) => ConnectionRequest::Telnet { tab_id, profile },
            ConnectionProfile::Serial(_) => ConnectionRequest::Serial { tab_id, profile },
        }
    };
    refresh_workspace(ui, state);

    let start = match start {
        ConnectionRequest::Telnet { tab_id, profile } => {
            if let Err(error) =
                start_telnet_connection(runtime, state.clone(), ui.clone(), tab_id, profile)
            {
                set_tab_status(
                    state,
                    ui,
                    tab_id,
                    &format!("Cannot start Telnet connection: {error}"),
                );
            }
            return;
        }
        ConnectionRequest::Serial { tab_id, profile } => {
            if let Err(error) =
                start_serial_connection(runtime, state.clone(), ui.clone(), tab_id, profile)
            {
                set_tab_status(
                    state,
                    ui,
                    tab_id,
                    &format!("Cannot start serial connection: {error}"),
                );
            }
            return;
        }
        ConnectionRequest::Ssh(start) => start,
    };

    let (tab_id, profile, cancelled, target) = match start {
        ConnectionStart::Authenticate {
            tab_id,
            profile,
            target,
        } => {
            begin_authentication(runtime, state.clone(), ui.clone(), tab_id, profile, target);
            return;
        }
        ConnectionStart::Probe {
            tab_id,
            profile,
            cancelled,
            target,
        } => (tab_id, profile, cancelled, target),
    };

    set_tab_status(state, ui, tab_id, "Checking SSH host key...");
    let Some(ssh) = profile.ssh() else {
        set_tab_status(state, ui, tab_id, "SSH profile is no longer available");
        return;
    };
    let host = ssh.host.clone();
    let port = ssh.port;
    info!(
        tab_id = %tab_id,
        session_id = %profile.id,
        host = %host,
        port,
        "probing unknown SSH host key"
    );
    let state_for_probe = state.clone();
    let ui_for_probe = ui.clone();
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
                    && terminal.connection_target() == target
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
                                    host: host.clone(),
                                    port,
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
}
