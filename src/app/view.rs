use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use slint::Model;

use super::file_icons::{FileIconKey, clear_global_cache, global_provider, prewarm_async};
use super::local_files::LocalDirectoryEntry;
use super::state::{SftpTransferPhase, SftpTransferSnapshot};
use super::*;

const ICON_PREWARM_PENDING_KEY_LIMIT: usize = 256;
const ICON_PREWARM_BATCH_KEY_LIMIT: usize = 64;
static PRIVATE_KEY_OPTION_GENERATION: AtomicU64 = AtomicU64::new(0);

pub(super) fn session_group_rows(sessions: &SessionStore) -> Vec<SessionGroupRow> {
    session_groups(sessions)
        .into_iter()
        .map(|group| {
            let group_name = group.name;
            let display_name = if group_name.is_empty() {
                "Ungrouped".to_owned()
            } else {
                group_name.clone()
            };
            let profiles = ModelRc::new(VecModel::from(
                group
                    .profiles
                    .into_iter()
                    .map(|profile| SessionProfileRow {
                        id: profile.id.to_string().into(),
                        name: profile.name.clone().into(),
                        details: profile_sidebar_details(profile).into(),
                        endpoint: profile_sidebar_endpoint(
                            profile,
                            &sessions.settings.workspace.session_mask_character,
                        )
                        .into(),
                        icon: compact_label(&profile.name, "--", 2).into(),
                        sftp_enabled: profile.ssh().is_some(),
                    })
                    .collect::<Vec<_>>(),
            ));
            SessionGroupRow {
                group_name: group_name.into(),
                name: display_name.clone().into(),
                icon: compact_label(
                    &display_name,
                    "Un",
                    usize::from(sessions.settings.workspace.collapsed_group_label_chars),
                )
                .into(),
                profiles,
            }
        })
        .collect()
}

pub(super) fn connection_option_rows(sessions: &SessionStore) -> Vec<ConnectableSessionRow> {
    sessions
        .sessions
        .iter()
        .map(|profile| ConnectableSessionRow {
            id: profile.id.to_string().into(),
            name: profile.name.clone().into(),
            endpoint: profile_sidebar_endpoint(
                profile,
                &sessions.settings.workspace.session_mask_character,
            )
            .into(),
        })
        .collect()
}

pub(super) fn group_option_rows(sessions: &SessionStore) -> Vec<SharedString> {
    group_options(sessions)
        .into_iter()
        .map(SharedString::from)
        .collect()
}

pub(super) fn shell_option_rows(settings: &AppSettings) -> Vec<SharedString> {
    settings
        .terminal
        .known_shells
        .iter()
        .cloned()
        .map(SharedString::from)
        .collect()
}

pub(super) fn font_option_rows(selected: &str, system_families: &[String]) -> Vec<SharedString> {
    font_options(selected, system_families)
        .into_iter()
        .map(SharedString::from)
        .collect()
}

pub(super) fn refresh_session_models(ui: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let state = Arc::clone(state);
    dispatch_ui(ui, move |ui| {
        let (rows, groups, options) = match state.lock() {
            Ok(app) => (
                session_group_rows(&app.sessions),
                group_option_rows(&app.sessions),
                connection_option_rows(&app.sessions),
            ),
            Err(_) => {
                ui.set_status("State lock poisoned".into());
                return;
            }
        };
        ui.set_sessions(ModelRc::new(VecModel::from(rows)));
        ui.set_group_options(ModelRc::new(VecModel::from(groups)));
        ui.set_connection_options(ModelRc::new(VecModel::from(options)));
    });
}

pub(super) fn refresh_workspace(ui: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    if let Some(router) = global_window_router() {
        let state = Arc::clone(state);
        let views = match state.lock() {
            Ok(app) => router.views(&app),
            Err(_) => {
                set_status(ui, "State lock poisoned");
                return;
            }
        };
        for view in views {
            let state = Arc::clone(&state);
            dispatch_ui(&view.ui, move |ui| {
                let settings_tab_id = view
                    .tabs
                    .iter()
                    .find(|tab| tab.kind == "settings")
                    .map(|tab| tab.id.to_string())
                    .unwrap_or_default();
                ui.set_workspace_tabs(ModelRc::new(VecModel::from(visible_workspace_tab_rows(
                    view.tabs,
                ))));
                ui.set_settings_tab_id(settings_tab_id.into());
                apply_active_snapshot(ui, view.snapshot, view.active_tab_id);
                apply_terminal_panes(ui, view.terminal_panes);
                drop(state);
                #[cfg(target_os = "macos")]
                schedule_macos_application_menu_configuration(ui);
            });
        }
        return;
    }
    let state = Arc::clone(state);
    dispatch_ui(ui, move |ui| {
        let (tabs, snapshot) = match state.lock() {
            Ok(app) => (
                visible_workspace_tab_rows(app.tab_summaries()),
                app.active_snapshot(),
            ),
            Err(_) => {
                ui.set_status("State lock poisoned".into());
                return;
            }
        };
        let settings_tab_id = tabs
            .iter()
            .find(|tab| tab.kind.as_str() == "settings")
            .map(|tab| tab.id.clone())
            .unwrap_or_default();
        ui.set_workspace_tabs(ModelRc::new(VecModel::from(tabs)));
        ui.set_settings_tab_id(settings_tab_id);
        apply_active_snapshot(ui, snapshot, None);
        ui.set_terminal_panes(ModelRc::new(VecModel::from(Vec::<TerminalPaneView>::new())));
        #[cfg(target_os = "macos")]
        schedule_macos_application_menu_configuration(ui);
    });
}

pub(super) fn visible_workspace_tab_rows(tabs: Vec<WorkspaceTabSummary>) -> Vec<WorkspaceTabRow> {
    tabs.into_iter()
        .map(|tab| WorkspaceTabRow {
            id: tab.id.to_string().into(),
            title: tab.title.into(),
            kind: tab.kind.into(),
            connected: tab.connected,
        })
        .collect()
}

fn sftp_entry_rows(
    entries: Vec<SftpEntry>,
    selected: &std::collections::HashSet<String>,
) -> Vec<SftpEntryRow> {
    entries
        .into_iter()
        .map(|entry| {
            let hidden = entry.name.starts_with('.');
            let is_selected = selected.contains(&entry.path);
            let icon_key = FileIconKey::for_entry(&entry.name, entry.is_dir, entry.is_symlink);
            SftpEntryRow {
                icon: slint_icon(&icon_key),
                has_icon: true,
                name: entry.name.into(),
                path: entry.path.into(),
                kind: if entry.is_dir {
                    "folder"
                } else if entry.is_symlink {
                    "link"
                } else {
                    "file"
                }
                .into(),
                size: format_file_size(entry.size, entry.is_dir).into(),
                modified: entry
                    .modified
                    .map(format_timestamp)
                    .unwrap_or_default()
                    .into(),
                hidden,
                selected: is_selected,
            }
        })
        .collect()
}

fn local_entry_rows(
    entries: Vec<LocalDirectoryEntry>,
    selected: &std::collections::HashSet<String>,
) -> Vec<SftpEntryRow> {
    entries
        .into_iter()
        .map(|entry| {
            let hidden = entry.name.starts_with('.');
            let is_selected = selected.contains(&entry.path);
            let icon_key = FileIconKey::for_entry(&entry.name, entry.is_dir, entry.is_symlink);
            SftpEntryRow {
                icon: slint_icon(&icon_key),
                has_icon: true,
                name: entry.name.into(),
                path: entry.path.into(),
                kind: if entry.is_dir {
                    "folder"
                } else if entry.is_symlink {
                    "link"
                } else {
                    "file"
                }
                .into(),
                size: format_file_size(entry.size, entry.is_dir).into(),
                modified: entry
                    .modified
                    .map(format_local_timestamp)
                    .unwrap_or_default()
                    .into(),
                hidden,
                selected: is_selected,
            }
        })
        .collect()
}

fn slint_icon(key: &FileIconKey) -> slint::Image {
    let icon = global_provider().cached_icon(key);
    let pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        icon.rgba(),
        icon.width(),
        icon.height(),
    );
    slint::Image::from_rgba8(pixels)
}

pub(super) fn prewarm_file_icons(
    runtime: &tokio::runtime::Handle,
    keys: Vec<FileIconKey>,
    ui: &slint::Weak<AppWindow>,
    state: &std::sync::Arc<std::sync::Mutex<AppState>>,
) {
    if keys.is_empty() {
        return;
    }
    let coordinator = icon_prewarm_coordinator();
    let should_start = {
        let Ok(mut pending) = coordinator.lock() else {
            return;
        };
        for key in keys {
            if pending.queued.contains(&key) {
                continue;
            }
            if pending.keys.len() >= ICON_PREWARM_PENDING_KEY_LIMIT {
                break;
            }
            if pending.queued.insert(key.clone()) {
                pending.keys.push_back(key);
            }
        }
        pending.target = Some(IconPrewarmTarget {
            ui: ui.clone(),
            state: Arc::clone(state),
        });
        if pending.running {
            false
        } else {
            pending.running = true;
            true
        }
    };
    if should_start {
        let runtime = runtime.clone();
        let _ = runtime.clone().spawn(run_icon_prewarm_worker(
            runtime,
            coordinator,
            ICON_PREWARM_BATCH_KEY_LIMIT,
        ));
    }
}

struct IconPrewarmTarget {
    ui: slint::Weak<AppWindow>,
    state: Arc<Mutex<AppState>>,
}

struct IconPrewarmCoordinator {
    keys: VecDeque<FileIconKey>,
    queued: HashSet<FileIconKey>,
    target: Option<IconPrewarmTarget>,
    running: bool,
    generation: u64,
}

static ICON_PREWARM_COORDINATOR: OnceLock<Arc<Mutex<IconPrewarmCoordinator>>> = OnceLock::new();

fn icon_prewarm_coordinator() -> Arc<Mutex<IconPrewarmCoordinator>> {
    ICON_PREWARM_COORDINATOR
        .get_or_init(|| {
            Arc::new(Mutex::new(IconPrewarmCoordinator {
                keys: VecDeque::new(),
                queued: HashSet::new(),
                target: None,
                running: false,
                generation: 0,
            }))
        })
        .clone()
}

async fn run_icon_prewarm_worker(
    runtime: tokio::runtime::Handle,
    coordinator: Arc<Mutex<IconPrewarmCoordinator>>,
    batch_limit: usize,
) {
    loop {
        let (keys, target, generation) = {
            let Ok(mut pending) = coordinator.lock() else {
                return;
            };
            if pending.keys.is_empty() {
                pending.running = false;
                pending.target = None;
                return;
            }
            let mut keys = Vec::with_capacity(batch_limit);
            while keys.len() < batch_limit {
                let Some(key) = pending.keys.pop_front() else {
                    break;
                };
                pending.queued.remove(&key);
                keys.push(key);
            }
            (
                keys,
                pending.target.as_ref().map(|target| IconPrewarmTarget {
                    ui: target.ui.clone(),
                    state: Arc::clone(&target.state),
                }),
                pending.generation,
            )
        };

        let prewarm = prewarm_async(&runtime, keys);
        if let Err(error) = prewarm.await {
            tracing::debug!(%error, "file icon prewarm task stopped");
        }
        let still_current = coordinator
            .lock()
            .is_ok_and(|pending| pending.generation == generation);
        if !still_current {
            clear_global_cache();
            continue;
        }
        if let Some(target) = target {
            dispatch_active_snapshot(&target.ui, &target.state);
        }
    }
}

pub(super) fn clear_file_icon_cache() {
    if let Some(coordinator) = ICON_PREWARM_COORDINATOR.get()
        && let Ok(mut pending) = coordinator.lock()
    {
        pending.keys.clear();
        pending.queued.clear();
        pending.target = None;
        pending.generation = pending.generation.wrapping_add(1);
    }
    clear_global_cache();
}

pub(super) fn sftp_icon_keys(entries: &[SftpEntry]) -> Vec<FileIconKey> {
    entries
        .iter()
        .map(|entry| FileIconKey::for_entry(&entry.name, entry.is_dir, entry.is_symlink))
        .collect()
}

pub(super) fn local_icon_keys(entries: &[LocalDirectoryEntry]) -> Vec<FileIconKey> {
    entries
        .iter()
        .map(|entry| FileIconKey::for_entry(&entry.name, entry.is_dir, entry.is_symlink))
        .collect()
}

fn sftp_transfer_rows(transfers: Vec<SftpTransferSnapshot>) -> Vec<SftpTransferRow> {
    transfers
        .into_iter()
        .map(|transfer| {
            let progress = if transfer.total_bytes == 0 {
                0.0
            } else {
                (transfer.downloaded_bytes as f64 / transfer.total_bytes as f64).clamp(0.0, 1.0)
                    as f32
            };
            let size = if transfer.total_bytes == 0 {
                format_file_size(transfer.downloaded_bytes, false)
            } else if transfer.downloaded_bytes >= transfer.total_bytes {
                format_file_size(transfer.total_bytes, false)
            } else {
                format!(
                    "{} / {}",
                    format_file_size(transfer.downloaded_bytes, false),
                    format_file_size(transfer.total_bytes, false)
                )
            };
            let speed = if transfer.phase.cancellable() && transfer.bytes_per_second > 0 {
                format!("{}/s", format_file_size(transfer.bytes_per_second, false))
            } else {
                String::new()
            };
            SftpTransferRow {
                id: transfer.id.to_string().into(),
                name: transfer.name.into(),
                state: transfer.phase.as_str().into(),
                status: transfer.status.into(),
                progress,
                size: size.into(),
                speed: speed.into(),
                cancellable: transfer.phase.cancellable(),
            }
        })
        .collect()
}

fn format_file_size(bytes: u64, is_dir: bool) -> String {
    if is_dir {
        return "-".to_owned();
    }
    if bytes < 1_024 {
        format!("{bytes} B")
    } else if bytes < 1_048_576 {
        format!("{:.1} KB", bytes as f64 / 1_024.0)
    } else if bytes < 1_073_741_824 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    }
}

fn format_timestamp(timestamp: u32) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(i64::from(timestamp), 0)
        .map(|time| time.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

fn format_local_timestamp(timestamp: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(timestamp)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

pub(super) fn set_tab_status(
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    tab_id: Uuid,
    message: &str,
) {
    let active = match state.lock() {
        Ok(mut app) => {
            let Some(terminal) = app.terminal_mut(tab_id) else {
                return;
            };
            terminal.status = message.to_owned();
            app.active_tab_id() == Some(tab_id)
        }
        Err(_) => {
            set_status(ui, "State lock poisoned");
            return;
        }
    };
    if active || global_window_router().is_some() {
        dispatch_active_snapshot(ui, state);
    }
}

pub(super) fn dispatch_active_snapshot(ui: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    if global_window_router().is_some() {
        refresh_workspace(ui, state);
        return;
    }
    let should_schedule = match state.lock() {
        Ok(app) => app.try_schedule_ui_refresh(),
        Err(_) => {
            set_status(ui, "State lock poisoned");
            return;
        }
    };
    if !should_schedule {
        return;
    }
    let state = Arc::clone(state);
    let state_for_ui = Arc::clone(&state);
    if !dispatch_ui_result(ui, move |ui| {
        // Worker output and resize events can queue faster than the UI event loop runs.
        // Resolve the snapshot here so an older queued event cannot restore stale dimensions.
        let snapshot = match state_for_ui.lock() {
            Ok(app) => {
                app.clear_ui_refresh_pending();
                app.active_snapshot()
            }
            Err(_) => {
                ui.set_status("State lock poisoned".into());
                return;
            }
        };
        apply_active_snapshot(ui, snapshot, None);
    }) {
        if let Ok(app) = state.lock() {
            app.clear_ui_refresh_pending();
        }
    }
}

pub(super) fn dispatch_terminal_output_snapshot(
    ui: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    output_received_at: std::time::Instant,
) {
    if global_window_router().is_some() {
        refresh_workspace(ui, state);
        return;
    }
    let should_schedule = match state.lock() {
        Ok(app) => app.try_schedule_ui_refresh(),
        Err(_) => {
            set_status(ui, "State lock poisoned");
            return;
        }
    };
    if !should_schedule {
        return;
    }
    let state = Arc::clone(state);
    let state_for_ui = Arc::clone(&state);
    let dispatch_requested_at = std::time::Instant::now();
    if !dispatch_ui_result(ui, move |ui| {
        let ui_started_at = std::time::Instant::now();
        let snapshot = match state_for_ui.lock() {
            Ok(app) => {
                app.clear_ui_refresh_pending();
                app.active_snapshot()
            }
            Err(_) => {
                ui.set_status("State lock poisoned".into());
                return;
            }
        };
        apply_active_snapshot(ui, snapshot, None);
        tracing::debug!(
            target: "ax_ssh::latency",
            event = "ssh-output",
            stage = "ui-applied",
            output_to_dispatch_us = duration_micros(
                dispatch_requested_at.saturating_duration_since(output_received_at),
            ),
            ui_queue_us = duration_micros(
                ui_started_at.saturating_duration_since(dispatch_requested_at),
            ),
            ui_apply_us = duration_micros(ui_started_at.elapsed()),
            output_to_ui_us = duration_micros(output_received_at.elapsed()),
            "SSH terminal output applied to UI"
        );
    }) {
        if let Ok(app) = state.lock() {
            app.clear_ui_refresh_pending();
        }
    }
}

fn duration_micros(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

pub(super) fn apply_active_snapshot(
    ui: &AppWindow,
    snapshot: ActiveTabSnapshot,
    workspace_tab_id: Option<Uuid>,
) {
    let active_pane_id = snapshot.id.map(|id| id.to_string()).unwrap_or_default();
    let active_tab_id = workspace_tab_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| active_pane_id.clone());
    ui.set_active_tab_id(active_tab_id.into());
    ui.set_active_pane_id(active_pane_id.into());
    ui.set_active_tab_kind(snapshot.kind.into());
    ui.set_active_tab_title(snapshot.title.into());
    ui.set_active_tab_status(snapshot.status.into());
    if let Some(editor) = snapshot.editor {
        ui.set_editor_credential_storage(editor.credential_storage.clone().into());
        ui.set_editor_default_credential_storage(editor.default_credential_storage.clone().into());
        let draft_id = editor.draft_id.to_string();
        if ui.get_editor_draft_id().as_str() != draft_id {
            ui.set_editor_profile_id(
                editor
                    .profile_id
                    .map(|profile_id| profile_id.to_string())
                    .unwrap_or_default()
                    .into(),
            );
            ui.set_editor_name(editor.name.into());
            ui.set_editor_group_name(editor.group_name.into());
            ui.set_editor_protocol(editor.protocol.into());
            ui.set_editor_host(editor.host.into());
            ui.set_editor_port(editor.port.into());
            ui.set_editor_username(editor.username.into());
            ui.set_editor_auth_method(editor.auth_method.into());
            ui.set_editor_private_key_path(editor.private_key_path.into());
            ui.set_editor_x11_forwarding(editor.x11_forwarding);
            ui.set_editor_serial_port(editor.serial_port.into());
            ui.set_editor_serial_baud_rate(editor.serial_baud_rate.into());
            ui.set_editor_serial_data_bits(editor.serial_data_bits.into());
            ui.set_editor_serial_stop_bits(editor.serial_stop_bits.into());
            ui.set_editor_serial_parity(editor.serial_parity.into());
            ui.set_editor_serial_flow_control(editor.serial_flow_control.into());
            // The Slint editor resets its local fields when this identity changes.
            // Publish it last so all source values form one coherent draft.
            ui.set_editor_draft_id(draft_id.into());
        }
    }
    let terminal = snapshot.terminal.unwrap_or_else(empty_terminal_snapshot);
    let rendered = render_terminal(
        terminal,
        TerminalRenderSettings {
            color_scheme: TerminalColorScheme::from_setting(
                ui.get_terminal_color_scheme().as_str(),
            ),
            default_foreground: to_rgb_color(ui.get_theme_terminal_foreground()),
            default_background: to_rgb_color(ui.get_theme_terminal_background()),
            selection_background: to_rgb_color(ui.get_theme_terminal_selection()),
            minimum_contrast_ratio: f64::from(
                ui.get_terminal_minimum_contrast_ratio().clamp(1.0, 21.0),
            ),
            bright_bold_text: ui.get_bright_bold_text(),
        },
    );
    apply_rendered_terminal(ui, rendered);
    ui.set_connected(snapshot.connected);
    ui.set_worker_running(snapshot.worker_running);
    apply_sftp_snapshot(ui, snapshot.sftp);
    apply_security_prompt(ui, snapshot.security_prompt);
}

fn apply_terminal_panes(ui: &AppWindow, panes: Vec<WindowTerminalPane>) {
    let settings = TerminalRenderSettings {
        color_scheme: TerminalColorScheme::from_setting(ui.get_terminal_color_scheme().as_str()),
        default_foreground: to_rgb_color(ui.get_theme_terminal_foreground()),
        default_background: to_rgb_color(ui.get_theme_terminal_background()),
        selection_background: to_rgb_color(ui.get_theme_terminal_selection()),
        minimum_contrast_ratio: f64::from(
            ui.get_terminal_minimum_contrast_ratio().clamp(1.0, 21.0),
        ),
        bright_bold_text: ui.get_bright_bold_text(),
    };
    let panes = panes
        .into_iter()
        .map(|pane| {
            let terminal = pane
                .snapshot
                .terminal
                .unwrap_or_else(empty_terminal_snapshot);
            let rendered = render_terminal(terminal, settings);
            TerminalPaneView {
                terminal: terminal_view_from_rendered(
                    pane.placement.tab_id,
                    pane.snapshot.connected,
                    rendered,
                    ui,
                ),
                x: pane.placement.x,
                y: pane.placement.y,
                width: pane.placement.width,
                height: pane.placement.height,
                focused: pane.placement.focused,
            }
        })
        .collect::<Vec<_>>();
    ui.set_terminal_panes(ModelRc::new(VecModel::from(panes)));
}

fn terminal_view_from_rendered(
    tab_id: Uuid,
    connected: bool,
    rendered: terminal_render::RenderedTerminal,
    ui: &AppWindow,
) -> TerminalViewState {
    let lines = rendered
        .lines
        .into_iter()
        .map(terminal_render_line)
        .collect::<Vec<_>>();
    TerminalViewState {
        terminal_id: tab_id.to_string().into(),
        connected,
        render_lines: ModelRc::new(VecModel::from(lines)),
        content_columns: rendered.max_columns.min(i32::MAX as usize) as i32,
        cursor_row: rendered.cursor_row.min(i32::MAX as usize) as i32,
        cursor_column: rendered.cursor_column.min(i32::MAX as usize) as i32,
        cursor_visible: rendered.cursor_visible,
        cursor_text: rendered.cursor_text.into(),
        font_family: ui.get_terminal_font_family(),
        font_size: ui.get_terminal_font_size() as f32,
        line_height_percent: ui.get_terminal_line_height_percent(),
        foreground: to_slint_color(rendered.foreground),
        background: to_slint_color(rendered.background),
        selection_background: to_slint_color(rendered.selection_background),
        right_click_copy_or_paste: ui.get_right_click_copy_or_paste(),
        option_as_meta: ui.get_option_as_meta(),
        copy_selection_shortcut: ui.get_copy_selection_shortcut(),
        paste_shortcut: ui.get_paste_shortcut(),
    }
}

fn apply_sftp_snapshot(ui: &AppWindow, snapshot: SftpBrowserSnapshot) {
    let transfer_active_count = snapshot
        .transfers
        .iter()
        .filter(|transfer| transfer.phase.cancellable())
        .count() as i32;
    let transfer_failed_count = snapshot
        .transfers
        .iter()
        .filter(|transfer| transfer.phase == SftpTransferPhase::Failed)
        .count() as i32;
    let transfer_completed_count = snapshot
        .transfers
        .iter()
        .filter(|transfer| transfer.phase == SftpTransferPhase::Completed)
        .count() as i32;
    ui.set_sftp_available(snapshot.available);
    ui.set_sftp_open(snapshot.open);
    ui.set_sftp_loading(snapshot.loading);
    ui.set_sftp_home(snapshot.home.into());
    ui.set_sftp_path(snapshot.path.into());
    ui.set_sftp_entries(ModelRc::new(VecModel::from(sftp_entry_rows(
        snapshot.entries,
        &snapshot.selected,
    ))));
    ui.set_sftp_has_more(snapshot.has_more);
    ui.set_sftp_truncated(snapshot.truncated);
    ui.set_sftp_status(snapshot.status.into());
    ui.set_sftp_can_go_back(snapshot.can_go_back);
    ui.set_sftp_can_go_forward(snapshot.can_go_forward);
    ui.set_sftp_selected_count(snapshot.selected_count as i32);
    ui.set_sftp_all_selected(snapshot.all_selected);
    ui.set_local_sftp_loading(snapshot.local.loading);
    ui.set_local_sftp_path(snapshot.local.path.into());
    ui.set_local_sftp_entries(ModelRc::new(VecModel::from(local_entry_rows(
        snapshot.local.entries,
        &snapshot.local.selected,
    ))));
    ui.set_local_sftp_truncated(snapshot.local.truncated);
    ui.set_local_sftp_status(snapshot.local.status.into());
    ui.set_local_sftp_selected_count(snapshot.local.selected_count as i32);
    ui.set_local_sftp_all_selected(snapshot.local.all_selected);
    ui.set_sftp_transfers(ModelRc::new(VecModel::from(sftp_transfer_rows(
        snapshot.transfers,
    ))));
    ui.set_sftp_transfer_active_count(transfer_active_count);
    ui.set_sftp_transfer_failed_count(transfer_failed_count);
    ui.set_sftp_transfer_completed_count(transfer_completed_count);
}

fn apply_security_prompt(ui: &AppWindow, prompt: ActiveSecurityPrompt) {
    match prompt {
        ActiveSecurityPrompt::None => {
            ui.set_host_key_dialog_open(false);
            ui.set_password_dialog_open(false);
            ui.set_password_dialog_tab_id("".into());
        }
        ActiveSecurityPrompt::HostKey(prompt) => {
            ui.set_host_key_endpoint(format!("{}:{}", prompt.host, prompt.port).into());
            ui.set_host_key_fingerprint(prompt.fingerprint.into());
            ui.set_host_key_changed(prompt.changed);
            ui.set_password_dialog_open(false);
            ui.set_password_dialog_tab_id("".into());
            ui.set_host_key_dialog_open(true);
        }
        ActiveSecurityPrompt::Authentication {
            tab_id,
            profile,
            vault_unlock_only,
        } => {
            let Some(ssh) = profile.ssh() else {
                ui.set_host_key_dialog_open(false);
                ui.set_password_dialog_open(false);
                ui.set_password_dialog_tab_id("".into());
                return;
            };
            let (private_key, key_path) = match &ssh.auth {
                AuthMethod::Password => (false, String::new()),
                AuthMethod::PrivateKey { path } => (true, path.display().to_string()),
                AuthMethod::SshAgent => {
                    ui.set_host_key_dialog_open(false);
                    ui.set_password_dialog_open(false);
                    ui.set_password_dialog_tab_id("".into());
                    return;
                }
            };
            let vault_storage = vault_unlock_only
                && !private_key
                && ssh.credential_storage == Some(CredentialStorage::EncryptedVault);
            ui.set_host_key_dialog_open(false);
            ui.set_password_endpoint(profile_endpoint(&profile).into());
            ui.set_password_private_key(private_key);
            ui.set_password_vault_storage(vault_storage);
            ui.set_password_vault_unlock_only(vault_unlock_only);
            ui.set_password_key_path(key_path.into());
            ui.set_password_dialog_tab_id(tab_id.to_string().into());
            ui.set_password_dialog_open(true);
        }
    }
}

pub(super) fn apply_settings_to_component(ui: &AppWindow, settings: &AppSettings) {
    apply_theme_to_component(ui, settings);
    ui.set_application_font_family(settings.appearance.application_font_family.clone().into());
    ui.set_application_font_index(font_option_index(
        &ui.get_application_font_options(),
        &settings.appearance.application_font_family,
    ));
    ui.set_terminal_font_family(settings.appearance.terminal_font_family.clone().into());
    ui.set_terminal_font_index(font_option_index(
        &ui.get_terminal_font_options(),
        &settings.appearance.terminal_font_family,
    ));
    ui.set_terminal_font_size(i32::from(settings.appearance.terminal_font_size));
    ui.set_terminal_line_height_percent(i32::from(
        settings.appearance.terminal_line_height_percent,
    ));
    ui.set_terminal_minimum_contrast_ratio(
        f32::from(settings.appearance.terminal_minimum_contrast_ratio_tenths) / 10.0,
    );
    ui.set_bright_bold_text(settings.appearance.bright_bold_text);
    ui.set_right_click_copy_or_paste(settings.appearance.right_click_copy_or_paste);
    ui.set_option_as_meta(settings.terminal.option_as_meta);
    ui.set_x11_server_provider(
        ax_ssh::x_server::provider_for_current_platform(settings.x11.provider)
            .as_setting()
            .into(),
    );
    ui.set_x11_server_provider_index(ax_ssh::x_server::provider_index(settings.x11.provider));
    ui.set_x11_server_app_path(settings.x11.app_path.clone().into());
    ui.set_x11_launch_on_connect(settings.x11.launch_on_connect);
    ui.set_x11_allow_no_auth(settings.x11.allow_no_auth);
    ui.set_scrollback_lines(settings.terminal.scrollback_lines as i32);
    ui.set_default_terminal_columns(i32::from(settings.terminal.default_columns));
    ui.set_default_terminal_rows(i32::from(settings.terminal.default_rows));
    ui.set_local_shell(settings.terminal.local_shell.clone().into());
    let local_shell_index = settings
        .terminal
        .known_shells
        .iter()
        .position(|shell| shell.eq_ignore_ascii_case(&settings.terminal.local_shell))
        .unwrap_or(0);
    ui.set_local_shell_index(local_shell_index.min(i32::MAX as usize) as i32);
    ui.set_credential_storage(settings.credential_storage.as_setting().into());
    ui.set_sidebar_width(i32::from(settings.workspace.sidebar_width));
    ui.set_tab_width(i32::from(settings.workspace.tab_width));
    ui.set_session_mask_character(settings.workspace.session_mask_character.clone().into());
    ui.set_collapsed_group_label_chars(i32::from(settings.workspace.collapsed_group_label_chars));
    ui.set_open_settings_shortcut(settings.shortcuts.open_settings.clone().into());
    ui.set_new_session_shortcut(settings.shortcuts.new_session.clone().into());
    ui.set_import_sessions_shortcut(settings.shortcuts.import_sessions.clone().into());
    ui.set_export_selected_shortcut(settings.shortcuts.export_selected.clone().into());
    ui.set_toggle_sidebar_shortcut(settings.shortcuts.toggle_sidebar.clone().into());
    ui.set_copy_selection_shortcut(settings.shortcuts.copy_selection.clone().into());
    ui.set_paste_shortcut(settings.shortcuts.paste.clone().into());
    ui.set_open_sftp_shortcut(settings.shortcuts.open_sftp.clone().into());
    ui.set_open_settings_menu_shortcut(menu_shortcut_keys(
        "open-settings",
        &settings.shortcuts.open_settings,
    ));
    ui.set_new_session_menu_shortcut(menu_shortcut_keys(
        "new-session",
        &settings.shortcuts.new_session,
    ));
    ui.set_import_sessions_menu_shortcut(menu_shortcut_keys(
        "import-sessions",
        &settings.shortcuts.import_sessions,
    ));
    ui.set_export_selected_menu_shortcut(menu_shortcut_keys(
        "export-selected",
        &settings.shortcuts.export_selected,
    ));
    ui.set_toggle_sidebar_menu_shortcut(menu_shortcut_keys(
        "toggle-sidebar",
        &settings.shortcuts.toggle_sidebar,
    ));
    ui.set_open_sftp_menu_shortcut(menu_shortcut_keys(
        "open-sftp",
        &settings.shortcuts.open_sftp,
    ));
    let defaults = ShortcutSettings::default();
    ui.set_default_open_settings_shortcut(defaults.open_settings.into());
    ui.set_default_new_session_shortcut(defaults.new_session.into());
    ui.set_default_import_sessions_shortcut(defaults.import_sessions.into());
    ui.set_default_export_selected_shortcut(defaults.export_selected.into());
    ui.set_default_toggle_sidebar_shortcut(defaults.toggle_sidebar.into());
    ui.set_default_copy_selection_shortcut(defaults.copy_selection.into());
    ui.set_default_paste_shortcut(defaults.paste.into());
    ui.set_default_open_sftp_shortcut(defaults.open_sftp.into());
    #[cfg(target_os = "macos")]
    schedule_macos_application_menu_configuration(ui);
}

fn menu_shortcut_keys(action: &'static str, setting: &str) -> slint::Keys {
    match menu_shortcut_from_setting(setting) {
        Ok(shortcut) => shortcut.keys,
        Err(error) => {
            warn!(action, %error, "cannot bind configured native menu shortcut");
            slint::Keys::default()
        }
    }
}

fn apply_theme_to_component(ui: &AppWindow, settings: &AppSettings) {
    let light = settings.appearance.theme.light_palette();
    let dark = settings.appearance.theme.dark_palette();
    let theme = ui.global::<Theme>();
    theme.set_application_font_family(settings.appearance.application_font_family.clone().into());
    theme.set_mode(settings.appearance.theme.mode.as_setting().into());
    theme.set_palette(settings.appearance.theme.palette.as_setting().into());
    set_theme_palette(&theme, &light, true);
    set_theme_palette(&theme, &dark, false);

    ui.set_theme_mode(settings.appearance.theme.mode.as_setting().into());
    ui.set_theme_palette(settings.appearance.theme.palette.as_setting().into());
    set_ui_theme_palette(ui, &settings.appearance.theme.custom_light, true);
    set_ui_theme_palette(ui, &settings.appearance.theme.custom_dark, false);
    ui.set_theme_revision(ui.get_theme_revision().wrapping_add(1));
}

fn set_theme_palette(theme: &Theme, palette: &ThemePalette, light: bool) {
    if light {
        theme.set_light_background(theme_color(&palette.background));
        theme.set_light_panel(theme_color(&palette.panel));
        theme.set_light_panel_alt(theme_color(&palette.panel_alt));
        theme.set_light_border(theme_color(&palette.border));
        theme.set_light_text(theme_color(&palette.text));
        theme.set_light_muted(theme_color(&palette.muted));
        theme.set_light_accent(theme_color(&palette.accent));
        theme.set_light_success(theme_color(&palette.success));
        theme.set_light_danger(theme_color(&palette.danger));
        theme.set_light_overlay(theme_color(&palette.overlay));
        theme.set_light_terminal_foreground(theme_color(&palette.terminal_foreground));
        theme.set_light_terminal_background(theme_color(&palette.terminal_background));
        theme.set_light_terminal_selection(theme_color(&palette.terminal_selection));
    } else {
        theme.set_dark_background(theme_color(&palette.background));
        theme.set_dark_panel(theme_color(&palette.panel));
        theme.set_dark_panel_alt(theme_color(&palette.panel_alt));
        theme.set_dark_border(theme_color(&palette.border));
        theme.set_dark_text(theme_color(&palette.text));
        theme.set_dark_muted(theme_color(&palette.muted));
        theme.set_dark_accent(theme_color(&palette.accent));
        theme.set_dark_success(theme_color(&palette.success));
        theme.set_dark_danger(theme_color(&palette.danger));
        theme.set_dark_overlay(theme_color(&palette.overlay));
        theme.set_dark_terminal_foreground(theme_color(&palette.terminal_foreground));
        theme.set_dark_terminal_background(theme_color(&palette.terminal_background));
        theme.set_dark_terminal_selection(theme_color(&palette.terminal_selection));
    }
}

fn set_ui_theme_palette(ui: &AppWindow, palette: &ThemePalette, light: bool) {
    if light {
        ui.set_theme_light_background(palette.background.clone().into());
        ui.set_theme_light_panel(palette.panel.clone().into());
        ui.set_theme_light_panel_alt(palette.panel_alt.clone().into());
        ui.set_theme_light_border(palette.border.clone().into());
        ui.set_theme_light_text(palette.text.clone().into());
        ui.set_theme_light_muted(palette.muted.clone().into());
        ui.set_theme_light_accent(palette.accent.clone().into());
        ui.set_theme_light_success(palette.success.clone().into());
        ui.set_theme_light_danger(palette.danger.clone().into());
        ui.set_theme_light_overlay(palette.overlay.clone().into());
        ui.set_theme_light_terminal_foreground(palette.terminal_foreground.clone().into());
        ui.set_theme_light_terminal_background(palette.terminal_background.clone().into());
        ui.set_theme_light_terminal_selection(palette.terminal_selection.clone().into());
    } else {
        ui.set_theme_dark_background(palette.background.clone().into());
        ui.set_theme_dark_panel(palette.panel.clone().into());
        ui.set_theme_dark_panel_alt(palette.panel_alt.clone().into());
        ui.set_theme_dark_border(palette.border.clone().into());
        ui.set_theme_dark_text(palette.text.clone().into());
        ui.set_theme_dark_muted(palette.muted.clone().into());
        ui.set_theme_dark_accent(palette.accent.clone().into());
        ui.set_theme_dark_success(palette.success.clone().into());
        ui.set_theme_dark_danger(palette.danger.clone().into());
        ui.set_theme_dark_overlay(palette.overlay.clone().into());
        ui.set_theme_dark_terminal_foreground(palette.terminal_foreground.clone().into());
        ui.set_theme_dark_terminal_background(palette.terminal_background.clone().into());
        ui.set_theme_dark_terminal_selection(palette.terminal_selection.clone().into());
    }
}

pub(super) fn empty_terminal_snapshot() -> TerminalSnapshot {
    TerminalSnapshot {
        text: String::new(),
        lines: vec![Default::default()],
        max_columns: 0,
        cursor_row: 0,
        cursor_column: 0,
        cursor_visible: false,
        cursor_text: " ".to_owned(),
    }
}

pub(super) fn apply_rendered_terminal(ui: &AppWindow, rendered: terminal_render::RenderedTerminal) {
    ui.set_terminal_content_columns(rendered.max_columns.min(i32::MAX as usize) as i32);
    ui.set_terminal_cursor_row(rendered.cursor_row.min(i32::MAX as usize) as i32);
    ui.set_terminal_cursor_column(rendered.cursor_column.min(i32::MAX as usize) as i32);
    ui.set_terminal_cursor_visible(rendered.cursor_visible);
    ui.set_terminal_cursor_text(rendered.cursor_text.into());
    ui.set_terminal_render_foreground(to_slint_color(rendered.foreground));
    ui.set_terminal_render_background(to_slint_color(rendered.background));
    ui.set_terminal_render_selection_background(to_slint_color(rendered.selection_background));
    let lines = rendered
        .lines
        .into_iter()
        .map(terminal_render_line)
        .collect::<Vec<_>>();
    ui.set_terminal_render_lines(ModelRc::new(VecModel::from(lines)));
}

pub(super) fn terminal_render_line(line: RenderedTerminalLine) -> TerminalRenderLine {
    let runs = line
        .runs
        .into_iter()
        .map(terminal_render_run)
        .collect::<Vec<_>>();
    TerminalRenderLine {
        runs: ModelRc::new(VecModel::from(runs)),
    }
}

pub(super) fn terminal_render_run(run: RenderedTerminalRun) -> TerminalRenderRun {
    TerminalRenderRun {
        text: run.text.into(),
        column: run.column.min(i32::MAX as usize) as i32,
        cells: run.cells.min(i32::MAX as usize) as i32,
        foreground: to_slint_color(run.foreground),
        background: to_slint_color(run.background),
        bold: run.bold,
        italic: run.italic,
        underline: run.underline,
        strikethrough: run.strikethrough,
    }
}

pub(super) fn to_slint_color(color: RgbColor) -> Color {
    Color::from_rgb_u8(color.red, color.green, color.blue)
}

fn to_rgb_color(color: Color) -> RgbColor {
    let rgba = color.to_argb_u8();
    RgbColor::new(rgba.red, rgba.green, rgba.blue)
}

fn theme_color(value: &str) -> Color {
    let value = value.trim().trim_start_matches('#');
    let fallback = Color::from_rgb_u8(23, 25, 24);
    let (red, green, blue, alpha) = match value.as_bytes() {
        [red_a, red_b, green_a, green_b, blue_a, blue_b] => (
            hex_byte(*red_a, *red_b),
            hex_byte(*green_a, *green_b),
            hex_byte(*blue_a, *blue_b),
            Some(255),
        ),
        [
            red_a,
            red_b,
            green_a,
            green_b,
            blue_a,
            blue_b,
            alpha_a,
            alpha_b,
        ] => (
            hex_byte(*red_a, *red_b),
            hex_byte(*green_a, *green_b),
            hex_byte(*blue_a, *blue_b),
            hex_byte(*alpha_a, *alpha_b),
        ),
        _ => return fallback,
    };
    match (red, green, blue, alpha) {
        (Some(red), Some(green), Some(blue), Some(alpha)) => {
            Color::from_argb_u8(alpha, red, green, blue)
        }
        _ => fallback,
    }
}

fn hex_byte(high: u8, low: u8) -> Option<u8> {
    let high = hex_digit(high)?;
    let low = hex_digit(low)?;
    Some(high * 16 + low)
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

pub(super) fn load_private_key_options(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
) {
    let generation = PRIVATE_KEY_OPTION_GENERATION
        .fetch_add(1, Ordering::AcqRel)
        .wrapping_add(1);
    runtime.spawn(async move {
        let result = tokio::task::spawn_blocking(discover_private_keys).await;
        match result {
            Ok(Ok(paths)) => {
                let options = paths
                    .into_iter()
                    .map(|path| SharedString::from(path.display().to_string()))
                    .collect::<Vec<_>>();
                dispatch_ui(&ui, move |ui| {
                    if PRIVATE_KEY_OPTION_GENERATION.load(Ordering::Acquire) != generation
                        || !state.lock().is_ok_and(|app| app.has_session_editor_tab())
                    {
                        return;
                    }
                    ui.set_private_key_options(ModelRc::new(VecModel::from(options)));
                });
            }
            Ok(Err(error)) => warn!(%error, "failed to discover local SSH private keys"),
            Err(error) => warn!(%error, "private-key discovery task failed"),
        }
    });
}

pub(super) fn load_font_options(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
) {
    runtime.spawn(async move {
        let discovery = tokio::task::spawn_blocking(discover_system_monospace_families).await;
        match discovery {
            Ok(system_families) => dispatch_ui(&ui, move |ui| {
                if !state.lock().is_ok_and(|app| app.has_settings_tab()) {
                    return;
                }
                let application_font = ui.get_application_font_family().to_string();
                let application_options = font_option_rows(&application_font, &system_families);
                let application_index =
                    font_option_index_in_slice(&application_options, &application_font);
                ui.set_application_font_options(ModelRc::new(VecModel::from(application_options)));
                ui.set_application_font_index(application_index);

                let terminal_font = ui.get_terminal_font_family().to_string();
                let terminal_options = font_option_rows(&terminal_font, &system_families);
                let terminal_index = font_option_index_in_slice(&terminal_options, &terminal_font);
                ui.set_terminal_font_options(ModelRc::new(VecModel::from(terminal_options)));
                ui.set_terminal_font_index(terminal_index);
            }),
            Err(error) => warn!(%error, "system monospace font discovery task failed"),
        }
    });
}

pub(super) fn load_x11_server_installations(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
) {
    runtime.spawn(async move {
        let discovery =
            tokio::task::spawn_blocking(ax_ssh::x_server::discovered_provider_locations);
        match tokio::time::timeout(std::time::Duration::from_secs(3), discovery).await {
            Ok(Ok(locations)) => dispatch_ui(&ui, move |ui| {
                if !state.lock().is_ok_and(|app| app.has_settings_tab()) {
                    return;
                }
                ui.set_x11_server_installations(ModelRc::new(VecModel::from(
                    locations
                        .into_iter()
                        .map(SharedString::from)
                        .collect::<Vec<_>>(),
                )));
            }),
            Ok(Err(error)) => warn!(%error, "X server location discovery task failed"),
            Err(_) => warn!("X server location discovery timed out"),
        }
    });
}

pub(super) fn load_local_shell_options(
    runtime: &Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
) {
    runtime.spawn(async move {
        let shells = match tokio::task::spawn_blocking(discover_shells).await {
            Ok(shells) => shells,
            Err(error) => {
                warn!(%error, "local-shell discovery task failed");
                return;
            }
        };
        let options = match state.lock() {
            Ok(mut app) if app.has_settings_tab() => {
                app.sessions.settings.terminal.merge_known_shells(shells);
                shell_option_rows(&app.sessions.settings)
            }
            Ok(_) => return,
            Err(_) => {
                set_status(&ui, "Cannot update local shell options");
                return;
            }
        };
        dispatch_ui(&ui, move |ui| {
            if !state.lock().is_ok_and(|app| app.has_settings_tab()) {
                return;
            }
            ui.set_local_shell_options(ModelRc::new(VecModel::from(options)));
            let selected = ui.get_local_shell().to_string();
            let index = ui
                .get_local_shell_options()
                .iter()
                .position(|shell| shell.as_str().eq_ignore_ascii_case(&selected))
                .unwrap_or(0);
            ui.set_local_shell_index(index.min(i32::MAX as usize) as i32);
        });
    });
}

pub(super) fn clear_settings_option_models(
    ui: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
) {
    let (application_font, terminal_font, shell_options) = match state.lock() {
        Ok(app) => (
            app.sessions
                .settings
                .appearance
                .application_font_family
                .clone(),
            app.sessions
                .settings
                .appearance
                .terminal_font_family
                .clone(),
            shell_option_rows(&app.sessions.settings),
        ),
        Err(_) => return,
    };
    dispatch_ui(ui, move |ui| {
        let application_options = font_option_rows(&application_font, &[]);
        let application_index = font_option_index_in_slice(&application_options, &application_font);
        ui.set_application_font_options(ModelRc::new(VecModel::from(application_options)));
        ui.set_application_font_index(application_index);

        let terminal_options = font_option_rows(&terminal_font, &[]);
        let terminal_index = font_option_index_in_slice(&terminal_options, &terminal_font);
        ui.set_terminal_font_options(ModelRc::new(VecModel::from(terminal_options)));
        ui.set_terminal_font_index(terminal_index);
        ui.set_local_shell_options(ModelRc::new(VecModel::from(shell_options)));
        ui.set_x11_server_installations(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    });
}

pub(super) fn clear_session_editor_option_models(ui: &slint::Weak<AppWindow>) {
    invalidate_private_key_option_load();
    dispatch_ui(ui, move |ui| {
        ui.set_private_key_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
        ui.set_serial_port_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    });
}

pub(super) fn clear_private_key_option_model(ui: &slint::Weak<AppWindow>) {
    invalidate_private_key_option_load();
    dispatch_ui(ui, move |ui| {
        ui.set_private_key_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    });
}

fn invalidate_private_key_option_load() {
    PRIVATE_KEY_OPTION_GENERATION.fetch_add(1, Ordering::AcqRel);
}

fn font_option_index(options: &ModelRc<SharedString>, selected: &str) -> i32 {
    font_option_index_in_slice(&options.iter().collect::<Vec<_>>(), selected)
}

fn font_option_index_in_slice(options: &[SharedString], selected: &str) -> i32 {
    options
        .iter()
        .position(|font| font.as_str().eq_ignore_ascii_case(selected))
        .unwrap_or(0)
        .min(i32::MAX as usize) as i32
}

pub(super) fn parse_uuid(value: &str, label: &str, ui: &slint::Weak<AppWindow>) -> Option<Uuid> {
    match value.parse::<Uuid>() {
        Ok(id) => Some(id),
        Err(error) => {
            set_status(ui, &format!("Invalid {label} id: {error}"));
            None
        }
    }
}

pub(super) fn set_status(ui: &slint::Weak<AppWindow>, message: &str) {
    let message = message.to_owned();
    dispatch_ui(ui, move |ui| ui.set_status(message.into()));
}

pub(super) fn dispatch_ui(
    ui: &slint::Weak<AppWindow>,
    action: impl FnOnce(&AppWindow) + Send + 'static,
) {
    let _ = dispatch_ui_result(ui, action);
}

pub(super) fn dispatch_ui_result(
    ui: &slint::Weak<AppWindow>,
    action: impl FnOnce(&AppWindow) + Send + 'static,
) -> bool {
    let ui = ui.clone();
    if slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui.upgrade() {
            action(&ui);
        }
    })
    .is_err()
    {
        debug!("Slint event loop is no longer available for UI update");
        false
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    #[test]
    fn settings_workbench_is_exposed_as_a_workspace_tab() {
        let rows = visible_workspace_tab_rows(vec![
            WorkspaceTabSummary {
                id: Uuid::new_v4(),
                title: "Settings".to_owned(),
                kind: "settings",
                connected: false,
            },
            WorkspaceTabSummary {
                id: Uuid::new_v4(),
                title: "New session".to_owned(),
                kind: "session-editor",
                connected: false,
            },
        ]);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind.as_str(), "settings");
        assert_eq!(rows[1].kind.as_str(), "session-editor");
    }

    #[test]
    fn session_group_rows_keep_profiles_nested_under_their_group() {
        let mut production_a = SessionProfile::new("prod-a", "a.example", "alice");
        production_a.group_name = " Production ".into();
        let mut production_b = SessionProfile::new("prod-b", "192.168.1.202", "zhushixin");
        production_b.group_name = "Production".into();
        let ungrouped = SessionProfile::new("local", "local.example", "carol");
        let sessions = SessionStore {
            sessions: vec![production_a, production_b, ungrouped],
            ..SessionStore::default()
        };
        let rows = session_group_rows(&sessions);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name.as_str(), "Production");
        assert_eq!(rows[0].icon.as_str(), "Pr");
        assert_eq!(rows[0].profiles.row_count(), 2);
        let production_a = rows[0].profiles.row_data(0).unwrap();
        assert_eq!(production_a.name.as_str(), "prod-a");
        assert_eq!(production_a.endpoint.as_str(), "al*ce@a.example:22");
        let production_b = rows[0].profiles.row_data(1).unwrap();
        assert_eq!(production_b.name.as_str(), "prod-b");
        assert_eq!(production_b.endpoint.as_str(), "zh*in@192.*.1.202:22");
        assert_eq!(
            production_b.details.as_str(),
            "SSH · zhushixin@192.168.1.202:22"
        );

        assert_eq!(rows[1].name.as_str(), "Ungrouped");
        assert_eq!(rows[1].icon.as_str(), "Un");
        assert_eq!(rows[1].profiles.row_count(), 1);
        assert_eq!(rows[1].profiles.row_data(0).unwrap().name.as_str(), "local");
    }

    #[test]
    fn full_group_labels_leave_server_badges_compact() {
        let mut server = SessionProfile::new("production-server", "prod.example", "alice");
        server.group_name = "Production systems".into();
        let mut sessions = SessionStore {
            sessions: vec![server],
            ..SessionStore::default()
        };
        sessions.settings.workspace.collapsed_group_label_chars = 0;

        let rows = session_group_rows(&sessions);

        assert_eq!(rows[0].icon.as_str(), "Production systems");
        assert_eq!(rows[0].profiles.row_data(0).unwrap().icon.as_str(), "pr");
    }

    #[test]
    fn empty_persistent_groups_remain_visible() {
        let sessions = SessionStore {
            groups: vec!["Empty".into()],
            ..SessionStore::default()
        };

        let rows = session_group_rows(&sessions);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name.as_str(), "Empty");
        assert_eq!(rows[0].profiles.row_count(), 0);
    }

    #[test]
    fn connection_options_include_collapsed_profiles_with_masked_endpoints() {
        let visible = SessionProfile::new("visible", "server.example", "alice");
        let hidden = SessionProfile::new("hidden", "192.168.1.202", "zhushixin");
        let sessions = SessionStore {
            sessions: vec![visible.clone(), hidden.clone()],
            ..SessionStore::default()
        };

        let options = connection_option_rows(&sessions);

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].id.as_str(), visible.id.to_string());
        assert_eq!(options[0].name.as_str(), "visible");
        assert_eq!(options[0].endpoint.as_str(), "al*ce@server.example:22");
        assert_eq!(options[1].id.as_str(), hidden.id.to_string());
        assert_eq!(options[1].name.as_str(), "hidden");
        assert_eq!(options[1].endpoint.as_str(), "zh*in@192.*.1.202:22");
    }
}
