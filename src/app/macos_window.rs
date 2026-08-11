//! Native macOS title-bar integration for the Slint window.

use anyhow::{Context, Result};
use ax_ssh::terminal::TerminalModifiers;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, Sel};
use objc2::{AnyThread, DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSAutoresizingMaskOptions, NSButton, NSCellImagePosition, NSDeleteFunctionKey,
    NSDownArrowFunctionKey, NSEndFunctionKey, NSEvent, NSEventModifierFlags, NSF1FunctionKey,
    NSHomeFunctionKey, NSImage, NSImageNameGoBackTemplate, NSInsertFunctionKey,
    NSLeftArrowFunctionKey, NSMenu, NSMenuItem, NSPageDownFunctionKey, NSPageUpFunctionKey,
    NSRightArrowFunctionKey, NSUpArrowFunctionKey, NSView, NSWindow, NSWindowButton,
};
use objc2_foundation::{MainThreadMarker, NSData, NSObject, NSPoint, NSRect, NSSize, NSString};
use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

use super::input::NativeMenuShortcut;

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

const TITLE_BAR_BUTTON_WIDTH: f64 = 28.0;
const TITLE_BAR_BUTTON_HEIGHT: f64 = 20.0;
const TITLE_BAR_BUTTON_SPACING: f64 = 2.0;
const TITLE_BAR_BUTTON_TRAILING_MARGIN: f64 = 12.0;

struct NativeTitleBarButtonIvars {
    activate: Box<dyn Fn()>,
}

define_class!(
    // SAFETY: NSButton has no subclassing requirements. The callback is
    // main-thread-only and the title-bar view retains this button for its life.
    #[unsafe(super(NSButton))]
    #[name = "AxSSHNativeTitleBarButton"]
    #[thread_kind = MainThreadOnly]
    #[ivars = NativeTitleBarButtonIvars]
    struct NativeTitleBarButton;

    // SAFETY: NSObjectProtocol has no additional implementation requirements.
    unsafe impl NSObjectProtocol for NativeTitleBarButton {}

    impl NativeTitleBarButton {
        #[unsafe(method(activate:))]
        fn activate(&self, _sender: Option<&AnyObject>) {
            (self.ivars().activate)();
        }
    }
);

impl NativeTitleBarButton {
    fn new(mtm: MainThreadMarker, activate: impl Fn() + 'static) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(NativeTitleBarButtonIvars {
            activate: Box::new(activate),
        });
        // SAFETY: `this` is an allocated NativeTitleBarButton and NSButton's
        // initializer has the selector signature used here.
        unsafe {
            msg_send![super(this), initWithFrame: NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(TITLE_BAR_BUTTON_WIDTH, TITLE_BAR_BUTTON_HEIGHT),
            )]
        }
    }
}

pub(super) fn configure(window: &slint::Window) -> Result<()> {
    with_native_window(window, |native_window| {
        native_window.setMovableByWindowBackground(false);
        Ok(())
    })
}

pub(super) fn configure_detached_titlebar_buttons(
    window: &slint::Window,
    show_terminal_actions: bool,
    split_right: impl Fn() + 'static,
    split_down: impl Fn() + 'static,
    return_workspace: impl Fn() + 'static,
) -> Result<()> {
    let mtm =
        MainThreadMarker::new().context("macOS title-bar setup must run on the main thread")?;
    let mut buttons = Vec::with_capacity(if show_terminal_actions { 3 } else { 1 });
    if show_terminal_actions {
        if let Some(button) = configure_titlebar_button(
            mtm,
            "rectangle.split.2x1",
            "Split active terminal vertically",
            None,
            split_right,
        ) {
            buttons.push(button);
        }
        if let Some(button) = configure_titlebar_button(
            mtm,
            "rectangle.split.1x2",
            "Split active terminal horizontally",
            None,
            split_down,
        ) {
            buttons.push(button);
        }
    }
    // SAFETY: AppKit exports this process-lifetime NSImageName constant on
    // every supported macOS version; this code only reads its shared value.
    let fallback_name = unsafe { NSImageNameGoBackTemplate };
    let return_button = configure_titlebar_button(
        mtm,
        "arrow.uturn.backward",
        "Return workspace to main window",
        NSImage::imageNamed(fallback_name),
        return_workspace,
    )
    .context("AppKit could not load the detached workspace return icon")?;
    buttons.push(return_button);

    with_native_window(window, |native_window| {
        let zoom_button = native_window
            .standardWindowButton(NSWindowButton::ZoomButton)
            .context("AppKit window has no standard zoom button")?;
        // SAFETY: AppKit owns the standard button and its superview for the
        // native window lifetime. The title-bar view retains added subviews.
        let title_bar = unsafe { zoom_button.superview() }
            .context("AppKit standard zoom button has no title-bar view")?;
        let bounds = title_bar.bounds();
        let control_width = TITLE_BAR_BUTTON_WIDTH * buttons.len() as f64
            + TITLE_BAR_BUTTON_SPACING * buttons.len().saturating_sub(1) as f64;
        let origin_x =
            (bounds.size.width - control_width - TITLE_BAR_BUTTON_TRAILING_MARGIN).max(0.0);
        let origin_y = ((bounds.size.height - TITLE_BAR_BUTTON_HEIGHT) / 2.0).max(0.0);
        for (index, button) in buttons.into_iter().enumerate() {
            let origin_x =
                origin_x + index as f64 * (TITLE_BAR_BUTTON_WIDTH + TITLE_BAR_BUTTON_SPACING);
            button.setFrame(NSRect::new(
                NSPoint::new(origin_x, origin_y),
                NSSize::new(TITLE_BAR_BUTTON_WIDTH, TITLE_BAR_BUTTON_HEIGHT),
            ));
            button.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
            title_bar.addSubview(&button);
        }
        Ok(())
    })
}

fn configure_titlebar_button(
    mtm: MainThreadMarker,
    system_symbol_name: &str,
    accessibility_description: &str,
    fallback_image: Option<Retained<NSImage>>,
    activate: impl Fn() + 'static,
) -> Option<Retained<NativeTitleBarButton>> {
    let button = NativeTitleBarButton::new(mtm, activate);
    let accessibility_description = NSString::from_str(accessibility_description);
    let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(system_symbol_name),
        Some(&accessibility_description),
    )
    .or(fallback_image)?;
    image.setAccessibilityDescription(Some(&accessibility_description));
    button.setTitle(&NSString::new());
    button.setImage(Some(&image));
    button.setImagePosition(NSCellImagePosition::ImageOnly);
    button.setToolTip(Some(&accessibility_description));
    // SAFETY: `activate:` is implemented by NativeTitleBarButton. NSControl
    // keeps targets weakly, while the title-bar view retains the button itself.
    unsafe {
        button.setTarget(Some(&button));
        button.setAction(Some(sel!(activate:)));
    }
    Some(button)
}

pub(super) fn configure_application_icon() -> Result<()> {
    let mtm = MainThreadMarker::new().context("macOS icon setup must run on the main thread")?;
    let data = NSData::with_bytes(include_bytes!(
        "../../assets/ion/terminal_icon_all_formats/terminal_icon_256.png"
    ));
    let icon = NSImage::initWithData(NSImage::alloc(), &data)
        .context("macOS could not decode the bundled AxSSH icon")?;
    let application = NSApplication::sharedApplication(mtm);
    // SAFETY: `icon` is a valid NSImage retained for the duration of this call;
    // AppKit retains the image when it installs the application icon.
    unsafe { application.setApplicationIconImage(Some(&icon)) };
    Ok(())
}

/// Return AppKit's aggregate physical modifier state for the event currently
/// being dispatched. Slint intentionally swaps Command and Control on Apple
/// platforms, and its internal left/right state can miss a `flagsChanged`
/// event; AppKit remains the authoritative source for the current key chord.
pub(super) fn current_modifier_state() -> TerminalModifiers {
    let flags = NSEvent::modifierFlags_class();
    TerminalModifiers {
        alt: flags.contains(NSEventModifierFlags::Option),
        control: flags.contains(NSEventModifierFlags::Control),
        meta: flags.contains(NSEventModifierFlags::Command),
        shift: flags.contains(NSEventModifierFlags::Shift),
    }
}

pub(super) fn configure_application_menu(
    shortcut: &NativeMenuShortcut,
    shortcut_enabled: bool,
    activate: impl Fn(NativeMenuSection) + 'static,
) -> Result<()> {
    let mtm = MainThreadMarker::new().context("macOS menu setup must run on the main thread")?;
    let application = NSApplication::sharedApplication(mtm);
    let main_menu = application
        .mainMenu()
        .context("AppKit application has no main menu")?;
    let application_menu = find_application_menu(&main_menu)?;
    let settings_title = NSString::from_str("Settings...");
    let settings_ellipsis_title = NSString::from_str("Settings…");
    let about_item = find_about_item(&application_menu);
    let target = NativeMenuTarget::new(mtm, activate);

    if let Some(about_item) = &about_item {
        bind_menu_item(about_item, &target, sel!(showAbout:));
    }

    let key_equivalent = NSString::from_str(&native_key_equivalent(&shortcut.key)?);
    let settings_item = match application_menu
        .itemWithTitle(&settings_title)
        .or_else(|| application_menu.itemWithTitle(&settings_ellipsis_title))
    {
        Some(item) => item,
        None => {
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
            let insert_index = about_item
                .as_ref()
                .map(|item| application_menu.indexOfItem(item).saturating_add(1))
                .unwrap_or(0);
            application_menu.insertItem_atIndex(&item, insert_index);
            item
        }
    };
    settings_item.setKeyEquivalent(&key_equivalent);
    settings_item.setKeyEquivalentModifierMask(native_modifier_mask(shortcut.modifiers));
    settings_item.setEnabled(shortcut_enabled);
    bind_menu_item(&settings_item, &target, sel!(openSettings:));
    Ok(())
}

fn find_application_menu(main_menu: &NSMenu) -> Result<Retained<NSMenu>> {
    let app_title = NSString::from_str("App");
    let mut named_submenu = None;
    let item_count = main_menu.numberOfItems().max(0);

    for index in 0..item_count {
        let Some(item) = main_menu.itemAtIndex(index) else {
            continue;
        };
        let Some(submenu) = item.submenu() else {
            continue;
        };
        if submenu.title().isEqualToString(&app_title) {
            named_submenu = Some(submenu.clone());
        }
        if find_about_item(&submenu).is_some() {
            return Ok(submenu);
        }
    }

    named_submenu.context("AppKit main menu has no application submenu")
}

fn find_about_item(menu: &NSMenu) -> Option<Retained<NSMenuItem>> {
    let item_count = menu.numberOfItems().max(0);
    for index in 0..item_count {
        let Some(item) = menu.itemAtIndex(index) else {
            continue;
        };
        if item.title().to_string().starts_with("About") {
            return Some(item);
        }
    }
    None
}

fn native_modifier_mask(modifiers: TerminalModifiers) -> NSEventModifierFlags {
    let mut mask = NSEventModifierFlags::empty();
    if modifiers.meta {
        mask |= NSEventModifierFlags::Command;
    }
    if modifiers.control {
        mask |= NSEventModifierFlags::Control;
    }
    if modifiers.alt {
        mask |= NSEventModifierFlags::Option;
    }
    if modifiers.shift {
        mask |= NSEventModifierFlags::Shift;
    }
    mask
}

fn native_key_equivalent(key: &str) -> Result<String> {
    let value = match key {
        "Backspace" => "\u{0008}".to_owned(),
        "Tab" | "Backtab" => "\t".to_owned(),
        "Enter" | "Return" => "\r".to_owned(),
        "Escape" => "\u{001b}".to_owned(),
        "Delete" => function_key(NSDeleteFunctionKey)?,
        "Space" => " ".to_owned(),
        "ArrowUp" | "UpArrow" => function_key(NSUpArrowFunctionKey)?,
        "ArrowDown" | "DownArrow" => function_key(NSDownArrowFunctionKey)?,
        "ArrowLeft" | "LeftArrow" => function_key(NSLeftArrowFunctionKey)?,
        "ArrowRight" | "RightArrow" => function_key(NSRightArrowFunctionKey)?,
        "Insert" => function_key(NSInsertFunctionKey)?,
        "Home" => function_key(NSHomeFunctionKey)?,
        "End" => function_key(NSEndFunctionKey)?,
        "PageUp" => function_key(NSPageUpFunctionKey)?,
        "PageDown" => function_key(NSPageDownFunctionKey)?,
        "Plus" => "+".to_owned(),
        "Comma" => ",".to_owned(),
        key if key
            .strip_prefix('F')
            .and_then(|number| number.parse::<u32>().ok())
            .is_some_and(|number| (1..=12).contains(&number)) =>
        {
            let number = key[1..]
                .parse::<u32>()
                .context("invalid function-key shortcut")?;
            function_key(NSF1FunctionKey + number - 1)?
        }
        key if key.chars().count() == 1 => key.to_lowercase(),
        _ => anyhow::bail!("unsupported macOS menu key"),
    };
    Ok(value)
}

fn function_key(code: u32) -> Result<String> {
    char::from_u32(code)
        .map(|key| key.to_string())
        .context("invalid AppKit function-key code")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_settings_shortcut_keys_to_appkit_equivalents() {
        assert_eq!(native_key_equivalent(",").unwrap(), ",");
        assert_eq!(native_key_equivalent("Enter").unwrap(), "\r");
        assert_eq!(
            native_key_equivalent("ArrowUp").unwrap(),
            char::from_u32(NSUpArrowFunctionKey).unwrap().to_string()
        );
        assert_eq!(
            native_key_equivalent("F12").unwrap(),
            char::from_u32(NSF1FunctionKey + 11).unwrap().to_string()
        );
        assert!(native_key_equivalent("NotAKey").is_err());
    }

    #[test]
    fn converts_physical_modifiers_to_appkit_flags() {
        let mask = native_modifier_mask(TerminalModifiers {
            alt: true,
            control: true,
            meta: true,
            shift: true,
        });
        assert!(mask.contains(NSEventModifierFlags::Option));
        assert!(mask.contains(NSEventModifierFlags::Control));
        assert!(mask.contains(NSEventModifierFlags::Command));
        assert!(mask.contains(NSEventModifierFlags::Shift));
    }
}
