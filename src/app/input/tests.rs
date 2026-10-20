use super::*;
use slint::SharedString;

#[test]
fn maps_slint_navigation_function_and_text_keys_to_terminal_domain() {
    let up = SharedString::from(Key::UpArrow);
    assert_eq!(
        terminal_key_from_slint(up.as_str(), TerminalModifiers::default()),
        TerminalKey::Up
    );
    assert_eq!(
        terminal_key_from_slint("x", TerminalModifiers::default()),
        TerminalKey::Text("x".into())
    );
    let f1 = SharedString::from(Key::F1);
    assert_eq!(
        terminal_key_from_slint(f1.as_str(), TerminalModifiers::default()),
        TerminalKey::Function(1)
    );
    let f12 = SharedString::from(Key::F12);
    assert_eq!(
        terminal_key_from_slint(f12.as_str(), TerminalModifiers::default()),
        TerminalKey::Function(12)
    );
}

#[test]
fn normalizes_slint_text_without_inventing_physical_identity() {
    let input =
        normalized_keyboard_input_from_slint("x", TerminalModifiers::default(), false, false);
    assert_eq!(input.key, ApplicationKeyboardKey::Text("x".to_owned()));
    assert_eq!(input.physical_keycode, None);
    assert_eq!(input.location, KeyLocation::Standard);
    assert!(!input.is_physical_key_event());
}

#[test]
fn terminal_mapping_prefers_layout_resolved_event_text() {
    let input = normalized_keyboard_input_from_ui(
        "A",
        "a",
        TerminalModifiers {
            alt: true,
            shift: true,
            ..TerminalModifiers::default()
        },
        UiKeyboardInputMetadata {
            uses_native_modifiers: true,
            ..UiKeyboardInputMetadata::default()
        },
    );
    assert_eq!(
        terminal_key_from_normalized_input(&input),
        Some(TerminalKey::Text("A".to_owned()))
    );
}

#[test]
fn terminal_mapping_falls_back_to_logical_text_when_event_text_is_empty() {
    let input = normalized_keyboard_input_from_ui(
        "",
        "a",
        TerminalModifiers {
            control: true,
            ..TerminalModifiers::default()
        },
        UiKeyboardInputMetadata::default(),
    );
    assert_eq!(
        terminal_key_from_normalized_input(&input),
        Some(TerminalKey::Text("a".to_owned()))
    );
}

#[test]
fn composing_input_is_not_taken_by_plain_terminal_routing() {
    let input = normalized_keyboard_input_from_ui(
        "x",
        "x",
        TerminalModifiers::default(),
        UiKeyboardInputMetadata {
            is_composing: true,
            uses_native_modifiers: true,
            ..UiKeyboardInputMetadata::default()
        },
    );
    assert!(!terminal_key_is_direct_for_input_with_platform(
        &input, false, false, false
    ));
}

#[test]
fn maps_every_extended_function_key_to_a_terminal_function() {
    for number in 13..=24 {
        let input = normalized_keyboard_input_from_ui(
            &format!("F{number}"),
            &format!("F{number}"),
            TerminalModifiers::default(),
            UiKeyboardInputMetadata::default(),
        );
        assert_eq!(
            terminal_key_from_normalized_input(&input),
            Some(TerminalKey::Function(number))
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn maps_native_character_and_navigation_keys_to_terminal_keys() {
    let character = WinitKey::Character("b".into());
    assert_eq!(
        terminal_key_from_native_key(&character),
        Some(TerminalKey::Text("b".into()))
    );
    let up = WinitKey::Named(NamedKey::ArrowUp);
    assert_eq!(terminal_key_from_native_key(&up), Some(TerminalKey::Up));
    assert_eq!(native_shortcut_key_name(&up).as_deref(), Some("ArrowUp"));
}

#[test]
fn normalizes_unshifted_slint_hyphen_text_when_shift_is_pressed() {
    let shift = TerminalModifiers {
        shift: true,
        ..TerminalModifiers::default()
    };
    assert_eq!(
        terminal_key_from_slint("-", shift),
        TerminalKey::Text("_".into())
    );
    assert_eq!(
        terminal_key_from_slint("_", shift),
        TerminalKey::Text("_".into())
    );
}

#[test]
fn formats_modified_shortcuts_and_ignores_plain_or_modifier_keys() {
    let (slint_control, slint_meta) = if cfg!(target_os = "macos") {
        (false, true)
    } else {
        (true, false)
    };
    assert_eq!(
        format_shortcut_event("b", false, slint_control, slint_meta, true),
        "Ctrl+Shift+B"
    );
    assert_eq!(
        format_shortcut_event("\u{0003}", false, slint_control, slint_meta, true),
        "Ctrl+Shift+C"
    );
    assert_eq!(format_shortcut_event("b", false, false, false, false), "");
    let control = SharedString::from(Key::Control);
    assert_eq!(
        format_shortcut_event(control.as_str(), false, slint_control, slint_meta, false),
        ""
    );

    let (slint_command, slint_command_meta, expected) = if cfg!(target_os = "macos") {
        (true, false, "Cmd+,")
    } else {
        (false, true, "Meta+,")
    };
    assert_eq!(
        format_shortcut_event(",", false, slint_command, slint_command_meta, false),
        expected
    );
}

#[test]
fn maps_persisted_shortcuts_to_slint_menu_keys_on_each_platform() {
    let apple = menu_shortcut_from_setting_for_platform("Cmd+Shift+I", true)
        .expect("Apple shortcut should parse");
    let expected_apple = slint::Keys::from_parts(["Control", "Shift", "I"])
        .expect("expected Apple shortcut should parse");
    assert!(apple.keys == expected_apple);

    let apple_control = menu_shortcut_from_setting_for_platform("Ctrl+ArrowUp", true)
        .expect("Apple Control shortcut should parse");
    let expected_apple_control = slint::Keys::from_parts(["Meta", "UpArrow"])
        .expect("expected Apple Control shortcut should parse");
    assert!(apple_control.keys == expected_apple_control);

    let other = menu_shortcut_from_setting_for_platform("Ctrl+,", false)
        .expect("non-Apple shortcut should parse");
    let expected_other = slint::Keys::from_parts(["Control", "Comma"])
        .expect("expected non-Apple shortcut should parse");
    assert!(other.keys == expected_other);

    let apple_previous = menu_shortcut_from_setting_for_platform("Cmd+Shift+[", true)
        .expect("Apple previous-tab shortcut should parse");
    let expected_apple_previous = slint::Keys::from_parts(["Control", "Shift", "["])
        .expect("expected Apple previous-tab shortcut should parse");
    assert!(apple_previous.keys == expected_apple_previous);

    let apple_next = menu_shortcut_from_setting_for_platform("Cmd+Shift+]", true)
        .expect("Apple next-tab shortcut should parse");
    let expected_apple_next = slint::Keys::from_parts(["Control", "Shift", "]"])
        .expect("expected Apple next-tab shortcut should parse");
    assert!(apple_next.keys == expected_apple_next);

    let other_previous = menu_shortcut_from_setting_for_platform("Ctrl+Shift+[", false)
        .expect("non-Apple previous-tab shortcut should parse");
    let expected_other_previous = slint::Keys::from_parts(["Control", "Shift", "["])
        .expect("expected non-Apple previous-tab shortcut should parse");
    assert!(other_previous.keys == expected_other_previous);

    let other_next = menu_shortcut_from_setting_for_platform("Ctrl+Shift+]", false)
        .expect("non-Apple next-tab shortcut should parse");
    let expected_other_next = slint::Keys::from_parts(["Control", "Shift", "]"])
        .expect("expected non-Apple next-tab shortcut should parse");
    assert!(other_next.keys == expected_other_next);

    let apple_select_all = menu_shortcut_from_setting_for_platform("Cmd+A", true)
        .expect("Apple terminal select-all shortcut should parse");
    let expected_apple_select_all = slint::Keys::from_parts(["Control", "A"])
        .expect("expected Apple terminal select-all shortcut should parse");
    assert!(apple_select_all.keys == expected_apple_select_all);

    let other_select_all = menu_shortcut_from_setting_for_platform("Ctrl+Shift+A", false)
        .expect("non-Apple terminal select-all shortcut should parse");
    let expected_other_select_all = slint::Keys::from_parts(["Control", "Shift", "A"])
        .expect("expected non-Apple terminal select-all shortcut should parse");
    assert!(other_select_all.keys == expected_other_select_all);
}

#[test]
fn maps_menu_special_key_labels_and_rejects_invalid_values() {
    let enter = menu_shortcut_from_setting_for_platform("Alt+Enter", false)
        .expect("Enter shortcut should parse");
    let expected_enter =
        slint::Keys::from_parts(["Alt", "Return"]).expect("expected Enter shortcut should parse");
    assert!(enter.keys == expected_enter);

    assert!(menu_shortcut_from_setting_for_platform("F1", false).is_err());
    assert!(menu_shortcut_from_setting_for_platform("Ctrl+NotAKey", false).is_err());
}

#[test]
fn restores_physical_control_and_command_from_slint_apple_modifiers() {
    assert_eq!(
        normalize_slint_modifiers_for_platform(false, false, true, false, true),
        TerminalModifiers {
            control: true,
            ..TerminalModifiers::default()
        }
    );
    assert_eq!(
        normalize_slint_modifiers_for_platform(false, true, false, false, true),
        TerminalModifiers {
            meta: true,
            ..TerminalModifiers::default()
        }
    );
    assert_eq!(
        normalize_slint_modifiers_for_platform(false, true, false, false, false),
        TerminalModifiers {
            control: true,
            ..TerminalModifiers::default()
        }
    );
}

#[test]
fn current_platform_modifier_state_overrides_stale_slint_modifiers() {
    let physical_control = TerminalModifiers {
        control: true,
        ..TerminalModifiers::default()
    };
    assert_eq!(
        normalize_slint_modifiers_with_current(false, true, false, false, Some(physical_control)),
        physical_control
    );
}

#[test]
fn native_modifier_snapshot_overrides_injected_slint_modifier_state() {
    clear_native_event_modifiers();
    update_native_event_modifiers(false, true, false, false);
    assert_eq!(
        terminal_input_modifiers(false, false, false, false, true),
        TerminalModifiers {
            control: true,
            ..TerminalModifiers::default()
        }
    );
    assert!(terminal_key_is_direct(
        "b", false, false, false, false, false, false
    ));
    clear_native_event_modifiers();
}

#[test]
fn committed_terminal_text_does_not_inherit_the_triggering_shortcut_modifier() {
    assert_eq!(
        terminal_input_modifiers(false, false, false, false, false),
        TerminalModifiers::default()
    );
}

#[test]
fn routes_terminal_input_from_one_normalized_modifier_source() {
    let control = TerminalModifiers {
        control: true,
        ..TerminalModifiers::default()
    };
    let command = TerminalModifiers {
        meta: true,
        ..TerminalModifiers::default()
    };
    let option = TerminalModifiers {
        alt: true,
        ..TerminalModifiers::default()
    };
    let control_alt = TerminalModifiers {
        control: true,
        alt: true,
        ..TerminalModifiers::default()
    };

    assert!(!terminal_key_is_direct_for_platform(
        "c",
        TerminalModifiers::default(),
        false,
        false,
        true,
    ));
    assert!(terminal_key_is_direct_for_platform(
        "c", control, false, false, true
    ));
    assert!(terminal_key_is_direct_for_platform(
        "c", command, false, false, true
    ));
    assert!(!terminal_key_is_direct_for_platform(
        "c", option, false, false, true
    ));
    assert!(terminal_key_is_direct_for_platform(
        "c", option, true, false, true,
    ));
    assert!(!terminal_key_is_direct_for_platform(
        "@",
        control_alt,
        false,
        false,
        false
    ));
    assert!(terminal_key_is_direct_for_platform(
        "c", control, false, false, false
    ));

    let f1 = SharedString::from(Key::F1);
    assert!(terminal_key_is_direct_for_platform(
        f1.as_str(),
        TerminalModifiers::default(),
        false,
        false,
        true,
    ));
    assert!(!terminal_key_is_direct_for_platform(
        f1.as_str(),
        TerminalModifiers::default(),
        false,
        true,
        true,
    ));
}

#[test]
fn never_routes_standalone_modifier_keys_to_the_terminal() {
    let modifiers = TerminalModifiers {
        alt: true,
        control: true,
        meta: true,
        shift: true,
    };
    let modifier_keys = [
        Key::Shift,
        Key::ShiftR,
        Key::Control,
        Key::ControlR,
        Key::Alt,
        Key::AltGr,
        Key::CapsLock,
        Key::Meta,
        Key::MetaR,
    ];

    for modifier_key in modifier_keys {
        let text = SharedString::from(modifier_key);
        let input = normalized_keyboard_input_from_slint(text.as_str(), modifiers, true, false);
        assert!(is_application_modifier_key(&input.key));
        assert!(!terminal_key_is_direct_for_platform(
            text.as_str(),
            modifiers,
            true,
            false,
            true,
        ));
        assert!(!terminal_key_is_direct_for_platform(
            text.as_str(),
            modifiers,
            true,
            false,
            false,
        ));
    }
}
