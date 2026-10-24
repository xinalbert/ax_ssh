//! Terminal application facade and callback composition.

mod clipboard;
mod geometry;
mod input;
mod local_shell;
mod native;
mod output;
mod panes;
mod targets;

pub(super) use self::local_shell::{resume_existing_local_shell, start_local_shell};
pub(super) use self::native::install_native_window_input_hook;
pub(super) use self::output::{
    TerminalOutputEffects, apply_terminal_output_effects, process_terminal_output,
};

#[cfg(test)]
pub(in crate::app) use self::clipboard::handle_osc52_clipboard_read_with;
use self::clipboard::*;
#[cfg(test)]
use self::geometry::*;
use self::input::*;
#[cfg(all(test, not(windows)))]
use self::local_shell::{finish_local_terminal, spawn_local_shell_monitor};
#[cfg(test)]
use self::native::*;
#[cfg(test)]
use self::targets::*;

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
    geometry::wire_geometry(ui, window_id);
    input::wire_input(ui, state.clone(), window_router.clone(), window_id);
    targets::wire_targets(
        ui,
        state.clone(),
        runtime.clone(),
        font_registry.clone(),
        terminal_font_started.clone(),
        window_router.clone(),
        window_id,
    );
    panes::wire_panes(
        ui,
        state,
        runtime,
        font_registry,
        terminal_font_started,
        window_router,
        window_id,
    );
}

#[cfg(test)]
mod tests;
