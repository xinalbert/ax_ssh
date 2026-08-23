//! Window lifecycle bridge for detached workspace creation and native actions.

use std::path::Path;

use i_slint_core::context::set_window_event_hook;
use i_slint_core::platform::WindowEvent;
use slint::Color;

use super::*;

pub(super) fn font_registry_for_restore(
    registry: &Arc<Mutex<FontRegistry>>,
) -> Arc<Mutex<FontRegistry>> {
    registry.clone()
}

pub(super) fn log_directory_for_restore(directory: &Path) -> PathBuf {
    directory.to_owned()
}

pub(super) fn restore_detached_workspaces(
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

pub(super) fn install_window_activation_hook(window_router: &WindowRouter) -> Result<()> {
    let router = window_router.clone();
    set_window_event_hook(Some(Box::new(move |adapter, event, _dispatch_result| {
        if let WindowEvent::WindowActiveChanged(active) = event {
            router.set_window_active_for_adapter(adapter, *active);
        }
    })))
    .map(|_| ())
    .context("failed to install Slint window activation hook")
}

fn initialize_detached_component(ui: &AppWindow, state: &Arc<Mutex<AppState>>) -> Result<()> {
    let settings = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?
        .sessions
        .settings
        .clone();
    // Detached windows render only the transferable Terminal/SFTP surface.
    // Keep sidebar, settings, and editor option models empty so every
    // detached window does not retain a duplicate copy of session/font data.
    ui.set_sessions(ModelRc::new(VecModel::from(Vec::<SessionGroupRow>::new())));
    ui.set_group_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_connection_options(ModelRc::new(VecModel::from(
        Vec::<ConnectableSessionRow>::new(),
    )));
    ui.set_private_key_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_serial_port_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_terminal_panes(ModelRc::new(VecModel::from(Vec::<TerminalPaneView>::new())));
    ui.set_terminal_dividers(ModelRc::new(VecModel::from(
        Vec::<TerminalPaneDividerView>::new(),
    )));
    ui.set_sftp_entries(ModelRc::new(VecModel::from(Vec::<SftpEntryRow>::new())));
    ui.set_local_shell_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_x11_server_provider_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_x11_server_installations(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_application_font_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_terminal_font_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
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

/// Release Slint-owned application data before a window adapter is dropped.
///
/// The renderer backend owns its surface through the window adapter, but the
/// application owns the models and strings assigned to the component. Clearing
/// them explicitly makes the lifetime independent of the selected renderer and
/// avoids retaining terminal/SFTP rows while a detached window is being hidden.
pub(super) fn release_window_resources(ui: &AppWindow) {
    ui.set_sessions(ModelRc::new(VecModel::from(Vec::<SessionGroupRow>::new())));
    ui.set_workspace_tabs(ModelRc::new(VecModel::from(Vec::<WorkspaceTabRow>::new())));
    ui.set_connection_options(ModelRc::new(VecModel::from(
        Vec::<ConnectableSessionRow>::new(),
    )));
    ui.set_group_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_private_key_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_serial_port_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_local_shell_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_x11_server_provider_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_x11_server_installations(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_application_font_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_terminal_font_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_terminal_panes(ModelRc::new(VecModel::from(Vec::<TerminalPaneView>::new())));
    ui.set_terminal_dividers(ModelRc::new(VecModel::from(
        Vec::<TerminalPaneDividerView>::new(),
    )));
    ui.set_sftp_entries(ModelRc::new(VecModel::from(Vec::<SftpEntryRow>::new())));
    ui.set_local_sftp_entries(ModelRc::new(VecModel::from(Vec::<SftpEntryRow>::new())));
    ui.set_sftp_active_transfers(ModelRc::new(VecModel::from(Vec::<SftpTransferRow>::new())));
    ui.set_sftp_failed_transfers(ModelRc::new(VecModel::from(Vec::<SftpTransferRow>::new())));
    ui.set_sftp_completed_transfers(ModelRc::new(VecModel::from(Vec::<SftpTransferRow>::new())));

    ui.set_editor_profile_id("".into());
    ui.set_editor_name("".into());
    ui.set_editor_group_name("".into());
    ui.set_editor_protocol("".into());
    ui.set_editor_host("".into());
    ui.set_editor_port("".into());
    ui.set_editor_username("".into());
    ui.set_editor_auth_method("".into());
    ui.set_editor_private_key_path("".into());
    ui.set_editor_sftp_remote_path("".into());
    ui.set_editor_sftp_local_path("".into());
    ui.set_editor_credential_storage("".into());
    ui.set_editor_default_credential_storage("".into());
    ui.set_editor_x11_forwarding(false);
    ui.set_editor_serial_port("".into());
    ui.set_editor_serial_baud_rate("".into());
    ui.set_editor_serial_data_bits("".into());
    ui.set_editor_serial_stop_bits("".into());
    ui.set_editor_serial_parity("".into());
    ui.set_editor_serial_flow_control("".into());
    // Publish the empty identity last so SessionEditorPane resets its private
    // draft/password fields from the already-cleared source values.
    ui.set_editor_draft_id("".into());

    ui.set_status("".into());
    ui.set_active_tab_id("".into());
    ui.set_active_pane_id("".into());
    ui.set_active_tab_kind("empty".into());
    ui.set_active_tab_title("".into());
    ui.set_active_tab_status("".into());
    ui.set_active_tab_notice_visible(false);
    ui.set_active_tab_notice_severity("".into());
    ui.set_active_tab_notice_title("".into());
    ui.set_active_tab_notice_message("".into());
    ui.set_active_tab_notice_primary_action("".into());
    ui.set_active_tab_notice_primary_label("".into());
    ui.set_active_tab_notice_secondary_action("".into());
    ui.set_active_tab_notice_secondary_label("".into());

    ui.set_host_key_dialog_open(false);
    ui.set_host_key_endpoint("".into());
    ui.set_host_key_fingerprint("".into());
    ui.set_host_key_changed(false);
    ui.set_host_key_revoked(false);
    ui.set_password_dialog_open(false);
    ui.set_password_endpoint("".into());
    ui.set_password_private_key(false);
    ui.set_password_vault_storage(false);
    ui.set_password_vault_unlock_only(false);
    ui.set_password_key_path("".into());
    ui.set_password_dialog_tab_id("".into());

    ui.set_sftp_available(false);
    ui.set_sftp_open(false);
    ui.set_sftp_loading(false);
    ui.set_sftp_home("".into());
    ui.set_sftp_path("".into());
    ui.set_sftp_has_more(false);
    ui.set_sftp_truncated(false);
    ui.set_sftp_status("".into());
    ui.set_sftp_can_go_back(false);
    ui.set_sftp_can_go_forward(false);
    ui.set_sftp_selected_count(0);
    ui.set_sftp_all_selected(false);
    ui.set_local_sftp_loading(false);
    ui.set_local_sftp_path("".into());
    ui.set_local_sftp_truncated(false);
    ui.set_local_sftp_status("".into());
    ui.set_local_sftp_selected_count(0);
    ui.set_local_sftp_all_selected(false);
    ui.set_sftp_transfer_active_count(0);
    ui.set_sftp_transfer_failed_count(0);
    ui.set_sftp_transfer_completed_count(0);
    ui.set_sftp_transfer_selected_active_count(0);
    ui.set_sftp_transfer_selected_pausable_count(0);
    ui.set_sftp_transfer_selected_resumable_count(0);
    ui.set_sftp_editor_path("".into());
    ui.set_sftp_editor_text("".into());
    ui.set_sftp_rename_name("".into());
    ui.set_sftp_editor_remote_changed(false);
    ui.set_sftp_editor_auto_upload(false);
    ui.set_sftp_editor_revision(0);

    ui.set_application_font_family("".into());
    ui.set_terminal_font_family("".into());
    tracing::debug!("released Slint window models and application-owned UI fields");
}

pub(super) fn hide_window_for_release(ui: &AppWindow) {
    if let Err(error) = ui.window().hide() {
        tracing::debug!(%error, "window was already hidden during resource release");
    }
}

pub(super) fn release_detached_windows(detached_windows: &Rc<RefCell<HashMap<Uuid, AppWindow>>>) {
    let windows = detached_windows
        .borrow_mut()
        .drain()
        .map(|(_, ui)| ui)
        .collect::<Vec<_>>();
    for ui in &windows {
        release_window_resources(ui);
        hide_window_for_release(ui);
    }
    if !windows.is_empty() {
        tracing::debug!(
            window_count = windows.len(),
            "released detached window resources"
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn wire_window_actions(
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
        let detached_ui = windows_for_return.borrow_mut().remove(&window_id);
        if let Some(detached_ui) = detached_ui {
            release_window_resources(&detached_ui);
            hide_window_for_release(&detached_ui);
        } else if let Some(ui) = ui_for_return.upgrade() {
            release_window_resources(&ui);
            hide_window_for_release(&ui);
        }
        refresh_workspace(&ui_for_return, &state_for_return);
    });

    if window_id != MAIN_WINDOW_ID {
        let state_for_close = state;
        let router_for_close = window_router;
        let windows_for_close = detached_windows;
        let ui_for_close = ui.as_weak();
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
                let detached_ui = windows_for_close.borrow_mut().remove(&window_id);
                if let Some(detached_ui) = detached_ui {
                    release_window_resources(&detached_ui);
                    hide_window_for_release(&detached_ui);
                } else if let Some(ui) = ui_for_close.upgrade() {
                    release_window_resources(&ui);
                    hide_window_for_release(&ui);
                }
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
pub(super) fn detached_titlebar_background(ui: &AppWindow) -> Color {
    if ui.get_active_tab_kind().as_str() == "terminal" {
        ui.get_theme_terminal_background()
    } else {
        ui.global::<Theme>().get_background()
    }
}
