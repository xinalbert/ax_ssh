use super::*;
use ax_ssh::controller::SftpSession;
use std::sync::Arc;

#[derive(Debug)]
struct NativePath(PathBuf);
impl egui::DroppedFile for NativePath {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn bytes(&self) -> std::result::Result<Vec<u8>, String> {
        panic!("UI must never read dropped file bytes")
    }
}

#[tokio::test]
async fn native_drop_needs_exact_visible_geometry_and_is_admitted_once_across_passes() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let client = session.client();
    let mut view = client.view().unwrap();
    view.connected = true;
    view.path = "/remote".into();
    let ctx = egui::Context::default();
    let mut drops = Drops::default();
    let mut passes = 0;
    let output = ctx.run_ui(
        egui::RawInput {
            dropped_files: vec![Arc::new(NativePath("/fixture/file.txt".into()))],
            ..Default::default()
        },
        |ui| {
            let response = ui.allocate_response(egui::vec2(240.0, 100.0), egui::Sense::hover());
            drops
                .target(
                    ui,
                    &response,
                    Uuid::nil(),
                    &view,
                    &client,
                    Target {
                        remote: true,
                        native_position: None,
                    },
                )
                .unwrap();
            assert_eq!(
                ui.input(|input| input.raw.dropped_files.len()),
                if passes == 0 { 1 } else { 0 }
            );
            drops
                .target(
                    ui,
                    &response,
                    Uuid::nil(),
                    &view,
                    &client,
                    Target {
                        remote: true,
                        native_position: Some(response.rect.center()),
                    },
                )
                .unwrap();
            assert!(ui.input(|input| input.raw.dropped_files.is_empty()));
            passes += 1;
            if passes == 1 {
                ui.ctx().request_discard("verify drop deduplication");
            }
        },
    );
    output.drop_without_applying_deltas();
    assert!(passes >= 2);
    assert!(drops.last_frame.is_some());
    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn disabled_or_clipped_targets_never_consume_native_files() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let client = session.client();
    let view = client.view().unwrap();
    let ctx = egui::Context::default();
    let mut drops = Drops::default();
    let output = ctx.run_ui(
        egui::RawInput {
            dropped_files: vec![Arc::new(NativePath("/fixture/file.txt".into()))],
            ..Default::default()
        },
        |ui| {
            ui.add_enabled_ui(false, |ui| {
                let response = ui.allocate_response(egui::vec2(240.0, 100.0), egui::Sense::hover());
                drops
                    .target(
                        ui,
                        &response,
                        Uuid::nil(),
                        &view,
                        &client,
                        Target {
                            remote: true,
                            native_position: Some(response.rect.center()),
                        },
                    )
                    .unwrap();
            });
            let response = ui.allocate_response(egui::vec2(240.0, 100.0), egui::Sense::hover());
            drops
                .target(
                    ui,
                    &response,
                    Uuid::nil(),
                    &view,
                    &client,
                    Target {
                        remote: true,
                        native_position: Some(response.rect.max + egui::vec2(20.0, 20.0)),
                    },
                )
                .unwrap();
            assert_eq!(ui.input(|input| input.raw.dropped_files.len()), 1);
        },
    );
    output.drop_without_applying_deltas();
    assert!(drops.last_frame.is_none());
    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn internal_drag_rejects_another_pane_or_connection_without_an_upload() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let client = session.client();
    let mut view = client.view().unwrap();
    view.connected = true;
    view.path = "/remote".into();
    let ctx = egui::Context::default();
    let mut drops = Drops::default();
    let pos = egui::pos2(30.0, 30.0);
    for (source_pane, generation) in [
        (Uuid::new_v4(), view.generation),
        (Uuid::nil(), view.generation + 1),
    ] {
        let output = ctx.run_ui(
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| {
                ui.allocate_response(egui::vec2(240.0, 100.0), egui::Sense::hover());
            },
        );
        output.drop_without_applying_deltas();
        egui::DragAndDrop::set_payload(
            &ctx,
            Drag {
                pane: source_pane,
                generation,
                revision: 0,
                path: "/fixture/file".into(),
                local: true,
            },
        );
        let mut rejected = false;
        let output = ctx.run_ui(
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| {
                let response = ui.allocate_response(egui::vec2(240.0, 100.0), egui::Sense::hover());
                rejected = drops
                    .target(
                        ui,
                        &response,
                        Uuid::nil(),
                        &view,
                        &client,
                        Target {
                            remote: true,
                            native_position: None,
                        },
                    )
                    .is_err();
            },
        );
        output.drop_without_applying_deltas();
        assert!(rejected);
    }
    assert!(client.view().unwrap().transfers.is_empty());
    session.shutdown().await.unwrap();
}
