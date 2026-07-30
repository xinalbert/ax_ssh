use super::*;

pub(in crate::app) fn prepare_authentication_retry(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    session_id: Uuid,
    attempt_id: Uuid,
    clear_credential_marker: bool,
) -> Result<bool> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    if !matches_attempt(&app, tab_id, session_id, attempt_id) {
        return Ok(false);
    }
    if let Some(terminal) = app.terminal_mut(tab_id) {
        terminal.worker = None;
        terminal.set_ssh_attempt(None);
        terminal.connected = false;
        terminal.worker_running = false;
    }
    app.pending_auth = Some(PendingAuth {
        tab_id,
        profile_id: session_id,
    });

    if clear_credential_marker {
        let mut candidate = app.sessions.clone();
        let profile = candidate
            .sessions
            .iter_mut()
            .find(|profile| profile.id == session_id)
            .context("session not found while clearing credential marker")?;
        profile.credential_stored = false;
        match app.config.save(&candidate) {
            Ok(()) => app.sessions = candidate,
            Err(error) => warn!(
                session_id = %session_id,
                %error,
                "failed to clear rejected credential marker"
            ),
        }
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
        terminal.worker = None;
        terminal.set_ssh_attempt(None);
        terminal.connected = false;
        terminal.worker_running = false;
    }
    app.pending_trust = Some(prompt);
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
                terminal.worker = None;
                terminal.set_ssh_attempt(None);
                terminal.connected = false;
                terminal.worker_running = false;
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

pub(in crate::app) fn set_credential_marker(
    state: &Arc<Mutex<AppState>>,
    session_id: Uuid,
    stored: bool,
) -> Result<()> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut candidate = app.sessions.clone();
    let profile = candidate
        .sessions
        .iter_mut()
        .find(|profile| profile.id == session_id)
        .context("session not found while updating credential marker")?;
    if profile.credential_stored == stored {
        return Ok(());
    }
    profile.credential_stored = stored;
    app.config.save(&candidate)?;
    app.sessions = candidate;
    Ok(())
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
