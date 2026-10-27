//! Frozen remote-entry drafts; I/O and retained receipts belong to the controller.
use super::*;
use ax_ssh::controller::{SftpEntryAction, SftpEntryView, SftpView};

pub(in crate::egui_app::workspace) struct EntryDialog {
    generation: u64,
    request_id: u64,
    entry: SftpEntryView,
    rename: bool,
    name: String,
    submitted: Option<Uuid>,
    error: Option<String>,
    focus: bool,
}

impl SftpPane {
    pub(super) fn entry_buttons(
        &mut self,
        ui: &mut egui::Ui,
        view: &SftpView,
        entry: &SftpEntryView,
    ) {
        let enabled =
            view.connected && !view.loading && !entry.is_symlink && self.entry_dialog.is_none();
        ui.push_id(("entry-actions", &entry.path), |ui| {
            for (rename, text) in [
                (true, tr!("Rename", ui.ctx())),
                (false, tr!("Delete", ui.ctx())),
            ] {
                if ui
                    .add_enabled(enabled, egui::Button::new(text).small())
                    .clicked()
                    && self.admit(ui)
                {
                    self.entry_dialog = Some(EntryDialog {
                        generation: view.generation,
                        request_id: view.request_id,
                        entry: entry.clone(),
                        rename,
                        name: entry.name.clone(),
                        submitted: None,
                        error: None,
                        focus: true,
                    });
                }
            }
        });
    }

    fn close_entry_dialog(&mut self, client: &SftpClient) -> Result<()> {
        if let Some(dialog) = self.entry_dialog.take()
            && let Some(id) = dialog.submitted
        {
            client.abandon_entry_result(id)?;
        }
        Ok(())
    }

    fn entry_dialog(
        &mut self,
        ui: &mut egui::Ui,
        client: &SftpClient,
        view: &SftpView,
        pane_id: Uuid,
    ) -> Result<()> {
        if ui.is_sizing_pass() || !ui.is_enabled() {
            return Ok(());
        }
        let Some(mut dialog) = self.entry_dialog.take() else {
            return Ok(());
        };
        if dialog.generation != view.generation || !view.connected {
            if let Some(id) = dialog.submitted {
                client.abandon_entry_result(id)?;
            }
            return Ok(());
        }
        if let Some(id) = dialog.submitted
            && let Some(result) = client.take_entry_result(id)?
        {
            dialog.submitted = None;
            match result {
                Ok(()) => return Ok(()),
                Err(message) => dialog.error = Some(message),
            }
        }
        let visible = view.request_id == dialog.request_id
            && !view.loading
            && view.entries.iter().any(|entry| {
                entry.path == dialog.entry.path
                    && entry.is_dir == dialog.entry.is_dir
                    && !entry.is_symlink
                    && entry.size == dialog.entry.size
                    && entry.modified == dialog.entry.modified
            });
        if !visible && dialog.submitted.is_none() {
            return Ok(());
        }
        let title = if dialog.rename {
            tr!("Rename selected remote entry", ui.ctx())
        } else {
            tr!("Delete remote entry?", ui.ctx())
        };
        let mut close = false;
        let response = egui::Modal::new(egui::Id::unique((pane_id, "remote-entry-action"))).show(
            ui.ctx(),
            |ui| {
                ui.set_width(440.0_f32.min((ui.ctx().content_rect().width() - 24.0).max(100.0)));
                ui.heading(title);
                ui.add(egui::Label::new(&dialog.entry.path).truncate());
                ui.add_enabled_ui(dialog.submitted.is_none() && visible, |ui| {
                    if dialog.rename {
                        let label = ui.label(tr!("New name", ui.ctx()));
                        let text = ui
                            .add(
                                egui::TextEdit::singleline(&mut dialog.name)
                                    .id(ui.make_persistent_id("entry-name"))
                                    .char_limit(512),
                            )
                            .labelled_by(label.id);
                        if dialog.focus && !ui.is_sizing_pass() && ui.is_enabled() {
                            text.request_focus();
                            dialog.focus = false;
                        }
                    } else {
                        ui.label(tr!(
                            "Deletion cannot be undone. Directories must be empty.",
                            ui.ctx()
                        ));
                    }
                    let label = if dialog.rename {
                        tr!("Rename", ui.ctx())
                    } else {
                        tr!("Delete", ui.ctx())
                    };
                    if ui.button(label).clicked() && self.admit(ui) {
                        let action = if dialog.rename {
                            SftpEntryAction::Rename(dialog.name.clone())
                        } else {
                            SftpEntryAction::Delete
                        };
                        match client.for_view(view).mutate_entry(
                            dialog.request_id,
                            &dialog.entry,
                            action,
                        ) {
                            Ok(id) => {
                                dialog.submitted = Some(id);
                                dialog.error = None;
                            }
                            Err(error) => dialog.error = Some(format!("{error:#}")),
                        }
                    }
                });
                if dialog.submitted.is_some() {
                    ui.label(tr!("Waiting for server…", ui.ctx()));
                }
                if let Some(error) = &dialog.error {
                    egui::ScrollArea::vertical()
                        .id_salt("entry-error")
                        .max_height(120.0)
                        .show(ui, |ui| {
                            ui.colored_label(Color32::LIGHT_RED, error);
                        });
                }
                let label = if dialog.submitted.is_some() {
                    tr!("Close", ui.ctx())
                } else {
                    tr!("Cancel", ui.ctx())
                };
                close = ui.button(label).clicked();
            },
        );
        close |= response.should_close();
        if close {
            if let Some(id) = dialog.submitted {
                client.abandon_entry_result(id)?;
            }
        } else {
            self.entry_dialog = Some(dialog);
        }
        Ok(())
    }
}

impl WorkspaceUi {
    pub(in crate::egui_app::workspace) fn entry_dialogs(
        &mut self,
        ui: &mut egui::Ui,
        client: &WorkspaceClient,
        view: &WorkspaceView,
    ) -> Result<()> {
        if ui.is_sizing_pass() || !ui.is_enabled() {
            return Ok(());
        }
        let active = view
            .tabs
            .iter()
            .find(|tab| Some(tab.id) == view.active_tab_id);
        let other_modal = view.busy
            || self.transfer.is_some()
            || self.editor.is_some()
            || self.groups_open
            || self.file_open
            || self.delete.is_some()
            || self
                .settings
                .as_ref()
                .is_some_and(|settings| settings.confirm_close);
        for (id, pane) in &mut self.panes {
            if pane.sftp.entry_dialog.is_none() {
                continue;
            }
            let Some(sftp) = client.sftp(*id)? else {
                pane.sftp.entry_dialog = None;
                continue;
            };
            if other_modal
                || !active
                    .is_some_and(|tab| tab.panes.iter().any(|pane| pane.id == *id && pane.sftp))
            {
                pane.sftp.close_entry_dialog(&sftp)?;
            } else {
                pane.sftp.entry_dialog(ui, &sftp, &sftp.view()?, *id)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
