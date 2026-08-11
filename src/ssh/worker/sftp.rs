use std::collections::HashMap;

use russh::{ChannelStream, client};
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio::time::timeout;

use crate::sftp::{SFTP_TRANSFER_EVENT_CAPACITY, SftpBrowserHandle, SftpDownloadHandle};

use super::*;

struct PendingSftpOpen {
    task_id: tokio::task::Id,
    cancellation: Option<oneshot::Sender<()>>,
    cancelled: bool,
    request: SftpDownloadRequest,
}

type SftpChannelStream = ChannelStream<client::Msg>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingSftpCancellation {
    Requested,
    AlreadyRequested,
    Missing,
}

pub(super) async fn run_sftp_session(
    connection: SshConnection,
    session_id: Uuid,
    initial_path: String,
    mut command_rx: mpsc::Receiver<SshCommand>,
    event_tx: mpsc::Sender<SshSessionEvent>,
) {
    let (browser_event_tx, mut browser_events) = mpsc::channel(SFTP_EVENT_CAPACITY);
    let (transfer_event_tx, mut transfer_events) = mpsc::channel(SFTP_TRANSFER_EVENT_CAPACITY);
    let stream = match open_initial_sftp_stream(&connection, &mut command_rx, session_id).await {
        None => {
            disconnect_sftp_connection(&connection, session_id).await;
            let _ = send_event(&event_tx, SshSessionEvent::Disconnected, session_id).await;
            return;
        }
        Some(Ok(stream)) => stream,
        Some(Err(error)) => {
            warn!(session_id = %session_id, %error, "SFTP-only worker failed to open subsystem");
            send_event(
                &event_tx,
                SshSessionEvent::Failed(bounded_error_message(&error)),
                session_id,
            )
            .await;
            disconnect_sftp_connection(&connection, session_id).await;
            return;
        }
    };
    let browser = match SftpBrowserHandle::spawn(
        &Handle::current(),
        stream,
        initial_path,
        browser_event_tx,
    ) {
        Ok(browser) => browser,
        Err(error) => {
            warn!(session_id = %session_id, %error, "SFTP-only worker failed to start browser");
            send_event(
                &event_tx,
                SshSessionEvent::Failed(bounded_error_message(&error)),
                session_id,
            )
            .await;
            disconnect_sftp_connection(&connection, session_id).await;
            return;
        }
    };
    if !send_event(&event_tx, SshSessionEvent::Connected, session_id).await {
        let _ = browser.shutdown().await;
        disconnect_sftp_connection(&connection, session_id).await;
        return;
    }

    let mut failed = false;
    let mut browser_error: Option<String> = None;
    let mut transfers = Vec::<SftpDownloadHandle>::new();
    let mut pending_openings = JoinSet::<Result<SftpChannelStream>>::new();
    let mut pending_by_transfer = HashMap::<Uuid, PendingSftpOpen>::new();
    loop {
        tokio::select! {
            opening = pending_openings.join_next_with_id(), if !pending_openings.is_empty() => {
                let Some(opening) = opening else {
                    continue;
                };
                let task_id = match &opening {
                    Ok((task_id, _)) => *task_id,
                    Err(error) => error.id(),
                };
                let Some((transfer_id, pending)) = take_pending_sftp_open(
                    &mut pending_by_transfer,
                    task_id,
                ) else {
                    warn!(%session_id, ?task_id, "completed an untracked SFTP subsystem opening task");
                    let _ = opening;
                    continue;
                };

                if pending.cancelled {
                    let _ = opening;
                    continue;
                }

                match opening {
                    Ok((_, Ok(stream))) => {
                        transfers.push(SftpDownloadHandle::spawn(
                            &Handle::current(),
                            stream,
                            pending.request,
                            transfer_event_tx.clone(),
                        ));
                    }
                    Ok((_, Err(error))) => {
                        send_sftp_transfer_event(
                            &event_tx,
                            SftpTransferEvent::Failed {
                                transfer_id,
                                message: bounded_error_message(&error),
                            },
                            session_id,
                        )
                        .await;
                    }
                    Err(error) => {
                        warn!(%session_id, %transfer_id, %error, "SFTP subsystem opening task failed");
                        send_sftp_transfer_event(
                            &event_tx,
                            SftpTransferEvent::Failed {
                                transfer_id,
                                message: bounded_text("SFTP subsystem opening task failed"),
                            },
                            session_id,
                        )
                        .await;
                    }
                }
            }
            command = command_rx.recv() => {
                reap_finished_sftp_transfers(&mut transfers, &event_tx, session_id).await;
                let result = match command {
                    Some(SshCommand::OpenSftp { path })
                    | Some(SshCommand::ListSftp { path }) => browser.request_list(path),
                    Some(SshCommand::LoadMoreSftp) => browser.request_load_more(),
                    Some(SshCommand::OpenSftpFile { request }) => {
                        let transfer_id = request.transfer_id();
                        let already_active = transfers
                            .iter()
                            .any(|transfer| transfer.transfer_id() == transfer_id)
                            || pending_by_transfer.contains_key(&transfer_id);
                        if already_active {
                            send_sftp_transfer_event(
                                &event_tx,
                                SftpTransferEvent::Failed {
                                    transfer_id,
                                    message: "SFTP transfer is already active".to_owned(),
                                },
                                session_id,
                            )
                            .await;
                            Ok(())
                        } else if sftp_transfer_limit_reached(
                            transfers.len(),
                            pending_by_transfer.len(),
                        ) {
                            send_sftp_transfer_event(
                                &event_tx,
                                SftpTransferEvent::Failed {
                                    transfer_id,
                                    message: format!(
                                        "SFTP allows at most {MAX_SFTP_TRANSFERS} active downloads"
                                    ),
                                },
                                session_id,
                            )
                            .await;
                            Ok(())
                        } else {
                            spawn_pending_sftp_open(
                                &mut pending_openings,
                                &mut pending_by_transfer,
                                &connection,
                                request,
                            );
                            Ok(())
                        }
                    }
                    Some(SshCommand::CancelSftpTransfer { transfer_id }) => {
                        if let Some(transfer) = transfers
                            .iter()
                            .find(|transfer| transfer.transfer_id() == transfer_id)
                        {
                            transfer.cancel();
                            Ok(())
                        } else {
                            match cancel_pending_sftp_open(
                                &mut pending_by_transfer,
                                transfer_id,
                            ) {
                                PendingSftpCancellation::Requested => {
                                    send_sftp_transfer_event(
                                        &event_tx,
                                        SftpTransferEvent::Cancelled { transfer_id },
                                        session_id,
                                    )
                                    .await;
                                }
                                PendingSftpCancellation::AlreadyRequested => {}
                                PendingSftpCancellation::Missing => {
                                    send_sftp_transfer_event(
                                        &event_tx,
                                        SftpTransferEvent::Failed {
                                            transfer_id,
                                            message: "SFTP transfer is no longer active".to_owned(),
                                        },
                                        session_id,
                                    )
                                    .await;
                                }
                            }
                            Ok(())
                        }
                    }
                    Some(SshCommand::CloseSftp | SshCommand::Disconnect) | None => break,
                    Some(SshCommand::Send { .. }) => {
                        Err(anyhow::anyhow!("SFTP-only session cannot accept terminal input"))
                    }
                };
                if let Err(error) = result {
                    failed = true;
                    send_event(
                        &event_tx,
                        SshSessionEvent::Failed(bounded_error_message(&error)),
                        session_id,
                    ).await;
                    break;
                }
            }
            transfer_event = transfer_events.recv() => {
                let Some(transfer_event) = transfer_event else {
                    failed = true;
                    send_event(
                        &event_tx,
                        SshSessionEvent::Failed("SFTP transfer event channel closed".to_owned()),
                        session_id,
                    )
                    .await;
                    break;
                };
                let terminal_transfer_id = match &transfer_event {
                    SftpTransferEvent::Completed { transfer_id, .. }
                    | SftpTransferEvent::Cancelled { transfer_id }
                    | SftpTransferEvent::Failed { transfer_id, .. } => Some(*transfer_id),
                    SftpTransferEvent::Started { .. } | SftpTransferEvent::Progress { .. } => None,
                };
                if !send_sftp_transfer_event(&event_tx, transfer_event, session_id).await {
                    break;
                }
                if let Some(transfer_id) = terminal_transfer_id
                    && let Some(index) = transfers
                        .iter()
                        .position(|transfer| transfer.transfer_id() == transfer_id)
                {
                    let transfer = transfers.swap_remove(index);
                    if let Err(error) = transfer.shutdown().await {
                        warn!(%session_id, %transfer_id, %error, "failed to join completed SFTP transfer");
                    }
                }
            }
            event = browser_events.recv() => {
                let Some(event) = event else {
                    failed = true;
                    send_event(
                        &event_tx,
                        SshSessionEvent::Failed("SFTP browser event channel closed".to_owned()),
                        session_id,
                    ).await;
                    break;
                };
                let closed = matches!(event, SftpBrowserEvent::Closed);
                if let SftpBrowserEvent::Failed(message) = &event {
                    browser_error = Some(message.clone());
                }
                if !send_sftp_event(&event_tx, event, session_id).await {
                    break;
                }
                if closed {
                    if let Some(message) = browser_error.take() {
                        failed = true;
                        send_event(
                            &event_tx,
                            SshSessionEvent::Failed(message),
                            session_id,
                        )
                        .await;
                    }
                    break;
                }
            }
        }
    }

    shutdown_pending_sftp_opens(&mut pending_openings, &mut pending_by_transfer, session_id).await;
    shutdown_sftp_transfers(transfers, session_id).await;
    if let Err(error) = browser.shutdown().await {
        warn!(session_id = %session_id, %error, "failed to shut down SFTP-only browser");
    }
    if let Err(error) = connection.disconnect().await {
        warn!(session_id = %session_id, %error, "SFTP-only transport disconnect failed");
    }
    if !failed {
        send_event(&event_tx, SshSessionEvent::Disconnected, session_id).await;
    }
}

async fn open_initial_sftp_stream(
    connection: &SshConnection,
    command_rx: &mut mpsc::Receiver<SshCommand>,
    session_id: Uuid,
) -> Option<Result<SftpChannelStream>> {
    let opening = connection.open_sftp_stream();
    tokio::pin!(opening);
    loop {
        tokio::select! {
            result = &mut opening => return Some(result),
            command = command_rx.recv() => match command {
                Some(SshCommand::CloseSftp | SshCommand::Disconnect) | None => {
                    debug!(%session_id, "SFTP subsystem opening cancelled by session close");
                    return None;
                }
                Some(_) => {
                    debug!(%session_id, "ignoring SFTP command while the initial subsystem is opening");
                }
            },
        }
    }
}

async fn disconnect_sftp_connection(connection: &SshConnection, session_id: Uuid) {
    match timeout(DISCONNECT_TIMEOUT, connection.disconnect()).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => warn!(%session_id, %error, "SFTP transport disconnect failed"),
        Err(_) => warn!(%session_id, "SFTP transport disconnect timed out"),
    }
}

fn spawn_pending_sftp_open(
    pending_openings: &mut JoinSet<Result<SftpChannelStream>>,
    pending_by_transfer: &mut HashMap<Uuid, PendingSftpOpen>,
    connection: &SshConnection,
    request: SftpDownloadRequest,
) {
    let transfer_id = request.transfer_id();
    let (cancellation, cancelled) = oneshot::channel();
    let connection = connection.clone();
    let task = pending_openings.spawn(async move {
        let stream = tokio::select! {
            result = connection.open_sftp_stream() => result,
            _ = cancelled => Err(anyhow::anyhow!("SFTP subsystem opening cancelled")),
        }?;
        Ok(stream)
    });
    pending_by_transfer.insert(
        transfer_id,
        PendingSftpOpen {
            task_id: task.id(),
            cancellation: Some(cancellation),
            cancelled: false,
            request,
        },
    );
}

fn take_pending_sftp_open(
    pending_by_transfer: &mut HashMap<Uuid, PendingSftpOpen>,
    task_id: tokio::task::Id,
) -> Option<(Uuid, PendingSftpOpen)> {
    let transfer_id = pending_by_transfer
        .iter()
        .find_map(|(transfer_id, pending)| (pending.task_id == task_id).then_some(*transfer_id))?;
    pending_by_transfer
        .remove(&transfer_id)
        .map(|pending| (transfer_id, pending))
}

fn cancel_pending_sftp_open(
    pending_by_transfer: &mut HashMap<Uuid, PendingSftpOpen>,
    transfer_id: Uuid,
) -> PendingSftpCancellation {
    let Some(pending) = pending_by_transfer.get_mut(&transfer_id) else {
        return PendingSftpCancellation::Missing;
    };
    if pending.cancelled {
        return PendingSftpCancellation::AlreadyRequested;
    }
    pending.cancelled = true;
    if let Some(cancellation) = pending.cancellation.take() {
        let _ = cancellation.send(());
    }
    PendingSftpCancellation::Requested
}

fn sftp_transfer_limit_reached(active_downloads: usize, pending_openings: usize) -> bool {
    active_downloads.saturating_add(pending_openings) >= MAX_SFTP_TRANSFERS
}

async fn shutdown_pending_sftp_opens(
    pending_openings: &mut JoinSet<Result<SftpChannelStream>>,
    pending_by_transfer: &mut HashMap<Uuid, PendingSftpOpen>,
    session_id: Uuid,
) {
    for pending in pending_by_transfer.values_mut() {
        pending.cancelled = true;
        if let Some(cancellation) = pending.cancellation.take() {
            let _ = cancellation.send(());
        }
    }

    let drain = async {
        while let Some(result) = pending_openings.join_next().await {
            match result {
                Ok(Ok(_stream)) => {}
                Ok(Err(_)) => {}
                Err(error) if error.is_cancelled() => {}
                Err(error) => {
                    warn!(%session_id, %error, "SFTP subsystem opening task failed during shutdown");
                }
            }
        }
    };
    if timeout(SFTP_OPEN_SHUTDOWN_TIMEOUT, drain).await.is_err() {
        warn!(session_id = %session_id, "SFTP subsystem opening shutdown timed out");
        pending_openings.abort_all();
        while pending_openings.join_next().await.is_some() {}
    }
    pending_by_transfer.clear();
}

async fn reap_finished_sftp_transfers(
    transfers: &mut Vec<SftpDownloadHandle>,
    event_tx: &mpsc::Sender<SshSessionEvent>,
    session_id: Uuid,
) {
    let mut index = 0;
    while index < transfers.len() {
        if !transfers[index].is_finished() {
            index += 1;
            continue;
        }
        let transfer = transfers.swap_remove(index);
        let transfer_id = transfer.transfer_id();
        if let Err(error) = transfer.shutdown().await {
            warn!(%session_id, %transfer_id, %error, "SFTP transfer task stopped unexpectedly");
            send_sftp_transfer_event(
                event_tx,
                SftpTransferEvent::Failed {
                    transfer_id,
                    message: bounded_error_message(&error),
                },
                session_id,
            )
            .await;
        }
    }
}

async fn shutdown_sftp_transfers(transfers: Vec<SftpDownloadHandle>, session_id: Uuid) {
    let mut shutdowns = JoinSet::new();
    for transfer in transfers {
        shutdowns.spawn(async move {
            let transfer_id = transfer.transfer_id();
            (transfer_id, transfer.shutdown().await)
        });
    }
    while let Some(result) = shutdowns.join_next().await {
        match result {
            Ok((_, Ok(()))) => {}
            Ok((transfer_id, Err(error))) => {
                warn!(%session_id, %transfer_id, %error, "failed to shut down SFTP transfer");
            }
            Err(error) if error.is_cancelled() => {}
            Err(error) => warn!(%session_id, %error, "SFTP transfer shutdown task failed"),
        }
    }
}

pub(super) async fn receive_sftp_event(
    events: &mut Option<mpsc::Receiver<SftpBrowserEvent>>,
) -> Option<SftpBrowserEvent> {
    match events {
        Some(events) => events.recv().await,
        None => None,
    }
}

pub(super) async fn send_sftp_event(
    event_tx: &mpsc::Sender<SshSessionEvent>,
    event: SftpBrowserEvent,
    session_id: Uuid,
) -> bool {
    send_event(event_tx, SshSessionEvent::Sftp(event), session_id).await
}

pub(super) async fn send_sftp_transfer_event(
    event_tx: &mpsc::Sender<SshSessionEvent>,
    event: SftpTransferEvent,
    session_id: Uuid,
) -> bool {
    send_event(event_tx, SshSessionEvent::SftpTransfer(event), session_id).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[test]
    fn sftp_transfer_limit_counts_pending_subsystem_openings() {
        assert!(!sftp_transfer_limit_reached(0, 0));
        assert!(!sftp_transfer_limit_reached(1, 0));
        assert!(sftp_transfer_limit_reached(1, 1));
        assert!(sftp_transfer_limit_reached(0, MAX_SFTP_TRANSFERS));
    }

    #[tokio::test]
    async fn pending_sftp_open_can_be_cancelled_before_a_transfer_starts() {
        let transfer_id = Uuid::new_v4();
        let cancelled = Arc::new(AtomicBool::new(false));
        let observed = cancelled.clone();
        let (cancellation, cancellation_rx) = oneshot::channel();
        let mut openings = JoinSet::<Result<SftpChannelStream>>::new();
        let task = openings.spawn(async move {
            let _ = cancellation_rx.await;
            observed.store(true, Ordering::Release);
            Err(anyhow::anyhow!("cancelled test opening"))
        });
        let mut pending = HashMap::from([(
            transfer_id,
            PendingSftpOpen {
                task_id: task.id(),
                cancellation: Some(cancellation),
                cancelled: false,
                request: SftpDownloadRequest::new(transfer_id, "/srv/file.txt".to_owned())
                    .expect("test request should validate"),
            },
        )]);

        assert_eq!(
            cancel_pending_sftp_open(&mut pending, transfer_id),
            PendingSftpCancellation::Requested
        );
        assert_eq!(
            cancel_pending_sftp_open(&mut pending, transfer_id),
            PendingSftpCancellation::AlreadyRequested
        );
        let completed = openings
            .join_next_with_id()
            .await
            .expect("cancelled opening should complete")
            .expect("cancelled opening task should join");
        let (_, pending_open) = take_pending_sftp_open(&mut pending, completed.0)
            .expect("completed opening should remain tracked until joined");

        assert!(pending_open.cancelled);
        assert!(completed.1.is_err());
        assert!(cancelled.load(Ordering::Acquire));
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn sftp_tab_shutdown_cancels_and_joins_pending_openings() {
        let transfer_id = Uuid::new_v4();
        let cancelled = Arc::new(AtomicBool::new(false));
        let observed = cancelled.clone();
        let (cancellation, cancellation_rx) = oneshot::channel();
        let mut openings = JoinSet::<Result<SftpChannelStream>>::new();
        let task = openings.spawn(async move {
            let _ = cancellation_rx.await;
            observed.store(true, Ordering::Release);
            Err(anyhow::anyhow!("cancelled by tab shutdown"))
        });
        let mut pending = HashMap::from([(
            transfer_id,
            PendingSftpOpen {
                task_id: task.id(),
                cancellation: Some(cancellation),
                cancelled: false,
                request: SftpDownloadRequest::new(transfer_id, "/srv/file.txt".to_owned())
                    .expect("test request should validate"),
            },
        )]);

        shutdown_pending_sftp_opens(&mut openings, &mut pending, Uuid::new_v4()).await;

        assert!(openings.is_empty());
        assert!(pending.is_empty());
        assert!(cancelled.load(Ordering::Acquire));
    }
}
