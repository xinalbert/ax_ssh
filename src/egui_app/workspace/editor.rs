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
    pub(super) fn sidebar(&mut self, ui: &mut egui::Ui, view: &WorkspaceView) -> Option<Action> {
        let mut action = None;
        ui.heading("Sessions");
        ui.horizontal(|ui| {
            if ui.button("New session").clicked() {
                action = Some(Action::Edit(None));
            }
            if ui.button("Groups").clicked() {
                self.groups_open = true;
            }
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text("Filter sessions")
                .char_limit(512),
        );
        let query = self.search.to_lowercase();
        egui::ScrollArea::vertical()
            .id_salt("session-list")
            .show(ui, |ui| {
                for group in std::iter::once("").chain(view.groups.iter().map(String::as_str)) {
                    egui::CollapsingHeader::new(if group.is_empty() { "Ungrouped" } else { group })
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
                                    let response = ui
                                        .selectable_label(false, &profile.name)
                                        .on_hover_text(endpoint(profile));
                                    if response.double_clicked() {
                                        action = Some(Action::Layout(
                                            WorkspaceAction::OpenProfile(profile.id),
                                        ));
                                    }
                                    response.context_menu(|ui| {
                                        if ui.button("Open terminal").clicked() {
                                            action = Some(Action::Layout(
                                                WorkspaceAction::OpenProfile(profile.id),
                                            ));
                                            ui.close();
                                        }
                                        if ui.button("Edit").clicked() {
                                            action = Some(Action::Edit(Some(profile.id)));
                                            ui.close();
                                        }
                                        if ui.button("Duplicate").clicked() {
                                            action = Some(Action::Duplicate(profile.id));
                                            ui.close();
                                        }
                                        if ui.button("Delete…").clicked() {
                                            self.delete = Some(profile.id);
                                            ui.close();
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        if ui.small_button("Open").clicked() {
                                            action = Some(Action::Layout(
                                                WorkspaceAction::OpenProfile(profile.id),
                                            ));
                                        }
                                        if ui.small_button("Edit").clicked() {
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
            egui::Window::new("Session editor").id(egui::Id::unique("session-editor")).open(&mut open).default_width(460.0).show(ctx, |ui| {
                ui.label("Saved connection details; passwords and passphrases are entered when connecting.");
                if editor.revision != view.catalog_revision { ui.colored_label(Color32::YELLOW, "Session list changed. Close and reopen this editor to use the latest version."); }
                ui.add_enabled_ui(!view.busy && editor.submitted.is_none(), |ui| {
                    ui.label("Name"); ui.add(egui::TextEdit::singleline(&mut editor.name).char_limit(128));
                    ui.label("Group (empty for Ungrouped)"); ui.add(egui::TextEdit::singleline(&mut editor.group).char_limit(64));
                    editor.form.fields(ui, false);
                    if ui.button("Save session").clicked() { action = Some(Action::SaveEditor); }
                });
                if editor.submitted.is_some() { ui.label("Saving…"); }
                if ui.button("Close / discard draft").clicked() { close_editor = true; }
            });
            close_editor |= !open;
        }
        if close_editor {
            self.editor = None;
        }
        egui::Window::new("Groups")
            .id(egui::Id::unique("session-groups"))
            .open(&mut self.groups_open)
            .show(ctx, |ui| {
                ui.label("Existing group");
                ui.add(egui::TextEdit::singleline(&mut self.group_old).char_limit(64));
                ui.label("New group name");
                ui.add(egui::TextEdit::singleline(&mut self.group_new).char_limit(64));
                ui.add_enabled_ui(!view.busy, |ui| {
                    if ui.button("Add").clicked() {
                        action = Some(Action::Catalog(CatalogAction::AddGroup(
                            self.group_new.clone(),
                        )));
                    }
                    if ui.button("Rename").clicked() {
                        action = Some(Action::Catalog(CatalogAction::RenameGroup {
                            old: self.group_old.clone(),
                            new: self.group_new.clone(),
                        }));
                    }
                    if ui.button("Remove group (keep sessions)").clicked() {
                        action = Some(Action::Catalog(CatalogAction::RemoveGroup(
                            self.group_old.clone(),
                        )));
                    }
                });
            });
        egui::Window::new("Workspace file").id(egui::Id::unique("workspace-file")).open(&mut self.file_open).show(ctx, |ui| {
            ui.label("Path to a workspace file (.axssh for export)");
            ui.add(egui::TextEdit::singleline(&mut self.file_path).char_limit(4096));
            ui.label("Open replaces the current layout after saving it. Recovered terminals wait for explicit connection.");
            ui.add_enabled_ui(!view.busy, |ui| {
                if ui.button("Open workspace").clicked() { action = Some(Action::Layout(WorkspaceAction::OpenFile(self.file_path.trim().into()))); }
                if ui.button("Save workspace as…").clicked() { action = Some(Action::Layout(WorkspaceAction::SaveFile(self.file_path.trim().into()))); }
            });
        });
        if let Some(id) = self.delete {
            let name = view
                .profiles
                .iter()
                .find(|profile| profile.id == id)
                .map(|profile| profile.name.as_str())
                .unwrap_or("this session");
            egui::Window::new("Delete saved session?")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(format!("Delete {name}? Existing terminals stay open."));
                    if ui.button("Delete session").clicked() {
                        action = Some(Action::Delete(id));
                    }
                    if ui.button("Cancel").clicked() {
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
