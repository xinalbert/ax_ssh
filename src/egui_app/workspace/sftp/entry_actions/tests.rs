use super::*;
use crate::egui_app::i18n::{Translator, current};
use ax_ssh::config::UiLanguage;
use ax_ssh::controller::{SftpIcon, SftpSession};
use std::sync::Arc;

fn entry() -> SftpEntryView {
    SftpEntryView {
        name: "User filename".into(),
        path: "/remote/User filename".into(),
        is_dir: false,
        is_symlink: false,
        size: 3,
        modified: Some(1),
        icon: SftpIcon::GenericFile,
    }
}
fn dialog(view: &SftpView, rename: bool) -> EntryDialog {
    EntryDialog {
        generation: view.generation,
        request_id: view.request_id,
        entry: view.entries[0].clone(),
        rename,
        name: "新 name".into(),
        submitted: None,
        error: None,
        focus: true,
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
        pane.entry_dialog(ui, client, view, Uuid::nil()).unwrap()
    })
}

#[tokio::test]
async fn localized_confirmation_keeps_user_data_and_drafts_and_submits_once_per_frame() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let client = session.client();
    let mut view = client.view().unwrap();
    session.shutdown().await.unwrap();
    view.connected = true;
    view.path = "/remote".into();
    view.entries = vec![entry()];
    let fonts = crate::egui_app::fonts::load().unwrap();
    for language in UiLanguage::bundled() {
        for rename in [true, false] {
            let ctx = egui::Context::default();
            ctx.set_fonts(fonts.clone());
            Translator::new(language, None).apply(&ctx);
            let mut pane = SftpPane {
                entry_dialog: Some(dialog(&view, rename)),
                ..Default::default()
            };
            for _ in 0..2 {
                draw(&ctx, &mut pane, &client, &view, vec![]).drop_without_applying_deltas();
            }
            let output = draw(&ctx, &mut pane, &client, &view, vec![]);
            let rendered = super::super::tests::texts(&output);
            output.drop_without_applying_deltas();
            assert!(
                rendered
                    .iter()
                    .any(|(text, _)| text == "/remote/User filename")
            );
            if !rename {
                assert!(rendered.iter().any(|(text, _)| {
                    text == current(&ctx)
                        .text("", "Deletion cannot be undone. Directories must be empty.")
                }));
            }
            let label = current(&ctx).text("", if rename { "Rename" } else { "Delete" });
            let pos = rendered
                .iter()
                .find(|(text, _)| text == label)
                .unwrap()
                .1
                .center();
            draw(&ctx, &mut pane, &client, &view, pointer(pos, true))
                .drop_without_applying_deltas();
            let (mut passes, mut admitted) = (0, 0);
            ctx.run_ui(input(pointer(pos, false)), |ui| {
                pane.entry_dialog(ui, &client, &view, Uuid::nil()).unwrap();
                admitted += usize::from(pane.entry_dialog.as_mut().unwrap().error.take().is_some());
                passes += 1;
                if passes == 1 {
                    ui.ctx().request_discard("remote write once");
                }
            })
            .drop_without_applying_deltas();
            assert!(passes >= 2);
            assert_eq!(admitted, 1, "{language:?} {rename}");
            let draft = pane.entry_dialog.as_ref().unwrap();
            assert_eq!(draft.name, "新 name");
            assert!(draft.submitted.is_none());
            draw(
                &ctx,
                &mut pane,
                &client,
                &view,
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            )
            .drop_without_applying_deltas();
            assert!(pane.entry_dialog.is_none());
        }
    }
}

#[tokio::test]
async fn sizing_disabled_and_stale_targets_never_create_or_retarget_a_confirmation() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let client = session.client();
    let mut view = client.view().unwrap();
    view.connected = true;
    view.path = "/remote".into();
    view.entries = vec![entry()];
    let ctx = egui::Context::default();
    let mut pane = SftpPane {
        entry_dialog: Some(dialog(&view, true)),
        ..Default::default()
    };
    ctx.run_ui(input(vec![]), |ui| {
        ui.scope_builder(egui::UiBuilder::new().sizing_pass(), |ui| {
            pane.entry_dialog(ui, &client, &view, Uuid::nil()).unwrap()
        });
        ui.add_enabled_ui(false, |ui| {
            pane.entry_dialog(ui, &client, &view, Uuid::nil()).unwrap()
        });
    })
    .drop_without_applying_deltas();
    assert!(pane.entry_dialog.as_ref().unwrap().focus);
    assert!(ctx.memory(|memory| memory.focused()).is_none());
    for case in 0..4 {
        pane.entry_dialog = Some(dialog(&view, true));
        let mut changed = view.clone();
        match case {
            0 => changed.generation += 1,
            1 => changed.request_id += 1,
            2 => changed.entries[0].modified = Some(2),
            _ => changed.connected = false,
        }
        draw(&ctx, &mut pane, &client, &changed, vec![]).drop_without_applying_deltas();
        assert!(pane.entry_dialog.is_none());
    }
    // Row controls are disabled for links/loading/disconnected snapshots.
    for case in 0..3 {
        let mut changed = view.clone();
        match case {
            0 => changed.entries[0].is_symlink = true,
            1 => changed.loading = true,
            _ => changed.connected = false,
        }
        let output = ctx.run_ui(input(vec![]), |ui| {
            pane.entry_buttons(ui, &changed, &changed.entries[0])
        });
        let pos = super::super::tests::texts(&output)
            .into_iter()
            .find(|(text, _)| text == "Delete")
            .unwrap()
            .1
            .center();
        output.drop_without_applying_deltas();
        for pressed in [true, false] {
            ctx.run_ui(input(pointer(pos, pressed)), |ui| {
                pane.entry_buttons(ui, &changed, &changed.entries[0])
            })
            .drop_without_applying_deltas();
        }
        assert!(pane.entry_dialog.is_none());
    }
    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn row_actions_open_frozen_dialogs_and_block_workspace_shortcuts() {
    let session = SftpSession::idle(&tokio::runtime::Handle::current(), Arc::new(|| {}));
    let mut view = session.client().view().unwrap();
    view.connected = true;
    view.path = "/remote".into();
    view.entries = vec![entry()];
    let ctx = egui::Context::default();
    for (label, rename) in [("Rename", true), ("Delete", false)] {
        let mut pane = SftpPane::default();
        let output = ctx.run_ui(input(vec![]), |ui| {
            pane.entry_buttons(ui, &view, &view.entries[0])
        });
        let pos = super::super::tests::texts(&output)
            .into_iter()
            .find(|(text, _)| text == label)
            .unwrap()
            .1
            .center();
        output.drop_without_applying_deltas();
        for pressed in [true, false] {
            ctx.run_ui(input(pointer(pos, pressed)), |ui| {
                pane.entry_buttons(ui, &view, &view.entries[0])
            })
            .drop_without_applying_deltas();
        }
        let draft = pane.entry_dialog.as_ref().unwrap();
        assert_eq!(draft.entry.path, view.entries[0].path);
        assert_eq!(draft.request_id, view.request_id);
        assert_eq!(draft.rename, rename);
        let mut surface = WorkspaceUi::default();
        surface.panes.insert(
            Uuid::nil(),
            PaneUi {
                terminal: TerminalPane::default(),
                connection: ConnectionForm::default(),
                connected: false,
                sftp: pane,
            },
        );
        let workspace = WorkspaceView {
            ready: true,
            busy: false,
            error: None,
            status: String::new(),
            catalog_revision: 0,
            settings: Arc::new(ax_ssh::config::AppSettings::default()),
            profiles: Arc::new(vec![]),
            groups: Arc::new(vec![]),
            active_tab_id: None,
            tabs: vec![],
        };
        assert!(surface.blocked(&workspace));
        assert!(
            !crate::egui_app::menus::Command::NewLocal
                .enabled_for_view(&workspace, surface.blocked(&workspace))
        );
        surface
            .panes
            .get_mut(&Uuid::nil())
            .unwrap()
            .sftp
            .entry_dialog = None;
        assert!(!surface.blocked(&workspace));
    }
    session.shutdown().await.unwrap();
}
