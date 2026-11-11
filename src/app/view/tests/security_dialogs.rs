use super::*;
use i_slint_core::accessibility::{AccessibilityAction, AccessibleStringProperty};
use i_slint_core::item_tree::ItemRc;
use i_slint_core::items::{ComplexText, InputType, TextInput};
use i_slint_core::lengths::LogicalPoint;
use i_slint_core::window::WindowInner;
use std::ops::ControlFlow;

fn find(harness: &LayoutTestHarness, predicate: impl Fn(&ItemRc) -> bool) -> Option<ItemRc> {
    ItemRc::new_root(WindowInner::from_pub(harness.ui.window()).component()).visit_descendants(
        |item| {
            if item.is_visible() && predicate(item) {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        },
    )
}

fn secret_input(harness: &LayoutTestHarness) -> Option<ItemRc> {
    find(harness, |item| {
        item.downcast::<TextInput>()
            .is_some_and(|input| input.as_pin_ref().input_type() == InputType::Password)
    })
}

fn click(harness: &LayoutTestHarness, label: &str) {
    let item = find(harness, |item| {
        item.accessible_string_property(AccessibleStringProperty::Label)
            .as_deref()
            == Some(label)
    })
    .unwrap_or_else(|| panic!("missing credential action: {label}"));
    let size = item.geometry().size;
    let point = item.map_to_window(LogicalPoint::new(size.width / 2.0, size.height / 2.0));
    harness.click(slint::LogicalPosition::new(point.x, point.y));
}

fn has_keychain_hint(harness: &LayoutTestHarness, expected: &str) -> bool {
    find(harness, |item| {
        item.downcast::<ComplexText>().is_some_and(|text| {
            ComplexText::FIELD_OFFSETS
                .text()
                .apply_pin(text.as_pin_ref())
                .get()
                .contains(expected)
        })
    })
    .is_some()
}

#[test]
fn keychain_restart_hint_follows_platform_language_and_credential_mode() {
    for detached in [false, true] {
        std::thread::spawn(move || {
            let harness = LayoutTestHarness::new();
            harness.ui.set_detached_window(detached);
            harness.ui.set_password_dialog_tab_id("hint-fixture".into());
            harness.ui.set_password_dialog_open(true);
            harness.ui.set_password_credential_unavailable(true);
            harness.ui.show().unwrap();
            harness.window.set_size(slint::PhysicalSize::new(960, 640));
            for apple in [false, true] {
                harness.ui.set_apple_platform(apple);
                harness.ui.set_password_private_key(false);
                harness.ui.set_password_credential_unavailable(true);
                for (locale, hint) in [("en", "Always Allow"), ("zh-CN", "始终允许")] {
                    slint::select_bundled_translation(locale).unwrap();
                    harness.settle();
                    assert_eq!(has_keychain_hint(&harness, hint), apple);
                    assert!(secret_input(&harness).is_none());
                }
                slint::select_bundled_translation("en").unwrap();
                harness.ui.set_password_credential_unavailable(false);
                harness.settle();
                assert!(!has_keychain_hint(&harness, "Always Allow"));
                let remember = find(&harness, |item| {
                    item.accessible_string_property(AccessibleStringProperty::Label)
                        .as_deref()
                        == Some("Save password (optional)")
                })
                .expect("remember password checkbox");
                remember.accessible_action(&AccessibilityAction::Default);
                harness.settle();
                assert_eq!(
                    has_keychain_hint(&harness, "Always Allow"),
                    apple,
                    "save hint: detached={detached}, apple={apple}, checked={:?}",
                    remember.accessible_string_property(AccessibleStringProperty::Checked),
                );
                harness.ui.set_password_private_key(true);
                harness.settle();
                assert!(!has_keychain_hint(&harness, "Always Allow"));
            }
            harness.ui.hide().unwrap();
        })
        .join()
        .expect("keychain persistence hint");
    }
}

#[test]
fn credential_failure_dialog_has_no_secret_fields_and_dispatches_explicit_choices() {
    std::thread::spawn(|| {
        let harness = LayoutTestHarness::new();
        let choice = Rc::new(std::cell::Cell::new(None));
        let chosen = choice.clone();
        harness
            .ui
            .on_resolve_credential_access(move |retry| chosen.set(Some(retry)));
        let cancelled = Rc::new(std::cell::Cell::new(false));
        let cancel = cancelled.clone();
        harness
            .ui
            .on_cancel_password_dialog(move || cancel.set(true));
        harness
            .ui
            .set_password_dialog_tab_id("credential-fixture".into());
        harness.ui.set_password_dialog_open(true);
        harness.ui.set_password_credential_unavailable(true);
        harness.ui.show().unwrap();
        harness.window.set_size(slint::PhysicalSize::new(1180, 740));
        harness.settle();
        assert!(secret_input(&harness).is_none());
        click(&harness, "Retry");
        assert_eq!(choice.get(), Some(true));
        click(&harness, "Enter SSH password");
        assert_eq!(choice.get(), Some(false));
        click(&harness, "Cancel");
        assert!(cancelled.get());

        // Switching modes on the same tab must destroy any previous secret draft.
        harness.ui.set_password_credential_unavailable(false);
        harness.settle();
        let input = secret_input(&harness).expect("manual server password field");
        input
            .downcast::<TextInput>()
            .unwrap()
            .as_pin_ref()
            .text
            .set("test-only-draft".into());
        harness.ui.set_password_credential_unavailable(true);
        harness.settle();
        assert!(secret_input(&harness).is_none());
        harness.ui.set_password_credential_unavailable(false);
        harness.settle();
        let input = secret_input(&harness).unwrap();
        assert!(
            input
                .downcast::<TextInput>()
                .unwrap()
                .as_pin_ref()
                .text()
                .is_empty()
        );
        harness.ui.hide().unwrap();
    })
    .join()
    .expect("credential choice interaction");
}
