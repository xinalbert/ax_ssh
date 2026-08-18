use super::*;

pub(in crate::app) fn refresh_workspace(ui: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    if let Some(router) = global_window_router() {
        refresh_workspace_multi_window(ui, state, &router, None);
        return;
    }
    let state = Arc::clone(state);
    dispatch_ui(ui, move |ui| {
        let (tabs, snapshot) = match state.lock() {
            Ok(app) => (
                visible_workspace_tab_rows(app.tab_summaries()),
                app.active_snapshot(),
            ),
            Err(_) => {
                ui.set_status("State lock poisoned".into());
                return;
            }
        };
        let tab_count = tabs.len();
        apply_menu_workspace_state(ui, snapshot.kind, tab_count, snapshot.id.is_some());
        let settings_tab_id = tabs
            .iter()
            .find(|tab| tab.kind.as_str() == "settings")
            .map(|tab| tab.id.clone())
            .unwrap_or_default();
        apply_workspace_tab_rows(ui, tabs);
        ui.set_settings_tab_id(settings_tab_id);
        apply_active_snapshot(ui, snapshot, None);
        ui.set_terminal_panes(ModelRc::new(VecModel::from(Vec::<TerminalPaneView>::new())));
        ui.set_terminal_dividers(ModelRc::new(VecModel::from(
            Vec::<TerminalPaneDividerView>::new(),
        )));
    });
}

pub(super) fn refresh_workspace_multi_window(
    ui: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    router: &WindowRouter,
    output_received_at: Option<Instant>,
) {
    let should_schedule = match state.lock() {
        Ok(app) => app.try_schedule_ui_refresh(),
        Err(_) => {
            set_status(ui, "State lock poisoned");
            return;
        }
    };
    if !should_schedule {
        let _ = COALESCED_WORKSPACE_REFRESHES.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |count| Some(count.saturating_add(1)),
        );
        return;
    }

    let state = Arc::clone(state);
    let router = router.clone();
    let ui_for_follow_up = ui.clone();
    let dispatch_requested_at = Instant::now();
    let state_for_ui = Arc::clone(&state);
    if slint::invoke_from_event_loop(move || {
        let ui_started_at = Instant::now();
        let views_started_at = Instant::now();
        let views = match state_for_ui.lock() {
            Ok(app) => router.views(&app),
            Err(poisoned) => {
                poisoned.into_inner().clear_ui_refresh_pending();
                return;
            }
        };
        let views_built_us = duration_micros(views_started_at.elapsed());
        let mut applied_view_count = 0usize;
        for view in views {
            let Some(ui) = view.ui.upgrade() else {
                continue;
            };
            let tab_count = view.tabs.len();
            apply_menu_workspace_state(
                &ui,
                view.snapshot.kind,
                tab_count,
                view.snapshot.id.is_some(),
            );
            let settings_tab_id = view
                .tabs
                .iter()
                .find(|tab| tab.kind == "settings")
                .map(|tab| tab.id.to_string())
                .unwrap_or_default();
            apply_workspace_tab_rows(&ui, visible_workspace_tab_rows(view.tabs));
            ui.set_settings_tab_id(settings_tab_id.into());
            apply_active_snapshot(&ui, view.snapshot, view.active_tab_id);
            apply_terminal_panes(&ui, view.terminal_panes, view.terminal_dividers);
            applied_view_count = applied_view_count.saturating_add(1);
        }
        if let Ok(app) = state_for_ui.lock() {
            app.clear_ui_refresh_pending();
        }
        let coalesced_refreshes = COALESCED_WORKSPACE_REFRESHES.swap(0, Ordering::AcqRel);
        tracing::debug!(
            target: "ax_ssh::latency",
            event = "workspace-refresh",
            stage = "ui-applied",
            view_count = applied_view_count,
            coalesced_refreshes,
            views_built_us,
            ui_queue_us = duration_micros(
                ui_started_at.saturating_duration_since(dispatch_requested_at),
            ),
            ui_apply_us = duration_micros(ui_started_at.elapsed()),
            output_to_ui_us = output_received_at
                .map(|received_at| duration_micros(received_at.elapsed())),
            "multi-window workspace views applied to UI"
        );
        if coalesced_refreshes > 0 {
            refresh_workspace_multi_window(&ui_for_follow_up, &state_for_ui, &router, None);
        }
    })
    .is_err()
    {
        if let Ok(app) = state.lock() {
            app.clear_ui_refresh_pending();
        }
        tracing::debug!(
            target: "ax_ssh::latency",
            event = "workspace-refresh",
            stage = "dispatch-rejected",
            "multi-window workspace refresh could not enter the UI event loop"
        );
    }
}

pub(in crate::app) fn visible_workspace_tab_rows(
    tabs: Vec<WorkspaceTabSummary>,
) -> Vec<WorkspaceTabRow> {
    tabs.into_iter()
        .map(|tab| WorkspaceTabRow {
            id: tab.id.to_string().into(),
            title: tab.title.into(),
            kind: tab.kind.into(),
            connected: tab.connected,
        })
        .collect()
}

fn apply_workspace_tab_rows(ui: &AppWindow, tabs: Vec<WorkspaceTabRow>) {
    let current = ui.get_workspace_tabs();
    if current.row_count() == tabs.len()
        && tabs.iter().enumerate().all(|(index, tab)| {
            current.row_data(index).is_some_and(|existing| {
                existing.id == tab.id
                    && existing.title == tab.title
                    && existing.kind == tab.kind
                    && existing.connected == tab.connected
            })
        })
    {
        return;
    }

    if current.row_count() == tabs.len()
        && tabs.iter().enumerate().all(|(index, tab)| {
            current
                .row_data(index)
                .is_some_and(|existing| existing.id == tab.id)
        })
    {
        for (index, tab) in tabs.into_iter().enumerate() {
            let changed = current.row_data(index).is_none_or(|existing| {
                existing.title != tab.title
                    || existing.kind != tab.kind
                    || existing.connected != tab.connected
            });
            if changed {
                current.set_row_data(index, tab);
            }
        }
        return;
    }

    ui.set_workspace_tabs(ModelRc::new(VecModel::from(tabs)));
}

fn apply_menu_workspace_state(
    ui: &AppWindow,
    active_kind: &str,
    tab_count: usize,
    has_active_tab: bool,
) {
    let terminal_active = active_kind == "terminal";
    if ui.get_menu_terminal_active() != terminal_active {
        ui.set_menu_terminal_active(terminal_active);
    }
    let has_multiple_tabs = tab_count > 1;
    if ui.get_menu_has_multiple_tabs() != has_multiple_tabs {
        ui.set_menu_has_multiple_tabs(has_multiple_tabs);
    }
    if ui.get_menu_has_active_tab() != has_active_tab {
        ui.set_menu_has_active_tab(has_active_tab);
    }
}
