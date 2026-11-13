//! SFTP transfers intent wiring and bounded application operations.

use super::*;
use crate::app::local_files::discover_local_uploads_streaming;

pub(super) fn wire_transfers(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let ui_for_selected_remote_download = ui.as_weak();
    let state_for_selected_remote_download = state.clone();
    let runtime_for_selected_remote_download = runtime.clone();
    let router_for_selected_remote_download = window_router.clone();
    ui.on_download_selected_remote_sftp(move || {
        log_ui_action("sftp.download-selected-remote");
        sync_window_active(
            &router_for_selected_remote_download,
            window_id,
            &state_for_selected_remote_download,
        );
        let prepared = prepare_selected_remote_downloads(
            &state_for_selected_remote_download,
            &router_for_selected_remote_download,
            window_id,
        );
        match prepared {
            Ok(prepared) => {
                queue_selected_remote_downloads(
                    &runtime_for_selected_remote_download,
                    state_for_selected_remote_download.clone(),
                    ui_for_selected_remote_download.clone(),
                    prepared,
                );
                dispatch_active_snapshot(
                    &ui_for_selected_remote_download,
                    &state_for_selected_remote_download,
                );
            }
            Err(error) => {
                set_status(
                    &ui_for_selected_remote_download,
                    &format!("Cannot download selected SFTP entries: {error}"),
                );
                dispatch_active_snapshot(
                    &ui_for_selected_remote_download,
                    &state_for_selected_remote_download,
                );
            }
        }
    });

    let ui_for_upload = ui.as_weak();
    let state_for_upload = state.clone();
    let runtime_for_upload = runtime.clone();
    let router_for_upload = window_router.clone();
    ui.on_upload_selected_local_sftp(move || {
        log_ui_action("sftp.upload-selected-local");
        sync_window_active(&router_for_upload, window_id, &state_for_upload);
        let prepared =
            prepare_selected_local_upload(&state_for_upload, &router_for_upload, window_id);
        let (tab_id, remote_directory, paths) = match prepared {
            Ok(value) => value,
            Err(error) => {
                set_status(
                    &ui_for_upload,
                    &format!("Cannot upload local file: {error}"),
                );
                return;
            }
        };
        queue_local_upload_paths(
            &runtime_for_upload,
            state_for_upload.clone(),
            ui_for_upload.clone(),
            tab_id,
            Uuid::new_v4(),
            remote_directory,
            paths,
        );
    });

    let ui_for_conflict = ui.as_weak();
    let state_for_conflict = state.clone();
    let router_for_conflict = window_router.clone();
    ui.on_resolve_sftp_transfer_conflict(move |id, choice, apply_to_batch| {
        log_ui_action("sftp.resolve-transfer-conflict");
        let Ok(transfer_id) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let choice = match choice.as_str() {
            "skip" => ax_ssh::sftp::SftpConflictChoice::Skip,
            "overwrite" => ax_ssh::sftp::SftpConflictChoice::Overwrite,
            "keep-both" => ax_ssh::sftp::SftpConflictChoice::KeepBoth,
            _ => return,
        };
        let result = with_window_sftp_terminal(
            &state_for_conflict,
            &router_for_conflict,
            window_id,
            |terminal| {
                if terminal
                    .sftp
                    .transfer_conflicts
                    .front()
                    .map(|item| item.transfer_id)
                    != Some(transfer_id)
                {
                    anyhow::bail!("file conflict is no longer active");
                }
                terminal
                    .worker
                    .as_ref()
                    .context("SFTP tab has no worker")?
                    .request_resolve_sftp_conflict(transfer_id, choice, apply_to_batch)?;
                terminal
                    .sftp
                    .resolve_transfer_conflict(transfer_id, apply_to_batch);
                Ok(())
            },
        );
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_conflict, &state_for_conflict),
            Err(error) => set_status(
                &ui_for_conflict,
                &format!("Cannot resolve file conflict: {error}"),
            ),
        }
    });

    let ui_for_transfer_pause = ui.as_weak();
    let state_for_transfer_pause = state.clone();
    let router_for_transfer_pause = window_router.clone();
    ui.on_pause_sftp_transfer(move |id| {
        log_ui_action("sftp.pause-transfer");
        sync_window_active(
            &router_for_transfer_pause,
            window_id,
            &state_for_transfer_pause,
        );
        let result = parse_transfer_id(id.as_str()).and_then(|transfer_id| {
            with_window_sftp_terminal(
                &state_for_transfer_pause,
                &router_for_transfer_pause,
                window_id,
                |terminal| {
                    if !terminal.sftp.transfer_is_pausable(transfer_id) {
                        anyhow::bail!("SFTP transfer is no longer pausable");
                    }
                    terminal
                        .worker
                        .as_ref()
                        .context("active SFTP tab has no worker")?
                        .request_pause_sftp_transfer(transfer_id)?;
                    if !terminal.sftp.request_transfer_pause(transfer_id) {
                        anyhow::bail!("SFTP transfer changed before pause was recorded");
                    }
                    Ok(())
                },
            )
        });
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_transfer_pause, &state_for_transfer_pause),
            Err(error) => set_status(
                &ui_for_transfer_pause,
                &format!("Cannot pause SFTP transfer: {error}"),
            ),
        }
    });

    let ui_for_transfer_resume = ui.as_weak();
    let state_for_transfer_resume = state.clone();
    let router_for_transfer_resume = window_router.clone();
    ui.on_resume_sftp_transfer(move |id| {
        log_ui_action("sftp.resume-transfer");
        sync_window_active(
            &router_for_transfer_resume,
            window_id,
            &state_for_transfer_resume,
        );
        let result = parse_transfer_id(id.as_str()).and_then(|transfer_id| {
            with_window_sftp_terminal(
                &state_for_transfer_resume,
                &router_for_transfer_resume,
                window_id,
                |terminal| {
                    if !terminal.sftp.transfer_is_resumable(transfer_id) {
                        anyhow::bail!("SFTP transfer is no longer resumable");
                    }
                    terminal
                        .worker
                        .as_ref()
                        .context("active SFTP tab has no worker")?
                        .request_resume_sftp_transfer(transfer_id)?;
                    if !terminal.sftp.request_transfer_resume(transfer_id) {
                        anyhow::bail!("SFTP transfer changed before resume was recorded");
                    }
                    Ok(())
                },
            )
        });
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_transfer_resume, &state_for_transfer_resume),
            Err(error) => set_status(
                &ui_for_transfer_resume,
                &format!("Cannot resume SFTP transfer: {error}"),
            ),
        }
    });

    let ui_for_transfer_cancel = ui.as_weak();
    let state_for_transfer_cancel = state.clone();
    let router_for_transfer_cancel = window_router.clone();
    ui.on_cancel_sftp_transfer(move |id| {
        log_ui_action("sftp.cancel-transfer");
        sync_window_active(
            &router_for_transfer_cancel,
            window_id,
            &state_for_transfer_cancel,
        );
        let result = parse_transfer_id(id.as_str()).and_then(|transfer_id| {
            with_window_sftp_transfer_intent(
                &state_for_transfer_cancel,
                &router_for_transfer_cancel,
                window_id,
                |terminal| {
                    if !terminal.sftp.transfer_is_cancellable(transfer_id) {
                        anyhow::bail!("SFTP transfer is no longer cancellable");
                    }
                    if !terminal.sftp.reconnect_replay_pending && terminal.connected {
                        terminal
                            .worker
                            .as_ref()
                            .context("active SFTP tab has no worker")?
                            .request_cancel_sftp_transfer(transfer_id)?;
                    }
                    if !terminal.sftp.request_transfer_cancel(transfer_id) {
                        anyhow::bail!("SFTP transfer changed before cancellation was recorded");
                    }
                    if !terminal.connected || terminal.sftp.reconnect_replay_pending {
                        terminal.sftp.finish_transfer(
                            transfer_id,
                            SftpTransferPhase::Cancelled,
                            "Cancelled".to_owned(),
                        );
                    }
                    Ok(())
                },
            )
        });
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_transfer_cancel, &state_for_transfer_cancel),
            Err(error) => set_status(
                &ui_for_transfer_cancel,
                &format!("Cannot cancel SFTP transfer: {error}"),
            ),
        }
    });

    let ui_for_transfer_selection = ui.as_weak();
    let state_for_transfer_selection = state.clone();
    let router_for_transfer_selection = window_router.clone();
    ui.on_toggle_sftp_transfer_selection(move |id, selected| {
        log_ui_action("sftp.toggle-transfer-selection");
        sync_window_active(
            &router_for_transfer_selection,
            window_id,
            &state_for_transfer_selection,
        );
        let result = parse_transfer_id(id.as_str()).and_then(|transfer_id| {
            with_window_sftp_terminal(
                &state_for_transfer_selection,
                &router_for_transfer_selection,
                window_id,
                |terminal| {
                    if !terminal
                        .sftp
                        .toggle_transfer_selection(transfer_id, selected)
                    {
                        anyhow::bail!("SFTP transfer is no longer available");
                    }
                    Ok(())
                },
            )
        });
        match result {
            Ok(()) => {
                dispatch_active_snapshot(&ui_for_transfer_selection, &state_for_transfer_selection)
            }
            Err(error) => set_status(
                &ui_for_transfer_selection,
                &format!("Cannot update SFTP transfer selection: {error}"),
            ),
        }
    });

    wire_selected_transfer_actions(ui, state.clone(), window_router.clone(), window_id);

    let ui_for_reveal_transfer = ui.as_weak();
    let state_for_reveal_transfer = state.clone();
    let runtime_for_reveal_transfer = runtime.clone();
    let router_for_reveal_transfer = window_router.clone();
    ui.on_reveal_sftp_transfer_local(move |id| {
        log_ui_action("sftp.reveal-transfer-local");
        sync_window_active(
            &router_for_reveal_transfer,
            window_id,
            &state_for_reveal_transfer,
        );
        let result = parse_transfer_id(id.as_str()).and_then(|transfer_id| {
            with_window_sftp_terminal(
                &state_for_reveal_transfer,
                &router_for_reveal_transfer,
                window_id,
                |terminal| {
                    terminal
                        .sftp
                        .completed_transfer_local_path(transfer_id)
                        .context("SFTP transfer has no local file to show")
                },
            )
        });
        match result {
            Ok(path) => reveal_local_path(
                &runtime_for_reveal_transfer,
                ui_for_reveal_transfer.clone(),
                path,
            ),
            Err(error) => set_status(
                &ui_for_reveal_transfer,
                &format!("Cannot show transferred file: {error}"),
            ),
        }
    });

    let ui_for_remove_transfer = ui.as_weak();
    let state_for_remove_transfer = state.clone();
    let router_for_remove_transfer = window_router.clone();
    ui.on_remove_sftp_transfer(move |id| {
        log_ui_action("sftp.remove-transfer");
        sync_window_active(
            &router_for_remove_transfer,
            window_id,
            &state_for_remove_transfer,
        );
        let result = parse_transfer_id(id.as_str()).and_then(|transfer_id| {
            with_window_sftp_terminal(
                &state_for_remove_transfer,
                &router_for_remove_transfer,
                window_id,
                |terminal| {
                    if !terminal.sftp.remove_finished_transfer(transfer_id) {
                        anyhow::bail!("SFTP transfer is still active or no longer available");
                    }
                    Ok(())
                },
            )
        });
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_remove_transfer, &state_for_remove_transfer),
            Err(error) => set_status(
                &ui_for_remove_transfer,
                &format!("Cannot remove SFTP transfer: {error}"),
            ),
        }
    });
}

struct PreparedRemoteDownloads {
    tab_id: Uuid,
    entries: Vec<SftpEntry>,
    local_directory: PathBuf,
    filters: Vec<String>,
}

fn sftp_submission_was_interrupted(error: &anyhow::Error) -> bool {
    error.downcast_ref::<ax_ssh::ssh::SftpSubmissionError>()
        == Some(&ax_ssh::ssh::SftpSubmissionError::Disconnected)
}

fn sftp_tab_is_reconnecting(state: &Arc<Mutex<AppState>>, tab_id: Uuid) -> bool {
    state.lock().is_ok_and(|app| {
        app.terminal(tab_id).is_some_and(|terminal| {
            terminal.is_sftp() && terminal.reconnect_enabled && !terminal.connected
        })
    })
}

fn sftp_tab_is_currently_connected(state: &Arc<Mutex<AppState>>, tab_id: Uuid) -> bool {
    state.lock().is_ok_and(|app| {
        app.terminal(tab_id)
            .is_some_and(|terminal| terminal.is_sftp() && terminal.connected)
    })
}

fn sftp_tab_has_replay_pending(state: &Arc<Mutex<AppState>>, tab_id: Uuid) -> bool {
    state.lock().is_ok_and(|app| {
        app.terminal(tab_id)
            .is_some_and(|terminal| terminal.is_sftp() && terminal.sftp.reconnect_replay_pending)
    })
}

fn transfer_intent_is_current(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    profile_id: Uuid,
    attempt_id: Uuid,
    intent: &SftpTransferIntent,
) -> bool {
    state.lock().is_ok_and(|app| {
        app.terminal(tab_id).is_some_and(|terminal| {
            terminal.profile_id() == Some(profile_id)
                && terminal.ssh_route().and_then(|(_, attempt)| attempt) == Some(attempt_id)
                && terminal.sftp.reconnect_replay_pending
                && terminal.sftp.has_transfer_intent(intent)
        })
    })
}

pub(in crate::app) async fn requeue_transfer_intents(
    state: Arc<Mutex<AppState>>,
    tab_id: Uuid,
    profile_id: Uuid,
    attempt_id: Uuid,
) {
    let (download, upload) = {
        let Ok(app) = state.lock() else {
            return;
        };
        let Some(terminal) = app.terminal(tab_id) else {
            return;
        };
        if terminal.profile_id() != Some(profile_id)
            || terminal.ssh_route().and_then(|(_, attempt)| attempt) != Some(attempt_id)
            || !terminal.sftp.reconnect_replay_pending
        {
            return;
        }
        let Some(worker) = terminal.worker.as_ref() else {
            return;
        };
        let Ok(download) = worker.sftp_download_submitter() else {
            return;
        };
        let Ok(upload) = worker.sftp_upload_submitter() else {
            return;
        };
        if terminal.sftp.transfers_paused
            && worker
                .request_sftp_transfer_action(
                    ax_ssh::sftp::SftpTransferAction::Pause,
                    true,
                    Vec::new(),
                )
                .is_err()
        {
            return;
        }
        let paused_ids = terminal
            .sftp
            .transfers
            .iter()
            .filter(|transfer| {
                matches!(
                    transfer.phase,
                    SftpTransferPhase::Paused | SftpTransferPhase::Pausing
                ) && !transfer.inherited_pause
            })
            .map(|transfer| transfer.id)
            .collect::<Vec<_>>();
        if !paused_ids.is_empty()
            && worker
                .request_sftp_transfer_action(
                    ax_ssh::sftp::SftpTransferAction::Pause,
                    false,
                    paused_ids,
                )
                .is_err()
        {
            return;
        }
        (download, upload)
    };
    let mut submitted = std::collections::HashSet::new();
    loop {
        let Some((revision, intents)) = state.lock().ok().and_then(|app| {
            let terminal = app.terminal(tab_id)?;
            (terminal.profile_id() == Some(profile_id)
                && terminal.ssh_route().and_then(|(_, attempt)| attempt) == Some(attempt_id)
                && terminal.sftp.reconnect_replay_pending)
                .then(|| terminal.sftp.transfer_intents_with_revision())
        }) else {
            return;
        };
        let mut retry_required = false;
        for intent in intents {
            let transfer_id = match &intent {
                SftpTransferIntent::DownloadRoot { transfer_id, .. }
                | SftpTransferIntent::Upload { transfer_id, .. } => *transfer_id,
                #[cfg(target_os = "macos")]
                SftpTransferIntent::DownloadFile { transfer_id, .. } => *transfer_id,
            };
            if submitted.contains(&transfer_id) {
                continue;
            }
            if !transfer_intent_is_current(&state, tab_id, profile_id, attempt_id, &intent) {
                continue;
            }
            let intent_for_recheck = intent.clone();
            let result = match intent {
                SftpTransferIntent::DownloadRoot {
                    transfer_id,
                    conflict_batch,
                    remote_path,
                    local_directory,
                    filter_patterns,
                    expected_directory,
                    completed_children: _,
                } => {
                    let excluded_remote_paths = state
                        .lock()
                        .ok()
                        .and_then(|app| {
                            app.terminal(tab_id).map(|terminal| {
                                terminal.sftp.completed_directory_child_paths(transfer_id)
                            })
                        })
                        .unwrap_or_default();
                    download
                        .clone()
                        .for_batch(conflict_batch)
                        .submit_with_excluded(
                            transfer_id,
                            remote_path,
                            local_directory,
                            filter_patterns,
                            expected_directory,
                            excluded_remote_paths,
                        )
                        .await
                }
                #[cfg(target_os = "macos")]
                SftpTransferIntent::DownloadFile {
                    transfer_id,
                    remote_path,
                    local_path,
                    total_bytes,
                    replay_on_reconnect: _,
                } => {
                    download
                        .submit_file(transfer_id, remote_path, local_path, total_bytes)
                        .await
                }
                SftpTransferIntent::Upload {
                    transfer_id,
                    batch_id,
                    remote_path,
                    local_path,
                    total_bytes,
                    remote_directories,
                } => {
                    upload
                        .submit(
                            batch_id,
                            vec![(
                                transfer_id,
                                remote_path,
                                local_path,
                                total_bytes,
                                remote_directories,
                            )],
                        )
                        .await
                }
            };
            if let Err(error) = result {
                tracing::debug!(tab_id = %tab_id, %error, "SFTP transfer intent could not be requeued after reconnect");
                if sftp_submission_was_interrupted(&error) {
                    retry_required = true;
                } else if transfer_intent_is_current(
                    &state,
                    tab_id,
                    profile_id,
                    attempt_id,
                    &intent_for_recheck,
                ) {
                    let _ = with_sftp_transfer_intent_for_tab(&state, tab_id, |terminal| {
                        terminal.sftp.finish_transfer(
                            transfer_id,
                            SftpTransferPhase::Failed,
                            "Transfer retry request was rejected".to_owned(),
                        );
                        Ok(())
                    });
                }
            } else {
                submitted.insert(transfer_id);
            }
        }
        if retry_required {
            return;
        }
        let replay_complete = state.lock().is_ok_and(|mut app| {
            let Some(terminal) = app.terminal_mut(tab_id) else {
                return false;
            };
            if terminal.profile_id() != Some(profile_id)
                || terminal.ssh_route().and_then(|(_, attempt)| attempt) != Some(attempt_id)
                || !terminal.sftp.reconnect_replay_pending
            {
                return false;
            }
            if terminal.sftp.transfer_intent_revision != revision {
                return false;
            }
            terminal.sftp.reconnect_replay_pending = false;
            true
        });
        if replay_complete {
            return;
        }
    }
}

fn prepare_selected_remote_downloads(
    state: &Arc<Mutex<AppState>>,
    router: &WindowRouter,
    window_id: Uuid,
) -> Result<PreparedRemoteDownloads> {
    let tab_id = router.active_tab(window_id).context("no active SFTP tab")?;
    let filters = active_sftp_transfer_filter_patterns(state)?;
    with_sftp_terminal_for_tab(state, tab_id, |terminal| {
        let entries = terminal
            .sftp
            .entries
            .iter()
            .filter(|entry| terminal.sftp.selected.contains(&entry.path))
            .cloned()
            .collect::<Vec<_>>();
        if entries.is_empty() {
            anyhow::bail!("no remote files or folders are selected");
        }
        let local_directory = PathBuf::from(&terminal.sftp.local.path);
        Ok(PreparedRemoteDownloads {
            tab_id,
            entries,
            local_directory,
            filters,
        })
    })
}

fn queue_selected_remote_downloads(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    prepared: PreparedRemoteDownloads,
) {
    let PreparedRemoteDownloads {
        tab_id,
        entries,
        local_directory,
        filters,
    } = prepared;
    let Some(generation) = submission_generation(&state, tab_id) else {
        return;
    };
    runtime.spawn(async move {
        let mut queued = 0_usize;
        let mut failed = 0_usize;
        let mut skipped = 0_usize;
        let mut first_issue = None;
        let batch_id = Uuid::new_v4();
        for entry in entries {
            if submission_generation(&state, tab_id) != Some(generation) {
                return;
            }
            let transfer_id = Uuid::new_v4();
            let name = entry.name.clone();
            let remote_path = entry.path.clone();
            let local_path = local_directory.join(&name);
            let filtered = ax_ssh::sftp::transfer_name_matches_filter(&name, &filters);
            if filtered {
                skipped = skipped.saturating_add(1);
                let _ = with_sftp_terminal_for_tab(&state, tab_id, |terminal| {
                    terminal.sftp.queue_transfer_event(
                        transfer_id,
                        name.clone(),
                        entry.size,
                        false,
                        Some(local_path.clone()),
                        Some(remote_path.clone()),
                    )?;
                    terminal.sftp.finish_transfer(
                        transfer_id,
                        SftpTransferPhase::Skipped,
                        "Excluded by transfer filter".to_owned(),
                    );
                    Ok(())
                });
                dispatch_active_snapshot(&ui, &state);
                continue;
            }
            if entry.is_symlink {
                failed = failed.saturating_add(1);
                first_issue.get_or_insert_with(|| "symbolic links cannot be downloaded".to_owned());
                let _ = with_sftp_terminal_for_tab(&state, tab_id, |terminal| {
                    terminal.sftp.queue_transfer_event(
                        transfer_id,
                        name.clone(),
                        entry.size,
                        false,
                        Some(local_path.clone()),
                        Some(remote_path.clone()),
                    )?;
                    terminal.sftp.finish_transfer(
                        transfer_id,
                        SftpTransferPhase::Failed,
                        "Symbolic links cannot be downloaded".to_owned(),
                    );
                    Ok(())
                });
                dispatch_active_snapshot(&ui, &state);
                continue;
            }
            let (reserved, connected) = loop {
                if submission_generation(&state, tab_id) != Some(generation) {
                    return;
                }
                match with_sftp_transfer_intent_for_tab(&state, tab_id, |terminal| {
                    if terminal.sftp.submission_generation != generation {
                        anyhow::bail!("SFTP batch was cancelled");
                    }
                    if !terminal.sftp.reserve_download_root(transfer_id) {
                        return Ok((false, terminal.connected));
                    }
                    let row = if entry.is_dir {
                        terminal.sftp.begin_download_discovery(
                            transfer_id,
                            name.clone(),
                            local_path.clone(),
                            remote_path.clone(),
                        )
                    } else {
                        terminal.sftp.queue_transfer_event(
                            transfer_id,
                            name.clone(),
                            entry.size,
                            false,
                            Some(local_path.clone()),
                            Some(remote_path.clone()),
                        )
                    };
                    if let Err(error) = row {
                        terminal
                            .sftp
                            .finish_download_root(transfer_id, 0, 0, 0, 0, false, None);
                        return Err(error);
                    }
                    terminal.sftp.register_download_root_intent(
                        transfer_id,
                        Some(batch_id),
                        remote_path.clone(),
                        local_directory.clone(),
                        filters.clone(),
                        entry.is_dir,
                    );
                    Ok((true, terminal.connected))
                }) {
                    Ok((true, connected)) => break (true, connected),
                    Ok((false, _)) => {
                        dispatch_active_snapshot(&ui, &state);
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Err(error) => {
                        failed = failed.saturating_add(1);
                        first_issue.get_or_insert_with(|| error.to_string());
                        break (false, false);
                    }
                }
            };
            if !reserved {
                continue;
            }
            dispatch_active_snapshot(&ui, &state);
            if !connected {
                if sftp_tab_is_reconnecting(&state, tab_id) {
                    queued = queued.saturating_add(1);
                    continue;
                }
                failed = failed.saturating_add(1);
                first_issue.get_or_insert_with(|| "SFTP session is no longer connected".to_owned());
                let _ = with_sftp_transfer_intent_for_tab(&state, tab_id, |terminal| {
                    terminal.sftp.finish_transfer(
                        transfer_id,
                        SftpTransferPhase::Failed,
                        "SFTP session is no longer connected".to_owned(),
                    );
                    Ok(())
                });
                continue;
            }
            if sftp_tab_has_replay_pending(&state, tab_id) {
                queued = queued.saturating_add(1);
                continue;
            }
            let submitter = with_sftp_transfer_intent_for_tab(&state, tab_id, |terminal| {
                if terminal.sftp.submission_generation != generation {
                    anyhow::bail!("SFTP batch was cancelled");
                }
                if !terminal.connected || terminal.sftp.reconnect_replay_pending {
                    anyhow::bail!("SFTP session is no longer connected");
                }
                terminal
                    .worker
                    .as_ref()
                    .context("SFTP tab has no worker")?
                    .sftp_download_submitter()
            });
            let result = match submitter {
                Ok(submitter) => {
                    submitter
                        .for_batch(Some(batch_id))
                        .submit(
                            transfer_id,
                            remote_path.clone(),
                            local_directory.clone(),
                            filters.clone(),
                            entry.is_dir,
                        )
                        .await
                }
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                if sftp_submission_was_interrupted(&error)
                    || sftp_tab_is_reconnecting(&state, tab_id)
                    || sftp_tab_has_replay_pending(&state, tab_id)
                {
                    queued = queued.saturating_add(1);
                    continue;
                }
                let _ = with_sftp_transfer_intent_for_tab(&state, tab_id, |terminal| {
                    terminal.sftp.finish_transfer(
                        transfer_id,
                        SftpTransferPhase::Failed,
                        "Download request could not be queued".to_owned(),
                    );
                    Ok(())
                });
                failed = failed.saturating_add(1);
                first_issue.get_or_insert_with(|| error.to_string());
            } else {
                queued = queued.saturating_add(1);
            }
            dispatch_active_snapshot(&ui, &state);
        }
        if failed > 0 || skipped > 0 {
            let detail = first_issue.unwrap_or_else(|| "Some entries were skipped".to_owned());
            set_status(
                &ui,
                &format!(
                    "Queued {queued} entries; {failed} failed and {skipped} skipped. {detail}"
                ),
            );
        }
        dispatch_active_snapshot(&ui, &state);
    });
}

async fn queue_upload_file_for_generation(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    batch_id: Uuid,
    file: PreparedLocalUpload,
    generation: Option<u64>,
) -> Result<()> {
    let (submitter, file, waiting_for_replay) =
        with_sftp_transfer_intent_for_tab(state, tab_id, |terminal| {
            if generation
                .is_some_and(|generation| terminal.sftp.submission_generation != generation)
            {
                anyhow::bail!("SFTP batch was cancelled");
            }
            let name = file
                .remote_path
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_owned();
            terminal.sftp.queue_upload_transfer(
                file.id,
                name,
                file.size,
                file.local_path.clone(),
                file.remote_path.clone(),
            )?;
            terminal.sftp.register_upload_intent(
                file.id,
                batch_id,
                file.remote_path.clone(),
                file.local_path.clone(),
                file.size,
                file.remote_directories.clone(),
            );
            let waiting_for_replay = terminal.sftp.reconnect_replay_pending;
            if !terminal.connected || waiting_for_replay {
                return Ok((None, file, waiting_for_replay));
            }
            let submitter = terminal
                .worker
                .as_ref()
                .context("SFTP tab has no worker")?
                .sftp_upload_submitter()?;
            Ok((Some(submitter), file, false))
        })?;
    let Some(submitter) = submitter else {
        let transfer_id = file.id;
        return if waiting_for_replay || sftp_tab_is_reconnecting(state, tab_id) {
            Ok(())
        } else {
            let _ = with_sftp_transfer_intent_for_tab(state, tab_id, |terminal| {
                terminal.sftp.finish_transfer(
                    transfer_id,
                    SftpTransferPhase::Failed,
                    "SFTP session is no longer connected".to_owned(),
                );
                Ok(())
            });
            anyhow::bail!("SFTP session is no longer connected")
        };
    };
    let transfer_id = file.id;
    if let Err(error) = submitter
        .submit(
            batch_id,
            vec![(
                file.id,
                file.remote_path,
                file.local_path,
                file.size,
                file.remote_directories,
            )],
        )
        .await
    {
        if sftp_submission_was_interrupted(&error)
            || sftp_tab_is_reconnecting(state, tab_id)
            || sftp_tab_has_replay_pending(state, tab_id)
        {
            return Ok(());
        }
        with_sftp_transfer_intent_for_tab(state, tab_id, |terminal| {
            terminal.sftp.finish_transfer(
                transfer_id,
                SftpTransferPhase::Failed,
                "Upload request was rejected".to_owned(),
            );
            Ok(())
        })?;
        return Err(error);
    }
    Ok(())
}

pub(super) fn queue_local_upload_paths(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    batch_id: Uuid,
    remote_directory: String,
    paths: Vec<PathBuf>,
) {
    let Some(generation) = submission_generation(&state, tab_id) else {
        return;
    };
    let state_for_task = state.clone();
    runtime.spawn(async move {
        let filters = match active_sftp_transfer_filter_patterns(&state_for_task) {
            Ok(filters) => filters,
            Err(error) => {
                set_status(&ui, &format!("Cannot read SFTP upload filters: {error}"));
                return;
            }
        };
        let (file_tx, mut file_rx) = tokio::sync::mpsc::channel(32);
        let discovery = tokio::task::spawn_blocking(move || {
            discover_local_uploads_streaming(paths, &remote_directory, &filters, &file_tx)
        });
        let mut file_count = 0_usize;
        let mut failed_count = 0_usize;
        let mut first_failure = None;
        let mut total_bytes = 0_u64;
        let mut cancellation_check = tokio::time::interval(Duration::from_millis(100));
        loop {
            let file = tokio::select! {
                file = file_rx.recv() => match file { Some(file) => file, None => break },
                _ = cancellation_check.tick() => {
                    if submission_generation(&state_for_task, tab_id) != Some(generation) { break; }
                    continue;
                }
            };
            if submission_generation(&state_for_task, tab_id) != Some(generation) {
                break;
            }
            total_bytes = total_bytes.saturating_add(file.size);
            file_count = file_count.saturating_add(1);
            if !sftp_tab_is_currently_connected(&state_for_task, tab_id)
                && !sftp_tab_is_reconnecting(&state_for_task, tab_id)
            {
                failed_count = failed_count.saturating_add(1);
                first_failure
                    .get_or_insert_with(|| "SFTP session is no longer connected".to_owned());
                continue;
            }
            if let Err(error) = queue_upload_file_for_generation(
                &state_for_task,
                tab_id,
                batch_id,
                file,
                Some(generation),
            )
            .await
            {
                failed_count = failed_count.saturating_add(1);
                first_failure.get_or_insert_with(|| error.to_string());
            }
            dispatch_active_snapshot(&ui, &state_for_task);
        }
        drop(file_rx);
        let discovery = discovery.await;
        if submission_generation(&state_for_task, tab_id) != Some(generation) {
            return;
        }
        match discovery {
            Ok(Ok(summary)) if file_count > 0 => {
                log_sftp_drag_drop("upload-queued", Some(file_count), Some(total_bytes));
                if summary.skipped_entries > 0 || failed_count > 0 {
                    let detail = summary
                        .first_issue
                        .or(first_failure)
                        .unwrap_or_else(|| "some files could not be queued".to_owned());
                    set_status(
                        &ui,
                        &format!(
                            "Discovered {} files; {} failed or skipped. First issue: {detail}",
                            summary.discovered_files,
                            summary.skipped_entries.saturating_add(failed_count)
                        ),
                    );
                }
            }
            Ok(Ok(summary)) if summary.skipped_entries > 0 => {
                let detail = summary
                    .first_issue
                    .unwrap_or_else(|| "some local entries could not be read".to_owned());
                set_status(
                    &ui,
                    &format!(
                        "No files queued; skipped {} entries. First issue: {detail}",
                        summary.skipped_entries
                    ),
                );
            }
            Ok(Ok(_)) => set_status(&ui, "Selected upload contains no eligible regular files"),
            Ok(Err(error)) => {
                log_sftp_drag_drop("local-file-rejected", Some(file_count), Some(total_bytes));
                set_status(&ui, &format!("Cannot finish upload discovery: {error}"));
            }
            Err(error) => {
                log_sftp_drag_drop(
                    "inspection-task-failed",
                    Some(file_count),
                    Some(total_bytes),
                );
                set_status(&ui, &format!("Dropped upload task failed: {error}"));
            }
        }
        dispatch_active_snapshot(&ui, &state_for_task);
    });
}

pub(super) fn wire_selected_transfer_actions(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    for action in [
        ax_ssh::sftp::SftpTransferAction::Pause,
        ax_ssh::sftp::SftpTransferAction::Resume,
        ax_ssh::sftp::SftpTransferAction::Cancel,
    ] {
        let state = state.clone();
        let router = window_router.clone();
        let weak = ui.as_weak();
        let callback = move || {
            sync_window_active(&router, window_id, &state);
            let result = with_window_sftp_transfer_intent(&state, &router, window_id, |terminal| {
                request_all_transfer_actions(terminal, action)
            });
            match result {
                Ok(()) => dispatch_active_snapshot(&weak, &state),
                Err(error) => set_status(&weak, &format!("Cannot control SFTP transfers: {error}")),
            }
        };
        match action {
            ax_ssh::sftp::SftpTransferAction::Pause => ui.on_pause_all_sftp_transfers(callback),
            ax_ssh::sftp::SftpTransferAction::Resume => ui.on_resume_all_sftp_transfers(callback),
            ax_ssh::sftp::SftpTransferAction::Cancel => ui.on_cancel_all_sftp_transfers(callback),
        }
    }

    let ui_for_pause = ui.as_weak();
    let state_for_pause = state.clone();
    let router_for_pause = window_router.clone();
    ui.on_pause_selected_sftp_transfers(move || {
        log_ui_action("sftp.pause-selected-transfers");
        sync_window_active(&router_for_pause, window_id, &state_for_pause);
        let result =
            with_window_sftp_terminal(&state_for_pause, &router_for_pause, window_id, |terminal| {
                let transfer_ids = terminal
                    .sftp
                    .selected_transfer_ids_for_active_page()
                    .into_iter()
                    .filter(|id| terminal.sftp.transfer_is_pausable(*id))
                    .collect::<Vec<_>>();
                request_selected_transfer_actions(
                    terminal,
                    transfer_ids,
                    SelectedTransferAction::Pause,
                )
            });
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_pause, &state_for_pause),
            Err(error) => set_status(
                &ui_for_pause,
                &format!("Cannot pause selected SFTP transfers: {error}"),
            ),
        }
    });

    let ui_for_resume = ui.as_weak();
    let state_for_resume = state.clone();
    let router_for_resume = window_router.clone();
    ui.on_resume_selected_sftp_transfers(move || {
        log_ui_action("sftp.resume-selected-transfers");
        sync_window_active(&router_for_resume, window_id, &state_for_resume);
        let result = with_window_sftp_terminal(
            &state_for_resume,
            &router_for_resume,
            window_id,
            |terminal| {
                let transfer_ids = terminal
                    .sftp
                    .selected_transfer_ids_for_active_page()
                    .into_iter()
                    .filter(|id| terminal.sftp.transfer_is_resumable(*id))
                    .collect::<Vec<_>>();
                request_selected_transfer_actions(
                    terminal,
                    transfer_ids,
                    SelectedTransferAction::Resume,
                )
            },
        );
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_resume, &state_for_resume),
            Err(error) => set_status(
                &ui_for_resume,
                &format!("Cannot resume selected SFTP transfers: {error}"),
            ),
        }
    });

    let ui_for_cancel = ui.as_weak();
    let state_for_cancel = state;
    let router_for_cancel = window_router;
    ui.on_cancel_selected_sftp_transfers(move || {
        log_ui_action("sftp.cancel-selected-transfers");
        sync_window_active(&router_for_cancel, window_id, &state_for_cancel);
        let result = with_window_sftp_transfer_intent(
            &state_for_cancel,
            &router_for_cancel,
            window_id,
            |terminal| {
                let transfer_ids = terminal.sftp.selected_transfer_ids_for_active_page();
                request_selected_transfer_actions(
                    terminal,
                    transfer_ids,
                    SelectedTransferAction::Cancel,
                )
            },
        );
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_cancel, &state_for_cancel),
            Err(error) => set_status(
                &ui_for_cancel,
                &format!("Cannot cancel selected SFTP transfers: {error}"),
            ),
        }
    });
}

#[derive(Clone, Copy)]
pub(super) enum SelectedTransferAction {
    Pause,
    Resume,
    Cancel,
}

pub(super) fn request_selected_transfer_actions(
    terminal: &mut TerminalTabState,
    transfer_ids: Vec<uuid::Uuid>,
    action: SelectedTransferAction,
) -> Result<()> {
    if transfer_ids.is_empty() {
        anyhow::bail!("no selected transfers support this action");
    }
    if matches!(action, SelectedTransferAction::Cancel)
        && (!terminal.connected || terminal.sftp.reconnect_replay_pending)
    {
        for transfer_id in transfer_ids {
            let _ = terminal.sftp.request_transfer_cancel(transfer_id);
            terminal.sftp.finish_transfer(
                transfer_id,
                SftpTransferPhase::Cancelled,
                "Cancelled".to_owned(),
            );
        }
        return Ok(());
    }
    let worker = terminal
        .worker
        .as_ref()
        .context("active SFTP tab has no worker")?;
    worker.request_sftp_transfer_action(
        match action {
            SelectedTransferAction::Pause => ax_ssh::sftp::SftpTransferAction::Pause,
            SelectedTransferAction::Resume => ax_ssh::sftp::SftpTransferAction::Resume,
            SelectedTransferAction::Cancel => ax_ssh::sftp::SftpTransferAction::Cancel,
        },
        false,
        transfer_ids.clone(),
    )?;
    for transfer_id in transfer_ids {
        match action {
            SelectedTransferAction::Pause => {
                let _ = terminal.sftp.request_transfer_pause(transfer_id);
            }
            SelectedTransferAction::Resume => {
                let _ = terminal.sftp.request_transfer_resume(transfer_id);
            }
            SelectedTransferAction::Cancel => {
                let _ = terminal.sftp.request_transfer_cancel(transfer_id);
            }
        }
    }
    Ok(())
}

pub(super) fn queue_remote_downloads(
    terminal: &mut TerminalTabState,
    entries: Vec<SftpEntry>,
    filter_patterns: &[String],
) -> Result<()> {
    if entries.is_empty() {
        anyhow::bail!("no remote files or folders are selected");
    }
    let local_directory = std::path::PathBuf::from(&terminal.sftp.local.path);
    let worker = terminal
        .worker
        .as_ref()
        .context("active SFTP tab has no worker")?;
    let mut accepted = 0_usize;
    let mut filtered = 0_usize;
    let batch_id = Uuid::new_v4();
    for entry in entries {
        if ax_ssh::sftp::transfer_name_matches_filter(&entry.name, filter_patterns) {
            filtered += 1;
            continue;
        }
        let transfer_id = uuid::Uuid::new_v4();
        if entry.is_symlink {
            terminal
                .sftp
                .queue_transfer(transfer_id, entry.name.clone(), entry.size)?;
            terminal.sftp.finish_transfer(
                transfer_id,
                SftpTransferPhase::Failed,
                "Symbolic links cannot be downloaded".to_owned(),
            );
            continue;
        }
        terminal.sftp.register_download_root_intent(
            transfer_id,
            Some(batch_id),
            entry.path.clone(),
            local_directory.clone(),
            filter_patterns.to_vec(),
            entry.is_dir,
        );
        match worker.request_open_sftp_file_in_batch(
            transfer_id,
            batch_id,
            entry.path,
            local_directory.clone(),
            filter_patterns.to_vec(),
            entry.is_dir,
        ) {
            Ok(()) => accepted += 1,
            Err(error) => {
                tracing::debug!(%error, "SFTP download request was rejected before the worker accepted it");
                terminal
                    .sftp
                    .queue_transfer(transfer_id, entry.name.clone(), entry.size)
                    .context("cannot record rejected SFTP download")?;
                terminal.sftp.finish_transfer(
                    transfer_id,
                    SftpTransferPhase::Failed,
                    "Download request was rejected".to_owned(),
                );
            }
        }
    }
    if accepted == 0 {
        if filtered > 0 {
            anyhow::bail!("all selected entries are excluded by SFTP transfer filters");
        }
        anyhow::bail!("no selected entries could be queued for download");
    }
    Ok(())
}

fn submission_generation(state: &Arc<Mutex<AppState>>, tab_id: Uuid) -> Option<u64> {
    state
        .lock()
        .ok()?
        .terminal(tab_id)
        .map(|terminal| terminal.sftp.submission_generation)
}

pub(super) fn request_all_transfer_actions(
    terminal: &mut TerminalTabState,
    action: ax_ssh::sftp::SftpTransferAction,
) -> Result<()> {
    use ax_ssh::sftp::SftpTransferAction;
    let ids = terminal
        .sftp
        .transfers
        .iter()
        .filter(|transfer| transfer.phase.cancellable())
        .map(|transfer| transfer.id)
        .collect::<Vec<_>>();
    let connected = terminal.connected && !terminal.sftp.reconnect_replay_pending;
    if connected {
        terminal
            .worker
            .as_ref()
            .context("active SFTP tab has no worker")?
            .request_sftp_transfer_action(action, true, ids.clone())?;
    }
    match action {
        SftpTransferAction::Pause => {
            terminal.sftp.transfers_paused = true;
            for id in ids {
                terminal.sftp.apply_transfer_control(id, true, true);
            }
        }
        SftpTransferAction::Resume => {
            terminal.sftp.transfers_paused = false;
            for id in ids {
                terminal.sftp.apply_transfer_control(id, false, false);
            }
        }
        SftpTransferAction::Cancel => {
            terminal.sftp.transfers_paused = false;
            terminal.sftp.submission_generation =
                terminal.sftp.submission_generation.wrapping_add(1);
            for id in ids {
                terminal.sftp.request_transfer_cancel(id);
                if !connected {
                    terminal.sftp.finish_transfer(
                        id,
                        SftpTransferPhase::Cancelled,
                        "Cancelled".to_owned(),
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
