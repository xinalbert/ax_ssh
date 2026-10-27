use super::*;
use crate::egui_app::menus::{self, Command};
use ax_ssh::config::SessionProfile;

#[tokio::test]
async fn sftp_shortcut_focuses_only_the_current_saved_ssh_path_without_connecting() {
    let directory = std::env::temp_dir().join(format!("axssh-sftp-key-{}", Uuid::new_v4()));
    let store = ConfigStore::new(directory.join("sessions.json"));
    let profile = SessionProfile::new("SFTP fixture", "example.invalid", "fixture");
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
    let original = std::fs::read(store.path()).unwrap();
    let workspace = Workspace::spawn(
        &tokio::runtime::Handle::current(),
        store.clone(),
        Arc::new(|| {}),
    );
    let client = workspace.client();
    wait(&client, |view| view.ready).await;
    client
        .action(WorkspaceAction::OpenProfile(profile.id))
        .unwrap();
    let view = wait(&client, |view| !view.tabs.is_empty()).await;
    let id = menus::sftp_target(&view).unwrap();
    assert!(Command::OpenSftp.enabled_for_view(&view, false));
    assert!(!Command::OpenSftp.enabled_for_view(&view, true));
    for profile in [
        SessionProfile::new_telnet("Telnet", "example.invalid"),
        SessionProfile::new_serial("Serial", "/fixture/absent"),
    ] {
        let mut candidate = view.clone();
        candidate.tabs[0].panes[0].profile = Some(profile);
        assert!(!Command::OpenSftp.enabled_for_view(&candidate, false));
    }
    let mut split = view.clone();
    let mut local_pane = split.tabs[0].panes[0].clone();
    local_pane.id = Uuid::new_v4();
    local_pane.local = true;
    local_pane.profile = None;
    let mut placement = split.tabs[0].layout.panes[0];
    placement.tab_id = local_pane.id;
    split.tabs[0].panes.push(local_pane);
    split.tabs[0].layout.panes[0].focused = false;
    split.tabs[0].layout.panes.push(placement);
    assert!(menus::sftp_target(&split).is_none());
    split.tabs[0].layout.panes[0].focused = true;
    split.tabs[0].layout.panes[1].focused = false;
    assert_eq!(menus::sftp_target(&split), Some(id));
    for variation in 0..7 {
        let mut candidate = view.clone();
        match variation {
            0 => candidate.ready = false,
            1 => candidate.tabs[0].settings = true,
            2 => candidate.tabs[0].panes[0].local = true,
            3 => candidate.tabs[0].panes[0].profile = None,
            4 => candidate.tabs[0].panes[0].sftp = false,
            5 => candidate.tabs[0].layout.panes[0].focused = false,
            _ => candidate.active_tab_id = None,
        }
        assert!(!Command::OpenSftp.enabled_for_view(&candidate, false));
    }
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::egui_app::fonts::load().unwrap());
    let mut surface = WorkspaceUi::default();
    let chord = menus::parse_shortcut(&view.settings.shortcuts.open_sftp).unwrap();
    let key = egui::Event::Key {
        key: chord.logical_key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: chord.modifiers,
    };
    let mut passes = 0;
    ctx.run_ui(
        egui::RawInput {
            focused: true,
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1500.0, 1200.0),
            )),
            events: vec![key],
            ..Default::default()
        },
        |ui| {
            surface.show(ui, &client).unwrap();
            passes += 1;
            if passes == 1 {
                ui.ctx().request_discard("verify SFTP focus once per frame");
            }
        },
    )
    .drop_without_applying_deltas();
    assert!(passes >= 2);
    assert!(surface.sftp_focus.is_none());
    assert!(!surface.panes[&id].sftp.focus_requested);
    let focused = ctx.memory(|memory| memory.focused()).unwrap();
    assert!(!surface.panes[&id].terminal.owns_widget(focused));
    assert!(!surface.panes[&id].terminal.focused());
    ctx.run_ui(
        egui::RawInput {
            focused: true,
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1500.0, 1200.0),
            )),
            events: vec![egui::Event::Text("/shortcut-fixture".into())],
            ..Default::default()
        },
        |ui| {
            surface.show(ui, &client).unwrap();
        },
    )
    .drop_without_applying_deltas();
    assert!(surface.panes[&id].sftp.path.contains("/shortcut-fixture"));
    ctx.memory_mut(|memory| memory.surrender_focus(focused));
    surface.menu_command = Some(Command::OpenSftp);
    ctx.run_ui(
        egui::RawInput {
            focused: true,
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1500.0, 1200.0),
            )),
            ..Default::default()
        },
        |ui| {
            surface.show(ui, &client).unwrap();
        },
    )
    .drop_without_applying_deltas();
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(focused));
    assert!(surface.menu_command.is_none());
    assert!(surface.panes[&id].sftp.secret.is_empty());
    let sftp = client.sftp(id).unwrap().unwrap().view().unwrap();
    assert!(!sftp.connected);
    assert!(sftp.prompt.is_none());
    assert!(
        !client
            .terminal(id)
            .unwrap()
            .unwrap()
            .view()
            .unwrap()
            .connected
    );
    assert_eq!(*client.view().unwrap().settings, *view.settings);
    assert_eq!(std::fs::read(store.path()).unwrap(), original);

    // A late native-menu event or old request must not target a new local tab.
    client.action(WorkspaceAction::NewLocal).unwrap();
    let local = wait(&client, |view| view.tabs.len() == 2).await;
    assert!(menus::sftp_target(&local).is_none());
    surface.menu_command = Some(Command::OpenSftp);
    surface.sftp_focus = Some(id);
    ctx.run_ui(egui::RawInput::default(), |ui| {
        surface.show(ui, &client).unwrap();
    })
    .drop_without_applying_deltas();
    assert!(surface.menu_command.is_none());
    assert!(surface.sftp_focus.is_none());
    assert!(!surface.panes[&id].sftp.focus_requested);
    workspace.shutdown().await.unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}
