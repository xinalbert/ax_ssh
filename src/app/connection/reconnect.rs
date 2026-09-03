use super::*;

const MAX_RECONNECT_ATTEMPTS: u8 = TerminalTabState::MAX_RECONNECT_ATTEMPTS;
const RECONNECT_BASE_DELAY: Duration = Duration::from_secs(1);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum ReconnectProtocol {
    Ssh,
    Telnet,
    Serial,
}

pub(in crate::app) fn schedule_reconnect(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    profile_id: Uuid,
    protocol: ReconnectProtocol,
    target: ConnectionTarget,
) {
    let (generation, _attempt, delay) = {
        let Ok(mut app) = state.lock() else {
            set_status(&ui, "Cannot read session state");
            return;
        };
        let Some(terminal) = app.terminal_mut(tab_id) else {
            return;
        };
        let Some((generation, attempt)) = terminal.begin_reconnect() else {
            if terminal.reconnect_attempt >= MAX_RECONNECT_ATTEMPTS {
                terminal.status = format!(
                    "Reconnect failed after {MAX_RECONNECT_ATTEMPTS} attempts; retry manually"
                );
            }
            return;
        };
        let delay = reconnect_delay(attempt);
        terminal.status = format!(
            "Connection lost; reconnecting in {}s ({attempt}/{MAX_RECONNECT_ATTEMPTS})",
            delay.as_secs()
        );
        (generation, attempt, delay)
    };
    refresh_workspace(&ui, &state);

    let runtime = runtime.clone();
    let runtime_for_task = runtime.clone();
    runtime.spawn(async move {
        tokio::time::sleep(delay).await;
        let profile = state.lock().ok().and_then(|app| {
            let terminal = app.terminal(tab_id)?;
            if !terminal.reconnect_current(generation) || terminal.profile_id() != Some(profile_id)
            {
                return None;
            }
            let profile = app
                .sessions
                .sessions
                .iter()
                .find(|profile| profile.id == profile_id)
                .cloned()?;
            profile_matches_reconnect_protocol(&profile, protocol).then_some(profile)
        });
        let Some(profile) = profile else {
            if clear_reconnect_attempt(
                &state,
                tab_id,
                generation,
                "Session changed, uses a different protocol, or was removed; reconnect cancelled",
            ) {
                refresh_workspace(&ui, &state);
            }
            return;
        };
        set_tab_status(&state, &ui, tab_id, "Reconnecting...");
        let result = match protocol {
            ReconnectProtocol::Ssh => {
                reconnect_ssh(&runtime_for_task, &state, &ui, tab_id, &profile, target)
            }
            ReconnectProtocol::Telnet => start_telnet_connection(
                &runtime_for_task,
                state.clone(),
                ui.clone(),
                tab_id,
                profile.clone(),
            ),
            ReconnectProtocol::Serial => start_serial_connection(
                &runtime_for_task,
                state.clone(),
                ui.clone(),
                tab_id,
                profile.clone(),
            ),
        };
        if let Err(error) = result
            && clear_reconnect_attempt(
                &state,
                tab_id,
                generation,
                &format!("Reconnect failed: {error}"),
            )
        {
            schedule_reconnect(
                &runtime_for_task,
                state,
                ui,
                tab_id,
                profile_id,
                protocol,
                target,
            );
        }
    });
}

fn reconnect_ssh(
    runtime: &Handle,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    tab_id: Uuid,
    profile: &SessionProfile,
    target: ConnectionTarget,
) -> Result<()> {
    let Some(ssh) = profile.ssh() else {
        anyhow::bail!("SSH profile is no longer available")
    };
    match ssh.auth {
        AuthMethod::PrivateKey { .. } | AuthMethod::SshAgent => {
            if !set_ssh_reconnect_phase(state, tab_id, profile.id) {
                anyhow::bail!("SSH reconnect is no longer current")
            }
            begin_authentication(
                runtime,
                state.clone(),
                ui.clone(),
                tab_id,
                profile.clone(),
                target,
            );
            Ok(())
        }
        AuthMethod::Password
            if ssh.credential_storage == Some(CredentialStorage::SystemKeyring)
                || (ssh.credential_storage == Some(CredentialStorage::EncryptedVault)
                    && ssh.credential_vault_key_in_keyring) =>
        {
            if !set_ssh_reconnect_phase(state, tab_id, profile.id) {
                anyhow::bail!("SSH reconnect is no longer current")
            }
            begin_authentication(
                runtime,
                state.clone(),
                ui.clone(),
                tab_id,
                profile.clone(),
                target,
            );
            Ok(())
        }
        AuthMethod::Password => {
            let message = if ssh.credential_storage == Some(CredentialStorage::EncryptedVault) {
                "Reconnect paused; unlock the saved SSH password to continue"
            } else {
                "Reconnect paused; enter the SSH password to continue"
            };
            if let Ok(mut app) = state.lock()
                && let Some(terminal) = app.terminal_mut(tab_id)
            {
                let generation = terminal.reconnect_generation();
                terminal.finish_reconnect_attempt(generation);
                terminal.status = message.to_owned();
                terminal.set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
                    vault_unlock_only: ssh.credential_storage
                        == Some(CredentialStorage::EncryptedVault),
                });
            }
            refresh_workspace(ui, state);
            Ok(())
        }
    }
}

fn set_ssh_reconnect_phase(state: &Arc<Mutex<AppState>>, tab_id: Uuid, profile_id: Uuid) -> bool {
    let Ok(mut app) = state.lock() else {
        return false;
    };
    app.terminal_mut(tab_id).is_some_and(|terminal| {
        terminal.profile_id() == Some(profile_id)
            && terminal.set_ssh_phase(SshConnectionPhase::AwaitingAuthentication {
                vault_unlock_only: false,
            })
    })
}

fn clear_reconnect_attempt(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    generation: u64,
    status: &str,
) -> bool {
    let Ok(mut app) = state.lock() else {
        return false;
    };
    let Some(terminal) = app.terminal_mut(tab_id) else {
        return false;
    };
    if !terminal.finish_reconnect_attempt(generation) {
        return false;
    }
    terminal.status = status.to_owned();
    true
}

fn reconnect_delay(attempt: u8) -> Duration {
    let shift = u32::from(attempt.saturating_sub(1)).min(5);
    (RECONNECT_BASE_DELAY * 2u32.saturating_pow(shift)).min(RECONNECT_MAX_DELAY)
}

fn profile_matches_reconnect_protocol(
    profile: &SessionProfile,
    protocol: ReconnectProtocol,
) -> bool {
    match protocol {
        ReconnectProtocol::Ssh => profile.ssh().is_some(),
        ReconnectProtocol::Telnet => profile.telnet().is_some(),
        ReconnectProtocol::Serial => profile.serial().is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_delay_is_bounded_exponential() {
        assert_eq!(reconnect_delay(1), Duration::from_secs(1));
        assert_eq!(reconnect_delay(2), Duration::from_secs(2));
        assert_eq!(reconnect_delay(5), Duration::from_secs(16));
        assert_eq!(reconnect_delay(8), Duration::from_secs(30));
    }

    #[test]
    fn reconnect_requires_the_original_profile_protocol() {
        let ssh = SessionProfile::new("ssh", "ssh.example", "alice");
        let telnet = SessionProfile::new_telnet("telnet", "telnet.example");
        let serial = SessionProfile::new_serial("serial", "/dev/cu.usbserial");

        assert!(profile_matches_reconnect_protocol(
            &ssh,
            ReconnectProtocol::Ssh
        ));
        assert!(profile_matches_reconnect_protocol(
            &telnet,
            ReconnectProtocol::Telnet
        ));
        assert!(profile_matches_reconnect_protocol(
            &serial,
            ReconnectProtocol::Serial
        ));
        assert!(!profile_matches_reconnect_protocol(
            &ssh,
            ReconnectProtocol::Telnet
        ));
        assert!(!profile_matches_reconnect_protocol(
            &telnet,
            ReconnectProtocol::Serial
        ));
        assert!(!profile_matches_reconnect_protocol(
            &serial,
            ReconnectProtocol::Ssh
        ));
    }
}
