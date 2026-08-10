use ax_ssh::terminal::{TerminalKey, TerminalModifiers};
use slint::platform::Key;

pub(super) struct MenuShortcut {
    pub(super) keys: slint::Keys,
    #[cfg(target_os = "macos")]
    pub(super) native: NativeMenuShortcut,
}

#[cfg(target_os = "macos")]
pub(super) struct NativeMenuShortcut {
    pub(super) key: String,
    pub(super) modifiers: TerminalModifiers,
}

pub(super) fn menu_shortcut_from_setting(shortcut: &str) -> anyhow::Result<MenuShortcut> {
    menu_shortcut_from_setting_for_platform(shortcut, cfg!(target_os = "macos"))
}

fn menu_shortcut_from_setting_for_platform(
    shortcut: &str,
    apple_platform: bool,
) -> anyhow::Result<MenuShortcut> {
    let shortcut = shortcut.trim();
    let Some((modifiers, key)) = shortcut.rsplit_once('+') else {
        anyhow::bail!("shortcut must include a modifier");
    };
    if key.is_empty() {
        anyhow::bail!("shortcut key is empty");
    }

    let mut parts = Vec::with_capacity(5);
    #[cfg(target_os = "macos")]
    let mut native_modifiers = TerminalModifiers::default();
    for modifier in modifiers.split('+') {
        match modifier {
            "Cmd" | "Meta" if apple_platform => {
                parts.push("Control".to_owned());
                #[cfg(target_os = "macos")]
                {
                    native_modifiers.meta = true;
                }
            }
            "Ctrl" if apple_platform => {
                parts.push("Meta".to_owned());
                #[cfg(target_os = "macos")]
                {
                    native_modifiers.control = true;
                }
            }
            "Cmd" | "Meta" => {
                parts.push("Meta".to_owned());
                #[cfg(target_os = "macos")]
                {
                    native_modifiers.meta = true;
                }
            }
            "Ctrl" => {
                parts.push("Control".to_owned());
                #[cfg(target_os = "macos")]
                {
                    native_modifiers.control = true;
                }
            }
            "Alt" => {
                parts.push("Alt".to_owned());
                #[cfg(target_os = "macos")]
                {
                    native_modifiers.alt = true;
                }
            }
            "Shift" => {
                parts.push("Shift".to_owned());
                #[cfg(target_os = "macos")]
                {
                    native_modifiers.shift = true;
                }
            }
            _ => anyhow::bail!("shortcut contains an unknown modifier"),
        }
    }
    parts.push(slint_menu_key_name(key));
    let keys = slint::Keys::from_parts(parts.iter().map(String::as_str))
        .map_err(|error| anyhow::anyhow!("shortcut cannot be used by the native menu: {error}"))?;

    Ok(MenuShortcut {
        keys,
        #[cfg(target_os = "macos")]
        native: NativeMenuShortcut {
            key: key.to_owned(),
            modifiers: native_modifiers,
        },
    })
}

fn slint_menu_key_name(key: &str) -> String {
    match key {
        "Enter" => "Return".to_owned(),
        "ArrowUp" => "UpArrow".to_owned(),
        "ArrowDown" => "DownArrow".to_owned(),
        "ArrowLeft" => "LeftArrow".to_owned(),
        "ArrowRight" => "RightArrow".to_owned(),
        "," => "Comma".to_owned(),
        character
            if character.chars().count() == 1
                && !character
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_alphabetic()) =>
        {
            character.to_lowercase()
        }
        named => named.to_owned(),
    }
}

pub(super) fn terminal_key_from_slint(text: &str, modifiers: TerminalModifiers) -> TerminalKey {
    let special = [
        (Key::Return, TerminalKey::Return),
        (Key::Backspace, TerminalKey::Backspace),
        (Key::Tab, TerminalKey::Tab),
        (Key::Backtab, TerminalKey::Tab),
        (Key::Escape, TerminalKey::Escape),
        (Key::UpArrow, TerminalKey::Up),
        (Key::DownArrow, TerminalKey::Down),
        (Key::RightArrow, TerminalKey::Right),
        (Key::LeftArrow, TerminalKey::Left),
        (Key::Insert, TerminalKey::Insert),
        (Key::Delete, TerminalKey::Delete),
        (Key::Home, TerminalKey::Home),
        (Key::End, TerminalKey::End),
        (Key::PageUp, TerminalKey::PageUp),
        (Key::PageDown, TerminalKey::PageDown),
        (Key::F1, TerminalKey::Function(1)),
        (Key::F2, TerminalKey::Function(2)),
        (Key::F3, TerminalKey::Function(3)),
        (Key::F4, TerminalKey::Function(4)),
        (Key::F5, TerminalKey::Function(5)),
        (Key::F6, TerminalKey::Function(6)),
        (Key::F7, TerminalKey::Function(7)),
        (Key::F8, TerminalKey::Function(8)),
        (Key::F9, TerminalKey::Function(9)),
        (Key::F10, TerminalKey::Function(10)),
        (Key::F11, TerminalKey::Function(11)),
        (Key::F12, TerminalKey::Function(12)),
    ];
    special
        .into_iter()
        .find_map(|(slint_key, terminal_key)| {
            matches_slint_key(text, slint_key).then_some(terminal_key)
        })
        .unwrap_or_else(|| {
            let text = if text == "-"
                && modifiers.shift
                && !modifiers.alt
                && !modifiers.control
                && !modifiers.meta
            {
                "_"
            } else {
                text
            };
            TerminalKey::Text(text.to_owned())
        })
}

#[cfg(test)]
fn format_shortcut_event(text: &str, alt: bool, control: bool, meta: bool, shift: bool) -> String {
    format_shortcut_event_with_modifiers(text, normalize_slint_modifiers(alt, control, meta, shift))
}

pub(super) fn format_shortcut_event_with_current_modifiers(
    text: &str,
    alt: bool,
    control: bool,
    meta: bool,
    shift: bool,
) -> String {
    format_shortcut_event_with_modifiers(text, normalize_event_modifiers(alt, control, meta, shift))
}

fn format_shortcut_event_with_modifiers(text: &str, modifiers: TerminalModifiers) -> String {
    if !modifiers.alt && !modifiers.control && !modifiers.meta && !modifiers.shift {
        return String::new();
    }
    let Some(key) = shortcut_key_name(text, modifiers.control) else {
        return String::new();
    };
    let mut parts = Vec::with_capacity(5);
    if modifiers.meta {
        parts.push(if cfg!(target_os = "macos") {
            "Cmd".to_owned()
        } else {
            "Meta".to_owned()
        });
    }
    if modifiers.control {
        parts.push("Ctrl".to_owned());
    }
    if modifiers.alt {
        parts.push("Alt".to_owned());
    }
    if modifiers.shift {
        parts.push("Shift".to_owned());
    }
    parts.push(key);
    parts.join("+")
}

pub(super) fn normalize_slint_modifiers(
    alt: bool,
    control: bool,
    meta: bool,
    shift: bool,
) -> TerminalModifiers {
    normalize_slint_modifiers_for_platform(alt, control, meta, shift, cfg!(target_os = "macos"))
}

pub(super) fn normalize_event_modifiers(
    alt: bool,
    control: bool,
    meta: bool,
    shift: bool,
) -> TerminalModifiers {
    normalize_slint_modifiers_with_current(alt, control, meta, shift, current_platform_modifiers())
}

pub(super) fn terminal_input_modifiers(
    alt: bool,
    control: bool,
    meta: bool,
    shift: bool,
    physical_key_event: bool,
) -> TerminalModifiers {
    if physical_key_event {
        normalize_event_modifiers(alt, control, meta, shift)
    } else {
        normalize_slint_modifiers(alt, control, meta, shift)
    }
}

fn normalize_slint_modifiers_with_current(
    alt: bool,
    control: bool,
    meta: bool,
    shift: bool,
    current: Option<TerminalModifiers>,
) -> TerminalModifiers {
    current.unwrap_or_else(|| normalize_slint_modifiers(alt, control, meta, shift))
}

#[cfg(target_os = "macos")]
fn current_platform_modifiers() -> Option<TerminalModifiers> {
    Some(super::macos_window::current_modifier_state())
}

#[cfg(not(target_os = "macos"))]
fn current_platform_modifiers() -> Option<TerminalModifiers> {
    None
}

pub(super) fn terminal_key_is_direct(
    text: &str,
    alt: bool,
    control: bool,
    meta: bool,
    shift: bool,
    option_as_meta: bool,
    preedit_active: bool,
) -> bool {
    let modifiers = normalize_event_modifiers(alt, control, meta, shift);
    terminal_key_is_direct_for_platform(
        text,
        modifiers,
        option_as_meta,
        preedit_active,
        cfg!(target_os = "macos"),
    )
}

fn terminal_key_is_direct_for_platform(
    text: &str,
    modifiers: TerminalModifiers,
    option_as_meta: bool,
    preedit_active: bool,
    apple_platform: bool,
) -> bool {
    // Slint represents modifier keys as C0 code points (for example, Control
    // is U+0011). A modifier press carries its own modifier state, which would
    // otherwise make it look like a terminal control chord such as Ctrl+Q.
    // Only a following non-modifier key may produce terminal input.
    if is_slint_modifier_key(text) {
        return false;
    }

    let direct_modifier = if apple_platform {
        modifiers.control || modifiers.meta || (option_as_meta && modifiers.alt && !modifiers.meta)
    } else {
        modifiers.control || modifiers.alt || modifiers.meta
    };
    if preedit_active && !direct_modifier {
        return false;
    }

    let key = terminal_key_from_slint(text, modifiers);
    if !matches!(key, TerminalKey::Text(_)) {
        return true;
    }

    if apple_platform {
        return direct_modifier;
    }

    // Ctrl+Alt printable text is commonly AltGr and must stay on TextInput.
    (modifiers.control && !(modifiers.alt && !text.is_empty()))
        || (modifiers.alt && !modifiers.control)
        || modifiers.meta
}

fn is_slint_modifier_key(text: &str) -> bool {
    [
        Key::Shift,
        Key::ShiftR,
        Key::Control,
        Key::ControlR,
        Key::Alt,
        Key::AltGr,
        Key::CapsLock,
        Key::Meta,
        Key::MetaR,
    ]
    .into_iter()
    .any(|key| matches_slint_key(text, key))
}

fn normalize_slint_modifiers_for_platform(
    alt: bool,
    control: bool,
    meta: bool,
    shift: bool,
    apple_platform: bool,
) -> TerminalModifiers {
    TerminalModifiers {
        alt,
        control: if apple_platform { meta } else { control },
        meta: if apple_platform { control } else { meta },
        shift,
    }
}

fn matches_slint_key(text: &str, key: Key) -> bool {
    let mut characters = text.chars();
    characters.next() == Some(char::from(key)) && characters.next().is_none()
}

fn shortcut_key_name(text: &str, control: bool) -> Option<String> {
    let modifier_keys = [
        Key::Alt,
        Key::AltGr,
        Key::Control,
        Key::ControlR,
        Key::Meta,
        Key::MetaR,
        Key::Shift,
        Key::ShiftR,
    ];
    if modifier_keys
        .into_iter()
        .any(|key| matches_slint_key(text, key))
    {
        return None;
    }
    let special_keys = [
        (Key::Backspace, "Backspace"),
        (Key::Tab, "Tab"),
        (Key::Backtab, "Backtab"),
        (Key::Return, "Enter"),
        (Key::Escape, "Escape"),
        (Key::Delete, "Delete"),
        (Key::Space, "Space"),
        (Key::UpArrow, "ArrowUp"),
        (Key::DownArrow, "ArrowDown"),
        (Key::LeftArrow, "ArrowLeft"),
        (Key::RightArrow, "ArrowRight"),
        (Key::Insert, "Insert"),
        (Key::Home, "Home"),
        (Key::End, "End"),
        (Key::PageUp, "PageUp"),
        (Key::PageDown, "PageDown"),
        (Key::F1, "F1"),
        (Key::F2, "F2"),
        (Key::F3, "F3"),
        (Key::F4, "F4"),
        (Key::F5, "F5"),
        (Key::F6, "F6"),
        (Key::F7, "F7"),
        (Key::F8, "F8"),
        (Key::F9, "F9"),
        (Key::F10, "F10"),
        (Key::F11, "F11"),
        (Key::F12, "F12"),
    ];
    if let Some((_, label)) = special_keys
        .into_iter()
        .find(|(key, _)| matches_slint_key(text, *key))
    {
        return Some(label.to_owned());
    }

    let mut characters = text.chars();
    let character = characters.next()?;
    if characters.next().is_some() {
        return None;
    }
    if control && ('\u{0001}'..='\u{000f}').contains(&character) {
        return Some(((character as u8 + b'A' - 1) as char).to_string());
    }
    if character.is_control() {
        return None;
    }
    Some(match character {
        '+' => "Plus".to_owned(),
        character if character.is_ascii_alphabetic() => character.to_ascii_uppercase().to_string(),
        character => character.to_string(),
    })
}

#[cfg(test)]
mod tests {
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
        let expected_enter = slint::Keys::from_parts(["Alt", "Return"])
            .expect("expected Enter shortcut should parse");
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
            normalize_slint_modifiers_with_current(
                false,
                true,
                false,
                false,
                Some(physical_control)
            ),
            physical_control
        );
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
            assert!(is_slint_modifier_key(text.as_str()));
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
}
