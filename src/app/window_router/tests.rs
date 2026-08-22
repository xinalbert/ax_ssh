use super::*;

fn router_test_state() -> AppState {
    AppState::new(
        ConfigStore::new(
            std::env::temp_dir().join(format!("ax-ssh-router-{}.json", Uuid::new_v4())),
        ),
        SessionStore::default(),
    )
}

fn test_router() -> WindowRouter {
    WindowRouter::new(slint::Weak::<AppWindow>::default())
}

#[test]
fn split_sessions_share_one_visible_workspace_tab() {
    let router = test_router();
    let mut app = router_test_state();
    let root_tab_id = app.open_local_shell_tab();
    assert!(router.activate_tab(MAIN_WINDOW_ID, root_tab_id, &mut app));
    let child_tab_id = app.open_local_shell_tab();
    assert!(router.complete_pane_split(
        MAIN_WINDOW_ID,
        root_tab_id,
        PaneDirection::Right,
        child_tab_id,
        &mut app,
    ));

    let view = router.views(&mut app).pop().expect("main window view");
    assert_eq!(
        view.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
        vec![root_tab_id]
    );
    assert_eq!(view.active_tab_id, Some(root_tab_id));
    assert_eq!(view.snapshot.id, Some(child_tab_id));
    assert!(view.snapshot.terminal.is_none());
    assert_eq!(view.terminal_panes.len(), 2);
    assert!(
        view.terminal_panes
            .iter()
            .all(|pane| pane.snapshot.terminal.is_some())
    );
    assert!(router.terminal_is_visible(root_tab_id));
    assert!(router.terminal_is_visible(child_tab_id));
    assert_eq!(
        router.terminal_presentation_mode(root_tab_id),
        terminal_presentation::TerminalPresentationMode::Unfocused
    );
    assert_eq!(
        router.terminal_presentation_mode(child_tab_id),
        terminal_presentation::TerminalPresentationMode::Focused
    );
    let updates = router.terminal_updates(&mut app, &HashSet::from([root_tab_id]));
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].panes.len(), 1);
    assert_eq!(updates[0].panes[0].placement.tab_id, root_tab_id);
    assert!(
        view.terminal_panes
            .iter()
            .find(|pane| pane.placement.tab_id == root_tab_id)
            .is_some_and(|pane| !pane.closable)
    );
    assert!(
        view.terminal_panes
            .iter()
            .find(|pane| pane.placement.tab_id == child_tab_id)
            .is_some_and(|pane| pane.closable)
    );
    assert_eq!(router.tab_ids(MAIN_WINDOW_ID, &app), vec![root_tab_id]);
}

#[test]
fn focusing_split_pane_returns_the_updated_layout() {
    let router = test_router();
    let mut app = router_test_state();
    let root_tab_id = app.open_local_shell_tab();
    assert!(router.activate_tab(MAIN_WINDOW_ID, root_tab_id, &mut app));
    let child_tab_id = app.open_local_shell_tab();
    assert!(router.complete_pane_split(
        MAIN_WINDOW_ID,
        root_tab_id,
        PaneDirection::Right,
        child_tab_id,
        &mut app,
    ));

    let layout = router
        .focus_terminal_pane(MAIN_WINDOW_ID, root_tab_id, &mut app)
        .expect("split pane should be focusable");
    assert_eq!(app.active_tab_id(), Some(root_tab_id));
    assert!(
        layout
            .panes
            .iter()
            .find(|pane| pane.tab_id == root_tab_id)
            .is_some_and(|pane| pane.focused)
    );
    assert!(
        layout
            .panes
            .iter()
            .find(|pane| pane.tab_id == child_tab_id)
            .is_some_and(|pane| !pane.focused)
    );
    assert_eq!(
        router.terminal_presentation_mode(root_tab_id),
        terminal_presentation::TerminalPresentationMode::Focused
    );
    assert_eq!(
        router.terminal_presentation_mode(child_tab_id),
        terminal_presentation::TerminalPresentationMode::Unfocused
    );
}

#[test]
fn inactive_workspace_terminal_is_hidden_from_presentation() {
    let router = test_router();
    let mut app = router_test_state();
    let first_tab_id = app.open_local_shell_tab();
    assert!(router.activate_tab(MAIN_WINDOW_ID, first_tab_id, &mut app));
    let second_tab_id = app.open_local_shell_tab();
    assert!(router.activate_tab(MAIN_WINDOW_ID, second_tab_id, &mut app));

    assert_eq!(
        router.terminal_presentation_mode(first_tab_id),
        terminal_presentation::TerminalPresentationMode::Hidden
    );
    assert_eq!(
        router.terminal_presentation_mode(second_tab_id),
        terminal_presentation::TerminalPresentationMode::Focused
    );
}

#[test]
fn inactive_native_window_uses_the_visible_unfocused_refresh_mode() {
    let router = test_router();
    let mut app = router_test_state();
    let tab_id = app.open_local_shell_tab();
    assert!(router.activate_tab(MAIN_WINDOW_ID, tab_id, &mut app));
    assert_eq!(
        router.terminal_presentation_mode(tab_id),
        terminal_presentation::TerminalPresentationMode::Focused
    );

    router.set_window_active(MAIN_WINDOW_ID, false);
    assert_eq!(
        router.terminal_presentation_mode(tab_id),
        terminal_presentation::TerminalPresentationMode::Unfocused
    );

    router.set_window_active(MAIN_WINDOW_ID, true);
    assert_eq!(
        router.terminal_presentation_mode(tab_id),
        terminal_presentation::TerminalPresentationMode::Focused
    );
}

#[test]
fn closing_a_child_pane_collapses_only_that_session() {
    let router = test_router();
    let mut app = router_test_state();
    let root_tab_id = app.open_local_shell_tab();
    assert!(router.activate_tab(MAIN_WINDOW_ID, root_tab_id, &mut app));
    let child_tab_id = app.open_local_shell_tab();
    assert!(router.complete_pane_split(
        MAIN_WINDOW_ID,
        root_tab_id,
        PaneDirection::Right,
        child_tab_id,
        &mut app,
    ));

    assert!(
        router
            .remove_terminal_child_pane(Some(MAIN_WINDOW_ID), root_tab_id, &mut app)
            .is_none()
    );
    assert!(app.terminal(root_tab_id).is_some());
    assert!(app.terminal(child_tab_id).is_some());

    let closed = router
        .remove_terminal_child_pane(Some(MAIN_WINDOW_ID), child_tab_id, &mut app)
        .expect("child pane should close");
    assert_eq!(
        closed.kind,
        ClosedTabKind::Terminal {
            release_file_icon_cache: false,
        }
    );
    assert!(app.terminal(root_tab_id).is_some());
    assert!(app.terminal(child_tab_id).is_none());
    assert_eq!(app.active_tab_id(), Some(root_tab_id));

    let view = router.views(&mut app).pop().expect("main window view");
    assert_eq!(view.terminal_panes.len(), 1);
    assert_eq!(view.terminal_panes[0].placement.tab_id, root_tab_id);
    assert!(!view.terminal_panes[0].closable);
    assert!(view.terminal_dividers.is_empty());
}

#[test]
fn closing_a_detached_child_updates_route_and_transfer_membership() {
    let router = test_router();
    let mut app = router_test_state();
    let root_tab_id = app.open_local_shell_tab();
    assert!(router.activate_tab(MAIN_WINDOW_ID, root_tab_id, &mut app));
    let child_tab_id = app.open_local_shell_tab();
    assert!(router.complete_pane_split(
        MAIN_WINDOW_ID,
        root_tab_id,
        PaneDirection::Right,
        child_tab_id,
        &mut app,
    ));
    let pane_tab_ids = router.pane_tab_ids(MAIN_WINDOW_ID, root_tab_id);
    let pane_tree = router
        .take_pane_tree_for_detach(MAIN_WINDOW_ID, root_tab_id)
        .expect("pane group should detach");
    let transfer = app
        .workspace_transfer_for_terminal_panes(&pane_tab_ids, MAIN_WINDOW_ID, root_tab_id)
        .expect("pane group transfer");
    let detached_id = Uuid::new_v4();
    router.register_detached(
        detached_id,
        slint::Weak::<AppWindow>::default(),
        transfer,
        Some(pane_tree),
    );

    assert!(
        router
            .remove_terminal_child_pane(Some(detached_id), child_tab_id, &mut app)
            .is_some()
    );
    assert_eq!(router.active_tab(detached_id), Some(root_tab_id));
    let detached = router
        .remove_detached(detached_id)
        .expect("detached route should remain available");
    assert_eq!(detached.transfer.tab_ids, vec![root_tab_id]);
    assert_eq!(detached.transfer.active_tab_id, Some(root_tab_id));
    assert_eq!(
        detached
            .pane_tree
            .expect("pane tree should remain")
            .tab_ids(),
        vec![root_tab_id]
    );
    assert!(app.terminal(root_tab_id).is_some());
    assert!(app.terminal(child_tab_id).is_none());
}

#[test]
fn switching_workspace_tabs_restores_the_group_focus_and_layout() {
    let router = test_router();
    let mut app = router_test_state();
    let root_tab_id = app.open_local_shell_tab();
    assert!(router.activate_tab(MAIN_WINDOW_ID, root_tab_id, &mut app));
    let child_tab_id = app.open_local_shell_tab();
    assert!(router.complete_pane_split(
        MAIN_WINDOW_ID,
        root_tab_id,
        PaneDirection::Down,
        child_tab_id,
        &mut app,
    ));
    let other_tab_id = app.open_local_shell_tab();
    assert!(router.activate_tab(MAIN_WINDOW_ID, other_tab_id, &mut app));
    let other_child_tab_id = app.open_local_shell_tab();
    assert!(router.complete_pane_split(
        MAIN_WINDOW_ID,
        other_tab_id,
        PaneDirection::Right,
        other_child_tab_id,
        &mut app,
    ));
    assert!(router.activate_tab(MAIN_WINDOW_ID, root_tab_id, &mut app));

    let view = router.views(&mut app).pop().expect("main window view");
    assert_eq!(
        view.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
        vec![root_tab_id, other_tab_id]
    );
    assert_eq!(view.active_tab_id, Some(root_tab_id));
    assert_eq!(view.snapshot.id, Some(child_tab_id));
    assert_eq!(view.terminal_panes.len(), 2);
    assert_eq!(app.active_tab_id(), Some(child_tab_id));

    assert!(router.activate_tab(MAIN_WINDOW_ID, other_tab_id, &mut app));
    let view = router.views(&mut app).pop().expect("main window view");
    assert_eq!(view.active_tab_id, Some(other_tab_id));
    assert_eq!(view.snapshot.id, Some(other_child_tab_id));
    assert_eq!(view.terminal_panes.len(), 2);
}

#[test]
fn resized_terminal_layout_survives_tab_switch_and_detach_return() {
    let router = test_router();
    let mut app = router_test_state();
    let root_tab_id = app.open_local_shell_tab();
    assert!(router.activate_tab(MAIN_WINDOW_ID, root_tab_id, &mut app));
    let child_tab_id = app.open_local_shell_tab();
    assert!(router.complete_pane_split(
        MAIN_WINDOW_ID,
        root_tab_id,
        PaneDirection::Right,
        child_tab_id,
        &mut app,
    ));
    assert!(
        router
            .resize_terminal_divider(MAIN_WINDOW_ID, 0, 0.7)
            .is_some()
    );

    let other_tab_id = app.open_local_shell_tab();
    assert!(router.activate_tab(MAIN_WINDOW_ID, other_tab_id, &mut app));
    assert!(
        router
            .resize_terminal_divider(MAIN_WINDOW_ID, 0, 0.4)
            .is_none()
    );
    assert!(router.activate_tab(MAIN_WINDOW_ID, root_tab_id, &mut app));
    let view = router.views(&mut app).pop().expect("main window view");
    assert!((view.terminal_dividers[0].ratio - 0.7).abs() < f32::EPSILON);
    assert!((view.terminal_panes[0].placement.width - 0.7).abs() < f32::EPSILON);

    let pane_tab_ids = router.pane_tab_ids(MAIN_WINDOW_ID, root_tab_id);
    let pane_tree = router
        .take_pane_tree_for_detach(MAIN_WINDOW_ID, root_tab_id)
        .expect("pane group should detach");
    let detached_id = Uuid::new_v4();
    let transfer = app
        .workspace_transfer_for_terminal_panes(&pane_tab_ids, MAIN_WINDOW_ID, root_tab_id)
        .expect("pane group transfer");
    router.register_detached(
        detached_id,
        slint::Weak::<AppWindow>::default(),
        transfer,
        Some(pane_tree),
    );
    let detached_view = router
        .views(&mut app)
        .into_iter()
        .find(|view| view.active_tab_id == Some(root_tab_id))
        .expect("detached window view");
    assert!((detached_view.terminal_dividers[0].ratio - 0.7).abs() < f32::EPSILON);

    let detached = router
        .remove_detached(detached_id)
        .expect("detached route should return");
    assert_eq!(router.restore_detached(&detached), Some(child_tab_id));
    let main_view = router
        .views(&mut app)
        .into_iter()
        .find(|view| view.active_tab_id == Some(root_tab_id))
        .expect("returned main window view");
    assert!((main_view.terminal_dividers[0].ratio - 0.7).abs() < f32::EPSILON);
}

#[test]
fn closing_and_detaching_use_the_whole_terminal_pane_group() {
    let router = test_router();
    let mut app = router_test_state();
    let root_tab_id = app.open_local_shell_tab();
    assert!(router.activate_tab(MAIN_WINDOW_ID, root_tab_id, &mut app));
    let child_tab_id = app.open_local_shell_tab();
    assert!(router.complete_pane_split(
        MAIN_WINDOW_ID,
        root_tab_id,
        PaneDirection::Right,
        child_tab_id,
        &mut app,
    ));
    let pane_tab_ids = router.pane_tab_ids(MAIN_WINDOW_ID, root_tab_id);
    let pane_tree = router
        .take_pane_tree_for_detach(MAIN_WINDOW_ID, root_tab_id)
        .expect("pane group should detach");
    let detached_id = Uuid::new_v4();
    let transfer = app
        .workspace_transfer_for_terminal_panes(&pane_tab_ids, MAIN_WINDOW_ID, root_tab_id)
        .expect("pane group transfer");
    router.register_detached(
        detached_id,
        slint::Weak::<AppWindow>::default(),
        transfer,
        Some(pane_tree),
    );

    assert!(router.tab_ids(MAIN_WINDOW_ID, &app).is_empty());
    assert_eq!(router.tab_ids(detached_id, &app), vec![root_tab_id]);
    assert_eq!(router.active_tab(detached_id), Some(child_tab_id));
    router.set_active(detached_id, root_tab_id);
    assert_eq!(router.active_tab(detached_id), Some(child_tab_id));
    let detached = router
        .remove_detached(detached_id)
        .expect("detached route should return");
    assert_eq!(router.restore_detached(&detached), Some(child_tab_id));
    assert_eq!(router.tab_ids(MAIN_WINDOW_ID, &app), vec![root_tab_id]);
    let closed_tab_ids = router.take_workspace_tab_ids(root_tab_id);
    assert_eq!(closed_tab_ids, vec![root_tab_id, child_tab_id]);
    for tab_id in closed_tab_ids {
        assert!(app.close_tab(tab_id).is_some());
    }
    assert!(app.terminal(root_tab_id).is_none());
    assert!(app.terminal(child_tab_id).is_none());
}

#[test]
fn child_pane_sftp_companion_stays_visible_and_returns_to_the_group() {
    let router = test_router();
    let mut app = router_test_state();
    let profile = SessionProfile::new("server", "server.example", "alice");
    let root_tab_id = app.open_terminal_tab(&profile);
    assert!(router.activate_tab(MAIN_WINDOW_ID, root_tab_id, &mut app));
    let child_tab_id = app.open_terminal_tab(&profile);
    assert!(router.complete_pane_split(
        MAIN_WINDOW_ID,
        root_tab_id,
        PaneDirection::Right,
        child_tab_id,
        &mut app,
    ));
    let sftp_tab_id = app.open_sftp_tab_with_companion(&profile, Some(child_tab_id));
    assert!(router.include_tab(MAIN_WINDOW_ID, sftp_tab_id));
    assert!(router.activate_tab(MAIN_WINDOW_ID, sftp_tab_id, &mut app));

    let view = router.views(&mut app).pop().expect("main window view");
    assert_eq!(
        view.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
        vec![root_tab_id, sftp_tab_id]
    );
    assert_eq!(view.active_tab_id, Some(sftp_tab_id));
    assert_eq!(
        app.switch_ssh_sftp_tab(),
        Some(SshSftpNavigation::Activated(child_tab_id))
    );
    router.set_active(MAIN_WINDOW_ID, child_tab_id);

    let view = router.views(&mut app).pop().expect("main window view");
    assert_eq!(view.active_tab_id, Some(root_tab_id));
    assert_eq!(view.snapshot.id, Some(child_tab_id));
    assert_eq!(view.terminal_panes.len(), 2);

    let closed_tab_ids = router.take_workspace_tab_ids(root_tab_id);
    for tab_id in closed_tab_ids {
        assert!(app.close_tab(tab_id).is_some());
    }
    let view = router.views(&mut app).pop().expect("main window view");
    assert_eq!(
        view.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
        vec![sftp_tab_id]
    );
    assert_eq!(view.active_tab_id, Some(sftp_tab_id));
}
