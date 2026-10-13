//! SFTP upload operations owned by the transfer worker.

use super::*;

pub(super) async fn upload_initialized<S>(
    stream: S,
    request: &SftpUploadRequest,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
) -> Result<UploadOutcome>
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
    let version = await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "SFTP upload handshake",
        session.init(),
    )
    .await?
    .context("SFTP upload handshake failed")?;
    let parent = request
        .remote_path
        .strip_suffix(&request.name)
        .unwrap_or("")
        .trim_end_matches('/');
    let temporary = format!(
        "{parent}/.{}.axssh-upload-{}",
        request.name, request.transfer_id
    );
    if let Err(error) =
        ensure_remote_upload_directories(&session, &request.remote_directories).await
    {
        close_upload_session(&session);
        return Err(error);
    }
    let existing = match inspect_remote_upload_target(&session, &request.remote_path).await {
        Ok(existing) => existing,
        Err(error) => {
            close_upload_session(&session);
            return Err(error);
        }
    };
    let target_result = match (existing, request.conflict_choice) {
        (Some(remote), None) => {
            close_upload_session(&session);
            return Ok(UploadOutcome::Conflict(remote));
        }
        (Some(_), Some(SftpUploadConflictChoice::Skip)) => {
            close_upload_session(&session);
            return Ok(UploadOutcome::Skipped);
        }
        (Some(remote), Some(SftpUploadConflictChoice::Overwrite)) => {
            if request
                .expected_remote
                .is_some_and(|expected| expected.size.is_none() && expected.modified.is_none())
            {
                Err(anyhow::anyhow!(
                    "server did not provide a remote fingerprint for overwrite"
                ))
            } else if request
                .expected_remote
                .is_some_and(|expected| expected != remote)
            {
                Err(anyhow::anyhow!(
                    "remote upload target changed while awaiting confirmation"
                ))
            } else if version
                .extensions
                .get(POSIX_RENAME_EXTENSION)
                .map(String::as_str)
                != Some("1")
            {
                Err(anyhow::anyhow!(
                    "server does not support atomic SFTP overwrite"
                ))
            } else {
                Ok((request.remote_path.clone(), true))
            }
        }
        (Some(_), Some(SftpUploadConflictChoice::KeepBoth)) => {
            find_keep_both_path(&session, &request.remote_path)
                .await
                .map(|path| (path, false))
        }
        (None, _) if request.expected_remote.is_some() => Err(anyhow::anyhow!(
            "remote upload target changed while awaiting confirmation"
        )),
        (None, _) => Ok((request.remote_path.clone(), false)),
    };
    let (target_path, overwrite) = match target_result {
        Ok(target) => target,
        Err(error) => {
            close_upload_session(&session);
            return Err(error);
        }
    };
    let handle = await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "opening remote upload file",
        session.open(
            temporary.clone(),
            OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
            FileAttributes::default(),
        ),
    )
    .await?
    .context("cannot open remote upload file")?
    .handle;
    let (mut chunks, mut reader) = start_local_upload_reader(request, cancellation);
    let mut offset = 0_u64;
    let mut next_offset = 0_u64;
    let mut last_progress = Instant::now();
    let result = async {
        let mut requests = RequestWindow::new();
        let remote = &session;
        let remote_handle = handle.as_str();
        while offset < request.total_bytes {
            cancellation.wait_until_running().await?;
            while requests.len() < REQUEST_WINDOW && next_offset < request.total_bytes {
                let chunk = await_step(
                    cancellation,
                    deadline,
                    REQUEST_TIMEOUT,
                    "reading local upload",
                    chunks.recv(),
                )
                .await?
                .context("local upload reader stopped before the validated end")??;
                let chunk_bytes = chunk.len() as u64;
                if chunk_bytes == 0
                    || chunk_bytes > u64::from(DOWNLOAD_CHUNK_BYTES)
                    || chunk_bytes > request.total_bytes - next_offset
                {
                    anyhow::bail!("local upload reader returned an invalid chunk size");
                }
                let chunk_offset = next_offset;
                requests
                    .push(async move {
                        cancellation.limit(chunk_bytes).await?;
                        await_step(
                            cancellation,
                            deadline,
                            REQUEST_TIMEOUT,
                            "writing remote upload",
                            remote.write(remote_handle.to_owned(), chunk_offset, chunk),
                        )
                        .await?
                        .context("cannot write remote upload")?;
                        Ok(chunk_bytes)
                    })
                    .await?;
                next_offset += chunk_bytes;
            }
            let chunk_bytes = requests
                .next()
                .await
                .context("SFTP write window ended unexpectedly")??;
            offset += chunk_bytes;
            if last_progress.elapsed() >= PROGRESS_INTERVAL || offset == request.total_bytes {
                send_progress(
                    event_tx,
                    SftpTransferEvent::Progress {
                        transfer_id: request.transfer_id,
                        downloaded_bytes: offset,
                        total_bytes: request.total_bytes,
                    },
                )?;
                last_progress = Instant::now();
            }
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    // Dropping the receiver releases a blocked local reader on every error.
    drop(chunks);
    let reader_result = match timeout(WRITER_CLEANUP_TIMEOUT, &mut reader).await {
        Ok(joined) => joined
            .context("local upload reader task failed")
            .and_then(|result| result),
        Err(_) => Err(anyhow::anyhow!("local upload reader cleanup timed out")),
    };
    let close_result = timeout(REQUEST_TIMEOUT, session.close(handle))
        .await
        .context("closing remote upload timed out")
        .and_then(|result| result.context("cannot close remote upload file"))
        .map(|_| ());
    let result = result.and(reader_result).and(close_result).and_then(|()| {
        if cancellation.is_cancelled() {
            Err(cancelled_error())
        } else {
            Ok(())
        }
    });
    if result.is_ok() {
        let published = async {
            let current = inspect_remote_upload_target(&session, &target_path).await?;
            if cancellation.is_cancelled() {
                return Err(cancelled_error());
            }
            if overwrite {
                if current != existing {
                    anyhow::bail!("remote upload target changed before publication");
                }
                posix_rename_upload(&session, &temporary, &target_path).await
            } else {
                if current.is_some() {
                    anyhow::bail!("remote upload target appeared before publication");
                }
                timeout(
                    REQUEST_TIMEOUT,
                    session.rename(temporary.clone(), target_path.clone()),
                )
                .await
                .context("publishing remote upload timed out")?
                .context("cannot publish remote upload")?;
                Ok(())
            }
        }
        .await;
        if let Err(error) = published {
            remove_upload_temporary(&session, &temporary).await;
            close_upload_session(&session);
            return Err(error);
        }
    } else {
        remove_upload_temporary(&session, &temporary).await;
    }
    close_upload_session(&session);
    result.map(|()| UploadOutcome::Published)
}

pub(super) fn close_upload_session(session: &RawSftpSession) {
    if let Err(error) = session.close_session() {
        tracing::warn!(%error, "failed to close upload SFTP session");
    }
}

pub(super) async fn remove_upload_temporary(session: &RawSftpSession, path: &str) {
    match timeout(REQUEST_TIMEOUT, session.remove(path.to_owned())).await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => tracing::warn!(%error, "failed to clean up remote upload temporary file"),
        Err(_) => tracing::warn!("remote upload temporary cleanup timed out"),
    }
}

pub(super) fn start_local_upload_reader(
    request: &SftpUploadRequest,
    cancellation: &TransferCancellation,
) -> (mpsc::Receiver<Result<Vec<u8>>>, JoinHandle<Result<()>>) {
    let request = request.clone();
    let cancellation = cancellation.clone();
    let (sender, receiver) = mpsc::channel(WRITER_QUEUE_CAPACITY);
    let task = tokio::task::spawn_blocking(move || {
        let result = read_local_upload_chunks(&request, &cancellation, &sender);
        if let Err(error) = &result
            && sender
                .blocking_send(Err(anyhow::anyhow!("{error:#}")))
                .is_err()
        {
            debug!("local upload reader stopped after its consumer closed");
        }
        result
    });
    (receiver, task)
}

pub(super) fn validate_local_upload_file(
    request: &SftpUploadRequest,
    file: &LocalFile,
) -> Result<()> {
    let metadata =
        fs::symlink_metadata(&request.local_path).context("cannot inspect local upload path")?;
    let opened = file
        .metadata()
        .context("cannot inspect opened local upload file")?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        anyhow::bail!("local upload source is not a regular file");
    }
    if local_upload_identity(&metadata) != *request.local_identity
        || local_upload_identity(&opened) != *request.local_identity
    {
        anyhow::bail!("local upload file changed while it was being uploaded");
    }
    Ok(())
}

pub(super) fn read_local_upload_chunks(
    request: &SftpUploadRequest,
    cancellation: &TransferCancellation,
    sender: &mpsc::Sender<Result<Vec<u8>>>,
) -> Result<()> {
    let metadata =
        fs::symlink_metadata(&request.local_path).context("cannot inspect local upload source")?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || local_upload_identity(&metadata) != *request.local_identity
    {
        anyhow::bail!("local upload file changed before it was opened");
    }
    let mut file =
        LocalFile::open(&request.local_path).context("cannot open local upload source")?;
    let mut offset = 0;
    while offset < request.total_bytes {
        if cancellation.is_cancelled() {
            return Err(cancelled_error());
        }
        validate_local_upload_file(request, &file)?;
        let length = (request.total_bytes - offset).min(u64::from(DOWNLOAD_CHUNK_BYTES)) as usize;
        let mut chunk = vec![0; length];
        file.read_exact(&mut chunk)
            .context("cannot read local upload source")?;
        sender
            .blocking_send(Ok(chunk))
            .context("local upload consumer closed")?;
        offset += length as u64;
    }
    validate_local_upload_file(request, &file)
}

pub(super) fn local_upload_identity(metadata: &fs::Metadata) -> LocalUploadIdentity {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        LocalUploadIdentity {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
    #[cfg(not(unix))]
    {
        LocalUploadIdentity {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
        }
    }
}

pub(super) async fn inspect_remote_upload_target(
    session: &RawSftpSession,
    path: &str,
) -> Result<Option<RemoteUploadFingerprint>> {
    match timeout(REQUEST_TIMEOUT, session.lstat(path.to_owned())).await {
        Ok(Ok(attrs)) if attrs.attrs.is_regular() => Ok(Some(RemoteUploadFingerprint {
            size: attrs.attrs.size,
            modified: attrs.attrs.mtime,
        })),
        Ok(Ok(_)) => anyhow::bail!("remote upload target is not a regular file"),
        Ok(Err(russh_sftp::client::error::Error::Status(status)))
            if status.status_code == StatusCode::NoSuchFile =>
        {
            Ok(None)
        }
        Ok(Err(error)) => Err(error).context("SFTP upload target check failed"),
        Err(_) => anyhow::bail!("SFTP upload target check timed out"),
    }
}

pub(super) async fn ensure_remote_upload_directories(
    session: &RawSftpSession,
    directories: &[String],
) -> Result<()> {
    for directory in directories {
        let check = timeout(REQUEST_TIMEOUT, session.lstat(directory.clone()))
            .await
            .context("SFTP upload directory check timed out")?;
        match check {
            Ok(attrs) if attrs.attrs.is_dir() => continue,
            Ok(_) => anyhow::bail!("remote upload directory is not a directory"),
            Err(russh_sftp::client::error::Error::Status(status))
                if status.status_code == StatusCode::NoSuchFile => {}
            Err(error) => return Err(error).context("SFTP upload directory check failed"),
        }
        let created = timeout(
            REQUEST_TIMEOUT,
            session.mkdir(directory.clone(), FileAttributes::default()),
        )
        .await
        .context("creating remote upload directory timed out")?;
        if let Err(error) = created {
            let check = timeout(REQUEST_TIMEOUT, session.lstat(directory.clone()))
                .await
                .context("rechecking remote upload directory timed out")?;
            match check {
                Ok(attrs) if attrs.attrs.is_dir() => continue,
                _ => return Err(error).context("cannot create remote upload directory"),
            }
        }
    }
    Ok(())
}

pub(super) async fn find_keep_both_path(
    session: &RawSftpSession,
    original: &str,
) -> Result<String> {
    let (parent, name) = original
        .rsplit_once('/')
        .context("remote upload path is invalid")?;
    let (stem, extension) = name
        .rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map_or((name, ""), |(stem, extension)| (stem, extension));
    for number in 1..=MAX_KEEP_BOTH_CANDIDATES {
        let candidate_name = if extension.is_empty() {
            format!("{stem} ({number})")
        } else {
            format!("{stem} ({number}).{extension}")
        };
        if candidate_name.chars().count() > MAX_NAME_CHARS {
            anyhow::bail!("remote file name is too long to keep both copies");
        }
        let candidate = format!("{parent}/{candidate_name}");
        validate_remote_path(&candidate)?;
        if inspect_remote_upload_target(session, &candidate)
            .await?
            .is_none()
        {
            return Ok(candidate);
        }
    }
    anyhow::bail!("no available remote name for another copy")
}

pub(super) async fn posix_rename_upload(
    session: &RawSftpSession,
    from: &str,
    to: &str,
) -> Result<()> {
    let payload = russh_sftp::ser::to_bytes(&(from, to))
        .context("cannot encode SFTP atomic overwrite request")?;
    match timeout(
        REQUEST_TIMEOUT,
        session.extended(POSIX_RENAME_EXTENSION, payload.to_vec()),
    )
    .await
    .context("SFTP atomic overwrite timed out")?
    .context("SFTP atomic overwrite failed")?
    {
        Packet::Status(status) if status.status_code == StatusCode::Ok => Ok(()),
        Packet::Status(status) => anyhow::bail!("SFTP atomic overwrite failed: {status:?}"),
        _ => anyhow::bail!("SFTP atomic overwrite returned an unexpected response"),
    }
}
