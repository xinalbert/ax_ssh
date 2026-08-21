use super::*;

pub(in crate::app) fn set_tab_status(
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    tab_id: Uuid,
    message: &str,
) {
    let active = match state.lock() {
        Ok(mut app) => {
            let Some(terminal) = app.terminal_mut(tab_id) else {
                return;
            };
            terminal.status = message.to_owned();
            app.active_tab_id() == Some(tab_id)
        }
        Err(_) => {
            set_status(ui, "State lock poisoned");
            return;
        }
    };
    if active || global_window_router().is_some() {
        dispatch_active_snapshot(ui, state);
    }
}

pub(in crate::app) fn dispatch_active_snapshot(
    ui: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
) {
    if let Some(router) = global_window_router() {
        refresh_workspace_multi_window(ui, state, &router, WorkspaceRefreshRequest::Full);
        return;
    }
    let state = Arc::clone(state);
    dispatch_ui(ui, move |ui| {
        // Worker output and resize events can queue faster than the UI event loop runs.
        // Resolve the snapshot here so an older queued event cannot restore stale dimensions.
        let snapshot = match state.lock() {
            Ok(mut app) => {
                let active_tab_id = app.active_tab_id();
                app.snapshot_without_terminal_for(active_tab_id)
            }
            Err(_) => {
                ui.set_status("State lock poisoned".into());
                return;
            }
        };
        apply_active_snapshot(ui, snapshot, None);
    });
}

pub(in crate::app) fn dispatch_terminal_output_snapshot(
    ui: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    output_received_at: std::time::Instant,
) {
    dispatch_terminal_snapshot_at(ui, state, tab_id, Some(output_received_at));
}

pub(in crate::app) fn dispatch_terminal_snapshot(
    ui: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
) {
    dispatch_terminal_snapshot_at(ui, state, tab_id, None);
}

fn dispatch_terminal_snapshot_at(
    ui: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    output_received_at: Option<std::time::Instant>,
) {
    if let Some(router) = global_window_router() {
        if router.terminal_is_visible(tab_id) {
            refresh_workspace_multi_window(
                ui,
                state,
                &router,
                WorkspaceRefreshRequest::Terminal {
                    tab_id,
                    output_received_at,
                },
            );
        }
        return;
    }
    dispatch_active_snapshot(ui, state);
}

pub(super) fn duration_micros(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

pub(in crate::app) fn apply_active_snapshot(
    ui: &AppWindow,
    snapshot: ActiveTabSnapshot,
    workspace_tab_id: Option<Uuid>,
) {
    let active_kind = snapshot.kind;
    let active_pane_id = snapshot.id.map(|id| id.to_string()).unwrap_or_default();
    let active_tab_id = workspace_tab_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| active_pane_id.clone());
    ui.set_active_tab_id(active_tab_id.into());
    ui.set_active_pane_id(active_pane_id.into());
    ui.set_active_tab_kind(snapshot.kind.into());
    ui.set_active_tab_title(snapshot.title.into());
    ui.set_active_tab_status(snapshot.status.into());
    let notice = snapshot.notice;
    ui.set_active_tab_notice_visible(notice.visible);
    ui.set_active_tab_notice_severity(notice.severity.into());
    ui.set_active_tab_notice_title(notice.title.into());
    ui.set_active_tab_notice_message(notice.message.into());
    ui.set_active_tab_notice_primary_action(notice.primary_action.into());
    ui.set_active_tab_notice_primary_label(notice.primary_label.into());
    ui.set_active_tab_notice_secondary_action(notice.secondary_action.into());
    ui.set_active_tab_notice_secondary_label(notice.secondary_label.into());
    if let Some(editor) = snapshot.editor {
        ui.set_editor_credential_storage(editor.credential_storage.clone().into());
        ui.set_editor_default_credential_storage(editor.default_credential_storage.clone().into());
        let draft_id = editor.draft_id.to_string();
        if ui.get_editor_draft_id().as_str() != draft_id {
            ui.set_editor_profile_id(
                editor
                    .profile_id
                    .map(|profile_id| profile_id.to_string())
                    .unwrap_or_default()
                    .into(),
            );
            ui.set_editor_name(editor.name.into());
            ui.set_editor_group_name(editor.group_name.into());
            ui.set_editor_protocol(editor.protocol.into());
            ui.set_editor_host(editor.host.into());
            ui.set_editor_port(editor.port.into());
            ui.set_editor_username(editor.username.into());
            ui.set_editor_auth_method(editor.auth_method.into());
            ui.set_editor_private_key_path(editor.private_key_path.into());
            ui.set_editor_sftp_remote_path(editor.sftp_remote_path.into());
            ui.set_editor_sftp_local_path(editor.sftp_local_path.into());
            ui.set_editor_x11_forwarding(editor.x11_forwarding);
            ui.set_editor_serial_port(editor.serial_port.into());
            ui.set_editor_serial_baud_rate(editor.serial_baud_rate.into());
            ui.set_editor_serial_data_bits(editor.serial_data_bits.into());
            ui.set_editor_serial_stop_bits(editor.serial_stop_bits.into());
            ui.set_editor_serial_parity(editor.serial_parity.into());
            ui.set_editor_serial_flow_control(editor.serial_flow_control.into());
            // The Slint editor resets its local fields when this identity changes.
            // Publish it last so all source values form one coherent draft.
            ui.set_editor_draft_id(draft_id.into());
        }
    }
    if active_kind == "sftp" {
        apply_sftp_snapshot(ui, snapshot.sftp);
    }
    apply_security_prompt(ui, snapshot.security_prompt);
}

pub(super) fn apply_terminal_panes(
    ui: &AppWindow,
    panes: Vec<WindowTerminalPane>,
    dividers: Vec<PaneDividerPlacement>,
) {
    let settings = terminal_render_settings(ui);

    let current_panes = ui.get_terminal_panes();
    let panes = panes
        .into_iter()
        .enumerate()
        .map(|(index, pane)| {
            let current = current_panes.row_data(index).filter(|current| {
                current.terminal.terminal_id.as_str() == pane.placement.tab_id.to_string()
            });
            terminal_pane_view(ui, settings, pane, current.as_ref())
        })
        .collect::<Vec<_>>();
    let dividers = dividers
        .into_iter()
        .map(|divider| TerminalPaneDividerView {
            id: divider.id,
            x: divider.x,
            y: divider.y,
            width: divider.width,
            height: divider.height,
            ratio: divider.ratio,
            vertical: divider.vertical,
        })
        .collect::<Vec<_>>();
    if update_terminal_pane_snapshot_models(
        &ui.get_terminal_panes(),
        &ui.get_terminal_dividers(),
        &panes,
        &dividers,
    ) {
        return;
    }
    ui.set_terminal_panes(ModelRc::new(VecModel::from(panes)));
    ui.set_terminal_dividers(ModelRc::new(VecModel::from(dividers)));
}

pub(super) fn apply_terminal_pane_updates(ui: &AppWindow, panes: Vec<WindowTerminalPane>) -> usize {
    let current_panes = ui.get_terminal_panes();
    let settings = terminal_render_settings(ui);
    let mut applied = 0usize;
    for pane in panes {
        let terminal_id = pane.placement.tab_id.to_string();
        let Some(index) = (0..current_panes.row_count()).find(|index| {
            current_panes.row_data(*index).is_some_and(|current| {
                current.terminal.terminal_id.as_str() == terminal_id.as_str()
            })
        }) else {
            continue;
        };
        let Some(current) = current_panes.row_data(index) else {
            continue;
        };
        let mut updated = terminal_pane_view(ui, settings, pane, Some(&current));
        let models_reused = reuse_terminal_render_models(&current.terminal, &mut updated.terminal);
        if !models_reused || !terminal_pane_shallow_eq(&current, &updated) {
            current_panes.set_row_data(index, updated);
        }
        applied = applied.saturating_add(1);
    }
    applied
}

fn terminal_pane_view(
    ui: &AppWindow,
    settings: TerminalRenderSettings,
    pane: WindowTerminalPane,
    current: Option<&TerminalPaneView>,
) -> TerminalPaneView {
    TerminalPaneView {
        terminal: terminal_view_from_snapshot(
            pane.placement.tab_id,
            pane.snapshot,
            settings,
            current.map(|current| &current.terminal),
            ui,
        ),
        x: pane.placement.x,
        y: pane.placement.y,
        width: pane.placement.width,
        height: pane.placement.height,
        focused: pane.placement.focused,
        closable: pane.closable,
    }
}

fn terminal_render_settings(ui: &AppWindow) -> TerminalRenderSettings {
    TerminalRenderSettings {
        color_scheme: TerminalColorScheme::from_setting(ui.get_terminal_color_scheme().as_str()),
        default_foreground: to_rgb_color(ui.get_theme_terminal_foreground()),
        default_background: to_rgb_color(ui.get_theme_terminal_background()),
        selection_background: to_rgb_color(ui.get_theme_terminal_selection()),
        text_brightness: f64::from(ui.get_terminal_text_brightness().clamp(0.60, 1.20)),
        bright_bold_text: ui.get_bright_bold_text(),
        semantic_highlighting: ui.get_terminal_semantic_highlighting(),
        semantic_colors: SemanticColorOverrides {
            success: semantic_color_override(ui.get_terminal_semantic_success_color().as_str()),
            info: semantic_color_override(ui.get_terminal_semantic_info_color().as_str()),
            warning: semantic_color_override(ui.get_terminal_semantic_warning_color().as_str()),
            error: semantic_color_override(ui.get_terminal_semantic_error_color().as_str()),
        },
    }
}

fn semantic_color_override(value: &str) -> Option<RgbColor> {
    let value = value.trim().strip_prefix('#')?;
    let bytes = value.as_bytes();
    let [red_a, red_b, green_a, green_b, blue_a, blue_b] = bytes else {
        return None;
    };
    Some(RgbColor::new(
        semantic_hex_byte(*red_a, *red_b)?,
        semantic_hex_byte(*green_a, *green_b)?,
        semantic_hex_byte(*blue_a, *blue_b)?,
    ))
}

fn semantic_hex_byte(high: u8, low: u8) -> Option<u8> {
    Some(semantic_hex_digit(high)? * 16 + semantic_hex_digit(low)?)
}

fn semantic_hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

pub(super) fn update_terminal_pane_snapshot_models(
    current_panes: &ModelRc<TerminalPaneView>,
    current_dividers: &ModelRc<TerminalPaneDividerView>,
    panes: &[TerminalPaneView],
    dividers: &[TerminalPaneDividerView],
) -> bool {
    if current_panes.row_count() != panes.len() || current_dividers.row_count() != dividers.len() {
        return false;
    }
    let panes_match = panes.iter().enumerate().all(|(index, pane)| {
        current_panes
            .row_data(index)
            .is_some_and(|current| current.terminal.terminal_id == pane.terminal.terminal_id)
    });
    let dividers_match = dividers.iter().enumerate().all(|(index, divider)| {
        current_dividers
            .row_data(index)
            .is_some_and(|current| current.id == divider.id && current.vertical == divider.vertical)
    });
    if !panes_match || !dividers_match {
        return false;
    }
    for (index, mut pane) in panes.iter().cloned().enumerate() {
        if let Some(current) = current_panes.row_data(index) {
            let models_reused = reuse_terminal_render_models(&current.terminal, &mut pane.terminal);
            if models_reused && terminal_pane_shallow_eq(&current, &pane) {
                continue;
            }
        }
        current_panes.set_row_data(index, pane);
    }
    for (index, divider) in dividers.iter().cloned().enumerate() {
        if current_dividers.row_data(index).as_ref() != Some(&divider) {
            current_dividers.set_row_data(index, divider);
        }
    }
    true
}

pub(super) fn reuse_terminal_render_models(
    current: &TerminalViewState,
    updated: &mut TerminalViewState,
) -> bool {
    // TerminalGrid already observes these nested models. Reuse them so output
    // emits a direct row notification without replacing the focused pane.
    if current
        .render_lines
        .as_any()
        .downcast_ref::<VecModel<TerminalRenderLine>>()
        .is_none()
    {
        return false;
    }

    let updated_cursor = updated.cursor_state.iter().collect::<Vec<_>>();
    let cursor_reused = replace_vec_model_rows(&current.cursor_state, updated_cursor);
    if cursor_reused {
        updated.cursor_state = current.cursor_state.clone();
    }

    let mut updated_lines = updated.render_lines.iter().collect::<Vec<_>>();
    for (index, updated_line) in updated_lines.iter_mut().enumerate() {
        let Some(current_line) = current.render_lines.row_data(index) else {
            continue;
        };
        let updated_backgrounds = updated_line.backgrounds.iter().collect::<Vec<_>>();
        let backgrounds_reused =
            replace_vec_model_rows(&current_line.backgrounds, updated_backgrounds);
        if backgrounds_reused {
            updated_line.backgrounds = current_line.backgrounds.clone();
        }
        let updated_decorations = updated_line.decorations.iter().collect::<Vec<_>>();
        let decorations_reused =
            replace_vec_model_rows(&current_line.decorations, updated_decorations);
        if decorations_reused {
            updated_line.decorations = current_line.decorations.clone();
        }
        let updated_runs = updated_line.runs.iter().collect::<Vec<_>>();
        if replace_vec_model_rows(&current_line.runs, updated_runs) {
            updated_line.runs = current_line.runs;
        }
    }
    let lines_reused = replace_vec_model_rows(&current.render_lines, updated_lines);
    if lines_reused {
        updated.render_lines = current.render_lines.clone();
    }
    cursor_reused && lines_reused
}

fn terminal_pane_shallow_eq(current: &TerminalPaneView, updated: &TerminalPaneView) -> bool {
    let current_terminal = &current.terminal;
    let updated_terminal = &updated.terminal;
    current.x == updated.x
        && current.y == updated.y
        && current.width == updated.width
        && current.height == updated.height
        && current.focused == updated.focused
        && current.closable == updated.closable
        && current_terminal.terminal_id == updated_terminal.terminal_id
        && current_terminal.connected == updated_terminal.connected
        && current_terminal.selection_revision == updated_terminal.selection_revision
        && current_terminal.notice == updated_terminal.notice
        && current_terminal.content_columns == updated_terminal.content_columns
        && current_terminal.font_family == updated_terminal.font_family
        && current_terminal.font_size == updated_terminal.font_size
        && current_terminal.line_height_percent == updated_terminal.line_height_percent
        && current_terminal.foreground == updated_terminal.foreground
        && current_terminal.background == updated_terminal.background
        && current_terminal.selection_background == updated_terminal.selection_background
        && current_terminal.compact_rendering == updated_terminal.compact_rendering
        && current_terminal.row_render_cache == updated_terminal.row_render_cache
        && current_terminal.mouse_button_reporting == updated_terminal.mouse_button_reporting
        && current_terminal.mouse_wheel_reporting == updated_terminal.mouse_wheel_reporting
        && current_terminal.right_click_copy_or_paste == updated_terminal.right_click_copy_or_paste
        && current_terminal.copy_selection_on_select == updated_terminal.copy_selection_on_select
        && current_terminal.option_as_meta == updated_terminal.option_as_meta
        && current_terminal.copy_selection_shortcut == updated_terminal.copy_selection_shortcut
        && current_terminal.paste_shortcut == updated_terminal.paste_shortcut
        && current_terminal.select_all_shortcut == updated_terminal.select_all_shortcut
        && current_terminal.mouse_local_selection_priority
            == updated_terminal.mouse_local_selection_priority
}

#[cfg(test)]
pub(in crate::app) fn update_terminal_render_lines(
    current: &ModelRc<TerminalRenderLine>,
    lines: &[TerminalRenderLine],
) -> bool {
    let Some(current_lines) = current
        .as_any()
        .downcast_ref::<VecModel<TerminalRenderLine>>()
    else {
        return false;
    };
    if current.row_count() != lines.len() {
        current_lines.set_vec(lines.to_vec());
        return true;
    }
    for (index, line) in lines.iter().cloned().enumerate() {
        let Some(current_line) = current.row_data(index) else {
            return false;
        };
        if update_terminal_render_backgrounds(&current_line.backgrounds, &line.backgrounds)
            && update_terminal_render_decorations(&current_line.decorations, &line.decorations)
            && update_terminal_render_runs(&current_line.runs, &line.runs)
        {
            continue;
        }
        current_lines.set_row_data(index, line);
    }
    true
}

#[cfg(test)]
fn update_terminal_render_runs(
    current: &ModelRc<TerminalRenderRun>,
    updated: &ModelRc<TerminalRenderRun>,
) -> bool {
    let rows = updated.iter().collect::<Vec<_>>();
    replace_vec_model_rows(current, rows)
}

#[cfg(test)]
fn update_terminal_render_backgrounds(
    current: &ModelRc<TerminalBackgroundRun>,
    updated: &ModelRc<TerminalBackgroundRun>,
) -> bool {
    let rows = updated.iter().collect::<Vec<_>>();
    replace_vec_model_rows(current, rows)
}

#[cfg(test)]
fn update_terminal_render_decorations(
    current: &ModelRc<TerminalDecorationRun>,
    updated: &ModelRc<TerminalDecorationRun>,
) -> bool {
    let rows = updated.iter().collect::<Vec<_>>();
    replace_vec_model_rows(current, rows)
}

pub(super) fn replace_vec_model_rows<T: Clone + PartialEq + 'static>(
    model: &ModelRc<T>,
    rows: Vec<T>,
) -> bool {
    let Some(vec_model) = model.as_any().downcast_ref::<VecModel<T>>() else {
        return false;
    };
    if model.row_count() == rows.len() {
        for (index, row) in rows.into_iter().enumerate() {
            if model.row_data(index).as_ref() != Some(&row) {
                vec_model.set_row_data(index, row);
            }
        }
    } else {
        vec_model.set_vec(rows);
    }
    true
}

pub(in crate::app) fn apply_terminal_pane_layout(ui: &AppWindow, layout: PaneLayout) -> bool {
    update_terminal_pane_layout_models(
        &ui.get_terminal_panes(),
        &ui.get_terminal_dividers(),
        layout,
    )
}

pub(super) fn update_terminal_pane_layout_models(
    panes: &ModelRc<TerminalPaneView>,
    dividers: &ModelRc<TerminalPaneDividerView>,
    layout: PaneLayout,
) -> bool {
    if panes.row_count() != layout.panes.len() || dividers.row_count() != layout.dividers.len() {
        return false;
    }

    let pane_updates = layout
        .panes
        .into_iter()
        .enumerate()
        .map(|(index, placement)| {
            let mut pane = panes.row_data(index)?;
            if pane.terminal.terminal_id.as_str() != placement.tab_id.to_string() {
                return None;
            }
            let changed = pane.x != placement.x
                || pane.y != placement.y
                || pane.width != placement.width
                || pane.height != placement.height
                || pane.focused != placement.focused;
            pane.x = placement.x;
            pane.y = placement.y;
            pane.width = placement.width;
            pane.height = placement.height;
            pane.focused = placement.focused;
            Some(changed.then_some((index, pane)))
        })
        .collect::<Option<Vec<_>>>();
    let divider_updates = layout
        .dividers
        .into_iter()
        .enumerate()
        .map(|(index, placement)| {
            let divider = dividers.row_data(index)?;
            if divider.id != placement.id || divider.vertical != placement.vertical {
                return None;
            }
            let updated = TerminalPaneDividerView {
                id: placement.id,
                x: placement.x,
                y: placement.y,
                width: placement.width,
                height: placement.height,
                ratio: placement.ratio,
                vertical: placement.vertical,
            };
            Some((divider != updated).then_some((index, updated)))
        })
        .collect::<Option<Vec<_>>>();
    let (Some(pane_updates), Some(divider_updates)) = (pane_updates, divider_updates) else {
        return false;
    };

    for (index, pane) in pane_updates.into_iter().flatten() {
        panes.set_row_data(index, pane);
    }
    for (index, divider) in divider_updates.into_iter().flatten() {
        dividers.set_row_data(index, divider);
    }
    true
}

pub(super) fn terminal_view_from_snapshot(
    tab_id: Uuid,
    snapshot: ActiveTabSnapshot,
    settings: TerminalRenderSettings,
    current: Option<&TerminalViewState>,
    ui: &AppWindow,
) -> TerminalViewState {
    let connected = snapshot.connected;
    let selection_revision = snapshot.selection_revision;
    let notice = snapshot.notice;
    let snapshot = snapshot.terminal.unwrap_or_else(empty_terminal_snapshot);
    let renderer = TerminalRenderer::new(settings);
    let current_lines = current.map(|current| &current.render_lines);
    let lines = render_snapshot_lines(&snapshot, &renderer, current_lines);
    let cursor_text: SharedString = snapshot.cursor_text.into();
    let cursor_row = snapshot.cursor_row.min(i32::MAX as usize) as i32;
    let cursor_column = snapshot.cursor_column.min(i32::MAX as usize) as i32;
    let cursor_state = TerminalCursorState {
        row: cursor_row,
        column: cursor_column,
        visible: snapshot.cursor_visible,
        text: cursor_text.clone(),
    };
    TerminalViewState {
        terminal_id: tab_id.to_string().into(),
        connected,
        selection_revision,
        notice: terminal_notice_view(notice),
        render_lines: ModelRc::new(VecModel::from(lines)),
        cursor_state: ModelRc::new(VecModel::from(vec![cursor_state])),
        content_columns: snapshot.max_columns.min(i32::MAX as usize) as i32,
        cursor_row,
        cursor_column,
        cursor_visible: snapshot.cursor_visible,
        cursor_text,
        font_family: ui.get_terminal_font_family(),
        font_size: ui.get_terminal_font_size() as f32,
        line_height_percent: ui.get_terminal_line_height_percent(),
        foreground: to_slint_color(renderer.foreground()),
        background: to_slint_color(renderer.background()),
        selection_background: to_slint_color(renderer.selection_background()),
        compact_rendering: ui.get_terminal_compact_rendering(),
        row_render_cache: ui.get_terminal_row_render_cache(),
        mouse_button_reporting: snapshot.mouse_button_reporting_active,
        mouse_wheel_reporting: snapshot.mouse_wheel_reporting_active,
        right_click_copy_or_paste: ui.get_right_click_copy_or_paste(),
        copy_selection_on_select: ui.get_copy_selection_on_select(),
        option_as_meta: ui.get_option_as_meta(),
        copy_selection_shortcut: ui.get_copy_selection_shortcut(),
        paste_shortcut: ui.get_paste_shortcut(),
        select_all_shortcut: ui.get_select_all_shortcut(),
        mouse_local_selection_priority: ui.get_terminal_mouse_local_selection_priority(),
    }
}

pub(super) fn render_snapshot_lines(
    snapshot: &TerminalSnapshot,
    renderer: &TerminalRenderer,
    current: Option<&ModelRc<TerminalRenderLine>>,
) -> Vec<TerminalRenderLine> {
    let render_cache_key = renderer.cache_key();
    let render_cache_key_low = render_cache_key as u32 as i32;
    let render_cache_key_high = (render_cache_key >> 32) as u32 as i32;
    snapshot
        .lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            current
                .and_then(|current| current.row_data(index))
                .filter(|current| {
                    current.source_revision_low == line.revision as u32 as i32
                        && current.source_revision_high == (line.revision >> 32) as u32 as i32
                        && current.render_cache_key_low == render_cache_key_low
                        && current.render_cache_key_high == render_cache_key_high
                })
                .unwrap_or_else(|| terminal_render_line(renderer.render_line(line)))
        })
        .collect()
}

fn terminal_notice_view(notice: TerminalNoticeSnapshot) -> TerminalNoticeViewState {
    TerminalNoticeViewState {
        visible: notice.visible,
        severity: notice.severity.into(),
        title: notice.title.into(),
        message: notice.message.into(),
        primary_action: notice.primary_action.into(),
        primary_label: notice.primary_label.into(),
        secondary_action: notice.secondary_action.into(),
        secondary_label: notice.secondary_label.into(),
    }
}

pub(super) fn apply_sftp_snapshot(ui: &AppWindow, snapshot: SftpBrowserSnapshot) {
    let transfer_active_count = snapshot
        .transfers
        .iter()
        .filter(|transfer| transfer.phase.active())
        .count() as i32;
    let transfer_failed_count = snapshot
        .transfers
        .iter()
        .filter(|transfer| {
            matches!(
                transfer.phase,
                SftpTransferPhase::Failed | SftpTransferPhase::Cancelled
            )
        })
        .count() as i32;
    let transfer_completed_count = snapshot
        .transfers
        .iter()
        .filter(|transfer| transfer.phase == SftpTransferPhase::Completed)
        .count() as i32;
    ui.set_sftp_available(snapshot.available);
    ui.set_sftp_open(snapshot.open);
    ui.set_sftp_loading(snapshot.loading);
    ui.set_sftp_home(snapshot.home.into());
    ui.set_sftp_path(snapshot.path.into());
    ui.set_sftp_entries(ModelRc::new(VecModel::from(sftp_entry_rows(
        snapshot.entries,
        &snapshot.selected,
    ))));
    ui.set_sftp_has_more(snapshot.has_more);
    ui.set_sftp_truncated(snapshot.truncated);
    ui.set_sftp_status(snapshot.status.into());
    ui.set_sftp_can_go_back(snapshot.can_go_back);
    ui.set_sftp_can_go_forward(snapshot.can_go_forward);
    ui.set_sftp_selected_count(snapshot.selected_count as i32);
    ui.set_sftp_all_selected(snapshot.all_selected);
    ui.set_local_sftp_loading(snapshot.local.loading);
    ui.set_local_sftp_path(snapshot.local.path.into());
    ui.set_local_sftp_entries(ModelRc::new(VecModel::from(local_entry_rows(
        snapshot.local.entries,
        &snapshot.local.selected,
    ))));
    ui.set_local_sftp_truncated(snapshot.local.truncated);
    ui.set_local_sftp_status(snapshot.local.status.into());
    ui.set_local_sftp_selected_count(snapshot.local.selected_count as i32);
    ui.set_local_sftp_all_selected(snapshot.local.all_selected);
    let mut active = Vec::new();
    let mut failed = Vec::new();
    let mut completed = Vec::new();
    for transfer in snapshot.transfers {
        match transfer.phase {
            SftpTransferPhase::Queued
            | SftpTransferPhase::Downloading
            | SftpTransferPhase::Pausing
            | SftpTransferPhase::Paused
            | SftpTransferPhase::Resuming
            | SftpTransferPhase::Cancelling
            | SftpTransferPhase::Opening => active.push(transfer),
            SftpTransferPhase::Failed | SftpTransferPhase::Cancelled => failed.push(transfer),
            SftpTransferPhase::Completed => completed.push(transfer),
        }
    }
    ui.set_sftp_active_transfers(ModelRc::new(VecModel::from(sftp_transfer_rows(active))));
    ui.set_sftp_failed_transfers(ModelRc::new(VecModel::from(sftp_transfer_rows(failed))));
    ui.set_sftp_completed_transfers(ModelRc::new(VecModel::from(sftp_transfer_rows(completed))));
    ui.set_sftp_transfer_active_count(transfer_active_count);
    ui.set_sftp_transfer_failed_count(transfer_failed_count);
    ui.set_sftp_transfer_completed_count(transfer_completed_count);
    ui.set_sftp_transfer_selected_active_count(snapshot.transfer_selected_active_count as i32);
    ui.set_sftp_transfer_selected_pausable_count(snapshot.transfer_selected_pausable_count as i32);
    ui.set_sftp_transfer_selected_resumable_count(
        snapshot.transfer_selected_resumable_count as i32,
    );
    ui.set_sftp_editor_path(snapshot.editor_path.unwrap_or_default().into());
    ui.set_sftp_editor_text(snapshot.editor_text.into());
    ui.set_sftp_rename_name(snapshot.rename_name.into());
    ui.set_sftp_editor_remote_changed(snapshot.editor_remote_changed);
    ui.set_sftp_editor_auto_upload(snapshot.editor_auto_upload);
    ui.set_sftp_editor_revision(snapshot.editor_revision as i32);
}

pub(super) fn apply_security_prompt(ui: &AppWindow, prompt: ActiveSecurityPrompt) {
    match prompt {
        ActiveSecurityPrompt::None => {
            ui.set_host_key_dialog_open(false);
            ui.set_password_dialog_open(false);
            ui.set_password_dialog_tab_id("".into());
        }
        ActiveSecurityPrompt::HostKey(prompt) => {
            ui.set_host_key_endpoint(format!("{}:{}", prompt.host, prompt.port).into());
            ui.set_host_key_fingerprint(prompt.fingerprint.into());
            ui.set_host_key_changed(prompt.changed);
            ui.set_host_key_revoked(prompt.revoked);
            ui.set_password_dialog_open(false);
            ui.set_password_dialog_tab_id("".into());
            ui.set_host_key_dialog_open(true);
        }
        ActiveSecurityPrompt::Authentication {
            tab_id,
            profile,
            vault_unlock_only,
        } => {
            let Some(ssh) = profile.ssh() else {
                ui.set_host_key_dialog_open(false);
                ui.set_password_dialog_open(false);
                ui.set_password_dialog_tab_id("".into());
                return;
            };
            let (private_key, key_path) = match &ssh.auth {
                AuthMethod::Password => (false, String::new()),
                AuthMethod::PrivateKey { path } => (true, path.display().to_string()),
                AuthMethod::SshAgent => {
                    ui.set_host_key_dialog_open(false);
                    ui.set_password_dialog_open(false);
                    ui.set_password_dialog_tab_id("".into());
                    return;
                }
            };
            let vault_storage = vault_unlock_only
                && !private_key
                && ssh.credential_storage == Some(CredentialStorage::EncryptedVault);
            ui.set_host_key_dialog_open(false);
            ui.set_host_key_revoked(false);
            ui.set_password_endpoint(profile_endpoint(&profile).into());
            ui.set_password_private_key(private_key);
            ui.set_password_vault_storage(vault_storage);
            ui.set_password_vault_unlock_only(vault_unlock_only);
            ui.set_password_key_path(key_path.into());
            ui.set_password_dialog_tab_id(tab_id.to_string().into());
            ui.set_password_dialog_open(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_color_override_accepts_canonical_hex_only() {
        assert_eq!(
            semantic_color_override(" #17A8CD "),
            Some(RgbColor::new(23, 168, 205))
        );
        for value in ["", "17A8CD", "#FFF", "#17A8CD00", "#17A8CZ"] {
            assert_eq!(semantic_color_override(value), None, "{value:?}");
        }
    }
}
