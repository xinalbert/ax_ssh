use super::super::*;

const MAX_WORKSPACE_FILE_PATH_BYTES: usize = 4096;

fn remap_workspace_snapshot_ids(
    mut snapshot: ax_ssh::config::WorkspaceSnapshot,
) -> ax_ssh::config::WorkspaceSnapshot {
    let remap = snapshot
        .tabs
        .iter()
        .map(|tab| (tab.id, Uuid::new_v4()))
        .collect::<HashMap<_, _>>();
    let map_id = |id: Uuid| remap.get(&id).copied().unwrap_or(id);
    for tab in &mut snapshot.tabs {
        tab.id = map_id(tab.id);
        tab.companion_tab_id = tab.companion_tab_id.map(map_id);
    }
    snapshot.active_tab_id = snapshot.active_tab_id.map(map_id);
    for window in &mut snapshot.windows {
        window.tab_ids = window.tab_ids.iter().copied().map(map_id).collect();
        window.active_tab_id = window.active_tab_id.map(map_id);
        window.focused_tab_id = window.focused_tab_id.map(map_id);
        window.panes = window
            .panes
            .drain(..)
            .map(|pane| remap_pane_ids(pane, &remap))
            .collect();
    }
    snapshot
}

fn remap_pane_ids(
    pane: ax_ssh::config::PaneNodeSnapshot,
    remap: &HashMap<Uuid, Uuid>,
) -> ax_ssh::config::PaneNodeSnapshot {
    match pane {
        ax_ssh::config::PaneNodeSnapshot::Leaf(id) => {
            ax_ssh::config::PaneNodeSnapshot::Leaf(remap.get(&id).copied().unwrap_or(id))
        }
        ax_ssh::config::PaneNodeSnapshot::Split {
            axis,
            ratio_milli,
            first,
            second,
        } => ax_ssh::config::PaneNodeSnapshot::Split {
            axis,
            ratio_milli,
            first: Box::new(remap_pane_ids(*first, remap)),
            second: Box::new(remap_pane_ids(*second, remap)),
        },
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::app) fn wire_workspace_file_actions(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    font_registry: Arc<Mutex<FontRegistry>>,
    log_directory: PathBuf,
    window_router: WindowRouter,
    detached_windows: Rc<RefCell<HashMap<Uuid, AppWindow>>>,
) {
    let pending_snapshot: Arc<Mutex<Option<ax_ssh::config::WorkspaceSnapshot>>> =
        Arc::new(Mutex::new(None));
    let workspace_open_active = Arc::new(AtomicBool::new(false));
    let pending_snapshot_for_apply = pending_snapshot.clone();
    let workspace_open_for_apply = workspace_open_active.clone();
    let state_for_apply = state.clone();
    let runtime_for_apply = runtime.clone();
    let font_registry_for_apply = font_registry.clone();
    let log_directory_for_apply = log_directory.clone();
    let router_for_apply = window_router.clone();
    let windows_for_apply = detached_windows.clone();
    let ui_for_apply = ui.as_weak();
    ui.on_workspace_file_loaded(move || {
        let Some(ui) = ui_for_apply.upgrade() else {
            workspace_open_for_apply.store(false, std::sync::atomic::Ordering::Release);
            return;
        };
        let snapshot = match pending_snapshot_for_apply.lock() {
            Ok(mut pending) => pending.take(),
            Err(_) => {
                workspace_open_for_apply.store(false, std::sync::atomic::Ordering::Release);
                ui.set_status("Cannot apply workspace state".into());
                return;
            }
        };
        let Some(snapshot) = snapshot else {
            workspace_open_for_apply.store(false, std::sync::atomic::Ordering::Release);
            return;
        };
        workspace_open_for_apply.store(false, std::sync::atomic::Ordering::Release);
        release_detached_windows(&windows_for_apply);
        router_for_apply.discard_detached();
        if let Ok(mut app) = state_for_apply.lock() {
            let _ = app.restore_workspace_tabs(&snapshot.tabs);
            router_for_apply.apply_snapshot(&snapshot, &mut app);
        } else {
            ui.set_status("Cannot apply workspace state".into());
            return;
        }
        let connection = ConnectionContext::new(
            ui.as_weak(),
            state_for_apply.clone(),
            runtime_for_apply.clone(),
            font_registry_for_apply.clone(),
            Arc::new(AtomicBool::new(false)),
        );
        for (tab_id, profile_id, target) in state_for_apply
            .lock()
            .ok()
            .map(|app| app.restored_connection_targets())
            .unwrap_or_default()
        {
            resume_existing_connection(&connection, tab_id, profile_id, target);
        }
        for tab_id in state_for_apply
            .lock()
            .ok()
            .map(|app| app.restored_local_tabs())
            .unwrap_or_default()
        {
            if let Err(error) = resume_existing_local_shell(
                &runtime_for_apply,
                state_for_apply.clone(),
                ui.as_weak(),
                tab_id,
            ) {
                warn!(%error, tab_id = %tab_id, "failed to restore local shell");
            }
        }
        restore_detached_workspaces(
            &snapshot,
            &state_for_apply,
            &runtime_for_apply,
            &font_registry_for_apply,
            &log_directory_for_apply,
            &router_for_apply,
            &windows_for_apply,
        );
        refresh_workspace(&ui.as_weak(), &state_for_apply);
        ui.set_status(format!("Workspace opened ({} tabs)", snapshot.tabs.len()).into());
    });

    let ui_for_action = ui.as_weak();
    let state_for_action = state.clone();
    let router_for_action = window_router.clone();
    ui.on_workspace_file_action(move |mode, raw_path| {
        let path = match workspace_file_path(raw_path.as_str()) {
            Ok(path) => path,
            Err(error) => {
                set_status(&ui_for_action, &error.to_string());
                return;
            }
        };
        match mode.as_str() {
            "save" => save_workspace_file(
                &ui_for_action,
                &state_for_action,
                &runtime,
                &router_for_action,
                path,
            ),
            "open" => open_workspace_file(
                &ui_for_action,
                &state_for_action,
                &runtime,
                &pending_snapshot,
                &workspace_open_active,
                path,
            ),
            _ => set_status(&ui_for_action, "Unknown workspace file action"),
        }
    });
}

fn workspace_file_path(raw_path: &str) -> Result<PathBuf> {
    let path = raw_path.trim();
    if path.is_empty() {
        anyhow::bail!("Workspace file path cannot be empty");
    }
    if path.len() > MAX_WORKSPACE_FILE_PATH_BYTES {
        anyhow::bail!("Workspace file path is too long");
    }
    if path.chars().any(char::is_control) {
        anyhow::bail!("Workspace file path contains a control character");
    }
    Ok(PathBuf::from(path))
}

fn save_workspace_file(
    ui: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    runtime: &Handle,
    window_router: &WindowRouter,
    path: PathBuf,
) {
    let snapshot = match state.lock() {
        Ok(app) => window_router.snapshot(&app),
        Err(_) => {
            set_status(ui, "Cannot read workspace state");
            return;
        }
    };
    let path_for_task = path.clone();
    let ui = ui.clone();
    runtime.spawn(async move {
        let result = match tokio::task::spawn_blocking(move || {
            ConfigStore::save_workspace_file(&path_for_task, &snapshot)
        })
        .await
        {
            Ok(result) => {
                result.with_context(|| format!("failed to save workspace {}", path.display()))
            }
            Err(error) => Err(anyhow::anyhow!("workspace save task failed: {error}")),
        };
        match result {
            Ok(()) => set_status(&ui, &format!("Workspace saved to {}", path.display())),
            Err(error) => set_status(&ui, &format!("Workspace save failed: {error}")),
        }
    });
}

fn open_workspace_file(
    ui: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    runtime: &Handle,
    pending_snapshot: &Arc<Mutex<Option<ax_ssh::config::WorkspaceSnapshot>>>,
    workspace_open_active: &Arc<AtomicBool>,
    path: PathBuf,
) {
    if workspace_open_active
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        )
        .is_err()
    {
        set_status(ui, "A workspace is already opening");
        return;
    }
    let workspace_open_active = workspace_open_active.clone();
    let ui = ui.clone();
    let state = state.clone();
    let pending_snapshot = pending_snapshot.clone();
    runtime.spawn(async move {
        let reset_open_gate = || {
            workspace_open_active.store(false, std::sync::atomic::Ordering::Release);
        };
        let path_for_task = path.clone();
        let snapshot = match tokio::task::spawn_blocking(move || {
            ConfigStore::load_workspace_file(&path_for_task)
        })
        .await
        {
            Ok(Ok(snapshot)) => snapshot,
            Ok(Err(error)) => {
                reset_open_gate();
                set_status(&ui, &format!("Workspace open failed: {error}"));
                return;
            }
            Err(error) => {
                reset_open_gate();
                set_status(&ui, &format!("Workspace load task failed: {error}"));
                return;
            }
        };

        let snapshot = remap_workspace_snapshot_ids(snapshot);
        let (workers, pending_probes) = match state.lock() {
            Ok(mut app) => app.drain_runtime_resources(),
            Err(_) => {
                reset_open_gate();
                set_status(&ui, "Cannot stop current workspace");
                return;
            }
        };
        for pending_probe in pending_probes {
            let _ = pending_probe.cancel.send(());
        }
        for worker in workers {
            if let Err(error) = worker.shutdown().await {
                warn!(%error, "failed to shut down worker before workspace replacement");
            }
        }

        if let Ok(mut pending) = pending_snapshot.lock() {
            *pending = Some(snapshot);
        } else {
            reset_open_gate();
            set_status(&ui, "Cannot queue workspace state");
            return;
        }
        if !dispatch_ui_result(&ui, move |ui| ui.invoke_workspace_file_loaded()) {
            reset_open_gate();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_file_path_rejects_empty_long_and_control_input() {
        assert!(workspace_file_path("  ").is_err());
        assert!(workspace_file_path(&"x".repeat(MAX_WORKSPACE_FILE_PATH_BYTES + 1)).is_err());
        assert!(workspace_file_path("workspace\n.json").is_err());
    }

    #[test]
    fn workspace_file_path_trims_surrounding_whitespace() {
        assert_eq!(
            workspace_file_path("  /tmp/team workspace.json  ")
                .expect("valid workspace path should be accepted"),
            PathBuf::from("/tmp/team workspace.json")
        );
    }

    #[test]
    fn opening_a_workspace_remaps_tab_and_pane_identities() {
        let tab_id = Uuid::new_v4();
        let snapshot = ax_ssh::config::WorkspaceSnapshot {
            version: ax_ssh::config::WORKSPACE_SNAPSHOT_VERSION,
            tabs: vec![ax_ssh::config::WorkspaceTabSnapshot {
                id: tab_id,
                companion_tab_id: Some(tab_id),
                kind: "terminal".to_owned(),
                ..Default::default()
            }],
            active_tab_id: Some(tab_id),
            windows: vec![ax_ssh::config::WorkspaceWindowSnapshot {
                panes: vec![ax_ssh::config::PaneNodeSnapshot::Leaf(tab_id)],
                tab_ids: vec![tab_id],
                active_tab_id: Some(tab_id),
                focused_tab_id: Some(tab_id),
                ..Default::default()
            }],
        };
        let remapped = remap_workspace_snapshot_ids(snapshot);
        let new_id = remapped.tabs[0].id;
        assert_ne!(new_id, tab_id);
        assert_eq!(remapped.active_tab_id, Some(new_id));
        assert_eq!(remapped.tabs[0].companion_tab_id, Some(new_id));
        assert_eq!(remapped.windows[0].tab_ids, vec![new_id]);
        assert_eq!(
            remapped.windows[0].panes,
            vec![ax_ssh::config::PaneNodeSnapshot::Leaf(new_id)]
        );
    }
}
