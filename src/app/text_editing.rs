//! Version-scoped adapter for the shared Slint text inputs.
//!
//! Only key intent crosses this boundary. The focused TextInput remains the
//! sole owner of text, grapheme movement, selection, IME, scrolling, and undo.

use i_slint_core::input::InternalKeyEvent;
use i_slint_core::items::{EventResult, InputType, KeyEvent, TextInput};
use i_slint_core::window::{PopupWindowLocation, WindowInner};
use slint::platform::Key;

#[derive(Clone, Copy)]
struct EditPolicy {
    apple_platform: bool,
    secret: bool,
    read_only: bool,
    single_line: bool,
    composing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditAction {
    ParagraphStart,
    ParagraphEnd,
    Backward,
    Forward,
    DeleteBackward,
    DeleteForward,
    PreviousLine,
    NextLine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditDecision {
    Native,
    Block,
    Apply {
        action: EditAction,
        extend_selection: bool,
    },
}

pub(super) fn handle_key(window: &slint::Window, event: &KeyEvent) -> EventResult {
    let adapter = editing_window(window);
    let inner = WindowInner::from_pub(adapter.window());
    // End the RefCell borrow before editing can invoke callbacks or move focus.
    let focused = inner.focus_item.borrow().upgrade();
    let Some(focused) = focused else {
        return EventResult::Reject;
    };
    let Some(input) = focused.downcast::<TextInput>() else {
        return EventResult::Reject;
    };
    let input = input.as_pin_ref();
    if !input.enabled() || !input.has_focus() || !focused.is_visible() {
        return EventResult::Reject;
    }
    let policy = EditPolicy {
        apple_platform: cfg!(target_os = "macos"),
        secret: input.input_type() == InputType::Password,
        read_only: input.read_only(),
        single_line: input.single_line(),
        composing: !input.preedit_text().is_empty(),
    };
    match edit_decision(event, policy) {
        EditDecision::Native => EventResult::Reject,
        EditDecision::Block => EventResult::Accept,
        EditDecision::Apply {
            action,
            extend_selection,
        } => {
            // Call this item directly, without redispatching through menus or
            // window shortcuts. The original key callback is in progress, so
            // Slint's callback guard lets its standard editor handle this key.
            let translated = translated_event(action, extend_selection);
            focused
                .borrow()
                .as_ref()
                .key_event(&translated, &adapter, &focused);
            EventResult::Accept
        }
    }
}

fn editing_window(window: &slint::Window) -> std::rc::Rc<dyn i_slint_core::window::WindowAdapter> {
    let mut adapter = WindowInner::from_pub(window).window_adapter();
    // Popup search fields may have a separate native window. Embedded popups
    // already share the current adapter; tooltips never have a focused item.
    for _ in 0..8 {
        let next = WindowInner::from_pub(adapter.window())
            .active_popups()
            .iter()
            .rev()
            .find_map(|popup| match &popup.location {
                PopupWindowLocation::TopLevel(next)
                    if WindowInner::from_pub(next.window())
                        .focus_item
                        .borrow()
                        .upgrade()
                        .is_some() =>
                {
                    Some(next.clone())
                }
                _ => None,
            });
        let Some(next) = next else {
            break;
        };
        if std::rc::Rc::ptr_eq(&adapter, &next) {
            break;
        }
        adapter = next;
    }
    adapter
}

fn edit_decision(event: &KeyEvent, policy: EditPolicy) -> EditDecision {
    let key = command_character(event.text.as_str());
    let modifiers = event.modifiers;
    if policy.secret
        && (modifiers.control || modifiers.meta)
        && (matches!(key, Some('c' | 'x'))
            || key == Some('a') && (modifiers.control || !policy.apple_platform))
    {
        return EditDecision::Block;
    }
    // On macOS Slint calls physical Control "meta" and Command "control".
    // Windows/Linux keep their native Ctrl shortcuts, including Ctrl+A.
    if !policy.apple_platform || !modifiers.meta || modifiers.control || modifiers.alt {
        return EditDecision::Native;
    }
    let action = match key {
        Some('a') => EditAction::ParagraphStart,
        Some('e') => EditAction::ParagraphEnd,
        Some('b') => EditAction::Backward,
        Some('f') => EditAction::Forward,
        Some('h') => EditAction::DeleteBackward,
        Some('d') => EditAction::DeleteForward,
        Some('p') => EditAction::PreviousLine,
        Some('n') => EditAction::NextLine,
        _ => return EditDecision::Native,
    };
    let deletes = matches!(
        action,
        EditAction::DeleteBackward | EditAction::DeleteForward
    );
    let changes_line = matches!(action, EditAction::PreviousLine | EditAction::NextLine);
    if policy.composing || deletes && policy.read_only || changes_line && policy.single_line {
        return EditDecision::Block;
    }
    EditDecision::Apply {
        action,
        extend_selection: modifiers.shift && !policy.secret && !deletes,
    }
}

fn command_character(text: &str) -> Option<char> {
    let mut characters = text.chars();
    let character = characters.next()?;
    if characters.next().is_some() {
        return None;
    }
    if character.is_ascii_alphabetic() {
        return Some(character.to_ascii_lowercase());
    }
    // Accept unambiguous Control character encodings. 0x10 is Slint's Shift
    // key too, so Control-P must arrive as its layout-resolved logical letter.
    match character {
        '\u{1}' => Some('a'),
        '\u{2}' => Some('b'),
        '\u{3}' => Some('c'),
        '\u{4}' => Some('d'),
        '\u{5}' => Some('e'),
        '\u{6}' => Some('f'),
        '\u{8}' => Some('h'),
        '\u{e}' => Some('n'),
        '\u{18}' => Some('x'),
        _ => None,
    }
}

fn translated_event(action: EditAction, extend_selection: bool) -> InternalKeyEvent {
    let (key, alt) = match action {
        EditAction::ParagraphStart => (Key::UpArrow, true),
        EditAction::ParagraphEnd => (Key::DownArrow, true),
        EditAction::Backward => (Key::LeftArrow, false),
        EditAction::Forward => (Key::RightArrow, false),
        EditAction::DeleteBackward => (Key::Backspace, false),
        EditAction::DeleteForward => (Key::Delete, false),
        EditAction::PreviousLine => (Key::UpArrow, false),
        EditAction::NextLine => (Key::DownArrow, false),
    };
    let mut key_event = KeyEvent::default();
    key_event.text = key.into();
    key_event.modifiers.alt = alt;
    key_event.modifiers.shift = extend_selection;
    InternalKeyEvent {
        key_event,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests;
