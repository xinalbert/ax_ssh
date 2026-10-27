use super::*;

mod sftp_shortcuts;

#[test]
fn sidebar_and_tabs_consume_icon_preview_revert_and_release_cached_textures() {
    use ax_ssh::controller::{PaneLayout, PanePlacement, WorkspacePaneView, WorkspaceTabView};
    let mut settings = ax_ssh::config::AppSettings::default();
    settings.appearance.local_shell_icon_override = Some("windows".into());
    let mut profile =
        ax_ssh::config::SessionProfile::new("Ubuntu is a user title", "example.invalid", "fixture");
    profile.system_icon_override = Some("debian".into());
    let local = Uuid::new_v4();
    let remote = profile.id;
    let view = WorkspaceView {
        ready: true,
        busy: false,
        error: None,
        status: String::new(),
        catalog_revision: 0,
        settings: std::sync::Arc::new(settings),
        profiles: std::sync::Arc::new(vec![profile.clone()]),
        groups: std::sync::Arc::new(vec![]),
        active_tab_id: Some(local),
        tabs: [(local, true, None), (remote, false, Some(profile))]
            .into_iter()
            .map(|(id, local, profile)| WorkspaceTabView {
                id,
                title: "User tab title".into(),
                settings: false,
                panes: vec![WorkspacePaneView {
                    id,
                    title: "User tab title".into(),
                    profile,
                    local,
                    sftp: false,
                }],
                layout: PaneLayout {
                    panes: vec![PanePlacement {
                        tab_id: id,
                        x: 0.0,
                        y: 0.0,
                        width: 1.0,
                        height: 1.0,
                        focused: true,
                    }],
                    dividers: vec![],
                },
            })
            .collect(),
    };
    let fonts = crate::egui_app::fonts::load().unwrap();
    for sidebar in [true, false] {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts.clone());
        let mut surface = WorkspaceUi::default();
        let mut preview = view.settings.appearance.clone();
        preview.local_shell_icon_override = Some("macos".into());
        preview.system_icons_follow_theme = true;
        let mut allocated = std::collections::HashSet::new();
        for (index, appearance) in [
            &view.settings.appearance,
            &preview,
            &view.settings.appearance,
        ]
        .into_iter()
        .enumerate()
        {
            let mut emitted = 0;
            for _ in 0..2 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1600.0, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        if sidebar {
                            assert!(
                                surface
                                    .sidebar(ui, &view, &view.settings.workspace, appearance)
                                    .is_none()
                            );
                        } else {
                            assert!(surface.tabs(ui, &view, 220.0, appearance).is_none());
                        }
                    },
                );
                for (id, deltas) in &output.textures_delta.set {
                    for delta in deltas {
                        let egui::ImageData::Color(image) = &delta.image;
                        if image.size == [48, 48] {
                            emitted += 1;
                            allocated.insert(*id);
                            if index == 1 {
                                assert!(
                                    image
                                        .pixels
                                        .iter()
                                        .all(|pixel| pixel.to_array() == [pixel.a(); 4])
                                );
                            }
                        }
                    }
                }
                output.drop_without_applying_deltas();
            }
            assert_eq!(
                emitted,
                if index == 2 {
                    0
                } else if sidebar {
                    1
                } else {
                    2
                }
            );
        }
        assert_eq!(
            view.settings
                .appearance
                .local_shell_icon_override
                .as_deref(),
            Some("windows")
        );
        assert!(!view.settings.appearance.system_icons_follow_theme);
        surface.clear_caches();
        let output = ctx.run_ui(egui::RawInput::default(), |_| {});
        assert!(
            allocated
                .iter()
                .all(|id| output.textures_delta.free.contains(id))
        );
        output.drop_without_applying_deltas();
    }
}
use ax_ssh::config::{ConfigStore, SessionStore, WORKSPACE_SNAPSHOT_VERSION, WorkspaceSnapshot};
use ax_ssh::controller::Workspace;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn sidebar_mask_preview_preserves_raw_search_profiles_and_saved_settings() {
    use crate::egui_app::i18n::Translator;
    use ax_ssh::config::{SessionProfile, UiLanguage};
    fn contains(shape: &egui::epaint::Shape, expected: &str) -> bool {
        match shape {
            egui::epaint::Shape::Text(text) => text.galley.job.text == expected,
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().any(|shape| contains(shape, expected))
            }
            _ => false,
        }
    }
    let directory = std::env::temp_dir().join(format!("axssh-sidebar-mask-{}", Uuid::new_v4()));
    let store = ConfigStore::new(directory.join("sessions.json"));
    let profile = SessionProfile::new("Original server", "192.168.1.202", "zhushixin");
    store
        .save(&SessionStore {
            sessions: vec![profile.clone()],
            ..Default::default()
        })
        .unwrap();
    store
        .save_workspace(&WorkspaceSnapshot {
            version: WORKSPACE_SNAPSHOT_VERSION,
            ..Default::default()
        })
        .unwrap();
    let original_file = std::fs::read(store.path()).unwrap();
    let workspace = Workspace::spawn(
        &tokio::runtime::Handle::current(),
        store.clone(),
        Arc::new(|| {}),
    );
    let client = workspace.client();
    let view = wait(&client, |view| view.ready).await;
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::egui_app::fonts::load().unwrap());
    let mut surface = WorkspaceUi {
        search: "168.1".into(),
        ..Default::default()
    };
    for language in UiLanguage::bundled() {
        Translator::new(language, None).apply(&ctx);
        for (mask, expected) in [
            ("#", "zh#in@192.#.1.202:22"),
            ("•", "zh•in@192.•.1.202:22"),
            (" ", "zh*in@192.*.1.202:22"),
        ] {
            let mut draft = view.settings.workspace.clone();
            draft.session_mask_character = mask.into();
            // Warm up CollapsingHeader geometry before asserting visible text.
            for _ in 0..2 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1400.0, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        assert!(
                            surface
                                .sidebar(ui, &view, &draft, &view.settings.appearance)
                                .is_none()
                        );
                    },
                );
                output.drop_without_applying_deltas();
            }
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    assert!(
                        surface
                            .sidebar(ui, &view, &draft, &view.settings.appearance)
                            .is_none()
                    );
                },
            );
            assert!(
                output
                    .shapes
                    .iter()
                    .any(|shape| contains(&shape.shape, expected)),
                "{}: {expected}",
                language.as_setting()
            );
            assert!(
                output
                    .shapes
                    .iter()
                    .any(|shape| contains(&shape.shape, "Original server"))
            );
            output.drop_without_applying_deltas();
            assert_eq!(draft.session_mask_character, mask);
            assert_eq!(view.profiles.as_ref(), std::slice::from_ref(&profile));
            assert_eq!(std::fs::read(store.path()).unwrap(), original_file);
        }
    }
    assert!(client.view().unwrap().tabs.is_empty());
    workspace.shutdown().await.unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn translated_workspace_keeps_user_names_paths_and_editor_drafts_unchanged() {
    use crate::egui_app::i18n::Translator;
    use ax_ssh::config::{SessionProfile, UiLanguage};
    fn text_present(shape: &egui::epaint::Shape, expected: &str) -> bool {
        match shape {
            egui::epaint::Shape::Text(text) => text.galley.job.text == expected,
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().any(|shape| text_present(shape, expected))
            }
            _ => false,
        }
    }
    let directory =
        std::env::temp_dir().join(format!("axssh-workspace-language-{}", Uuid::new_v4()));
    let store = ConfigStore::new(directory.join("sessions.json"));
    let mut profile = SessionProfile::new("Settings", "unchanged.example", "original-user");
    profile.group_name = "Connection".into();
    let sessions = SessionStore {
        sessions: vec![profile.clone()],
        groups: vec!["Connection".into()],
        ..Default::default()
    };
    store.save(&sessions).unwrap();
    store
        .save_workspace(&WorkspaceSnapshot {
            version: WORKSPACE_SNAPSHOT_VERSION,
            ..Default::default()
        })
        .unwrap();
    let original_file = std::fs::read(store.path()).unwrap();
    let workspace = Workspace::spawn(
        &tokio::runtime::Handle::current(),
        store.clone(),
        Arc::new(|| {}),
    );
    let client = workspace.client();
    let view = wait(&client, |view| view.ready).await;
    let mut surface = WorkspaceUi {
        editor: Some(Editor::new(Some(profile.clone()), view.catalog_revision)),
        groups_open: true,
        group_old: "Connection".into(),
        group_new: "Settings".into(),
        file_open: true,
        file_path: "/fixture/Settings.axssh".into(),
        delete: Some(profile.id),
        ..Default::default()
    };
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::egui_app::fonts::load().unwrap());
    for language in UiLanguage::bundled() {
        Translator::new(language, None).apply(&ctx);
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 1200.0),
                )),
                ..Default::default()
            },
            |ui| {
                assert!(
                    surface
                        .sidebar(
                            ui,
                            &view,
                            &view.settings.workspace,
                            &view.settings.appearance
                        )
                        .is_none()
                );
                assert!(surface.dialogs(ui.ctx(), &view).is_none());
            },
        );
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| text_present(&shape.shape, "Settings"))
        );
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| text_present(&shape.shape, "Connection"))
        );
        output.drop_without_applying_deltas();
        assert_eq!(surface.editor.as_ref().unwrap().profile().unwrap(), profile);
        assert_eq!(surface.file_path, "/fixture/Settings.axssh");
        assert_eq!(surface.group_old, "Connection");
        assert_eq!(surface.group_new, "Settings");
        assert_eq!(surface.delete, Some(profile.id));
        assert_eq!(std::fs::read(store.path()).unwrap(), original_file);
    }
    workspace.shutdown().await.unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn multiple_layout_passes_admit_one_split_and_tiny_layouts_remain_bounded() {
    let directory =
        std::env::temp_dir().join(format!("axssh-egui-workspace-ui-{}", Uuid::new_v4()));
    let store = ConfigStore::new(directory.join("sessions.json"));
    let mut sessions = SessionStore::default();
    sessions.settings.terminal.local_shell = "axssh-no-such-fixture-shell".into();
    store.save(&sessions).unwrap();
    store
        .save_workspace(&WorkspaceSnapshot {
            version: WORKSPACE_SNAPSHOT_VERSION,
            ..Default::default()
        })
        .unwrap();
    let workspace = Workspace::spawn(&tokio::runtime::Handle::current(), store, Arc::new(|| {}));
    let client = workspace.client();
    wait(&client, |view| view.ready).await;
    client.action(WorkspaceAction::NewConnection).unwrap();
    wait(&client, |view| view.tabs.len() == 1).await;
    let ctx = egui::Context::default();
    ctx.set_fonts(super::super::fonts::load().unwrap());
    let mut workspace_ui = WorkspaceUi::default();
    let mut passes = 0;
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1400.0, 900.0),
            )),
            focused: true,
            events: vec![egui::Event::Key {
                key: egui::Key::D,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
            ..Default::default()
        },
        |ui| {
            workspace_ui.show(ui, &client).unwrap();
            passes += 1;
            if passes == 1 {
                ui.ctx()
                    .request_discard("verify one action across layout passes");
            }
        },
    );
    output.drop_without_applying_deltas();
    assert!(passes >= 2);
    let view = wait(&client, |view| view.tabs[0].panes.len() >= 2).await;
    assert_eq!(view.tabs[0].panes.len(), 2);
    for _ in 0..6 {
        client
            .action(WorkspaceAction::Split(PaneDirection::Right))
            .unwrap();
    }
    wait(&client, |view| view.tabs[0].panes.len() == 8).await;
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(400.0, 240.0),
            )),
            ..Default::default()
        },
        |ui| {
            workspace_ui.show(ui, &client).unwrap();
        },
    );
    output.drop_without_applying_deltas();
    assert!(workspace_ui.panes.len() <= 8);
    workspace.shutdown().await.unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

async fn wait(
    client: &WorkspaceClient,
    predicate: impl Fn(&WorkspaceView) -> bool,
) -> WorkspaceView {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let view = client.view().unwrap();
            if predicate(&view) {
                return view;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn menu_commands_are_consumed_once_and_settings_close_keeps_the_draft() {
    let directory = std::env::temp_dir().join(format!("axssh-menu-ui-{}", Uuid::new_v4()));
    let store = ConfigStore::new(directory.join("sessions.json"));
    store.save(&SessionStore::default()).unwrap();
    store
        .save_workspace(&WorkspaceSnapshot {
            version: WORKSPACE_SNAPSHOT_VERSION,
            ..Default::default()
        })
        .unwrap();
    let workspace = Workspace::spawn(&tokio::runtime::Handle::current(), store, Arc::new(|| {}));
    let client = workspace.client();
    wait(&client, |view| view.ready).await;
    let ctx = egui::Context::default();
    let mut workspace_ui = WorkspaceUi {
        menu_command: Some(super::super::menus::Command::ToggleSidebar),
        ..Default::default()
    };
    let mut passes = 0;
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        workspace_ui.show(ui, &client).unwrap();
        passes += 1;
        if passes == 1 {
            ui.ctx()
                .request_discard("verify menu action de-duplication");
        }
    });
    output.drop_without_applying_deltas();
    assert!(passes >= 2);
    assert!(workspace_ui.sidebar_hidden);
    client.action(WorkspaceAction::OpenSettings).unwrap();
    let view = wait(&client, |view| view.tabs.len() == 1).await;
    let id = view.active_tab_id.unwrap();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        workspace_ui.show(ui, &client).unwrap();
    });
    output.drop_without_applying_deltas();
    workspace_ui
        .settings
        .as_mut()
        .unwrap()
        .draft
        .appearance
        .terminal_font_size = 27;
    workspace_ui
        .apply(
            Action::Layout(WorkspaceAction::CloseTab(id)),
            &client,
            &view,
        )
        .unwrap();
    assert!(workspace_ui.settings.as_ref().unwrap().confirm_close);
    assert_eq!(client.view().unwrap().tabs.len(), 1);
    assert_eq!(
        workspace_ui
            .settings
            .as_ref()
            .unwrap()
            .draft
            .appearance
            .terminal_font_size,
        27
    );
    workspace.shutdown().await.unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}
