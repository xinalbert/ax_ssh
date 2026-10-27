//! Workspace composition; only UI drafts, focus and bounded Painter caches live here.

use super::i18n::tr;
use super::{connection::ConnectionForm, terminal::TerminalPane};
use anyhow::Result;
use ax_ssh::controller::{
    CatalogAction, PaneDirection, WorkspaceAction, WorkspaceClient, WorkspaceView,
};
use eframe::egui::{self, Color32, Rect, Sense, Stroke, Vec2};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

mod editor;
mod sftp;
mod tabs;
mod transfer;
use editor::Editor;

struct PaneUi {
    terminal: TerminalPane,
    connection: ConnectionForm,
    connected: bool,
    sftp: SftpPane,
}

#[derive(Default)]
struct SftpPane {
    generation: Option<u64>,
    prompt_id: Option<Uuid>,
    last_action_frame: Option<u64>,
    path: String,
    focus_requested: bool,
    local_directory: String,
    upload_source: String,
    upload_remote: String,
    secret: zeroize::Zeroizing<String>,
    icons: super::file_icons::FileIconTextures,
    edits: super::sftp_edit::EditControls,
    drops: super::sftp_drop::Drops,
    transfers: sftp::TransferControls,
    remote_selection: sftp::RemoteSelection,
    local_selection: sftp::LocalSelection,
    entry_dialog: Option<sftp::EntryDialog>,
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
    icons: super::system_icons::Icons,
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
    sftp_focus: Option<Uuid>,
    transfer: Option<transfer::TransferUi>,
}

impl WorkspaceUi {
    pub(super) fn font_appearance<'a>(
        &'a self,
        saved: &'a ax_ssh::config::AppSettings,
    ) -> &'a ax_ssh::config::AppearanceSettings {
        &self
            .settings
            .as_ref()
            .map_or(saved, |draft| draft.effective(saved))
            .appearance
    }

    pub(super) fn permits_paste(&self, view: &WorkspaceView, owner: egui::Id) -> bool {
        !self.blocked(view)
            && self.menu_command.is_none()
            && view
                .tabs
                .iter()
                .find(|tab| Some(tab.id) == view.active_tab_id)
                .filter(|tab| !tab.settings)
                .and_then(|tab| tab.layout.panes.iter().find(|pane| pane.focused))
                .and_then(|pane| self.panes.get(&pane.tab_id))
                .is_some_and(|pane| pane.terminal.owns_widget(owner))
    }

    #[cfg(target_os = "macos")]
    pub(super) fn terminal_keyboard_active(
        &self,
        ctx: &egui::Context,
        view: &WorkspaceView,
    ) -> bool {
        ctx.memory(|memory| memory.focused())
            .is_some_and(|owner| self.permits_paste(view, owner))
    }

    pub(super) fn clear_caches(&mut self) {
        self.icons.clear();
        if let Some(settings) = &mut self.settings {
            settings.clear_caches();
        }
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
        let mut effective = self
            .settings
            .as_ref()
            .map(|draft| draft.effective(&view.settings))
            .unwrap_or(&view.settings)
            .clone();
        effective.appearance.theme = ax_ssh::config::ThemeSettings::normalized(
            effective.appearance.theme.mode.as_setting(),
            effective.appearance.theme.palette.as_setting(),
            effective.appearance.theme.custom_light.clone(),
            effective.appearance.theme.custom_dark.clone(),
        );
        self.theme.apply(ui.ctx(), &effective.appearance.theme);
        let title = view
            .tabs
            .iter()
            .find(|tab| Some(tab.id) == view.active_tab_id)
            .map(|tab| {
                format!(
                    "{} — AxSSH",
                    if tab.settings {
                        tr!("Settings", ui.ctx())
                    } else {
                        &tab.title
                    }
                )
            })
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
        let blocked = self.blocked(&view);
        let enabled = |cmd: super::menus::Command| cmd.enabled_for_view(&view, blocked);
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
                if let Some(next) = self.tabs(
                    ui,
                    &view,
                    f32::from(effective.workspace.tab_width),
                    &effective.appearance,
                ) {
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
        let sftp_focus = if ui.is_sizing_pass() {
            None
        } else {
            self.sftp_focus.take().filter(|id| {
                view.ready && !blocked && super::menus::sftp_target(&view) == Some(*id)
            })
        };
        if !self.sidebar_hidden {
            egui::Panel::left("saved-sessions")
                .exact_size(f32::from(effective.workspace.sidebar_width))
                .resizable(false)
                .show(ui, |ui| {
                    ui.add_enabled_ui(view.ready && !blocked, |ui| {
                        if let Some(next) =
                            self.sidebar(ui, &view, &effective.workspace, &effective.appearance)
                        {
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
                    ui.label(tr!("Open a saved session or create a new tab.", ui.ctx()));
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
                    if ui.button(tr!("Split Columns", ui.ctx())).clicked() {
                        action = Some(Action::Layout(WorkspaceAction::Split(PaneDirection::Right)));
                    }
                    if ui.button(tr!("Split Rows", ui.ctx())).clicked() {
                        action = Some(Action::Layout(WorkspaceAction::Split(PaneDirection::Down)));
                    }
                    for (label, direction) in [
                        ("←", PaneDirection::Left),
                        ("↑", PaneDirection::Up),
                        ("↓", PaneDirection::Down),
                        ("→", PaneDirection::Right),
                    ] {
                        if ui
                            .button(label)
                            .on_hover_text(tr!("Focus pane", ui.ctx()))
                            .clicked()
                        {
                            action =
                                Some(Action::Layout(WorkspaceAction::FocusDirection(direction)));
                        }
                    }
                    if ui.button(tr!("Close pane", ui.ctx())).clicked()
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
                let blocked = self.blocked(&view);
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
                    pane.terminal
                        .configure_interaction(&view.settings.appearance);
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
                        && sftp_focus != Some(metadata.id)
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
                                        pane.sftp.focus_requested = sftp_focus == Some(metadata.id);
                                        pane.sftp.show(
                                            ui,
                                            &sftp_client,
                                            metadata.profile.as_ref(),
                                            metadata.id,
                                            self.native_drop_position,
                                        )?;
                                    }
                                    if pane.sftp.entry_dialog.is_some() {
                                        pane.terminal.suspend()?;
                                        return Ok(());
                                    }
                                    if terminal_view.prompt.is_some() {
                                        pane.terminal.suspend()?;
                                        return Ok(());
                                    }
                                    pane.terminal.remote_clipboard(
                                        ui,
                                        &terminal_client,
                                        &terminal_view,
                                        view.settings.terminal.osc52_clipboard,
                                    )?;
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
        self.entry_dialogs(ui, client, &view)?;
        self.transfer_dialog(ui, client, &view)?;
        if let Some(settings) = &mut self.settings {
            settings.close_dialog(ui.ctx(), client, &view);
        }
        if self.about_open {
            egui::Window::new(tr!("About AxSSH", ui.ctx()))
                .id(egui::Id::unique("about-dialog"))
                .open(&mut self.about_open)
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.heading("AxSSH");
                    ui.label(format!("{} · egui 0.37.0", env!("CARGO_PKG_VERSION")));
                    ui.hyperlink_to(
                        tr!("Source and licenses", ui.ctx()),
                        "https://github.com/xinalbert/ax_ssh",
                    );
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
        self.panes
            .values()
            .any(|pane| pane.sftp.entry_dialog.is_some())
            || view.busy
            || self.transfer.is_some()
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
            Command::ImportSessions => {
                self.transfer = Some(transfer::TransferUi::import(view));
                return Ok(None);
            }
            Command::ExportSelected => {
                self.transfer = Some(transfer::TransferUi::export(view));
                return Ok(None);
            }
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
            Command::OpenSftp => {
                self.sftp_focus = super::menus::sftp_target(view);
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
            Command::Paste => {
                if let Some(id) = view
                    .tabs
                    .iter()
                    .find(|tab| Some(tab.id) == view.active_tab_id)
                    .and_then(|tab| tab.layout.panes.iter().find(|pane| pane.focused))
                    .map(|pane| pane.tab_id)
                    && let Some(pane) = self.panes.get(&id)
                {
                    pane.terminal.request_paste(ui.ctx())?;
                }
                return Ok(None);
            }
            Command::Copy => {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestCopy);
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

#[cfg(test)]
mod tests;
