use super::*;

#[test]
fn sftp_primary_actions_and_compact_path_keep_their_pointer_routes() {
    std::thread::spawn(|| {
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;

        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
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
        let path = "/a/b/c/d/e/f/g/当前目录";
        ui.set_local_sftp_path(path.into());
        ui.set_local_sftp_path_segments(sftp_path_rows(
            ModelRc::default(),
            path,
            SftpPathKind::Remote,
        ));

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
        // A compact leaf must start immediately after navigation, without
        // retaining the invisible ancestors' layout space.
        harness.click(slint::LogicalPosition::new(
            local.x
                + theme.get_sftp_column_padding()
                + 3.0 * (theme.get_sftp_nav_icon_button_width() + theme.get_space_sm())
                + 24.0,
            local.y
                + theme.get_sftp_toolbar_height()
                + theme.get_sftp_path_row_padding_y()
                + theme.get_sftp_control_min_height() / 2.0,
        ));
        assert_eq!(paths.borrow().as_slice(), &[SharedString::from(path)]);

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
