//! Slint application controller.
//!
//! This module is the only place that knows about generated Slint types. It
//! maps user intent to domain operations and returns owned worker events to the
//! Slint event loop.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use slint::platform::{Clipboard, Key};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};
use tokio::runtime::{Handle, Runtime};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Duration;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use ax_ssh::config::{
    AppSettings, AuthMethod, ConfigStore, SessionProfile, SessionStore, ShortcutSettings,
    TerminalColorScheme, normalize_group_name,
};
use ax_ssh::local_shell::{LocalShellEvent, LocalShellHandle, discover_shells};
use ax_ssh::ssh::{SshSessionEvent, SshSessionHandle, discover_private_keys, probe_host_key};
use ax_ssh::terminal::{
    TerminalKey, TerminalModifiers, TerminalSnapshot, encode_key as encode_terminal_key,
};

use self::credential_tasks::{
    delete_password as delete_stored_password, load_password as load_stored_password,
    save_password as save_stored_password,
};
use self::session_groups::{group_icon, group_options, profile_endpoint, session_groups};
use self::state::{
    ActiveTabSnapshot, AppState, ConnectionStart, PendingAuth, PendingHostKey, PendingProbe,
    TerminalTabState, TerminalWorker, WorkspaceTabSummary, prepare_authentication_retry,
    prepare_host_key_retry, retire_session_attempt, session_attempt_is_active,
    set_credential_marker,
};
use self::terminal_render::{
    RenderedTerminalLine, RenderedTerminalRun, RgbColor, TerminalRenderSettings, render_terminal,
};

mod credential_tasks;
#[cfg(target_os = "macos")]
mod macos_window;
mod session_groups;
mod state;
mod terminal_render;

slint::include_modules!();

pub fn run() -> Result<()> {
    let config_path = ConfigStore::default_path()?;
    let config = ConfigStore::new(config_path);
    let mut sessions = config.load().context("failed to load session profiles")?;
    if sessions
        .settings
        .terminal
        .merge_known_shells(discover_shells())
        && let Err(error) = config.save(&sessions)
    {
        warn!(%error, "failed to persist newly discovered local shells");
    }
    let runtime = Runtime::new().context("failed to start Tokio runtime")?;
    let state = Arc::new(Mutex::new(AppState::new(config, sessions)));
    let ui = AppWindow::new().context("failed to create Slint window")?;

    let (rows, group_icons, groups, settings) = {
        let app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        (
            session_rows(&app.sessions, &app.expanded_groups),
            group_icon_rows(&app.sessions),
            group_option_rows(&app.sessions),
            app.sessions.settings.clone(),
        )
    };
    ui.set_sessions(ModelRc::new(VecModel::from(rows)));
    ui.set_group_icons(ModelRc::new(VecModel::from(group_icons)));
    ui.set_group_options(ModelRc::new(VecModel::from(groups)));
    ui.set_private_key_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_terminal_render_lines(ModelRc::new(VecModel::from(
        Vec::<TerminalRenderLine>::new(),
    )));
    ui.set_local_shell_options(ModelRc::new(VecModel::from(shell_option_rows(&settings))));
    let default_shortcuts = ShortcutSettings::default();
    ui.set_default_open_settings_shortcut(default_shortcuts.open_settings.into());
    ui.set_default_toggle_sidebar_shortcut(default_shortcuts.toggle_sidebar.into());
    ui.set_default_copy_selection_shortcut(default_shortcuts.copy_selection.into());
    ui.set_default_paste_shortcut(default_shortcuts.paste.into());
    ui.set_apple_platform(cfg!(target_os = "macos"));
    ui.set_app_version(env!("CARGO_PKG_VERSION").into());
    apply_settings_to_component(&ui, &settings);
    apply_active_snapshot(&ui, ActiveTabSnapshot::default());
    ui.set_workspace_tabs(ModelRc::new(VecModel::from(Vec::<WorkspaceTabRow>::new())));
    ui.set_status("Ready".into());
    ui.set_unified_titlebar(false);

    wire_callbacks(&ui, state.clone(), runtime.handle().clone());
    load_private_key_options(runtime.handle(), ui.as_weak());
    #[cfg(target_os = "macos")]
    {
        ui.show().context("failed to create macOS window")?;
        let ui_for_window = ui.as_weak();
        slint::Timer::single_shot(Duration::from_millis(100), move || {
            let Some(ui) = ui_for_window.upgrade() else {
                return;
            };
            match macos_window::configure(ui.window()) {
                Ok(()) => ui.set_unified_titlebar(true),
                Err(error) => warn!(%error, "falling back to the standard macOS title bar"),
            }
        });
    }
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
            warn!(%error, "failed to shut down terminal worker cleanly");
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
        if group_name.is_empty() {
            rows.extend(profiles.into_iter().map(|profile| SessionRow {
                id: profile.id.to_string().into(),
                group_name: "".into(),
                name: profile.name.clone().into(),
                endpoint: profile_endpoint(profile).into(),
                is_group: false,
                expanded: false,
            }));
            continue;
        }
        let expanded = expanded_groups.contains(&group_name);
        rows.push(SessionRow {
            id: "".into(),
            group_name: group_name.clone().into(),
            name: group_name.clone().into(),
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

fn group_icon_rows(sessions: &SessionStore) -> Vec<SessionGroupIconRow> {
    session_groups(sessions)
        .into_iter()
        .map(|group| {
            let display_name = if group.name.is_empty() {
                "Ungrouped".to_owned()
            } else {
                group.name.clone()
            };
            SessionGroupIconRow {
                group_name: group.name.clone().into(),
                icon: group_icon(&group.name).into(),
                accessible_name: format!("Open {display_name} group").into(),
            }
        })
        .collect()
}

fn shell_option_rows(settings: &AppSettings) -> Vec<SharedString> {
    settings
        .terminal
        .known_shells
        .iter()
        .cloned()
        .map(SharedString::from)
        .collect()
}

fn wire_callbacks(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    ui.on_format_shortcut(move |text, alt, control, meta, shift| {
        format_shortcut_event(text.as_str(), alt, control, meta, shift).into()
    });
    let ui_for_clipboard_write = ui.as_weak();
    ui.on_write_clipboard(move |text| {
        if let Some(ui) = ui_for_clipboard_write.upgrade() {
            set_platform_clipboard_text(&ui, text.as_str());
        }
    });
    let ui_for_clipboard_read = ui.as_weak();
    ui.on_read_clipboard(move || {
        ui_for_clipboard_read
            .upgrade()
            .map(|ui| platform_clipboard_text(&ui))
            .unwrap_or_default()
            .into()
    });
    #[cfg(target_os = "macos")]
    {
        let ui_for_drag = ui.as_weak();
        ui.on_drag_window(move || {
            let Some(ui) = ui_for_drag.upgrade() else {
                return;
            };
            if let Err(error) = macos_window::start_drag(ui.window()) {
                warn!(%error, "failed to start native macOS window drag");
            }
        });
    }
    wire_workspace_tabs(ui, state.clone(), runtime.clone());
    wire_session_editor(ui, state.clone(), runtime.clone());
    wire_connection_request(ui, state.clone(), runtime.clone());
    wire_host_key_confirmation(ui, state.clone(), runtime.clone());
    wire_authentication(ui, state.clone(), runtime.clone());
    wire_settings(ui, state.clone(), runtime);
    wire_terminal(ui, state);
}

fn set_platform_clipboard_text(ui: &AppWindow, text: &str) {
    // Slint 1.17 exposes clipboard access through its window context.
    slint::private_unstable_api::re_exports::WindowInner::from_pub(ui.window())
        .context()
        .platform()
        .set_clipboard_text(text, Clipboard::DefaultClipboard);
}

fn platform_clipboard_text(ui: &AppWindow) -> String {
    slint::private_unstable_api::re_exports::WindowInner::from_pub(ui.window())
        .context()
        .platform()
        .clipboard_text(Clipboard::DefaultClipboard)
        .unwrap_or_default()
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

    let ui_for_local = ui.as_weak();
    let state_for_local = state.clone();
    let runtime_for_local = runtime.clone();
    ui.on_open_local_shell(move || {
        if let Err(error) = start_local_shell(
            &runtime_for_local,
            state_for_local.clone(),
            ui_for_local.clone(),
        ) {
            set_status(&ui_for_local, &format!("Cannot open local shell: {error}"));
        }
    });

    let ui_for_group = ui.as_weak();
    let state_for_group = state.clone();
    ui.on_activate_group(move |group_name| {
        let group_name = normalize_group_name(group_name.as_str());
        match state_for_group.lock() {
            Ok(mut app) => {
                app.expanded_groups.insert(group_name);
            }
            Err(_) => {
                set_status(&ui_for_group, "Cannot update group state");
                return;
            }
        }
        refresh_session_models(&ui_for_group, &state_for_group);
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
                set_status(
                    &ui,
                    &format!("Cannot close terminal worker cleanly: {error}"),
                );
            }
        });
    }
    refresh_workspace(ui, state);
}

fn start_local_shell(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
) -> Result<()> {
    let (tab_id, events) = {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        let shell = app.sessions.settings.terminal.local_shell.clone();
        let columns = u32::from(app.sessions.settings.terminal.default_columns);
        let rows = u32::from(app.sessions.settings.terminal.default_rows);
        let tab_id = app.open_local_shell_tab();
        let (worker, events) = LocalShellHandle::spawn(shell, columns, rows);
        let terminal = app
            .terminal_mut(tab_id)
            .context("local terminal tab disappeared while starting worker")?;
        terminal.worker = Some(TerminalWorker::Local(worker));
        (tab_id, events)
    };
    refresh_workspace(&ui, &state);
    spawn_local_shell_monitor(runtime, state, ui, tab_id, events);
    Ok(())
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

fn wire_terminal(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_for_key = ui.as_weak();
    let state_for_key = state.clone();
    ui.on_terminal_key(move |text, alt, control, meta, shift| {
        let modifiers = normalize_slint_modifiers(alt, control, meta, shift);
        let key = terminal_key_from_slint(text.as_str(), modifiers);
        let result = state_for_key
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))
            .and_then(|mut app| {
                let application_cursor = app
                    .active_terminal()
                    .context("no active terminal")?
                    .terminal
                    .application_cursor();
                let Some(data) = encode_terminal_key(&key, modifiers, application_cursor) else {
                    return Ok((false, None));
                };
                let viewport_changed = {
                    let terminal = app.active_terminal_mut().context("no active terminal")?;
                    let viewport_changed = terminal.terminal.scroll_to_bottom();
                    terminal
                        .worker
                        .as_ref()
                        .context("active terminal has no worker")?
                        .request_send(data)?;
                    viewport_changed
                };
                Ok((true, viewport_changed.then(|| app.active_snapshot())))
            });
        match result {
            Ok((handled, Some(snapshot))) => {
                dispatch_active_snapshot(&ui_for_key, snapshot);
                handled
            }
            Ok((handled, None)) => handled,
            Err(error) => {
                set_status(&ui_for_key, &format!("Cannot send terminal input: {error}"));
                true
            }
        }
    });

    let ui_for_resize = ui.as_weak();
    let state_for_resize = state.clone();
    ui.on_resize_terminal(move |columns, rows| {
        let result = state_for_resize
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))
            .and_then(|app| {
                app.active_terminal()
                    .context("no active terminal")?
                    .worker
                    .as_ref()
                    .context("active terminal has no worker")?
                    .request_resize(columns.max(1) as u32, rows.max(1) as u32)
            });
        if let Err(error) = result {
            debug!(%error, "terminal resize ignored");
            set_status(&ui_for_resize, &format!("Cannot resize terminal: {error}"));
        }
    });

    let ui_for_scroll = ui.as_weak();
    let state_for_scroll = state.clone();
    ui.on_scroll_terminal(move |lines| {
        let snapshot = state_for_scroll.lock().ok().and_then(|mut app| {
            let changed = app.active_terminal_mut()?.terminal.scroll(lines);
            changed.then(|| app.active_snapshot())
        });
        if let Some(snapshot) = snapshot {
            dispatch_active_snapshot(&ui_for_scroll, snapshot);
        }
    });

    ui.on_terminal_selection_text(move |anchor_row, anchor_column, focus_row, focus_column| {
        state
            .lock()
            .ok()
            .and_then(|app| {
                app.active_terminal().map(|terminal| {
                    terminal.terminal.selection_text(
                        anchor_row.max(0) as usize,
                        anchor_column.max(0) as usize,
                        focus_row.max(0) as usize,
                        focus_column.max(0) as usize,
                    )
                })
            })
            .unwrap_or_default()
            .into()
    });
}

fn terminal_key_from_slint(text: &str, modifiers: TerminalModifiers) -> TerminalKey {
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
        .unwrap_or_else(|| {
            let text = if text == "-"
                && modifiers.shift
                && !modifiers.alt
                && !modifiers.control
                && !modifiers.meta
            {
                "_"
            } else {
                text
            };
            TerminalKey::Text(text.to_owned())
        })
}

fn matches_slint_key(text: &str, key: Key) -> bool {
    let mut characters = text.chars();
    characters.next() == Some(char::from(key)) && characters.next().is_none()
}

fn format_shortcut_event(text: &str, alt: bool, control: bool, meta: bool, shift: bool) -> String {
    let modifiers = normalize_slint_modifiers(alt, control, meta, shift);
    if !modifiers.alt && !modifiers.control && !modifiers.meta && !modifiers.shift {
        return String::new();
    }
    let Some(key) = shortcut_key_name(text, modifiers.control) else {
        return String::new();
    };
    let mut parts = Vec::with_capacity(5);
    if modifiers.meta {
        parts.push(if cfg!(target_os = "macos") {
            "Cmd".to_owned()
        } else {
            "Meta".to_owned()
        });
    }
    if modifiers.control {
        parts.push("Ctrl".to_owned());
    }
    if modifiers.alt {
        parts.push("Alt".to_owned());
    }
    if modifiers.shift {
        parts.push("Shift".to_owned());
    }
    parts.push(key);
    parts.join("+")
}

fn normalize_slint_modifiers(
    alt: bool,
    control: bool,
    meta: bool,
    shift: bool,
) -> TerminalModifiers {
    normalize_slint_modifiers_for_platform(alt, control, meta, shift, cfg!(target_os = "macos"))
}

fn normalize_slint_modifiers_for_platform(
    alt: bool,
    control: bool,
    meta: bool,
    shift: bool,
    apple_platform: bool,
) -> TerminalModifiers {
    TerminalModifiers {
        alt,
        control: if apple_platform { meta } else { control },
        meta: if apple_platform { control } else { meta },
        shift,
    }
}

fn shortcut_key_name(text: &str, control: bool) -> Option<String> {
    let modifier_keys = [
        Key::Alt,
        Key::AltGr,
        Key::Control,
        Key::ControlR,
        Key::Meta,
        Key::MetaR,
        Key::Shift,
        Key::ShiftR,
    ];
    if modifier_keys
        .into_iter()
        .any(|key| matches_slint_key(text, key))
    {
        return None;
    }
    let special_keys = [
        (Key::Backspace, "Backspace"),
        (Key::Tab, "Tab"),
        (Key::Backtab, "Backtab"),
        (Key::Return, "Enter"),
        (Key::Escape, "Escape"),
        (Key::Delete, "Delete"),
        (Key::Space, "Space"),
        (Key::UpArrow, "ArrowUp"),
        (Key::DownArrow, "ArrowDown"),
        (Key::LeftArrow, "ArrowLeft"),
        (Key::RightArrow, "ArrowRight"),
        (Key::Insert, "Insert"),
        (Key::Home, "Home"),
        (Key::End, "End"),
        (Key::PageUp, "PageUp"),
        (Key::PageDown, "PageDown"),
        (Key::F1, "F1"),
        (Key::F2, "F2"),
        (Key::F3, "F3"),
        (Key::F4, "F4"),
        (Key::F5, "F5"),
        (Key::F6, "F6"),
        (Key::F7, "F7"),
        (Key::F8, "F8"),
        (Key::F9, "F9"),
        (Key::F10, "F10"),
        (Key::F11, "F11"),
        (Key::F12, "F12"),
    ];
    if let Some((_, label)) = special_keys
        .into_iter()
        .find(|(key, _)| matches_slint_key(text, *key))
    {
        return Some(label.to_owned());
    }

    let mut characters = text.chars();
    let character = characters.next()?;
    if characters.next().is_some() {
        return None;
    }
    if control && ('\u{0001}'..='\u{000f}').contains(&character) {
        return Some(((character as u8 + b'A' - 1) as char).to_string());
    }
    if character.is_control() {
        return None;
    }
    Some(match character {
        '+' => "Plus".to_owned(),
        character if character.is_ascii_alphabetic() => character.to_ascii_uppercase().to_string(),
        character => character.to_string(),
    })
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
              line_height_percent,
              color_scheme,
              brightness_percent,
              bright_bold_text,
              right_click_copy_or_paste,
              local_shell,
              scrollback_lines,
              default_columns,
              default_rows,
              sidebar_width,
              tab_width,
              open_settings_shortcut,
              toggle_sidebar_shortcut,
              copy_selection_shortcut,
              paste_shortcut| {
            let shortcuts = ShortcutSettings {
                open_settings: open_settings_shortcut.as_str().to_owned(),
                toggle_sidebar: toggle_sidebar_shortcut.as_str().to_owned(),
                copy_selection: copy_selection_shortcut.as_str().to_owned(),
                paste: paste_shortcut.as_str().to_owned(),
            };
            if let Err(error) = shortcuts.validate() {
                set_status(&ui_for_save, &format!("Cannot save shortcuts: {error}"));
                return;
            }
            let known_shells = match state.lock() {
                Ok(app) => app.sessions.settings.terminal.known_shells.clone(),
                Err(_) => {
                    set_status(&ui_for_save, "Cannot read local shell settings");
                    return;
                }
            };
            let settings = AppSettings::normalized(
                font_family.as_str(),
                font_size,
                line_height_percent,
                color_scheme.as_str(),
                brightness_percent,
                bright_bold_text,
                right_click_copy_or_paste,
                scrollback_lines,
                default_columns,
                default_rows,
                local_shell.as_str(),
                &known_shells,
                sidebar_width,
                tab_width,
                &shortcuts.open_settings,
                &shortcuts.toggle_sidebar,
                &shortcuts.copy_selection,
                &shortcuts.paste,
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

fn spawn_local_shell_monitor(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    mut events: mpsc::Receiver<LocalShellEvent>,
) {
    runtime.spawn(async move {
        let mut terminal_event = false;
        while let Some(event) = events.recv().await {
            match event {
                LocalShellEvent::Started { shell } => {
                    let Some(snapshot) = mutate_local_terminal(&state, tab_id, |terminal| {
                        terminal.connected = true;
                        terminal.worker_running = true;
                        terminal.status = format!("Local shell: {shell}");
                    }) else {
                        continue;
                    };
                    info!(tab_id = %tab_id, shell = %shell, "local shell started");
                    if let Some(snapshot) = snapshot {
                        dispatch_active_snapshot(&ui, snapshot);
                    }
                    refresh_workspace(&ui, &state);
                }
                LocalShellEvent::Output(data) => {
                    if let Some(Some(snapshot)) =
                        mutate_local_terminal(&state, tab_id, |terminal| {
                            terminal.terminal.process(&data);
                        })
                    {
                        dispatch_active_snapshot(&ui, snapshot);
                    }
                }
                LocalShellEvent::Resized { columns, rows } => {
                    if let Some(Some(snapshot)) =
                        mutate_local_terminal(&state, tab_id, |terminal| {
                            terminal.terminal.resize(columns as usize, rows as usize);
                        })
                    {
                        dispatch_active_snapshot(&ui, snapshot);
                    }
                }
                LocalShellEvent::Exited { status } => {
                    terminal_event = true;
                    if finish_local_terminal(
                        &state,
                        tab_id,
                        &format!("Local shell exited: {status}"),
                    ) {
                        refresh_workspace(&ui, &state);
                    }
                }
                LocalShellEvent::Failed(message) => {
                    terminal_event = true;
                    if finish_local_terminal(
                        &state,
                        tab_id,
                        &format!("Local shell failed: {message}"),
                    ) {
                        refresh_workspace(&ui, &state);
                    }
                }
            }
        }
        if !terminal_event && finish_local_terminal(&state, tab_id, "Local shell worker stopped") {
            refresh_workspace(&ui, &state);
        }
        debug!(tab_id = %tab_id, "local shell event monitor stopped");
    });
}

fn mutate_local_terminal(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    action: impl FnOnce(&mut TerminalTabState),
) -> Option<Option<ActiveTabSnapshot>> {
    let mut app = state.lock().ok()?;
    if !app.terminal(tab_id).is_some_and(TerminalTabState::is_local) {
        return None;
    }
    action(app.terminal_mut(tab_id)?);
    Some((app.active_tab_id() == Some(tab_id)).then(|| app.active_snapshot()))
}

fn finish_local_terminal(state: &Arc<Mutex<AppState>>, tab_id: Uuid, status: &str) -> bool {
    match state.lock() {
        Ok(mut app) if app.terminal(tab_id).is_some_and(TerminalTabState::is_local) => {
            if let Some(terminal) = app.terminal_mut(tab_id) {
                terminal.worker = None;
                terminal.connected = false;
                terminal.worker_running = false;
                terminal.status = status.to_owned();
            }
            true
        }
        Ok(_) | Err(_) => false,
    }
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
    action: impl FnOnce(&mut TerminalTabState),
) -> Option<Option<ActiveTabSnapshot>> {
    let mut app = state.lock().ok()?;
    let current = app
        .terminal(tab_id)
        .and_then(TerminalTabState::ssh_route)
        .is_some_and(|route| route == (profile_id, Some(attempt_id)));
    if !current {
        return None;
    }
    action(app.terminal_mut(tab_id)?);
    Some((app.active_tab_id() == Some(tab_id)).then(|| app.active_snapshot()))
}

fn refresh_session_models(ui: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let (rows, group_icons, groups) = match state.lock() {
        Ok(app) => (
            session_rows(&app.sessions, &app.expanded_groups),
            group_icon_rows(&app.sessions),
            group_option_rows(&app.sessions),
        ),
        Err(_) => {
            set_status(ui, "State lock poisoned");
            return;
        }
    };
    dispatch_ui(ui, move |ui| {
        ui.set_sessions(ModelRc::new(VecModel::from(rows)));
        ui.set_group_icons(ModelRc::new(VecModel::from(group_icons)));
        ui.set_group_options(ModelRc::new(VecModel::from(groups)));
    });
}

fn refresh_workspace(ui: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let (tabs, snapshot) = match state.lock() {
        Ok(app) => (
            visible_workspace_tab_rows(app.tab_summaries()),
            app.active_snapshot(),
        ),
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

fn visible_workspace_tab_rows(tabs: Vec<WorkspaceTabSummary>) -> Vec<WorkspaceTabRow> {
    tabs.into_iter()
        .filter(|tab| tab.kind != "settings")
        .map(|tab| WorkspaceTabRow {
            id: tab.id.to_string().into(),
            title: tab.title.into(),
            kind: tab.kind.into(),
            connected: tab.connected,
        })
        .collect()
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
    let terminal = snapshot.terminal.unwrap_or_else(empty_terminal_snapshot);
    let rendered = render_terminal(
        terminal,
        TerminalRenderSettings {
            color_scheme: TerminalColorScheme::from_setting(
                ui.get_terminal_color_scheme().as_str(),
            ),
            brightness_percent: ui.get_terminal_brightness_percent().clamp(60, 140) as u16,
            bright_bold_text: ui.get_bright_bold_text(),
        },
    );
    apply_rendered_terminal(ui, rendered);
    ui.set_connected(snapshot.connected);
    ui.set_worker_running(snapshot.worker_running);
}

fn apply_settings(ui: &slint::Weak<AppWindow>, settings: AppSettings) {
    dispatch_ui(ui, move |ui| apply_settings_to_component(ui, &settings));
}

fn apply_settings_to_component(ui: &AppWindow, settings: &AppSettings) {
    ui.set_terminal_font_family(settings.appearance.terminal_font_family.clone().into());
    ui.set_terminal_font_size(i32::from(settings.appearance.terminal_font_size));
    ui.set_terminal_line_height_percent(i32::from(
        settings.appearance.terminal_line_height_percent,
    ));
    ui.set_terminal_color_scheme(
        settings
            .appearance
            .terminal_color_scheme
            .as_setting()
            .into(),
    );
    ui.set_terminal_brightness_percent(i32::from(settings.appearance.terminal_brightness_percent));
    ui.set_bright_bold_text(settings.appearance.bright_bold_text);
    ui.set_right_click_copy_or_paste(settings.appearance.right_click_copy_or_paste);
    ui.set_scrollback_lines(settings.terminal.scrollback_lines as i32);
    ui.set_default_terminal_columns(i32::from(settings.terminal.default_columns));
    ui.set_default_terminal_rows(i32::from(settings.terminal.default_rows));
    ui.set_local_shell(settings.terminal.local_shell.clone().into());
    let local_shell_index = settings
        .terminal
        .known_shells
        .iter()
        .position(|shell| shell.eq_ignore_ascii_case(&settings.terminal.local_shell))
        .unwrap_or(0);
    ui.set_local_shell_index(local_shell_index.min(i32::MAX as usize) as i32);
    ui.set_sidebar_width(i32::from(settings.workspace.sidebar_width));
    ui.set_tab_width(i32::from(settings.workspace.tab_width));
    ui.set_open_settings_shortcut(settings.shortcuts.open_settings.clone().into());
    ui.set_toggle_sidebar_shortcut(settings.shortcuts.toggle_sidebar.clone().into());
    ui.set_copy_selection_shortcut(settings.shortcuts.copy_selection.clone().into());
    ui.set_paste_shortcut(settings.shortcuts.paste.clone().into());
}

fn empty_terminal_snapshot() -> TerminalSnapshot {
    TerminalSnapshot {
        text: String::new(),
        lines: vec![Default::default()],
        max_columns: 0,
        cursor_row: 0,
        cursor_column: 0,
        cursor_visible: false,
        cursor_text: " ".to_owned(),
    }
}

fn apply_rendered_terminal(ui: &AppWindow, rendered: terminal_render::RenderedTerminal) {
    ui.set_terminal_content_columns(rendered.max_columns.min(i32::MAX as usize) as i32);
    ui.set_terminal_cursor_row(rendered.cursor_row.min(i32::MAX as usize) as i32);
    ui.set_terminal_cursor_column(rendered.cursor_column.min(i32::MAX as usize) as i32);
    ui.set_terminal_cursor_visible(rendered.cursor_visible);
    ui.set_terminal_cursor_text(rendered.cursor_text.into());
    ui.set_terminal_foreground(to_slint_color(rendered.foreground));
    ui.set_terminal_background(to_slint_color(rendered.background));
    ui.set_terminal_selection_background(to_slint_color(rendered.selection_background));
    let lines = rendered
        .lines
        .into_iter()
        .map(terminal_render_line)
        .collect::<Vec<_>>();
    ui.set_terminal_render_lines(ModelRc::new(VecModel::from(lines)));
}

fn terminal_render_line(line: RenderedTerminalLine) -> TerminalRenderLine {
    let runs = line
        .runs
        .into_iter()
        .map(terminal_render_run)
        .collect::<Vec<_>>();
    TerminalRenderLine {
        runs: ModelRc::new(VecModel::from(runs)),
    }
}

fn terminal_render_run(run: RenderedTerminalRun) -> TerminalRenderRun {
    TerminalRenderRun {
        text: run.text.into(),
        column: run.column.min(i32::MAX as usize) as i32,
        cells: run.cells.min(i32::MAX as usize) as i32,
        foreground: to_slint_color(run.foreground),
        background: to_slint_color(run.background),
        bold: run.bold,
        italic: run.italic,
        underline: run.underline,
        strikethrough: run.strikethrough,
    }
}

fn to_slint_color(color: RgbColor) -> Color {
    Color::from_rgb_u8(color.red, color.green, color.blue)
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
    fn settings_workbench_is_not_exposed_as_a_workspace_tab() {
        let rows = visible_workspace_tab_rows(vec![
            WorkspaceTabSummary {
                id: Uuid::new_v4(),
                title: "Settings".to_owned(),
                kind: "settings",
                connected: false,
            },
            WorkspaceTabSummary {
                id: Uuid::new_v4(),
                title: "New session".to_owned(),
                kind: "session-editor",
                connected: false,
            },
        ]);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind.as_str(), "session-editor");
    }

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
        assert!(!rows[3].is_group);
        assert_eq!(rows[3].name.as_str(), "local");
        assert!(!rows[3].expanded);
    }

    #[test]
    fn maps_slint_navigation_and_text_keys_to_terminal_domain() {
        let up = SharedString::from(Key::UpArrow);
        assert_eq!(
            terminal_key_from_slint(up.as_str(), TerminalModifiers::default()),
            TerminalKey::Up
        );
        assert_eq!(
            terminal_key_from_slint("x", TerminalModifiers::default()),
            TerminalKey::Text("x".into())
        );
    }

    #[test]
    fn normalizes_unshifted_slint_hyphen_text_when_shift_is_pressed() {
        let shift = TerminalModifiers {
            shift: true,
            ..TerminalModifiers::default()
        };
        assert_eq!(
            terminal_key_from_slint("-", shift),
            TerminalKey::Text("_".into())
        );
        assert_eq!(
            terminal_key_from_slint("_", shift),
            TerminalKey::Text("_".into())
        );
    }

    #[test]
    fn formats_modified_shortcuts_and_ignores_plain_or_modifier_keys() {
        let (slint_control, slint_meta) = if cfg!(target_os = "macos") {
            (false, true)
        } else {
            (true, false)
        };
        assert_eq!(
            format_shortcut_event("b", false, slint_control, slint_meta, true),
            "Ctrl+Shift+B"
        );
        assert_eq!(
            format_shortcut_event("\u{0003}", false, slint_control, slint_meta, true),
            "Ctrl+Shift+C"
        );
        assert_eq!(format_shortcut_event("b", false, false, false, false), "");
        let control = SharedString::from(Key::Control);
        assert_eq!(
            format_shortcut_event(control.as_str(), false, slint_control, slint_meta, false,),
            ""
        );

        let (slint_command, slint_command_meta, expected) = if cfg!(target_os = "macos") {
            (true, false, "Cmd+,")
        } else {
            (false, true, "Meta+,")
        };
        assert_eq!(
            format_shortcut_event(",", false, slint_command, slint_command_meta, false,),
            expected
        );
    }

    #[test]
    fn restores_physical_control_and_command_from_slint_apple_modifiers() {
        assert_eq!(
            normalize_slint_modifiers_for_platform(false, false, true, false, true),
            TerminalModifiers {
                control: true,
                ..TerminalModifiers::default()
            }
        );
        assert_eq!(
            normalize_slint_modifiers_for_platform(false, true, false, false, true),
            TerminalModifiers {
                meta: true,
                ..TerminalModifiers::default()
            }
        );
        assert_eq!(
            normalize_slint_modifiers_for_platform(false, true, false, false, false),
            TerminalModifiers {
                control: true,
                ..TerminalModifiers::default()
            }
        );
    }
}
