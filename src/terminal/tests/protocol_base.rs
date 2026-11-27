//! Byte-level compatibility checks through the same parser used for live tabs.

use super::*;

#[test]
fn sgr_21_uses_double_underline_without_canceling_bold() {
    let mut terminal = TerminalModel::new(20, 4, 10);
    terminal.process(b"\x1b[1;21mA\x1b[24mB\x1b[22mC");
    let snapshot = terminal.snapshot();
    let runs = &snapshot.lines[0].runs;
    assert!(runs[0].style.bold);
    assert_eq!(
        runs[0].style.underline_style,
        TerminalUnderlineStyle::Double
    );
    assert!(runs[1].style.bold);
    assert_eq!(runs[1].style.underline_style, TerminalUnderlineStyle::None);
    assert!(!runs[2].style.bold);
}

#[test]
fn line_feed_new_line_mode_applies_to_lf_vt_and_ff() {
    let mut terminal = TerminalModel::new(20, 6, 10);
    assert_eq!(
        terminal.process_with_responses(b"\x1b[20h\x1b[20$pA\nB\x0bC\x0cD\x1b[20l\x1b[20$p\nE"),
        [b"\x1b[20;1$y", b"\x1b[20;2$y"].map(|reply| reply.to_vec())
    );
    assert_eq!(terminal.contents(), "A\nB\nC\nD\n E");
}

#[test]
fn index_keeps_its_column_and_return_uses_the_negotiated_mode() {
    let mut terminal = TerminalModel::new(20, 4, 10);
    let key = TerminalKey::Return;
    let modifiers = TerminalModifiers::default();
    assert_eq!(
        terminal.encode_input_key(&key, modifiers),
        Some(b"\r".to_vec())
    );

    terminal.process(b"\x1b[20hA\x1bDB\x1bEC");
    assert_eq!(terminal.contents(), "A\n B\nC");
    assert_eq!(
        terminal.encode_input_key(&key, modifiers),
        Some(b"\r\n".to_vec())
    );

    terminal.process(b"\x1b[20l");
    assert_eq!(
        terminal.encode_input_key(&key, modifiers),
        Some(b"\r".to_vec())
    );
}

#[test]
fn extended_mouse_mode_queries_follow_sequence_order() {
    let input = b"\x1b[?1015$p\x1b[?1015h\x1b[?1015$p\x1b[?1006h\x1b[?1015$p\x1b[?1016h\x1b[?1016$p\x1bc\x1b[?1016$p";
    let expected = [
        b"\x1b[?1015;2$y",
        b"\x1b[?1015;1$y",
        b"\x1b[?1015;2$y",
        b"\x1b[?1016;1$y",
        b"\x1b[?1016;2$y",
    ];
    for split in 0..=input.len() {
        let mut terminal = TerminalModel::new(20, 4, 10);
        let mut replies = terminal.process_with_responses(&input[..split]);
        replies.extend(terminal.process_with_responses(&input[split..]));
        assert_eq!(
            replies,
            expected.map(|reply| reply.to_vec()),
            "split {split}"
        );
    }
}

#[test]
fn saved_cursor_is_independent_on_each_screen() {
    let mut terminal = TerminalModel::new(20, 4, 10);
    terminal.process(b"primary\x1b[?1048h\r\x1b[?1048l!");
    assert_eq!(terminal.viewport_mode(), TerminalViewportMode::Follow);
    assert_eq!(terminal.contents(), "primary!");

    terminal.process(b"\x1b[?1049h\x1b[Halternate\x1b[?1048h\r\x1b[?1048l!");
    assert_eq!(
        terminal.viewport_mode(),
        TerminalViewportMode::AlternateScreen
    );
    assert_eq!(terminal.contents(), "alternate!");
    terminal.process(b"\x1b[?1049l\x1b[?1048l?");
    assert_eq!(terminal.contents(), "primary!?");
}

#[test]
fn legacy_alternate_switches_do_not_clear_or_switch_the_primary_screen() {
    let mut terminal = TerminalModel::new(20, 4, 10);
    assert_eq!(
        terminal.process_with_responses(b"primary\x1b[?47h\x1b[?1047h\x1b[?47$p\x1b[?1047$p"),
        [b"\x1b[?47;0$y".as_slice(), b"\x1b[?1047;0$y"]
    );
    assert_eq!(terminal.viewport_mode(), TerminalViewportMode::Follow);
    assert_eq!(terminal.contents(), "primary");
}

#[test]
fn osc_color_replies_use_query_time_palette_and_matching_terminators() {
    let input = b"\x1b]10;?\x07\x1b]10;#123456\x1b\\\x1b]10;?\x1b\\\x1b]110\x07\x1b]10;?\x07";
    let expected: &[&[u8]] = &[
        b"\x1b]10;rgb:e5e5/e5e5/e5e5\x07",
        b"\x1b]10;rgb:1212/3434/5656\x1b\\",
        b"\x1b]10;rgb:e5e5/e5e5/e5e5\x07",
    ];
    for split in 0..=input.len() {
        let mut terminal = TerminalModel::new(20, 4, 10);
        let mut replies = terminal.process_with_responses(&input[..split]);
        replies.extend(terminal.process_with_responses(&input[split..]));
        assert_eq!(replies, expected, "split {split}");
    }
}

#[test]
fn cell_size_and_color_queries_share_the_bounded_ordered_queue() {
    let mut terminal = TerminalModel::new(20, 4, 10);
    assert!(
        terminal
            .process_with_responses(b"\x1b[16t\x1b[14t")
            .is_empty()
    );
    let metrics = TerminalGridMetrics::from_physical_cells(20, 4, 9.0, 19.0).unwrap();
    assert_eq!(
        terminal.set_grid_metrics(metrics),
        [b"\x1b[6;19;9t".as_slice(), b"\x1b[4;76;180t"]
    );
    let queries = b"\x1b[16t\x1b]4;1;?\x07".repeat(PROTOCOL_RESPONSE_CAPACITY);
    let replies = terminal.process_with_responses(&queries);
    assert_eq!(replies.len(), PROTOCOL_RESPONSE_CAPACITY);
    let (pairs, remainder) = replies.as_chunks::<2>();
    assert!(remainder.is_empty());
    for [cell, color] in pairs {
        assert_eq!(cell, b"\x1b[6;19;9t");
        assert_eq!(color, b"\x1b]4;1;rgb:cdcd/0000/0000\x07");
    }
}

#[test]
fn canceled_and_oversized_osc_sequences_are_discarded() {
    let mut terminal = TerminalModel::new_with_osc52_clipboard(20, 4, 10, true);
    terminal.process(b"\x1b]2;cancel\x18\x1b]52;c;SGVsbG8=\x1a\x1b]2;abort\x1b[?1015h");
    assert_eq!(terminal.take_title_update(), None);
    assert_eq!(terminal.take_clipboard_store(), None);
    assert_eq!(
        terminal.process_with_responses(b"\x1b[?1015$p"),
        [b"\x1b[?1015;1$y".as_slice()]
    );

    terminal.process(b"\x1b]2;");
    for _ in 0..25 {
        terminal.process(&[b'x'; 4096]);
    }
    terminal.process(b"\x1b");
    terminal.process(b"\\");
    assert_eq!(terminal.take_title_update(), None);
    terminal.process(b"\x1b]2;new\x1b");
    assert_eq!(terminal.take_title_update(), None);
    terminal.process(b"\\");
    assert_eq!(terminal.take_title_update(), Some(Some("new".to_owned())));
}

#[test]
fn osc52_accepts_only_the_default_clipboard_and_complete_payloads() {
    let mut terminal = TerminalModel::new_with_osc52_clipboard(20, 4, 10, true);
    terminal
        .process(b"\x1b]52;cp;?\x07\x1b]52;cp;SGVsbG8=\x07\x1b]52;c;?;extra\x07\x1b]52;c;!\x07");
    assert!(terminal.take_clipboard_load().is_none());
    assert_eq!(terminal.take_clipboard_store(), None);
    terminal.process(b"\x1b]52;;?\x1b\\\x1b]52;c;\x07");
    let formatter = terminal.take_clipboard_load().unwrap();
    assert_eq!(formatter("Hello"), "\x1b]52;c;SGVsbG8=\x1b\\");
    assert_eq!(terminal.take_clipboard_store().as_deref(), Some(""));
}

#[test]
fn hyperlink_hit_spans_use_ids_and_reject_oversized_uris() {
    let mut terminal = TerminalModel::new(20, 4, 10);
    terminal.process(
        b"\x1b]8;id=first;https://example.test\x07ab\x1b]8;id=second;https://example.test\x07cd",
    );
    assert_eq!(
        terminal.hyperlink_at_cell(0, 1),
        Some(("https://example.test".into(), 0, 2))
    );
    assert_eq!(
        terminal.hyperlink_at_cell(0, 2),
        Some(("https://example.test".into(), 2, 4))
    );
    terminal.process(format!("\x1b]8;;https://example.test/{}\x07e", "x".repeat(2048)).as_bytes());
    assert_eq!(terminal.hyperlink_at_cell(0, 4), None);
}

#[test]
fn unicode_sequences_preserve_scalar_columns_and_source_text_through_reflow() {
    let samples = [
        ("A", 1),
        ("中\u{301}", 2),
        ("👩\u{200d}💻", 4),
        ("♥\u{fe0e}", 1),
        ("♥\u{fe0f}", 1),
        ("👋🏽", 4),
        ("🇨🇳", 2),
        ("1\u{fe0f}\u{20e3}", 1),
        ("·", 1),
        ("الع", 3),
        ("कि", 2),
        ("Z", 1),
    ];
    let mut terminal = TerminalModel::new(40, 4, 10);
    terminal.process(b"\x1b[4;1H");
    let mut text = String::new();
    let mut column = 0;
    for (sample, width) in samples {
        text.push_str(sample);
        for byte in sample.as_bytes() {
            terminal.process(std::slice::from_ref(byte));
        }
        column += width;
        assert_eq!(terminal.snapshot().cursor_column, column, "{sample:?}");
    }
    assert_eq!(terminal.selection_text(3, 0, 3, column - 1), text);

    for columns in [10, 11] {
        terminal.resize(columns, 4);
        let narrow = terminal.snapshot();
        let first_row = narrow
            .lines
            .iter()
            .position(|line| {
                line.runs
                    .first()
                    .is_some_and(|run| run.text.starts_with('A'))
            })
            .unwrap();
        assert!(narrow.cursor_row > first_row);
        assert_eq!(
            terminal.selection_text(first_row, 0, narrow.cursor_row, narrow.cursor_column - 1),
            text,
            "{columns} columns"
        );
        terminal.resize(40, 4);
        let wide = terminal.snapshot();
        assert_eq!(wide.cursor_column, column, "{columns} columns");
        assert_eq!(
            terminal.selection_text(wide.cursor_row, 0, wide.cursor_row, column - 1),
            text,
            "{columns} columns"
        );
    }
}

#[test]
fn reflow_keeps_the_cursor_after_text_when_a_wide_cell_wraps() {
    let text = "123456789中Z";
    for (columns, narrow_cursor) in [(10, 3), (11, 1)] {
        let mut terminal = TerminalModel::new(40, 4, 10);
        terminal.process(b"\x1b[4;1H");
        terminal.process(text.as_bytes());
        assert_eq!(terminal.snapshot().cursor_column, 12);

        terminal.resize(columns, 4);
        let actual_narrow_cursor = terminal.snapshot().cursor_column;
        terminal.resize(40, 4);
        let wide = terminal.snapshot();
        assert_eq!(
            terminal.selection_text(wide.cursor_row, 0, wide.cursor_row, 11),
            text
        );
        terminal.process(b"!");
        assert_eq!(
            (
                actual_narrow_cursor,
                wide.cursor_column,
                terminal.contents().lines().last()
            ),
            (narrow_cursor, 12, Some("123456789中Z!")),
            "{columns} columns"
        );
    }
}

#[test]
fn reflow_keeps_the_write_position_at_a_full_row_after_a_wide_wrap() {
    let text = "123456789中12345678";
    let mut terminal = TerminalModel::new(40, 4, 10);
    terminal.process(b"\x1b[4;1H");
    terminal.process(text.as_bytes());
    terminal.resize(10, 4);
    let narrow = terminal.snapshot();
    assert_eq!(narrow.cursor_column, 9);
    assert!(terminal.term.grid().cursor.input_needs_wrap);

    terminal.process(b"!");
    assert_eq!(
        terminal.contents().lines().last(),
        Some("123456789中12345678!")
    );
    terminal.resize(40, 4);
    assert_eq!(terminal.snapshot().cursor_column, 20);
}

#[test]
fn reflow_counts_each_wide_wrap_after_scrolling_into_history() {
    let text = "123456789中1234567文Z";
    let mut terminal = TerminalModel::new(40, 3, 10);
    terminal.process(b"first\r\nsecond\r\n");
    terminal.process(text.as_bytes());
    terminal.resize(10, 3);
    assert_eq!(terminal.snapshot().cursor_column, 3);
    assert!(
        terminal.scroll(1),
        "reflow should leave history above the viewport"
    );
    terminal.scroll_to_bottom();

    terminal.resize(40, 3);
    let wide = terminal.snapshot();
    assert_eq!(wide.cursor_column, 21);
    assert_eq!(
        terminal.selection_text(wide.cursor_row, 0, wide.cursor_row, 20),
        text
    );
    terminal.process(b"!");
    assert_eq!(terminal.contents(), "first\nsecond\n123456789中1234567文Z!");
}

#[test]
fn deleting_a_cell_with_a_combining_mark_keeps_neighbor_cells_intact() {
    let mut terminal = TerminalModel::new(20, 4, 10);
    terminal.process("Ae\u{301}中B".as_bytes());
    assert_eq!(terminal.selection_text(0, 1, 0, 1), "e\u{301}");
    terminal.process(b"\x1b[1;2H\x1b[P");
    assert_eq!(terminal.selection_text(0, 0, 0, 3), "A中B");
    assert_eq!(terminal.snapshot().cursor_column, 1);
}
