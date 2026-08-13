use std::collections::{HashMap, VecDeque};

use russh::{ChannelStream, client};
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio::time::timeout;

use crate::sftp::{
    MAX_RECURSIVE_DOWNLOAD_FILES, SFTP_TRANSFER_EVENT_CAPACITY, SftpBrowserHandle,
    SftpDownloadHandle, SftpDownloadRequest, SftpDownloadRoot, SftpUploadHandle, SftpWriteEvent,
    discover_download_requests, execute_sftp_write,
};

use super::*;

struct PendingSftpOpen {
    task_id: tokio::task::Id,
    cancellation: Option<oneshot::Sender<()>>,
    cancelled: bool,
    request: SftpDownloadRequest,
}

struct PendingDiscovery {
    task_id: tokio::task::Id,
    cancellation: Option<oneshot::Sender<()>>,
    cancelled: bool,
    root: SftpDownloadRoot,
}

type SftpChannelStream = ChannelStream<client::Msg>;

enum ActiveSftpTransfer {
    Download(SftpDownloadHandle),
    Upload(SftpUploadHandle),
}

impl ActiveSftpTransfer {
    fn transfer_id(&self) -> Uuid {
        match self {
            Self::Download(transfer) => transfer.transfer_id(),
            Self::Upload(transfer) => transfer.transfer_id(),
        }
    }

    fn is_finished(&self) -> bool {
        match self {
            Self::Download(transfer) => transfer.is_finished(),
            Self::Upload(transfer) => transfer.is_finished(),
        }
    }

    fn cancel(&self) {
        match self {
            Self::Download(transfer) => transfer.cancel(),
            Self::Upload(transfer) => transfer.cancel(),
        }
    }

    fn pause(&self) {
        match self {
            Self::Download(transfer) => transfer.pause(),
            Self::Upload(transfer) => transfer.pause(),
        }
    }

    fn resume(&self) {
        match self {
            Self::Download(transfer) => transfer.resume(),
            Self::Upload(transfer) => transfer.resume(),
        }
    }

    async fn shutdown(self) -> Result<()> {
        match self {
            Self::Download(transfer) => transfer.shutdown().await,
            Self::Upload(transfer) => transfer.shutdown().await,
        }
    }
}

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
    let mut transfers = Vec::<ActiveSftpTransfer>::new();
    let mut pending_openings = JoinSet::<Result<SftpChannelStream>>::new();
    let mut pending_by_transfer = HashMap::<Uuid, PendingSftpOpen>::new();
    let mut queued_requests = VecDeque::<SftpDownloadRequest>::new();
    let mut discoveries = JoinSet::<Result<Vec<SftpDownloadRequest>>>::new();
    let mut discovery_by_transfer = HashMap::<Uuid, PendingDiscovery>::new();
    loop {
        tokio::select! {
            discovery = discoveries.join_next_with_id(), if !discoveries.is_empty() => {
                let Some(discovery) = discovery else {
                    continue;
                };
                let (task_id, result) = match discovery {
                    Ok((task_id, result)) => (task_id, Ok(result)),
                    Err(error) => (error.id(), Err(error)),
                };
                let Some((transfer_id, discovery)) = take_pending_discovery(&mut discovery_by_transfer, task_id) else {
                    continue;
                };
                if discovery.cancelled {
                    continue;
                }
                match result {
                    Ok(Ok(requests)) => {
                        if requests.len() > available_sftp_transfer_slots(
                            &transfers,
                            &pending_by_transfer,
                            &queued_requests,
                            &discovery_by_transfer,
                        ) {
                            let _ = send_sftp_transfer_event(
                                &event_tx,
                                SftpTransferEvent::DiscoveryFailed {
                                    transfer_id,
                                    name: discovery.root.name().to_owned(),
                                    message: format!(
                                        "SFTP transfer queue is full; it can contain at most {MAX_RECURSIVE_DOWNLOAD_FILES} files"
                                    ),
                                },
                                session_id,
                            )
                            .await;
                        } else {
                            for request in requests {
                                let event = SftpTransferEvent::Queued {
                                    transfer_id: request.transfer_id(),
                                    remote_path: request.remote_path().to_owned(),
                                    name: request.name().to_owned(),
                                    total_bytes: request.total_bytes(),
                                };
                                if !send_sftp_transfer_event(&event_tx, event, session_id).await {
                                    break;
                                }
                                queued_requests.push_back(request);
                            }
                        }
                    }
                    Ok(Err(error)) => {
                        let _ = send_sftp_transfer_event(
                            &event_tx,
                            SftpTransferEvent::DiscoveryFailed {
                                transfer_id,
                                name: discovery.root.name().to_owned(),
                                message: bounded_error_message(&error),
                            },
                            session_id,
                        ).await;
                    }
                    Err(error) => {
                        let _ = send_sftp_transfer_event(
                            &event_tx,
                            SftpTransferEvent::DiscoveryFailed {
                                transfer_id,
                                name: discovery.root.name().to_owned(),
                                message: bounded_error_message(&anyhow::Error::from(error)),
                            },
                            session_id,
                        ).await;
                    }
                }
                start_queued_sftp_transfers(
                    &mut queued_requests,
                    &mut pending_openings,
                    &mut pending_by_transfer,
                    &connection,
                    transfers.len(),
                );
            }
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
                        transfers.push(ActiveSftpTransfer::Download(SftpDownloadHandle::spawn(
                            &Handle::current(),
                            stream,
                            pending.request,
                            transfer_event_tx.clone(),
                        )));
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
                start_queued_sftp_transfers(
                    &mut queued_requests,
                    &mut pending_openings,
                    &mut pending_by_transfer,
                    &connection,
                    transfers.len(),
                );
            }
            command = command_rx.recv() => {
                reap_finished_sftp_transfers(&mut transfers, &event_tx, session_id).await;
                let result = match command {
                    Some(SshCommand::OpenSftp { path })
                    | Some(SshCommand::ListSftp { path }) => browser.request_list(path),
                    Some(SshCommand::LoadMoreSftp) => browser.request_load_more(),
                    Some(SshCommand::OpenSftpFile { root }) => {
                        let transfer_id = root.transfer_id();
                        let already_active = transfers.iter().any(|transfer| transfer.transfer_id() == transfer_id)
                            || pending_by_transfer.contains_key(&transfer_id)
                            || discovery_by_transfer.contains_key(&transfer_id)
                            || queued_requests.iter().any(|request| request.transfer_id() == transfer_id);
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
                        } else {
                            spawn_pending_sftp_discovery(
                                &mut discoveries,
                                &mut discovery_by_transfer,
                                &connection,
                                root,
                            );
                            Ok(())
                        }
                    }
                    Some(SshCommand::OpenSftpUpload { request }) => {
                        let transfer_id = request.transfer_id();
                        let already_active = transfers.iter().any(|transfer| transfer.transfer_id() == transfer_id)
                            || pending_by_transfer.contains_key(&transfer_id)
                            || discovery_by_transfer.contains_key(&transfer_id)
                            || queued_requests.iter().any(|queued| queued.transfer_id() == transfer_id);
                        if already_active {
                            send_sftp_transfer_event(
                                &event_tx,
                                SftpTransferEvent::Failed {
                                    transfer_id,
                                    message: "SFTP transfer is already active".to_owned(),
                                },
                                session_id,
                            ).await;
                        } else if transfers.len() >= MAX_SFTP_TRANSFERS {
                            send_sftp_transfer_event(
                                &event_tx,
                                SftpTransferEvent::Failed {
                                    transfer_id,
                                    message: "SFTP transfer queue is full".to_owned(),
                                },
                                session_id,
                            ).await;
                        } else {
                            let stream = connection.open_sftp_stream().await;
                            match stream {
                                Ok(stream) => {
                                    send_sftp_transfer_event(
                                        &event_tx,
                                        SftpTransferEvent::Queued {
                                            transfer_id,
                                            remote_path: String::new(),
                                            name: request.name().to_owned(),
                                            total_bytes: request.total_bytes(),
                                        },
                                        session_id,
                                    ).await;
                                    transfers.push(ActiveSftpTransfer::Upload(
                                        SftpUploadHandle::spawn(
                                            &Handle::current(),
                                            stream,
                                            request,
                                            transfer_event_tx.clone(),
                                        ),
                                    ));
                                }
                                Err(error) => {
                                    send_sftp_transfer_event(
                                        &event_tx,
                                        SftpTransferEvent::Failed {
                                            transfer_id,
                                            message: bounded_error_message(&error),
                                        },
                                        session_id,
                                    ).await;
                                }
                            }
                        }
                        Ok(())
                    }
                    Some(SshCommand::CancelSftpTransfer { transfer_id }) => {
                        if let Some(transfer) = transfers
                            .iter()
                            .find(|transfer| transfer.transfer_id() == transfer_id)
                        {
                            transfer.cancel();
                            Ok(())
                        } else if cancel_pending_discovery(&mut discovery_by_transfer, transfer_id) {
                            send_sftp_transfer_event(
                                &event_tx,
                                SftpTransferEvent::Cancelled { transfer_id },
                                session_id,
                            ).await;
                            Ok(())
                        } else if let Some(index) = queued_requests
                            .iter()
                            .position(|request| request.transfer_id() == transfer_id)
                        {
                            queued_requests.remove(index);
                            send_sftp_transfer_event(
                                &event_tx,
                                SftpTransferEvent::Cancelled { transfer_id },
                                session_id,
                            ).await;
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
                    Some(SshCommand::PauseSftpTransfer { transfer_id }) => {
                        if let Some(transfer) = transfers
                            .iter()
                            .find(|transfer| transfer.transfer_id() == transfer_id)
                        {
                            transfer.pause();
                        }
                        Ok(())
                    }
                    Some(SshCommand::ResumeSftpTransfer { transfer_id }) => {
                        if let Some(transfer) = transfers
                            .iter()
                            .find(|transfer| transfer.transfer_id() == transfer_id)
                        {
                            transfer.resume();
                        }
                        Ok(())
                    }
                    Some(SshCommand::SftpWrite { operation_id, operation }) => {
                        let result = match connection.open_sftp_stream().await {
                            Ok(stream) => execute_sftp_write(stream, operation).await,
                            Err(error) => Err(error).context("cannot open SFTP write subsystem"),
                        };
                        match result {
                            Ok(crate::sftp::SftpWriteResult::Completed { path }) => {
                                send_event(
                                    &event_tx,
                                    SshSessionEvent::SftpWrite(SftpWriteEvent::Completed {
                                        operation_id,
                                        path,
                                    }),
                                    session_id,
                                )
                                .await;
                            }
                            Ok(crate::sftp::SftpWriteResult::Updated {
                                path,
                                size,
                                modified,
                            }) => {
                                send_event(
                                    &event_tx,
                                    SshSessionEvent::SftpWrite(SftpWriteEvent::Updated {
                                        operation_id,
                                        path,
                                        size,
                                        modified,
                                    }),
                                    session_id,
                                )
                                .await;
                            }
                            Ok(crate::sftp::SftpWriteResult::Text {
                                path,
                                data,
                                expected_size,
                                expected_modified,
                            }) => {
                                send_event(
                                    &event_tx,
                                    SshSessionEvent::SftpWrite(SftpWriteEvent::Text {
                                        operation_id,
                                        path,
                                        data,
                                        expected_size,
                                        expected_modified,
                                    }),
                                    session_id,
                                )
                                .await;
                            }
                            Ok(crate::sftp::SftpWriteResult::Metadata { path, size, modified }) => {
                                send_event(
                                    &event_tx,
                                    SshSessionEvent::SftpWrite(SftpWriteEvent::Metadata {
                                        operation_id,
                                        path,
                                        size,
                                        modified,
                                    }),
                                    session_id,
                                )
                                .await;
                            }
                            Err(error) => {
                                send_event(
                                    &event_tx,
                                    SshSessionEvent::SftpWrite(SftpWriteEvent::Failed {
                                        operation_id,
                                        message: bounded_error_message(&error),
                                    }),
                                    session_id,
                                )
                                .await;
                            }
                        }
                        Ok(())
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
                    SftpTransferEvent::Queued { .. }
                    | SftpTransferEvent::Started { .. }
                    | SftpTransferEvent::Progress { .. }
                    | SftpTransferEvent::Paused { .. }
                    | SftpTransferEvent::Resumed { .. }
                    | SftpTransferEvent::DiscoveryFailed { .. } => None,
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
                    start_queued_sftp_transfers(
                        &mut queued_requests,
                        &mut pending_openings,
                        &mut pending_by_transfer,
                        &connection,
                        transfers.len(),
                    );
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

    shutdown_pending_sftp_discoveries(&mut discoveries, &mut discovery_by_transfer, session_id)
        .await;
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

fn spawn_pending_sftp_discovery(
    discoveries: &mut JoinSet<Result<Vec<SftpDownloadRequest>>>,
    discovery_by_transfer: &mut HashMap<Uuid, PendingDiscovery>,
    connection: &SshConnection,
    root: SftpDownloadRoot,
) {
    let transfer_id = root.transfer_id();
    let (cancellation, cancelled) = oneshot::channel();
    let connection = connection.clone();
    let discovery_root = root.clone();
    let task = discoveries.spawn(async move {
        tokio::select! {
            result = async {
                let stream = connection
                    .open_sftp_stream()
                    .await
                    .context("cannot open SFTP subsystem for recursive discovery")?;
                discover_download_requests(stream, discovery_root).await
            } => result,
            _ = cancelled => Err(anyhow::anyhow!("SFTP recursive discovery cancelled")),
        }
    });
    discovery_by_transfer.insert(
        transfer_id,
        PendingDiscovery {
            task_id: task.id(),
            cancellation: Some(cancellation),
            cancelled: false,
            root,
        },
    );
}

fn take_pending_discovery(
    discoveries: &mut HashMap<Uuid, PendingDiscovery>,
    task_id: tokio::task::Id,
) -> Option<(Uuid, PendingDiscovery)> {
    let transfer_id = discoveries.iter().find_map(|(transfer_id, discovery)| {
        (discovery.task_id == task_id).then_some(*transfer_id)
    })?;
    discoveries
        .remove(&transfer_id)
        .map(|discovery| (transfer_id, discovery))
}

fn cancel_pending_discovery(
    discovery_by_transfer: &mut HashMap<Uuid, PendingDiscovery>,
    transfer_id: Uuid,
) -> bool {
    let Some(discovery) = discovery_by_transfer.get_mut(&transfer_id) else {
        return false;
    };
    if discovery.cancelled {
        return true;
    }
    discovery.cancelled = true;
    if let Some(cancellation) = discovery.cancellation.take() {
        let _ = cancellation.send(());
    }
    true
}

fn start_queued_sftp_transfers(
    queued_requests: &mut VecDeque<SftpDownloadRequest>,
    pending_openings: &mut JoinSet<Result<SftpChannelStream>>,
    pending_by_transfer: &mut HashMap<Uuid, PendingSftpOpen>,
    connection: &SshConnection,
    active_downloads: usize,
) {
    while !sftp_transfer_limit_reached(active_downloads, pending_by_transfer.len()) {
        let Some(request) = queued_requests.pop_front() else {
            break;
        };
        spawn_pending_sftp_open(pending_openings, pending_by_transfer, connection, request);
    }
}

async fn shutdown_pending_sftp_discoveries(
    discoveries: &mut JoinSet<Result<Vec<SftpDownloadRequest>>>,
    discovery_by_transfer: &mut HashMap<Uuid, PendingDiscovery>,
    session_id: Uuid,
) {
    for discovery in discovery_by_transfer.values_mut() {
        discovery.cancelled = true;
        if let Some(cancellation) = discovery.cancellation.take() {
            let _ = cancellation.send(());
        }
    }

    let drain = async {
        while let Some(result) = discoveries.join_next().await {
            if let Err(error) = result
                && !error.is_cancelled()
            {
                warn!(%session_id, %error, "SFTP recursive discovery task failed during shutdown");
            }
        }
    };
    if timeout(SFTP_OPEN_SHUTDOWN_TIMEOUT, drain).await.is_err() {
        warn!(%session_id, "SFTP recursive discovery shutdown timed out");
        discoveries.abort_all();
        while discoveries.join_next().await.is_some() {}
    }
    discovery_by_transfer.clear();
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

fn available_sftp_transfer_slots(
    transfers: &[ActiveSftpTransfer],
    pending_by_transfer: &HashMap<Uuid, PendingSftpOpen>,
    queued_requests: &VecDeque<SftpDownloadRequest>,
    discovery_by_transfer: &HashMap<Uuid, PendingDiscovery>,
) -> usize {
    MAX_RECURSIVE_DOWNLOAD_FILES.saturating_sub(
        transfers
            .len()
            .saturating_add(pending_by_transfer.len())
            .saturating_add(queued_requests.len())
            .saturating_add(discovery_by_transfer.len()),
    )
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
    transfers: &mut Vec<ActiveSftpTransfer>,
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

async fn shutdown_sftp_transfers(transfers: Vec<ActiveSftpTransfer>, session_id: Uuid) {
    let mut shutdowns = JoinSet::new();
    for transfer in transfers {
        shutdowns.spawn(async move {
            let transfer_id = transfer.transfer_id();
            let result = match transfer {
                ActiveSftpTransfer::Download(transfer) => transfer.shutdown().await,
                ActiveSftpTransfer::Upload(transfer) => transfer.shutdown().await,
            };
            (transfer_id, result)
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
    async fn recursive_discoveries_reserve_global_transfer_slots() {
        let mut openings = JoinSet::<Result<SftpChannelStream>>::new();
        let task = openings.spawn(std::future::pending());
        let pending = HashMap::from([(
            Uuid::new_v4(),
            PendingSftpOpen {
                task_id: task.id(),
                cancellation: None,
                cancelled: false,
                request: SftpDownloadRequest::new(Uuid::new_v4(), "/srv/file.txt".to_owned())
                    .expect("test request should validate"),
            },
        )]);
        let queued = VecDeque::from([
            SftpDownloadRequest::new(Uuid::new_v4(), "/srv/one.txt".to_owned())
                .expect("test request should validate"),
            SftpDownloadRequest::new(Uuid::new_v4(), "/srv/two.txt".to_owned())
                .expect("test request should validate"),
        ]);

        assert_eq!(
            available_sftp_transfer_slots(&[], &pending, &queued, &HashMap::new()),
            MAX_RECURSIVE_DOWNLOAD_FILES - 3
        );
        openings.abort_all();
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
