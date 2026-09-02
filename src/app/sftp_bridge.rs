use super::local_files::{
    LOCAL_DIRECTORY_PATH_LIMIT, LocalDirectoryEntry, read_local_directory,
    validate_local_file_for_open,
};
use super::*;
use std::path::PathBuf;

const LOCAL_DIRECTORY_TIMEOUT: Duration = Duration::from_secs(5);
const LOCAL_OPEN_TIMEOUT: Duration = Duration::from_secs(5);
const LOCAL_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const MAX_REMOTE_NAME_CHARS: usize = 512;

type SlintDataTransfer = slint::private_unstable_api::re_exports::DataTransfer;

fn local_file_drag_data(path: &str) -> SlintDataTransfer {
    let mut data = SlintDataTransfer::default();
    if path.len() <= LOCAL_DIRECTORY_PATH_LIMIT
        && !path.is_empty()
        && !path.chars().any(char::is_control)
    {
        data.set_plain_text(path.to_owned().into());
    }
    data
}

fn parse_dropped_local_path(text: &str) -> Result<PathBuf> {
    let raw = text
        .split(['\n', '\r'])
        .map(str::trim)
        .find(|value| !value.is_empty())
        .context("dropped data did not contain a local file path")?;
    if raw.len() > LOCAL_DIRECTORY_PATH_LIMIT || raw.chars().any(char::is_control) {
        anyhow::bail!("dropped local file path is invalid or too long");
    }
    let path = if let Some(uri) = raw.strip_prefix("file://") {
        let (authority, encoded_path) = uri.split_once('/').unwrap_or((uri, ""));
        if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
            anyhow::bail!("remote file URI hosts are not accepted");
        }
        let decoded = percent_decode_path(&format!("/{encoded_path}"))?;
        #[cfg(windows)]
        let decoded = decoded
            .strip_prefix('/')
            .filter(|value| value.as_bytes().get(1) == Some(&b':'))
            .unwrap_or(&decoded)
            .to_owned();
        PathBuf::from(decoded)
    } else {
        PathBuf::from(raw)
    };
    if path.as_os_str().is_empty() {
        anyhow::bail!("dropped local file path is empty");
    }
    Ok(path)
}

fn percent_decode_path(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                anyhow::bail!("invalid file URI escape");
            }
            let high = (bytes[index + 1] as char)
                .to_digit(16)
                .context("invalid file URI escape")?;
            let low = (bytes[index + 2] as char)
                .to_digit(16)
                .context("invalid file URI escape")?;
            decoded.push(((high << 4) | low) as u8);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).context("file URI is not valid UTF-8")
}

fn queue_sftp_write(
    state: &Arc<Mutex<AppState>>,
    operation: ax_ssh::sftp::SftpWriteOperation,
) -> Result<()> {
    with_active_sftp_terminal(state, |terminal| {
        let id = Uuid::new_v4();
        terminal
            .worker
            .as_ref()
            .context("active SFTP tab has no worker")?
            .request_sftp_write(id, operation)
    })
}

fn queue_local_upload_path(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    local_path: PathBuf,
) {
    let state_for_task = state.clone();
    runtime.spawn(async move {
        let read = tokio::task::spawn_blocking({
            let local_path = local_path.clone();
            move || {
                let metadata = std::fs::symlink_metadata(&local_path)
                    .with_context(|| format!("cannot inspect dropped local file {local_path:?}"))?;
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    anyhow::bail!("dropped path is not a regular local file");
                }
                if metadata.len() > ax_ssh::sftp::MAX_UPLOAD_BYTES {
                    anyhow::bail!("local file exceeds the upload size limit");
                }
                let name = local_path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .filter(|value| !value.is_empty())
                    .context("dropped local file has no valid name")?
                    .to_owned();
                Ok::<_, anyhow::Error>((name, metadata.len()))
            }
        })
        .await;
        let (name, total_bytes) = match read {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => {
                set_status(&ui, &format!("Cannot prepare dropped upload: {error}"));
                return;
            }
            Err(error) => {
                set_status(&ui, &format!("Dropped upload task failed: {error}"));
                return;
            }
        };
        let queued = with_active_sftp_terminal(&state_for_task, |terminal| {
            let remote_path = if terminal.sftp.path == "/" {
                format!("/{name}")
            } else {
                format!("{}/{}", terminal.sftp.path.trim_end_matches('/'), name)
            };
            let transfer_id = Uuid::new_v4();
            terminal
                .sftp
                .queue_transfer(transfer_id, name, total_bytes)?;
            terminal
                .worker
                .as_ref()
                .context("active SFTP tab has no worker")?
                .request_open_sftp_upload(transfer_id, remote_path, local_path, total_bytes)
        });
        match queued {
            Ok(()) => dispatch_active_snapshot(&ui, &state_for_task),
            Err(error) => set_status(&ui, &format!("Cannot queue dropped upload: {error}")),
        }
    });
}

pub(super) fn wire_sftp(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let ui_for_list = ui.as_weak();
    let state_for_list = state.clone();
    let router_for_list = window_router.clone();
    ui.on_list_sftp_directory(move |path| {
        log_ui_action("sftp.list-remote");
        sync_window_active(&router_for_list, window_id, &state_for_list);
        let result = queue_remote_navigation(
            &state_for_list,
            SftpNavigation::Direct,
            Some(path.as_str().to_owned()),
        );
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_list, &state_for_list),
            Err(error) => {
                set_status(&ui_for_list, &format!("Cannot browse SFTP: {error}"));
                dispatch_active_snapshot(&ui_for_list, &state_for_list);
            }
        }
    });

    let ui_for_back = ui.as_weak();
    let state_for_back = state.clone();
    let router_for_back = window_router.clone();
    ui.on_navigate_sftp_back(move || {
        log_ui_action("sftp.navigate-back");
        sync_window_active(&router_for_back, window_id, &state_for_back);
        let result = queue_remote_navigation(&state_for_back, SftpNavigation::Back, None);
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_back, &state_for_back),
            Err(error) => {
                set_status(&ui_for_back, &format!("Cannot go back in SFTP: {error}"));
                dispatch_active_snapshot(&ui_for_back, &state_for_back);
            }
        }
    });

    let ui_for_forward = ui.as_weak();
    let state_for_forward = state.clone();
    let router_for_forward = window_router.clone();
    ui.on_navigate_sftp_forward(move || {
        log_ui_action("sftp.navigate-forward");
        sync_window_active(&router_for_forward, window_id, &state_for_forward);
        let result = queue_remote_navigation(&state_for_forward, SftpNavigation::Forward, None);
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_forward, &state_for_forward),
            Err(error) => {
                set_status(
                    &ui_for_forward,
                    &format!("Cannot go forward in SFTP: {error}"),
                );
                dispatch_active_snapshot(&ui_for_forward, &state_for_forward);
            }
        }
    });

    let ui_for_more = ui.as_weak();
    let state_for_more = state.clone();
    let router_for_more = window_router.clone();
    ui.on_load_more_sftp(move || {
        log_ui_action("sftp.load-more");
        sync_window_active(&router_for_more, window_id, &state_for_more);
        let result = with_active_sftp_terminal(&state_for_more, |terminal| {
            terminal
                .worker
                .as_ref()
                .context("active SSH terminal has no worker")?
                .request_load_more_sftp()?;
            terminal.sftp.loading = true;
            terminal.sftp.status = "Loading more files...".to_owned();
            Ok(())
        });
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_more, &state_for_more),
            Err(error) => set_status(&ui_for_more, &format!("Cannot load SFTP page: {error}")),
        }
    });

    let ui_for_close = ui.as_weak();
    let state_for_close = state.clone();
    let router_for_close = window_router.clone();
    ui.on_close_sftp(move || {
        log_ui_action("sftp.close");
        sync_window_active(&router_for_close, window_id, &state_for_close);
        let result = with_active_sftp_terminal(&state_for_close, |terminal| {
            terminal
                .worker
                .as_ref()
                .context("active SSH terminal has no worker")?
                .request_close_sftp()?;
            terminal.sftp.reset();
            Ok(())
        });
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_close, &state_for_close),
            Err(error) => set_status(&ui_for_close, &format!("Cannot close SFTP: {error}")),
        }
    });

    let ui_for_remote_selection = ui.as_weak();
    let state_for_remote_selection = state.clone();
    let router_for_remote_selection = window_router.clone();
    ui.on_toggle_remote_sftp_selection(move |path, selected| {
        log_ui_action("sftp.toggle-remote-selection");
        sync_window_active(
            &router_for_remote_selection,
            window_id,
            &state_for_remote_selection,
        );
        let result = with_active_sftp_terminal(&state_for_remote_selection, |terminal| {
            if !terminal.sftp.toggle_selection(path.as_str(), selected) {
                anyhow::bail!("remote entry is no longer visible");
            }
            Ok(())
        });
        match result {
            Ok(()) => {
                dispatch_active_snapshot(&ui_for_remote_selection, &state_for_remote_selection)
            }
            Err(error) => set_status(
                &ui_for_remote_selection,
                &format!("Cannot update SFTP selection: {error}"),
            ),
        }
    });

    let ui_for_remote_select_all = ui.as_weak();
    let state_for_remote_select_all = state.clone();
    let router_for_remote_select_all = window_router.clone();
    ui.on_select_all_remote_sftp(move |selected| {
        log_ui_action("sftp.select-all-remote");
        sync_window_active(
            &router_for_remote_select_all,
            window_id,
            &state_for_remote_select_all,
        );
        let result = with_active_sftp_terminal(&state_for_remote_select_all, |terminal| {
            terminal.sftp.select_all(selected);
            Ok(())
        });
        match result {
            Ok(()) => {
                dispatch_active_snapshot(&ui_for_remote_select_all, &state_for_remote_select_all)
            }
            Err(error) => set_status(
                &ui_for_remote_select_all,
                &format!("Cannot update SFTP selection: {error}"),
            ),
        }
    });

    let ui_for_remote_open = ui.as_weak();
    let state_for_remote_open = state.clone();
    let router_for_remote_open = window_router.clone();
    ui.on_open_remote_sftp_file(move |path| {
        log_ui_action("sftp.open-remote-file");
        sync_window_active(&router_for_remote_open, window_id, &state_for_remote_open);
        let result = with_active_sftp_terminal(&state_for_remote_open, |terminal| {
            let entry = terminal
                .sftp
                .entries
                .iter()
                .find(|entry| entry.path == path.as_str())
                .cloned()
                .context("remote entry is no longer visible")?;
            queue_remote_downloads(terminal, vec![entry])?;
            Ok(())
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
        let result = with_active_sftp_terminal(&state_for_selected_remote_download, |terminal| {
            let selected = terminal
                .sftp
                .entries
                .iter()
                .filter(|entry| terminal.sftp.selected.contains(&entry.path))
                .cloned()
                .collect::<Vec<_>>();
            queue_remote_downloads(terminal, selected)
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

    let ui_for_remove = ui.as_weak();
    let state_for_remove = state.clone();
    let router_for_remove = window_router.clone();
    ui.on_remove_selected_remote_sftp(move || {
        log_ui_action("sftp.remove-selected-remote");
        sync_window_active(&router_for_remove, window_id, &state_for_remove);
        let result = with_active_sftp_terminal(&state_for_remove, |terminal| {
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
        });
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_remove, &state_for_remove),
            Err(error) => set_status(
                &ui_for_remove,
                &format!("Cannot delete remote entries: {error}"),
            ),
        }
    });

    let ui_for_load = ui.as_weak();
    let state_for_load = state.clone();
    let router_for_load = window_router.clone();
    ui.on_load_remote_sftp_file(move |path| {
        log_ui_action("sftp.load-remote-file");
        sync_window_active(&router_for_load, window_id, &state_for_load);
        let result = queue_sftp_write(
            &state_for_load,
            ax_ssh::sftp::SftpWriteOperation::ReadText {
                path: path.to_string(),
            },
        );
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_load, &state_for_load),
            Err(error) => set_status(&ui_for_load, &format!("Cannot load remote file: {error}")),
        }
    });

    let ui_for_save = ui.as_weak();
    let state_for_save = state.clone();
    let router_for_save = window_router.clone();
    ui.on_save_remote_sftp_file(move |path, text| {
        log_ui_action("sftp.save-remote-file");
        sync_window_active(&router_for_save, window_id, &state_for_save);
        let result = with_active_sftp_terminal(&state_for_save, |terminal| {
            let is_current_editor_path =
                terminal.sftp.editor_path.as_deref() == Some(path.as_str());
            let expected_size = is_current_editor_path
                .then(|| {
                    terminal.sftp.editor_expected_size.or_else(|| {
                        terminal
                            .sftp
                            .entries
                            .iter()
                            .find(|entry| entry.path == path.as_str())
                            .map(|entry| entry.size)
                    })
                })
                .flatten();
            let expected_modified = is_current_editor_path
                .then_some(terminal.sftp.editor_expected_modified)
                .flatten();
            terminal
                .worker
                .as_ref()
                .context("active SFTP tab has no worker")?
                .request_sftp_write(
                    Uuid::new_v4(),
                    ax_ssh::sftp::SftpWriteOperation::WriteText {
                        path: path.to_string(),
                        data: text.as_bytes().to_vec(),
                        expected_size,
                        expected_modified,
                    },
                )
        });
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_save, &state_for_save),
            Err(error) => set_status(&ui_for_save, &format!("Cannot save remote file: {error}")),
        }
    });

    let ui_for_editor_text = ui.as_weak();
    let state_for_editor_text = state.clone();
    let router_for_editor_text = window_router.clone();
    let runtime_for_editor_text = runtime.clone();
    ui.on_editor_text_changed_sftp(move |text| {
        log_ui_action("sftp.editor-text-changed");
        sync_window_active(&router_for_editor_text, window_id, &state_for_editor_text);
        let result = with_active_sftp_terminal(&state_for_editor_text, |terminal| {
            Ok(terminal.sftp.set_editor_text(text.to_string()))
        });
        let changed = match result {
            Ok(changed) => changed,
            Err(error) => {
                set_status(
                    &ui_for_editor_text,
                    &format!("Cannot update editor state: {error}"),
                );
                return;
            }
        };
        dispatch_active_snapshot(&ui_for_editor_text, &state_for_editor_text);
        let Some((path, revision)) = changed else {
            return;
        };
        let should_upload = state_for_editor_text
            .lock()
            .ok()
            .and_then(|app| {
                app.active_terminal()
                    .map(|terminal| terminal.sftp.editor_auto_upload)
            })
            .unwrap_or(false);
        if !should_upload {
            return;
        }
        let state_for_task = state_for_editor_text.clone();
        let ui_for_task = ui_for_editor_text.clone();
        runtime_for_editor_text.spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let queued = with_active_sftp_terminal(&state_for_task, |terminal| {
                if !terminal.sftp.editor_is_current(&path, revision)
                    || terminal.sftp.editor_remote_changed
                {
                    anyhow::bail!("remote file changed; automatic upload was skipped");
                }
                terminal
                    .worker
                    .as_ref()
                    .context("active SFTP tab has no worker")?
                    .request_sftp_write(
                        Uuid::new_v4(),
                        ax_ssh::sftp::SftpWriteOperation::WriteText {
                            path: path.clone(),
                            data: terminal.sftp.editor_text.as_bytes().to_vec(),
                            expected_size: terminal.sftp.editor_expected_size,
                            expected_modified: terminal.sftp.editor_expected_modified,
                        },
                    )
            });
            if let Err(error) = queued {
                set_status(&ui_for_task, &format!("Automatic upload skipped: {error}"));
            }
        });
    });

    let ui_for_auto = ui.as_weak();
    let state_for_auto = state.clone();
    let router_for_auto = window_router.clone();
    ui.on_toggle_editor_auto_upload_sftp(move |enabled| {
        log_ui_action("sftp.toggle-editor-auto-upload");
        sync_window_active(&router_for_auto, window_id, &state_for_auto);
        match with_active_sftp_terminal(&state_for_auto, |terminal| {
            terminal.sftp.set_editor_auto_upload(enabled);
            Ok(())
        }) {
            Ok(()) => dispatch_active_snapshot(&ui_for_auto, &state_for_auto),
            Err(error) => set_status(&ui_for_auto, &format!("Cannot change auto upload: {error}")),
        }
    });

    let ui_for_drop = ui.as_weak();
    let state_for_drop = state.clone();
    let router_for_drop = window_router.clone();
    let runtime_for_drop = runtime.clone();
    ui.on_dropped_local_files_sftp(move |data| {
        log_ui_action("sftp.drop-local-files");
        sync_window_active(&router_for_drop, window_id, &state_for_drop);
        let text = match data.plain_text() {
            Ok(text) => text.to_string(),
            Err(error) => {
                set_status(
                    &ui_for_drop,
                    &format!("Dropped data is not a readable path: {error}"),
                );
                return;
            }
        };
        let path = match parse_dropped_local_path(text.as_str()) {
            Ok(path) => path,
            Err(error) => {
                set_status(&ui_for_drop, &format!("Cannot use dropped path: {error}"));
                return;
            }
        };
        queue_local_upload_path(
            &runtime_for_drop,
            state_for_drop.clone(),
            ui_for_drop.clone(),
            path,
        );
    });

    ui.on_drag_local_file_sftp(|path| local_file_drag_data(path.as_str()));

    let ui_for_upload = ui.as_weak();
    let state_for_upload = state.clone();
    let runtime_for_upload = runtime.clone();
    let router_for_upload = window_router.clone();
    ui.on_upload_selected_local_sftp(move || {
        log_ui_action("sftp.upload-selected-local");
        sync_window_active(&router_for_upload, window_id, &state_for_upload);
        let prepared = with_active_sftp_terminal(&state_for_upload, |terminal| {
            let selected = terminal
                .sftp
                .local
                .entries
                .iter()
                .filter(|entry| terminal.sftp.local.selected.contains(&entry.path))
                .filter(|entry| !entry.is_dir && !entry.is_symlink)
                .cloned()
                .collect::<Vec<_>>();
            if selected.len() != 1 {
                anyhow::bail!("select exactly one regular local file to upload");
            }
            let entry = &selected[0];
            if entry.size > ax_ssh::sftp::MAX_UPLOAD_BYTES {
                anyhow::bail!("local file exceeds the upload size limit");
            }
            let name = entry.name.clone();
            let remote_path = if terminal.sftp.path == "/" {
                format!("/{name}")
            } else {
                format!("{}/{}", terminal.sftp.path.trim_end_matches('/'), name)
            };
            Ok((entry.path.clone(), entry.size, remote_path))
        });
        let (local_path, expected_size, remote_path) = match prepared {
            Ok(value) => value,
            Err(error) => {
                set_status(
                    &ui_for_upload,
                    &format!("Cannot upload local file: {error}"),
                );
                return;
            }
        };
        let state_for_upload_task = state_for_upload.clone();
        let ui_for_upload_task = ui_for_upload.clone();
        runtime_for_upload.spawn(async move {
            let queued = with_active_sftp_terminal(&state_for_upload_task, |terminal| {
                let transfer_id = Uuid::new_v4();
                terminal.sftp.queue_transfer(
                    transfer_id,
                    remote_path
                        .rsplit('/')
                        .next()
                        .unwrap_or_default()
                        .to_owned(),
                    expected_size,
                )?;
                terminal
                    .worker
                    .as_ref()
                    .context("active SFTP tab has no worker")?
                    .request_open_sftp_upload(
                        transfer_id,
                        remote_path,
                        PathBuf::from(local_path),
                        expected_size,
                    )
            });
            if let Err(error) = queued {
                set_status(
                    &ui_for_upload_task,
                    &format!("Cannot queue local upload: {error}"),
                );
            } else {
                dispatch_active_snapshot(&ui_for_upload_task, &state_for_upload_task);
            }
        });
    });

    let ui_for_rename = ui.as_weak();
    let state_for_rename = state.clone();
    let router_for_rename = window_router.clone();
    ui.on_rename_remote_sftp(move |new_name| {
        log_ui_action("sftp.rename-remote");
        sync_window_active(&router_for_rename, window_id, &state_for_rename);
        let result = with_active_sftp_terminal(&state_for_rename, |terminal| {
            let entry = terminal
                .sftp
                .entries
                .iter()
                .filter(|entry| terminal.sftp.selected.contains(&entry.path))
                .cloned()
                .collect::<Vec<_>>();
            if entry.len() != 1 {
                anyhow::bail!("select exactly one remote entry to rename");
            }
            let entry = entry
                .into_iter()
                .next()
                .context("remote entry is no longer visible")?;
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
        });
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
    ui.on_edit_remote_sftp(move || {
        log_ui_action("sftp.edit-remote");
        sync_window_active(&router_for_edit, window_id, &state_for_edit);
        let result = with_active_sftp_terminal(&state_for_edit, |terminal| {
            let entry = terminal
                .sftp
                .entries
                .iter()
                .filter(|entry| terminal.sftp.selected.contains(&entry.path))
                .cloned()
                .collect::<Vec<_>>();
            if entry.len() != 1 || entry[0].is_dir || entry[0].is_symlink {
                anyhow::bail!("select exactly one regular remote file to edit");
            }
            terminal
                .worker
                .as_ref()
                .context("active SFTP tab has no worker")?
                .request_sftp_write(
                    Uuid::new_v4(),
                    ax_ssh::sftp::SftpWriteOperation::ReadText {
                        path: entry[0].path.clone(),
                    },
                )
        });
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_edit, &state_for_edit),
            Err(error) => set_status(&ui_for_edit, &format!("Cannot edit remote file: {error}")),
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
            with_active_sftp_terminal(&state_for_transfer_pause, |terminal| {
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
            })
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
            with_active_sftp_terminal(&state_for_transfer_resume, |terminal| {
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
            })
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
            with_active_sftp_terminal(&state_for_transfer_cancel, |terminal| {
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
            })
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
            with_active_sftp_terminal(&state_for_transfer_selection, |terminal| {
                if !terminal
                    .sftp
                    .toggle_transfer_selection(transfer_id, selected)
                {
                    anyhow::bail!("SFTP transfer is no longer available");
                }
                Ok(())
            })
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

    let ui_for_local_selection = ui.as_weak();
    let state_for_local_selection = state.clone();
    let router_for_local_selection = window_router.clone();
    ui.on_toggle_local_sftp_selection(move |path, selected| {
        log_ui_action("sftp.toggle-local-selection");
        sync_window_active(
            &router_for_local_selection,
            window_id,
            &state_for_local_selection,
        );
        let result = with_active_sftp_terminal(&state_for_local_selection, |terminal| {
            if !terminal
                .sftp
                .local
                .toggle_selection(path.as_str(), selected)
            {
                anyhow::bail!("local entry is no longer visible");
            }
            Ok(())
        });
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_local_selection, &state_for_local_selection),
            Err(error) => set_status(
                &ui_for_local_selection,
                &format!("Cannot update local selection: {error}"),
            ),
        }
    });

    let ui_for_local_select_all = ui.as_weak();
    let state_for_local_select_all = state.clone();
    let router_for_local_select_all = window_router.clone();
    ui.on_select_all_local_sftp(move |selected| {
        log_ui_action("sftp.select-all-local");
        sync_window_active(
            &router_for_local_select_all,
            window_id,
            &state_for_local_select_all,
        );
        let result = with_active_sftp_terminal(&state_for_local_select_all, |terminal| {
            terminal.sftp.local.select_all(selected);
            Ok(())
        });
        match result {
            Ok(()) => {
                dispatch_active_snapshot(&ui_for_local_select_all, &state_for_local_select_all)
            }
            Err(error) => set_status(
                &ui_for_local_select_all,
                &format!("Cannot update local selection: {error}"),
            ),
        }
    });

    let ui_for_local_open = ui.as_weak();
    let state_for_local_open = state.clone();
    let runtime_for_local_open = runtime.clone();
    let router_for_local_open = window_router.clone();
    ui.on_open_local_sftp_file(move |path| {
        log_ui_action("sftp.open-local-file");
        sync_window_active(&router_for_local_open, window_id, &state_for_local_open);
        let request = prepare_local_file_open(&state_for_local_open, path.as_str());
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

    let ui_for_local = ui.as_weak();
    let state_for_local = state;
    let router_for_local = window_router;
    ui.on_list_local_sftp_directory(move |path| {
        log_ui_action("sftp.list-local");
        sync_window_active(&router_for_local, window_id, &state_for_local);
        let path = path.as_str().trim().to_owned();
        if path.is_empty() || path.len() > LOCAL_DIRECTORY_PATH_LIMIT {
            set_status(&ui_for_local, "Choose a valid local directory path");
            return;
        }
        let (tab_id, request_id) = match state_for_local.lock() {
            Ok(mut app) => {
                let Some(tab_id) = app.active_tab_id() else {
                    set_status(&ui_for_local, "No active SFTP tab");
                    return;
                };
                let Some(terminal) = app.terminal_mut(tab_id) else {
                    set_status(&ui_for_local, "No active SFTP tab");
                    return;
                };
                if !terminal.is_sftp() {
                    set_status(
                        &ui_for_local,
                        "Local files are available only in an SFTP tab",
                    );
                    return;
                }
                (tab_id, terminal.sftp.local.begin_load(path.clone()))
            }
            Err(_) => {
                set_status(&ui_for_local, "Cannot read local directory state");
                return;
            }
        };
        dispatch_active_snapshot(&ui_for_local, &state_for_local);
        load_local_directory(
            &runtime,
            state_for_local.clone(),
            ui_for_local.clone(),
            tab_id,
            request_id,
            path,
        );
    });
}

fn wire_selected_transfer_actions(
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
        let result = with_active_sftp_terminal(&state_for_pause, |terminal| {
            let transfer_ids = terminal
                .sftp
                .selected_transfer_ids_for_active_page()
                .into_iter()
                .filter(|id| terminal.sftp.transfer_is_pausable(*id))
                .collect::<Vec<_>>();
            request_selected_transfer_actions(terminal, transfer_ids, SelectedTransferAction::Pause)
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
        let result = with_active_sftp_terminal(&state_for_resume, |terminal| {
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
        });
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
        let result = with_active_sftp_terminal(&state_for_cancel, |terminal| {
            let transfer_ids = terminal.sftp.selected_transfer_ids_for_active_page();
            request_selected_transfer_actions(
                terminal,
                transfer_ids,
                SelectedTransferAction::Cancel,
            )
        });
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
enum SelectedTransferAction {
    Pause,
    Resume,
    Cancel,
}

fn request_selected_transfer_actions(
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

fn queue_remote_downloads(terminal: &mut TerminalTabState, entries: Vec<SftpEntry>) -> Result<()> {
    if entries.is_empty() {
        anyhow::bail!("no remote files or folders are selected");
    }
    let local_directory = std::path::PathBuf::from(&terminal.sftp.local.path);
    let worker = terminal
        .worker
        .as_ref()
        .context("active SFTP tab has no worker")?;
    let mut accepted = 0_usize;
    for entry in entries {
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
        match worker.request_open_sftp_file(transfer_id, entry.path, local_directory.clone()) {
            Ok(()) => accepted += 1,
            Err(error) => {
                let _ = terminal
                    .sftp
                    .queue_transfer(transfer_id, entry.name.clone(), entry.size);
                terminal.sftp.finish_transfer(
                    transfer_id,
                    SftpTransferPhase::Failed,
                    "Download request was rejected".to_owned(),
                );
                tracing::debug!(%error, "SFTP download request was rejected before the worker accepted it");
            }
        }
    }
    if accepted == 0 {
        anyhow::bail!("no selected entries could be queued for download");
    }
    Ok(())
}

struct LocalOpenRequest {
    tab_id: uuid::Uuid,
    request_id: u64,
    directory: String,
    entry: LocalDirectoryEntry,
}

fn prepare_local_file_open(
    state: &Arc<Mutex<AppState>>,
    requested_path: &str,
) -> Result<LocalOpenRequest> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let tab_id = app.active_tab_id().context("no active SFTP tab")?;
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
    })
}

fn open_local_file(
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
        let snapshot = tokio::time::timeout(
            LOCAL_SNAPSHOT_TIMEOUT,
            tokio::task::spawn_blocking(move || {
                ax_ssh::sftp::snapshot_local_file_for_open(validated.file, &validated.name)
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

fn local_open_snapshot_is_current(
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

fn finish_local_file_open(
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

fn queue_remote_navigation(
    state: &Arc<Mutex<AppState>>,
    kind: SftpNavigation,
    path: Option<String>,
) -> Result<()> {
    with_active_sftp_terminal(state, |terminal| {
        queue_remote_navigation_for_terminal(terminal, kind, path)
    })
}

pub(super) fn navigate_sftp_tab_to_path(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    path: String,
) -> Result<()> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let terminal = app
        .terminal_mut(tab_id)
        .context("SFTP companion is missing")?;
    if !terminal.is_sftp() {
        anyhow::bail!("SFTP companion has the wrong session type");
    }
    if !terminal.connected {
        if terminal.worker_running || !matches!(terminal.ssh_phase, SshConnectionPhase::Idle) {
            terminal.sftp_initial_path = Some(path);
            return Ok(());
        }
        anyhow::bail!("SFTP companion is not connected");
    }
    queue_remote_navigation_for_terminal(terminal, SftpNavigation::Direct, Some(path))
}

fn queue_remote_navigation_for_terminal(
    terminal: &mut TerminalTabState,
    kind: SftpNavigation,
    path: Option<String>,
) -> Result<()> {
    let worker = terminal
        .worker
        .as_ref()
        .context("active SSH terminal has no worker")?;
    let request_path = terminal.sftp.begin_navigation(kind, path)?;
    let result = worker.request_list_sftp(request_path);
    if let Err(error) = result {
        terminal.sftp.cancel_navigation();
        terminal.sftp.status = "SFTP directory request was rejected".to_owned();
        return Err(error);
    }
    Ok(())
}

fn load_local_directory(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: uuid::Uuid,
    request_id: u64,
    path: String,
) {
    let runtime = runtime.clone();
    let runtime_for_icons = runtime.clone();
    runtime.spawn(async move {
        let listed = tokio::time::timeout(
            LOCAL_DIRECTORY_TIMEOUT,
            tokio::task::spawn_blocking(move || read_local_directory(&path)),
        )
        .await;
        let mut icon_keys = Vec::new();
        let message = match listed {
            Ok(Ok(Ok(listing))) => {
                icon_keys = local_icon_keys(&listing.entries);
                apply_local_directory_listing(&state, tab_id, request_id, listing)
            }
            Ok(Ok(Err(error))) => apply_local_directory_failure(
                &state,
                tab_id,
                request_id,
                format!("Cannot list local directory: {error}"),
            ),
            Ok(Err(error)) => apply_local_directory_failure(
                &state,
                tab_id,
                request_id,
                format!("Local directory task failed: {error}"),
            ),
            Err(_) => apply_local_directory_failure(
                &state,
                tab_id,
                request_id,
                "Local directory listing timed out".to_owned(),
            ),
        };
        if message {
            dispatch_active_snapshot(&ui, &state);
            prewarm_file_icons(&runtime_for_icons, icon_keys, &ui, &state);
        }
    });
}

fn apply_local_directory_listing(
    state: &Arc<Mutex<AppState>>,
    tab_id: uuid::Uuid,
    request_id: u64,
    listing: super::local_files::LocalDirectoryListing,
) -> bool {
    let Ok(mut app) = state.lock() else {
        return false;
    };
    let active = app.active_tab_id() == Some(tab_id);
    let Some(terminal) = app.terminal_mut(tab_id) else {
        return false;
    };
    if !terminal.is_sftp() || terminal.sftp.local.request_id != request_id {
        return false;
    }
    terminal
        .sftp
        .local
        .complete(listing.path, listing.entries, listing.truncated);
    active
}

fn apply_local_directory_failure(
    state: &Arc<Mutex<AppState>>,
    tab_id: uuid::Uuid,
    request_id: u64,
    message: String,
) -> bool {
    let Ok(mut app) = state.lock() else {
        return false;
    };
    let active = app.active_tab_id() == Some(tab_id);
    let Some(terminal) = app.terminal_mut(tab_id) else {
        return false;
    };
    if !terminal.is_sftp() || terminal.sftp.local.request_id != request_id {
        return false;
    }
    terminal.sftp.local.fail(message);
    active
}

fn with_active_sftp_terminal<T>(
    state: &Arc<Mutex<AppState>>,
    action: impl FnOnce(&mut TerminalTabState) -> Result<T>,
) -> Result<T> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let terminal = app.active_terminal_mut().context("no active terminal")?;
    if !terminal.is_sftp() {
        anyhow::bail!("SFTP is available only in an SFTP tab");
    }
    if !terminal.connected {
        anyhow::bail!("SFTP session is not connected");
    }
    action(terminal)
}

fn parse_transfer_id(value: &str) -> Result<uuid::Uuid> {
    value
        .parse::<uuid::Uuid>()
        .context("invalid SFTP transfer id")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_local_path_accepts_file_uri_and_rejects_remote_hosts() {
        assert_eq!(
            parse_dropped_local_path("file:///tmp/hello%20world.txt").unwrap(),
            PathBuf::from("/tmp/hello world.txt")
        );
        assert!(parse_dropped_local_path("file://other-host/tmp/file.txt").is_err());
        assert!(parse_dropped_local_path("file:///tmp/bad%2").is_err());
    }

    #[test]
    fn dropped_local_path_uses_only_the_first_non_empty_line() {
        assert_eq!(
            parse_dropped_local_path("\n /tmp/first.txt\n/tmp/second.txt").unwrap(),
            PathBuf::from("/tmp/first.txt")
        );
    }
}
