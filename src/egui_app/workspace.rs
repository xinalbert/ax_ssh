//! Workspace composition; only UI drafts, focus and bounded Painter caches live here.

use super::{connection::ConnectionForm, terminal::TerminalPane};
use anyhow::Result;
use ax_ssh::controller::{
    CatalogAction, PaneDirection, SftpClient, WorkspaceAction, WorkspaceClient, WorkspaceView,
};
use eframe::egui::{self, Color32, Rect, Sense, Stroke, Vec2};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use uuid::Uuid;

mod editor;
mod tabs;
use editor::Editor;

struct PaneUi {
    terminal: TerminalPane,
    connection: ConnectionForm,
    connected: bool,
    sftp: SftpPane,
}

#[derive(Default)]
struct SftpPane {
    path: String,
    local_directory: String,
    upload_source: String,
    upload_remote: String,
    secret: zeroize::Zeroizing<String>,
    icons: super::file_icons::FileIconTextures,
    edits: super::sftp_edit::EditControls,
    drops: super::sftp_drop::Drops,
    error: Option<String>,
}

pub(super) enum Action {
    Layout(WorkspaceAction),
    Edit(Option<Uuid>),
    Duplicate(Uuid),
    SaveEditor,
    Delete(Uuid),
    Catalog(CatalogAction),
}

#[derive(Default)]
pub(super) struct WorkspaceUi {
    pub(super) native_menu: bool,
    pub(super) menu_command: Option<super::menus::Command>,
    settings: Option<super::settings::SettingsUi>,
    theme: super::theme::Theme,
    last_title: Option<String>,
    sidebar_hidden: bool,
    about_open: bool,
    pub(super) native_drop_position: Option<egui::Pos2>,
    panes: HashMap<Uuid, PaneUi>,
    visible: HashSet<Uuid>,
    search: String,
    editor: Option<Editor>,
    groups_open: bool,
    file_open: bool,
    file_path: String,
    group_old: String,
    group_new: String,
    delete: Option<Uuid>,
    error: Option<String>,
    last_action_frame: Option<u64>,
    transactions: Vec<Uuid>,
    last_active: Option<Uuid>,
    last_focused: Option<Uuid>,
    focus_after_change: bool,
}

impl WorkspaceUi {
    pub(super) fn clear_caches(&mut self) {
        for pane in self.panes.values_mut() {
            pane.terminal.clear_cache();
            pane.sftp.icons.clear();
        }
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui, client: &WorkspaceClient) -> Result<()> {
        let view = client.view()?;
        let settings_id = view.tabs.iter().find(|tab| tab.settings).map(|tab| tab.id);
        if settings_id.is_some() && self.settings.is_none() {
            self.settings = Some(super::settings::SettingsUi::new(&view));
        }
        if settings_id.is_none() {
            self.settings = None;
        }
        if let Some(settings) = &mut self.settings {
            if settings.poll(client, &view)? {
                if settings.exit {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                } else if let Some(id) = settings_id {
                    client.action(WorkspaceAction::CloseTab(id))?;
                }
            }
            if ui.input(|input| input.viewport().close_requested())
                && (settings.dirty() || settings.saving())
            {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::CancelClose);
                settings.request_close(true);
            }
        }
        let effective = self
            .settings
            .as_ref()
            .map(|draft| draft.effective(&view.settings))
            .unwrap_or(&view.settings)
            .clone();
        self.theme.apply(ui.ctx(), &effective.appearance.theme);
        let title = view
            .tabs
            .iter()
            .find(|tab| Some(tab.id) == view.active_tab_id)
            .map(|tab| format!("{} — AxSSH", tab.title))
            .unwrap_or_else(|| "AxSSH".into());
        if self.last_title.as_ref() != Some(&title) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = Some(title);
        }
        for request in self.transactions.clone() {
            if let Some(result) = client.take_catalog_result(request)? {
                self.transactions.retain(|id| *id != request);
                if let Some(editor) = &mut self.editor
                    && editor
                        .submitted
                        .as_ref()
                        .is_some_and(|(id, _)| *id == request)
                {
                    editor.submitted = None;
                    if result.error.is_none() {
                        editor.revision = result.revision;
                    }
                }
                if let Some(error) = result.error {
                    self.error = Some(error);
                }
            }
        }
        let mut action = None;
        let can_act =
            !ui.is_sizing_pass() && self.last_action_frame != Some(ui.ctx().cumulative_frame_nr());
        let terminal = view
            .tabs
            .iter()
            .any(|tab| Some(tab.id) == view.active_tab_id && !tab.settings);
        let blocked = self.blocked(&view);
        let enabled = |cmd: super::menus::Command| {
            cmd.enabled(view.ready, blocked, terminal, view.tabs.len())
        };
        let mut command = None;
        if can_act {
            command = self
                .menu_command
                .take()
                .filter(|cmd| enabled(*cmd))
                .or_else(|| super::menus::keyboard(ui, &view.settings, self.native_menu, enabled));
        }
        egui::Panel::top("workspace-tabs").show(ui, |ui| {
            if !self.native_menu
                && let Some(cmd) = super::menus::bar(ui, &view.settings, enabled)
            {
                command = Some(cmd);
            }
            ui.add_enabled_ui(!blocked && view.ready, |ui| {
                if let Some(next) = self.tabs(ui, &view, f32::from(effective.workspace.tab_width)) {
                    action = Some(next);
                }
            });
            if let Some(error) = self.error.as_ref().or(view.error.as_ref()) {
                ui.colored_label(Color32::LIGHT_RED, error);
            }
        });
        if can_act && let Some(command) = command {
            action = self.command(command, ui, &view)?;
            if action.is_none() {
                self.last_action_frame = Some(ui.ctx().cumulative_frame_nr());
            }
        }
        if !self.sidebar_hidden {
            egui::Panel::left("saved-sessions")
                .exact_size(f32::from(effective.workspace.sidebar_width))
                .resizable(false)
                .show(ui, |ui| {
                    ui.add_enabled_ui(view.ready && !blocked, |ui| {
                        if let Some(next) = self.sidebar(ui, &view) {
                            action = Some(next);
                        }
                    });
                });
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(ui.style()))
            .show(ui, |ui| -> Result<()> {
                if !view.ready {
                    ui.label(&view.status);
                    return Ok(());
                }
                let Some(tab) = view
                    .tabs
                    .iter()
                    .find(|tab| Some(tab.id) == view.active_tab_id)
                else {
                    ui.label("Open a local shell or saved connection to start.");
                    return Ok(());
                };
                if tab.settings {
                    for pane in self.panes.values_mut() {
                        pane.terminal.suspend()?;
                        pane.terminal.surrender_focus(ui.ctx());
                        pane.sftp.icons.clear();
                    }
                    self.visible.clear();
                    self.last_active = view.active_tab_id;
                    if let Some(settings) = &mut self.settings {
                        settings.show(ui, client, &view);
                    }
                    return Ok(());
                }
                let focused = tab
                    .layout
                    .panes
                    .iter()
                    .find(|pane| pane.focused)
                    .map(|pane| pane.tab_id);
                let moved = self.last_active != view.active_tab_id || self.last_focused != focused;
                let request_focus =
                    moved && (self.focus_after_change || self.last_active.is_none());
                if request_focus {
                    for pane in self.panes.values() {
                        pane.terminal.surrender_focus(ui.ctx());
                    }
                }
                if moved {
                    self.focus_after_change = false;
                }
                self.last_active = view.active_tab_id;
                self.last_focused = focused;
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Split columns").clicked() {
                        action = Some(Action::Layout(WorkspaceAction::Split(PaneDirection::Right)));
                    }
                    if ui.button("Split rows").clicked() {
                        action = Some(Action::Layout(WorkspaceAction::Split(PaneDirection::Down)));
                    }
                    for (label, direction) in [
                        ("←", PaneDirection::Left),
                        ("↑", PaneDirection::Up),
                        ("↓", PaneDirection::Down),
                        ("→", PaneDirection::Right),
                    ] {
                        if ui.button(label).on_hover_text("Focus pane").clicked() {
                            action =
                                Some(Action::Layout(WorkspaceAction::FocusDirection(direction)));
                        }
                    }
                    if ui.button("Close pane").clicked()
                        && let Some(id) = focused
                    {
                        action = Some(Action::Layout(WorkspaceAction::ClosePane(id)));
                    }
                });
                let base = ui.available_rect_before_wrap();
                ui.allocate_rect(base, Sense::hover());
                let visible: HashSet<_> = tab.panes.iter().map(|pane| pane.id).collect();
                for id in self.visible.difference(&visible) {
                    if let Some(pane) = self.panes.get_mut(id) {
                        pane.terminal.suspend()?;
                        pane.sftp.icons.clear();
                    }
                }
                self.visible = visible;
                let blocked = view.busy
                    || self.editor.is_some()
                    || self.groups_open
                    || self.file_open
                    || self.delete.is_some()
                    || self
                        .settings
                        .as_ref()
                        .is_some_and(|settings| settings.confirm_close);
                for placement in &tab.layout.panes {
                    let Some(metadata) = tab.panes.iter().find(|pane| pane.id == placement.tab_id)
                    else {
                        continue;
                    };
                    let Some(terminal_client) = client.terminal(metadata.id)? else {
                        continue;
                    };
                    let terminal_view = terminal_client.view()?;
                    let pane = self.panes.entry(metadata.id).or_insert_with(|| PaneUi {
                        terminal: TerminalPane::default(),
                        connection: metadata
                            .profile
                            .as_ref()
                            .map(ConnectionForm::from_profile)
                            .unwrap_or_else(|| {
                                if metadata.local {
                                    ConnectionForm::local()
                                } else {
                                    ConnectionForm::default()
                                }
                            }),
                        connected: false,
                        sftp: SftpPane::default(),
                    });
                    pane.terminal.configure(&effective.appearance);
                    let rect = Rect::from_min_size(
                        base.min
                            + Vec2::new(placement.x * base.width(), placement.y * base.height()),
                        Vec2::new(
                            placement.width * base.width(),
                            placement.height * base.height(),
                        ),
                    )
                    .shrink(2.0);
                    if rect.width() < 12.0 || rect.height() < 12.0 {
                        pane.terminal.suspend()?;
                        continue;
                    }
                    ui.painter().rect_stroke(
                        rect,
                        0.0,
                        Stroke::new(
                            1.0,
                            if placement.focused {
                                Color32::LIGHT_BLUE
                            } else {
                                Color32::DARK_GRAY
                            },
                        ),
                        egui::StrokeKind::Inside,
                    );
                    if ui.input(|input| input.pointer.any_pressed())
                        && ui.rect_contains_pointer(rect)
                        && !blocked
                    {
                        action = Some(Action::Layout(WorkspaceAction::Focus(metadata.id)));
                    }
                    let focus = placement.focused
                        && !blocked
                        && ((request_focus && (metadata.local || terminal_view.connected))
                            || (terminal_view.connected && !pane.connected));
                    pane.connected = terminal_view.connected;
                    let result = ui
                        .scope_builder(
                            egui::UiBuilder::new()
                                .id_salt(metadata.id)
                                .max_rect(rect.shrink(3.0))
                                .layout(egui::Layout::top_down(egui::Align::Min)),
                            |ui| -> Result<()> {
                                ui.set_clip_rect(rect);
                                ui.label(format!("{} · {}", metadata.title, terminal_view.status));
                                ui.add_enabled_ui(!blocked, |ui| -> Result<()> {
                                    pane.connection.show(ui, &terminal_client, &terminal_view)?;
                                    if metadata.sftp
                                        && let Some(sftp_client) = client.sftp(metadata.id)?
                                    {
                                        pane.sftp.show(
                                            ui,
                                            &sftp_client,
                                            metadata.profile.as_ref(),
                                            metadata.id,
                                            self.native_drop_position,
                                        )?;
                                    }
                                    if terminal_view.prompt.is_some() {
                                        pane.terminal.suspend()?;
                                        return Ok(());
                                    }
                                    pane.terminal.show_with_focus(
                                        ui,
                                        &terminal_client,
                                        &terminal_view,
                                        focus,
                                    )
                                })
                                .inner
                            },
                        )
                        .inner;
                    result?;
                    if pane.terminal.focused() && !placement.focused {
                        action = Some(Action::Layout(WorkspaceAction::Focus(metadata.id)));
                    }
                }
                for divider in &tab.layout.dividers {
                    let region = Rect::from_min_size(
                        base.min + Vec2::new(divider.x * base.width(), divider.y * base.height()),
                        Vec2::new(divider.width * base.width(), divider.height * base.height()),
                    );
                    let center = if divider.vertical {
                        egui::pos2(
                            region.left() + region.width() * divider.ratio,
                            region.center().y,
                        )
                    } else {
                        egui::pos2(
                            region.center().x,
                            region.top() + region.height() * divider.ratio,
                        )
                    };
                    let handle = Rect::from_center_size(
                        center,
                        if divider.vertical {
                            Vec2::new(5.0, region.height())
                        } else {
                            Vec2::new(region.width(), 5.0)
                        },
                    );
                    let response = ui
                        .interact(
                            handle,
                            ui.scope_id().with((tab.id, divider.id)),
                            Sense::drag(),
                        )
                        .on_hover_cursor(if divider.vertical {
                            egui::CursorIcon::ResizeHorizontal
                        } else {
                            egui::CursorIcon::ResizeVertical
                        });
                    if !blocked
                        && response.dragged()
                        && let Some(pos) = response.interact_pointer_pos()
                    {
                        let ratio = if divider.vertical {
                            (pos.x - region.left()) / region.width().max(1.0)
                        } else {
                            (pos.y - region.top()) / region.height().max(1.0)
                        };
                        action = Some(Action::Layout(WorkspaceAction::ResizeSplit {
                            tab: tab.id,
                            divider: divider.id,
                            ratio,
                        }));
                    }
                }
                Ok(())
            })
            .inner?;
        self.panes.retain(|id, _| {
            view.tabs
                .iter()
                .any(|tab| tab.panes.iter().any(|pane| pane.id == *id))
        });
        if let Some(settings) = &mut self.settings {
            settings.close_dialog(ui.ctx(), client, &view);
        }
        if self.about_open {
            egui::Window::new("About AxSSH")
                .open(&mut self.about_open)
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.heading("AxSSH");
                    ui.label(format!("{} · egui 0.37.0", env!("CARGO_PKG_VERSION")));
                    ui.hyperlink_to("Source and licenses", "https://github.com/xinalbert/ax_ssh");
                });
        }
        if let Some(next) = self.dialogs(ui.ctx(), &view) {
            action = Some(next);
        }
        if can_act && let Some(action) = action {
            self.last_action_frame = Some(ui.ctx().cumulative_frame_nr());
            self.error = self
                .apply(action, client, &view)
                .err()
                .map(|error| format!("{error:#}"));
        }
        Ok(())
    }

    pub(super) fn blocked(&self, view: &WorkspaceView) -> bool {
        view.busy
            || self.editor.is_some()
            || self.groups_open
            || self.file_open
            || self.delete.is_some()
            || self
                .settings
                .as_ref()
                .is_some_and(|settings| settings.confirm_close)
    }

    fn command(
        &mut self,
        command: super::menus::Command,
        ui: &mut egui::Ui,
        view: &WorkspaceView,
    ) -> Result<Option<Action>> {
        use super::menus::Command;
        let layout = match command {
            Command::NewProfile => return Ok(Some(Action::Edit(None))),
            Command::NewConnection => WorkspaceAction::NewConnection,
            Command::NewLocal => WorkspaceAction::NewLocal,
            Command::SaveWorkspace => WorkspaceAction::SaveNow,
            Command::Settings => WorkspaceAction::OpenSettings,
            Command::SplitRight => WorkspaceAction::Split(PaneDirection::Right),
            Command::SplitDown => WorkspaceAction::Split(PaneDirection::Down),
            Command::PreviousTab => WorkspaceAction::Cycle(-1),
            Command::NextTab => WorkspaceAction::Cycle(1),
            Command::CloseTab => {
                let Some(id) = view.active_tab_id else {
                    return Ok(None);
                };
                WorkspaceAction::CloseTab(id)
            }
            Command::WorkspaceFile => {
                self.file_open = true;
                return Ok(None);
            }
            Command::ToggleSidebar => {
                self.sidebar_hidden = !self.sidebar_hidden;
                return Ok(None);
            }
            Command::About => {
                self.about_open = true;
                return Ok(None);
            }
            Command::Quit => {
                if let Some(settings) = &mut self.settings {
                    settings.request_close(true);
                } else {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                return Ok(None);
            }
            Command::Copy | Command::Paste => {
                ui.ctx().send_viewport_cmd(if command == Command::Copy {
                    egui::ViewportCommand::RequestCopy
                } else {
                    egui::ViewportCommand::RequestPaste
                });
                return Ok(None);
            }
        };
        Ok(Some(Action::Layout(layout)))
    }

    fn apply(
        &mut self,
        action: Action,
        client: &WorkspaceClient,
        view: &WorkspaceView,
    ) -> Result<()> {
        match action {
            Action::Layout(action) => {
                if matches!(action, WorkspaceAction::OpenFile(_)) {
                    anyhow::ensure!(
                        self.settings
                            .as_ref()
                            .is_none_or(|settings| !settings.dirty() && !settings.saving()),
                        "save or discard settings before opening another workspace"
                    );
                }
                if let WorkspaceAction::CloseTab(id) = action
                    && view.tabs.iter().any(|tab| tab.id == id && tab.settings)
                    && let Some(settings) = &mut self.settings
                {
                    settings.request_close(false);
                    return Ok(());
                }
                self.focus_after_change = matches!(
                    action,
                    WorkspaceAction::Activate(_)
                        | WorkspaceAction::Cycle(_)
                        | WorkspaceAction::NewLocal
                        | WorkspaceAction::CloseTab(_)
                        | WorkspaceAction::ClosePane(_)
                        | WorkspaceAction::Split(_)
                        | WorkspaceAction::FocusDirection(_)
                );
                if matches!(
                    action,
                    WorkspaceAction::OpenFile(_) | WorkspaceAction::SaveFile(_)
                ) {
                    self.file_open = false;
                }
                client.action(action)?;
            }
            Action::Edit(id) => {
                self.editor = Some(Editor::new(
                    id.and_then(|id| view.profiles.iter().find(|profile| profile.id == id))
                        .cloned(),
                    view.catalog_revision,
                ))
            }
            Action::Duplicate(id) => {
                if let Some(mut profile) = view
                    .profiles
                    .iter()
                    .find(|profile| profile.id == id)
                    .cloned()
                {
                    profile.id = Uuid::new_v4();
                    profile.name = format!("{} copy", profile.name).chars().take(128).collect();
                    self.editor = Some(Editor::new(Some(profile), view.catalog_revision));
                }
            }
            Action::SaveEditor => {
                if let Some(editor) = &mut self.editor {
                    let profile = editor.profile()?;
                    let request = client.catalog(
                        editor.revision,
                        CatalogAction::Upsert(Box::new(profile.clone())),
                    )?;
                    self.transactions.push(request);
                    editor.submitted = Some((request, profile));
                }
            }
            Action::Delete(id) => {
                self.delete = None;
                self.transactions
                    .push(client.catalog(view.catalog_revision, CatalogAction::Delete(id))?);
            }
            Action::Catalog(action) => self
                .transactions
                .push(client.catalog(view.catalog_revision, action)?),
        }
        Ok(())
    }
}

impl SftpPane {
    fn show(
        &mut self,
        ui: &mut egui::Ui,
        client: &SftpClient,
        profile: Option<&ax_ssh::config::SessionProfile>,
        pane_id: Uuid,
        native_position: Option<egui::Pos2>,
    ) -> Result<()> {
        let view = client.view()?;
        let client = client.for_view(&view);
        let mut edit_action = None;
        self.icons.sync(ui.ctx(), &view.icons);
        if self.path.is_empty() && !view.home.is_empty() {
            self.path = view.home.clone();
        }
        if self.local_directory.is_empty() {
            self.local_directory = ".".into();
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong("SFTP companion");
            if ui.button("Connect").clicked()
                && let Some(profile) = profile.cloned()
            {
                let secret =
                    std::mem::replace(&mut self.secret, zeroize::Zeroizing::new(String::new()));
                if let Err(error) = client.connect(profile, secret) {
                    self.error = Some(format!("{error:#}"));
                }
            }
            if ui.button("Disconnect").clicked() {
                let _ = client.disconnect();
            }
        });
        if !view.connected {
            ui.horizontal(|ui| {
                ui.label("Password / key passphrase");
                let mut output = egui::TextEdit::singleline(&mut *self.secret)
                    .password(true)
                    .char_limit(16 * 1024)
                    .show(ui);
                output.state.clear_undoer();
                output.state.store(ui.ctx(), output.response.id);
            });
        }
        if let Some(prompt) = &view.prompt {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!(
                    "SSH prompt for {}@{}:{}",
                    prompt.username, prompt.host, prompt.port
                ),
            );
            if matches!(
                prompt.kind,
                ax_ssh::controller::TerminalPromptKind::HostKey {
                    confirmable: false,
                    ..
                }
            ) {
                ui.label("Host key rejected by trust policy");
            } else if ui.button("Retry with fresh credentials").clicked() {
                let secret =
                    std::mem::replace(&mut self.secret, zeroize::Zeroizing::new(String::new()));
                if let Err(error) = client.respond(prompt.id, secret) {
                    self.error = Some(format!("{error:#}"));
                }
            }
        }
        ui.label(&view.status);
        ui.horizontal(|ui| {
            ui.label("Remote path");
            ui.add(egui::TextEdit::singleline(&mut self.path).char_limit(4096));
            if ui.button("List").clicked() {
                let path = if self.path.trim().is_empty() {
                    view.home.clone()
                } else {
                    self.path.trim().to_owned()
                };
                if let Err(error) = client.list(path) {
                    self.error = Some(format!("{error:#}"));
                }
            }
            if view.has_more && ui.button("More").clicked() {
                let _ = client.load_more();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Download directory");
            ui.add(egui::TextEdit::singleline(&mut self.local_directory).char_limit(4096));
            if ui.button("Browse local").clicked()
                && let Err(error) = client.list_local(PathBuf::from(self.local_directory.trim()))
            {
                self.error = Some(format!("{error:#}"));
            }
        });
        let local_response = egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label("Local files · drop remote entries here to download");
            if view.local_loading {
                ui.label("Reading local directory…");
            }
            ui.label(&view.local_path);
            egui::ScrollArea::vertical()
                .id_salt("local-files")
                .max_height(140.0)
                .show(ui, |ui| {
                    for entry in &view.local_entries {
                        ui.horizontal(|ui| {
                            self.icons.show(ui, &entry.icon);
                            super::sftp_drop::Drops::source(
                                ui,
                                pane_id,
                                &view,
                                &entry.path,
                                &entry.name,
                                true,
                            );
                            if entry.is_dir
                                && ui.small_button("Open").clicked()
                                && let Err(error) = client.list_local(entry.path.clone().into())
                            {
                                self.error = Some(format!("{error:#}"));
                            }
                            if !entry.is_dir && ui.small_button("Use upload").clicked() {
                                self.upload_source = entry.path.clone();
                                self.upload_remote =
                                    format!("{}/{}", view.path.trim_end_matches('/'), entry.name);
                            }
                        });
                    }
                });
        });
        if let Err(error) = self.drops.target(
            ui,
            &local_response.response,
            pane_id,
            &view,
            &client,
            super::sftp_drop::Target {
                remote: false,
                native_position: None,
            },
        ) {
            self.error = Some(format!("{error:#}"));
        }
        ui.horizontal(|ui| {
            ui.label("Upload file");
            ui.add(egui::TextEdit::singleline(&mut self.upload_source).char_limit(4096));
            ui.label("Remote path");
            ui.add(egui::TextEdit::singleline(&mut self.upload_remote).char_limit(4096));
            if ui.button("Upload").clicked()
                && let Err(error) = client.upload(
                    PathBuf::from(self.upload_source.trim()),
                    self.upload_remote.trim().to_owned(),
                )
            {
                self.error = Some(format!("{error:#}"));
            }
        });
        let remote_response = egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label("Remote files · drop local files or folders here to upload");
            egui::ScrollArea::vertical()
                .id_salt("remote-files")
                .max_height(180.0)
                .show(ui, |ui| {
                    for entry in &view.entries {
                        ui.horizontal(|ui| {
                            self.icons.show(ui, &entry.icon);
                            super::sftp_drop::Drops::source(
                                ui,
                                pane_id,
                                &view,
                                &entry.path,
                                &entry.name,
                                false,
                            );
                            if ui
                                .add_enabled(
                                    view.connected
                                        && !view.loading
                                        && !entry.is_dir
                                        && !entry.is_symlink,
                                    egui::Button::new("Edit externally"),
                                )
                                .clicked()
                            {
                                edit_action =
                                    Some(super::sftp_edit::EditAction::Open(entry.path.clone()));
                            }
                            if !entry.is_dir
                                && ui.small_button("Download").clicked()
                                && let Err(error) = client.download(
                                    entry.path.clone(),
                                    self.local_directory.clone().into(),
                                )
                            {
                                self.error = Some(format!("{error:#}"));
                            }
                        });
                    }
                });
        });
        if let Err(error) = self.drops.target(
            ui,
            &remote_response.response,
            pane_id,
            &view,
            &client,
            super::sftp_drop::Target {
                remote: true,
                native_position,
            },
        ) {
            self.error = Some(format!("{error:#}"));
        }
        if view.upload_preparing
            && ui.button("Cancel upload preparation").clicked()
            && let Err(error) = client.cancel_upload_preparation()
        {
            self.error = Some(format!("{error:#}"));
        }
        if let Some(action) = super::sftp_edit::EditControls::show(ui, &view) {
            edit_action = Some(action);
        }
        if let Some(action) = edit_action
            && let Err(error) = self.edits.apply(
                ui.ctx().cumulative_frame_nr(),
                ui.is_sizing_pass(),
                action,
                &client,
            )
        {
            self.error = Some(format!("{error:#}"));
        }
        for transfer in &view.transfers {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} · {} / {} · {}",
                    transfer.name, transfer.downloaded_bytes, transfer.total_bytes, transfer.status
                ));
                if transfer.conflict {
                    if ui.small_button("Overwrite").clicked() {
                        let _ = client.resolve_conflict(
                            transfer.id,
                            ax_ssh::sftp::SftpUploadConflictChoice::Overwrite,
                        );
                    }
                    if ui.small_button("Skip").clicked() {
                        let _ = client.resolve_conflict(
                            transfer.id,
                            ax_ssh::sftp::SftpUploadConflictChoice::Skip,
                        );
                    }
                }
                if ui.small_button("Cancel").clicked() {
                    let _ = client.cancel(transfer.id);
                }
            });
        }
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
