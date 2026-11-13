use super::*;
use i_slint_core::accessibility::AccessibleStringProperty;
use i_slint_core::item_tree::ItemRc;
use i_slint_core::window::WindowInner;
use slint::platform::{PointerEventButton, WindowEvent};
use std::cell::{Cell, RefCell};
use std::ops::ControlFlow;
use std::rc::Rc;

fn tab(harness: &LayoutTestHarness, label: &str) -> ItemRc {
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
        .unwrap_or_else(|| panic!("missing tab {label}"))
}

fn tab_point(harness: &LayoutTestHarness, label: &str) -> slint::LogicalPosition {
    let item = tab(harness, label);
    let mut point = item.geometry().center();
    // The summary's left side excludes the Close/Move buttons.
    point.x = item.geometry().origin.x + 45.0;
    let point = item.map_to_window(point);
    slint::LogicalPosition::new(point.x, point.y)
}

fn press(harness: &LayoutTestHarness, position: slint::LogicalPosition) {
    harness
        .ui
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    harness
        .ui
        .window()
        .dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
}

fn drag(harness: &LayoutTestHarness, from: slint::LogicalPosition, to: slint::LogicalPosition) {
    press(harness, from);
    for step in 1..=12 {
        let ratio = step as f32 / 12.0;
        harness
            .ui
            .window()
            .dispatch_event(WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(
                    from.x + (to.x - from.x) * ratio,
                    from.y + (to.y - from.y) * ratio,
                ),
            });
    }
    harness
        .ui
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position: to,
            button: PointerEventButton::Left,
        });
    harness.settle();
}

fn rows(count: usize) -> Vec<WorkspaceTabRow> {
    (0..count)
        .map(|index| WorkspaceTabRow {
            id: format!("tab-{index}").into(),
            title: format!("Drag tab {index}").into(),
            kind: "settings".into(),
            ..Default::default()
        })
        .collect()
}

#[test]
fn tab_reorder_uses_content_coordinates_with_platform_offsets_and_scroll() {
    for apple in [false, true] {
        for scale in [1.0, 2.0] {
            std::thread::spawn(move || {
                let harness = LayoutTestHarness::new();
                let ui = &harness.ui;
                ui.set_apple_platform(apple);
                ui.set_tab_width(180);
                ui.set_sidebar_width(220);
                ui.set_workspace_tabs(ModelRc::new(VecModel::from(rows(7))));
                ui.set_active_tab_id("tab-0".into());
                let drags = Rc::new(Cell::new(0));
                let received = drags.clone();
                ui.on_start_native_window_drag(move || received.set(received.get() + 1));
                let moves = Rc::new(RefCell::new(Vec::new()));
                let received = moves.clone();
                ui.on_move_tab(move |id, index| received.borrow_mut().push((id, index)));
                ui.window().dispatch_event(WindowEvent::ScaleFactorChanged {
                    scale_factor: scale,
                });
                ui.show().unwrap();
                harness.window.set_size(slint::PhysicalSize::new(
                    (1000.0 * scale) as u32,
                    (600.0 * scale) as u32,
                ));
                harness.settle();

                let first = tab_point(&harness, "Drag tab 0");
                drag(
                    &harness,
                    first,
                    slint::LogicalPosition::new(first.x + 360.0, first.y),
                );
                assert_eq!(
                    moves.borrow().as_slice(),
                    &[(SharedString::from("tab-0"), 2)]
                );

                moves.borrow_mut().clear();
                ui.set_active_tab_id("tab-6".into());
                harness.settle();
                let last = tab_point(&harness, "Drag tab 6");
                assert!(last.x < 1000.0, "active Tab must have scrolled into view");
                drag(
                    &harness,
                    last,
                    slint::LogicalPosition::new(last.x - 360.0, last.y),
                );
                assert_eq!(
                    moves.borrow().as_slice(),
                    &[(SharedString::from("tab-6"), 4)]
                );
                assert_eq!(
                    drags.get(),
                    0,
                    "Tab gestures must never request a window drag"
                );
            })
            .join()
            .expect("platform/scale Tab reorder regression");
        }
    }
}

#[test]
fn macos_leading_clearance_requests_drag_only_inside_its_actual_row() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        ui.set_apple_platform(true);
        ui.set_tab_width(180);
        ui.set_workspace_tabs(ModelRc::new(VecModel::from(rows(1))));
        let drags = Rc::new(Cell::new(0));
        let received = drags.clone();
        ui.on_start_native_window_drag(move || received.set(received.get() + 1));
        ui.show().unwrap();
        harness.window.set_size(slint::PhysicalSize::new(1000, 600));
        harness.settle();
        let first = tab_point(&harness, "Drag tab 0");
        harness.click(slint::LogicalPosition::new(75.0, first.y));
        assert_eq!(
            drags.get(),
            1,
            "leading clearance explicitly owns its window drag"
        );
        harness.click(slint::LogicalPosition::new(
            75.0,
            first.y + ui.global::<Theme>().get_tab_height(),
        ));
        assert_eq!(
            drags.get(),
            1,
            "sidebar below the actual titlebar is not a drag region"
        );
        harness.click(slint::LogicalPosition::new(600.0, first.y));
        assert_eq!(
            drags.get(),
            2,
            "unoccupied Tab strip explicitly owns its window drag"
        );
        drag(&harness, first, slint::LogicalPosition::new(40.0, first.y));
        assert_eq!(
            drags.get(),
            2,
            "crossing into the clearance must keep the Tab's pointer grab"
        );
    })
    .join()
    .expect("macOS titlebar hit regions");
}
