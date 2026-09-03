use super::*;
use ax_ssh::config::{AuthMethod, CredentialStorage};

pub(in crate::app) fn prepare_authentication_retry(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    session_id: Uuid,
    attempt_id: Uuid,
) -> Result<bool> {
    prepare_retry_with_phase(
        state,
        tab_id,
        session_id,
        attempt_id,
        SshConnectionPhase::AwaitingAuthentication {
            vault_unlock_only: false,
        },
    )
}

pub(in crate::app) fn prepare_stored_credential_retry(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    session_id: Uuid,
    attempt_id: Uuid,
) -> Result<bool> {
    prepare_retry_with_phase(
        state,
        tab_id,
        session_id,
        attempt_id,
        SshConnectionPhase::LoadingStoredCredential,
    )
}

pub(in crate::app) fn finish_stored_credential_retry(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    session_id: Uuid,
) -> bool {
    let Ok(mut app) = state.lock() else {
        return false;
    };
    let current = app.terminal(tab_id).is_some_and(|terminal| {
        terminal
            .ssh_route()
            .is_some_and(|route| route.0 == session_id)
            && matches!(
                terminal.ssh_phase(),
                Some(SshConnectionPhase::LoadingStoredCredential)
            )
    });
    if !current {
        return false;
    }
    app.terminal_mut(tab_id).is_some_and(|terminal| {
        terminal.set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
            vault_unlock_only: false,
        })
    })
}

fn prepare_retry_with_phase(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    session_id: Uuid,
    attempt_id: Uuid,
    phase: SshConnectionPhase,
) -> Result<bool> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    if !matches_attempt(&app, tab_id, session_id, attempt_id) {
        return Ok(false);
    }
    if let Some(terminal) = app.terminal_mut(tab_id) {
        let generation = terminal.reconnect_generation();
        terminal.finish_reconnect_attempt(generation);
        terminal.worker = None;
        terminal.set_ssh_attempt(None);
        terminal.connected = false;
        terminal.worker_running = false;
        terminal.set_ssh_phase(phase);
    }

    Ok(true)
}

pub(in crate::app) fn prepare_host_key_retry(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    session_id: Uuid,
    attempt_id: Uuid,
    prompt: PendingHostKey,
) -> Result<bool> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    if !matches_attempt(&app, tab_id, session_id, attempt_id) {
        return Ok(false);
    }
    if let Some(terminal) = app.terminal_mut(tab_id) {
        let generation = terminal.reconnect_generation();
        terminal.finish_reconnect_attempt(generation);
        terminal.worker = None;
        terminal.set_ssh_attempt(None);
        terminal.connected = false;
        terminal.worker_running = false;
        terminal.set_ssh_phase(SshConnectionPhase::AwaitingHostKey(prompt));
    }
    Ok(true)
}

pub(in crate::app) fn retire_session_attempt(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    session_id: Uuid,
    attempt_id: Uuid,
) -> bool {
    match state.lock() {
        Ok(mut app) if matches_attempt(&app, tab_id, session_id, attempt_id) => {
            if let Some(terminal) = app.terminal_mut(tab_id) {
                let generation = terminal.reconnect_generation();
                terminal.finish_reconnect_attempt(generation);
                terminal.worker = None;
                terminal.set_ssh_attempt(None);
                terminal.connected = false;
                terminal.worker_running = false;
                terminal.sftp.reset();
            }
            true
        }
        Ok(_) => false,
        Err(_) => {
            error!(
                tab_id = %tab_id,
                session_id = %session_id,
                %attempt_id,
                "state lock poisoned while retiring SSH worker"
            );
            false
        }
    }
}

pub(in crate::app) fn set_credential_storage_while_loading(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    session_id: Uuid,
    credential_storage: Option<CredentialStorage>,
    expected_profile: Option<&SessionProfile>,
) -> Result<bool> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let current = app.terminal(tab_id).is_some_and(|terminal| {
        terminal
            .ssh_route()
            .is_some_and(|route| route.0 == session_id)
            && matches!(
                terminal.ssh_phase(),
                Some(SshConnectionPhase::LoadingStoredCredential)
            )
    });
    if !current {
        return Ok(false);
    }

    let mut candidate = app.sessions.clone();
    let profile = candidate
        .sessions
        .iter_mut()
        .find(|profile| profile.id == session_id)
        .context("session not found while updating credential storage")?;
    if expected_profile.is_some_and(|expected| profile != expected) {
        return Ok(false);
    }
    let ssh = profile
        .ssh_mut()
        .context("only SSH profiles can update credential storage")?;
    if ssh.credential_storage == credential_storage {
        return Ok(true);
    }
    if credential_storage.is_some() && !matches!(ssh.auth, AuthMethod::Password) {
        anyhow::bail!("non-password profiles cannot store password credentials");
    }
    ssh.credential_storage = credential_storage;
    if credential_storage != Some(CredentialStorage::EncryptedVault) {
        ssh.credential_vault_key_in_keyring = false;
    }
    app.config.save(&candidate)?;
    app.sessions = candidate;
    Ok(true)
}

pub(in crate::app) fn session_attempt_is_active(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    session_id: Uuid,
    attempt_id: Uuid,
) -> bool {
    state
        .lock()
        .is_ok_and(|app| matches_attempt(&app, tab_id, session_id, attempt_id))
}

fn matches_attempt(app: &AppState, tab_id: Uuid, session_id: Uuid, attempt_id: Uuid) -> bool {
    app.terminal(tab_id)
        .and_then(TerminalTabState::ssh_route)
        .is_some_and(|route| route == (session_id, Some(attempt_id)))
}
