//! Slint application controller.
//!
//! This module is the only place that knows about generated Slint types. It
//! maps user intent to domain operations and returns owned worker events to the
//! Slint event loop.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use slint::platform::Clipboard;
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};
use tokio::runtime::{Handle, Runtime};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Duration;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use ax_ssh::config::{
    AppSettings, AuthMethod, ConfigStore, CredentialStorage, SessionProfile, SessionStore,
    ShortcutSettings, TerminalColorScheme, ThemePalette, ThemeSettings, normalize_group_name,
};
use ax_ssh::local_shell::{LocalShellEvent, LocalShellHandle, discover_shells};
use ax_ssh::ssh::{SshSessionEvent, SshSessionHandle, discover_private_keys, probe_host_key};
use ax_ssh::terminal::{TerminalSnapshot, encode_key as encode_terminal_key};

use self::credential_tasks::{
    delete_password, load_system_password, load_vault_password, save_password,
};
use self::input::{
    format_shortcut_event_with_current_modifiers, terminal_input_modifiers,
    terminal_key_from_slint, terminal_key_is_control_chord, terminal_key_is_direct,
};
use self::session_groups::{
    compact_label, group_options, profile_endpoint, profile_sidebar_endpoint, session_groups,
};
use self::state::{
    ActiveSecurityPrompt, ActiveTabSnapshot, AppState, ConnectionStart, PendingHostKey,
    PendingProbe, SshConnectionPhase, TerminalTabState, TerminalWorker, WorkspaceTabSummary,
    finish_stored_credential_retry, prepare_authentication_retry, prepare_host_key_retry,
    prepare_stored_credential_retry, retire_session_attempt, session_attempt_is_active,
    set_credential_storage, set_credential_storage_while_loading,
};
use self::terminal_render::{
    RenderedTerminalLine, RenderedTerminalRun, RgbColor, TerminalRenderSettings, render_terminal,
};

mod connection;
mod connection_monitor;
mod credential_tasks;
mod input;
#[cfg(target_os = "macos")]
mod macos_window;
mod session_groups;
mod settings_bridge;
mod state;
mod terminal_bridge;
mod terminal_render;
mod view;
mod workspace;

use self::connection::*;
use self::connection_monitor::*;
use self::settings_bridge::*;
use self::terminal_bridge::*;
use self::view::*;
use self::workspace::*;

slint::include_modules!();

pub fn run() -> Result<()> {
    let config_path = ConfigStore::default_path()?;
    let config = ConfigStore::new(config_path);
    let mut sessions = config.load().context("failed to load session profiles")?;
    if sessions
        .settings
        .terminal
        .merge_known_shells(discover_shells())
        && let Err(error) = config.save(&sessions)
    {
        warn!(%error, "failed to persist newly discovered local shells");
    }
    let runtime = Runtime::new().context("failed to start Tokio runtime")?;
    let state = Arc::new(Mutex::new(AppState::new(config, sessions)));
    let ui = AppWindow::new().context("failed to create Slint window")?;

    let (rows, groups, connection_options, settings) = {
        let app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        (
            session_group_rows(&app.sessions),
            group_option_rows(&app.sessions),
            connection_option_rows(&app.sessions),
            app.sessions.settings.clone(),
        )
    };
    ui.set_sessions(ModelRc::new(VecModel::from(rows)));
    ui.set_group_options(ModelRc::new(VecModel::from(groups)));
    ui.set_connection_options(ModelRc::new(VecModel::from(connection_options)));
    ui.set_private_key_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_terminal_render_lines(ModelRc::new(VecModel::from(
        Vec::<TerminalRenderLine>::new(),
    )));
    ui.set_local_shell_options(ModelRc::new(VecModel::from(shell_option_rows(&settings))));
    let default_shortcuts = ShortcutSettings::default();
    ui.set_default_open_settings_shortcut(default_shortcuts.open_settings.into());
    ui.set_default_toggle_sidebar_shortcut(default_shortcuts.toggle_sidebar.into());
    ui.set_default_copy_selection_shortcut(default_shortcuts.copy_selection.into());
    ui.set_default_paste_shortcut(default_shortcuts.paste.into());
    ui.set_apple_platform(cfg!(target_os = "macos"));
    ui.set_app_version(env!("CARGO_PKG_VERSION").into());
    apply_settings_to_component(&ui, &settings);
    apply_active_snapshot(&ui, ActiveTabSnapshot::default());
    ui.set_workspace_tabs(ModelRc::new(VecModel::from(Vec::<WorkspaceTabRow>::new())));
    ui.set_status("".into());
    wire_callbacks(&ui, state.clone(), runtime.handle().clone());
    load_private_key_options(runtime.handle(), ui.as_weak());
    #[cfg(target_os = "macos")]
    {
        ui.show().context("failed to create macOS window")?;
        let ui_for_window = ui.as_weak();
        slint::Timer::single_shot(Duration::from_millis(100), move || {
            let Some(ui) = ui_for_window.upgrade() else {
                return;
            };
            if let Err(error) = macos_window::configure(ui.window()) {
                warn!(%error, "failed to configure the standard macOS title bar");
            }
            configure_macos_application_menu(&ui);
        });
    }
    info!("AxSSH UI initialized");
    let ui_result = ui.run().context("Slint event loop failed");

    let (workers, pending_probes) = {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned during shutdown"))?;
        app.drain_runtime_resources()
    };
    for pending_probe in pending_probes {
        if pending_probe.cancel.send(()).is_err() {
            debug!(
                tab_id = %pending_probe.tab_id,
                session_id = %pending_probe.profile_id,
                "host-key probe already stopped during shutdown"
            );
        }
    }
    for worker in workers {
        if let Err(error) = runtime.block_on(worker.shutdown()) {
            warn!(%error, "failed to shut down terminal worker cleanly");
        }
    }

    drop(ui);
    runtime.shutdown_timeout(Duration::from_secs(3));
    ui_result?;
    info!("AxSSH UI stopped");
    Ok(())
}

fn wire_callbacks(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    ui.on_format_shortcut(move |text, alt, control, meta, shift| {
        format_shortcut_event_with_current_modifiers(text.as_str(), alt, control, meta, shift)
            .into()
    });
    ui.on_terminal_key_direct(
        move |text, alt, control, meta, shift, option_as_meta, preedit_active| {
            terminal_key_is_direct(
                text.as_str(),
                alt,
                control,
                meta,
                shift,
                option_as_meta,
                preedit_active,
            )
        },
    );
    ui.on_terminal_key_is_control(move |alt, control, meta, shift| {
        terminal_key_is_control_chord(alt, control, meta, shift)
    });
    let ui_for_clipboard_write = ui.as_weak();
    ui.on_write_clipboard(move |text| {
        if let Some(ui) = ui_for_clipboard_write.upgrade() {
            set_platform_clipboard_text(&ui, text.as_str());
        }
    });
    let ui_for_clipboard_read = ui.as_weak();
    ui.on_read_clipboard(move || {
        ui_for_clipboard_read
            .upgrade()
            .map(|ui| platform_clipboard_text(&ui))
            .unwrap_or_default()
            .into()
    });
    wire_workspace_tabs(ui, state.clone(), runtime.clone());
    wire_session_editor(ui, state.clone(), runtime.clone());
    wire_session_management(ui, state.clone(), runtime.clone());
    wire_connection_request(ui, state.clone(), runtime.clone());
    wire_host_key_confirmation(ui, state.clone(), runtime.clone());
    wire_authentication(ui, state.clone(), runtime.clone());
    wire_settings(ui, state.clone(), runtime);
    wire_terminal(ui, state);
}

fn set_platform_clipboard_text(ui: &AppWindow, text: &str) {
    // Slint 1.17 exposes clipboard access through its window context.
    slint::private_unstable_api::re_exports::WindowInner::from_pub(ui.window())
        .context()
        .platform()
        .set_clipboard_text(text, Clipboard::DefaultClipboard);
}

fn platform_clipboard_text(ui: &AppWindow) -> String {
    slint::private_unstable_api::re_exports::WindowInner::from_pub(ui.window())
        .context()
        .platform()
        .clipboard_text(Clipboard::DefaultClipboard)
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn configure_macos_application_menu(ui: &AppWindow) {
    let ui_for_menu = ui.as_weak();
    if let Err(error) = macos_window::configure_application_menu(move |section| {
        let Some(ui) = ui_for_menu.upgrade() else {
            return;
        };
        let section = match section {
            macos_window::NativeMenuSection::Settings => "General",
            macos_window::NativeMenuSection::About => "About",
        };
        ui.invoke_request_settings_section(section.into());
        ui.invoke_open_settings();
    }) {
        warn!(%error, "failed to connect the standard macOS application menu");
    }
}
