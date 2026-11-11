use super::*;
use i_slint_core::accessibility::AccessibleStringProperty;
use i_slint_core::item_tree::ItemRc;
use i_slint_core::lengths::LogicalPoint;
use i_slint_core::window::WindowInner;
use slint::platform::Key;
use std::ops::ControlFlow;

fn action(harness: &LayoutTestHarness, label: &str) -> Option<ItemRc> {
    ItemRc::new_root(WindowInner::from_pub(harness.ui.window()).component()).visit_descendants(
        |item| {
            if item.is_visible()
                && item
                    .accessible_string_property(AccessibleStringProperty::Label)
                    .as_deref()
                    == Some(label)
            {
                return ControlFlow::Break(item.clone());
            }
            ControlFlow::Continue(())
        },
    )
}

fn click(harness: &LayoutTestHarness, label: &str) {
    let item = action(harness, label).unwrap_or_else(|| panic!("missing action: {label}"));
    let size = item.geometry().size;
    let point = item.map_to_window(LogicalPoint::new(size.width / 2.0, size.height / 2.0));
    harness.click(slint::LogicalPosition::new(point.x, point.y));
}

fn sessions() -> SessionStore {
    let mut production = SessionProfile::new("Production server", "prod.example", "fixture");
    production.group_name = "Production".into();
    SessionStore {
        sessions: vec![
            production,
            SessionProfile::new("Ungrouped server", "other.example", "fixture"),
        ],
        ..Default::default()
    }
}

#[test]
fn sidebar_modes_keep_each_groups_disclosure_through_clicks_keys_and_refreshes() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        let sessions = sessions();
        harness
            .ui
            .set_sessions(ModelRc::new(VecModel::from(session_group_rows(
                &sessions,
                "",
                &HashMap::new(),
            ))));
        harness.ui.show().unwrap();
        harness.window.set_size(slint::PhysicalSize::new(1180, 740));
        harness.settle();

        assert!(action(&harness, "Expand Production group").is_some());
        click(&harness, "Collapse session sidebar");
        assert!(
            action(&harness, "Expand Production group").is_some(),
            "switching to the rail must keep the default collapsed group"
        );
        assert!(action(&harness, "Connect to Production server").is_none());
        click(&harness, "Expand Production group");
        assert!(action(&harness, "Connect to Production server").is_some());
        assert!(action(&harness, "Connect to Ungrouped server").is_none());

        click(&harness, "Expand session sidebar");
        assert!(action(&harness, "Collapse Production group").is_some());
        assert!(action(&harness, "Connect to Production server").is_some());
        click(&harness, "Collapse Production group");
        harness.press_key(Key::Return);
        assert!(action(&harness, "Connect to Production server").is_some());
        harness.press_key(Key::Space);
        assert!(action(&harness, "Connect to Production server").is_none());
        click(&harness, "Collapse session sidebar");
        assert!(action(&harness, "Expand Production group").is_some());
        click(&harness, "Expand Production group");
        harness.press_key(Key::Space);
        assert!(action(&harness, "Connect to Production server").is_none());
        harness.press_key(Key::Return);
        assert!(action(&harness, "Connect to Production server").is_some());
        click(&harness, "Expand Ungrouped group");

        let releases = HashMap::from([(sessions.sessions[0].id, "Ubuntu 24.04 LTS".into())]);
        assert!(update_session_group_model(
            &harness.ui.get_sessions(),
            &session_group_rows(&sessions, "", &releases),
        ));
        harness.settle();
        click(&harness, "Expand session sidebar");
        assert!(action(&harness, "Connect to Production server").is_some());
        assert!(action(&harness, "Connect to Ungrouped server").is_some());
        harness.ui.hide().unwrap();
    })
    .join()
    .expect("sidebar disclosure regression");
}

#[test]
fn searching_expands_groups_and_sidebar_switches_keep_manual_disclosure() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        harness
            .ui
            .set_sessions(ModelRc::new(VecModel::from(session_group_rows(
                &sessions(),
                "",
                &HashMap::new(),
            ))));
        harness.ui.show().unwrap();
        harness.window.set_size(slint::PhysicalSize::new(1180, 740));
        harness.settle();
        harness.ui.set_session_filter("server".into());
        harness.settle();
        assert!(action(&harness, "Connect to Production server").is_some());
        assert!(action(&harness, "Connect to Ungrouped server").is_some());
        click(&harness, "Collapse Production group");
        click(&harness, "Collapse session sidebar");
        assert!(action(&harness, "Expand Production group").is_some());
        assert!(action(&harness, "Connect to Production server").is_none());
        assert!(action(&harness, "Connect to Ungrouped server").is_some());
        click(&harness, "Expand session sidebar");
        assert!(action(&harness, "Expand Production group").is_some());
        harness.ui.set_session_filter("prod".into());
        harness.settle();
        assert!(action(&harness, "Connect to Production server").is_some());
        harness.ui.hide().unwrap();
    })
    .join()
    .expect("search disclosure regression");
}
