use super::*;
use i_slint_core::accessibility::AccessibleStringProperty;
use i_slint_core::item_tree::ItemRc;
use i_slint_core::lengths::LogicalPoint;
use i_slint_core::window::WindowInner;
use slint::platform::{Key, WindowEvent};
use std::cell::{Cell, RefCell};
use std::ops::ControlFlow;
use std::rc::Rc;

fn connecting_pane(id: Uuid) -> TerminalPaneView {
    let mut pane = terminal_pane_view(id, 0.0, 1.0);
    pane.terminal.content_columns = 10;
    pane.terminal.font_family = "JetBrains Mono".into();
    pane.terminal.font_size = 14.0;
    pane.terminal.line_height_percent = 100;
    pane.terminal.copy_selection_shortcut = "Ctrl+Shift+C".into();
    pane.terminal.paste_shortcut = "Ctrl+Shift+V".into();
    pane.terminal.select_all_shortcut = "Ctrl+Shift+A".into();
    pane
}

fn action_item(harness: &LayoutTestHarness, label: &str) -> ItemRc {
    ItemRc::new_root(WindowInner::from_pub(harness.ui.window()).component())
        .visit_descendants(|item| {
            if item.is_visible()
                && item
                    .accessible_string_property(AccessibleStringProperty::Label)
                    .as_deref()
                    == Some(label)
            {
                return ControlFlow::Break(item.clone());
            }
            ControlFlow::Continue(())
        })
        .expect("visible sidebar action")
}

#[test]
fn connecting_terminals_report_real_geometry_in_main_and_detached_windows() {
    for detached in [false, true] {
        std::thread::spawn(move || {
            let harness = LayoutTestHarness::new();
            let ui = &harness.ui;
            let id = Uuid::from_u128(1);
            ui.set_detached_window(detached);
            ui.set_active_tab_kind("terminal".into());
            ui.set_terminal_panes(ModelRc::new(VecModel::from(vec![connecting_pane(id)])));
            let requests = Rc::new(RefCell::new(Vec::new()));
            let received = requests.clone();
            ui.on_resize_terminal(move |tab_id, columns, rows, _, _| {
                assert_eq!(tab_id, id.to_string());
                received.borrow_mut().push((columns, rows));
            });
            ui.show().expect("connecting terminal window");
            harness.window.set_size(slint::PhysicalSize::new(1180, 740));
            harness.settle();
            let initial = *requests.borrow().last().expect("resize before connection");
            assert!(initial.0 > 10 && initial.1 > 3, "{initial:?}");
            requests.borrow_mut().clear();
            harness.window.set_size(slint::PhysicalSize::new(900, 600));
            harness.settle();
            let resized = *requests.borrow().last().expect("resize while connecting");
            assert!(resized.0 > 10 && resized.0 < initial.0, "{resized:?}");
            assert!(resized.1 > 3 && resized.1 < initial.1, "{resized:?}");
            ui.hide().expect("hide connecting terminal window");
        })
        .join()
        .expect("connecting terminal geometry regression");
    }
}

#[test]
fn sidebar_connection_moves_focus_before_login_and_does_not_buffer_enter() {
    for collapsed in [false, true] {
        std::thread::spawn(move || {
            let harness = LayoutTestHarness::new();
            let ui = &harness.ui;
            ui.set_session_filter("Server".into());
            ui.set_sessions(ModelRc::new(VecModel::from(vec![SessionGroupRow {
                name: "Servers".into(),
                group_name: "Servers".into(),
                profiles: ModelRc::new(VecModel::from(vec![SessionProfileRow {
                    id: "server".into(),
                    name: "Server".into(),
                    ..Default::default()
                }])),
            }])));
            let connections = Rc::new(Cell::new(0));
            let opened = connections.clone();
            let weak = ui.as_weak();
            ui.on_connect_session(move |profile_id| {
                assert_eq!(profile_id, "server");
                opened.set(opened.get() + 1);
                let ui = weak.upgrade().expect("connection window");
                let id = Uuid::from_u128(opened.get());
                ui.set_active_tab_id(id.to_string().into());
                ui.set_active_tab_kind("terminal".into());
                ui.set_terminal_panes(ModelRc::new(VecModel::from(vec![connecting_pane(id)])));
            });
            let inputs = Rc::new(Cell::new(0));
            let received = inputs.clone();
            ui.on_terminal_key_direct(|_| true);
            ui.on_terminal_key(move |_| {
                received.set(received.get() + 1);
                true
            });
            ui.show().expect("sidebar connection window");
            harness.window.set_size(slint::PhysicalSize::new(1180, 740));
            harness.settle();
            if collapsed {
                let toggle = action_item(&harness, "Collapse session sidebar");
                let size = toggle.geometry().size;
                let point =
                    toggle.map_to_window(LogicalPoint::new(size.width / 2.0, size.height / 2.0));
                harness.click(slint::LogicalPosition::new(point.x, point.y));
            }
            harness.settle();
            let server = action_item(&harness, "Connect to Server");
            let size = server.geometry().size;
            let point =
                server.map_to_window(LogicalPoint::new(size.width / 2.0, size.height / 2.0));
            let position = slint::LogicalPosition::new(point.x, point.y);
            harness.click(position);
            assert_eq!(connections.get(), 1);
            for _ in 0..5 {
                harness.press_key(Key::Return);
            }
            ui.window()
                .dispatch_event(WindowEvent::KeyPressed { text: "x".into() });
            ui.window()
                .dispatch_event(WindowEvent::KeyReleased { text: "x".into() });
            harness.settle();
            assert_eq!(
                connections.get(),
                1,
                "Enter belongs to the pending terminal"
            );
            assert_eq!(inputs.get(), 0, "no input before login");

            ui.set_host_key_dialog_open(true);
            harness.settle();
            assert!(ui.get_modal_open());
            let focus = WindowInner::from_pub(ui.window())
                .focus_item
                .borrow()
                .upgrade();
            ui.set_window_active(false);
            ui.set_window_active(true);
            harness.settle();
            assert_eq!(
                WindowInner::from_pub(ui.window())
                    .focus_item
                    .borrow()
                    .upgrade(),
                focus,
                "pending terminal must not take focus from host-key confirmation"
            );
            ui.set_host_key_dialog_open(false);
            harness.settle();

            let mut pane = connecting_pane(Uuid::from_u128(1));
            pane.terminal.connected = true;
            ui.set_terminal_panes(ModelRc::new(VecModel::from(vec![pane])));
            harness.settle();
            assert!(
                WindowInner::from_pub(ui.window())
                    .focus_item
                    .borrow()
                    .upgrade()
                    .is_some_and(|item| item
                        .downcast::<i_slint_core::items::TextInput>()
                        .is_some()),
                "connection hands focus to the native IME input"
            );
            assert_eq!(inputs.get(), 0, "waiting input must not replay after login");
            harness.press_key(Key::Return);
            assert_eq!(inputs.get(), 1, "connected terminal receives new Enter");
            assert_eq!(connections.get(), 1);
            harness.click(position);
            assert_eq!(
                connections.get(),
                2,
                "explicit second connection stays supported"
            );
            ui.hide().expect("hide sidebar connection window");
        })
        .join()
        .expect("pending terminal focus regression");
    }
}
