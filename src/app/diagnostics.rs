use std::borrow::Cow;
use std::time::Duration;

use ax_ssh::terminal::{TerminalKey, TerminalModifiers};

use super::input::{normalize_event_modifiers, terminal_key_from_slint};

const TARGET: &str = "ax_ssh::diagnostics";
const LATENCY_TARGET: &str = "ax_ssh::latency";

pub(super) fn log_keyboard_event(
    text: &str,
    alt: bool,
    control: bool,
    meta: bool,
    shift: bool,
    route: &str,
    action: &str,
) {
    let modifiers = normalize_event_modifiers(alt, control, meta, shift);
    let key = terminal_key_from_slint(text, modifiers);
    let route = safe_keyboard_route(route);
    let action = safe_keyboard_action(action);
    tracing::debug!(
        target: TARGET,
        event = "keyboard",
        key = %terminal_key_label(&key),
        alt = modifiers.alt,
        control = modifiers.control,
        meta = modifiers.meta,
        shift = modifiers.shift,
        route,
        action,
        "keyboard event routed"
    );
}

pub(super) fn log_terminal_input(
    key: &TerminalKey,
    modifiers: TerminalModifiers,
    physical_key_event: bool,
) {
    tracing::debug!(
        target: TARGET,
        event = "keyboard",
        key = %terminal_key_label(key),
        alt = modifiers.alt,
        control = modifiers.control,
        meta = modifiers.meta,
        shift = modifiers.shift,
        physical = physical_key_event,
        route = "terminal",
        action = "send-input",
        "terminal input recognized"
    );
}

pub(super) fn log_terminal_input_latency(outcome: &'static str, elapsed: Duration) {
    tracing::debug!(
        target: LATENCY_TARGET,
        event = "terminal-input",
        stage = "ui-to-worker-request",
        outcome,
        elapsed_us = duration_micros(elapsed),
        "terminal input request completed"
    );
}

pub(super) fn log_ui_action(action: &'static str) {
    tracing::debug!(
        target: TARGET,
        event = "ui-action",
        action,
        "UI action invoked"
    );
}

pub(super) fn log_menu_action(action: &str) {
    tracing::debug!(
        target: TARGET,
        event = "menu-action",
        action = safe_menu_action(action),
        "menu action invoked"
    );
}

pub(super) fn log_ui_action_outcome(action: &'static str, outcome: &'static str) {
    tracing::debug!(
        target: TARGET,
        event = "ui-action",
        action,
        outcome,
        "UI action completed"
    );
}

fn terminal_key_label(key: &TerminalKey) -> Cow<'static, str> {
    match key {
        TerminalKey::Text(_) => Cow::Borrowed("Text"),
        TerminalKey::Return => Cow::Borrowed("Enter"),
        TerminalKey::Backspace => Cow::Borrowed("Backspace"),
        TerminalKey::Tab => Cow::Borrowed("Tab"),
        TerminalKey::Escape => Cow::Borrowed("Escape"),
        TerminalKey::Up => Cow::Borrowed("ArrowUp"),
        TerminalKey::Down => Cow::Borrowed("ArrowDown"),
        TerminalKey::Right => Cow::Borrowed("ArrowRight"),
        TerminalKey::Left => Cow::Borrowed("ArrowLeft"),
        TerminalKey::Insert => Cow::Borrowed("Insert"),
        TerminalKey::Delete => Cow::Borrowed("Delete"),
        TerminalKey::Home => Cow::Borrowed("Home"),
        TerminalKey::End => Cow::Borrowed("End"),
        TerminalKey::PageUp => Cow::Borrowed("PageUp"),
        TerminalKey::PageDown => Cow::Borrowed("PageDown"),
        TerminalKey::Function(number) => Cow::Owned(format!("F{number}")),
    }
}

fn duration_micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

fn safe_keyboard_route(route: &str) -> &'static str {
    match route {
        "transient-control" => "transient-control",
        _ => "unknown",
    }
}

fn safe_keyboard_action(action: &str) -> &'static str {
    match action {
        "dismiss-transient" => "dismiss-transient",
        _ => "unknown",
    }
}

fn safe_menu_action(action: &str) -> &'static str {
    match action {
        "close-tab" => "close-tab",
        "export-selected" => "export-selected",
        "import-sessions" => "import-sessions",
        "new-local-shell" => "new-local-shell",
        "new-session" => "new-session",
        "open-about" => "open-about",
        "open-settings" => "open-settings",
        "open-sftp" => "open-sftp",
        "open-shortcuts" => "open-shortcuts",
        "refresh-sftp" => "refresh-sftp",
        "toggle-sidebar" => "toggle-sidebar",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_text_is_always_redacted() {
        assert_eq!(
            terminal_key_label(&TerminalKey::Text("password".to_owned())),
            "Text"
        );
        assert_eq!(
            terminal_key_label(&TerminalKey::Text("multi-line paste\n".to_owned())),
            "Text"
        );
    }

    #[test]
    fn special_keys_have_stable_diagnostic_labels() {
        assert_eq!(terminal_key_label(&TerminalKey::Return), "Enter");
        assert_eq!(terminal_key_label(&TerminalKey::Up), "ArrowUp");
        assert_eq!(terminal_key_label(&TerminalKey::Function(12)), "F12");
    }

    #[test]
    fn diagnostic_routes_and_actions_reject_arbitrary_values() {
        assert_eq!(
            safe_keyboard_route("transient-control"),
            "transient-control"
        );
        assert_eq!(safe_keyboard_route("user supplied"), "unknown");
        assert_eq!(
            safe_keyboard_action("dismiss-transient"),
            "dismiss-transient"
        );
        assert_eq!(safe_keyboard_action("secret"), "unknown");
        assert_eq!(safe_menu_action("open-settings"), "open-settings");
        assert_eq!(safe_menu_action("user supplied"), "unknown");
    }
}
