//! Settings draft/preview only; the workspace owner validates and atomically saves.
use super::i18n::{self, mark, tr};
use anyhow::Result;
use ax_ssh::config::{AppSettings, ThemeMode, UiLanguage};
use ax_ssh::controller::{CatalogAction, WorkspaceClient, WorkspaceView};
use eframe::egui;
use uuid::Uuid;

mod appearance;
mod audio;
mod icons;
mod sftp;
mod shells;
mod shortcuts;
mod terminal;
mod workspace;
mod x11;

pub(super) fn set_x11_locations(ctx: &egui::Context, locations: Vec<String>) {
    x11::set_locations(ctx, locations);
}

pub(super) fn set_local_shells(ctx: &egui::Context, choices: Vec<String>) {
    shells::set_discovered(ctx, choices);
}

pub(super) struct SettingsUi {
    discovered_shells: std::sync::Arc<[String]>,
    icons: super::system_icons::Icons,
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
    filter_text: String,
}
impl SettingsUi {
    pub(super) fn clear_caches(&mut self) {
        self.icons.clear();
    }

    pub(super) fn new(view: &WorkspaceView) -> Self {
        Self {
            discovered_shells: Default::default(),
            icons: Default::default(),
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
            filter_text: view
                .settings
                .sftp_transfer_filters
                .custom_patterns
                .join("\n"),
        }
    }
    pub(super) fn dirty(&self) -> bool {
        self.draft != self.base
            || self.filter_text != self.base.sftp_transfer_filters.custom_patterns.join("\n")
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
            self.filter_text = self.base.sftp_transfer_filters.custom_patterns.join("\n");
            self.revision = view.catalog_revision;
            if self.close_after_save {
                self.confirm_close = false;
                self.close_ready = true;
            }
            self.close_after_save = false;
        } else if !self.dirty() && !self.saving() && view.catalog_revision > self.revision {
            self.base = (*view.settings).clone();
            self.draft = self.base.clone();
            self.filter_text = self.base.sftp_transfer_filters.custom_patterns.join("\n");
            self.revision = view.catalog_revision;
        }
        Ok(std::mem::take(&mut self.close_ready))
    }
    fn save(&mut self, client: &WorkspaceClient, view: &WorkspaceView, close: bool) -> Result<()> {
        self.validate()?;
        self.draft.sftp_transfer_filters = ax_ssh::config::SftpTransferFilterSettings::normalized(
            self.draft.sftp_transfer_filters.enabled,
            self.draft.sftp_transfer_filters.use_platform_defaults,
            &self.filter_text,
        );
        // A catalog edit may have occurred while this settings draft was open.
        // Preserve it by rebasing only when the saved settings themselves agree.
        anyhow::ensure!(
            *view.settings == self.base,
            "settings changed; revert or reopen before saving"
        );
        let mut candidate = self.draft.clone();
        candidate.terminal = shells::with_options(&candidate.terminal, &self.discovered_shells);
        self.submitted = Some(client.catalog(
            view.catalog_revision,
            CatalogAction::Settings(Box::new(candidate)),
        )?);
        self.close_after_save = close;
        self.error = None;
        Ok(())
    }
    fn validate(&self) -> Result<()> {
        ax_ssh::audio::AudioFormat {
            sample_rate: self.draft.audio.sample_rate,
            channels: self.draft.audio.channels,
        }
        .validate()?;
        let path = &self.draft.x11.app_path;
        anyhow::ensure!(
            path.chars().count() <= 1024 && !path.chars().any(char::is_control),
            "invalid local X server application path"
        );
        for family in [
            &self.draft.appearance.application_font_family,
            &self.draft.appearance.terminal_font_family,
        ] {
            anyhow::ensure!(
                ax_ssh::font_resources::valid_font_family(family).is_some(),
                "invalid font family"
            );
        }
        shortcuts::validate(&self.draft)?;
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
        self.discovered_shells = shells::discovered(ui.ctx());
        let mut save = false;
        ui.heading(tr!("Settings", ui.ctx()));
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.preview, tr!("Live preview", ui.ctx()));
            ui.label(if self.saving() {
                tr!("Saving…", ui.ctx())
            } else if self.dirty() {
                tr!("Unsaved changes", ui.ctx())
            } else {
                tr!("Saved", ui.ctx())
            });
            save = ui
                .add_enabled(
                    !self.saving() && self.dirty(),
                    egui::Button::new(tr!("Save", ui.ctx())),
                )
                .clicked();
            if ui
                .add_enabled(!self.saving(), egui::Button::new(tr!("Revert", ui.ctx())))
                .clicked()
            {
                self.base = (*view.settings).clone();
                self.draft = self.base.clone();
                self.filter_text = self.base.sftp_transfer_filters.custom_patterns.join("\n");
                self.revision = view.catalog_revision;
                self.error = None;
            }
        });
        ui.add_enabled_ui(!self.saving(), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(tr!("Language", ui.ctx()));
                let name = |language: UiLanguage| {
                    if language == UiLanguage::SYSTEM {
                        tr!("Follow system", ui.ctx())
                    } else {
                        language.native_name()
                    }
                };
                let selected = name(self.draft.ui_language);
                let response = egui::ComboBox::from_id_salt("ui-language")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for language in
                            std::iter::once(UiLanguage::SYSTEM).chain(UiLanguage::bundled())
                        {
                            let label = if language == UiLanguage::SYSTEM {
                                tr!("Follow system", ui.ctx())
                            } else {
                                language.native_name()
                            };
                            ui.selectable_value(&mut self.draft.ui_language, language, label);
                        }
                    });
                response
                    .response
                    .accessible_name(tr!("Interface language", ui.ctx()));
            });
            ui.label(tr!("Language changes apply after Save.", ui.ctx()));
        });
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            for (i, label) in [
                mark!("Appearance"),
                mark!("Terminal"),
                mark!("Workspace"),
                mark!("SFTP"),
                mark!("Keyboard Shortcuts"),
                mark!("X11"),
                mark!("About"),
            ]
            .into_iter()
            .enumerate()
            {
                ui.push_id(i, |ui| {
                    ui.selectable_value(
                        &mut self.category,
                        i,
                        i18n::current(ui.ctx()).text("", label),
                    );
                });
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().id_salt("settings-content").show(ui, |ui| {
            ui.add_enabled_ui(!self.saving(), |ui| match self.category {
                0 => {
                    let fonts = super::fonts::view(ui.ctx());
                    for (id, label, selected) in [
                        ("application-font-family", mark!("Application font family"), &mut self.draft.appearance.application_font_family),
                        ("terminal-font-family", mark!("Terminal font family"), &mut self.draft.appearance.terminal_font_family),
                    ] {
                        ui.horizontal(|ui| {
                            let label = i18n::current(ui.ctx()).text("", label);
                            ui.label(label);
                            egui::ComboBox::from_id_salt(id).selected_text(selected.as_str()).show_ui(ui, |ui| {
                                for family in ax_ssh::font_resources::font_options(selected, &fonts.options) {
                                    ui.selectable_value(selected, family.clone(), &family);
                                }
                            }).response.accessible_name(label);
                        });
                    }
                    if fonts.pending { ui.spinner(); }
                    for warning in fonts.warnings.iter() { ui.colored_label(egui::Color32::YELLOW, warning); }
                    ui.horizontal(|ui| {
                        ui.label(tr!("Display mode", ui.ctx()));
                        egui::ComboBox::from_id_salt("theme-mode").selected_text(mode_label(self.draft.appearance.theme.mode, ui.ctx())).show_ui(ui, |ui| {
                            for mode in [ThemeMode::System,ThemeMode::Light,ThemeMode::Dark] {
                                ui.selectable_value(&mut self.draft.appearance.theme.mode, mode, mode_label(mode, ui.ctx()));
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.label(tr!("Color palette", ui.ctx()));
                        egui::ComboBox::from_id_salt("theme-palette").selected_text(self.draft.appearance.theme.palette.as_setting()).show_ui(ui, |ui| {
                            for name in ["axssh", "solarized", "arctic", "tokyo", "ember", "forest", "custom"] {
                                ui.selectable_value(&mut self.draft.appearance.theme.palette, ax_ssh::config::ThemePaletteKind::from_setting(name), name);
                            }
                        });
                    });
                    if self.draft.appearance.theme.palette == ax_ssh::config::ThemePaletteKind::Custom {
                        appearance::theme_palette(
                            ui,
                            "light",
                            &mut self.draft.appearance.theme.custom_light,
                        );
                        appearance::theme_palette(
                            ui,
                            "dark",
                            &mut self.draft.appearance.theme.custom_dark,
                        );
                    }
                    ui.add(egui::Slider::new(&mut self.draft.appearance.terminal_font_size, ax_ssh::config::MIN_TERMINAL_FONT_SIZE..=ax_ssh::config::MAX_TERMINAL_FONT_SIZE).text(tr!("Terminal font size", ui.ctx())));
                    ui.add(egui::Slider::new(&mut self.draft.appearance.terminal_line_height_percent, ax_ssh::config::MIN_TERMINAL_LINE_HEIGHT..=ax_ssh::config::MAX_TERMINAL_LINE_HEIGHT).text(tr!("Line height", ui.ctx())));
                    appearance::text_colors(ui, &mut self.draft.appearance);
                    icons::show(ui, &mut self.draft.appearance, &mut self.icons);
                    appearance::semantic(ui, &mut self.draft.appearance);
                    appearance::refresh_rates(ui, &mut self.draft.appearance);
                    ui.checkbox(&mut self.draft.appearance.terminal_cursor_blink, tr!("Blink terminal cursor", ui.ctx()));
                    ui.label(tr!("Terminal preview", ui.ctx()));
                    let size = f32::from(self.draft.appearance.terminal_font_size);
                    ui.label(egui::RichText::new("AxSSH $ echo 'Hello 世界'\nHello 世界\n▌").font(super::fonts::font(size, false, false)));
                }
                1 => {
                    terminal::show(ui, &mut self.draft.terminal, &mut self.draft.appearance, &self.discovered_shells);
                    audio::show(ui, &mut self.draft.audio);
                }
                2 => workspace::show(ui, &mut self.draft.workspace),
                3 => sftp::show(ui, &mut self.draft, &mut self.filter_text),
                4 => shortcuts::show(ui, &mut self.draft.shortcuts),
                5 => x11::show(ui, &mut self.draft.x11),
                _ => {
                    ui.heading("AxSSH");
                    ui.label(format!("{} · egui {}", env!("CARGO_PKG_VERSION"), "0.37.0"));
                    ui.label(tr!("A focused terminal workspace for SSH, Telnet, serial, and local shell sessions.", ui.ctx()));
                    ui.hyperlink_to(tr!("Source and licenses", ui.ctx()), "https://github.com/xinalbert/ax_ssh");
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
            ui.heading(tr!("Save settings changes?", ui.ctx()));
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            ui.add_enabled_ui(!self.saving(), |ui| {
                ui.horizontal(|ui| {
                    if ui.button(tr!("Save", ui.ctx())).clicked() {
                        self.save_once(ui, client, view, true);
                    }
                    if ui.button(tr!("Discard", ui.ctx())).clicked() {
                        self.draft = self.base.clone();
                        self.filter_text =
                            self.base.sftp_transfer_filters.custom_patterns.join("\n");
                        self.confirm_close = false;
                        self.close_ready = true;
                    }
                    if ui.button(tr!("Cancel", ui.ctx())).clicked() {
                        self.confirm_close = false;
                        self.exit = false;
                    }
                });
            });
        });
    }
}

fn mode_label(mode: ThemeMode, ctx: &egui::Context) -> &'static str {
    match mode {
        ThemeMode::System => tr!("Follow system", ctx),
        ThemeMode::Light => tr!("Light", ctx),
        ThemeMode::Dark => tr!("Dark", ctx),
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
    async fn shell_discovery_is_transient_until_save_and_new_terminals_use_the_committed_choice() {
        let directory =
            std::env::temp_dir().join(format!("axssh-shell-settings-{}", Uuid::new_v4()));
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
        let ctx = egui::Context::default();
        set_local_shells(&ctx, vec!["fixture-discovered-shell".into()]);
        let mut settings = SettingsUi::new(&view);
        settings.category = 1;
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1200.0),
                )),
                ..Default::default()
            },
            |ui| settings.show(ui, &client, &view),
        )
        .drop_without_applying_deltas();
        assert!(!settings.dirty());
        assert!(
            settings
                .discovered_shells
                .iter()
                .any(|choice| choice == "fixture-discovered-shell")
        );
        assert_eq!(
            client.view().unwrap().settings.terminal,
            view.settings.terminal
        );
        assert_eq!(
            store.load().unwrap().settings.terminal,
            view.settings.terminal
        );
        let selected = format!("axssh-unavailable-{}", Uuid::new_v4());
        settings.draft.terminal.local_shell = selected.clone();
        let draft = settings.draft.clone();
        settings.preview = true;
        assert_eq!(
            settings.effective(&view.settings).terminal.local_shell,
            selected
        );
        assert_eq!(
            client.view().unwrap().settings.terminal,
            view.settings.terminal
        );
        let backup = directory.join("before-shell-save.json");
        std::fs::rename(store.path(), &backup).unwrap();
        std::fs::create_dir(store.path()).unwrap();
        settings.save(&client, &view, false).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while settings.saving() {
                settings.poll(&client, &client.view().unwrap()).unwrap();
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(settings.error.is_some());
        assert_eq!(settings.draft, draft);
        assert_eq!(
            client.view().unwrap().settings.terminal,
            view.settings.terminal
        );
        settings.preview = false;
        assert_eq!(
            settings.effective(&view.settings).terminal,
            view.settings.terminal
        );
        std::fs::remove_dir(store.path()).unwrap();
        std::fs::rename(backup, store.path()).unwrap();
        settings
            .save(&client, &client.view().unwrap(), false)
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while settings.saving() {
                settings.poll(&client, &client.view().unwrap()).unwrap();
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(settings.error.is_none());
        assert!(!settings.dirty());
        let committed = store.load().unwrap().settings.terminal;
        assert_eq!(committed.local_shell, selected);
        assert_eq!(
            committed.known_shells,
            [
                ax_ssh::config::SYSTEM_DEFAULT_SHELL.to_owned(),
                selected.clone(),
                "fixture-discovered-shell".into()
            ]
        );
        client
            .action(ax_ssh::controller::WorkspaceAction::NewLocal)
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let view = client.view().unwrap();
                if let Some(id) = view
                    .tabs
                    .iter()
                    .flat_map(|tab| &tab.panes)
                    .find(|pane| pane.local)
                    .map(|pane| pane.id)
                {
                    let terminal = client.terminal(id).unwrap().unwrap().view().unwrap();
                    if terminal.status.contains(&selected) {
                        assert!(!terminal.connected);
                        break;
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        workspace.shutdown().await.unwrap();
        std::fs::remove_dir_all(directory).unwrap();
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
        settings.draft.appearance.terminal_text_brightness_percent = 62;
        settings.draft.appearance.system_icons_follow_theme = true;
        settings.draft.appearance.local_shell_icon_override = Some("Mac OS".into());
        settings.draft.appearance.focused_terminal_refresh_fps = 0;
        settings.draft.appearance.unfocused_terminal_refresh_fps = u16::MAX;
        settings.draft.appearance.terminal_semantic_highlighting = true;
        settings.draft.appearance.terminal_semantic_colors.success = " #aabbcc ".into();
        settings.draft.appearance.terminal_semantic_colors.info = "#112233".into();
        settings.draft.appearance.terminal_semantic_colors.warning = "#01020380".into();
        settings.draft.appearance.terminal_semantic_colors.error = "invalid".into();
        settings.draft.appearance.bright_bold_text = false;
        settings.draft.appearance.theme.palette = ax_ssh::config::ThemePaletteKind::Custom;
        settings.draft.appearance.theme.custom_light.background = "#111111".into();
        settings.draft.appearance.theme.custom_light.text = "#F7F8F7".into();
        settings.draft.appearance.theme.custom_dark.background = "not-a-color".into();
        settings.draft.appearance.theme.custom_dark.text = "#222222".into();
        settings.draft.shortcuts.open_sftp = "Ctrl+Alt+M".into();
        settings.draft.shortcuts.import_sessions = "Ctrl+Alt+I".into();
        settings.draft.shortcuts.export_selected = "Ctrl+Alt+E".into();
        settings.draft.sftp_transfers.global_uploads = 3;
        settings.draft.sftp_transfers.local_open_max_file_mib = 0;
        settings.draft.appearance.copy_selection_on_select = true;
        settings.draft.terminal.option_as_meta = true;
        settings.draft.terminal.osc52_clipboard = true;
        settings.draft.workspace.session_mask_character = "  #  ".into();
        settings.draft.audio.sample_rate = 96000;
        settings.draft.audio.channels = 4;
        settings.draft.audio.whisper_executable = "/fixture/legacy-whisper".into();
        settings.draft.audio.whisper_model = "/fixture/legacy-model".into();
        settings.draft.x11 = ax_ssh::config::X11Settings {
            provider: ax_ssh::config::X11ServerProvider::Custom,
            app_path: " /fixture/private-x-server ".into(),
            launch_on_connect: false,
            allow_no_auth: true,
        };
        settings.draft.appearance.right_click_copy_or_paste = true;
        settings
            .draft
            .appearance
            .terminal_mouse_local_selection_priority = true;
        settings.draft.appearance.application_font_family = "Iosevka Term".into();
        settings.draft.appearance.terminal_font_family = "Monaspace Neon Var".into();
        settings.filter_text = " *.tmp \n*.tmp\nfolder/ignored\n".into();
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
        assert_eq!(
            settings
                .effective(&view.settings)
                .workspace
                .session_mask_character,
            "  #  "
        );
        settings.preview = false;
        assert_eq!(settings.effective(&view.settings), &*view.settings);
        settings.request_close(false);
        assert!(settings.confirm_close);
        assert!(!settings.poll(&client, &view).unwrap());
        assert_eq!(
            super::super::menus::Command::OpenSftp.shortcut(&client.view().unwrap().settings),
            Some(view.settings.shortcuts.open_sftp.clone())
        );
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
        assert_eq!(
            client.view().unwrap().settings.shortcuts.open_sftp,
            "Ctrl+Alt+M"
        );
        assert_eq!(
            store.load().unwrap().settings.shortcuts.open_sftp,
            "Ctrl+Alt+M"
        );
        assert_eq!(
            client.view().unwrap().settings.shortcuts.import_sessions,
            "Ctrl+Alt+I"
        );
        assert_eq!(
            store.load().unwrap().settings.shortcuts.export_selected,
            "Ctrl+Alt+E"
        );
        assert!(!settings.dirty());
        assert!(!settings.confirm_close);
        assert_eq!(settings.draft.workspace.session_mask_character, "#");
        assert_eq!(
            store
                .load()
                .unwrap()
                .settings
                .workspace
                .session_mask_character,
            "#"
        );
        assert_eq!(settings.filter_text, "*.tmp");
        let appearance = store.load().unwrap().settings.appearance;
        assert_eq!(
            appearance.theme.palette,
            ax_ssh::config::ThemePaletteKind::Custom
        );
        assert_ne!(appearance.theme.custom_light.background, "#111111");
        assert_ne!(appearance.theme.custom_light.text, "#F7F8F7");
        assert_eq!(
            appearance.theme.custom_dark.background,
            ax_ssh::config::ThemePalette::axssh_dark().background
        );
        assert_ne!(appearance.theme.custom_dark.text, "#222222");
        assert_eq!(appearance.terminal_text_brightness_percent, 60);
        assert!(appearance.system_icons_follow_theme);
        assert_eq!(
            appearance.local_shell_icon_override.as_deref(),
            Some("macos")
        );
        assert_eq!(appearance.focused_terminal_refresh_fps, 1);
        assert_eq!(appearance.unfocused_terminal_refresh_fps, 120);
        assert!(appearance.terminal_semantic_highlighting);
        assert_eq!(appearance.terminal_semantic_colors.success, "#AABBCC");
        assert_eq!(appearance.terminal_semantic_colors.info, "#112233");
        assert!(appearance.terminal_semantic_colors.warning.is_empty());
        assert!(appearance.terminal_semantic_colors.error.is_empty());
        assert!(!appearance.bright_bold_text);
        assert!(store.load().unwrap().settings.terminal.option_as_meta);
        assert!(store.load().unwrap().settings.terminal.osc52_clipboard);
        let audio = store.load().unwrap().settings.audio;
        assert_eq!((audio.sample_rate, audio.channels), (96000, 4));
        assert_eq!(audio.whisper_executable, "/fixture/legacy-whisper");
        assert_eq!(audio.whisper_model, "/fixture/legacy-model");
        let x11 = store.load().unwrap().settings.x11;
        assert_eq!(x11.provider, ax_ssh::config::X11ServerProvider::Custom);
        assert_eq!(x11.app_path, "/fixture/private-x-server");
        assert!(!x11.launch_on_connect);
        assert!(x11.allow_no_auth);
        assert_eq!(
            store.load().unwrap().settings.sftp_transfers.global_uploads,
            3
        );
        assert_eq!(
            store
                .load()
                .unwrap()
                .settings
                .sftp_transfers
                .local_open_max_file_mib,
            0
        );
        assert!(appearance.copy_selection_on_select);
        assert!(appearance.right_click_copy_or_paste);
        assert!(appearance.terminal_mouse_local_selection_priority);
        assert_eq!(appearance.application_font_family, "Iosevka Term");
        assert_eq!(appearance.terminal_font_family, "Monaspace Neon Var");
        assert_eq!(
            store
                .load()
                .unwrap()
                .settings
                .sftp_transfer_filters
                .custom_patterns,
            ["*.tmp"]
        );
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

    #[tokio::test]
    async fn language_draft_and_failed_save_never_change_the_active_translation() {
        let directory = std::env::temp_dir().join(format!("axssh-language-ui-{}", Uuid::new_v4()));
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
        let ctx = egui::Context::default();
        let mut settings = SettingsUi::new(&view);
        settings.draft.ui_language = UiLanguage::from_setting("ja");
        settings.draft.appearance.terminal_text_brightness_percent = 120;
        settings.draft.appearance.system_icons_follow_theme = true;
        settings.draft.appearance.local_shell_icon_override = Some("debian".into());
        settings.draft.appearance.focused_terminal_refresh_fps = 30;
        settings.draft.appearance.unfocused_terminal_refresh_fps = 2;
        settings.draft.appearance.terminal_semantic_highlighting = true;
        settings.draft.appearance.terminal_semantic_colors.error = "#ABCDEF".into();
        settings.draft.appearance.bright_bold_text = false;
        settings.draft.workspace.session_mask_character = "•".into();
        settings.draft.audio.sample_rate = 8000;
        settings.draft.audio.channels = 1;
        settings.draft.x11.allow_no_auth = true;
        settings.draft.x11.launch_on_connect = false;
        settings.draft.x11.app_path = "/fixture/uncommitted-x-server".into();
        settings.draft.sftp_transfers.local_open_max_file_mib = 1;
        settings.draft.sftp_transfers.global_uploads = 3;
        settings.draft.terminal.option_as_meta = true;
        settings.draft.terminal.osc52_clipboard = true;
        settings.draft.appearance.copy_selection_on_select = true;
        settings.draft.appearance.right_click_copy_or_paste = true;
        settings
            .draft
            .appearance
            .terminal_mouse_local_selection_priority = true;
        settings.draft.appearance.terminal_font_family = "Iosevka Term".into();
        settings.draft.shortcuts.open_sftp = "Ctrl+Alt+M".into();
        settings.draft.shortcuts.import_sessions = "Ctrl+Alt+I".into();
        settings.draft.shortcuts.export_selected = "Ctrl+Alt+E".into();
        settings.preview = true;
        i18n::Translator::new(view.settings.ui_language, Some("en")).apply(&ctx);
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            settings.show(ui, &client, &view)
        });
        output.drop_without_applying_deltas();
        assert_eq!(i18n::current(&ctx).text("", "Settings"), "Settings");
        assert!(settings.dirty());
        assert_eq!(client.view().unwrap().settings.x11, view.settings.x11);
        assert_eq!(client.view().unwrap().settings.audio, view.settings.audio);
        let backup = directory.join("original.json");
        std::fs::rename(store.path(), &backup).unwrap();
        std::fs::create_dir(store.path()).unwrap();
        settings.save(&client, &view, false).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let current = client.view().unwrap();
                settings.poll(&client, &current).unwrap();
                i18n::Translator::new(current.settings.ui_language, Some("en")).apply(&ctx);
                if !settings.saving() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(settings.draft.shortcuts.open_sftp, "Ctrl+Alt+M");
        assert_eq!(settings.draft.shortcuts.import_sessions, "Ctrl+Alt+I");
        assert_eq!(settings.draft.shortcuts.export_selected, "Ctrl+Alt+E");
        assert_eq!(
            client.view().unwrap().settings.shortcuts,
            view.settings.shortcuts
        );
        assert!(settings.error.is_some());
        assert!(settings.draft.appearance.system_icons_follow_theme);
        assert_eq!(
            settings
                .draft
                .appearance
                .local_shell_icon_override
                .as_deref(),
            Some("debian")
        );
        assert_eq!(settings.draft.appearance.focused_terminal_refresh_fps, 30);
        assert_eq!(settings.draft.appearance.unfocused_terminal_refresh_fps, 2);
        assert_eq!(
            client.view().unwrap().settings.appearance,
            view.settings.appearance
        );
        assert_eq!(
            settings
                .effective(&view.settings)
                .appearance
                .terminal_text_brightness_percent,
            120
        );
        assert!(
            !settings
                .effective(&view.settings)
                .appearance
                .bright_bold_text
        );
        assert!(
            settings
                .effective(&view.settings)
                .appearance
                .terminal_semantic_highlighting
        );
        assert_eq!(
            settings
                .effective(&view.settings)
                .appearance
                .terminal_semantic_colors
                .error,
            "#ABCDEF"
        );
        assert_eq!(settings.draft.workspace.session_mask_character, "•");
        assert_eq!(
            client.view().unwrap().settings.workspace,
            view.settings.workspace
        );
        assert_eq!(
            settings
                .effective(&view.settings)
                .workspace
                .session_mask_character,
            "•"
        );
        assert!(!client.view().unwrap().settings.terminal.option_as_meta);
        assert!(!client.view().unwrap().settings.terminal.osc52_clipboard);
        assert_eq!(
            client
                .view()
                .unwrap()
                .settings
                .sftp_transfers
                .local_open_max_file_mib,
            view.settings.sftp_transfers.local_open_max_file_mib
        );
        assert_eq!(
            client
                .view()
                .unwrap()
                .settings
                .sftp_transfers
                .global_uploads,
            view.settings.sftp_transfers.global_uploads
        );
        assert!(
            !client
                .view()
                .unwrap()
                .settings
                .appearance
                .copy_selection_on_select
        );
        assert!(
            !client
                .view()
                .unwrap()
                .settings
                .appearance
                .right_click_copy_or_paste
        );
        assert!(
            !client
                .view()
                .unwrap()
                .settings
                .appearance
                .terminal_mouse_local_selection_priority
        );
        assert_eq!(
            settings.draft.appearance.terminal_font_family,
            "Iosevka Term"
        );
        assert_eq!(
            settings
                .effective(&view.settings)
                .appearance
                .terminal_font_family,
            "Iosevka Term"
        );
        settings.preview = false;
        assert_eq!(
            settings.effective(&view.settings).appearance,
            view.settings.appearance
        );
        assert_eq!(client.view().unwrap().settings.x11, view.settings.x11);
        assert_eq!(client.view().unwrap().settings.audio, view.settings.audio);
        assert_eq!(
            settings
                .effective(&view.settings)
                .appearance
                .terminal_font_family,
            view.settings.appearance.terminal_font_family
        );
        settings.preview = true;
        assert_eq!(settings.draft.ui_language.as_setting(), "ja");
        assert_eq!(i18n::current(&ctx).text("", "Settings"), "Settings");
        std::fs::remove_dir(store.path()).unwrap();
        std::fs::rename(backup, store.path()).unwrap();
        settings
            .save(&client, &client.view().unwrap(), false)
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let current = client.view().unwrap();
                settings.poll(&client, &current).unwrap();
                i18n::Translator::new(current.settings.ui_language, Some("en")).apply(&ctx);
                if !settings.saving() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            client.view().unwrap().settings.shortcuts.open_sftp,
            "Ctrl+Alt+M"
        );
        assert_eq!(
            store.load().unwrap().settings.shortcuts.open_sftp,
            "Ctrl+Alt+M"
        );
        assert_eq!(
            client.view().unwrap().settings.shortcuts.import_sessions,
            "Ctrl+Alt+I"
        );
        assert_eq!(
            store.load().unwrap().settings.shortcuts.export_selected,
            "Ctrl+Alt+E"
        );
        assert_eq!(i18n::current(&ctx).text("", "Settings"), "設定");
        assert_eq!(client.view().unwrap().settings.audio, settings.draft.audio);
        assert_eq!(store.load().unwrap().settings.audio, settings.draft.audio);
        assert!(
            store
                .load()
                .unwrap()
                .settings
                .appearance
                .system_icons_follow_theme
        );
        assert_eq!(
            store
                .load()
                .unwrap()
                .settings
                .appearance
                .local_shell_icon_override
                .as_deref(),
            Some("debian")
        );
        assert_eq!(
            store
                .load()
                .unwrap()
                .settings
                .appearance
                .focused_terminal_refresh_fps,
            30
        );
        assert_eq!(
            store
                .load()
                .unwrap()
                .settings
                .appearance
                .unfocused_terminal_refresh_fps,
            2
        );
        assert_eq!(
            client
                .view()
                .unwrap()
                .settings
                .appearance
                .terminal_text_brightness_percent,
            120
        );
        assert!(!store.load().unwrap().settings.appearance.bright_bold_text);
        assert_eq!(
            store
                .load()
                .unwrap()
                .settings
                .appearance
                .terminal_semantic_colors
                .error,
            "#ABCDEF"
        );
        assert!(
            client
                .view()
                .unwrap()
                .settings
                .appearance
                .terminal_semantic_highlighting
        );
        assert_eq!(
            client
                .view()
                .unwrap()
                .settings
                .workspace
                .session_mask_character,
            "•"
        );
        assert_eq!(
            store
                .load()
                .unwrap()
                .settings
                .workspace
                .session_mask_character,
            "•"
        );
        assert_eq!(
            store.load().unwrap().settings.ui_language.as_setting(),
            "ja"
        );
        assert!(!settings.dirty());
        workspace.shutdown().await.unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
}
