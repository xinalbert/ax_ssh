use std::time::Duration;

use super::input::{ApplicationKeyboardKey, NormalizedKeyboardInput};

const TARGET: &str = "ax_ssh::diagnostics";
const LATENCY_TARGET: &str = "ax_ssh::latency";

pub(super) fn log_keyboard_event(input: &NormalizedKeyboardInput, route: &str, action: &str) {
    let route = safe_keyboard_route(route);
    let action = safe_keyboard_action(action);
    tracing::debug!(
        target: TARGET,
        event = "keyboard",
        key = %application_key_label(&input.key),
        alt = input.modifiers.alt,
        control = input.modifiers.control,
        meta = input.modifiers.meta,
        shift = input.modifiers.shift,
        physical_keycode = ?input.physical_keycode,
        location = ?input.location,
        composing = input.is_composing,
        repeat = input.is_repeat,
        synthetic = input.is_synthetic,
        route,
        action,
        "keyboard event routed"
    );
}

pub(super) fn log_terminal_input(input: &super::input::NormalizedKeyboardInput) {
    tracing::debug!(
        target: TARGET,
        event = "keyboard",
        key = %application_key_label(&input.key),
        alt = input.modifiers.alt,
        control = input.modifiers.control,
        meta = input.modifiers.meta,
        shift = input.modifiers.shift,
        physical = input.is_physical_key_event(),
        physical_keycode = ?input.physical_keycode,
        location = ?input.location,
        composing = input.is_composing,
        repeat = input.is_repeat,
        route = "terminal",
        action = "send-input",
        "terminal input recognized"
    );
}

pub(super) fn log_terminal_input_latency(
    outcome: &'static str,
    elapsed: Duration,
    state_lock_elapsed: Option<Duration>,
    worker_request_elapsed: Option<Duration>,
) {
    tracing::debug!(
        target: LATENCY_TARGET,
        event = "terminal-input",
        stage = "ui-to-worker-request",
        outcome,
        elapsed_us = duration_micros(elapsed),
        state_lock_us = state_lock_elapsed.map(duration_micros),
        worker_request_us = worker_request_elapsed.map(duration_micros),
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

fn application_key_label(key: &ApplicationKeyboardKey) -> &'static str {
    match key {
        ApplicationKeyboardKey::Text(_) => "Text",
        ApplicationKeyboardKey::Named(key) => match key {
            super::input::ApplicationKeyboardNamedKey::Return => "Enter",
            super::input::ApplicationKeyboardNamedKey::Backspace => "Backspace",
            super::input::ApplicationKeyboardNamedKey::Tab => "Tab",
            super::input::ApplicationKeyboardNamedKey::Escape => "Escape",
            super::input::ApplicationKeyboardNamedKey::Up => "ArrowUp",
            super::input::ApplicationKeyboardNamedKey::Down => "ArrowDown",
            super::input::ApplicationKeyboardNamedKey::Right => "ArrowRight",
            super::input::ApplicationKeyboardNamedKey::Left => "ArrowLeft",
            super::input::ApplicationKeyboardNamedKey::Insert => "Insert",
            super::input::ApplicationKeyboardNamedKey::Delete => "Delete",
            super::input::ApplicationKeyboardNamedKey::Home => "Home",
            super::input::ApplicationKeyboardNamedKey::End => "End",
            super::input::ApplicationKeyboardNamedKey::PageUp => "PageUp",
            super::input::ApplicationKeyboardNamedKey::PageDown => "PageDown",
            super::input::ApplicationKeyboardNamedKey::Function(_) => "Function",
            super::input::ApplicationKeyboardNamedKey::Space => "Space",
            super::input::ApplicationKeyboardNamedKey::Shift => "Shift",
            super::input::ApplicationKeyboardNamedKey::Control => "Control",
            super::input::ApplicationKeyboardNamedKey::Alt => "Alt",
            super::input::ApplicationKeyboardNamedKey::AltGraph => "AltGraph",
            super::input::ApplicationKeyboardNamedKey::CapsLock => "CapsLock",
            super::input::ApplicationKeyboardNamedKey::Meta => "Meta",
        },
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
        "copy-terminal" => "copy-terminal",
        "export-selected" => "export-selected",
        "import-sessions" => "import-sessions",
        "new-local-shell" => "new-local-shell",
        "new-session" => "new-session",
        "open-about" => "open-about",
        "open-recent-workspace" => "open-recent-workspace",
        "open-settings" => "open-settings",
        "open-workspace" => "open-workspace",
        "clear-recent-workspaces" => "clear-recent-workspaces",
        "open-shortcuts" => "open-shortcuts",
        "paste-terminal" => "paste-terminal",
        "next-tab" => "next-tab",
        "previous-tab" => "previous-tab",
        "refresh-sftp" => "refresh-sftp",
        "save-workspace" => "save-workspace",
        "select-all-terminal" => "select-all-terminal",
        "switch-ssh-sftp" => "switch-ssh-sftp",
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
            application_key_label(&ApplicationKeyboardKey::Text("password".to_owned())),
            "Text"
        );
        assert_eq!(
            application_key_label(&ApplicationKeyboardKey::Text(
                "multi-line paste\n".to_owned()
            )),
            "Text"
        );
    }

    #[test]
    fn special_keys_have_stable_diagnostic_labels() {
        assert_eq!(
            application_key_label(&ApplicationKeyboardKey::Named(
                crate::app::input::ApplicationKeyboardNamedKey::Return
            )),
            "Enter"
        );
        assert_eq!(
            application_key_label(&ApplicationKeyboardKey::Named(
                crate::app::input::ApplicationKeyboardNamedKey::Up
            )),
            "ArrowUp"
        );
        assert_eq!(
            application_key_label(&ApplicationKeyboardKey::Named(
                crate::app::input::ApplicationKeyboardNamedKey::Function(12)
            )),
            "Function"
        );
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
        assert_eq!(safe_menu_action("open-workspace"), "open-workspace");
        assert_eq!(
            safe_menu_action("open-recent-workspace"),
            "open-recent-workspace"
        );
        assert_eq!(
            safe_menu_action("clear-recent-workspaces"),
            "clear-recent-workspaces"
        );
        assert_eq!(safe_menu_action("previous-tab"), "previous-tab");
        assert_eq!(safe_menu_action("next-tab"), "next-tab");
        assert_eq!(safe_menu_action("copy-terminal"), "copy-terminal");
        assert_eq!(safe_menu_action("paste-terminal"), "paste-terminal");
        assert_eq!(
            safe_menu_action("select-all-terminal"),
            "select-all-terminal"
        );
        assert_eq!(safe_menu_action("save-workspace"), "save-workspace");
        assert_eq!(safe_menu_action("user supplied"), "unknown");
    }
}
