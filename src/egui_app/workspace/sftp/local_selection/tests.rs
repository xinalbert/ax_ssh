use super::*;
use crate::egui_app::i18n::{Translator, current};
use ax_ssh::config::UiLanguage;
use ax_ssh::controller::{SftpIcon, SftpSession};
use std::sync::Arc;

fn entry(index: usize, directory: bool) -> LocalEntryView {
    LocalEntryView {
        name: format!("User file {index}"),
        path: format!("/local/file{index}"),
        is_dir: directory,
        size: 1,
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
        pane.local_toolbar(ui, view, &client.for_view(view));
        for entry in &view.local_entries {
            pane.local_select(ui, view, entry);
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
        pane.local_toolbar(ui, view, &client.for_view(view));
        for entry in &view.local_entries {
            pane.local_select(ui, view, entry);
        }
        admitted += usize::from(pane.error.take().is_some());
        passes += 1;
        if passes == 1 {
            ui.ctx().request_discard("selected upload once");
        }
    })
    .drop_without_applying_deltas();
    assert!(passes >= 2);
    admitted
}

#[tokio::test]
async fn local_selection_is_bounded_pane_local_and_clears_on_revision_or_connection_changes() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let client = session.client();
    let mut view = client.view().unwrap();
    view.connected = true;
    view.path = "/remote".into();
    view.local_path = "/local".into();
    view.local_entries = (0..70).map(|index| entry(index, false)).collect();
    let ctx = egui::Context::default();
    let mut pane = SftpPane::default();
    click(&ctx, &mut pane, &client, &view, "Select All");
    assert_eq!(pane.local_selection.selected.len(), 64);
    assert!(SftpPane::default().local_selection.selected.is_empty());
    view.local_entries[0].is_dir = true;
    view.local_entries.remove(1);
    pane.local_selection.sync(&view);
    assert_eq!(pane.local_selection.selected.len(), 62);
    view.local_entries.reverse();
    pane.local_selection.sync(&view);
    assert_eq!(pane.local_selection.selected.len(), 62);
    view.local_revision += 1;
    pane.local_selection.sync(&view);
    assert!(pane.local_selection.selected.is_empty());
    pane.local_selection
        .selected
        .insert("/local/file3".into(), false);
    view.generation += 1;
    pane.local_selection.sync(&view);
    assert!(pane.local_selection.selected.is_empty());
    pane.local_selection
        .selected
        .insert("/local/file3".into(), false);
    view.connected = false;
    pane.local_selection.sync(&view);
    assert!(pane.local_selection.selected.is_empty());
    ctx.run_ui(input(vec![]), |ui| {
        ui.add_enabled_ui(false, |ui| assert!(!pane.local_selection.admit(ui)));
        ui.scope_builder(egui::UiBuilder::new().sizing_pass(), |ui| {
            assert!(!pane.local_selection.admit(ui))
        });
        assert!(pane.local_selection.admit(ui));
        assert!(!pane.local_selection.admit(ui));
    })
    .drop_without_applying_deltas();
    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn localized_file_and_folder_selection_uploads_once_and_respects_preparation_and_readiness() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let client = session.client();
    let mut view = client.view().unwrap();
    session.shutdown().await.unwrap();
    view.connected = true;
    view.path = "/remote".into();
    view.local_path = "/local".into();
    view.local_entries = vec![entry(1, false), entry(2, true)];
    let fonts = crate::egui_app::fonts::load().unwrap();
    for language in UiLanguage::bundled() {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts.clone());
        Translator::new(language, None).apply(&ctx);
        let mut pane = SftpPane::default();
        let label = |key| current(&ctx).text("", key).to_owned();
        click(&ctx, &mut pane, &client, &view, &label("Select All"));
        assert_eq!(pane.local_selection.selected.len(), 2);
        click(&ctx, &mut pane, &client, &view, &label("Clear"));
        assert!(pane.local_selection.selected.is_empty());
        click(
            &ctx,
            &mut pane,
            &client,
            &view,
            &label("Select {0}").replace("{0}", "User file 2"),
        );
        assert_eq!(pane.local_selection.selected.len(), 1);
        assert_eq!(
            click(&ctx, &mut pane, &client, &view, &label("Upload selected")),
            1,
            "{language:?}"
        );
        assert_eq!(pane.local_selection.selected.len(), 1);
        for case in 0..3 {
            let mut disabled = view.clone();
            match case {
                0 => disabled.upload_preparing = true,
                1 => disabled.local_loading = true,
                _ => disabled.loading = true,
            }
            assert_eq!(
                click(
                    &ctx,
                    &mut pane,
                    &client,
                    &disabled,
                    &label("Upload selected")
                ),
                0,
                "{language:?} {case}"
            );
        }
    }
}
