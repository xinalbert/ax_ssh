//! SFTP drag intent wiring and bounded application operations.

use super::*;

pub(super) fn wire_drag(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let ui_for_drop = ui.as_weak();
    let state_for_drop = state.clone();
    let router_for_drop = window_router.clone();
    ui.on_dropped_local_files_sftp(move |data| {
        log_ui_action("sftp.drop-on-local-pane");
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
        handle_drop_on_local_pane(
            &state_for_drop,
            &ui_for_drop,
            &router_for_drop,
            window_id,
            text.as_str(),
        );
    });

    let state_for_local_drag = state.clone();
    let router_for_local_drag = window_router.clone();
    ui.on_drag_local_file_sftp(move |path| {
        let dragged = PathBuf::from(path.as_str());
        let paths = with_window_sftp_terminal(
            &state_for_local_drag,
            &router_for_local_drag,
            window_id,
            |terminal| {
                if !terminal.sftp.local.selected.contains(path.as_str()) {
                    return Ok(vec![dragged.clone()]);
                }
                Ok(terminal
                    .sftp
                    .local
                    .entries
                    .iter()
                    .filter(|entry| {
                        terminal.sftp.local.selected.contains(&entry.path) && !entry.is_symlink
                    })
                    .map(|entry| PathBuf::from(&entry.path))
                    .collect())
            },
        )
        .unwrap_or_else(|_| vec![dragged]);
        local_file_drag_data(&paths)
    });
    ui.on_drag_remote_file_sftp(|path| remote_file_drag_data(path.as_str()));

    ui.on_sftp_drag_diagnostic(|stage, local_source| {
        log_sftp_drag_lifecycle(stage.as_str(), local_source);
    });

    #[cfg(target_os = "macos")]
    {
        let ui_for_native_drag = ui.as_weak();
        let state_for_native_drag = state.clone();
        let router_for_native_drag = window_router.clone();
        ui.on_begin_native_remote_file_drag_sftp(move |path| {
            log_ui_action("sftp.drag-native-remote-file");
            sync_window_active(&router_for_native_drag, window_id, &state_for_native_drag);
            let Some(ui) = ui_for_native_drag.upgrade() else {
                return;
            };
            let result = begin_native_remote_file_drag(
                &state_for_native_drag,
                &ui,
                &router_for_native_drag,
                window_id,
                path.as_str(),
            );
            if let Err(error) = result {
                set_status(
                    &ui_for_native_drag,
                    &format!("Cannot start native SFTP drag: {error}"),
                );
            }
        });
    }

    let ui_for_remote_drop = ui.as_weak();
    let state_for_remote_drop = state.clone();
    let router_for_remote_drop = window_router.clone();
    let runtime_for_remote_drop = runtime.clone();
    ui.on_dropped_remote_files_sftp(move |data| {
        log_ui_action("sftp.drop-on-remote-pane");
        sync_window_active(&router_for_remote_drop, window_id, &state_for_remote_drop);
        let text = match data.plain_text() {
            Ok(text) => text.to_string(),
            Err(error) => {
                log_sftp_drag_drop("payload-unreadable", None, None);
                set_status(
                    &ui_for_remote_drop,
                    &format!("Dropped data is not a readable path: {error}"),
                );
                return;
            }
        };
        log_sftp_drag_drop("payload-received", None, u64::try_from(text.len()).ok());
        handle_drop_on_remote_pane(
            &runtime_for_remote_drop,
            &state_for_remote_drop,
            &ui_for_remote_drop,
            &router_for_remote_drop,
            window_id,
            text.as_str(),
        );
    });
}

type SlintDataTransfer = slint::private_unstable_api::re_exports::DataTransfer;

pub(super) fn local_file_drag_data(paths: &[PathBuf]) -> SlintDataTransfer {
    let mut data = SlintDataTransfer::default();
    if !paths.is_empty() && paths.len() <= MAX_DROPPED_LOCAL_PATHS {
        let text = paths
            .iter()
            .filter_map(|path| path.to_str())
            .collect::<Vec<_>>()
            .join("\n");
        if text.len() <= MAX_DROPPED_LOCAL_DATA_BYTES
            && text.lines().count() == paths.len()
            && text.lines().all(|path| {
                !path.is_empty()
                    && path.len() <= LOCAL_DIRECTORY_PATH_LIMIT
                    && !path.chars().any(char::is_control)
            })
        {
            data.set_plain_text(format!("{LOCAL_DRAG_PREFIX}{text}").into());
        }
    }
    data
}

pub(super) fn remote_file_drag_data(path: &str) -> SlintDataTransfer {
    let mut data = SlintDataTransfer::default();
    if path.len() <= LOCAL_DIRECTORY_PATH_LIMIT
        && !path.is_empty()
        && !path.chars().any(char::is_control)
    {
        data.set_plain_text(format!("{REMOTE_DRAG_PREFIX}{path}").into());
    }
    data
}

pub(super) enum SftpDragPayload {
    Local(Vec<PathBuf>),
    Remote(String),
}

pub(super) fn log_sftp_drag_lifecycle(stage: &str, local_source: bool) {
    let stage = match stage {
        "started" => "started",
        "finished-copy" => "finished-copy",
        "finished-no-copy" => "finished-no-copy",
        _ => "unknown",
    };
    tracing::debug!(
        target: SFTP_DRAG_TARGET,
        event = "internal-drag",
        source_pane = if local_source { "local" } else { "remote" },
        stage,
        "SFTP internal drag lifecycle"
    );
}

pub(super) fn log_sftp_drag_drop(
    stage: &'static str,
    file_count: Option<usize>,
    byte_count: Option<u64>,
) {
    tracing::debug!(
        target: SFTP_DRAG_TARGET,
        event = "drop-on-remote-pane",
        stage,
        file_count,
        byte_count,
        "SFTP dropped upload route"
    );
}

pub(super) fn parse_sftp_drag_payload(text: &str) -> Result<SftpDragPayload> {
    if let Some(path) = text.strip_prefix(REMOTE_DRAG_PREFIX) {
        if path.is_empty()
            || path.len() > LOCAL_DIRECTORY_PATH_LIMIT
            || path.chars().any(char::is_control)
        {
            anyhow::bail!("dropped remote path is invalid or too long");
        }
        return Ok(SftpDragPayload::Remote(path.to_owned()));
    }
    let local_text = text.strip_prefix(LOCAL_DRAG_PREFIX).unwrap_or(text);
    Ok(SftpDragPayload::Local(parse_dropped_local_paths(
        local_text,
    )?))
}

pub(super) fn parse_single_dropped_path(raw: &str) -> Result<PathBuf> {
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

/// Parse all non-empty lines of dropped text as local paths.
///
/// Each line may be a bare POSIX/Windows path or a `file://` URI.  Remote
/// authority hosts and control characters are rejected.  Duplicate paths are
/// removed while preserving first-seen order so the caller can queue them as
/// independent transfers.
pub(super) fn parse_dropped_local_paths(text: &str) -> Result<Vec<PathBuf>> {
    if text.len() > MAX_DROPPED_LOCAL_DATA_BYTES {
        anyhow::bail!("dropped data is too large");
    }
    let mut seen = std::collections::HashSet::new();
    let mut paths = Vec::new();
    for raw in text.split(['\n', '\r']).map(str::trim) {
        if raw.is_empty() {
            continue;
        }
        let path = parse_single_dropped_path(raw)?;
        if seen.insert(path.clone()) {
            if paths.len() >= MAX_DROPPED_LOCAL_PATHS {
                anyhow::bail!(
                    "dropped data contains too many local files (maximum {MAX_DROPPED_LOCAL_PATHS})"
                );
            }
            paths.push(path);
        }
    }
    if paths.is_empty() {
        anyhow::bail!("dropped data did not contain a local file path");
    }
    Ok(paths)
}

pub(super) fn percent_decode_path(value: &str) -> Result<String> {
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

/// Queue an external local file only after the UI bridge has resolved the
/// current pointer to the declared Remote files drop target. This deliberately
/// has no active-directory fallback: Winit's `DroppedFile` carries no target.
pub(in crate::app) fn handle_native_dropped_file_on_remote_pane(
    runtime: &Handle,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    window_router: &WindowRouter,
    window_id: Uuid,
    path: &std::path::Path,
    batch_id: Uuid,
) {
    log_ui_action("sftp.drop-native-file");
    sync_window_active(window_router, window_id, state);
    match active_sftp_upload_target(state, window_router, window_id) {
        Ok((tab_id, remote_directory)) => {
            log_sftp_drag_drop("native-upload-target-resolved", Some(1), None);
            queue_local_upload_paths(
                runtime,
                state.clone(),
                ui.clone(),
                tab_id,
                batch_id,
                remote_directory,
                vec![path.to_owned()],
            );
        }
        Err(error) => {
            log_sftp_drag_drop("native-upload-target-rejected", Some(1), None);
            set_status(ui, &format!("Cannot prepare dropped upload: {error}"));
        }
    }
}

pub(super) fn handle_drop_on_local_pane(
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    router: &WindowRouter,
    window_id: Uuid,
    text: &str,
) {
    match parse_sftp_drag_payload(text) {
        Ok(SftpDragPayload::Local(_)) => {
            set_status(ui, "Drop a local file onto the remote pane to upload it");
        }
        Ok(SftpDragPayload::Remote(path)) => {
            let filter_patterns = match active_sftp_transfer_filter_patterns(state) {
                Ok(patterns) => patterns,
                Err(error) => {
                    set_status(ui, &format!("Cannot read SFTP transfer filters: {error}"));
                    return;
                }
            };
            let result = with_window_sftp_terminal(state, router, window_id, |terminal| {
                let entry = terminal
                    .sftp
                    .entries
                    .iter()
                    .find(|entry| entry.path == path)
                    .cloned()
                    .context("remote entry is no longer visible")?;
                queue_remote_downloads(terminal, vec![entry], &filter_patterns)
            });
            match result {
                Ok(()) => dispatch_active_snapshot(ui, state),
                Err(error) => set_status(ui, &format!("Cannot queue remote download: {error}")),
            }
        }
        Err(error) => set_status(ui, &format!("Cannot use dropped path: {error}")),
    }
}

pub(super) fn handle_drop_on_remote_pane(
    runtime: &Handle,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    router: &WindowRouter,
    window_id: Uuid,
    text: &str,
) {
    let paths = match parse_sftp_drag_payload(text) {
        Ok(SftpDragPayload::Local(paths)) => {
            log_sftp_drag_drop(
                "payload-parsed",
                Some(paths.len()),
                u64::try_from(text.len()).ok(),
            );
            paths
        }
        Ok(SftpDragPayload::Remote(_)) => {
            log_sftp_drag_drop(
                "remote-payload-rejected",
                None,
                u64::try_from(text.len()).ok(),
            );
            set_status(ui, "Remote files can only be dropped onto the local pane");
            return;
        }
        Err(error) => {
            log_sftp_drag_drop("payload-rejected", None, u64::try_from(text.len()).ok());
            set_status(ui, &format!("Cannot use dropped path: {error}"));
            return;
        }
    };
    let (tab_id, remote_directory) = match active_sftp_upload_target(state, router, window_id) {
        Ok(target) => {
            log_sftp_drag_drop("upload-target-resolved", Some(paths.len()), None);
            target
        }
        Err(error) => {
            log_sftp_drag_drop("upload-target-rejected", Some(paths.len()), None);
            set_status(ui, &format!("Cannot prepare dropped upload: {error}"));
            return;
        }
    };
    let batch_id = Uuid::new_v4();
    queue_local_upload_paths(
        runtime,
        state.clone(),
        ui.clone(),
        tab_id,
        batch_id,
        remote_directory,
        paths,
    );
}

#[cfg(target_os = "macos")]
pub(super) fn begin_native_remote_file_drag(
    state: &Arc<Mutex<AppState>>,
    ui: &AppWindow,
    router: &WindowRouter,
    window_id: Uuid,
    remote_path: &str,
) -> Result<()> {
    let filter_patterns = active_sftp_transfer_filter_patterns(state)?;
    let tab_id = router
        .active_tab(window_id)
        .context("no active SFTP tab for this window")?;
    let (file_name, remote_path, total_bytes, local_target) =
        with_sftp_terminal_for_tab(state, tab_id, |terminal| {
            let entry = terminal
                .sftp
                .entries
                .iter()
                .find(|entry| entry.path == remote_path)
                .cloned()
                .context("remote entry is no longer visible")?;
            if entry.is_dir || entry.is_symlink {
                anyhow::bail!("only regular remote files support native dragging")
            }
            if ax_ssh::sftp::transfer_name_matches_filter(&entry.name, &filter_patterns) {
                anyhow::bail!("file is excluded by SFTP transfer filters")
            }
            let local_directory = PathBuf::from(&terminal.sftp.local.path);
            if local_directory.as_os_str().is_empty() {
                anyhow::bail!("local SFTP directory is not ready")
            }
            Ok((
                entry.name.clone(),
                entry.path,
                entry.size,
                local_directory.join(entry.name),
            ))
        })?;

    let state_for_transfer = state.clone();
    let ui_for_transfer = ui.as_weak();
    let local_drop_region = ui.invoke_native_sftp_local_drop_region();
    let local_drop_region = local_drop_region
        .accepts
        .then(|| {
            super::macos_file_drag::NativeDropRegion::from_logical(
                local_drop_region.x,
                local_drop_region.y,
                local_drop_region.width,
                local_drop_region.height,
            )
        })
        .flatten();
    super::macos_file_drag::begin_file_promise_drag(
        ui.window(),
        file_name,
        local_target,
        local_drop_region,
        move |transfer_id, target| {
            let result = queue_native_remote_download(
                &state_for_transfer,
                tab_id,
                transfer_id,
                remote_path.clone(),
                total_bytes,
                target,
            );
            if result.is_err() {
                set_status(
                    &ui_for_transfer,
                    "Cannot queue the native SFTP file download",
                );
            }
            result
        },
    )
}

#[cfg(target_os = "macos")]
pub(super) fn queue_native_remote_download(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    transfer_id: Uuid,
    remote_path: String,
    total_bytes: u64,
    local_target: PathBuf,
) -> Result<()> {
    with_sftp_terminal_for_tab(state, tab_id, |terminal| {
        let name = remote_path
            .rsplit('/')
            .next()
            .filter(|name| !name.is_empty())
            .context("remote download target is missing a file name")?
            .to_owned();
        terminal
            .sftp
            .queue_transfer(transfer_id, name, total_bytes)?;
        let result = terminal
            .worker
            .as_ref()
            .context("SFTP tab has no worker")?
            .request_open_sftp_file_at_local_path(
                transfer_id,
                remote_path,
                local_target,
                total_bytes,
            );
        if let Err(error) = result {
            terminal.sftp.finish_transfer(
                transfer_id,
                SftpTransferPhase::Failed,
                "Native download request was rejected".to_owned(),
            );
            return Err(error);
        }
        Ok(())
    })
}
