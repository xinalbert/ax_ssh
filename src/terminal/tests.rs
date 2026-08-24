//! Terminal model regression tests.

use super::*;

#[test]
fn parses_colored_output_and_carriage_return_updates() {
    let mut terminal = TerminalModel::new(80, 24, 10);
    terminal.process(b"\x1b[32mready\x1b[0m\rbusy\r\nnext");
    let snapshot = terminal.snapshot();

    assert_eq!(terminal.contents(), "busyy\nnext");
    assert_eq!((snapshot.cursor_row, snapshot.cursor_column), (1, 4));
    assert_eq!(snapshot.lines[0].runs.len(), 2);
    assert_eq!(snapshot.lines[0].runs[0].text, "busy");
    assert_eq!(snapshot.lines[0].runs[0].column, 0);
    assert_eq!(
        snapshot.lines[0].runs[1].style.foreground,
        TerminalColor::Indexed(2)
    );
}

#[test]
fn snapshots_reuse_undamaged_visible_line_identities() {
    let mut terminal = TerminalModel::new(20, 3, 10);
    terminal.process(b"first\r\nsecond");
    let first = terminal.snapshot();
    let second = terminal.snapshot();

    assert!(
        first
            .lines
            .iter()
            .zip(&second.lines)
            .all(|(first, second)| Arc::ptr_eq(first, second))
    );

    terminal.process(b"\rupdated");
    let updated = terminal.snapshot();
    assert!(Arc::ptr_eq(&second.lines[0], &updated.lines[0]));
    assert!(!Arc::ptr_eq(&second.lines[1], &updated.lines[1]));
    assert!(Arc::ptr_eq(&second.lines[2], &updated.lines[2]));
}

#[test]
fn snapshots_rebuild_rows_when_the_visible_scrollback_offset_changes() {
    let mut terminal = TerminalModel::new(10, 3, 10);
    terminal.process(b"one\r\ntwo\r\nthree\r\nfour");
    let live = terminal.snapshot();
    assert_eq!(snapshot_line_text(&live, 0), "two");
    assert_eq!(snapshot_line_text(&live, 2), "four");

    assert!(terminal.scroll(1));
    let history = terminal.snapshot();
    assert_eq!(snapshot_line_text(&history, 0), "one");
    assert_eq!(snapshot_line_text(&history, 2), "three");
    assert!(
        live.lines
            .iter()
            .zip(&history.lines)
            .all(|(live, history)| !Arc::ptr_eq(live, history))
    );
}

#[test]
fn parses_standard_extended_truecolor_and_attributes() {
    let mut terminal = TerminalModel::new(80, 24, 10);
    terminal.process(b"\x1b[1;3;4;31mred\x1b[22;23;24;38;5;208mindex\x1b[48;2;1;2;3;7mflip");
    let runs = terminal.snapshot().lines[0].runs.clone();

    assert_eq!(runs.len(), 3);
    assert!(runs[0].style.bold);
    assert!(runs[0].style.italic);
    assert!(runs[0].style.underline);
    assert_eq!(runs[0].style.foreground, TerminalColor::Indexed(1));
    assert_eq!(runs[1].style.foreground, TerminalColor::Indexed(208));
    assert!(!runs[1].style.bold);
    assert_eq!(
        runs[2].style.background,
        TerminalColor::Rgb {
            red: 1,
            green: 2,
            blue: 3,
        }
    );
    assert!(runs[2].style.inverse);
}

#[test]
fn terminal_protocol_queries_return_bounded_transport_responses() {
    let mut terminal = TerminalModel::new(80, 24, 10);

    assert_eq!(
        terminal.process_with_responses(b"\x1b[6n"),
        vec![b"\x1b[1;1R".to_vec()]
    );

    let repeated_query = b"\x1b[5n".repeat(PROTOCOL_RESPONSE_CAPACITY + 4);
    let responses = terminal.process_with_responses(&repeated_query);
    assert_eq!(responses.len(), PROTOCOL_RESPONSE_CAPACITY);
    assert!(responses.iter().all(|response| response == b"\x1b[0n"));
    assert_eq!(
        terminal.process_with_responses(b"\x1b[5n"),
        vec![b"\x1b[0n".to_vec()]
    );
}

#[test]
fn wide_characters_occupy_two_grid_cells() {
    let mut terminal = TerminalModel::new(80, 24, 10);
    terminal.process("A中B".as_bytes());
    let snapshot = terminal.snapshot();

    assert_eq!(terminal.contents(), "A中B");
    assert_eq!(snapshot.lines[0].runs.len(), 3);
    assert_eq!(snapshot.lines[0].runs[1].text, "中");
    assert_eq!(snapshot.lines[0].runs[1].column, 1);
    assert_eq!(snapshot.lines[0].runs[1].cells, 2);
    assert_eq!(snapshot.lines[0].runs[2].column, 3);
    assert_eq!(snapshot.cursor_column, 4);
}

#[test]
fn cursor_on_a_wide_cell_uses_its_leading_column_and_width() {
    let mut terminal = TerminalModel::new(20, 3, 10);
    terminal.process("中\x1b[1G".as_bytes());
    let leading = terminal.snapshot();
    assert_eq!(leading.cursor_column, 0);
    assert_eq!(leading.cursor_cells, 2);
    assert_eq!(leading.cursor_text, "中");

    terminal.process(b"\x1b[2G");
    let spacer = terminal.snapshot();
    assert_eq!(spacer.cursor_column, 0);
    assert_eq!(spacer.cursor_cells, 2);
    assert_eq!(spacer.cursor_text, "中");
}

#[test]
fn non_ascii_single_cell_runs_do_not_shape_across_ascii() {
    let mut terminal = TerminalModel::new(80, 24, 10);
    terminal.process("A┌─┐B".as_bytes());
    let runs = terminal.snapshot().lines[0].runs.clone();

    assert_eq!(runs.len(), 5);
    assert_eq!(
        runs[..4]
            .iter()
            .map(|run| run.text.as_str())
            .collect::<Vec<_>>(),
        vec!["A", "┌", "─", "┐"]
    );
    assert!(runs[4].text.starts_with('B'));
    assert_eq!(
        runs.iter().map(|run| run.column).collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4]
    );
}

#[test]
fn visible_row_target_text_preserves_cell_columns_after_wide_characters() {
    let mut terminal = TerminalModel::new(80, 3, 10);
    terminal.process("\u{4e2d} https://example.test".as_bytes());

    assert_eq!(
        terminal.visible_row_text_at_cell(0, 3),
        Some(("\u{4e2d} https://example.test".to_owned(), 2))
    );
    assert_eq!(terminal.visible_row_text_at_cell(0, 1), None);
}

#[test]
fn visible_row_cell_span_maps_target_characters_after_a_wide_prefix() {
    let mut terminal = TerminalModel::new(80, 3, 10);
    terminal.process("中 https://example.test".as_bytes());

    assert_eq!(
        terminal.visible_row_cell_span_for_characters(0, 2, 22),
        Some((3, 23))
    );
}

#[test]
fn terminal_grid_clamps_to_the_small_screen_floor() {
    let mut terminal = TerminalModel::new(1, 1, 10);
    let snapshot = terminal.snapshot();
    assert_eq!(
        (snapshot.max_columns, snapshot.lines.len()),
        (
            usize::from(crate::terminal_dimensions::MIN_TERMINAL_COLUMNS),
            usize::from(crate::terminal_dimensions::MIN_TERMINAL_ROWS),
        )
    );

    terminal.resize(0, 0);
    let snapshot = terminal.snapshot();
    assert_eq!(
        (snapshot.max_columns, snapshot.lines.len()),
        (
            usize::from(crate::terminal_dimensions::MIN_TERMINAL_COLUMNS),
            usize::from(crate::terminal_dimensions::MIN_TERMINAL_ROWS),
        )
    );
}

#[test]
fn growing_a_primary_terminal_without_scrollback_keeps_content_at_the_top() {
    let mut terminal = TerminalModel::new(10, 3, 10);
    terminal.process(b"one\r\ntwo\r\nthree");
    assert_eq!(terminal.snapshot().cursor_row, 2);

    terminal.resize(10, 5);
    let snapshot = terminal.snapshot();
    assert_eq!(snapshot.cursor_row, 2);
    assert_eq!(snapshot_line_text(&snapshot, 0), "one");
    assert_eq!(snapshot_line_text(&snapshot, 1), "two");
    assert_eq!(snapshot_line_text(&snapshot, 2), "three");
    assert_eq!(snapshot_line_text(&snapshot, 3), "");
    assert_eq!(snapshot_line_text(&snapshot, 4), "");

    terminal.process(b"!");
    assert_eq!(snapshot_line_text(&terminal.snapshot(), 2), "three!");
}

#[test]
fn growing_a_primary_terminal_away_from_the_bottom_keeps_its_cursor_row() {
    let mut terminal = TerminalModel::new(10, 3, 10);
    terminal.process(b"one\r\ntwo");
    assert_eq!(terminal.snapshot().cursor_row, 1);

    terminal.resize(10, 5);
    let snapshot = terminal.snapshot();
    assert_eq!(snapshot.cursor_row, 1);
    assert_eq!(snapshot_line_text(&snapshot, 0), "one");
    assert_eq!(snapshot_line_text(&snapshot, 1), "two");
}

#[test]
fn growing_an_alternate_terminal_keeps_standard_resize_behavior() {
    let mut terminal = TerminalModel::new(10, 3, 10);
    terminal.process(b"\x1b[?1049hone\r\ntwo\r\nthree");
    assert_eq!(terminal.snapshot().cursor_row, 2);

    terminal.resize(10, 5);
    let snapshot = terminal.snapshot();
    assert_eq!(snapshot.cursor_row, 2);
    assert_eq!(snapshot_line_text(&snapshot, 0), "one");
    assert_eq!(snapshot_line_text(&snapshot, 2), "three");
}

#[test]
fn growing_a_live_terminal_restores_recent_scrollback_above_the_viewport() {
    let mut terminal = TerminalModel::new(10, 3, 10);
    terminal.process(b"one\r\ntwo\r\nthree\r\nfour");

    terminal.resize(10, 4);
    let snapshot = terminal.snapshot();
    assert_eq!(snapshot.cursor_row, 3);
    assert_eq!(snapshot_line_text(&snapshot, 0), "one");
    assert_eq!(snapshot_line_text(&snapshot, 1), "two");
    assert_eq!(snapshot_line_text(&snapshot, 3), "four");
}

#[test]
fn repeated_primary_terminal_resizes_do_not_create_top_blank_rows() {
    let mut terminal = TerminalModel::new(10, 3, 10);
    terminal.process(b"one\r\ntwo\r\nthree");

    terminal.resize(10, 5);
    terminal.resize(10, 3);
    let snapshot = terminal.snapshot();
    assert_eq!(snapshot.cursor_row, 2);
    assert_eq!(snapshot_line_text(&snapshot, 0), "one");
    assert_eq!(snapshot_line_text(&snapshot, 1), "two");
    assert_eq!(snapshot_line_text(&snapshot, 2), "three");

    terminal.resize(10, 5);
    let snapshot = terminal.snapshot();
    assert_eq!(snapshot.cursor_row, 2);
    assert_eq!(snapshot_line_text(&snapshot, 0), "one");
    assert_eq!(snapshot_line_text(&snapshot, 1), "two");
    assert_eq!(snapshot_line_text(&snapshot, 2), "three");
    assert_eq!(snapshot_line_text(&snapshot, 3), "");
    assert_eq!(snapshot_line_text(&snapshot, 4), "");
}

#[test]
fn growing_while_viewing_scrollback_keeps_the_standard_cursor_position() {
    let mut terminal = TerminalModel::new(10, 3, 10);
    terminal.process(b"one\r\ntwo\r\nthree\r\nfour");
    assert!(terminal.scroll(1));

    terminal.resize(10, 5);
    let snapshot = terminal.snapshot();
    assert_eq!(snapshot.cursor_row, 3);
    assert_eq!(snapshot_line_text(&snapshot, 0), "one");
    assert_eq!(snapshot_line_text(&snapshot, 3), "four");
}

#[test]
fn narrowing_and_widening_a_primary_terminal_reflows_soft_wrapped_content() {
    let mut terminal = TerminalModel::new(20, 5, 20);
    terminal.process(b"\x1b[5;1H0123456789abcdefghij");

    terminal.resize(10, 5);
    let narrow = terminal.snapshot();
    assert_eq!(narrow.cursor_row, 4);
    assert_eq!(snapshot_line_text(&narrow, 3), "0123456789");
    assert_eq!(snapshot_line_text(&narrow, 4), "abcdefghij");

    terminal.resize(20, 5);
    let wide = terminal.snapshot();
    assert_eq!(wide.cursor_row, 4);
    assert_eq!(snapshot_line_text(&wide, 4), "0123456789abcdefghij");
}

#[test]
fn primary_reflow_preserves_hard_breaks_and_wide_characters() {
    let mut terminal = TerminalModel::new(20, 5, 20);
    terminal.process("first中line\r\nsecond中line".as_bytes());

    terminal.resize(10, 5);
    terminal.resize(20, 5);

    assert_eq!(terminal.contents(), "first中line\nsecond中line");
}

#[test]
fn alternate_screen_resize_does_not_reflow_existing_rows() {
    let mut terminal = TerminalModel::new(20, 5, 20);
    terminal.process(b"\x1b[?1049h0123456789abcdefghij");

    terminal.resize(10, 5);
    terminal.resize(20, 5);

    assert_ne!(terminal.contents(), "0123456789abcdefghij");
}

#[test]
fn repeated_resize_preserves_hard_break_columns() {
    let mut terminal = TerminalModel::new(80, 24, 100);
    terminal.process(
        b"2026-08-13 21:33:54\r\n$:\r\nzhushixin@compute-0-0 :\r\n~\r\n2026-08-13 21:33:54\r\n$:\r\nzhushixin@compute-0-0 :\r\n~\r\n",
    );
    for (columns, rows) in [(160, 40), (200, 50), (120, 30), (180, 45), (80, 24)] {
        terminal.resize(columns, rows);
        let snapshot = terminal.snapshot();
        let occupied_columns = snapshot
            .lines
            .iter()
            .filter_map(|line| line.runs.first())
            .map(|run| run.column)
            .collect::<Vec<_>>();
        assert_eq!(occupied_columns, vec![0; 8], "resize {columns}x{rows}");
    }
}

fn snapshot_line_text(snapshot: &TerminalSnapshot, row: usize) -> String {
    let text = snapshot.lines[row]
        .runs
        .iter()
        .map(|run| run.text.as_str())
        .collect::<String>();
    text.trim_end().to_owned()
}

#[test]
fn cursor_visibility_and_cell_text_follow_terminal_state() {
    let mut terminal = TerminalModel::new(80, 24, 10);
    terminal.process(b"prompt\x1b[2D");
    let snapshot = terminal.snapshot();
    assert!(snapshot.cursor_visible);
    assert_eq!(snapshot.cursor_text, "p");

    terminal.process(b"\x1b[?25l");
    assert!(!terminal.snapshot().cursor_visible);
}

#[test]
fn tracks_application_cursor_mode() {
    let mut terminal = TerminalModel::new(80, 24, 10);
    assert!(!terminal.application_cursor());

    terminal.process(b"\x1b[?1h");
    assert!(terminal.application_cursor());

    terminal.process(b"\x1b[?1l");
    assert!(!terminal.application_cursor());
}

#[test]
fn encodes_sgr_click_release_wheel_drag_and_modifiers() {
    let mut terminal = TerminalModel::new(80, 24, 10);
    terminal.process(b"\x1b[?1000h\x1b[?1006h");
    assert_eq!(
        terminal.encode_mouse_event(TerminalMouseEvent {
            kind: TerminalMouseEventKind::Press,
            button: TerminalMouseButton::Left,
            column: 2,
            row: 3,
            modifiers: TerminalMouseModifiers {
                shift: true,
                alt: false,
                control: true
            },
        }),
        Some(b"\x1b[<20;3;4M".to_vec())
    );
    assert_eq!(
        terminal.encode_mouse_event(TerminalMouseEvent {
            kind: TerminalMouseEventKind::Release,
            button: TerminalMouseButton::Left,
            column: 2,
            row: 3,
            modifiers: TerminalMouseModifiers {
                shift: true,
                alt: true,
                control: true,
            },
        }),
        Some(b"\x1b[<28;3;4m".to_vec())
    );
    assert_eq!(
        terminal.encode_mouse_event(TerminalMouseEvent {
            kind: TerminalMouseEventKind::Press,
            button: TerminalMouseButton::WheelDown,
            column: 0,
            row: 0,
            modifiers: TerminalMouseModifiers::default(),
        }),
        Some(b"\x1b[<65;1;1M".to_vec())
    );
}

#[test]
fn encodes_x10_and_utf8_coordinates_with_bounds() {
    let mut terminal = TerminalModel::new(300, 100, 10);
    terminal.process(b"\x1b[?1000h");
    let event = TerminalMouseEvent {
        kind: TerminalMouseEventKind::Press,
        button: TerminalMouseButton::Right,
        column: 299,
        row: 99,
        modifiers: TerminalMouseModifiers::default(),
    };
    assert_eq!(terminal.encode_mouse_event(event), None);
    terminal.process(b"\x1b[?1005h");
    assert_eq!(
        terminal.encode_mouse_event(event),
        Some(vec![27, 91, 77, 34, 197, 140, 194, 132])
    );

    assert_eq!(
        terminal.encode_mouse_event(TerminalMouseEvent {
            kind: TerminalMouseEventKind::Release,
            button: TerminalMouseButton::Right,
            column: 2,
            row: 3,
            modifiers: TerminalMouseModifiers {
                shift: true,
                alt: true,
                control: true,
            },
        }),
        Some(vec![27, 91, 77, 63, 35, 36])
    );
}

#[test]
fn mouse_reporting_modes_gate_press_drag_and_motion_independently() {
    let mut terminal = TerminalModel::new(80, 24, 10);
    let press = TerminalMouseEvent {
        kind: TerminalMouseEventKind::Press,
        button: TerminalMouseButton::Left,
        column: 1,
        row: 1,
        modifiers: TerminalMouseModifiers::default(),
    };
    let release = TerminalMouseEvent {
        kind: TerminalMouseEventKind::Release,
        ..press
    };
    let motion = TerminalMouseEvent {
        kind: TerminalMouseEventKind::Motion,
        ..press
    };
    let cell_motion = TerminalMouseEvent {
        button: TerminalMouseButton::None,
        ..motion
    };
    let invalid_release = TerminalMouseEvent {
        button: TerminalMouseButton::None,
        ..release
    };

    terminal.process(b"\x1b[?1000h");
    assert!(terminal.encode_mouse_event(press).is_some());
    assert!(terminal.encode_mouse_event(release).is_some());
    assert!(terminal.encode_mouse_event(invalid_release).is_none());
    assert!(terminal.encode_mouse_event(motion).is_none());
    assert!(terminal.encode_mouse_event(cell_motion).is_none());

    terminal.process(b"\x1b[?1000l\x1b[?1002h");
    assert!(terminal.encode_mouse_event(press).is_some());
    assert!(terminal.encode_mouse_event(release).is_some());
    assert!(terminal.encode_mouse_event(motion).is_some());
    assert!(terminal.encode_mouse_event(cell_motion).is_none());

    terminal.process(b"\x1b[?1002l\x1b[?1003h");
    assert!(terminal.encode_mouse_event(press).is_some());
    assert!(terminal.encode_mouse_event(release).is_some());
    assert!(terminal.encode_mouse_event(motion).is_some());
    assert!(terminal.encode_mouse_event(cell_motion).is_some());
}

#[test]
fn button_and_wheel_reporting_capabilities_are_independent() {
    let mut terminal = TerminalModel::new(80, 24, 10);

    terminal.process(b"\x1b[?1007h");
    assert!(!terminal.mouse_button_reporting_active());
    assert!(!terminal.mouse_wheel_reporting_active());

    terminal.process(b"\x1b[?1049h");
    assert!(!terminal.mouse_button_reporting_active());
    assert!(terminal.mouse_wheel_reporting_active());

    terminal.process(b"\x1b[?1000h");
    assert!(terminal.mouse_button_reporting_active());
    assert!(terminal.mouse_wheel_reporting_active());

    terminal.process(b"\x1b[?1000l\x1b[?1002h");
    assert!(terminal.mouse_button_reporting_active());
    assert!(terminal.mouse_wheel_reporting_active());

    terminal.process(b"\x1b[?1002l\x1b[?1003h");
    assert!(terminal.mouse_button_reporting_active());
    assert!(terminal.mouse_wheel_reporting_active());
}

#[test]
fn mouse_coordinates_follow_wide_character_cell_columns() {
    let mut terminal = TerminalModel::new(80, 24, 10);
    terminal.process("中A".as_bytes());
    terminal.process(b"\x1b[?1000h\x1b[?1006h");

    let snapshot = terminal.snapshot();
    assert_eq!(snapshot.lines[0].runs[0].column, 0);
    assert_eq!(snapshot.lines[0].runs[0].cells, 2);
    assert_eq!(snapshot.lines[0].runs[1].column, 2);
    assert_eq!(
        terminal.encode_mouse_event(TerminalMouseEvent {
            kind: TerminalMouseEventKind::Press,
            button: TerminalMouseButton::Left,
            column: 2,
            row: 0,
            modifiers: TerminalMouseModifiers::default(),
        }),
        Some(b"\x1b[<0;3;1M".to_vec())
    );
}

#[test]
fn reports_drag_and_motion_only_when_enabled() {
    let mut terminal = TerminalModel::new(80, 24, 10);
    let event = TerminalMouseEvent {
        kind: TerminalMouseEventKind::Motion,
        button: TerminalMouseButton::Left,
        column: 1,
        row: 1,
        modifiers: TerminalMouseModifiers::default(),
    };
    assert_eq!(terminal.encode_mouse_event(event), None);
    terminal.process(b"\x1b[?1002h");
    assert!(terminal.encode_mouse_event(event).is_some());
    terminal.process(b"\x1b[?1002l\x1b[?1003h");
    assert!(terminal.encode_mouse_event(event).is_some());
}

#[test]
fn alternate_screen_scroll_uses_application_cursor_sequences() {
    let mut terminal = TerminalModel::new(80, 24, 10);
    terminal.process(b"\x1b[?1049h\x1b[?1007h");
    assert_eq!(
        terminal.encode_mouse_event(TerminalMouseEvent {
            kind: TerminalMouseEventKind::Press,
            button: TerminalMouseButton::WheelUp,
            column: 0,
            row: 0,
            modifiers: TerminalMouseModifiers::default(),
        }),
        Some(b"\x1b[A".to_vec())
    );
    terminal.process(b"\x1b[?1h");
    assert_eq!(
        terminal.encode_mouse_event(TerminalMouseEvent {
            kind: TerminalMouseEventKind::Press,
            button: TerminalMouseButton::WheelDown,
            column: 0,
            row: 0,
            modifiers: TerminalMouseModifiers::default(),
        }),
        Some(b"\x1bOB".to_vec())
    );
}

#[test]
fn selection_uses_cell_coordinates_and_ignores_wide_continuations() {
    let mut terminal = TerminalModel::new(80, 24, 10);
    terminal.process("one\r\nt中ree".as_bytes());

    assert_eq!(terminal.selection_text(0, 1, 1, 2), "ne\nt中");
}

#[test]
fn selection_preserves_hard_breaks_after_a_soft_wrap() {
    let mut terminal = TerminalModel::new(10, 4, 10);
    terminal.process(b"0123456789A\r\n\r\nlast");

    assert_eq!(terminal.selection_text(0, 0, 3, 3), "0123456789A\n\nlast");
}

#[test]
fn selection_does_not_insert_newline_between_soft_wrapped_rows() {
    let mut terminal = TerminalModel::new(10, 3, 10);
    terminal.process(b"0123456789A");

    assert_eq!(terminal.selection_text(0, 0, 1, 0), "0123456789A");
}

#[test]
fn selection_text_reads_latest_cells_after_output_refresh() {
    let mut terminal = TerminalModel::new(10, 3, 10);
    terminal.process(b"hello");
    terminal.process(b"\rworld");

    assert_eq!(terminal.selection_text(0, 0, 0, 4), "world");
}

#[test]
fn semantic_selection_uses_terminal_punctuation_boundaries() {
    let mut terminal = TerminalModel::new(20, 3, 10);
    terminal.process(b"foo'bar");

    assert_eq!(
        terminal.semantic_selection_range(0, 1),
        Some(TerminalSelectionRange {
            start_row: 0,
            start_column: 0,
            end_row: 0,
            end_column: 2,
        })
    );
    assert_eq!(
        terminal.semantic_selection_range(0, 5),
        Some(TerminalSelectionRange {
            start_row: 0,
            start_column: 4,
            end_row: 0,
            end_column: 6,
        })
    );
}

#[test]
fn semantic_selection_handles_cjk_cells_and_matching_brackets() {
    let mut terminal = TerminalModel::new(20, 3, 10);
    terminal.process("中中文 (value)".as_bytes());

    assert_eq!(
        terminal.semantic_selection_range(0, 1),
        Some(TerminalSelectionRange {
            start_row: 0,
            start_column: 0,
            end_row: 0,
            end_column: 5,
        })
    );
    assert_eq!(
        terminal.semantic_selection_range(0, 7),
        Some(TerminalSelectionRange {
            start_row: 0,
            start_column: 7,
            end_row: 0,
            end_column: 13,
        })
    );
}

#[test]
fn semantic_selection_is_clipped_to_scrolled_viewport() {
    let mut terminal = TerminalModel::new(10, 2, 10);
    terminal.process(b"abcdefghijKLMNOPQRSTuvwxyz\r\nlast\r\n");
    assert!(terminal.scroll(1));

    let range = terminal
        .semantic_selection_range(0, 1)
        .expect("visible word should have a semantic range");
    assert_eq!(
        range,
        TerminalSelectionRange {
            start_row: 0,
            start_column: 0,
            end_row: 1,
            end_column: 5,
        }
    );
    assert_eq!(
        terminal.selection_text(
            range.start_row,
            range.start_column,
            range.end_row,
            range.end_column,
        ),
        "KLMNOPQRSTuvwxyz"
    );
}

#[test]
fn semantic_selection_preserves_soft_wrapped_words() {
    let mut terminal = TerminalModel::new(10, 3, 10);
    terminal.process(b"abcdefghijk");

    let range = terminal
        .semantic_selection_range(0, 1)
        .expect("wrapped word should have a semantic range");
    assert_eq!(
        range,
        TerminalSelectionRange {
            start_row: 0,
            start_column: 0,
            end_row: 1,
            end_column: 0,
        }
    );
    assert_eq!(
        terminal.selection_text(
            range.start_row,
            range.start_column,
            range.end_row,
            range.end_column,
        ),
        "abcdefghijk"
    );
}

#[test]
fn line_selection_uses_logical_lines_and_preserves_hard_breaks() {
    let mut terminal = TerminalModel::new(10, 4, 10);
    terminal.process(b"0123456789A\r\nnext");

    assert_eq!(
        terminal.line_selection_range(0, 1),
        Some(TerminalSelectionRange {
            start_row: 0,
            start_column: 0,
            end_row: 1,
            end_column: 9,
        })
    );
    assert_eq!(terminal.selection_text(0, 0, 1, 9), "0123456789A");
    assert_eq!(terminal.selection_text(0, 0, 2, 9), "0123456789A\nnext");
    assert_eq!(
        terminal.line_selection_range(2, 1),
        Some(TerminalSelectionRange {
            start_row: 2,
            start_column: 0,
            end_row: 2,
            end_column: 9,
        })
    );
}

#[test]
fn line_selection_preserves_soft_wrapped_lines() {
    let mut terminal = TerminalModel::new(10, 3, 10);
    terminal.process(b"abcdefghijk");

    let range = terminal
        .line_selection_range(0, 1)
        .expect("wrapped line should have a line range");
    assert_eq!(
        range,
        TerminalSelectionRange {
            start_row: 0,
            start_column: 0,
            end_row: 1,
            end_column: 9,
        }
    );
    assert_eq!(
        terminal.selection_text(
            range.start_row,
            range.start_column,
            range.end_row,
            range.end_column,
        ),
        "abcdefghijk"
    );
}

#[test]
fn line_selection_is_clipped_to_scrolled_viewport() {
    let mut terminal = TerminalModel::new(10, 2, 10);
    terminal.process(b"abcdefghijKLMNOPQRSTuvwxyz\r\nlast\r\n");
    assert!(terminal.scroll(1));

    assert_eq!(
        terminal.line_selection_range(0, 1),
        Some(TerminalSelectionRange {
            start_row: 0,
            start_column: 0,
            end_row: 1,
            end_column: 9,
        })
    );
}

#[test]
fn scrollback_is_bounded_and_scrollable() {
    let mut terminal = TerminalModel::new(80, 5, 2);
    for index in 0..12 {
        terminal.process(format!("line-{index}\r\n").as_bytes());
    }
    let live = terminal.contents();
    assert!(!live.contains("line-0"));
    assert!(live.contains("line-11"));
    assert!(terminal.scroll(10));
    assert!(terminal.contents().contains("line-8"));
    assert!(!terminal.snapshot().cursor_visible);
    assert!(terminal.scroll_to_bottom());
}

#[test]
fn detached_view_preserves_its_position_while_output_arrives() {
    let mut terminal = TerminalModel::new(20, 3, 20);
    terminal.process(b"one\r\ntwo\r\nthree\r\nfour");
    assert!(terminal.scroll(1));
    let before = terminal.snapshot();
    assert_eq!(before.viewport_mode, TerminalViewportMode::Detached);
    assert!(before.display_offset > 0);
    let before_top = snapshot_line_text(&before, 0);

    terminal.process(b"\r\nfive");
    let after = terminal.snapshot();
    assert_eq!(after.viewport_mode, TerminalViewportMode::Detached);
    assert_eq!(snapshot_line_text(&after, 0), before_top);

    assert!(terminal.scroll_to_bottom());
    assert_eq!(
        terminal.snapshot().viewport_mode,
        TerminalViewportMode::Follow
    );
}

#[test]
fn alternate_screen_resets_local_viewport_state() {
    let mut terminal = TerminalModel::new(20, 3, 20);
    terminal.process(b"one\r\ntwo\r\nthree\r\nfour");
    assert!(terminal.scroll(1));
    assert_eq!(terminal.viewport_mode(), TerminalViewportMode::Detached);

    terminal.process(b"\x1b[?1049h");
    let alternate = terminal.snapshot();
    assert_eq!(
        alternate.viewport_mode,
        TerminalViewportMode::AlternateScreen
    );
    assert_eq!(alternate.display_offset, 0);
    assert!(!terminal.scroll(1));

    terminal.process(b"\x1b[?1049l");
    assert_eq!(
        terminal.snapshot().viewport_mode,
        TerminalViewportMode::Follow
    );
}

#[test]
fn changing_scrollback_preserves_the_visible_grid() {
    let mut terminal = TerminalModel::new(80, 24, 10);
    terminal.process(b"prompt");
    terminal.set_scrollback_lines(20);

    assert_eq!(terminal.contents(), "prompt");
    assert_eq!(terminal.snapshot().cursor_column, 6);
}
