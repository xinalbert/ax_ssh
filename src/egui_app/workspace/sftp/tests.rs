use super::*;
use crate::egui_app::i18n::{Translator, current};
use ax_ssh::config::UiLanguage;
use ax_ssh::controller::{
    SftpEntryView, SftpIcon, SftpSession, SftpTransferView, SftpView, TerminalPrompt,
    TerminalPromptKind,
};
use std::sync::Arc;

pub(super) fn texts(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
    fn collect(shape: &egui::epaint::Shape, result: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                result.push((text.galley.job.text.clone(), text.visual_bounding_rect()))
            }
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, result);
                }
            }
            _ => {}
        }
    }
    let mut result = Vec::new();
    for shape in &output.shapes {
        collect(&shape.shape, &mut result);
    }
    result
}

fn input(events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1800.0, 1800.0),
        )),
        focused: true,
        events,
        ..Default::default()
    }
}

#[tokio::test]
async fn localized_transfer_controls_follow_capability_and_admit_once_across_passes() {
    use ax_ssh::controller::SftpTransferPhase as Phase;
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let client = session.client();
    let mut view = client.view().unwrap();
    // A closed queue makes each admitted UI intent observable without a native server.
    session.shutdown().await.unwrap();
    let fonts = crate::egui_app::fonts::load().unwrap();
    for language in UiLanguage::bundled() {
        for (phase, pausable, connected, label, expected) in [
            (Phase::Transferring, true, true, "Pause", 1),
            (Phase::Paused, true, true, "Resume", 1),
            (Phase::Transferring, false, true, "Pause", 0),
            (Phase::Queued, true, true, "Pause", 0),
            (Phase::Pausing, true, true, "Resume", 0),
            (Phase::Completed, true, true, "Cancel", 0),
            (Phase::Transferring, true, false, "Pause", 0),
        ] {
            let ctx = egui::Context::default();
            ctx.set_fonts(fonts.clone());
            Translator::new(language, None).apply(&ctx);
            let mut pane = SftpPane::default();
            view.connected = connected;
            view.transfers = vec![SftpTransferView {
                id: Uuid::nil(),
                name: "User transfer".into(),
                status: "User status".into(),
                downloaded_bytes: 10,
                total_bytes: 100,
                conflict: false,
                phase,
                pausable,
            }];
            let output = draw(&ctx, &mut pane, &client, &view, vec![]);
            let position = texts(&output)
                .into_iter()
                .find(|(text, _)| text == current(&ctx).text("", label))
                .unwrap()
                .1
                .center();
            output.drop_without_applying_deltas();
            draw(
                &ctx,
                &mut pane,
                &client,
                &view,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            )
            .drop_without_applying_deltas();
            let mut passes = 0;
            let mut admitted = 0;
            let output = ctx.run_ui(
                input(vec![egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }]),
                |ui| {
                    ui.push_id("sftp-companion", |ui| {
                        pane.show_view(ui, &client, &view, None, Uuid::nil(), None)
                            .unwrap();
                    });
                    admitted += usize::from(pane.error.take().is_some());
                    passes += 1;
                    if passes == 1 {
                        ui.ctx().request_discard("verify one transfer control");
                    }
                },
            );
            output.drop_without_applying_deltas();
            assert!(passes >= 2);
            assert_eq!(admitted, expected, "{language:?} {phase:?} {label}");
            assert_eq!(view.transfers[0].status, "User status");
            ctx.run_ui(input(vec![]), |ui| {
                ui.add_enabled_ui(false, |ui| assert!(!pane.admit(ui)));
                ui.scope_builder(egui::UiBuilder::new().sizing_pass(), |ui| {
                    assert!(!pane.admit(ui))
                });
            })
            .drop_without_applying_deltas();
        }
    }
}

#[tokio::test]
async fn full_transfer_history_keeps_the_remaining_pane_geometry_bounded() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let client = session.client();
    let mut view = client.view().unwrap();
    let ctx = egui::Context::default();
    let mut pane = SftpPane::default();
    let mut bottoms = Vec::new();
    for count in [1, 64] {
        view.transfers = (0..count)
            .map(|index| SftpTransferView {
                id: Uuid::from_u128(index),
                name: format!("fixture transfer {index}"),
                status: "fixture".into(),
                downloaded_bytes: 0,
                total_bytes: 100,
                conflict: false,
                phase: ax_ssh::controller::SftpTransferPhase::Transferring,
                pausable: true,
            })
            .collect();
        let mut bottom = 0.0;
        for _ in 0..2 {
            ctx.run_ui(input(vec![]), |ui| {
                pane.show_view(ui, &client, &view, None, Uuid::nil(), None)
                    .unwrap();
                bottom = ui.cursor().top();
            })
            .drop_without_applying_deltas();
        }
        bottoms.push(bottom);
    }
    assert!(bottoms.iter().all(|bottom| bottom.is_finite()));
    assert!(bottoms[1] - bottoms[0] <= 180.0);
    session.shutdown().await.unwrap();
}

fn draw(
    ctx: &egui::Context,
    pane: &mut SftpPane,
    client: &SftpClient,
    view: &SftpView,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run_ui(input(events), |ui| {
        ui.push_id("sftp-companion", |ui| {
            pane.show_view(ui, client, view, None, Uuid::nil(), None)
                .unwrap()
        });
    })
}

#[tokio::test]
async fn path_focus_request_waits_for_real_layout_and_is_discarded_when_disabled() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let client = session.client();
    let view = client.view().unwrap();
    let ctx = egui::Context::default();
    let mut pane = SftpPane {
        focus_requested: true,
        ..Default::default()
    };
    ctx.run_ui(input(vec![]), |ui| {
        ui.scope_builder(egui::UiBuilder::new().sizing_pass(), |ui| {
            pane.show_view(ui, &client, &view, None, Uuid::nil(), None)
                .unwrap();
        });
    })
    .drop_without_applying_deltas();
    assert!(pane.focus_requested);
    assert!(ctx.memory(|memory| memory.focused()).is_none());
    ctx.run_ui(input(vec![]), |ui| {
        ui.add_enabled_ui(false, |ui| {
            pane.show_view(ui, &client, &view, None, Uuid::nil(), None)
                .unwrap();
        });
    })
    .drop_without_applying_deltas();
    assert!(!pane.focus_requested);
    assert!(ctx.memory(|memory| memory.focused()).is_none());
    draw(&ctx, &mut pane, &client, &view, vec![]).drop_without_applying_deltas();
    assert!(ctx.memory(|memory| memory.focused()).is_none());
    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn sftp_translation_keeps_data_and_fingerprints_and_clears_old_credentials() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let client = session.client();
    let mut view = client.view().unwrap();
    view.path = "/remote/Settings".into();
    view.entries.push(SftpEntryView {
        name: "Settings".into(),
        path: "/remote/Settings/Settings".into(),
        is_dir: false,
        is_symlink: false,
        size: 20,
        modified: None,
        icon: SftpIcon::GenericFile,
    });
    view.transfers.push(SftpTransferView {
        id: Uuid::new_v4(),
        name: "Upload".into(),
        status: "fixture-transfer-status".into(),
        downloaded_bytes: 1,
        total_bytes: 2,
        conflict: true,
        phase: ax_ssh::controller::SftpTransferPhase::Conflict,
        pausable: false,
    });
    view.edits.push(ax_ssh::remote_edit::RemoteEditView {
        id: Uuid::new_v4(),
        path: "/remote/Settings/edit.txt".into(),
        local_path: Some("/fixture/private-copy".into()),
        status: "fixture-edit-status".into(),
        pending: false,
        upload_requested: false,
        failed: false,
        changed: true,
    });
    view.prompt = Some(TerminalPrompt {
        id: Uuid::new_v4(),
        host: "original.example".into(),
        username: "original-user".into(),
        port: 2222,
        kind: TerminalPromptKind::HostKey {
            expected: Some("SHA256:expected-fixture".into()),
            actual: "SHA256:observed-fixture".into(),
            confirmable: false,
        },
    });
    let mut pane = SftpPane {
        path: "/remote/Settings".into(),
        local_directory: "/fixture/Download".into(),
        upload_source: "/fixture/Upload".into(),
        upload_remote: "/remote/Settings/Upload".into(),
        secret: zeroize::Zeroizing::new("fixture-stale-secret".into()),
        ..Default::default()
    };
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::egui_app::fonts::load().unwrap());
    let output = draw(&ctx, &mut pane, &client, &view, vec![]);
    output.drop_without_applying_deltas();
    assert!(pane.secret.is_empty());
    for language in UiLanguage::bundled() {
        Translator::new(language, None).apply(&ctx);
        pane.secret = zeroize::Zeroizing::new("fixture-fresh-secret".into());
        let output = draw(&ctx, &mut pane, &client, &view, vec![]);
        let painted = texts(&output);
        for expected in [
            "Settings",
            "SHA256:observed-fixture",
            "SHA256:expected-fixture",
            "Upload · 1 / 2 · fixture-transfer-status",
            "/remote/Settings/edit.txt",
            "/fixture/private-copy",
            "fixture-edit-status",
        ] {
            assert!(
                painted.iter().any(|(text, _)| text == expected),
                "{}: {expected}",
                language.as_setting()
            );
        }
        assert!(painted.iter().all(|(text, _)| text
            != current(&ctx).text("", "Trust once and connect")
            && !text.contains("fixture-fresh-secret")));
        output.drop_without_applying_deltas();
        assert_eq!(pane.secret.as_str(), "fixture-fresh-secret");
        assert_eq!(pane.path, "/remote/Settings");
        assert_eq!(pane.local_directory, "/fixture/Download");
        assert_eq!(pane.upload_source, "/fixture/Upload");
        assert_eq!(pane.upload_remote, "/remote/Settings/Upload");
    }
    view.prompt.as_mut().unwrap().id = Uuid::new_v4();
    draw(&ctx, &mut pane, &client, &view, vec![]).drop_without_applying_deltas();
    assert!(pane.secret.is_empty());
    pane.secret = zeroize::Zeroizing::new("fixture-old-generation".into());
    view.generation += 1;
    draw(&ctx, &mut pane, &client, &view, vec![]).drop_without_applying_deltas();
    assert!(pane.secret.is_empty());
    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn translated_sftp_actions_surface_queue_errors_once_across_layout_passes() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let client = session.client();
    session.shutdown().await.unwrap();
    let mut view = client.view().unwrap();
    view.connected = true; // The active snapshot outlives its closed command queue.
    view.transfers.push(SftpTransferView {
        id: Uuid::new_v4(),
        name: "fixture.txt".into(),
        status: "conflict".into(),
        downloaded_bytes: 0,
        total_bytes: 2,
        conflict: true,
        phase: ax_ssh::controller::SftpTransferPhase::Conflict,
        pausable: false,
    });
    view.local_entries.push(ax_ssh::controller::LocalEntryView {
        name: "fixture-local.txt".into(),
        path: "/fixture/local.txt".into(),
        is_dir: false,
        size: 10,
        icon: SftpIcon::GenericFile,
    });
    let fonts = crate::egui_app::fonts::load().unwrap();
    for language in UiLanguage::bundled() {
        for source in ["Disconnect", "Cancel", "Open"] {
            let ctx = egui::Context::default();
            ctx.set_fonts(fonts.clone());
            Translator::new(language, None).apply(&ctx);
            let mut pane = SftpPane::default();
            let output = draw(&ctx, &mut pane, &client, &view, vec![]);
            let label = current(&ctx).text("", source);
            let pos = texts(&output)
                .iter()
                .find(|(text, _)| text == label)
                .unwrap()
                .1
                .center();
            output.drop_without_applying_deltas();
            let event = |pressed| {
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            draw(&ctx, &mut pane, &client, &view, event(true)).drop_without_applying_deltas();
            let mut passes = 0;
            let mut errors = 0;
            let output = ctx.run_ui(input(event(false)), |ui| {
                ui.push_id("sftp-companion", |ui| {
                    pane.show_view(ui, &client, &view, None, Uuid::nil(), None)
                        .unwrap();
                });
                // Taking the visible error lets us count repeated failed enqueues
                // without exposing the controller's private receiver to the UI.
                if let Some(error) = pane.error.take() {
                    assert!(error.contains("queue"));
                    errors += 1;
                }
                passes += 1;
                if passes == 1 {
                    ui.ctx().request_discard("exercise SFTP layout retry");
                }
            });
            output.drop_without_applying_deltas();
            assert!(passes >= 2);
            assert_eq!(errors, 1, "{} {source}", language.as_setting());
        }
    }
}
