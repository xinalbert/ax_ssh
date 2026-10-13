use super::*;
use slint::Model;

mod sftp_navigation;

#[test]
fn bundled_ui_languages_select_on_a_headless_component() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        super::super::settings_bridge::wire_ui_language_options(&harness.ui);
        let options = harness
            .ui
            .invoke_ui_language_options("System test label".into());
        assert_eq!(options.row_count(), UiLanguage::bundled().len() + 1);
        assert_eq!(
            options.row_data(0).expect("system option"),
            "System test label"
        );
        for language in UiLanguage::bundled().rev() {
            super::settings::apply_ui_language_to_open_windows(&harness.ui, language)
                .expect("catalog should be compiled into the application");
            assert_eq!(harness.ui.get_ui_language().as_str(), language.as_setting());
            assert_eq!(
                harness.ui.get_ui_language_index(),
                language.selector_index()
            );
            assert_eq!(
                options
                    .row_data(language.selector_index() as usize)
                    .expect("native option"),
                language.native_name(),
            );
        }
        assert!(slint::select_bundled_translation("unsupported").is_err());
        assert_eq!(harness.ui.get_ui_language().as_str(), "en");
    })
    .join()
    .expect("isolated UI language selection should complete");
}

#[test]
fn sftp_unchanged_sources_skip_row_formatting_and_icons() {
    let builds = std::cell::Cell::new(0);
    let build = |entries: Vec<u8>, _: &HashSet<String>| {
        builds.set(builds.get() + 1);
        entries
            .into_iter()
            .map(|size| SftpEntryRow {
                path: "/file".into(),
                size: size.to_string().into(),
                ..Default::default()
            })
            .collect()
    };
    let selection = HashSet::new();
    let current = update_file_rows(ModelRc::default(), vec![1], &selection, 0, build);
    let unchanged = update_file_rows(current.clone(), vec![1], &selection, 0, build);
    assert_eq!(
        builds.get(),
        1,
        "progress must skip file formatting and icon conversion"
    );
    assert!(current == unchanged);
    let changed_icon = update_file_rows(unchanged, vec![1], &selection, 1, build);
    assert_eq!(builds.get(), 2);
    assert!(current == changed_icon);
    let changed_entry = update_file_rows(changed_icon, vec![2], &selection, 1, build);
    assert_eq!(builds.get(), 3);
    assert!(current == changed_entry);
    assert_eq!(changed_entry.row_data(0).expect("row").size.as_str(), "2");
}

#[test]
fn sftp_path_models_reuse_unchanged_rows_and_reset_navigation_identity() {
    let current = sftp_path_rows(ModelRc::default(), "/home/研究/data", SftpPathKind::Remote);
    assert_eq!(current.row_count(), 4);
    assert_eq!(
        current.row_data(2).expect("unicode ancestor").path.as_str(),
        "/home/研究"
    );
    let refreshed = sftp_path_rows(current.clone(), "/home/研究/data", SftpPathKind::Remote);
    assert!(
        current == refreshed,
        "progress updates must retain breadcrumb identity"
    );
    assert!(refreshed.row_data(3).expect("current directory").current);
    let moved = sftp_path_rows(refreshed, "/other/directory", SftpPathKind::Remote);
    assert!(current != moved);
    assert_eq!(
        moved.row_data(2).expect("new directory").path.as_str(),
        "/other/directory"
    );
    let fallback = sftp_path_rows(moved, "relative/path", SftpPathKind::Remote);
    assert_eq!(
        fallback.row_count(),
        0,
        "relative prefixes must never become navigation targets"
    );
}

#[test]
fn sftp_progress_keeps_file_and_transfer_models_stable() {
    let entries = vec![SftpEntry {
        name: "file.txt".into(),
        path: "/file.txt".into(),
        is_dir: false,
        is_symlink: false,
        size: 42,
        modified: Some(1234),
    }];
    let rows = sftp_entry_rows(entries.clone(), &HashSet::new());
    let current = ModelRc::new(VecModel::from(rows.clone()));
    let refreshed = update_sftp_rows(
        current.clone(),
        sftp_entry_rows(entries.clone(), &HashSet::new()),
        |a, b| a.path == b.path,
    );
    assert!(current == refreshed);
    assert_eq!(rows, refreshed.iter().collect::<Vec<_>>());
    let selected = HashSet::from(["/file.txt".to_owned()]);
    let refreshed = update_sftp_rows(refreshed, sftp_entry_rows(entries, &selected), |a, b| {
        a.path == b.path
    });
    assert!(current == refreshed);
    assert!(refreshed.row_data(0).expect("row").selected);

    let transfer = SftpTransferRow {
        id: "transfer".into(),
        progress: 0.1,
        ..Default::default()
    };
    let current = ModelRc::new(VecModel::from(vec![transfer.clone()]));
    let updated = SftpTransferRow {
        progress: 0.5,
        ..transfer
    };
    let refreshed = update_sftp_rows(current.clone(), vec![updated], |a, b| a.id == b.id);
    assert!(current == refreshed);
    assert_eq!(refreshed.row_data(0).expect("transfer").progress, 0.5);
}

#[test]
fn sftp_pages_append_in_place_and_navigation_replaces_the_model() {
    let first = SftpEntryRow {
        path: "/first".into(),
        ..Default::default()
    };
    let second = SftpEntryRow {
        path: "/second".into(),
        ..Default::default()
    };
    let current = ModelRc::new(VecModel::from(vec![first.clone()]));
    let appended = update_sftp_rows(current.clone(), vec![first.clone(), second], |a, b| {
        a.path == b.path
    });
    assert!(current == appended);
    assert_eq!(appended.row_count(), 2);
    let shrunk = update_sftp_rows(appended, vec![first], |a, b| a.path == b.path);
    assert!(current == shrunk);
    assert_eq!(shrunk.row_count(), 1);
    let moved = update_sftp_rows(
        shrunk.clone(),
        vec![SftpEntryRow {
            path: "/other/directory".into(),
            ..Default::default()
        }],
        |a, b| a.path == b.path,
    );
    assert!(shrunk != moved);
}

fn terminal_pane_view(tab_id: Uuid, x: f32, width: f32) -> TerminalPaneView {
    TerminalPaneView {
        terminal: TerminalViewState {
            terminal_id: tab_id.to_string().into(),
            ..Default::default()
        },
        x,
        y: 0.0,
        width,
        height: 1.0,
        focused: x == 0.0,
        closable: false,
    }
}

#[test]
fn main_window_alternate_screen_wheel_reaches_terminal_pointer_callback() {
    alternate_screen_wheel_reaches_terminal_pointer_callback(false);
}

#[test]
fn detached_window_alternate_screen_wheel_reaches_terminal_pointer_callback() {
    alternate_screen_wheel_reaches_terminal_pointer_callback(true);
}

fn alternate_screen_wheel_reaches_terminal_pointer_callback(detached: bool) {
    // Isolate Slint's thread-local platform from other component tests.
    std::thread::spawn(move || {
        use std::cell::RefCell;
        use std::rc::Rc;

        use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
        use slint::platform::{Platform, WindowAdapter, WindowEvent};

        struct TestPlatform(Rc<MinimalSoftwareWindow>);

        impl Platform for TestPlatform {
            fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
                Ok(self.0.clone())
            }
        }

        let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        slint::platform::set_platform(Box::new(TestPlatform(window.clone())))
            .expect("isolated platform should initialize");

        let ui = AppWindow::new().expect("application window should instantiate");
        let mut model = ax_ssh::terminal::TerminalModel::new(80, 24, 10);
        model.process(b"\x1b[?1049h\x1b[?1h");
        let terminal_snapshot = model.snapshot();
        assert!(terminal_snapshot.mouse_wheel_reporting_active);
        assert!(!terminal_snapshot.mouse_button_reporting_active);
        ui.set_detached_window(detached);
        ui.set_active_tab_kind("terminal".into());
        let mut pane = terminal_pane_view(Uuid::from_u128(1), 0.0, 1.0);
        pane.terminal = TerminalViewState {
            terminal_id: Uuid::from_u128(1).to_string().into(),
            connected: true,
            content_columns: 80,
            mouse_wheel_reporting: terminal_snapshot.mouse_wheel_reporting_active,
            render_lines: ModelRc::new(VecModel::from(
                (0..24)
                    .map(|_| TerminalRenderLine::default())
                    .collect::<Vec<_>>(),
            )),
            font_family: "JetBrains Mono".into(),
            font_size: 16.0,
            line_height_percent: 100,
            ..Default::default()
        };
        ui.set_terminal_panes(ModelRc::new(VecModel::from(vec![pane])));
        let remote_wheel = Rc::new(RefCell::new(Vec::new()));
        let received = remote_wheel.clone();
        ui.on_terminal_pointer_input(move |id, event| {
            received.borrow_mut().push((id, event));
        });
        let local_scroll = Rc::new(RefCell::new(Vec::new()));
        let scrolled = local_scroll.clone();
        ui.on_scroll_terminal(move |_, lines| scrolled.borrow_mut().push(lines));

        ui.show().expect("headless application window should show");
        window.set_size(slint::PhysicalSize::new(800, 600));
        // Stay inside the terminal in both layouts, including the main
        // window's sidebar and title bar.
        let point = slint::LogicalPosition::new(600.0, 300.0);
        window
            .window()
            .dispatch_event(WindowEvent::PointerMoved { position: point });
        window
            .window()
            .dispatch_event(WindowEvent::PointerScrolled {
                position: point,
                delta_x: 0.0,
                delta_y: 60.0,
            });

        let received = remote_wheel.borrow();
        let [(id, event)] = received.as_slice() else {
            panic!("expected one remote wheel callback, got {}", received.len());
        };
        assert_eq!(id.as_str(), Uuid::from_u128(1).to_string());
        assert_eq!(event.button, 3);
        assert_eq!(event.kind, 0);
        assert!(event.repeat_count > 0);
        assert!(local_scroll.borrow().is_empty());
    })
    .join()
    .expect("terminal pane test thread should finish");
}

struct LayoutTestHarness {
    ui: AppWindow,
    window: std::rc::Rc<slint::platform::software_renderer::MinimalSoftwareWindow>,
    time: std::rc::Rc<std::cell::Cell<std::time::Duration>>,
}

#[test]
fn detached_system_icon_properties_follow_identity_and_clear_stale_metadata() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        ui.set_detached_window(true);
        let summary = |icon: Option<&str>, release: Option<&str>| WorkspaceTabSummary {
            latency: Default::default(),
            id: Uuid::from_u128(1),
            title: "Server".into(),
            kind: "terminal",
            connected: true,
            local: false,
            system_icon: icon.map(str::to_owned),
            system_release: release.map(str::to_owned),
        };
        apply_active_session_icon(ui, Some(summary(None, Some("Debian GNU/Linux 12"))));
        assert_eq!(ui.get_active_tab_system_platform(), "debian");
        assert_eq!(ui.get_active_tab_system_release(), "Debian GNU/Linux 12");
        let detected = ui.get_active_tab_system_image();
        assert!(detected.size().width > 0);
        apply_active_session_icon(
            ui,
            Some(summary(Some("ubuntu"), Some("Debian GNU/Linux 12"))),
        );
        assert_eq!(ui.get_active_tab_system_platform(), "ubuntu");
        assert_ne!(ui.get_active_tab_system_image(), detected);
        apply_active_session_icon(ui, Some(summary(None, Some("Linux"))));
        assert_eq!(ui.get_active_tab_system_platform(), "linux");
        let tux = ui.get_active_tab_system_image();
        apply_active_session_icon(ui, Some(summary(None, Some("FreeBSD 14"))));
        assert!(ui.get_active_tab_system_platform().is_empty());
        assert_ne!(ui.get_active_tab_system_image(), tux);
        apply_active_session_icon(ui, None);
        assert!(ui.get_active_tab_system_platform().is_empty());
        assert!(ui.get_active_tab_system_release().is_empty());
        assert!(
            ui.get_active_tab_system_image().size().width > 0,
            "generic server fallback should remain available"
        );
    })
    .join()
    .expect("icon mapping should finish");
}

impl LayoutTestHarness {
    fn new() -> Self {
        use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
        use slint::platform::{Platform, WindowAdapter};
        use std::cell::Cell;
        use std::rc::Rc;
        use std::time::Duration;

        struct LayoutPlatform {
            window: Rc<MinimalSoftwareWindow>,
            time: Rc<Cell<Duration>>,
        }

        impl Platform for LayoutPlatform {
            fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
                Ok(self.window.clone())
            }

            fn duration_since_start(&self) -> Duration {
                self.time.get()
            }
        }

        let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        let time = Rc::new(Cell::new(Duration::ZERO));
        slint::platform::set_platform(Box::new(LayoutPlatform {
            window: window.clone(),
            time: time.clone(),
        }))
        .expect("isolated layout platform should initialize");
        let mut fonts = font_bridge::FontRegistry::new();
        let font = fonts
            .resources()
            .load_bundled_font("JetBrains Mono")
            .expect("test font should load")
            .expect("test font should be bundled");
        fonts
            .register_loaded_font(font)
            .expect("test font should register");

        Self {
            ui: AppWindow::new().expect("test UI should instantiate"),
            window,
            time,
        }
    }

    fn settle(&self) {
        use slint::platform::WindowAdapter;
        use slint::platform::software_renderer::Rgb565Pixel;
        use std::time::Duration;

        let mut pixels = Vec::new();
        for _ in 0..16 {
            self.time.set(self.time.get() + Duration::from_millis(20));
            slint::platform::update_timers_and_animations();
            let size = self.window.window().size();
            pixels.resize(
                size.width as usize * size.height as usize,
                Rgb565Pixel::default(),
            );
            // Rendering resolves numerical layout; no image is saved or inspected.
            self.window.draw_if_needed(|renderer| {
                renderer.render(&mut pixels, size.width as usize);
            });
        }
    }

    fn click(&self, position: slint::LogicalPosition) {
        use slint::platform::{PointerEventButton, WindowAdapter, WindowEvent};
        let window = self.window.window();
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        window.dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
        self.settle();
    }

    fn press_key(&self, key: slint::platform::Key) {
        use slint::platform::{WindowAdapter, WindowEvent};
        let text: SharedString = key.into();
        self.window
            .window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        self.window
            .window()
            .dispatch_event(WindowEvent::KeyReleased { text });
        self.settle();
    }
}

#[test]
fn themed_menu_submenu_restores_parent_keyboard_focus_and_routes_literal_paths() {
    std::thread::spawn(|| {
        use slint::platform::Key;
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;

        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        wire_menu_helpers(ui);
        ui.set_recent_workspace_paths(ModelRc::new(VecModel::from(vec![
            SharedString::from("clear-recent-workspaces"),
            SharedString::from("/second-workspace"),
        ])));
        let new_sessions = Rc::new(Cell::new(0));
        let received_sessions = new_sessions.clone();
        ui.on_new_session(move || received_sessions.set(received_sessions.get() + 1));
        let files = Rc::new(RefCell::new(Vec::new()));
        let received_files = files.clone();
        ui.on_workspace_file_action(move |action, path| {
            received_files.borrow_mut().push((action, path));
        });
        ui.show().expect("headless menu window should show");
        harness.window.set_size(slint::PhysicalSize::new(1180, 740));
        harness.settle();
        let title = slint::LogicalPosition::new(20.0, 16.0);
        harness.click(title);
        harness.press_key(Key::DownArrow);
        harness.press_key(Key::DownArrow);
        harness.press_key(Key::RightArrow);
        harness.press_key(Key::LeftArrow);
        harness.press_key(Key::Home);
        harness.press_key(Key::Return);
        assert_eq!(
            new_sessions.get(),
            1,
            "Left must restore parent keyboard input"
        );
        assert!(files.borrow().is_empty());

        harness.click(title);
        harness.press_key(Key::DownArrow);
        harness.press_key(Key::DownArrow);
        harness.press_key(Key::RightArrow);
        harness.press_key(Key::Return);
        assert_eq!(
            files.borrow().as_slice(),
            &[(
                SharedString::from("open-recent"),
                SharedString::from("clear-recent-workspaces"),
            )],
            "a workspace name matching a command must still open that workspace"
        );
    })
    .join()
    .expect("submenu keyboard test should finish");
}

#[test]
fn themed_menu_submenu_keeps_parent_pointer_actions_reachable() {
    std::thread::spawn(|| {
        use slint::platform::{Key, WindowAdapter, WindowEvent};
        use std::cell::Cell;
        use std::rc::Rc;

        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        wire_menu_helpers(ui);
        ui.set_recent_workspace_paths(ModelRc::new(VecModel::from(vec![SharedString::from(
            "/recent-workspace",
        )])));
        let new_sessions = Rc::new(Cell::new(0));
        let received_sessions = new_sessions.clone();
        ui.on_new_session(move || received_sessions.set(received_sessions.get() + 1));
        ui.show().expect("headless menu window should show");
        harness.window.set_size(slint::PhysicalSize::new(1180, 740));
        harness.settle();
        harness.click(slint::LogicalPosition::new(20.0, 16.0));
        let popup_width = || {
            let window = i_slint_core::window::WindowInner::from_pub(harness.window.window());
            let popups = window.active_popups();
            let popup = popups.last().expect("menu should remain open");
            i_slint_core::item_tree::ItemRc::new_root(popup.component.clone())
                .geometry()
                .size
                .width
        };
        let parent_width = popup_width();
        harness.press_key(Key::DownArrow);
        harness.press_key(Key::DownArrow);
        let theme = ui.global::<Theme>();
        let top = theme.get_desktop_titlebar_height() + theme.get_menu_padding();
        let recent_y = top + 2.5 * theme.get_menu_row_height() + theme.get_menu_separator_height();
        harness
            .window
            .window()
            .dispatch_event(WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(20.0, recent_y),
            });
        // Advance past the submenu hover delay; resolve layout without saving an image.
        harness.settle();
        assert!(popup_width() > parent_width, "hover must open the submenu");
        harness.click(slint::LogicalPosition::new(
            20.0,
            top + theme.get_menu_row_height() / 2.0,
        ));
        assert_eq!(
            new_sessions.get(),
            1,
            "an open submenu must allow a pointer action in its parent"
        );
    })
    .join()
    .expect("submenu pointer test should finish");
}

#[test]
fn desktop_main_window_drags_from_menu_titlebar_only() {
    window_drag_gestures(false);
}

#[test]
fn desktop_detached_window_drags_from_its_titlebar() {
    window_drag_gestures(true);
}

fn window_drag_gestures(detached: bool) {
    std::thread::spawn(move || {
        use slint::platform::{PointerEventButton, WindowAdapter, WindowEvent};
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;

        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        ui.set_detached_window(detached);
        ui.set_sidebar_width(220);
        ui.set_workspace_tabs(ModelRc::new(VecModel::from(vec![WorkspaceTabRow {
            id: "test-tab".into(),
            title: "Test tab".into(),
            kind: "settings".into(),
            connected: false,
            system_release: String::new().into(),
            system_platform: String::new().into(),
            ..Default::default()
        }])));
        let drags = Rc::new(Cell::new(0));
        let received_drags = drags.clone();
        ui.on_start_native_window_drag(move || received_drags.set(received_drags.get() + 1));
        let activated = Rc::new(RefCell::new(Vec::new()));
        let received_tabs = activated.clone();
        ui.on_activate_tab(move |id| received_tabs.borrow_mut().push(id));

        ui.show().expect("headless window should show");
        harness.window.set_size(slint::PhysicalSize::new(1180, 740));
        harness.settle();
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

        click(600.0, 16.0, PointerEventButton::Left);
        assert_eq!(
            drags.get(),
            1,
            "titlebar blank must request native dragging"
        );
        click(600.0, 16.0, PointerEventButton::Right);
        assert_eq!(drags.get(), 1, "right press must not start window dragging");

        click(1114.0, 16.0, PointerEventButton::Left);
        assert_eq!(drags.get(), 1, "window controls must not start dragging");
        if !detached {
            click(270.0, 48.0, PointerEventButton::Left);
            assert_eq!(
                activated.borrow().as_slice(),
                &[SharedString::from("test-tab")]
            );
            assert_eq!(drags.get(), 1, "Tab click must only activate the Tab");
            click(700.0, 48.0, PointerEventButton::Left);
            assert_eq!(
                drags.get(),
                1,
                "Windows/Linux Tab-row blank must not drag the window"
            );
        }
    })
    .join()
    .expect("window gesture test thread should finish");
}

#[test]
fn detached_sftp_geometry_ignores_the_hidden_main_sidebar() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        ui.set_detached_window(true);
        ui.set_active_tab_kind("sftp".into());
        ui.set_sftp_available(true);
        ui.set_sftp_open(true);
        ui.set_local_sftp_loaded(true);
        ui.set_local_sftp_path("/test-local".into());
        ui.set_sidebar_width(220);
        ui.show().expect("headless window should show");
        harness.window.set_size(slint::PhysicalSize::new(1180, 740));
        harness.settle();
        let initial = ui.invoke_native_sftp_local_drop_region();
        assert!(initial.accepts && initial.width > 0.0 && initial.height > 0.0);

        ui.set_sidebar_width(360);
        harness.settle();
        let after_sidebar = ui.invoke_native_sftp_local_drop_region();
        assert_eq!(
            after_sidebar, initial,
            "hidden sidebar must not replace detached geometry"
        );

        harness.window.set_size(slint::PhysicalSize::new(1342, 805));
        harness.settle();
        let enlarged = ui.invoke_native_sftp_local_drop_region();
        assert!(enlarged.width > initial.width && enlarged.height > initial.height);

        ui.set_detached_window(false);
        harness.settle();
        let main = ui.invoke_native_sftp_local_drop_region();
        assert!(main.accepts && main.width < enlarged.width);
        ui.set_sidebar_width(220);
        harness.settle();
        assert!(ui.invoke_native_sftp_local_drop_region().width > main.width);
        ui.hide().expect("headless window should hide");
    })
    .join()
    .expect("SFTP layout assertions should pass");
}

#[test]
fn main_terminal_has_no_manual_media_row() {
    terminal_has_no_manual_media_row(false);
}

#[test]
fn detached_terminal_has_no_manual_media_row() {
    terminal_has_no_manual_media_row(true);
}

fn terminal_has_no_manual_media_row(detached: bool) {
    std::thread::spawn(move || {
        use std::cell::RefCell;
        use std::rc::Rc;

        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        ui.set_detached_window(detached);
        ui.set_active_tab_kind("terminal".into());
        let mut pane = terminal_pane_view(Uuid::from_u128(1), 0.0, 1.0);
        pane.terminal.connected = true;
        pane.terminal.font_family = "JetBrains Mono".into();
        pane.terminal.font_size = 17.0;
        pane.terminal.line_height_percent = 100;
        let panes = Rc::new(VecModel::from(vec![pane.clone()]));
        ui.set_terminal_panes(ModelRc::from(panes.clone()));
        let geometry = Rc::new(RefCell::new(None));
        let callback_geometry = geometry.clone();
        ui.on_terminal_geometry(
            move |_, _, pane_y, _, _, _, grid_y, _, grid_height, _, _, _, rows| {
                *callback_geometry.borrow_mut() = Some((grid_y - pane_y, grid_height, rows));
            },
        );
        ui.show().expect("headless window should show");
        harness.window.set_size(slint::PhysicalSize::new(1180, 740));
        harness.settle();
        let idle = geometry.borrow().expect("terminal should report geometry");
        assert!(
            idle.0.abs() < 0.01,
            "idle terminal must start at pane top: {idle:?}"
        );

        // Historical media snapshots must not recreate buttons, errors, or a
        // transcript editor in a CLI-owned recording workflow.
        pane.terminal.media_mode = "transcribing".into();
        pane.terminal.media_status = "Microphone failed: configure whisper-cli".into();
        pane.terminal.transcript = "old transcript".into();
        panes.set_row_data(0, pane.clone());
        harness.settle();
        assert_eq!(*geometry.borrow(), Some(idle));

        pane.closable = true;
        panes.set_row_data(0, pane);
        harness.settle();
        let split = geometry
            .borrow()
            .expect("split pane should report geometry");
        assert!(
            (split.0 - 24.0).abs() < 0.01,
            "split close header must remain: {split:?}"
        );
        assert!(split.1 < idle.1 && split.2 < idle.2);
        ui.hide().expect("headless window should hide");
    })
    .join()
    .expect("terminal media layout assertions should pass");
}

#[test]
fn detached_terminal_has_one_resize_source_and_ignores_main_sidebar_width() {
    // A dedicated thread isolates Slint's thread-local platform from other tests.
    std::thread::spawn(|| {
        use std::cell::RefCell;
        use std::collections::BTreeSet;
        use std::rc::Rc;

        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        let window = &harness.window;
        ui.set_detached_window(true);
        ui.set_active_tab_kind("terminal".into());
        ui.set_sidebar_width(220);
        let mut pane = terminal_pane_view(Uuid::from_u128(1), 0.0, 1.0);
        pane.focused = false;
        pane.terminal.connected = true;
        pane.terminal.font_family = "JetBrains Mono".into();
        // 17.11px gives a 10.266px Latin advance in the bundled 600/1000
        // monospace face. A 50-cell preferred-width probe rounds 513.3px to
        // 514px and would report 10.28px, drifting across long box lines.
        pane.terminal.font_size = 17.11;
        pane.terminal.line_height_percent = 100;
        ui.set_terminal_panes(ModelRc::new(VecModel::from(vec![pane])));

        let requests = Rc::new(RefCell::new(Vec::new()));
        let requests_for_callback = requests.clone();
        let cell_widths = Rc::new(RefCell::new(Vec::new()));
        let widths_for_callback = cell_widths.clone();
        ui.on_resize_terminal(move |_, columns, rows, cell_width, _| {
            requests_for_callback.borrow_mut().push((columns, rows));
            widths_for_callback.borrow_mut().push(cell_width);
        });
        ui.show().expect("headless window should show");
        window.set_size(slint::PhysicalSize::new(1180, 740));

        harness.settle();
        assert!(
            cell_widths
                .borrow()
                .iter()
                .any(|width| (width - 10.266).abs() < 0.002),
            "terminal cell width should follow the font advance, not a rounded short probe: {:?}",
            cell_widths.borrow()
        );
        let sizes: BTreeSet<_> = requests.borrow_mut().drain(..).collect();
        assert_eq!(
            sizes.len(),
            1,
            "one visible pane must have one size: {sizes:?}"
        );
        let initial_columns = sizes.first().expect("pane should report its size").0;

        ui.set_sidebar_width(360);
        harness.settle();
        assert!(
            requests.borrow().is_empty(),
            "a detached terminal must not resize when the main sidebar changes: {:?}",
            requests.borrow()
        );

        window.set_size(slint::PhysicalSize::new(1342, 805));
        harness.settle();
        let sizes: BTreeSet<_> = requests.borrow_mut().drain(..).collect();
        assert_eq!(
            sizes.len(),
            1,
            "window resize must have one size: {sizes:?}"
        );
        let detached_columns = sizes.first().expect("pane should resize with its window").0;
        assert!(detached_columns > initial_columns);

        ui.set_detached_window(false);
        harness.settle();
        let main_columns = requests
            .borrow_mut()
            .drain(..)
            .next_back()
            .expect("main terminal should mount and report its size")
            .0;
        assert!(main_columns < detached_columns);
        ui.set_sidebar_width(220);
        harness.settle();
        assert!(
            requests
                .borrow()
                .last()
                .is_some_and(|size| size.0 > main_columns),
            "main terminal should continue to follow its sidebar width"
        );
        ui.hide().expect("headless window should hide");
    })
    .join()
    .expect("terminal layout assertions should pass");
}

#[test]
fn terminal_pane_snapshots_update_existing_model_rows() {
    let first_id = Uuid::from_u128(1);
    let second_id = Uuid::from_u128(2);
    let mut first_pane = terminal_pane_view(first_id, 0.0, 0.5);
    first_pane.terminal.render_lines =
        ModelRc::new(VecModel::from(vec![terminal_render_line_with_text(
            "before",
        )]));
    first_pane.terminal.cursor_state = ModelRc::new(VecModel::from(vec![TerminalCursorState {
        row: 1,
        column: 2,
        cells: 1,
        visible: true,
        shape: 0,
        blinking: false,
        text: "b".into(),
    }]));
    let panes = ModelRc::new(VecModel::from(vec![
        first_pane,
        terminal_pane_view(second_id, 0.5, 0.5),
    ]));
    let original_lines = panes.row_data(0).unwrap().terminal.render_lines;
    let original_runs = original_lines.row_data(0).unwrap().runs;
    let original_cursor = panes.row_data(0).unwrap().terminal.cursor_state;
    let dividers = ModelRc::new(VecModel::from(vec![TerminalPaneDividerView {
        id: 0,
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
        ratio: 0.5,
        vertical: true,
    }]));
    let mut updated_panes = vec![
        terminal_pane_view(first_id, 0.0, 0.6),
        terminal_pane_view(second_id, 0.6, 0.4),
    ];
    updated_panes[0].terminal.connected = true;
    updated_panes[0].terminal.render_lines =
        ModelRc::new(VecModel::from(vec![terminal_render_line_with_text(
            "after",
        )]));
    updated_panes[0].terminal.cursor_state =
        ModelRc::new(VecModel::from(vec![TerminalCursorState {
            row: 3,
            column: 7,
            cells: 1,
            visible: true,
            shape: 0,
            blinking: false,
            text: "a".into(),
        }]));
    let updated_dividers = vec![TerminalPaneDividerView {
        id: 0,
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
        ratio: 0.6,
        vertical: true,
    }];

    assert!(update_terminal_pane_snapshot_models(
        &panes,
        &dividers,
        &updated_panes,
        &updated_dividers,
    ));
    let updated_first_pane = panes.row_data(0).unwrap();
    assert!(updated_first_pane.terminal.connected);
    assert_eq!(updated_first_pane.terminal.render_lines, original_lines);
    assert_eq!(updated_first_pane.terminal.cursor_state, original_cursor);
    let updated_cursor = original_cursor.row_data(0).unwrap();
    assert_eq!(updated_cursor.row, 3);
    assert_eq!(updated_cursor.column, 7);
    assert_eq!(updated_cursor.text.as_str(), "a");
    let updated_first_line = original_lines.row_data(0).unwrap();
    assert_eq!(updated_first_line.runs, original_runs);
    assert_eq!(original_runs.row_data(0).unwrap().text.as_str(), "after");
    assert_eq!(panes.row_data(1).unwrap().x, 0.6);
    assert_eq!(dividers.row_data(0).unwrap().ratio, 0.6);
}

#[test]
fn terminal_render_lines_reuse_existing_rows_when_output_is_unchanged() {
    let current = ModelRc::new(VecModel::from(vec![terminal_render_line_with_text("same")]));
    let original_line = current.row_data(0).expect("current line should exist");
    let original_backgrounds = original_line.backgrounds.clone();
    let original_decorations = original_line.decorations.clone();
    let original_runs = original_line.runs.clone();
    let updated = vec![terminal_render_line_with_text("same")];

    assert!(update_terminal_render_lines(&current, &updated));
    assert_eq!(
        current.row_data(0).expect("line should remain"),
        original_line
    );
    assert_eq!(
        current.row_data(0).expect("line should remain").runs,
        original_runs
    );
    assert_eq!(
        current.row_data(0).expect("line should remain").backgrounds,
        original_backgrounds
    );
    assert_eq!(
        current.row_data(0).expect("line should remain").decorations,
        original_decorations
    );

    let changed = vec![terminal_render_line_with_text("changed")];
    assert!(update_terminal_render_lines(&current, &changed));
    assert_eq!(
        current.row_data(0).expect("line should remain").runs,
        original_runs
    );
    assert_eq!(
        current.row_data(0).expect("line should remain").backgrounds,
        original_backgrounds
    );
    assert_eq!(
        current.row_data(0).expect("line should remain").decorations,
        original_decorations
    );
    assert_eq!(
        current
            .row_data(0)
            .expect("changed line should remain")
            .runs
            .row_data(0)
            .expect("changed run should exist")
            .text
            .as_str(),
        "changed"
    );

    let runs_after_change = current.row_data(0).expect("line should remain").runs;
    let split_runs = vec![
        terminal_render_run_with_text("left"),
        terminal_render_run_with_text("right"),
    ];
    assert!(update_terminal_render_lines(
        &current,
        &[TerminalRenderLine {
            source_revision_low: 0,
            source_revision_high: 0,
            render_cache_key_low: 0,
            render_cache_key_high: 0,
            backgrounds: ModelRc::new(VecModel::from(Vec::<TerminalBackgroundRun>::new())),
            decorations: ModelRc::new(VecModel::from(Vec::<TerminalDecorationRun>::new())),
            runs: ModelRc::new(VecModel::from(split_runs)),
        }],
    ));
    assert_eq!(
        current.row_data(0).expect("line should remain").runs,
        runs_after_change
    );
    assert_eq!(
        current
            .row_data(0)
            .expect("line should remain")
            .runs
            .row_count(),
        2
    );
}

#[test]
fn terminal_render_lines_refresh_render_key_without_replacing_nested_models() {
    let current = ModelRc::new(VecModel::from(vec![terminal_render_line_with_revision(
        "same", 7, 11,
    )]));
    let original_runs = current.row_data(0).expect("current line").runs;
    let updated = vec![terminal_render_line_with_revision("same", 7, 12)];

    assert!(update_terminal_render_lines(&current, &updated));

    let line = current.row_data(0).expect("updated line");
    assert_eq!(line.render_cache_key_low, 12);
    assert_eq!(line.runs, original_runs);
}

#[test]
fn terminal_pane_snapshots_reset_existing_nested_models_when_row_counts_change() {
    let tab_id = Uuid::from_u128(1);
    let mut pane = terminal_pane_view(tab_id, 0.0, 1.0);
    pane.terminal.render_lines =
        ModelRc::new(VecModel::from(vec![terminal_render_line_with_text(
            "before",
        )]));
    let panes = ModelRc::new(VecModel::from(vec![pane]));
    let original_lines = panes.row_data(0).unwrap().terminal.render_lines;
    let original_runs = original_lines.row_data(0).unwrap().runs;
    let dividers = ModelRc::new(VecModel::from(Vec::<TerminalPaneDividerView>::new()));
    let mut updated_pane = terminal_pane_view(tab_id, 0.0, 1.0);
    updated_pane.terminal.render_lines = ModelRc::new(VecModel::from(vec![
        TerminalRenderLine {
            source_revision_low: 0,
            source_revision_high: 0,
            render_cache_key_low: 0,
            render_cache_key_high: 0,
            backgrounds: ModelRc::new(VecModel::from(Vec::<TerminalBackgroundRun>::new())),
            decorations: ModelRc::new(VecModel::from(Vec::<TerminalDecorationRun>::new())),
            runs: ModelRc::new(VecModel::from(vec![
                terminal_render_run_with_text("after"),
                terminal_render_run_with_text("-suffix"),
            ])),
        },
        terminal_render_line_with_text("second-line"),
    ]));

    assert!(update_terminal_pane_snapshot_models(
        &panes,
        &dividers,
        &[updated_pane],
        &[],
    ));
    let updated_lines = panes.row_data(0).unwrap().terminal.render_lines;
    assert_eq!(updated_lines, original_lines);
    assert_eq!(updated_lines.row_count(), 2);
    let updated_runs = updated_lines.row_data(0).unwrap().runs;
    assert_eq!(updated_runs, original_runs);
    assert_eq!(updated_runs.row_count(), 2);
    assert_eq!(updated_runs.row_data(0).unwrap().text.as_str(), "after");
    assert_eq!(updated_runs.row_data(1).unwrap().text.as_str(), "-suffix");
    assert_eq!(
        updated_lines
            .row_data(1)
            .unwrap()
            .runs
            .row_data(0)
            .unwrap()
            .text
            .as_str(),
        "second-line"
    );
}

#[test]
fn terminal_pane_snapshot_keeps_parent_row_when_only_nested_models_change() {
    let tab_id = Uuid::from_u128(1);
    let mut pane = terminal_pane_view(tab_id, 0.0, 1.0);
    pane.terminal.render_lines =
        ModelRc::new(VecModel::from(vec![terminal_render_line_with_text(
            "before",
        )]));
    pane.terminal.cursor_state = ModelRc::new(VecModel::from(vec![TerminalCursorState {
        row: 0,
        column: 0,
        cells: 1,
        visible: true,
        shape: 0,
        blinking: false,
        text: "b".into(),
    }]));
    let panes = ModelRc::new(VecModel::from(vec![pane]));
    let dividers = ModelRc::new(VecModel::from(Vec::<TerminalPaneDividerView>::new()));
    let mut updated = terminal_pane_view(tab_id, 0.0, 1.0);
    updated.terminal.render_lines =
        ModelRc::new(VecModel::from(vec![terminal_render_line_with_text(
            "after",
        )]));
    updated.terminal.cursor_state = ModelRc::new(VecModel::from(vec![TerminalCursorState {
        row: 3,
        column: 7,
        cells: 1,
        visible: true,
        shape: 0,
        blinking: false,
        text: "a".into(),
    }]));
    updated.terminal.cursor_row = 3;
    updated.terminal.cursor_column = 7;
    updated.terminal.cursor_text = "a".into();

    assert!(update_terminal_pane_snapshot_models(
        &panes,
        &dividers,
        &[updated],
        &[],
    ));
    let current = panes.row_data(0).expect("pane row");
    assert_eq!(current.terminal.cursor_row, 0);
    assert_eq!(current.terminal.cursor_column, 0);
    assert_eq!(
        current
            .terminal
            .cursor_state
            .row_data(0)
            .expect("cursor row")
            .row,
        3
    );
    assert_eq!(
        current
            .terminal
            .render_lines
            .row_data(0)
            .expect("render line")
            .runs
            .row_data(0)
            .expect("render run")
            .text
            .as_str(),
        "after"
    );
}

#[test]
fn terminal_pane_snapshot_reuses_unchanged_rows_when_one_row_changes() {
    let tab_id = Uuid::from_u128(1);
    let mut pane = terminal_pane_view(tab_id, 0.0, 1.0);
    pane.terminal.render_lines = ModelRc::new(VecModel::from(vec![
        terminal_render_line_with_revision("stable", 11, 1),
        terminal_render_line_with_revision("before", 12, 1),
    ]));
    let panes = ModelRc::new(VecModel::from(vec![pane]));
    let current_pane = panes.row_data(0).expect("current pane");
    let current_lines = current_pane.terminal.render_lines;
    let stable_line = current_lines.row_data(0).expect("stable line");
    let stable_runs = stable_line.runs.clone();
    let changed_runs = current_lines.row_data(1).expect("changed line").runs;
    let dividers = ModelRc::new(VecModel::from(Vec::<TerminalPaneDividerView>::new()));

    let mut updated = terminal_pane_view(tab_id, 0.0, 1.0);
    updated.terminal.render_lines = ModelRc::new(VecModel::from(vec![
        terminal_render_line_with_revision("stable", 11, 1),
        terminal_render_line_with_revision("after", 13, 1),
    ]));

    assert!(update_terminal_pane_snapshot_models(
        &panes,
        &dividers,
        &[updated],
        &[],
    ));

    let updated_pane = panes.row_data(0).expect("updated pane");
    assert_eq!(updated_pane.terminal.render_lines, current_lines);
    assert_eq!(current_lines.row_data(0).expect("stable line"), stable_line);
    assert_eq!(
        current_lines.row_data(0).expect("stable line").runs,
        stable_runs
    );
    assert_eq!(
        current_lines.row_data(1).expect("changed line").runs,
        changed_runs
    );
    assert_eq!(
        current_lines
            .row_data(1)
            .expect("changed line")
            .runs
            .row_data(0)
            .expect("changed run")
            .text
            .as_str(),
        "after"
    );
}

#[test]
fn terminal_render_cache_reuses_only_matching_line_and_settings_revisions() {
    let snapshot = TerminalSnapshot {
        lines: vec![Arc::new(ax_ssh::terminal::TerminalStyledLine {
            revision: 7,
            runs: vec![ax_ssh::terminal::TerminalStyledRun {
                text: "ready".to_owned(),
                column: 0,
                cells: 5,
                style: Default::default(),
                hyperlink: None,
            }],
        })],
        dirty_rows: vec![0],
        full_refresh: true,
        max_columns: 10,
        cursor_row: 0,
        cursor_column: 5,
        cursor_cells: 1,
        cursor_visible: true,
        cursor_shape: ax_ssh::terminal::TerminalCursorShape::Block,
        cursor_blinking: false,
        foreground_color: ax_ssh::terminal::TerminalColor::Default,
        background_color: ax_ssh::terminal::TerminalColor::Default,
        cursor_color: ax_ssh::terminal::TerminalColor::Default,
        cursor_text: " ".to_owned(),
        display_offset: 0,
        viewport_mode: ax_ssh::terminal::TerminalViewportMode::Follow,
        mouse_reporting: Default::default(),
        mouse_button_reporting_active: false,
        mouse_wheel_reporting_active: false,
        bell_revision: 0,
    };
    let settings = TerminalRenderSettings {
        color_scheme: TerminalColorScheme::Dark,
        default_foreground: RgbColor::new(204, 204, 204),
        default_background: RgbColor::new(30, 30, 30),
        selection_background: RgbColor::new(38, 79, 120),
        text_brightness: 1.0,
        bright_bold_text: true,
        semantic_highlighting: true,
        semantic_colors: SemanticColorOverrides::default(),
    };
    let renderer = TerminalRenderer::new(settings);
    let first = render_snapshot_lines(&snapshot, &renderer, None);
    let first_runs = first[0].runs.clone();
    let current = ModelRc::new(VecModel::from(first));

    let cached = render_snapshot_lines(&snapshot, &renderer, Some(&current));
    assert_eq!(cached[0].runs, first_runs);

    let changed_renderer = TerminalRenderer::new(TerminalRenderSettings {
        text_brightness: 0.9,
        ..settings
    });
    let settings_changed = render_snapshot_lines(&snapshot, &changed_renderer, Some(&current));
    assert_ne!(settings_changed[0].runs, first_runs);

    let mut changed_line = snapshot.lines[0].as_ref().clone();
    changed_line.revision = u64::from(u32::MAX) + 8;
    let changed_snapshot = TerminalSnapshot {
        lines: vec![Arc::new(changed_line)],
        ..snapshot
    };
    let line_changed = render_snapshot_lines(&changed_snapshot, &renderer, Some(&current));
    assert_ne!(line_changed[0].runs, first_runs);
}

fn terminal_render_line_with_text(text: &str) -> TerminalRenderLine {
    terminal_render_line_with_revision(text, 0, 0)
}

fn terminal_render_line_with_revision(
    text: &str,
    source_revision: u64,
    render_cache_key: u64,
) -> TerminalRenderLine {
    TerminalRenderLine {
        source_revision_low: source_revision as u32 as i32,
        source_revision_high: (source_revision >> 32) as u32 as i32,
        render_cache_key_low: render_cache_key as u32 as i32,
        render_cache_key_high: (render_cache_key >> 32) as u32 as i32,
        backgrounds: ModelRc::new(VecModel::from(Vec::<TerminalBackgroundRun>::new())),
        decorations: ModelRc::new(VecModel::from(Vec::<TerminalDecorationRun>::new())),
        runs: ModelRc::new(VecModel::from(vec![terminal_render_run_with_text(text)])),
    }
}

fn terminal_render_run_with_text(text: &str) -> TerminalRenderRun {
    TerminalRenderRun {
        text: text.into(),
        cells: 1,
        ..Default::default()
    }
}

#[test]
fn terminal_pane_layout_updates_existing_model_rows() {
    let first_id = Uuid::from_u128(1);
    let second_id = Uuid::from_u128(2);
    let panes = ModelRc::new(VecModel::from(vec![
        terminal_pane_view(first_id, 0.0, 0.5),
        terminal_pane_view(second_id, 0.5, 0.5),
    ]));
    let dividers = ModelRc::new(VecModel::from(vec![TerminalPaneDividerView {
        id: 0,
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
        ratio: 0.5,
        vertical: true,
    }]));
    let layout = PaneLayout {
        panes: vec![
            PanePlacement {
                tab_id: first_id,
                x: 0.0,
                y: 0.0,
                width: 0.7,
                height: 1.0,
                focused: true,
            },
            PanePlacement {
                tab_id: second_id,
                x: 0.7,
                y: 0.0,
                width: 0.3,
                height: 1.0,
                focused: false,
            },
        ],
        dividers: vec![PaneDividerPlacement {
            id: 0,
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            ratio: 0.7,
            vertical: true,
        }],
    };

    assert!(update_terminal_pane_layout_models(
        &panes, &dividers, layout
    ));
    assert_eq!(panes.row_data(0).unwrap().width, 0.7);
    assert_eq!(panes.row_data(1).unwrap().x, 0.7);
    assert_eq!(dividers.row_data(0).unwrap().ratio, 0.7);
}

#[test]
fn terminal_pane_layout_rejects_stale_model_identity_without_partial_update() {
    let visible_id = Uuid::from_u128(1);
    let stale_id = Uuid::from_u128(2);
    let panes = ModelRc::new(VecModel::from(vec![terminal_pane_view(
        visible_id, 0.0, 1.0,
    )]));
    let dividers = ModelRc::new(VecModel::from(Vec::<TerminalPaneDividerView>::new()));
    let layout = PaneLayout {
        panes: vec![PanePlacement {
            tab_id: stale_id,
            x: 0.0,
            y: 0.0,
            width: 0.7,
            height: 1.0,
            focused: true,
        }],
        dividers: Vec::new(),
    };

    assert!(!update_terminal_pane_layout_models(
        &panes, &dividers, layout
    ));
    assert_eq!(panes.row_data(0).unwrap().width, 1.0);
}

#[test]
fn terminal_select_all_shortcut_preserves_shell_control_a() {
    assert_eq!(terminal_select_all_shortcut_for_platform(true), "Cmd+A");
    assert_eq!(
        terminal_select_all_shortcut_for_platform(false),
        "Ctrl+Shift+A"
    );
}

#[test]
fn settings_workbench_is_exposed_as_a_workspace_tab() {
    let rows = visible_workspace_tab_rows(vec![
        WorkspaceTabSummary {
            latency: Default::default(),
            id: Uuid::new_v4(),
            title: "Settings".to_owned(),
            kind: "settings",
            connected: false,
            local: false,
            system_icon: None,
            system_release: None,
        },
        WorkspaceTabSummary {
            latency: Default::default(),
            id: Uuid::new_v4(),
            title: "New session".to_owned(),
            kind: "session-editor",
            connected: false,
            local: false,
            system_icon: None,
            system_release: None,
        },
    ]);

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].kind.as_str(), "settings");
    assert_eq!(rows[1].kind.as_str(), "session-editor");
}

#[test]
fn workspace_tabs_expose_release_and_platform_with_safe_fallbacks() {
    let rows = visible_workspace_tab_rows(vec![
        WorkspaceTabSummary {
            latency: Default::default(),
            id: Uuid::new_v4(),
            title: "Windows host".to_owned(),
            kind: "terminal",
            connected: true,
            local: false,
            system_icon: None,
            system_release: Some("Microsoft Windows Server 2022".to_owned()),
        },
        WorkspaceTabSummary {
            latency: Default::default(),
            id: Uuid::new_v4(),
            title: "Unknown host".to_owned(),
            kind: "terminal",
            connected: true,
            local: false,
            system_icon: None,
            system_release: None,
        },
    ]);

    assert_eq!(
        rows[0].system_release.as_str(),
        "Microsoft Windows Server 2022"
    );
    assert_eq!(rows[0].system_platform.as_str(), "windows");
    assert!(rows[1].system_release.is_empty());
    assert!(rows[1].system_platform.is_empty());
}

#[test]
fn local_shell_icons_use_host_platform_without_guessing_from_the_title() {
    let rows = visible_workspace_tab_rows(vec![
        WorkspaceTabSummary {
            latency: Default::default(),
            id: Uuid::new_v4(),
            title: "build".to_owned(),
            kind: "terminal",
            connected: false,
            local: true,
            system_icon: None,
            system_release: None,
        },
        WorkspaceTabSummary {
            latency: Default::default(),
            id: Uuid::new_v4(),
            title: "Local Shell #1".to_owned(),
            kind: "terminal",
            connected: true,
            local: false,
            system_icon: None,
            system_release: None,
        },
    ]);

    let expected = match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        "linux" => "linux",
        _ => "",
    };
    assert_eq!(rows[0].system_platform.as_str(), expected);
    assert!(rows[0].system_release.is_empty());
    assert!(rows[1].system_platform.is_empty());
}

#[test]
fn workspace_tab_release_changes_update_existing_rows_and_local_sidebar_platform() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        apply_settings_to_component(&harness.ui, &AppSettings::default());
        assert_eq!(
            harness.ui.get_local_shell_platform().as_str(),
            local_system_platform()
        );

        let mut row = WorkspaceTabRow {
            id: Uuid::new_v4().to_string().into(),
            title: "remote".into(),
            kind: "terminal".into(),
            connected: true,
            ..Default::default()
        };
        let model = ModelRc::new(VecModel::from(vec![row.clone()]));
        harness.ui.set_workspace_tabs(model.clone());

        // Release arrives after Connected, and each field must trigger a row update.
        for (release, platform) in [
            ("Microsoft Windows Server 2022", ""),
            ("Microsoft Windows Server 2022", "windows"),
            ("", "windows"),
            ("", ""),
        ] {
            row.system_release = release.into();
            row.system_platform = platform.into();
            workspace::apply_workspace_tab_rows(&harness.ui, vec![row.clone()]);
            assert!(harness.ui.get_workspace_tabs() == model);
            assert_eq!(model.row_data(0), Some(row.clone()));
        }
    })
    .join()
    .expect("isolated tab model test should finish");
}

#[test]
fn session_group_rows_keep_profiles_nested_under_their_group() {
    let mut production_a = SessionProfile::new("prod-a", "a.example", "alice");
    production_a.group_name = " Production ".into();
    let mut production_b = SessionProfile::new("prod-b", "192.168.1.202", "zhushixin");
    production_b.group_name = "Production".into();
    let ungrouped = SessionProfile::new("local", "local.example", "carol");
    let sessions = SessionStore {
        sessions: vec![production_a, production_b, ungrouped],
        ..SessionStore::default()
    };
    let rows = session_group_rows(&sessions, "", &HashMap::new());

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name.as_str(), "Production");
    assert_eq!(rows[0].profiles.row_count(), 2);
    let production_a = rows[0].profiles.row_data(0).unwrap();
    assert_eq!(production_a.name.as_str(), "prod-a");
    assert_eq!(production_a.endpoint.as_str(), "al*ce@a.example:22");
    let production_b = rows[0].profiles.row_data(1).unwrap();
    assert_eq!(production_b.name.as_str(), "prod-b");
    assert_eq!(production_b.endpoint.as_str(), "zh*in@192.*.1.202:22");
    assert_eq!(
        production_b.details.as_str(),
        "SSH · zhushixin@192.168.1.202:22"
    );

    assert_eq!(rows[1].name.as_str(), "Ungrouped");
    assert_eq!(rows[1].profiles.row_count(), 1);
    assert_eq!(rows[1].profiles.row_data(0).unwrap().name.as_str(), "local");
}

#[test]
fn session_group_search_matches_profiles_and_groups() {
    let mut production = SessionProfile::new("GPU node", "gpu01.example", "alice");
    production.group_name = "Research Cluster".into();
    let local = SessionProfile::new("Mac mini", "mac.local", "albert");
    let sessions = SessionStore {
        sessions: vec![production, local],
        ..SessionStore::default()
    };

    let profile_match = session_group_rows(&sessions, "gpu01", &HashMap::new());
    assert_eq!(profile_match.len(), 1);
    assert_eq!(profile_match[0].profiles.row_count(), 1);

    let group_match = session_group_rows(&sessions, "research", &HashMap::new());
    assert_eq!(group_match.len(), 1);
    assert_eq!(group_match[0].profiles.row_count(), 1);
}

#[test]
fn session_rows_derive_system_icons_from_each_profiles_release() {
    let windows = SessionProfile::new("Windows", "win.example", "alice");
    let linux = SessionProfile::new("Linux", "linux.example", "alice");
    let macos = SessionProfile::new("Mac", "mac.example", "alice");
    let unknown = SessionProfile::new("Unknown", "unknown.example", "alice");
    let undetected = SessionProfile::new("Undetected", "new.example", "alice");
    let releases = HashMap::from([
        (windows.id, "Microsoft Windows Server 2022".to_owned()),
        (linux.id, "Ubuntu 24.04 LTS".to_owned()),
        (macos.id, "macOS 15.5".to_owned()),
        (unknown.id, "FreeBSD 14.3".to_owned()),
    ]);
    let sessions = SessionStore {
        sessions: vec![windows, linux, macos, unknown, undetected],
        ..SessionStore::default()
    };
    let rows = session_group_rows(&sessions, "", &releases);
    let platforms = rows[0]
        .profiles
        .iter()
        .map(|profile| profile.system_platform.to_string())
        .collect::<Vec<_>>();
    assert_eq!(platforms, ["windows", "ubuntu", "macos", "", ""]);
}

#[test]
fn manual_icon_precedes_cached_detection_in_sidebar_and_tab_rows() {
    let mut profile = SessionProfile::new("Server", "server.example", "alice");
    profile.detected_system =
        ax_ssh::config::SystemDetection::new(&profile, "Debian 12", "debian").map(Box::new);
    profile.system_icon_override = Some("ubuntu".into());
    let mut sessions = SessionStore::default();
    sessions.upsert(profile.clone());
    let rows = session_group_rows(&sessions, "", &HashMap::new());
    assert_eq!(
        rows[0].profiles.row_data(0).unwrap().system_platform,
        "ubuntu"
    );
    let mut state = AppState::new(ConfigStore::new("/unused/icon-test.json"), sessions);
    state.open_terminal_tab(&profile);
    assert_eq!(
        visible_workspace_tab_rows(state.tab_summaries())[0].system_platform,
        "ubuntu"
    );
    state.sessions.sessions[0].system_icon_override = None;
    let rows = session_group_rows(&state.sessions, "", &state.profile_system_releases());
    assert_eq!(
        rows[0].profiles.row_data(0).unwrap().system_platform,
        "debian"
    );
    assert_eq!(
        visible_workspace_tab_rows(state.tab_summaries())[0].system_platform,
        "debian"
    );
}

#[test]
fn release_updates_reuse_group_model_and_reject_changed_group_identity() {
    let mut server = SessionProfile::new("server", "server.example", "alice");
    server.group_name = "Production".into();
    let releases = HashMap::from([(server.id, "Ubuntu 24.04 LTS".to_owned())]);
    let mut sessions = SessionStore {
        sessions: vec![server],
        ..SessionStore::default()
    };
    let model = ModelRc::new(VecModel::from(session_group_rows(
        &sessions,
        "",
        &HashMap::new(),
    )));
    let rows = session_group_rows(&sessions, "", &releases);
    assert!(update_session_group_model(&model, &rows));
    assert_eq!(
        model
            .row_data(0)
            .unwrap()
            .profiles
            .row_data(0)
            .unwrap()
            .system_platform,
        "ubuntu"
    );

    sessions.sessions[0].group_name = "Staging".into();
    let moved_rows = session_group_rows(&sessions, "", &releases);
    assert!(!update_session_group_model(&model, &moved_rows));
    assert_eq!(model.row_data(0).unwrap().group_name, "Production");
    assert!(!update_session_group_model(&model, &[]));
}

#[test]
fn empty_persistent_groups_remain_visible() {
    let sessions = SessionStore {
        groups: vec!["Empty".into()],
        ..SessionStore::default()
    };

    let rows = session_group_rows(&sessions, "", &HashMap::new());

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name.as_str(), "Empty");
    assert_eq!(rows[0].profiles.row_count(), 0);
}

#[test]
fn connection_options_include_collapsed_profiles_with_masked_endpoints() {
    let visible = SessionProfile::new("visible", "server.example", "alice");
    let hidden = SessionProfile::new("hidden", "192.168.1.202", "zhushixin");
    let sessions = SessionStore {
        sessions: vec![visible.clone(), hidden.clone()],
        ..SessionStore::default()
    };

    let options = connection_option_rows(&sessions, &HashMap::new());

    assert_eq!(options.len(), 2);
    assert_eq!(options[0].id.as_str(), visible.id.to_string());
    assert_eq!(options[0].name.as_str(), "visible");
    assert_eq!(options[0].endpoint.as_str(), "al*ce@server.example:22");
    assert_eq!(options[1].id.as_str(), hidden.id.to_string());
    assert_eq!(options[1].name.as_str(), "hidden");
    assert_eq!(options[1].endpoint.as_str(), "zh*in@192.*.1.202:22");
    assert!(
        options
            .iter()
            .all(|option| option.system_platform.is_empty())
    );
}

#[test]
fn connection_options_share_sidebar_icon_priority_and_reject_stale_detection() {
    let mut manual = SessionProfile::new("Manual", "manual.example", "alice");
    manual.system_icon_override = Some("UBUNTU".into());
    manual.detected_system =
        ax_ssh::config::SystemDetection::new(&manual, "Debian 12", "debian").map(Box::new);
    let mut cached = SessionProfile::new("Cached", "cached.example", "alice");
    cached.detected_system =
        ax_ssh::config::SystemDetection::new(&cached, "Debian 12", "debian").map(Box::new);
    let runtime = SessionProfile::new("Runtime", "runtime.example", "alice");
    let unknown = SessionProfile::new("Unknown", "unknown.example", "alice");
    let mut stale = SessionProfile::new("Stale", "old.example", "alice");
    stale.detected_system =
        ax_ssh::config::SystemDetection::new(&stale, "Debian 12", "debian").map(Box::new);
    stale.ssh_mut().unwrap().host = "new.example".into();
    let releases = HashMap::from([
        (manual.id, "Fedora Linux 42".into()),
        (cached.id, "Fedora Linux 42".into()),
        (runtime.id, "Microsoft Windows Server 2022".into()),
    ]);
    let mut sessions = SessionStore {
        sessions: vec![manual, cached, runtime, unknown, stale],
        ..SessionStore::default()
    };

    let options = connection_option_rows(&sessions, &releases);
    let platforms = options
        .iter()
        .map(|option| option.system_platform.as_str())
        .collect::<Vec<_>>();
    assert_eq!(platforms, ["ubuntu", "debian", "windows", "", ""]);
    let sidebar = session_group_rows(&sessions, "", &releases);
    for option in &options {
        let profile = sidebar
            .iter()
            .flat_map(|group| group.profiles.iter())
            .find(|profile| profile.id == option.id)
            .expect("connection option should have a sidebar profile");
        assert_eq!(option.system_platform, profile.system_platform);
    }

    sessions.sessions[0].system_icon_override = None;
    assert_eq!(
        connection_option_rows(&sessions, &releases)[0].system_platform,
        "debian"
    );
    sessions.sessions[1].system_icon_override = Some("Rocky Linux".into());
    assert_eq!(
        connection_option_rows(&sessions, &releases)[1].system_platform,
        "rocky-linux"
    );
}
