use super::local_files::{
    LOCAL_DIRECTORY_PATH_LIMIT, LocalDirectoryEntry, read_local_directory,
    validate_local_file_for_open,
};
use super::*;
use std::path::{Path, PathBuf};

const LOCAL_DIRECTORY_TIMEOUT: Duration = Duration::from_secs(5);
const LOCAL_OPEN_TIMEOUT: Duration = Duration::from_secs(5);
const LOCAL_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const MAX_REMOTE_NAME_CHARS: usize = 512;
const MAX_DROPPED_LOCAL_PATHS: usize = 32;
const MAX_DROPPED_LOCAL_DATA_BYTES: usize = LOCAL_DIRECTORY_PATH_LIMIT * MAX_DROPPED_LOCAL_PATHS;
const LOCAL_DRAG_PREFIX: &str = "axssh-local-path:";
const REMOTE_DRAG_PREFIX: &str = "axssh-remote-path:";
const SFTP_DRAG_TARGET: &str = "ax_ssh::sftp_drag";

type SlintDataTransfer = slint::private_unstable_api::re_exports::DataTransfer;

fn local_file_drag_data(path: &str) -> SlintDataTransfer {
    let mut data = SlintDataTransfer::default();
    if path.len() <= LOCAL_DIRECTORY_PATH_LIMIT
        && !path.is_empty()
        && !path.chars().any(char::is_control)
    {
        data.set_plain_text(format!("{LOCAL_DRAG_PREFIX}{path}").into());
    }
    data
}

fn remote_file_drag_data(path: &str) -> SlintDataTransfer {
    let mut data = SlintDataTransfer::default();
    if path.len() <= LOCAL_DIRECTORY_PATH_LIMIT
        && !path.is_empty()
        && !path.chars().any(char::is_control)
    {
        data.set_plain_text(format!("{REMOTE_DRAG_PREFIX}{path}").into());
    }
    data
}

enum SftpDragPayload {
    Local(Vec<PathBuf>),
    Remote(String),
}

fn log_sftp_drag_lifecycle(stage: &str, local_source: bool) {
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

fn log_sftp_drag_drop(stage: &'static str, file_count: Option<usize>, byte_count: Option<u64>) {
    tracing::debug!(
        target: SFTP_DRAG_TARGET,
        event = "drop-on-remote-pane",
        stage,
        file_count,
        byte_count,
        "SFTP dropped upload route"
    );
}

fn parse_sftp_drag_payload(text: &str) -> Result<SftpDragPayload> {
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

fn parse_single_dropped_path(raw: &str) -> Result<PathBuf> {
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
fn parse_dropped_local_paths(text: &str) -> Result<Vec<PathBuf>> {
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
    router: &WindowRouter,
    window_id: Uuid,
    operation: ax_ssh::sftp::SftpWriteOperation,
) -> Result<()> {
    with_window_sftp_terminal(state, router, window_id, |terminal| {
        let id = Uuid::new_v4();
        terminal
            .worker
            .as_ref()
            .context("active SFTP tab has no worker")?
            .request_sftp_write(id, operation)
    })
}

fn join_remote_upload_path(directory: &str, name: &str) -> String {
    if directory == "/" || directory.is_empty() {
        format!("/{name}")
    } else {
        format!("{}/{}", directory.trim_end_matches('/'), name)
    }
}

fn with_sftp_terminal_for_tab<T>(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    action: impl FnOnce(&mut TerminalTabState) -> Result<T>,
) -> Result<T> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let terminal = app
        .terminal_mut(tab_id)
        .context("SFTP tab is no longer available")?;
    if !terminal.is_sftp() {
        anyhow::bail!("upload target is no longer an SFTP tab");
    }
    if !terminal.connected {
        anyhow::bail!("SFTP session is no longer connected");
    }
    action(terminal)
}

fn active_sftp_upload_target(
    state: &Arc<Mutex<AppState>>,
    router: &WindowRouter,
    window_id: Uuid,
) -> Result<(Uuid, String)> {
    let tab_id = router.active_tab(window_id).context("no active terminal")?;
    let mut remote_directory = String::new();
    with_sftp_terminal_for_tab(state, tab_id, |terminal| {
        remote_directory = terminal.sftp.path.trim().to_owned();
        if terminal.sftp.loading {
            anyhow::bail!("remote SFTP directory is still loading");
        }
        if remote_directory.is_empty() {
            anyhow::bail!("remote SFTP directory is not ready");
        }
        Ok(())
    })?;
    Ok((tab_id, remote_directory))
}

fn active_sftp_transfer_filter_patterns(state: &Arc<Mutex<AppState>>) -> Result<Vec<String>> {
    let app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    Ok(app
        .sessions
        .settings
        .sftp_transfer_filters
        .effective_patterns())
}

fn prepare_selected_local_upload(
    state: &Arc<Mutex<AppState>>,
    router: &WindowRouter,
    window_id: Uuid,
) -> Result<(Uuid, PathBuf, u64, String)> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let tab_id = router.active_tab(window_id).context("no active terminal")?;
    let terminal = app.terminal_mut(tab_id).context("no active terminal")?;
    if !terminal.is_sftp() {
        anyhow::bail!("SFTP is available only in an SFTP tab");
    }
    if !terminal.connected {
        anyhow::bail!("SFTP session is not connected");
    }
    let remote_directory = terminal.sftp.path.trim().to_owned();
    if remote_directory.is_empty() {
        anyhow::bail!("remote SFTP directory is not ready");
    }
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
    let filter_patterns = app
        .sessions
        .settings
        .sftp_transfer_filters
        .effective_patterns();
    if ax_ssh::sftp::transfer_name_matches_filter(&entry.name, &filter_patterns) {
        anyhow::bail!("local file is excluded by SFTP transfer filters");
    }
    Ok((
        tab_id,
        entry.path.clone().into(),
        entry.size,
        join_remote_upload_path(&remote_directory, &entry.name),
    ))
}

fn queue_upload_for_tab(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    remote_path: String,
    local_path: PathBuf,
    total_bytes: u64,
) -> Result<()> {
    let name = remote_path
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .context("remote upload target is missing a file name")?
        .to_owned();
    let filter_patterns = active_sftp_transfer_filter_patterns(state)?;
    if ax_ssh::sftp::transfer_name_matches_filter(&name, &filter_patterns) {
        anyhow::bail!("file is excluded by SFTP transfer filters");
    }
    with_sftp_terminal_for_tab(state, tab_id, |terminal| {
        let transfer_id = Uuid::new_v4();
        terminal.sftp.queue_upload_transfer(
            transfer_id,
            name.clone(),
            total_bytes,
            local_path.clone(),
            remote_path.clone(),
        )?;
        let result = match terminal.worker.as_ref() {
            Some(worker) => {
                worker.request_open_sftp_upload(transfer_id, remote_path, local_path, total_bytes)
            }
            None => Err(anyhow::anyhow!("SFTP tab has no worker")),
        };
        if let Err(error) = result {
            terminal.sftp.finish_transfer(
                transfer_id,
                SftpTransferPhase::Failed,
                "Upload request was rejected".to_owned(),
            );
            return Err(error);
        }
        Ok(())
    })
}

fn queue_local_upload_path(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    remote_directory: String,
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
                log_sftp_drag_drop("local-file-rejected", None, None);
                set_status(&ui, &format!("Cannot prepare dropped upload: {error}"));
                return;
            }
            Err(error) => {
                log_sftp_drag_drop("inspection-task-failed", None, None);
                set_status(&ui, &format!("Dropped upload task failed: {error}"));
                return;
            }
        };
        log_sftp_drag_drop("local-file-validated", Some(1), Some(total_bytes));
        let remote_path = join_remote_upload_path(&remote_directory, &name);
        let queued = queue_upload_for_tab(
            &state_for_task,
            tab_id,
            remote_path,
            local_path,
            total_bytes,
        );
        match queued {
            Ok(()) => {
                log_sftp_drag_drop("upload-queued", Some(1), Some(total_bytes));
                dispatch_active_snapshot(&ui, &state_for_task);
            }
            Err(error) => {
                log_sftp_drag_drop("upload-rejected", Some(1), Some(total_bytes));
                set_status(&ui, &format!("Cannot queue dropped upload: {error}"));
            }
        }
    });
}

/// Queue an external local file only after the UI bridge has resolved the
/// current pointer to the declared Remote files drop target. This deliberately
/// has no active-directory fallback: Winit's `DroppedFile` carries no target.
pub(super) fn handle_native_dropped_file_on_remote_pane(
    runtime: &Handle,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    window_router: &WindowRouter,
    window_id: Uuid,
    path: &std::path::Path,
) {
    log_ui_action("sftp.drop-native-file");
    sync_window_active(window_router, window_id, state);
    match active_sftp_upload_target(state, window_router, window_id) {
        Ok((tab_id, remote_directory)) => {
            log_sftp_drag_drop("native-upload-target-resolved", Some(1), None);
            queue_local_upload_path(
                runtime,
                state.clone(),
                ui.clone(),
                tab_id,
                remote_directory,
                path.to_owned(),
            );
        }
        Err(error) => {
            log_sftp_drag_drop("native-upload-target-rejected", Some(1), None);
            set_status(ui, &format!("Cannot prepare dropped upload: {error}"));
        }
    }
}

fn handle_drop_on_local_pane(
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

fn handle_drop_on_remote_pane(
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
    for path in paths {
        queue_local_upload_path(
            runtime,
            state.clone(),
            ui.clone(),
            tab_id,
            remote_directory.clone(),
            path,
        );
    }
}

#[cfg(target_os = "macos")]
fn begin_native_remote_file_drag(
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
fn queue_native_remote_download(
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
            &router_for_list,
            window_id,
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
        let result = queue_remote_navigation(
            &state_for_back,
            &router_for_back,
            window_id,
            SftpNavigation::Back,
            None,
        );
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
        let result = queue_remote_navigation(
            &state_for_forward,
            &router_for_forward,
            window_id,
            SftpNavigation::Forward,
            None,
        );
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

    let ui_for_local_back = ui.as_weak();
    let state_for_local_back = state.clone();
    let router_for_local_back = window_router.clone();
    let runtime_for_local_back = runtime.clone();
    ui.on_navigate_local_sftp_back(move || {
        log_ui_action("sftp.navigate-local-back");
        sync_window_active(&router_for_local_back, window_id, &state_for_local_back);
        match queue_local_navigation(
            &state_for_local_back,
            &router_for_local_back,
            window_id,
            SftpNavigation::Back,
            None,
        ) {
            Ok((tab_id, request_id, path)) => {
                dispatch_active_snapshot(&ui_for_local_back, &state_for_local_back);
                load_local_directory(
                    &runtime_for_local_back,
                    state_for_local_back.clone(),
                    ui_for_local_back.clone(),
                    tab_id,
                    request_id,
                    path,
                );
            }
            Err(error) => {
                set_status(
                    &ui_for_local_back,
                    &format!("Cannot go back in local files: {error}"),
                );
                dispatch_active_snapshot(&ui_for_local_back, &state_for_local_back);
            }
        }
    });

    let ui_for_local_forward = ui.as_weak();
    let state_for_local_forward = state.clone();
    let router_for_local_forward = window_router.clone();
    let runtime_for_local_forward = runtime.clone();
    ui.on_navigate_local_sftp_forward(move || {
        log_ui_action("sftp.navigate-local-forward");
        sync_window_active(
            &router_for_local_forward,
            window_id,
            &state_for_local_forward,
        );
        match queue_local_navigation(
            &state_for_local_forward,
            &router_for_local_forward,
            window_id,
            SftpNavigation::Forward,
            None,
        ) {
            Ok((tab_id, request_id, path)) => {
                dispatch_active_snapshot(&ui_for_local_forward, &state_for_local_forward);
                load_local_directory(
                    &runtime_for_local_forward,
                    state_for_local_forward.clone(),
                    ui_for_local_forward.clone(),
                    tab_id,
                    request_id,
                    path,
                );
            }
            Err(error) => {
                set_status(
                    &ui_for_local_forward,
                    &format!("Cannot go forward in local files: {error}"),
                );
                dispatch_active_snapshot(&ui_for_local_forward, &state_for_local_forward);
            }
        }
    });

    let ui_for_more = ui.as_weak();
    let state_for_more = state.clone();
    let router_for_more = window_router.clone();
    ui.on_load_more_sftp(move || {
        log_ui_action("sftp.load-more");
        sync_window_active(&router_for_more, window_id, &state_for_more);
        let result =
            with_window_sftp_terminal(&state_for_more, &router_for_more, window_id, |terminal| {
                terminal
                    .worker
                    .as_ref()
                    .context("active SSH terminal has no worker")?
                    .request_load_more_sftp()?;
                terminal.sftp.request_id = terminal.sftp.request_id.wrapping_add(1).max(1);
                terminal.sftp.loading = true;
                terminal.sftp.status = "Loading more files...".to_owned();
                Ok(())
            });
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_more, &state_for_more),
            Err(error) => set_status(&ui_for_more, &format!("Cannot load SFTP page: {error}")),
        }
    });

    let ui_for_local_more = ui.as_weak();
    let state_for_local_more = state.clone();
    let router_for_local_more = window_router.clone();
    ui.on_load_more_local_sftp(move || {
        log_ui_action("sftp.load-more-local");
        sync_window_active(&router_for_local_more, window_id, &state_for_local_more);
        let result = with_window_sftp_terminal(
            &state_for_local_more,
            &router_for_local_more,
            window_id,
            |terminal| {
                if !terminal.sftp.local.load_more() {
                    anyhow::bail!("no more local files are available");
                }
                Ok(())
            },
        );
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_local_more, &state_for_local_more),
            Err(error) => set_status(
                &ui_for_local_more,
                &format!("Cannot load local directory page: {error}"),
            ),
        }
    });

    let ui_for_remote_sort = ui.as_weak();
    let state_for_remote_sort = state.clone();
    let router_for_remote_sort = window_router.clone();
    ui.on_sort_remote_sftp(move |column| {
        log_ui_action("sftp.sort-remote");
        sync_window_active(&router_for_remote_sort, window_id, &state_for_remote_sort);
        let result = with_window_sftp_terminal(
            &state_for_remote_sort,
            &router_for_remote_sort,
            window_id,
            |terminal| {
                if !terminal.sftp.toggle_sort(column.as_str()) {
                    anyhow::bail!("unknown SFTP sort column");
                }
                Ok(())
            },
        );
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_remote_sort, &state_for_remote_sort),
            Err(error) => set_status(
                &ui_for_remote_sort,
                &format!("Cannot sort remote files: {error}"),
            ),
        }
    });

    let ui_for_local_sort = ui.as_weak();
    let state_for_local_sort = state.clone();
    let router_for_local_sort = window_router.clone();
    ui.on_sort_local_sftp(move |column| {
        log_ui_action("sftp.sort-local");
        sync_window_active(&router_for_local_sort, window_id, &state_for_local_sort);
        let result = with_window_sftp_terminal(
            &state_for_local_sort,
            &router_for_local_sort,
            window_id,
            |terminal| {
                if !terminal.sftp.toggle_local_sort(column.as_str()) {
                    anyhow::bail!("unknown local SFTP sort column");
                }
                Ok(())
            },
        );
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_local_sort, &state_for_local_sort),
            Err(error) => set_status(
                &ui_for_local_sort,
                &format!("Cannot sort local files: {error}"),
            ),
        }
    });

    let ui_for_close = ui.as_weak();
    let state_for_close = state.clone();
    let router_for_close = window_router.clone();
    ui.on_close_sftp(move || {
        log_ui_action("sftp.close");
        sync_window_active(&router_for_close, window_id, &state_for_close);
        let result =
            with_window_sftp_terminal(&state_for_close, &router_for_close, window_id, |terminal| {
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
        let result = with_window_sftp_terminal(
            &state_for_remote_selection,
            &router_for_remote_selection,
            window_id,
            |terminal| {
                if !terminal.sftp.toggle_selection(path.as_str(), selected) {
                    anyhow::bail!("remote entry is no longer visible");
                }
                Ok(())
            },
        );
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
        let result = with_window_sftp_terminal(
            &state_for_remote_select_all,
            &router_for_remote_select_all,
            window_id,
            |terminal| {
                terminal.sftp.select_all(selected);
                Ok(())
            },
        );
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

    let ui_for_load = ui.as_weak();
    let state_for_load = state.clone();
    let router_for_load = window_router.clone();
    ui.on_load_remote_sftp_file(move |path| {
        log_ui_action("sftp.load-remote-file");
        sync_window_active(&router_for_load, window_id, &state_for_load);
        let result = queue_sftp_write(
            &state_for_load,
            &router_for_load,
            window_id,
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
        let result =
            with_window_sftp_terminal(&state_for_save, &router_for_save, window_id, |terminal| {
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
        let tab_id = match router_for_editor_text.active_tab(window_id) {
            Some(tab_id) => tab_id,
            None => {
                set_status(&ui_for_editor_text, "No active SFTP tab");
                return;
            }
        };
        let result = with_sftp_terminal_for_tab(&state_for_editor_text, tab_id, |terminal| {
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
                app.terminal(tab_id)
                    .filter(|terminal| terminal.is_sftp())
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
            let queued = with_sftp_terminal_for_tab(&state_for_task, tab_id, |terminal| {
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
        match with_window_sftp_terminal(&state_for_auto, &router_for_auto, window_id, |terminal| {
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

    ui.on_drag_local_file_sftp(|path| local_file_drag_data(path.as_str()));
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

    let ui_for_upload = ui.as_weak();
    let state_for_upload = state.clone();
    let runtime_for_upload = runtime.clone();
    let router_for_upload = window_router.clone();
    ui.on_upload_selected_local_sftp(move || {
        log_ui_action("sftp.upload-selected-local");
        sync_window_active(&router_for_upload, window_id, &state_for_upload);
        let prepared =
            prepare_selected_local_upload(&state_for_upload, &router_for_upload, window_id);
        let (tab_id, local_path, expected_size, remote_path) = match prepared {
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
            let queued = queue_upload_for_tab(
                &state_for_upload_task,
                tab_id,
                remote_path,
                local_path,
                expected_size,
            );
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
        let result = with_window_sftp_terminal(
            &state_for_rename,
            &router_for_rename,
            window_id,
            |terminal| {
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
    ui.on_edit_remote_sftp(move || {
        log_ui_action("sftp.edit-remote");
        sync_window_active(&router_for_edit, window_id, &state_for_edit);
        let result =
            with_window_sftp_terminal(&state_for_edit, &router_for_edit, window_id, |terminal| {
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
        let result = with_window_sftp_terminal(
            &state_for_local_selection,
            &router_for_local_selection,
            window_id,
            |terminal| {
                if !terminal
                    .sftp
                    .local
                    .toggle_selection(path.as_str(), selected)
                {
                    anyhow::bail!("local entry is no longer visible");
                }
                Ok(())
            },
        );
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
        let result = with_window_sftp_terminal(
            &state_for_local_select_all,
            &router_for_local_select_all,
            window_id,
            |terminal| {
                terminal.sftp.local.select_all(selected);
                Ok(())
            },
        );
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
        let (tab_id, request_id, request_path) = match queue_local_navigation(
            &state_for_local,
            &router_for_local,
            window_id,
            SftpNavigation::Direct,
            Some(path),
        ) {
            Ok(result) => result,
            Err(error) => {
                set_status(
                    &ui_for_local,
                    &format!("Cannot browse local files: {error}"),
                );
                dispatch_active_snapshot(&ui_for_local, &state_for_local);
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
            request_path,
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

fn queue_remote_downloads(
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
        if filtered > 0 {
            anyhow::bail!("all selected entries are excluded by SFTP transfer filters");
        }
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

fn prepare_local_entry_reveal(
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

fn reveal_local_path(runtime: &Handle, ui: slint::Weak<AppWindow>, path: PathBuf) {
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
fn reveal_local_path_blocking(path: &Path) -> Result<()> {
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
fn reveal_local_path_blocking(path: &Path) -> Result<()> {
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

fn prepare_local_file_open(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    requested_path: &str,
) -> Result<LocalOpenRequest> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
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
    router: &WindowRouter,
    window_id: Uuid,
    kind: SftpNavigation,
    path: Option<String>,
) -> Result<()> {
    with_window_sftp_terminal(state, router, window_id, |terminal| {
        queue_remote_navigation_for_terminal(terminal, kind, path)
    })
}

fn queue_local_navigation(
    state: &Arc<Mutex<AppState>>,
    router: &WindowRouter,
    window_id: Uuid,
    kind: SftpNavigation,
    path: Option<String>,
) -> Result<(Uuid, u64, String)> {
    let tab_id = router
        .active_tab(window_id)
        .context("no active SFTP tab for this window")?;
    let (request_id, request_path) = with_sftp_terminal_for_tab(state, tab_id, |terminal| {
        if !terminal.is_sftp() {
            anyhow::bail!("local files are available only in an SFTP tab");
        }
        terminal.sftp.local.begin_navigation(kind, path)
    })?;
    Ok((tab_id, request_id, request_path))
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
    let (_, request_path) = terminal.sftp.begin_navigation(kind, path)?;
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
    let Some(terminal) = app.terminal_mut(tab_id) else {
        return false;
    };
    if !terminal.is_sftp() || terminal.sftp.local.request_id != request_id {
        return false;
    }
    terminal.sftp.local.complete(
        listing.path,
        listing.entries,
        listing.truncated,
        listing.skipped_entries,
    );
    true
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
    let Some(terminal) = app.terminal_mut(tab_id) else {
        return false;
    };
    if !terminal.is_sftp() || terminal.sftp.local.request_id != request_id {
        return false;
    }
    terminal.sftp.local.fail(message);
    true
}

/// Resolve the SFTP operation target from the originating window route.
/// `AppState`'s process-wide active Tab is presentation state, not an intent
/// context, because another window may be focused at the same time.
fn with_window_sftp_terminal<T>(
    state: &Arc<Mutex<AppState>>,
    router: &WindowRouter,
    window_id: Uuid,
    action: impl FnOnce(&mut TerminalTabState) -> Result<T>,
) -> Result<T> {
    let tab_id = router
        .active_tab(window_id)
        .context("no active SFTP tab for this window")?;
    with_sftp_terminal_for_tab(state, tab_id, action)
}

fn parse_transfer_id(value: &str) -> Result<uuid::Uuid> {
    value
        .parse::<uuid::Uuid>()
        .context("invalid SFTP transfer id")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> AppState {
        AppState::new(
            ConfigStore::new(
                std::env::temp_dir().join(format!("ax-ssh-sftp-{}.json", Uuid::new_v4())),
            ),
            SessionStore::default(),
        )
    }

    #[test]
    fn window_sftp_context_ignores_global_active_tab() {
        let router = WindowRouter::new(slint::Weak::<AppWindow>::default());
        let mut app = test_state();
        let profile = SessionProfile::new("remote", "remote.example", "alice");
        let routed_tab = app.open_sftp_tab(&profile);
        let global_tab = app.open_sftp_tab(&profile);
        app.terminal_mut(routed_tab)
            .expect("routed SFTP tab")
            .connected = true;
        app.terminal_mut(global_tab)
            .expect("global SFTP tab")
            .connected = true;
        assert!(router.activate_tab(MAIN_WINDOW_ID, routed_tab, &mut app));
        assert!(app.activate_tab(global_tab));

        let state = Arc::new(Mutex::new(app));
        with_window_sftp_terminal(&state, &router, MAIN_WINDOW_ID, |terminal| {
            terminal.sftp.status = "routed".to_owned();
            Ok(())
        })
        .expect("window route should resolve an SFTP tab");
        let app = state.lock().expect("state lock");
        assert_eq!(app.active_tab_id(), Some(global_tab));
        assert_eq!(
            app.terminal(routed_tab).expect("routed tab").sftp.status,
            "routed"
        );
        assert_ne!(
            app.terminal(global_tab).expect("global tab").sftp.status,
            "routed"
        );
    }

    #[test]
    fn active_sftp_upload_target_revalidates_live_readiness() {
        let router = WindowRouter::new(slint::Weak::<AppWindow>::default());
        let mut app = test_state();
        let profile = SessionProfile::new("remote", "remote.example", "alice");
        let tab_id = app.open_sftp_tab(&profile);
        assert!(router.activate_tab(MAIN_WINDOW_ID, tab_id, &mut app));
        let state = Arc::new(Mutex::new(app));

        let error = active_sftp_upload_target(&state, &router, MAIN_WINDOW_ID)
            .expect_err("a disconnected SFTP tab must reject an upload");
        assert!(error.to_string().contains("no longer connected"));

        {
            let mut app = state.lock().expect("state lock");
            let terminal = app.terminal_mut(tab_id).expect("SFTP tab");
            terminal.connected = true;
            terminal.sftp.path = " /remote/inbox ".to_owned();
            terminal.sftp.loading = true;
        }
        let error = active_sftp_upload_target(&state, &router, MAIN_WINDOW_ID)
            .expect_err("a loading SFTP directory must reject an upload");
        assert!(error.to_string().contains("still loading"));

        {
            let mut app = state.lock().expect("state lock");
            app.terminal_mut(tab_id).expect("SFTP tab").sftp.loading = false;
        }
        assert_eq!(
            active_sftp_upload_target(&state, &router, MAIN_WINDOW_ID)
                .expect("a ready SFTP tab should resolve an upload target"),
            (tab_id, "/remote/inbox".to_owned())
        );
    }

    #[test]
    fn dropped_local_paths_parses_multiple_lines() {
        let paths =
            parse_dropped_local_paths("file:///tmp/a.txt\n/tmp/b.txt\nfile:///tmp/c%20d.txt")
                .unwrap();
        assert_eq!(paths.len(), 3);
        assert_eq!(paths[0], PathBuf::from("/tmp/a.txt"));
        assert_eq!(paths[1], PathBuf::from("/tmp/b.txt"));
        assert_eq!(paths[2], PathBuf::from("/tmp/c d.txt"));
    }

    #[test]
    fn dropped_local_paths_rejects_invalid_uri_escape() {
        assert!(parse_dropped_local_paths("file:///tmp/bad%2").is_err());
    }

    #[test]
    fn dropped_local_paths_accepts_localhost_authority() {
        let paths = parse_dropped_local_paths("file://localhost/tmp/ok.txt").unwrap();
        assert_eq!(paths, vec![PathBuf::from("/tmp/ok.txt")]);
    }

    #[test]
    fn dropped_local_paths_deduplicates() {
        let paths = parse_dropped_local_paths("/tmp/a.txt\n/tmp/a.txt\nfile:///tmp/a.txt").unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], PathBuf::from("/tmp/a.txt"));
    }

    #[test]
    fn dropped_local_paths_skips_blank_lines() {
        let paths = parse_dropped_local_paths("\n\n/tmp/x.txt\n\n").unwrap();
        assert_eq!(paths, vec![PathBuf::from("/tmp/x.txt")]);
    }

    #[test]
    fn dropped_local_paths_rejects_empty() {
        assert!(parse_dropped_local_paths("\n\n  \n").is_err());
    }

    #[test]
    fn dropped_local_paths_rejects_remote_host_in_uri_list() {
        assert!(
            parse_dropped_local_paths("file:///tmp/ok.txt\nfile://evil-host/tmp/bad.txt").is_err()
        );
    }

    #[test]
    fn dropped_local_paths_is_bounded() {
        let text = (0..=MAX_DROPPED_LOCAL_PATHS)
            .map(|index| format!("/tmp/file-{index}.txt"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(parse_dropped_local_paths(&text).is_err());
    }

    #[test]
    fn dropped_local_paths_rejects_oversized_payload() {
        let text = " ".repeat(MAX_DROPPED_LOCAL_DATA_BYTES + 1);
        assert!(parse_dropped_local_paths(&text).is_err());
    }

    #[test]
    fn sftp_drag_payload_distinguishes_local_and_remote_paths() {
        match parse_sftp_drag_payload(&format!("{LOCAL_DRAG_PREFIX}/tmp/a.txt")).unwrap() {
            SftpDragPayload::Local(paths) => assert_eq!(paths, vec![PathBuf::from("/tmp/a.txt")]),
            SftpDragPayload::Remote(_) => panic!("local drag payload was classified as remote"),
        }
        match parse_sftp_drag_payload(&format!("{REMOTE_DRAG_PREFIX}/var/log/a.txt")).unwrap() {
            SftpDragPayload::Remote(path) => assert_eq!(path, "/var/log/a.txt"),
            SftpDragPayload::Local(_) => panic!("remote drag payload was classified as local"),
        }
    }
}
