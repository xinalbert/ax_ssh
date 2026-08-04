use super::local_files::{LOCAL_DIRECTORY_PATH_LIMIT, read_local_directory};
use super::*;

const LOCAL_DIRECTORY_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn wire_sftp(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    let ui_for_list = ui.as_weak();
    let state_for_list = state.clone();
    ui.on_list_sftp_directory(move |path| {
        log_ui_action("sftp.list-remote");
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
    ui.on_navigate_sftp_back(move || {
        log_ui_action("sftp.navigate-back");
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
    ui.on_navigate_sftp_forward(move || {
        log_ui_action("sftp.navigate-forward");
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
    ui.on_load_more_sftp(move || {
        log_ui_action("sftp.load-more");
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
    ui.on_close_sftp(move || {
        log_ui_action("sftp.close");
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
    ui.on_toggle_remote_sftp_selection(move |path, selected| {
        log_ui_action("sftp.toggle-remote-selection");
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
    ui.on_select_all_remote_sftp(move |selected| {
        log_ui_action("sftp.select-all-remote");
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

    let ui_for_local_selection = ui.as_weak();
    let state_for_local_selection = state.clone();
    ui.on_toggle_local_sftp_selection(move |path, selected| {
        log_ui_action("sftp.toggle-local-selection");
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
    ui.on_select_all_local_sftp(move |selected| {
        log_ui_action("sftp.select-all-local");
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

    let ui_for_local = ui.as_weak();
    let state_for_local = state;
    ui.on_list_local_sftp_directory(move |path| {
        log_ui_action("sftp.list-local");
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
    runtime.spawn(async move {
        let listed = tokio::time::timeout(
            LOCAL_DIRECTORY_TIMEOUT,
            tokio::task::spawn_blocking(move || read_local_directory(&path)),
        )
        .await;
        let message = match listed {
            Ok(Ok(Ok(listing))) => {
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
