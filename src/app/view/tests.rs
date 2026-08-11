use super::*;
use slint::Model;

fn terminal_pane_view(tab_id: Uuid, x: f32, width: f32) -> TerminalPaneView {
    TerminalPaneView {
        terminal: TerminalViewState {
            terminal_id: tab_id.to_string().into(),
            ..Default::default()
        },
        x,
        y: 0.0,
        width,
        height: 1.0,
        focused: x == 0.0,
        closable: false,
    }
}

#[test]
fn terminal_pane_snapshots_update_existing_model_rows() {
    let first_id = Uuid::from_u128(1);
    let second_id = Uuid::from_u128(2);
    let mut first_pane = terminal_pane_view(first_id, 0.0, 0.5);
    first_pane.terminal.render_lines =
        ModelRc::new(VecModel::from(vec![terminal_render_line_with_text(
            "before",
        )]));
    first_pane.terminal.cursor_state = ModelRc::new(VecModel::from(vec![TerminalCursorState {
        row: 1,
        column: 2,
        visible: true,
        text: "b".into(),
    }]));
    let panes = ModelRc::new(VecModel::from(vec![
        first_pane,
        terminal_pane_view(second_id, 0.5, 0.5),
    ]));
    let original_lines = panes.row_data(0).unwrap().terminal.render_lines;
    let original_runs = original_lines.row_data(0).unwrap().runs;
    let original_cursor = panes.row_data(0).unwrap().terminal.cursor_state;
    let dividers = ModelRc::new(VecModel::from(vec![TerminalPaneDividerView {
        id: 0,
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
        ratio: 0.5,
        vertical: true,
    }]));
    let mut updated_panes = vec![
        terminal_pane_view(first_id, 0.0, 0.6),
        terminal_pane_view(second_id, 0.6, 0.4),
    ];
    updated_panes[0].terminal.connected = true;
    updated_panes[0].terminal.render_lines =
        ModelRc::new(VecModel::from(vec![terminal_render_line_with_text(
            "after",
        )]));
    updated_panes[0].terminal.cursor_state =
        ModelRc::new(VecModel::from(vec![TerminalCursorState {
            row: 3,
            column: 7,
            visible: true,
            text: "a".into(),
        }]));
    let updated_dividers = vec![TerminalPaneDividerView {
        id: 0,
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
        ratio: 0.6,
        vertical: true,
    }];

    assert!(update_terminal_pane_snapshot_models(
        &panes,
        &dividers,
        &updated_panes,
        &updated_dividers,
    ));
    let updated_first_pane = panes.row_data(0).unwrap();
    assert!(updated_first_pane.terminal.connected);
    assert_eq!(updated_first_pane.terminal.render_lines, original_lines);
    assert_eq!(updated_first_pane.terminal.cursor_state, original_cursor);
    let updated_cursor = original_cursor.row_data(0).unwrap();
    assert_eq!(updated_cursor.row, 3);
    assert_eq!(updated_cursor.column, 7);
    assert_eq!(updated_cursor.text.as_str(), "a");
    let updated_first_line = original_lines.row_data(0).unwrap();
    assert_eq!(updated_first_line.runs, original_runs);
    assert_eq!(original_runs.row_data(0).unwrap().text.as_str(), "after");
    assert_eq!(panes.row_data(1).unwrap().x, 0.6);
    assert_eq!(dividers.row_data(0).unwrap().ratio, 0.6);
}

#[test]
fn terminal_pane_snapshots_reset_existing_nested_models_when_row_counts_change() {
    let tab_id = Uuid::from_u128(1);
    let mut pane = terminal_pane_view(tab_id, 0.0, 1.0);
    pane.terminal.render_lines =
        ModelRc::new(VecModel::from(vec![terminal_render_line_with_text(
            "before",
        )]));
    let panes = ModelRc::new(VecModel::from(vec![pane]));
    let original_lines = panes.row_data(0).unwrap().terminal.render_lines;
    let original_runs = original_lines.row_data(0).unwrap().runs;
    let dividers = ModelRc::new(VecModel::from(Vec::<TerminalPaneDividerView>::new()));
    let mut updated_pane = terminal_pane_view(tab_id, 0.0, 1.0);
    updated_pane.terminal.render_lines = ModelRc::new(VecModel::from(vec![
        TerminalRenderLine {
            runs: ModelRc::new(VecModel::from(vec![
                terminal_render_run_with_text("after"),
                terminal_render_run_with_text("-suffix"),
            ])),
        },
        terminal_render_line_with_text("second-line"),
    ]));

    assert!(update_terminal_pane_snapshot_models(
        &panes,
        &dividers,
        &[updated_pane],
        &[],
    ));
    let updated_lines = panes.row_data(0).unwrap().terminal.render_lines;
    assert_eq!(updated_lines, original_lines);
    assert_eq!(updated_lines.row_count(), 2);
    let updated_runs = updated_lines.row_data(0).unwrap().runs;
    assert_eq!(updated_runs, original_runs);
    assert_eq!(updated_runs.row_count(), 2);
    assert_eq!(updated_runs.row_data(0).unwrap().text.as_str(), "after");
    assert_eq!(updated_runs.row_data(1).unwrap().text.as_str(), "-suffix");
    assert_eq!(
        updated_lines
            .row_data(1)
            .unwrap()
            .runs
            .row_data(0)
            .unwrap()
            .text
            .as_str(),
        "second-line"
    );
}

fn terminal_render_line_with_text(text: &str) -> TerminalRenderLine {
    TerminalRenderLine {
        runs: ModelRc::new(VecModel::from(vec![terminal_render_run_with_text(text)])),
    }
}

fn terminal_render_run_with_text(text: &str) -> TerminalRenderRun {
    TerminalRenderRun {
        text: text.into(),
        cells: 1,
        ..Default::default()
    }
}

#[test]
fn terminal_pane_layout_updates_existing_model_rows() {
    let first_id = Uuid::from_u128(1);
    let second_id = Uuid::from_u128(2);
    let panes = ModelRc::new(VecModel::from(vec![
        terminal_pane_view(first_id, 0.0, 0.5),
        terminal_pane_view(second_id, 0.5, 0.5),
    ]));
    let dividers = ModelRc::new(VecModel::from(vec![TerminalPaneDividerView {
        id: 0,
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
        ratio: 0.5,
        vertical: true,
    }]));
    let layout = PaneLayout {
        panes: vec![
            PanePlacement {
                tab_id: first_id,
                x: 0.0,
                y: 0.0,
                width: 0.7,
                height: 1.0,
                focused: true,
            },
            PanePlacement {
                tab_id: second_id,
                x: 0.7,
                y: 0.0,
                width: 0.3,
                height: 1.0,
                focused: false,
            },
        ],
        dividers: vec![PaneDividerPlacement {
            id: 0,
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            ratio: 0.7,
            vertical: true,
        }],
    };

    assert!(update_terminal_pane_layout_models(
        &panes, &dividers, layout
    ));
    assert_eq!(panes.row_data(0).unwrap().width, 0.7);
    assert_eq!(panes.row_data(1).unwrap().x, 0.7);
    assert_eq!(dividers.row_data(0).unwrap().ratio, 0.7);
}

#[test]
fn terminal_pane_layout_rejects_stale_model_identity_without_partial_update() {
    let visible_id = Uuid::from_u128(1);
    let stale_id = Uuid::from_u128(2);
    let panes = ModelRc::new(VecModel::from(vec![terminal_pane_view(
        visible_id, 0.0, 1.0,
    )]));
    let dividers = ModelRc::new(VecModel::from(Vec::<TerminalPaneDividerView>::new()));
    let layout = PaneLayout {
        panes: vec![PanePlacement {
            tab_id: stale_id,
            x: 0.0,
            y: 0.0,
            width: 0.7,
            height: 1.0,
            focused: true,
        }],
        dividers: Vec::new(),
    };

    assert!(!update_terminal_pane_layout_models(
        &panes, &dividers, layout
    ));
    assert_eq!(panes.row_data(0).unwrap().width, 1.0);
}

#[test]
fn terminal_select_all_shortcut_preserves_shell_control_a() {
    assert_eq!(terminal_select_all_shortcut_for_platform(true), "Cmd+A");
    assert_eq!(
        terminal_select_all_shortcut_for_platform(false),
        "Ctrl+Shift+A"
    );
}

#[test]
fn settings_workbench_is_exposed_as_a_workspace_tab() {
    let rows = visible_workspace_tab_rows(vec![
        WorkspaceTabSummary {
            id: Uuid::new_v4(),
            title: "Settings".to_owned(),
            kind: "settings",
            connected: false,
        },
        WorkspaceTabSummary {
            id: Uuid::new_v4(),
            title: "New session".to_owned(),
            kind: "session-editor",
            connected: false,
        },
    ]);

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].kind.as_str(), "settings");
    assert_eq!(rows[1].kind.as_str(), "session-editor");
}

#[test]
fn session_group_rows_keep_profiles_nested_under_their_group() {
    let mut production_a = SessionProfile::new("prod-a", "a.example", "alice");
    production_a.group_name = " Production ".into();
    let mut production_b = SessionProfile::new("prod-b", "192.168.1.202", "zhushixin");
    production_b.group_name = "Production".into();
    let ungrouped = SessionProfile::new("local", "local.example", "carol");
    let sessions = SessionStore {
        sessions: vec![production_a, production_b, ungrouped],
        ..SessionStore::default()
    };
    let rows = session_group_rows(&sessions);

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name.as_str(), "Production");
    assert_eq!(rows[0].icon.as_str(), "Pr");
    assert_eq!(rows[0].profiles.row_count(), 2);
    let production_a = rows[0].profiles.row_data(0).unwrap();
    assert_eq!(production_a.name.as_str(), "prod-a");
    assert_eq!(production_a.endpoint.as_str(), "al*ce@a.example:22");
    let production_b = rows[0].profiles.row_data(1).unwrap();
    assert_eq!(production_b.name.as_str(), "prod-b");
    assert_eq!(production_b.endpoint.as_str(), "zh*in@192.*.1.202:22");
    assert_eq!(
        production_b.details.as_str(),
        "SSH · zhushixin@192.168.1.202:22"
    );

    assert_eq!(rows[1].name.as_str(), "Ungrouped");
    assert_eq!(rows[1].icon.as_str(), "Un");
    assert_eq!(rows[1].profiles.row_count(), 1);
    assert_eq!(rows[1].profiles.row_data(0).unwrap().name.as_str(), "local");
}

#[test]
fn full_group_labels_leave_server_badges_compact() {
    let mut server = SessionProfile::new("production-server", "prod.example", "alice");
    server.group_name = "Production systems".into();
    let mut sessions = SessionStore {
        sessions: vec![server],
        ..SessionStore::default()
    };
    sessions.settings.workspace.collapsed_group_label_chars = 0;

    let rows = session_group_rows(&sessions);

    assert_eq!(rows[0].icon.as_str(), "Production systems");
    assert_eq!(rows[0].profiles.row_data(0).unwrap().icon.as_str(), "pr");
}

#[test]
fn empty_persistent_groups_remain_visible() {
    let sessions = SessionStore {
        groups: vec!["Empty".into()],
        ..SessionStore::default()
    };

    let rows = session_group_rows(&sessions);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name.as_str(), "Empty");
    assert_eq!(rows[0].profiles.row_count(), 0);
}

#[test]
fn connection_options_include_collapsed_profiles_with_masked_endpoints() {
    let visible = SessionProfile::new("visible", "server.example", "alice");
    let hidden = SessionProfile::new("hidden", "192.168.1.202", "zhushixin");
    let sessions = SessionStore {
        sessions: vec![visible.clone(), hidden.clone()],
        ..SessionStore::default()
    };

    let options = connection_option_rows(&sessions);

    assert_eq!(options.len(), 2);
    assert_eq!(options[0].id.as_str(), visible.id.to_string());
    assert_eq!(options[0].name.as_str(), "visible");
    assert_eq!(options[0].endpoint.as_str(), "al*ce@server.example:22");
    assert_eq!(options[1].id.as_str(), hidden.id.to_string());
    assert_eq!(options[1].name.as_str(), "hidden");
    assert_eq!(options[1].endpoint.as_str(), "zh*in@192.*.1.202:22");
}
