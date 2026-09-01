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

#[derive(Clone)]
pub(in crate::app) struct ConnectionContext {
    ui: slint::Weak<AppWindow>,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    font_registry: Arc<Mutex<FontRegistry>>,
    terminal_font_started: Arc<std::sync::atomic::AtomicBool>,
}

impl ConnectionContext {
    pub(in crate::app) fn new(
        ui: slint::Weak<AppWindow>,
        state: Arc<Mutex<AppState>>,
        runtime: Handle,
        font_registry: Arc<Mutex<FontRegistry>>,
        terminal_font_started: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self {
            ui,
            state,
            runtime,
            font_registry,
            terminal_font_started,
        }
    }
}

pub(in crate::app) fn wire_connection_request(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    font_registry: Arc<Mutex<FontRegistry>>,
    terminal_font_started: Arc<std::sync::atomic::AtomicBool>,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let context = ConnectionContext::new(
        ui.as_weak(),
        state,
        runtime,
        font_registry,
        terminal_font_started,
    );
    let context_for_connect = context.clone();
    let router_for_connect = window_router.clone();
    ui.on_connect_session(move |id| {
        log_ui_action("connection.open-terminal");
        let Some(_) = request_connection(
            &context_for_connect,
            id.as_str(),
            ConnectionTarget::Terminal,
            None,
            {
                let router = router_for_connect.clone();
                move |tab_id, app| router.activate_tab(window_id, tab_id, app)
            },
        ) else {
            return;
        };
    });

    let context_for_sftp = context.clone();
    let router_for_sftp = window_router.clone();
    ui.on_open_sftp_session(move |id| {
        log_ui_action("connection.open-sftp-session");
        let Some(_) = request_connection(
            &context_for_sftp,
            id.as_str(),
            ConnectionTarget::Sftp,
            None,
            {
                let router = router_for_sftp.clone();
                move |tab_id, app| router.activate_tab(window_id, tab_id, app)
            },
        ) else {
            return;
        };
    });

    let context_for_active_sftp = context;
    let router_for_active_sftp = window_router;
    ui.on_open_sftp(move || {
        log_ui_action("connection.switch-ssh-sftp");
        sync_window_active(
            &router_for_active_sftp,
            window_id,
            &context_for_active_sftp.state,
        );
        let navigation = match context_for_active_sftp.state.lock() {
            Ok(mut app) => app.switch_ssh_sftp_tab(),
            Err(_) => {
                set_status(
                    &context_for_active_sftp.ui,
                    "Cannot read active SSH session",
                );
                return;
            }
        };
        let Some(navigation) = navigation else {
            set_status(
                &context_for_active_sftp.ui,
                "Select an SSH or SFTP tab first",
            );
            return;
        };
        match navigation {
            SshSftpNavigation::Activated(tab_id) => {
                router_for_active_sftp.set_active(window_id, tab_id);
                refresh_workspace(&context_for_active_sftp.ui, &context_for_active_sftp.state);
            }
            SshSftpNavigation::Connect {
                profile_id,
                target,
                companion_tab_id,
            } => {
                let _ = request_profile_connection(
                    &context_for_active_sftp,
                    profile_id,
                    target,
                    Some(companion_tab_id),
                    None,
                    None,
                    {
                        let router = router_for_active_sftp.clone();
                        move |new_tab_id, app| {
                            router.include_tab(window_id, new_tab_id)
                                && router.activate_tab(window_id, new_tab_id, app)
                        }
                    },
                );
            }
        }
    });
}

fn request_connection<F>(
    context: &ConnectionContext,
    id: &str,
    target: ConnectionTarget,
    companion_tab_id: Option<Uuid>,
    register_tab: F,
) -> Option<Uuid>
where
    F: FnOnce(Uuid, &mut AppState) -> bool,
{
    let profile_id = parse_uuid(id, "session", &context.ui)?;
    request_profile_connection(
        context,
        profile_id,
        target,
        companion_tab_id,
        None,
        None,
        register_tab,
    )
}

pub(in crate::app) fn request_profile_connection<F>(
    context: &ConnectionContext,
    profile_id: Uuid,
    target: ConnectionTarget,
    companion_tab_id: Option<Uuid>,
    sftp_initial_path: Option<String>,
    one_time_password: Option<zeroize::Zeroizing<String>>,
    register_tab: F,
) -> Option<Uuid>
where
    F: FnOnce(Uuid, &mut AppState) -> bool,
{
    let ConnectionContext {
        ui,
        state,
        runtime,
        font_registry,
        terminal_font_started,
    } = context;
    if target == ConnectionTarget::Terminal {
        load_terminal_font_on_demand(
            runtime,
            ui.clone(),
            state.clone(),
            font_registry.clone(),
            terminal_font_started.clone(),
        );
    }
    let start = {
        let mut app = match state.lock() {
            Ok(app) => app,
            Err(_) => {
                set_status(ui, "Cannot read session state");
                return None;
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
            return None;
        };
        if target == ConnectionTarget::Sftp && profile.ssh().is_none() {
            set_status(ui, "SFTP is available only for SSH servers");
            return None;
        }
        let tab_id = match target {
            ConnectionTarget::Terminal => {
                app.open_terminal_tab_with_companion(&profile, companion_tab_id)
            }
            ConnectionTarget::Sftp => app.open_sftp_tab_with_companion_at_path(
                &profile,
                companion_tab_id,
                sftp_initial_path,
            ),
        };
        if let Some(password) = one_time_password
            && profile.ssh().is_some()
            && let Some(terminal) = app.terminal_mut(tab_id)
        {
            terminal.set_pending_auth_secret(password);
        }
        if !register_tab(tab_id, &mut app) {
            let _ = app.close_tab(tab_id);
            set_status(
                ui,
                "Cannot attach connection to the requested terminal pane",
            );
            return None;
        }
        match &profile.connection {
            ConnectionProfile::Ssh(ssh) if ssh.host_key_fingerprint.is_some() => {
                let Some(terminal) = app.terminal_mut(tab_id) else {
                    set_status(ui, "Cannot prepare SSH tab");
                    return None;
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
                    return None;
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
            return Some(tab_id);
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
            return Some(tab_id);
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
            return Some(tab_id);
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
        return Some(tab_id);
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
    let runtime_for_probe = runtime.clone();
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
                let current = terminal
                    .ssh_route()
                    .is_some_and(|route| route.0 == profile.id)
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
                        Some(Ok(probe))
                            if probe.decision == ax_ssh::ssh::TrustDecision::Trusted =>
                        {
                            terminal.set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
                                vault_unlock_only: false,
                            });
                            Some(Ok(true))
                        }
                        Some(Ok(probe)) => {
                            terminal.set_ssh_phase(SshConnectionPhase::AwaitingHostKey(
                                PendingHostKey {
                                    tab_id,
                                    profile_id: profile.id,
                                    host: host.clone(),
                                    port,
                                    fingerprint: probe.fingerprint,
                                    public_key: probe.public_key,
                                    changed: probe.decision == ax_ssh::ssh::TrustDecision::Changed,
                                    revoked: probe.decision == ax_ssh::ssh::TrustDecision::Revoked,
                                },
                            ));
                            Some(Ok(false))
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
            Some(Ok(true)) => {
                begin_authentication(
                    &runtime_for_probe,
                    state_for_probe.clone(),
                    ui_for_probe.clone(),
                    tab_id,
                    profile.clone(),
                    target,
                );
            }
            Some(Ok(false)) => {
                set_tab_status(
                    &state_for_probe,
                    &ui_for_probe,
                    tab_id,
                    "Verify the SSH host key before connecting",
                );
                refresh_workspace(&ui_for_probe, &state_for_probe);
            }
            Some(Err(error)) => {
                warn!(
                    tab_id = %tab_id,
                    session_id = %profile.id,
                    error = ?error,
                    "SSH host-key probe failed"
                );
                set_tab_status(
                    &state_for_probe,
                    &ui_for_probe,
                    tab_id,
                    &format!("Host-key check failed: {error:#}"),
                );
            }
            None => debug!(tab_id = %tab_id, "cancelled or stale host-key probe result ignored"),
        }
    });
    Some(tab_id)
}

pub(in crate::app) fn resume_existing_connection(
    context: &ConnectionContext,
    tab_id: Uuid,
    profile_id: Uuid,
    target: ConnectionTarget,
) {
    let Some(profile) = context.state.lock().ok().and_then(|app| {
        app.sessions
            .sessions
            .iter()
            .find(|profile| profile.id == profile_id)
            .cloned()
    }) else {
        return;
    };
    match profile.connection {
        ConnectionProfile::Ssh(ref ssh) if ssh.host_key_fingerprint.is_some() => {
            if let Ok(mut app) = context.state.lock()
                && let Some(terminal) = app.terminal_mut(tab_id)
            {
                terminal.set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
                    vault_unlock_only: false,
                });
                terminal.status = "Restored; authenticating...".to_owned();
            }
            begin_authentication(
                &context.runtime,
                context.state.clone(),
                context.ui.clone(),
                tab_id,
                profile,
                target,
            );
        }
        ConnectionProfile::Ssh(_) => probe_existing_connection(context, tab_id, profile, target),
        ConnectionProfile::Telnet(_) => {
            if let Err(error) = start_telnet_connection(
                &context.runtime,
                context.state.clone(),
                context.ui.clone(),
                tab_id,
                profile,
            ) {
                set_tab_status(
                    &context.state,
                    &context.ui,
                    tab_id,
                    &format!("Reconnect unavailable: {error}"),
                );
            }
        }
        ConnectionProfile::Serial(_) => {
            if let Err(error) = start_serial_connection(
                &context.runtime,
                context.state.clone(),
                context.ui.clone(),
                tab_id,
                profile,
            ) {
                set_tab_status(
                    &context.state,
                    &context.ui,
                    tab_id,
                    &format!("Reconnect unavailable: {error}"),
                );
            }
        }
    }
}

fn probe_existing_connection(
    context: &ConnectionContext,
    tab_id: Uuid,
    profile: SessionProfile,
    target: ConnectionTarget,
) {
    let Some(ssh) = profile.ssh() else {
        set_tab_status(
            &context.state,
            &context.ui,
            tab_id,
            "Restored SSH profile is invalid",
        );
        return;
    };
    let (cancel, cancelled) = oneshot::channel();
    let host = ssh.host.clone();
    let port = ssh.port;
    let Ok(mut app) = context.state.lock() else {
        set_status(&context.ui, "Cannot read restored session state");
        return;
    };
    let Some(terminal) = app.terminal_mut(tab_id) else {
        return;
    };
    terminal.set_ssh_phase(SshConnectionPhase::Probing(PendingProbe {
        tab_id,
        profile_id: profile.id,
        cancel,
    }));
    terminal.status = "Checking SSH host key before reconnecting...".to_owned();
    drop(app);
    let state = context.state.clone();
    let ui = context.ui.clone();
    let runtime_for_probe = context.runtime.clone();
    context.runtime.spawn(async move {
        let result = tokio::select! {
            _ = cancelled => None,
            result = probe_host_key(&profile) => Some(result),
        };
        let outcome = match state.lock() {
            Ok(mut app) => {
                let Some(terminal) = app.terminal_mut(tab_id) else {
                    return;
                };
                let current = terminal
                    .ssh_route()
                    .is_some_and(|route| route.0 == profile.id)
                    && terminal.connection_target() == target
                    && matches!(terminal.ssh_phase(), Some(SshConnectionPhase::Probing(_)));
                if !current {
                    None
                } else {
                    match result {
                        Some(Ok(probe))
                            if probe.decision == ax_ssh::ssh::TrustDecision::Trusted =>
                        {
                            terminal.set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
                                vault_unlock_only: false,
                            });
                            Some(Ok(true))
                        }
                        Some(Ok(probe)) => {
                            terminal.set_ssh_phase(SshConnectionPhase::AwaitingHostKey(
                                PendingHostKey {
                                    tab_id,
                                    profile_id: profile.id,
                                    host: host.clone(),
                                    port,
                                    fingerprint: probe.fingerprint,
                                    public_key: probe.public_key,
                                    changed: probe.decision == ax_ssh::ssh::TrustDecision::Changed,
                                    revoked: probe.decision == ax_ssh::ssh::TrustDecision::Revoked,
                                },
                            ));
                            Some(Ok(false))
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
            Some(Ok(true)) => {
                begin_authentication(
                    &runtime_for_probe,
                    state.clone(),
                    ui.clone(),
                    tab_id,
                    profile.clone(),
                    target,
                );
            }
            Some(Ok(false)) => {
                set_tab_status(
                    &state,
                    &ui,
                    tab_id,
                    "Verify the SSH host key before reconnecting",
                );
                refresh_workspace(&ui, &state);
            }
            Some(Err(error)) => set_tab_status(
                &state,
                &ui,
                tab_id,
                &format!("Host-key check failed: {error:#}"),
            ),
            None => {}
        }
    });
}
