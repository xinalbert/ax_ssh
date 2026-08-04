use super::*;
use crate::app::credential_tasks::CredentialRollback;
use serde::{Deserialize, Serialize};

const SESSION_TRANSFER_FORMAT: &str = "axssh-session-export";
const SESSION_TRANSFER_VERSION: u32 = 1;
const MAX_SESSION_TRANSFER_BYTES: usize = 256 * 1024;
const MAX_SESSION_TRANSFER_PROFILES: usize = 128;
const MAX_TRANSFER_SESSION_NAME_CHARS: usize = 128;
const MAX_TRANSFER_HOST_CHARS: usize = 512;
const MAX_TRANSFER_USERNAME_CHARS: usize = 256;
const MAX_TRANSFER_KEY_PATH_CHARS: usize = 4096;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum SessionTransferKind {
    Server,
    Group,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SessionTransferEnvelope {
    format: String,
    version: u32,
    kind: SessionTransferKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group_name: Option<String>,
    profiles: Vec<SessionProfile>,
}

#[derive(Clone, Copy)]
enum SessionImportMode {
    SingleServer,
    IntoGroup,
    Automatic,
}

pub(super) fn wire_workspace_tabs(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    let ui_for_settings = ui.as_weak();
    let state_for_settings = state.clone();
    ui.on_open_settings(move || {
        log_ui_action("workspace.open-settings");
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
        log_ui_action("workspace.new-session");
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

    let ui_for_new_in_group = ui.as_weak();
    let state_for_new_in_group = state.clone();
    ui.on_new_session_in_group(move |group_name| {
        log_ui_action("workspace.new-session-in-group");
        match state_for_new_in_group.lock() {
            Ok(mut app) => {
                app.open_session_editor_for_group(group_name.as_str());
            }
            Err(_) => {
                set_status(&ui_for_new_in_group, "Cannot update workspace tabs");
                return;
            }
        }
        refresh_workspace(&ui_for_new_in_group, &state_for_new_in_group);
    });

    let ui_for_edit = ui.as_weak();
    let state_for_edit = state.clone();
    ui.on_edit_session(move |id| {
        log_ui_action("workspace.edit-session");
        let id = match parse_uuid(id.as_str(), "session", &ui_for_edit) {
            Some(id) => id,
            None => return,
        };
        let opened = state_for_edit
            .lock()
            .is_ok_and(|mut app| app.open_session_editor_for_profile(id));
        if !opened {
            set_status(&ui_for_edit, "Session not found");
            return;
        }
        refresh_workspace(&ui_for_edit, &state_for_edit);
    });

    let ui_for_local = ui.as_weak();
    let state_for_local = state.clone();
    let runtime_for_local = runtime.clone();
    ui.on_open_local_shell(move || {
        log_ui_action("workspace.open-local-shell");
        if let Err(error) = start_local_shell(
            &runtime_for_local,
            state_for_local.clone(),
            ui_for_local.clone(),
        ) {
            set_status(&ui_for_local, &format!("Cannot open local shell: {error}"));
        }
    });

    let ui_for_activate = ui.as_weak();
    let state_for_activate = state.clone();
    ui.on_activate_tab(move |id| {
        log_ui_action("workspace.activate-tab");
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

    let ui_for_move = ui.as_weak();
    let state_for_move = state.clone();
    ui.on_move_tab(move |id, target_index| {
        log_ui_action("workspace.move-tab");
        let id = match parse_uuid(id.as_str(), "tab", &ui_for_move) {
            Some(id) => id,
            None => return,
        };
        let moved = state_for_move
            .lock()
            .is_ok_and(|mut app| app.move_tab(id, target_index.max(0) as usize));
        if !moved {
            set_status(&ui_for_move, "Tab not found");
            return;
        }
        refresh_workspace(&ui_for_move, &state_for_move);
    });

    let ui_for_close = ui.as_weak();
    let state_for_close = state.clone();
    let runtime_for_close = runtime.clone();
    ui.on_close_tab(move |id| {
        log_ui_action("workspace.close-tab");
        let id = match parse_uuid(id.as_str(), "tab", &ui_for_close) {
            Some(id) => id,
            None => return,
        };
        close_workspace_tab(id, &state_for_close, &ui_for_close, &runtime_for_close);
    });

    let ui_for_cancel_editor = ui.as_weak();
    let state_for_cancel_editor = state;
    ui.on_cancel_session_dialog(move || {
        log_ui_action("session-editor.cancel");
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

pub(super) fn close_workspace_tab(
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

pub(super) fn wire_session_editor(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    let ui_for_save = ui.as_weak();
    let state_for_save = state.clone();
    ui.on_save_session(
        move |name,
              group_name,
              protocol,
              host,
              port,
              username,
              auth_method,
              private_key_path,
              password,
              remember_password,
              credential_storage,
              vault_password,
              x11_forwarding,
              serial_port,
              serial_baud_rate,
              serial_data_bits,
              serial_stop_bits,
              serial_parity,
              serial_flow_control,
              connect_after_save| {
            log_ui_action("session-editor.save");
            let (editor_tab_id, existing_profile, serial_descriptor) = match state_for_save.lock() {
                Ok(app) => {
                    let Some(profile_id) = app.active_editor_profile_id() else {
                        set_status(&ui_for_save, "Session editor is not active");
                        return;
                    };
                    let existing_profile = profile_id.and_then(|profile_id| {
                        app.sessions
                            .sessions
                            .iter()
                            .find(|profile| profile.id == profile_id)
                            .cloned()
                    });
                    if profile_id.is_some() && existing_profile.is_none() {
                        set_status(&ui_for_save, "Session not found");
                        return;
                    }
                    let serial_descriptor = app
                        .serial_ports()
                        .iter()
                        .find(|port| port.port_name == serial_port.as_str().trim())
                        .cloned();
                    (
                        app.active_tab_id(),
                        existing_profile,
                        serial_descriptor,
                    )
                }
                Err(_) => {
                    set_status(&ui_for_save, "Cannot read session state");
                    return;
                }
                };
            let (profile, credential_change, connection_password) =
                match profile_from_editor_with_password(
                existing_profile.as_ref(),
                name.as_str(),
                group_name.as_str(),
                protocol.as_str(),
                host.as_str(),
                port.as_str(),
                username.as_str(),
                auth_method.as_str(),
                private_key_path.as_str(),
                password.as_str(),
                remember_password,
                credential_storage.as_str(),
                vault_password.as_str(),
                x11_forwarding,
                serial_port.as_str(),
                serial_baud_rate.as_str(),
                serial_data_bits.as_str(),
                serial_stop_bits.as_str(),
                serial_parity.as_str(),
                serial_flow_control.as_str(),
                serial_descriptor.as_ref(),
            ) {
                Ok(result) => result,
                Err(error) => {
                    set_status(&ui_for_save, &format!("Cannot save session: {error}"));
                    return;
                }
            };
            let profile_id = profile.id;
            let has_connection_password = connection_password.is_some();
            if let Err(error) = profile.validate() {
                set_status(&ui_for_save, &format!("Cannot save session: {error}"));
                return;
            }
            let should_connect = should_connect_after_save(connect_after_save, &profile);
            let connection_password = if should_connect {
                connection_password
            } else {
                None
            };
            let state = state_for_save.clone();
            let ui = ui_for_save.clone();
            set_status(&ui_for_save, "Saving session...");
            let runtime_for_save = runtime.clone();
            let runtime_for_connect = runtime.clone();
            runtime_for_save.spawn(async move {
                let credential_rollback = match apply_credential_change(
                    profile_id,
                    credential_change,
                )
                .await
                {
                    Ok(rollback) => rollback,
                    Err(error) => {
                        warn!(session_id = %profile_id, %error, "failed to update session credential");
                        set_status(&ui, &format!("Cannot update password: {error}"));
                        return;
                    }
                };

                let save_result = (|| -> Result<()> {
                    let mut app = state
                        .lock()
                        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
                    let mut candidate = app.sessions.clone();
                    candidate.upsert(profile.clone());
                    app.config.save(&candidate)?;
                    app.sessions = candidate;
                    Ok(())
                })();

                if let Err(error) = save_result {
                    if let Some(rollback) = credential_rollback
                        && let Err(rollback_error) = rollback.restore().await
                    {
                        warn!(session_id = %profile_id, %rollback_error, "failed to restore credential after profile save failure");
                    }
                    set_status(&ui, &format!("Cannot save session: {error}"));
                    return;
                }

                info!(
                    session_id = %profile_id,
                    protocol = profile.protocol().as_setting(),
                    credential_storage = ?profile.ssh().and_then(|ssh| ssh.credential_storage),
                    private_key = profile.ssh().is_some_and(|ssh| matches!(ssh.auth, AuthMethod::PrivateKey { .. })),
                    "session profile saved"
                );
                refresh_session_models(&ui, &state);
                if let Some(editor_tab_id) = editor_tab_id {
                    let _ = state.lock().map(|mut app| app.close_tab(editor_tab_id));
                }
                refresh_workspace(&ui, &state);
                if should_connect {
                    request_profile_connection(
                        &ui,
                        &state,
                        &runtime_for_connect,
                        profile_id,
                        ConnectionTarget::Terminal,
                        None,
                        connection_password,
                    );
                } else if has_connection_password && !remember_password {
                    set_status(&ui, "Session saved; password was not remembered");
                } else {
                    set_status(&ui, "Session saved");
                }
            });
        },
    );
}

pub(super) fn wire_session_management(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
) {
    let ui_for_duplicate = ui.as_weak();
    let state_for_duplicate = state.clone();
    let runtime_for_duplicate = runtime.clone();
    ui.on_duplicate_session(move |id| {
        log_ui_action("session.duplicate");
        let id = match parse_uuid(id.as_str(), "session", &ui_for_duplicate) {
            Some(id) => id,
            None => return,
        };
        let ui = ui_for_duplicate.clone();
        let state = state_for_duplicate.clone();
        runtime_for_duplicate.spawn(async move {
            match duplicate_session_profile(&state, id) {
                Ok(message) => {
                    refresh_session_models(&ui, &state);
                    refresh_workspace(&ui, &state);
                    set_status(&ui, &message);
                }
                Err(error) => {
                    set_status(&ui, &format!("Cannot duplicate session: {error}"));
                }
            }
        });
    });

    let ui_for_transfer = ui.as_weak();
    let state_for_transfer = state.clone();
    let runtime_for_transfer = runtime.clone();
    ui.on_session_transfer_action(move |action, target| {
        let action = action.as_str();
        let target = target.as_str().to_owned();
        match action {
            "copy-server" | "copy-group" => {
                log_ui_action(if action == "copy-server" {
                    "session.copy-config"
                } else {
                    "group.copy-config"
                });
                let export = match state_for_transfer.lock() {
                    Ok(app) if action == "copy-server" => {
                        export_session_profile(&app.sessions, &target)
                    }
                    Ok(app) => export_session_group(&app.sessions, &target),
                    Err(_) => Err(anyhow::anyhow!("state lock poisoned")),
                };
                match export {
                    Ok(text) => {
                        if let Some(ui) = ui_for_transfer.upgrade() {
                            set_platform_clipboard_text(&ui, &text);
                            set_status(
                                &ui_for_transfer,
                                if action == "copy-server" {
                                    "Server copied as AxSSH JSON"
                                } else {
                                    "Group copied as AxSSH JSON"
                                },
                            );
                        }
                    }
                    Err(error) => set_status(
                        &ui_for_transfer,
                        &format!("Cannot copy session configuration: {error}"),
                    ),
                }
            }
            "duplicate-group" => {
                log_ui_action("group.duplicate");
                let ui = ui_for_transfer.clone();
                let state = state_for_transfer.clone();
                runtime_for_transfer.spawn(async move {
                    match duplicate_session_group(&state, &target) {
                        Ok(message) => {
                            refresh_session_models(&ui, &state);
                            refresh_workspace(&ui, &state);
                            set_status(&ui, &message);
                        }
                        Err(error) => {
                            set_status(&ui, &format!("Cannot duplicate group: {error}"));
                        }
                    }
                });
            }
            "import-server" | "import-group" | "import-any" => {
                log_ui_action(match action {
                    "import-server" => "session.import-config",
                    "import-group" => "group.import-config",
                    _ => "session-list.import-config",
                });
                let Some(ui) = ui_for_transfer.upgrade() else {
                    return;
                };
                let clipboard = platform_clipboard_text(&ui);
                let mode = match action {
                    "import-server" => SessionImportMode::SingleServer,
                    "import-group" => SessionImportMode::IntoGroup,
                    _ => SessionImportMode::Automatic,
                };
                let ui = ui_for_transfer.clone();
                let state = state_for_transfer.clone();
                runtime_for_transfer.spawn(async move {
                    match import_session_transfer(&state, &clipboard, mode, &target) {
                        Ok(message) => {
                            refresh_session_models(&ui, &state);
                            refresh_workspace(&ui, &state);
                            set_status(&ui, &message);
                        }
                        Err(error) => {
                            set_status(&ui, &format!("Cannot import sessions: {error}"));
                        }
                    }
                });
            }
            "export-none" => {
                log_ui_action("session.export-without-selection");
                set_status(
                    &ui_for_transfer,
                    "Select a group or server before exporting",
                );
            }
            _ => set_status(&ui_for_transfer, "Unknown session transfer action"),
        }
    });

    let ui_for_action = ui.as_weak();
    ui.on_manage_session_action(move |action, target, value| {
        log_ui_action("session-management.execute");
        let action = action.as_str().to_owned();
        let target = target.as_str().to_owned();
        let value = value.as_str().to_owned();
        let ui = ui_for_action.clone();
        let state = state.clone();
        runtime.spawn(async move {
            let result = if action == "delete-session" {
                delete_session_profile(&state, &target).await
            } else {
                update_session_group(&state, &action, &target, &value)
            };
            match result {
                Ok(message) => {
                    refresh_session_models(&ui, &state);
                    refresh_workspace(&ui, &state);
                    set_status(&ui, &message);
                }
                Err(error) => {
                    set_status(&ui, &format!("Cannot update sessions: {error}"));
                }
            }
        });
    });
}

fn duplicate_session_profile(state: &Arc<Mutex<AppState>>, session_id: Uuid) -> Result<String> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut candidate = app.sessions.clone();
    let source = candidate
        .sessions
        .iter()
        .find(|profile| profile.id == session_id)
        .cloned()
        .context("session not found")?;
    let mut duplicate = source.clone();
    duplicate.id = Uuid::new_v4();
    duplicate.name = duplicate_session_name(&candidate, &source.name)?;
    if let Some(ssh) = duplicate.ssh_mut() {
        ssh.credential_storage = None;
    }
    duplicate.validate()?;
    candidate.upsert(duplicate);
    app.config.save(&candidate)?;
    app.sessions = candidate;
    info!(source_session_id = %session_id, "session profile duplicated");
    Ok(format!("Session {} duplicated", source.name))
}

fn export_session_profile(sessions: &SessionStore, session_id: &str) -> Result<String> {
    let session_id = Uuid::parse_str(session_id).context("invalid session id")?;
    let mut profile = sessions
        .sessions
        .iter()
        .find(|profile| profile.id == session_id)
        .cloned()
        .context("session not found")?;
    sanitize_transferred_profile(&mut profile);
    serialize_session_transfer(SessionTransferEnvelope {
        format: SESSION_TRANSFER_FORMAT.to_owned(),
        version: SESSION_TRANSFER_VERSION,
        kind: SessionTransferKind::Server,
        group_name: None,
        profiles: vec![profile],
    })
}

fn export_session_group(sessions: &SessionStore, group_name: &str) -> Result<String> {
    let group_name = normalized_transfer_group_name(group_name)?;
    if !sessions.groups.iter().any(|group| group == &group_name) {
        anyhow::bail!("group not found");
    }
    let mut profiles = sessions
        .sessions
        .iter()
        .filter(|profile| normalize_group_name(&profile.group_name) == group_name)
        .cloned()
        .collect::<Vec<_>>();
    if profiles.len() > MAX_SESSION_TRANSFER_PROFILES {
        anyhow::bail!("group exceeds the {MAX_SESSION_TRANSFER_PROFILES} server export limit");
    }
    for profile in &mut profiles {
        sanitize_transferred_profile(profile);
    }
    serialize_session_transfer(SessionTransferEnvelope {
        format: SESSION_TRANSFER_FORMAT.to_owned(),
        version: SESSION_TRANSFER_VERSION,
        kind: SessionTransferKind::Group,
        group_name: Some(group_name),
        profiles,
    })
}

fn serialize_session_transfer(envelope: SessionTransferEnvelope) -> Result<String> {
    validate_session_transfer(&envelope)?;
    let text =
        serde_json::to_string_pretty(&envelope).context("cannot serialize session export")?;
    if text.len() > MAX_SESSION_TRANSFER_BYTES {
        anyhow::bail!("session export exceeds the 256 KiB clipboard limit");
    }
    Ok(text)
}

fn parse_session_transfer(text: &str) -> Result<SessionTransferEnvelope> {
    if text.trim().is_empty() {
        anyhow::bail!("clipboard is empty");
    }
    if text.len() > MAX_SESSION_TRANSFER_BYTES {
        anyhow::bail!("clipboard data exceeds the 256 KiB import limit");
    }
    let mut envelope: SessionTransferEnvelope =
        serde_json::from_str(text).context("clipboard does not contain AxSSH session JSON")?;
    for profile in &mut envelope.profiles {
        sanitize_transferred_profile(profile);
    }
    validate_session_transfer(&envelope)?;
    Ok(envelope)
}

fn validate_session_transfer(envelope: &SessionTransferEnvelope) -> Result<()> {
    if envelope.format != SESSION_TRANSFER_FORMAT {
        anyhow::bail!("clipboard data is not an AxSSH session export");
    }
    if envelope.version != SESSION_TRANSFER_VERSION {
        anyhow::bail!("unsupported AxSSH session export version");
    }
    if envelope.profiles.len() > MAX_SESSION_TRANSFER_PROFILES {
        anyhow::bail!("session export exceeds the {MAX_SESSION_TRANSFER_PROFILES} server limit");
    }
    match envelope.kind {
        SessionTransferKind::Server => {
            if envelope.group_name.is_some() || envelope.profiles.len() != 1 {
                anyhow::bail!("server export must contain exactly one server");
            }
        }
        SessionTransferKind::Group => {
            let group_name = envelope
                .group_name
                .as_deref()
                .context("group export is missing its group name")?;
            normalized_transfer_group_name(group_name)?;
        }
    }
    for profile in &envelope.profiles {
        validate_transferred_profile(profile)?;
    }
    Ok(())
}

fn validate_transferred_profile(profile: &SessionProfile) -> Result<()> {
    profile.validate()?;
    validate_transfer_text(
        "session name",
        &profile.name,
        MAX_TRANSFER_SESSION_NAME_CHARS,
    )?;
    match &profile.connection {
        ConnectionProfile::Ssh(ssh) => {
            validate_transfer_text("SSH host", &ssh.host, MAX_TRANSFER_HOST_CHARS)?;
            validate_transfer_text("SSH username", &ssh.username, MAX_TRANSFER_USERNAME_CHARS)?;
            if let AuthMethod::PrivateKey { path } = &ssh.auth {
                let path = path
                    .to_str()
                    .context("private key path must use valid Unicode")?;
                validate_transfer_text("private key path", path, MAX_TRANSFER_KEY_PATH_CHARS)?;
            }
        }
        ConnectionProfile::Telnet(telnet) => {
            validate_transfer_text("Telnet host", &telnet.host, MAX_TRANSFER_HOST_CHARS)?;
        }
        ConnectionProfile::Serial(serial) => {
            validate_transfer_text("serial port", &serial.port_name, MAX_TRANSFER_HOST_CHARS)?;
            if let Some(serial_number) = &serial.usb_serial_number {
                validate_transfer_text("USB serial number", serial_number, 256)?;
            }
        }
    }
    Ok(())
}

fn validate_transfer_text(label: &str, value: &str, max_chars: usize) -> Result<()> {
    let value = value.trim();
    if value.is_empty() {
        anyhow::bail!("{label} cannot be empty");
    }
    if value.chars().count() > max_chars || value.chars().any(char::is_control) {
        anyhow::bail!("{label} is invalid");
    }
    Ok(())
}

fn sanitize_transferred_profile(profile: &mut SessionProfile) {
    profile.id = Uuid::nil();
    profile.group_name = normalize_group_name(&profile.group_name);
    if let Some(ssh) = profile.ssh_mut() {
        ssh.credential_storage = None;
        ssh.host_key_fingerprint = None;
    }
}

fn normalized_transfer_group_name(group_name: &str) -> Result<String> {
    let group_name = normalize_group_name(group_name);
    let mut validation_store = SessionStore::default();
    validation_store.add_group(&group_name)?;
    Ok(group_name)
}

fn duplicate_session_group(state: &Arc<Mutex<AppState>>, group_name: &str) -> Result<String> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut candidate = app.sessions.clone();
    let source_group = normalized_transfer_group_name(group_name)?;
    if !candidate.groups.iter().any(|group| group == &source_group) {
        anyhow::bail!("group not found");
    }
    let source_profiles = candidate
        .sessions
        .iter()
        .filter(|profile| normalize_group_name(&profile.group_name) == source_group)
        .cloned()
        .collect::<Vec<_>>();
    if source_profiles.len() > MAX_SESSION_TRANSFER_PROFILES {
        anyhow::bail!("group exceeds the {MAX_SESSION_TRANSFER_PROFILES} server duplicate limit");
    }
    let duplicate_group = duplicate_group_name(&candidate, &source_group)?;
    candidate.add_group(&duplicate_group)?;
    for source in source_profiles {
        let mut duplicate = source.clone();
        duplicate.id = Uuid::new_v4();
        duplicate.name = duplicate_session_name(&candidate, &source.name)?;
        duplicate.group_name = duplicate_group.clone();
        if let Some(ssh) = duplicate.ssh_mut() {
            ssh.credential_storage = None;
        }
        duplicate.validate()?;
        candidate.upsert(duplicate);
    }
    app.config.save(&candidate)?;
    app.sessions = candidate;
    info!("session group duplicated");
    Ok(format!(
        "Group {source_group} duplicated as {duplicate_group}"
    ))
}

fn import_session_transfer(
    state: &Arc<Mutex<AppState>>,
    text: &str,
    mode: SessionImportMode,
    target_group: &str,
) -> Result<String> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut candidate = app.sessions.clone();
    let (profile_count, imported_group) =
        import_session_transfer_into_store(&mut candidate, text, mode, target_group)?;
    app.config.save(&candidate)?;
    app.sessions = candidate;
    info!(profile_count, "session configuration imported");
    if profile_count == 0 {
        Ok(format!(
            "Group {} imported",
            imported_group.context("import did not create a group")?
        ))
    } else if profile_count == 1 {
        Ok("Imported 1 server".to_owned())
    } else {
        Ok(format!("Imported {profile_count} servers"))
    }
}

fn import_session_transfer_into_store(
    sessions: &mut SessionStore,
    text: &str,
    mode: SessionImportMode,
    target_group: &str,
) -> Result<(usize, Option<String>)> {
    let envelope = parse_session_transfer(text)?;
    let target_group = normalize_group_name(target_group);
    let imported_group = match mode {
        SessionImportMode::SingleServer => {
            if envelope.kind != SessionTransferKind::Server {
                anyhow::bail!("Import Server requires a single-server export");
            }
            if !target_group.is_empty() {
                ensure_session_group(sessions, &target_group)?;
            }
            Some(target_group)
        }
        SessionImportMode::IntoGroup => {
            let target_group = normalized_transfer_group_name(&target_group)?;
            if envelope.profiles.is_empty() {
                anyhow::bail!("clipboard export contains no servers");
            }
            ensure_session_group(sessions, &target_group)?;
            Some(target_group)
        }
        SessionImportMode::Automatic => match envelope.kind {
            SessionTransferKind::Group => {
                let source_group = envelope
                    .group_name
                    .as_deref()
                    .context("group export is missing its group name")?;
                let group = unique_imported_group_name(sessions, source_group)?;
                sessions.add_group(&group)?;
                Some(group)
            }
            SessionTransferKind::Server => None,
        },
    };

    let profile_count = envelope.profiles.len();
    for mut profile in envelope.profiles {
        let destination_group = imported_group
            .clone()
            .unwrap_or_else(|| normalize_group_name(&profile.group_name));
        if !destination_group.is_empty() {
            ensure_session_group(sessions, &destination_group)?;
        }
        profile.id = Uuid::new_v4();
        profile.group_name = destination_group;
        profile.name = unique_imported_session_name(sessions, &profile.name)?;
        sanitize_imported_profile(&mut profile);
        validate_transferred_profile(&profile)?;
        sessions.upsert(profile);
    }
    Ok((
        profile_count,
        imported_group.filter(|group| !group.is_empty()),
    ))
}

fn sanitize_imported_profile(profile: &mut SessionProfile) {
    let id = profile.id;
    let group_name = profile.group_name.clone();
    sanitize_transferred_profile(profile);
    profile.id = id;
    profile.group_name = group_name;
}

fn ensure_session_group(sessions: &mut SessionStore, group_name: &str) -> Result<()> {
    if group_name.is_empty() || sessions.groups.iter().any(|group| group == group_name) {
        return Ok(());
    }
    sessions.add_group(group_name)?;
    Ok(())
}

fn duplicate_group_name(sessions: &SessionStore, name: &str) -> Result<String> {
    let name = normalized_transfer_group_name(name)?;
    let mut copy_number = 1u32;
    loop {
        let suffix = if copy_number == 1 {
            " Copy".to_owned()
        } else {
            format!(" Copy {copy_number}")
        };
        let stem_limit = 64usize.saturating_sub(suffix.chars().count());
        let stem = name.chars().take(stem_limit).collect::<String>();
        let candidate = format!("{}{suffix}", stem.trim_end());
        normalized_transfer_group_name(&candidate)?;
        if !sessions.groups.iter().any(|group| group == &candidate) {
            return Ok(candidate);
        }
        copy_number = copy_number
            .checked_add(1)
            .context("too many duplicate group names")?;
    }
}

fn unique_imported_group_name(sessions: &SessionStore, name: &str) -> Result<String> {
    let name = normalized_transfer_group_name(name)?;
    if !sessions.groups.iter().any(|group| group == &name) {
        return Ok(name);
    }
    duplicate_group_name(sessions, &name)
}

fn unique_imported_session_name(sessions: &SessionStore, name: &str) -> Result<String> {
    if !sessions.sessions.iter().any(|profile| profile.name == name) {
        return Ok(name.to_owned());
    }
    let mut copy_number = 1u32;
    loop {
        let suffix = if copy_number == 1 {
            " Copy".to_owned()
        } else {
            format!(" Copy {copy_number}")
        };
        let stem_limit = MAX_TRANSFER_SESSION_NAME_CHARS.saturating_sub(suffix.chars().count());
        let stem = name.chars().take(stem_limit).collect::<String>();
        let candidate = format!("{}{suffix}", stem.trim_end());
        if !sessions
            .sessions
            .iter()
            .any(|profile| profile.name == candidate)
        {
            return Ok(candidate);
        }
        copy_number = copy_number
            .checked_add(1)
            .context("too many imported session name conflicts")?;
    }
}

fn duplicate_session_name(sessions: &SessionStore, name: &str) -> Result<String> {
    let base = format!("{name} Copy");
    if !sessions.sessions.iter().any(|profile| profile.name == base) {
        return Ok(base);
    }
    let mut suffix = 2u32;
    loop {
        let candidate = format!("{base} {suffix}");
        if !sessions
            .sessions
            .iter()
            .any(|profile| profile.name == candidate)
        {
            return Ok(candidate);
        }
        suffix = suffix
            .checked_add(1)
            .context("too many duplicate session names")?;
    }
}

fn update_session_group(
    state: &Arc<Mutex<AppState>>,
    action: &str,
    target: &str,
    value: &str,
) -> Result<String> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut candidate = app.sessions.clone();
    let (changed, message) = match action {
        "new-group" => {
            let group_name = normalize_group_name(value);
            let changed = candidate.add_group(&group_name)?;
            (changed, format!("Group {group_name} created"))
        }
        "rename-group" => {
            let old_name = normalize_group_name(target);
            let new_name = normalize_group_name(value);
            let changed = candidate.rename_group(&old_name, &new_name)?;
            (changed, format!("Group renamed to {new_name}"))
        }
        "delete-group" => {
            let group_name = normalize_group_name(target);
            let changed = candidate.remove_group(&group_name);
            (changed, format!("Group {group_name} removed"))
        }
        _ => anyhow::bail!("unknown session action"),
    };
    if !changed {
        anyhow::bail!("group was not changed");
    }
    app.config.save(&candidate)?;
    app.sessions = candidate;
    Ok(message)
}

async fn delete_session_profile(state: &Arc<Mutex<AppState>>, session_id: &str) -> Result<String> {
    let session_id = Uuid::parse_str(session_id).context("invalid session id")?;
    let profile = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?
        .sessions
        .sessions
        .iter()
        .find(|profile| profile.id == session_id)
        .cloned()
        .context("session not found")?;
    let credential_rollback = apply_credential_change(
        session_id,
        if let Some(storage) = profile.ssh().and_then(|ssh| ssh.credential_storage) {
            CredentialChange::Delete(storage)
        } else {
            CredentialChange::None
        },
    )
    .await?;
    let save_result = (|| -> Result<()> {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        let mut candidate = app.sessions.clone();
        if !candidate.remove(session_id) {
            anyhow::bail!("session not found");
        }
        app.config.save(&candidate)?;
        app.sessions = candidate;
        if app.active_editor_profile_id() == Some(Some(session_id))
            && let Some(tab_id) = app.active_tab_id()
        {
            let _ = app.close_tab(tab_id);
        }
        Ok(())
    })();
    if let Err(error) = save_result {
        if let Some(rollback) = credential_rollback
            && let Err(rollback_error) = rollback.restore().await
        {
            warn!(session_id = %session_id, %rollback_error, "failed to restore credential after session delete failure");
        }
        return Err(error);
    }
    info!(session_id = %session_id, "session profile deleted");
    Ok(format!("Session {} deleted", profile.name))
}

enum CredentialChange {
    None,
    Delete(CredentialStorage),
    Store {
        storage: CredentialStorage,
        previous_storage: Option<CredentialStorage>,
        password: zeroize::Zeroizing<String>,
        vault_password: Option<zeroize::Zeroizing<String>>,
    },
}

async fn apply_credential_change(
    session_id: Uuid,
    change: CredentialChange,
) -> Result<Option<CredentialRollback>> {
    match change {
        CredentialChange::None => Ok(None),
        CredentialChange::Delete(storage) => delete_password(session_id, storage).await.map(Some),
        CredentialChange::Store {
            storage,
            previous_storage,
            password,
            vault_password,
        } => save_password(
            storage,
            session_id,
            password,
            vault_password,
            previous_storage,
        )
        .await
        .map(Some),
    }
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
fn profile_from_editor(
    existing: Option<&SessionProfile>,
    name: &str,
    group_name: &str,
    protocol: &str,
    host: &str,
    port: &str,
    username: &str,
    auth_method: &str,
    private_key_path: &str,
    x11_forwarding: bool,
    serial_port: &str,
    serial_baud_rate: &str,
    serial_data_bits: &str,
    serial_stop_bits: &str,
    serial_parity: &str,
    serial_flow_control: &str,
    serial_descriptor: Option<&SerialPortDescriptor>,
) -> Result<(SessionProfile, CredentialChange)> {
    let (profile, credential_change, connection_password) = profile_from_editor_with_password(
        existing,
        name,
        group_name,
        protocol,
        host,
        port,
        username,
        auth_method,
        private_key_path,
        "",
        false,
        CredentialStorage::SystemKeyring.as_setting(),
        "",
        x11_forwarding,
        serial_port,
        serial_baud_rate,
        serial_data_bits,
        serial_stop_bits,
        serial_parity,
        serial_flow_control,
        serial_descriptor,
    )?;
    debug_assert!(connection_password.is_none());
    Ok((profile, credential_change))
}

#[allow(clippy::too_many_arguments)]
fn profile_from_editor_with_password(
    existing: Option<&SessionProfile>,
    name: &str,
    group_name: &str,
    protocol: &str,
    host: &str,
    port: &str,
    username: &str,
    auth_method: &str,
    private_key_path: &str,
    password: &str,
    remember_password: bool,
    credential_storage: &str,
    vault_password: &str,
    x11_forwarding: bool,
    serial_port: &str,
    serial_baud_rate: &str,
    serial_data_bits: &str,
    serial_stop_bits: &str,
    serial_parity: &str,
    serial_flow_control: &str,
    serial_descriptor: Option<&SerialPortDescriptor>,
) -> Result<(
    SessionProfile,
    CredentialChange,
    Option<zeroize::Zeroizing<String>>,
)> {
    let private_key = auth_method == "Private key";
    let ssh_protocol = protocol.eq_ignore_ascii_case("SSH");
    let existing_storage = existing
        .and_then(SessionProfile::ssh)
        .and_then(|ssh| ssh.credential_storage);
    let password_auth = !private_key && ssh_protocol;
    let password_storage = (password_auth && !password.is_empty() && remember_password)
        .then(|| CredentialStorage::from_setting(credential_storage));
    if let Some(storage) = password_storage
        && storage == CredentialStorage::EncryptedVault
        && vault_password.is_empty()
    {
        anyhow::bail!("vault password is required when saving an SSH password");
    }
    let credential_change = if let Some(storage) = password_storage {
        CredentialChange::Store {
            storage,
            previous_storage: existing_storage,
            password: zeroize::Zeroizing::new(password.to_owned()),
            vault_password: (storage == CredentialStorage::EncryptedVault)
                .then(|| zeroize::Zeroizing::new(vault_password.to_owned())),
        }
    } else if !ssh_protocol || private_key {
        existing_storage.map_or(CredentialChange::None, CredentialChange::Delete)
    } else {
        CredentialChange::None
    };
    let connection_password = (password_auth && !password.is_empty())
        .then(|| zeroize::Zeroizing::new(password.to_owned()));

    let mut profile = if ssh_protocol {
        let port = parse_network_port(port)?;
        let normalized_host = host.trim();
        let mut profile = SessionProfile::new(name.trim(), normalized_host, username.trim());
        let preserved_fingerprint = existing.and_then(SessionProfile::ssh).and_then(|ssh| {
            (ssh.host.trim() == normalized_host && ssh.port == port)
                .then(|| ssh.host_key_fingerprint.clone())
                .flatten()
        });
        let ssh = profile
            .ssh_mut()
            .context("new SSH profile is missing SSH configuration")?;
        ssh.port = port;
        ssh.auth = if private_key {
            AuthMethod::PrivateKey {
                path: PathBuf::from(private_key_path.trim()),
            }
        } else {
            AuthMethod::Password
        };
        ssh.credential_storage = if private_key {
            None
        } else {
            password_storage.or(existing_storage)
        };
        ssh.host_key_fingerprint = preserved_fingerprint;
        ssh.x11_forwarding = x11_forwarding;
        profile
    } else if protocol.eq_ignore_ascii_case("Telnet") {
        let port = parse_network_port(port)?;
        let mut profile = SessionProfile::new_telnet(name.trim(), host.trim());
        let ConnectionProfile::Telnet(config) = &mut profile.connection else {
            anyhow::bail!("new Telnet profile is missing Telnet configuration");
        };
        config.port = port;
        profile
    } else if protocol.eq_ignore_ascii_case("Serial") {
        let baud_rate = serial_baud_rate
            .trim()
            .parse::<u32>()
            .context("baud rate must be a number")?;
        let port_name = serial_port.trim();
        let mut profile = SessionProfile::new_serial(name.trim(), port_name);
        let ConnectionProfile::Serial(config) = &mut profile.connection else {
            anyhow::bail!("new Serial profile is missing Serial configuration");
        };
        config.baud_rate = baud_rate;
        config.data_bits = SerialDataBits::from_setting(serial_data_bits);
        config.stop_bits = SerialStopBits::from_setting(serial_stop_bits);
        config.parity = SerialParity::from_setting(serial_parity);
        config.flow_control = SerialFlowControl::from_setting(serial_flow_control);
        if let Some(descriptor) = serial_descriptor {
            descriptor.apply_identity_to(config);
        } else if let Some(existing) = existing.and_then(SessionProfile::serial)
            && existing.port_name == port_name
        {
            config.usb_vendor_id = existing.usb_vendor_id;
            config.usb_product_id = existing.usb_product_id;
            config
                .usb_serial_number
                .clone_from(&existing.usb_serial_number);
        }
        profile
    } else {
        anyhow::bail!("unknown connection protocol");
    };
    if let Some(existing) = existing {
        profile.id = existing.id;
    }
    profile.group_name = normalize_group_name(group_name);
    Ok((profile, credential_change, connection_password))
}

fn should_connect_after_save(connect_after_save: bool, profile: &SessionProfile) -> bool {
    connect_after_save && profile.ssh().is_some()
}

fn parse_network_port(value: &str) -> Result<u16> {
    match value.trim().parse::<u16>() {
        Ok(port) if port > 0 => Ok(port),
        _ => anyhow::bail!("port must be a number between 1 and 65535"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_connect_is_limited_to_ssh_profiles() {
        let ssh = SessionProfile::new("server", "server.example", "alice");
        let telnet = SessionProfile::new_telnet("console", "console.example");

        assert!(should_connect_after_save(true, &ssh));
        assert!(!should_connect_after_save(false, &ssh));
        assert!(!should_connect_after_save(true, &telnet));
    }

    #[test]
    fn editing_without_a_password_preserves_an_existing_credential() {
        let mut existing = SessionProfile::new("old", "old.example", "alice");
        let ssh = existing.ssh_mut().expect("profile should be SSH");
        ssh.credential_storage = Some(CredentialStorage::EncryptedVault);
        ssh.host_key_fingerprint = Some("SHA256:trusted".into());
        ssh.x11_forwarding = true;

        let (profile, change) = profile_from_editor(
            Some(&existing),
            "new",
            "Production",
            "SSH",
            "old.example",
            "22",
            "alice",
            "Password",
            "",
            true,
            "",
            "115200",
            "8",
            "1",
            "none",
            "none",
            None,
        )
        .expect("profile should update");

        assert_eq!(profile.id, existing.id);
        assert_eq!(
            profile
                .ssh()
                .expect("updated profile should be SSH")
                .credential_storage,
            Some(CredentialStorage::EncryptedVault)
        );
        assert_eq!(
            profile
                .ssh()
                .expect("updated profile should be SSH")
                .host_key_fingerprint,
            existing
                .ssh()
                .expect("existing profile should be SSH")
                .host_key_fingerprint
        );
        assert!(
            profile
                .ssh()
                .expect("updated profile should be SSH")
                .x11_forwarding
        );
        assert!(matches!(change, CredentialChange::None));
    }

    #[test]
    fn remembering_a_password_updates_the_existing_credential_backend() {
        let mut existing = SessionProfile::new("old", "old.example", "alice");
        let ssh = existing.ssh_mut().expect("profile should be SSH");
        ssh.credential_storage = Some(CredentialStorage::SystemKeyring);

        let (profile, change, connection_password) = profile_from_editor_with_password(
            Some(&existing),
            "updated",
            "",
            "SSH",
            "old.example",
            "22",
            "alice",
            "Password",
            "",
            "new-password",
            true,
            "system-keyring",
            "",
            true,
            "",
            "115200",
            "8",
            "1",
            "none",
            "none",
            None,
        )
        .expect("profile should update");

        assert_eq!(
            profile.ssh().and_then(|ssh| ssh.credential_storage),
            Some(CredentialStorage::SystemKeyring)
        );
        assert_eq!(
            connection_password
                .as_ref()
                .expect("entered password should be available for immediate connection")
                .as_str(),
            "new-password"
        );
        match change {
            CredentialChange::Store {
                storage,
                previous_storage,
                password,
                vault_password,
            } => {
                assert_eq!(storage, CredentialStorage::SystemKeyring);
                assert_eq!(previous_storage, Some(CredentialStorage::SystemKeyring));
                assert_eq!(password.as_str(), "new-password");
                assert!(vault_password.is_none());
            }
            CredentialChange::None | CredentialChange::Delete(_) => {
                panic!("entered password should be stored")
            }
        }
    }

    #[test]
    fn new_password_is_one_time_by_default() {
        let (profile, change, connection_password) = profile_from_editor_with_password(
            None,
            "new",
            "",
            "SSH",
            "new.example",
            "22",
            "alice",
            "Password",
            "",
            "new-password",
            false,
            "encrypted-vault",
            "",
            true,
            "",
            "115200",
            "8",
            "1",
            "none",
            "none",
            None,
        )
        .expect("one-time password should not require a vault password");

        assert_eq!(profile.ssh().and_then(|ssh| ssh.credential_storage), None);
        assert!(matches!(change, CredentialChange::None));
        assert_eq!(
            connection_password
                .expect("one-time password should be returned for immediate connection")
                .as_str(),
            "new-password"
        );
    }

    #[test]
    fn remembering_new_password_in_vault_requires_vault_password() {
        let error = match profile_from_editor_with_password(
            None,
            "new",
            "",
            "SSH",
            "new.example",
            "22",
            "alice",
            "Password",
            "",
            "new-password",
            true,
            "encrypted-vault",
            "",
            true,
            "",
            "115200",
            "8",
            "1",
            "none",
            "none",
            None,
        ) {
            Ok(_) => panic!("encrypted-vault password updates require a vault password"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("vault password"));

        let (profile, change, connection_password) = profile_from_editor_with_password(
            None,
            "new",
            "",
            "SSH",
            "new.example",
            "22",
            "alice",
            "Password",
            "",
            "new-password",
            true,
            "encrypted-vault",
            "vault-password",
            true,
            "",
            "115200",
            "8",
            "1",
            "none",
            "none",
            None,
        )
        .expect("profile should save with a vault password");
        assert_eq!(
            profile.ssh().and_then(|ssh| ssh.credential_storage),
            Some(CredentialStorage::EncryptedVault)
        );
        assert!(matches!(change, CredentialChange::Store { .. }));
        assert_eq!(
            connection_password
                .expect("remembered password should also be used for immediate connection")
                .as_str(),
            "new-password"
        );
    }

    #[test]
    fn one_time_password_preserves_an_existing_remembered_credential() {
        let mut existing = SessionProfile::new("old", "old.example", "alice");
        existing
            .ssh_mut()
            .expect("profile should be SSH")
            .credential_storage = Some(CredentialStorage::SystemKeyring);

        let (profile, change, connection_password) = profile_from_editor_with_password(
            Some(&existing),
            "updated",
            "",
            "SSH",
            "old.example",
            "22",
            "alice",
            "Password",
            "",
            "temporary-password",
            false,
            "encrypted-vault",
            "",
            true,
            "",
            "115200",
            "8",
            "1",
            "none",
            "none",
            None,
        )
        .expect("one-time password should update the connection draft");

        assert_eq!(
            profile.ssh().and_then(|ssh| ssh.credential_storage),
            Some(CredentialStorage::SystemKeyring)
        );
        assert!(matches!(change, CredentialChange::None));
        assert_eq!(
            connection_password
                .expect("one-time password should be available for connection")
                .as_str(),
            "temporary-password"
        );
    }

    #[test]
    fn non_ssh_editor_protocols_cannot_retain_x11_state() {
        let (telnet, _) = profile_from_editor(
            None,
            "console",
            "",
            "Telnet",
            "router.example",
            "23",
            "",
            "Password",
            "",
            true,
            "",
            "115200",
            "8",
            "1",
            "none",
            "none",
            None,
        )
        .expect("Telnet profile should be created");
        let encoded = serde_json::to_string(&telnet).expect("Telnet profile should serialize");
        assert!(!encoded.contains("x11_forwarding"));
    }

    #[test]
    fn switching_to_private_key_preserves_trust_and_deletes_the_credential() {
        let mut existing = SessionProfile::new("old", "old.example", "alice");
        let ssh = existing.ssh_mut().expect("profile should be SSH");
        ssh.credential_storage = Some(CredentialStorage::SystemKeyring);
        ssh.host_key_fingerprint = Some("SHA256:trusted".into());

        let (profile, change) = profile_from_editor(
            Some(&existing),
            "new",
            "",
            "SSH",
            "old.example",
            "22",
            "alice",
            "Private key",
            "/tmp/id_ed25519",
            false,
            "",
            "115200",
            "8",
            "1",
            "none",
            "none",
            None,
        )
        .expect("profile should update");

        assert_eq!(profile.id, existing.id);
        assert_eq!(
            profile
                .ssh()
                .expect("updated profile should be SSH")
                .credential_storage,
            None
        );
        assert_eq!(
            profile
                .ssh()
                .expect("updated profile should be SSH")
                .host_key_fingerprint,
            existing
                .ssh()
                .expect("existing profile should be SSH")
                .host_key_fingerprint
        );
        assert!(matches!(
            change,
            CredentialChange::Delete(CredentialStorage::SystemKeyring)
        ));
    }

    #[test]
    fn group_management_persists_and_moves_profiles_to_ungrouped_on_delete() {
        let path = std::env::temp_dir().join(format!("ax-ssh-groups-{}.json", Uuid::new_v4()));
        let mut sessions = SessionStore::default();
        let mut profile = SessionProfile::new("server", "server.example", "alice");
        profile.group_name = "Production".into();
        sessions.upsert(profile.clone());
        let state = Arc::new(Mutex::new(AppState::new(ConfigStore::new(&path), sessions)));

        update_session_group(&state, "new-group", "", "Staging").expect("group should be added");
        update_session_group(&state, "rename-group", "Staging", "QA")
            .expect("group should be renamed");
        update_session_group(&state, "delete-group", "Production", "")
            .expect("group should be removed");

        let app = state.lock().expect("state should remain readable");
        assert_eq!(app.sessions.groups, ["QA"]);
        assert_eq!(app.sessions.sessions[0].group_name, "");
        assert_eq!(
            app.config.load().expect("saved state should load"),
            app.sessions
        );
        drop(app);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn duplicate_session_name_gets_a_unique_copy_suffix() {
        let mut sessions = SessionStore::default();
        sessions.upsert(SessionProfile::new("server Copy", "copy.example", "alice"));
        sessions.upsert(SessionProfile::new(
            "server Copy 2",
            "copy2.example",
            "alice",
        ));

        assert_eq!(
            duplicate_session_name(&sessions, "server").unwrap(),
            "server Copy 3"
        );
    }

    #[test]
    fn duplicating_a_profile_keeps_connection_trust_without_reusing_credentials() {
        let path = std::env::temp_dir().join(format!("ax-ssh-duplicate-{}.json", Uuid::new_v4()));
        let mut sessions = SessionStore::default();
        let mut source = SessionProfile::new("production", "server.example", "alice");
        source.group_name = "Production".into();
        let source_ssh = source.ssh_mut().expect("profile should be SSH");
        source_ssh.credential_storage = Some(CredentialStorage::SystemKeyring);
        source_ssh.host_key_fingerprint = Some("SHA256:trusted".into());
        source_ssh.x11_forwarding = true;
        sessions.upsert(source.clone());
        let state = Arc::new(Mutex::new(AppState::new(ConfigStore::new(&path), sessions)));

        duplicate_session_profile(&state, source.id).expect("profile should duplicate");

        let app = state.lock().expect("state should remain readable");
        assert_eq!(app.sessions.sessions.len(), 2);
        let duplicate = app
            .sessions
            .sessions
            .iter()
            .find(|profile| profile.id != source.id)
            .expect("duplicate should be present");
        assert_eq!(duplicate.name, "production Copy");
        assert_eq!(duplicate.group_name, source.group_name);
        let duplicate_ssh = duplicate.ssh().expect("duplicate should be SSH");
        let source_ssh = source.ssh().expect("source should be SSH");
        assert_eq!(duplicate_ssh.host, source_ssh.host);
        assert_eq!(duplicate_ssh.port, source_ssh.port);
        assert_eq!(duplicate_ssh.username, source_ssh.username);
        assert_eq!(duplicate_ssh.auth, source_ssh.auth);
        assert!(duplicate_ssh.x11_forwarding);
        assert_eq!(duplicate_ssh.credential_storage, None);
        assert_eq!(
            duplicate_ssh.host_key_fingerprint.as_deref(),
            Some("SHA256:trusted")
        );
        assert_eq!(
            app.config.load().expect("saved state should load"),
            app.sessions
        );
        drop(app);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn server_export_removes_identity_credentials_and_host_trust() {
        let mut sessions = SessionStore::default();
        let mut source = SessionProfile::new("production", "server.example", "alice");
        source.group_name = "Production".into();
        let source_ssh = source.ssh_mut().expect("profile should be SSH");
        source_ssh.credential_storage = Some(CredentialStorage::SystemKeyring);
        source_ssh.host_key_fingerprint = Some("SHA256:trusted".into());
        source_ssh.x11_forwarding = true;
        sessions.upsert(source.clone());

        let text = export_session_profile(&sessions, &source.id.to_string())
            .expect("profile should export");
        let exported: SessionTransferEnvelope =
            serde_json::from_str(&text).expect("export should be valid JSON");

        assert_eq!(exported.format, SESSION_TRANSFER_FORMAT);
        assert_eq!(exported.version, SESSION_TRANSFER_VERSION);
        assert_eq!(exported.kind, SessionTransferKind::Server);
        assert_eq!(exported.profiles.len(), 1);
        let profile = &exported.profiles[0];
        assert_eq!(profile.id, Uuid::nil());
        assert_eq!(profile.group_name, "Production");
        let ssh = profile.ssh().expect("exported profile should be SSH");
        assert_eq!(ssh.credential_storage, None);
        assert_eq!(ssh.host_key_fingerprint, None);
        assert!(ssh.x11_forwarding);
    }

    #[test]
    fn group_export_round_trips_profiles_and_empty_groups_without_security_state() {
        let mut sessions = SessionStore::default();
        sessions
            .add_group("Production")
            .expect("group should be added");
        sessions.add_group("Empty").expect("group should be added");
        let mut source = SessionProfile::new("server", "server.example", "alice");
        source.group_name = "Production".into();
        let source_id = source.id;
        let source_ssh = source.ssh_mut().expect("profile should be SSH");
        source_ssh.credential_storage = Some(CredentialStorage::SystemKeyring);
        source_ssh.host_key_fingerprint = Some("SHA256:trusted".into());
        source_ssh.x11_forwarding = true;
        sessions.upsert(source);

        let text =
            export_session_group(&sessions, "Production").expect("populated group should export");
        let exported: SessionTransferEnvelope =
            serde_json::from_str(&text).expect("export should be valid JSON");
        assert_eq!(exported.kind, SessionTransferKind::Group);
        assert_eq!(exported.group_name.as_deref(), Some("Production"));
        assert_eq!(exported.profiles.len(), 1);
        let exported_profile = &exported.profiles[0];
        assert_eq!(exported_profile.id, Uuid::nil());
        let exported_ssh = exported_profile
            .ssh()
            .expect("exported profile should be SSH");
        assert_eq!(exported_ssh.credential_storage, None);
        assert_eq!(exported_ssh.host_key_fingerprint, None);
        assert!(exported_ssh.x11_forwarding);

        let mut imported = SessionStore::default();
        let (count, imported_group) = import_session_transfer_into_store(
            &mut imported,
            &text,
            SessionImportMode::Automatic,
            "",
        )
        .expect("group export should import");
        assert_eq!(count, 1);
        assert_eq!(imported_group.as_deref(), Some("Production"));
        let imported_profile = imported
            .sessions
            .first()
            .expect("imported profile should be present");
        assert_ne!(imported_profile.id, source_id);
        assert_ne!(imported_profile.id, Uuid::nil());
        assert_eq!(imported_profile.group_name, "Production");
        let imported_ssh = imported_profile
            .ssh()
            .expect("imported profile should be SSH");
        assert_eq!(imported_ssh.credential_storage, None);
        assert_eq!(imported_ssh.host_key_fingerprint, None);
        assert!(imported_ssh.x11_forwarding);

        let empty_text =
            export_session_group(&sessions, "Empty").expect("empty group should export");
        let empty_export: SessionTransferEnvelope =
            serde_json::from_str(&empty_text).expect("empty export should be valid JSON");
        assert_eq!(empty_export.group_name.as_deref(), Some("Empty"));
        assert!(empty_export.profiles.is_empty());
    }

    #[test]
    fn importing_a_server_rekeys_redacts_and_resolves_name_conflicts() {
        let mut sessions = SessionStore::default();
        sessions.upsert(SessionProfile::new(
            "production",
            "existing.example",
            "alice",
        ));
        let mut source = SessionProfile::new("production", "server.example", "bob");
        source.group_name = "Production".into();
        let source_id = source.id;
        let source_ssh = source.ssh_mut().expect("profile should be SSH");
        source_ssh.credential_storage = Some(CredentialStorage::EncryptedVault);
        source_ssh.host_key_fingerprint = Some("SHA256:trusted".into());
        let text = serde_json::to_string(&SessionTransferEnvelope {
            format: SESSION_TRANSFER_FORMAT.into(),
            version: SESSION_TRANSFER_VERSION,
            kind: SessionTransferKind::Server,
            group_name: None,
            profiles: vec![source],
        })
        .expect("fixture should serialize");

        let (count, imported_group) = import_session_transfer_into_store(
            &mut sessions,
            &text,
            SessionImportMode::Automatic,
            "",
        )
        .expect("profile should import");

        assert_eq!(count, 1);
        assert_eq!(imported_group, None);
        assert!(sessions.groups.iter().any(|group| group == "Production"));
        let imported = sessions
            .sessions
            .iter()
            .find(|profile| profile.name == "production Copy")
            .expect("renamed import should be present");
        assert_ne!(imported.id, source_id);
        assert_ne!(imported.id, Uuid::nil());
        assert_eq!(imported.group_name, "Production");
        let ssh = imported.ssh().expect("imported profile should be SSH");
        assert_eq!(ssh.credential_storage, None);
        assert_eq!(ssh.host_key_fingerprint, None);
    }

    #[test]
    fn duplicating_a_group_rekeys_profiles_and_keeps_empty_groups_supported() {
        let path = std::env::temp_dir().join(format!("ax-ssh-group-copy-{}.json", Uuid::new_v4()));
        let mut sessions = SessionStore::default();
        sessions
            .add_group("Production")
            .expect("group should be added");
        sessions.add_group("Empty").expect("group should be added");
        let mut source = SessionProfile::new("server", "server.example", "alice");
        source.group_name = "Production".into();
        let source_ssh = source.ssh_mut().expect("profile should be SSH");
        source_ssh.credential_storage = Some(CredentialStorage::SystemKeyring);
        source_ssh.host_key_fingerprint = Some("SHA256:trusted".into());
        sessions.upsert(source.clone());
        let state = Arc::new(Mutex::new(AppState::new(ConfigStore::new(&path), sessions)));

        duplicate_session_group(&state, "Production").expect("group should duplicate");
        duplicate_session_group(&state, "Empty").expect("empty group should duplicate");

        let app = state.lock().expect("state should remain readable");
        assert!(
            app.sessions
                .groups
                .iter()
                .any(|group| group == "Production Copy")
        );
        assert!(
            app.sessions
                .groups
                .iter()
                .any(|group| group == "Empty Copy")
        );
        let duplicate = app
            .sessions
            .sessions
            .iter()
            .find(|profile| profile.group_name == "Production Copy")
            .expect("duplicated group profile should be present");
        assert_ne!(duplicate.id, source.id);
        assert_eq!(duplicate.name, "server Copy");
        let ssh = duplicate.ssh().expect("duplicated profile should be SSH");
        assert_eq!(ssh.credential_storage, None);
        assert_eq!(ssh.host_key_fingerprint.as_deref(), Some("SHA256:trusted"));
        drop(app);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn session_import_rejects_oversized_clipboard_data() {
        let oversized = "x".repeat(MAX_SESSION_TRANSFER_BYTES + 1);
        assert!(parse_session_transfer(&oversized).is_err());
    }

    #[test]
    fn automatic_group_import_normalizes_conflicts_and_preserves_empty_groups() {
        let mut sessions = SessionStore::default();
        sessions
            .add_group("Production")
            .expect("existing group should be added");
        let text = serde_json::to_string(&SessionTransferEnvelope {
            format: SESSION_TRANSFER_FORMAT.into(),
            version: SESSION_TRANSFER_VERSION,
            kind: SessionTransferKind::Group,
            group_name: Some(" Production ".into()),
            profiles: Vec::new(),
        })
        .expect("fixture should serialize");

        let (count, imported_group) = import_session_transfer_into_store(
            &mut sessions,
            &text,
            SessionImportMode::Automatic,
            "",
        )
        .expect("empty group should import");

        assert_eq!(count, 0);
        assert_eq!(imported_group.as_deref(), Some("Production Copy"));
        assert!(
            sessions
                .groups
                .iter()
                .any(|group| group == "Production Copy")
        );
    }

    #[tokio::test]
    async fn deleting_a_profile_keeps_open_terminal_tabs() {
        let path = std::env::temp_dir().join(format!("ax-ssh-delete-{}.json", Uuid::new_v4()));
        let mut sessions = SessionStore::default();
        let profile = SessionProfile::new("server", "server.example", "alice");
        sessions.upsert(profile.clone());
        let mut app = AppState::new(ConfigStore::new(&path), sessions);
        let terminal_id = app.open_terminal_tab(&profile);
        let state = Arc::new(Mutex::new(app));

        delete_session_profile(&state, &profile.id.to_string())
            .await
            .expect("profile should be deleted");

        let app = state.lock().expect("state should remain readable");
        assert!(app.sessions.sessions.is_empty());
        assert!(app.terminal(terminal_id).is_some());
        drop(app);
        let _ = std::fs::remove_file(path);
    }
}
