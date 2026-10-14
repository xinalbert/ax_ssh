use super::*;
use i_slint_core::item_tree::ItemRc;
use i_slint_core::items::{ComplexText, InputType, TextInput, TextWrap};
use i_slint_core::lengths::LogicalPoint;
use i_slint_core::window::{WindowInner, WindowKind};
use slint::platform::{WindowAdapter, WindowEvent};
use std::ops::ControlFlow;
use std::time::Duration;

fn displayed_text(harness: &LayoutTestHarness, value: &str) -> ItemRc {
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
    .expect("visible display text")
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
                  _,
                  connect| {
                assert!(connect, "Save & connect keeps its intent");
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
