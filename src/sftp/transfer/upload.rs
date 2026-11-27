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
    if let Err(error) =
        ensure_remote_upload_directories(&session, &request.remote_directories, cancellation).await
    {
        close_upload_session(&session);
        return Err(error);
    }
    cancellation.wait_until_running().await?;
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
        (Some(_), Some(SftpConflictChoice::Skip)) => {
            close_upload_session(&session);
            return Ok(UploadOutcome::Skipped);
        }
        (Some(remote), Some(SftpConflictChoice::Overwrite)) => {
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
        (Some(_), Some(SftpConflictChoice::KeepBoth)) => {
            find_keep_both_path(&session, &request.remote_path, cancellation)
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
    let checkpoint = if let Some(store) = &request.resume {
        let (record, existing) = load_checkpoint(
            store.clone(),
            ResumeKey {
                transfer_id: request.transfer_id,
                direction: TransferDirection::Upload,
                remote_path: request.remote_path.clone(),
                local_path: request.local_path.clone(),
            },
            request.total_bytes,
        )
        .await?;
        if let Some(mut record) = record {
            if existing {
                anyhow::ensure!(
                    record.local_source.as_ref() == Some(request.local_identity.as_ref()),
                    "SFTP upload source changed; restart the transfer"
                );
            }
            record.local_source = Some(request.local_identity.as_ref().clone());
            Some((store, record, existing))
        } else {
            None
        }
    } else {
        None
    };
    let partial_id = checkpoint
        .as_ref()
        .map_or(request.execution_id(), |(_, record, _)| record.partial_id);
    let temporary = remote_upload_partial_path(parent, &request.name, partial_id);
    let existing_checkpoint = checkpoint
        .as_ref()
        .is_some_and(|(_, _, existing)| *existing);
    let remote_partial = if existing_checkpoint {
        let attrs = await_step(
            cancellation,
            deadline,
            REQUEST_TIMEOUT,
            "inspecting partial upload",
            session.lstat(temporary.clone()),
        )
        .await??;
        Some(validate_regular_metadata(&attrs.attrs)?)
    } else {
        None
    };
    let handle = await_step(
        cancellation,
        deadline,
        REQUEST_TIMEOUT,
        "opening remote upload file",
        session.open(
            temporary.clone(),
            if existing_checkpoint {
                OpenFlags::READ | OpenFlags::WRITE
            } else {
                OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE
            },
            if checkpoint.is_some() {
                FileAttributes {
                    permissions: Some(0o600),
                    ..FileAttributes::empty()
                }
            } else {
                FileAttributes::default()
            },
        ),
    )
    .await?
    .context("cannot open remote upload file")?
    .handle;
    let setup = async {
        let Some((store, record, existing)) = checkpoint.as_ref() else {
            return Ok(Sha256::new());
        };
        if !existing {
            save_checkpoint(store, record).await?;
            return Ok(Sha256::new());
        }
        let attrs = await_step(
            cancellation,
            deadline,
            REQUEST_TIMEOUT,
            "checking opened partial upload",
            session.fstat(handle.clone()),
        )
        .await??;
        let opened = validate_regular_metadata(&attrs.attrs)?;
        anyhow::ensure!(
            Some(opened) == remote_partial
                && opened.size >= record.offset
                && opened.size <= record.size,
            "SFTP partial upload does not match its checkpoint"
        );
        send_transfer_state(
            event_tx,
            SftpTransferEvent::CheckingPartial {
                transfer_id: request.transfer_id,
                total_bytes: request.total_bytes,
            },
        )
        .await?;
        let hasher = verify_local_upload_prefix(request, record, cancellation).await?;
        verify_remote_prefix(&session, &handle, record, cancellation, deadline).await?;
        if opened.size > record.offset {
            await_step(
                cancellation,
                deadline,
                REQUEST_TIMEOUT,
                "discarding unconfirmed upload tail",
                session.fsetstat(
                    handle.clone(),
                    FileAttributes {
                        size: Some(record.offset),
                        ..FileAttributes::empty()
                    },
                ),
            )
            .await??;
        }
        send_transfer_state(
            event_tx,
            SftpTransferEvent::ContinuedFromPartial {
                transfer_id: request.transfer_id,
                offset: record.offset,
                total_bytes: request.total_bytes,
            },
        )
        .await?;
        Ok::<_, anyhow::Error>(hasher)
    }
    .await;
    let mut queued_hasher = match setup {
        Ok(hasher) => hasher,
        Err(error) => {
            if let Err(close_error) = timeout(REQUEST_TIMEOUT, session.close(handle)).await {
                debug!(%close_error, "cannot close unverified SFTP upload");
            }
            if !existing_checkpoint {
                remove_upload_temporary(&session, &temporary).await;
            }
            close_upload_session(&session);
            return Err(error);
        }
    };
    let mut checkpoint = checkpoint.map(|(store, record, _)| (store, record));
    let mut offset = checkpoint.as_ref().map_or(0, |(_, record)| record.offset);
    let (mut chunks, mut reader) = start_local_upload_reader_at(request, cancellation, offset);
    let mut next_offset = offset;
    let mut confirmed_digest: [u8; 32] = queued_hasher.clone().finalize().into();
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
                let digest = checkpoint.as_ref().map(|_| {
                    queued_hasher.update(&chunk);
                    queued_hasher.clone().finalize().into()
                });
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
                        Ok((chunk_bytes, digest))
                    })
                    .await?;
                next_offset += chunk_bytes;
            }
            let (chunk_bytes, digest) = requests
                .next()
                .await
                .context("SFTP write window ended unexpectedly")??;
            offset += chunk_bytes;
            if let (Some((store, record)), Some(digest)) = (&mut checkpoint, digest) {
                confirmed_digest = digest;
                if offset - record.offset >= CHECKPOINT_INTERVAL {
                    record.offset = offset;
                    record.digest = confirmed_digest;
                    save_checkpoint(store, record).await?;
                }
            }
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
    let preserve = cancellation.preserves_checkpoint()
        || result.as_ref().is_err_and(transfer_error_is_recoverable);
    if let Some((store, record)) = &mut checkpoint
        && offset > record.offset
    {
        record.offset = offset;
        record.digest = confirmed_digest;
        if let Err(error) = save_checkpoint(store, record).await {
            warn!(transfer_id = %request.transfer_id, %error, "could not advance SFTP upload checkpoint");
        }
    }
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
            cancellation.wait_until_running().await?;
            let current = inspect_remote_upload_target(&session, &target_path).await?;
            cancellation.wait_until_running().await?;
            if overwrite {
                anyhow::ensure!(
                    current == existing,
                    "remote upload target changed before publication"
                );
            } else {
                anyhow::ensure!(
                    current.is_none(),
                    "remote upload target appeared before publication"
                );
            }
            if let Some((store, record)) = &mut checkpoint {
                record.publishing = true;
                save_checkpoint(store, record).await?;
            }
            if overwrite {
                posix_rename_upload(&session, &temporary, &target_path)
                    .await
                    .context(UploadPublicationUncertain)
            } else {
                timeout(
                    REQUEST_TIMEOUT,
                    session.rename(temporary.clone(), target_path.clone()),
                )
                .await
                .context("publishing remote upload timed out")
                .context(UploadPublicationUncertain)?
                .context("cannot publish remote upload")
                .context(UploadPublicationUncertain)?;
                Ok(())
            }
        }
        .await;
        if let Err(error) = published {
            if !error.is::<UploadPublicationUncertain>()
                && !cancellation.preserves_checkpoint()
                && !transfer_error_is_recoverable(&error)
            {
                remove_upload_temporary(&session, &temporary).await;
                if let Some((store, _)) = &checkpoint {
                    remove_checkpoint(store, request.transfer_id).await;
                }
            }
            close_upload_session(&session);
            return Err(error);
        }
        if let Some((store, _)) = &checkpoint {
            remove_checkpoint(store, request.transfer_id).await;
        }
    } else if checkpoint.is_none()
        || (!preserve && !result.as_ref().is_err_and(transfer_error_is_recoverable))
    {
        remove_upload_temporary(&session, &temporary).await;
        if let Some((store, _)) = &checkpoint {
            remove_checkpoint(store, request.transfer_id).await;
        }
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

#[cfg(test)]
pub(super) fn start_local_upload_reader(
    request: &SftpUploadRequest,
    cancellation: &TransferCancellation,
) -> (mpsc::Receiver<Result<Vec<u8>>>, JoinHandle<Result<()>>) {
    start_local_upload_reader_at(request, cancellation, 0)
}

fn start_local_upload_reader_at(
    request: &SftpUploadRequest,
    cancellation: &TransferCancellation,
    offset: u64,
) -> (mpsc::Receiver<Result<Vec<u8>>>, JoinHandle<Result<()>>) {
    let request = request.clone();
    let cancellation = cancellation.clone();
    let (sender, receiver) = mpsc::channel(WRITER_QUEUE_CAPACITY);
    let task = tokio::task::spawn_blocking(move || {
        let result = read_local_upload_chunks(&request, &cancellation, &sender, offset);
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
    if local_file_identity(&metadata) != *request.local_identity
        || local_file_identity(&opened) != *request.local_identity
    {
        anyhow::bail!("local upload file changed while it was being uploaded");
    }
    Ok(())
}

pub(super) fn read_local_upload_chunks(
    request: &SftpUploadRequest,
    cancellation: &TransferCancellation,
    sender: &mpsc::Sender<Result<Vec<u8>>>,
    mut offset: u64,
) -> Result<()> {
    let metadata =
        fs::symlink_metadata(&request.local_path).context("cannot inspect local upload source")?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || local_file_identity(&metadata) != *request.local_identity
    {
        anyhow::bail!("local upload file changed before it was opened");
    }
    let mut file =
        LocalFile::open(&request.local_path).context("cannot open local upload source")?;
    file.seek(SeekFrom::Start(offset))
        .context("cannot seek local upload source")?;
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

async fn verify_local_upload_prefix(
    request: &SftpUploadRequest,
    record: &ResumeRecord,
    cancellation: &TransferCancellation,
) -> Result<Sha256> {
    let (request, record, cancellation) = (request.clone(), record.clone(), cancellation.clone());
    tokio::task::spawn_blocking(move || {
        let mut file = LocalFile::open(&request.local_path)
            .context("cannot open upload source for checkpoint verification")?;
        validate_local_upload_file(&request, &file)?;
        let hasher = hash_local_prefix(&mut file, record.offset, &cancellation)?;
        validate_local_upload_file(&request, &file)?;
        let digest: [u8; 32] = hasher.clone().finalize().into();
        anyhow::ensure!(
            record.digest == digest,
            "SFTP upload source prefix changed; restart the transfer"
        );
        Ok(hasher)
    })
    .await
    .context("SFTP upload prefix verification task failed")?
}

pub(super) fn local_file_identity(metadata: &fs::Metadata) -> LocalFileIdentity {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        LocalFileIdentity {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
    #[cfg(not(unix))]
    {
        LocalFileIdentity {
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
    match remote_upload_entry_attributes(session, path).await? {
        Some(attrs) if attrs.file_type().is_file() => Ok(Some(RemoteUploadFingerprint {
            size: attrs.size,
            modified: attrs.mtime,
        })),
        Some(_) => anyhow::bail!("remote upload target is not a regular file"),
        None => Ok(None),
    }
}

async fn remote_upload_entry_attributes(
    session: &RawSftpSession,
    path: &str,
) -> Result<Option<FileAttributes>> {
    match timeout(REQUEST_TIMEOUT, session.lstat(path.to_owned())).await {
        Ok(Ok(attrs)) => Ok(Some(attrs.attrs)),
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
    cancellation: &TransferCancellation,
) -> Result<()> {
    for directory in directories {
        cancellation.wait_until_running().await?;
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
        cancellation.wait_until_running().await?;
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
    cancellation: &TransferCancellation,
) -> Result<String> {
    let (parent, name) = original
        .rsplit_once('/')
        .context("remote upload path is invalid")?;
    let (stem, extension) = name
        .rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map_or((name, ""), |(stem, extension)| (stem, extension));
    let mut number = 1_u64;
    loop {
        cancellation.wait_until_running().await?;
        let candidate_name = if extension.is_empty() {
            format!("{stem} ({number})")
        } else {
            format!("{stem} ({number}).{extension}")
        };
        let candidate = format!("{parent}/{candidate_name}");
        validate_remote_path(&candidate)?;
        // Any existing entry occupies a candidate name. In particular, never
        // follow a link or treat a directory as a failed file overwrite.
        if remote_upload_entry_attributes(session, &candidate)
            .await?
            .is_none()
        {
            return Ok(candidate);
        }
        number = number
            .checked_add(1)
            .context("no available remote name for another copy")?;
    }
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
