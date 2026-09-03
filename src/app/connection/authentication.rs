use super::*;
use crate::app::credential_tasks::credential_storage_for_save;

pub(in crate::app) fn begin_authentication(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    profile: SessionProfile,
    target: ConnectionTarget,
) {
    let Some(ssh) = profile.ssh() else {
        set_tab_status(
            &state,
            &ui,
            tab_id,
            "SSH authentication requires an SSH profile",
        );
        return;
    };
    let credential_storage = ssh.credential_storage;
    let direct_authentication = match ssh.auth {
        AuthMethod::Password => None,
        AuthMethod::PrivateKey { .. } => Some((
            "Loading private key...",
            "Cannot start private-key connection",
        )),
        AuthMethod::SshAgent => Some((
            "Connecting with SSH agent...",
            "Cannot start SSH-agent connection",
        )),
    };
    if let Some((status, failure)) = direct_authentication {
        set_tab_status(&state, &ui, tab_id, status);
        if let Err(error) = start_session_worker(
            runtime,
            state.clone(),
            ui.clone(),
            tab_id,
            profile.clone(),
            zeroize::Zeroizing::new(String::new()),
            None,
            false,
            AuthenticationStart::Prompt,
            target,
        ) {
            set_awaiting_authentication(&state, tab_id, profile.id, false);
            set_tab_status(&state, &ui, tab_id, &format!("{failure}: {error}"));
            refresh_workspace(&ui, &state);
        }
        return;
    }
    let one_time_password = match state.lock() {
        Ok(mut app) => match app.terminal_mut(tab_id) {
            Some(terminal)
                if terminal
                    .ssh_route()
                    .is_some_and(|route| route.0 == profile.id)
                    && matches!(
                        terminal.ssh_phase(),
                        Some(SshConnectionPhase::AwaitingAuthentication { .. })
                    ) =>
            {
                terminal.take_pending_auth_secret()
            }
            Some(_) | None => None,
        },
        Err(_) => {
            set_tab_status(&state, &ui, tab_id, "Cannot read session state");
            return;
        }
    };
    if let Some(password) = one_time_password {
        set_tab_status(
            &state,
            &ui,
            tab_id,
            "Connecting with the one-time password...",
        );
        if let Err(error) = start_session_worker(
            runtime,
            state.clone(),
            ui.clone(),
            tab_id,
            profile.clone(),
            password,
            None,
            false,
            AuthenticationStart::Prompt,
            target,
        ) {
            set_awaiting_authentication(&state, tab_id, profile.id, false);
            set_tab_status(
                &state,
                &ui,
                tab_id,
                &format!("Cannot start password connection: {error}"),
            );
            refresh_workspace(&ui, &state);
        }
        return;
    }
    let Some(storage) = credential_storage else {
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
                    profile.clone(),
                    secret,
                    None,
                    true,
                    AuthenticationStart::StoredCredential,
                    target,
                ) {
                    set_status(&ui, &format!("Cannot start connection: {error}"));
                }
            }
            Ok(None) => {
                let persistence = match state.lock() {
                    Ok(app) => app.persistence_coordinator.clone(),
                    Err(_) => {
                        set_status(&ui, "Cannot read session state");
                        return;
                    }
                };
                let _persistence_guard = persistence.gate.lock().await;
                match set_credential_storage_while_loading(
                    &state,
                    tab_id,
                    profile.id,
                    None,
                    Some(&profile),
                ) {
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

pub(in crate::app) fn wire_authentication(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let ui_for_auth = ui.as_weak();
    let state_for_auth = state.clone();
    let runtime_for_auth = runtime.clone();
    let router_for_auth = window_router.clone();
    ui.on_authenticate_session(move |password, vault_password, remember_password, storage| {
        log_ui_action("authentication.submit");
        sync_window_active(&router_for_auth, window_id, &state_for_auth);
        let password = zeroize::Zeroizing::new(password.as_str().to_owned());
        let vault_password = zeroize::Zeroizing::new(vault_password.as_str().to_owned());
        let target = match state_for_auth.lock() {
            Ok(app) => {
                app.active_tab_id().and_then(|tab_id| {
                    let terminal = app.terminal(tab_id)?;
                    let (profile_id, _) = terminal.ssh_route()?;
                    let vault_unlock_only = match terminal.ssh_phase()? {
                        SshConnectionPhase::AwaitingAuthentication { vault_unlock_only } => {
                            *vault_unlock_only
                        }
                        SshConnectionPhase::Idle
                        | SshConnectionPhase::Probing(_)
                        | SshConnectionPhase::AwaitingHostKey(_)
                        | SshConnectionPhase::ConfirmingHostKey(_)
                        | SshConnectionPhase::LoadingStoredCredential => return None,
                    };
                    let profile = app
                        .sessions
                        .sessions
                        .iter()
                        .find(|profile| profile.id == profile_id)
                        .cloned()?;
                    Some((tab_id, profile, vault_unlock_only, terminal.connection_target()))
                })
            }
            Err(_) => {
                set_status(&ui_for_auth, "Cannot read session state");
                return false;
            }
        };
        let Some((tab_id, profile, vault_unlock_only, connection_target)) = target else {
            set_status(&ui_for_auth, "No SSH tab is awaiting authentication");
            return false;
        };
        let Some(ssh) = profile.ssh() else {
            set_status(&ui_for_auth, "Authentication request is not an SSH profile");
            return false;
        };
        let password_auth = matches!(ssh.auth, AuthMethod::Password);
        let previous_storage = ssh.credential_storage;
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
                            profile.clone(),
                            secret,
                            None,
                            true,
                            AuthenticationStart::StoredCredential,
                            connection_target,
                        ) {
                            set_status(&ui, &format!("Cannot start connection: {error}"));
                        }
                    }
                    Ok(None) => {
                        let persistence = match state.lock() {
                            Ok(app) => app.persistence_coordinator.clone(),
                            Err(_) => {
                                set_status(&ui, "Cannot read session state");
                                return;
                            }
                        };
                        let _persistence_guard = persistence.gate.lock().await;
                        match set_credential_storage_while_loading(
                            &state,
                            tab_id,
                            profile.id,
                            None,
                            Some(&profile),
                        ) {
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
        let credential_to_store = if password_auth {
            let storage = match selected_storage_for_remember(
                remember_password,
                storage.as_str(),
                vault_password.as_str(),
            ) {
                Ok(storage) => storage,
                Err(error) => {
                    set_status(&ui_for_auth, &format!("Cannot save password: {error}"));
                    return false;
                }
            };
            storage.map(|storage| {
                PendingCredentialStore {
                    expected_profile: profile.clone(),
                    storage,
                    previous_storage,
                    vault_password: (storage == CredentialStorage::EncryptedVault)
                        .then(|| vault_password.clone()),
                    secret: password.clone(),
                }
            })
        } else {
            None
        };
        match start_session_worker(
            &runtime_for_auth,
            state_for_auth.clone(),
            ui_for_auth.clone(),
            tab_id,
            profile.clone(),
            password,
            credential_to_store,
            false,
            AuthenticationStart::Prompt,
            connection_target,
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
    let router_for_cancel = window_router.clone();
    ui.on_cancel_password_dialog(move || {
        log_ui_action("authentication.cancel");
        sync_window_active(&router_for_cancel, window_id, &state_for_cancel);
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
    let router_for_disconnect = window_router;
    let state_for_disconnect = state;
    ui.on_disconnect_session(move || {
        log_ui_action("connection.disconnect");
        sync_window_active(&router_for_disconnect, window_id, &state_for_disconnect);
        let result = state_for_disconnect
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))
            .and_then(|mut app| {
                let terminal = app.active_terminal_mut().context("no active terminal")?;
                terminal.cancel_reconnect();
                terminal
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

fn selected_storage_for_remember(
    remember_password: bool,
    setting: &str,
    vault_password: &str,
) -> anyhow::Result<Option<CredentialStorage>> {
    if !remember_password {
        return Ok(None);
    }
    credential_storage_for_save(
        CredentialStorage::from_setting(setting),
        !vault_password.is_empty(),
    )
    .map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_storage_uses_explicit_selection_only_when_remembering() {
        let storage = selected_storage_for_remember(false, "encrypted-vault", "")
            .expect("disabled password saving should not validate the backend");
        assert_eq!(storage, None);
        let error = selected_storage_for_remember(true, "encrypted-vault", "")
            .expect_err("encrypted vault saves require a vault password");
        assert_eq!(
            error.to_string(),
            "vault password is required for encrypted application vault"
        );
        let storage = selected_storage_for_remember(true, "encrypted-vault", "vault-password")
            .expect("encrypted vault saves should accept a vault password");
        assert_eq!(storage, Some(CredentialStorage::EncryptedVault));
        let storage = selected_storage_for_remember(true, "system-keyring", "")
            .expect("system credential saves should not require a vault password");
        assert_eq!(storage, Some(CredentialStorage::SystemKeyring));
    }
}
