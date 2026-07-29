//! Slint application controller.
//!
//! This module is the only place that knows about generated Slint types. It
//! maps user intent to domain operations and returns owned worker events to the
//! Slint event loop.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use slint::platform::Key;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use tokio::runtime::{Handle, Runtime};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Duration;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use ax_ssh::config::{
    AppSettings, AuthMethod, ConfigStore, SessionProfile, SessionStore, normalize_group_name,
};
use ax_ssh::ssh::{SshSessionEvent, SshSessionHandle, discover_private_keys, probe_host_key};
use ax_ssh::terminal::{TerminalKey, TerminalModifiers, encode_key as encode_terminal_key};

use self::credential_tasks::{
    delete_password as delete_stored_password, load_password as load_stored_password,
    save_password as save_stored_password,
};
use self::session_groups::{group_options, profile_endpoint, session_groups};
use self::state::{
    ActiveTabSnapshot, AppState, ConnectionStart, PendingAuth, PendingHostKey, PendingProbe,
    prepare_authentication_retry, prepare_host_key_retry, retire_session_attempt,
    session_attempt_is_active, set_credential_marker,
};

mod credential_tasks;
mod session_groups;
mod state;

slint::include_modules!();

pub fn run() -> Result<()> {
    let config_path = ConfigStore::default_path()?;
    let config = ConfigStore::new(config_path);
    let sessions = config.load().context("failed to load session profiles")?;
    let runtime = Runtime::new().context("failed to start Tokio runtime")?;
    let state = Arc::new(Mutex::new(AppState::new(config, sessions)));
    let ui = AppWindow::new().context("failed to create Slint window")?;

    let (rows, groups, settings) = {
        let app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        (
            session_rows(&app.sessions, &app.expanded_groups),
            group_option_rows(&app.sessions),
            app.sessions.settings.clone(),
        )
    };
    ui.set_sessions(ModelRc::new(VecModel::from(rows)));
    ui.set_group_options(ModelRc::new(VecModel::from(groups)));
    ui.set_private_key_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_terminal_font_options(ModelRc::new(VecModel::from(vec![
        SharedString::from("JetBrains Mono"),
        SharedString::from("monospace"),
    ])));
    apply_settings_to_component(&ui, &settings);
    apply_active_snapshot(&ui, ActiveTabSnapshot::default());
    ui.set_workspace_tabs(ModelRc::new(VecModel::from(Vec::<WorkspaceTabRow>::new())));
    ui.set_status("Ready".into());

    wire_callbacks(&ui, state.clone(), runtime.handle().clone());
    load_private_key_options(runtime.handle(), ui.as_weak());
    info!("AxSSH UI initialized");
    let ui_result = ui.run().context("Slint event loop failed");

    let (workers, pending_probe) = {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned during shutdown"))?;
        app.drain_runtime_resources()
    };
    if let Some(pending_probe) = pending_probe
        && pending_probe.cancel.send(()).is_err()
    {
        debug!(
            tab_id = %pending_probe.tab_id,
            session_id = %pending_probe.profile_id,
            "host-key probe already stopped during shutdown"
        );
    }
    for worker in workers {
        if let Err(error) = runtime.block_on(worker.shutdown()) {
            warn!(%error, "failed to shut down SSH worker cleanly");
        }
    }

    drop(ui);
    runtime.shutdown_timeout(Duration::from_secs(3));
    ui_result?;
    info!("AxSSH UI stopped");
    Ok(())
}

fn session_rows(sessions: &SessionStore, expanded_groups: &BTreeSet<String>) -> Vec<SessionRow> {
    let mut rows = Vec::new();
    for group in session_groups(sessions) {
        let group_name = group.name;
        let profiles = group.profiles;
        let expanded = expanded_groups.contains(&group_name);
        rows.push(SessionRow {
            id: "".into(),
            group_name: group_name.clone().into(),
            name: if group_name.is_empty() {
                "Ungrouped".into()
            } else {
                group_name.clone().into()
            },
            endpoint: profiles.len().to_string().into(),
            is_group: true,
            expanded,
        });
        if expanded {
            rows.extend(profiles.into_iter().map(|profile| SessionRow {
                id: profile.id.to_string().into(),
                group_name: group_name.clone().into(),
                name: profile.name.clone().into(),
                endpoint: profile_endpoint(profile).into(),
                is_group: false,
                expanded: false,
            }));
        }
    }
    rows
}

fn group_option_rows(sessions: &SessionStore) -> Vec<SharedString> {
    group_options(sessions)
        .into_iter()
        .map(SharedString::from)
        .collect()
}

fn wire_callbacks(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    wire_workspace_tabs(ui, state.clone(), runtime.clone());
    wire_session_editor(ui, state.clone(), runtime.clone());
    wire_connection_request(ui, state.clone(), runtime.clone());
    wire_host_key_confirmation(ui, state.clone(), runtime.clone());
    wire_authentication(ui, state.clone(), runtime.clone());
    wire_settings(ui, state.clone(), runtime);
    wire_terminal(ui, state);
}

fn wire_workspace_tabs(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    let ui_for_settings = ui.as_weak();
    let state_for_settings = state.clone();
    ui.on_open_settings(move || {
        match state_for_settings.lock() {
            Ok(mut app) => {
                app.open_settings_tab();
            }
            Err(_) => {
                set_status(&ui_for_settings, "Cannot update workspace tabs");
                return;
            }
        }
        refresh_workspace(&ui_for_settings, &state_for_settings);
    });

    let ui_for_new = ui.as_weak();
    let state_for_new = state.clone();
    ui.on_new_session(move || {
        match state_for_new.lock() {
            Ok(mut app) => {
                app.open_session_editor_tab();
            }
            Err(_) => {
                set_status(&ui_for_new, "Cannot update workspace tabs");
                return;
            }
        }
        refresh_workspace(&ui_for_new, &state_for_new);
    });

    let ui_for_activate = ui.as_weak();
    let state_for_activate = state.clone();
    ui.on_activate_tab(move |id| {
        let id = match parse_uuid(id.as_str(), "tab", &ui_for_activate) {
            Some(id) => id,
            None => return,
        };
        let activated = state_for_activate
            .lock()
            .is_ok_and(|mut app| app.activate_tab(id));
        if !activated {
            set_status(&ui_for_activate, "Tab not found");
            return;
        }
        refresh_workspace(&ui_for_activate, &state_for_activate);
    });

    let ui_for_close = ui.as_weak();
    let state_for_close = state.clone();
    let runtime_for_close = runtime.clone();
    ui.on_close_tab(move |id| {
        let id = match parse_uuid(id.as_str(), "tab", &ui_for_close) {
            Some(id) => id,
            None => return,
        };
        close_workspace_tab(id, &state_for_close, &ui_for_close, &runtime_for_close);
    });

    let ui_for_cancel_editor = ui.as_weak();
    let state_for_cancel_editor = state;
    ui.on_cancel_session_dialog(move || {
        let active_id = state_for_cancel_editor
            .lock()
            .ok()
            .and_then(|app| app.active_tab_id());
        if let Some(active_id) = active_id {
            close_workspace_tab(
                active_id,
                &state_for_cancel_editor,
                &ui_for_cancel_editor,
                &runtime,
            );
        }
    });
}

fn close_workspace_tab(
    tab_id: Uuid,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    runtime: &Handle,
) {
    let closed = match state.lock() {
        Ok(mut app) => app.close_tab(tab_id),
        Err(_) => {
            set_status(ui, "Cannot update workspace tabs");
            return;
        }
    };
    let Some(closed) = closed else {
        set_status(ui, "Tab not found");
        return;
    };
    if let Some(probe) = closed.pending_probe
        && probe.cancel.send(()).is_err()
    {
        debug!(tab_id = %tab_id, "host-key probe already stopped while closing tab");
    }
    if closed.dismissed_prompt {
        set_dialog_open(ui, Dialog::HostKey, false);
        set_dialog_open(ui, Dialog::Password, false);
    }
    if let Some(worker) = closed.worker {
        let ui = ui.clone();
        runtime.spawn(async move {
            if let Err(error) = worker.shutdown().await {
                warn!(tab_id = %tab_id, %error, "failed to shut down closed tab worker");
                set_status(&ui, &format!("Cannot close SSH worker cleanly: {error}"));
            }
        });
    }
    refresh_workspace(ui, state);
}

fn wire_session_editor(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    let ui_for_save = ui.as_weak();
    let state_for_save = state.clone();
    ui.on_save_session(
        move |name,
              group_name,
              host,
              port,
              username,
              auth_method,
              private_key_path,
              password,
              remember_password| {
            let parsed_port = match port.trim().parse::<u16>() {
                Ok(port) if port > 0 => port,
                _ => {
                    set_status(&ui_for_save, "Port must be a number between 1 and 65535");
                    return;
                }
            };
            let private_key = auth_method.as_str() == "Private key";
            if !private_key && remember_password && password.is_empty() {
                set_status(
                    &ui_for_save,
                    "Enter a password before enabling password storage",
                );
                return;
            }

            let mut profile = SessionProfile::new(name.as_str(), host.as_str(), username.as_str());
            profile = SessionProfile {
                group_name: normalize_group_name(group_name.as_str()),
                port: parsed_port,
                auth: if private_key {
                    AuthMethod::PrivateKey {
                        path: PathBuf::from(private_key_path.trim()),
                    }
                } else {
                    AuthMethod::Password
                },
                credential_stored: !private_key && remember_password,
                ..profile
            };
            let profile_id = profile.id;
            if let Err(error) = profile.validate() {
                set_status(&ui_for_save, &format!("Cannot save session: {error}"));
                return;
            }
            let editor_tab_id = state_for_save
                .lock()
                .ok()
                .and_then(|app| app.active_tab_id());
            let secret = password.as_str().to_owned();
            let state = state_for_save.clone();
            let ui = ui_for_save.clone();
            set_status(&ui_for_save, "Saving session...");
            runtime.spawn(async move {
                if !private_key
                    && remember_password
                    && let Err(error) = save_stored_password(profile_id, secret).await
                {
                    warn!(session_id = %profile_id, %error, "failed to save session credential");
                    set_status(&ui, &format!("Cannot save password: {error}"));
                    return;
                }

                let save_result = (|| -> Result<()> {
                    let mut app = state
                        .lock()
                        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
                    let mut candidate = app.sessions.clone();
                    candidate.upsert(profile.clone());
                    app.config.save(&candidate)?;
                    app.sessions = candidate;
                    app.expanded_groups.insert(profile.group_name.clone());
                    Ok(())
                })();

                if let Err(error) = save_result {
                    if !private_key
                        && remember_password
                        && let Err(cleanup_error) = delete_stored_password(profile_id).await
                    {
                        warn!(
                            session_id = %profile_id,
                            %cleanup_error,
                            "failed to roll back credential after profile save failure"
                        );
                    }
                    set_status(&ui, &format!("Cannot save session: {error}"));
                    return;
                }

                info!(
                    session_id = %profile_id,
                    credential_stored = !private_key && remember_password,
                    private_key,
                    "session profile saved"
                );
                refresh_session_models(&ui, &state);
                if let Some(editor_tab_id) = editor_tab_id {
                    let _ = state.lock().map(|mut app| app.close_tab(editor_tab_id));
                }
                refresh_workspace(&ui, &state);
                set_status(&ui, "Session saved");
            });
        },
    );

    let ui_for_group = ui.as_weak();
    ui.on_toggle_group(move |group_name| {
        let group_name = normalize_group_name(group_name.as_str());
        match state.lock() {
            Ok(mut app) => {
                if !app.expanded_groups.insert(group_name.clone()) {
                    app.expanded_groups.remove(&group_name);
                }
            }
            Err(_) => {
                set_status(&ui_for_group, "Cannot update group state");
                return;
            }
        }
        refresh_session_models(&ui_for_group, &state);
    });
}

fn wire_connection_request(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
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

fn wire_host_key_confirmation(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
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

fn begin_authentication(
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
fn start_session_worker(
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
        terminal.attempt_id = Some(attempt_id);
        terminal.worker = Some(worker);
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

fn wire_authentication(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
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
                    .context("no active SSH terminal")?
                    .worker
                    .as_ref()
                    .context("active terminal has no SSH worker")?
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

fn wire_terminal(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_for_key = ui.as_weak();
    let state_for_key = state.clone();
    ui.on_terminal_key(move |text, alt, control, meta, shift| {
        if control && shift && text.as_str().eq_ignore_ascii_case("c") {
            return false;
        }
        let key = terminal_key_from_slint(text.as_str());
        let modifiers = TerminalModifiers {
            alt,
            control,
            meta,
            shift,
        };
        let Some(data) = encode_terminal_key(&key, modifiers) else {
            return false;
        };
        let result = state_for_key
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))
            .and_then(|app| {
                app.active_terminal()
                    .context("no active SSH terminal")?
                    .worker
                    .as_ref()
                    .context("active terminal has no SSH worker")?
                    .request_send(data)
            });
        if let Err(error) = result {
            set_status(&ui_for_key, &format!("Cannot send terminal input: {error}"));
        }
        true
    });

    let ui_for_resize = ui.as_weak();
    ui.on_resize_terminal(move |columns, rows| {
        let result = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))
            .and_then(|app| {
                app.active_terminal()
                    .context("no active SSH terminal")?
                    .worker
                    .as_ref()
                    .context("active terminal has no SSH worker")?
                    .request_resize(columns.max(1) as u32, rows.max(1) as u32)
            });
        if let Err(error) = result {
            debug!(%error, "terminal resize ignored");
            set_status(&ui_for_resize, &format!("Cannot resize terminal: {error}"));
        }
    });
}

fn terminal_key_from_slint(text: &str) -> TerminalKey {
    let special = [
        (Key::Return, TerminalKey::Return),
        (Key::Backspace, TerminalKey::Backspace),
        (Key::Tab, TerminalKey::Tab),
        (Key::Backtab, TerminalKey::Tab),
        (Key::Escape, TerminalKey::Escape),
        (Key::UpArrow, TerminalKey::Up),
        (Key::DownArrow, TerminalKey::Down),
        (Key::RightArrow, TerminalKey::Right),
        (Key::LeftArrow, TerminalKey::Left),
        (Key::Insert, TerminalKey::Insert),
        (Key::Delete, TerminalKey::Delete),
        (Key::Home, TerminalKey::Home),
        (Key::End, TerminalKey::End),
        (Key::PageUp, TerminalKey::PageUp),
        (Key::PageDown, TerminalKey::PageDown),
    ];
    special
        .into_iter()
        .find_map(|(slint_key, terminal_key)| {
            matches_slint_key(text, slint_key).then_some(terminal_key)
        })
        .unwrap_or_else(|| TerminalKey::Text(text.to_owned()))
}

fn matches_slint_key(text: &str, key: Key) -> bool {
    let mut characters = text.chars();
    characters.next() == Some(char::from(key)) && characters.next().is_none()
}

fn wire_settings(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    let ui_for_cancel = ui.as_weak();
    let state_for_cancel = state.clone();
    let runtime_for_cancel = runtime.clone();
    ui.on_cancel_settings(move || {
        let active_id = state_for_cancel
            .lock()
            .ok()
            .and_then(|app| app.active_tab_id());
        if let Some(active_id) = active_id {
            close_workspace_tab(
                active_id,
                &state_for_cancel,
                &ui_for_cancel,
                &runtime_for_cancel,
            );
        }
    });

    let ui_for_save = ui.as_weak();
    ui.on_save_settings(
        move |font_family,
              font_size,
              scrollback_lines,
              default_columns,
              default_rows,
              sidebar_width,
              tab_width| {
            let settings = AppSettings::normalized(
                font_family.as_str(),
                font_size,
                scrollback_lines,
                default_columns,
                default_rows,
                sidebar_width,
                tab_width,
            );
            let state = state.clone();
            let ui = ui_for_save.clone();
            set_status(&ui_for_save, "Saving workspace settings...");
            runtime.spawn(async move {
                let save_result = (|| -> Result<()> {
                    let mut app = state
                        .lock()
                        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
                    let mut candidate = app.sessions.clone();
                    candidate.settings = settings.clone();
                    app.config.save(&candidate)?;
                    app.sessions = candidate;
                    app.apply_scrollback_setting();
                    Ok(())
                })();
                match save_result {
                    Ok(()) => {
                        apply_settings(&ui, settings);
                        refresh_workspace(&ui, &state);
                        set_status(&ui, "Workspace settings saved");
                    }
                    Err(error) => {
                        set_status(&ui, &format!("Cannot save workspace settings: {error}"));
                    }
                }
            });
        },
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_session_monitor(
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
                    let Some(snapshot) = mutate_terminal_attempt(
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
                    if let Some(snapshot) = snapshot {
                        dispatch_active_snapshot(&ui, snapshot);
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
                    if let Some(Some(snapshot)) = mutate_terminal_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        |terminal| terminal.terminal.process(&data),
                    ) {
                        dispatch_active_snapshot(&ui, snapshot);
                    }
                }
                SshSessionEvent::Resized { columns, rows } => {
                    if let Some(Some(snapshot)) = mutate_terminal_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        |terminal| terminal.terminal.resize(columns as usize, rows as usize),
                    ) {
                        dispatch_active_snapshot(&ui, snapshot);
                    }
                }
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
fn persist_authenticated_credential(
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

fn mutate_terminal_attempt(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    profile_id: Uuid,
    attempt_id: Uuid,
    action: impl FnOnce(&mut state::TerminalTabState),
) -> Option<Option<ActiveTabSnapshot>> {
    let mut app = state.lock().ok()?;
    let current = app.terminal(tab_id).is_some_and(|terminal| {
        terminal.profile_id == profile_id && terminal.attempt_id == Some(attempt_id)
    });
    if !current {
        return None;
    }
    action(app.terminal_mut(tab_id)?);
    Some((app.active_tab_id() == Some(tab_id)).then(|| app.active_snapshot()))
}

fn refresh_session_models(ui: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let (rows, groups) = match state.lock() {
        Ok(app) => (
            session_rows(&app.sessions, &app.expanded_groups),
            group_option_rows(&app.sessions),
        ),
        Err(_) => {
            set_status(ui, "State lock poisoned");
            return;
        }
    };
    dispatch_ui(ui, move |ui| {
        ui.set_sessions(ModelRc::new(VecModel::from(rows)));
        ui.set_group_options(ModelRc::new(VecModel::from(groups)));
    });
}

fn refresh_workspace(ui: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let (tabs, snapshot) = match state.lock() {
        Ok(app) => {
            let tabs = app
                .tab_summaries()
                .into_iter()
                .map(|tab| WorkspaceTabRow {
                    id: tab.id.to_string().into(),
                    title: tab.title.into(),
                    kind: tab.kind.into(),
                    connected: tab.connected,
                })
                .collect::<Vec<_>>();
            (tabs, app.active_snapshot())
        }
        Err(_) => {
            set_status(ui, "State lock poisoned");
            return;
        }
    };
    dispatch_ui(ui, move |ui| {
        ui.set_workspace_tabs(ModelRc::new(VecModel::from(tabs)));
        apply_active_snapshot(ui, snapshot);
    });
}

fn set_tab_status(
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    tab_id: Uuid,
    message: &str,
) {
    let snapshot = match state.lock() {
        Ok(mut app) => {
            let Some(terminal) = app.terminal_mut(tab_id) else {
                return;
            };
            terminal.status = message.to_owned();
            (app.active_tab_id() == Some(tab_id)).then(|| app.active_snapshot())
        }
        Err(_) => {
            set_status(ui, "State lock poisoned");
            return;
        }
    };
    if let Some(snapshot) = snapshot {
        dispatch_active_snapshot(ui, snapshot);
    }
}

fn dispatch_active_snapshot(ui: &slint::Weak<AppWindow>, snapshot: ActiveTabSnapshot) {
    dispatch_ui(ui, move |ui| apply_active_snapshot(ui, snapshot));
}

fn apply_active_snapshot(ui: &AppWindow, snapshot: ActiveTabSnapshot) {
    ui.set_active_tab_id(
        snapshot
            .id
            .map(|id| id.to_string())
            .unwrap_or_default()
            .into(),
    );
    ui.set_active_tab_kind(snapshot.kind.into());
    ui.set_active_tab_title(snapshot.title.into());
    ui.set_active_tab_status(snapshot.status.into());
    ui.set_terminal_output(snapshot.output.into());
    ui.set_connected(snapshot.connected);
    ui.set_worker_running(snapshot.worker_running);
}

fn apply_settings(ui: &slint::Weak<AppWindow>, settings: AppSettings) {
    dispatch_ui(ui, move |ui| apply_settings_to_component(ui, &settings));
}

fn apply_settings_to_component(ui: &AppWindow, settings: &AppSettings) {
    ui.set_terminal_font_family(settings.appearance.terminal_font_family.clone().into());
    ui.set_terminal_font_size(i32::from(settings.appearance.terminal_font_size));
    ui.set_scrollback_lines(settings.terminal.scrollback_lines as i32);
    ui.set_default_terminal_columns(i32::from(settings.terminal.default_columns));
    ui.set_default_terminal_rows(i32::from(settings.terminal.default_rows));
    ui.set_sidebar_width(i32::from(settings.workspace.sidebar_width));
    ui.set_tab_width(i32::from(settings.workspace.tab_width));
}

#[derive(Clone, Copy)]
enum Dialog {
    HostKey,
    Password,
}

fn set_dialog_open(ui: &slint::Weak<AppWindow>, dialog: Dialog, open: bool) {
    dispatch_ui(ui, move |ui| match dialog {
        Dialog::HostKey => ui.set_host_key_dialog_open(open),
        Dialog::Password => ui.set_password_dialog_open(open),
    });
}

fn show_host_key_prompt(ui: &slint::Weak<AppWindow>, prompt: &PendingHostKey) {
    let endpoint = format!("{}:{}", prompt.host, prompt.port);
    let fingerprint = prompt.fingerprint.clone();
    let changed = prompt.changed;
    dispatch_ui(ui, move |ui| {
        ui.set_host_key_endpoint(endpoint.into());
        ui.set_host_key_fingerprint(fingerprint.into());
        ui.set_host_key_changed(changed);
        ui.set_host_key_dialog_open(true);
    });
}

fn show_auth_prompt(
    ui: &slint::Weak<AppWindow>,
    profile: &SessionProfile,
    remember_password: bool,
) {
    let endpoint = profile_endpoint(profile);
    let (private_key, key_path) = match &profile.auth {
        AuthMethod::Password => (false, String::new()),
        AuthMethod::PrivateKey { path } => (true, path.display().to_string()),
    };
    dispatch_ui(ui, move |ui| {
        ui.set_password_endpoint(endpoint.into());
        ui.set_password_remember_default(!private_key && remember_password);
        ui.set_password_private_key(private_key);
        ui.set_password_key_path(key_path.into());
        ui.set_password_dialog_open(true);
    });
}

fn load_private_key_options(runtime: &Handle, ui: slint::Weak<AppWindow>) {
    runtime.spawn(async move {
        let result = tokio::task::spawn_blocking(discover_private_keys).await;
        match result {
            Ok(Ok(paths)) => {
                let options = paths
                    .into_iter()
                    .map(|path| SharedString::from(path.display().to_string()))
                    .collect::<Vec<_>>();
                dispatch_ui(&ui, move |ui| {
                    ui.set_private_key_options(ModelRc::new(VecModel::from(options)));
                });
            }
            Ok(Err(error)) => warn!(%error, "failed to discover local SSH private keys"),
            Err(error) => warn!(%error, "private-key discovery task failed"),
        }
    });
}

fn parse_uuid(value: &str, label: &str, ui: &slint::Weak<AppWindow>) -> Option<Uuid> {
    match value.parse::<Uuid>() {
        Ok(id) => Some(id),
        Err(error) => {
            set_status(ui, &format!("Invalid {label} id: {error}"));
            None
        }
    }
}

fn set_status(ui: &slint::Weak<AppWindow>, message: &str) {
    let message = message.to_owned();
    dispatch_ui(ui, move |ui| ui.set_status(message.into()));
}

fn dispatch_ui(ui: &slint::Weak<AppWindow>, action: impl FnOnce(&AppWindow) + Send + 'static) {
    let ui = ui.clone();
    if slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui.upgrade() {
            action(&ui);
        }
    })
    .is_err()
    {
        debug!("Slint event loop is no longer available for UI update");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_rows_group_profiles_and_respect_expansion() {
        let mut production_a = SessionProfile::new("prod-a", "a.example", "alice");
        production_a.group_name = " Production ".into();
        let mut production_b = SessionProfile::new("prod-b", "b.example", "bob");
        production_b.group_name = "Production".into();
        let ungrouped = SessionProfile::new("local", "local.example", "carol");
        let sessions = SessionStore {
            sessions: vec![production_a, production_b, ungrouped],
            ..SessionStore::default()
        };
        let expanded = BTreeSet::from(["Production".to_owned()]);

        let rows = session_rows(&sessions, &expanded);

        assert_eq!(rows.len(), 4);
        assert!(rows[0].is_group);
        assert_eq!(rows[0].name.as_str(), "Production");
        assert!(rows[0].expanded);
        assert!(!rows[1].is_group);
        assert_eq!(rows[1].name.as_str(), "prod-a");
        assert!(!rows[2].is_group);
        assert_eq!(rows[2].name.as_str(), "prod-b");
        assert!(rows[3].is_group);
        assert_eq!(rows[3].name.as_str(), "Ungrouped");
        assert!(!rows[3].expanded);
    }

    #[test]
    fn maps_slint_navigation_and_text_keys_to_terminal_domain() {
        let up = SharedString::from(Key::UpArrow);
        assert_eq!(terminal_key_from_slint(up.as_str()), TerminalKey::Up);
        assert_eq!(terminal_key_from_slint("x"), TerminalKey::Text("x".into()));
    }
}
