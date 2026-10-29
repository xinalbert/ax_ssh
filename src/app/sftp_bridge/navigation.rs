//! SFTP navigation intent wiring and bounded application operations.

use super::*;

pub(super) fn wire_navigation(
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

    let ui_for_up = ui.as_weak();
    let state_for_up = state.clone();
    let router_for_up = window_router.clone();
    ui.on_navigate_sftp_up(move || {
        log_ui_action("sftp.navigate-up");
        sync_window_active(&router_for_up, window_id, &state_for_up);
        let result = queue_remote_navigation(
            &state_for_up,
            &router_for_up,
            window_id,
            SftpNavigation::Up,
            None,
        );
        match result {
            Ok(()) => dispatch_active_snapshot(&ui_for_up, &state_for_up),
            Err(error) => {
                set_status(&ui_for_up, &format!("Cannot go up in SFTP: {error}"));
                dispatch_active_snapshot(&ui_for_up, &state_for_up);
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

    let ui_for_local_up = ui.as_weak();
    let state_for_local_up = state.clone();
    let router_for_local_up = window_router.clone();
    let runtime_for_local_up = runtime.clone();
    ui.on_navigate_local_sftp_up(move || {
        log_ui_action("sftp.navigate-local-up");
        sync_window_active(&router_for_local_up, window_id, &state_for_local_up);
        match queue_local_navigation(
            &state_for_local_up,
            &router_for_local_up,
            window_id,
            SftpNavigation::Up,
            None,
        ) {
            Ok((tab_id, request_id, path)) => {
                dispatch_active_snapshot(&ui_for_local_up, &state_for_local_up);
                load_local_directory(
                    &runtime_for_local_up,
                    state_for_local_up.clone(),
                    ui_for_local_up.clone(),
                    tab_id,
                    request_id,
                    path,
                );
            }
            Err(error) => {
                set_status(
                    &ui_for_local_up,
                    &format!("Cannot go up in local files: {error}"),
                );
                dispatch_active_snapshot(&ui_for_local_up, &state_for_local_up);
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
                let request_id = terminal.sftp.begin_load_more()?;
                let result = terminal
                    .worker
                    .as_ref()
                    .context("active SSH terminal has no worker")?
                    .request_load_more_sftp(request_id);
                if let Err(error) = result {
                    terminal.sftp.cancel_navigation();
                    return Err(error);
                }
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

    let ui_for_local = ui.as_weak();
    let state_for_local = state;
    let router_for_local = window_router;
    ui.on_list_local_sftp_directory(move |path| {
        log_ui_action("sftp.list-local");
        sync_window_active(&router_for_local, window_id, &state_for_local);
        let path = path.as_str().trim().to_owned();
        if path.is_empty() {
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

pub(super) fn queue_remote_navigation(
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

pub(super) fn queue_local_navigation(
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

pub(in crate::app) fn navigate_sftp_tab_to_path(
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

pub(super) fn queue_remote_navigation_for_terminal(
    terminal: &mut TerminalTabState,
    kind: SftpNavigation,
    path: Option<String>,
) -> Result<()> {
    let worker = terminal
        .worker
        .as_ref()
        .context("active SSH terminal has no worker")?;
    let (request_id, request_path) = terminal.sftp.begin_navigation(kind, path)?;
    let result = worker.request_list_sftp(request_id, request_path);
    if let Err(error) = result {
        terminal.sftp.cancel_navigation();
        terminal.sftp.status = "SFTP directory request was rejected".to_owned();
        return Err(error);
    }
    Ok(())
}

pub(in crate::app) fn load_local_directory(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: uuid::Uuid,
    request_id: u64,
    path: String,
) {
    let runtime = runtime.clone();
    let runtime_for_icons = runtime.clone();
    runtime.clone().spawn(async move {
        let listed = tokio::time::timeout(
            LOCAL_DIRECTORY_TIMEOUT,
            tokio::task::spawn_blocking(move || read_local_directory(&path)),
        )
        .await;
        let mut icon_keys = Vec::new();
        let mut watch_loaded_directory = false;
        let message = match listed {
            Ok(Ok(Ok(listing))) => {
                icon_keys = local_icon_keys(&listing.entries);
                let applied = apply_local_directory_listing(&state, tab_id, request_id, listing);
                watch_loaded_directory = applied;
                applied
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
            if watch_loaded_directory {
                super::local_directory_watch::ensure_local_directory_watch(
                    &runtime,
                    state.clone(),
                    ui.clone(),
                    tab_id,
                );
            }
            let pending = state.lock().ok().and_then(|mut app| {
                let terminal = app.terminal_mut(tab_id)?;
                if !terminal.is_sftp() {
                    return None;
                }
                match terminal.sftp.local.begin_pending_auto_refresh() {
                    Ok(request) => request,
                    Err(error) => {
                        warn!(tab_id = %tab_id, %error, "could not finish pending local directory refresh");
                        None
                    }
                }
            });
            dispatch_active_snapshot(&ui, &state);
            prewarm_file_icons(&runtime_for_icons, icon_keys, &ui, &state);
            if let Some((request_id, path)) = pending {
                load_local_directory(&runtime, state, ui, tab_id, request_id, path);
            }
        }
    });
}

pub(super) fn apply_local_directory_listing(
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

pub(super) fn apply_local_directory_failure(
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
