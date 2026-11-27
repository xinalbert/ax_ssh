//! User-requested commands and port forwarding owned by a terminal worker.

use anyhow::{Context, Result, anyhow};
use russh::ChannelMsg;
use tokio::sync::{oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};

use super::SshConnection;
use crate::ssh::forwarding::{self, ForwardListener, ForwardRule, ForwardState, RemoteDispatcher};

const MAX_COMMAND_BYTES: usize = 8 * 1024;
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

pub enum SshTool {
    Execute { command: String },
    CancelCommand,
    StartForward(ForwardRule),
    StopForward,
    StopForwardRule(ForwardRule),
    ForwardStatus,
    ForwardRuleStatus(ForwardRule),
}

pub enum SshToolResult {
    Command {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        exit_status: Option<u32>,
        exit_signal: bool,
        truncated: bool,
    },
    CommandCancelled,
    Forward(ForwardRule),
    Forwarding(Vec<ForwardRule>),
    ForwardStarting,
    ForwardStopped,
}

type Reply = oneshot::Sender<Result<SshToolResult>>;

pub(super) struct ToolTasks {
    remote_dispatcher: RemoteDispatcher,
    command: Option<RunningCommand>,
    forwards: Vec<RunningForward>,
}

struct RunningCommand {
    cancel: watch::Sender<bool>,
    task: JoinHandle<()>,
}

struct RunningForward {
    requested: ForwardRule,
    state: watch::Receiver<ForwardState>,
    stop: oneshot::Sender<()>,
    task: JoinHandle<Result<()>>,
}

impl RunningForward {
    fn is_running(&self) -> bool {
        !self.task.is_finished() && !matches!(*self.state.borrow(), ForwardState::Stopped)
    }

    fn status(&self) -> SshToolResult {
        if self.task.is_finished() {
            return SshToolResult::ForwardStopped;
        }
        match &*self.state.borrow() {
            ForwardState::Starting => SshToolResult::ForwardStarting,
            ForwardState::Active(rule) => SshToolResult::Forward(rule.clone()),
            ForwardState::Stopped => SshToolResult::ForwardStopped,
        }
    }
}

impl ToolTasks {
    pub(super) fn new(remote_dispatcher: RemoteDispatcher) -> Self {
        Self {
            remote_dispatcher,
            command: None,
            forwards: Vec::new(),
        }
    }

    pub(super) async fn submit(&mut self, connection: &SshConnection, tool: SshTool, reply: Reply) {
        self.forwards.retain(|forward| {
            !forward.task.is_finished() && !matches!(*forward.state.borrow(), ForwardState::Stopped)
        });
        let result =
            match tool {
                SshTool::Execute { command } => {
                    if self
                        .command
                        .as_ref()
                        .is_some_and(|job| !job.task.is_finished())
                    {
                        Err(anyhow!("a remote command is already running"))
                    } else if command.trim().is_empty()
                        || command.len() > MAX_COMMAND_BYTES
                        || command.contains('\0')
                    {
                        Err(anyhow!(
                            "enter a remote command of at most {MAX_COMMAND_BYTES} bytes"
                        ))
                    } else {
                        let (cancel, cancelled) = watch::channel(false);
                        let connection = connection.clone();
                        let task = tokio::spawn(async move {
                            respond(reply, execute(&connection, &command, cancelled).await);
                        });
                        self.command = Some(RunningCommand { cancel, task });
                        return;
                    }
                }
                SshTool::CancelCommand => {
                    self.cancel_command().await;
                    Ok(SshToolResult::CommandCancelled)
                }
                SshTool::StartForward(rule) => {
                    if let Err(error) = rule.validate() {
                        Err(error)
                    } else if self.forwards.iter().any(|forward| {
                        forward.is_running() && forward_rule_conflicts(&rule, forward)
                    }) {
                        Err(anyhow!("stop the existing forward before starting another"))
                    } else {
                        let (stop, stopped) = oneshot::channel();
                        let (state, status) = watch::channel(ForwardState::Starting);
                        let task = tokio::spawn(forwarding::run(
                            connection.clone(),
                            self.remote_dispatcher.clone(),
                            rule.clone(),
                            state,
                            stopped,
                            move |outcome| respond(reply, outcome.map(SshToolResult::Forward)),
                        ));
                        self.forwards.push(RunningForward {
                            requested: rule,
                            state: status,
                            stop,
                            task,
                        });
                        return;
                    }
                }
                SshTool::StopForward => self
                    .stop_forward()
                    .await
                    .map(|()| SshToolResult::ForwardStopped),
                SshTool::StopForwardRule(rule) => self
                    .stop_forward_rule(&rule)
                    .await
                    .map(|()| self.status(None)),
                SshTool::ForwardStatus => Ok(self.status(None)),
                SshTool::ForwardRuleStatus(rule) => Ok(self.status(Some(&rule))),
            };
        respond(reply, result);
    }

    async fn stop_forward(&mut self) -> Result<()> {
        while let Some(RunningForward { stop, task, .. }) = self.forwards.pop() {
            if stop.send(()).is_err() {
                tracing::debug!("SSH forwarding task already closed");
            }
            task.await
                .context("SSH forwarding task did not exit cleanly")??;
        }
        Ok(())
    }

    async fn stop_forward_rule(&mut self, requested: &ForwardRule) -> Result<()> {
        let Some(index) = self
            .forwards
            .iter()
            .position(|forward| forward_rule_matches(requested, forward))
        else {
            anyhow::bail!("forwarding rule is not active");
        };
        let RunningForward { stop, task, .. } = self.forwards.swap_remove(index);
        if stop.send(()).is_err() {
            tracing::debug!("SSH forwarding task already closed");
        }
        task.await
            .context("SSH forwarding task did not exit cleanly")??;
        Ok(())
    }

    fn status(&self, requested: Option<&ForwardRule>) -> SshToolResult {
        let mut matching = self
            .forwards
            .iter()
            .filter(|forward| requested.is_none_or(|rule| &forward.requested == rule));
        let active = matching
            .clone()
            .filter_map(|forward| match forward.status() {
                SshToolResult::Forward(rule) => Some(rule),
                SshToolResult::ForwardStarting
                | SshToolResult::ForwardStopped
                | SshToolResult::Forwarding(_)
                | SshToolResult::Command { .. }
                | SshToolResult::CommandCancelled => None,
            })
            .collect::<Vec<_>>();
        match active.len() {
            0 if matching
                .any(|forward| matches!(forward.status(), SshToolResult::ForwardStarting)) =>
            {
                SshToolResult::ForwardStarting
            }
            0 => SshToolResult::ForwardStopped,
            1 => active
                .into_iter()
                .next()
                .map_or(SshToolResult::ForwardStopped, SshToolResult::Forward),
            _ => SshToolResult::Forwarding(active),
        }
    }

    pub(super) async fn shutdown(&mut self) {
        if let Err(error) = self.stop_forward().await {
            tracing::warn!(%error, "cannot stop SSH forwarding");
        }
        self.cancel_command().await;
    }

    async fn cancel_command(&mut self) {
        if let Some(job) = self.command.take() {
            job.cancel.send_replace(true);
            if let Err(error) = job.task.await {
                tracing::warn!(%error, "SSH command task did not exit cleanly");
            }
        }
    }
}

impl Drop for ToolTasks {
    fn drop(&mut self) {
        for forward in &self.forwards {
            forward.task.abort();
        }
        if let Some(job) = &self.command {
            job.task.abort();
        }
    }
}

fn forward_rule_matches(requested: &ForwardRule, active: &RunningForward) -> bool {
    let SshToolResult::Forward(active) = active.status() else {
        return false;
    };
    match (requested, &active) {
        (ForwardRule::Dynamic { port: requested }, ForwardRule::Dynamic { port: active }) => {
            *requested == 0 || requested == active
        }
        (
            ForwardRule::Local {
                listen: ForwardListener::Tcp(requested),
                target: requested_target,
            },
            ForwardRule::Local {
                listen: ForwardListener::Tcp(active),
                target: active_target,
            },
        )
        | (
            ForwardRule::Remote {
                listen: ForwardListener::Tcp(requested),
                target: requested_target,
            },
            ForwardRule::Remote {
                listen: ForwardListener::Tcp(active),
                target: active_target,
            },
        ) => (*requested == 0 || requested == active) && requested_target == active_target,
        (
            ForwardRule::Local { listen, target },
            ForwardRule::Local {
                listen: active_listen,
                target: active_target,
            },
        )
        | (
            ForwardRule::Remote { listen, target },
            ForwardRule::Remote {
                listen: active_listen,
                target: active_target,
            },
        ) => listen == active_listen && target == active_target,
        _ => false,
    }
}

fn forward_rule_conflicts(requested: &ForwardRule, active: &RunningForward) -> bool {
    let SshToolResult::Forward(active) = active.status() else {
        return false;
    };
    match (requested, &active) {
        (ForwardRule::Dynamic { port: requested }, ForwardRule::Dynamic { port: active }) => {
            *requested != 0 && requested == active
        }
        (
            ForwardRule::Local {
                listen: ForwardListener::Tcp(requested),
                target: requested_target,
            },
            ForwardRule::Local {
                listen: ForwardListener::Tcp(active),
                target: active_target,
            },
        )
        | (
            ForwardRule::Remote {
                listen: ForwardListener::Tcp(requested),
                target: requested_target,
            },
            ForwardRule::Remote {
                listen: ForwardListener::Tcp(active),
                target: active_target,
            },
        ) => *requested != 0 && requested == active && requested_target == active_target,
        (
            ForwardRule::Local { listen, target },
            ForwardRule::Local {
                listen: active_listen,
                target: active_target,
            },
        )
        | (
            ForwardRule::Remote { listen, target },
            ForwardRule::Remote {
                listen: active_listen,
                target: active_target,
            },
        ) => listen == active_listen && target == active_target,
        _ => false,
    }
}

fn respond(reply: Reply, result: Result<SshToolResult>) {
    if reply.send(result).is_err() {
        tracing::debug!("SSH tool requester closed");
    }
}

async fn execute(
    connection: &SshConnection,
    command: &str,
    mut cancel: watch::Receiver<bool>,
) -> Result<SshToolResult> {
    let mut channel = tokio::select! {
        channel = connection.open_auxiliary_channel() => channel?,
        _ = cancel.wait_for(|cancel| *cancel) => return Ok(SshToolResult::CommandCancelled),
    };
    let result = tokio::select! {
        result = timeout(COMMAND_TIMEOUT, async {
            channel.exec(true, command.as_bytes()).await?;
            match channel.wait().await {
                Some(ChannelMsg::Success) => channel.eof().await?,
                Some(ChannelMsg::Failure) => anyhow::bail!("the server rejected the command"),
                _ => anyhow::bail!("the server closed the channel before confirming the command"),
            }
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut exit_status = None;
            let mut exit_signal = false;
            let mut truncated = false;
            while let Some(message) = channel.wait().await {
                let remaining = MAX_OUTPUT_BYTES - stdout.len() - stderr.len();
                let (destination, data) = match message {
                    ChannelMsg::Data { data } => (&mut stdout, data),
                    ChannelMsg::ExtendedData { ext: 1, data } => (&mut stderr, data),
                    ChannelMsg::ExitStatus { exit_status: status } => {
                        exit_status = Some(status);
                        continue;
                    }
                    ChannelMsg::ExitSignal { .. } => {
                        exit_signal = true;
                        continue;
                    }
                    ChannelMsg::Close => break,
                    _ => continue,
                };
                truncated |= data.len() > remaining;
                destination.extend_from_slice(&data[..data.len().min(remaining)]);
            }
            Ok(SshToolResult::Command { stdout, stderr, exit_status, exit_signal, truncated })
        }) => result.context("remote command timed out").and_then(|result| result),
        _ = cancel.wait_for(|cancel| *cancel) => Ok(SshToolResult::CommandCancelled),
    };
    if !matches!(
        timeout(Duration::from_secs(2), channel.close()).await,
        Ok(Ok(()))
    ) {
        tracing::debug!("remote command channel could not be closed");
    }
    result
}
