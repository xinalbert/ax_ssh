//! SFTP transfers intent wiring and bounded application operations.

use super::*;

pub(super) fn wire_transfers(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let ui_for_selected_remote_download = ui.as_weak();
    let state_for_selected_remote_download = state.clone();
    let router_for_selected_remote_download = window_router.clone();
    ui.on_download_selected_remote_sftp(move || {
        log_ui_action("sftp.download-selected-remote");
        sync_window_active(
            &router_for_selected_remote_download,
            window_id,
            &state_for_selected_remote_download,
        );
        let filter_patterns =
            active_sftp_transfer_filter_patterns(&state_for_selected_remote_download);
        let result = filter_patterns.and_then(|filter_patterns| {
            with_window_sftp_terminal(
                &state_for_selected_remote_download,
                &router_for_selected_remote_download,
                window_id,
                |terminal| {
                    let selected = terminal
                        .sftp
                        .entries
                        .iter()
                        .filter(|entry| terminal.sftp.selected.contains(&entry.path))
                        .cloned()
                        .collect::<Vec<_>>();
                    queue_remote_downloads(terminal, selected, &filter_patterns)
                },
            )
        });
        match result {
            Ok(()) => dispatch_active_snapshot(
                &ui_for_selected_remote_download,
                &state_for_selected_remote_download,
            ),
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
    ui.on_resolve_sftp_upload_conflict(move |id, choice, apply_to_batch| {
        log_ui_action("sftp.resolve-upload-conflict");
        let Ok(transfer_id) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let choice = match choice.as_str() {
            "skip" => ax_ssh::sftp::SftpUploadConflictChoice::Skip,
            "overwrite" => ax_ssh::sftp::SftpUploadConflictChoice::Overwrite,
            "keep-both" => ax_ssh::sftp::SftpUploadConflictChoice::KeepBoth,
            _ => return,
        };
        let result = with_window_sftp_terminal(
            &state_for_conflict,
            &router_for_conflict,
            window_id,
            |terminal| {
                if terminal
                    .sftp
                    .upload_conflicts
                    .front()
                    .map(|item| item.transfer_id)
                    != Some(transfer_id)
                {
                    anyhow::bail!("upload conflict is no longer active");
                }
                terminal
                    .worker
                    .as_ref()
                    .context("SFTP tab has no worker")?
                    .request_resolve_sftp_upload_conflict(transfer_id, choice, apply_to_batch)?;
                terminal
                    .sftp
                    .resolve_upload_conflict(transfer_id, apply_to_batch);
                Ok(())
            },
        );
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_conflict, &state_for_conflict),
            Err(error) => set_status(
                &ui_for_conflict,
                &format!("Cannot resolve upload conflict: {error}"),
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
            with_window_sftp_terminal(
                &state_for_transfer_cancel,
                &router_for_transfer_cancel,
                window_id,
                |terminal| {
                    if !terminal.sftp.transfer_is_cancellable(transfer_id) {
                        anyhow::bail!("SFTP transfer is no longer cancellable");
                    }
                    terminal
                        .worker
                        .as_ref()
                        .context("active SFTP tab has no worker")?
                        .request_cancel_sftp_transfer(transfer_id)?;
                    if !terminal.sftp.request_transfer_cancel(transfer_id) {
                        anyhow::bail!("SFTP transfer changed before cancellation was recorded");
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

pub(super) fn queue_upload_batch_for_tab(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    batch_id: Uuid,
    files: Vec<PreparedLocalUpload>,
) -> Result<()> {
    with_sftp_terminal_for_tab(state, tab_id, |terminal| {
        if !terminal.sftp.can_queue_uploads(files.len()) {
            anyhow::bail!("SFTP transfer history has no room for this upload batch");
        }
        for file in &files {
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
        }
        let requests = files
            .iter()
            .map(|file| {
                (
                    file.id,
                    file.remote_path.clone(),
                    file.local_path.clone(),
                    file.size,
                    file.remote_directories.clone(),
                )
            })
            .collect();
        let result = terminal
            .worker
            .as_ref()
            .context("SFTP tab has no worker")
            .and_then(|worker| worker.request_open_sftp_upload_batch(batch_id, requests));
        if let Err(error) = result {
            for file in &files {
                terminal.sftp.finish_transfer(
                    file.id,
                    SftpTransferPhase::Failed,
                    "Upload request was rejected".to_owned(),
                );
            }
            return Err(error);
        }
        Ok(())
    })
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
    let state_for_task = state.clone();
    runtime.spawn(async move {
        let max_file_bytes = match sftp_max_file_bytes_for_tab(&state_for_task, tab_id) {
            Ok(limit) => limit,
            Err(error) => {
                set_status(&ui, &format!("Cannot read SFTP transfer limits: {error}"));
                return;
            }
        };
        let filters = match active_sftp_transfer_filter_patterns(&state_for_task) {
            Ok(filters) => filters,
            Err(error) => {
                set_status(&ui, &format!("Cannot read SFTP upload filters: {error}"));
                return;
            }
        };
        let read = tokio::task::spawn_blocking(move || {
            discover_local_uploads(paths, &remote_directory, &filters, max_file_bytes)
        })
        .await;
        let files = match read {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => {
                log_sftp_drag_drop("local-file-rejected", None, None);
                set_status(&ui, &format!("Cannot prepare upload batch: {error}"));
                return;
            }
            Err(error) => {
                log_sftp_drag_drop("inspection-task-failed", None, None);
                set_status(&ui, &format!("Dropped upload task failed: {error}"));
                return;
            }
        };
        let file_count = files.len();
        let total_bytes = files.iter().map(|file| file.size).sum();
        log_sftp_drag_drop("local-file-validated", Some(file_count), Some(total_bytes));
        let queued = queue_upload_batch_for_tab(&state_for_task, tab_id, batch_id, files);
        match queued {
            Ok(()) => {
                log_sftp_drag_drop("upload-queued", Some(file_count), Some(total_bytes));
                dispatch_active_snapshot(&ui, &state_for_task);
            }
            Err(error) => {
                log_sftp_drag_drop("upload-rejected", Some(file_count), Some(total_bytes));
                set_status(&ui, &format!("Cannot queue upload batch: {error}"));
                dispatch_active_snapshot(&ui, &state_for_task);
            }
        }
    });
}

pub(super) fn wire_selected_transfer_actions(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    window_router: WindowRouter,
    window_id: Uuid,
) {
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
        let result = with_window_sftp_terminal(
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
    let worker = terminal
        .worker
        .as_ref()
        .context("active SFTP tab has no worker")?;
    for transfer_id in &transfer_ids {
        match action {
            SelectedTransferAction::Pause => worker.request_pause_sftp_transfer(*transfer_id)?,
            SelectedTransferAction::Resume => worker.request_resume_sftp_transfer(*transfer_id)?,
            SelectedTransferAction::Cancel => worker.request_cancel_sftp_transfer(*transfer_id)?,
        }
    }
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
        if !entry.is_dir {
            terminal
                .sftp
                .queue_transfer(transfer_id, entry.name.clone(), entry.size)?;
        }
        match worker.request_open_sftp_file(
            transfer_id,
            entry.path,
            local_directory.clone(),
            filter_patterns.to_vec(),
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
