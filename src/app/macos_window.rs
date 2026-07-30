//! Native macOS title-bar integration for the Slint window.

use anyhow::{Context, Result};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, Sel};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSMenuItem, NSView, NSWindow};
use objc2_foundation::{MainThreadMarker, NSObject, NSString};
use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

#[derive(Clone, Copy)]
pub(super) enum NativeMenuSection {
    Settings,
    About,
}

struct NativeMenuTargetIvars {
    activate: Box<dyn Fn(NativeMenuSection)>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements. The target and its
    // callback are main-thread-only and are released with the Objective-C object.
    #[unsafe(super(NSObject))]
    #[name = "AxSSHNativeMenuTarget"]
    #[thread_kind = MainThreadOnly]
    #[ivars = NativeMenuTargetIvars]
    struct NativeMenuTarget;

    // SAFETY: NSObjectProtocol has no additional implementation requirements.
    unsafe impl NSObjectProtocol for NativeMenuTarget {}

    impl NativeMenuTarget {
        #[unsafe(method(openSettings:))]
        fn open_settings(&self, _sender: Option<&AnyObject>) {
            (self.ivars().activate)(NativeMenuSection::Settings);
        }

        #[unsafe(method(showAbout:))]
        fn show_about(&self, _sender: Option<&AnyObject>) {
            (self.ivars().activate)(NativeMenuSection::About);
        }
    }
);

impl NativeMenuTarget {
    fn new(
        mtm: MainThreadMarker,
        activate: impl Fn(NativeMenuSection) + 'static,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(NativeMenuTargetIvars {
            activate: Box::new(activate),
        });
        // SAFETY: `this` is an allocated NativeMenuTarget and NSObject's `init`
        // has the selector signature used here.
        unsafe { msg_send![super(this), init] }
    }
}

pub(super) fn configure(window: &slint::Window) -> Result<()> {
    with_native_window(window, |native_window| {
        native_window.setMovableByWindowBackground(false);
        Ok(())
    })
}

pub(super) fn configure_application_menu(
    activate: impl Fn(NativeMenuSection) + 'static,
) -> Result<()> {
    let mtm = MainThreadMarker::new().context("macOS menu setup must run on the main thread")?;
    let application = NSApplication::sharedApplication(mtm);
    let main_menu = application
        .mainMenu()
        .context("AppKit application has no main menu")?;
    let application_item = main_menu
        .itemAtIndex(0)
        .context("AppKit main menu has no application item")?;
    let application_menu = application_item
        .submenu()
        .context("AppKit application item has no submenu")?;
    let about_item = application_menu
        .itemAtIndex(0)
        .context("AppKit application menu has no About item")?;
    let target = NativeMenuTarget::new(mtm, activate);

    bind_menu_item(&about_item, &target, sel!(showAbout:));

    let settings_title = NSString::from_str("Settings...");
    let settings_item = match application_menu.itemWithTitle(&settings_title) {
        Some(item) => item,
        None => {
            let key_equivalent = NSString::from_str(",");
            // SAFETY: `openSettings:` is implemented by NativeMenuTarget with
            // the NSMenuItem action signature.
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &settings_title,
                    Some(sel!(openSettings:)),
                    &key_equivalent,
                )
            };
            application_menu.insertItem_atIndex(&item, 1);
            item
        }
    };
    settings_item.setKeyEquivalentModifierMask(NSEventModifierFlags::Command);
    bind_menu_item(&settings_item, &target, sel!(openSettings:));
    Ok(())
}

fn with_native_window<T>(
    window: &slint::Window,
    operation: impl FnOnce(&NSWindow) -> Result<T>,
) -> Result<T> {
    let handle = window.window_handle();
    let raw = handle
        .window_handle()
        .context("macOS window handle is not available")?
        .as_raw();
    let RawWindowHandle::AppKit(appkit) = raw else {
        anyhow::bail!("Slint did not create an AppKit window");
    };

    // SAFETY: raw-window-handle guarantees that `ns_view` points to the live
    // NSView owned by this window for the lifetime of the borrowed handle.
    let view = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
    let native_window = view.window().context("AppKit view has no NSWindow")?;
    operation(&native_window)
}

fn bind_menu_item(item: &NSMenuItem, target: &NativeMenuTarget, action: Sel) {
    // SAFETY: The selectors passed here are implemented by NativeMenuTarget.
    // NSMenuItem keeps a weak target, so representedObject retains the same
    // target for as long as the menu item remains installed.
    unsafe {
        item.setAction(Some(action));
        item.setTarget(Some(target));
        item.setRepresentedObject(Some(target));
    }
}
