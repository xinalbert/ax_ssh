//! Settings draft/preview only; the workspace owner validates and atomically saves.
use anyhow::Result;
use ax_ssh::config::{AppSettings, ThemeMode};
use ax_ssh::controller::{CatalogAction, WorkspaceClient, WorkspaceView};
use eframe::egui;
use uuid::Uuid;

pub(super) struct SettingsUi {
    pub(super) draft: AppSettings,
    base: AppSettings,
    revision: u64,
    submitted: Option<Uuid>,
    acknowledged: Option<u64>,
    preview: bool,
    category: usize,
    pub(super) confirm_close: bool,
    pub(super) exit: bool,
    close_after_save: bool,
    close_ready: bool,
    last_action: Option<u64>,
    error: Option<String>,
}
impl SettingsUi {
    pub(super) fn new(view: &WorkspaceView) -> Self {
        Self {
            draft: (*view.settings).clone(),
            base: (*view.settings).clone(),
            revision: view.catalog_revision,
            submitted: None,
            acknowledged: None,
            preview: true,
            category: 0,
            confirm_close: false,
            exit: false,
            close_after_save: false,
            close_ready: false,
            last_action: None,
            error: None,
        }
    }
    pub(super) fn dirty(&self) -> bool {
        self.draft != self.base
    }
    pub(super) fn saving(&self) -> bool {
        self.submitted.is_some() || self.acknowledged.is_some()
    }
    pub(super) fn effective<'a>(&'a self, saved: &'a AppSettings) -> &'a AppSettings {
        if self.preview { &self.draft } else { saved }
    }
    pub(super) fn request_close(&mut self, exit: bool) {
        self.exit = exit;
        if self.dirty() || self.saving() {
            self.confirm_close = true;
        } else {
            self.close_ready = true;
        }
    }
    pub(super) fn poll(&mut self, client: &WorkspaceClient, view: &WorkspaceView) -> Result<bool> {
        if let Some(request) = self.submitted
            && let Some(result) = client.take_catalog_result(request)?
        {
            self.submitted = None;
            if let Some(error) = result.error {
                self.error = Some(error);
                self.close_after_save = false;
            } else {
                self.acknowledged = Some(result.revision);
            }
        }
        if self
            .acknowledged
            .is_some_and(|revision| view.catalog_revision >= revision)
        {
            self.acknowledged = None;
            self.base = (*view.settings).clone();
            self.draft = self.base.clone();
            self.revision = view.catalog_revision;
            if self.close_after_save {
                self.confirm_close = false;
                self.close_ready = true;
            }
            self.close_after_save = false;
        } else if !self.dirty() && !self.saving() && view.catalog_revision > self.revision {
            self.base = (*view.settings).clone();
            self.draft = self.base.clone();
            self.revision = view.catalog_revision;
        }
        Ok(std::mem::take(&mut self.close_ready))
    }
    fn save(&mut self, client: &WorkspaceClient, view: &WorkspaceView, close: bool) -> Result<()> {
        self.validate()?;
        // A catalog edit may have occurred while this settings draft was open.
        // Preserve it by rebasing only when the saved settings themselves agree.
        anyhow::ensure!(
            *view.settings == self.base,
            "settings changed; revert or reopen before saving"
        );
        self.submitted = Some(client.catalog(
            view.catalog_revision,
            CatalogAction::Settings(Box::new(self.draft.clone())),
        )?);
        self.close_after_save = close;
        self.error = None;
        Ok(())
    }
    fn validate(&self) -> Result<()> {
        self.draft.shortcuts.validate()?;
        let shortcuts = &self.draft.shortcuts;
        let mut keys = Vec::new();
        for text in [
            &shortcuts.open_settings,
            &shortcuts.new_session,
            &shortcuts.toggle_sidebar,
        ] {
            let key = super::menus::parse_shortcut(text)?;
            anyhow::ensure!(!keys.contains(&key), "shortcuts resolve to the same key");
            for command in [
                super::menus::Command::SplitRight,
                super::menus::Command::SplitDown,
                super::menus::Command::PreviousTab,
                super::menus::Command::NextTab,
                super::menus::Command::CloseTab,
                super::menus::Command::Quit,
            ] {
                let reserved = command
                    .shortcut(&self.draft)
                    .and_then(|key| super::menus::parse_shortcut(&key).ok());
                anyhow::ensure!(
                    reserved != Some(key),
                    "shortcut is reserved by a pane, tab or window command"
                );
            }
            keys.push(key);
        }
        let shell = &self.draft.terminal.local_shell;
        anyhow::ensure!(
            !shell.trim().is_empty()
                && shell.chars().count() <= 256
                && !shell.chars().any(char::is_control),
            "invalid local shell"
        );
        Ok(())
    }
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        client: &WorkspaceClient,
        view: &WorkspaceView,
    ) {
        let mut save = false;
        ui.heading("Settings");
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.preview, "Live preview");
            ui.label(if self.saving() {
                "Saving…"
            } else if self.dirty() {
                "Unsaved changes"
            } else {
                "Saved"
            });
            save = ui
                .add_enabled(!self.saving() && self.dirty(), egui::Button::new("Save"))
                .clicked();
            if ui
                .add_enabled(!self.saving(), egui::Button::new("Revert"))
                .clicked()
            {
                self.base = (*view.settings).clone();
                self.draft = self.base.clone();
                self.revision = view.catalog_revision;
                self.error = None;
            }
        });
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            for (i, label) in [
                "Appearance",
                "Terminal",
                "Workspace",
                "Keyboard Shortcuts",
                "About",
            ]
            .into_iter()
            .enumerate()
            {
                ui.selectable_value(&mut self.category, i, label);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().id_salt("settings-content").show(ui, |ui| {
            ui.add_enabled_ui(!self.saving(), |ui| match self.category {
                0 => {
                    ui.horizontal(|ui| { ui.label("Theme"); egui::ComboBox::from_id_salt("theme-mode").selected_text(self.draft.appearance.theme.mode.as_setting()).show_ui(ui, |ui| { for mode in [ThemeMode::System,ThemeMode::Light,ThemeMode::Dark] { ui.selectable_value(&mut self.draft.appearance.theme.mode, mode, mode.as_setting()); } }); });
                    ui.horizontal(|ui| {
                        ui.label("Palette");
                        egui::ComboBox::from_id_salt("theme-palette").selected_text(self.draft.appearance.theme.palette.as_setting()).show_ui(ui, |ui| {
                            for name in ["axssh", "solarized", "arctic", "tokyo", "ember", "forest"] {
                                ui.selectable_value(&mut self.draft.appearance.theme.palette, ax_ssh::config::ThemePaletteKind::from_setting(name), name);
                            }
                        });
                    });
                    ui.add(egui::Slider::new(&mut self.draft.appearance.terminal_font_size, ax_ssh::config::MIN_TERMINAL_FONT_SIZE..=ax_ssh::config::MAX_TERMINAL_FONT_SIZE).text("Terminal font size"));
                    ui.add(egui::Slider::new(&mut self.draft.appearance.terminal_line_height_percent, ax_ssh::config::MIN_TERMINAL_LINE_HEIGHT..=ax_ssh::config::MAX_TERMINAL_LINE_HEIGHT).text("Line height (%)"));
                    ui.checkbox(&mut self.draft.appearance.terminal_cursor_blink, "Blink terminal cursor");
                    ui.label("Terminal preview");
                    let size = f32::from(self.draft.appearance.terminal_font_size);
                    ui.label(egui::RichText::new("AxSSH $ echo 'Hello 世界'\nHello 世界\n▌").font(super::fonts::font(size, false, false)));
                }
                1 => {
                    ui.label("Local shell (new local tabs)");
                    ui.add(egui::TextEdit::singleline(&mut self.draft.terminal.local_shell).char_limit(256));
                    ui.label("Use System default for the platform shell; otherwise enter an executable path.");
                }
                2 => {
                    ui.add(egui::Slider::new(&mut self.draft.workspace.sidebar_width, ax_ssh::config::MIN_SIDEBAR_WIDTH..=ax_ssh::config::MAX_SIDEBAR_WIDTH).text("Sidebar width"));
                    ui.add(egui::Slider::new(&mut self.draft.workspace.tab_width, ax_ssh::config::MIN_TAB_WIDTH..=ax_ssh::config::MAX_TAB_WIDTH).text("Tab width"));
                }
                3 => {
                    for (label, text) in [("Settings", &mut self.draft.shortcuts.open_settings), ("New server", &mut self.draft.shortcuts.new_session), ("Session sidebar", &mut self.draft.shortcuts.toggle_sidebar)] {
                        ui.horizontal_wrapped(|ui| { ui.label(label); ui.add(egui::TextEdit::singleline(text).char_limit(64)); });
                    }
                    ui.label("Modifiers: Ctrl, Cmd/Meta, Alt, Shift. Examples: Ctrl+N, Cmd+,.");
                    ui.label("Tab: Ctrl+Tab / Ctrl+Shift+Tab · Close: Cmd/Ctrl+W\nSplit: Cmd/Ctrl+D / Cmd/Ctrl+Shift+D");
                }
                _ => {
                    ui.heading("AxSSH");
                    ui.label(format!("{} · egui {}", env!("CARGO_PKG_VERSION"), "0.37.0"));
                    ui.label("Independent Rust desktop terminal workspace");
                    ui.hyperlink_to("Source and licenses", "https://github.com/xinalbert/ax_ssh");
                }
            });
        });
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        if save {
            self.save_once(ui, client, view, false);
        }
    }
    fn save_once(
        &mut self,
        ui: &egui::Ui,
        client: &WorkspaceClient,
        view: &WorkspaceView,
        close: bool,
    ) {
        let frame = ui.ctx().cumulative_frame_nr();
        if !ui.is_sizing_pass() && self.last_action != Some(frame) && !self.saving() {
            self.last_action = Some(frame);
            if let Err(error) = self.save(client, view, close) {
                self.error = Some(format!("{error:#}"));
            }
        }
    }
    pub(super) fn close_dialog(
        &mut self,
        ctx: &egui::Context,
        client: &WorkspaceClient,
        view: &WorkspaceView,
    ) {
        if !self.confirm_close {
            return;
        }
        egui::Modal::new(egui::Id::unique("settings-close")).show(ctx, |ui| {
            ui.heading("Save settings changes?");
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            ui.add_enabled_ui(!self.saving(), |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        self.save_once(ui, client, view, true);
                    }
                    if ui.button("Discard").clicked() {
                        self.draft = self.base.clone();
                        self.confirm_close = false;
                        self.close_ready = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_close = false;
                        self.exit = false;
                    }
                });
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_ssh::config::{
        ConfigStore, SessionStore, WORKSPACE_SNAPSHOT_VERSION, WorkspaceSnapshot,
    };
    use ax_ssh::controller::Workspace;
    use std::sync::Arc;

    async fn ready(client: &WorkspaceClient) -> WorkspaceView {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let view = client.view().unwrap();
                if view.ready && !view.busy {
                    return view;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn settings_preview_close_save_and_stale_draft_keep_explicit_ownership() {
        let directory = std::env::temp_dir().join(format!("axssh-settings-ui-{}", Uuid::new_v4()));
        let store = ConfigStore::new(directory.join("sessions.json"));
        store.save(&SessionStore::default()).unwrap();
        store
            .save_workspace(&WorkspaceSnapshot {
                version: WORKSPACE_SNAPSHOT_VERSION,
                ..Default::default()
            })
            .unwrap();
        let workspace = Workspace::spawn(
            &tokio::runtime::Handle::current(),
            store.clone(),
            Arc::new(|| {}),
        );
        let client = workspace.client();
        let view = ready(&client).await;
        let mut settings = SettingsUi::new(&view);
        settings.draft.appearance.terminal_font_size = 25;
        assert_eq!(
            settings
                .effective(&view.settings)
                .appearance
                .terminal_font_size,
            25
        );
        assert_ne!(
            store.load().unwrap().settings.appearance.terminal_font_size,
            25
        );
        settings.preview = false;
        assert_eq!(settings.effective(&view.settings), &*view.settings);
        settings.request_close(false);
        assert!(settings.confirm_close);
        assert!(!settings.poll(&client, &view).unwrap());
        settings.save(&client, &view, true).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let view = client.view().unwrap();
                if settings.poll(&client, &view).unwrap() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(!settings.dirty());
        assert!(!settings.confirm_close);
        assert_eq!(
            store.load().unwrap().settings.appearance.terminal_font_size,
            25
        );
        let saved = client.view().unwrap();
        let mut stale = SettingsUi::new(&view);
        stale.draft.appearance.terminal_font_size = 26;
        assert!(stale.save(&client, &saved, false).is_err());
        assert_eq!(stale.draft.appearance.terminal_font_size, 26);
        stale.draft.shortcuts.new_session = if cfg!(target_os = "macos") {
            "Cmd+W"
        } else {
            "Ctrl+W"
        }
        .into();
        assert!(stale.validate().is_err());
        workspace.shutdown().await.unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
}
