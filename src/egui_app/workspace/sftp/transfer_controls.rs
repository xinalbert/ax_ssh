//! Pane-local selection and bounded batch presentation; no transport work.
use super::*;
use crate::egui_app::i18n::{self, mark};
use ax_ssh::controller::{
    MAX_SFTP_TRANSFER_BATCH, SftpClient, SftpTransferControl as Control, SftpView,
};
use std::collections::BTreeSet;

#[derive(Default)]
pub(in crate::egui_app::workspace) struct TransferControls {
    generation: Option<u64>,
    selected: BTreeSet<Uuid>,
    selection_frame: Option<u64>,
    pending_conflict: Option<Uuid>,
    apply_conflict_to_batch: bool,
}

impl TransferControls {
    fn sync(&mut self, view: &SftpView) {
        if self.generation != Some(view.generation) || !view.connected {
            self.selected.clear();
            self.pending_conflict = None;
            self.apply_conflict_to_batch = false;
        }
        self.generation = Some(view.generation);
        self.selected.retain(|id| {
            view.transfers
                .iter()
                .any(|entry| entry.id == *id && !entry.phase.finished())
        });
        if self.pending_conflict.is_some_and(|id| {
            !view
                .transfers
                .iter()
                .any(|entry| entry.id == id && entry.conflict)
        }) {
            self.pending_conflict = None;
            self.apply_conflict_to_batch = false;
        }
    }
    fn admit_selection(&mut self, ui: &egui::Ui) -> bool {
        let frame = ui.ctx().cumulative_frame_nr();
        if !ui.is_enabled() || ui.is_sizing_pass() || self.selection_frame == Some(frame) {
            return false;
        }
        self.selection_frame = Some(frame);
        true
    }
    fn eligible(&self, view: &SftpView, control: Control) -> bool {
        view.connected
            && view
                .transfers
                .iter()
                .any(|entry| self.selected.contains(&entry.id) && control.allows(entry))
    }
}

impl SftpPane {
    pub(super) fn show_transfers(
        &mut self,
        ui: &mut egui::Ui,
        client: &SftpClient,
        view: &SftpView,
    ) {
        if !ui.is_sizing_pass() {
            self.transfers.sync(view);
        }
        let mut command = None;
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    view.connected && view.transfers.iter().any(|entry| !entry.phase.finished()),
                    egui::Button::new(tr!("Select All", ui.ctx())).small(),
                )
                .clicked()
                && self.transfers.admit_selection(ui)
            {
                self.transfers.selected = view
                    .transfers
                    .iter()
                    .filter(|entry| !entry.phase.finished())
                    .take(MAX_SFTP_TRANSFER_BATCH)
                    .map(|entry| entry.id)
                    .collect();
            }
            if ui
                .add_enabled(
                    !self.transfers.selected.is_empty(),
                    egui::Button::new(tr!("Clear", ui.ctx())).small(),
                )
                .clicked()
                && self.transfers.admit_selection(ui)
            {
                self.transfers.selected.clear();
            }
            ui.label(format!(
                "{}: {}",
                tr!("Selected", ui.ctx()),
                self.transfers.selected.len()
            ));
            for (control, label) in [
                (Control::Pause, mark!("Pause selected")),
                (Control::Resume, mark!("Resume selected")),
                (Control::Cancel, mark!("Cancel selected")),
            ] {
                if ui
                    .add_enabled(
                        self.transfers.eligible(view, control),
                        egui::Button::new(i18n::current(ui.ctx()).text("", label)).small(),
                    )
                    .clicked()
                    && self.admit(ui)
                {
                    command = Some(control);
                }
            }
        });
        if let Some(control) = command
            && let Err(error) =
                client.control_transfers(self.transfers.selected.iter().copied().collect(), control)
        {
            self.error = Some(format!("{error:#}"));
        }
        if let Some(result) = &view.last_batch {
            let label = match result.control {
                Control::Pause => tr!("Pause selected", ui.ctx()),
                Control::Resume => tr!("Resume selected", ui.ctx()),
                Control::Cancel => tr!("Cancel selected", ui.ctx()),
            };
            ui.label(format!(
                "{} · {} · {} {}",
                label,
                tr!("Queued ({0})", ui.ctx()).replace("{0}", &result.queued.to_string()),
                tr!("Skipped", ui.ctx()),
                result.skipped
            ));
            let failed =
                tr!("Failed ({0})", ui.ctx()).replace("{0}", &result.failures.len().to_string());
            egui::CollapsingHeader::new(failed)
                .id_salt("batch-failures")
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("batch-errors")
                        .max_height(160.0)
                        .show(ui, |ui| {
                            for failure in &result.failures {
                                let name = view
                                    .transfers
                                    .iter()
                                    .find(|item| item.id == failure.transfer_id)
                                    .map(|item| item.name.clone())
                                    .unwrap_or_else(|| failure.transfer_id.to_string());
                                ui.label(format!("{name}: {}", failure.message));
                            }
                        });
                });
        }
        egui::ScrollArea::vertical()
            .id_salt("transfer-controls")
            .max_height(160.0)
            .show(ui, |ui| {
                for transfer in &view.transfers {
                    ui.push_id(transfer.id, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let selected = self.transfers.selected.contains(&transfer.id);
                            if ui
                                .add_enabled(
                                    view.connected && !transfer.phase.finished(),
                                    egui::Button::selectable(
                                        selected,
                                        format!(
                                            "{} · {} / {} · {}",
                                            transfer.name,
                                            transfer.downloaded_bytes,
                                            transfer.total_bytes,
                                            transfer.status
                                        ),
                                    ),
                                )
                                .clicked()
                                && self.transfers.admit_selection(ui)
                            {
                                if selected {
                                    self.transfers.selected.remove(&transfer.id);
                                } else if self.transfers.selected.len() < MAX_SFTP_TRANSFER_BATCH {
                                    self.transfers.selected.insert(transfer.id);
                                }
                            }
                            if transfer.conflict && self.transfers.pending_conflict.is_none() {
                                ui.horizontal_wrapped(|ui| {
                                    if ui
                                        .small_button(tr!("Remote file already exists", ui.ctx()))
                                        .clicked()
                                        && self.admit(ui)
                                    {
                                        self.transfers.pending_conflict = Some(transfer.id);
                                        self.transfers.apply_conflict_to_batch = false;
                                    }
                                });
                            }
                            if transfer.conflict
                                && self.transfers.pending_conflict == Some(transfer.id)
                            {
                                ui.label(tr!("Waiting for server…", ui.ctx()));
                            }
                            if ui
                                .add_enabled(
                                    view.connected && transfer.can_pause(),
                                    egui::Button::new(tr!("Pause", ui.ctx())).small(),
                                )
                                .clicked()
                                && self.admit(ui)
                                && let Err(error) = client.pause(transfer.id)
                            {
                                self.error = Some(format!("{error:#}"));
                            }
                            if ui
                                .add_enabled(
                                    view.connected && transfer.can_resume(),
                                    egui::Button::new(tr!("Resume", ui.ctx())).small(),
                                )
                                .clicked()
                                && self.admit(ui)
                                && let Err(error) = client.resume(transfer.id)
                            {
                                self.error = Some(format!("{error:#}"));
                            }
                            if ui
                                .add_enabled(
                                    view.connected && transfer.can_cancel(),
                                    egui::Button::new(tr!("Cancel", ui.ctx())).small(),
                                )
                                .clicked()
                                && self.admit(ui)
                                && let Err(error) = client.cancel(transfer.id)
                            {
                                self.error = Some(format!("{error:#}"));
                            }
                        });
                    });
                }
            });
        self.show_conflict_modal(ui, client, view);
    }

    fn show_conflict_modal(&mut self, ui: &mut egui::Ui, client: &SftpClient, view: &SftpView) {
        if ui.is_sizing_pass() || !ui.is_enabled() {
            return;
        }
        let Some(transfer_id) = self.transfers.pending_conflict else {
            return;
        };
        let Some(transfer) = view
            .transfers
            .iter()
            .find(|transfer| transfer.id == transfer_id && transfer.conflict)
        else {
            self.transfers.pending_conflict = None;
            self.transfers.apply_conflict_to_batch = false;
            return;
        };
        let name = transfer.name.clone();
        let mut choice = None;
        let response = egui::Modal::new(
            ui.make_persistent_id(("sftp-upload-conflict", transfer_id)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(440.0_f32.min((ui.ctx().content_rect().width() - 24.0).max(100.0)));
            ui.heading(tr!("Remote file already exists", ui.ctx()));
            ui.label(name.clone());
            ui.checkbox(
                &mut self.transfers.apply_conflict_to_batch,
                tr!("Apply this choice to this upload batch", ui.ctx()),
            );
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr!("Skip", ui.ctx())).clicked() && self.admit(ui) {
                    choice = Some(ax_ssh::sftp::SftpUploadConflictChoice::Skip);
                }
                if ui.button(tr!("Keep both", ui.ctx())).clicked() && self.admit(ui) {
                    choice = Some(ax_ssh::sftp::SftpUploadConflictChoice::KeepBoth);
                }
                if ui.button(tr!("Overwrite", ui.ctx())).clicked() && self.admit(ui) {
                    choice = Some(ax_ssh::sftp::SftpUploadConflictChoice::Overwrite);
                }
            });
        });
        let close = response.should_close();
        if choice.is_none() && close {
            choice = Some(ax_ssh::sftp::SftpUploadConflictChoice::Skip);
        }
        let Some(choice) = choice else {
            return;
        };
        let apply_to_batch = self.transfers.apply_conflict_to_batch;
        match client
            .for_view(view)
            .resolve_conflict_with_batch(transfer_id, choice, apply_to_batch)
        {
            Ok(()) => {
                self.transfers.pending_conflict = None;
                self.transfers.apply_conflict_to_batch = false;
            }
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }
}

#[cfg(test)]
mod tests;
