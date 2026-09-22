use std::cell::Cell;

use ax_ssh::terminal::{TerminalKey, TerminalModifiers};
use slint::platform::Key;
use slint::winit_030::winit::keyboard::{KeyCode, KeyLocation};
#[cfg(target_os = "macos")]
use slint::winit_030::winit::{
    event::KeyEvent as WinitKeyEvent,
    keyboard::{Key as WinitKey, NamedKey, PhysicalKey},
};

thread_local! {
    /// Modifier state captured from the native window event immediately before
    /// Slint dispatches a key event. This preserves the event-scoped state of
    /// synthetic macOS key events without changing the TextInput/IME path.
    static NATIVE_EVENT_MODIFIERS: Cell<Option<TerminalModifiers>> = const { Cell::new(None) };
}

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

/// Layout-independent key identity used by every application input consumer.
/// Terminal encoding is deliberately deferred until the event crosses into
/// `src/terminal/input.rs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ApplicationKeyboardKey {
    Text(String),
    Named(ApplicationKeyboardNamedKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ApplicationKeyboardNamedKey {
    Return,
    Backspace,
    Tab,
    Escape,
    Up,
    Down,
    Right,
    Left,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Function(u8),
    Space,
    Shift,
    Control,
    Alt,
    AltGraph,
    CapsLock,
    Meta,
}

/// The application-wide keyboard boundary shared by Slint and native Winit
/// events. Logical text is layout/IME-aware; physical code and location remain
/// available as event metadata without changing the standard text path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NormalizedKeyboardInput {
    pub(super) text: String,
    pub(super) key: ApplicationKeyboardKey,
    pub(super) modifiers: TerminalModifiers,
    pub(super) physical_keycode: Option<KeyCode>,
    pub(super) location: KeyLocation,
    pub(super) is_composing: bool,
    pub(super) is_repeat: bool,
    pub(super) is_synthetic: bool,
    pub(super) is_paste: bool,
    pub(super) uses_native_modifiers: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct UiKeyboardInputMetadata {
    pub(super) is_composing: bool,
    pub(super) is_repeat: bool,
    pub(super) is_synthetic: bool,
    pub(super) is_paste: bool,
    pub(super) uses_native_modifiers: bool,
}

impl NormalizedKeyboardInput {
    pub(super) fn is_physical_key_event(&self) -> bool {
        self.uses_native_modifiers
    }
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

#[cfg(test)]
pub(super) fn terminal_key_from_slint(text: &str, modifiers: TerminalModifiers) -> TerminalKey {
    if let Some(number) = extended_function_key_number(text) {
        return TerminalKey::Function(number);
    }
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

fn application_key_from_slint(text: &str) -> ApplicationKeyboardKey {
    if let Some(number) = extended_function_key_number(text) {
        return ApplicationKeyboardKey::Named(ApplicationKeyboardNamedKey::Function(number));
    }
    let named_keys = [
        (Key::Return, ApplicationKeyboardNamedKey::Return),
        (Key::Backspace, ApplicationKeyboardNamedKey::Backspace),
        (Key::Tab, ApplicationKeyboardNamedKey::Tab),
        (Key::Backtab, ApplicationKeyboardNamedKey::Tab),
        (Key::Escape, ApplicationKeyboardNamedKey::Escape),
        (Key::UpArrow, ApplicationKeyboardNamedKey::Up),
        (Key::DownArrow, ApplicationKeyboardNamedKey::Down),
        (Key::RightArrow, ApplicationKeyboardNamedKey::Right),
        (Key::LeftArrow, ApplicationKeyboardNamedKey::Left),
        (Key::Insert, ApplicationKeyboardNamedKey::Insert),
        (Key::Delete, ApplicationKeyboardNamedKey::Delete),
        (Key::Home, ApplicationKeyboardNamedKey::Home),
        (Key::End, ApplicationKeyboardNamedKey::End),
        (Key::PageUp, ApplicationKeyboardNamedKey::PageUp),
        (Key::PageDown, ApplicationKeyboardNamedKey::PageDown),
        (Key::F1, ApplicationKeyboardNamedKey::Function(1)),
        (Key::F2, ApplicationKeyboardNamedKey::Function(2)),
        (Key::F3, ApplicationKeyboardNamedKey::Function(3)),
        (Key::F4, ApplicationKeyboardNamedKey::Function(4)),
        (Key::F5, ApplicationKeyboardNamedKey::Function(5)),
        (Key::F6, ApplicationKeyboardNamedKey::Function(6)),
        (Key::F7, ApplicationKeyboardNamedKey::Function(7)),
        (Key::F8, ApplicationKeyboardNamedKey::Function(8)),
        (Key::F9, ApplicationKeyboardNamedKey::Function(9)),
        (Key::F10, ApplicationKeyboardNamedKey::Function(10)),
        (Key::F11, ApplicationKeyboardNamedKey::Function(11)),
        (Key::F12, ApplicationKeyboardNamedKey::Function(12)),
        (Key::Space, ApplicationKeyboardNamedKey::Space),
        (Key::Shift, ApplicationKeyboardNamedKey::Shift),
        (Key::ShiftR, ApplicationKeyboardNamedKey::Shift),
        (Key::Control, ApplicationKeyboardNamedKey::Control),
        (Key::ControlR, ApplicationKeyboardNamedKey::Control),
        (Key::Alt, ApplicationKeyboardNamedKey::Alt),
        (Key::AltGr, ApplicationKeyboardNamedKey::AltGraph),
        (Key::CapsLock, ApplicationKeyboardNamedKey::CapsLock),
        (Key::Meta, ApplicationKeyboardNamedKey::Meta),
        (Key::MetaR, ApplicationKeyboardNamedKey::Meta),
    ];
    named_keys
        .into_iter()
        .find_map(|(slint_key, key)| matches_slint_key(text, slint_key).then_some(key))
        .map(ApplicationKeyboardKey::Named)
        .unwrap_or_else(|| ApplicationKeyboardKey::Text(text.to_owned()))
}

#[cfg(target_os = "macos")]
fn application_key_from_native_key(key: &WinitKey) -> Option<ApplicationKeyboardKey> {
    let named = match key {
        WinitKey::Character(text) => return Some(ApplicationKeyboardKey::Text(text.to_string())),
        WinitKey::Named(key) => key,
        WinitKey::Dead(_) | WinitKey::Unidentified(_) => return None,
    };
    let key = match named {
        NamedKey::Enter => ApplicationKeyboardNamedKey::Return,
        NamedKey::Backspace => ApplicationKeyboardNamedKey::Backspace,
        NamedKey::Tab => ApplicationKeyboardNamedKey::Tab,
        NamedKey::Escape => ApplicationKeyboardNamedKey::Escape,
        NamedKey::ArrowUp => ApplicationKeyboardNamedKey::Up,
        NamedKey::ArrowDown => ApplicationKeyboardNamedKey::Down,
        NamedKey::ArrowLeft => ApplicationKeyboardNamedKey::Left,
        NamedKey::ArrowRight => ApplicationKeyboardNamedKey::Right,
        NamedKey::Insert => ApplicationKeyboardNamedKey::Insert,
        NamedKey::Delete => ApplicationKeyboardNamedKey::Delete,
        NamedKey::Home => ApplicationKeyboardNamedKey::Home,
        NamedKey::End => ApplicationKeyboardNamedKey::End,
        NamedKey::PageUp => ApplicationKeyboardNamedKey::PageUp,
        NamedKey::PageDown => ApplicationKeyboardNamedKey::PageDown,
        NamedKey::F1 => ApplicationKeyboardNamedKey::Function(1),
        NamedKey::F2 => ApplicationKeyboardNamedKey::Function(2),
        NamedKey::F3 => ApplicationKeyboardNamedKey::Function(3),
        NamedKey::F4 => ApplicationKeyboardNamedKey::Function(4),
        NamedKey::F5 => ApplicationKeyboardNamedKey::Function(5),
        NamedKey::F6 => ApplicationKeyboardNamedKey::Function(6),
        NamedKey::F7 => ApplicationKeyboardNamedKey::Function(7),
        NamedKey::F8 => ApplicationKeyboardNamedKey::Function(8),
        NamedKey::F9 => ApplicationKeyboardNamedKey::Function(9),
        NamedKey::F10 => ApplicationKeyboardNamedKey::Function(10),
        NamedKey::F11 => ApplicationKeyboardNamedKey::Function(11),
        NamedKey::F12 => ApplicationKeyboardNamedKey::Function(12),
        NamedKey::F13 => ApplicationKeyboardNamedKey::Function(13),
        NamedKey::F14 => ApplicationKeyboardNamedKey::Function(14),
        NamedKey::F15 => ApplicationKeyboardNamedKey::Function(15),
        NamedKey::F16 => ApplicationKeyboardNamedKey::Function(16),
        NamedKey::F17 => ApplicationKeyboardNamedKey::Function(17),
        NamedKey::F18 => ApplicationKeyboardNamedKey::Function(18),
        NamedKey::F19 => ApplicationKeyboardNamedKey::Function(19),
        NamedKey::F20 => ApplicationKeyboardNamedKey::Function(20),
        NamedKey::F21 => ApplicationKeyboardNamedKey::Function(21),
        NamedKey::F22 => ApplicationKeyboardNamedKey::Function(22),
        NamedKey::F23 => ApplicationKeyboardNamedKey::Function(23),
        NamedKey::F24 => ApplicationKeyboardNamedKey::Function(24),
        NamedKey::Space => ApplicationKeyboardNamedKey::Space,
        NamedKey::Shift => ApplicationKeyboardNamedKey::Shift,
        NamedKey::Control => ApplicationKeyboardNamedKey::Control,
        NamedKey::Alt => ApplicationKeyboardNamedKey::Alt,
        NamedKey::AltGraph => ApplicationKeyboardNamedKey::AltGraph,
        NamedKey::CapsLock => ApplicationKeyboardNamedKey::CapsLock,
        NamedKey::Meta => ApplicationKeyboardNamedKey::Meta,
        _ => return None,
    };
    Some(ApplicationKeyboardKey::Named(key))
}

pub(super) fn terminal_key_from_normalized_input(
    input: &NormalizedKeyboardInput,
) -> Option<TerminalKey> {
    match &input.key {
        ApplicationKeyboardKey::Text(logical_text) => {
            // The logical key identifies the key class, but native events may
            // carry the layout-resolved character in `text` (for example
            // Shift+A or an Alt-modified punctuation key). Prefer that event
            // text and only fall back to the logical character when a backend
            // did not provide one.
            let event_text = if input.text.is_empty() {
                logical_text.as_str()
            } else {
                input.text.as_str()
            };
            let text = if event_text == "-"
                && input.modifiers.shift
                && !input.modifiers.alt
                && !input.modifiers.control
                && !input.modifiers.meta
            {
                "_"
            } else {
                event_text
            };
            Some(TerminalKey::Text(text.to_owned()))
        }
        ApplicationKeyboardKey::Named(key) => Some(match key {
            ApplicationKeyboardNamedKey::Return => TerminalKey::Return,
            ApplicationKeyboardNamedKey::Backspace => TerminalKey::Backspace,
            ApplicationKeyboardNamedKey::Tab => TerminalKey::Tab,
            ApplicationKeyboardNamedKey::Escape => TerminalKey::Escape,
            ApplicationKeyboardNamedKey::Up => TerminalKey::Up,
            ApplicationKeyboardNamedKey::Down => TerminalKey::Down,
            ApplicationKeyboardNamedKey::Right => TerminalKey::Right,
            ApplicationKeyboardNamedKey::Left => TerminalKey::Left,
            ApplicationKeyboardNamedKey::Insert => TerminalKey::Insert,
            ApplicationKeyboardNamedKey::Delete => TerminalKey::Delete,
            ApplicationKeyboardNamedKey::Home => TerminalKey::Home,
            ApplicationKeyboardNamedKey::End => TerminalKey::End,
            ApplicationKeyboardNamedKey::PageUp => TerminalKey::PageUp,
            ApplicationKeyboardNamedKey::PageDown => TerminalKey::PageDown,
            ApplicationKeyboardNamedKey::Function(number) => TerminalKey::Function(*number),
            ApplicationKeyboardNamedKey::Space => TerminalKey::Text(" ".to_owned()),
            ApplicationKeyboardNamedKey::Shift
            | ApplicationKeyboardNamedKey::Control
            | ApplicationKeyboardNamedKey::Alt
            | ApplicationKeyboardNamedKey::AltGraph
            | ApplicationKeyboardNamedKey::CapsLock
            | ApplicationKeyboardNamedKey::Meta => return None,
        }),
    }
}

#[cfg(test)]
pub(super) fn normalized_keyboard_input_from_slint(
    text: &str,
    modifiers: TerminalModifiers,
    physical_key_event: bool,
    is_composing: bool,
) -> NormalizedKeyboardInput {
    normalized_keyboard_input_from_ui(
        text,
        text,
        modifiers,
        UiKeyboardInputMetadata {
            is_composing,
            uses_native_modifiers: physical_key_event,
            ..UiKeyboardInputMetadata::default()
        },
    )
}

pub(super) fn normalized_keyboard_input_from_ui(
    text: &str,
    logical_key: &str,
    modifiers: TerminalModifiers,
    metadata: UiKeyboardInputMetadata,
) -> NormalizedKeyboardInput {
    NormalizedKeyboardInput {
        text: text.to_owned(),
        key: application_key_from_slint(if logical_key.is_empty() {
            text
        } else {
            logical_key
        }),
        modifiers,
        physical_keycode: None,
        location: KeyLocation::Standard,
        is_composing: metadata.is_composing,
        is_repeat: metadata.is_repeat,
        is_synthetic: metadata.is_synthetic,
        is_paste: metadata.is_paste,
        uses_native_modifiers: metadata.uses_native_modifiers,
    }
}

#[cfg(target_os = "macos")]
pub(super) fn normalized_keyboard_input_from_winit(
    event: &WinitKeyEvent,
    modifiers: TerminalModifiers,
    is_synthetic: bool,
) -> Option<NormalizedKeyboardInput> {
    let physical_keycode = match event.physical_key {
        PhysicalKey::Code(keycode) => Some(keycode),
        PhysicalKey::Unidentified(_) => None,
    };
    let key = application_key_from_native_key(&event.logical_key)?;
    let text = event
        .text
        .as_ref()
        .map(ToString::to_string)
        .or_else(|| event.logical_key.to_text().map(str::to_owned))
        .unwrap_or_default();
    Some(NormalizedKeyboardInput {
        text,
        key,
        modifiers,
        physical_keycode,
        location: event.location,
        is_composing: false,
        is_repeat: event.repeat,
        is_synthetic,
        is_paste: false,
        uses_native_modifiers: true,
    })
}

#[cfg(all(test, target_os = "macos"))]
pub(super) fn terminal_key_from_native_key(key: &WinitKey) -> Option<TerminalKey> {
    let key = match key {
        WinitKey::Character(text) => return Some(TerminalKey::Text(text.to_string())),
        WinitKey::Named(key) => key,
        WinitKey::Dead(_) | WinitKey::Unidentified(_) => return None,
    };
    let terminal_key = match key {
        NamedKey::Enter => TerminalKey::Return,
        NamedKey::Backspace => TerminalKey::Backspace,
        NamedKey::Tab => TerminalKey::Tab,
        NamedKey::Escape => TerminalKey::Escape,
        NamedKey::ArrowUp => TerminalKey::Up,
        NamedKey::ArrowDown => TerminalKey::Down,
        NamedKey::ArrowLeft => TerminalKey::Left,
        NamedKey::ArrowRight => TerminalKey::Right,
        NamedKey::Insert => TerminalKey::Insert,
        NamedKey::Delete => TerminalKey::Delete,
        NamedKey::Home => TerminalKey::Home,
        NamedKey::End => TerminalKey::End,
        NamedKey::PageUp => TerminalKey::PageUp,
        NamedKey::PageDown => TerminalKey::PageDown,
        NamedKey::F1 => TerminalKey::Function(1),
        NamedKey::F2 => TerminalKey::Function(2),
        NamedKey::F3 => TerminalKey::Function(3),
        NamedKey::F4 => TerminalKey::Function(4),
        NamedKey::F5 => TerminalKey::Function(5),
        NamedKey::F6 => TerminalKey::Function(6),
        NamedKey::F7 => TerminalKey::Function(7),
        NamedKey::F8 => TerminalKey::Function(8),
        NamedKey::F9 => TerminalKey::Function(9),
        NamedKey::F10 => TerminalKey::Function(10),
        NamedKey::F11 => TerminalKey::Function(11),
        NamedKey::F12 => TerminalKey::Function(12),
        NamedKey::F13 => TerminalKey::Function(13),
        NamedKey::F14 => TerminalKey::Function(14),
        NamedKey::F15 => TerminalKey::Function(15),
        NamedKey::F16 => TerminalKey::Function(16),
        NamedKey::F17 => TerminalKey::Function(17),
        NamedKey::F18 => TerminalKey::Function(18),
        NamedKey::F19 => TerminalKey::Function(19),
        NamedKey::F20 => TerminalKey::Function(20),
        NamedKey::F21 => TerminalKey::Function(21),
        NamedKey::F22 => TerminalKey::Function(22),
        NamedKey::F23 => TerminalKey::Function(23),
        NamedKey::F24 => TerminalKey::Function(24),
        NamedKey::Space => TerminalKey::Text(" ".to_owned()),
        _ => return None,
    };
    Some(terminal_key)
}

#[cfg(target_os = "macos")]
pub(super) fn native_shortcut_key_name(key: &WinitKey) -> Option<String> {
    let name = match key {
        WinitKey::Character(text) => {
            let mut characters = text.chars();
            let character = characters.next()?;
            if characters.next().is_some() || character.is_control() {
                return None;
            }
            return Some(match character {
                '+' => "Plus".to_owned(),
                character if character.is_ascii_alphabetic() => {
                    character.to_ascii_uppercase().to_string()
                }
                character => character.to_string(),
            });
        }
        WinitKey::Named(key) => match key {
            NamedKey::Backspace => "Backspace",
            NamedKey::Tab => "Tab",
            NamedKey::Enter => "Enter",
            NamedKey::Escape => "Escape",
            NamedKey::Delete => "Delete",
            NamedKey::Space => "Space",
            NamedKey::ArrowUp => "ArrowUp",
            NamedKey::ArrowDown => "ArrowDown",
            NamedKey::ArrowLeft => "ArrowLeft",
            NamedKey::ArrowRight => "ArrowRight",
            NamedKey::Insert => "Insert",
            NamedKey::Home => "Home",
            NamedKey::End => "End",
            NamedKey::PageUp => "PageUp",
            NamedKey::PageDown => "PageDown",
            NamedKey::F1 => "F1",
            NamedKey::F2 => "F2",
            NamedKey::F3 => "F3",
            NamedKey::F4 => "F4",
            NamedKey::F5 => "F5",
            NamedKey::F6 => "F6",
            NamedKey::F7 => "F7",
            NamedKey::F8 => "F8",
            NamedKey::F9 => "F9",
            NamedKey::F10 => "F10",
            NamedKey::F11 => "F11",
            NamedKey::F12 => "F12",
            NamedKey::F13 => "F13",
            NamedKey::F14 => "F14",
            NamedKey::F15 => "F15",
            NamedKey::F16 => "F16",
            NamedKey::F17 => "F17",
            NamedKey::F18 => "F18",
            NamedKey::F19 => "F19",
            NamedKey::F20 => "F20",
            NamedKey::F21 => "F21",
            NamedKey::F22 => "F22",
            NamedKey::F23 => "F23",
            NamedKey::F24 => "F24",
            _ => return None,
        },
        WinitKey::Dead(_) | WinitKey::Unidentified(_) => return None,
    };
    Some(name.to_owned())
}

#[cfg(target_os = "macos")]
pub(super) fn native_shortcut_matches_setting(
    shortcut: &str,
    key: &str,
    modifiers: TerminalModifiers,
) -> bool {
    menu_shortcut_from_setting(shortcut).is_ok_and(|parsed| {
        parsed.native.key.eq_ignore_ascii_case(key) && parsed.native.modifiers == modifiers
    })
}

#[cfg(test)]
fn format_shortcut_event(text: &str, alt: bool, control: bool, meta: bool, shift: bool) -> String {
    format_shortcut_event_with_modifiers(text, normalize_slint_modifiers(alt, control, meta, shift))
}

pub(super) fn format_shortcut_event_with_current_modifiers(
    input: &NormalizedKeyboardInput,
) -> String {
    format_shortcut_event_with_modifiers(&input.text, input.modifiers)
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
    normalize_slint_modifiers_with_current(
        alt,
        control,
        meta,
        shift,
        native_event_modifiers().or_else(current_platform_modifiers),
    )
}

pub(super) fn update_native_event_modifiers(alt: bool, control: bool, meta: bool, shift: bool) {
    NATIVE_EVENT_MODIFIERS.with(|state| {
        state.set(Some(TerminalModifiers {
            alt,
            control,
            meta,
            shift,
        }));
    });
}

pub(super) fn clear_native_event_modifiers() {
    NATIVE_EVENT_MODIFIERS.with(|state| state.set(None));
}

fn native_event_modifiers() -> Option<TerminalModifiers> {
    NATIVE_EVENT_MODIFIERS.with(Cell::get)
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

pub(super) fn terminal_key_is_direct_for_input(
    input: &NormalizedKeyboardInput,
    option_as_meta: bool,
    preedit_active: bool,
) -> bool {
    terminal_key_is_direct_for_input_with_platform(
        input,
        option_as_meta,
        preedit_active,
        cfg!(target_os = "macos"),
    )
}

#[cfg(test)]
pub(super) fn terminal_key_is_direct(
    text: &str,
    alt: bool,
    control: bool,
    meta: bool,
    shift: bool,
    option_as_meta: bool,
    preedit_active: bool,
) -> bool {
    let input = normalized_keyboard_input_from_ui(
        text,
        text,
        normalize_event_modifiers(alt, control, meta, shift),
        UiKeyboardInputMetadata {
            is_composing: preedit_active,
            uses_native_modifiers: true,
            ..UiKeyboardInputMetadata::default()
        },
    );
    terminal_key_is_direct_for_input(&input, option_as_meta, preedit_active)
}

#[cfg(test)]
fn terminal_key_is_direct_for_platform(
    text: &str,
    modifiers: TerminalModifiers,
    option_as_meta: bool,
    preedit_active: bool,
    apple_platform: bool,
) -> bool {
    let input = normalized_keyboard_input_from_ui(
        text,
        text,
        modifiers,
        UiKeyboardInputMetadata {
            is_composing: preedit_active,
            uses_native_modifiers: true,
            ..UiKeyboardInputMetadata::default()
        },
    );
    terminal_key_is_direct_for_input_with_platform(
        &input,
        option_as_meta,
        preedit_active,
        apple_platform,
    )
}

fn terminal_key_is_direct_for_input_with_platform(
    input: &NormalizedKeyboardInput,
    option_as_meta: bool,
    preedit_active: bool,
    apple_platform: bool,
) -> bool {
    // Slint represents modifier keys as C0 code points (for example, Control
    // is U+0011). A modifier press carries its own modifier state, which would
    // otherwise make it look like a terminal control chord such as Ctrl+Q.
    // Only a following non-modifier key may produce terminal input.
    if is_application_modifier_key(&input.key) {
        return false;
    }

    let direct_modifier = if apple_platform {
        input.modifiers.control || input.modifiers.meta || option_as_meta && input.modifiers.alt
    } else {
        input.modifiers.control || input.modifiers.alt || input.modifiers.meta
    };
    if (preedit_active || input.is_composing) && !direct_modifier {
        return false;
    }

    let Some(key) = terminal_key_from_normalized_input(input) else {
        return false;
    };
    if !matches!(key, TerminalKey::Text(_)) {
        return true;
    }

    if apple_platform {
        return direct_modifier;
    }

    // Ctrl+Alt printable text is commonly AltGr and must stay on TextInput.
    if input.modifiers.control && input.modifiers.alt {
        return input.text.is_empty() || input.modifiers.meta;
    }
    input.modifiers.control || input.modifiers.alt || input.modifiers.meta
}

fn is_application_modifier_key(key: &ApplicationKeyboardKey) -> bool {
    matches!(
        key,
        ApplicationKeyboardKey::Named(
            ApplicationKeyboardNamedKey::Shift
                | ApplicationKeyboardNamedKey::Control
                | ApplicationKeyboardNamedKey::Alt
                | ApplicationKeyboardNamedKey::AltGraph
                | ApplicationKeyboardNamedKey::CapsLock
                | ApplicationKeyboardNamedKey::Meta
        )
    )
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

fn extended_function_key_number(text: &str) -> Option<u8> {
    let number = text.strip_prefix('F')?.parse::<u8>().ok()?;
    (13..=24).contains(&number).then_some(number)
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
    if extended_function_key_number(text).is_some() {
        return Some(text.to_owned());
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
}
