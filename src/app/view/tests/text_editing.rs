use super::*;
use i_slint_core::item_tree::{ItemRc, ParentItemTraversalMode};
use i_slint_core::items::{FocusReason, InputType, TextInput};
use i_slint_core::window::WindowInner;
use slint::platform::{Key, WindowAdapter, WindowEvent};
use std::ops::ControlFlow;

fn send_chord(harness: &LayoutTestHarness, modifier: Key, text: &str, shift: bool) {
    let window = harness.ui.window();
    window.dispatch_event(WindowEvent::KeyPressed {
        text: modifier.into(),
    });
    if shift {
        window.dispatch_event(WindowEvent::KeyPressed {
            text: Key::Shift.into(),
        });
    }
    window.dispatch_event(WindowEvent::KeyPressed { text: text.into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: text.into() });
    if shift {
        window.dispatch_event(WindowEvent::KeyReleased {
            text: Key::Shift.into(),
        });
    }
    window.dispatch_event(WindowEvent::KeyReleased {
        text: modifier.into(),
    });
    harness.settle();
}

fn physical_control() -> Key {
    if cfg!(target_os = "macos") {
        Key::Meta
    } else {
        Key::Control
    }
}

fn find_input(
    harness: &LayoutTestHarness,
    visible_only: bool,
    matches: impl Fn(std::pin::Pin<&TextInput>) -> bool,
) -> ItemRc {
    let root = ItemRc::new_root(WindowInner::from_pub(harness.ui.window()).component());
    root.visit_descendants(|item| {
        if (!visible_only || item.is_visible())
            && let Some(input) = item.downcast::<TextInput>()
            && matches(input.as_pin_ref())
        {
            return ControlFlow::Break(item.clone());
        }
        ControlFlow::Continue(())
    })
    .expect("matching input must be visible")
}

fn focus_input(harness: &LayoutTestHarness, item: &ItemRc) {
    WindowInner::from_pub(harness.ui.window()).set_focus_item(
        item,
        true,
        FocusReason::TabNavigation,
    );
    harness.settle();
}

#[test]
fn main_address_input_uses_platform_shortcuts_and_preserves_unicode_undo() {
    address_input_shortcuts(false);
}

#[test]
fn detached_address_input_uses_platform_shortcuts_and_preserves_unicode_undo() {
    address_input_shortcuts(true);
}

fn address_input_shortcuts(detached: bool) {
    std::thread::spawn(move || {
        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        ui.set_apple_platform(cfg!(target_os = "macos"));
        ui.set_detached_window(detached);
        ui.set_active_tab_kind("sftp".into());
        ui.set_sftp_available(true);
        ui.set_sftp_open(true);
        ui.set_local_sftp_loaded(true);
        ui.set_sftp_path("/remote".into());
        let path = "/目录/👩🏽‍💻/e\u{301}/results";
        ui.set_local_sftp_path(path.into());
        ui.show().expect("address input window");
        harness.window.set_size(slint::PhysicalSize::new(1180, 740));
        harness.settle();
        let item = find_input(&harness, false, |input| input.text().as_str() == path);
        let mut scope = item.clone();
        while !scope.is_visible()
            || scope
                .downcast::<i_slint_core::items::FocusScope>()
                .is_none()
        {
            scope = scope
                .parent_item(ParentItemTraversalMode::StopAtPopups)
                .expect("path focus scope");
        }
        focus_input(&harness, &scope);
        send_chord(&harness, Key::Control, "l", false);
        assert!(
            item.is_visible(),
            "platform path shortcut must start editing"
        );
        let input = item.downcast::<TextInput>().expect("TextInput");
        let input = input.as_pin_ref();
        let adapter: std::rc::Rc<dyn WindowAdapter> = harness.window.clone();
        input.set_selection_offsets(&adapter, &item, path.len() as i32, path.len() as i32);

        send_chord(&harness, Key::Control, "a", false);
        assert_eq!(input.anchor_position_byte_offset(), 0);
        assert_eq!(input.cursor_position_byte_offset(), path.len() as i32);
        send_chord(&harness, Key::Control, "c", false);
        assert_eq!(harness.clipboard.borrow()[0].as_deref(), Some(path));

        if cfg!(target_os = "macos") {
            send_chord(&harness, physical_control(), "a", false);
            assert_eq!(input.cursor_position_byte_offset(), 0);
            assert_eq!(input.anchor_position_byte_offset(), 0);
            send_chord(&harness, physical_control(), "e", true);
            assert_eq!(input.cursor_position_byte_offset(), path.len() as i32);
            assert_eq!(input.anchor_position_byte_offset(), 0);
            send_chord(&harness, physical_control(), "e", false);
            send_chord(&harness, physical_control(), "a", true);
            assert_eq!(input.cursor_position_byte_offset(), 0);
            assert_eq!(input.anchor_position_byte_offset(), path.len() as i32);
            let start = "/目录/".len() as i32;
            let end = start + "👩🏽‍💻".len() as i32;
            input.set_selection_offsets(&adapter, &item, start, start);
            send_chord(&harness, physical_control(), "f", true);
            assert_eq!(input.anchor_position_byte_offset(), start);
            assert_eq!(input.cursor_position_byte_offset(), end);
            send_chord(&harness, physical_control(), "d", false);
            assert_eq!(input.text().as_str(), "/目录//e\u{301}/results");
            send_chord(&harness, Key::Control, "z", false);
            assert_eq!(
                input.text().as_str(),
                path,
                "delete must retain native undo"
            );
            input.set_selection_offsets(&adapter, &item, end, end);
            send_chord(&harness, physical_control(), "b", false);
            assert_eq!(input.cursor_position_byte_offset(), start);

            input.set_selection_offsets(&adapter, &item, end, end);
            input.preedit_text.set("拼".into());
            send_chord(&harness, physical_control(), "a", false);
            assert_eq!(input.cursor_position_byte_offset(), end);
            assert_eq!(input.preedit_text().as_str(), "拼");
            assert_eq!(input.text().as_str(), path);
            input.preedit_text.set("".into());
        } else {
            assert_eq!(input.text().as_str(), path, "Ctrl+A remains select-all");
            input.set_selection_offsets(&adapter, &item, path.len() as i32, path.len() as i32);
            send_chord(
                &harness,
                Key::Control,
                SharedString::from(Key::Home).as_str(),
                false,
            );
            assert_eq!(input.cursor_position_byte_offset(), 0);
            send_chord(
                &harness,
                Key::Control,
                SharedString::from(Key::End).as_str(),
                false,
            );
            assert_eq!(input.cursor_position_byte_offset(), path.len() as i32);
        }
        assert_eq!(input.text().as_str(), path);
        ui.hide().expect("hide address input window");
    })
    .join()
    .expect("address input keyboard regression");
}

#[test]
fn protected_input_keeps_clipboards_private_and_supports_safe_navigation() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        ui.set_password_dialog_open(true);
        ui.set_password_dialog_tab_id("test-prompt".into());
        ui.show().expect("password window");
        harness.window.set_size(slint::PhysicalSize::new(1180, 740));
        harness.settle();
        let item = find_input(&harness, true, |input| {
            input.input_type() == InputType::Password
        });
        focus_input(&harness, &item);
        let input = item.downcast::<TextInput>().expect("password input");
        let input = input.as_pin_ref();
        let adapter: std::rc::Rc<dyn WindowAdapter> = harness.window.clone();
        let secret = "test-only-secret";
        ui.window().dispatch_event(WindowEvent::KeyPressed {
            text: secret.into(),
        });
        ui.window().dispatch_event(WindowEvent::KeyReleased {
            text: secret.into(),
        });
        harness.settle();
        assert_eq!(input.text().as_str(), secret);
        *harness.clipboard.borrow_mut() = [
            Some("clipboard sentinel".into()),
            Some("selection sentinel".into()),
        ];
        for key in ["a", "c", "x"] {
            send_chord(&harness, Key::Control, key, false);
            assert_eq!(input.text().as_str(), secret);
            assert_eq!(
                input.anchor_position_byte_offset(),
                input.cursor_position_byte_offset()
            );
        }
        input.set_selection_offsets(&adapter, &item, 0, secret.len() as i32);
        for key in ["c", "x"] {
            send_chord(&harness, Key::Control, key, false);
            assert_eq!(input.text().as_str(), secret);
        }
        assert_eq!(
            harness.clipboard.borrow()[0].as_deref(),
            Some("clipboard sentinel")
        );
        assert_eq!(
            harness.clipboard.borrow()[1].as_deref(),
            Some("selection sentinel")
        );
        input.set_selection_offsets(&adapter, &item, secret.len() as i32, secret.len() as i32);
        if cfg!(target_os = "macos") {
            send_chord(&harness, physical_control(), "a", true);
            assert_eq!(input.cursor_position_byte_offset(), 0);
            assert_eq!(input.anchor_position_byte_offset(), 0);
            send_chord(&harness, physical_control(), "e", false);
            assert_eq!(input.cursor_position_byte_offset(), secret.len() as i32);
        }
        // Clipboard protection also works without an application callback.
        ui.global::<TextEditing>()
            .on_handle_key(|_| i_slint_core::items::EventResult::Reject);
        input.set_selection_offsets(&adapter, &item, 0, secret.len() as i32);
        send_chord(&harness, Key::Control, "c", false);
        send_chord(&harness, Key::Control, "x", false);
        assert_eq!(input.text().as_str(), secret);
        assert_eq!(
            harness.clipboard.borrow()[0].as_deref(),
            Some("clipboard sentinel")
        );
        ui.hide().expect("hide password window");
    })
    .join()
    .expect("protected input regression");
}

#[test]
fn multiline_input_uses_paragraph_boundaries_and_native_vertical_movement() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        let text = "first\nsecond\nthird";
        ui.set_active_tab_kind("settings".into());
        ui.set_sftp_filter_custom_patterns(text.into());
        ui.invoke_request_settings_section("SFTP".into());
        ui.show().expect("settings window");
        harness.window.set_size(slint::PhysicalSize::new(1180, 900));
        harness.settle();
        let item = find_input(&harness, true, |input| {
            !input.single_line() && input.text().as_str() == text
        });
        focus_input(&harness, &item);
        let input = item.downcast::<TextInput>().expect("multiline input");
        let input = input.as_pin_ref();
        let adapter: std::rc::Rc<dyn WindowAdapter> = harness.window.clone();
        input.set_selection_offsets(&adapter, &item, 1, 1);
        if cfg!(target_os = "macos") {
            send_chord(&harness, physical_control(), "n", false);
            assert_eq!(input.cursor_position_byte_offset(), 7);
            send_chord(&harness, physical_control(), "a", false);
            assert_eq!(input.cursor_position_byte_offset(), 6);
            send_chord(&harness, physical_control(), "e", false);
            assert_eq!(input.cursor_position_byte_offset(), 12);
            send_chord(&harness, physical_control(), "p", false);
            assert_eq!(input.cursor_position_byte_offset(), 5);
        } else {
            send_chord(&harness, Key::Control, "a", false);
            assert_eq!(input.anchor_position_byte_offset(), 0);
            assert_eq!(input.cursor_position_byte_offset(), text.len() as i32);
        }
        assert_eq!(input.text().as_str(), text);
        ui.hide().expect("hide settings window");
    })
    .join()
    .expect("multiline input regression");
}
