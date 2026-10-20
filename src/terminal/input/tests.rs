use super::*;

#[test]
fn paste_normalizes_lines_and_removes_escape_bytes() {
    assert_eq!(
        encode_paste("one\r\ntwo\nthree\x1b", false),
        Some(b"one\rtwo\rthree".to_vec())
    );
}

#[test]
fn bracketed_paste_wraps_the_complete_payload() {
    assert_eq!(
        encode_paste("one\ntwo", true),
        Some(b"\x1b[200~one\rtwo\x1b[201~".to_vec())
    );
}

#[test]
fn paste_rejects_empty_or_overlarge_payloads() {
    assert_eq!(encode_paste("\x1b", false), None);
    let over_limit = "x".repeat(TERMINAL_PASTE_MAX_BYTES + 1);
    assert_eq!(encode_paste(&over_limit, false), None);
}

#[test]
fn encodes_text_return_backspace_and_navigation() {
    assert_eq!(
        encode_key(
            &TerminalKey::Text("ls".into()),
            TerminalModifiers::default(),
            false,
        ),
        Some(b"ls".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::Return, TerminalModifiers::default(), false),
        Some(b"\r".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::Backspace, TerminalModifiers::default(), false,),
        Some(b"\x7f".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::Up, TerminalModifiers::default(), false),
        Some(b"\x1b[A".to_vec())
    );
}

#[test]
fn encodes_unmodified_navigation_for_normal_and_application_cursor_modes() {
    let modifiers = TerminalModifiers::default();
    assert_eq!(
        encode_key(&TerminalKey::Up, modifiers, false),
        Some(b"\x1b[A".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::Down, modifiers, false),
        Some(b"\x1b[B".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::Up, modifiers, true),
        Some(b"\x1bOA".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::Down, modifiers, true),
        Some(b"\x1bOB".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::Home, modifiers, false),
        Some(b"\x1b[H".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::End, modifiers, false),
        Some(b"\x1b[F".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::Home, modifiers, true),
        Some(b"\x1bOH".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::End, modifiers, true),
        Some(b"\x1bOF".to_vec())
    );
}

#[test]
fn encodes_control_and_alt_text() {
    let control = TerminalModifiers {
        control: true,
        ..TerminalModifiers::default()
    };
    assert_eq!(
        encode_key(&TerminalKey::Text("c".into()), control, false),
        Some(vec![0x03])
    );
    assert_eq!(
        encode_key(&TerminalKey::Text("[".into()), control, false),
        Some(vec![0x1b])
    );
    assert_eq!(
        encode_key(&TerminalKey::Text("\u{0002}".into()), control, false),
        Some(vec![0x02])
    );
    assert_eq!(
        encode_key(&TerminalKey::Text("\u{0003}".into()), control, false),
        Some(vec![0x03])
    );
    let control_shift = TerminalModifiers {
        control: true,
        shift: true,
        ..TerminalModifiers::default()
    };
    assert_eq!(
        encode_key(&TerminalKey::Backspace, control_shift, false),
        Some(vec![0x08])
    );

    let alt = TerminalModifiers {
        alt: true,
        ..TerminalModifiers::default()
    };
    assert_eq!(
        encode_key(&TerminalKey::Text("b".into()), alt, false),
        Some(b"\x1bb".to_vec())
    );
}

#[test]
fn drops_bare_modifier_codes_without_dropping_real_control_bytes() {
    let shift = TerminalModifiers {
        shift: true,
        ..TerminalModifiers::default()
    };
    assert_eq!(
        encode_key(&TerminalKey::Text("\u{0010}".into()), shift, false),
        None
    );
    assert_eq!(
        encode_key(
            &TerminalKey::Text("\u{0015}".into()),
            TerminalModifiers::default(),
            false,
        ),
        None
    );

    let control = TerminalModifiers {
        control: true,
        ..TerminalModifiers::default()
    };
    assert_eq!(
        encode_key(&TerminalKey::Text("\u{0010}".into()), control, false),
        Some(vec![0x10])
    );
    assert_eq!(
        encode_key(&TerminalKey::Text("\u{0011}".into()), control, false),
        Some(vec![0x11])
    );
}

#[test]
fn encodes_xterm_modified_navigation() {
    let modifiers = TerminalModifiers {
        control: true,
        shift: true,
        ..TerminalModifiers::default()
    };
    assert_eq!(
        encode_key(&TerminalKey::Left, modifiers, true),
        Some(b"\x1b[1;6D".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::Delete, modifiers, true),
        Some(b"\x1b[3;6~".to_vec())
    );
}

#[test]
fn encodes_standard_special_key_boundaries_without_extension_protocols() {
    let alt = TerminalModifiers {
        alt: true,
        ..TerminalModifiers::default()
    };
    let control = TerminalModifiers {
        control: true,
        ..TerminalModifiers::default()
    };
    assert_eq!(encode_key(&TerminalKey::Escape, alt, false), None);
    assert_eq!(encode_key(&TerminalKey::Tab, alt, false), None);
    assert_eq!(encode_key(&TerminalKey::Tab, control, false), None);
    assert_eq!(encode_key(&TerminalKey::Return, control, false), None);
    assert_eq!(
        encode_key(&TerminalKey::Return, alt, false),
        Some(b"\x1b\r".to_vec())
    );
    assert_eq!(
        encode_key(
            &TerminalKey::Text("A".to_owned()),
            TerminalModifiers {
                alt: true,
                shift: true,
                ..TerminalModifiers::default()
            },
            false,
        ),
        Some(b"\x1bA".to_vec())
    );
}

#[test]
fn encodes_function_keys_with_xterm_sequences() {
    let modifiers = TerminalModifiers::default();
    let expected = [
        (1, b"\x1bOP".as_slice()),
        (2, b"\x1bOQ".as_slice()),
        (3, b"\x1bOR".as_slice()),
        (4, b"\x1bOS".as_slice()),
        (5, b"\x1b[15~".as_slice()),
        (6, b"\x1b[17~".as_slice()),
        (7, b"\x1b[18~".as_slice()),
        (8, b"\x1b[19~".as_slice()),
        (9, b"\x1b[20~".as_slice()),
        (10, b"\x1b[21~".as_slice()),
        (11, b"\x1b[23~".as_slice()),
        (12, b"\x1b[24~".as_slice()),
        (13, b"\x1b[1;2P".as_slice()),
        (14, b"\x1b[1;2Q".as_slice()),
        (15, b"\x1b[1;2R".as_slice()),
        (16, b"\x1b[1;2S".as_slice()),
        (17, b"\x1b[15;2~".as_slice()),
        (18, b"\x1b[17;2~".as_slice()),
        (19, b"\x1b[18;2~".as_slice()),
        (20, b"\x1b[19;2~".as_slice()),
        (21, b"\x1b[20;2~".as_slice()),
        (22, b"\x1b[21;2~".as_slice()),
        (23, b"\x1b[23;2~".as_slice()),
        (24, b"\x1b[24;2~".as_slice()),
    ];
    for (number, sequence) in expected {
        assert_eq!(
            encode_key(&TerminalKey::Function(number), modifiers, false),
            Some(sequence.to_vec()),
        );
    }
    assert_eq!(
        encode_key(&TerminalKey::Function(1), modifiers, false),
        Some(b"\x1bOP".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::Function(4), modifiers, false),
        Some(b"\x1bOS".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::Function(5), modifiers, false),
        Some(b"\x1b[15~".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::Function(12), modifiers, false),
        Some(b"\x1b[24~".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::Function(13), modifiers, false),
        Some(b"\x1b[1;2P".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::Function(16), modifiers, false),
        Some(b"\x1b[1;2S".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::Function(17), modifiers, false),
        Some(b"\x1b[15;2~".to_vec())
    );
    assert_eq!(
        encode_key(&TerminalKey::Function(24), modifiers, false),
        Some(b"\x1b[24;2~".to_vec())
    );
    assert_eq!(
        encode_key(
            &TerminalKey::Function(5),
            TerminalModifiers {
                control: true,
                ..TerminalModifiers::default()
            },
            false,
        ),
        Some(b"\x1b[15;5~".to_vec())
    );
    assert_eq!(
        encode_key(
            &TerminalKey::Function(13),
            TerminalModifiers {
                control: true,
                ..TerminalModifiers::default()
            },
            false,
        ),
        Some(b"\x1b[1;5P".to_vec())
    );
    assert_eq!(
        encode_key(
            &TerminalKey::Function(24),
            TerminalModifiers {
                alt: true,
                ..TerminalModifiers::default()
            },
            false,
        ),
        Some(b"\x1b[24;3~".to_vec())
    );
}

#[test]
fn keeps_macos_line_and_word_navigation_conventional() {
    let meta = TerminalModifiers {
        meta: true,
        ..TerminalModifiers::default()
    };
    assert_eq!(
        encode_key_for_platform(&TerminalKey::Left, meta, true, TerminalPlatform::MacOs,),
        Some(vec![0x01])
    );
    let alt = TerminalModifiers {
        alt: true,
        ..TerminalModifiers::default()
    };
    assert_eq!(
        encode_key_for_platform(&TerminalKey::Right, alt, true, TerminalPlatform::MacOs,),
        Some(b"\x1bf".to_vec())
    );
    assert_eq!(
        encode_key_for_platform(&TerminalKey::Right, alt, true, TerminalPlatform::Other,),
        Some(b"\x1b[1;3C".to_vec())
    );
}

#[test]
fn leaves_unhandled_meta_shortcuts_to_the_ui() {
    let meta = TerminalModifiers {
        meta: true,
        ..TerminalModifiers::default()
    };
    assert_eq!(
        encode_key(&TerminalKey::Text("c".into()), meta, false),
        None
    );
}
