//! SFTP composition and bounded frontend intent admission; no filesystem or transport work.
use super::*;
use ax_ssh::controller::SftpClient;
use std::path::PathBuf;

mod local_selection;
pub(super) use local_selection::LocalSelection;
mod remote_selection;
pub(super) use remote_selection::RemoteSelection;
mod entry_actions;
pub(super) use entry_actions::EntryDialog;
mod transfer_controls;
pub(super) use transfer_controls::TransferControls;

impl SftpPane {
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        client: &SftpClient,
        profile: Option<&ax_ssh::config::SessionProfile>,
        pane_id: Uuid,
        native_position: Option<egui::Pos2>,
    ) -> Result<()> {
        let view = client.view()?;
        ui.push_id("sftp-companion", |ui| {
            self.show_view(ui, client, &view, profile, pane_id, native_position)
        })
        .inner
    }

    fn show_view(
        &mut self,
        ui: &mut egui::Ui,
        client: &SftpClient,
        view: &ax_ssh::controller::SftpView,
        profile: Option<&ax_ssh::config::SessionProfile>,
        pane_id: Uuid,
        native_position: Option<egui::Pos2>,
    ) -> Result<()> {
        let prompt_id = view.prompt.as_ref().map(|prompt| prompt.id);
        if self.generation != Some(view.generation) || self.prompt_id != prompt_id {
            self.secret = zeroize::Zeroizing::new(String::new());
            self.generation = Some(view.generation);
            self.prompt_id = prompt_id;
        }
        let client = client.for_view(view);
        let mut edit_action = None;
        self.icons.sync(ui.ctx(), &view.icons);
        if self.path.is_empty() && !view.home.is_empty() {
            self.path = view.home.clone();
        }
        if self.local_directory.is_empty() {
            self.local_directory = ".".into();
        }
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.strong(tr!("SFTP", ui.ctx()));
            if ui
                .add_enabled(
                    profile.is_some() && view.prompt.is_none(),
                    egui::Button::new(tr!("Connect", ui.ctx())),
                )
                .clicked()
                && self.admit(ui)
                && let Some(profile) = profile.cloned()
            {
                let secret =
                    std::mem::replace(&mut self.secret, zeroize::Zeroizing::new(String::new()));
                if let Err(error) = client.connect(profile, secret) {
                    self.error = Some(format!("{error:#}"));
                }
            }
            if ui.button(tr!("Disconnect", ui.ctx())).clicked()
                && self.admit(ui)
                && let Err(error) = client.disconnect()
            {
                self.error = Some(format!("{error:#}"));
            }
        });
        if let Some(prompt) = &view.prompt {
            if let Some(action) = super::super::connection::ssh_prompt(ui, prompt, &mut self.secret)
                && self.admit(ui)
            {
                let result = match action {
                    super::super::connection::PromptAction::Respond(id) => {
                        let secret = std::mem::replace(
                            &mut self.secret,
                            zeroize::Zeroizing::new(String::new()),
                        );
                        client.respond(id, secret)
                    }
                    super::super::connection::PromptAction::Disconnect => client.disconnect(),
                };
                if let Err(error) = result {
                    self.error = Some(format!("{error:#}"));
                }
            }
        } else if !view.connected {
            ui.horizontal_wrapped(|ui| {
                ui.label(tr!("Password / passphrase", ui.ctx()));
                super::super::connection::secret_edit(ui, &mut self.secret);
            });
        }
        ui.label(&view.status);
        ui.horizontal_wrapped(|ui| {
            let label = ui.label(tr!("Remote path", ui.ctx()));
            let path = ui
                .add(
                    egui::TextEdit::singleline(&mut self.path)
                        .id(ui.make_persistent_id("remote-path"))
                        .char_limit(4096),
                )
                .labelled_by(label.id);
            if !ui.is_sizing_pass() && std::mem::take(&mut self.focus_requested) && ui.is_enabled()
            {
                path.request_focus();
                path.scroll_to_me(Some(egui::Align::Center));
            }
            if ui.button(tr!("List", ui.ctx())).clicked() && self.admit(ui) {
                let path = if self.path.trim().is_empty() {
                    view.home.clone()
                } else {
                    self.path.trim().to_owned()
                };
                if let Err(error) = client.list(path) {
                    self.error = Some(format!("{error:#}"));
                }
            }
            if view.has_more
                && ui.button(tr!("More", ui.ctx())).clicked()
                && self.admit(ui)
                && let Err(error) = client.load_more()
            {
                self.error = Some(format!("{error:#}"));
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(tr!("Download directory", ui.ctx()));
            ui.add(egui::TextEdit::singleline(&mut self.local_directory).char_limit(4096));
            if ui.button(tr!("Browse local", ui.ctx())).clicked()
                && self.admit(ui)
                && let Err(error) = client.list_local(PathBuf::from(self.local_directory.trim()))
            {
                self.error = Some(format!("{error:#}"));
            }
        });
        let local_response = egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(tr!(
                "Local files · drop remote entries here to download",
                ui.ctx()
            ));
            if view.local_loading {
                ui.label(tr!("Reading local directory…", ui.ctx()));
            }
            ui.label(&view.local_path);
            self.local_toolbar(ui, view, &client);
            egui::ScrollArea::vertical()
                .id_salt("local-files")
                .max_height(140.0)
                .show(ui, |ui| {
                    for entry in &view.local_entries {
                        ui.horizontal_wrapped(|ui| {
                            self.icons.show(ui, &entry.icon);
                            self.local_select(ui, view, entry);
                            super::super::sftp_drop::Drops::source(
                                ui,
                                pane_id,
                                view,
                                &entry.path,
                                &entry.name,
                                true,
                                Some(self.local_selection.drag_paths(&entry.path)),
                            );
                            if entry.is_dir
                                && ui.small_button(tr!("Open", ui.ctx())).clicked()
                                && self.admit(ui)
                                && let Err(error) = client.list_local(entry.path.clone().into())
                            {
                                self.error = Some(format!("{error:#}"));
                            }
                            if !entry.is_dir
                                && ui
                                    .add_enabled(
                                        !view.local_loading,
                                        egui::Button::new(tr!("Open", ui.ctx())).small(),
                                    )
                                    .clicked()
                                && self.admit(ui)
                                && let Err(error) =
                                    client.open_local(entry.path.clone(), view.local_revision)
                            {
                                self.error = Some(format!("{error:#}"));
                            }
                            if !entry.is_dir
                                && ui.small_button(tr!("Use for upload", ui.ctx())).clicked()
                                && self.admit(ui)
                            {
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
            view,
            &client,
            super::super::sftp_drop::Target {
                remote: false,
                native_position: None,
            },
        ) {
            self.error = Some(format!("{error:#}"));
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(tr!("Upload file", ui.ctx()));
            ui.add(egui::TextEdit::singleline(&mut self.upload_source).char_limit(4096));
            ui.label(tr!("Remote path", ui.ctx()));
            ui.add(egui::TextEdit::singleline(&mut self.upload_remote).char_limit(4096));
            if ui.button(tr!("Upload", ui.ctx())).clicked()
                && self.admit(ui)
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
            ui.label(tr!(
                "Remote files · drop local files or folders here to upload",
                ui.ctx()
            ));
            self.remote_toolbar(ui, view, &client);
            egui::ScrollArea::vertical()
                .id_salt("remote-files")
                .max_height(180.0)
                .show(ui, |ui| {
                    for entry in &view.entries {
                        ui.horizontal_wrapped(|ui| {
                            self.icons.show(ui, &entry.icon);
                            self.remote_select(ui, view, entry);
                            self.entry_buttons(ui, view, entry);
                            super::super::sftp_drop::Drops::source(
                                ui,
                                pane_id,
                                view,
                                &entry.path,
                                &entry.name,
                                false,
                                Some(self.remote_selection.drag_paths(&entry.path)),
                            );
                            if ui
                                .add_enabled(
                                    view.connected
                                        && !view.loading
                                        && !entry.is_dir
                                        && !entry.is_symlink,
                                    egui::Button::new(tr!("Edit externally", ui.ctx())),
                                )
                                .clicked()
                            {
                                edit_action = Some(super::super::sftp_edit::EditAction::Open(
                                    entry.path.clone(),
                                ));
                            }
                            if !entry.is_dir
                                && ui.small_button(tr!("Download", ui.ctx())).clicked()
                                && self.admit(ui)
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
            view,
            &client,
            super::super::sftp_drop::Target {
                remote: true,
                native_position,
            },
        ) {
            self.error = Some(format!("{error:#}"));
        }
        if view.upload_preparing
            && ui
                .button(tr!("Cancel upload preparation", ui.ctx()))
                .clicked()
            && self.admit(ui)
            && let Err(error) = client.cancel_upload_preparation()
        {
            self.error = Some(format!("{error:#}"));
        }
        if let Some(action) = super::super::sftp_edit::EditControls::show(ui, view) {
            edit_action = Some(action);
        }
        if let Some(action) = edit_action
            && self.admit(ui)
            && let Err(error) = self.edits.apply(
                ui.ctx().cumulative_frame_nr(),
                ui.is_sizing_pass(),
                action,
                &client,
            )
        {
            self.error = Some(format!("{error:#}"));
        }
        self.show_transfers(ui, &client, view);
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        Ok(())
    }
    fn admit(&mut self, ui: &egui::Ui) -> bool {
        let frame = ui.ctx().cumulative_frame_nr();
        if ui.is_sizing_pass() || !ui.is_enabled() || self.last_action_frame == Some(frame) {
            return false;
        }
        self.last_action_frame = Some(frame);
        true
    }
}

#[cfg(test)]
mod tests;
