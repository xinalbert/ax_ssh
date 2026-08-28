use super::*;

async fn confirm_host_key(
    state: &Arc<Mutex<AppState>>,
    pending: &PendingHostKey,
    expected_profile: &SessionProfile,
    mutation_token: Uuid,
) -> Result<Option<(Uuid, SessionProfile, ConnectionTarget)>> {
    ensure_profile_mutation_current(
        state,
        pending.profile_id,
        mutation_token,
        Some(expected_profile),
    )?;
    let public_key = pending
        .public_key
        .clone()
        .context("confirmed host key has no public key")?;
    let host = pending.host.clone();
    let port = pending.port;
    let changed = pending.changed;
    tokio::task::spawn_blocking(move || {
        if changed {
            ax_ssh::ssh::replace_confirmed_known_host(&host, port, &public_key)
        } else {
            ax_ssh::ssh::append_confirmed_known_host(&host, port, &public_key)
        }
    })
    .await
    .context("host-key file task failed")??;

    ensure_profile_mutation_current(
        state,
        pending.profile_id,
        mutation_token,
        Some(expected_profile),
    )?;
    let (config, candidate, trusted_profile) = {
        let app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        let profile = app
            .sessions
            .sessions
            .iter()
            .find(|profile| profile.id == pending.profile_id)
            .cloned()
            .context("session was removed while confirming host key")?;
        if profile != *expected_profile {
            anyhow::bail!("session changed while confirming host key");
        }
        let Some(ssh) = profile.ssh() else {
            anyhow::bail!("host-key confirmation requires an SSH profile");
        };
        if ssh.host != pending.host || ssh.port != pending.port {
            anyhow::bail!("session endpoint changed while confirming host key");
        }
        let mut candidate = app.sessions.clone();
        if let Some(ssh) = candidate
            .sessions
            .iter_mut()
            .find(|profile| profile.id == pending.profile_id)
            .and_then(SessionProfile::ssh_mut)
        {
            ssh.host_key_fingerprint = Some(pending.fingerprint.clone());
        }
        let trusted_profile = candidate
            .sessions
            .iter()
            .find(|profile| profile.id == pending.profile_id)
            .cloned()
            .context("session was removed while confirming host key")?;
        (app.config.clone(), candidate, trusted_profile)
    };
    let candidate_for_save = candidate.clone();
    tokio::task::spawn_blocking(move || config.save(&candidate_for_save))
        .await
        .context("profile save task failed")??;

    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let current = app
        .sessions
        .sessions
        .iter()
        .find(|profile| profile.id == pending.profile_id);
    if !app.profile_mutation_is_current(pending.profile_id, mutation_token)
        || current != Some(expected_profile)
    {
        anyhow::bail!("session changed while confirming host key");
    }
    app.sessions = candidate;
    app.finish_profile_mutation(pending.profile_id, mutation_token);
    let Some(terminal) = app.terminal_mut(pending.tab_id) else {
        return Ok(None);
    };
    if matches!(
        terminal.ssh_phase(),
        Some(SshConnectionPhase::ConfirmingHostKey(current))
            if current.profile_id == pending.profile_id
                && current.fingerprint == pending.fingerprint
    ) {
        terminal.set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
            vault_unlock_only: false,
        });
        return Ok(Some((
            pending.tab_id,
            trusted_profile,
            terminal.connection_target(),
        )));
    }
    Ok(None)
}

async fn confirm_revoked_host_key(
    state: &Arc<Mutex<AppState>>,
    pending: &PendingHostKey,
    expected_profile: &SessionProfile,
    mutation_token: Uuid,
) -> Result<Option<(Uuid, SessionProfile, ConnectionTarget)>> {
    // Clear the profile pin before removing @revoked. If removal fails, the
    // revoked record remains authoritative and cannot be bypassed.
    ensure_profile_mutation_current(
        state,
        pending.profile_id,
        mutation_token,
        Some(expected_profile),
    )?;
    let (config, candidate) = {
        let app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        if app
            .sessions
            .sessions
            .iter()
            .find(|profile| profile.id == pending.profile_id)
            != Some(expected_profile)
        {
            anyhow::bail!("session changed while confirming revoked host key");
        }
        let mut candidate = app.sessions.clone();
        let profile = candidate
            .sessions
            .iter_mut()
            .find(|profile| profile.id == pending.profile_id)
            .context("session was removed while confirming revoked host key")?;
        let Some(ssh) = profile.ssh_mut() else {
            anyhow::bail!("host-key confirmation requires an SSH profile");
        };
        if ssh.host != pending.host || ssh.port != pending.port {
            anyhow::bail!("session endpoint changed while confirming revoked host key");
        }
        ssh.host_key_fingerprint = None;
        (app.config.clone(), candidate)
    };
    let candidate_for_save = candidate.clone();
    tokio::task::spawn_blocking(move || config.save(&candidate_for_save))
        .await
        .context("profile save task failed")??;

    // Commit the cleared pin before touching the revoked record. A failure in
    // the second operation therefore leaves both in-memory and persisted
    // profile state unable to bypass the still-revoked key.
    {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        let current = app
            .sessions
            .sessions
            .iter()
            .find(|profile| profile.id == pending.profile_id);
        if !app.profile_mutation_is_current(pending.profile_id, mutation_token)
            || current != Some(expected_profile)
        {
            anyhow::bail!("session changed while confirming revoked host key");
        }
        app.sessions = candidate;
        app.finish_profile_mutation(pending.profile_id, mutation_token);
    }

    let host = pending.host.clone();
    let port = pending.port;
    let fingerprint = pending.fingerprint.clone();
    let removed = tokio::task::spawn_blocking(move || {
        ax_ssh::ssh::remove_known_host(&host, port, &fingerprint)
    })
    .await
    .context("host-key file task failed")??;

    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let Some(terminal) = app.terminal_mut(pending.tab_id) else {
        return Ok(None);
    };
    if !matches!(
        terminal.ssh_phase(),
        Some(SshConnectionPhase::ConfirmingHostKey(current))
            if current.profile_id == pending.profile_id
                && current.fingerprint == pending.fingerprint
    ) {
        return Ok(None);
    }
    terminal.set_ssh_phase(SshConnectionPhase::Idle);
    terminal.status = if removed {
        "Revoked host-key record removed; verify the replacement key before reconnecting".to_owned()
    } else {
        "Profile pin cleared; no revoked host-key record was found".to_owned()
    };
    Ok(None)
}

pub(in crate::app) fn wire_host_key_confirmation(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let ui_for_confirm = ui.as_weak();
    let state_for_confirm = state.clone();
    let router_for_confirm = window_router.clone();
    ui.on_confirm_host_key(move || {
        log_ui_action("host-key.confirm");
        sync_window_active(&router_for_confirm, window_id, &state_for_confirm);
        let (pending, expected_profile, mutation_token, coordinator) = {
            let mut app = match state_for_confirm.lock() {
                Ok(app) => app,
                Err(_) => {
                    set_status(&ui_for_confirm, "Cannot update session state");
                    return;
                }
            };
            let Some(active_tab_id) = app.active_tab_id() else {
                set_status(&ui_for_confirm, "No host key is awaiting confirmation");
                return;
            };
            let Some(pending) = app.terminal(active_tab_id).and_then(|terminal| {
                match terminal.ssh_phase() {
                    Some(SshConnectionPhase::AwaitingHostKey(prompt))
                        if prompt.tab_id == active_tab_id
                            && terminal
                                .ssh_route()
                                .is_some_and(|route| route.0 == prompt.profile_id) =>
                    {
                        Some(prompt.clone())
                    }
                    _ => None,
                }
            }) else {
                set_status(&ui_for_confirm, "No host key is awaiting confirmation");
                return;
            };
            let Some(expected_profile) = app
                .sessions
                .sessions
                .iter()
                .find(|profile| profile.id == pending.profile_id)
                .cloned()
            else {
                set_status(&ui_for_confirm, "Session endpoint or tab changed; check the host key again");
                return;
            };
            let valid_endpoint = expected_profile
                .ssh()
                .is_some_and(|ssh| ssh.host == pending.host && ssh.port == pending.port);
            if !valid_endpoint {
                set_status(&ui_for_confirm, "Session endpoint or tab changed; check the host key again");
                return;
            }
            if pending.changed && pending.public_key.is_none() {
                set_status(&ui_for_confirm, "Changed host key public data is unavailable; retry the probe");
                return;
            }
            if app.profile_mutation_is_pending(pending.profile_id) {
                set_status(&ui_for_confirm, "Session is already being modified");
                return;
            }
            let mutation_token = app.begin_profile_mutation(pending.profile_id);
            if let Some(terminal) = app.terminal_mut(pending.tab_id) {
                terminal.set_ssh_phase(SshConnectionPhase::ConfirmingHostKey(pending.clone()));
                terminal.status = if pending.revoked {
                    "Removing revoked host-key record...".to_owned()
                } else {
                    "Saving host-key trust...".to_owned()
                };
            }
            (
                pending,
                expected_profile,
                mutation_token,
                app.persistence_coordinator.clone(),
            )
        };
        let state = state_for_confirm.clone();
        let ui = ui_for_confirm.clone();
        let runtime_for_confirm = runtime.clone();
        runtime.spawn(async move {
            let _gate = coordinator.gate.lock().await;
            let result = if pending.revoked {
                confirm_revoked_host_key(&state, &pending, &expected_profile, mutation_token).await
            } else {
                confirm_host_key(&state, &pending, &expected_profile, mutation_token).await
            };
            match result {
                Ok(Some((tab_id, profile, target))) => {
                    info!(tab_id = %tab_id, session_id = %profile.id, fingerprint = %pending.fingerprint, "SSH host key trusted by user");
                    refresh_workspace(&ui, &state);
                    begin_authentication(&runtime_for_confirm, state, ui, tab_id, profile, target);
                }
                Ok(None) => refresh_workspace(&ui, &state),
                Err(error) => {
                    finish_profile_mutation(&state, pending.profile_id, mutation_token);
                    if let Ok(mut app) = state.lock()
                        && let Some(terminal) = app.terminal_mut(pending.tab_id)
                        && matches!(terminal.ssh_phase(), Some(SshConnectionPhase::ConfirmingHostKey(_)))
                    {
                        terminal.set_ssh_phase(SshConnectionPhase::AwaitingHostKey(pending.clone()));
                    }
                    set_tab_status(&state, &ui, pending.tab_id, &format!("Host-key update failed: {error}"));
                    refresh_workspace(&ui, &state);
                }
            }
        });
    });

    let ui_for_reject = ui.as_weak();
    let state_for_reject = state.clone();
    let router_for_reject = window_router;
    ui.on_reject_host_key(move || {
        log_ui_action("host-key.reject");
        sync_window_active(&router_for_reject, window_id, &state_for_reject);
        let pending = match state_for_reject.lock() {
            Ok(mut app) => {
                let active_tab_id = app.active_tab_id();
                let pending = active_tab_id
                    .and_then(|tab_id| app.terminal(tab_id).map(|terminal| (tab_id, terminal)))
                    .and_then(|(tab_id, terminal)| match terminal.ssh_phase() {
                        Some(SshConnectionPhase::AwaitingHostKey(prompt))
                            if prompt.tab_id == tab_id =>
                        {
                            Some(prompt.clone())
                        }
                        Some(SshConnectionPhase::Idle)
                        | Some(SshConnectionPhase::Probing(_))
                        | Some(SshConnectionPhase::AwaitingAuthentication { .. })
                        | Some(SshConnectionPhase::LoadingStoredCredential)
                        | Some(SshConnectionPhase::AwaitingHostKey(_))
                        | Some(SshConnectionPhase::ConfirmingHostKey(_))
                        | None => None,
                    });
                if let Some(pending) = &pending
                    && let Some(terminal) = app.terminal_mut(pending.tab_id)
                {
                    terminal.set_ssh_phase(SshConnectionPhase::Idle);
                }
                pending
            }
            Err(_) => {
                set_status(&ui_for_reject, "Cannot update session state");
                return;
            }
        };
        if let Some(pending) = pending {
            set_tab_status(
                &state_for_reject,
                &ui_for_reject,
                pending.tab_id,
                "Connection cancelled; host key was not trusted",
            );
            refresh_workspace(&ui_for_reject, &state_for_reject);
        }
    });
}
