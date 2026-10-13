//! Terminal clipboard application boundary.

use super::*;

const OSC52_CLIPBOARD_READ_TIMEOUT: Duration = Duration::from_secs(20);

pub(super) fn schedule_osc52_clipboard_read_timeout(
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    tab_id: Uuid,
    token: u64,
    generation: u64,
) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    runtime.spawn(async move {
        tokio::time::sleep(OSC52_CLIPBOARD_READ_TIMEOUT).await;
        let cleared = state.lock().ok().is_some_and(|mut app| {
            app.terminal_mut(tab_id)
                .is_some_and(|terminal| terminal.clear_clipboard_read(token, generation))
        });
        if cleared {
            refresh_workspace(&ui, &state);
        }
    });
}

pub(super) fn handle_osc52_clipboard_read(
    router: &WindowRouter,
    window_id: Uuid,
    tab_id: Uuid,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
) -> bool {
    let Some(ui) = ui.upgrade() else {
        return false;
    };
    let Some((token, generation)) = state.lock().ok().and_then(|app| {
        if !router.owns_terminal_pane(window_id, tab_id, &app) {
            return None;
        }
        app.terminal(tab_id)
            .and_then(TerminalTabState::clipboard_read_key)
    }) else {
        return false;
    };

    let text = match platform_clipboard_text(&ui) {
        Ok(text) if text.len() <= ax_ssh::terminal::MAX_OSC52_CLIPBOARD_BYTES => text,
        result => {
            let message = match result {
                Ok(_) => "Clipboard exceeds the OSC 52 limit (64 KiB)".to_owned(),
                Err(error) => format!("Cannot read clipboard: {error}"),
            };
            handle_osc52_clipboard_deny(router, window_id, tab_id, state, &ui.as_weak());
            ui.set_status(message.into());
            return false;
        }
    };
    let sent = {
        let Ok(mut app) = state.lock() else {
            return false;
        };
        if !router.owns_terminal_pane(window_id, tab_id, &app) {
            return false;
        }
        let Some(terminal) = app.terminal_mut(tab_id) else {
            return false;
        };
        if terminal.clipboard_read_key() != Some((token, generation)) {
            return false;
        }
        let Some(formatter) = terminal.take_pending_clipboard_read() else {
            return false;
        };
        let Some(worker) = terminal.worker.as_ref() else {
            return false;
        };
        match worker.request_send_clipboard_response(formatter(&text).into_bytes()) {
            Ok(()) => true,
            Err(error) => {
                ui.set_status(format!("Cannot send clipboard: {error}").into());
                false
            }
        }
    };
    refresh_workspace(&ui.as_weak(), state);
    sent
}

pub(super) fn handle_osc52_clipboard_deny(
    router: &WindowRouter,
    window_id: Uuid,
    tab_id: Uuid,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
) -> bool {
    let cleared = state.lock().ok().is_some_and(|mut app| {
        if !router.owns_terminal_pane(window_id, tab_id, &app) {
            return false;
        }
        app.terminal_mut(tab_id)
            .is_some_and(TerminalTabState::clear_pending_clipboard_read)
    });
    if cleared {
        refresh_workspace(ui, state);
    }
    cleared
}
