//! Directory-local path selection; metadata and intents only.
use super::*;
use ax_ssh::controller::{MAX_SFTP_DOWNLOAD_ROOTS, SftpEntryView, SftpView};
use std::collections::BTreeMap;

#[derive(Default)]
pub(in crate::egui_app::workspace) struct RemoteSelection {
    source: Option<(u64, u64, String)>,
    selected: BTreeMap<String, bool>,
    frame: Option<u64>,
}

impl RemoteSelection {
    pub(super) fn drag_paths(&self, path: &str) -> Vec<String> {
        self.selected
            .get(path)
            .map(|_| self.selected.keys().cloned().collect())
            .unwrap_or_else(|| vec![path.to_owned()])
    }

    fn sync(&mut self, view: &SftpView) {
        let source = (view.generation, view.request_id, view.path.clone());
        if self.source.as_ref() != Some(&source) || !view.connected {
            self.selected.clear();
        }
        self.source = Some(source);
        self.selected.retain(|path, directory| {
            view.entries
                .iter()
                .any(|entry| &entry.path == path && !entry.is_symlink && entry.is_dir == *directory)
        });
    }
    fn admit(&mut self, ui: &egui::Ui) -> bool {
        let frame = ui.ctx().cumulative_frame_nr();
        if !ui.is_enabled() || ui.is_sizing_pass() || self.frame == Some(frame) {
            return false;
        }
        self.frame = Some(frame);
        true
    }
}

impl SftpPane {
    pub(super) fn remote_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        view: &SftpView,
        client: &SftpClient,
    ) {
        if !ui.is_sizing_pass() {
            self.remote_selection.sync(view);
        }
        ui.push_id("remote-selection", |ui| {
            ui.horizontal_wrapped(|ui| {
                let ready = view.connected && !view.loading;
                if ui
                    .add_enabled(
                        ready && view.entries.iter().any(|entry| !entry.is_symlink),
                        egui::Button::new(tr!("Select All", ui.ctx())).small(),
                    )
                    .clicked()
                    && self.remote_selection.admit(ui)
                {
                    self.remote_selection.selected = view
                        .entries
                        .iter()
                        .filter(|entry| !entry.is_symlink)
                        .take(MAX_SFTP_DOWNLOAD_ROOTS)
                        .map(|entry| (entry.path.clone(), entry.is_dir))
                        .collect();
                }
                if ui
                    .add_enabled(
                        !self.remote_selection.selected.is_empty(),
                        egui::Button::new(tr!("Clear", ui.ctx())).small(),
                    )
                    .clicked()
                    && self.remote_selection.admit(ui)
                {
                    self.remote_selection.selected.clear();
                }
                ui.label(format!(
                    "{}: {}",
                    tr!("Selected", ui.ctx()),
                    self.remote_selection.selected.len()
                ));
                if ui
                    .add_enabled(
                        ready
                            && !view.local_loading
                            && !view.local_path.is_empty()
                            && !self.remote_selection.selected.is_empty(),
                        egui::Button::new(tr!("Download selected", ui.ctx())).small(),
                    )
                    .clicked()
                    && self.admit(ui)
                    && let Err(error) = client.download_selected(
                        self.remote_selection.selected.keys().cloned().collect(),
                        view.request_id,
                        view.local_path.clone(),
                        view.local_revision,
                    )
                {
                    self.error = Some(format!("{error:#}"));
                }
            });
            if let Some(result) = &view.last_download_batch {
                ui.label(format!(
                    "{} · {} · {} {}",
                    tr!("Download selected", ui.ctx()),
                    tr!("Queued ({0})", ui.ctx()).replace("{0}", &result.queued.to_string()),
                    tr!("Skipped", ui.ctx()),
                    result.skipped
                ));
                if !result.failures.is_empty() {
                    egui::CollapsingHeader::new(
                        tr!("Failed ({0})", ui.ctx())
                            .replace("{0}", &result.failures.len().to_string()),
                    )
                    .id_salt("download-failures")
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("download-errors")
                            .max_height(120.0)
                            .show(ui, |ui| {
                                for failure in &result.failures {
                                    ui.add(egui::Label::new(&failure.path).truncate());
                                    ui.label(&failure.message);
                                }
                            });
                    });
                }
            }
        });
    }

    pub(super) fn remote_select(
        &mut self,
        ui: &mut egui::Ui,
        view: &SftpView,
        entry: &SftpEntryView,
    ) {
        ui.push_id(("select-remote", &entry.path), |ui| {
            let selected = self.remote_selection.selected.contains_key(&entry.path);
            let label = tr!("Select {0}", ui.ctx()).replace("{0}", &entry.name);
            if ui
                .add_enabled(
                    view.connected
                        && !view.loading
                        && !entry.is_symlink
                        && (selected
                            || self.remote_selection.selected.len() < MAX_SFTP_DOWNLOAD_ROOTS),
                    egui::Button::selectable(selected, label).small(),
                )
                .clicked()
                && self.remote_selection.admit(ui)
            {
                if selected {
                    self.remote_selection.selected.remove(&entry.path);
                } else {
                    self.remote_selection
                        .selected
                        .insert(entry.path.clone(), entry.is_dir);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests;
