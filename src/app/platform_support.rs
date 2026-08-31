use std::path::Path;

#[cfg(target_os = "macos")]
use anyhow::{Context, Result};
use slint::ComponentHandle;
use slint::platform::Clipboard;
#[cfg(target_os = "macos")]
use tracing::warn;

#[cfg(target_os = "macos")]
use super::diagnostics::log_menu_action;
#[cfg(target_os = "macos")]
use super::input::menu_shortcut_from_setting;
use super::view::set_status;
use super::*;

pub(super) const ISSUES_URL: &str = "https://github.com/xinalbert/ax_ssh/issues/new";

pub(super) fn set_platform_clipboard_text(ui: &AppWindow, text: &str) {
    // Slint 1.17 exposes clipboard access through its window context.
    slint::private_unstable_api::re_exports::WindowInner::from_pub(ui.window())
        .context()
        .platform()
        .set_clipboard_text(text, Clipboard::DefaultClipboard);
}

pub(super) fn platform_clipboard_text(ui: &AppWindow) -> String {
    slint::private_unstable_api::re_exports::WindowInner::from_pub(ui.window())
        .context()
        .platform()
        .clipboard_text(Clipboard::DefaultClipboard)
        .unwrap_or_default()
}

pub(super) fn build_revision() -> &'static str {
    option_env!("AXSSH_BUILD_REVISION").unwrap_or("unknown")
}

pub(super) fn diagnostic_info() -> String {
    format!(
        "AxSSH diagnostics\nversion: {}\nbuild-revision: {}\nos: {}\narch: {}\nprofile: {}\n",
        env!("CARGO_PKG_VERSION"),
        build_revision(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
    )
}

pub(super) fn open_external_target(
    ui: &slint::Weak<AppWindow>,
    target: &str,
    failure_message: &str,
) {
    if open::that_detached(target).is_err() {
        tracing::warn!(target: "ax_ssh::diagnostics", operation = "open-external-target", "failed to open support target");
        set_status(ui, failure_message);
    }
}

pub(super) fn open_external_path(ui: &slint::Weak<AppWindow>, path: &Path, failure_message: &str) {
    if open::that_detached(path).is_err() {
        tracing::warn!(target: "ax_ssh::diagnostics", operation = "open-log-directory", "failed to open log directory");
        set_status(ui, failure_message);
    }
}

#[cfg(target_os = "macos")]
const MACOS_APPLICATION_MENU_MAX_RETRIES: u8 = 8;

#[cfg(target_os = "macos")]
pub(super) fn configure_macos_application_menu(ui: &AppWindow) -> Result<()> {
    let shortcut = menu_shortcut_from_setting(ui.get_open_settings_shortcut().as_str())
        .context("cannot configure the macOS Settings shortcut")?;
    let ui_for_menu = ui.as_weak();
    macos_window::configure_application_menu(
        &shortcut.native,
        ui.get_menu_shortcuts_enabled(),
        move |section| {
            let Some(ui) = ui_for_menu.upgrade() else {
                return;
            };
            let section = match section {
                macos_window::NativeMenuSection::Settings => {
                    log_menu_action("open-settings");
                    "General"
                }
                macos_window::NativeMenuSection::About => {
                    log_menu_action("open-about");
                    "About"
                }
            };
            ui.invoke_request_settings_section(section.into());
            ui.invoke_open_settings();
        },
    )
}

#[cfg(target_os = "macos")]
pub(super) fn schedule_macos_application_menu_configuration(ui: &AppWindow) {
    let ui_for_menu = ui.as_weak();
    slint::Timer::single_shot(Duration::from_millis(1), move || {
        retry_macos_application_menu_configuration(ui_for_menu, 0);
    });
}

#[cfg(target_os = "macos")]
fn retry_macos_application_menu_configuration(ui: slint::Weak<AppWindow>, attempt: u8) {
    let Some(ui) = ui.upgrade() else {
        return;
    };
    match configure_macos_application_menu(&ui) {
        Ok(()) => {}
        Err(_) if should_retry_macos_application_menu_configuration(attempt) => {
            let ui = ui.as_weak();
            slint::Timer::single_shot(Duration::from_millis(25), move || {
                retry_macos_application_menu_configuration(ui, attempt + 1);
            });
        }
        Err(error) => {
            warn!(
                attempts = u16::from(attempt) + 1,
                %error,
                "failed to connect the standard macOS application menu after retries"
            );
        }
    }
}

#[cfg(target_os = "macos")]
fn should_retry_macos_application_menu_configuration(attempt: u8) -> bool {
    attempt < MACOS_APPLICATION_MENU_MAX_RETRIES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_diagnostics_are_build_metadata_only() {
        let diagnostics = diagnostic_info();

        assert!(diagnostics.contains("version: "));
        assert!(diagnostics.contains("build-revision: "));
        assert!(diagnostics.contains("os: "));
        assert!(diagnostics.contains("arch: "));
        assert!(diagnostics.contains("profile: "));
        for forbidden in ["host:", "password", "session-id", "sessions.json"] {
            assert!(!diagnostics.contains(forbidden));
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn retries_macos_application_menu_within_the_bounded_budget() {
        assert!(should_retry_macos_application_menu_configuration(0));
        assert!(should_retry_macos_application_menu_configuration(
            MACOS_APPLICATION_MENU_MAX_RETRIES - 1
        ));
        assert!(!should_retry_macos_application_menu_configuration(
            MACOS_APPLICATION_MENU_MAX_RETRIES
        ));
    }
}
