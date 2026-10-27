use super::*;
use crate::egui_app::i18n::{Translator, current};
use ax_ssh::config::UiLanguage;
use ax_ssh::controller::{SftpDownloadBatchResult, SftpIcon, SftpSession};
use std::sync::Arc;

fn entry(index: usize, directory: bool, link: bool) -> SftpEntryView {
    SftpEntryView {
        path: format!("/remote/file{index}"),
        name: format!("User file {index}"),
        is_dir: directory,
        is_symlink: link,
        size: 1,
        modified: None,
        icon: SftpIcon::GenericFile,
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
fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
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
        pane.remote_toolbar(ui, view, &client.for_view(view));
        for entry in &view.entries {
            pane.remote_select(ui, view, entry);
        }
    })
}
fn click(
    ctx: &egui::Context,
    pane: &mut SftpPane,
    client: &SftpClient,
    view: &SftpView,
    label: &str,
) -> usize {
    let output = draw(ctx, pane, client, view, vec![]);
    let texts = super::super::tests::texts(&output);
    output.drop_without_applying_deltas();
    let pos = texts
        .iter()
        .find(|(text, _)| text == label)
        .unwrap()
        .1
        .center();
    draw(ctx, pane, client, view, pointer(pos, true)).drop_without_applying_deltas();
    let (mut passes, mut admitted) = (0, 0);
    ctx.run_ui(input(pointer(pos, false)), |ui| {
        pane.remote_toolbar(ui, view, &client.for_view(view));
        for entry in &view.entries {
            pane.remote_select(ui, view, entry);
        }
        admitted += usize::from(pane.error.take().is_some());
        passes += 1;
        if passes == 1 {
            ui.ctx().request_discard("one selection or download");
        }
    })
    .drop_without_applying_deltas();
    assert!(passes >= 2);
    admitted
}

#[tokio::test]
async fn selection_is_directory_local_pruned_by_path_and_type_and_limited_to_64() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let client = session.client();
    let mut view = client.view().unwrap();
    view.connected = true;
    view.path = "/remote".into();
    view.local_path = "/local".into();
    view.entries = (0..70).map(|index| entry(index, false, false)).collect();
    let ctx = egui::Context::default();
    let mut pane = SftpPane::default();
    click(&ctx, &mut pane, &client, &view, "Select All");
    assert_eq!(pane.remote_selection.selected.len(), 64);
    assert!(SftpPane::default().remote_selection.selected.is_empty());
    view.entries[0].is_dir = true;
    view.entries[1].is_symlink = true;
    view.entries.remove(2);
    pane.remote_selection.sync(&view);
    assert_eq!(pane.remote_selection.selected.len(), 61);
    view.entries.reverse();
    pane.remote_selection.sync(&view);
    assert_eq!(pane.remote_selection.selected.len(), 61);
    view.request_id += 1;
    pane.remote_selection.sync(&view);
    assert!(pane.remote_selection.selected.is_empty());
    pane.remote_selection
        .selected
        .insert("/remote/file3".into(), false);
    view.generation += 1;
    pane.remote_selection.sync(&view);
    assert!(pane.remote_selection.selected.is_empty());
    pane.remote_selection
        .selected
        .insert("/remote/file3".into(), false);
    view.connected = false;
    pane.remote_selection.sync(&view);
    assert!(pane.remote_selection.selected.is_empty());
    ctx.run_ui(input(vec![]), |ui| {
        ui.add_enabled_ui(false, |ui| assert!(!pane.remote_selection.admit(ui)));
        ui.scope_builder(egui::UiBuilder::new().sizing_pass(), |ui| {
            assert!(!pane.remote_selection.admit(ui))
        });
        assert!(pane.remote_selection.admit(ui));
        assert!(!pane.remote_selection.admit(ui));
    })
    .drop_without_applying_deltas();
    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn localized_row_select_all_clear_and_download_are_single_admission_and_preserve_selection() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let client = session.client();
    let mut view = client.view().unwrap();
    session.shutdown().await.unwrap();
    view.connected = true;
    view.path = "/remote".into();
    view.local_path = "/local".into();
    view.entries = vec![
        entry(1, false, false),
        entry(2, true, false),
        entry(3, false, true),
    ];
    let fonts = crate::egui_app::fonts::load().unwrap();
    for language in UiLanguage::bundled() {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts.clone());
        Translator::new(language, None).apply(&ctx);
        let mut pane = SftpPane::default();
        let label = |key| current(&ctx).text("", key).to_owned();
        click(&ctx, &mut pane, &client, &view, &label("Select All"));
        assert_eq!(pane.remote_selection.selected.len(), 2);
        click(&ctx, &mut pane, &client, &view, &label("Clear"));
        assert!(pane.remote_selection.selected.is_empty());
        click(
            &ctx,
            &mut pane,
            &client,
            &view,
            &label("Select {0}").replace("{0}", "User file 1"),
        );
        assert_eq!(pane.remote_selection.selected.len(), 1);
        assert_eq!(
            click(&ctx, &mut pane, &client, &view, &label("Download selected")),
            1,
            "{language:?}"
        );
        let selected = pane.remote_selection.selected.clone();
        view.last_download_batch = Some(SftpDownloadBatchResult {
            requested: 1,
            queued: 1,
            skipped: 0,
            failures: vec![],
        });
        let output = draw(&ctx, &mut pane, &client, &view, vec![]);
        let rendered = super::super::tests::texts(&output);
        output.drop_without_applying_deltas();
        assert!(
            rendered
                .iter()
                .any(|(text, _)| text.contains(&label("Queued ({0})").replace("{0}", "1")))
        );
        assert_eq!(pane.remote_selection.selected, selected);
        assert!(pane.error.is_none());
        view.last_download_batch = None;
        view.local_loading = true;
        assert_eq!(
            click(&ctx, &mut pane, &client, &view, &label("Download selected")),
            0
        );
        view.local_loading = false;
    }
}
