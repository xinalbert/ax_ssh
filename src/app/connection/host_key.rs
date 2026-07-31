use super::*;

pub(in crate::app) fn wire_host_key_confirmation(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
) {
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
            let Some(active_tab_id) = app.active_tab_id() else {
                set_status(&ui_for_confirm, "No host key is awaiting confirmation");
                return;
            };
            let Some(pending) = app
                .terminal(active_tab_id)
                .and_then(TerminalTabState::ssh_phase)
                .and_then(|phase| match phase {
                    SshConnectionPhase::AwaitingHostKey(prompt)
                        if app
                            .terminal(active_tab_id)
                            .and_then(TerminalTabState::ssh_route)
                            .is_some_and(|route| {
                                route.0 == prompt.profile_id && prompt.tab_id == active_tab_id
                            }) =>
                    {
                        Some(prompt.clone())
                    }
                    SshConnectionPhase::AwaitingHostKey(_) => None,
                    SshConnectionPhase::Idle
                    | SshConnectionPhase::Probing(_)
                    | SshConnectionPhase::AwaitingAuthentication { .. }
                    | SshConnectionPhase::LoadingStoredCredential => None,
                })
            else {
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
            if pending.tab_id != active_tab_id
                || app
                    .terminal(pending.tab_id)
                    .and_then(TerminalTabState::ssh_route)
                    .is_none_or(|route| route.0 != pending.profile_id)
                || profile.host != pending.host
                || profile.port != pending.port
            {
                if let Some(terminal) = app.terminal_mut(pending.tab_id) {
                    terminal.set_ssh_phase(SshConnectionPhase::Idle);
                }
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
            let Some(terminal) = app.terminal_mut(pending.tab_id) else {
                set_status(&ui_for_confirm, "SSH terminal tab is no longer available");
                return;
            };
            let still_awaiting_this_key = matches!(
                terminal.ssh_phase(),
                Some(SshConnectionPhase::AwaitingHostKey(current))
                    if current.profile_id == pending.profile_id
                        && current.host == pending.host
                        && current.port == pending.port
                        && current.fingerprint == pending.fingerprint
            );
            if !still_awaiting_this_key {
                set_status(
                    &ui_for_confirm,
                    "Host-key confirmation is no longer current",
                );
                return;
            }
            terminal.set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
                vault_unlock_only: false,
            });
            (pending.tab_id, profile)
        };

        info!(
            tab_id = %tab_id,
            session_id = %profile.id,
            fingerprint = ?profile.host_key_fingerprint,
            "SSH host key trusted by user"
        );
        refresh_workspace(&ui_for_confirm, &state_for_confirm);
        begin_authentication(
            &runtime,
            state_for_confirm.clone(),
            ui_for_confirm.clone(),
            tab_id,
            profile,
        );
    });

    let ui_for_reject = ui.as_weak();
    let state_for_reject = state.clone();
    ui.on_reject_host_key(move || {
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
