use super::*;

#[test]
fn output_effects_are_drained_once_and_clipboard_is_denied_by_default() {
    let mut model = TerminalModel::new(80, 24, 10);
    let output = TerminalOutput::process(
        &mut model,
        b"\x1b]2;fixture-title\x07\x07\x1b]52;c;dGVzdA==\x07\x1b]52;c;?\x07\x1b[6n",
    );
    assert_eq!(output.title_update, Some(Some("fixture-title".to_owned())));
    assert!(output.bell);
    assert_eq!(output.responses, vec![b"\x1b[1;1R".to_vec()]);
    assert!(output.clipboard_store.is_none());
    assert!(output.clipboard_load.is_none());
    let drained = TerminalOutput::process(&mut model, &[]);
    assert!(drained.title_update.is_none());
    assert!(!drained.bell);
    assert!(drained.responses.is_empty());
}

#[test]
fn permission_enabled_effects_remain_owned_requests_without_platform_side_effects() {
    let mut model = TerminalModel::new_with_osc52_clipboard(80, 24, 10, true);
    let output = TerminalOutput::process(
        &mut model,
        b"\x1b]52;c;dGVzdA==\x07\x1b]52;c;?\x07\x1b[?2026h",
    );
    assert_eq!(output.clipboard_store.as_deref(), Some("test"));
    assert!(output.clipboard_load.is_some());
    assert_eq!(
        output.clipboard_permission_generation,
        model.osc52_permission_generation()
    );
    assert!(output.presentation_hold.is_some());
    let drained = TerminalOutput::process(&mut model, b"\x1b[?2026l");
    assert!(drained.clipboard_store.is_none());
    assert!(drained.clipboard_load.is_none());
    assert!(drained.presentation_hold.is_none());
}
