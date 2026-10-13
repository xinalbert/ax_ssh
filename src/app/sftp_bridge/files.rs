//! SFTP files intent wiring and bounded application operations.

use super::*;

pub(super) fn wire_files(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let ui_for_remote_open = ui.as_weak();
    let state_for_remote_open = state.clone();
    let router_for_remote_open = window_router.clone();
    ui.on_open_remote_sftp_file(move |path| {
        log_ui_action("sftp.open-remote-file");
        sync_window_active(&router_for_remote_open, window_id, &state_for_remote_open);
        let filter_patterns = active_sftp_transfer_filter_patterns(&state_for_remote_open);
        let result = filter_patterns.and_then(|filter_patterns| {
            with_window_sftp_terminal(
                &state_for_remote_open,
                &router_for_remote_open,
                window_id,
                |terminal| {
                    let entry = terminal
                        .sftp
                        .entries
                        .iter()
                        .find(|entry| entry.path == path.as_str())
                        .cloned()
                        .context("remote entry is no longer visible")?;
                    queue_remote_downloads(terminal, vec![entry], &filter_patterns)?;
                    Ok(())
                },
            )
        });
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_remote_open, &state_for_remote_open),
            Err(error) => {
                set_status(
                    &ui_for_remote_open,
                    &format!("Cannot open remote file: {error}"),
                );
                dispatch_active_snapshot(&ui_for_remote_open, &state_for_remote_open);
            }
        }
    });

    let ui_for_remove = ui.as_weak();
    let state_for_remove = state.clone();
    let router_for_remove = window_router.clone();
    ui.on_remove_selected_remote_sftp(move || {
        log_ui_action("sftp.remove-selected-remote");
        sync_window_active(&router_for_remove, window_id, &state_for_remove);
        let result = with_window_sftp_terminal(
            &state_for_remove,
            &router_for_remove,
            window_id,
            |terminal| {
                let selected = terminal
                    .sftp
                    .entries
                    .iter()
                    .filter(|entry| terminal.sftp.selected.contains(&entry.path))
                    .cloned()
                    .collect::<Vec<_>>();
                if selected.is_empty() {
                    anyhow::bail!("no remote entries are selected");
                }
                let worker = terminal
                    .worker
                    .as_ref()
                    .context("active SSH terminal has no worker")?;
                for entry in selected {
                    worker.request_sftp_write(
                        Uuid::new_v4(),
                        ax_ssh::sftp::SftpWriteOperation::Remove {
                            path: entry.path,
                            directory: entry.is_dir,
                        },
                    )?;
                }
                terminal.sftp.selected.clear();
                Ok(())
            },
        );
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_remove, &state_for_remove),
            Err(error) => set_status(
                &ui_for_remove,
                &format!("Cannot delete remote entries: {error}"),
            ),
        }
    });

    let ui_for_save = ui.as_weak();
    let state_for_save = state.clone();
    let router_for_save = window_router.clone();
    ui.on_upload_edited_sftp_file(move || {
        log_ui_action("sftp.upload-edited-file");
        sync_window_active(&router_for_save, window_id, &state_for_save);
        match with_window_sftp_terminal(&state_for_save, &router_for_save, window_id, |terminal| {
            terminal.sftp.external_edits.request_upload()
        }) {
            Ok(()) => dispatch_active_snapshot(&ui_for_save, &state_for_save),
            Err(error) => set_status(&ui_for_save, &format!("Cannot upload edited file: {error}")),
        }
    });

    let ui_for_stop = ui.as_weak();
    let state_for_stop = state.clone();
    let router_for_stop = window_router.clone();
    ui.on_stop_editing_sftp_file(move || {
        sync_window_active(&router_for_stop, window_id, &state_for_stop);
        match with_window_sftp_terminal(&state_for_stop, &router_for_stop, window_id, |terminal| {
            terminal.sftp.external_edits.stop_current();
            terminal.sftp.status = "External editing stopped; local copy retained".into();
            Ok(())
        }) {
            Ok(()) => dispatch_active_snapshot(&ui_for_stop, &state_for_stop),
            Err(error) => set_status(&ui_for_stop, &format!("Cannot stop editing: {error}")),
        }
    });

    let ui_for_reveal = ui.as_weak();
    let state_for_reveal = state.clone();
    let router_for_reveal = window_router.clone();
    let runtime_for_reveal = runtime.clone();
    ui.on_reveal_sftp_edit_copy(move || {
        let path = with_window_sftp_terminal(
            &state_for_reveal,
            &router_for_reveal,
            window_id,
            |terminal| {
                let (_, path, _, _) = terminal.sftp.external_edits.presentation();
                if path.is_empty() {
                    anyhow::bail!("edit copy is not ready");
                }
                Ok(PathBuf::from(path))
            },
        );
        match path {
            Ok(path) => reveal_local_path(&runtime_for_reveal, ui_for_reveal.clone(), path),
            Err(error) => set_status(&ui_for_reveal, &format!("Cannot show edit copy: {error}")),
        }
    });

    let ui_for_auto = ui.as_weak();
    let state_for_auto = state.clone();
    let router_for_auto = window_router.clone();
    ui.on_toggle_editor_auto_upload_sftp(move |enabled| {
        log_ui_action("sftp.toggle-editor-auto-upload");
        sync_window_active(&router_for_auto, window_id, &state_for_auto);
        match with_window_sftp_terminal(&state_for_auto, &router_for_auto, window_id, |terminal| {
            terminal.sftp.external_edits.auto_upload = enabled;
            Ok(())
        }) {
            Ok(()) => dispatch_active_snapshot(&ui_for_auto, &state_for_auto),
            Err(error) => set_status(&ui_for_auto, &format!("Cannot change auto upload: {error}")),
        }
    });

    let ui_for_rename = ui.as_weak();
    let state_for_rename = state.clone();
    let router_for_rename = window_router.clone();
    ui.on_rename_remote_sftp(move |tab_id, path, new_name| {
        log_ui_action("sftp.rename-remote");
        sync_window_active(&router_for_rename, window_id, &state_for_rename);
        let result = with_window_sftp_rename_target(
            &state_for_rename,
            &router_for_rename,
            window_id,
            tab_id.as_str(),
            path.as_str(),
            |terminal, entry| {
                let new_name = new_name.trim().to_owned();
                if new_name.is_empty()
                    || new_name == "."
                    || new_name == ".."
                    || new_name.chars().count() > MAX_REMOTE_NAME_CHARS
                    || new_name.contains(['/', '\\'])
                    || new_name.chars().any(char::is_control)
                {
                    anyhow::bail!("remote name is invalid");
                }
                let parent = entry
                    .path
                    .rsplit_once('/')
                    .map(|(parent, _)| parent)
                    .unwrap_or("");
                let new_path = format!("{parent}/{new_name}");
                terminal
                    .worker
                    .as_ref()
                    .context("active SFTP tab has no worker")?
                    .request_sftp_write(
                        Uuid::new_v4(),
                        ax_ssh::sftp::SftpWriteOperation::Rename {
                            old_path: entry.path,
                            new_path,
                        },
                    )
            },
        );
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_rename, &state_for_rename),
            Err(error) => set_status(
                &ui_for_rename,
                &format!("Cannot rename remote entry: {error}"),
            ),
        }
    });

    let ui_for_edit = ui.as_weak();
    let state_for_edit = state.clone();
    let router_for_edit = window_router.clone();
    let runtime_for_edit = runtime.clone();
    ui.on_edit_remote_sftp(move || {
        log_ui_action("sftp.edit-remote");
        sync_window_active(&router_for_edit, window_id, &state_for_edit);
        let max_bytes = match state_for_edit.lock() {
            Ok(app) => app
                .sessions
                .settings
                .sftp_transfers
                .remote_edit_max_file_bytes(),
            Err(_) => {
                set_status(
                    &ui_for_edit,
                    "Cannot edit remote file: application state lock poisoned",
                );
                return;
            }
        };
        let result =
            with_window_sftp_terminal(&state_for_edit, &router_for_edit, window_id, |terminal| {
                let entries: Vec<_> = terminal
                    .sftp
                    .entries
                    .iter()
                    .filter(|entry| terminal.sftp.selected.contains(&entry.path))
                    .collect();
                if entries.len() != 1 || entries[0].is_dir || entries[0].is_symlink {
                    anyhow::bail!("select exactly one regular remote file to edit");
                }
                let path = entries[0].path.clone();
                let (id, local, download) = terminal.sftp.external_edits.open(path.clone())?;
                if download {
                    let queued = terminal
                        .worker
                        .as_ref()
                        .context("active SFTP tab has no worker")?
                        .request_sftp_write(
                            id,
                            ax_ssh::sftp::SftpWriteOperation::ReadEditFile { path, max_bytes },
                        );
                    if let Err(error) = queued {
                        terminal.sftp.external_edits.apply_result(
                            &ax_ssh::sftp::SftpWriteEvent::Failed {
                                operation_id: id,
                                message: format!("{error:#}"),
                            },
                        );
                        return Err(error);
                    }
                }
                Ok(local.is_some())
            });
        match result {
            Ok(reopen) => {
                if reopen
                    && let Some(tab_id) = router_for_edit.active_tab(window_id)
                    && let Err(error) = super::remote_edit::reopen_current(
                        &runtime_for_edit,
                        state_for_edit.clone(),
                        ui_for_edit.clone(),
                        tab_id,
                    )
                {
                    set_status(&ui_for_edit, &format!("Cannot reopen edit copy: {error}"));
                }
                dispatch_active_snapshot(&ui_for_edit, &state_for_edit);
            }
            Err(error) => set_status(&ui_for_edit, &format!("Cannot edit remote file: {error}")),
        }
    });

    let ui_for_local_open = ui.as_weak();
    let state_for_local_open = state.clone();
    let runtime_for_local_open = runtime.clone();
    let router_for_local_open = window_router.clone();
    ui.on_open_local_sftp_file(move |path| {
        log_ui_action("sftp.open-local-file");
        sync_window_active(&router_for_local_open, window_id, &state_for_local_open);
        let tab_id = router_for_local_open.active_tab(window_id);
        let request = tab_id.context("no active SFTP tab").and_then(|tab_id| {
            prepare_local_file_open(&state_for_local_open, tab_id, path.as_str())
        });
        match request {
            Ok(request) => {
                dispatch_active_snapshot(&ui_for_local_open, &state_for_local_open);
                open_local_file(
                    &runtime_for_local_open,
                    state_for_local_open.clone(),
                    ui_for_local_open.clone(),
                    request,
                );
            }
            Err(error) => {
                set_status(
                    &ui_for_local_open,
                    &format!("Cannot open local file: {error}"),
                );
                dispatch_active_snapshot(&ui_for_local_open, &state_for_local_open);
            }
        }
    });

    let ui_for_reveal_local = ui.as_weak();
    let state_for_reveal_local = state.clone();
    let runtime_for_reveal_local = runtime.clone();
    let router_for_reveal_local = window_router.clone();
    ui.on_reveal_local_sftp_file(move |path| {
        log_ui_action("sftp.reveal-local-file");
        sync_window_active(&router_for_reveal_local, window_id, &state_for_reveal_local);
        let result = router_for_reveal_local
            .active_tab(window_id)
            .context("no active SFTP tab")
            .and_then(|tab_id| {
                prepare_local_entry_reveal(&state_for_reveal_local, tab_id, path.as_str())
            });
        match result {
            Ok(path) => {
                reveal_local_path(&runtime_for_reveal_local, ui_for_reveal_local.clone(), path)
            }
            Err(error) => set_status(
                &ui_for_reveal_local,
                &format!("Cannot show local file: {error}"),
            ),
        }
    });
}

pub(super) struct LocalOpenRequest {
    tab_id: uuid::Uuid,
    request_id: u64,
    directory: String,
    entry: LocalDirectoryEntry,
    max_file_bytes: u64,
}

pub(super) fn prepare_local_entry_reveal(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    requested_path: &str,
) -> Result<PathBuf> {
    with_sftp_terminal_for_tab(state, tab_id, |terminal| {
        let entry = terminal
            .sftp
            .local
            .entries
            .iter()
            .find(|entry| entry.path == requested_path)
            .context("local entry is no longer visible")?;
        if entry.is_symlink {
            anyhow::bail!("symbolic links cannot be shown from SFTP in this version");
        }
        let directory = Path::new(&terminal.sftp.local.path);
        let path = PathBuf::from(&entry.path);
        if path.parent() != Some(directory) {
            anyhow::bail!("local entry is outside the current directory snapshot");
        }
        Ok(path)
    })
}

pub(super) fn reveal_local_path(runtime: &Handle, ui: slint::Weak<AppWindow>, path: PathBuf) {
    runtime.spawn(async move {
        let revealed = tokio::time::timeout(
            LOCAL_OPEN_TIMEOUT,
            tokio::task::spawn_blocking(move || reveal_local_path_blocking(&path)),
        )
        .await;
        match revealed {
            Ok(Ok(Ok(()))) => set_status(&ui, "Opened local file location"),
            Ok(Ok(Err(error))) => {
                set_status(&ui, &format!("Cannot show local file location: {error}"));
            }
            Ok(Err(error)) => {
                set_status(&ui, &format!("Local file location task failed: {error}"));
            }
            Err(_) => {
                set_status(&ui, "Local file location task timed out");
            }
        }
    });
}

#[cfg(target_os = "macos")]
pub(super) fn reveal_local_path_blocking(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("cannot inspect local path {path:?}"))?;
    if metadata.file_type().is_symlink() {
        anyhow::bail!("local path is a symbolic link");
    }
    std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
        .with_context(|| format!("cannot reveal local path {path:?}"))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub(super) fn reveal_local_path_blocking(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("cannot inspect local path {path:?}"))?;
    if metadata.file_type().is_symlink() {
        anyhow::bail!("local path is a symbolic link");
    }
    let parent = path
        .parent()
        .context("local path has no parent directory")?;
    open::that_detached(parent).with_context(|| format!("cannot open local directory {parent:?}"))
}

pub(super) fn prepare_local_file_open(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    requested_path: &str,
) -> Result<LocalOpenRequest> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let max_file_bytes = app
        .sessions
        .settings
        .sftp_transfers
        .local_open_max_file_bytes();
    let terminal = app.terminal_mut(tab_id).context("no active SFTP tab")?;
    if !terminal.is_sftp() {
        anyhow::bail!("local files are available only in an SFTP tab");
    }
    let entry = terminal
        .sftp
        .local
        .entries
        .iter()
        .find(|entry| entry.path == requested_path)
        .cloned()
        .context("local entry is no longer visible")?;
    if entry.is_dir {
        anyhow::bail!("directories must be opened by navigation");
    }
    if entry.is_symlink {
        anyhow::bail!("symbolic links cannot be opened from SFTP in this version");
    }
    terminal.sftp.local.status = format!("Opening {}...", entry.name);
    Ok(LocalOpenRequest {
        tab_id,
        request_id: terminal.sftp.local.request_id,
        directory: terminal.sftp.local.path.clone(),
        entry,
        max_file_bytes,
    })
}

pub(super) fn open_local_file(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    request: LocalOpenRequest,
) {
    runtime.spawn(async move {
        let directory = request.directory.clone();
        let entry = request.entry.clone();
        let validated = tokio::time::timeout(
            LOCAL_OPEN_TIMEOUT,
            tokio::task::spawn_blocking(move || validate_local_file_for_open(&directory, &entry)),
        )
        .await;
        let validated = match validated {
            Ok(Ok(Ok(validated))) => validated,
            Ok(Ok(Err(error))) => {
                finish_local_file_open(
                    &state,
                    &request,
                    format!("Cannot open {}: {error}", request.entry.name),
                );
                dispatch_active_snapshot(&ui, &state);
                return;
            }
            Ok(Err(error)) => {
                finish_local_file_open(
                    &state,
                    &request,
                    format!("Local file check failed: {error}"),
                );
                dispatch_active_snapshot(&ui, &state);
                return;
            }
            Err(_) => {
                finish_local_file_open(&state, &request, "Local file check timed out".to_owned());
                dispatch_active_snapshot(&ui, &state);
                return;
            }
        };

        if !local_open_snapshot_is_current(&state, &request) {
            return;
        }
        let max_file_bytes = request.max_file_bytes;
        let cancellation = Arc::new(AtomicBool::new(false));
        let _cancel_guard = LocalSnapshotCancelGuard(cancellation.clone());
        let snapshot = tokio::time::timeout(
            LOCAL_SNAPSHOT_TIMEOUT,
            tokio::task::spawn_blocking(move || {
                ax_ssh::sftp::snapshot_local_file_for_open(
                    validated.file,
                    &validated.name,
                    max_file_bytes,
                    &cancellation,
                )
            }),
        )
        .await;
        let target = match snapshot {
            Ok(Ok(Ok(target))) => target,
            Ok(Ok(Err(error))) => {
                finish_local_file_open(
                    &state,
                    &request,
                    format!("Cannot snapshot {}: {error}", request.entry.name),
                );
                dispatch_active_snapshot(&ui, &state);
                return;
            }
            Ok(Err(error)) => {
                finish_local_file_open(
                    &state,
                    &request,
                    format!("Local snapshot task failed: {error}"),
                );
                dispatch_active_snapshot(&ui, &state);
                return;
            }
            Err(_) => {
                finish_local_file_open(&state, &request, "Local snapshot timed out".to_owned());
                dispatch_active_snapshot(&ui, &state);
                return;
            }
        };
        let opened = tokio::time::timeout(
            LOCAL_OPEN_TIMEOUT,
            tokio::task::spawn_blocking(move || open::that_detached(target)),
        )
        .await;
        let status = match opened {
            Ok(Ok(Ok(()))) => format!("Opened {}", request.entry.name),
            Ok(Ok(Err(error))) => format!("Cannot open {}: {error}", request.entry.name),
            Ok(Err(error)) => format!("Local file opener failed: {error}"),
            Err(_) => "Local file opener timed out".to_owned(),
        };
        finish_local_file_open(&state, &request, status);
        dispatch_active_snapshot(&ui, &state);
    });
}

pub(super) fn local_open_snapshot_is_current(
    state: &Arc<Mutex<AppState>>,
    request: &LocalOpenRequest,
) -> bool {
    let Ok(app) = state.lock() else {
        return false;
    };
    app.terminal(request.tab_id).is_some_and(|terminal| {
        terminal.is_sftp()
            && terminal.sftp.local.request_id == request.request_id
            && terminal.sftp.local.path == request.directory
            && terminal
                .sftp
                .local
                .entries
                .iter()
                .any(|entry| entry.path == request.entry.path && !entry.is_dir && !entry.is_symlink)
    })
}

pub(super) fn finish_local_file_open(
    state: &Arc<Mutex<AppState>>,
    request: &LocalOpenRequest,
    status: String,
) {
    let Ok(mut app) = state.lock() else {
        return;
    };
    let Some(terminal) = app.terminal_mut(request.tab_id) else {
        return;
    };
    if terminal.is_sftp()
        && terminal.sftp.local.request_id == request.request_id
        && terminal.sftp.local.path == request.directory
    {
        terminal.sftp.local.status = status;
    }
}

pub(super) fn with_window_sftp_rename_target<T>(
    state: &Arc<Mutex<AppState>>,
    router: &WindowRouter,
    window_id: Uuid,
    tab_id: &str,
    path: &str,
    action: impl FnOnce(&mut TerminalTabState, ax_ssh::sftp::SftpEntry) -> Result<T>,
) -> Result<T> {
    let tab_id = tab_id.parse::<Uuid>().context("invalid SFTP rename tab")?;
    if router.active_tab(window_id) != Some(tab_id) {
        anyhow::bail!("SFTP rename tab is no longer active in this window");
    }
    with_sftp_terminal_for_tab(state, tab_id, |terminal| {
        if terminal.sftp.loading || !terminal.sftp.open {
            anyhow::bail!("remote directory is no longer ready for rename");
        }
        if terminal.sftp.selected.len() != 1 || !terminal.sftp.selected.contains(path) {
            anyhow::bail!("select only the original remote entry to rename");
        }
        let entry = terminal
            .sftp
            .entries
            .iter()
            .find(|entry| entry.path == path)
            .cloned()
            .context("remote entry is no longer visible")?;
        action(terminal, entry)
    })
}
