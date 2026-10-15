use std::path::Path;

#[cfg(target_os = "macos")]
use anyhow::{Context, Result};
#[cfg(target_os = "macos")]
use slint::ComponentHandle;
#[cfg(target_os = "macos")]
use slint::winit_030::WinitWindowAccessor;
use std::cell::RefCell;
#[cfg(target_os = "macos")]
use tracing::warn;

#[cfg(target_os = "macos")]
use super::diagnostics::log_menu_action;
#[cfg(target_os = "macos")]
use super::input::menu_shortcut_from_setting;
use super::view::set_status;
use super::*;

pub(super) const ISSUES_URL: &str = "https://github.com/xinalbert/ax_ssh/issues/new";

// A persistent owner is required by X11/Wayland. All access is serialized on
// the UI thread, including detached windows; failures remain distinguishable
// from a valid empty string.
thread_local! {
    static CLIPBOARD: RefCell<Option<arboard::Clipboard>> = const { RefCell::new(None) };
}

pub(super) fn with_clipboard<T>(
    operation: impl FnOnce(&mut arboard::Clipboard) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    use anyhow::Context as _;
    CLIPBOARD.with(|slot| {
        let mut slot = slot.try_borrow_mut().context("clipboard is busy")?;
        if slot.is_none() {
            *slot = Some(arboard::Clipboard::new().context("cannot open system clipboard")?);
        }
        let clipboard = slot.as_mut().context("clipboard is unavailable")?;
        operation(clipboard)
    })
}

pub(super) fn release_clipboard() {
    CLIPBOARD.with(|slot| {
        slot.borrow_mut().take();
    });
}

pub(super) fn set_platform_clipboard_text(ui: &AppWindow, text: &str) -> bool {
    match with_clipboard(|clipboard| Ok(clipboard.set_text(text)?)) {
        Ok(()) => true,
        Err(error) => {
            ui.set_status(format!("Cannot write clipboard: {error}").into());
            false
        }
    }
}

pub(super) fn platform_clipboard_text(_ui: &AppWindow) -> anyhow::Result<String> {
    with_clipboard(|clipboard| Ok(clipboard.get_text()?))
}

pub(super) fn build_revision() -> &'static str {
    option_env!("AXSSH_BUILD_REVISION").unwrap_or("unknown")
}

pub(super) fn diagnostic_info() -> String {
    let renderer = renderer_diagnostics();
    format!(
        "AxSSH diagnostics\nversion: {}\nbuild-revision: {}\nos: {}\narch: {}\nprofile: {}\nrenderer-requested: {}\nrenderer-selected: {}\nrenderer-source: {}\nrenderer-fallback-reason: {}\nmetal-device: {}\nrenderer-window-count: {}\nrenderer-fault-count: {}\nrenderer-shader-faults-observed: {}\nrenderer-shader-error-capture: application-errors-only\n",
        env!("CARGO_PKG_VERSION"),
        build_revision(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        renderer.requested_backend,
        renderer.selected_backend,
        renderer.source,
        renderer.fallback_reason,
        renderer.metal_device,
        renderer.window_count,
        renderer.fault_count,
        renderer.shader_fault_count,
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
    if !ui.get_window_active() {
        return Ok(());
    }
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
            let main_ui = GLOBAL_WINDOW_ROUTER
                .get()
                .and_then(WindowRouter::main_ui)
                .and_then(|ui| ui.upgrade());
            if let Some(target) = activate_application_menu_section(&ui, main_ui, section)
                && ui.get_detached_window()
            {
                target.window().set_minimized(false);
                if target
                    .window()
                    .with_winit_window(|window| window.focus_window())
                    .is_none()
                {
                    tracing::debug!("main window has no native adapter for application-menu focus");
                }
            }
        },
    )
}

#[cfg(target_os = "macos")]
fn activate_application_menu_section(
    source: &AppWindow,
    main_ui: Option<AppWindow>,
    section: macos_window::NativeMenuSection,
) -> Option<AppWindow> {
    if !source.get_menu_shortcuts_enabled() {
        return None;
    }
    let target = if source.get_detached_window() {
        main_ui?
    } else {
        source.clone_strong()
    };
    if !target.get_menu_shortcuts_enabled() {
        return None;
    }
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
    target.invoke_request_settings_section(section.into());
    target.invoke_open_settings();
    Some(target)
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

    #[cfg(target_os = "macos")]
    #[test]
    fn application_menu_sections_from_child_use_main_window_and_both_modal_guards() {
        std::thread::spawn(|| {
            use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
            use slint::platform::{Platform, WindowAdapter};
            use std::cell::Cell;
            use std::rc::Rc;

            struct TestPlatform;
            impl Platform for TestPlatform {
                fn create_window_adapter(
                    &self,
                ) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
                    Ok(MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer))
                }
            }
            slint::platform::set_platform(Box::new(TestPlatform)).expect("headless menu platform");
            let main = AppWindow::new().expect("main component");
            let child = AppWindow::new().expect("child component");
            child.set_detached_window(true);
            let main_actions = Rc::new(Cell::new(0));
            let received = main_actions.clone();
            main.on_open_settings(move || received.set(received.get() + 1));
            let child_actions = Rc::new(Cell::new(0));
            let received = child_actions.clone();
            child.on_open_settings(move || received.set(received.get() + 1));
            for section in [
                macos_window::NativeMenuSection::Settings,
                macos_window::NativeMenuSection::About,
            ] {
                assert!(
                    activate_application_menu_section(&child, Some(main.clone_strong()), section)
                        .is_some()
                );
            }
            assert_eq!(main_actions.get(), 2);
            assert_eq!(child_actions.get(), 0);
            main.set_password_dialog_open(true);
            assert!(
                activate_application_menu_section(
                    &child,
                    Some(main.clone_strong()),
                    macos_window::NativeMenuSection::Settings
                )
                .is_none()
            );
            main.set_password_dialog_open(false);
            child.set_password_dialog_open(true);
            assert!(
                activate_application_menu_section(
                    &child,
                    Some(main.clone_strong()),
                    macos_window::NativeMenuSection::Settings
                )
                .is_none()
            );
            child.set_password_dialog_open(false);
            assert!(
                activate_application_menu_section(
                    &child,
                    None,
                    macos_window::NativeMenuSection::Settings
                )
                .is_none()
            );
            assert_eq!(main_actions.get(), 2);
            assert_eq!(child_actions.get(), 0);
        })
        .join()
        .expect("application menu owner and security gates");
    }

    #[test]
    fn copied_diagnostics_are_build_metadata_only() {
        let diagnostics = diagnostic_info();

        assert!(diagnostics.contains("version: "));
        assert!(diagnostics.contains("build-revision: "));
        assert!(diagnostics.contains("os: "));
        assert!(diagnostics.contains("arch: "));
        assert!(diagnostics.contains("profile: "));
        assert!(diagnostics.contains("renderer-selected: "));
        assert!(diagnostics.contains("metal-device: "));
        assert!(diagnostics.contains("renderer-window-count: "));
        assert!(diagnostics.contains("renderer-shader-error-capture: application-errors-only"));
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
