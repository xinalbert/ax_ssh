use super::*;

#[test]
fn deferred_and_synchronized_output_never_escape_a_published_frame() {
    let mut model = TerminalModel::new(80, 24, 10);
    let mut frames = TerminalFrames::default();
    model.process(b"before");
    let before = frames.snapshot(Some(&mut model)).unwrap();
    model.process(b"\r\nafter");
    frames.defer_output();
    assert_eq!(
        frames.snapshot(Some(&mut model)).unwrap().lines,
        before.lines
    );
    assert!(frames.prepare(Some(&mut model)));
    let after = frames.snapshot(Some(&mut model)).unwrap();
    assert_ne!(after.lines, before.lines);
    model.process(b"\x1b[?2026h\x1b[2J\x1b[Hredraw");
    frames.defer_output();
    assert!(!frames.prepare(Some(&mut model)));
    assert_eq!(
        frames.snapshot(Some(&mut model)).unwrap().lines,
        after.lines
    );
    model.process(b"\x1b[?2026l");
    assert!(frames.prepare(Some(&mut model)));
    assert_ne!(
        frames.snapshot(Some(&mut model)).unwrap().lines,
        after.lines
    );
}

#[test]
fn skipped_frame_damage_is_merged_but_control_only_output_does_not_repaint() {
    let mut model = TerminalModel::new(80, 24, 10);
    let mut frames = TerminalFrames::default();
    frames.snapshot(Some(&mut model)).unwrap();
    model.process(b"\x1b[31m");
    assert!(!frames.prepare(Some(&mut model)));
    model.process(b"\x1b[1;1Hfirst");
    assert!(frames.prepare(Some(&mut model)));
    model.process(b"\x1b[2;1Hsecond");
    let snapshot = frames.snapshot(Some(&mut model)).unwrap();
    assert!(snapshot.dirty_rows.contains(&0));
    assert!(snapshot.dirty_rows.contains(&1));
    model.resize(40, 10);
    frames.discard_pending();
    let snapshot = frames.snapshot(Some(&mut model)).unwrap();
    assert_eq!((snapshot.max_columns, snapshot.lines.len()), (40, 10));
    assert!(snapshot.full_refresh);
}
