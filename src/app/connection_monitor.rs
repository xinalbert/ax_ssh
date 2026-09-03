use super::*;
use tokio::time::timeout;

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
    target: ConnectionTarget,
) {
    let Some(ssh) = profile.ssh().cloned() else {
        error!(session_id = %profile.id, "SSH monitor received a non-SSH profile");
        return;
    };
    let runtime_for_monitor = runtime.clone();
    runtime.spawn(async move {
        let mut terminal_event = false;
        let mut presentation =
            crate::app::terminal_presentation::TerminalPresentation::new();
        loop {
            let event = tokio::select! {
                event = events.recv() => {
                    let Some(event) = event else {
                        break;
                    };
                    event
                }
                ready = presentation.wait_until_ready(tab_id), if presentation.has_pending_output() => {
                    if prepare_terminal_output_snapshot(&state, tab_id) {
                        if let Some(received_at) = ready.output_received_at {
                            dispatch_terminal_output_snapshot(&ui, &state, tab_id, received_at);
                        } else {
                            dispatch_terminal_snapshot(&ui, &state, tab_id);
                        }
                    }
                    continue;
                }
            };
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
                            let generation = terminal.reconnect_generation();
                            terminal.mark_reconnect_connected(generation);
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
                    let mut response_error = None;
                    if mutate_terminal_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        |terminal| {
                            if let Err(error) = process_terminal_output(terminal, &data) {
                                response_error = Some(error);
                            }
                        },
                    )
                    .is_some()
                        && !data.is_empty()
                    {
                        presentation.record_output(Some(received_at));
                    }
                    if let Some(error) = response_error {
                        warn!(
                            tab_id = %tab_id,
                            session_id = %profile.id,
                            %error,
                            "failed to send SSH terminal protocol response"
                        );
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
                            SftpTransferEvent::Queued {
                                transfer_id,
                                remote_path: _,
                                name,
                                total_bytes,
                            } => {
                                let _ = terminal
                                    .sftp
                                    .queue_transfer(transfer_id, name, total_bytes);
                            }
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
                            SftpTransferEvent::Paused {
                                transfer_id,
                                downloaded_bytes,
                                total_bytes,
                            } => {
                                terminal.sftp.update_transfer_progress(
                                    transfer_id,
                                    downloaded_bytes,
                                    total_bytes,
                                );
                                terminal.sftp.pause_transfer(transfer_id);
                            }
                            SftpTransferEvent::Resumed {
                                transfer_id,
                                downloaded_bytes,
                                total_bytes,
                            } => {
                                terminal.sftp.resume_transfer(
                                    transfer_id,
                                    downloaded_bytes,
                                    total_bytes,
                                );
                            }
                            SftpTransferEvent::DiscoveryFailed {
                                transfer_id,
                                name,
                                message,
                            } => {
                                terminal
                                    .sftp
                                    .record_transfer_failure(transfer_id, name, message);
                            }
                            SftpTransferEvent::Completed {
                                transfer_id,
                                local_path,
                                total_bytes,
                            } => {
                                if local_path.as_os_str().is_empty() {
                                    terminal.sftp.finish_transfer(
                                        transfer_id,
                                        SftpTransferPhase::Completed,
                                        "Uploaded".to_owned(),
                                    );
                                } else if terminal.sftp.mark_transfer_opening(transfer_id, total_bytes) {
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
                SshSessionEvent::SftpWrite(event) => {
                    let monitor_path = match &event {
                        ax_ssh::sftp::SftpWriteEvent::Text { path, .. } => Some(path.clone()),
                        ax_ssh::sftp::SftpWriteEvent::Updated { path, .. } => Some(path.clone()),
                        _ => None,
                    };
                    let Some(active) = mutate_terminal_attempt(
                        &state,
                        tab_id,
                        profile.id,
                        attempt_id,
                        |terminal| match event {
                            ax_ssh::sftp::SftpWriteEvent::Completed { path, .. } => {
                                terminal.sftp.status = format!("Updated {path}");
                            }
                            ax_ssh::sftp::SftpWriteEvent::Updated {
                                path,
                                size,
                                modified,
                                ..
                            } => {
                                terminal.sftp.status = format!("Updated {path}");
                                terminal.sftp.editor_path = Some(path);
                                terminal.sftp.editor_expected_size = Some(size);
                                terminal.sftp.editor_expected_modified = modified;
                                terminal.sftp.editor_remote_changed = false;
                                terminal.sftp.editor_monitor_generation = terminal
                                    .sftp
                                    .editor_monitor_generation
                                    .wrapping_add(1);
                            }
                            ax_ssh::sftp::SftpWriteEvent::Text {
                                path,
                                data,
                                expected_size,
                                expected_modified,
                                ..
                            } => {
                                terminal.sftp.status = format!("Loaded {path}");
                                terminal.sftp.editor_path = Some(path);
                                terminal.sftp.editor_text = String::from_utf8_lossy(&data).into_owned();
                                terminal.sftp.editor_expected_size = Some(expected_size);
                                terminal.sftp.editor_expected_modified = expected_modified;
                                terminal.sftp.editor_remote_changed = false;
                                terminal.sftp.editor_revision = terminal.sftp.editor_revision.wrapping_add(1);
                                terminal.sftp.editor_monitor_generation = terminal
                                    .sftp
                                    .editor_monitor_generation
                                    .wrapping_add(1);
                            }
                            ax_ssh::sftp::SftpWriteEvent::Metadata {
                                path,
                                size,
                                modified,
                                ..
                            } => {
                                let changed = terminal.sftp.editor_expected_size != Some(size)
                                    || terminal.sftp.editor_expected_modified != modified;
                                terminal.sftp.editor_remote_changed = changed;
                                terminal.sftp.status = if changed {
                                    format!("Remote file changed: {path}")
                                } else {
                                    format!("Remote file unchanged: {path}")
                                };
                            }
                            ax_ssh::sftp::SftpWriteEvent::Failed { message, .. } => {
                                terminal.sftp.status = format!("SFTP write failed: {message}");
                            }
                        },
                    ) else {
                        continue;
                    };
                    if active {
                        dispatch_active_snapshot(&ui, &state);
                    }
                    if let Some(path) = monitor_path {
                        spawn_remote_editor_monitor(
                            &runtime_for_monitor,
                            state.clone(),
                            ui.clone(),
                            tab_id,
                            profile.id,
                            attempt_id,
                            path,
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
                        schedule_reconnect(
                            &runtime_for_monitor,
                            state.clone(),
                            ui.clone(),
                            tab_id,
                            profile.id,
                            ReconnectProtocol::Ssh,
                            target,
                        );
                        refresh_workspace(&ui, &state);
                    }
                }
                SshSessionEvent::HostKeyRejected {
                    expected,
                    actual,
                    public_key,
                } => {
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
                        public_key,
                        changed: expected.is_some(),
                        revoked: false,
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
                SshSessionEvent::HostKeyRevoked {
                    actual,
                    public_key,
                } => {
                    terminal_event = true;
                    let prompt = PendingHostKey {
                        tab_id,
                        profile_id: profile.id,
                        host: ssh.host.clone(),
                        port: ssh.port,
                        fingerprint: actual,
                        public_key,
                        changed: false,
                        revoked: true,
                    };
                    if prepare_host_key_retry(&state, tab_id, profile.id, attempt_id, prompt).unwrap_or(false) {
                        set_tab_status(&state, &ui, tab_id, "SSH host key is revoked; remove the record explicitly");
                        refresh_workspace(&ui, &state);
                    }
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
                        let persistence = match state.lock() {
                            Ok(app) => app.persistence_coordinator.clone(),
                            Err(_) => {
                                set_status(&ui, "Cannot read session state");
                                continue;
                            }
                        };
                        let _persistence_guard = persistence.gate.lock().await;
                        if let Some(storage) = ssh.credential_storage {
                            match delete_password(
                                profile.id,
                                storage,
                                ssh.credential_vault_key_in_keyring,
                            )
                            .await
                            {
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
                                        Some(&profile),
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
                        schedule_reconnect(
                            &runtime_for_monitor,
                            state.clone(),
                            ui.clone(),
                            tab_id,
                            profile.id,
                            ReconnectProtocol::Ssh,
                            target,
                        );
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
            if terminal_event {
                presentation.clear_pending_output();
            }
        }

        presentation.clear_pending_output();
        let retired = retire_session_attempt(&state, tab_id, profile.id, attempt_id);
        if !terminal_event && retired {
            schedule_reconnect(
                &runtime_for_monitor,
                state.clone(),
                ui.clone(),
                tab_id,
                profile.id,
                ReconnectProtocol::Ssh,
                target,
            );
            refresh_workspace(&ui, &state);
        }
        debug!(tab_id = %tab_id, session_id = %profile.id, "SSH event monitor stopped");
    });
}

fn spawn_remote_editor_monitor(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    profile_id: Uuid,
    attempt_id: Uuid,
    path: String,
) {
    runtime.spawn(async move {
        let generation = state.lock().ok().and_then(|app| {
            app.terminal(tab_id)
                .and_then(|terminal| {
                    terminal
                        .sftp
                        .editor_path
                        .as_deref()
                        .filter(|current| *current == path.as_str())
                })
                .and_then(|_| {
                    app.terminal(tab_id)
                        .map(|terminal| terminal.sftp.editor_monitor_generation)
                })
        });
        let Some(generation) = generation else {
            return;
        };
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if !session_attempt_is_active(&state, tab_id, profile_id, attempt_id) {
                break;
            }
            let queued = state.lock().ok().and_then(|mut app| {
                let terminal = app.terminal_mut(tab_id)?;
                if terminal.sftp.editor_path.as_deref() != Some(path.as_str())
                    || terminal.sftp.editor_monitor_generation != generation
                {
                    return None;
                }
                terminal.worker.as_ref().and_then(|worker| {
                    worker
                        .request_sftp_write(
                            Uuid::new_v4(),
                            ax_ssh::sftp::SftpWriteOperation::CheckMetadata { path: path.clone() },
                        )
                        .ok()
                })
            });
            if queued.is_none() {
                break;
            }
            dispatch_active_snapshot(&ui, &state);
        }
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
        let persistence = match state.lock() {
            Ok(app) => app.persistence_coordinator.clone(),
            Err(_) => {
                set_status(&ui, "Cannot read session state");
                return;
            }
        };
        let mutation_token = match begin_profile_mutation(
            &state,
            session_id,
            Some(&credential.expected_profile),
        ) {
            Ok(token) => token,
            Err(error) => {
                debug!(session_id = %session_id, %error, "skipping credential persistence for a changed profile");
                return;
            }
        };
        let _mutation_guard = persistence.gate.lock().await;
        if let Err(error) = ensure_profile_mutation_current(
            &state,
            session_id,
            mutation_token,
            Some(&credential.expected_profile),
        ) {
            finish_profile_mutation(&state, session_id, mutation_token);
            debug!(session_id = %session_id, %error, "stale credential persistence ignored");
            return;
        }
        let rollback = match save_password(
            credential.storage,
            session_id,
            credential.secret,
            credential.vault_password,
            credential.vault_password_generated,
            credential.previous_vault_password_generated,
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
                finish_profile_mutation(&state, session_id, mutation_token);
                return;
            }
        };

        if let Err(error) = ensure_profile_mutation_current(
            &state,
            session_id,
            mutation_token,
            Some(&credential.expected_profile),
        ) {
            if let Err(cleanup_error) = rollback.restore().await {
                warn!(session_id = %session_id, %cleanup_error, "failed to restore credential after profile changed");
            }
            finish_profile_mutation(&state, session_id, mutation_token);
            debug!(session_id = %session_id, %error, "stale credential storage update ignored");
            return;
        }
        if let Err(error) = commit_profile_credential_storage(
            &state,
            &credential.expected_profile,
            credential.storage,
            credential.vault_password_generated,
            mutation_token,
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
    pub(super) expected_profile: SessionProfile,
    pub(super) storage: CredentialStorage,
    pub(super) previous_storage: Option<CredentialStorage>,
    pub(super) secret: zeroize::Zeroizing<String>,
    pub(super) vault_password: Option<zeroize::Zeroizing<String>>,
    pub(super) vault_password_generated: bool,
    pub(super) previous_vault_password_generated: bool,
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
    fn downloaded_file_opener_failure_stays_failed() {
        let (phase, status) =
            classify_downloaded_file_open(Err("no default application".to_owned()));
        assert_eq!(phase, SftpTransferPhase::Failed);
        assert!(status.contains("no default application"));

        let (phase, status) = classify_downloaded_file_open(Ok(()));
        assert_eq!(phase, SftpTransferPhase::Completed);
        assert_eq!(status, "Opened");
    }
}
