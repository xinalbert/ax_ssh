use super::*;

#[tokio::test]
async fn hierarchy_wakes_paused_children_on_parent_resume_and_cancel() {
    let mut controls = TransferControls::new(Arc::new(TransferRateLimiter::new(None)));
    let root = Uuid::new_v4();
    let child = Uuid::new_v4();
    let root_control = controls.register(root, None);
    let child_control = controls.register(child, Some(root));
    controls.apply(SftpTransferAction::Pause, false, vec![root]);
    assert!(child_control.inherits_pause());
    let waiter_control = child_control.clone();
    let waiter = tokio::spawn(async move { waiter_control.wait_until_running().await });
    tokio::task::yield_now().await;
    assert!(!waiter.is_finished());
    controls.apply(SftpTransferAction::Resume, false, vec![child]);
    assert!(
        child_control.is_paused(),
        "child cannot override parent pause"
    );
    controls.apply(SftpTransferAction::Resume, false, vec![root]);
    waiter.await.expect("join").expect("parent resumes child");
    root_control.pause();
    let waiter = tokio::spawn(async move { child_control.wait_until_running().await });
    tokio::task::yield_now().await;
    root_control.cancel();
    assert!(waiter.await.expect("join").is_err());
}

#[test]
fn page_controls_cover_future_tasks_and_cancel_does_not_poison_new_tasks() {
    let mut controls = TransferControls::new(Arc::new(TransferRateLimiter::new(None)));
    controls.apply(SftpTransferAction::Pause, true, Vec::new());
    let id = Uuid::new_v4();
    let first = controls.register(id, None);
    assert!(first.is_paused());
    assert!(!controls.runnable(id));
    controls.apply(SftpTransferAction::Resume, true, Vec::new());
    assert!(!first.is_paused());
    first.pause();
    controls.apply(SftpTransferAction::Resume, true, Vec::new());
    assert!(
        !first.is_paused(),
        "Resume all clears individual pauses too"
    );
    controls.apply(SftpTransferAction::Cancel, true, Vec::new());
    assert!(first.is_cancelled());
    let next = controls.register(Uuid::new_v4(), None);
    assert!(!next.is_cancelled());
    assert!(!next.is_paused());
}

#[test]
fn prestart_tombstones_survive_sweeps_until_submission_and_storage_is_bounded() {
    let mut controls = TransferControls::new(Arc::new(TransferRateLimiter::new(None)));
    let id = Uuid::new_v4();
    controls.apply(SftpTransferAction::Cancel, true, vec![id]);
    controls.apply(SftpTransferAction::Cancel, false, vec![id]);
    controls.retain_live(&HashSet::new());
    assert!(
        controls.register(id, None).is_cancelled(),
        "late submission stays cancelled"
    );
    controls.retain_live(&HashSet::new());
    assert!(controls.entries.is_empty());
    for _ in 0..MAX_CONTROLS {
        controls.register(Uuid::new_v4(), None);
    }
    let excess_id = Uuid::new_v4();
    assert!(controls.register(excess_id, None).is_cancelled());
    assert!(
        controls.get(excess_id).is_cancelled(),
        "unadmitted work must never run"
    );
    assert_eq!(controls.entries.len(), MAX_CONTROLS);
}
