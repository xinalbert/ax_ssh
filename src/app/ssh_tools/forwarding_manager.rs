use super::*;
use crate::app::workspace::save_forwarding_rule;
use tokio::time::timeout;

pub(super) fn wire(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    persistence: Arc<PersistenceCoordinator>,
) {
    let reply = ui.as_weak();
    let toggle_state = state.clone();
    let toggle_persistence = persistence.clone();
    let toggle_runtime = runtime.clone();
    ui.on_forwarding_manager_toggle(move |row, enabled| {
        let Some(window) = manager_window(&reply) else {
            return;
        };
        window.set_forwarding_manager_error("".into());
        let state = toggle_state.clone();
        let persistence = toggle_persistence.clone();
        let reply = reply.clone();
        toggle_runtime.spawn(async move {
            let result = async {
                let id = profile_id(row.profile_id.as_str())?;
                set_forwarding_preset_enabled(
                    &state,
                    &persistence,
                    id,
                    saved_preset(&row)?,
                    enabled,
                )
                .await
            }
            .await;
            if let Err(error) = result {
                let message = format!("{error:#}");
                let response = reply.clone();
                if let Err(error) = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = manager_window(&response) {
                        ui.set_forwarding_manager_error(message.into());
                    }
                }) {
                    debug!(%error, "cannot publish forwarding rule error");
                }
            }
            refresh_workspace(&reply, &state);
        });
    });

    let reply = ui.as_weak();
    ui.on_forwarding_manager_submit(move |request, revision| {
        if manager_window(&reply).is_none() {
            return;
        }
        let reply = reply.clone();
        let state = state.clone();
        let persistence = persistence.clone();
        let check = request.action == "check";
        runtime.spawn(async move {
            let result = apply(&state, &persistence, request).await;
            let (kind, details) = match result {
                Ok((kind, details)) => (kind, details),
                Err(error) => ("error", format!("{error:#}")),
            };
            if !check && kind != "error" {
                refresh_workspace(&reply, &state);
            }
            if let Err(error) = slint::invoke_from_event_loop(move || {
                if let Some(ui) = reply.upgrade() {
                    ui.invoke_finish_forwarding_manager(revision, kind.into(), details.into());
                }
            }) {
                debug!(%error, "cannot publish forwarding manager result");
            }
        });
    });
}

fn manager_window(ui: &slint::Weak<AppWindow>) -> Option<AppWindow> {
    ui.upgrade().filter(|window| {
        !window.get_detached_window() && window.get_active_tab_kind() == "forwarding-manager"
    })
}

fn profile_id(value: &str) -> Result<Uuid> {
    Uuid::parse_str(value).context("select a saved SSH server")
}

fn saved_preset(row: &ForwardingManagerRow) -> Result<ForwardingPreset> {
    Ok(ForwardingPreset {
        mode: ForwardingPresetMode::from_setting(row.mode.as_str())
            .context("unknown forwarding mode")?,
        listen: row.listen.to_string(),
        target: row.target.to_string(),
        enabled: row.enabled,
    })
}

async fn apply(
    state: &Arc<Mutex<AppState>>,
    persistence: &PersistenceCoordinator,
    request: ForwardingManagerRequest,
) -> Result<(&'static str, String)> {
    if request.action == "check" {
        return check_forwarding(state, &request.original).await;
    }
    let (id, original) = match request.action.as_str() {
        "new" => (profile_id(request.profile_id.as_str())?, None),
        "edit" | "delete" => (
            profile_id(request.original.profile_id.as_str())?,
            Some(saved_preset(&request.original)?),
        ),
        _ => anyhow::bail!("unknown forwarding rule action"),
    };
    let replacement = if request.action == "delete" {
        None
    } else {
        let mode = ForwardingPresetMode::from_setting(request.mode.as_str())
            .context("unknown forwarding mode")?;
        let mut preset =
            ForwardingPreset::new(mode, request.listen.as_str(), request.target.as_str())?;
        preset.enabled = request.enabled;
        Some(preset)
    };
    save_forwarding_rule(state, persistence, id, original, replacement).await?;
    Ok(("saved", String::new()))
}

async fn check_forwarding(
    state: &Arc<Mutex<AppState>>,
    row: &ForwardingManagerRow,
) -> Result<(&'static str, String)> {
    let id = profile_id(row.profile_id.as_str())?;
    let preset = saved_preset(row)?;
    let rule = parse_forwarding_preset(&preset)?;
    let replies = {
        let app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("cannot read session state"))?;
        anyhow::ensure!(
            app.sessions
                .sessions
                .iter()
                .find(|profile| profile.id == id)
                .and_then(|profile| profile.ssh())
                .is_some_and(|ssh| ssh.forwarding_history.contains(&preset)),
            "forwarding rule changed; reopen the rule and try again"
        );
        app.connected_forwarding_workers(id)
            .map(|worker| worker.request_tool(SshTool::ForwardRuleStatus(rule.clone())))
            .collect::<Vec<_>>()
    };
    if replies.is_empty() {
        return Ok(("offline", String::new()));
    }
    let mut listeners = Vec::new();
    let mut starting = false;
    let mut unavailable = false;
    let finished = timeout(Duration::from_secs(3), async {
        for reply in replies {
            let Ok(reply) = reply else {
                unavailable = true;
                continue;
            };
            match reply.await {
                Ok(Ok(SshToolResult::Forward(active))) => listeners.push(describe_forward(&active)),
                Ok(Ok(SshToolResult::Forwarding(active))) => {
                    listeners.extend(active.iter().map(describe_forward));
                }
                Ok(Ok(SshToolResult::ForwardStarting)) => starting = true,
                Ok(Ok(SshToolResult::ForwardStopped)) => {}
                _ => unavailable = true,
            }
        }
    })
    .await;
    if !listeners.is_empty() {
        listeners.sort_unstable();
        listeners.dedup();
        Ok(("listening", listeners.join("\n")))
    } else if starting {
        Ok(("starting", String::new()))
    } else if finished.is_err() || unavailable {
        Ok(("unavailable", String::new()))
    } else {
        Ok(("inactive", String::new()))
    }
}
