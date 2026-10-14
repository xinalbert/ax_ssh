use super::*;

fn policy(apple_platform: bool) -> EditPolicy {
    EditPolicy {
        apple_platform,
        secret: false,
        read_only: false,
        single_line: false,
        composing: false,
    }
}

fn chord(text: &str, control: bool, meta: bool, shift: bool) -> KeyEvent {
    let mut event = KeyEvent::default();
    event.text = text.into();
    event.modifiers.control = control;
    event.modifiers.meta = meta;
    event.modifiers.shift = shift;
    event
}

#[test]
fn apple_control_chords_extend_selection_and_preserve_primary_shortcuts() {
    for (key, action) in [
        ("a", EditAction::ParagraphStart),
        ("E", EditAction::ParagraphEnd),
        ("b", EditAction::Backward),
        ("f", EditAction::Forward),
        ("p", EditAction::PreviousLine),
        ("n", EditAction::NextLine),
    ] {
        assert_eq!(
            edit_decision(&chord(key, false, true, true), policy(true)),
            EditDecision::Apply {
                action,
                extend_selection: true
            },
        );
        assert_eq!(
            edit_decision(&chord(key, true, false, false), policy(true)),
            EditDecision::Native
        );
        assert_eq!(
            edit_decision(&chord(key, true, false, false), policy(false)),
            EditDecision::Native
        );
        assert_eq!(
            edit_decision(&chord(key, false, true, false), policy(false)),
            EditDecision::Native
        );
    }
    let mut option_control = chord("a", false, true, false);
    option_control.modifiers.alt = true;
    assert_eq!(
        edit_decision(&option_control, policy(true)),
        EditDecision::Native
    );
    assert_eq!(
        edit_decision(&chord("a", true, true, false), policy(true)),
        EditDecision::Native
    );
}

#[test]
fn protected_inputs_block_clipboard_but_allow_control_navigation_without_selection() {
    for apple_platform in [false, true] {
        let policy = EditPolicy {
            secret: true,
            ..policy(apple_platform)
        };
        for key in ["a", "c", "x"] {
            assert_eq!(
                edit_decision(&chord(key, true, false, true), policy),
                EditDecision::Block
            );
        }
        for key in ["c", "x"] {
            assert_eq!(
                edit_decision(&chord(key, false, true, false), policy),
                EditDecision::Block
            );
        }
        assert_eq!(
            edit_decision(&chord("v", true, false, false), policy),
            EditDecision::Native
        );
        assert_eq!(
            edit_decision(&chord("a", false, true, true), policy),
            if apple_platform {
                EditDecision::Apply {
                    action: EditAction::ParagraphStart,
                    extend_selection: false,
                }
            } else {
                EditDecision::Block
            },
        );
    }
}

#[test]
fn composition_read_only_and_single_line_limits_keep_text_unchanged() {
    let composing = EditPolicy {
        composing: true,
        ..policy(true)
    };
    for key in ["a", "e", "b", "f", "h", "d", "p", "n"] {
        assert_eq!(
            edit_decision(&chord(key, false, true, false), composing),
            EditDecision::Block
        );
    }
    let read_only = EditPolicy {
        read_only: true,
        ..policy(true)
    };
    for key in ["h", "d"] {
        assert_eq!(
            edit_decision(&chord(key, false, true, false), read_only),
            EditDecision::Block
        );
    }
    assert!(matches!(
        edit_decision(&chord("a", false, true, false), read_only),
        EditDecision::Apply {
            action: EditAction::ParagraphStart,
            ..
        }
    ));
    let single_line = EditPolicy {
        single_line: true,
        ..policy(true)
    };
    for key in ["p", "n"] {
        assert_eq!(
            edit_decision(&chord(key, false, true, false), single_line),
            EditDecision::Block
        );
    }
}

#[test]
fn key_identity_does_not_confuse_modifier_events_or_multicharacter_input() {
    for text in [
        "",
        "ae",
        "你好",
        slint::SharedString::from(Key::Shift).as_str(),
    ] {
        assert_eq!(
            edit_decision(&chord(text, false, true, false), policy(true)),
            EditDecision::Native
        );
    }
    assert_eq!(
        edit_decision(&chord("\u{1}", false, true, false), policy(true)),
        EditDecision::Apply {
            action: EditAction::ParagraphStart,
            extend_selection: false
        },
    );
    assert_eq!(
        edit_decision(&chord("h", false, true, true), policy(true)),
        EditDecision::Apply {
            action: EditAction::DeleteBackward,
            extend_selection: false
        },
    );
}
