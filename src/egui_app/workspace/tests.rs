use super::*;
use ax_ssh::config::{ConfigStore, SessionStore, WORKSPACE_SNAPSHOT_VERSION, WorkspaceSnapshot};
use ax_ssh::controller::Workspace;
use std::sync::Arc;
use std::time::Duration;

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
