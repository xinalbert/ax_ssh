//! Explicit JSON drafts and request ownership; parsing and persistence stay in the owner.
use super::*;
use ax_ssh::session_transfer::{MAX_SESSION_TRANSFER_BYTES, SessionExportTarget};

enum Mode {
    Import,
    Export,
}

pub(super) struct TransferUi {
    mode: Mode,
    revision: u64,
    text: String,
    target: Option<SessionExportTarget>,
    submitted: Option<Uuid>,
    last_action: Option<u64>,
    focus: bool,
    copied: bool,
    error: Option<String>,
}

impl TransferUi {
    pub(super) fn import(view: &WorkspaceView) -> Self {
        Self {
            mode: Mode::Import,
            revision: view.catalog_revision,
            text: String::new(),
            target: None,
            submitted: None,
            last_action: None,
            focus: true,
            copied: false,
            error: None,
        }
    }

    pub(super) fn export(view: &WorkspaceView) -> Self {
        let mut dialog = Self::import(view);
        dialog.mode = Mode::Export;
        dialog.target = view
            .tabs
            .iter()
            .find(|tab| Some(tab.id) == view.active_tab_id)
            .and_then(|tab| {
                tab.layout
                    .panes
                    .iter()
                    .find(|pane| pane.focused)
                    .and_then(|focused| tab.panes.iter().find(|pane| pane.id == focused.tab_id))
            })
            .and_then(|pane| pane.profile.as_ref())
            .or_else(|| view.profiles.first())
            .map(|profile| SessionExportTarget::Server(profile.id))
            .or_else(|| view.groups.first().cloned().map(SessionExportTarget::Group));
        dialog
    }

    fn submit(&mut self, ui: &egui::Ui, client: &WorkspaceClient) {
        let frame = ui.ctx().cumulative_frame_nr();
        if ui.is_sizing_pass() || self.last_action == Some(frame) || self.submitted.is_some() {
            return;
        }
        self.last_action = Some(frame);
        self.error = None;
        self.copied = false;
        let action = match self.mode {
            Mode::Import => {
                if self.text.len() > MAX_SESSION_TRANSFER_BYTES {
                    self.error = Some("session import exceeds the 256 KiB limit".into());
                    return;
                }
                CatalogAction::Import(self.text.clone())
            }
            Mode::Export => {
                let Some(target) = self.target.clone() else {
                    return;
                };
                CatalogAction::Export(target)
            }
        };
        match client.catalog(self.revision, action) {
            Ok(request) => self.submitted = Some(request),
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }

    fn poll(&mut self, ui: &egui::Ui, client: &WorkspaceClient) -> Result<bool> {
        if ui.is_sizing_pass() {
            return Ok(false);
        }
        let Some(request) = self.submitted else {
            return Ok(false);
        };
        let Some(result) = client.take_catalog_result(request)? else {
            return Ok(false);
        };
        self.submitted = None;
        let revision = client.view()?.catalog_revision;
        ui.ctx().request_repaint();
        if let Some(error) = result.error {
            self.error = Some(error);
            self.revision = revision;
            return Ok(false);
        }
        match self.mode {
            Mode::Import => Ok(true),
            Mode::Export => {
                if revision == self.revision
                    && result.revision == self.revision
                    && ui.input(|input| input.focused)
                {
                    if let Some(text) = result.exported {
                        ui.ctx().copy_text(text);
                        self.copied = true;
                    }
                } else {
                    self.error =
                        Some("session list or window focus changed; retry the export".into());
                    self.revision = revision;
                }
                Ok(false)
            }
        }
    }

    fn show(&mut self, ui: &egui::Ui, view: &WorkspaceView, client: &WorkspaceClient) -> bool {
        let mut open = true;
        let title = match self.mode {
            Mode::Import => tr!("Import from Clipboard", ui.ctx()),
            Mode::Export => tr!("Export Selected to Clipboard", ui.ctx()),
        };
        let response =
            egui::Modal::new(egui::Id::unique("session-transfer")).show(ui.ctx(), |ui| {
                ui.set_width(540.0_f32.min(ui.ctx().content_rect().width().max(100.0) - 24.0));
                let heading = ui.heading(title);
                ui.add_enabled_ui(!view.busy && self.submitted.is_none(), |ui| {
                    match self.mode {
                        Mode::Import => {
                            let label = ui.label(format!(
                                "AxSSH JSON · 256 KiB · 128 {}",
                                tr!("Sessions", ui.ctx())
                            ));
                            let response = ui
                                .add(
                                    egui::TextEdit::multiline(&mut self.text)
                                        .id(ui.make_persistent_id("session-json"))
                                        .desired_rows(12)
                                        .desired_width(f32::INFINITY)
                                        .char_limit(MAX_SESSION_TRANSFER_BYTES),
                                )
                                .labelled_by(label.id);
                            if !ui.is_sizing_pass()
                                && std::mem::take(&mut self.focus)
                                && ui.is_enabled()
                            {
                                response.request_focus();
                            }
                            if ui
                                .add_enabled(
                                    !self.text.trim().is_empty(),
                                    egui::Button::new(tr!("Import from Clipboard", ui.ctx())),
                                )
                                .clicked()
                            {
                                self.submit(ui, client);
                            }
                        }
                        Mode::Export => {
                            let selected = match &self.target {
                                Some(SessionExportTarget::Server(id)) => view
                                    .profiles
                                    .iter()
                                    .find(|p| p.id == *id)
                                    .map(|p| p.name.as_str()),
                                Some(SessionExportTarget::Group(name)) => Some(name.as_str()),
                                None => None,
                            }
                            .unwrap_or("");
                            egui::ComboBox::from_id_salt("export-target")
                                .selected_text(selected)
                                .show_ui(ui, |ui| {
                                    ui.label(tr!("Sessions", ui.ctx()));
                                    for profile in view.profiles.iter() {
                                        ui.selectable_value(
                                            &mut self.target,
                                            Some(SessionExportTarget::Server(profile.id)),
                                            &profile.name,
                                        );
                                    }
                                    ui.separator();
                                    ui.label(tr!("Group", ui.ctx()));
                                    for group in view.groups.iter() {
                                        ui.selectable_value(
                                            &mut self.target,
                                            Some(SessionExportTarget::Group(group.clone())),
                                            group,
                                        );
                                    }
                                })
                                .response
                                .labelled_by(heading.id);
                            if ui
                                .add_enabled(
                                    self.target.is_some(),
                                    egui::Button::new(tr!("Copy", ui.ctx())),
                                )
                                .clicked()
                            {
                                self.submit(ui, client);
                            }
                            if self.copied {
                                ui.label(format!("✓ {}", tr!("Copy", ui.ctx())));
                            }
                        }
                    }
                });
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
                if ui.button(tr!("Close", ui.ctx())).clicked() {
                    open = false;
                }
            });
        open && !response.should_close()
    }
}

impl WorkspaceUi {
    pub(super) fn transfer_dialog(
        &mut self,
        ui: &egui::Ui,
        client: &WorkspaceClient,
        view: &WorkspaceView,
    ) -> Result<()> {
        if ui.is_sizing_pass() {
            return Ok(());
        }
        let Some(dialog) = &mut self.transfer else {
            return Ok(());
        };
        // Observe Close before taking a result: late export completions must not copy.
        let open = dialog.show(ui, view, client);
        if !open || dialog.poll(ui, client)? {
            if let Some(request) = dialog.submitted.take() {
                self.transactions.push(request);
            }
            self.transfer = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
