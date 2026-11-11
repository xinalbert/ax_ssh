use super::*;
use crate::app::credential_tasks::{credential_storage_for_save, vault_password_for_save};

async fn clear_missing_auto_vault_credential(
    state: &Arc<Mutex<AppState>>,
    lookup: &CredentialLookup,
) -> bool {
    let tab_id = lookup.tab_id;
    let profile = &lookup.profile;
    let persistence = match state.lock() {
        Ok(app) => app.persistence_coordinator.clone(),
        Err(_) => return false,
    };
    let _persistence_guard = persistence.gate.lock().await;
    if !state.lock().is_ok_and(|app| lookup.is_current(&app)) {
        return false;
    }
    let rollback = match delete_password(profile.id, CredentialStorage::EncryptedVault, true).await
    {
        Ok(rollback) => rollback,
        Err(error) => {
            warn!(session_id = %profile.id, %error, "failed to remove unusable automatic vault credential");
            return false;
        }
    };
    match set_credential_storage_while_loading(
        state,
        tab_id,
        profile.id,
        None,
        Some(profile),
        lookup.lookup_id,
    ) {
        Ok(true) => return true,
        Ok(false) => {
            if let Err(error) = rollback.restore().await {
                warn!(session_id = %profile.id, %error, "failed to restore stale automatic vault credential");
            }
        }
        Err(error) => {
            if let Err(restore_error) = rollback.restore().await {
                warn!(session_id = %profile.id, %restore_error, "failed to restore automatic vault credential after profile save failure");
            }
            warn!(
                session_id = %profile.id,
                %error,
                    "failed to clear unusable automatic vault credential reference"
            );
        }
    }
    false
}

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
    if storage == CredentialStorage::EncryptedVault && !ssh.credential_vault_key_saved {
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
    let Some(lookup_id) = set_loading_stored_credential(&state, tab_id, profile.id) else {
        return;
    };
    let lookup = CredentialLookup {
        tab_id,
        profile,
        target,
        lookup_id,
        vault_unlock_only: false,
    };
    set_tab_status(
        &state,
        &ui,
        tab_id,
        "Reading saved password; complete any system authorization prompt...",
    );
    refresh_workspace(&ui, &state);
    let runtime_for_lookup = runtime.clone();
    runtime.spawn(async move {
        let result = if storage == CredentialStorage::EncryptedVault {
            match load_vault_unlock_password(lookup.profile.id).await {
                Ok(Some(unlock)) => {
                    if !state.lock().is_ok_and(|app| lookup.is_current(&app)) {
                        return;
                    }
                    load_vault_password(lookup.profile.id, unlock).await
                }
                Ok(None) => Ok(None),
                Err(error) => Err(error),
            }
        } else {
            load_system_password(lookup.profile.id).await
        };
        lookup
            .complete(&runtime_for_lookup, state, ui, result)
            .await;
    });
}

/// The UI may disconnect, close, edit or retry while a native prompt is open.
/// Every completion belongs to this lookup, never just to a matching phase.
struct CredentialLookup {
    tab_id: Uuid,
    profile: SessionProfile,
    target: ConnectionTarget,
    lookup_id: Uuid,
    vault_unlock_only: bool,
}

impl CredentialLookup {
    fn is_current(&self, app: &AppState) -> bool {
        app.sessions
            .sessions
            .iter()
            .any(|profile| profile == &self.profile)
            && terminal_has_phase(
                app,
                self.tab_id,
                self.profile.id,
                |phase| matches!(phase, SshConnectionPhase::LoadingStoredCredential(id) if *id == self.lookup_id),
            )
    }

    fn finish(
        &self,
        state: &Arc<Mutex<AppState>>,
        phase: SshConnectionPhase,
        status: String,
    ) -> bool {
        let Ok(mut app) = state.lock() else {
            return false;
        };
        if !self.is_current(&app) {
            return false;
        }
        let Some(terminal) = app.terminal_mut(self.tab_id) else {
            return false;
        };
        terminal.set_ssh_phase(phase);
        terminal.status = status;
        true
    }

    async fn complete(
        mut self,
        runtime: &Handle,
        state: Arc<Mutex<AppState>>,
        ui: slint::Weak<AppWindow>,
        result: Result<Option<zeroize::Zeroizing<String>>>,
    ) {
        if !state.lock().is_ok_and(|app| self.is_current(&app)) {
            return;
        }
        match result {
            Ok(Some(secret)) => {
                if let Err(error) = start_session_worker(
                    runtime,
                    state.clone(),
                    ui.clone(),
                    self.tab_id,
                    self.profile.clone(),
                    secret,
                    None,
                    true,
                    AuthenticationStart::StoredCredential(self.lookup_id),
                    self.target,
                ) && self.finish(
                    &state,
                    SshConnectionPhase::StoredCredentialUnavailable {
                        vault_unlock_only: self.vault_unlock_only,
                    },
                    format!("Cannot start connection: {error}"),
                ) {
                    refresh_workspace(&ui, &state);
                }
            }
            Ok(None) => {
                let cleared = if self
                    .profile
                    .ssh()
                    .is_some_and(|ssh| ssh.credential_vault_key_saved)
                {
                    clear_missing_auto_vault_credential(&state, &self).await
                } else {
                    let persistence = match state.lock() {
                        Ok(app) => app.persistence_coordinator.clone(),
                        Err(_) => return,
                    };
                    let _guard = persistence.gate.lock().await;
                    match set_credential_storage_while_loading(
                        &state,
                        self.tab_id,
                        self.profile.id,
                        None,
                        Some(&self.profile),
                        self.lookup_id,
                    ) {
                        Ok(true) => true,
                        Ok(false) => return,
                        Err(error) => {
                            warn!(session_id = %self.profile.id, %error, "failed to clear missing credential reference");
                            false
                        }
                    }
                };
                if cleared && let Some(ssh) = self.profile.ssh_mut() {
                    ssh.credential_storage = None;
                    ssh.credential_vault_key_saved = false;
                }
                if self.finish(
                    &state,
                    SshConnectionPhase::AwaitingAuthentication {
                        vault_unlock_only: false,
                    },
                    "Saved password was not found; enter the server SSH password".into(),
                ) {
                    refresh_workspace(&ui, &state);
                }
            }
            Err(error) => {
                warn!(session_id = %self.profile.id, %error, "saved credential lookup failed");
                if self.finish(&state, SshConnectionPhase::StoredCredentialUnavailable {
                    vault_unlock_only: self.vault_unlock_only,
                }, format!("Saved password unavailable; retry access or enter the server SSH password: {error:#}")) {
                    refresh_workspace(&ui, &state);
                }
            }
        }
    }
}

pub(in crate::app) fn wire_authentication(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let ui_for_choice = ui.as_weak();
    let state_for_choice = state.clone();
    let runtime_for_choice = runtime.clone();
    let router_for_choice = window_router.clone();
    ui.on_resolve_credential_access(move |retry| {
        sync_window_active(&router_for_choice, window_id, &state_for_choice);
        let pending = state_for_choice.lock().ok().and_then(|mut app| {
            let tab_id = app.active_tab_id()?;
            let terminal = app.terminal(tab_id)?;
            let (profile_id, _) = terminal.ssh_route()?;
            if !matches!(
                terminal.ssh_phase(),
                Some(SshConnectionPhase::StoredCredentialUnavailable { .. })
            ) {
                return None;
            }
            let target = terminal.connection_target();
            let profile = app
                .sessions
                .sessions
                .iter()
                .find(|profile| profile.id == profile_id)?
                .clone();
            app.terminal_mut(tab_id)?
                .set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
                    vault_unlock_only: false,
                });
            Some((tab_id, profile, target))
        });
        let Some((tab_id, profile, target)) = pending else {
            return;
        };
        if retry {
            begin_authentication(
                &runtime_for_choice,
                state_for_choice.clone(),
                ui_for_choice.clone(),
                tab_id,
                profile,
                target,
            );
        } else {
            set_tab_status(
                &state_for_choice,
                &ui_for_choice,
                tab_id,
                "Enter the server SSH password",
            );
            refresh_workspace(&ui_for_choice, &state_for_choice);
        }
    });
    let ui_for_auth = ui.as_weak();
    let state_for_auth = state.clone();
    let runtime_for_auth = runtime.clone();
    let router_for_auth = window_router.clone();
    ui.on_authenticate_session(
        move |password, vault_password, remember_password, storage| {
            log_ui_action("authentication.submit");
            sync_window_active(&router_for_auth, window_id, &state_for_auth);
            let password = zeroize::Zeroizing::new(password.as_str().to_owned());
            let vault_password = zeroize::Zeroizing::new(vault_password.as_str().to_owned());
            let target = match state_for_auth.lock() {
                Ok(app) => app.active_tab_id().and_then(|tab_id| {
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
                        | SshConnectionPhase::LoadingStoredCredential(_)
                        | SshConnectionPhase::StoredCredentialUnavailable { .. } => return None,
                    };
                    let profile = app
                        .sessions
                        .sessions
                        .iter()
                        .find(|profile| profile.id == profile_id)
                        .cloned()?;
                    Some((
                        tab_id,
                        profile,
                        vault_unlock_only,
                        terminal.connection_target(),
                    ))
                }),
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
                    set_status(
                        &ui_for_auth,
                        "Enter the vault password to unlock the saved SSH password",
                    );
                    return false;
                }
                let Some(lookup_id) =
                    set_loading_stored_credential(&state_for_auth, tab_id, profile.id)
                else {
                    set_status(&ui_for_auth, "Authentication request is no longer current");
                    return false;
                };
                refresh_workspace(&ui_for_auth, &state_for_auth);
                let state = state_for_auth.clone();
                let ui = ui_for_auth.clone();
                let runtime = runtime_for_auth.clone();
                let lookup = CredentialLookup {
                    tab_id,
                    profile,
                    target: connection_target,
                    lookup_id,
                    vault_unlock_only: true,
                };
                runtime_for_auth.spawn(async move {
                    let result = load_vault_password(lookup.profile.id, vault_password).await;
                    lookup.complete(&runtime, state, ui, result).await;
                });
                return true;
            }
            if password_auth && password.is_empty() {
                set_status(&ui_for_auth, "Password cannot be empty");
                return false;
            }
            let credential_to_store = if password_auth {
                let storage = selected_storage_for_remember(remember_password, storage.as_str());
                storage.map(|storage| {
                    let (vault_password, vault_password_generated) =
                        vault_password_for_save(storage, vault_password.as_str());
                    PendingCredentialStore {
                        expected_profile: profile.clone(),
                        storage,
                        previous_storage,
                        vault_password: (storage == CredentialStorage::EncryptedVault)
                            .then_some(vault_password),
                        vault_password_generated,
                        previous_vault_password_generated: ssh.credential_vault_key_saved,
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
        },
    );

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
                        Some(
                            SshConnectionPhase::AwaitingAuthentication { .. }
                                | SshConnectionPhase::StoredCredentialUnavailable { .. }
                        )
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
                if terminal.worker.is_none()
                    && terminal
                        .ssh_phase()
                        .is_some_and(|phase| !matches!(phase, SshConnectionPhase::Idle))
                {
                    if let Some(probe) = terminal.take_pending_probe()
                        && probe.cancel.send(()).is_err()
                    {
                        debug!("host-key probe already stopped while disconnecting");
                    }
                    terminal.set_ssh_phase(SshConnectionPhase::Idle);
                    terminal.status = "Disconnected".into();
                    return Ok(());
                }
                terminal
                    .worker
                    .as_ref()
                    .context("active terminal has no worker")?
                    .request_disconnect()
            });
        match result {
            Ok(()) => {
                set_status(&ui_for_disconnect, "Disconnect requested");
                refresh_workspace(&ui_for_disconnect, &state_for_disconnect);
            }
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
) -> Option<CredentialStorage> {
    if !remember_password {
        return None;
    }
    Some(credential_storage_for_save(
        CredentialStorage::from_setting(setting),
        true,
    ))
}

#[cfg(test)]
mod tests;
