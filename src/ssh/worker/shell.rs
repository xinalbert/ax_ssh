use std::time::Instant;

use tokio::task::JoinSet;
use tokio::time::{MissedTickBehavior, interval, timeout};

use crate::sftp::{SftpBrowserHandle, SftpTransferEvent};

use super::super::x11::{X11ChannelRequest, X11Dispatcher, X11Forwarding, X11PreparationError};
use super::super::{SshEvent, SshShell, X11RequestStatus};
use super::sftp::{receive_sftp_event, send_sftp_event, send_sftp_transfer_event};
use super::*;

pub(super) struct TerminalSessionTask {
    pub(super) connection: SshConnection,
    pub(super) session_id: Uuid,
    pub(super) x11_requested: bool,
    pub(super) x11_forwarding: Option<X11Forwarding>,
    pub(super) x11_dispatcher: Option<X11Dispatcher>,
    pub(super) x11_requests: Option<mpsc::Receiver<X11ChannelRequest>>,
    pub(super) command_rx: mpsc::Receiver<SshCommand>,
    pub(super) resize_rx: watch::Receiver<TerminalSize>,
    pub(super) event_tx: mpsc::Sender<SshSessionEvent>,
}

pub(super) async fn run_terminal_session(task: TerminalSessionTask) {
    let TerminalSessionTask {
        connection,
        session_id,
        x11_requested,
        mut x11_forwarding,
        x11_dispatcher,
        mut x11_requests,
        mut command_rx,
        mut resize_rx,
        event_tx,
    } = task;
    let initial_size = *resize_rx.borrow_and_update();
    let (mut shell, x11_request_status) = match connection
        .open_shell(
            initial_size.columns,
            initial_size.rows,
            x11_forwarding.as_ref(),
        )
        .await
    {
        Ok(shell) => shell,
        Err(error) => {
            warn!(session_id = %session_id, %error, "SSH worker failed to open interactive shell");
            send_event(
                &event_tx,
                SshSessionEvent::Failed(bounded_error_message(&error)),
                session_id,
            )
            .await;
            return;
        }
    };

    match x11_request_status {
        X11RequestStatus::Enabled => {
            if let Some(dispatcher) = &x11_dispatcher {
                dispatcher.enable();
            }
        }
        X11RequestStatus::Rejected => {
            if let Some(dispatcher) = &x11_dispatcher {
                dispatcher.disable();
            }
            x11_requests = None;
            x11_forwarding = None;
        }
        X11RequestStatus::NotRequested => {}
    }

    if !send_event(&event_tx, SshSessionEvent::Connected, session_id).await {
        close_shell(&shell, session_id).await;
        return;
    }
    if x11_request_status == X11RequestStatus::Enabled {
        if !send_event(&event_tx, SshSessionEvent::X11ForwardingEnabled, session_id).await {
            close_shell(&shell, session_id).await;
            return;
        }
    } else if x11_requested {
        let message = "The SSH server rejected X11 forwarding".to_owned();
        if !send_event(
            &event_tx,
            SshSessionEvent::X11ForwardingUnavailable(message),
            session_id,
        )
        .await
        {
            close_shell(&shell, session_id).await;
            return;
        }
    }

    let mut output_flush = interval(OUTPUT_FLUSH_INTERVAL);
    output_flush.set_missed_tick_behavior(MissedTickBehavior::Skip);
    output_flush.tick().await;
    let mut output = Vec::new();
    let mut output_received_at = None;
    let mut awaiting_output_after_input = None;
    let mut sftp: Option<SftpBrowserHandle> = None;
    let mut sftp_events: Option<mpsc::Receiver<SftpBrowserEvent>> = None;
    let mut x11_relays = JoinSet::new();
    let mut x11_unavailable_reported = false;
    let mut failed = false;
    loop {
        tokio::select! {
            command = command_rx.recv() => {
                match command {
                    Some(SshCommand::Send {
                        input_sequence,
                        queued_at,
                        data,
                    }) => {
                        tracing::debug!(
                            target: LATENCY_TARGET,
                            event = "ssh-input",
                            stage = "worker-dequeued",
                            session_id = %session_id,
                            input_sequence,
                            queue_us = elapsed_micros(queued_at),
                            "SSH worker dequeued terminal input"
                        );
                        let send_started_at = Instant::now();
                        if let Err(error) = shell.send(data).await {
                            tracing::debug!(
                                target: LATENCY_TARGET,
                                event = "ssh-input",
                                stage = "russh-call-failed",
                                session_id = %session_id,
                                input_sequence,
                                call_us = elapsed_micros(send_started_at),
                                "russh terminal data call failed"
                            );
                            warn!(session_id = %session_id, %error, "failed to send terminal input");
                            send_event(
                                &event_tx,
                                SshSessionEvent::Failed(bounded_error_message(&error)),
                                session_id,
                            ).await;
                            failed = true;
                            break;
                        }
                        tracing::debug!(
                            target: LATENCY_TARGET,
                            event = "ssh-input",
                            stage = "russh-call-complete",
                            session_id = %session_id,
                            input_sequence,
                            call_us = elapsed_micros(send_started_at),
                            "russh terminal data call completed"
                        );
                        awaiting_output_after_input = Some((input_sequence, Instant::now()));
                        continue;
                    }
                    Some(SshCommand::OpenSftp { path }) => {
                        if sftp.as_ref().is_some_and(SftpBrowserHandle::is_finished) {
                            if let Some(browser) = sftp.take()
                                && let Err(error) = browser.shutdown().await
                            {
                                warn!(session_id = %session_id, %error, "failed to join stopped SFTP browser");
                            }
                            sftp_events = None;
                        }
                        if sftp.is_some() {
                            if let Some(browser) = sftp.as_ref()
                                && let Err(error) = browser.request_list(path)
                            {
                                send_sftp_event(
                                    &event_tx,
                                    SftpBrowserEvent::Failed(bounded_error_message(&error)),
                                    session_id,
                                ).await;
                            }
                        } else {
                            let (browser_event_tx, browser_event_rx) =
                                mpsc::channel(SFTP_EVENT_CAPACITY);
                            match connection.open_sftp_stream().await {
                                Ok(stream) => match SftpBrowserHandle::spawn(
                                    &Handle::current(),
                                    stream,
                                    path,
                                    browser_event_tx,
                                ) {
                                    Ok(browser) => {
                                        sftp = Some(browser);
                                        sftp_events = Some(browser_event_rx);
                                    }
                                    Err(error) => {
                                        send_sftp_event(
                                            &event_tx,
                                            SftpBrowserEvent::Failed(bounded_error_message(&error)),
                                            session_id,
                                        ).await;
                                        send_sftp_event(
                                            &event_tx,
                                            SftpBrowserEvent::Closed,
                                            session_id,
                                        ).await;
                                    }
                                },
                                Err(error) => {
                                    send_sftp_event(
                                        &event_tx,
                                        SftpBrowserEvent::Failed(bounded_error_message(&error)),
                                        session_id,
                                    ).await;
                                    send_sftp_event(
                                        &event_tx,
                                        SftpBrowserEvent::Closed,
                                        session_id,
                                    ).await;
                                }
                            }
                        }
                        continue;
                    }
                    Some(SshCommand::ListSftp { path }) => {
                        match sftp.as_ref() {
                            Some(browser) => {
                                if let Err(error) = browser.request_list(path) {
                                    send_sftp_event(
                                        &event_tx,
                                        SftpBrowserEvent::Failed(bounded_error_message(&error)),
                                        session_id,
                                    ).await;
                                }
                            }
                            None => {
                                send_sftp_event(
                                    &event_tx,
                                    SftpBrowserEvent::Failed("SFTP browser is not open".to_owned()),
                                    session_id,
                                ).await;
                                send_sftp_event(
                                    &event_tx,
                                    SftpBrowserEvent::Closed,
                                    session_id,
                                ).await;
                            }
                        }
                        continue;
                    }
                    Some(SshCommand::LoadMoreSftp) => {
                        match sftp.as_ref() {
                            Some(browser) => {
                                if let Err(error) = browser.request_load_more() {
                                    send_sftp_event(
                                        &event_tx,
                                        SftpBrowserEvent::Failed(bounded_error_message(&error)),
                                        session_id,
                                    ).await;
                                }
                            }
                            None => {
                                send_sftp_event(
                                    &event_tx,
                                    SftpBrowserEvent::Failed("SFTP browser is not open".to_owned()),
                                    session_id,
                                ).await;
                                send_sftp_event(
                                    &event_tx,
                                    SftpBrowserEvent::Closed,
                                    session_id,
                                ).await;
                            }
                        }
                        continue;
                    }
                    Some(SshCommand::CloseSftp) => {
                        if let Some(browser) = sftp.take()
                            && let Err(error) = browser.shutdown().await
                        {
                            warn!(session_id = %session_id, %error, "failed to close SFTP browser");
                        }
                        sftp_events = None;
                        send_sftp_event(&event_tx, SftpBrowserEvent::Closed, session_id).await;
                        continue;
                    }
                    Some(SshCommand::OpenSftpFile { root }) => {
                        send_sftp_transfer_event(
                            &event_tx,
                            SftpTransferEvent::Failed {
                                transfer_id: root.transfer_id(),
                                message: "Remote file opening is available only in an SFTP tab"
                                    .to_owned(),
                            },
                            session_id,
                        )
                        .await;
                        continue;
                    }
                    Some(SshCommand::OpenSftpUpload { request }) => {
                        send_sftp_transfer_event(
                            &event_tx,
                            SftpTransferEvent::Failed {
                                transfer_id: request.transfer_id(),
                                message: "Remote upload is available only in an SFTP tab".to_owned(),
                            },
                            session_id,
                        )
                        .await;
                        continue;
                    }
                    Some(SshCommand::CancelSftpTransfer { .. })
                    | Some(SshCommand::PauseSftpTransfer { .. })
                    | Some(SshCommand::ResumeSftpTransfer { .. })
                    | Some(SshCommand::SftpWrite { .. }) => {
                        continue;
                    }
                    Some(SshCommand::Disconnect) => {
                        info!(session_id = %session_id, "SSH disconnect requested");
                    }
                    None => {
                        info!(session_id = %session_id, "SSH controller dropped; disconnecting worker");
                    }
                }
                break;
            }
            changed = resize_rx.changed() => {
                if changed.is_err() {
                    info!(session_id = %session_id, "terminal resize controller dropped; disconnecting worker");
                    break;
                }
                let size = *resize_rx.borrow_and_update();
                if let Err(error) = shell.resize(size.columns, size.rows).await {
                    warn!(session_id = %session_id, %error, "failed to resize remote terminal");
                    send_event(
                        &event_tx,
                        SshSessionEvent::Failed(bounded_error_message(&error)),
                        session_id,
                    ).await;
                    failed = true;
                    break;
                }
                if !send_event(
                    &event_tx,
                    SshSessionEvent::Resized {
                        columns: size.columns,
                        rows: size.rows,
                    },
                    session_id,
                ).await {
                    break;
                }
            }
            event = shell.next_event() => {
                match event {
                    Ok(Some(SshEvent::Output(data))) => {
                        let received_at = Instant::now();
                        output_received_at.get_or_insert(received_at);
                        let input_observation = awaiting_output_after_input.take();
                        let flush_for_input = input_observation.is_some();
                        if let Some((input_sequence, send_completed_at)) = input_observation {
                            tracing::debug!(
                                target: LATENCY_TARGET,
                                event = "ssh-output",
                                stage = "first-output-after-input",
                                session_id = %session_id,
                                input_sequence,
                                since_send_call_us = elapsed_micros(send_completed_at),
                                association = "temporal-only",
                                "SSH worker received output after terminal input"
                            );
                        }
                        output.extend_from_slice(&data);
                        if (flush_for_input || output.len() >= MAX_OUTPUT_BATCH_BYTES)
                            && !flush_output(
                                &event_tx,
                                &mut output,
                                &mut output_received_at,
                                session_id,
                            ).await
                        {
                            break;
                        }
                    }
                    Ok(Some(SshEvent::Disconnected)) => {
                        info!(session_id = %session_id, "SSH shell closed by remote peer");
                        break;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        warn!(session_id = %session_id, %error, "SSH shell receive failed");
                        send_event(
                            &event_tx,
                            SshSessionEvent::Failed(bounded_error_message(&error)),
                            session_id,
                        ).await;
                        failed = true;
                        break;
                    }
                }
            }
            event = receive_sftp_event(&mut sftp_events), if sftp_events.is_some() => {
                match event {
                    Some(event) => {
                        let closed = matches!(event, SftpBrowserEvent::Closed);
                        if !send_sftp_event(&event_tx, event, session_id).await {
                            break;
                        }
                        if closed {
                            if let Some(browser) = sftp.take()
                                && let Err(error) = browser.shutdown().await
                            {
                                warn!(session_id = %session_id, %error, "failed to join closed SFTP browser");
                            }
                            sftp_events = None;
                        }
                    }
                    None => {
                        if let Some(browser) = sftp.take()
                            && let Err(error) = browser.shutdown().await
                        {
                            warn!(session_id = %session_id, %error, "failed to join SFTP browser after event channel closed");
                        }
                        sftp_events = None;
                    }
                }
            }
            request = receive_x11_request(&mut x11_requests), if x11_requests.is_some() => {
                match request {
                    Some(request) if x11_relays.len() >= MAX_X11_RELAYS => {
                        request.reject(russh::ChannelOpenFailure::ResourceShortage).await;
                    }
                    Some(request) => {
                        let Some(forwarding) = x11_forwarding.clone() else {
                            request
                                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                                .await;
                            continue;
                        };
                        x11_relays.spawn(async move { forwarding.relay(request).await });
                    }
                    None => {
                        x11_requests = None;
                    }
                }
            }
            relay = x11_relays.join_next(), if !x11_relays.is_empty() => {
                match relay {
                    Some(Ok(Ok(()))) | None => {}
                    Some(Ok(Err(error))) => {
                        warn!(session_id = %session_id, %error, "SSH X11 relay stopped with an error");
                        if error.downcast_ref::<X11PreparationError>().is_some()
                            && !x11_unavailable_reported
                        {
                            x11_unavailable_reported = true;
                            if !send_event(
                                &event_tx,
                                SshSessionEvent::X11ForwardingUnavailable(bounded_error_message(&error)),
                                session_id,
                            )
                            .await
                            {
                                break;
                            }
                        }
                    }
                    Some(Err(error)) if error.is_cancelled() => {}
                    Some(Err(error)) => {
                        warn!(session_id = %session_id, %error, "SSH X11 relay task failed");
                    }
                }
            }
            _ = output_flush.tick() => {
                if !flush_output(
                    &event_tx,
                    &mut output,
                    &mut output_received_at,
                    session_id,
                ).await {
                    break;
                }
            }
        }
    }

    flush_output(&event_tx, &mut output, &mut output_received_at, session_id).await;
    if let Some(browser) = sftp
        && let Err(error) = browser.shutdown().await
    {
        warn!(session_id = %session_id, %error, "failed to shut down SFTP browser");
    }
    if let Some(dispatcher) = &x11_dispatcher {
        dispatcher.disable();
    }
    drop(x11_requests.take());
    shutdown_x11_relays(&mut x11_relays, session_id).await;
    close_shell(&shell, session_id).await;
    if let Err(error) = connection.disconnect().await {
        warn!(session_id = %session_id, %error, "SSH transport disconnect failed");
    }
    if !failed {
        send_event(&event_tx, SshSessionEvent::Disconnected, session_id).await;
    }
}

pub(super) fn x11_requested_for(mode: SshSessionMode, profile: &SessionProfile) -> bool {
    mode == SshSessionMode::Terminal && profile.ssh().is_some_and(|config| config.x11_forwarding)
}

async fn receive_x11_request(
    requests: &mut Option<mpsc::Receiver<X11ChannelRequest>>,
) -> Option<X11ChannelRequest> {
    match requests {
        Some(requests) => requests.recv().await,
        None => None,
    }
}

async fn shutdown_x11_relays(relays: &mut JoinSet<Result<()>>, session_id: Uuid) {
    relays.abort_all();
    if timeout(X11_RELAY_SHUTDOWN_TIMEOUT, async {
        while relays.join_next().await.is_some() {}
    })
    .await
    .is_err()
    {
        warn!(session_id = %session_id, "SSH X11 relay shutdown timed out");
    }
}

async fn close_shell(shell: &SshShell, session_id: Uuid) {
    match timeout(DISCONNECT_TIMEOUT, shell.disconnect()).await {
        Ok(Ok(())) => info!(session_id = %session_id, "SSH shell disconnected"),
        Ok(Err(error)) => warn!(session_id = %session_id, %error, "SSH shell disconnect failed"),
        Err(_) => warn!(session_id = %session_id, "SSH shell disconnect timed out"),
    }
}

async fn flush_output(
    event_tx: &mpsc::Sender<SshSessionEvent>,
    output: &mut Vec<u8>,
    output_received_at: &mut Option<Instant>,
    session_id: Uuid,
) -> bool {
    if output.is_empty() {
        return true;
    }
    let data = std::mem::take(output);
    let received_at = output_received_at.take().unwrap_or_else(Instant::now);
    send_event(
        event_tx,
        SshSessionEvent::Output { data, received_at },
        session_id,
    )
    .await
}

fn elapsed_micros(started_at: Instant) -> u64 {
    u64::try_from(started_at.elapsed().as_micros()).unwrap_or(u64::MAX)
}
#[cfg(test)]
mod x11_tests {
    use std::future::pending;

    use super::*;

    #[test]
    fn x11_is_requested_only_for_opted_in_terminal_profiles() {
        let mut ssh = SessionProfile::new("ssh", "host.example", "alice");
        assert!(x11_requested_for(SshSessionMode::Terminal, &ssh));
        ssh.ssh_mut().expect("profile should be SSH").x11_forwarding = false;
        assert!(!x11_requested_for(SshSessionMode::Terminal, &ssh));
        ssh.ssh_mut().expect("profile should be SSH").x11_forwarding = true;
        assert!(!x11_requested_for(SshSessionMode::Sftp, &ssh));

        let telnet = SessionProfile::new_telnet("telnet", "host.example");
        assert!(!x11_requested_for(SshSessionMode::Terminal, &telnet));
    }

    #[tokio::test]
    async fn relay_shutdown_aborts_and_joins_active_tasks() {
        let mut relays = JoinSet::new();
        relays.spawn(async {
            pending::<()>().await;
            Ok::<(), anyhow::Error>(())
        });
        shutdown_x11_relays(&mut relays, Uuid::new_v4()).await;
        assert!(relays.is_empty());
    }
}
