use super::*;
use i_slint_core::item_tree::ItemRc;
use i_slint_core::items::{
    BasicBorderRectangle, BorderRectangle, ComplexText, InputType, TextInput, TextWrap,
};
use i_slint_core::lengths::LogicalPoint;
use i_slint_core::window::{WindowInner, WindowKind};
use slint::platform::{WindowAdapter, WindowEvent};
use std::ops::ControlFlow;
use std::time::Duration;

fn displayed_text(harness: &LayoutTestHarness, value: &str) -> ItemRc {
    find_displayed_text(harness, value).unwrap_or_else(|| panic!("missing visible text: {value}"))
}

fn find_displayed_text(harness: &LayoutTestHarness, value: &str) -> Option<ItemRc> {
    let root = ItemRc::new_root(WindowInner::from_pub(harness.ui.window()).component());
    root.visit_descendants(|item| {
        if item.is_visible()
            && let Some(text) = item.downcast::<ComplexText>()
            && ComplexText::FIELD_OFFSETS
                .text()
                .apply_pin(text.as_pin_ref())
                .get()
                .as_str()
                == value
        {
            return ControlFlow::Break(item.clone());
        }
        ControlFlow::Continue(())
    })
}

fn natural_width(harness: &LayoutTestHarness, item: &ItemRc) -> f32 {
    let text = item.downcast::<ComplexText>().expect("display text");
    harness
        .window
        .renderer()
        .text_size(text.as_pin_ref(), item, None, TextWrap::NoWrap)
        .width
}

fn hover(harness: &LayoutTestHarness, item: &ItemRc) {
    let size = item.geometry().size;
    let point = item.map_to_window(LogicalPoint::new(size.width / 2.0, size.height / 2.0));
    harness
        .ui
        .window()
        .dispatch_event(WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(point.x, point.y),
        });
    harness
        .time
        .set(harness.time.get() + Duration::from_secs(1));
    slint::platform::update_timers_and_animations();
    harness.settle();
}

fn tooltip_text(harness: &LayoutTestHarness) -> Option<SharedString> {
    let window = WindowInner::from_pub(harness.ui.window());
    let popups = window.active_popups();
    let popup = popups
        .iter()
        .rev()
        .find(|popup| matches!(popup.window_kind, WindowKind::ToolTip))?;
    ItemRc::new_root(popup.component.clone()).visit_descendants(|item| {
        if item.is_visible()
            && let Some(text) = item.downcast::<ComplexText>()
        {
            return ControlFlow::Break(
                ComplexText::FIELD_OFFSETS
                    .text()
                    .apply_pin(text.as_pin_ref())
                    .get(),
            );
        }
        ControlFlow::Continue(())
    })
}

fn assert_tooltip_surface_bounds(harness: &LayoutTestHarness) {
    let window = WindowInner::from_pub(harness.ui.window());
    let popups = window.active_popups();
    let popup = popups
        .iter()
        .find(|popup| matches!(popup.window_kind, WindowKind::ToolTip))
        .expect("visible tooltip");
    let root = ItemRc::new_root(popup.component.clone());
    let max_width = harness.ui.global::<Theme>().get_text_tooltip_max_width();
    assert!(
        root.geometry().size.width <= max_width + 0.5,
        "tooltip outer width {:?} exceeds content limit {max_width}",
        root.geometry().size,
    );
    let mut surfaces = Vec::new();
    root.visit_descendants(|item| {
        let background = if let Some(rect) = item.downcast::<BasicBorderRectangle>() {
            BasicBorderRectangle::FIELD_OFFSETS
                .background()
                .apply_pin(rect.as_pin_ref())
                .get()
        } else if let Some(rect) = item.downcast::<BorderRectangle>() {
            BorderRectangle::FIELD_OFFSETS
                .background()
                .apply_pin(rect.as_pin_ref())
                .get()
        } else {
            return ControlFlow::<()>::Continue(());
        };
        if item.is_visible() && background.color().alpha() > 0 {
            let bounds = item.geometry();
            surfaces.push((
                bounds.origin.x,
                bounds.origin.y,
                bounds.size.width,
                bounds.size.height,
                background.color(),
            ));
        }
        ControlFlow::<()>::Continue(())
    });
    assert_eq!(
        surfaces.len(),
        2,
        "the theme surface covers the standard frame"
    );
    assert_eq!(
        (surfaces[0].0, surfaces[0].1, surfaces[0].2, surfaces[0].3),
        (surfaces[1].0, surfaces[1].1, surfaces[1].2, surfaces[1].3),
        "the theme-colored surface must cover the standard frame exactly"
    );
    let theme_background = harness.ui.global::<Theme>().get_tooltip_background();
    assert_ne!(
        surfaces[0].4, theme_background,
        "fixture distinguishes the palette"
    );
    assert_eq!(
        surfaces[1].4, theme_background,
        "the visible tooltip surface must follow the application theme"
    );
}

fn show_editor(harness: &LayoutTestHarness, width: u32) {
    harness.ui.set_active_tab_kind("session-editor".into());
    harness.ui.set_editor_draft_id("overflow-test".into());
    harness.ui.show().expect("session editor window");
    // Keep the footer in the viewport; rendering resolves numbers only.
    harness
        .window
        .set_size(slint::PhysicalSize::new(width, 1600));
    harness.settle();
}

#[test]
fn tab_summary_hover_tracks_latency_without_taking_focus_or_pointer_actions() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        let mut tab = WorkspaceTabRow {
            id: "ssh-tab".into(),
            title: "SSH".into(),
            kind: "terminal".into(),
            connected: true,
            latency_state: "good".into(),
            latency_ms: 42,
            latency_updated_at: "15:30:00".into(),
            system_release: "Debian GNU/Linux 12".into(),
            ..Default::default()
        };
        let model = std::rc::Rc::new(VecModel::from(vec![tab.clone()]));
        ui.set_workspace_tabs(ModelRc::from(model.clone()));
        let activated = std::rc::Rc::new(std::cell::RefCell::new(None));
        let received = activated.clone();
        ui.on_activate_tab(move |id| *received.borrow_mut() = Some(id));
        let closed = std::rc::Rc::new(std::cell::RefCell::new(None));
        let received = closed.clone();
        ui.on_close_tab(move |id| *received.borrow_mut() = Some(id));
        ui.show().expect("tab hover window");
        harness.window.set_size(slint::PhysicalSize::new(1180, 740));
        let theme = ui.global::<Theme>();
        theme.set_mode("dark".into());
        theme.set_dark_panel_alt(slint::Color::from_rgb_u8(0x12, 0x34, 0x56));
        harness.settle();

        let title = displayed_text(&harness, "SSH");
        assert!(natural_width(&harness, &title) <= title.geometry().size.width + 0.5);
        let focus_before = WindowInner::from_pub(ui.window())
            .focus_item
            .borrow()
            .upgrade();
        hover(&harness, &title);
        assert_eq!(
            tooltip_text(&harness).as_deref(),
            Some("SSH\nDebian GNU/Linux 12\nConnected · SSH RTT: 42 ms\nUpdated: 15:30:00")
        );
        for value in [
            "albertxin@Mac-mini:/Volumes/albert_xin/2026/soft/axsoft/ax_ssh",
            "持续输出终端的连接详情和很长的本地工作目录：/Volumes/albert_xin/2026/soft/axsoft/ax_ssh",
            "SSH",
        ] {
            tab.title = value.into();
            model.set_row_data(0, tab.clone());
            harness.settle();
            hover(&harness, &displayed_text(&harness, value));
            assert_tooltip_surface_bounds(&harness);
        }
        assert_eq!(
            WindowInner::from_pub(ui.window())
                .focus_item
                .borrow()
                .upgrade(),
            focus_before,
            "hover must keep keyboard focus in the current page"
        );

        tab.latency_ms = 73;
        tab.latency_updated_at = "15:30:10".into();
        model.set_row_data(0, tab.clone());
        harness.settle();
        assert_eq!(
            tooltip_text(&harness).as_deref(),
            Some("SSH\nDebian GNU/Linux 12\nConnected · SSH RTT: 73 ms\nUpdated: 15:30:10"),
            "an open tooltip must follow fresh samples"
        );

        for (state, message) in [
            ("measuring", "Connected · Measuring SSH latency"),
            ("timeout", "Connected · SSH latency probe timed out"),
            ("stale", "Connected · SSH latency data expired"),
            ("unavailable", "Connected · SSH latency unavailable"),
            ("", "Connected"),
        ] {
            tab.latency_state = state.into();
            model.set_row_data(0, tab.clone());
            harness.settle();
            assert_eq!(
                tooltip_text(&harness).as_deref(),
                Some(format!("SSH\nDebian GNU/Linux 12\n{message}").as_str()),
                "{state} must not expose the previous numeric reading"
            );
        }
        tab.connected = false;
        model.set_row_data(0, tab);
        harness.settle();
        assert_eq!(
            tooltip_text(&harness).as_deref(),
            Some("SSH\nDebian GNU/Linux 12"),
            "disconnect must remove connection data"
        );

        let size = title.geometry().size;
        let point = title.map_to_window(LogicalPoint::new(size.width / 2.0, size.height / 2.0));
        harness.click(slint::LogicalPosition::new(point.x, point.y));
        assert_eq!(activated.borrow().as_deref(), Some("ssh-tab"));
        let close = ItemRc::new_root(WindowInner::from_pub(ui.window()).component())
            .visit_descendants(|item| {
                if item.is_visible()
                    && item
                        .accessible_string_property(
                            i_slint_core::accessibility::AccessibleStringProperty::Label,
                        )
                        .as_deref()
                        == Some("Close tab")
                {
                    return ControlFlow::Break(item.clone());
                }
                ControlFlow::Continue(())
            })
            .expect("accessible tab close control");
        hover(&harness, &close);
        assert!(
            tooltip_text(&harness).is_none(),
            "close keeps its own region"
        );
        let point = close.map_to_window(LogicalPoint::new(
            close.geometry().size.width / 2.0,
            close.geometry().size.height / 2.0,
        ));
        harness.click(slint::LogicalPosition::new(point.x, point.y));
        assert_eq!(closed.borrow().as_deref(), Some("ssh-tab"));
        ui.hide().expect("hide tab hover window");
    })
    .join()
    .expect("tab hover and pointer regression");
}

#[test]
fn shared_text_buttons_fit_session_actions_across_languages_and_fonts() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        show_editor(&harness, 1180);
        for (locale, labels) in [
            ("en", ["Cancel", "Save & connect", "Save session"]),
            (
                "de",
                ["Abbrechen", "Speichern und verbinden", "Sitzung speichern"],
            ),
            ("zh-CN", ["取消", "保存并连接", "保存会话"]),
        ] {
            super::super::settings::apply_ui_language_to_open_windows(
                &harness.ui,
                UiLanguage::from_setting(locale),
            )
            .expect("bundled locale");
            for family in ["JetBrains Mono", "sans-serif"] {
                harness
                    .ui
                    .global::<Theme>()
                    .set_application_font_family(family.into());
                for width in [1180, 900] {
                    harness
                        .window
                        .set_size(slint::PhysicalSize::new(width, 1600));
                    harness.settle();
                    for label in labels {
                        let item = displayed_text(&harness, label);
                        assert!(
                            natural_width(&harness, &item) <= item.geometry().size.width + 0.5,
                            "{locale}/{family}/{width}: {label} must fit its button"
                        );
                    }
                }
            }
        }
        harness.ui.hide().expect("hide session editor");
    })
    .join()
    .expect("session action sizing regression");
}

#[test]
fn forwarding_manager_keeps_table_compact_and_description_on_tab_hover() {
    use i_slint_core::item_tree::ParentItemTraversalMode;
    use i_slint_core::items::{ClippedImage, Flickable, ImageItem};

    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        let kind = "forwarding-manager";
        let title = "Forwarding Manager";
        let description = "Manage forwarding rules from all saved SSH servers.";
        ui.set_active_tab_kind(kind.into());
        ui.set_active_tab_id(kind.into());
        ui.set_workspace_tabs(ModelRc::new(VecModel::from(vec![WorkspaceTabRow {
            id: kind.into(),
            title: title.into(),
            kind: kind.into(),
            ..Default::default()
        }])));
        ui.set_forwarding_manager_rows(ModelRc::new(VecModel::from(vec![
            ForwardingManagerRow {
                profile_id: "first".into(),
                profile_name: "First SSH host".into(),
                mode: "local".into(),
                listen: "127.0.0.1:18081".into(),
                target: "localhost:81".into(),
                enabled: true,
                ..Default::default()
            },
            ForwardingManagerRow {
                profile_id: "second".into(),
                profile_name: "Second SSH host".into(),
                profile_index: 1,
                history_index: 2,
                mode: "remote".into(),
                listen: "127.0.0.1:19082".into(),
                target: "localhost:82".into(),
                ..Default::default()
            },
        ])));
        let changes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let received = changes.clone();
        ui.on_forwarding_manager_toggle(move |row, enabled| {
            received
                .borrow_mut()
                .push((row.profile_id, row.history_index, enabled));
        });
        ui.show().expect("forwarding manager layout window");
        let row_height = ui.global::<Theme>().get_sftp_row_height();
        for width in [1180, 520] {
            harness
                .window
                .set_size(slint::PhysicalSize::new(width, 740));
            harness.settle();
            for (column, cell) in [
                ("Server", "First SSH host"),
                ("Listen", "127.0.0.1:19082"),
                ("Destination", "localhost:82"),
            ] {
                let header = displayed_text(&harness, column);
                let row = displayed_text(&harness, cell);
                let left = |item: &ItemRc| item.map_to_window(LogicalPoint::default()).x;
                assert!(
                    (left(&header) - left(&row)).abs() < 0.5,
                    "{column} at {width}px"
                );
            }
            let mut ancestor = Some(displayed_text(&harness, "Second SSH host"));
            let viewport = loop {
                let item = ancestor.expect("forwarding ListView viewport");
                if item.downcast::<Flickable>().is_some() {
                    break item;
                }
                ancestor = item.parent_item(ParentItemTraversalMode::StopAtPopups);
            };
            assert!((viewport.geometry().size.height - 2.0 * row_height).abs() <= 4.0);
        }
        for name in ["First SSH host", "Second SSH host"] {
            let row = displayed_text(&harness, name);
            let enabled = displayed_text(&harness, "Enabled");
            let x = enabled.map_to_window(LogicalPoint::new(8.0, 0.0)).x;
            let y = row
                .map_to_window(LogicalPoint::new(0.0, row_height / 2.0))
                .y;
            harness.click(slint::LogicalPosition::new(x, y));
        }
        assert_eq!(
            changes.borrow().as_slice(),
            &[("first".into(), 0, false), ("second".into(), 2, true)]
        );
        assert!(find_displayed_text(&harness, description).is_none());
        hover(&harness, &displayed_text(&harness, title));
        assert_eq!(
            tooltip_text(&harness).as_deref(),
            Some(format!("{title}\n{description}").as_str())
        );

        ui.set_forwarding_manager_profiles(ModelRc::new(VecModel::from(vec![
            ServerOption {
                id: "first".into(),
                name: "Group / First SSH host".into(),
                system_platform: "ubuntu".into(),
            },
            ServerOption {
                id: "second".into(),
                name: "Group / Second SSH host".into(),
                system_platform: "debian".into(),
            },
        ])));
        harness.settle();
        assert!(find_displayed_text(&harness, "New forwarding").is_none());
        let click = |label| {
            let item = displayed_text(&harness, label);
            let size = item.geometry().size;
            let point = item.map_to_window(LogicalPoint::new(size.width / 2.0, size.height / 2.0));
            harness.click(slint::LogicalPosition::new(point.x, point.y));
            harness.settle();
        };
        super::wire_menu_helpers(ui);
        let checked = std::rc::Rc::new(std::cell::RefCell::new(None));
        let received = checked.clone();
        ui.on_forwarding_manager_submit(move |request, revision| {
            *received.borrow_mut() = Some((request, revision));
        });
        click("…");
        harness.press_key(slint::platform::Key::Return);
        let (request, revision) = checked.borrow_mut().take().expect("row check request");
        assert_eq!(request.action, "check");
        assert_eq!(request.original.profile_id, "first");
        ui.invoke_finish_forwarding_manager(revision, "offline".into(), "".into());
        harness.settle();
        displayed_text(&harness, "Configuration valid. SSH server is offline.");
        click("Close");

        use slint::platform::{Key, PointerEventButton};
        let second = displayed_text(&harness, "Second SSH host")
            .map_to_window(LogicalPoint::new(8.0, row_height / 2.0));
        let right_click = |y| {
            let position = slint::LogicalPosition::new(second.x, y);
            let button = PointerEventButton::Right;
            ui.window()
                .dispatch_event(WindowEvent::PointerMoved { position });
            ui.window()
                .dispatch_event(WindowEvent::PointerPressed { position, button });
            ui.window()
                .dispatch_event(WindowEvent::PointerReleased { position, button });
            harness.settle();
        };
        right_click(second.y);
        harness.press_key(Key::End);
        harness.press_key(Key::Return);
        assert!(ui.get_modal_open());

        let icon_size = ui.global::<Theme>().get_sidebar_glyph_size();
        let option_icons = |root: &ItemRc, label: Option<&ItemRc>| {
            let mut icons = Vec::new();
            root.visit_descendants(|item| {
                let size = item.geometry().size;
                if !item.is_visible()
                    || (size.width - icon_size).abs() > 0.5
                    || (size.height - icon_size).abs() > 0.5
                {
                    return ControlFlow::<()>::Continue(());
                }
                if let Some(label) = label {
                    let center =
                        item.map_to_window(LogicalPoint::new(size.width / 2.0, size.height / 2.0));
                    let label_start = label
                        .map_to_window(LogicalPoint::new(0.0, label.geometry().size.height / 2.0));
                    if (center.y - label_start.y).abs() > 0.5 || center.x >= label_start.x {
                        return ControlFlow::Continue(());
                    }
                }
                if let Some(image) = item.downcast::<ImageItem>() {
                    icons.push(image.as_pin_ref().source());
                } else if let Some(image) = item.downcast::<ClippedImage>() {
                    icons.push(image.as_pin_ref().source());
                }
                ControlFlow::Continue(())
            });
            icons
        };
        let window = WindowInner::from_pub(ui.window());
        let main = ItemRc::new_root(window.component());
        let second_icon = option_icons(
            &main,
            Some(&displayed_text(&harness, "Group / Second SSH host")),
        );
        assert_eq!(second_icon.len(), 1);
        click("Group / Second SSH host");
        let popup_icons = {
            let popups = window.active_popups();
            let popup = popups
                .iter()
                .find(|popup| matches!(popup.window_kind, WindowKind::Popup))
                .expect("server dropdown popup");
            option_icons(&ItemRc::new_root(popup.component.clone()), None)
        };
        assert_eq!(popup_icons.len(), 2);
        assert!(popup_icons.iter().all(|icon| icon.size().width > 0));
        assert_eq!(second_icon[0], popup_icons[1]);
        harness.press_key(Key::UpArrow);
        harness.press_key(Key::Return);
        let first_icon = option_icons(
            &main,
            Some(&displayed_text(&harness, "Group / First SSH host")),
        );
        assert_eq!(first_icon.as_slice(), &popup_icons[..1]);
        assert_ne!(first_icon, second_icon);
        for (placeholder, value) in [
            ("Listening port or Unix socket path", "19080"),
            (
                "Destination host:port or Unix socket path",
                "localhost:8080",
            ),
        ] {
            click(placeholder);
            ui.window()
                .dispatch_event(WindowEvent::KeyPressed { text: value.into() });
            ui.window()
                .dispatch_event(WindowEvent::KeyReleased { text: value.into() });
            harness.settle();
        }
        click("Save");
        let (request, revision) = checked.borrow_mut().take().expect("new rule request");
        assert_eq!(request.action, "new");
        assert_eq!(request.profile_id, "first");
        ui.invoke_finish_forwarding_manager(revision, "saved".into(), "".into());
        harness.settle();
        assert!(!ui.get_modal_open());

        for clear_rows in [false, true] {
            if clear_rows {
                ui.set_forwarding_manager_rows(ModelRc::default());
                harness.settle();
            }
            right_click(second.y + 100.0);
            harness.press_key(Key::Return);
            assert!(
                ui.get_modal_open(),
                "background menu opens with empty={clear_rows}"
            );
            assert!(find_displayed_text(&harness, "Group / First SSH host").is_none());
            assert!(find_displayed_text(&harness, "Group / Second SSH host").is_none());
            click("Cancel");
        }

        ui.set_connection_options(ModelRc::new(VecModel::from(vec![ConnectableSessionRow {
            id: "first".into(),
            name: "Saved SSH host".into(),
            endpoint: "alice@server.example".into(),
            system_platform: "ubuntu".into(),
        }])));
        let connection = std::rc::Rc::new(std::cell::RefCell::new(None));
        let received = connection.clone();
        ui.on_connect_session(move |id| *received.borrow_mut() = Some(id));
        use i_slint_core::accessibility::AccessibleStringProperty;
        let add = main
            .visit_descendants(|item| {
                if item.is_visible()
                    && item
                        .accessible_string_property(AccessibleStringProperty::Label)
                        .as_deref()
                        == Some("Open saved connection")
                {
                    return ControlFlow::Break(item.clone());
                }
                ControlFlow::Continue(())
            })
            .expect("saved connection picker button");
        let size = add.geometry().size;
        let point = add.map_to_window(LogicalPoint::new(size.width / 2.0, size.height / 2.0));
        harness.click(slint::LogicalPosition::new(point.x, point.y));
        let name = displayed_text(&harness, "Saved SSH host");
        let endpoint = displayed_text(&harness, "alice@server.example");
        let top = name.map_to_window(LogicalPoint::default());
        let below = endpoint.map_to_window(LogicalPoint::default());
        assert!((top.x - below.x).abs() < 0.5);
        assert!(below.y >= top.y + name.geometry().size.height);
        assert!(option_icons(&main, None).contains(&first_icon[0]));
        click("alice@server.example");
        assert_eq!(connection.borrow().as_deref(), Some("first"));

        ui.hide().expect("hide forwarding manager window");
    })
    .join()
    .expect("forwarding manager table regression");
}

#[test]
fn sftp_transfer_counts_fit_their_labels_when_the_toolbar_has_room() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        ui.set_active_tab_kind("sftp".into());
        ui.set_sftp_available(true);
        ui.set_sftp_open(true);
        ui.global::<Theme>()
            .set_application_font_family("JetBrains Mono".into());
        ui.show().expect("SFTP layout window");
        harness.window.set_size(slint::PhysicalSize::new(1180, 740));
        for (locale, labels) in [
            ("en", ["Transferring", "Failed", "Success"]),
            ("de", ["Wird übertragen", "Fehlgeschlagen", "Erfolg"]),
            ("zh-CN", ["传输中", "失败", "成功"]),
        ] {
            super::super::settings::apply_ui_language_to_open_windows(
                ui,
                UiLanguage::from_setting(locale),
            )
            .expect("bundled locale");
            for counts in [[1, 2, 3], [1234, 5678, 9012]] {
                ui.set_sftp_transfer_active_count(counts[0]);
                ui.set_sftp_transfer_failed_count(counts[1]);
                ui.set_sftp_transfer_completed_count(counts[2]);
                harness.settle();
                for (label, count) in labels.into_iter().zip(counts) {
                    let label = format!("{label} ({count})");
                    let item = displayed_text(&harness, &label);
                    let required = natural_width(&harness, &item);
                    let allocated = item.geometry().size.width;
                    assert!(
                        allocated + 0.5 >= required,
                        "{locale}: {label} has {allocated}px and needs {required}px"
                    );
                }
            }
        }
        ui.hide().expect("hide SFTP layout window");
    })
    .join()
    .expect("SFTP transfer count sizing regression");
}

#[test]
fn shared_text_overflow_hover_preserves_full_label_and_button_click() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        let saves = std::rc::Rc::new(std::cell::Cell::new(0));
        let received = saves.clone();
        ui.on_save_session(
            move |_,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  _,
                  connect,
                  forward_agent| {
                assert!(connect, "Save & connect keeps its intent");
                assert!(!forward_agent, "Agent forwarding defaults off");
                received.set(received.get() + 1);
            },
        );
        show_editor(&harness, 1180);
        let label = "Save & connect";
        hover(&harness, &displayed_text(&harness, label));
        assert!(
            tooltip_text(&harness).is_none(),
            "fitting text needs no hint"
        );

        harness.window.set_size(slint::PhysicalSize::new(520, 1600));
        harness.settle();
        let item = displayed_text(&harness, label);
        assert!(natural_width(&harness, &item) > item.geometry().size.width + 0.5);
        let focused_before = WindowInner::from_pub(ui.window())
            .focus_item
            .borrow()
            .upgrade();
        hover(&harness, &item);
        assert_eq!(tooltip_text(&harness).as_deref(), Some(label));
        assert_eq!(
            WindowInner::from_pub(ui.window())
                .focus_item
                .borrow()
                .upgrade(),
            focused_before,
            "Tooltip must not take keyboard focus"
        );
        let size = item.geometry().size;
        let point = item.map_to_window(LogicalPoint::new(size.width / 2.0, size.height / 2.0));
        harness.click(slint::LogicalPosition::new(point.x, point.y));
        assert_eq!(saves.get(), 1, "Tooltip must not swallow button clicks");
        // Standard desktop buttons receive keyboard focus through Tab;
        // macOS does not focus them merely because they were clicked.
        let button_has_focus = || {
            use i_slint_core::accessibility::AccessibleStringProperty;
            use i_slint_core::item_tree::ParentItemTraversalMode;

            let mut item = WindowInner::from_pub(ui.window())
                .focus_item
                .borrow()
                .upgrade();
            while let Some(current) = item {
                if current
                    .accessible_string_property(AccessibleStringProperty::Label)
                    .as_deref()
                    == Some(label)
                {
                    return true;
                }
                item = current.parent_item(ParentItemTraversalMode::StopAtPopups);
            }
            false
        };
        for _ in 0..64 {
            if button_has_focus() {
                break;
            }
            harness.press_key(slint::platform::Key::Tab);
        }
        assert!(
            button_has_focus(),
            "the standard button remains Tab reachable"
        );
        harness.press_key(slint::platform::Key::Return);
        harness.press_key(slint::platform::Key::Space);
        assert_eq!(
            saves.get(),
            3,
            "the shared surface must preserve the standard button's focus and keyboard actions"
        );

        let description = "Select the transport used by this saved connection.";
        let item = displayed_text(&harness, description);
        assert!(natural_width(&harness, &item) > item.geometry().size.width + 0.5);
        hover(&harness, &item);
        assert_eq!(tooltip_text(&harness).as_deref(), Some(description));
        harness
            .window
            .set_size(slint::PhysicalSize::new(1180, 1600));
        harness.settle();
        hover(&harness, &displayed_text(&harness, description));
        assert!(
            tooltip_text(&harness).is_none(),
            "resize clears overflow hints"
        );
        ui.hide().expect("hide overflow window");
    })
    .join()
    .expect("shared hover and click regression");
}

#[test]
fn shared_text_overflow_password_hint_contains_only_empty_field_instructions() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        show_editor(&harness, 520);
        let placeholder = "Optional; enter only when connecting or saving";
        let item = displayed_text(&harness, placeholder);
        hover(&harness, &item);
        assert_eq!(tooltip_text(&harness).as_deref(), Some(placeholder));

        let size = item.geometry().size;
        let point = item.map_to_window(LogicalPoint::new(size.width / 2.0, size.height / 2.0));
        harness.click(slint::LogicalPosition::new(point.x, point.y));
        let secret = "test-only-private-value";
        harness.ui.window().dispatch_event(WindowEvent::KeyPressed {
            text: secret.into(),
        });
        harness
            .ui
            .window()
            .dispatch_event(WindowEvent::KeyReleased {
                text: secret.into(),
            });
        harness.settle();
        let focused = WindowInner::from_pub(harness.ui.window())
            .focus_item
            .borrow()
            .upgrade()
            .expect("focused password editor");
        let input = focused.downcast::<TextInput>().expect("password TextInput");
        assert_eq!(input.as_pin_ref().input_type(), InputType::Password);
        assert_eq!(input.as_pin_ref().text().as_str(), secret);
        hover(&harness, &item);
        assert!(
            tooltip_text(&harness).is_none(),
            "password values never become hints"
        );
        harness.ui.hide().expect("hide password hint window");
    })
    .join()
    .expect("protected placeholder hover regression");
}

#[test]
fn shared_text_overflow_hints_observe_hover_above_dropdown_and_menu_handlers() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        let ui = &harness.ui;
        let group = "A long group name that cannot fit the session editor group picker";
        ui.set_group_options(ModelRc::new(VecModel::from(vec![SharedString::from(
            group,
        )])));
        show_editor(&harness, 1180);
        let item = displayed_text(&harness, group);
        assert!(natural_width(&harness, &item) > item.geometry().size.width + 0.5);
        hover(&harness, &item);
        assert_eq!(tooltip_text(&harness).as_deref(), Some(group));
        let size = item.geometry().size;
        let point = item.map_to_window(LogicalPoint::new(size.width / 2.0, size.height / 2.0));
        harness.click(slint::LogicalPosition::new(point.x, point.y));
        assert!(
            WindowInner::from_pub(ui.window())
                .active_popups()
                .iter()
                .any(|popup| matches!(popup.window_kind, WindowKind::Popup)),
            "dropdown click still opens its options"
        );
        harness.press_key(slint::platform::Key::Escape);

        wire_menu_helpers(ui);
        let path =
            "/workspace/a-very-long-directory-name/another-directory/latest-results/session.axssh";
        ui.set_recent_workspace_paths(ModelRc::new(VecModel::from(vec![SharedString::from(path)])));
        harness.click(slint::LogicalPosition::new(20.0, 16.0));
        harness.press_key(slint::platform::Key::DownArrow);
        harness.press_key(slint::platform::Key::DownArrow);
        harness.press_key(slint::platform::Key::RightArrow);
        let popups = WindowInner::from_pub(ui.window()).active_popups();
        let popup = popups.last().expect("recent workspace submenu");
        let item = ItemRc::new_root(popup.component.clone())
            .visit_descendants(|item| {
                if item.is_visible()
                    && let Some(text) = item.downcast::<ComplexText>()
                    && ComplexText::FIELD_OFFSETS
                        .text()
                        .apply_pin(text.as_pin_ref())
                        .get()
                        .as_str()
                        == path
                {
                    return ControlFlow::Break(item.clone());
                }
                ControlFlow::Continue(())
            })
            .expect("recent workspace text");
        drop(popups);
        assert!(natural_width(&harness, &item) > item.geometry().size.width + 0.5);
        let size = item.geometry().size;
        let point =
            item.map_to_native_window(LogicalPoint::new(size.width / 2.0, size.height / 2.0));
        ui.window().dispatch_event(WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(point.x, point.y),
        });
        harness
            .time
            .set(harness.time.get() + Duration::from_secs(1));
        slint::platform::update_timers_and_animations();
        harness.settle();
        assert_eq!(tooltip_text(&harness).as_deref(), Some(path));
        ui.hide().expect("hide interactive hint window");
    })
    .join()
    .expect("interactive label hover regression");
}
