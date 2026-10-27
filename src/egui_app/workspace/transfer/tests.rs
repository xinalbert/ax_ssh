use super::*;
use ax_ssh::config::{
    ConfigStore, SessionProfile, SessionStore, WORKSPACE_SNAPSHOT_VERSION, WorkspaceSnapshot,
};
use ax_ssh::controller::Workspace;
use std::{path::PathBuf, sync::Arc, time::Duration};

struct Fixture {
    directory: PathBuf,
    store: ConfigStore,
    workspace: Workspace,
    client: WorkspaceClient,
    view: WorkspaceView,
    json: String,
}

impl Fixture {
    async fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("axssh-transfer-ui-{}", Uuid::new_v4()));
        let store = ConfigStore::new(directory.join("sessions.json"));
        let mut sessions = SessionStore::default();
        let profile = SessionProfile::new("User session", "example.invalid", "fixture");
        let id = profile.id;
        sessions.sessions.push(profile);
        sessions.add_group("User empty group").unwrap();
        store.save(&sessions).unwrap();
        store
            .save_workspace(&WorkspaceSnapshot {
                version: WORKSPACE_SNAPSHOT_VERSION,
                ..Default::default()
            })
            .unwrap();
        let json =
            ax_ssh::session_transfer::export_session_profile(&sessions, &id.to_string()).unwrap();
        let workspace = Workspace::spawn(
            &tokio::runtime::Handle::current(),
            store.clone(),
            Arc::new(|| {}),
        );
        let client = workspace.client();
        let view = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let view = client.view().unwrap();
                if view.ready {
                    break view;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        Self {
            directory,
            store,
            workspace,
            client,
            view,
            json,
        }
    }
    async fn finish(self) {
        self.workspace.shutdown().await.unwrap();
        std::fs::remove_dir_all(self.directory).unwrap();
    }
}

fn frame(
    ctx: &egui::Context,
    surface: &mut WorkspaceUi,
    client: &WorkspaceClient,
    events: Vec<egui::Event>,
    focused: bool,
) -> egui::FullOutput {
    ctx.run_ui(
        egui::RawInput {
            focused,
            events,
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 1000.0),
            )),
            ..Default::default()
        },
        |ui| {
            surface.show(ui, client).unwrap();
        },
    )
}

fn copies(output: &egui::FullOutput) -> Vec<&str> {
    output
        .platform_output
        .commands
        .iter()
        .filter_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

async fn receipt(client: &WorkspaceClient, request: Uuid) -> ax_ssh::controller::CatalogResult {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(result) = client.take_catalog_result(request).unwrap() {
                break result;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}

async fn flush(fixture: &Fixture, revision: u64) {
    let request = fixture
        .client
        .catalog(
            revision,
            CatalogAction::Export(SessionExportTarget::Server(fixture.view.profiles[0].id)),
        )
        .unwrap();
    assert!(receipt(&fixture.client, request).await.error.is_none());
}

#[tokio::test]
async fn import_menu_paste_and_retry_preserve_drafts_and_commit_only_once() {
    let fixture = Fixture::new().await;
    let ctx = egui::Context::default();
    let mut surface = WorkspaceUi {
        menu_command: Some(crate::egui_app::menus::Command::ImportSessions),
        ..Default::default()
    };
    for _ in 0..2 {
        frame(&ctx, &mut surface, &fixture.client, vec![], true).drop_without_applying_deltas();
    }
    assert!(surface.blocked(&fixture.view));
    frame(
        &ctx,
        &mut surface,
        &fixture.client,
        vec![egui::Event::Paste(fixture.json.clone())],
        true,
    )
    .drop_without_applying_deltas();
    assert_eq!(surface.transfer.as_ref().unwrap().text, fixture.json);
    // A multi-byte draft must pass the byte budget before it can be cloned into a command.
    let dialog = surface.transfer.as_mut().unwrap();
    dialog.text = "界".repeat(MAX_SESSION_TRANSFER_BYTES / 3 + 1);
    ctx.run_ui(egui::RawInput::default(), |ui| {
        dialog.submit(ui, &fixture.client);
    })
    .drop_without_applying_deltas();
    assert!(dialog.submitted.is_none());
    assert!(dialog.error.is_some());
    dialog.text = fixture.json.clone();
    let backup = fixture.directory.join("before.json");
    std::fs::rename(fixture.store.path(), &backup).unwrap();
    std::fs::create_dir(fixture.store.path()).unwrap();
    ctx.run_ui(egui::RawInput::default(), |ui| {
        let dialog = surface.transfer.as_mut().unwrap();
        ui.scope_builder(egui::UiBuilder::new().sizing_pass(), |ui| {
            dialog.submit(ui, &fixture.client);
        });
        assert!(dialog.submitted.is_none());
        dialog.submit(ui, &fixture.client);
        let request = dialog.submitted.unwrap();
        dialog.submit(ui, &fixture.client);
        assert_eq!(dialog.submitted, Some(request));
        surface.show(ui, &fixture.client).unwrap();
    })
    .drop_without_applying_deltas();
    flush(&fixture, 0).await;
    frame(&ctx, &mut surface, &fixture.client, vec![], true).drop_without_applying_deltas();
    let dialog = surface.transfer.as_ref().unwrap();
    assert!(dialog.error.is_some());
    assert_eq!(dialog.text, fixture.json);
    assert_eq!(fixture.client.view().unwrap().profiles.len(), 1);
    std::fs::remove_dir(fixture.store.path()).unwrap();
    std::fs::rename(backup, fixture.store.path()).unwrap();
    ctx.run_ui(egui::RawInput::default(), |ui| {
        surface
            .transfer
            .as_mut()
            .unwrap()
            .submit(ui, &fixture.client);
        surface.show(ui, &fixture.client).unwrap();
    })
    .drop_without_applying_deltas();
    flush(&fixture, 1).await;
    frame(&ctx, &mut surface, &fixture.client, vec![], true).drop_without_applying_deltas();
    assert!(surface.transfer.is_none());
    assert_eq!(fixture.store.load().unwrap().sessions.len(), 2);
    assert_eq!(
        fixture.store.load().unwrap().settings,
        *fixture.view.settings
    );
    assert!(fixture.client.view().unwrap().tabs.is_empty());
    fixture.finish().await;
}

#[tokio::test]
async fn export_is_explicit_and_copies_one_sanitized_receipt_across_passes() {
    let fixture = Fixture::new().await;
    let ctx = egui::Context::default();
    let mut surface = WorkspaceUi {
        transfer: Some(TransferUi::export(&fixture.view)),
        ..Default::default()
    };
    let output = frame(&ctx, &mut surface, &fixture.client, vec![], true);
    assert!(copies(&output).is_empty());
    output.drop_without_applying_deltas();
    ctx.run_ui(egui::RawInput::default(), |ui| {
        surface
            .transfer
            .as_mut()
            .unwrap()
            .submit(ui, &fixture.client);
        surface.show(ui, &fixture.client).unwrap();
    })
    .drop_without_applying_deltas();
    flush(&fixture, 0).await;
    let mut passes = 0;
    let output = ctx.run_ui(
        egui::RawInput {
            focused: true,
            ..Default::default()
        },
        |ui| {
            surface.show(ui, &fixture.client).unwrap();
            passes += 1;
            if passes == 1 {
                ui.ctx().request_discard("verify one clipboard result");
            }
        },
    );
    assert!(passes >= 2);
    assert_eq!(copies(&output), vec![fixture.json.as_str()]);
    output.drop_without_applying_deltas();
    let output = frame(&ctx, &mut surface, &fixture.client, vec![], true);
    assert!(copies(&output).is_empty());
    output.drop_without_applying_deltas();
    assert_eq!(fixture.client.view().unwrap().catalog_revision, 0);
    fixture.finish().await;
}

#[tokio::test]
async fn closed_stale_or_unfocused_exports_never_change_the_clipboard() {
    let fixture = Fixture::new().await;
    let ctx = egui::Context::default();
    for case in 0..3 {
        let current = fixture.client.view().unwrap();
        let mut surface = WorkspaceUi {
            transfer: Some(TransferUi::export(&current)),
            ..Default::default()
        };
        for _ in 0..2 {
            frame(&ctx, &mut surface, &fixture.client, vec![], true).drop_without_applying_deltas();
        }
        ctx.run_ui(egui::RawInput::default(), |ui| {
            surface
                .transfer
                .as_mut()
                .unwrap()
                .submit(ui, &fixture.client);
            surface.show(ui, &fixture.client).unwrap();
        })
        .drop_without_applying_deltas();
        let revision = if case == 1 {
            let request = fixture
                .client
                .catalog(
                    current.catalog_revision,
                    CatalogAction::AddGroup("Changed".into()),
                )
                .unwrap();
            let result = receipt(&fixture.client, request).await;
            assert!(result.error.is_none());
            result.revision
        } else {
            current.catalog_revision
        };
        flush(&fixture, revision).await;
        let events = if case == 0 {
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        } else {
            vec![]
        };
        let output = frame(&ctx, &mut surface, &fixture.client, events, case != 2);
        assert!(copies(&output).is_empty(), "case {case}");
        output.drop_without_applying_deltas();
        if case == 0 {
            assert!(surface.transfer.is_none());
            let output = frame(&ctx, &mut surface, &fixture.client, vec![], true);
            assert!(copies(&output).is_empty());
            output.drop_without_applying_deltas();
            assert!(surface.transactions.is_empty());
        } else {
            assert!(surface.transfer.as_ref().unwrap().error.is_some());
        }
    }
    fixture.finish().await;
}

#[tokio::test]
async fn localized_import_keeps_a_stable_json_field_and_does_not_touch_the_catalog() {
    let fixture = Fixture::new().await;
    let fonts = crate::egui_app::fonts::load().unwrap();
    let mut stable = None;
    for language in ax_ssh::config::UiLanguage::bundled() {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts.clone());
        crate::egui_app::i18n::Translator::new(language, None).apply(&ctx);
        let mut surface = WorkspaceUi {
            transfer: Some(TransferUi::import(&fixture.view)),
            ..Default::default()
        };
        for _ in 0..2 {
            frame(&ctx, &mut surface, &fixture.client, vec![], true).drop_without_applying_deltas();
        }
        let focus = ctx.memory(|memory| memory.focused()).unwrap();
        assert_eq!(*stable.get_or_insert(focus), focus);
        let output = frame(
            &ctx,
            &mut surface,
            &fixture.client,
            vec![egui::Event::Paste(fixture.json.clone())],
            true,
        );
        assert!(copies(&output).is_empty());
        output.drop_without_applying_deltas();
        assert_eq!(surface.transfer.as_ref().unwrap().text, fixture.json);
        assert!(surface.transfer.as_ref().unwrap().submitted.is_none());
        assert_eq!(fixture.client.view().unwrap().catalog_revision, 0);
    }
    fixture.finish().await;
}
