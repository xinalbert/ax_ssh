//! Slint application controller.
//!
//! This module is the only place that knows about generated Slint types. It
//! maps user intent to domain operations and returns owned worker events to the
//! Slint event loop.

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use tokio::runtime::{Handle, Runtime};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Duration;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use ax_ssh::config::{ConfigStore, SessionProfile, SessionStore};
use ax_ssh::ssh::{SshSessionEvent, SshSessionHandle, probe_host_key};

slint::include_modules!();

pub fn run() -> Result<()> {
    let config_path = ConfigStore::default_path()?;
    let config = ConfigStore::new(config_path);
    let sessions = config.load().context("failed to load session profiles")?;
    let runtime = Runtime::new().context("failed to start Tokio runtime")?;
    let state = Arc::new(Mutex::new(AppState::new(config, sessions)));
    let ui = AppWindow::new().context("failed to create Slint window")?;

    let rows = session_rows(
        &state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?
            .sessions,
    );
    ui.set_sessions(ModelRc::new(VecModel::from(rows)));
    ui.set_status("Ready".into());

    wire_callbacks(&ui, state.clone(), runtime.handle().clone());
    info!("AxSSH UI initialized");
    let ui_result = ui.run().context("Slint event loop failed");

    let (active_session, pending_probe) = {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned during shutdown"))?;
        (app.active_session.take(), app.pending_probe.take())
    };
    if let Some(pending_probe) = pending_probe
        && pending_probe.cancel.send(()).is_err()
    {
        debug!(
            session_id = %pending_probe.profile_id,
            "host-key probe already stopped during shutdown"
        );
    }
    if let Some(active_session) = active_session {
        let profile_id = active_session.profile_id;
        if let Err(error) = runtime.block_on(active_session.worker.shutdown()) {
            warn!(
                session_id = %profile_id,
                %error,
                "failed to shut down active SSH worker cleanly"
            );
        }
    }

    drop(ui);
    runtime.shutdown_timeout(Duration::from_secs(3));
    ui_result?;
    info!("AxSSH UI stopped");
    Ok(())
}

struct AppState {
    config: ConfigStore,
    sessions: SessionStore,
    pending_probe: Option<PendingProbe>,
    pending_trust: Option<PendingHostKey>,
    pending_auth: Option<Uuid>,
    active_session: Option<ActiveSession>,
}

impl AppState {
    fn new(config: ConfigStore, sessions: SessionStore) -> Self {
        Self {
            config,
            sessions,
            pending_probe: None,
            pending_trust: None,
            pending_auth: None,
            active_session: None,
        }
    }

    fn discard_finished_worker(&mut self) {
        if self
            .active_session
            .as_ref()
            .is_some_and(|active| active.worker.is_finished())
        {
            self.active_session = None;
        }
    }

    fn connection_flow_busy(&mut self) -> bool {
        self.discard_finished_worker();
        self.active_session.is_some()
            || self.pending_probe.is_some()
            || self.pending_trust.is_some()
            || self.pending_auth.is_some()
    }
}

#[derive(Clone)]
struct PendingHostKey {
    profile_id: Uuid,
    host: String,
    port: u16,
    fingerprint: String,
    changed: bool,
}

struct PendingProbe {
    profile_id: Uuid,
    cancel: oneshot::Sender<()>,
}

struct ActiveSession {
    profile_id: Uuid,
    worker: SshSessionHandle,
}

enum ConnectionStart {
    Password(SessionProfile),
    Probe(SessionProfile, oneshot::Receiver<()>),
}

fn session_rows(sessions: &SessionStore) -> Vec<SessionRow> {
    sessions
        .sessions
        .iter()
        .map(|profile| SessionRow {
            id: profile.id.to_string().into(),
            name: profile.name.clone().into(),
            endpoint: profile_endpoint(profile).into(),
        })
        .collect()
}

fn profile_endpoint(profile: &SessionProfile) -> String {
    format!("{}@{}:{}", profile.username, profile.host, profile.port)
}

fn wire_callbacks(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    wire_session_editor(ui, state.clone());
    wire_connection_request(ui, state.clone(), runtime.clone());
    wire_host_key_confirmation(ui, state.clone());
    wire_authentication(ui, state.clone(), runtime);
}

fn wire_session_editor(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    let state_for_save = state.clone();
    let ui_for_save = ui_weak.clone();
    ui.on_save_session(move |name, host, port, username| {
        let parsed_port = match port.trim().parse::<u16>() {
            Ok(port) if port > 0 => port,
            _ => {
                set_status(&ui_for_save, "Port must be a number between 1 and 65535");
                return;
            }
        };
        let profile = SessionProfile::new(name.as_str(), host.as_str(), username.as_str());
        let profile = SessionProfile {
            port: parsed_port,
            ..profile
        };
        let profile_id = profile.id;
        if let Err(error) = profile.validate().and_then(|_| {
            let mut app = state_for_save
                .lock()
                .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
            let mut candidate = app.sessions.clone();
            candidate.upsert(profile);
            app.config.save(&candidate)?;
            app.sessions = candidate;
            Ok(())
        }) {
            set_status(&ui_for_save, &format!("Cannot save session: {error}"));
            return;
        }
        info!(session_id = %profile_id, "session profile saved");
        refresh_rows(&ui_for_save, &state_for_save);
        set_dialog_open(&ui_for_save, Dialog::NewSession, false);
        set_status(&ui_for_save, "Session saved");
    });

    let ui_for_new = ui_weak.clone();
    let state_for_new = state.clone();
    ui.on_new_session(move || {
        let prompt_open = match state_for_new.lock() {
            Ok(app) => {
                app.pending_probe.is_some()
                    || app.pending_trust.is_some()
                    || app.pending_auth.is_some()
            }
            Err(_) => {
                set_status(&ui_for_new, "Cannot read session state");
                return;
            }
        };
        if prompt_open {
            set_status(&ui_for_new, "Finish or cancel the connection prompt first");
            return;
        }
        set_dialog_open(&ui_for_new, Dialog::NewSession, true);
    });

    let ui_for_cancel = ui_weak;
    ui.on_cancel_session_dialog(move || {
        set_dialog_open(&ui_for_cancel, Dialog::NewSession, false);
    });
}

fn wire_connection_request(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    let ui_for_connect = ui.as_weak();
    ui.on_connect_session(move |id| {
        let id = match id.as_str().parse::<Uuid>() {
            Ok(id) => id,
            Err(error) => {
                set_status(&ui_for_connect, &format!("Invalid session id: {error}"));
                return;
            }
        };
        let start = {
            let mut app = match state.lock() {
                Ok(app) => app,
                Err(_) => {
                    set_status(&ui_for_connect, "Cannot read session state");
                    return;
                }
            };
            if app.connection_flow_busy() {
                set_status(
                    &ui_for_connect,
                    "Finish or cancel the current connection action first",
                );
                return;
            }
            let Some(profile) = app
                .sessions
                .sessions
                .iter()
                .find(|profile| profile.id == id)
                .cloned()
            else {
                set_status(&ui_for_connect, "Session not found");
                return;
            };
            if profile.host_key_fingerprint.is_some() {
                app.pending_auth = Some(profile.id);
                ConnectionStart::Password(profile)
            } else {
                let (cancel, cancelled) = oneshot::channel();
                app.pending_probe = Some(PendingProbe {
                    profile_id: profile.id,
                    cancel,
                });
                ConnectionStart::Probe(profile, cancelled)
            }
        };

        let (profile, cancelled) = match start {
            ConnectionStart::Password(profile) => {
                show_password_prompt(&ui_for_connect, &profile);
                set_status(&ui_for_connect, "Password required");
                return;
            }
            ConnectionStart::Probe(profile, cancelled) => (profile, cancelled),
        };

        let state_for_probe = state.clone();
        let ui_for_probe = ui_for_connect.clone();
        set_probe_state(&ui_for_connect, true, &profile.name);
        set_status(&ui_for_connect, "Checking SSH host key...");
        info!(
            session_id = %profile.id,
            host = %profile.host,
            port = profile.port,
            "probing unknown SSH host key"
        );
        runtime.spawn(async move {
            let result = tokio::select! {
                _ = cancelled => None,
                result = probe_host_key(&profile) => Some(result),
            };
            let prompt = match state_for_probe.lock() {
                Ok(mut app)
                    if app
                        .pending_probe
                        .as_ref()
                        .is_some_and(|probe| probe.profile_id == profile.id) =>
                {
                    app.pending_probe = None;
                    match result {
                        Some(Ok(fingerprint)) => {
                            let prompt = PendingHostKey {
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
            set_probe_state(&ui_for_probe, false, "");
            match prompt {
                Some(Ok(prompt)) => {
                    show_host_key_prompt(&ui_for_probe, &prompt);
                    set_status(&ui_for_probe, "Verify the SSH host key before connecting");
                }
                Some(Err(error)) => {
                    warn!(
                        session_id = %profile.id,
                        %error,
                        "SSH host-key probe failed"
                    );
                    set_status(&ui_for_probe, &format!("Host-key check failed: {error}"));
                }
                None => debug!(session_id = %profile.id, "cancelled or stale host-key probe result ignored"),
            }
        });
    });
}

fn wire_host_key_confirmation(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_for_confirm = ui.as_weak();
    let state_for_confirm = state.clone();
    ui.on_confirm_host_key(move || {
        let profile = {
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
            if profile.host != pending.host || profile.port != pending.port {
                set_status(
                    &ui_for_confirm,
                    "Session endpoint changed; check the host key again",
                );
                app.pending_trust = None;
                set_dialog_open(&ui_for_confirm, Dialog::HostKey, false);
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
            app.pending_auth = Some(profile.id);
            profile
        };

        info!(
            session_id = %profile.id,
            fingerprint = ?profile.host_key_fingerprint,
            "SSH host key trusted by user"
        );
        set_dialog_open(&ui_for_confirm, Dialog::HostKey, false);
        show_password_prompt(&ui_for_confirm, &profile);
        set_status(&ui_for_confirm, "Host key saved; password required");
    });

    let ui_for_reject = ui.as_weak();
    ui.on_reject_host_key(move || {
        match state.lock() {
            Ok(mut app) => app.pending_trust = None,
            Err(_) => {
                set_status(&ui_for_reject, "Cannot update session state");
                return;
            }
        }
        set_dialog_open(&ui_for_reject, Dialog::HostKey, false);
        set_status(
            &ui_for_reject,
            "Connection cancelled; host key was not trusted",
        );
    });
}

fn wire_authentication(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    let ui_for_auth = ui.as_weak();
    let state_for_auth = state.clone();
    let runtime_for_auth = runtime.clone();
    ui.on_authenticate_session(move |password| {
        if password.is_empty() {
            set_status(&ui_for_auth, "Password cannot be empty");
            return;
        }
        let secret = password.as_str().to_owned();
        let (profile, events) = {
            let mut app = match state_for_auth.lock() {
                Ok(app) => app,
                Err(_) => {
                    set_status(&ui_for_auth, "Cannot read session state");
                    return;
                }
            };
            app.discard_finished_worker();
            if app.active_session.is_some() {
                set_status(&ui_for_auth, "Disconnect the active session first");
                return;
            }
            let Some(profile_id) = app.pending_auth else {
                set_status(&ui_for_auth, "No session is awaiting authentication");
                return;
            };
            let Some(profile) = app
                .sessions
                .sessions
                .iter()
                .find(|profile| profile.id == profile_id)
                .cloned()
            else {
                set_status(&ui_for_auth, "Session not found");
                return;
            };
            if profile.host_key_fingerprint.is_none() {
                set_status(&ui_for_auth, "Verify the SSH host key first");
                return;
            }
            let (worker, events) =
                SshSessionHandle::spawn(&runtime_for_auth, profile.clone(), secret);
            app.pending_auth = None;
            app.active_session = Some(ActiveSession {
                profile_id: profile.id,
                worker,
            });
            (profile, events)
        };

        set_dialog_open(&ui_for_auth, Dialog::Password, false);
        set_connection_state(&ui_for_auth, true, false, &profile.name);
        set_status(
            &ui_for_auth,
            &format!("Connecting to {}...", profile_endpoint(&profile)),
        );
        spawn_session_monitor(
            &runtime_for_auth,
            state_for_auth.clone(),
            ui_for_auth.clone(),
            profile,
            events,
        );
    });

    let ui_for_cancel = ui.as_weak();
    let state_for_cancel = state.clone();
    ui.on_cancel_password_dialog(move || {
        match state_for_cancel.lock() {
            Ok(mut app) => app.pending_auth = None,
            Err(_) => {
                set_status(&ui_for_cancel, "Cannot update session state");
                return;
            }
        }
        set_dialog_open(&ui_for_cancel, Dialog::Password, false);
        set_status(&ui_for_cancel, "Authentication cancelled");
    });

    let ui_for_disconnect = ui.as_weak();
    ui.on_disconnect_session(move || {
        let (cancel_probe, result) = match state.lock() {
            Ok(mut app) => {
                app.discard_finished_worker();
                if let Some(probe) = app.pending_probe.take() {
                    (Some(probe), Ok(()))
                } else {
                    (
                        None,
                        app.active_session
                            .as_ref()
                            .context("no active SSH session")
                            .and_then(|active| active.worker.request_disconnect()),
                    )
                }
            }
            Err(_) => (None, Err(anyhow::anyhow!("state lock poisoned"))),
        };
        if let Some(probe) = cancel_probe {
            if probe.cancel.send(()).is_err() {
                debug!(session_id = %probe.profile_id, "host-key probe already stopped");
            }
            set_probe_state(&ui_for_disconnect, false, "");
            set_status(&ui_for_disconnect, "Host-key check cancelled");
            return;
        }
        match result {
            Ok(()) => set_status(&ui_for_disconnect, "Disconnecting..."),
            Err(error) => set_status(
                &ui_for_disconnect,
                &format!("Cannot disconnect session: {error}"),
            ),
        }
    });
}

fn spawn_session_monitor(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    profile: SessionProfile,
    mut events: mpsc::Receiver<SshSessionEvent>,
) {
    runtime.spawn(async move {
        let mut terminal_event = false;
        while let Some(event) = events.recv().await {
            match event {
                SshSessionEvent::Connected => {
                    info!(session_id = %profile.id, "SSH worker reported connected");
                    set_connection_state(&ui, true, true, &profile.name);
                    set_status(&ui, &format!("Connected to {}", profile_endpoint(&profile)));
                }
                SshSessionEvent::Disconnected => {
                    terminal_event = true;
                    set_connection_state(&ui, false, false, "");
                    set_status(&ui, "Disconnected");
                }
                SshSessionEvent::HostKeyRejected { expected, actual } => {
                    terminal_event = true;
                    warn!(
                        session_id = %profile.id,
                        expected = ?expected,
                        fingerprint = %actual,
                        "SSH worker rejected host key"
                    );
                    let prompt = PendingHostKey {
                        profile_id: profile.id,
                        host: profile.host.clone(),
                        port: profile.port,
                        fingerprint: actual,
                        changed: expected.is_some(),
                    };
                    match state.lock() {
                        Ok(mut app) => app.pending_trust = Some(prompt.clone()),
                        Err(_) => {
                            set_status(&ui, "Cannot update host-key confirmation state");
                            continue;
                        }
                    }
                    set_connection_state(&ui, false, false, "");
                    show_host_key_prompt(&ui, &prompt);
                    set_status(&ui, "SSH host key changed; verify it before reconnecting");
                }
                SshSessionEvent::Failed(message) => {
                    terminal_event = true;
                    set_connection_state(&ui, false, false, "");
                    set_status(&ui, &format!("Connection failed: {message}"));
                }
            }
        }

        match state.lock() {
            Ok(mut app)
                if app
                    .active_session
                    .as_ref()
                    .is_some_and(|active| active.profile_id == profile.id) =>
            {
                app.active_session = None;
            }
            Ok(_) => {}
            Err(_) => error!("state lock poisoned while retiring SSH worker"),
        }
        if !terminal_event {
            set_connection_state(&ui, false, false, "");
            set_status(&ui, "SSH worker stopped");
        }
        debug!(session_id = %profile.id, "SSH event monitor stopped");
    });
}

fn refresh_rows(ui: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let rows = match state.lock() {
        Ok(app) => session_rows(&app.sessions),
        Err(_) => {
            set_status(ui, "State lock poisoned");
            return;
        }
    };
    dispatch_ui(ui, move |ui| {
        ui.set_sessions(ModelRc::new(VecModel::from(rows)));
    });
}

#[derive(Clone, Copy)]
enum Dialog {
    NewSession,
    HostKey,
    Password,
}

fn set_dialog_open(ui: &slint::Weak<AppWindow>, dialog: Dialog, open: bool) {
    dispatch_ui(ui, move |ui| match dialog {
        Dialog::NewSession => ui.set_new_session_dialog_open(open),
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

fn show_password_prompt(ui: &slint::Weak<AppWindow>, profile: &SessionProfile) {
    let endpoint = profile_endpoint(profile);
    dispatch_ui(ui, move |ui| {
        ui.set_password_endpoint(endpoint.into());
        ui.set_password_dialog_open(true);
    });
}

fn set_connection_state(
    ui: &slint::Weak<AppWindow>,
    worker_running: bool,
    connected: bool,
    name: &str,
) {
    let name = name.to_owned();
    dispatch_ui(ui, move |ui| {
        ui.set_worker_running(worker_running);
        ui.set_connected(connected);
        ui.set_active_session_name(name.into());
    });
}

fn set_probe_state(ui: &slint::Weak<AppWindow>, running: bool, name: &str) {
    let name = name.to_owned();
    dispatch_ui(ui, move |ui| {
        ui.set_probe_running(running);
        if running {
            ui.set_active_session_name(name.into());
        } else if !ui.get_worker_running() {
            ui.set_active_session_name("".into());
        }
    });
}

fn set_status(ui: &slint::Weak<AppWindow>, message: &str) {
    let message = message.to_owned();
    dispatch_ui(ui, move |ui| {
        ui.set_status(SharedString::from(message));
    });
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
