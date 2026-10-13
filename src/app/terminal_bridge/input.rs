//! Terminal input application boundary.

use super::*;

const MAX_MOUSE_WHEEL_REPORTS: i32 = 256;

/// Encode a bounded source event, then apply the optional alternate-scroll
/// policy to each wheel step. The model remains authoritative about whether
/// the current terminal modes accept this pointer event.
pub(super) fn encode_terminal_mouse_input(
    model: &TerminalModel,
    event: TerminalMouseEvent,
    pixel_x: usize,
    pixel_y: usize,
    repeat_count: i32,
    alternate_scroll_to_home: bool,
) -> Option<Vec<u8>> {
    let repeat_count = repeat_count.clamp(1, MAX_MOUSE_WHEEL_REPORTS) as usize;
    let wheel = matches!(
        event.button,
        TerminalMouseButton::WheelUp
            | TerminalMouseButton::WheelDown
            | TerminalMouseButton::WheelLeft
            | TerminalMouseButton::WheelRight
    );
    if repeat_count > 1 && (event.kind != TerminalMouseEventKind::Press || !wheel) {
        return None;
    }
    let data = model.encode_mouse_event_with_pixels(event, pixel_x, pixel_y)?;
    // A wheel event accepted without reporting can only be alternate-scroll.
    // End every Up/Down step with an unmodified Home to act on the destination
    // line. The receiving application owns the resulting cursor position.
    if alternate_scroll_to_home
        && !model.mouse_reporting().enabled()
        && event.kind == TerminalMouseEventKind::Press
        && matches!(
            event.button,
            TerminalMouseButton::WheelUp | TerminalMouseButton::WheelDown
        )
    {
        let home = encode_key(
            &TerminalKey::Home,
            TerminalModifiers::default(),
            model.application_cursor(),
        )?;
        let mut repeated = Vec::with_capacity((home.len() + data.len()) * repeat_count);
        for _ in 0..repeat_count {
            repeated.extend_from_slice(&data);
            repeated.extend_from_slice(&home);
        }
        return Some(repeated);
    }
    Some(data.repeat(repeat_count))
}

pub(super) fn sync_terminal_query_palette(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let palette = crate::app::view::terminal::terminal_query_palette(ui);
    if let Ok(mut app) = state.lock() {
        app.set_terminal_query_palette(palette);
    }
}

pub(super) struct TerminalInputContext<'a> {
    pub(super) ui: &'a slint::Weak<AppWindow>,
    pub(super) state: &'a Arc<Mutex<AppState>>,
    pub(super) window_router: &'a WindowRouter,
    pub(super) window_id: Uuid,
}

impl TerminalInputContext<'_> {
    pub(super) fn dispatch(
        &self,
        tab_id: Uuid,
        input: crate::app::input::NormalizedKeyboardInput,
    ) -> bool {
        let input_started_at = std::time::Instant::now();
        let mut state_lock_elapsed = None;
        let mut worker_request_elapsed = None;
        log_terminal_input(&input);
        let state_lock_started_at = std::time::Instant::now();
        let result = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))
            .and_then(|mut app| {
                state_lock_elapsed = Some(state_lock_started_at.elapsed());
                if self
                    .window_router
                    .workspace_actions_locked(self.window_id, &app)
                {
                    return Ok((false, false));
                }
                if !self
                    .window_router
                    .owns_terminal_pane(self.window_id, tab_id, &app)
                {
                    return Ok((true, false));
                }
                let modifiers = if cfg!(target_os = "macos")
                    && !app.sessions.settings.terminal.option_as_meta
                {
                    TerminalModifiers {
                        alt: false,
                        ..input.modifiers
                    }
                } else {
                    input.modifiers
                };
                let terminal = app.terminal(tab_id).context("terminal tab not found")?;
                if !terminal.connected {
                    return Ok((false, false));
                }
                let model = terminal
                    .terminal
                    .as_ref()
                    .context("active tab has no terminal model")?;
                let data = if input.is_paste {
                    model
                        .encode_paste(&input.text)
                        .context("terminal paste exceeds the bounded input limit")?
                } else {
                    let application_cursor = model.application_cursor();
                    let Some(key) = crate::app::input::terminal_key_from_normalized_input(&input)
                    else {
                        return Ok((false, false));
                    };
                    let Some(data) = encode_key(&key, modifiers, application_cursor) else {
                        return Ok((false, false));
                    };
                    data
                };
                {
                    let terminal = app.terminal(tab_id).context("terminal tab not found")?;
                    let worker_request_started_at = std::time::Instant::now();
                    let worker = terminal
                        .worker
                        .as_ref()
                        .context("active terminal has no worker")?;
                    let request_result = if input.is_paste {
                        worker.request_send_paste(data)
                    } else {
                        worker.request_send_kind(
                            data,
                            crate::app::state::terminal::TerminalInputKind::Key,
                        )
                    };
                    worker_request_elapsed = Some(worker_request_started_at.elapsed());
                    request_result?;
                }
                let viewport_changed = app.scroll_terminal_to_bottom(tab_id);
                Ok((true, viewport_changed))
            });
        match result {
            Ok((handled, true)) => {
                log_terminal_input_latency(
                    "handled-and-scrolled",
                    input_started_at.elapsed(),
                    state_lock_elapsed,
                    worker_request_elapsed,
                );
                log_ui_action_outcome("terminal.send-input", "handled-and-scrolled");
                dispatch_terminal_snapshot(self.ui, self.state, tab_id);
                handled
            }
            Ok((handled, false)) => {
                log_terminal_input_latency(
                    if handled { "handled" } else { "ignored" },
                    input_started_at.elapsed(),
                    state_lock_elapsed,
                    worker_request_elapsed,
                );
                log_ui_action_outcome(
                    "terminal.send-input",
                    if handled { "handled" } else { "ignored" },
                );
                handled
            }
            Err(error) => {
                log_terminal_input_latency(
                    "error",
                    input_started_at.elapsed(),
                    state_lock_elapsed,
                    worker_request_elapsed,
                );
                log_ui_action_outcome("terminal.send-input", "error");
                debug!(%error, "terminal input failed");
                set_status(self.ui, &format!("Cannot send terminal input: {error}"));
                true
            }
        }
    }
}

pub(super) fn report_terminal_focus_state(
    state: &Arc<Mutex<AppState>>,
    window_router: &WindowRouter,
    window_id: Uuid,
    tab_id: Uuid,
    focused: bool,
) -> Result<bool> {
    let app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    if focused && !window_router.owns_terminal_pane(window_id, tab_id, &app) {
        return Ok(false);
    }
    let Some(terminal) = app.terminal(tab_id) else {
        return Ok(false);
    };
    if !terminal.connected {
        return Ok(false);
    }
    let model = terminal
        .terminal
        .as_ref()
        .context("active terminal has no terminal model")?;
    let Some(data) = model.encode_focus_event(focused) else {
        return Ok(true);
    };
    let worker = terminal
        .worker
        .as_ref()
        .context("active terminal has no worker")?;
    worker.request_send_kind(data, crate::app::state::terminal::TerminalInputKind::Focus)?;
    Ok(true)
}

pub(super) fn wire_input(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let ui_for_theme = ui.as_weak();
    let state_for_theme = state.clone();
    ui.on_refresh_terminal_appearance(move || {
        log_ui_action("terminal.refresh-appearance");
        // A theme change only changes the visual snapshot; it must not resize or
        // otherwise disturb the PTY worker that owns the active terminal.
        if let Some(ui) = ui_for_theme.upgrade() {
            sync_terminal_query_palette(&ui, &state_for_theme);
        }
        dispatch_active_snapshot(&ui_for_theme, &state_for_theme);
    });

    let ui_for_key = ui.as_weak();
    let state_for_key = state.clone();
    let router_for_key = window_router.clone();
    ui.on_terminal_key(move |request| {
        let Some(tab_id) = parse_uuid(request.terminal_id.as_str(), "terminal", &ui_for_key) else {
            return true;
        };
        let input_event = super::normalized_keyboard_input_from_slint_event(&request.event);
        TerminalInputContext {
            ui: &ui_for_key,
            state: &state_for_key,
            window_router: &router_for_key,
            window_id,
        }
        .dispatch(tab_id, input_event)
    });

    let ui_for_resize = ui.as_weak();
    let state_for_resize = state.clone();
    let router_for_resize = window_router.clone();
    ui.on_resize_terminal(move |tab_id, columns, rows, cell_width, cell_height| {
        log_ui_action("terminal.resize");
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_resize) else {
            return;
        };
        let result = state_for_resize
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))
            .and_then(|mut app| {
                if !router_for_resize.owns_terminal_pane(window_id, tab_id, &app) {
                    anyhow::bail!("terminal pane is no longer visible in this window");
                }
                let columns = columns.max(1) as u32;
                let rows = rows.max(1) as u32;
                let scale_factor = ui_for_resize
                    .upgrade()
                    .map(|ui| f64::from(ui.window().scale_factor()).max(0.01))
                    .unwrap_or(1.0);
                let to_physical = |value: f32| {
                    (f64::from(value).max(0.0) * scale_factor)
                        .round()
                        .clamp(1.0, f64::from(u32::MAX)) as u32
                };
                app.resize_terminal_with_metrics(
                    tab_id,
                    columns,
                    rows,
                    to_physical(cell_width),
                    to_physical(cell_height),
                )
            });
        match result {
            Ok(true) => {
                log_ui_action_outcome("terminal.resize", "accepted");
                dispatch_terminal_snapshot(&ui_for_resize, &state_for_resize, tab_id);
            }
            Ok(false) => {
                log_ui_action_outcome("terminal.resize", "unchanged");
            }
            Err(error) => {
                log_ui_action_outcome("terminal.resize", "ignored");
                debug!(%error, "terminal resize ignored");
                set_status(&ui_for_resize, &format!("Cannot resize terminal: {error}"));
            }
        }
    });

    let ui_for_scroll = ui.as_weak();
    let state_for_scroll = state.clone();
    let router_for_scroll = window_router.clone();
    ui.on_scroll_terminal(move |tab_id, lines| {
        log_ui_action("terminal.scroll");
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_scroll) else {
            return;
        };
        let mut before_offset = None;
        let mut after_offset = None;
        let mut owned = false;
        let changed = state_for_scroll
            .lock()
            .ok()
            .and_then(|mut app| {
                owned = router_for_scroll.owns_terminal_pane(window_id, tab_id, &app);
                if !owned {
                    return None;
                }
                before_offset = app
                    .terminal(tab_id)
                    .and_then(|terminal| terminal.terminal.as_ref())
                    .map(TerminalModel::display_offset);
                let changed = app.scroll_terminal(tab_id, lines);
                after_offset = app
                    .terminal(tab_id)
                    .and_then(|terminal| terminal.terminal.as_ref())
                    .map(TerminalModel::display_offset);
                Some(changed)
            })
            .unwrap_or(false);
        tracing::debug!(
            target: "ax_ssh::terminal_scroll",
            tab_id = %tab_id,
            lines,
            owned,
            changed,
            before_offset = ?before_offset,
            after_offset = ?after_offset,
            "terminal scroll callback processed"
        );
        if changed {
            log_ui_action_outcome("terminal.scroll", "changed");
            dispatch_terminal_snapshot(&ui_for_scroll, &state_for_scroll, tab_id);
        } else {
            log_ui_action_outcome("terminal.scroll", "unchanged");
        }
    });

    let ui_for_mouse = ui.as_weak();
    let state_for_mouse = state.clone();
    let router_for_mouse = window_router.clone();
    ui.on_terminal_pointer_input(move |tab_id, input| {
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_mouse) else {
            return;
        };
        let button = match input.button {
            0 => TerminalMouseButton::Left,
            1 => TerminalMouseButton::Middle,
            2 => TerminalMouseButton::Right,
            3 => TerminalMouseButton::WheelUp,
            4 => TerminalMouseButton::WheelDown,
            5 => TerminalMouseButton::WheelLeft,
            6 => TerminalMouseButton::WheelRight,
            7 => TerminalMouseButton::Auxiliary8,
            8 => TerminalMouseButton::Auxiliary9,
            9 => TerminalMouseButton::None,
            _ => return,
        };
        let kind = match input.kind {
            0 => TerminalMouseEventKind::Press,
            1 => TerminalMouseEventKind::Release,
            2 => TerminalMouseEventKind::Motion,
            _ => return,
        };
        let scale_factor = ui_for_mouse
            .upgrade()
            .map(|ui| f64::from(ui.window().scale_factor()).max(0.01))
            .unwrap_or(1.0);
        let to_physical = |value: f32| {
            (f64::from(value).max(0.0) * scale_factor)
                .floor()
                .clamp(0.0, (usize::MAX - 1) as f64) as usize
                + 1
        };
        let pixel_x = to_physical(input.x);
        let pixel_y = to_physical(input.y);
        let result = state_for_mouse
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))
            .and_then(|mut app| {
                if !router_for_mouse.owns_terminal_pane(window_id, tab_id, &app) {
                    anyhow::bail!("terminal pane is no longer visible in this window");
                }
                let alternate_scroll_to_home =
                    app.sessions.settings.terminal.alternate_scroll_to_home;
                let terminal = app.terminal_mut(tab_id).context("terminal tab not found")?;
                if !terminal.connected {
                    return Ok(true);
                }
                let model = terminal
                    .terminal
                    .as_ref()
                    .context("active tab has no terminal model")?;
                let Some(data) = encode_terminal_mouse_input(
                    model,
                    TerminalMouseEvent {
                        kind,
                        button,
                        column: input.column.max(0) as usize,
                        row: input.row.max(0) as usize,
                        modifiers: TerminalMouseModifiers {
                            shift: input.shift,
                            alt: input.alt,
                            control: input.control,
                        },
                    },
                    pixel_x,
                    pixel_y,
                    input.repeat_count,
                    alternate_scroll_to_home,
                ) else {
                    return Ok(true);
                };
                let worker = terminal
                    .worker
                    .as_ref()
                    .context("active terminal has no worker")?;
                if kind == TerminalMouseEventKind::Motion {
                    worker.request_send_motion_kind(
                        data,
                        crate::app::state::terminal::TerminalInputKind::Pointer,
                    )
                } else {
                    worker
                        .request_send_kind(
                            data,
                            crate::app::state::terminal::TerminalInputKind::Pointer,
                        )
                        .map(|()| true)
                }
            });
        match result {
            Ok(true) => {}
            Ok(false) => {
                debug!(%tab_id, "terminal mouse motion dropped under worker backpressure");
            }
            Err(error) => {
                debug!(%error, "terminal mouse event failed");
                set_status(
                    &ui_for_mouse,
                    &format!("Cannot send terminal mouse event: {error}"),
                );
            }
        }
    });

    let focused_terminal = Rc::new(RefCell::new(None));
    let focused_terminal_for_callback = focused_terminal.clone();
    let ui_for_focus_report = ui.as_weak();
    let state_for_focus_report = state.clone();
    let router_for_focus_report = window_router.clone();
    ui.on_terminal_focus_state(move |tab_id, focused| {
        let requested_tab_id = if tab_id.is_empty() {
            None
        } else {
            let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_focus_report) else {
                return;
            };
            Some(tab_id)
        };
        if focused {
            let Some(tab_id) = requested_tab_id else {
                return;
            };
            let previous_tab_id = *focused_terminal_for_callback.borrow();
            if previous_tab_id == Some(tab_id) {
                return;
            }
            if let Some(previous_tab_id) = focused_terminal_for_callback.replace(None)
                && let Err(error) = report_terminal_focus_state(
                    &state_for_focus_report,
                    &router_for_focus_report,
                    window_id,
                    previous_tab_id,
                    false,
                )
            {
                debug!(%error, %previous_tab_id, "terminal focus-out report failed");
            }
            match report_terminal_focus_state(
                &state_for_focus_report,
                &router_for_focus_report,
                window_id,
                tab_id,
                true,
            ) {
                Ok(true) => focused_terminal_for_callback.replace(Some(tab_id)),
                Ok(false) => None,
                Err(error) => {
                    debug!(%error, %tab_id, "terminal focus-in report failed");
                    None
                }
            };
        } else {
            let previous_tab_id = *focused_terminal_for_callback.borrow();
            if let Some(previous_tab_id) = previous_tab_id
                && (requested_tab_id.is_none() || requested_tab_id == Some(previous_tab_id))
            {
                focused_terminal_for_callback.replace(None);
                if let Err(error) = report_terminal_focus_state(
                    &state_for_focus_report,
                    &router_for_focus_report,
                    window_id,
                    previous_tab_id,
                    false,
                ) {
                    debug!(%error, %previous_tab_id, "terminal focus-out report failed");
                }
            }
        }
    });
}
