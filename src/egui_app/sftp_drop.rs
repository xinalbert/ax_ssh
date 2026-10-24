//! UI drag payloads and once-per-frame admission; no file reads or protocol work.

use anyhow::{Result, ensure};
use ax_ssh::controller::{SftpClient, SftpView};
use eframe::egui;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone)]
pub(super) struct Drag {
    pane: Uuid,
    generation: u64,
    revision: u64,
    path: String,
    local: bool,
}

pub(super) struct Target {
    pub(super) remote: bool,
    pub(super) native_position: Option<egui::Pos2>,
}

#[derive(Default)]
pub(super) struct Drops {
    last_frame: Option<u64>,
}

impl Drops {
    pub(super) fn source(
        ui: &mut egui::Ui,
        pane: Uuid,
        view: &SftpView,
        path: &str,
        name: &str,
        local: bool,
    ) {
        let response = ui.add(egui::Label::new(name).sense(egui::Sense::click_and_drag()));
        response.dnd_set_drag_payload(Drag {
            pane,
            generation: view.generation,
            revision: if local {
                view.local_revision
            } else {
                view.request_id
            },
            path: path.into(),
            local,
        });
    }

    pub(super) fn target(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        pane: Uuid,
        view: &SftpView,
        client: &SftpClient,
        target: Target,
    ) -> Result<()> {
        if !ui.is_enabled()
            || ui.is_sizing_pass()
            || self.last_frame == Some(ui.ctx().cumulative_frame_nr())
        {
            return Ok(());
        }
        let Target {
            remote,
            native_position,
        } = target;
        let rect = response.rect.intersect(ui.clip_rect());
        if ui
            .ctx()
            .pointer_interact_pos()
            .is_some_and(|position| rect.contains(position))
            && let Some(payload) = response.dnd_release_payload::<Drag>()
        {
            self.last_frame = Some(ui.ctx().cumulative_frame_nr());
            ensure!(
                payload.pane == pane && payload.generation == view.generation,
                "drag belongs to another pane or connection"
            );
            let bound = client.for_view(view);
            if remote {
                ensure!(payload.local, "drop local files on the remote files area");
                bound.upload_local_drop(
                    payload.path.clone(),
                    payload.revision,
                    view.path.clone(),
                    view.request_id,
                )?;
            } else {
                ensure!(!payload.local, "drop remote files on the local files area");
                bound.download_drop(
                    payload.path.clone(),
                    payload.revision,
                    view.local_path.clone(),
                    view.local_revision,
                )?;
            }
        }
        if remote
            && native_position
                .is_some_and(|position| position.is_finite() && rect.contains(position))
            && ui.input(|input| !input.raw.dropped_files.is_empty())
        {
            self.last_frame = Some(ui.ctx().cumulative_frame_nr());
            ensure!(
                view.connected && !view.loading && !view.path.is_empty(),
                "SFTP upload directory is not ready"
            );
            ensure!(
                ui.input(|input| input.raw.dropped_files.len()) <= 64,
                "drop contains too many local roots"
            );
            let paths = ui
                .input_mut(|input| std::mem::take(&mut input.raw.dropped_files))
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect::<Vec<PathBuf>>();
            client
                .for_view(view)
                .upload_roots(paths, view.path.clone(), view.request_id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
