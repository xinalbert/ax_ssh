use super::local_files::{
    LOCAL_DIRECTORY_PATH_LIMIT, LocalDirectoryEntry, read_local_directory,
    validate_local_file_for_open,
};
use super::*;

const LOCAL_DIRECTORY_TIMEOUT: Duration = Duration::from_secs(5);
const LOCAL_OPEN_TIMEOUT: Duration = Duration::from_secs(5);
const LOCAL_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(30 * 60);

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
            if entry.is_dir {
                anyhow::bail!("directories must be opened by navigation");
            }
            if entry.is_symlink {
                anyhow::bail!("symbolic links cannot be downloaded in this version");
            }
            let transfer_id = uuid::Uuid::new_v4();
            terminal
                .sftp
                .queue_transfer(transfer_id, entry.name, entry.size)?;
            let request = terminal
                .worker
                .as_ref()
                .context("active SFTP tab has no worker")?
                .request_open_sftp_file(transfer_id, entry.path);
            if let Err(error) = request {
                terminal.sftp.finish_transfer(
                    transfer_id,
                    SftpTransferPhase::Failed,
                    "Download request was rejected".to_owned(),
                );
                return Err(error);
            }
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
        let result = id
            .as_str()
            .parse::<uuid::Uuid>()
            .context("invalid SFTP transfer id")
            .and_then(|transfer_id| {
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
    })
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

fn with_active_sftp_terminal(
    state: &Arc<Mutex<AppState>>,
    action: impl FnOnce(&mut TerminalTabState) -> Result<()>,
) -> Result<()> {
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
