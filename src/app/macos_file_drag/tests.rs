use super::*;
use std::rc::Rc;

fn owner() -> FilePromiseOwner {
    FilePromiseOwner {
        tab_id: Uuid::new_v4(),
        attempt_id: Uuid::new_v4(),
    }
}

fn completion() -> (Rc<RefCell<Vec<bool>>>, PromiseCompletion) {
    let results = Rc::new(RefCell::new(Vec::new()));
    let output = results.clone();
    let callback = RcBlock::new(move |error: *mut NSError| {
        output.borrow_mut().push(error.is_null());
    });
    (results, callback)
}

#[test]
fn native_drop_region_requires_finite_non_empty_logical_geometry() {
    assert!(NativeDropRegion::from_logical(10.0, 20.0, 30.0, 40.0).is_some());
    assert!(NativeDropRegion::from_logical(-1.0, 0.0, 30.0, 40.0).is_none());
    assert!(NativeDropRegion::from_logical(0.0, 0.0, 0.0, 40.0).is_none());
    assert!(NativeDropRegion::from_logical(0.0, 0.0, f32::NAN, 40.0).is_none());
}

#[test]
fn mouse_down_events_preserve_the_origin_and_are_consumed_once() {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let mut events = MouseDownEvents::default();
    events
        .record(first, NSEventType::LeftMouseDown, 1, 1, "first press")
        .expect("first event");
    events
        .record(second, NSEventType::LeftMouseDown, 2, 2, "second press")
        .expect("second event");
    assert_eq!(
        events.take(first, 1, true).expect("original event"),
        "first press"
    );
    assert!(
        events.take(first, 1, true).is_err(),
        "one gesture cannot start twice"
    );
    assert_eq!(
        events.take(second, 2, true).expect("independent window"),
        "second press"
    );
}

#[test]
fn mouse_down_events_reject_release_wrong_window_and_stale_capture() {
    let window = Uuid::new_v4();
    let mut events = MouseDownEvents::default();
    for (kind, event_window, native_window) in [
        (NSEventType::LeftMouseUp, 1, 1),
        (NSEventType::LeftMouseDragged, 1, 1),
        (NSEventType::LeftMouseDown, 1, 2),
        (NSEventType::LeftMouseDown, 0, 0),
    ] {
        events
            .record(window, NSEventType::LeftMouseDown, 1, 1, "old press")
            .expect("old event");
        assert!(
            events
                .record(window, kind, event_window, native_window, "invalid")
                .is_err()
        );
        assert!(
            events.take(window, 1, true).is_err(),
            "rejected input clears the old capture"
        );
    }
    events
        .record(window, NSEventType::LeftMouseDown, 1, 1, "released")
        .expect("press");
    events.clear(window);
    assert!(
        events.take(window, 1, true).is_err(),
        "up is cleared before Slint flushes moved"
    );
    events
        .record(window, NSEventType::LeftMouseDown, 1, 1, "missed up")
        .expect("press");
    assert!(
        events.take(window, 1, false).is_err(),
        "physical button state is authoritative"
    );
    events
        .record(window, NSEventType::LeftMouseDown, 1, 1, "replaced window")
        .expect("press");
    assert!(events.take(window, 2, true).is_err());
}

#[test]
fn mouse_down_storage_is_bounded_and_replaces_an_existing_window() {
    let mut events = MouseDownEvents::default();
    let first = Uuid::new_v4();
    events
        .record(first, NSEventType::LeftMouseDown, 1, 1, 0)
        .expect("first press");
    for number in 1..MAX_MOUSE_DOWN_WINDOWS {
        events
            .record(Uuid::new_v4(), NSEventType::LeftMouseDown, 1, 1, number)
            .expect("bounded press");
    }
    assert!(
        events
            .record(Uuid::new_v4(), NSEventType::LeftMouseDown, 1, 1, 99)
            .is_err()
    );
    events
        .record(first, NSEventType::LeftMouseDown, 1, 1, 42)
        .expect("replace own press");
    assert_eq!(events.take(first, 1, true).expect("replacement event"), 42);
}

#[test]
fn promise_completion_resolves_the_originating_drag_once() {
    let mut registry = PromiseTransferRegistry::default();
    let first = (Uuid::new_v4(), Uuid::new_v4());
    let second = (Uuid::new_v4(), Uuid::new_v4());
    registry.register(first.0, first.1, owner());
    registry.register(second.0, second.1, owner());
    assert_eq!(registry.take_completed_drag(second.0), Some(second.1));
    assert_eq!(registry.take_completed_drag(first.0), Some(first.1));
    assert_eq!(registry.take_completed_drag(first.0), None);
}

#[test]
fn retiring_a_promise_attempt_preserves_new_connections_and_other_tabs() {
    let original = owner();
    let newer = FilePromiseOwner {
        attempt_id: Uuid::new_v4(),
        ..original
    };
    let other = owner();
    let mut registry = PromiseTransferRegistry::default();
    let transfers = [
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    ];
    for (transfer, origin) in transfers
        .into_iter()
        .zip([original, original, newer, other])
    {
        registry.register(transfer, transfer, origin);
    }
    registry.remove_attempt(original);
    assert_eq!(registry.take_completed_drag(transfers[0]), None);
    assert_eq!(registry.take_completed_drag(transfers[1]), None);
    assert_eq!(
        registry.take_completed_drag(transfers[2]),
        Some(transfers[2])
    );
    assert_eq!(
        registry.take_completed_drag(transfers[3]),
        Some(transfers[3])
    );
}

#[test]
fn stopped_monitor_guard_removes_registered_promises() {
    let origin = owner();
    let transfer = Uuid::new_v4();
    promised_transfers()
        .lock()
        .expect("registry")
        .register(transfer, Uuid::new_v4(), origin);
    drop(FilePromiseAttemptGuard::new(
        origin.tab_id,
        origin.attempt_id,
    ));
    assert_eq!(
        promised_transfers()
            .lock()
            .expect("registry")
            .take_completed_drag(transfer),
        None
    );
}

#[test]
fn promise_request_completes_once_on_success_failure_or_owner_drop() {
    for succeeded in [true, false] {
        let (results, callback) = completion();
        let mut request = PromiseRequest::new(owner());
        let mut callback = Some(callback);
        request
            .start(Uuid::new_v4(), &mut callback)
            .expect("accepted promise");
        assert!(callback.is_none());
        request.finish(succeeded);
        request.finish(!succeeded);
        drop(request);
        assert_eq!(*results.borrow(), [succeeded]);
    }
    let (results, callback) = completion();
    let mut request = PromiseRequest::new(owner());
    request
        .start(Uuid::new_v4(), &mut Some(callback))
        .expect("accepted promise");
    drop(request);
    assert_eq!(
        *results.borrow(),
        [false],
        "cancelled owner reports failure"
    );
}

#[test]
fn duplicate_promise_request_rejects_only_the_new_callback() {
    let (original_results, original) = completion();
    let (duplicate_results, duplicate) = completion();
    let mut request = PromiseRequest::new(owner());
    request
        .start(Uuid::new_v4(), &mut Some(original))
        .expect("first promise");
    let mut duplicate = Some(duplicate);
    assert!(request.start(Uuid::new_v4(), &mut duplicate).is_err());
    call_promise_completion(
        &duplicate.expect("rejected caller keeps its callback"),
        false,
    );
    assert!(
        original_results.borrow().is_empty(),
        "original request remains active"
    );
    request.finish(true);
    assert_eq!(*original_results.borrow(), [true]);
    assert_eq!(*duplicate_results.borrow(), [false]);
}

#[test]
fn invalid_or_late_finder_destinations_always_complete_with_failure() {
    for target in [
        Err(anyhow::anyhow!("destination has no path")),
        Ok(PathBuf::from("/unused")),
    ] {
        let (results, callback) = completion();
        fulfill_file_promise(Uuid::new_v4(), target, callback);
        assert_eq!(*results.borrow(), [false]);
    }
}

#[test]
fn finder_destination_requires_an_absolute_file_url_and_preserves_url_encoding() {
    let file_url = NSURL::URLWithString(&NSString::from_str("file:///tmp/report%20one.txt"))
        .expect("file URL");
    assert_eq!(
        file_promise_target(&file_url).expect("local destination"),
        PathBuf::from("/tmp/report one.txt")
    );
    for url in [
        "https://example.invalid/tmp/file.txt",
        "relative.txt",
        "data:text/plain,example",
    ] {
        let url = NSURL::URLWithString(&NSString::from_str(url)).expect("URL fixture");
        let (results, callback) = completion();
        let target = file_promise_target(&url);
        assert!(target.is_err());
        fulfill_file_promise(Uuid::new_v4(), target, callback);
        assert_eq!(*results.borrow(), [false]);
    }
}

#[tokio::test]
async fn ssh_monitor_disconnect_failure_and_eof_release_the_originating_promises() {
    use crate::app::{
        AppState, ConfigStore, ConnectionTarget, SessionProfile, SessionStore, SshSessionEvent,
    };

    for terminal_event in [
        Some(SshSessionEvent::Disconnected),
        Some(SshSessionEvent::Failed("fixture failure".into())),
        None,
    ] {
        let profile = SessionProfile::new("fixture", "fixture.invalid", "fixture");
        let mut app = AppState::new(
            ConfigStore::new(
                std::env::temp_dir().join(format!("axssh-promise-{}.json", Uuid::new_v4())),
            ),
            SessionStore::default(),
        );
        let tab_id = app.open_sftp_tab(&profile);
        let attempt_id = Uuid::new_v4();
        let terminal = app.terminal_mut(tab_id).expect("SFTP tab");
        terminal.set_ssh_attempt(Some(attempt_id));
        terminal.worker_running = true;
        terminal.cancel_reconnect();
        let origin = FilePromiseOwner { tab_id, attempt_id };
        let transfer = Uuid::new_v4();
        promised_transfers()
            .lock()
            .expect("registry")
            .register(transfer, Uuid::new_v4(), origin);
        let (sender, events) = tokio::sync::mpsc::channel(2);
        let task = crate::app::connection_monitor::spawn_session_monitor(
            &tokio::runtime::Handle::current(),
            std::sync::Arc::new(std::sync::Mutex::new(app)),
            slint::Weak::default(),
            tab_id,
            profile,
            attempt_id,
            events,
            None,
            false,
            ConnectionTarget::Sftp,
        )
        .expect("monitor");
        if let Some(event) = terminal_event {
            sender.send(event).await.expect("terminal event");
        }
        drop(sender);
        tokio::time::timeout(std::time::Duration::from_secs(3), task)
            .await
            .expect("monitor deadline")
            .expect("monitor joins");
        assert_eq!(
            promised_transfers()
                .lock()
                .expect("registry")
                .take_completed_drag(transfer),
            None
        );
    }
}
