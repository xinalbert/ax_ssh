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
fn terminal_render_lines_reuse_existing_rows_when_output_is_unchanged() {
    let current = ModelRc::new(VecModel::from(vec![terminal_render_line_with_text("same")]));
    let original_line = current.row_data(0).expect("current line should exist");
    let original_backgrounds = original_line.backgrounds.clone();
    let original_decorations = original_line.decorations.clone();
    let original_runs = original_line.runs.clone();
    let updated = vec![terminal_render_line_with_text("same")];

    assert!(update_terminal_render_lines(&current, &updated));
    assert_eq!(
        current.row_data(0).expect("line should remain"),
        original_line
    );
    assert_eq!(
        current.row_data(0).expect("line should remain").runs,
        original_runs
    );
    assert_eq!(
        current.row_data(0).expect("line should remain").backgrounds,
        original_backgrounds
    );
    assert_eq!(
        current.row_data(0).expect("line should remain").decorations,
        original_decorations
    );

    let changed = vec![terminal_render_line_with_text("changed")];
    assert!(update_terminal_render_lines(&current, &changed));
    assert_eq!(
        current.row_data(0).expect("line should remain").runs,
        original_runs
    );
    assert_eq!(
        current.row_data(0).expect("line should remain").backgrounds,
        original_backgrounds
    );
    assert_eq!(
        current.row_data(0).expect("line should remain").decorations,
        original_decorations
    );
    assert_eq!(
        current
            .row_data(0)
            .expect("changed line should remain")
            .runs
            .row_data(0)
            .expect("changed run should exist")
            .text
            .as_str(),
        "changed"
    );

    let runs_after_change = current.row_data(0).expect("line should remain").runs;
    let split_runs = vec![
        terminal_render_run_with_text("left"),
        terminal_render_run_with_text("right"),
    ];
    assert!(update_terminal_render_lines(
        &current,
        &[TerminalRenderLine {
            source_revision_low: 0,
            source_revision_high: 0,
            render_cache_key_low: 0,
            render_cache_key_high: 0,
            backgrounds: ModelRc::new(VecModel::from(Vec::<TerminalBackgroundRun>::new())),
            decorations: ModelRc::new(VecModel::from(Vec::<TerminalDecorationRun>::new())),
            runs: ModelRc::new(VecModel::from(split_runs)),
        }],
    ));
    assert_eq!(
        current.row_data(0).expect("line should remain").runs,
        runs_after_change
    );
    assert_eq!(
        current
            .row_data(0)
            .expect("line should remain")
            .runs
            .row_count(),
        2
    );
}

#[test]
fn terminal_render_tiles_group_rows_and_preserve_unchanged_tile_models() {
    let before_lines = (0..40)
        .map(|row| terminal_render_line_with_text(&format!("row-{row}")))
        .collect::<Vec<_>>();
    let current_lines = ModelRc::new(VecModel::from(before_lines.clone()));
    let current_tiles = ModelRc::new(VecModel::from(terminal_render_tiles(before_lines)));
    assert_eq!(current_tiles.row_count(), 5);
    assert_eq!(current_tiles.row_data(0).unwrap().start_row, 0);
    assert_eq!(current_tiles.row_data(4).unwrap().start_row, 32);
    assert_eq!(current_tiles.row_data(4).unwrap().rows.row_count(), 8);

    let first_tile_rows = current_tiles.row_data(0).unwrap().rows.clone();
    let third_tile_rows = current_tiles.row_data(2).unwrap().rows.clone();
    let mut after_lines = current_lines.iter().collect::<Vec<_>>();
    after_lines[19] = terminal_render_line_with_text("changed");
    let updated_lines = ModelRc::new(VecModel::from(after_lines.clone()));
    let mut updated_tiles = ModelRc::new(VecModel::from(terminal_render_tiles(after_lines)));

    assert!(reuse_terminal_render_tiles(
        &current_tiles,
        &mut updated_tiles,
        &updated_lines,
    ));
    assert_eq!(updated_tiles, current_tiles);
    assert_eq!(current_tiles.row_data(0).unwrap().rows, first_tile_rows);
    assert_eq!(current_tiles.row_data(2).unwrap().rows, third_tile_rows);
    assert_eq!(
        current_tiles
            .row_data(2)
            .unwrap()
            .rows
            .row_data(3)
            .unwrap()
            .runs
            .row_data(0)
            .unwrap()
            .text
            .as_str(),
        "changed"
    );
}

#[test]
fn terminal_render_tiles_keep_short_tail_tile() {
    let lines = (0..19)
        .map(|row| terminal_render_line_with_text(&format!("row-{row}")))
        .collect::<Vec<_>>();
    let tiles = terminal_render_tiles(lines);
    assert_eq!(tiles.len(), 3);
    assert_eq!(tiles[0].rows.row_count(), 8);
    assert_eq!(tiles[1].rows.row_count(), 8);
    assert_eq!(tiles[2].start_row, 16);
    assert_eq!(tiles[2].rows.row_count(), 3);
}

#[test]
fn terminal_render_tiles_follow_partition_strategy_sizes() {
    let lines = (0..17)
        .map(|row| terminal_render_line_with_text(&format!("row-{row}")))
        .collect::<Vec<_>>();

    let rows = terminal_render_tiles_with_rows(lines.clone(), 1);
    assert_eq!(rows.len(), 17);
    assert_eq!(rows[16].start_row, 16);

    let tiles = terminal_render_tiles_with_rows(lines.clone(), 8);
    assert_eq!(tiles.len(), 3);
    assert_eq!(tiles[1].start_row, 8);
    assert_eq!(tiles[2].rows.row_count(), 1);

    let large_tiles = terminal_render_tiles_with_rows(lines, 16);
    assert_eq!(large_tiles.len(), 2);
    assert_eq!(large_tiles[1].start_row, 16);
    assert_eq!(large_tiles[1].rows.row_count(), 1);
}

#[test]
fn terminal_render_tiles_use_revision_summary_for_dirty_tile_updates() {
    let before_lines = (0..16)
        .map(|row| terminal_render_line_with_revision(&format!("row-{row}"), row + 1))
        .collect::<Vec<_>>();
    let current_tiles = ModelRc::new(VecModel::from(terminal_render_tiles(before_lines.clone())));
    let clean_tile_rows = current_tiles.row_data(0).unwrap().rows.clone();
    let current_lines = ModelRc::new(VecModel::from(before_lines));

    let mut after_lines = current_lines.iter().collect::<Vec<_>>();
    after_lines[9] = terminal_render_line_with_revision("changed", 99);
    let updated_lines = ModelRc::new(VecModel::from(after_lines.clone()));
    let mut updated_tiles = ModelRc::new(VecModel::from(terminal_render_tiles(after_lines)));

    assert!(reuse_terminal_render_tiles(
        &current_tiles,
        &mut updated_tiles,
        &updated_lines,
    ));
    assert_eq!(updated_tiles, current_tiles);
    assert_eq!(current_tiles.row_data(0).unwrap().rows, clean_tile_rows);
    assert_eq!(current_tiles.row_data(0).unwrap().source_revision_high, 0);
    assert_eq!(current_tiles.row_data(0).unwrap().source_revision_low, 8);
    assert_eq!(current_tiles.row_data(1).unwrap().source_revision_high, 0);
    assert_eq!(current_tiles.row_data(1).unwrap().source_revision_low, 99);
    assert_eq!(
        current_tiles
            .row_data(1)
            .unwrap()
            .rows
            .row_data(1)
            .unwrap()
            .runs
            .row_data(0)
            .unwrap()
            .text
            .as_str(),
        "changed"
    );
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
            source_revision_low: 0,
            source_revision_high: 0,
            render_cache_key_low: 0,
            render_cache_key_high: 0,
            backgrounds: ModelRc::new(VecModel::from(Vec::<TerminalBackgroundRun>::new())),
            decorations: ModelRc::new(VecModel::from(Vec::<TerminalDecorationRun>::new())),
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

#[test]
fn terminal_pane_snapshot_keeps_parent_row_when_only_nested_models_change() {
    let tab_id = Uuid::from_u128(1);
    let mut pane = terminal_pane_view(tab_id, 0.0, 1.0);
    pane.terminal.render_lines =
        ModelRc::new(VecModel::from(vec![terminal_render_line_with_text(
            "before",
        )]));
    pane.terminal.cursor_state = ModelRc::new(VecModel::from(vec![TerminalCursorState {
        row: 0,
        column: 0,
        visible: true,
        text: "b".into(),
    }]));
    let panes = ModelRc::new(VecModel::from(vec![pane]));
    let dividers = ModelRc::new(VecModel::from(Vec::<TerminalPaneDividerView>::new()));
    let mut updated = terminal_pane_view(tab_id, 0.0, 1.0);
    updated.terminal.render_lines =
        ModelRc::new(VecModel::from(vec![terminal_render_line_with_text(
            "after",
        )]));
    updated.terminal.cursor_state = ModelRc::new(VecModel::from(vec![TerminalCursorState {
        row: 3,
        column: 7,
        visible: true,
        text: "a".into(),
    }]));
    updated.terminal.cursor_row = 3;
    updated.terminal.cursor_column = 7;
    updated.terminal.cursor_text = "a".into();

    assert!(update_terminal_pane_snapshot_models(
        &panes,
        &dividers,
        &[updated],
        &[],
    ));
    let current = panes.row_data(0).expect("pane row");
    assert_eq!(current.terminal.cursor_row, 0);
    assert_eq!(current.terminal.cursor_column, 0);
    assert_eq!(
        current
            .terminal
            .cursor_state
            .row_data(0)
            .expect("cursor row")
            .row,
        3
    );
    assert_eq!(
        current
            .terminal
            .render_lines
            .row_data(0)
            .expect("render line")
            .runs
            .row_data(0)
            .expect("render run")
            .text
            .as_str(),
        "after"
    );
}

#[test]
fn terminal_render_cache_reuses_only_matching_line_and_settings_revisions() {
    let snapshot = TerminalSnapshot {
        lines: vec![Arc::new(ax_ssh::terminal::TerminalStyledLine {
            revision: 7,
            runs: vec![ax_ssh::terminal::TerminalStyledRun {
                text: "ready".to_owned(),
                column: 0,
                cells: 5,
                style: Default::default(),
            }],
        })],
        max_columns: 10,
        cursor_row: 0,
        cursor_column: 5,
        cursor_visible: true,
        cursor_text: " ".to_owned(),
        mouse_reporting: Default::default(),
        mouse_button_reporting_active: false,
        mouse_wheel_reporting_active: false,
    };
    let settings = TerminalRenderSettings {
        color_scheme: TerminalColorScheme::Dark,
        default_foreground: RgbColor::new(204, 204, 204),
        default_background: RgbColor::new(30, 30, 30),
        selection_background: RgbColor::new(38, 79, 120),
        text_brightness: 1.0,
        bright_bold_text: true,
        semantic_highlighting: true,
        semantic_colors: SemanticColorOverrides::default(),
    };
    let renderer = TerminalRenderer::new(settings);
    let first = render_snapshot_lines(&snapshot, &renderer, None);
    let first_runs = first[0].runs.clone();
    let current = ModelRc::new(VecModel::from(first));

    let cached = render_snapshot_lines(&snapshot, &renderer, Some(&current));
    assert_eq!(cached[0].runs, first_runs);

    let changed_renderer = TerminalRenderer::new(TerminalRenderSettings {
        text_brightness: 0.9,
        ..settings
    });
    let settings_changed = render_snapshot_lines(&snapshot, &changed_renderer, Some(&current));
    assert_ne!(settings_changed[0].runs, first_runs);

    let mut changed_line = snapshot.lines[0].as_ref().clone();
    changed_line.revision = u64::from(u32::MAX) + 8;
    let changed_snapshot = TerminalSnapshot {
        lines: vec![Arc::new(changed_line)],
        ..snapshot
    };
    let line_changed = render_snapshot_lines(&changed_snapshot, &renderer, Some(&current));
    assert_ne!(line_changed[0].runs, first_runs);
}

fn terminal_render_line_with_text(text: &str) -> TerminalRenderLine {
    TerminalRenderLine {
        source_revision_low: 0,
        source_revision_high: 0,
        render_cache_key_low: 0,
        render_cache_key_high: 0,
        backgrounds: ModelRc::new(VecModel::from(Vec::<TerminalBackgroundRun>::new())),
        decorations: ModelRc::new(VecModel::from(Vec::<TerminalDecorationRun>::new())),
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

fn terminal_render_line_with_revision(text: &str, revision: u64) -> TerminalRenderLine {
    TerminalRenderLine {
        source_revision_low: revision as u32 as i32,
        source_revision_high: (revision >> 32) as u32 as i32,
        render_cache_key_low: 1,
        render_cache_key_high: 0,
        backgrounds: ModelRc::new(VecModel::from(Vec::<TerminalBackgroundRun>::new())),
        decorations: ModelRc::new(VecModel::from(Vec::<TerminalDecorationRun>::new())),
        runs: ModelRc::new(VecModel::from(vec![terminal_render_run_with_text(text)])),
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
