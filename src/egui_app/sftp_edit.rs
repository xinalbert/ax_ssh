//! External-edit controls contain public metadata and user intent only.

use super::i18n::tr;
use anyhow::Result;
use ax_ssh::controller::{SftpClient, SftpView};
use eframe::egui;
use uuid::Uuid;

pub(super) enum EditAction {
    Open(String),
    Upload(Uuid),
    Reopen(Uuid),
    Stop(Uuid),
    AutoUpload(bool),
}

#[derive(Default)]
pub(super) struct EditControls {
    last_action_frame: Option<u64>,
}

impl EditControls {
    pub(super) fn show(ui: &mut egui::Ui, view: &SftpView) -> Option<EditAction> {
        if view.edits.is_empty() {
            return None;
        }
        let mut action = None;
        ui.separator();
        ui.strong(tr!("External editing", ui.ctx()));
        ui.label(tr!(
            "Save in the system application. Copies remain available after editing stops.",
            ui.ctx()
        ));
        let mut automatic = view.edit_auto_upload;
        if ui
            .checkbox(
                &mut automatic,
                tr!("Upload saved changes automatically", ui.ctx()),
            )
            .changed()
        {
            action = Some(EditAction::AutoUpload(automatic));
        }
        for edit in &view.edits {
            ui.push_id(edit.id, |ui| {
                ui.label(&edit.path);
                ui.label(&edit.status);
                if let Some(path) = &edit.local_path {
                    ui.label(path.display().to_string());
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            edit.local_path.is_some() && !edit.pending,
                            egui::Button::new(tr!("Upload changes", ui.ctx())),
                        )
                        .clicked()
                    {
                        action = Some(EditAction::Upload(edit.id));
                    }
                    if ui
                        .add_enabled(
                            edit.local_path.is_some(),
                            egui::Button::new(tr!("Reopen editor", ui.ctx())),
                        )
                        .clicked()
                    {
                        action = Some(EditAction::Reopen(edit.id));
                    }
                    if ui.button(tr!("Stop editing", ui.ctx())).clicked() {
                        action = Some(EditAction::Stop(edit.id));
                    }
                });
            });
        }
        action
    }

    pub(super) fn apply(
        &mut self,
        frame: u64,
        sizing: bool,
        action: EditAction,
        client: &SftpClient,
    ) -> Result<()> {
        if sizing || self.last_action_frame == Some(frame) {
            return Ok(());
        }
        self.last_action_frame = Some(frame);
        match action {
            EditAction::Open(path) => client.edit(path),
            EditAction::Upload(id) => client.upload_edit(id),
            EditAction::Reopen(id) => client.reopen_edit(id),
            EditAction::Stop(id) => client.stop_edit(id),
            EditAction::AutoUpload(enabled) => client.auto_upload_edits(enabled),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_ssh::controller::SftpSession;
    use std::sync::Arc;

    #[tokio::test]
    async fn edit_intents_are_admitted_once_per_frame_and_skip_sizing_passes() {
        let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
        let client = session.client();
        let mut controls = EditControls::default();
        controls
            .apply(1, true, EditAction::AutoUpload(false), &client)
            .unwrap();
        assert!(controls.last_action_frame.is_none());
        controls
            .apply(1, false, EditAction::AutoUpload(false), &client)
            .unwrap();
        controls
            .apply(1, false, EditAction::AutoUpload(true), &client)
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while client.view().unwrap().edit_auto_upload {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        session.shutdown().await.unwrap();
    }
}
