use ax_ssh::terminal::{TerminalKey, TerminalModifiers};
use slint::platform::Key;

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

pub(super) fn format_shortcut_event(
    text: &str,
    alt: bool,
    control: bool,
    meta: bool,
    shift: bool,
) -> String {
    let modifiers = normalize_slint_modifiers(alt, control, meta, shift);
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
    fn maps_slint_navigation_and_text_keys_to_terminal_domain() {
        let up = SharedString::from(Key::UpArrow);
        assert_eq!(
            terminal_key_from_slint(up.as_str(), TerminalModifiers::default()),
            TerminalKey::Up
        );
        assert_eq!(
            terminal_key_from_slint("x", TerminalModifiers::default()),
            TerminalKey::Text("x".into())
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
}
