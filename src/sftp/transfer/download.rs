//! SFTP download operations owned by the transfer worker.

use super::*;

pub(super) async fn download_to_local<S>(
    stream: S,
    request: &SftpDownloadRequest,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
) -> Result<(PathBuf, u64)>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let deadline = Instant::now() + DOWNLOAD_TIMEOUT;
    let session = RawSftpSession::new_with_config(
        PacketLimitedStream::new(stream),
        Config {
            max_packet_len: MAX_PACKET_BYTES,
            max_concurrent_reads: 16,
            max_concurrent_writes: 1,
            max_write_packet_len: MAX_PACKET_BYTES,
            request_timeout_secs: REQUEST_TIMEOUT.as_secs(),
        },
    );
    await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "SFTP transfer handshake",
        session.init(),
    )
    .await?
    .context("SFTP transfer handshake failed")?;

    let result =
        download_initialized_local(&session, request, cancellation, event_tx, deadline).await;
    if let Err(error) = session.close_session() {
        debug!(%error, "failed to close local SFTP download session");
    }
    result
}

pub(super) async fn download_initialized_local(
    session: &RawSftpSession,
    request: &SftpDownloadRequest,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
    deadline: Instant,
) -> Result<(PathBuf, u64)> {
    let initial = lstat_regular_file(session, &request.remote_path, cancellation, deadline).await?;
    if initial.size != request.total_bytes {
        anyhow::bail!("remote file size changed since the download was queued");
    }
    let handle = await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "opening remote file",
        session.open(
            request.remote_path.clone(),
            OpenFlags::READ,
            FileAttributes::empty(),
        ),
    )
    .await?
    .with_context(|| format!("cannot open remote file {:?}", request.remote_path))?
    .handle;

    let result = download_open_local_handle(
        session,
        &handle,
        request,
        initial,
        cancellation,
        event_tx,
        deadline,
    )
    .await;
    close_remote_handle(session, handle).await;
    result
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn download_open_local_handle(
    session: &RawSftpSession,
    handle: &str,
    request: &SftpDownloadRequest,
    initial: RemoteFileMetadata,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
    deadline: Instant,
) -> Result<(PathBuf, u64)> {
    let opened = await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "validating opened remote file",
        session.fstat(handle.to_owned()),
    )
    .await?
    .context("cannot inspect opened remote file")?;
    ensure_same_remote_file(initial, validate_regular_metadata(&opened.attrs)?)?;
    let reopened =
        lstat_regular_file(session, &request.remote_path, cancellation, deadline).await?;
    ensure_same_remote_file(initial, reopened)?;

    send_transfer_state(
        event_tx,
        SftpTransferEvent::Started {
            transfer_id: request.transfer_id,
            remote_path: request.remote_path.clone(),
            name: request.name.clone(),
            total_bytes: initial.size,
        },
    )
    .await?;

    let target = request
        .local_target
        .clone()
        .context("local SFTP download is missing its target")?;
    let pending = prepare_local_download(target, cancellation, deadline).await?;
    let (chunk_tx, chunk_rx) = mpsc::channel(WRITER_QUEUE_CAPACITY);
    let writer_cancel = cancellation.clone();
    let expected_bytes = initial.size;
    let mut writer = tokio::task::spawn_blocking(move || {
        write_local_file(pending, chunk_rx, expected_bytes, &writer_cancel)
    });

    let stream_result = stream_remote_file(
        session,
        handle,
        &request.remote_path,
        request.transfer_id,
        initial,
        cancellation,
        event_tx,
        &chunk_tx,
        deadline,
    )
    .await;
    if stream_result.is_err() {
        // A full byte count is insufficient: EOF and metadata validation must
        // also succeed before the writer is allowed to publish its part file.
        cancellation.cancel();
    }
    drop(chunk_tx);

    match stream_result {
        Ok(()) => {
            let local_path =
                await_local_writer_success(&mut writer, cancellation, deadline).await?;
            Ok((local_path, initial.size))
        }
        Err(stream_error) => {
            let writer_error = await_local_writer_cleanup(&mut writer).await;
            if is_cancelled(&stream_error) {
                return Err(stream_error);
            }
            match writer_error {
                Some(error) if !is_incomplete_local_write(&error) && !is_cancelled(&error) => {
                    Err(error)
                }
                _ => Err(stream_error),
            }
        }
    }
}

pub(super) async fn download_to_cache<S>(
    stream: S,
    request: &SftpDownloadRequest,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
    cache_dir: &Path,
) -> Result<(PathBuf, u64)>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let deadline = Instant::now() + DOWNLOAD_TIMEOUT;
    let session = RawSftpSession::new_with_config(
        PacketLimitedStream::new(stream),
        Config {
            max_packet_len: MAX_PACKET_BYTES,
            max_concurrent_reads: 16,
            max_concurrent_writes: 1,
            max_write_packet_len: MAX_PACKET_BYTES,
            request_timeout_secs: REQUEST_TIMEOUT.as_secs(),
        },
    );
    await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "SFTP transfer handshake",
        session.init(),
    )
    .await?
    .context("SFTP transfer handshake failed")?;

    let result = download_initialized(
        &session,
        request,
        cancellation,
        event_tx,
        cache_dir,
        deadline,
    )
    .await;
    if let Err(error) = session.close_session() {
        debug!(%error, "failed to close SFTP transfer session");
    }
    result
}

pub(super) async fn download_initialized(
    session: &RawSftpSession,
    request: &SftpDownloadRequest,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
    cache_dir: &Path,
    deadline: Instant,
) -> Result<(PathBuf, u64)> {
    let initial = lstat_regular_file(session, &request.remote_path, cancellation, deadline).await?;
    if initial.size > MAX_CACHE_OPEN_BYTES {
        anyhow::bail!(
            "remote file exceeds the private-cache open limit of {MAX_CACHE_OPEN_BYTES} bytes"
        );
    }
    let handle = await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "opening remote file",
        session.open(
            request.remote_path.clone(),
            OpenFlags::READ,
            FileAttributes::empty(),
        ),
    )
    .await?
    .with_context(|| format!("cannot open remote file {:?}", request.remote_path))?
    .handle;

    let result = download_open_handle(
        session,
        &handle,
        request,
        initial,
        cancellation,
        event_tx,
        cache_dir,
        deadline,
    )
    .await;
    close_remote_handle(session, handle).await;
    result
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn download_open_handle(
    session: &RawSftpSession,
    handle: &str,
    request: &SftpDownloadRequest,
    initial: RemoteFileMetadata,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
    cache_dir: &Path,
    deadline: Instant,
) -> Result<(PathBuf, u64)> {
    let opened = await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "validating opened remote file",
        session.fstat(handle.to_owned()),
    )
    .await?
    .context("cannot inspect opened remote file")?;
    let opened = validate_regular_metadata(&opened.attrs)?;
    ensure_same_remote_file(initial, opened)?;

    // Recheck the path after opening so a path replaced with a symlink is
    // rejected even if the already-open handle still points at a regular file.
    let reopened =
        lstat_regular_file(session, &request.remote_path, cancellation, deadline).await?;
    ensure_same_remote_file(initial, reopened)?;

    await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "announcing SFTP transfer",
        event_tx.send(SftpTransferEvent::Started {
            transfer_id: request.transfer_id,
            remote_path: request.remote_path.clone(),
            name: request.name.clone(),
            total_bytes: initial.size,
        }),
    )
    .await?
    .context("SFTP transfer event receiver dropped")?;

    let pending = prepare_cache_file(
        cache_dir.to_owned(),
        request.name.clone(),
        initial.size,
        cancellation,
        deadline,
    )
    .await?;
    let (chunk_tx, chunk_rx) = mpsc::channel(WRITER_QUEUE_CAPACITY);
    let writer_cancel = cancellation.clone();
    let expected_bytes = initial.size;
    let mut writer = tokio::task::spawn_blocking(move || {
        write_cache_file(pending, chunk_rx, expected_bytes, &writer_cancel)
    });

    let stream_result = stream_remote_file(
        session,
        handle,
        &request.remote_path,
        request.transfer_id,
        initial,
        cancellation,
        event_tx,
        &chunk_tx,
        deadline,
    )
    .await;
    if stream_result.is_err() {
        cancellation.cancel();
    }
    drop(chunk_tx);

    match stream_result {
        Ok(()) => {
            let local_path = await_writer_success(&mut writer, cancellation, deadline).await?;
            Ok((local_path, initial.size))
        }
        Err(stream_error) => {
            let writer_error = await_writer_cleanup(&mut writer).await;
            if is_cancelled(&stream_error) {
                return Err(stream_error);
            }
            match writer_error {
                Some(error) if !is_incomplete_cache_write(&error) && !is_cancelled(&error) => {
                    Err(error)
                }
                _ => Err(stream_error),
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn stream_remote_file(
    session: &RawSftpSession,
    handle: &str,
    remote_path: &str,
    transfer_id: Uuid,
    expected: RemoteFileMetadata,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
    chunk_tx: &mpsc::Sender<Vec<u8>>,
    deadline: Instant,
) -> Result<()> {
    let mut downloaded_bytes = 0_u64;
    let mut next_offset = 0_u64;
    let mut requests = RequestWindow::new();
    let mut last_progress = Instant::now();
    while downloaded_bytes < expected.size {
        if cancellation.is_paused() {
            send_transfer_state(
                event_tx,
                SftpTransferEvent::Paused {
                    transfer_id,
                    downloaded_bytes,
                    total_bytes: expected.size,
                },
            )
            .await?;
            cancellation.wait_until_running().await?;
            send_transfer_state(
                event_tx,
                SftpTransferEvent::Resumed {
                    transfer_id,
                    downloaded_bytes,
                    total_bytes: expected.size,
                },
            )
            .await?;
        }
        while requests.len() < REQUEST_WINDOW && next_offset < expected.size {
            let length = (expected.size - next_offset).min(u64::from(DOWNLOAD_CHUNK_BYTES)) as u32;
            requests
                .push(read_remote_chunk(
                    session,
                    handle,
                    next_offset,
                    length,
                    expected.size,
                    cancellation,
                    deadline,
                ))
                .await?;
            next_offset += u64::from(length);
        }
        let data = requests
            .next()
            .await
            .context("SFTP read window ended unexpectedly")??;
        let chunk_bytes = data.len() as u64;
        await_step(
            cancellation,
            deadline,
            REQUEST_TIMEOUT,
            "writing local download",
            chunk_tx.send(data),
        )
        .await?
        .map_err(|_| CacheWriterStopped)?;
        downloaded_bytes += chunk_bytes;

        if last_progress.elapsed() >= PROGRESS_INTERVAL || downloaded_bytes == expected.size {
            send_progress(
                event_tx,
                SftpTransferEvent::Progress {
                    transfer_id,
                    downloaded_bytes,
                    total_bytes: expected.size,
                },
            )?;
            last_progress = Instant::now();
        }
    }

    // Reading one byte beyond the validated size catches growth even when a
    // server's later metadata response is stale.
    let eof = await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "confirming remote end of file",
        session.read(handle.to_owned(), expected.size, 1),
    )
    .await?;
    match eof {
        Err(russh_sftp::client::error::Error::Status(status))
            if status.status_code == StatusCode::Eof => {}
        Ok(data) if data.data.is_empty() => {}
        Ok(_) => anyhow::bail!("remote file grew while it was being downloaded"),
        Err(error) => return Err(error).context("cannot confirm remote end of file"),
    }

    let final_handle = await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "revalidating downloaded remote file",
        session.fstat(handle.to_owned()),
    )
    .await?
    .context("cannot revalidate downloaded remote file")?;
    ensure_same_remote_file(expected, validate_regular_metadata(&final_handle.attrs)?)?;
    let final_path = lstat_regular_file(session, remote_path, cancellation, deadline).await?;
    ensure_same_remote_file(expected, final_path)?;
    Ok(())
}

pub(super) async fn read_remote_chunk(
    session: &RawSftpSession,
    handle: &str,
    offset: u64,
    length: u32,
    total_bytes: u64,
    cancellation: &TransferCancellation,
    deadline: Instant,
) -> Result<Vec<u8>> {
    cancellation.limit(u64::from(length)).await?;
    let mut chunk = Vec::new();
    while chunk.len() < length as usize {
        let remaining = length - chunk.len() as u32;
        let response = await_step(
            cancellation,
            deadline,
            REQUEST_TIMEOUT,
            "reading remote file",
            session.read(handle.to_owned(), offset + chunk.len() as u64, remaining),
        )
        .await?;
        let data = match response {
            Ok(data) => data.data,
            Err(russh_sftp::client::error::Error::Status(status))
                if status.status_code == StatusCode::Eof =>
            {
                anyhow::bail!(
                    "remote file ended after {} of {total_bytes} bytes",
                    offset + chunk.len() as u64
                );
            }
            Err(error) => return Err(error).context("cannot read complete remote file chunk"),
        };
        if data.is_empty() {
            anyhow::bail!("remote file returned an empty data packet before its validated end");
        }
        if data.len() > remaining as usize {
            anyhow::bail!("remote file returned more bytes than requested");
        }
        // Short replies leave a gap in this slot, never in another slot's
        // offset range. Keep repairing it before emitting the ordered chunk.
        if chunk.is_empty() {
            // Keep full replies in their original allocation. In particular,
            // do not preallocate another chunk while its reply is pending.
            chunk = data;
        } else {
            chunk.extend_from_slice(&data);
        }
    }
    Ok(chunk)
}

pub(super) async fn lstat_regular_file(
    session: &RawSftpSession,
    path: &str,
    cancellation: &TransferCancellation,
    deadline: Instant,
) -> Result<RemoteFileMetadata> {
    let attrs = await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "inspecting remote file",
        session.lstat(path.to_owned()),
    )
    .await?
    .with_context(|| format!("cannot inspect remote file {path:?}"))?;
    validate_regular_metadata(&attrs.attrs)
}

pub(super) async fn close_remote_handle(session: &RawSftpSession, handle: String) {
    match timeout(REQUEST_TIMEOUT, session.close(handle)).await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => debug!(%error, "failed to close SFTP download handle"),
        Err(_) => debug!("timed out closing SFTP download handle"),
    }
}
