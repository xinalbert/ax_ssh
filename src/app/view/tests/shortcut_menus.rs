use super::*;
use slint::platform::{Key, WindowEvent};
use std::cell::RefCell;
use std::rc::Rc;

fn chord(ui: &AppWindow, text: &str, shift: bool) {
    let window = ui.window();
    window.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
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
        text: Key::Control.into(),
    });
}

fn configure_terminal(ui: &AppWindow, id: Uuid, detached: bool) {
    ui.set_apple_platform(cfg!(target_os = "macos"));
    ui.set_detached_window(detached);
    apply_settings_to_component(ui, &AppSettings::default());
    ui.set_active_tab_kind("terminal".into());
    ui.set_active_tab_id(id.to_string().into());
    ui.set_menu_terminal_active(true);
    ui.set_menu_has_active_tab(true);
    let mut pane = terminal_pane_view(id, 0.0, 1.0);
    pane.terminal.connected = true;
    pane.terminal.font_family = "JetBrains Mono".into();
    pane.terminal.font_size = 14.0;
    pane.terminal.line_height_percent = 100;
    pane.terminal.copy_selection_shortcut = ui.get_copy_selection_shortcut();
    pane.terminal.paste_shortcut = ui.get_paste_shortcut();
    pane.terminal.select_all_shortcut = ui.get_select_all_shortcut();
    pane.terminal.content_columns = 80;
    pane.terminal.render_lines = ModelRc::new(VecModel::from(vec![TerminalRenderLine::default()]));
    ui.set_terminal_panes(ModelRc::new(VecModel::from(vec![pane])));
    ui.on_format_shortcut(|event| {
        let input = super::super::super::normalized_keyboard_input_from_slint_event(&event);
        super::super::super::input::format_shortcut_event_with_current_modifiers(&input).into()
    });
}

#[test]
fn detached_menu_shortcuts_stay_with_their_window_and_respect_modal_state() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        let main = &harness.ui;
        let child = AppWindow::new().expect("separate child component");
        let main_id = Uuid::from_u128(1);
        let child_id = Uuid::from_u128(2);
        configure_terminal(main, main_id, false);
        configure_terminal(&child, child_id, true);
        let actions = Rc::new(RefCell::new(Vec::new()));
        let edits = Rc::new(RefCell::new(Vec::new()));
        let keys = Rc::new(RefCell::new(Vec::new()));
        let cycles = Rc::new(RefCell::new(Vec::new()));
        let new_sessions = Rc::new(RefCell::new(Vec::new()));
        let copies = Rc::new(RefCell::new(Vec::new()));
        for (ui, name) in [(main, "main"), (&child, "child")] {
            let received = actions.clone();
            ui.on_menu_action(move |action| received.borrow_mut().push((name, action)));
            let received = edits.clone();
            ui.on_terminal_pane_command(move |id, command| {
                received.borrow_mut().push((name, id, command));
                true
            });
            let received = keys.clone();
            ui.on_terminal_key(move |request| {
                received.borrow_mut().push((name, request));
                true
            });
            let received = cycles.clone();
            ui.on_cycle_tab(move |forward| received.borrow_mut().push((name, forward)));
            let received = new_sessions.clone();
            ui.on_new_session(move || received.borrow_mut().push(name));
            ui.on_terminal_selection_text(move |_, _, _, _, _| {
                format!("{name}-visible-text").into()
            });
            let received = copies.clone();
            ui.on_write_clipboard(move |text| received.borrow_mut().push((name, text)));
            ui.show().expect("headless menu window");
            ui.window().set_size(slint::PhysicalSize::new(900, 600));
        }
        harness.settle();
        let paste_shift = !cfg!(target_os = "macos");
        for (ui, name, id) in [
            (main, "main", main_id),
            (&child, "child", child_id),
            (main, "main", main_id),
        ] {
            chord(ui, "v", paste_shift);
            harness.settle();
            assert_eq!(
                actions.borrow().last(),
                Some(&(name, "paste-terminal".into()))
            );
            assert_eq!(
                edits.borrow().last(),
                Some(&(name, id.to_string().into(), "paste-clipboard".into()))
            );
        }
        assert_eq!(edits.borrow().len(), 3, "one action per chord");
        assert!(
            keys.borrow().is_empty(),
            "menu paste must not also send typed input"
        );

        chord(&child, "n", false);
        harness.settle();
        assert!(
            new_sessions.borrow().is_empty(),
            "child cannot create an invisible editor"
        );
        chord(main, "n", false);
        harness.settle();
        assert_eq!(new_sessions.borrow().as_slice(), &["main"]);

        child.set_menu_has_multiple_tabs(true);
        chord(&child, "[", true);
        chord(&child, "]", true);
        harness.settle();
        assert_eq!(
            cycles.borrow().as_slice(),
            &[("child", false), ("child", true)]
        );

        let copy_shift = !cfg!(target_os = "macos");
        chord(&child, "a", copy_shift);
        harness.settle();
        assert!(copies.borrow().is_empty(), "selection does not auto-copy");
        chord(&child, "c", copy_shift);
        harness.settle();
        assert_eq!(
            copies.borrow().as_slice(),
            &[("child", "child-visible-text".into())]
        );
        assert_eq!(
            actions.borrow().last(),
            Some(&("child", "copy-terminal".into()))
        );

        child.set_password_dialog_open(true);
        harness.settle();
        actions.borrow_mut().clear();
        chord(&child, "v", paste_shift);
        harness.settle();
        assert!(
            actions.borrow().is_empty(),
            "modal blocks terminal menu actions"
        );
        assert_eq!(edits.borrow().len(), 3);
        child.set_password_dialog_open(false);
        child.set_active_tab_kind("sftp".into());
        child.set_menu_terminal_active(false);
        child.set_sftp_available(true);
        child.set_local_sftp_loaded(true);
        harness.settle();
        chord(&child, "v", paste_shift);
        harness.settle();
        assert!(
            actions.borrow().is_empty(),
            "SFTP leaves editing to its text inputs"
        );
        assert_eq!(edits.borrow().len(), 3);
        child.hide().expect("hide child");
        main.hide().expect("hide main");
    })
    .join()
    .expect("generated menu shortcut routing");
}
