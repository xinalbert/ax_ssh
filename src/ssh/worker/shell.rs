use std::time::Instant;

use tokio::task::JoinSet;
use tokio::time::{Instant as TokioInstant, sleep, timeout};

use crate::sftp::SftpTransferEvent;

use super::super::x11::{X11ChannelRequest, X11Dispatcher, X11Forwarding, X11PreparationError};
use super::super::{SshEvent, SshShell, X11RequestStatus};
use super::sftp::send_sftp_transfer_event;
use super::*;

pub(super) struct TerminalSessionTask {
    pub(super) connection: SshConnection,
    pub(super) session_id: Uuid,
    pub(super) x11_mode: Option<crate::config::X11ForwardingMode>,
    pub(super) x11_settings: crate::config::X11Settings,
    pub(super) audio_format: crate::audio::AudioFormat,
    pub(super) x11_dispatcher: Option<X11Dispatcher>,
    pub(super) x11_requests: Option<mpsc::Receiver<X11ChannelRequest>>,
    pub(super) command_rx: mpsc::Receiver<SshCommand>,
    pub(super) latency_foreground_rx: watch::Receiver<bool>,
    pub(super) resize_rx: watch::Receiver<TerminalSize>,
    pub(super) event_tx: mpsc::Sender<SshSessionEvent>,
}

pub(super) async fn run_terminal_session(task: TerminalSessionTask) {
    let TerminalSessionTask {
        connection,
        session_id,
        x11_mode,
        x11_settings,
        audio_format,
        x11_dispatcher,
        mut x11_requests,
        mut command_rx,
        mut resize_rx,
        latency_foreground_rx,
        event_tx,
    } = task;
    let (mut x11_forwarding, x11_unavailable) = match x11_mode {
        Some(mode) => match X11Forwarding::prepare(x11_settings, mode).await {
            Ok(forwarding) => (Some(forwarding), None),
            Err(error) => {
                warn!(session_id = %session_id, %error, "SSH X11 forwarding preparation failed");
                (None, Some(bounded_error_message(&error)))
            }
        },
        None => (None, None),
    };
    if x11_forwarding.is_none() {
        x11_requests = None;
    }
    let mut media_tasks = JoinSet::new();
    let (media_stop, media_shutdown) = watch::channel(false);
    let (automatic_stop, automatic_shutdown) = watch::channel(false);
    let (audio_ready, source_ready) = oneshot::channel();
    let audio_connection = connection.clone();
    let stop_on_shutdown = automatic_stop.clone();
    let mut shutdown = media_shutdown.clone();
    let audio_events = event_tx.clone();
    media_tasks.spawn(async move {
        let forwarding = super::media::forward_automatic_microphone(
            audio_connection,
            audio_format,
            automatic_shutdown,
            audio_ready,
            audio_events,
        );
        tokio::pin!(forwarding);
        let result = tokio::select! {
            result = &mut forwarding => result,
            _ = crate::audio::wait_for_stop(&mut shutdown) => {
                stop_on_shutdown.send_replace(true);
                forwarding.await
            }
        };
        if let Err(error) = result {
            debug!(%error, "standard remote microphone bridge unavailable");
        }
    });
    let setup = timeout(Duration::from_secs(5), source_ready);
    tokio::pin!(setup);
    let audio_source = loop {
        tokio::select! {
            result = &mut setup => {
                match result {
                    Ok(Ok(Ok(source))) => break Some(source),
                    _ => { automatic_stop.send_replace(true); break None; }
                }
            }
            command = command_rx.recv() => {
                if matches!(command, None | Some(SshCommand::Disconnect)) {
                    shutdown_media_tasks(&media_stop, &mut media_tasks).await;
                    if let Err(error) = connection.disconnect().await {
                        warn!(%error, "cannot disconnect during audio source setup");
                    }
                    send_event(&event_tx, SshSessionEvent::Disconnected, session_id).await;
                    return;
                }
            }
        }
    };
    let x11_requested = x11_forwarding.is_some();
    let initial_size = *resize_rx.borrow_and_update();
    let (mut shell, x11_request_status) = match connection
        .open_shell(
            initial_size,
            x11_forwarding.as_ref(),
            audio_source.as_deref(),
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
            shutdown_media_tasks(&media_stop, &mut media_tasks).await;
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
        shutdown_media_tasks(&media_stop, &mut media_tasks).await;
        close_shell(&shell, session_id).await;
        return;
    }
    if let Some(message) = x11_unavailable {
        if !send_event(
            &event_tx,
            SshSessionEvent::X11ForwardingUnavailable(message),
            session_id,
        )
        .await
        {
            shutdown_media_tasks(&media_stop, &mut media_tasks).await;
            close_shell(&shell, session_id).await;
            return;
        }
    } else if x11_request_status == X11RequestStatus::Enabled {
        if !send_event(&event_tx, SshSessionEvent::X11ForwardingEnabled, session_id).await {
            shutdown_media_tasks(&media_stop, &mut media_tasks).await;
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
            shutdown_media_tasks(&media_stop, &mut media_tasks).await;
            close_shell(&shell, session_id).await;
            return;
        }
    }

    let mut latency = LatencyMonitor::new(connection.clone(), latency_foreground_rx);
    let output_flush = sleep(OUTPUT_FLUSH_INTERVAL);
    tokio::pin!(output_flush);
    let mut output = Vec::new();
    let mut output_received_at = None;
    let mut awaiting_output_after_input = None;
    let mut x11_relays = JoinSet::new();
    let mut release_probes = JoinSet::new();
    let attachment_directories = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let mut microphone_task = None;
    let mut release_probe_started = false;
    let mut x11_unavailable_reported = false;
    let mut failed = false;
    let mut shell_exited = false;
    loop {
        tokio::select! {
            event = latency.next_event() => {
                if !send_event(&event_tx, event, session_id).await { break; }
            }
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
                        let send_result = async {
                            for chunk in data.chunks(TERMINAL_INPUT_CHUNK_BYTES) {
                                shell.send(chunk.to_vec()).await?;
                            }
                            Ok::<_, anyhow::Error>(())
                        }
                        .await;
                        if let Err(error) = send_result {
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
                    Some(SshCommand::ClipboardImage { png, reply }) => {
                        if media_tasks.len() >= 3 {
                            if reply.send(Err(anyhow::anyhow!("another media operation is busy"))).is_err() { debug!("attachment requester cancelled"); }
                        } else {
                            let connection = connection.clone();
                            let directories = attachment_directories.clone();
                            media_tasks.spawn(async move {
                                let result = super::media::upload_image(connection, png, directories).await;
                                if reply.send(result).is_err() { debug!("attachment requester cancelled"); }
                            });
                        }
                        continue;
                    }
                    Some(SshCommand::Microphone { format, pcm, mut stop, ready, finished }) => {
                        if microphone_task.as_ref().is_some_and(|task: &tokio::task::AbortHandle| !task.is_finished()) {
                            if ready.send(Err(anyhow::anyhow!("remote microphone already active"))).is_err() { debug!("microphone requester cancelled"); }
                            if finished.send(Ok(())).is_err() { debug!("microphone requester cancelled"); }
                        } else {
                            let connection = connection.clone();
                            let mut shutdown = media_shutdown.clone();
                            let (cancel, cancellation) = watch::channel(false);
                            microphone_task = Some(media_tasks.spawn(async move {
                                let forwarding = super::media::forward_microphone(connection, format, pcm, cancellation, ready);
                                tokio::pin!(forwarding);
                                let result = tokio::select! {
                                    result = &mut forwarding => result,
                                    _ = crate::audio::wait_for_stop(&mut stop) => {
                                        cancel.send_replace(true);
                                        forwarding.await
                                    }
                                    _ = crate::audio::wait_for_stop(&mut shutdown) => {
                                        cancel.send_replace(true);
                                        forwarding.await
                                    }
                                };
                                if finished.send(result).is_err() { debug!("microphone requester cancelled"); }
                            }));
                        }
                        continue;
                    }
                    Some(SshCommand::ReadSystemRelease) => {
                        if release_probe_started {
                            continue;
                        }
                        release_probe_started = true;
                        let connection = connection.clone();
                        let event_tx = event_tx.clone();
                        release_probes.spawn(async move {
                            let release = connection.read_system_release().await;
                            let _ = send_event(
                                &event_tx,
                                SshSessionEvent::SystemRelease(release),
                                session_id,
                            )
                            .await;
                        });
                        continue;
                    }
                    Some(SshCommand::ListSftp { .. } | SshCommand::LoadMoreSftp { .. }) => continue,
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
                    Some(SshCommand::OpenSftpFileAtLocalPath { request }) => {
                        send_sftp_transfer_event(
                            &event_tx,
                            SftpTransferEvent::Failed {
                                transfer_id: request.transfer_id(),
                                message: "Remote file opening is available only in an SFTP tab"
                                    .to_owned(),
                            },
                            session_id,
                        )
                        .await;
                        continue;
                    }
                    Some(SshCommand::OpenSftpUploads { requests }) => {
                        for request in requests {
                            send_sftp_transfer_event(
                                &event_tx,
                                SftpTransferEvent::Failed {
                                    transfer_id: request.transfer_id(),
                                    message: "Remote upload is available only in an SFTP tab".to_owned(),
                                },
                                session_id,
                            )
                            .await;
                        }
                        continue;
                    }
                    Some(SshCommand::CancelSftpTransfer { .. })
                    | Some(SshCommand::PauseSftpTransfer { .. })
                    | Some(SshCommand::ResumeSftpTransfer { .. })
                    | Some(SshCommand::ResolveSftpUploadConflict { .. })
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
                if let Err(error) = shell.resize(size).await {
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
                        columns: size.columns(),
                        rows: size.rows(),
                    },
                    session_id,
                ).await {
                    break;
                }
            }
            event = shell.next_event() => {
                match event {
                    Ok(SshEvent::Output(data)) => {
                        let received_at = Instant::now();
                        let arm_output_flush = output.is_empty();
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
                        if arm_output_flush && !output.is_empty() {
                            output_flush
                                .as_mut()
                                .reset(TokioInstant::now() + OUTPUT_FLUSH_INTERVAL);
                        }
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
                    Ok(SshEvent::ShellExited) => {
                        info!(session_id = %session_id, "SSH shell closed by remote peer");
                        shell_exited = true;
                        break;
                    }
                    Ok(SshEvent::Disconnected) => {
                        info!(session_id = %session_id, "SSH transport closed while waiting for shell output");
                        break;
                    }
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
            media = media_tasks.join_next(), if !media_tasks.is_empty() => {
                if let Some(Err(error)) = media && !error.is_cancelled() { warn!(%error, "SSH media task failed"); }
            }
            probe = release_probes.join_next(), if !release_probes.is_empty() => {
                if let Some(Err(error)) = probe
                    && !error.is_cancelled()
                {
                    warn!(session_id = %session_id, %error, "SSH system release probe task failed");
                }
            }
            _ = &mut output_flush, if !output.is_empty() => {
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
    if let Some(dispatcher) = &x11_dispatcher {
        dispatcher.disable();
    }
    drop(x11_requests.take());
    shutdown_x11_relays(&mut x11_relays, session_id).await;
    release_probes.abort_all();
    while release_probes.join_next().await.is_some() {}
    shutdown_media_tasks(&media_stop, &mut media_tasks).await;
    if let Err(error) =
        super::media::cleanup_attachments(&connection, &attachment_directories).await
    {
        warn!(%error, "SSH attachment cleanup failed");
    }
    close_shell(&shell, session_id).await;
    if let Err(error) = connection.disconnect().await {
        warn!(session_id = %session_id, %error, "SSH transport disconnect failed");
    }
    if !failed {
        let event = if shell_exited {
            SshSessionEvent::ShellExited
        } else {
            SshSessionEvent::Disconnected
        };
        send_event(&event_tx, event, session_id).await;
    }
}

pub(super) fn x11_mode_for(
    mode: SshSessionMode,
    profile: &SessionProfile,
) -> Option<crate::config::X11ForwardingMode> {
    (mode == SshSessionMode::Terminal)
        .then(|| profile.ssh().map(|config| config.x11_forwarding))
        .flatten()
        .filter(|mode| mode.enabled())
}

async fn shutdown_media_tasks(stop: &watch::Sender<bool>, tasks: &mut JoinSet<()>) {
    stop.send_replace(true);
    if timeout(Duration::from_secs(5), async {
        while let Some(result) = tasks.join_next().await {
            if let Err(error) = result
                && !error.is_cancelled()
            {
                warn!(%error, "SSH media shutdown failed");
            }
        }
    })
    .await
    .is_err()
    {
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        warn!("SSH media shutdown timed out");
    }
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
        SshSessionEvent::Output(crate::terminal::TerminalOutputChunk::with_received_at(
            data,
            received_at,
        )),
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
    fn x11_mode_is_requested_only_for_opted_in_terminal_profiles() {
        let mut ssh = SessionProfile::new("ssh", "host.example", "alice");
        assert_eq!(
            x11_mode_for(SshSessionMode::Terminal, &ssh),
            Some(crate::config::X11ForwardingMode::Trusted)
        );
        ssh.ssh_mut().expect("profile should be SSH").x11_forwarding =
            crate::config::X11ForwardingMode::Off;
        assert_eq!(x11_mode_for(SshSessionMode::Terminal, &ssh), None);
        ssh.ssh_mut().expect("profile should be SSH").x11_forwarding =
            crate::config::X11ForwardingMode::Untrusted;
        assert_eq!(x11_mode_for(SshSessionMode::Sftp, &ssh), None);

        let telnet = SessionProfile::new_telnet("telnet", "host.example");
        assert_eq!(x11_mode_for(SshSessionMode::Terminal, &telnet), None);
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
