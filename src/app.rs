//! Slint application controller.
//!
//! This module is the only place that knows about generated Slint types. It
//! maps user intent to domain operations and returns owned worker events to the
//! Slint event loop.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
#[cfg(target_os = "macos")]
use slint::TimerMode;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use tokio::runtime::{Handle, Runtime};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Duration;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use ax_ssh::config::{
    AppSettings, AppSettingsInput, AppearanceSettingsInput, AuthMethod, ConfigStore,
    ConnectionProfile, CredentialStorage, MAX_HOST_CHARS, MAX_PRIVATE_KEY_PATH_CHARS,
    MAX_SESSION_NAME_CHARS, MAX_USERNAME_CHARS, RendererPreference, SerialDataBits,
    SerialFlowControl, SerialParity, SerialStopBits, SessionProfile, SessionStore,
    ShortcutSettings, TerminalColorScheme, TerminalSemanticColorsInput, TerminalSettingsInput,
    ThemePalette, ThemeSettings, UiLanguage, WorkspaceSettingsInput, X11Settings,
    normalize_group_name,
};
use ax_ssh::local_shell::{LocalShellEvent, LocalShellHandle, discover_shells};
use ax_ssh::serial::{
    SerialPortDescriptor, SerialSessionEvent, SerialSessionHandle, discover_serial_ports,
    resolve_serial_port,
};
use ax_ssh::sftp::{SftpBrowserEvent, SftpEntry, SftpTransferEvent};
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
use self::panes::{
    MAX_TERMINAL_PANES, PaneCommand, PaneDirection, PaneDividerPlacement, PaneLayout,
    PanePlacement, PaneTree,
};
use self::session_groups::{
    compact_label, group_options, profile_endpoint, profile_sidebar_details,
    profile_sidebar_endpoint, session_groups,
};
use self::state::{
    ActiveSecurityPrompt, ActiveTabSnapshot, AppState, ClosedTab, ClosedTabKind, ConnectionStart,
    ConnectionTarget, PendingHostKey, PendingProbe, SftpBrowserSnapshot, SftpNavigation,
    SftpTransferPhase, SshConnectionPhase, SshSftpNavigation, TerminalNoticeSnapshot,
    TerminalTabState, TerminalWorker, WorkspaceTabSummary, WorkspaceTransfer,
    finish_stored_credential_retry, prepare_authentication_retry, prepare_host_key_retry,
    prepare_stored_credential_retry, retire_session_attempt, session_attempt_is_active,
    set_credential_storage, set_credential_storage_while_loading,
};
use self::terminal_render::{
    RenderedTerminalLine, RenderedTerminalRun, RgbColor, SemanticColorOverrides,
    TerminalRenderSettings, TerminalRenderer,
};

mod connection;
mod connection_monitor;
mod credential_tasks;
mod diagnostics;
mod file_icons;
mod font_bridge;
mod input;
mod local_files;
#[cfg(target_os = "macos")]
mod macos_window;
mod panes;
mod platform_support;
mod runtime;
mod serial_bridge;
mod session_groups;
mod settings_bridge;
mod sftp_bridge;
mod software_presentation;
mod state;
mod terminal_bridge;
mod terminal_presentation;
mod terminal_render;
mod terminal_targets;
mod view;
mod window_bridge;
mod window_router;
mod workspace;

use self::connection::*;
use self::connection_monitor::*;
use self::diagnostics::*;
use self::font_bridge::*;
use self::platform_support::*;
use self::runtime::*;
use self::serial_bridge::*;
use self::settings_bridge::*;
use self::sftp_bridge::*;
use self::terminal_bridge::*;
use self::view::*;
use self::window_bridge::*;
use self::window_router::*;
use self::workspace::*;

slint::include_modules!();

const MAIN_WINDOW_ID: Uuid = Uuid::from_u128(0);
pub fn run(log_directory: PathBuf) -> Result<()> {
    let config_path = ConfigStore::default_path()?;
    let config = ConfigStore::new(config_path);
    let sessions = config.load().context("failed to load session profiles")?;
    select_slint_renderer(sessions.settings.appearance.renderer_preference)
        .context("failed to select Slint renderer")?;
    let workspace_snapshot = match config.load_workspace() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            warn!(%error, "workspace snapshot could not be loaded; starting with an empty workspace");
            None
        }
    };
    let tokio_worker_threads = tokio_worker_thread_count();
    let runtime = build_tokio_runtime(tokio_worker_threads)
        .context("failed to start bounded Tokio runtime")?;
    info!(
        worker_threads = tokio_worker_threads,
        max_blocking_threads = MAX_TOKIO_BLOCKING_THREADS,
        blocking_thread_keep_alive_ms = TOKIO_BLOCKING_THREAD_KEEP_ALIVE.as_millis(),
        "Tokio runtime initialized with bounded worker pools"
    );
    let initial_font_families = vec![sessions.settings.appearance.application_font_family.clone()];
    let font_registry = Arc::new(Mutex::new(FontRegistry::new()));
    let initial_fonts =
        load_startup_bundled_fonts(runtime.handle(), &font_registry, initial_font_families);
    let software_renderer =
        software_renderer_selected(sessions.settings.appearance.renderer_preference);
    software_presentation::set_enabled(software_renderer);
    let state = Arc::new(Mutex::new(AppState::new(config, sessions)));
    let restore_font_registry = font_registry.clone();
    let restore_terminal_font_started = Arc::new(AtomicBool::new(false));
    let ui = AppWindow::new().context("failed to create Slint window")?;
    ui.set_software_presentation_enabled(software_presentation::is_enabled());
    let window_router = WindowRouter::new(ui.as_weak());
    window_router.set_terminal_presentation_software_renderer(software_renderer);
    let _ = GLOBAL_WINDOW_ROUTER.set(window_router.clone());
    install_window_activation_hook(&window_router)?;
    let detached_windows: Rc<RefCell<HashMap<Uuid, AppWindow>>> =
        Rc::new(RefCell::new(HashMap::new()));

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
    ui.set_terminal_panes(ModelRc::new(VecModel::from(Vec::<TerminalPaneView>::new())));
    ui.set_terminal_dividers(ModelRc::new(VecModel::from(
        Vec::<TerminalPaneDividerView>::new(),
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
    let (previous_tab_shortcut, next_tab_shortcut) = if cfg!(target_os = "macos") {
        ("Cmd+Shift+[", "Cmd+Shift+]")
    } else {
        ("Ctrl+Shift+[", "Ctrl+Shift+]")
    };
    ui.set_previous_tab_menu_shortcut(
        menu_shortcut_from_setting(previous_tab_shortcut)
            .context("failed to configure the previous-tab menu shortcut")?
            .keys,
    );
    ui.set_next_tab_menu_shortcut(
        menu_shortcut_from_setting(next_tab_shortcut)
            .context("failed to configure the next-tab menu shortcut")?
            .keys,
    );
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
    select_ui_language(settings.ui_language)?;
    apply_settings_to_component(&ui, &settings);
    apply_terminal_presentation_policy(&window_router, &settings);
    apply_active_snapshot(&ui, ActiveTabSnapshot::default(), None);
    ui.set_workspace_tabs(ModelRc::new(VecModel::from(Vec::<WorkspaceTabRow>::new())));
    ui.set_status("".into());
    wire_callbacks(
        &ui,
        WindowCallbackContext {
            state: state.clone(),
            runtime: runtime.handle().clone(),
            font_registry: font_registry.clone(),
            log_directory: log_directory.clone(),
            window_router: window_router.clone(),
            window_id: MAIN_WINDOW_ID,
            detached_windows: detached_windows.clone(),
        },
    );
    if let Some(ref snapshot) = workspace_snapshot {
        match snapshot.validate() {
            Ok(()) => {
                if let Ok(mut app) = state.lock() {
                    let _ = app.restore_workspace_tabs(&snapshot.tabs);
                    window_router.apply_snapshot(snapshot, &mut app);
                }
                let restore_context = ConnectionContext::new(
                    ui.as_weak(),
                    state.clone(),
                    runtime.handle().clone(),
                    restore_font_registry.clone(),
                    restore_terminal_font_started.clone(),
                );
                let restored = state
                    .lock()
                    .ok()
                    .map(|app| app.restored_connection_targets())
                    .unwrap_or_default();
                for (tab_id, profile_id, target) in restored {
                    resume_existing_connection(&restore_context, tab_id, profile_id, target);
                }
                let local_tabs = state
                    .lock()
                    .ok()
                    .map(|app| app.restored_local_tabs())
                    .unwrap_or_default();
                for tab_id in local_tabs {
                    if let Err(error) = resume_existing_local_shell(
                        runtime.handle(),
                        state.clone(),
                        ui.as_weak(),
                        tab_id,
                    ) {
                        warn!(%error, tab_id = %tab_id, "failed to restore local shell");
                    }
                }
                refresh_workspace(&ui.as_weak(), &state);
            }
            Err(error) => {
                warn!(%error, "workspace snapshot validation failed; starting with an empty workspace")
            }
        }
    }
    ui.show().context("failed to show main window")?;
    if let Some(snapshot) = workspace_snapshot.as_ref() {
        restore_detached_workspaces(
            snapshot,
            &state,
            runtime.handle(),
            &font_registry_for_restore(&font_registry),
            &log_directory_for_restore(&log_directory),
            &window_router,
            &detached_windows,
        );
    }
    #[cfg(target_os = "macos")]
    let _window_activation_poll = {
        let timer = slint::Timer::default();
        let router_for_activation_poll = window_router.clone();
        timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
            router_for_activation_poll.sync_window_activation_from_native();
        });
        timer
    };
    #[cfg(target_os = "macos")]
    {
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
            schedule_macos_application_menu_configuration(&ui);
        });
    }
    info!("AxSSH UI initialized");
    let ui_result = slint::run_event_loop().context("Slint event loop failed");

    if let Ok(app) = state.lock() {
        let snapshot = window_router.snapshot(&app);
        if let Err(error) = app.config.save_workspace(&snapshot) {
            warn!(%error, "failed to save workspace snapshot during shutdown");
        }
    }
    let (workers, pending_probes) = match state.lock() {
        Ok(mut app) => app.drain_runtime_resources(),
        Err(_) => {
            warn!("state lock poisoned during shutdown; continuing resource cleanup");
            (Vec::new(), Vec::new())
        }
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

    #[cfg(target_os = "macos")]
    drop(_window_activation_poll);
    release_detached_windows(&detached_windows);
    clear_file_icon_cache();
    software_presentation::remove_layout(&ui, MAIN_WINDOW_ID);
    release_window_resources(&ui);
    hide_window_for_release(&ui);
    drop(ui);
    drop(restore_font_registry);
    drop(restore_terminal_font_started);
    drop(font_registry);
    drop(state);
    drop(window_router);
    drop(detached_windows);
    info!("shutting down Tokio runtime");
    runtime.shutdown_timeout(Duration::from_secs(3));
    ui_result?;
    info!("AxSSH UI stopped");
    Ok(())
}

struct WindowCallbackContext {
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    font_registry: Arc<Mutex<FontRegistry>>,
    log_directory: PathBuf,
    window_router: WindowRouter,
    window_id: Uuid,
    detached_windows: Rc<RefCell<HashMap<Uuid, AppWindow>>>,
}

fn wire_callbacks(ui: &AppWindow, context: WindowCallbackContext) {
    let WindowCallbackContext {
        state,
        runtime,
        font_registry,
        log_directory,
        window_router,
        window_id,
        detached_windows,
    } = context;
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
    let terminal_font_started = Arc::new(AtomicBool::new(false));
    let state_for_window_actions = state.clone();
    let runtime_for_window_actions = runtime.clone();
    let font_registry_for_window_actions = font_registry.clone();
    wire_workspace_tabs(
        ui,
        state.clone(),
        runtime.clone(),
        font_registry.clone(),
        terminal_font_started.clone(),
        window_router.clone(),
        window_id,
    );
    let profile_mutations = Arc::new(ProfileMutationCoordinator::default());
    wire_session_editor(
        ui,
        SessionEditorContext::new(
            state.clone(),
            runtime.clone(),
            profile_mutations.clone(),
            font_registry.clone(),
            terminal_font_started.clone(),
            window_router.clone(),
            window_id,
        ),
    );
    wire_serial_port_discovery(ui, state.clone(), runtime.clone());
    wire_session_management(ui, state.clone(), runtime.clone(), profile_mutations);
    wire_connection_request(
        ui,
        state.clone(),
        runtime.clone(),
        font_registry.clone(),
        terminal_font_started.clone(),
        window_router.clone(),
        window_id,
    );
    wire_host_key_confirmation(
        ui,
        state.clone(),
        runtime.clone(),
        window_router.clone(),
        window_id,
    );
    wire_authentication(
        ui,
        state.clone(),
        runtime.clone(),
        window_router.clone(),
        window_id,
    );
    wire_settings(
        ui,
        state.clone(),
        runtime.clone(),
        font_registry.clone(),
        window_router.clone(),
    );
    if let Ok(app) = state.lock() {
        software_presentation::set_rows(
            ui,
            window_id,
            app.sessions
                .settings
                .appearance
                .terminal_software_block_rows,
        );
    }
    wire_sftp(
        ui,
        state.clone(),
        runtime.clone(),
        window_router.clone(),
        window_id,
    );
    wire_terminal(
        ui,
        state,
        runtime,
        font_registry,
        terminal_font_started,
        window_router.clone(),
        window_id,
    );
    wire_window_actions(
        ui,
        state_for_window_actions,
        runtime_for_window_actions,
        font_registry_for_window_actions,
        log_directory,
        window_router,
        window_id,
        detached_windows,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_preference_uses_platform_default_or_explicit_backend() {
        assert_eq!(renderer_backend_name(RendererPreference::Gpu), "winit-skia");
        assert_eq!(
            renderer_backend_name(RendererPreference::Software),
            "winit-software"
        );
        assert_eq!(
            renderer_backend_name(RendererPreference::Automatic),
            if cfg!(target_os = "macos") {
                "winit-skia"
            } else {
                "winit-software"
            }
        );
    }

    #[test]
    fn tokio_worker_thread_count_is_bounded_and_has_a_minimum() {
        assert_eq!(
            super::MIN_TOKIO_WORKER_THREADS,
            super::tokio_worker_thread_count_for_parallelism(1)
        );
        assert_eq!(2, super::tokio_worker_thread_count_for_parallelism(2));
        assert_eq!(4, super::tokio_worker_thread_count_for_parallelism(4));
        assert_eq!(4, super::tokio_worker_thread_count_for_parallelism(32));
    }
}
