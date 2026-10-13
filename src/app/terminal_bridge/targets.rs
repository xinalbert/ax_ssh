//! Terminal targets application boundary.

use super::*;

pub(super) fn terminal_target_modifier_held(control: bool, _meta: bool) -> bool {
    // Slint normalizes the platform primary shortcut modifier into `control`:
    // Cmd on macOS and Ctrl elsewhere.
    control
}

pub(super) fn terminal_semantic_selection(
    range: TerminalSelectionRange,
) -> TerminalSemanticSelection {
    TerminalSemanticSelection {
        active: true,
        anchor_row: range.start_row as i32,
        anchor_column: range.start_column as i32,
        focus_row: range.end_row as i32,
        focus_column: range.end_column as i32,
    }
}

pub(super) fn terminal_url_selection_range(
    terminal: &TerminalModel,
    row: usize,
    column: usize,
) -> Option<TerminalSelectionRange> {
    let context = terminal.visible_logical_line_target_context_at_cell(row, column)?;
    let target_match = terminal_target_match_at_context(&context)?;
    if !matches!(target_match.target, TerminalTarget::Url(_)) {
        return None;
    }

    let first = target_match.segments.first()?;
    let last = target_match.segments.last()?;
    let (start_column, _) =
        terminal.visible_row_cell_span_for_characters(first.row, first.start, first.end)?;
    let (_, end_column_exclusive) =
        terminal.visible_row_cell_span_for_characters(last.row, last.start, last.end)?;
    let end_column = end_column_exclusive.checked_sub(1)?;

    Some(TerminalSelectionRange {
        start_row: first.row,
        start_column,
        end_row: last.row,
        end_column,
    })
}

pub(super) fn terminal_target_for_pane(
    state: &Arc<Mutex<AppState>>,
    window_router: &WindowRouter,
    window_id: Uuid,
    tab_id: Uuid,
    row: i32,
    column: i32,
) -> Option<TerminalTarget> {
    let row = usize::try_from(row).ok()?;
    let column = usize::try_from(column).ok()?;
    let app = state.lock().ok()?;
    if !window_router.owns_terminal_pane(window_id, tab_id, &app) {
        return None;
    }
    let terminal = app.terminal(tab_id)?;
    if !terminal.connected {
        return None;
    }
    let terminal_model = terminal.terminal.as_ref()?;
    if let Some((uri, _, _)) = terminal_model.hyperlink_at_cell(row, column) {
        return Some(TerminalTarget::Url(uri));
    }
    let context = terminal_model.visible_logical_line_target_context_at_cell(row, column)?;
    terminal_target_match_at_context(&context).map(|target_match| target_match.target)
}

pub(super) fn terminal_target_highlight_for_pane(
    state: &Arc<Mutex<AppState>>,
    window_router: &WindowRouter,
    window_id: Uuid,
    tab_id: Uuid,
    row: i32,
    column: i32,
) -> Option<TerminalTargetHighlight> {
    let row = usize::try_from(row).ok()?;
    let column = usize::try_from(column).ok()?;
    let app = state.lock().ok()?;
    if !window_router.owns_terminal_pane(window_id, tab_id, &app) {
        return None;
    }
    let terminal = app.terminal(tab_id)?;
    if !terminal.connected {
        return None;
    }
    let terminal = terminal.terminal.as_ref()?;
    if let Some((_, start, end)) = terminal.hyperlink_at_cell(row, column) {
        return Some(TerminalTargetHighlight {
            active: true,
            segments: ModelRc::new(VecModel::from(vec![TerminalTargetHighlightSegment {
                row: i32::try_from(row).ok()?,
                start_column: i32::try_from(start).ok()?,
                end_column: i32::try_from(end).ok()?,
            }])),
        });
    }
    let context: TerminalTargetContext =
        terminal.visible_logical_line_target_context_at_cell(row, column)?;
    let target_match = terminal_target_match_at_context(&context)?;
    let segments = target_match
        .segments
        .into_iter()
        .filter_map(|segment| {
            let (start_column, end_column) = terminal.visible_row_cell_span_for_characters(
                segment.row,
                segment.start,
                segment.end,
            )?;
            Some(TerminalTargetHighlightSegment {
                row: i32::try_from(segment.row).ok()?,
                start_column: i32::try_from(start_column).ok()?,
                end_column: i32::try_from(end_column).ok()?,
            })
        })
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return None;
    }
    Some(TerminalTargetHighlight {
        active: true,
        segments: ModelRc::new(VecModel::from(segments)),
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn open_terminal_remote_path(
    state: &Arc<Mutex<AppState>>,
    window_router: &WindowRouter,
    window_id: Uuid,
    terminal_tab_id: Uuid,
    path: String,
    ui: &slint::Weak<AppWindow>,
    runtime: &Handle,
    font_registry: &Arc<Mutex<FontRegistry>>,
    terminal_font_started: &Arc<std::sync::atomic::AtomicBool>,
) {
    enum PathRoute {
        ExistingSftp(Uuid),
        NewSftp(Uuid),
    }

    let route = (|| -> Result<PathRoute> {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        if !window_router.owns_terminal_pane(window_id, terminal_tab_id, &app) {
            anyhow::bail!("terminal pane is no longer visible in this window");
        }
        let profile_id = {
            let terminal = app
                .terminal(terminal_tab_id)
                .context("terminal tab is no longer available")?;
            if !terminal.connected {
                anyhow::bail!("terminal session is not connected");
            }
            terminal
                .ssh_route()
                .map(|(profile_id, _)| profile_id)
                .context("remote paths require an SSH terminal")?
        };
        if let Some(sftp_tab_id) = app.sftp_companion_id(terminal_tab_id) {
            if !window_router
                .tab_ids(window_id, &app)
                .contains(&sftp_tab_id)
            {
                anyhow::bail!("SFTP companion is not in this window");
            }
            if !app.activate_tab(sftp_tab_id) {
                anyhow::bail!("SFTP companion is no longer available");
            }
            Ok(PathRoute::ExistingSftp(sftp_tab_id))
        } else {
            Ok(PathRoute::NewSftp(profile_id))
        }
    })();

    match route {
        Ok(PathRoute::ExistingSftp(sftp_tab_id)) => {
            window_router.set_active(window_id, sftp_tab_id);
            match navigate_sftp_tab_to_path(state, sftp_tab_id, path) {
                Ok(()) => {
                    log_ui_action_outcome("terminal.open-target", "sftp-existing");
                    refresh_workspace(ui, state);
                }
                Err(error) => {
                    log_ui_action_outcome("terminal.open-target", "sftp-unavailable");
                    set_status(ui, &format!("Cannot open SFTP location: {error}"));
                    refresh_workspace(ui, state);
                }
            }
        }
        Ok(PathRoute::NewSftp(profile_id)) => {
            let connection = ConnectionContext::new(
                ui.clone(),
                state.clone(),
                runtime.clone(),
                font_registry.clone(),
                terminal_font_started.clone(),
            );
            let _ = request_profile_connection(
                &connection,
                profile_id,
                ConnectionTarget::Sftp,
                Some(terminal_tab_id),
                Some(path),
                None,
                {
                    let router = window_router.clone();
                    move |new_tab_id, app| {
                        router.include_tab(window_id, new_tab_id)
                            && router.activate_tab(window_id, new_tab_id, app)
                    }
                },
            );
            log_ui_action_outcome("terminal.open-target", "sftp-new");
        }
        Err(error) => {
            log_ui_action_outcome("terminal.open-target", "sftp-rejected");
            set_status(ui, &format!("Cannot open SFTP location: {error}"));
        }
    }
}

pub(super) fn open_terminal_url(runtime: &Handle, ui: slint::Weak<AppWindow>, url: String) {
    runtime.spawn(async move {
        let opened = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || open::that_detached(url)),
        )
        .await;
        if !matches!(opened, Ok(Ok(Ok(())))) {
            tracing::warn!(
                target: "ax_ssh::diagnostics",
                operation = "open-terminal-url",
                "failed to open terminal URL"
            );
            set_status(&ui, "Cannot open URL");
        }
    });
}

pub(super) fn wire_targets(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    font_registry: Arc<Mutex<FontRegistry>>,
    terminal_font_started: Arc<std::sync::atomic::AtomicBool>,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let ui_for_selection = ui.as_weak();
    let state_for_selection = state.clone();
    let router_for_selection = window_router.clone();
    ui.on_terminal_selection_text(
        move |tab_id, anchor_row, anchor_column, focus_row, focus_column| {
            log_ui_action("terminal.selection-read");
            let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_selection) else {
                return SharedString::default();
            };
            state_for_selection
                .lock()
                .ok()
                .and_then(|app| {
                    if !router_for_selection.owns_terminal_pane(window_id, tab_id, &app) {
                        return None;
                    }
                    app.terminal(tab_id)
                        .and_then(|terminal| terminal.terminal.as_ref())
                        .map(|terminal| {
                            terminal.selection_text(
                                anchor_row.max(0) as usize,
                                anchor_column.max(0) as usize,
                                focus_row.max(0) as usize,
                                focus_column.max(0) as usize,
                            )
                        })
                })
                .unwrap_or_default()
                .into()
        },
    );

    let ui_for_semantic_selection = ui.as_weak();
    let state_for_semantic_selection = state.clone();
    let router_for_semantic_selection = window_router.clone();
    ui.on_terminal_semantic_selection_range(move |tab_id, row, column| {
        log_ui_action("terminal.semantic-selection-read");
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_semantic_selection)
        else {
            return TerminalSemanticSelection::default();
        };
        state_for_semantic_selection
            .lock()
            .ok()
            .and_then(|app| {
                if !router_for_semantic_selection.owns_terminal_pane(window_id, tab_id, &app) {
                    return None;
                }
                app.terminal(tab_id)
                    .and_then(|terminal| terminal.terminal.as_ref())
                    .and_then(|terminal| {
                        let row = row.max(0) as usize;
                        let column = column.max(0) as usize;
                        terminal_url_selection_range(terminal, row, column)
                            .or_else(|| terminal.semantic_selection_range(row, column))
                    })
            })
            .map(terminal_semantic_selection)
            .unwrap_or_default()
    });

    let ui_for_line_selection = ui.as_weak();
    let state_for_line_selection = state.clone();
    let router_for_line_selection = window_router.clone();
    ui.on_terminal_line_selection_range(move |tab_id, row, column| {
        log_ui_action("terminal.line-selection-read");
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_line_selection) else {
            return TerminalSemanticSelection::default();
        };
        state_for_line_selection
            .lock()
            .ok()
            .and_then(|app| {
                if !router_for_line_selection.owns_terminal_pane(window_id, tab_id, &app) {
                    return None;
                }
                app.terminal(tab_id)
                    .and_then(|terminal| terminal.terminal.as_ref())
                    .and_then(|terminal| {
                        terminal.line_selection_range(row.max(0) as usize, column.max(0) as usize)
                    })
            })
            .map(terminal_semantic_selection)
            .unwrap_or_default()
    });

    let ui_for_target_hover = ui.as_weak();
    let state_for_target_hover = state.clone();
    let router_for_target_hover = window_router.clone();
    ui.on_terminal_target_at_cell(move |tab_id, row, column, control, meta| {
        if !terminal_target_modifier_held(control, meta) {
            return TerminalTargetHighlight::default();
        }
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_target_hover) else {
            return TerminalTargetHighlight::default();
        };
        terminal_target_highlight_for_pane(
            &state_for_target_hover,
            &router_for_target_hover,
            window_id,
            tab_id,
            row,
            column,
        )
        .unwrap_or_default()
    });

    let ui_for_target_open = ui.as_weak();
    let state_for_target_open = state.clone();
    let runtime_for_target_open = runtime.clone();
    let font_registry_for_target_open = font_registry.clone();
    let terminal_font_started_for_target_open = terminal_font_started.clone();
    let router_for_target_open = window_router.clone();
    ui.on_activate_terminal_target(move |tab_id, row, column, control, meta| {
        if !terminal_target_modifier_held(control, meta) {
            return false;
        }
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_target_open) else {
            return false;
        };
        let Some(target) = terminal_target_for_pane(
            &state_for_target_open,
            &router_for_target_open,
            window_id,
            tab_id,
            row,
            column,
        ) else {
            return false;
        };

        log_ui_action("terminal.open-target");
        match target {
            TerminalTarget::Url(url) => {
                open_terminal_url(&runtime_for_target_open, ui_for_target_open.clone(), url);
                log_ui_action_outcome("terminal.open-target", "url");
            }
            TerminalTarget::RemotePath(path) => {
                open_terminal_remote_path(
                    &state_for_target_open,
                    &router_for_target_open,
                    window_id,
                    tab_id,
                    path,
                    &ui_for_target_open,
                    &runtime_for_target_open,
                    &font_registry_for_target_open,
                    &terminal_font_started_for_target_open,
                );
            }
        }
        true
    });
}
