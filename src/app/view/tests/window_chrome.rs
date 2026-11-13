use super::*;
use i_slint_core::accessibility::AccessibleStringProperty;
use i_slint_core::item_tree::ItemRc;
use i_slint_core::window::WindowInner;
use slint::platform::{PointerEventButton, WindowAdapter, WindowEvent};
use std::cell::{Cell, RefCell};
use std::ops::ControlFlow;
use std::rc::Rc;

fn control(harness: &LayoutTestHarness, label: &str) -> Option<ItemRc> {
    ItemRc::new_root(WindowInner::from_pub(harness.ui.window()).component()).visit_descendants(
        |item| {
            if item.is_visible()
                && item
                    .accessible_string_property(AccessibleStringProperty::Label)
                    .as_deref()
                    == Some(label)
            {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        },
    )
}

fn control_center(harness: &LayoutTestHarness, label: &str) -> slint::LogicalPosition {
    let item = control(harness, label).unwrap_or_else(|| panic!("missing window action: {label}"));
    let point = item.map_to_window(item.geometry().center());
    slint::LogicalPosition::new(point.x, point.y)
}

#[test]
fn desktop_main_window_drags_from_menu_titlebar_only() {
    window_gestures(false, false);
}

#[test]
fn desktop_detached_window_drags_from_its_titlebar() {
    window_gestures(true, false);
}

#[test]
fn macos_main_window_uses_application_titlebar_gestures() {
    window_gestures(false, true);
}

#[test]
fn macos_detached_window_uses_application_titlebar_gestures() {
    window_gestures(true, true);
}

fn window_gestures(detached: bool, apple: bool) {
    std::thread::spawn(move || {
        let harness = LayoutTestHarness::new();
        slint::select_bundled_translation("en").expect("source UI language");
        let ui = &harness.ui;
        ui.set_apple_platform(apple);
        ui.set_detached_window(detached);
        ui.window().set_maximized(false);
        ui.set_sidebar_width(220);
        ui.set_workspace_tabs(ModelRc::new(VecModel::from(vec![WorkspaceTabRow {
            id: "test-tab".into(),
            title: "Test tab".into(),
            kind: "settings".into(),
            ..Default::default()
        }])));
        let drags = Rc::new(Cell::new(0));
        let received_drags = drags.clone();
        ui.on_start_native_window_drag(move || received_drags.set(received_drags.get() + 1));
        let activated = Rc::new(RefCell::new(Vec::new()));
        let received_tabs = activated.clone();
        ui.on_activate_tab(move |id| received_tabs.borrow_mut().push(id));
        let closes = Rc::new(Cell::new(0));
        let received_closes = closes.clone();
        ui.window().on_close_requested(move || {
            received_closes.set(received_closes.get() + 1);
            slint::CloseRequestResponse::KeepWindowShown
        });
        ui.show().expect("headless window should show");
        harness.window.set_size(slint::PhysicalSize::new(1180, 740));
        harness.settle();
        let window_item = WindowInner::from_pub(ui.window())
            .window_item()
            .expect("application window item");
        assert!(window_item.as_pin_ref().no_frame());
        // The headless backend draws macOS's screen-menu fallback inside the
        // client window. Locate the real control instead of assuming its y.
        let maximize = control_center(&harness, "Maximize Window");
        let click = |x, y, button| {
            let position = slint::LogicalPosition::new(x, y);
            harness
                .window
                .window()
                .dispatch_event(WindowEvent::PointerMoved { position });
            harness
                .window
                .window()
                .dispatch_event(WindowEvent::PointerPressed { position, button });
            harness
                .window
                .window()
                .dispatch_event(WindowEvent::PointerReleased { position, button });
        };
        click(600.0, maximize.y, PointerEventButton::Left);
        assert_eq!(
            drags.get(),
            1,
            "titlebar blank must request native dragging"
        );
        click(600.0, maximize.y, PointerEventButton::Right);
        assert_eq!(drags.get(), 1, "right press must not start window dragging");
        harness.click(maximize);
        assert!(ui.window().is_maximized(), "custom maximize must work");
        harness.click(control_center(&harness, "Restore Window"));
        assert!(!ui.window().is_maximized(), "custom restore must work");
        assert_eq!(drags.get(), 1, "window controls must not start dragging");
        if !detached {
            let tab_y = maximize.y + ui.global::<Theme>().get_desktop_titlebar_height();
            click(270.0, tab_y, PointerEventButton::Left);
            assert_eq!(
                activated.borrow().as_slice(),
                &[SharedString::from("test-tab")]
            );
            click(700.0, tab_y, PointerEventButton::Left);
            assert_eq!(drags.get(), 1, "Tab row must not drag the window");
        }
        harness.click(control_center(&harness, "Close Window"));
        assert_eq!(
            closes.get(),
            1,
            "custom close must use the normal close-request route"
        );
        harness.click(control_center(&harness, "Minimize Window"));
        assert!(ui.window().is_minimized(), "custom minimize must work");
    })
    .join()
    .expect("window gesture test thread should finish");
}

#[test]
fn detached_titlebar_routes_split_and_return_to_the_active_workspace() {
    for apple in [false, true] {
        std::thread::spawn(move || {
            let harness = LayoutTestHarness::new();
            slint::select_bundled_translation("en").expect("source UI language");
            let ui = &harness.ui;
            wire_menu_helpers(ui);
            ui.set_apple_platform(apple);
            ui.set_detached_window(true);
            ui.set_active_tab_id("workspace-tab".into());
            ui.set_active_pane_id("focused-pane".into());
            ui.set_active_tab_kind("terminal".into());
            ui.set_menu_terminal_active(true);
            ui.set_menu_has_active_tab(true);
            let commands = Rc::new(RefCell::new(Vec::new()));
            let received = commands.clone();
            ui.on_terminal_pane_command(move |id, command| {
                received.borrow_mut().push((id, command));
                true
            });
            let returns = Rc::new(RefCell::new(Vec::new()));
            let received = returns.clone();
            ui.on_return_workspace(move |id| received.borrow_mut().push(id));
            ui.show().expect("headless window should show");
            harness.window.set_size(slint::PhysicalSize::new(1180, 740));
            harness.settle();
            let vertical = control_center(&harness, "Split active terminal vertically");
            let horizontal = control_center(&harness, "Split active terminal horizontally");
            assert!(
                horizontal.x > vertical.x,
                "split controls must have separate hit areas: {vertical:?}, {horizontal:?}"
            );
            harness.click(vertical);
            harness.settle();
            harness.click(horizontal);
            assert_eq!(
                commands.borrow().as_slice(),
                &[
                    (
                        SharedString::from("focused-pane"),
                        SharedString::from("split-right")
                    ),
                    (
                        SharedString::from("focused-pane"),
                        SharedString::from("split-down")
                    ),
                ]
            );
            ui.set_active_tab_kind("sftp".into());
            ui.set_menu_terminal_active(false);
            harness.settle();
            assert!(control(&harness, "Split active terminal vertically").is_none());
            harness.click(control_center(&harness, "Return Workspace to Main Window"));
            assert_eq!(
                returns.borrow().as_slice(),
                &[SharedString::from("workspace-tab")]
            );
        })
        .join()
        .expect("detached titlebar action test should finish");
    }
}
