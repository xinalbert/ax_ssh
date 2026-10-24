//! Terminal output application boundary.

use super::*;

pub(in crate::app) fn process_terminal_output(
    terminal: &mut TerminalTabState,
    data: &[u8],
) -> Result<TerminalOutputEffects> {
    let ax_ssh::controller::TerminalOutput {
        responses,
        title_update,
        bell,
        clipboard_store,
        clipboard_load,
        clipboard_permission_generation,
        presentation_hold,
    } = ax_ssh::controller::TerminalOutput::process(
        terminal
            .terminal
            .as_mut()
            .context("terminal tab has no terminal model")?,
        data,
    );
    let clipboard_generation = terminal.reconnect_generation();
    if !data.is_empty() {
        terminal.defer_output_presentation();
    }
    let clipboard_read_requested = clipboard_load
        .and_then(|formatter| terminal.offer_clipboard_read(formatter))
        .is_some();
    if responses.is_empty() {
        return Ok(TerminalOutputEffects {
            presentation_hold,
            title_update,
            bell,
            clipboard_store,
            clipboard_generation,
            clipboard_permission_generation,
            clipboard_read_requested,
        });
    }
    let worker = terminal
        .worker
        .as_ref()
        .context("terminal protocol response has no transport worker")?;
    for response in responses {
        worker
            .request_send_kind(
                response,
                crate::app::state::terminal::TerminalInputKind::Protocol,
            )
            .context("cannot queue terminal protocol response")?;
    }
    Ok(TerminalOutputEffects {
        presentation_hold,
        title_update,
        bell,
        clipboard_store,
        clipboard_generation,
        clipboard_permission_generation,
        clipboard_read_requested,
    })
}

#[derive(Default)]
pub(in crate::app) struct TerminalOutputEffects {
    pub(in crate::app) presentation_hold: Option<Duration>,
    pub(in crate::app) title_update: Option<Option<String>>,
    pub(in crate::app) bell: bool,
    pub(in crate::app) clipboard_store: Option<String>,
    pub(in crate::app) clipboard_generation: u64,
    pub(in crate::app) clipboard_permission_generation: u64,
    pub(in crate::app) clipboard_read_requested: bool,
}

pub(in crate::app) fn apply_terminal_output_effects(
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    tab_id: Uuid,
    effects: TerminalOutputEffects,
) {
    if effects.title_update.is_none()
        && !effects.bell
        && effects.clipboard_store.is_none()
        && !effects.clipboard_read_requested
    {
        return;
    }
    if let Some(text) = effects.clipboard_store {
        let state = state.clone();
        let generation = effects.clipboard_generation;
        let permission_generation = effects.clipboard_permission_generation;
        dispatch_ui(ui, move |ui| {
            let allowed = state.lock().ok().is_some_and(|app| {
                app.sessions.settings.terminal.osc52_clipboard
                    && app.terminal(tab_id).is_some_and(|t| {
                        t.reconnect_generation() == generation
                            && t.terminal.as_ref().is_some_and(|model| {
                                model.osc52_clipboard_enabled()
                                    && model.osc52_permission_generation() == permission_generation
                            })
                            && t.connected
                            && t.worker_running
                    })
            });
            if allowed {
                let _written = set_platform_clipboard_text(ui, &text);
            }
        });
    }
    let title_changed = match state.lock() {
        Ok(mut app) => effects
            .title_update
            .map(|title| app.apply_terminal_title(tab_id, title))
            .unwrap_or(false),
        Err(_) => false,
    };
    if effects.clipboard_read_requested {
        let request = state.lock().ok().and_then(|app| {
            app.terminal(tab_id)
                .and_then(TerminalTabState::clipboard_read_key)
        });
        if let Some((token, generation)) = request {
            refresh_workspace(ui, state);
            schedule_osc52_clipboard_read_timeout(
                state.clone(),
                ui.clone(),
                tab_id,
                token,
                generation,
            );
        }
    }
    if title_changed || effects.bell {
        refresh_workspace(ui, state);
    }
}
