use super::*;

#[tokio::test(start_paused = true)]
async fn consent_timeout_only_clears_the_matching_token_and_connection_generation() {
    let mut app = AppState::new(
        ConfigStore::new(
            std::env::temp_dir().join(format!("axssh-consent-{}.json", Uuid::new_v4())),
        ),
        SessionStore::default(),
    );
    let matching = app.open_local_shell_tab();
    let replacement = app.open_local_shell_tab();
    let reconnected = app.open_local_shell_tab();
    for id in [matching, replacement, reconnected] {
        app.terminal_mut(id)
            .unwrap()
            .set_test_pending_clipboard_read(Arc::new(|_| String::new()));
    }
    let state = Arc::new(Mutex::new(app));
    for id in [matching, replacement, reconnected, Uuid::new_v4()] {
        schedule_osc52_clipboard_read_timeout(
            state.clone(),
            slint::Weak::default(),
            id,
            if id == replacement { 2 } else { 1 },
            0,
        );
    }
    // These do not match the original consent lease.
    {
        let mut app = state.lock().unwrap();
        app.terminal_mut(reconnected).unwrap().reconnect_generation = 1;
    }
    tokio::task::yield_now().await;
    tokio::time::advance(OSC52_CLIPBOARD_READ_TIMEOUT - Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    assert!(
        state
            .lock()
            .unwrap()
            .terminal(matching)
            .unwrap()
            .notice_snapshot()
            .primary_action
            == "allow-osc52-clipboard-read"
    );
    tokio::time::advance(Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    let app = state.lock().unwrap();
    assert!(
        app.terminal(matching)
            .unwrap()
            .notice_snapshot()
            .primary_action
            != "allow-osc52-clipboard-read"
    );
    assert!(
        app.terminal(replacement)
            .unwrap()
            .notice_snapshot()
            .primary_action
            == "allow-osc52-clipboard-read"
    );
    assert!(
        app.terminal(reconnected)
            .unwrap()
            .notice_snapshot()
            .primary_action
            == "allow-osc52-clipboard-read"
    );
}

#[test]
fn missing_runtime_or_ui_does_not_read_the_system_clipboard() {
    let mut app = AppState::new(
        ConfigStore::new(
            std::env::temp_dir().join(format!("axssh-consent-no-runtime-{}.json", Uuid::new_v4())),
        ),
        SessionStore::default(),
    );
    let tab = app.open_local_shell_tab();
    let ui = slint::Weak::default();
    let router = WindowRouter::new(ui.clone());
    assert!(router.activate_tab(MAIN_WINDOW_ID, tab, &mut app));
    app.terminal_mut(tab)
        .unwrap()
        .set_test_pending_clipboard_read(Arc::new(|_| String::new()));
    let state = Arc::new(Mutex::new(app));
    schedule_osc52_clipboard_read_timeout(state.clone(), ui.clone(), tab, 1, 0);
    assert!(!handle_osc52_clipboard_read(
        &router,
        MAIN_WINDOW_ID,
        tab,
        &state,
        &ui
    ));
    assert!(!handle_osc52_clipboard_deny(
        &router,
        Uuid::new_v4(),
        tab,
        &state,
        &ui
    ));
    assert!(
        state
            .lock()
            .unwrap()
            .terminal(tab)
            .unwrap()
            .notice_snapshot()
            .primary_action
            == "allow-osc52-clipboard-read"
    );
    assert!(handle_osc52_clipboard_deny(
        &router,
        MAIN_WINDOW_ID,
        tab,
        &state,
        &ui
    ));
    assert!(!handle_osc52_clipboard_deny(
        &router,
        MAIN_WINDOW_ID,
        tab,
        &state,
        &ui
    ));
}
