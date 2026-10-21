use super::*;
use crate::app::panes::PanePlacement;
use crate::app::window_router::WindowTerminalPane;
use ax_ssh::terminal::TerminalModel;
use i_slint_core::input::{InternalKeyEvent, KeyEventType};
use i_slint_core::item_tree::ItemRc;
use i_slint_core::items::{
    BasicBorderRectangle, BorderRectangle, ComplexText, KeyEvent, TextInput,
};
use i_slint_core::window::WindowInner;
use slint::platform::WindowEvent;
use std::cell::RefCell;
use std::ops::ControlFlow;
use std::rc::Rc;
use std::time::Duration;

fn window_pane(id: Uuid, terminal: &mut TerminalModel) -> WindowTerminalPane {
    WindowTerminalPane {
        placement: PanePlacement {
            tab_id: id,
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            focused: true,
        },
        snapshot: ActiveTabSnapshot {
            id: Some(id),
            kind: "terminal",
            connected: true,
            terminal: Some(terminal.snapshot()),
            ..Default::default()
        },
        closable: false,
    }
}

fn mount(harness: &LayoutTestHarness, detached: bool, id: Uuid, terminal: &mut TerminalModel) {
    let ui = &harness.ui;
    apply_settings_to_component(ui, &AppSettings::default());
    ui.set_detached_window(detached);
    ui.set_active_tab_kind("terminal".into());
    ui.set_active_tab_id(id.to_string().into());
    apply_terminal_panes(ui, vec![window_pane(id, terminal)], vec![]);
    ui.show().expect("terminal cursor window");
    harness.window.set_size(slint::PhysicalSize::new(900, 600));
    harness.settle();
}

fn cursor_item(harness: &LayoutTestHarness, color: slint::Color) -> ItemRc {
    let expected = slint::Brush::SolidColor(color);
    ItemRc::new_root(WindowInner::from_pub(harness.ui.window()).component())
        .visit_descendants(|item| {
            let border = if let Some(rect) = item.downcast::<BasicBorderRectangle>() {
                BasicBorderRectangle::FIELD_OFFSETS
                    .border_color()
                    .apply_pin(rect.as_pin_ref())
                    .get()
            } else if let Some(rect) = item.downcast::<BorderRectangle>() {
                BorderRectangle::FIELD_OFFSETS
                    .border_color()
                    .apply_pin(rect.as_pin_ref())
                    .get()
            } else {
                return ControlFlow::Continue(());
            };
            if border == expected {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
        .expect("cursor with protocol-selected color")
}

#[test]
fn cursor_only_protocol_updates_reach_the_grid_without_replacing_models_or_focus() {
    for detached in [false, true] {
        std::thread::spawn(move || {
            let harness = LayoutTestHarness::new();
            let ui = &harness.ui;
            let id = Uuid::from_u128(1);
            let mut terminal = TerminalModel::new(80, 24, 10);
            terminal.process(b"\x1b]12;#123456\x07\x1b[2 q");
            mount(&harness, detached, id, &mut terminal);
            let before = ui.get_terminal_panes().row_data(0).expect("pane");
            let focused = WindowInner::from_pub(ui.window())
                .focus_item
                .borrow()
                .upgrade();
            let cursor = cursor_item(&harness, slint::Color::from_rgb_u8(0x12, 0x34, 0x56));
            let block = cursor.geometry().size;
            assert!(cursor.is_visible());

            terminal.process(b"\x1b[4 q");
            assert_eq!(
                apply_terminal_pane_updates(ui, vec![window_pane(id, &mut terminal)]),
                1
            );
            harness.settle();
            assert!(
                cursor.geometry().size.height < block.height / 2.0,
                "underline height"
            );

            terminal.process(b"\x1b[6 q");
            apply_terminal_pane_updates(ui, vec![window_pane(id, &mut terminal)]);
            harness.settle();
            assert!(
                cursor.geometry().size.width < block.width / 2.0,
                "beam width"
            );
            assert_eq!(cursor.geometry().size.height, block.height);

            terminal.process(b"\x1b]12;#654321\x07");
            apply_terminal_pane_updates(ui, vec![window_pane(id, &mut terminal)]);
            harness.settle();
            let recolored = cursor_item(&harness, slint::Color::from_rgb_u8(0x65, 0x43, 0x21));
            assert_eq!(cursor, recolored, "cursor item identity stays stable");

            terminal.process(b"\x1b[?12h");
            apply_terminal_pane_updates(ui, vec![window_pane(id, &mut terminal)]);
            harness.settle();
            let visibility = cursor.is_visible();
            harness
                .time
                .set(harness.time.get() + Duration::from_millis(540));
            slint::platform::update_timers_and_animations();
            assert_ne!(
                cursor.is_visible(),
                visibility,
                "protocol blink request reaches timer"
            );
            terminal.process(b"\x1b[?12l");
            apply_terminal_pane_updates(ui, vec![window_pane(id, &mut terminal)]);
            harness.settle();
            assert!(
                cursor.is_visible(),
                "steady cursor is visible at either old blink phase"
            );

            let after = ui.get_terminal_panes().row_data(0).expect("pane");
            assert_eq!(before.terminal.cursor_state, after.terminal.cursor_state);
            assert_eq!(before.terminal.render_lines, after.terminal.render_lines);
            assert_eq!(
                focused,
                WindowInner::from_pub(ui.window())
                    .focus_item
                    .borrow()
                    .upgrade()
            );
            ui.hide().expect("hide cursor window");
        })
        .join()
        .expect("cursor protocol update regression");
    }
}

fn visible_text(ui: &AppWindow, expected: &str) -> bool {
    ItemRc::new_root(WindowInner::from_pub(ui.window()).component())
        .visit_descendants(|item| {
            if item.is_visible()
                && let Some(text) = item.downcast::<ComplexText>()
                && ComplexText::FIELD_OFFSETS
                    .text()
                    .apply_pin(text.as_pin_ref())
                    .get()
                    == expected
            {
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(())
        })
        .is_some()
}

fn preedit(ui: &AppWindow, text: &str) {
    ui.window()
        .dispatch_event(WindowEvent::internal(InternalKeyEvent {
            event_type: KeyEventType::UpdateComposition,
            preedit_text: text.into(),
            preedit_selection: Some(0..text.len() as i32),
            ..Default::default()
        }));
}

#[test]
fn local_preedit_stays_visible_with_a_hidden_remote_cursor_and_only_commits_send_text() {
    for detached in [false, true] {
        std::thread::spawn(move || {
            let harness = LayoutTestHarness::new();
            let ui = &harness.ui;
            let id = Uuid::from_u128(1);
            let mut terminal = TerminalModel::new(80, 24, 10);
            terminal.process(b"\x1b[?25l\x1b[3;5H");
            mount(&harness, detached, id, &mut terminal);
            let received = Rc::new(RefCell::new(Vec::new()));
            let sent = received.clone();
            ui.on_terminal_key(move |request| {
                sent.borrow_mut().push(request.event.text.to_string());
                true
            });
            let focused = WindowInner::from_pub(ui.window())
                .focus_item
                .borrow()
                .upgrade()
                .expect("terminal input focus");
            let input = focused.downcast::<TextInput>().expect("IME proxy");
            preedit(ui, "zhong");
            harness.settle();
            assert_eq!(input.as_pin_ref().preedit_text().as_str(), "zhong");
            assert!(
                visible_text(ui, "zhong"),
                "local pinyin remains visible with DECTCEM off"
            );
            assert!(
                received.borrow().is_empty(),
                "preedit never reaches the terminal worker"
            );
            preedit(ui, "");
            harness.settle();
            assert!(!visible_text(ui, "zhong"), "cancel removes the overlay");
            assert!(received.borrow().is_empty());

            preedit(ui, "zhong");
            let mut commit = KeyEvent::default();
            commit.text = "中".into();
            ui.window()
                .dispatch_event(WindowEvent::internal(InternalKeyEvent {
                    event_type: KeyEventType::CommitComposition,
                    key_event: commit,
                    ..Default::default()
                }));
            harness.settle();
            assert_eq!(&*received.borrow(), &["中"]);
            assert!(!visible_text(ui, "zhong"));
            assert!(input.as_pin_ref().text().is_empty());
            assert!(
                !terminal.cursor_visibility_requested(),
                "local IME leaves remote cursor hidden"
            );
            ui.hide().expect("hide preedit window");
        })
        .join()
        .expect("hidden cursor preedit regression");
    }
}
