//! Slint application controller.
//!
//! This module is the only place that knows about generated Slint types. It
//! maps user intent to domain operations and returns owned worker events to the
//! Slint event loop.

use std::path::{Path, PathBuf};
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
    AppSettings, AuthMethod, ConfigStore, ConnectionProfile, CredentialStorage, MAX_HOST_CHARS,
    MAX_PRIVATE_KEY_PATH_CHARS, MAX_SESSION_NAME_CHARS, MAX_USERNAME_CHARS, SerialDataBits,
    SerialFlowControl, SerialParity, SerialStopBits, SessionProfile, SessionStore,
    ShortcutSettings, TerminalColorScheme, ThemePalette, ThemeSettings, X11Settings,
    normalize_group_name,
};
use ax_ssh::local_shell::{LocalShellEvent, LocalShellHandle, discover_shells};
use ax_ssh::serial::{
    SerialPortDescriptor, SerialSessionEvent, SerialSessionHandle, discover_serial_ports,
    resolve_serial_port,
};
use ax_ssh::sftp::{SftpBrowserEvent, SftpEntry};
use ax_ssh::ssh::{SshSessionEvent, SshSessionHandle, discover_private_keys, probe_host_key};
use ax_ssh::telnet::{TelnetSessionEvent, TelnetSessionHandle};
use ax_ssh::terminal::{TerminalSnapshot, encode_key as encode_terminal_key};

use self::credential_tasks::{
    delete_password, load_system_password, load_vault_password, save_password,
};
use self::input::{
    format_shortcut_event_with_current_modifiers, menu_shortcut_from_setting,
    terminal_input_modifiers, terminal_key_from_slint, terminal_key_is_direct,
};
use self::session_groups::{
    compact_label, group_options, profile_endpoint, profile_sidebar_details,
    profile_sidebar_endpoint, session_groups,
};
use self::state::{
    ActiveSecurityPrompt, ActiveTabSnapshot, AppState, ConnectionStart, ConnectionTarget,
    PendingHostKey, PendingProbe, SftpBrowserSnapshot, SftpNavigation, SshConnectionPhase,
    SshSftpNavigation, TerminalTabState, TerminalWorker, WorkspaceTabSummary,
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
mod diagnostics;
mod font_bridge;
mod input;
mod local_files;
#[cfg(target_os = "macos")]
mod macos_window;
mod serial_bridge;
mod session_groups;
mod settings_bridge;
mod sftp_bridge;
mod state;
mod terminal_bridge;
mod terminal_render;
mod view;
mod workspace;

use self::connection::*;
use self::connection_monitor::*;
use self::diagnostics::*;
use self::font_bridge::*;
use self::serial_bridge::*;
use self::settings_bridge::*;
use self::sftp_bridge::*;
use self::terminal_bridge::*;
use self::view::*;
use self::workspace::*;

slint::include_modules!();

const ISSUES_URL: &str = "https://github.com/xinalbert/ax_ssh/issues/new";

pub fn run(log_directory: PathBuf) -> Result<()> {
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
    let initial_font_families = vec![
        sessions.settings.appearance.application_font_family.clone(),
        sessions.settings.appearance.terminal_font_family.clone(),
    ];
    let font_registry = Arc::new(Mutex::new(FontRegistry::new()));
    let initial_fonts =
        load_startup_bundled_fonts(runtime.handle(), &font_registry, initial_font_families);
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
    ui.set_serial_port_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_terminal_render_lines(ModelRc::new(VecModel::from(
        Vec::<TerminalRenderLine>::new(),
    )));
    ui.set_sftp_entries(ModelRc::new(VecModel::from(Vec::<SftpEntryRow>::new())));
    ui.set_local_shell_options(ModelRc::new(VecModel::from(shell_option_rows(&settings))));
    ui.set_x11_server_provider_options(ModelRc::new(VecModel::from(
        ax_ssh::x_server::provider_options()
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
    ui.set_x11_server_installations(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_application_font_options(ModelRc::new(VecModel::from(font_option_rows(
        &settings.appearance.application_font_family,
        &[],
    ))));
    ui.set_terminal_font_options(ModelRc::new(VecModel::from(font_option_rows(
        &settings.appearance.terminal_font_family,
        &[],
    ))));
    let default_shortcuts = ShortcutSettings::default();
    ui.set_default_open_settings_shortcut(default_shortcuts.open_settings.into());
    ui.set_default_new_session_shortcut(default_shortcuts.new_session.into());
    ui.set_default_toggle_sidebar_shortcut(default_shortcuts.toggle_sidebar.into());
    ui.set_default_copy_selection_shortcut(default_shortcuts.copy_selection.into());
    ui.set_default_paste_shortcut(default_shortcuts.paste.into());
    ui.set_default_open_sftp_shortcut(default_shortcuts.open_sftp.into());
    ui.set_apple_platform(cfg!(target_os = "macos"));
    ui.set_app_version(format!("{} ({})", env!("CARGO_PKG_VERSION"), build_revision()).into());
    for initial_font in initial_fonts {
        if let Err(error) = font_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("font registry lock poisoned"))?
            .register_loaded_font(initial_font)
        {
            warn!(%error, "failed to register a configured bundled font");
        }
    }
    apply_settings_to_component(&ui, &settings);
    apply_active_snapshot(&ui, ActiveTabSnapshot::default());
    ui.set_workspace_tabs(ModelRc::new(VecModel::from(Vec::<WorkspaceTabRow>::new())));
    ui.set_status("".into());
    wire_callbacks(
        &ui,
        state.clone(),
        runtime.handle().clone(),
        font_registry,
        log_directory,
    );
    load_private_key_options(runtime.handle(), ui.as_weak());
    load_font_options(runtime.handle(), ui.as_weak());
    load_x11_server_installations(runtime.handle(), ui.as_weak());
    #[cfg(target_os = "macos")]
    {
        ui.show().context("failed to create macOS window")?;
        let ui_for_window = ui.as_weak();
        slint::Timer::single_shot(Duration::from_millis(100), move || {
            let Some(ui) = ui_for_window.upgrade() else {
                return;
            };
            if let Err(error) = macos_window::configure_application_icon() {
                warn!(%error, "failed to configure the macOS application icon");
            }
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

fn load_startup_bundled_fonts(
    runtime: &Handle,
    font_registry: &Arc<Mutex<FontRegistry>>,
    selected_families: Vec<String>,
) -> Vec<LoadedBundledFont> {
    let resources = match font_registry.lock() {
        Ok(registry) => registry.resources(),
        Err(_) => {
            warn!("cannot access font resources during startup");
            return Vec::new();
        }
    };
    match runtime.block_on(async move {
        tokio::task::spawn_blocking(move || resources.load_bundled_fonts(&selected_families)).await
    }) {
        Ok(Ok(fonts)) => fonts,
        Ok(Err(error)) => {
            warn!(%error, "failed to read bundled font resources during startup");
            Vec::new()
        }
        Err(error) => {
            warn!(%error, "bundled font task failed during startup");
            Vec::new()
        }
    }
}

fn wire_callbacks(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    font_registry: Arc<Mutex<FontRegistry>>,
    log_directory: PathBuf,
) {
    ui.on_log_keyboard_event(move |text, alt, control, meta, shift, route, action| {
        log_keyboard_event(
            text.as_str(),
            alt,
            control,
            meta,
            shift,
            route.as_str(),
            action.as_str(),
        );
    });
    ui.on_menu_action(move |action| log_menu_action(action.as_str()));
    #[cfg(target_os = "macos")]
    {
        let ui_for_menu_state = ui.as_weak();
        ui.on_menu_state_changed(move || {
            if let Some(ui) = ui_for_menu_state.upgrade() {
                schedule_macos_application_menu_configuration(&ui);
            }
        });
    }
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
    let ui_for_clipboard_write = ui.as_weak();
    ui.on_write_clipboard(move |text| {
        log_ui_action("clipboard.write");
        if let Some(ui) = ui_for_clipboard_write.upgrade() {
            set_platform_clipboard_text(&ui, text.as_str());
        }
    });
    let ui_for_clipboard_read = ui.as_weak();
    ui.on_read_clipboard(move || {
        log_ui_action("clipboard.read");
        ui_for_clipboard_read
            .upgrade()
            .map(|ui| platform_clipboard_text(&ui))
            .unwrap_or_default()
            .into()
    });
    let ui_for_report_bug = ui.as_weak();
    ui.on_report_bug(move || {
        log_ui_action("about.report-bug");
        open_external_target(
            &ui_for_report_bug,
            ISSUES_URL,
            "Cannot open bug report page",
        );
    });
    let ui_for_open_logs = ui.as_weak();
    let log_directory_for_callback = log_directory.clone();
    ui.on_open_log_directory(move || {
        log_ui_action("about.open-log-directory");
        open_external_path(
            &ui_for_open_logs,
            &log_directory_for_callback,
            "Cannot open log directory",
        );
    });
    let ui_for_diagnostics = ui.as_weak();
    ui.on_copy_diagnostic_info(move || {
        log_ui_action("about.copy-diagnostic-info");
        let Some(ui) = ui_for_diagnostics.upgrade() else {
            return;
        };
        let diagnostics = diagnostic_info();
        set_platform_clipboard_text(&ui, &diagnostics);
        ui.set_status("Diagnostic information copied".into());
    });
    let ui_for_copy_session = ui.as_weak();
    let state_for_copy_session = state.clone();
    ui.on_copy_session(move |id| {
        log_ui_action("session.copy-address");
        let session_id = match Uuid::parse_str(id.as_str()) {
            Ok(session_id) => session_id,
            Err(_) => {
                set_status(&ui_for_copy_session, "Session not found");
                return;
            }
        };
        let endpoint = match state_for_copy_session.lock() {
            Ok(app) => app
                .sessions
                .sessions
                .iter()
                .find(|profile| profile.id == session_id)
                .map(profile_endpoint),
            Err(_) => {
                set_status(&ui_for_copy_session, "Cannot read session state");
                return;
            }
        };
        let Some(endpoint) = endpoint else {
            set_status(&ui_for_copy_session, "Session not found");
            return;
        };
        if let Some(ui) = ui_for_copy_session.upgrade() {
            set_platform_clipboard_text(&ui, &endpoint);
            set_status(&ui_for_copy_session, "Address copied");
        }
    });
    wire_workspace_tabs(ui, state.clone(), runtime.clone());
    wire_session_editor(ui, state.clone(), runtime.clone());
    wire_serial_port_discovery(ui, state.clone(), runtime.clone());
    wire_session_management(ui, state.clone(), runtime.clone());
    wire_connection_request(ui, state.clone(), runtime.clone());
    wire_host_key_confirmation(ui, state.clone(), runtime.clone());
    wire_authentication(ui, state.clone(), runtime.clone());
    wire_settings(ui, state.clone(), runtime.clone(), font_registry);
    wire_sftp(ui, state.clone(), runtime.clone());
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

fn build_revision() -> &'static str {
    option_env!("AXSSH_BUILD_REVISION").unwrap_or("unknown")
}

fn diagnostic_info() -> String {
    format!(
        "AxSSH diagnostics\nversion: {}\nbuild-revision: {}\nos: {}\narch: {}\nprofile: {}\n",
        env!("CARGO_PKG_VERSION"),
        build_revision(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
    )
}

fn open_external_target(ui: &slint::Weak<AppWindow>, target: &str, failure_message: &str) {
    if open::that_detached(target).is_err() {
        tracing::warn!(target: "ax_ssh::diagnostics", operation = "open-external-target", "failed to open support target");
        set_status(ui, failure_message);
    }
}

fn open_external_path(ui: &slint::Weak<AppWindow>, path: &Path, failure_message: &str) {
    if open::that_detached(path).is_err() {
        tracing::warn!(target: "ax_ssh::diagnostics", operation = "open-log-directory", "failed to open log directory");
        set_status(ui, failure_message);
    }
}

#[cfg(test)]
mod support_tests {
    use super::*;

    #[test]
    fn copied_diagnostics_are_build_metadata_only() {
        let diagnostics = diagnostic_info();

        assert!(diagnostics.contains("version: "));
        assert!(diagnostics.contains("build-revision: "));
        assert!(diagnostics.contains("os: "));
        assert!(diagnostics.contains("arch: "));
        assert!(diagnostics.contains("profile: "));
        for forbidden in ["host:", "password", "session-id", "sessions.json"] {
            assert!(!diagnostics.contains(forbidden));
        }
    }
}

#[cfg(target_os = "macos")]
fn configure_macos_application_menu(ui: &AppWindow) {
    let shortcut = match menu_shortcut_from_setting(ui.get_open_settings_shortcut().as_str()) {
        Ok(shortcut) => shortcut,
        Err(error) => {
            warn!(%error, "cannot configure the macOS Settings shortcut");
            return;
        }
    };
    let ui_for_menu = ui.as_weak();
    if let Err(error) = macos_window::configure_application_menu(
        &shortcut.native,
        ui.get_menu_shortcuts_enabled(),
        move |section| {
            let Some(ui) = ui_for_menu.upgrade() else {
                return;
            };
            let section = match section {
                macos_window::NativeMenuSection::Settings => {
                    log_menu_action("open-settings");
                    "General"
                }
                macos_window::NativeMenuSection::About => {
                    log_menu_action("open-about");
                    "About"
                }
            };
            ui.invoke_request_settings_section(section.into());
            ui.invoke_open_settings();
        },
    ) {
        warn!(%error, "failed to connect the standard macOS application menu");
    }
}

#[cfg(target_os = "macos")]
fn schedule_macos_application_menu_configuration(ui: &AppWindow) {
    let ui_for_menu = ui.as_weak();
    slint::Timer::single_shot(Duration::from_millis(1), move || {
        if let Some(ui) = ui_for_menu.upgrade() {
            configure_macos_application_menu(&ui);
        }
    });
}
