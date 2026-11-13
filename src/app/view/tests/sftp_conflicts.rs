use super::*;
use i_slint_core::accessibility::{AccessibilityAction, AccessibleStringProperty};
use i_slint_core::item_tree::ItemRc;
use i_slint_core::window::WindowInner;
use std::cell::RefCell;
use std::ops::ControlFlow;

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

#[test]
fn sftp_conflict_dialog_routes_choices_resets_batch_and_yields_to_security() {
    for detached in [false, true] {
        std::thread::spawn(move || {
            let harness = LayoutTestHarness::new();
            harness.ui.set_detached_window(detached);
            slint::select_bundled_translation("en").expect("English");
            let choices = Rc::new(RefCell::new(Vec::new()));
            let recorded = choices.clone();
            harness
                .ui
                .on_resolve_sftp_transfer_conflict(move |id, choice, batch| {
                    recorded
                        .borrow_mut()
                        .push((id.to_string(), choice.to_string(), batch));
                });
            harness.ui.set_sftp_transfer_conflict_open(true);
            harness
                .ui
                .set_sftp_transfer_conflict_name("report.txt".into());
            harness.ui.show().expect("headless show");
            harness.window.set_size(slint::PhysicalSize::new(1180, 740));
            for upload in [false, true] {
                harness.ui.set_sftp_transfer_conflict_upload(upload);
                harness.ui.set_sftp_transfer_conflict_id("first".into());
                harness.settle();
                control(&harness, "Skip")
                    .expect("skip")
                    .accessible_action(&AccessibilityAction::Default);
                assert_eq!(
                    choices.borrow().last(),
                    Some(&("first".into(), "skip".into(), false))
                );
                let batch = if upload {
                    "Apply this choice to this upload batch"
                } else {
                    "Apply this choice to this download batch"
                };
                control(&harness, batch)
                    .expect("batch checkbox")
                    .accessible_action(&AccessibilityAction::Default);
                harness.settle();
                control(&harness, "Overwrite")
                    .expect("overwrite")
                    .accessible_action(&AccessibilityAction::Default);
                assert_eq!(
                    choices.borrow().last(),
                    Some(&("first".into(), "overwrite".into(), true))
                );
                harness.ui.set_sftp_transfer_conflict_id("next".into());
                harness.settle();
                control(&harness, "Keep both")
                    .expect("keep both")
                    .accessible_action(&AccessibilityAction::Default);
                assert_eq!(
                    choices.borrow().last(),
                    Some(&("next".into(), "keep-both".into(), false))
                );
            }
            harness.ui.set_password_dialog_open(true);
            harness.ui.set_password_dialog_tab_id("security".into());
            harness.settle();
            assert!(
                control(&harness, "Keep both").is_none(),
                "security owns the modal"
            );
            harness.ui.hide().expect("headless hide");
        })
        .join()
        .expect("conflict interaction");
    }
}
