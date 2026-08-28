use super::*;
use crate::app::credential_tasks::{CredentialRollback, credential_storage_for_save};
use crate::app::state::PersistenceCoordinator;

pub(in crate::app) struct SessionEditorContext {
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    persistence: Arc<PersistenceCoordinator>,
    font_registry: Arc<Mutex<FontRegistry>>,
    terminal_font_started: Arc<std::sync::atomic::AtomicBool>,
    window_router: WindowRouter,
    window_id: Uuid,
}

impl SessionEditorContext {
    pub(in crate::app) fn new(
        state: Arc<Mutex<AppState>>,
        runtime: Handle,
        persistence: Arc<PersistenceCoordinator>,
        font_registry: Arc<Mutex<FontRegistry>>,
        terminal_font_started: Arc<std::sync::atomic::AtomicBool>,
        window_router: WindowRouter,
        window_id: Uuid,
    ) -> Self {
        Self {
            state,
            runtime,
            persistence,
            font_registry,
            terminal_font_started,
            window_router,
            window_id,
        }
    }
}

pub(in crate::app) fn wire_session_editor(ui: &AppWindow, context: SessionEditorContext) {
    let SessionEditorContext {
        state,
        runtime,
        persistence,
        font_registry,
        terminal_font_started,
        window_router,
        window_id,
    } = context;
    let ui_for_private_keys = ui.as_weak();
    let state_for_private_keys = state.clone();
    let runtime_for_private_keys = runtime.clone();
    ui.on_private_key_mode_changed(move |enabled| {
        if enabled {
            log_ui_action("session-editor.enter-private-key-mode");
            load_private_key_options(
                &runtime_for_private_keys,
                state_for_private_keys.clone(),
                ui_for_private_keys.clone(),
            );
        } else {
            clear_private_key_option_model(&ui_for_private_keys);
        }
    });

    let ui_for_save = ui.as_weak();
    let state_for_save = state.clone();
    let router_for_save = window_router;
    ui.on_save_session(
        move |name,
              group_name,
              protocol,
              host,
              port,
              username,
              auth_method,
              private_key_path,
              sftp_remote_path,
              sftp_local_path,
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
            let (editor_tab_id, editor_draft_id, existing_profile, serial_descriptor) = match state_for_save.lock() {
                Ok(app) => {
                    let Some((editor_tab_id, editor_draft_id, profile_id)) = app.active_editor_identity() else {
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
                        editor_tab_id,
                        editor_draft_id,
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
                sftp_remote_path.as_str(),
                sftp_local_path.as_str(),
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
            let mutation_token = match begin_profile_mutation(
                &state_for_save,
                profile_id,
                existing_profile.as_ref(),
            ) {
                Ok(token) => token,
                Err(error) => {
                    set_status(&ui_for_save, &format!("Cannot save session: {error}"));
                    return;
                }
            };
            let should_connect = should_connect_after_save(connect_after_save, &profile);
            let connection_password = if should_connect {
                connection_password
            } else {
                None
            };
            let state = state_for_save.clone();
            let ui = ui_for_save.clone();
            let router = router_for_save.clone();
            set_status(&ui_for_save, "Saving session...");
            let runtime_for_save = runtime.clone();
            let runtime_for_connect = runtime.clone();
            let font_registry_for_connect = font_registry.clone();
            let terminal_font_started_for_connect = terminal_font_started.clone();
            let persistence = persistence.clone();
            runtime_for_save.spawn(async move {
                let _mutation_guard = persistence.gate.lock().await;
                if let Err(error) = ensure_profile_mutation_current(
                    &state,
                    profile_id,
                    mutation_token,
                    existing_profile.as_ref(),
                ) {
                    debug!(session_id = %profile_id, %error, "stale profile save ignored");
                    finish_profile_mutation(&state, profile_id, mutation_token);
                    set_status(&ui, "Session changed while saving; retry your changes");
                    return;
                }
                let credential_rollback = match apply_credential_change(
                    profile_id,
                    credential_change,
                )
                .await
                {
                    Ok(rollback) => rollback,
                    Err(error) => {
                        finish_profile_mutation(&state, profile_id, mutation_token);
                        warn!(session_id = %profile_id, %error, "failed to update session credential");
                        set_status(&ui, &format!("Cannot update password: {error}"));
                        return;
                    }
                };

                let save_result = commit_profile_save(
                    &state,
                    &profile,
                    existing_profile.as_ref(),
                    mutation_token,
                );

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
                let editor_closed = state.lock().is_ok_and(|mut app| {
                    if app.editor_matches(editor_tab_id, editor_draft_id) {
                        app.close_tab(editor_tab_id).is_some()
                    } else {
                        false
                    }
                });
                if editor_closed {
                    clear_session_editor_resources(&ui);
                }
                refresh_workspace(&ui, &state);
                if should_connect {
                    let connection = ConnectionContext::new(
                        ui.clone(),
                        state.clone(),
                        runtime_for_connect,
                        font_registry_for_connect,
                        terminal_font_started_for_connect,
                    );
                    let _ = request_profile_connection(
                        &connection,
                        profile_id,
                        ConnectionTarget::Terminal,
                        None,
                        None,
                        connection_password,
                        move |tab_id, app| router.activate_tab(window_id, tab_id, app),
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

pub(super) async fn delete_session_profile(
    state: &Arc<Mutex<AppState>>,
    persistence: &PersistenceCoordinator,
    session_id: &str,
) -> Result<String> {
    let session_id = Uuid::parse_str(session_id).context("invalid session id")?;
    let profile = {
        let app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        app.sessions
            .sessions
            .iter()
            .find(|profile| profile.id == session_id)
            .cloned()
            .context("session not found")?
    };
    let mutation_token = begin_profile_mutation(state, session_id, Some(&profile))?;
    let _mutation_guard = persistence.gate.lock().await;
    if let Err(error) =
        ensure_profile_mutation_current(state, session_id, mutation_token, Some(&profile))
    {
        finish_profile_mutation(state, session_id, mutation_token);
        return Err(error);
    }
    let credential_rollback = match apply_credential_change(
        session_id,
        if let Some(storage) = profile.ssh().and_then(|ssh| ssh.credential_storage) {
            CredentialChange::Delete(storage)
        } else {
            CredentialChange::None
        },
    )
    .await
    {
        Ok(rollback) => rollback,
        Err(error) => {
            finish_profile_mutation(state, session_id, mutation_token);
            return Err(error);
        }
    };
    let save_result = commit_profile_delete(state, &profile, mutation_token);
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

pub(in crate::app) fn begin_profile_mutation(
    state: &Arc<Mutex<AppState>>,
    profile_id: Uuid,
    expected: Option<&SessionProfile>,
) -> Result<Uuid> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let current = app
        .sessions
        .sessions
        .iter()
        .find(|profile| profile.id == profile_id);
    if current != expected {
        anyhow::bail!("session changed before the save started");
    }
    if app.profile_mutation_is_pending(profile_id) {
        anyhow::bail!("session is already being modified");
    }
    Ok(app.begin_profile_mutation(profile_id))
}

pub(in crate::app) fn ensure_profile_mutation_current(
    state: &Arc<Mutex<AppState>>,
    profile_id: Uuid,
    token: Uuid,
    expected: Option<&SessionProfile>,
) -> Result<()> {
    let app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let current = app
        .sessions
        .sessions
        .iter()
        .find(|profile| profile.id == profile_id);
    if !app.profile_mutation_is_current(profile_id, token) || current != expected {
        anyhow::bail!("session mutation was superseded");
    }
    Ok(())
}

pub(in crate::app) fn finish_profile_mutation(
    state: &Arc<Mutex<AppState>>,
    profile_id: Uuid,
    token: Uuid,
) {
    if let Ok(mut app) = state.lock() {
        app.finish_profile_mutation(profile_id, token);
    }
}

pub(super) fn commit_profile_save(
    state: &Arc<Mutex<AppState>>,
    profile: &SessionProfile,
    expected: Option<&SessionProfile>,
    token: Uuid,
) -> Result<()> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    if !app.profile_mutation_is_current(profile.id, token) {
        anyhow::bail!("session save was superseded");
    }
    let result = (|| {
        let current = app
            .sessions
            .sessions
            .iter()
            .find(|candidate| candidate.id == profile.id);
        if current != expected {
            anyhow::bail!("session changed while the save was running");
        }
        let mut candidate = app.sessions.clone();
        candidate.upsert(profile.clone());
        app.config.save(&candidate)?;
        app.sessions = candidate;
        Ok(())
    })();
    app.finish_profile_mutation(profile.id, token);
    result
}

pub(in crate::app) fn commit_profile_credential_storage(
    state: &Arc<Mutex<AppState>>,
    expected: &SessionProfile,
    storage: CredentialStorage,
    token: Uuid,
) -> Result<()> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    if !app.profile_mutation_is_current(expected.id, token) {
        anyhow::bail!("credential storage update was superseded");
    }
    let result = (|| {
        let current = app
            .sessions
            .sessions
            .iter()
            .find(|profile| profile.id == expected.id);
        if current != Some(expected) {
            anyhow::bail!("session changed while saving credential storage");
        }
        let mut candidate = app.sessions.clone();
        let ssh = candidate
            .sessions
            .iter_mut()
            .find(|profile| profile.id == expected.id)
            .and_then(SessionProfile::ssh_mut)
            .context("credential storage requires an SSH profile")?;
        if !matches!(ssh.auth, AuthMethod::Password) {
            anyhow::bail!("non-password profiles cannot store password credentials");
        }
        if ssh.credential_storage != Some(storage) {
            ssh.credential_storage = Some(storage);
            app.config.save(&candidate)?;
        }
        app.sessions = candidate;
        Ok(())
    })();
    app.finish_profile_mutation(expected.id, token);
    result
}

pub(super) fn commit_profile_delete(
    state: &Arc<Mutex<AppState>>,
    expected: &SessionProfile,
    token: Uuid,
) -> Result<()> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    if !app.profile_mutation_is_current(expected.id, token) {
        anyhow::bail!("session delete was superseded");
    }
    let result = (|| {
        let current = app
            .sessions
            .sessions
            .iter()
            .find(|profile| profile.id == expected.id);
        if current != Some(expected) {
            anyhow::bail!("session changed while the delete was running");
        }
        let mut candidate = app.sessions.clone();
        if !candidate.remove(expected.id) {
            anyhow::bail!("session not found");
        }
        app.config.save(&candidate)?;
        app.sessions = candidate;
        if app.active_editor_profile_id() == Some(Some(expected.id))
            && let Some(tab_id) = app.active_tab_id()
        {
            let _ = app.close_tab(tab_id);
        }
        Ok(())
    })();
    app.finish_profile_mutation(expected.id, token);
    result
}

pub(super) enum CredentialChange {
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
pub(super) fn profile_from_editor(
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
        "~",
        "",
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
pub(super) fn profile_from_editor_with_password(
    existing: Option<&SessionProfile>,
    name: &str,
    group_name: &str,
    protocol: &str,
    host: &str,
    port: &str,
    username: &str,
    auth_method: &str,
    private_key_path: &str,
    sftp_remote_path: &str,
    sftp_local_path: &str,
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
    let ssh_protocol = protocol.eq_ignore_ascii_case("SSH");
    let password_auth = ssh_protocol && auth_method == "Password";
    let private_key = ssh_protocol && auth_method == "Private key";
    let ssh_agent = ssh_protocol && auth_method == "SSH agent";
    if ssh_protocol && !password_auth && !private_key && !ssh_agent {
        anyhow::bail!("unsupported SSH authentication method");
    }
    let existing_storage = existing
        .and_then(SessionProfile::ssh)
        .and_then(|ssh| ssh.credential_storage);
    let password_storage =
        (password_auth && !password.is_empty() && remember_password).then(|| {
            credential_storage_for_save(
                CredentialStorage::from_setting(credential_storage),
                !vault_password.is_empty(),
            )
        });
    let credential_change = if let Some(storage) = password_storage {
        CredentialChange::Store {
            storage,
            previous_storage: existing_storage,
            password: zeroize::Zeroizing::new(password.to_owned()),
            vault_password: (storage == CredentialStorage::EncryptedVault)
                .then(|| zeroize::Zeroizing::new(vault_password.to_owned())),
        }
    } else if !ssh_protocol || !password_auth {
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
        ssh.auth = match auth_method {
            "Password" => AuthMethod::Password,
            "Private key" => AuthMethod::PrivateKey {
                path: PathBuf::from(private_key_path.trim()),
            },
            "SSH agent" => AuthMethod::SshAgent,
            _ => anyhow::bail!("unsupported SSH authentication method"),
        };
        ssh.credential_storage = if password_auth {
            password_storage.or(existing_storage)
        } else {
            None
        };
        ssh.host_key_fingerprint = preserved_fingerprint;
        ssh.x11_forwarding = x11_forwarding;
        ssh.sftp_remote_path = sftp_remote_path.trim().to_owned();
        ssh.sftp_local_path = sftp_local_path.trim().to_owned();
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
    profile.validate()?;
    Ok((profile, credential_change, connection_password))
}

pub(super) fn should_connect_after_save(
    connect_after_save: bool,
    profile: &SessionProfile,
) -> bool {
    connect_after_save && profile.ssh().is_some()
}

fn parse_network_port(value: &str) -> Result<u16> {
    match value.trim().parse::<u16>() {
        Ok(port) if port > 0 => Ok(port),
        _ => anyhow::bail!("port must be a number between 1 and 65535"),
    }
}
