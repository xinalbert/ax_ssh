use super::*;
use i_slint_core::input::{InternalKeyEvent, KeyEventType};
use i_slint_core::items::{KeyEvent, TextInput};
use i_slint_core::window::WindowInner;
use slint::platform::{Key, WindowEvent};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn key(ui: &AppWindow, text: SharedString) {
    ui.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    ui.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

fn chord(ui: &AppWindow, modifier: Key, text: &str, shift: bool) {
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: modifier.into(),
    });
    if shift {
        ui.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::Shift.into(),
        });
    }
    // The real Winit hook records this before dispatch; the headless platform
    // has no physical key state for the macOS fallback to query.
    let control = if cfg!(target_os = "macos") {
        modifier == Key::Meta
    } else {
        modifier == Key::Control
    };
    let meta = if cfg!(target_os = "macos") {
        modifier == Key::Control
    } else {
        modifier == Key::Meta
    };
    crate::app::input::update_native_event_modifiers(false, control, meta, shift);
    key(ui, text.into());
    if shift {
        ui.window().dispatch_event(WindowEvent::KeyReleased {
            text: Key::Shift.into(),
        });
    }
    ui.window().dispatch_event(WindowEvent::KeyReleased {
        text: modifier.into(),
    });
    crate::app::input::clear_native_event_modifiers();
}

#[test]
fn main_terminal_proxy_keeps_local_undo_out_of_cleared_input() {
    terminal_proxy_undo(false);
}

#[test]
fn detached_terminal_proxy_keeps_local_undo_out_of_cleared_input() {
    terminal_proxy_undo(true);
}

fn terminal_proxy_undo(detached: bool) {
    std::thread::spawn(move || {
        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        let id = Uuid::from_u128(1);
        ui.set_apple_platform(cfg!(target_os = "macos"));
        ui.set_detached_window(detached);
        apply_settings_to_component(ui, &AppSettings::default());
        ui.set_active_tab_kind("terminal".into());
        ui.set_active_tab_id(id.to_string().into());
        let mut pane = terminal_pane_view(id, 0.0, 1.0);
        pane.terminal.connected = true;
        pane.terminal.font_family = "JetBrains Mono".into();
        pane.terminal.font_size = 14.0;
        pane.terminal.line_height_percent = 100;
        pane.terminal.content_columns = 80;
        pane.terminal.copy_selection_shortcut = ui.get_copy_selection_shortcut();
        pane.terminal.paste_shortcut = ui.get_paste_shortcut();
        pane.terminal.select_all_shortcut = ui.get_select_all_shortcut();
        ui.set_terminal_panes(ModelRc::new(VecModel::from(vec![pane.clone()])));
        ui.on_format_shortcut(|event| {
            let input = crate::app::normalized_keyboard_input_from_slint_event(&event);
            crate::app::input::format_shortcut_event_with_current_modifiers(&input).into()
        });
        let direct_enabled = Rc::new(Cell::new(true));
        let direct = direct_enabled.clone();
        ui.on_terminal_key_direct(move |request| {
            let input = crate::app::normalized_keyboard_input_from_terminal_direct_event(&request);
            direct.get()
                && crate::app::input::terminal_key_is_direct_for_input(
                    &input,
                    request.option_as_meta,
                    request.preedit_active,
                )
        });
        let input_enabled = Rc::new(Cell::new(true));
        let enabled = input_enabled.clone();
        let bytes = Rc::new(RefCell::new(Vec::new()));
        let received = bytes.clone();
        ui.on_terminal_key(move |request| {
            assert_eq!(request.terminal_id.as_str(), id.to_string());
            if !enabled.get() {
                return false;
            }
            let input = crate::app::normalized_keyboard_input_from_slint_event(&request.event);
            let Some(key) = crate::app::input::terminal_key_from_normalized_input(&input) else {
                return false;
            };
            let Some(encoded) = ax_ssh::terminal::encode_key(&key, input.modifiers, false) else {
                return false;
            };
            received.borrow_mut().push(encoded);
            true
        });
        ui.show().expect("terminal input window");
        harness.window.set_size(slint::PhysicalSize::new(900, 600));
        harness.settle();
        let focused = WindowInner::from_pub(ui.window())
            .focus_item
            .borrow()
            .upgrade()
            .expect("terminal input focus");
        let input = focused.downcast::<TextInput>().expect("IME proxy");
        let input = input.as_pin_ref();

        key(ui, "fixture".into());
        let mut composition = KeyEvent::default();
        composition.text = "中文".into();
        ui.window()
            .dispatch_event(WindowEvent::internal(InternalKeyEvent {
                event_type: KeyEventType::CommitComposition,
                key_event: composition,
                ..Default::default()
            }));
        harness.settle();
        assert_eq!(
            &*bytes.borrow(),
            &[b"fixture".to_vec(), "中文".as_bytes().to_vec()]
        );
        assert!(
            input.text().is_empty(),
            "committed input is sent and cleared"
        );

        if cfg!(target_os = "macos") {
            chord(ui, Key::Control, "z", false);
            chord(ui, Key::Control, "z", true);
            harness.settle();
            assert!(input.text().is_empty());
            assert_eq!(bytes.borrow().len(), 2, "Command is not terminal Control");
        }

        // Both an unclassified key and a rejected terminal input must stop at
        // the proxy instead of falling through to TextInput's editing history.
        input_enabled.set(false);
        for direct in [true, false] {
            direct_enabled.set(direct);
            for (text, shift) in [("z", false), ("z", true), ("Z", true), ("y", false)] {
                chord(ui, Key::Control, text, shift);
                harness.settle();
                assert!(input.text().is_empty());
                assert_eq!(bytes.borrow().len(), 2, "no stale text may be resent");
            }
        }

        input_enabled.set(true);
        direct_enabled.set(true);
        bytes.borrow_mut().clear();
        let physical_control = if cfg!(target_os = "macos") {
            Key::Meta
        } else {
            Key::Control
        };
        chord(ui, physical_control, "z", false);
        chord(ui, physical_control, "y", false);
        assert_eq!(
            &*bytes.borrow(),
            &[vec![0x1a], vec![0x19]],
            "physical Control remains terminal input"
        );

        let paste = KeyboardEvent {
            text: "z".into(),
            logical_key: "z".into(),
            control: true,
            ..Default::default()
        };
        pane.terminal.paste_shortcut = ui.invoke_format_shortcut(paste);
        ui.get_terminal_panes().set_row_data(0, pane);
        let actions = Rc::new(RefCell::new(Vec::new()));
        let received = actions.clone();
        ui.on_terminal_pane_command(move |tab_id, action| {
            assert_eq!(tab_id.as_str(), id.to_string());
            received.borrow_mut().push(action.to_string());
            true
        });
        harness.settle();
        chord(ui, Key::Control, "z", false);
        harness.settle();
        assert_eq!(&*actions.borrow(), &["paste-clipboard"]);
        assert_eq!(bytes.borrow().len(), 2, "configured paste is consumed once");
        ui.hide().expect("hide terminal input window");
    })
    .join()
    .expect("terminal proxy undo regression");
}
