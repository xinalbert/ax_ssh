use super::*;

#[test]
fn windows_local_path_presentation_keeps_original_state_and_remote_path_routes() {
    for detached in [false, true] {
        std::thread::spawn(move || {
            use i_slint_core::accessibility::AccessibleStringProperty;
            use i_slint_core::item_tree::ItemRc;
            use i_slint_core::window::WindowInner;
            use std::cell::RefCell;
            use std::ops::ControlFlow;
            use std::rc::Rc;

            let harness = LayoutTestHarness::new();
            let ui = &harness.ui;
            wire_menu_helpers(ui);
            ui.set_detached_window(detached);
            ui.set_active_tab_kind("sftp".into());
            ui.set_sftp_available(true);
            ui.set_sftp_open(true);
            ui.set_local_sftp_loaded(true);
            let navigated = Rc::new(RefCell::new(Vec::new()));
            let received = navigated.clone();
            ui.on_list_local_sftp_directory(move |path| received.borrow_mut().push(path));
            let copied = Rc::new(RefCell::new(Vec::new()));
            let received = copied.clone();
            ui.on_write_clipboard(move |path| received.borrow_mut().push(path));
            ui.show().expect("headless Windows path window");
            harness.window.set_size(slint::PhysicalSize::new(1800, 740));

            for (raw, readable) in [
                (r"\\?\D:\software\Xming", r"D:\software\Xming"),
                (r"\\?\UNC\server\share\研究", r"\\server\share\研究"),
            ] {
                ui.set_local_sftp_path(raw.into());
                ui.set_sftp_path(raw.into());
                harness.settle();
                let local = ui.invoke_native_sftp_local_drop_region();
                let action = |label: &str, local_action: bool| {
                    ItemRc::new_root(WindowInner::from_pub(ui.window()).component())
                        .visit_descendants(|item| {
                            if item.is_visible()
                                && item
                                    .accessible_string_property(AccessibleStringProperty::Label)
                                    .as_deref()
                                    == Some(label)
                                && (item.map_to_window(item.geometry().origin).x >= local.x)
                                    == local_action
                            {
                                return ControlFlow::Break(item.clone());
                            }
                            ControlFlow::Continue(())
                        })
                        .expect("visible path action")
                };
                let click = |item: ItemRc| {
                    let point = item.map_to_window(item.geometry().center());
                    harness.click(slint::LogicalPosition::new(point.x, point.y));
                };
                for expected in [raw, readable] {
                    let found = ItemRc::new_root(WindowInner::from_pub(ui.window()).component())
                        .visit_descendants(|item| {
                            if item.is_visible()
                                && let Some(text) =
                                    item.downcast::<i_slint_core::items::ComplexText>()
                                && i_slint_core::items::ComplexText::FIELD_OFFSETS
                                    .text()
                                    .apply_pin(text.as_pin_ref())
                                    .get()
                                    == expected
                            {
                                return ControlFlow::Break(());
                            }
                            ControlFlow::Continue(())
                        });
                    assert!(found.is_some(), "visible path must contain {expected:?}");
                }
                // Copy buttons and editor use the same complete readable path.
                click(action("Copy current path", true));
                assert_eq!(
                    copied.borrow().last().map(SharedString::as_str),
                    Some(readable)
                );
                click(action(&format!("Edit path: {readable}"), true));
                harness.press_key(slint::platform::Key::Return);
                assert_eq!(
                    navigated.borrow().last().map(SharedString::as_str),
                    Some(readable)
                );

                click(action("Path: Settings", true));
                harness.press_key(slint::platform::Key::DownArrow);
                harness.press_key(slint::platform::Key::Return);
                assert_eq!(
                    copied.borrow().last().map(SharedString::as_str),
                    Some(readable)
                );

                // Existing operational callbacks retain their canonical identity.
                click(action("Refresh", true));
                assert_eq!(
                    navigated.borrow().last().map(SharedString::as_str),
                    Some(raw)
                );
                assert_eq!(ui.get_local_sftp_path(), raw);
                click(action("Copy current path", false));
                assert_eq!(copied.borrow().last().map(SharedString::as_str), Some(raw));
                assert_eq!(ui.get_sftp_path(), raw);
            }
            ui.hide().expect("headless Windows path window");
        })
        .join()
        .expect("Windows path display and intent assertions");
    }
}

#[test]
fn sftp_primary_actions_and_full_path_editor_keep_their_pointer_routes() {
    std::thread::spawn(|| {
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;

        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        wire_menu_helpers(ui);
        ui.set_detached_window(true);
        ui.set_active_tab_kind("sftp".into());
        ui.set_sftp_available(true);
        ui.set_sftp_open(true);
        ui.set_local_sftp_loaded(true);
        ui.set_sftp_upload_ready(true);
        ui.set_local_sftp_upload_selection_ready(true);
        ui.set_sftp_selected_count(2);
        ui.set_local_sftp_selected_count(2);
        ui.set_sftp_path("/remote".into());
        let path = "/a/b/c/d/e/f/g/very-long-intermediate-directory/another-long-directory/final-artifacts/当前目录";
        ui.set_local_sftp_path(path.into());

        let downloads = Rc::new(Cell::new(0));
        let received_downloads = downloads.clone();
        ui.on_download_selected_remote_sftp(move || {
            received_downloads.set(received_downloads.get() + 1);
        });
        let uploads = Rc::new(Cell::new(0));
        let received_uploads = uploads.clone();
        ui.on_upload_selected_local_sftp(move || {
            received_uploads.set(received_uploads.get() + 1);
        });
        let paths = Rc::new(RefCell::new(Vec::new()));
        let received_paths = paths.clone();
        ui.on_list_local_sftp_directory(move |path| received_paths.borrow_mut().push(path));
        let copied = Rc::new(RefCell::new(Vec::new()));
        let received_copies = copied.clone();
        ui.on_write_clipboard(move |path| received_copies.borrow_mut().push(path));

        ui.show().expect("headless SFTP window should show");
        for width in [1180, 940] {
            harness
                .window
                .set_size(slint::PhysicalSize::new(width, 740));
            harness.settle();
            let local = ui.invoke_native_sftp_local_drop_region();
            let theme = ui.global::<Theme>();
            let title_y = local.y + theme.get_sftp_toolbar_height() / 2.0;
            harness.click(slint::LogicalPosition::new(
                local.x + local.width - theme.get_sftp_column_padding() - 24.0,
                title_y,
            ));
            harness.click(slint::LogicalPosition::new(local.x - 32.0, title_y));
        }
        assert_eq!(uploads.get(), 2, "local header must route upload intent");
        assert_eq!(
            downloads.get(),
            2,
            "remote header must route download intent"
        );

        let local = ui.invoke_native_sftp_local_drop_region();
        let theme = ui.global::<Theme>();
        // Click the path after the navigation buttons; editing must submit
        // the original string even when only its ends fit in the display.
        harness.click(slint::LogicalPosition::new(
            local.x
                + theme.get_sftp_column_padding()
                + 3.0 * (theme.get_sftp_nav_icon_button_width() + theme.get_space_xs())
                + 24.0,
            local.y
                + theme.get_sftp_toolbar_height()
                + theme.get_sftp_path_row_padding_y()
                + theme.get_sftp_control_min_height() / 2.0,
        ));
        harness.press_key(slint::platform::Key::Return);
        assert_eq!(paths.borrow().as_slice(), &[SharedString::from(path)]);

        // The gear menu's copy action also keeps the exact original path.
        harness.click(slint::LogicalPosition::new(
            local.x + local.width - theme.get_sftp_column_padding() - 16.0,
            local.y
                + theme.get_sftp_toolbar_height()
                + theme.get_sftp_path_row_padding_y()
                + theme.get_sftp_control_min_height() / 2.0,
        ));
        harness.press_key(slint::platform::Key::DownArrow);
        harness.press_key(slint::platform::Key::Return);
        assert_eq!(copied.borrow().as_slice(), &[SharedString::from(path)]);

        ui.set_sftp_selected_count(0);
        ui.set_local_sftp_upload_selection_ready(false);
        harness.settle();
        let title_y = local.y + theme.get_sftp_toolbar_height() / 2.0;
        harness.click(slint::LogicalPosition::new(local.x - 32.0, title_y));
        harness.click(slint::LogicalPosition::new(
            local.x + local.width - theme.get_sftp_column_padding() - 24.0,
            title_y,
        ));
        assert_eq!(downloads.get(), 2, "empty selection must disable download");
        assert_eq!(uploads.get(), 2, "unready selection must disable upload");
        ui.hide().expect("headless SFTP window should hide");
    })
    .join()
    .expect("SFTP pointer routes should pass");
}

#[test]
fn sftp_middle_elision_fits_real_font_width_and_restores_full_path_on_resize() {
    std::thread::spawn(|| {
        use slint::platform::WindowAdapter;
        use std::ops::ControlFlow;

        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        ui.set_detached_window(true);
        ui.set_active_tab_kind("sftp".into());
        ui.set_sftp_available(true);
        ui.set_sftp_open(true);
        ui.set_local_sftp_loaded(true);
        let path =
            "/root/共享数据/研究/working/directory/intermediate-folder/latest-artifacts/最新结果";
        ui.set_local_sftp_path(path.into());
        ui.show().expect("headless path window should show");

        let displayed_path = || {
            let window = i_slint_core::window::WindowInner::from_pub(ui.window());
            let root = i_slint_core::item_tree::ItemRc::new_root(window.component());
            root.visit_descendants(|item| {
                if !item.is_visible() {
                    return ControlFlow::Continue(());
                }
                if let Some(text) = item.downcast::<i_slint_core::items::ComplexText>() {
                    let value = i_slint_core::items::ComplexText::FIELD_OFFSETS
                        .text()
                        .apply_pin(text.as_pin_ref())
                        .get();
                    if value.starts_with('/') && value.ends_with("最新结果") {
                        let measured = harness.window.renderer().text_size(
                            text.as_pin_ref(),
                            item,
                            None,
                            i_slint_core::items::TextWrap::NoWrap,
                        );
                        return ControlFlow::Break((
                            value,
                            measured.width,
                            item.geometry().size.width,
                        ));
                    }
                }
                ControlFlow::Continue(())
            })
            .expect("visible path text must be present")
        };

        harness.window.set_size(slint::PhysicalSize::new(760, 740));
        harness.settle();
        let (shortened, measured, available) = displayed_path();
        assert!(shortened.contains('…'), "narrow path must elide the middle");
        assert!(
            measured <= available + 0.5,
            "display must fit actual font width"
        );
        let (head, tail) = shortened.split_once('…').expect("ellipsis");
        assert!(path.starts_with(head) && path.ends_with(tail));
        assert!(tail.chars().count() >= head.chars().count());
        let local = ui.invoke_native_sftp_local_drop_region();
        assert!(
            available > local.width * 0.5,
            "path must keep most of its row alongside navigation controls"
        );

        harness.window.set_size(slint::PhysicalSize::new(1800, 740));
        harness.settle();
        let (expanded, measured, available) = displayed_path();
        assert_eq!(expanded, path, "a fitting path must be shown in full");
        assert!(measured <= available + 0.5);
        ui.hide().expect("headless path window should hide");
    })
    .join()
    .expect("path fitting assertions should pass");
}
