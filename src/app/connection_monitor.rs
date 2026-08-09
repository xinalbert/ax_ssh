use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_session_monitor(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    profile: SessionProfile,
    attempt_id: Uuid,
    mut events: mpsc::Receiver<SshSessionEvent>,
    mut credential_to_store: Option<PendingCredentialStore>,
    used_stored_credential: bool,
) {
    let Some(ssh) = profile.ssh().cloned() else {
        error!(session_id = %profile.id, "SSH monitor received a non-SSH profile");
        return;
    };
    let runtime_for_monitor = runtime.clone();
    runtime.spawn(async move {
        let mut terminal_event = false;
        while let Some(event) = events.recv().await {
            match event {
                SshSessionEvent::Connected => {
                    let Some(active) = mutate_terminal_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        |terminal| {
                            terminal.connected = true;
                            terminal.worker_running = true;
                            terminal.status = format!("Connected to {}", profile_endpoint(&profile));
                        },
                    ) else {
                        continue;
                    };
                    info!(tab_id = %tab_id, session_id = %profile.id, "SSH worker reported connected");
                    if active {
                        dispatch_active_snapshot(&ui, &state);
                    }
                    refresh_workspace(&ui, &state);
                    if let Some(credential) = credential_to_store.take() {
                        persist_authenticated_credential(
                            &runtime_for_monitor,
                            state.clone(),
                            ui.clone(),
                            tab_id,
                            profile.id,
                            attempt_id,
                            credential,
                        );
                    }
                }
                SshSessionEvent::Output { data, received_at } => {
                    if let Some(true) = mutate_terminal_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        |terminal| {
                            if let Some(model) = terminal.terminal.as_mut() {
                                model.process(&data);
                            }
                        },
                    ) {
                        dispatch_terminal_output_snapshot(&ui, &state, received_at);
                    }
                }
                SshSessionEvent::Sftp(event) => {
                    let icon_keys = match &event {
                        SftpBrowserEvent::DirectoryPage { entries, .. } => sftp_icon_keys(entries),
                        _ => Vec::new(),
                    };
                    let Some(active) = mutate_terminal_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        |terminal| {
                            if let SftpBrowserEvent::Failed(message) = &event {
                                terminal.status = format!("SFTP: {message}");
                            } else if matches!(&event, SftpBrowserEvent::DirectoryPage { .. })
                                && terminal.status.starts_with("SFTP: ")
                            {
                                terminal.status =
                                    format!("Connected to {}", profile_endpoint(&profile));
                            }
                            apply_sftp_event(&mut terminal.sftp, event);
                        },
                    ) else {
                        continue;
                    };
                    if active {
                        dispatch_active_snapshot(&ui, &state);
                    }
                    prewarm_file_icons(&runtime_for_monitor, icon_keys, &ui, &state);
                }
                SshSessionEvent::SftpTransfer(event) => {
                    let mut completed_open = None;
                    let Some(active) = mutate_terminal_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        |terminal| match event {
                            SftpTransferEvent::Started {
                                transfer_id,
                                remote_path: _,
                                name,
                                total_bytes,
                            } => {
                                terminal
                                    .sftp
                                    .start_transfer(transfer_id, name, total_bytes);
                            }
                            SftpTransferEvent::Progress {
                                transfer_id,
                                downloaded_bytes,
                                total_bytes,
                            } => {
                                terminal.sftp.update_transfer_progress(
                                    transfer_id,
                                    downloaded_bytes,
                                    total_bytes,
                                );
                            }
                            SftpTransferEvent::Completed {
                                transfer_id,
                                local_path,
                                total_bytes,
                            } => {
                                if terminal
                                    .sftp
                                    .mark_transfer_opening(transfer_id, total_bytes)
                                {
                                    completed_open = Some((transfer_id, local_path));
                                }
                            }
                            SftpTransferEvent::Cancelled { transfer_id } => {
                                terminal.sftp.finish_transfer(
                                    transfer_id,
                                    SftpTransferPhase::Cancelled,
                                    "Cancelled".to_owned(),
                                );
                            }
                            SftpTransferEvent::Failed {
                                transfer_id,
                                message,
                            } => {
                                terminal.sftp.finish_transfer(
                                    transfer_id,
                                    SftpTransferPhase::Failed,
                                    message,
                                );
                            }
                        },
                    ) else {
                        continue;
                    };
                    if active {
                        dispatch_active_snapshot(&ui, &state);
                    }
                    if let Some((transfer_id, local_path)) = completed_open {
                        open_downloaded_sftp_file(
                            &runtime_for_monitor,
                            state.clone(),
                            ui.clone(),
                            tab_id,
                            profile.id,
                            attempt_id,
                            transfer_id,
                            local_path,
                        );
                    }
                }
                SshSessionEvent::X11ForwardingEnabled => {
                    if let Some(active) = mutate_terminal_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        |terminal| {
                            terminal.status = format!(
                                "Connected to {} - X11 forwarding enabled",
                                profile_endpoint(&profile)
                            );
                        },
                    ) {
                        if active {
                            dispatch_active_snapshot(&ui, &state);
                        }
                        refresh_workspace(&ui, &state);
                    }
                }
                SshSessionEvent::X11ForwardingUnavailable(message) => {
                    if let Some(active) = mutate_terminal_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        |terminal| {
                            terminal.status = format!(
                                "Connected to {}; X11 unavailable: {message}",
                                profile_endpoint(&profile)
                            );
                        },
                    ) {
                        if active {
                            dispatch_active_snapshot(&ui, &state);
                        }
                        refresh_workspace(&ui, &state);
                    }
                }
                // The resize callback updates the active model immediately after its request is
                // accepted. A delayed worker acknowledgement must not restore an older grid.
                SshSessionEvent::Resized { .. } => {}
                SshSessionEvent::Disconnected => {
                    terminal_event = true;
                    let _ = mutate_terminal_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        |terminal| terminal.sftp.reset(),
                    );
                    if retire_session_attempt(&state, tab_id, profile.id, attempt_id) {
                        set_tab_status(&state, &ui, tab_id, "Disconnected");
                        refresh_workspace(&ui, &state);
                    }
                }
                SshSessionEvent::HostKeyRejected { expected, actual } => {
                    terminal_event = true;
                    warn!(
                        tab_id = %tab_id,
                        session_id = %profile.id,
                        expected = ?expected,
                        fingerprint = %actual,
                        "SSH worker rejected host key"
                    );
                    let prompt = PendingHostKey {
                        tab_id,
                        profile_id: profile.id,
                        host: ssh.host.clone(),
                        port: ssh.port,
                        fingerprint: actual,
                        changed: expected.is_some(),
                    };
                    match prepare_host_key_retry(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        prompt.clone(),
                    ) {
                        Ok(true) => {}
                        Ok(false) => {
                            debug!(tab_id = %tab_id, %attempt_id, "stale host-key rejection ignored");
                            continue;
                        }
                        Err(error) => {
                            error!(tab_id = %tab_id, %error, "cannot prepare host-key retry");
                            continue;
                        }
                    }
                    set_tab_status(
                        &state,
                        &ui,
                        tab_id,
                        "SSH host key changed; verify it before reconnecting",
                    );
                    refresh_workspace(&ui, &state);
                }
                SshSessionEvent::AuthenticationFailed => {
                    terminal_event = true;
                    if matches!(ssh.auth, AuthMethod::SshAgent) {
                        if retire_session_attempt(&state, tab_id, profile.id, attempt_id) {
                            set_tab_status(
                                &state,
                                &ui,
                                tab_id,
                                "The SSH server rejected the available agent identities",
                            );
                            refresh_workspace(&ui, &state);
                        }
                        continue;
                    }
                    let retry = if used_stored_credential {
                        prepare_stored_credential_retry(&state, tab_id, profile.id, attempt_id)
                    } else {
                        prepare_authentication_retry(&state, tab_id, profile.id, attempt_id)
                    };
                    let retry_current = match retry {
                        Ok(current) => current,
                        Err(error) => {
                            error!(tab_id = %tab_id, %attempt_id, %error, "failed to prepare authentication retry");
                            false
                        }
                    };
                    if !retry_current {
                        continue;
                    }
                    if used_stored_credential {
                        if let Some(storage) = ssh.credential_storage {
                            match delete_password(profile.id, storage).await {
                                Ok(rollback) => {
                                    if !session_is_loading_stored_credential(
                                        &state,
                                        tab_id,
                                        profile.id,
                                    ) {
                                        if let Err(error) = rollback.restore().await {
                                            warn!(session_id = %profile.id, %error, "failed to restore stale rejected credential");
                                        }
                                        continue;
                                    }
                                    match set_credential_storage_while_loading(
                                        &state,
                                        tab_id,
                                        profile.id,
                                        None,
                                    ) {
                                        Ok(true) => {}
                                        Ok(false) => {
                                            if let Err(error) = rollback.restore().await {
                                                warn!(session_id = %profile.id, %error, "failed to restore stale rejected credential");
                                            }
                                            continue;
                                        }
                                        Err(error) => {
                                            warn!(session_id = %profile.id, %error, "failed to clear rejected credential storage reference");
                                            if let Err(restore_error) = rollback.restore().await {
                                                warn!(session_id = %profile.id, %restore_error, "failed to restore rejected credential after storage reference save failure");
                                            }
                                        }
                                    }
                                }
                                Err(error) => {
                                    warn!(session_id = %profile.id, %error, "failed to remove rejected stored credential");
                                }
                            }
                        }
                        if !finish_stored_credential_retry(&state, tab_id, profile.id) {
                            debug!(tab_id = %tab_id, %attempt_id, "stale saved-credential cleanup ignored");
                            continue;
                        }
                    }
                    set_tab_status(
                        &state,
                        &ui,
                        tab_id,
                        if matches!(ssh.auth, AuthMethod::PrivateKey { .. }) {
                            "The server rejected this private key"
                        } else if used_stored_credential {
                            "Saved password was rejected; enter a new password"
                        } else {
                            "Authentication failed; check the password and try again"
                        },
                    );
                    refresh_workspace(&ui, &state);
                }
                SshSessionEvent::PrivateKeyFailed(message) => {
                    terminal_event = true;
                    let retry_current = prepare_authentication_retry(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                    )
                    .unwrap_or(false);
                    if retry_current {
                        set_tab_status(
                            &state,
                            &ui,
                            tab_id,
                            &format!("Private key could not be loaded: {message}"),
                        );
                        refresh_workspace(&ui, &state);
                    }
                }
                SshSessionEvent::Failed(message) => {
                    terminal_event = true;
                    let _ = mutate_terminal_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        |terminal| terminal.sftp.reset(),
                    );
                    if retire_session_attempt(&state, tab_id, profile.id, attempt_id) {
                        set_tab_status(
                            &state,
                            &ui,
                            tab_id,
                            &format!("Connection failed: {message}"),
                        );
                        refresh_workspace(&ui, &state);
                    }
                }
            }
        }

        let retired = retire_session_attempt(&state, tab_id, profile.id, attempt_id);
        if !terminal_event && retired {
            set_tab_status(&state, &ui, tab_id, "SSH worker stopped");
            refresh_workspace(&ui, &state);
        }
        debug!(tab_id = %tab_id, session_id = %profile.id, "SSH event monitor stopped");
    });
}

#[allow(clippy::too_many_arguments)]
fn open_downloaded_sftp_file(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    profile_id: Uuid,
    attempt_id: Uuid,
    transfer_id: Uuid,
    local_path: PathBuf,
) {
    runtime.spawn(async move {
        if !session_attempt_is_active(&state, tab_id, profile_id, attempt_id) {
            return;
        }
        let opened = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || open::that_detached(local_path)),
        )
        .await;
        let open_result = match opened {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(error))) => Err(format!("Cannot open downloaded file: {error}")),
            Ok(Err(error)) => Err(format!("File opener task failed: {error}")),
            Err(_) => Err("File opener timed out".to_owned()),
        };
        let (phase, status) = classify_downloaded_file_open(open_result);
        if let Some(active) =
            mutate_terminal_attempt(&state, tab_id, profile_id, attempt_id, |terminal| {
                terminal.sftp.finish_transfer(transfer_id, phase, status)
            })
            && active
        {
            dispatch_active_snapshot(&ui, &state);
        }
    });
}

fn classify_downloaded_file_open(result: Result<(), String>) -> (SftpTransferPhase, String) {
    match result {
        Ok(()) => (SftpTransferPhase::Completed, "Opened".to_owned()),
        Err(message) => (SftpTransferPhase::Failed, bounded_transfer_message(message)),
    }
}

fn bounded_transfer_message(message: String) -> String {
    const MAX_TRANSFER_STATUS_CHARS: usize = 512;
    let mut chars = message.chars();
    let mut bounded = chars
        .by_ref()
        .take(MAX_TRANSFER_STATUS_CHARS)
        .collect::<String>();
    if chars.next().is_some() {
        bounded.push_str("...");
    }
    bounded
}

fn apply_sftp_event(state: &mut super::state::SftpBrowserState, event: SftpBrowserEvent) {
    match event {
        SftpBrowserEvent::Opened { home } => {
            state.reset_navigation();
            state.open = true;
            state.loading = true;
            state.home = home;
            state.status = "Loading directory...".to_owned();
        }
        SftpBrowserEvent::DirectoryPage {
            path,
            entries,
            append,
            has_more,
            truncated,
        } => {
            state.open = true;
            state.loading = false;
            if !append {
                state.complete_navigation(path.clone());
            } else {
                state.path = path;
            }
            if append {
                state.entries.extend(entries);
            } else {
                state.entries = entries;
            }
            state
                .selected
                .retain(|selected| state.entries.iter().any(|entry| &entry.path == selected));
            state.has_more = has_more;
            state.truncated = truncated;
            state.status = if truncated {
                "Directory limit reached".to_owned()
            } else {
                format!("{} items", state.entries.len())
            };
        }
        SftpBrowserEvent::Failed(message) => {
            state.cancel_navigation();
            state.status = message;
        }
        SftpBrowserEvent::Closed => {
            state.open = false;
            state.loading = false;
            state.has_more = false;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn persist_authenticated_credential(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    session_id: Uuid,
    attempt_id: Uuid,
    credential: PendingCredentialStore,
) {
    runtime.spawn(async move {
        let rollback = match save_password(
            credential.storage,
            session_id,
            credential.secret,
            credential.vault_password,
            credential.previous_storage,
        )
        .await {
            Ok(rollback) => rollback,
            Err(error) => {
                warn!(session_id = %session_id, %error, "failed to save authenticated credential");
                if session_attempt_is_active(&state, tab_id, session_id, attempt_id) {
                    set_tab_status(
                        &state,
                        &ui,
                        tab_id,
                        &format!("Connected, but password could not be saved: {error}"),
                    );
                }
                return;
            }
        };

        if let Err(error) = set_credential_storage(
            &state,
            session_id,
            Some(credential.storage),
        ) {
            warn!(session_id = %session_id, %error, "failed to persist credential storage policy");
            if let Err(cleanup_error) = rollback.restore().await {
                warn!(session_id = %session_id, %cleanup_error, "failed to restore credential after storage reference save failure");
            }
            if session_attempt_is_active(&state, tab_id, session_id, attempt_id) {
                set_tab_status(
                    &state,
                    &ui,
                    tab_id,
                    &format!("Connected, but password preference could not be saved: {error}"),
                );
            }
            return;
        }

        let storage = credential.storage;
        info!(session_id = %session_id, storage = storage.as_setting(), "authenticated password stored");
        if session_attempt_is_active(&state, tab_id, session_id, attempt_id) {
            set_tab_status(
                &state,
                &ui,
                tab_id,
                &format!("Connected; password saved in {}", storage.as_setting()),
            );
        }
    });
}

pub(super) struct PendingCredentialStore {
    pub(super) storage: CredentialStorage,
    pub(super) previous_storage: Option<CredentialStorage>,
    pub(super) secret: zeroize::Zeroizing<String>,
    pub(super) vault_password: Option<zeroize::Zeroizing<String>>,
}

pub(super) fn mutate_terminal_attempt(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    profile_id: Uuid,
    attempt_id: Uuid,
    action: impl FnOnce(&mut TerminalTabState),
) -> Option<bool> {
    let mut app = state.lock().ok()?;
    let current = app
        .terminal(tab_id)
        .and_then(TerminalTabState::ssh_route)
        .is_some_and(|route| route == (profile_id, Some(attempt_id)));
    if !current {
        return None;
    }
    action(app.terminal_mut(tab_id)?);
    Some(true)
}

fn session_is_loading_stored_credential(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    profile_id: Uuid,
) -> bool {
    state.lock().is_ok_and(|app| {
        app.terminal(tab_id).is_some_and(|terminal| {
            terminal
                .ssh_route()
                .is_some_and(|route| route.0 == profile_id)
                && matches!(
                    terminal.ssh_phase(),
                    Some(SshConnectionPhase::LoadingStoredCredential)
                )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::SftpBrowserState;

    fn entry(name: &str) -> SftpEntry {
        SftpEntry {
            name: name.to_owned(),
            path: format!("/home/alice/{name}"),
            is_dir: false,
            is_symlink: false,
            size: 1,
            modified: None,
        }
    }

    #[test]
    fn sftp_events_replace_append_fail_and_close_recoverably() {
        let mut state = SftpBrowserState::default();

        apply_sftp_event(
            &mut state,
            SftpBrowserEvent::Opened {
                home: "/home/alice".to_owned(),
            },
        );
        assert!(state.open);
        assert!(state.loading);

        apply_sftp_event(
            &mut state,
            SftpBrowserEvent::DirectoryPage {
                path: "/home/alice".to_owned(),
                entries: vec![entry("first")],
                append: false,
                has_more: true,
                truncated: false,
            },
        );
        apply_sftp_event(
            &mut state,
            SftpBrowserEvent::DirectoryPage {
                path: "/home/alice".to_owned(),
                entries: vec![entry("second")],
                append: true,
                has_more: false,
                truncated: false,
            },
        );
        assert_eq!(state.entries.len(), 2);
        assert_eq!(state.status, "2 items");

        apply_sftp_event(
            &mut state,
            SftpBrowserEvent::Failed("permission denied".to_owned()),
        );
        assert!(!state.loading);
        assert_eq!(state.status, "permission denied");

        apply_sftp_event(&mut state, SftpBrowserEvent::Closed);
        assert!(!state.open);
        assert!(!state.has_more);
    }

    #[test]
    fn failed_download_opener_never_transitions_to_completed() {
        let (phase, status) =
            classify_downloaded_file_open(Err("no default application".to_owned()));
        assert_eq!(phase, SftpTransferPhase::Failed);
        assert!(status.contains("no default application"));

        let (phase, status) = classify_downloaded_file_open(Ok(()));
        assert_eq!(phase, SftpTransferPhase::Completed);
        assert_eq!(status, "Opened");
    }
}
