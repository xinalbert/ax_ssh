//! Slint application controller.
//!
//! This module is the only place that knows about generated Slint types. It
//! maps user intent to domain operations and returns owned worker events to the
//! Slint event loop.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
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
    AppSettings, AppSettingsInput, AppearanceSettingsInput, AuthMethod, ConfigStore,
    ConnectionProfile, CredentialStorage, MAX_HOST_CHARS, MAX_PRIVATE_KEY_PATH_CHARS,
    MAX_SESSION_NAME_CHARS, MAX_USERNAME_CHARS, SerialDataBits, SerialFlowControl, SerialParity,
    SerialStopBits, SessionProfile, SessionStore, ShortcutSettings, TerminalColorScheme,
    TerminalSemanticColorsInput, TerminalSettingsInput, ThemePalette, ThemeSettings, UiLanguage,
    WorkspaceSettingsInput, X11Settings, normalize_group_name,
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
    TerminalRenderSettings, render_terminal,
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
mod serial_bridge;
mod session_groups;
mod settings_bridge;
mod sftp_bridge;
mod state;
mod terminal_bridge;
mod terminal_render;
mod terminal_targets;
mod view;
mod window_router;
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
use self::window_router::*;
use self::workspace::*;

slint::include_modules!();

const ISSUES_URL: &str = "https://github.com/xinalbert/ax_ssh/issues/new";

const MAIN_WINDOW_ID: Uuid = Uuid::from_u128(0);

pub fn run(log_directory: PathBuf) -> Result<()> {
    select_slint_renderer().context("failed to select Slint renderer")?;
    let config_path = ConfigStore::default_path()?;
    let config = ConfigStore::new(config_path);
    let sessions = config.load().context("failed to load session profiles")?;
    let workspace_snapshot = match config.load_workspace() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            warn!(%error, "workspace snapshot could not be loaded; starting with an empty workspace");
            None
        }
    };
    let runtime = Runtime::new().context("failed to start Tokio runtime")?;
    let initial_font_families = vec![sessions.settings.appearance.application_font_family.clone()];
    let font_registry = Arc::new(Mutex::new(FontRegistry::new()));
    let initial_fonts =
        load_startup_bundled_fonts(runtime.handle(), &font_registry, initial_font_families);
    let state = Arc::new(Mutex::new(AppState::new(config, sessions)));
    let restore_font_registry = font_registry.clone();
    let restore_terminal_font_started = Arc::new(AtomicBool::new(false));
    let ui = AppWindow::new().context("failed to create Slint window")?;
    let window_router = WindowRouter::new(ui.as_weak());
    let _ = GLOBAL_WINDOW_ROUTER.set(window_router.clone());
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
    ui.set_terminal_render_lines(ModelRc::new(VecModel::from(
        Vec::<TerminalRenderLine>::new(),
    )));
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

    clear_file_icon_cache();
    drop(ui);
    runtime.shutdown_timeout(Duration::from_secs(3));
    ui_result?;
    info!("AxSSH UI stopped");
    Ok(())
}

fn select_slint_renderer() -> Result<()> {
    let selector = if std::env::var_os("SLINT_BACKEND").is_some() {
        // Keep the standard Slint environment override available for diagnostics
        // and explicit software-renderer fallback runs.
        slint::BackendSelector::new()
    } else if cfg!(target_os = "macos") {
        slint::BackendSelector::new().backend_name("winit-skia".into())
    } else {
        slint::BackendSelector::new().backend_name("winit-software".into())
    };

    selector.select().map_err(Into::into)
}

fn font_registry_for_restore(registry: &Arc<Mutex<FontRegistry>>) -> Arc<Mutex<FontRegistry>> {
    registry.clone()
}

fn log_directory_for_restore(directory: &Path) -> PathBuf {
    directory.to_owned()
}

fn restore_detached_workspaces(
    snapshot: &ax_ssh::config::WorkspaceSnapshot,
    state: &Arc<Mutex<AppState>>,
    runtime: &Handle,
    font_registry: &Arc<Mutex<FontRegistry>>,
    log_directory: &Path,
    window_router: &WindowRouter,
    detached_windows: &Rc<RefCell<HashMap<Uuid, AppWindow>>>,
) {
    for window in snapshot
        .windows
        .iter()
        .filter(|window| window.id != MAIN_WINDOW_ID)
    {
        let Some(pane_snapshot) = window.panes.first().cloned() else {
            continue;
        };
        let Some(workspace_tab_id) = pane_root_tab_id(&pane_snapshot) else {
            continue;
        };
        let focused_tab_id = window.focused_tab_id.unwrap_or(workspace_tab_id);
        let Some(pane_tree) =
            PaneTree::from_snapshot(workspace_tab_id, pane_snapshot, focused_tab_id)
        else {
            warn!(window_id = %window.id, "skipping invalid detached workspace pane tree");
            continue;
        };
        let Some(active_tab_id) = window.active_tab_id else {
            continue;
        };
        let transfer = WorkspaceTransfer {
            source_window_id: MAIN_WINDOW_ID,
            tab_ids: window.tab_ids.clone(),
            active_tab_id: Some(active_tab_id),
        };
        let detached_ui = match AppWindow::new().and_then(|ui| {
            initialize_detached_component(&ui, state)
                .map_err(|error| slint::PlatformError::from(error.to_string()))?;
            ui.set_detached_window(true);
            Ok(ui)
        }) {
            Ok(ui) => ui,
            Err(error) => {
                warn!(%error, window_id = %window.id, "failed to recreate detached workspace");
                continue;
            }
        };
        let new_window_id = Uuid::new_v4();
        window_router.register_detached(
            new_window_id,
            detached_ui.as_weak(),
            transfer,
            Some(pane_tree),
        );
        wire_callbacks(
            &detached_ui,
            WindowCallbackContext {
                state: state.clone(),
                runtime: runtime.clone(),
                font_registry: font_registry.clone(),
                log_directory: log_directory.to_owned(),
                window_router: window_router.clone(),
                window_id: new_window_id,
                detached_windows: detached_windows.clone(),
            },
        );
        if let Err(error) = detached_ui.show() {
            warn!(%error, window_id = %new_window_id, "failed to show restored detached workspace");
            continue;
        }
        detached_windows
            .borrow_mut()
            .insert(new_window_id, detached_ui);
    }
}

fn pane_root_tab_id(snapshot: &ax_ssh::config::PaneNodeSnapshot) -> Option<Uuid> {
    match snapshot {
        ax_ssh::config::PaneNodeSnapshot::Leaf(id) => Some(*id),
        ax_ssh::config::PaneNodeSnapshot::Split { first, .. } => pane_root_tab_id(first),
    }
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
    wire_settings(ui, state.clone(), runtime.clone(), font_registry.clone());
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

fn initialize_detached_component(ui: &AppWindow, state: &Arc<Mutex<AppState>>) -> Result<()> {
    let settings = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?
        .sessions
        .settings
        .clone();
    let sessions = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    ui.set_sessions(ModelRc::new(VecModel::from(session_group_rows(
        &sessions.sessions,
    ))));
    ui.set_group_options(ModelRc::new(VecModel::from(group_option_rows(
        &sessions.sessions,
    ))));
    ui.set_connection_options(ModelRc::new(VecModel::from(connection_option_rows(
        &sessions.sessions,
    ))));
    drop(sessions);
    ui.set_private_key_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_serial_port_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_terminal_render_lines(ModelRc::new(VecModel::from(
        Vec::<TerminalRenderLine>::new(),
    )));
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
    ui.set_app_version(format!("{} ({})", env!("CARGO_PKG_VERSION"), build_revision()).into());
    apply_settings_to_component(ui, &settings);
    apply_active_snapshot(ui, ActiveTabSnapshot::default(), None);
    ui.set_workspace_tabs(ModelRc::new(VecModel::from(Vec::<WorkspaceTabRow>::new())));
    ui.set_status("".into());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn wire_window_actions(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    font_registry: Arc<Mutex<FontRegistry>>,
    log_directory: PathBuf,
    window_router: WindowRouter,
    window_id: Uuid,
    detached_windows: Rc<RefCell<HashMap<Uuid, AppWindow>>>,
) {
    let ui_for_detach = ui.as_weak();
    let state_for_detach = state.clone();
    let runtime_for_detach = runtime.clone();
    let font_registry_for_detach = font_registry.clone();
    let router_for_detach = window_router.clone();
    let windows_for_detach = detached_windows.clone();
    ui.on_detach_workspace(move |id| {
        log_ui_action("workspace.detach-window");
        if window_id != MAIN_WINDOW_ID {
            set_status(
                &ui_for_detach,
                "Return this workspace before moving it again",
            );
            return;
        }
        let tab_id = match parse_uuid(id.as_str(), "tab", &ui_for_detach) {
            Some(tab_id) => tab_id,
            None => return,
        };
        let (transfer, pane_tree) = match state_for_detach.lock() {
            Ok(mut app) => {
                if !router_for_detach.tab_ids(window_id, &app).contains(&tab_id) {
                    set_status(&ui_for_detach, "Tab not found in this window");
                    return;
                }
                let pane_anchor = app.terminal_companion_id(tab_id).or_else(|| {
                    app.terminal(tab_id)
                        .is_some_and(|terminal| !terminal.is_sftp())
                        .then_some(tab_id)
                });
                let pane_tab_ids = pane_anchor
                    .map(|anchor| router_for_detach.pane_tab_ids(window_id, anchor))
                    .unwrap_or_default();
                let transfer = if pane_tab_ids.is_empty() {
                    app.workspace_transfer_for_sftp(tab_id, window_id)
                } else {
                    app.workspace_transfer_for_terminal_panes(&pane_tab_ids, window_id, tab_id)
                };
                if transfer.is_some() {
                    let _ = app.activate_tab(tab_id);
                }
                let pane_tree = pane_anchor.and_then(|anchor| {
                    router_for_detach.take_pane_tree_for_detach(window_id, anchor)
                });
                (transfer, pane_tree)
            }
            Err(_) => {
                set_status(&ui_for_detach, "Cannot read workspace state");
                return;
            }
        };
        let Some(transfer) = transfer else {
            set_status(&ui_for_detach, "Select a terminal workspace first");
            return;
        };
        // Winit delivers Slint callbacks while dispatching the source-window
        // input event. Create the native window on the next UI turn so that
        // its registration cannot re-enter that dispatch.
        let ui_for_show = ui_for_detach.clone();
        let state_for_show = state_for_detach.clone();
        let runtime_for_show = runtime_for_detach.clone();
        let font_registry_for_show = font_registry_for_detach.clone();
        let router_for_show = router_for_detach.clone();
        let windows_for_show = windows_for_detach.clone();
        let log_directory_for_show = log_directory.clone();
        slint::Timer::single_shot(Duration::from_millis(0), move || {
            let detached_id = Uuid::new_v4();
            #[cfg(target_os = "macos")]
            let show_terminal_titlebar_actions = pane_tree.is_some();
            let detached_ui = match AppWindow::new()
                .context("failed to create detached Slint window")
                .and_then(|detached_ui| {
                    initialize_detached_component(&detached_ui, &state_for_show)?;
                    detached_ui.set_detached_window(true);
                    Ok(detached_ui)
                }) {
                Ok(detached_ui) => detached_ui,
                Err(error) => {
                    let active_tab_id = router_for_show.restore_detached(&DetachedRoute {
                        transfer: transfer.clone(),
                        pane_tree: pane_tree.clone(),
                    });
                    if let Some(active_tab_id) = active_tab_id
                        && let Ok(mut app) = state_for_show.lock()
                    {
                        let _ = app.activate_tab(active_tab_id);
                    }
                    warn!(%error, "failed to create detached workspace window");
                    set_status(
                        &ui_for_show,
                        &format!("Cannot open detached workspace: {error}"),
                    );
                    return;
                }
            };
            router_for_show.register_detached(
                detached_id,
                detached_ui.as_weak(),
                transfer,
                pane_tree,
            );
            wire_callbacks(
                &detached_ui,
                WindowCallbackContext {
                    state: state_for_show.clone(),
                    runtime: runtime_for_show,
                    font_registry: font_registry_for_show,
                    log_directory: log_directory_for_show,
                    window_router: router_for_show.clone(),
                    window_id: detached_id,
                    detached_windows: windows_for_show.clone(),
                },
            );
            if let Err(error) = detached_ui.show() {
                warn!(%error, "failed to show detached workspace window");
                if let Some(detached) = router_for_show.remove_detached(detached_id) {
                    let active_tab_id = router_for_show.restore_detached(&detached);
                    if let Some(active_tab_id) = active_tab_id
                        && let Ok(mut app) = state_for_show.lock()
                    {
                        let _ = app.activate_tab(active_tab_id);
                    }
                }
                set_status(
                    &ui_for_show,
                    &format!("Cannot show detached workspace: {error}"),
                );
                return;
            }
            #[cfg(target_os = "macos")]
            schedule_macos_detached_titlebar_buttons(&detached_ui, show_terminal_titlebar_actions);
            windows_for_show
                .borrow_mut()
                .insert(detached_id, detached_ui);
            refresh_workspace(&ui_for_show, &state_for_show);
        });
    });

    let ui_for_return = ui.as_weak();
    let state_for_return = state.clone();
    let router_for_return = window_router.clone();
    let windows_for_return = detached_windows.clone();
    ui.on_return_workspace(move |id| {
        if window_id == MAIN_WINDOW_ID {
            return;
        }
        let tab_id = match parse_uuid(id.as_str(), "tab", &ui_for_return) {
            Some(tab_id) => tab_id,
            None => return,
        };
        let belongs_to_window = state_for_return
            .lock()
            .is_ok_and(|app| router_for_return.tab_ids(window_id, &app).contains(&tab_id));
        if !belongs_to_window {
            set_status(&ui_for_return, "Tab not found in this window");
            return;
        }
        router_for_return.set_active(window_id, tab_id);
        let Some(detached) = router_for_return.remove_detached(window_id) else {
            return;
        };
        let active_tab_id = router_for_return.restore_detached(&detached);
        if let Some(active_tab_id) = active_tab_id
            && let Ok(mut app) = state_for_return.lock()
        {
            let _ = app.activate_tab(active_tab_id);
        }
        if let Some(ui) = ui_for_return.upgrade() {
            let _ = ui.window().hide();
        }
        let windows = windows_for_return.clone();
        slint::Timer::single_shot(Duration::from_millis(0), move || {
            windows.borrow_mut().remove(&window_id);
        });
        refresh_workspace(&ui_for_return, &state_for_return);
    });

    if window_id != MAIN_WINDOW_ID {
        let state_for_close = state;
        let router_for_close = window_router;
        let windows_for_close = detached_windows;
        ui.window().on_close_requested(move || {
            if let Some(detached) = router_for_close.remove_detached(window_id) {
                let active_tab_id = router_for_close.restore_detached(&detached);
                if let Some(active_tab_id) = active_tab_id
                    && let Ok(mut app) = state_for_close.lock()
                {
                    let _ = app.activate_tab(active_tab_id);
                }
                if let Some(main_ui) = router_for_close.main_ui() {
                    refresh_workspace(&main_ui, &state_for_close);
                }
                let windows = windows_for_close.clone();
                slint::Timer::single_shot(Duration::from_millis(0), move || {
                    windows.borrow_mut().remove(&window_id);
                });
            }
            slint::CloseRequestResponse::HideWindow
        });
    }
}

#[cfg(target_os = "macos")]
fn schedule_macos_detached_titlebar_buttons(ui: &AppWindow, show_terminal_actions: bool) {
    let ui_for_titlebar = ui.as_weak();
    slint::Timer::single_shot(Duration::from_millis(100), move || {
        let Some(ui) = ui_for_titlebar.upgrade() else {
            return;
        };
        let ui_for_split_right = ui.as_weak();
        let ui_for_split_down = ui.as_weak();
        let ui_for_return = ui.as_weak();
        if let Err(error) = macos_window::configure_detached_titlebar_buttons(
            ui.window(),
            detached_titlebar_background(&ui),
            show_terminal_actions,
            move || {
                if let Some(ui) = ui_for_split_right.upgrade() {
                    let _ = ui.invoke_terminal_pane_command(
                        ui.get_active_pane_id(),
                        "split-right".into(),
                    );
                }
            },
            move || {
                if let Some(ui) = ui_for_split_down.upgrade() {
                    let _ = ui
                        .invoke_terminal_pane_command(ui.get_active_pane_id(), "split-down".into());
                }
            },
            move || {
                log_ui_action("workspace.return-window");
                if let Some(ui) = ui_for_return.upgrade() {
                    ui.invoke_return_workspace(ui.get_active_tab_id());
                }
            },
        ) {
            warn!(%error, "failed to configure detached macOS title-bar buttons");
        }
    });
}

#[cfg(target_os = "macos")]
fn detached_titlebar_background(ui: &AppWindow) -> Color {
    if ui.get_active_tab_kind().as_str() == "terminal" {
        ui.get_theme_terminal_background()
    } else {
        ui.global::<Theme>().get_background()
    }
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

#[cfg(target_os = "macos")]
const MACOS_APPLICATION_MENU_MAX_RETRIES: u8 = 8;

#[cfg(target_os = "macos")]
fn configure_macos_application_menu(ui: &AppWindow) -> Result<()> {
    let shortcut = menu_shortcut_from_setting(ui.get_open_settings_shortcut().as_str())
        .context("cannot configure the macOS Settings shortcut")?;
    let ui_for_menu = ui.as_weak();
    macos_window::configure_application_menu(
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
    )
}

#[cfg(target_os = "macos")]
fn schedule_macos_application_menu_configuration(ui: &AppWindow) {
    let ui_for_menu = ui.as_weak();
    slint::Timer::single_shot(Duration::from_millis(1), move || {
        retry_macos_application_menu_configuration(ui_for_menu, 0);
    });
}

#[cfg(target_os = "macos")]
fn retry_macos_application_menu_configuration(ui: slint::Weak<AppWindow>, attempt: u8) {
    let Some(ui) = ui.upgrade() else {
        return;
    };
    match configure_macos_application_menu(&ui) {
        Ok(()) => {}
        Err(_) if should_retry_macos_application_menu_configuration(attempt) => {
            let ui = ui.as_weak();
            slint::Timer::single_shot(Duration::from_millis(25), move || {
                retry_macos_application_menu_configuration(ui, attempt + 1);
            });
        }
        Err(error) => {
            warn!(
                attempts = u16::from(attempt) + 1,
                %error,
                "failed to connect the standard macOS application menu after retries"
            );
        }
    }
}

#[cfg(target_os = "macos")]
fn should_retry_macos_application_menu_configuration(attempt: u8) -> bool {
    attempt < MACOS_APPLICATION_MENU_MAX_RETRIES
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

    #[cfg(target_os = "macos")]
    #[test]
    fn retries_macos_application_menu_within_the_bounded_budget() {
        assert!(should_retry_macos_application_menu_configuration(0));
        assert!(should_retry_macos_application_menu_configuration(
            MACOS_APPLICATION_MENU_MAX_RETRIES - 1
        ));
        assert!(!should_retry_macos_application_menu_configuration(
            MACOS_APPLICATION_MENU_MAX_RETRIES
        ));
    }
}
