use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

use super::input::{clear_native_event_modifiers, update_native_event_modifiers};
#[cfg(target_os = "macos")]
use super::input::{
    native_shortcut_key_name, native_shortcut_matches_setting, normalized_keyboard_input_from_winit,
};
use super::*;
use crate::app::state::PaneSessionSource;
use crate::app::terminal_targets::{TerminalTarget, terminal_target_match_at_context};
use ax_ssh::terminal::{
    TerminalModel, TerminalModifiers, TerminalMouseButton, TerminalMouseEvent,
    TerminalMouseEventKind, TerminalMouseModifiers, TerminalSelectionRange, TerminalTargetContext,
    encode_key,
};
#[cfg(target_os = "macos")]
use slint::winit_030::winit::event::ElementState;
use slint::winit_030::{
    EventResult, WinitWindowAccessor,
    winit::{event::WindowEvent, keyboard::ModifiersState},
};

const MAX_MOUSE_WHEEL_REPORTS: i32 = 256;
const OSC52_CLIPBOARD_READ_TIMEOUT: Duration = Duration::from_secs(20);

fn sync_terminal_query_palette(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let palette = super::view::terminal::terminal_query_palette(ui);
    if let Ok(mut app) = state.lock() {
        app.set_terminal_query_palette(palette);
    }
}

/// A `DroppedFile` has no location. On macOS, the native bridge obtains the
/// current AppKit cursor position at drop time; other platforms rely on the
/// latest Winit cursor move from the current external-file hover. Every route
/// resolves the declared Remote files target before it creates an upload.
#[derive(Default)]
struct NativeFileDropPointer {
    hovered_file_count: u16,
    upload_batch: Option<(Uuid, Instant)>,
    #[cfg(not(target_os = "macos"))]
    last_physical_position: Option<(f64, f64)>,
}

impl NativeFileDropPointer {
    fn begin_external_file_hover(&mut self) {
        if self.hovered_file_count == 0
            && self
                .upload_batch
                .is_some_and(|(_, last)| last.elapsed() > Duration::from_millis(500))
        {
            self.upload_batch = None;
        }
        #[cfg(not(target_os = "macos"))]
        if self.hovered_file_count == 0 {
            self.last_physical_position = None;
        }
        self.hovered_file_count = self.hovered_file_count.saturating_add(1);
    }

    fn complete_external_file_drop(&mut self) {
        self.hovered_file_count = self.hovered_file_count.saturating_sub(1);
        #[cfg(not(target_os = "macos"))]
        if self.hovered_file_count == 0 {
            self.last_physical_position = None;
        }
    }

    fn upload_batch_id(&mut self) -> Uuid {
        let now = Instant::now();
        let id = self
            .upload_batch
            .filter(|(_, last)| now.duration_since(*last) <= Duration::from_millis(500))
            .map_or_else(Uuid::new_v4, |(id, _)| id);
        self.upload_batch = Some((id, now));
        id
    }

    #[cfg(not(target_os = "macos"))]
    fn record_cursor_position(&mut self, x: f64, y: f64) {
        if x.is_finite() && y.is_finite() {
            self.last_physical_position = Some((x, y));
        }
    }

    #[cfg(target_os = "macos")]
    fn clear(&mut self) {
        self.hovered_file_count = 0;
        self.upload_batch = None;
    }

    #[cfg(not(target_os = "macos"))]
    fn clear(&mut self) {
        self.hovered_file_count = 0;
        self.upload_batch = None;
        self.last_physical_position = None;
    }

    #[cfg(not(target_os = "macos"))]
    fn logical_position(&self, scale_factor: f64) -> Option<(f32, f32)> {
        if self.hovered_file_count == 0 || !scale_factor.is_finite() || scale_factor <= 0.0 {
            return None;
        }
        let (x, y) = self.last_physical_position?;
        let x = x / scale_factor;
        let y = y / scale_factor;
        if !x.is_finite()
            || !y.is_finite()
            || x < f64::from(f32::MIN)
            || x > f64::from(f32::MAX)
            || y < f64::from(f32::MIN)
            || y > f64::from(f32::MAX)
        {
            return None;
        }
        Some((x as f32, y as f32))
    }
}

#[cfg(target_os = "macos")]
fn macos_file_drop_position(
    read_current_position: impl FnOnce() -> Option<(f32, f32)>,
) -> Option<(f32, f32)> {
    read_current_position()
}

fn log_native_file_drop(stage: &'static str, hovered_file_count: u16, target: Option<&str>) {
    tracing::debug!(
        target: "ax_ssh::sftp_drag",
        event = "native-file-drop",
        stage,
        hovered_file_count,
        target = target.unwrap_or("not-resolved"),
        "SFTP native file drop route"
    );
}

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
/// dispatches the corresponding key event. Normal text, IME input, and
/// physical keypad input continue through Slint's standard keyboard path.
pub(super) fn install_native_window_input_hook(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    let modifiers = Rc::new(Cell::new(ModifiersState::default()));
    let modifiers_for_event = modifiers.clone();
    #[cfg(target_os = "macos")]
    let ui_for_native = ui.as_weak();
    let state_for_drop = state.clone();
    let runtime_for_drop = runtime.clone();
    let router_for_drop = window_router.clone();
    let ui_for_drop = ui.as_weak();
    let native_file_drop_pointer = Rc::new(RefCell::new(NativeFileDropPointer::default()));
    let native_file_drop_pointer_for_event = native_file_drop_pointer.clone();
    ui.window().on_winit_window_event(move |_window, event| {
        match event {
            WindowEvent::DroppedFile(path) => {
                let Some(ui) = ui_for_drop.upgrade() else {
                    log_native_file_drop("dropped-ui-gone", 0, None);
                    return EventResult::Propagate;
                };
                let (target, hovered_file_count, upload_batch_id) = {
                    let mut pointer = native_file_drop_pointer_for_event.borrow_mut();
                    let hovered_file_count = pointer.hovered_file_count;
                    log_native_file_drop("dropped-received", hovered_file_count, None);
                    #[cfg(target_os = "macos")]
                    let logical_position = macos_file_drop_position(|| {
                        super::macos_window::current_cursor_position(ui.window()).ok()
                    });
                    #[cfg(not(target_os = "macos"))]
                    let logical_position =
                        pointer.logical_position(f64::from(ui.window().scale_factor()).max(0.01));
                    let target =
                        logical_position.map(|(x, y)| ui.invoke_native_sftp_drop_target_at(x, y));
                    let upload_batch_id = pointer.upload_batch_id();
                    pointer.complete_external_file_drop();
                    if target.is_none() {
                        log_native_file_drop("position-unavailable", hovered_file_count, None);
                    }
                    (target, hovered_file_count, upload_batch_id)
                };
                match target.as_deref() {
                    Some("remote") => {
                        log_native_file_drop("target-remote", hovered_file_count, Some("remote"));
                        super::sftp_bridge::handle_native_dropped_file_on_remote_pane(
                            &runtime_for_drop,
                            &state_for_drop,
                            &ui_for_drop,
                            &router_for_drop,
                            window_id,
                            path,
                            upload_batch_id,
                        );
                    }
                    Some(target) => {
                        log_native_file_drop("target-rejected", hovered_file_count, Some(target));
                    }
                    None => {}
                }
            }
            WindowEvent::HoveredFile(_) => {
                let hovered_file_count = {
                    let mut pointer = native_file_drop_pointer_for_event.borrow_mut();
                    pointer.begin_external_file_hover();
                    pointer.hovered_file_count
                };
                log_native_file_drop("hovered", hovered_file_count, None);
            }
            WindowEvent::HoveredFileCancelled => {
                native_file_drop_pointer_for_event.borrow_mut().clear();
                log_native_file_drop("hover-cancelled", 0, None);
            }
            #[cfg(not(target_os = "macos"))]
            WindowEvent::CursorMoved { position, .. } => {
                native_file_drop_pointer_for_event
                    .borrow_mut()
                    .record_cursor_position(position.x, position.y);
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
                native_file_drop_pointer_for_event.borrow_mut().clear();
            }
            #[cfg(target_os = "macos")]
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } if *is_synthetic || event.state != ElementState::Pressed => {
                return EventResult::Propagate;
            }
            #[cfg(target_os = "macos")]
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                let modifiers = modifiers_for_event.get();
                let mut physical_modifiers = TerminalModifiers {
                    alt: modifiers.alt_key(),
                    control: modifiers.control_key(),
                    meta: modifiers.super_key(),
                    shift: modifiers.shift_key(),
                };
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
                let Some(ui) = ui_for_native.upgrade() else {
                    return EventResult::Propagate;
                };
                if ui.get_active_tab_kind().as_str() != "terminal" {
                    return EventResult::Propagate;
                }
                if !physical_modifiers.control || physical_modifiers.meta {
                    return EventResult::Propagate;
                }
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
                let Some(input_event) =
                    normalized_keyboard_input_from_winit(event, physical_modifiers, *is_synthetic)
                else {
                    return EventResult::Propagate;
                };
                let Some(tab_id) = window_router.active_tab(window_id) else {
                    return EventResult::Propagate;
                };
                let input = TerminalInputContext {
                    ui: &ui_for_native,
                    state: &state,
                    window_router: &window_router,
                    window_id,
                };
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
            .and_then(|app| {
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
                    let Some(key) = super::input::terminal_key_from_normalized_input(&input) else {
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
                        worker
                            .request_send_kind(data, super::state::terminal::TerminalInputKind::Key)
                    };
                    worker_request_elapsed = Some(worker_request_started_at.elapsed());
                    request_result?;
                }
                Ok((true, false))
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

fn report_terminal_focus_state(
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
    worker.request_send_kind(data, super::state::terminal::TerminalInputKind::Focus)?;
    Ok(true)
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
    sync_terminal_query_palette(ui, &state);

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
        let repeat_count = input.repeat_count.clamp(1, MAX_MOUSE_WHEEL_REPORTS) as usize;
        let wheel = matches!(
            button,
            TerminalMouseButton::WheelUp
                | TerminalMouseButton::WheelDown
                | TerminalMouseButton::WheelLeft
                | TerminalMouseButton::WheelRight
        );
        if repeat_count > 1 && (kind != TerminalMouseEventKind::Press || !wheel) {
            debug!(%tab_id, "discarded invalid repeated terminal mouse event");
            return;
        }
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
                let terminal = app.terminal_mut(tab_id).context("terminal tab not found")?;
                if !terminal.connected {
                    return Ok(true);
                }
                let model = terminal
                    .terminal
                    .as_ref()
                    .context("active tab has no terminal model")?;
                let Some(data) = model.encode_mouse_event_with_pixels(
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
                ) else {
                    return Ok(true);
                };
                let data = if repeat_count == 1 {
                    data
                } else {
                    let mut repeated = Vec::with_capacity(data.len() * repeat_count);
                    for _ in 0..repeat_count {
                        repeated.extend_from_slice(&data);
                    }
                    repeated
                };
                let worker = terminal
                    .worker
                    .as_ref()
                    .context("active terminal has no worker")?;
                if kind == TerminalMouseEventKind::Motion {
                    worker.request_send_motion_kind(
                        data,
                        super::state::terminal::TerminalInputKind::Pointer,
                    )
                } else {
                    worker
                        .request_send_kind(data, super::state::terminal::TerminalInputKind::Pointer)
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
        if command.as_str() == "allow-osc52-clipboard-read" {
            return handle_osc52_clipboard_read(
                &router_for_command,
                window_id,
                tab_id,
                &state_for_command,
                &ui_for_command,
            );
        }
        if command.as_str() == "deny-osc52-clipboard-read" {
            return handle_osc52_clipboard_deny(
                &router_for_command,
                window_id,
                tab_id,
                &state_for_command,
                &ui_for_command,
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

fn terminal_semantic_selection(range: TerminalSelectionRange) -> TerminalSemanticSelection {
    TerminalSemanticSelection {
        active: true,
        anchor_row: range.start_row as i32,
        anchor_column: range.start_column as i32,
        focus_row: range.end_row as i32,
        focus_column: range.end_column as i32,
    }
}

fn terminal_url_selection_range(
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
    let terminal_model = terminal.terminal.as_ref()?;
    if let Some((uri, _, _)) = terminal_model.hyperlink_at_cell(row, column) {
        return Some(TerminalTarget::Url(uri));
    }
    let context = terminal_model.visible_logical_line_target_context_at_cell(row, column)?;
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
                LocalShellEvent::Output(output) => {
                    let data = &output.data;
                    super::diagnostics::log_terminal_output_chunk(
                        "local",
                        data.len(),
                        output.received_at,
                    );
                    let mut response_error = None;
                    let mut presentation_hold = None;
                    let mut output_effects = TerminalOutputEffects::default();
                    if mutate_local_terminal(&state, tab_id, |terminal| {
                        match process_terminal_output(terminal, data) {
                            Ok(effects) => {
                                presentation_hold = effects.presentation_hold;
                                output_effects = effects;
                            }
                            Err(error) => response_error = Some(error),
                        }
                    })
                    .is_some()
                        && !data.is_empty()
                    {
                        presentation.record_output(Some(output.received_at), presentation_hold);
                    }
                    apply_terminal_output_effects(&state, &ui, tab_id, output_effects);
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
) -> Result<TerminalOutputEffects> {
    let (responses, presentation_hold, title_update, bell, clipboard_store, clipboard_formatter) = {
        let model = terminal
            .terminal
            .as_mut()
            .context("terminal tab has no terminal model")?;
        (
            model.process_with_responses(data),
            model.synchronized_output_remaining(),
            model.take_title_update(),
            model.take_bell(),
            model.take_clipboard_store(),
            model.take_clipboard_load(),
        )
    };
    let clipboard_read_requested = clipboard_formatter
        .and_then(|formatter| terminal.offer_clipboard_read(formatter))
        .is_some();
    if responses.is_empty() {
        return Ok(TerminalOutputEffects {
            presentation_hold,
            title_update,
            bell,
            clipboard_store,
            clipboard_read_requested,
        });
    }
    let worker = terminal
        .worker
        .as_ref()
        .context("terminal protocol response has no transport worker")?;
    for response in responses {
        worker
            .request_send_kind(
                response,
                super::state::terminal::TerminalInputKind::Protocol,
            )
            .context("cannot queue terminal protocol response")?;
    }
    Ok(TerminalOutputEffects {
        presentation_hold,
        title_update,
        bell,
        clipboard_store,
        clipboard_read_requested,
    })
}

#[derive(Default)]
pub(super) struct TerminalOutputEffects {
    pub(super) presentation_hold: Option<Duration>,
    pub(super) title_update: Option<Option<String>>,
    pub(super) bell: bool,
    pub(super) clipboard_store: Option<String>,
    pub(super) clipboard_read_requested: bool,
}

pub(super) fn apply_terminal_output_effects(
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    tab_id: Uuid,
    effects: TerminalOutputEffects,
) {
    if effects.title_update.is_none()
        && !effects.bell
        && effects.clipboard_store.is_none()
        && !effects.clipboard_read_requested
    {
        return;
    }
    if let Some(text) = effects.clipboard_store {
        dispatch_ui(ui, move |ui| set_platform_clipboard_text(ui, &text));
    }
    let title_changed = match state.lock() {
        Ok(mut app) => effects
            .title_update
            .map(|title| app.apply_terminal_title(tab_id, title))
            .unwrap_or(false),
        Err(_) => false,
    };
    if effects.clipboard_read_requested {
        let request = state.lock().ok().and_then(|app| {
            app.terminal(tab_id)
                .and_then(TerminalTabState::clipboard_read_key)
        });
        if let Some((token, generation)) = request {
            refresh_workspace(ui, state);
            schedule_osc52_clipboard_read_timeout(
                state.clone(),
                ui.clone(),
                tab_id,
                token,
                generation,
            );
        }
    }
    if title_changed || effects.bell {
        refresh_workspace(ui, state);
    }
}

fn schedule_osc52_clipboard_read_timeout(
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    token: u64,
    generation: u64,
) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    runtime.spawn(async move {
        tokio::time::sleep(OSC52_CLIPBOARD_READ_TIMEOUT).await;
        let cleared = state.lock().ok().is_some_and(|mut app| {
            app.terminal_mut(tab_id)
                .is_some_and(|terminal| terminal.clear_clipboard_read(token, generation))
        });
        if cleared {
            refresh_workspace(&ui, &state);
        }
    });
}

fn handle_osc52_clipboard_read(
    router: &WindowRouter,
    window_id: Uuid,
    tab_id: Uuid,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
) -> bool {
    let Some(ui) = ui.upgrade() else {
        return false;
    };
    let Some((token, generation)) = state.lock().ok().and_then(|app| {
        if !router.owns_terminal_pane(window_id, tab_id, &app) {
            return None;
        }
        app.terminal(tab_id)
            .and_then(TerminalTabState::clipboard_read_key)
    }) else {
        return false;
    };

    let text = platform_clipboard_text(&ui);
    let sent = {
        let Ok(mut app) = state.lock() else {
            return false;
        };
        if !router.owns_terminal_pane(window_id, tab_id, &app) {
            return false;
        }
        let Some(terminal) = app.terminal_mut(tab_id) else {
            return false;
        };
        if terminal.clipboard_read_key() != Some((token, generation)) {
            return false;
        }
        let Some(formatter) = terminal.take_pending_clipboard_read() else {
            return false;
        };
        let Some(worker) = terminal.worker.as_ref() else {
            return false;
        };
        worker.request_send(formatter(&text).into_bytes()).is_ok()
    };
    refresh_workspace(&ui.as_weak(), state);
    sent
}

fn handle_osc52_clipboard_deny(
    router: &WindowRouter,
    window_id: Uuid,
    tab_id: Uuid,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
) -> bool {
    let cleared = state.lock().ok().is_some_and(|mut app| {
        if !router.owns_terminal_pane(window_id, tab_id, &app) {
            return false;
        }
        app.terminal_mut(tab_id)
            .is_some_and(TerminalTabState::clear_pending_clipboard_read)
    });
    if cleared {
        refresh_workspace(ui, state);
    }
    cleared
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
            terminal.clear_pending_clipboard_read();
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
    fn semantic_selection_prefers_complete_url_and_omits_trailing_punctuation() {
        let mut terminal = TerminalModel::new(80, 3, 10);
        terminal.process(b"See https://example.test/releases/v1.2).\r\n");

        let range = terminal_url_selection_range(&terminal, 0, 12)
            .expect("URL should provide a bounded selection range");
        assert_eq!(
            terminal.selection_text(
                range.start_row,
                range.start_column,
                range.end_row,
                range.end_column,
            ),
            "https://example.test/releases/v1.2"
        );
    }

    #[test]
    fn semantic_selection_keeps_soft_wrapped_url_in_one_range() {
        let mut terminal = TerminalModel::new(24, 3, 10);
        terminal.process(b"https://example.test/very-long/path?q=1");

        let range = terminal_url_selection_range(&terminal, 1, 2)
            .expect("soft-wrapped URL should provide a bounded selection range");
        assert_eq!(
            terminal.selection_text(
                range.start_row,
                range.start_column,
                range.end_row,
                range.end_column,
            ),
            "https://example.test/very-long/path?q=1"
        );
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
    fn consecutive_native_file_events_share_one_upload_batch() {
        let mut pointer = NativeFileDropPointer::default();
        let first = pointer.upload_batch_id();
        pointer.complete_external_file_drop();
        assert_eq!(pointer.upload_batch_id(), first);
        pointer.clear();
        assert_ne!(pointer.upload_batch_id(), first);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn native_file_drop_requires_current_external_hover_and_coordinates() {
        let mut pointer = NativeFileDropPointer::default();
        pointer.record_cursor_position(80.0, 48.0);
        assert_eq!(pointer.logical_position(2.0), None);

        pointer.begin_external_file_hover();
        assert_eq!(pointer.logical_position(2.0), None);
        pointer.record_cursor_position(80.0, 48.0);
        assert_eq!(pointer.logical_position(2.0), Some((40.0, 24.0)));
        assert_eq!(pointer.logical_position(0.0), None);

        pointer.begin_external_file_hover();
        pointer.complete_external_file_drop();
        assert_eq!(pointer.logical_position(2.0), Some((40.0, 24.0)));
        pointer.complete_external_file_drop();
        assert_eq!(pointer.logical_position(2.0), None);

        pointer.begin_external_file_hover();
        pointer.clear();
        assert_eq!(pointer.logical_position(2.0), None);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn native_file_drop_rejects_non_finite_pointer_coordinates() {
        let mut pointer = NativeFileDropPointer::default();
        pointer.begin_external_file_hover();
        pointer.record_cursor_position(f64::NAN, 12.0);
        assert_eq!(pointer.logical_position(1.0), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_file_drop_uses_appkit_position_without_hover_state() {
        assert_eq!(macos_file_drop_position(|| None), None);
        assert_eq!(
            macos_file_drop_position(|| Some((30.0, 20.0))),
            Some((30.0, 20.0))
        );
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
