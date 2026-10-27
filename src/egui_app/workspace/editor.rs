use super::*;
use anyhow::Result;
use ax_ssh::config::{ConnectionProfile, SessionProfile, normalize_group_name};

pub(super) struct Editor {
    pub(super) revision: u64,
    pub(super) submitted: Option<(Uuid, SessionProfile)>,
    name: String,
    group: String,
    form: ConnectionForm,
}

impl Editor {
    pub(super) fn new(profile: Option<SessionProfile>, revision: u64) -> Self {
        let profile = profile.unwrap_or_else(|| SessionProfile::new("New session", "", ""));
        Self {
            revision,
            submitted: None,
            name: profile.name.clone(),
            group: profile.group_name.clone(),
            form: ConnectionForm::from_profile(&profile),
        }
    }

    pub(super) fn profile(&self) -> Result<SessionProfile> {
        let mut profile = self.form.profile()?;
        profile.name = self.name.trim().into();
        profile.group_name = normalize_group_name(&self.group);
        profile.validate()?;
        Ok(profile)
    }
}

impl WorkspaceUi {
    pub(super) fn sidebar(
        &mut self,
        ui: &mut egui::Ui,
        view: &WorkspaceView,
        settings: &ax_ssh::config::WorkspaceSettings,
        appearance: &ax_ssh::config::AppearanceSettings,
    ) -> Option<Action> {
        // Drafts can temporarily be empty/invalid while editing. Match config's
        // saved fallback without mutating the draft or any connection profile.
        let settings =
            ax_ssh::config::WorkspaceSettings::normalized(ax_ssh::config::WorkspaceSettingsInput {
                sidebar_width: settings.sidebar_width.into(),
                tab_width: settings.tab_width.into(),
                session_mask_character: &settings.session_mask_character,
                collapsed_group_label_chars: settings.collapsed_group_label_chars.into(),
            });
        let mut action = None;
        ui.heading(tr!("Sessions", ui.ctx()));
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr!("New Connection", ui.ctx())).clicked() {
                action = Some(Action::Edit(None));
            }
            if ui.button(tr!("Group", ui.ctx())).clicked() {
                self.groups_open = true;
            }
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text(tr!("Search connections...", ui.ctx()))
                .char_limit(512),
        );
        let query = self.search.to_lowercase();
        egui::ScrollArea::vertical()
            .id_salt("session-list")
            .show(ui, |ui| {
                for group in std::iter::once("").chain(view.groups.iter().map(String::as_str)) {
                    egui::CollapsingHeader::new(if group.is_empty() {
                        tr!("Ungrouped", ui.ctx())
                    } else {
                        group
                    })
                    .id_salt(("group", group))
                    .default_open(true)
                    .show(ui, |ui| {
                        for profile in view.profiles.iter().filter(|profile| {
                            profile.group_name == group
                                && (query.is_empty()
                                    || profile.name.to_lowercase().contains(&query)
                                    || endpoint(profile).to_lowercase().contains(&query))
                        }) {
                            ui.push_id(profile.id, |ui| {
                                let image = self.icons.image(
                                    ui,
                                    super::super::system_icons::profile_icon(profile),
                                    appearance.system_icons_follow_theme,
                                );
                                let button = image.map_or_else(
                                    || egui::Button::new(&profile.name),
                                    |image| egui::Button::image_and_text(image, &profile.name),
                                );
                                let response = ui
                                    .add(button.selected(false).frame(false))
                                    .on_hover_text(endpoint(profile));
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(
                                            ax_ssh::controller::profile_sidebar_endpoint(
                                                profile,
                                                &settings.session_mask_character,
                                            ),
                                        )
                                        .small()
                                        .weak(),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(endpoint(profile));
                                if response.double_clicked() {
                                    action = Some(Action::Layout(WorkspaceAction::OpenProfile(
                                        profile.id,
                                    )));
                                }
                                response.context_menu(|ui| {
                                    if ui.button(tr!("Open saved connection", ui.ctx())).clicked() {
                                        action = Some(Action::Layout(
                                            WorkspaceAction::OpenProfile(profile.id),
                                        ));
                                        ui.close();
                                    }
                                    if ui.button(tr!("Edit", ui.ctx())).clicked() {
                                        action = Some(Action::Edit(Some(profile.id)));
                                        ui.close();
                                    }
                                    if ui.button(tr!("Duplicate", ui.ctx())).clicked() {
                                        action = Some(Action::Duplicate(profile.id));
                                        ui.close();
                                    }
                                    if ui.button(tr!("Delete Server", ui.ctx())).clicked() {
                                        self.delete = Some(profile.id);
                                        ui.close();
                                    }
                                });
                                ui.horizontal(|ui| {
                                    if ui.small_button(tr!("Open", ui.ctx())).clicked() {
                                        action = Some(Action::Layout(
                                            WorkspaceAction::OpenProfile(profile.id),
                                        ));
                                    }
                                    if ui.small_button(tr!("Edit", ui.ctx())).clicked() {
                                        action = Some(Action::Edit(Some(profile.id)));
                                    }
                                });
                            });
                        }
                    });
                }
            });
        action
    }

    pub(super) fn dialogs(&mut self, ctx: &egui::Context, view: &WorkspaceView) -> Option<Action> {
        let mut action = None;
        let mut close_editor = false;
        if let Some(editor) = &mut self.editor {
            let mut open = true;
            egui::Window::new(tr!("Saved connection", ctx)).id(egui::Id::unique("session-editor")).open(&mut open).default_width(460.0).show(ctx, |ui| {
                ui.label(tr!("Saved connection details; passwords and passphrases are entered when connecting.", ui.ctx()));
                if editor.revision != view.catalog_revision { ui.colored_label(Color32::YELLOW, tr!("Session list changed. Close and reopen this editor to use the latest version.", ui.ctx())); }
                ui.add_enabled_ui(!view.busy && editor.submitted.is_none(), |ui| {
                    ui.label(tr!("Session name", ui.ctx())); ui.add(egui::TextEdit::singleline(&mut editor.name).char_limit(128));
                    ui.label(tr!("Group (empty for Ungrouped)", ui.ctx())); ui.add(egui::TextEdit::singleline(&mut editor.group).char_limit(64));
                    editor.form.fields(ui, false);
                    if ui.button(tr!("Save session", ui.ctx())).clicked() { action = Some(Action::SaveEditor); }
                });
                if editor.submitted.is_some() { ui.label(tr!("Saving…", ui.ctx())); }
                if ui.button(tr!("Close / discard draft", ui.ctx())).clicked() { close_editor = true; }
            });
            close_editor |= !open;
        }
        if close_editor {
            self.editor = None;
        }
        egui::Window::new(tr!("Existing session groups", ctx))
            .id(egui::Id::unique("session-groups"))
            .open(&mut self.groups_open)
            .show(ctx, |ui| {
                ui.label(tr!("Existing group", ui.ctx()));
                ui.add(egui::TextEdit::singleline(&mut self.group_old).char_limit(64));
                ui.label(tr!("Group name", ui.ctx()));
                ui.add(egui::TextEdit::singleline(&mut self.group_new).char_limit(64));
                ui.add_enabled_ui(!view.busy, |ui| {
                    if ui.button(tr!("Add", ui.ctx())).clicked() {
                        action = Some(Action::Catalog(CatalogAction::AddGroup(
                            self.group_new.clone(),
                        )));
                    }
                    if ui.button(tr!("Rename group", ui.ctx())).clicked() {
                        action = Some(Action::Catalog(CatalogAction::RenameGroup {
                            old: self.group_old.clone(),
                            new: self.group_new.clone(),
                        }));
                    }
                    if ui
                        .button(tr!("Remove group (keep sessions)", ui.ctx()))
                        .clicked()
                    {
                        action = Some(Action::Catalog(CatalogAction::RemoveGroup(
                            self.group_old.clone(),
                        )));
                    }
                });
            });
        egui::Window::new(tr!("Workspace", ctx)).id(egui::Id::unique("workspace-file")).open(&mut self.file_open).show(ctx, |ui| {
            ui.label(tr!("Workspace file path", ui.ctx()));
            ui.add(egui::TextEdit::singleline(&mut self.file_path).hint_text(".axssh").char_limit(4096));
            ui.label(tr!("Open replaces the current layout after saving it. Recovered terminals wait for explicit connection.", ui.ctx()));
            ui.add_enabled_ui(!view.busy, |ui| {
                if ui.button(tr!("Open Workspace", ui.ctx())).clicked() { action = Some(Action::Layout(WorkspaceAction::OpenFile(self.file_path.trim().into()))); }
                if ui.button(tr!("Save Workspace As…", ui.ctx())).clicked() { action = Some(Action::Layout(WorkspaceAction::SaveFile(self.file_path.trim().into()))); }
            });
        });
        if let Some(id) = self.delete {
            let name = view
                .profiles
                .iter()
                .find(|profile| profile.id == id)
                .map(|profile| profile.name.as_str())
                .unwrap_or("");
            egui::Window::new(tr!("Delete saved session?", ctx))
                .id(egui::Id::unique("delete-session"))
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(name);
                    ui.label(tr!("Existing terminals stay open.", ui.ctx()));
                    if ui.button(tr!("Delete Server", ui.ctx())).clicked() {
                        action = Some(Action::Delete(id));
                    }
                    if ui.button(tr!("Cancel", ui.ctx())).clicked() {
                        self.delete = None;
                    }
                });
        }
        action
    }
}

fn endpoint(profile: &SessionProfile) -> String {
    match &profile.connection {
        ConnectionProfile::Ssh(config) => {
            format!("SSH {}@{}:{}", config.username, config.host, config.port)
        }
        ConnectionProfile::Telnet(config) => format!("Telnet {}:{}", config.host, config.port),
        ConnectionProfile::Serial(config) => {
            format!("Serial {} · {}", config.port_name, config.baud_rate)
        }
    }
}
