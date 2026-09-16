use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    time::Duration,
};

use super::input::{
    clear_native_event_modifiers, normalized_keyboard_input_from_winit,
    update_native_event_modifiers,
};
#[cfg(target_os = "macos")]
use super::input::{native_shortcut_key_name, native_shortcut_matches_setting};
use super::*;
use crate::app::state::PaneSessionSource;
use crate::app::terminal_targets::{TerminalTarget, terminal_target_match_at_context};
use ax_ssh::terminal::{
    TerminalModel, TerminalModifiers, TerminalMouseButton, TerminalMouseEvent,
    TerminalMouseEventKind, TerminalMouseModifiers, TerminalTargetContext, encode_key_with_modes,
};
use slint::winit_030::winit::event::ElementState;
use slint::winit_030::{
    EventResult, WinitWindowAccessor,
    winit::{event::WindowEvent, keyboard::ModifiersState},
};

#[derive(Clone, Copy, Debug)]
struct TerminalGeometrySample {
    pane_x: f32,
    pane_y: f32,
    pane_width: f32,
    pane_height: f32,
    grid_x: f32,
    grid_y: f32,
    grid_width: f32,
    grid_height: f32,
    cell_width: f32,
    cell_height: f32,
    columns: i32,
    rows: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerminalGeometrySignature {
    window_physical_width: u32,
    window_physical_height: u32,
    winit_physical_width: u32,
    winit_physical_height: u32,
    scale_milli: u32,
    pane_x: i32,
    pane_y: i32,
    pane_width: i32,
    pane_height: i32,
    grid_x: i32,
    grid_y: i32,
    grid_width: i32,
    grid_height: i32,
    cell_width: i32,
    cell_height: i32,
    columns: i32,
    rows: i32,
    detached_window: bool,
    software_presentation_enabled: bool,
}

#[derive(Default)]
struct TerminalGeometryDiagnostics {
    last: HashMap<String, TerminalGeometrySignature>,
}

impl TerminalGeometryDiagnostics {
    fn record(
        &mut self,
        ui: &AppWindow,
        window_id: Uuid,
        terminal_id: &str,
        sample: TerminalGeometrySample,
    ) {
        let window_size = ui.window().size();
        let scale_factor = f64::from(ui.window().scale_factor()).max(0.01);
        let (winit_physical_width, winit_physical_height) = ui
            .window()
            .with_winit_window(|window| {
                let size = window.inner_size();
                (size.width, size.height)
            })
            .unwrap_or((0, 0));
        let detached_window = ui.get_detached_window();
        let software_presentation_enabled = ui.get_software_presentation_enabled();
        let signature = TerminalGeometrySignature {
            window_physical_width: window_size.width,
            window_physical_height: window_size.height,
            winit_physical_width,
            winit_physical_height,
            scale_milli: quantize_scale(scale_factor),
            pane_x: quantize_logical(sample.pane_x),
            pane_y: quantize_logical(sample.pane_y),
            pane_width: quantize_logical(sample.pane_width),
            pane_height: quantize_logical(sample.pane_height),
            grid_x: quantize_logical(sample.grid_x),
            grid_y: quantize_logical(sample.grid_y),
            grid_width: quantize_logical(sample.grid_width),
            grid_height: quantize_logical(sample.grid_height),
            cell_width: quantize_logical(sample.cell_width),
            cell_height: quantize_logical(sample.cell_height),
            columns: sample.columns,
            rows: sample.rows,
            detached_window,
            software_presentation_enabled,
        };
        let key = format!("{window_id}:{terminal_id}");
        if self
            .last
            .get(&key)
            .is_some_and(|previous| *previous == signature)
        {
            return;
        }
        if !self.last.contains_key(&key) && self.last.len() >= 256 {
            self.last.clear();
        }
        self.last.insert(key, signature);

        let active_tab_kind = ui.get_active_tab_kind();
        let renderer_preference = ui.get_renderer_preference();
        let window_logical_width = f64::from(window_size.width) / scale_factor;
        let window_logical_height = f64::from(window_size.height) / scale_factor;
        let pane_right_gap =
            window_logical_width - f64::from(sample.pane_x) - f64::from(sample.pane_width);
        let pane_bottom_gap =
            window_logical_height - f64::from(sample.pane_y) - f64::from(sample.pane_height);
        let grid_right_gap = f64::from(sample.pane_width)
            - (f64::from(sample.grid_x) - f64::from(sample.pane_x))
            - f64::from(sample.grid_width);
        let grid_bottom_gap = f64::from(sample.pane_height)
            - (f64::from(sample.grid_y) - f64::from(sample.pane_y))
            - f64::from(sample.grid_height);
        let terminal_columns = sample.columns.max(0);
        let terminal_rows = sample.rows.max(0);
        let cell_remainder_width = f64::from(sample.grid_width)
            - f64::from(sample.cell_width) * f64::from(terminal_columns);
        let cell_remainder_height = f64::from(sample.grid_height)
            - f64::from(sample.cell_height) * f64::from(terminal_rows);

        tracing::debug!(
            target: "ax_ssh::diagnostics",
            event = "terminal-geometry",
            window_id = %window_id,
            terminal_id,
            active_tab_kind = active_tab_kind.as_str(),
            detached_window,
            renderer_preference = renderer_preference.as_str(),
            software_presentation_enabled,
            scale_factor,
            window_physical_width = window_size.width,
            window_physical_height = window_size.height,
            winit_physical_width,
            winit_physical_height,
            window_logical_width,
            window_logical_height,
            pane_x = sample.pane_x,
            pane_y = sample.pane_y,
            pane_width = sample.pane_width,
            pane_height = sample.pane_height,
            pane_right_gap,
            pane_bottom_gap,
            grid_x = sample.grid_x,
            grid_y = sample.grid_y,
            grid_width = sample.grid_width,
            grid_height = sample.grid_height,
            grid_right_gap,
            grid_bottom_gap,
            cell_width = sample.cell_width,
            cell_height = sample.cell_height,
            terminal_columns = sample.columns,
            terminal_rows = sample.rows,
            cell_remainder_width,
            cell_remainder_height,
            "terminal geometry changed"
        );
    }
}

fn quantize_logical(value: f32) -> i32 {
    if !value.is_finite() {
        return i32::MIN;
    }
    (f64::from(value) * 10.0)
        .round()
        .clamp(f64::from(i32::MIN + 1), f64::from(i32::MAX)) as i32
}

fn quantize_scale(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    (value * 1000.0).round().clamp(0.0, f64::from(u32::MAX)) as u32
}

pub(super) fn start_local_shell(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    register_tab: impl FnOnce(Uuid, &mut AppState) -> bool,
) -> Result<Uuid> {
    let (tab_id, events) = {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        let shell = app.sessions.settings.terminal.local_shell.clone();
        let columns = u32::from(app.sessions.settings.terminal.default_columns);
        let rows = u32::from(app.sessions.settings.terminal.default_rows);
        let tab_id = app.open_local_shell_tab();
        if !register_tab(tab_id, &mut app) {
            let _ = app.close_tab(tab_id);
            anyhow::bail!("cannot attach local shell to the requested terminal pane");
        }
        let (worker, events) = LocalShellHandle::spawn(shell, columns, rows);
        let terminal = app
            .terminal_mut(tab_id)
            .context("local terminal tab disappeared while starting worker")?;
        terminal.worker = Some(TerminalWorker::Local(worker));
        (tab_id, events)
    };
    refresh_workspace(&ui, &state);
    spawn_local_shell_monitor(runtime, state, ui, tab_id, events);
    Ok(tab_id)
}

pub(super) fn resume_existing_local_shell(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
) -> Result<()> {
    let events = {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        let shell = app.sessions.settings.terminal.local_shell.clone();
        let columns = u32::from(app.sessions.settings.terminal.default_columns);
        let rows = u32::from(app.sessions.settings.terminal.default_rows);
        let terminal = app
            .terminal_mut(tab_id)
            .context("restored local tab disappeared")?;
        if terminal.worker.is_some() {
            return Ok(());
        }
        let (worker, events) = LocalShellHandle::spawn(shell.clone(), columns, rows);
        terminal.worker = Some(TerminalWorker::Local(worker));
        terminal.worker_running = true;
        terminal.connected = false;
        terminal.status = "Restored; starting local shell...".to_owned();
        events
    };
    refresh_workspace(&ui, &state);
    spawn_local_shell_monitor(runtime, state, ui, tab_id, events);
    Ok(())
}

/// Register after a Slint window is shown, when its Winit adapter exists.
///
/// Native modifier snapshots are recorded for every platform before Slint
/// dispatches the corresponding key event. Physical Winit key identity is
/// normalized at this boundary for application-keypad input on every desktop
/// platform. Normal text and IME input continue through Slint's TextInput path.
pub(super) fn install_terminal_keypad_input_hook(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let modifiers = Rc::new(Cell::new(ModifiersState::default()));
    let modifiers_for_event = modifiers.clone();
    let ui_for_keypad = ui.as_weak();
    let state_for_drop = state.clone();
    let runtime_for_drop = runtime.clone();
    let router_for_drop = window_router.clone();
    let ui_for_drop = ui.as_weak();
    ui.window().on_winit_window_event(move |_window, event| {
        match event {
            WindowEvent::DroppedFile(path) => {
                super::sftp_bridge::handle_native_dropped_file(
                    &runtime_for_drop,
                    &state_for_drop,
                    &ui_for_drop,
                    &router_for_drop,
                    window_id,
                    path,
                );
            }
            WindowEvent::ModifiersChanged(next) => {
                modifiers_for_event.set(next.state());
                let state = next.state();
                update_native_event_modifiers(
                    state.alt_key(),
                    state.control_key(),
                    state.super_key(),
                    state.shift_key(),
                );
            }
            WindowEvent::Focused(false) => {
                modifiers_for_event.set(ModifiersState::default());
                clear_native_event_modifiers();
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                if *is_synthetic || event.state != ElementState::Pressed {
                    return EventResult::Propagate;
                }
                let Some(ui) = ui_for_keypad.upgrade() else {
                    return EventResult::Propagate;
                };
                if ui.get_active_tab_kind().as_str() != "terminal" {
                    return EventResult::Propagate;
                }
                let modifiers = modifiers_for_event.get();
                let mut physical_modifiers = TerminalModifiers {
                    alt: modifiers.alt_key(),
                    control: modifiers.control_key(),
                    meta: modifiers.super_key(),
                    shift: modifiers.shift_key(),
                };
                #[cfg(target_os = "macos")]
                if !physical_modifiers.control {
                    let current = super::macos_window::current_modifier_state();
                    if current.control || current.meta || current.alt || current.shift {
                        physical_modifiers = current;
                        update_native_event_modifiers(
                            current.alt,
                            current.control,
                            current.meta,
                            current.shift,
                        );
                    }
                }

                #[cfg(target_os = "macos")]
                if physical_modifiers.control && !physical_modifiers.meta {
                    let Some(key_name) = native_shortcut_key_name(&event.logical_key) else {
                        return EventResult::Propagate;
                    };
                    let settings = match state.lock() {
                        Ok(app) => app.sessions.settings.shortcuts.clone(),
                        Err(_) => return EventResult::Propagate,
                    };
                    let application_shortcut = [
                        settings.open_settings.as_str(),
                        settings.new_session.as_str(),
                        settings.import_sessions.as_str(),
                        settings.export_selected.as_str(),
                        settings.toggle_sidebar.as_str(),
                        settings.copy_selection.as_str(),
                        settings.paste.as_str(),
                        settings.open_sftp.as_str(),
                    ]
                    .into_iter()
                    .any(|shortcut| {
                        native_shortcut_matches_setting(shortcut, &key_name, physical_modifiers)
                    });
                    if application_shortcut {
                        return EventResult::Propagate;
                    }
                    let Some(input_event) = normalized_keyboard_input_from_winit(
                        event,
                        physical_modifiers,
                        *is_synthetic,
                    ) else {
                        return EventResult::Propagate;
                    };
                    let Some(tab_id) = window_router.active_tab(window_id) else {
                        return EventResult::Propagate;
                    };
                    let input = TerminalInputContext {
                        ui: &ui_for_keypad,
                        state: &state,
                        window_router: &window_router,
                        window_id,
                    };
                    if input.dispatch(tab_id, input_event) {
                        return EventResult::PreventDefault;
                    }
                }

                let Some(input_event) =
                    normalized_keyboard_input_from_winit(event, physical_modifiers, *is_synthetic)
                else {
                    return EventResult::Propagate;
                };
                let modifiers = input_event.modifiers;
                if !input_event.is_physical_keypad()
                    || modifiers.alt
                    || modifiers.control
                    || modifiers.meta
                    || modifiers.shift
                {
                    return EventResult::Propagate;
                }
                let Some(tab_id) = window_router.active_tab(window_id) else {
                    return EventResult::Propagate;
                };
                let input = TerminalInputContext {
                    ui: &ui_for_keypad,
                    state: &state,
                    window_router: &window_router,
                    window_id,
                };
                if !input.application_keypad_active(tab_id) {
                    return EventResult::Propagate;
                }
                if input.dispatch(tab_id, input_event) {
                    return EventResult::PreventDefault;
                }
            }
            _ => {}
        }
        EventResult::Propagate
    });
}

struct TerminalInputContext<'a> {
    ui: &'a slint::Weak<AppWindow>,
    state: &'a Arc<Mutex<AppState>>,
    window_router: &'a WindowRouter,
    window_id: Uuid,
}

impl TerminalInputContext<'_> {
    fn application_keypad_active(&self, tab_id: Uuid) -> bool {
        self.state.lock().is_ok_and(|app| {
            !self
                .window_router
                .workspace_actions_locked(self.window_id, &app)
                && self
                    .window_router
                    .owns_terminal_pane(self.window_id, tab_id, &app)
                && app.terminal(tab_id).is_some_and(|terminal| {
                    terminal.connected
                        && terminal
                            .terminal
                            .as_ref()
                            .is_some_and(|model| model.application_keypad())
                })
        })
    }

    fn dispatch(&self, tab_id: Uuid, input: super::input::NormalizedKeyboardInput) -> bool {
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
                let application_cursor = model.application_cursor();
                let application_keypad = model.application_keypad();
                let Some(key) = super::input::terminal_key_from_normalized_input(&input) else {
                    return Ok((false, false));
                };
                let Some(data) =
                    encode_key_with_modes(&key, modifiers, application_cursor, application_keypad)
                else {
                    return Ok((false, false));
                };
                let viewport_changed = app.scroll_terminal_to_bottom(tab_id);
                {
                    let terminal = app.terminal(tab_id).context("terminal tab not found")?;
                    let worker_request_started_at = std::time::Instant::now();
                    let request_result = terminal
                        .worker
                        .as_ref()
                        .context("active terminal has no worker")?
                        .request_send(data);
                    worker_request_elapsed = Some(worker_request_started_at.elapsed());
                    request_result?;
                }
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

pub(super) fn wire_terminal(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    font_registry: Arc<Mutex<FontRegistry>>,
    terminal_font_started: Arc<std::sync::atomic::AtomicBool>,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let geometry_diagnostics = Rc::new(RefCell::new(TerminalGeometryDiagnostics::default()));
    let geometry_diagnostics_for_callback = geometry_diagnostics.clone();
    let ui_for_geometry = ui.as_weak();
    ui.on_terminal_geometry(
        move |terminal_id,
              pane_x,
              pane_y,
              pane_width,
              pane_height,
              grid_x,
              grid_y,
              grid_width,
              grid_height,
              cell_width,
              cell_height,
              columns,
              rows| {
            if let Some(ui) = ui_for_geometry.upgrade() {
                geometry_diagnostics_for_callback.borrow_mut().record(
                    &ui,
                    window_id,
                    terminal_id.as_str(),
                    TerminalGeometrySample {
                        pane_x,
                        pane_y,
                        pane_width,
                        pane_height,
                        grid_x,
                        grid_y,
                        grid_width,
                        grid_height,
                        cell_width,
                        cell_height,
                        columns,
                        rows,
                    },
                );
            }
        },
    );

    let ui_for_presentation = ui.as_weak();
    ui.on_terminal_presentation_layout(move |terminal_id, x, y, width, height, row_height| {
        if let Some(ui) = ui_for_presentation.upgrade() {
            software_presentation::update_region(
                &ui,
                window_id,
                terminal_id.as_str(),
                software_presentation::LogicalRegion {
                    x,
                    y,
                    width,
                    height,
                    row_height,
                },
            );
        }
    });
    let ui_for_presentation_reset = ui.as_weak();
    ui.on_terminal_presentation_layout_reset(move || {
        if let Some(ui) = ui_for_presentation_reset.upgrade() {
            software_presentation::clear_layout(&ui, window_id);
        }
    });

    let ui_for_theme = ui.as_weak();
    let state_for_theme = state.clone();
    ui.on_refresh_terminal_appearance(move || {
        log_ui_action("terminal.refresh-appearance");
        // A theme change only changes the visual snapshot; it must not resize or
        // otherwise disturb the PTY worker that owns the active terminal.
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
    ui.on_resize_terminal(move |tab_id, columns, rows| {
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
                app.resize_terminal(tab_id, columns, rows)
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
    ui.on_mouse_event(
        move |tab_id, row, column, button, kind, shift, alt, control| {
            let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_mouse) else {
                return;
            };
            let button = match button {
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
            let kind = match kind {
                0 => TerminalMouseEventKind::Press,
                1 => TerminalMouseEventKind::Release,
                2 => TerminalMouseEventKind::Motion,
                _ => return,
            };
            let result = state_for_mouse
                .lock()
                .map_err(|_| anyhow::anyhow!("state lock poisoned"))
                .and_then(|mut app| {
                    if !router_for_mouse.owns_terminal_pane(window_id, tab_id, &app) {
                        anyhow::bail!("terminal pane is no longer visible in this window");
                    }
                    let terminal = app.terminal_mut(tab_id).context("terminal tab not found")?;
                    if !terminal.connected {
                        return Ok(true);
                    }
                    let model = terminal
                        .terminal
                        .as_ref()
                        .context("active tab has no terminal model")?;
                    let Some(data) = model.encode_mouse_event(TerminalMouseEvent {
                        kind,
                        button,
                        column: column.max(0) as usize,
                        row: row.max(0) as usize,
                        modifiers: TerminalMouseModifiers {
                            shift,
                            alt,
                            control,
                        },
                    }) else {
                        return Ok(true);
                    };
                    let worker = terminal
                        .worker
                        .as_ref()
                        .context("active terminal has no worker")?;
                    if kind == TerminalMouseEventKind::Motion {
                        worker.request_send_motion(data)
                    } else {
                        worker.request_send(data).map(|()| true)
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
        },
    );

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
                        terminal
                            .semantic_selection_range(row.max(0) as usize, column.max(0) as usize)
                    })
            })
            .map(|range| TerminalSemanticSelection {
                active: true,
                anchor_row: range.start_row as i32,
                anchor_column: range.start_column as i32,
                focus_row: range.end_row as i32,
                focus_column: range.end_column as i32,
            })
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
            .map(|range| TerminalSemanticSelection {
                active: true,
                anchor_row: range.start_row as i32,
                anchor_column: range.start_column as i32,
                focus_row: range.end_row as i32,
                focus_column: range.end_column as i32,
            })
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

    let ui_for_focus = ui.as_weak();
    let state_for_focus = state.clone();
    let router_for_focus = window_router.clone();
    ui.on_terminal_pane_focus(move |tab_id| {
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_focus) else {
            return;
        };
        let layout = state_for_focus
            .lock()
            .ok()
            .and_then(|mut app| router_for_focus.focus_terminal_pane(window_id, tab_id, &mut app));
        if let Some(layout) = layout {
            let applied_in_place = ui_for_focus
                .upgrade()
                .is_some_and(|ui| apply_terminal_pane_layout(&ui, layout));
            if !applied_in_place {
                refresh_workspace(&ui_for_focus, &state_for_focus);
            }
        }
    });

    let ui_for_divider = ui.as_weak();
    let state_for_divider = state.clone();
    let router_for_divider = window_router.clone();
    ui.on_resize_terminal_divider(move |divider_id, ratio| {
        let Some(layout) = router_for_divider.resize_terminal_divider(window_id, divider_id, ratio)
        else {
            return false;
        };
        let applied_in_place = ui_for_divider
            .upgrade()
            .is_some_and(|ui| apply_terminal_pane_layout(&ui, layout));
        if !applied_in_place {
            refresh_workspace(&ui_for_divider, &state_for_divider);
        }
        true
    });

    let ui_for_command = ui.as_weak();
    let state_for_command = state.clone();
    let runtime_for_command = runtime;
    let font_registry_for_command = font_registry;
    let terminal_font_started_for_command = terminal_font_started;
    let router_for_command = window_router;
    ui.on_terminal_pane_command(move |tab_id, command| {
        if state_for_command
            .lock()
            .is_ok_and(|app| router_for_command.workspace_actions_locked(window_id, &app))
        {
            return false;
        }
        let Some(tab_id) = parse_uuid(tab_id.as_str(), "terminal", &ui_for_command) else {
            return false;
        };
        if command.as_str() == "close" {
            return close_terminal_child_pane(
                &router_for_command,
                Some(window_id),
                tab_id,
                &state_for_command,
                &ui_for_command,
                &runtime_for_command,
            );
        }
        if command.as_str() == "close-tab" {
            return close_terminal_notice_tab(
                &router_for_command,
                window_id,
                tab_id,
                &state_for_command,
                &ui_for_command,
                &runtime_for_command,
            );
        }
        if command.as_str() == "retry" {
            return retry_terminal_notice_tab(
                &router_for_command,
                window_id,
                tab_id,
                &state_for_command,
                &ui_for_command,
                &runtime_for_command,
                &font_registry_for_command,
                &terminal_font_started_for_command,
            );
        }
        let Some((direction, action)) = PaneDirection::from_command(command.as_str()) else {
            return false;
        };
        match action {
            PaneCommand::Focus => {
                let layout = state_for_command.lock().ok().and_then(|mut app| {
                    router_for_command
                        .focus_terminal_pane(window_id, tab_id, &mut app)
                        .and_then(|_| {
                            router_for_command.focus_pane_direction(window_id, direction, &mut app)
                        })
                });
                let Some(layout) = layout else {
                    return false;
                };
                let applied_in_place = ui_for_command
                    .upgrade()
                    .is_some_and(|ui| apply_terminal_pane_layout(&ui, layout));
                if !applied_in_place {
                    refresh_workspace(&ui_for_command, &state_for_command);
                }
                true
            }
            PaneCommand::Split => {
                let source = match state_for_command.lock() {
                    Ok(mut app) => {
                        if router_for_command.prepare_pane_split(window_id, tab_id, &mut app) {
                            app.pane_session_source(tab_id)
                        } else {
                            None
                        }
                    }
                    Err(_) => {
                        set_status(&ui_for_command, "Cannot read workspace state");
                        None
                    }
                };
                let Some(source) = source else {
                    return false;
                };
                let new_tab_id = match source {
                    PaneSessionSource::LocalShell => {
                        load_terminal_font_on_demand(
                            &runtime_for_command,
                            ui_for_command.clone(),
                            state_for_command.clone(),
                            font_registry_for_command.clone(),
                            terminal_font_started_for_command.clone(),
                        );
                        match start_local_shell(
                            &runtime_for_command,
                            state_for_command.clone(),
                            ui_for_command.clone(),
                            {
                                let router = router_for_command.clone();
                                move |new_tab_id, app| {
                                    router.complete_pane_split(
                                        window_id, tab_id, direction, new_tab_id, app,
                                    )
                                }
                            },
                        ) {
                            Ok(tab_id) => Some(tab_id),
                            Err(error) => {
                                set_status(
                                    &ui_for_command,
                                    &format!("Cannot create terminal pane: {error}"),
                                );
                                None
                            }
                        }
                    }
                    PaneSessionSource::ProfileConnection(profile_id) => {
                        let connection = ConnectionContext::new(
                            ui_for_command.clone(),
                            state_for_command.clone(),
                            runtime_for_command.clone(),
                            font_registry_for_command.clone(),
                            terminal_font_started_for_command.clone(),
                        );
                        request_profile_connection(
                            &connection,
                            profile_id,
                            ConnectionTarget::Terminal,
                            None,
                            None,
                            None,
                            {
                                let router = router_for_command.clone();
                                move |new_tab_id, app| {
                                    router.complete_pane_split(
                                        window_id, tab_id, direction, new_tab_id, app,
                                    )
                                }
                            },
                        )
                    }
                };
                let Some(_) = new_tab_id else {
                    return false;
                };
                true
            }
        }
    });
}

fn close_terminal_notice_tab(
    router: &WindowRouter,
    window_id: Uuid,
    tab_id: Uuid,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    runtime: &Handle,
) -> bool {
    if close_terminal_child_pane(router, Some(window_id), tab_id, state, ui, runtime) {
        return true;
    }
    if !state.lock().ok().is_some_and(|app| {
        router.owns_terminal_pane(window_id, tab_id, &app)
            || router.tab_ids(window_id, &app).contains(&tab_id)
    }) {
        return false;
    }
    close_workspace_tab(tab_id, state, ui, runtime);
    true
}

enum TerminalRetryRoute {
    Local,
    Profile {
        profile_id: Uuid,
        target: ConnectionTarget,
    },
}

#[allow(clippy::too_many_arguments)]
fn retry_terminal_notice_tab(
    router: &WindowRouter,
    window_id: Uuid,
    tab_id: Uuid,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    runtime: &Handle,
    font_registry: &Arc<Mutex<FontRegistry>>,
    terminal_font_started: &Arc<std::sync::atomic::AtomicBool>,
) -> bool {
    let route = match state.lock() {
        Ok(mut app) => {
            if !router.owns_terminal_pane(window_id, tab_id, &app) {
                return false;
            }
            let Some(terminal) = app.terminal_mut(tab_id) else {
                return false;
            };
            if terminal.worker.is_some() || terminal.worker_running {
                return false;
            }
            terminal.prepare_manual_retry();
            if terminal.is_local() {
                TerminalRetryRoute::Local
            } else if let Some(profile_id) = terminal.profile_id() {
                TerminalRetryRoute::Profile {
                    profile_id,
                    target: terminal.connection_target(),
                }
            } else {
                return false;
            }
        }
        Err(_) => {
            set_status(ui, "Cannot read workspace state");
            return false;
        }
    };

    match route {
        TerminalRetryRoute::Local => {
            if let Err(error) =
                resume_existing_local_shell(runtime, state.clone(), ui.clone(), tab_id)
            {
                set_tab_status(
                    state,
                    ui,
                    tab_id,
                    &format!("Cannot restart terminal: {error}"),
                );
                return false;
            }
        }
        TerminalRetryRoute::Profile { profile_id, target } => {
            let connection = ConnectionContext::new(
                ui.clone(),
                state.clone(),
                runtime.clone(),
                font_registry.clone(),
                terminal_font_started.clone(),
            );
            resume_existing_connection(&connection, tab_id, profile_id, target);
        }
    }
    true
}

fn terminal_target_modifier_held(control: bool, _meta: bool) -> bool {
    // Slint normalizes the platform primary shortcut modifier into `control`:
    // Cmd on macOS and Ctrl elsewhere.
    control
}

fn terminal_target_for_pane(
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
    let context = terminal
        .terminal
        .as_ref()?
        .visible_logical_line_target_context_at_cell(row, column)?;
    terminal_target_match_at_context(&context).map(|target_match| target_match.target)
}

fn terminal_target_highlight_for_pane(
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
fn open_terminal_remote_path(
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

fn open_terminal_url(runtime: &Handle, ui: slint::Weak<AppWindow>, url: String) {
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

pub(super) fn spawn_local_shell_monitor(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    mut events: mpsc::Receiver<LocalShellEvent>,
) {
    let runtime_for_monitor = runtime.clone();
    runtime.spawn(async move {
        let mut terminal_event = false;
        let mut finished_worker = None;
        let mut presentation =
            crate::app::terminal_presentation::TerminalPresentation::new();
        loop {
            let event = tokio::select! {
                event = events.recv() => {
                    let Some(event) = event else {
                        break;
                    };
                    event
                }
                _ = presentation.wait_until_ready(tab_id), if presentation.has_pending_output() => {
                    if prepare_terminal_output_snapshot(&state, tab_id) {
                        dispatch_terminal_snapshot(&ui, &state, tab_id);
                    }
                    continue;
                }
            };
            match event {
                LocalShellEvent::Started { shell } => {
                    let Some(active) = mutate_local_terminal(&state, tab_id, |terminal| {
                        terminal.connected = true;
                        terminal.worker_running = true;
                        terminal.status = format!("Local shell: {shell}");
                    }) else {
                        continue;
                    };
                    info!(tab_id = %tab_id, shell = %shell, "local shell started");
                    if active {
                        dispatch_active_snapshot(&ui, &state);
                    }
                    refresh_workspace(&ui, &state);
                }
                LocalShellEvent::Output(data) => {
                    let mut response_error = None;
                    let mut presentation_hold = None;
                    if mutate_local_terminal(&state, tab_id, |terminal| {
                        match process_terminal_output(terminal, &data) {
                            Ok(hold) => presentation_hold = hold,
                            Err(error) => response_error = Some(error),
                        }
                    })
                    .is_some()
                        && !data.is_empty()
                    {
                        presentation.record_output(None, presentation_hold);
                    }
                    if let Some(error) = response_error {
                        warn!(tab_id = %tab_id, %error, "failed to send local terminal protocol response");
                    }
                }
                // The UI updates its terminal snapshot as soon as this resize request is accepted.
                // Ignoring this later acknowledgement prevents a stale worker event from reverting it.
                LocalShellEvent::Resized { .. } => {}
                LocalShellEvent::Exited { status } => {
                    terminal_event = true;
                    presentation.clear_pending_output();
                    if let Some(finished) = finish_local_terminal(
                        &state,
                        tab_id,
                        &format!("Local shell exited: {status}"),
                    ) {
                        finished_worker = finished.worker;
                        if !global_window_router().is_some_and(|router| {
                            close_terminal_child_pane(
                                &router,
                                None,
                                tab_id,
                                &state,
                                &ui,
                                &runtime_for_monitor,
                            )
                        }) {
                            close_workspace_tab(
                                tab_id,
                                &state,
                                &ui,
                                &runtime_for_monitor,
                            );
                        }
                    }
                    break;
                }
                LocalShellEvent::Failed(message) => {
                    terminal_event = true;
                    presentation.clear_pending_output();
                    warn!(tab_id = %tab_id, error = %message, "local shell worker failed");
                    if let Some(finished) = finish_local_terminal(
                        &state,
                        tab_id,
                        &format!("Local shell failed: {message}"),
                    ) {
                        finished_worker = finished.worker;
                        refresh_workspace(&ui, &state);
                    }
                    break;
                }
            }
        }
        presentation.clear_pending_output();
        if !terminal_event
            && let Some(finished) =
                finish_local_terminal(&state, tab_id, "Local shell worker stopped")
        {
            finished_worker = finished.worker;
            refresh_workspace(&ui, &state);
        }
        if let Some(worker) = finished_worker
            && let Err(error) = worker.shutdown().await
        {
            warn!(tab_id = %tab_id, %error, "failed to reclaim stopped local shell worker");
        }
        debug!(tab_id = %tab_id, "local shell event monitor stopped");
    });
}

pub(super) fn process_terminal_output(
    terminal: &mut TerminalTabState,
    data: &[u8],
) -> Result<Option<Duration>> {
    let model = terminal
        .terminal
        .as_mut()
        .context("terminal tab has no terminal model")?;
    let responses = model.process_with_responses(data);
    let presentation_hold = model.output_frame_hold_remaining();
    if responses.is_empty() {
        return Ok(presentation_hold);
    }
    let worker = terminal
        .worker
        .as_ref()
        .context("terminal protocol response has no transport worker")?;
    for response in responses {
        worker
            .request_send(response)
            .context("cannot queue terminal protocol response")?;
    }
    Ok(presentation_hold)
}

pub(super) fn mutate_local_terminal(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    action: impl FnOnce(&mut TerminalTabState),
) -> Option<bool> {
    let mut app = state.lock().ok()?;
    if !app.terminal(tab_id).is_some_and(TerminalTabState::is_local) {
        return None;
    }
    action(app.terminal_mut(tab_id)?);
    Some(true)
}

struct FinishedLocalTerminal {
    worker: Option<TerminalWorker>,
}

fn finish_local_terminal(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    status: &str,
) -> Option<FinishedLocalTerminal> {
    match state.lock() {
        Ok(mut app) if app.terminal(tab_id).is_some_and(TerminalTabState::is_local) => {
            let terminal = app.terminal_mut(tab_id)?;
            let worker = terminal.worker.take();
            terminal.connected = false;
            terminal.worker_running = false;
            terminal.status = status.to_owned();
            Some(FinishedLocalTerminal { worker })
        }
        Ok(_) | Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_target_uses_slint_primary_shortcut_modifier() {
        assert!(terminal_target_modifier_held(true, false));
        assert!(!terminal_target_modifier_held(false, true));
    }

    #[test]
    fn terminal_geometry_quantization_is_stable_for_diagnostics() {
        assert_eq!(quantize_logical(12.34), 123);
        assert_eq!(quantize_logical(12.36), 124);
        assert_eq!(quantize_logical(f32::NAN), i32::MIN);
        assert_eq!(quantize_scale(2.0), 2_000);
        assert_eq!(quantize_scale(f64::NAN), 0);
    }

    #[test]
    fn terminal_output_preserves_local_selection_revision() {
        let mut app = AppState::new(
            ConfigStore::new(
                std::env::temp_dir().join(format!("axssh-output-revision-{}.json", Uuid::new_v4())),
            ),
            SessionStore::default(),
        );
        let tab_id = app.open_local_shell_tab();
        let terminal = app
            .terminal_mut(tab_id)
            .expect("local terminal should exist");
        let before = terminal.selection_revision;

        process_terminal_output(terminal, b"output").expect("terminal output should be processed");

        assert_eq!(terminal.selection_revision, before);

        process_terminal_output(terminal, b"").expect("empty output should be accepted");

        assert_eq!(terminal.selection_revision, before);
    }

    #[cfg(not(windows))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn finishing_local_terminal_returns_its_worker_for_explicit_shutdown() {
        let state = Arc::new(Mutex::new(AppState::new(
            ConfigStore::new(
                std::env::temp_dir().join(format!("axssh-local-finish-{}.json", Uuid::new_v4())),
            ),
            SessionStore::default(),
        )));
        let (tab_id, mut events) = {
            let mut app = state.lock().expect("state should lock");
            let tab_id = app.open_local_shell_tab();
            let (worker, events) =
                LocalShellHandle::spawn(ax_ssh::local_shell::SYSTEM_SHELL.into(), 80, 24);
            app.terminal_mut(tab_id)
                .expect("local terminal should exist")
                .worker = Some(TerminalWorker::Local(worker));
            (tab_id, events)
        };

        let started = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("local shell should report startup");
        assert!(matches!(started, Some(LocalShellEvent::Started { .. })));

        let finished = finish_local_terminal(&state, tab_id, "finished")
            .expect("local terminal should transition to finished");
        let worker = finished
            .worker
            .expect("finished transition must preserve the worker owner");
        {
            let app = state.lock().expect("state should lock");
            let terminal = app.terminal(tab_id).expect("local terminal should remain");
            assert!(terminal.worker.is_none());
            assert!(!terminal.connected);
            assert!(!terminal.worker_running);
            assert_eq!(terminal.status, "finished");
        }
        tokio::time::timeout(Duration::from_secs(5), worker.shutdown())
            .await
            .expect("worker shutdown must remain bounded")
            .expect("finished local worker should shut down cleanly");
    }

    #[cfg(not(windows))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn local_shell_exit_closes_its_workspace_tab_and_selects_following_tab() {
        let state = Arc::new(Mutex::new(AppState::new(
            ConfigStore::new(
                std::env::temp_dir().join(format!("axssh-local-exit-{}.json", Uuid::new_v4())),
            ),
            SessionStore::default(),
        )));
        let (exited_tab_id, following_tab_id, events) = {
            let mut app = state.lock().expect("state should lock");
            let _preceding_tab_id = app.open_local_shell_tab();
            let exited_tab_id = app.open_local_shell_tab();
            let following_tab_id = app.open_local_shell_tab();
            assert!(app.activate_tab(exited_tab_id));

            let (worker, events) =
                LocalShellHandle::spawn(ax_ssh::local_shell::SYSTEM_SHELL.into(), 80, 24);
            worker
                .request_send(b"exit\n".to_vec())
                .expect("exit command should queue");
            app.terminal_mut(exited_tab_id)
                .expect("local terminal should exist")
                .worker = Some(TerminalWorker::Local(worker));
            (exited_tab_id, following_tab_id, events)
        };

        spawn_local_shell_monitor(
            &Handle::current(),
            state.clone(),
            slint::Weak::<AppWindow>::default(),
            exited_tab_id,
            events,
        );

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let closed_and_focused = state.lock().is_ok_and(|app| {
                    app.terminal(exited_tab_id).is_none()
                        && app.active_tab_id() == Some(following_tab_id)
                });
                if closed_and_focused {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("local exit should close its tab and focus the following tab");
    }
}
