use super::*;
use crate::egui_app::i18n::{Translator, current};
use ax_ssh::config::UiLanguage;
use ax_ssh::controller::{
    SftpSession, SftpTransferBatchResult, SftpTransferPhase as Phase, SftpTransferView,
};
use std::sync::Arc;

fn entry(id: u128, phase: Phase, pausable: bool) -> SftpTransferView {
    SftpTransferView {
        id: Uuid::from_u128(id),
        name: format!("User file {id}"),
        status: "User status".into(),
        downloaded_bytes: 1,
        total_bytes: 100,
        conflict: false,
        phase,
        pausable,
    }
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

fn pointer(position: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

fn draw(
    ctx: &egui::Context,
    pane: &mut SftpPane,
    client: &SftpClient,
    view: &SftpView,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run_ui(input(events), |ui| {
        pane.show_transfers(ui, &client.for_view(view), view)
    })
}

fn position(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
    super::super::tests::texts(output)
        .into_iter()
        .find(|(text, _)| text == label)
        .unwrap()
        .1
        .center()
}

fn click(
    ctx: &egui::Context,
    pane: &mut SftpPane,
    client: &SftpClient,
    view: &SftpView,
    label: &str,
) {
    let output = draw(ctx, pane, client, view, vec![]);
    let pos = position(&output, label);
    output.drop_without_applying_deltas();
    draw(ctx, pane, client, view, pointer(pos, true)).drop_without_applying_deltas();
    let mut passes = 0;
    ctx.run_ui(input(pointer(pos, false)), |ui| {
        pane.show_transfers(ui, &client.for_view(view), view);
        passes += 1;
        if passes == 1 {
            ui.ctx().request_discard("selection must change once");
        }
    })
    .drop_without_applying_deltas();
    assert!(passes >= 2);
}

#[tokio::test]
async fn selection_is_pane_local_pruned_by_identity_and_bounded() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let mut view = session.client().view().unwrap();
    view.connected = true;
    view.transfers = vec![
        entry(1, Phase::Transferring, true),
        entry(2, Phase::Paused, true),
        entry(3, Phase::Cancelling, false),
    ];
    let mut first = TransferControls::default();
    let second = TransferControls::default();
    first.sync(&view);
    first
        .selected
        .extend(view.transfers.iter().map(|row| row.id));
    assert!(second.selected.is_empty());
    assert!(first.eligible(&view, Control::Pause));
    assert!(first.eligible(&view, Control::Resume));
    view.transfers[0].phase = Phase::Completed;
    view.transfers.remove(1);
    first.sync(&view);
    assert_eq!(first.selected, BTreeSet::from([Uuid::from_u128(3)]));
    assert!(!first.eligible(&view, Control::Cancel));
    view.generation += 1;
    first.sync(&view);
    assert!(first.selected.is_empty());
    first.selected.insert(Uuid::from_u128(3));
    view.connected = false;
    first.sync(&view);
    assert!(first.selected.is_empty());
    let ctx = egui::Context::default();
    ctx.run_ui(input(vec![]), |ui| {
        ui.add_enabled_ui(false, |ui| assert!(!first.admit_selection(ui)));
        ui.scope_builder(egui::UiBuilder::new().sizing_pass(), |ui| {
            assert!(!first.admit_selection(ui))
        });
        assert!(first.admit_selection(ui));
        assert!(!first.admit_selection(ui));
    })
    .drop_without_applying_deltas();
    view.connected = true;
    view.transfers = (1..=64)
        .map(|id| entry(id, Phase::Transferring, true))
        .collect();
    let client = session.client();
    let mut pane = SftpPane::default();
    click(&ctx, &mut pane, &client, &view, "Select All");
    assert_eq!(pane.transfers.selected.len(), MAX_SFTP_TRANSFER_BATCH);
    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn localized_selection_and_batch_controls_admit_once_and_skip_disabled_actions() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let client = session.client();
    let mut view = client.view().unwrap();
    session.shutdown().await.unwrap();
    let fonts = crate::egui_app::fonts::load().unwrap();
    view.connected = true;
    view.transfers = vec![
        entry(1, Phase::Transferring, true),
        entry(2, Phase::Paused, true),
        entry(3, Phase::Transferring, false),
        entry(4, Phase::Completed, true),
    ];
    for language in UiLanguage::bundled() {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts.clone());
        Translator::new(language, None).apply(&ctx);
        let mut pane = SftpPane::default();
        let label = |key| current(&ctx).text("", key).to_owned();
        click(&ctx, &mut pane, &client, &view, &label("Select All"));
        assert_eq!(pane.transfers.selected.len(), 3, "{language:?}");
        click(&ctx, &mut pane, &client, &view, &label("Clear"));
        assert!(pane.transfers.selected.is_empty());
        click(
            &ctx,
            &mut pane,
            &client,
            &view,
            "User file 1 · 1 / 100 · User status",
        );
        assert_eq!(
            pane.transfers.selected,
            BTreeSet::from([Uuid::from_u128(1)])
        );
        for (key, expected) in [
            ("Pause selected", 1),
            ("Resume selected", 0),
            ("Cancel selected", 1),
        ] {
            let output = draw(&ctx, &mut pane, &client, &view, vec![]);
            let pos = position(&output, &label(key));
            output.drop_without_applying_deltas();
            draw(&ctx, &mut pane, &client, &view, pointer(pos, true))
                .drop_without_applying_deltas();
            let (mut passes, mut admitted) = (0, 0);
            ctx.run_ui(input(pointer(pos, false)), |ui| {
                pane.show_transfers(ui, &client.for_view(&view), &view);
                admitted += usize::from(pane.error.take().is_some());
                passes += 1;
                if passes == 1 {
                    ui.ctx().request_discard("one batch intent");
                }
            })
            .drop_without_applying_deltas();
            assert!(passes >= 2);
            assert_eq!(admitted, expected, "{language:?} {key}");
        }
        view.last_batch = Some(SftpTransferBatchResult {
            control: Control::Pause,
            requested: 3,
            queued: 1,
            skipped: 2,
            failures: vec![],
        });
        let output = draw(&ctx, &mut pane, &client, &view, vec![]);
        let rendered = super::super::tests::texts(&output);
        assert!(
            rendered
                .iter()
                .any(|(text, _)| text.contains(&label("Queued ({0})").replace("{0}", "1")))
        );
        output.drop_without_applying_deltas();
        assert!(pane.error.is_none());
        assert_eq!(pane.transfers.selected.len(), 1);
        view.last_batch = None;
    }
}

#[tokio::test]
async fn conflict_modal_offers_keep_both_and_applies_choice_to_batch_once() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let client = session.client();
    let mut view = client.view().unwrap();
    view.connected = true;
    view.transfers = vec![SftpTransferView {
        id: Uuid::from_u128(77),
        name: "conflict.txt".into(),
        status: "Conflict".into(),
        downloaded_bytes: 0,
        total_bytes: 10,
        conflict: true,
        phase: Phase::Conflict,
        pausable: false,
    }];
    let mut pane = SftpPane::default();
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::egui_app::fonts::load().unwrap());

    pane.transfers.generation = Some(view.generation);
    pane.transfers.pending_conflict = Some(Uuid::from_u128(77));
    draw(&ctx, &mut pane, &client, &view, vec![]).drop_without_applying_deltas();
    let output = draw(&ctx, &mut pane, &client, &view, vec![]);
    assert!(
        super::super::tests::texts(&output)
            .iter()
            .any(|(text, _)| text == "Keep both")
    );
    assert!(
        super::super::tests::texts(&output)
            .iter()
            .any(|(text, _)| text == "Apply this choice to this upload batch")
    );
    output.drop_without_applying_deltas();

    click(
        &ctx,
        &mut pane,
        &client,
        &view,
        "Apply this choice to this upload batch",
    );
    assert!(pane.transfers.apply_conflict_to_batch);
    click(&ctx, &mut pane, &client, &view, "Keep both");
    assert!(pane.transfers.pending_conflict.is_none());
    assert!(!pane.transfers.apply_conflict_to_batch);
    session.shutdown().await.unwrap();
}
