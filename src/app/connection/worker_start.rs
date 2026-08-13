use super::*;

#[derive(Clone, Copy)]
pub(super) enum AuthenticationStart {
    Prompt,
    StoredCredential,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn start_session_worker(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    profile_id: Uuid,
    secret: zeroize::Zeroizing<String>,
    credential_to_store: Option<PendingCredentialStore>,
    used_stored_credential: bool,
    source: AuthenticationStart,
    target: ConnectionTarget,
) -> Result<()> {
    let attempt_id = Uuid::new_v4();
    let (profile, events, credential_to_store) = {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        if !terminal_has_phase(&app, tab_id, profile_id, |phase| match source {
            AuthenticationStart::Prompt => {
                matches!(phase, SshConnectionPhase::AwaitingAuthentication { .. })
            }
            AuthenticationStart::StoredCredential => {
                matches!(phase, SshConnectionPhase::LoadingStoredCredential)
            }
        }) {
            anyhow::bail!("terminal tab is not awaiting authentication");
        }
        let mut profile = app
            .sessions
            .sessions
            .iter()
            .find(|profile| profile.id == profile_id)
            .cloned()
            .context("session not found")?;
        if target == ConnectionTarget::Sftp {
            let initial_path = app
                .terminal_mut(tab_id)
                .and_then(|terminal| terminal.sftp_initial_path.take());
            if let Some(initial_path) = initial_path
                && let Some(ssh) = profile.ssh_mut()
            {
                ssh.sftp_remote_path = initial_path;
            }
        }
        if app.terminal(tab_id).is_none_or(|terminal| {
            terminal.worker.is_some() || terminal.connection_target() != target
        }) {
            anyhow::bail!("terminal tab is missing or already has a worker");
        }
        let columns = u32::from(app.sessions.settings.terminal.default_columns);
        let rows = u32::from(app.sessions.settings.terminal.default_rows);
        let x11_settings = app.sessions.settings.x11.clone();
        let (worker, events) = match target {
            ConnectionTarget::Terminal => SshSessionHandle::spawn_with_x11_settings(
                runtime,
                tab_id,
                profile.clone(),
                secret,
                columns,
                rows,
                x11_settings,
            ),
            ConnectionTarget::Sftp => {
                SshSessionHandle::spawn_sftp(runtime, tab_id, profile.clone(), secret)
            }
        };
        let terminal = app
            .terminal_mut(tab_id)
            .context("terminal tab disappeared while starting worker")?;
        if !terminal.set_ssh_attempt(Some(attempt_id)) {
            anyhow::bail!("terminal tab is not an SSH terminal");
        }
        terminal.worker = Some(TerminalWorker::Ssh(worker));
        terminal.enable_reconnect();
        terminal.worker_running = true;
        terminal.connected = false;
        terminal.status = format!("Connecting to {}...", profile_endpoint(&profile));
        terminal.set_ssh_phase(SshConnectionPhase::Idle);
        (profile, events, credential_to_store)
    };

    refresh_workspace(&ui, &state);
    spawn_session_monitor(
        runtime,
        state,
        ui,
        tab_id,
        profile,
        attempt_id,
        events,
        credential_to_store,
        used_stored_credential,
        target,
    );
    Ok(())
}

pub(super) fn terminal_has_phase(
    app: &AppState,
    tab_id: Uuid,
    profile_id: Uuid,
    predicate: impl FnOnce(&SshConnectionPhase) -> bool,
) -> bool {
    app.terminal(tab_id).is_some_and(|terminal| {
        terminal
            .ssh_route()
            .is_some_and(|route| route.0 == profile_id)
    }) && app
        .terminal(tab_id)
        .and_then(TerminalTabState::ssh_phase)
        .is_some_and(predicate)
}

pub(super) fn set_loading_stored_credential(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    profile_id: Uuid,
) -> bool {
    let Ok(mut app) = state.lock() else {
        return false;
    };
    if !terminal_has_phase(&app, tab_id, profile_id, |phase| {
        matches!(phase, SshConnectionPhase::AwaitingAuthentication { .. })
    }) {
        return false;
    }
    app.terminal_mut(tab_id)
        .is_some_and(|terminal| terminal.set_ssh_phase(SshConnectionPhase::LoadingStoredCredential))
}

pub(super) fn set_awaiting_authentication(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    profile_id: Uuid,
    vault_unlock_only: bool,
) -> bool {
    let Ok(mut app) = state.lock() else {
        return false;
    };
    if !terminal_has_phase(&app, tab_id, profile_id, |phase| {
        matches!(
            phase,
            SshConnectionPhase::AwaitingAuthentication { .. }
                | SshConnectionPhase::LoadingStoredCredential
        )
    }) {
        return false;
    }
    app.terminal_mut(tab_id).is_some_and(|terminal| {
        terminal.set_ssh_phase(SshConnectionPhase::AwaitingAuthentication { vault_unlock_only })
    })
}
