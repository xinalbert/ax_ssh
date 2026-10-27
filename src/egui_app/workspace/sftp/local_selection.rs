//! Pane-local selection for the existing retained upload preparation path.
use super::*;
use ax_ssh::controller::{LocalEntryView, MAX_SFTP_UPLOAD_ROOTS, SftpView};
use std::collections::BTreeMap;

#[derive(Default)]
pub(in crate::egui_app::workspace) struct LocalSelection {
    source: Option<(u64, u64, String)>,
    selected: BTreeMap<String, bool>,
    frame: Option<u64>,
}

impl LocalSelection {
    pub(super) fn drag_paths(&self, path: &str) -> Vec<String> {
        self.selected
            .get(path)
            .map(|_| self.selected.keys().cloned().collect())
            .unwrap_or_else(|| vec![path.to_owned()])
    }

    fn sync(&mut self, view: &SftpView) {
        let source = (
            view.generation,
            view.local_revision,
            view.local_path.clone(),
        );
        if self.source.as_ref() != Some(&source) || !view.connected {
            self.selected.clear();
        }
        self.source = Some(source);
        self.selected.retain(|path, directory| {
            view.local_entries
                .iter()
                .any(|entry| &entry.path == path && entry.is_dir == *directory)
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
    pub(super) fn local_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        view: &SftpView,
        client: &SftpClient,
    ) {
        if !ui.is_sizing_pass() {
            self.local_selection.sync(view);
        }
        ui.push_id("local-selection", |ui| {
            ui.horizontal_wrapped(|ui| {
                let ready = view.connected && !view.local_loading;
                if ui
                    .add_enabled(
                        ready && !view.local_entries.is_empty(),
                        egui::Button::new(tr!("Select All", ui.ctx())).small(),
                    )
                    .clicked()
                    && self.local_selection.admit(ui)
                {
                    self.local_selection.selected = view
                        .local_entries
                        .iter()
                        .take(MAX_SFTP_UPLOAD_ROOTS)
                        .map(|entry| (entry.path.clone(), entry.is_dir))
                        .collect();
                }
                if ui
                    .add_enabled(
                        !self.local_selection.selected.is_empty(),
                        egui::Button::new(tr!("Clear", ui.ctx())).small(),
                    )
                    .clicked()
                    && self.local_selection.admit(ui)
                {
                    self.local_selection.selected.clear();
                }
                ui.label(format!(
                    "{}: {}",
                    tr!("Selected", ui.ctx()),
                    self.local_selection.selected.len()
                ));
                if ui
                    .add_enabled(
                        ready
                            && !view.loading
                            && !view.path.is_empty()
                            && !view.upload_preparing
                            && !self.local_selection.selected.is_empty(),
                        egui::Button::new(tr!("Upload selected", ui.ctx())).small(),
                    )
                    .clicked()
                    && self.admit(ui)
                    && let Err(error) = client.upload_selected(
                        self.local_selection.selected.keys().cloned().collect(),
                        view.local_revision,
                        view.path.clone(),
                        view.request_id,
                    )
                {
                    self.error = Some(format!("{error:#}"));
                }
            });
        });
    }
    pub(super) fn local_select(
        &mut self,
        ui: &mut egui::Ui,
        view: &SftpView,
        entry: &LocalEntryView,
    ) {
        ui.push_id(("select-local", &entry.path), |ui| {
            let selected = self.local_selection.selected.contains_key(&entry.path);
            let label = tr!("Select {0}", ui.ctx()).replace("{0}", &entry.name);
            if ui
                .add_enabled(
                    view.connected
                        && !view.local_loading
                        && (selected
                            || self.local_selection.selected.len() < MAX_SFTP_UPLOAD_ROOTS),
                    egui::Button::selectable(selected, label).small().truncate(),
                )
                .clicked()
                && self.local_selection.admit(ui)
            {
                if selected {
                    self.local_selection.selected.remove(&entry.path);
                } else {
                    self.local_selection
                        .selected
                        .insert(entry.path.clone(), entry.is_dir);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests;
