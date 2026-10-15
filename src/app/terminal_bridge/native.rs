//! Terminal native application boundary.

use super::*;

/// A `DroppedFile` has no location. macOS, Windows, and X11 query their native
/// cursor position at drop time; platforms without a synchronous cursor API
/// use the latest Winit cursor move from the current external-file hover.
/// Every route resolves the declared Remote files target before it creates an
/// upload.
#[derive(Default)]
pub(super) struct NativeFileDropPointer {
    pub(super) hovered_file_count: u16,
    pub(super) upload_batch: Option<(Uuid, Instant)>,
    #[cfg(not(target_os = "macos"))]
    pub(super) last_physical_position: Option<(f64, f64)>,
}

impl NativeFileDropPointer {
    pub(super) fn begin_external_file_hover(&mut self) {
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

    pub(super) fn complete_external_file_drop(&mut self) {
        self.hovered_file_count = self.hovered_file_count.saturating_sub(1);
        #[cfg(not(target_os = "macos"))]
        if self.hovered_file_count == 0 {
            self.last_physical_position = None;
        }
    }

    pub(super) fn upload_batch_id(&mut self) -> Uuid {
        let now = Instant::now();
        let id = self
            .upload_batch
            .filter(|(_, last)| now.duration_since(*last) <= Duration::from_millis(500))
            .map_or_else(Uuid::new_v4, |(id, _)| id);
        self.upload_batch = Some((id, now));
        id
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn record_cursor_position(&mut self, x: f64, y: f64) {
        if x.is_finite() && y.is_finite() {
            self.last_physical_position = Some((x, y));
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn clear(&mut self) {
        self.hovered_file_count = 0;
        self.upload_batch = None;
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn clear(&mut self) {
        self.hovered_file_count = 0;
        self.upload_batch = None;
        self.last_physical_position = None;
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn logical_position(&self, scale_factor: f64) -> Option<(f32, f32)> {
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
pub(super) fn macos_file_drop_position(
    read_current_position: impl FnOnce() -> Option<(f32, f32)>,
) -> Option<(f32, f32)> {
    read_current_position()
}

pub(super) fn log_native_file_drop(
    stage: &'static str,
    hovered_file_count: u16,
    target: Option<&str>,
) {
    tracing::debug!(
        target: "ax_ssh::sftp_drag",
        event = "native-file-drop",
        stage,
        hovered_file_count,
        target = target.unwrap_or("not-resolved"),
        "SFTP native file drop route"
    );
}

/// Register after a Slint window is shown, when its Winit adapter exists.
///
/// Native modifier snapshots are recorded for every platform before Slint
/// dispatches the corresponding key event. Normal text, IME input, and
/// physical keypad input continue through Slint's standard keyboard path.
pub(in crate::app) fn install_native_window_input_hook(
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
        if matches!(
            event,
            WindowEvent::Moved(_)
                | WindowEvent::Resized(_)
                | WindowEvent::CloseRequested
                | WindowEvent::ScaleFactorChanged { .. }
        ) {
            _window.with_winit_window(|window| window_router.capture_placement(window_id, window));
        }
        match event {
            WindowEvent::MouseWheel { delta, phase, .. } => {
                let modifiers = modifiers_for_event.get();
                tracing::debug!(
                    target: "ax_ssh::terminal_scroll",
                    stage = "native-wheel",
                    %window_id,
                    delta = ?delta,
                    phase = ?phase,
                    scale_factor = _window.scale_factor(),
                    shift = modifiers.shift_key(),
                    alt = modifiers.alt_key(),
                    control = modifiers.control_key(),
                    meta = modifiers.super_key(),
                    "native wheel event received"
                );
            }
            #[cfg(target_os = "macos")]
            WindowEvent::MouseInput {
                state,
                button: slint::winit_030::winit::event::MouseButton::Left,
                ..
            } => {
                if *state == ElementState::Pressed {
                    if let Some(ui) = ui_for_native.upgrade()
                        && super::super::macos_file_drag::capture_file_drag_mouse_down(
                            ui.window(),
                            window_id,
                        )
                        .is_err()
                    {
                        tracing::debug!(target: "ax_ssh::sftp_drag", stage = "mouse-down-rejected",
                            "SFTP native drag mouse-down capture rejected");
                    }
                } else {
                    // Clear before Slint flushes the last coalesced move. That
                    // move must not start an AppKit drag after the physical up.
                    super::super::macos_file_drag::clear_file_drag_mouse_down(window_id);
                }
            }
            #[cfg(target_os = "macos")]
            WindowEvent::CloseRequested => {
                super::super::macos_file_drag::clear_file_drag_mouse_down(window_id);
            }
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
                        crate::app::macos_window::current_cursor_position(ui.window()).ok()
                    });
                    #[cfg(not(target_os = "macos"))]
                    let logical_position = {
                        let scale_factor = f64::from(ui.window().scale_factor()).max(0.01);
                        let hover_position = pointer.logical_position(scale_factor);
                        _window
                            .with_winit_window(|window| {
                                crate::app::native_file_drop::logical_position(
                                    window,
                                    scale_factor,
                                    hover_position,
                                )
                            })
                            .unwrap_or(hover_position)
                    };
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
                        crate::app::sftp_bridge::handle_native_dropped_file_on_remote_pane(
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
                #[cfg(target_os = "macos")]
                super::super::macos_file_drag::clear_file_drag_mouse_down(window_id);
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
                    let current = crate::app::macos_window::current_modifier_state();
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
