//! Window-bound SSH tools. Transport objects stay in the SSH worker.

mod forwarding_manager;

use super::state::TerminalBackend;
use super::workspace::{record_forwarding_preset, set_forwarding_preset_enabled};
use super::*;
use ax_ssh::config::{ForwardingPreset, ForwardingPresetMode};
use ax_ssh::ssh::{ForwardListener, ForwardRule, ForwardTarget, SshTool, SshToolResult};

pub(super) fn wire(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    router: WindowRouter,
    window: Uuid,
    persistence: Arc<PersistenceCoordinator>,
) {
    forwarding_manager::wire(ui, state.clone(), runtime.clone(), persistence.clone());

    let open_ui = ui.as_weak();
    let open_state = state.clone();
    let open_router = router.clone();
    ui.on_ssh_tool_target(move |pane| {
        let target = (|| {
            let pane = Uuid::parse_str(pane.as_str()).ok()?;
            let app = open_state.lock().ok()?;
            if !open_router.owns_terminal_pane(window, pane, &app) {
                return None;
            }
            let terminal = app.terminal(pane)?;
            let (_, attempt) = terminal.ssh_route()?;
            let attempt = attempt?;
            (terminal.connected && matches!(terminal.backend, TerminalBackend::Ssh { .. }))
                .then(|| format!("{pane}:{attempt}"))
        })();
        if let Some(target) = target {
            target.into()
        } else {
            set_status(&open_ui, "Select a connected SSH terminal");
            "".into()
        }
    });

    let history_state = state.clone();
    let history_router = router.clone();
    ui.on_ssh_tool_history(move |target| {
        let options =
            forwarding_history_for_target(target.as_str(), &history_state, &history_router, window)
                .into_iter()
                .map(|preset| preset.label().into())
                .collect::<Vec<SharedString>>();
        ModelRc::new(VecModel::from(options))
    });

    let entry_state = state.clone();
    let entry_router = router.clone();
    ui.on_ssh_tool_history_entry(move |target, index| {
        forwarding_history_for_target(target.as_str(), &entry_state, &entry_router, window)
            .get(index.max(0) as usize)
            .map(history_row)
            .unwrap_or_else(empty_history_row)
    });

    let action_ui = ui.as_weak();
    let action_persistence = persistence;
    ui.on_ssh_tool_action(move |target, action, first, second, third, revision| {
        let action_name = action.as_str().to_owned();
        let history_listen = first.as_str().trim().to_owned();
        let history_target = second.as_str().trim().to_owned();
        let history_mode = third.as_str().trim().to_owned();
        let request = (|| {
            let tool = parse_tool(
                action.as_str(),
                first.as_str(),
                second.as_str(),
                third.as_str(),
            )?;
            let (pane, attempt) = target.split_once(':').context("invalid SSH connection")?;
            let pane = Uuid::parse_str(pane)?;
            let attempt = Uuid::parse_str(attempt)?;
            let app = state
                .lock()
                .map_err(|_| anyhow::anyhow!("cannot read session state"))?;
            anyhow::ensure!(
                router.owns_terminal_pane(window, pane, &app),
                "SSH pane is no longer in this window"
            );
            let terminal = app
                .terminal(pane)
                .filter(|terminal| {
                    terminal.connected
                        && terminal
                            .ssh_route()
                            .is_some_and(|(_, current)| current == Some(attempt))
                })
                .context("SSH connection changed. Reopen the tool to use the new connection.")?;
            match terminal.worker.as_ref() {
                Some(TerminalWorker::Ssh(worker)) => worker.request_tool(tool),
                _ => anyhow::bail!("SSH tools require a connected SSH terminal"),
            }
        })();
        let reply_ui = action_ui.clone();
        let action_state = state.clone();
        let persistence = action_persistence.clone();
        runtime.spawn(async move {
            let result = match request {
                Ok(receiver) => receiver
                    .await
                    .context("SSH worker disconnected")
                    .and_then(|result| result),
                Err(error) => Err(error),
            };
            let history_result = if matches!(
                (&action_name[..], &result),
                ("forward" | "enable-forward", Ok(SshToolResult::Forward(_)))
                    | (
                        "disable-forward",
                        Ok(SshToolResult::ForwardStopped
                            | SshToolResult::Forward(_)
                            | SshToolResult::Forwarding(_))
                    )
            ) {
                profile_id_for_target(&target, &action_state).map(|profile_id| async move {
                    if action_name == "forward" {
                        record_forwarding_preset(
                            &action_state,
                            &persistence,
                            profile_id,
                            &history_mode,
                            &history_listen,
                            &history_target,
                        )
                        .await
                    } else {
                        let preset = forwarding_history_for_profile(&action_state, profile_id)
                            .and_then(|history| {
                                history.into_iter().find(|preset| {
                                    preset.mode.as_setting() == history_mode
                                        && preset.listen == history_listen
                                        && preset.target == history_target
                                })
                            })
                            .context("forwarding history entry not found")?;
                        set_forwarding_preset_enabled(
                            &action_state,
                            &persistence,
                            profile_id,
                            preset,
                            action_name == "enable-forward",
                        )
                        .await
                    }
                })
            } else {
                None
            };
            let history_result = match history_result {
                Some(result) => Some(result.await),
                None => None,
            };
            let (mut message, alternate) = summarize(result);
            if let Some(Err(error)) = history_result {
                message.push_str(&format!("\n\nCould not save forwarding history: {error:#}"));
            }
            if let Err(error) = slint::invoke_from_event_loop(move || {
                if let Some(ui) = reply_ui.upgrade() {
                    ui.invoke_finish_ssh_tool(revision, message.into(), alternate);
                }
            }) {
                debug!(%error, "cannot publish SSH tool result");
            }
        });
    });
}

fn forwarding_history_for_target(
    target: &str,
    state: &Arc<Mutex<AppState>>,
    router: &WindowRouter,
    window: Uuid,
) -> Vec<ForwardingPreset> {
    let Some((pane, _attempt)) = target.split_once(':') else {
        return Vec::new();
    };
    let Ok(pane) = Uuid::parse_str(pane) else {
        return Vec::new();
    };
    let Ok(app) = state.lock() else {
        return Vec::new();
    };
    if !router.owns_terminal_pane(window, pane, &app) {
        return Vec::new();
    }
    let Some(terminal) = app.terminal(pane) else {
        return Vec::new();
    };
    let Some((profile_id, _)) = terminal.ssh_route() else {
        return Vec::new();
    };
    app.sessions
        .sessions
        .iter()
        .find(|profile| profile.id == profile_id)
        .and_then(|profile| profile.ssh())
        .map(|ssh| ssh.forwarding_history.clone())
        .unwrap_or_default()
}

fn profile_id_for_target(target: &str, state: &Arc<Mutex<AppState>>) -> Option<Uuid> {
    let (pane, _) = target.split_once(':')?;
    let pane = Uuid::parse_str(pane).ok()?;
    let app = state.lock().ok()?;
    app.terminal(pane)
        .and_then(|terminal| terminal.ssh_route().map(|route| route.0))
}

fn forwarding_history_for_profile(
    state: &Arc<Mutex<AppState>>,
    profile_id: Uuid,
) -> Option<Vec<ForwardingPreset>> {
    let app = state.lock().ok()?;
    app.sessions
        .sessions
        .iter()
        .find(|profile| profile.id == profile_id)
        .and_then(|profile| profile.ssh())
        .map(|ssh| ssh.forwarding_history.clone())
}

fn history_row(preset: &ForwardingPreset) -> SshForwardingHistoryRow {
    SshForwardingHistoryRow {
        mode: match preset.mode {
            ForwardingPresetMode::Local => 0,
            ForwardingPresetMode::Remote => 1,
            ForwardingPresetMode::Dynamic => 2,
        },
        listen: preset.listen.clone().into(),
        target: preset.target.clone().into(),
        enabled: preset.enabled,
    }
}

fn empty_history_row() -> SshForwardingHistoryRow {
    SshForwardingHistoryRow {
        mode: 0,
        listen: "".into(),
        target: "".into(),
        enabled: false,
    }
}

fn parse_tool(action: &str, first: &str, second: &str, third: &str) -> Result<SshTool> {
    Ok(match action {
        "exec" => SshTool::Execute {
            command: first.to_owned(),
        },
        "cancel" => SshTool::CancelCommand,
        "forward" => SshTool::StartForward(parse_forward(first.trim(), second.trim(), third)?),
        "enable-forward" => {
            SshTool::StartForward(parse_forward(first.trim(), second.trim(), third)?)
        }
        "disable-forward" => {
            SshTool::StopForwardRule(parse_forward(first.trim(), second.trim(), third)?)
        }
        "stop" => SshTool::StopForward,
        "status" => SshTool::ForwardStatus,
        _ => anyhow::bail!("unknown SSH tool"),
    })
}

pub(super) fn parse_forwarding_preset(preset: &ForwardingPreset) -> Result<ForwardRule> {
    preset.validate()?;
    let rule = parse_forward(&preset.listen, &preset.target, preset.mode.as_setting())?;
    rule.validate()?;
    Ok(rule)
}

fn parse_forward(listen: &str, target: &str, mode: &str) -> Result<ForwardRule> {
    if mode == "dynamic" {
        return Ok(ForwardRule::Dynamic {
            port: listen.parse().context("invalid SOCKS5 port")?,
        });
    }
    let listen = if listen.starts_with('/') {
        ForwardListener::UnixSocket(listen.to_owned())
    } else {
        ForwardListener::Tcp(
            listen
                .parse()
                .context("enter a listening port or an absolute socket path")?,
        )
    };
    let target = if target.starts_with('/') {
        ForwardTarget::UnixSocket(target.to_owned())
    } else {
        let (host, port) = target
            .rsplit_once(':')
            .context("enter a destination as host:port or an absolute socket path")?;
        let host = if let Some(ipv6) = host
            .strip_prefix('[')
            .and_then(|part| part.strip_suffix(']'))
        {
            ipv6.parse::<std::net::Ipv6Addr>()
                .context("invalid bracketed IPv6 destination")?
                .to_string()
        } else {
            anyhow::ensure!(
                !host.contains([':', '[', ']']),
                "enclose the IPv6 destination in brackets"
            );
            host.to_owned()
        };
        ForwardTarget::Tcp {
            host,
            port: port.parse().context("invalid destination port")?,
        }
    };
    match mode {
        "local" => Ok(ForwardRule::Local { listen, target }),
        "remote" => Ok(ForwardRule::Remote { listen, target }),
        _ => anyhow::bail!("unknown forwarding mode"),
    }
}

fn describe_forward(rule: &ForwardRule) -> String {
    let (label, listen, target) = match rule {
        ForwardRule::Local { listen, target } => ("Local", listen, target),
        ForwardRule::Remote { listen, target } => ("Remote", listen, target),
        ForwardRule::Dynamic { port } => return format!("SOCKS5 · 127.0.0.1:{port}"),
    };
    let listen = match listen {
        ForwardListener::Tcp(port) => format!("127.0.0.1:{port}"),
        ForwardListener::UnixSocket(path) => path.clone(),
    };
    let target = match target {
        ForwardTarget::Tcp { host, port } if host.contains(':') => format!("[{host}]:{port}"),
        ForwardTarget::Tcp { host, port } => format!("{host}:{port}"),
        ForwardTarget::UnixSocket(path) => path.clone(),
    };
    format!("{label} · {listen} → {target}")
}

fn summarize(result: Result<SshToolResult>) -> (String, bool) {
    match result {
        Ok(SshToolResult::Forward(rule)) => (describe_forward(&rule), true),
        Ok(SshToolResult::Forwarding(rules)) => (
            rules
                .iter()
                .map(describe_forward)
                .collect::<Vec<_>>()
                .join("\n"),
            true,
        ),
        Ok(SshToolResult::ForwardStarting) => ("Starting forwarding…".to_owned(), true),
        Ok(SshToolResult::ForwardStopped) => ("No active forwarding".to_owned(), false),
        Ok(SshToolResult::CommandCancelled) => ("Command cancelled locally".to_owned(), false),
        Ok(SshToolResult::Command {
            stdout,
            stderr,
            exit_status,
            exit_signal,
            truncated,
        }) => {
            let status = match (exit_status, exit_signal) {
                (_, true) => "Remote command exited after a signal".to_owned(),
                (Some(code), false) => format!("Exit status {code}"),
                (None, false) => "Remote exit status unavailable".to_owned(),
            };
            let stdout = display_output(&stdout);
            let stderr = display_output(&stderr);
            let truncated = if truncated {
                "\n\nOutput truncated at 64 KiB"
            } else {
                ""
            };
            (
                format!("{status}\n\nstdout\n{stdout}\n\nstderr\n{stderr}{truncated}"),
                false,
            )
        }
        Err(error) => (format!("{error:#}"), false),
    }
}

fn display_output(output: &[u8]) -> String {
    String::from_utf8_lossy(output)
        .chars()
        .filter(|&c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}
