use super::*;

#[test]
fn connection_reset_drops_admitted_and_pending_frames_but_advances_selection_identity() {
    let mut old = TerminalModel::new(80, 24, 100);
    old.process(b"old connection");
    let mut viewport = TerminalViewport::default();
    viewport.snapshot(Some(&mut old)).unwrap();
    old.process(b" pending");
    viewport.defer_output();
    viewport.prepare(Some(&mut old));
    viewport.reset();
    assert_eq!(viewport.selection_revision(), 1);
    assert!(viewport.snapshot(None).is_none());
    let mut new = TerminalModel::new(80, 24, 100);
    assert_eq!(
        viewport.snapshot(Some(&mut new)).unwrap().lines,
        new.snapshot().lines
    );
}

#[test]
fn resize_replaces_an_admitted_frame_and_invalidates_selection_once() {
    let mut model = TerminalModel::new(80, 24, 100);
    let mut viewport = TerminalViewport::default();
    viewport.snapshot(Some(&mut model)).unwrap();
    model.process(b"pending output");
    viewport.defer_output();
    assert!(viewport.prepare(Some(&mut model)));

    let size = TerminalSize::model(40, 10);
    assert!(viewport.resize(&mut model, size));
    assert_eq!(viewport.selection_revision(), 1);
    let snapshot = viewport.snapshot(Some(&mut model)).unwrap();
    assert_eq!((snapshot.max_columns, snapshot.lines.len()), (40, 10));
    assert!(snapshot.full_refresh);
    assert!(!viewport.resize(&mut model, size));
    assert_eq!(viewport.selection_revision(), 1);
}

#[test]
fn scrolling_and_following_change_selection_identity_but_output_does_not() {
    let mut model = TerminalModel::new(20, 3, 100);
    let mut viewport = TerminalViewport::default();
    for index in 0..32 {
        model.process(format!("line-{index}\r\n").as_bytes());
    }
    viewport.snapshot(Some(&mut model)).unwrap();
    assert!(viewport.scroll(&mut model, 1));
    assert_eq!(viewport.selection_revision(), 1);
    let snapshot = viewport.snapshot(Some(&mut model)).unwrap();
    assert_eq!(snapshot.display_offset, 1);
    assert!(!viewport.scroll(&mut model, 0));
    assert_eq!(viewport.selection_revision(), 1);

    assert!(viewport.scroll_to_bottom(&mut model));
    assert_eq!(viewport.selection_revision(), 2);
    assert_eq!(
        viewport.snapshot(Some(&mut model)).unwrap().display_offset,
        0
    );
    assert!(!viewport.scroll_to_bottom(&mut model));
    model.process(b"ordinary output");
    viewport.defer_output();
    assert!(viewport.prepare(Some(&mut model)));
    viewport.snapshot(Some(&mut model)).unwrap();
    viewport.discard_pending();
    assert_eq!(viewport.selection_revision(), 2);
}

#[test]
fn resize_during_sync_preserves_the_visible_frame_until_release() {
    let mut model = TerminalModel::new(80, 24, 100);
    let mut viewport = TerminalViewport::default();
    model.process(b"before");
    let before = viewport.snapshot(Some(&mut model)).unwrap();
    model.process(b"\x1b[?2026h\x1b[2J\x1b[Hafter");
    viewport.defer_output();
    assert!(viewport.resize(&mut model, TerminalSize::model(40, 10)));
    assert!(!viewport.prepare(Some(&mut model)));
    assert_eq!(
        viewport.snapshot(Some(&mut model)).unwrap().lines,
        before.lines
    );
    assert_eq!(viewport.selection_revision(), 1);

    model.process(b"\x1b[?2026l");
    assert!(viewport.prepare(Some(&mut model)));
    let after = viewport.snapshot(Some(&mut model)).unwrap();
    assert_eq!((after.max_columns, after.lines.len()), (40, 10));
    assert_ne!(after.lines, before.lines);
}

#[test]
fn selection_identity_wraps_without_affecting_another_terminal() {
    let mut first_model = TerminalModel::new(80, 24, 100);
    let mut first = TerminalViewport {
        selection_revision: i32::MAX,
        ..Default::default()
    };
    let second = TerminalViewport::default();
    assert!(first.resize(&mut first_model, TerminalSize::model(40, 10)));
    assert_eq!(first.selection_revision(), 1);
    assert_eq!(second.selection_revision(), 0);
}
