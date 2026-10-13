//! SFTP discovery operations owned by the transfer worker.

use super::*;

pub(super) fn validate_download_path(path: &str) -> Result<&str> {
    validate_remote_path(path)?;
    let name = path.rsplit('/').next().unwrap_or_default();
    if name.is_empty() || name == "." || name == ".." {
        anyhow::bail!("remote download path must name a file");
    }
    if name.chars().count() > MAX_NAME_CHARS {
        anyhow::bail!("remote file name cannot exceed {MAX_NAME_CHARS} characters");
    }
    Ok(name)
}

pub(crate) async fn discover_download_requests<S>(
    stream: S,
    root: SftpDownloadRoot,
) -> Result<Vec<SftpDownloadRequest>>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
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
    timeout(REQUEST_TIMEOUT, session.init())
        .await
        .context("SFTP recursive-download handshake timed out")?
        .context("SFTP recursive-download handshake failed")?;
    let result = match timeout(
        DISCOVERY_TIMEOUT,
        discover_initialized_download_requests(&session, &root),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(anyhow::anyhow!(
            "SFTP recursive download exceeded the overall timeout"
        )),
    };
    if let Err(error) = session.close_session() {
        debug!(%error, "failed to close SFTP recursive-download session");
    }
    result
}

/// Returns whether a single SFTP entry name matches one of the configured
/// transfer-filter patterns. Patterns use `*` as a zero-or-more wildcard.
pub fn transfer_name_matches_filter(name: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .any(|pattern| wildcard_matches(pattern, name))
}

pub(super) fn wildcard_matches(pattern: &str, value: &str) -> bool {
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let mut pattern_index = 0;
    let mut value_index = 0;
    let mut star_index = None;
    let mut star_value_index = 0;
    while value_index < value.len() {
        if pattern_index < pattern.len()
            && pattern[pattern_index] != b'*'
            && pattern[pattern_index] == value[value_index]
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            pattern_index += 1;
            star_value_index = value_index;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            star_value_index += 1;
            value_index = star_value_index;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

pub(super) async fn discover_initialized_download_requests(
    session: &RawSftpSession,
    root: &SftpDownloadRoot,
) -> Result<Vec<SftpDownloadRequest>> {
    let attrs = timeout(REQUEST_TIMEOUT, session.lstat(root.remote_path.clone()))
        .await
        .context("inspecting recursive-download root timed out")?
        .with_context(|| format!("cannot inspect remote path {:?}", root.remote_path))?;
    if attrs.attrs.is_symlink() {
        anyhow::bail!("remote symbolic links cannot be downloaded");
    }
    if attrs.attrs.is_regular() {
        if transfer_name_matches_filter(&root.name, root.filter_patterns()) {
            anyhow::bail!("remote path is excluded by SFTP transfer filters");
        }
        let total_bytes = validate_regular_metadata(&attrs.attrs)?.size;
        if total_bytes > root.max_file_bytes {
            anyhow::bail!("remote file exceeds the configured SFTP file size limit");
        }
        return Ok(vec![SftpDownloadRequest::for_local_download(
            root.transfer_id,
            root.remote_path.clone(),
            root.local_directory.clone(),
            vec![root.name.clone()],
            total_bytes,
        )?]);
    }
    if !attrs.attrs.is_dir() {
        anyhow::bail!("remote path is neither a regular file nor a directory");
    }

    let mut requests = Vec::new();
    let mut pending = std::collections::VecDeque::from([(
        root.remote_path.clone(),
        vec![root.name.clone()],
        0_usize,
    )]);
    let mut directories = 1_usize;
    let mut scanned_entries = 0_usize;
    let mut total_text_bytes = root.remote_path.len().saturating_add(root.name.len());
    while let Some((remote_directory, local_components, depth)) = pending.pop_front() {
        let handle = timeout(REQUEST_TIMEOUT, session.opendir(remote_directory.clone()))
            .await
            .context("opening remote download directory timed out")?
            .with_context(|| format!("cannot open remote directory {remote_directory:?}"))?
            .handle;
        let directory_result = discover_directory_entries(
            session,
            &handle,
            &remote_directory,
            &local_components,
            depth,
            &mut pending,
            &mut requests,
            &mut scanned_entries,
            &mut directories,
            &mut total_text_bytes,
            root,
        )
        .await;
        close_remote_handle(session, handle).await;
        directory_result?;
    }
    if requests.is_empty() {
        anyhow::bail!("remote directory contains no regular files eligible for download");
    }
    Ok(requests)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn discover_directory_entries(
    session: &RawSftpSession,
    handle: &str,
    remote_directory: &str,
    local_components: &[String],
    depth: usize,
    pending: &mut std::collections::VecDeque<(String, Vec<String>, usize)>,
    requests: &mut Vec<SftpDownloadRequest>,
    scanned_entries: &mut usize,
    directories: &mut usize,
    total_text_bytes: &mut usize,
    root: &SftpDownloadRoot,
) -> Result<()> {
    loop {
        let response = timeout(REQUEST_TIMEOUT, session.readdir(handle.to_owned()))
            .await
            .context("reading remote download directory timed out")?;
        let files = match response {
            Ok(names) if names.files.is_empty() => break,
            Ok(names) => names.files,
            Err(russh_sftp::client::error::Error::Status(status))
                if status.status_code == StatusCode::Eof =>
            {
                break;
            }
            Err(error) => return Err(error).context("cannot read remote download directory"),
        };
        for file in files {
            reserve_recursive_entry(scanned_entries)?;
            let Some((name, path, kind, size)) = bounded_discovery_entry(remote_directory, file)
            else {
                continue;
            };
            if transfer_name_matches_filter(&name, root.filter_patterns()) {
                continue;
            }
            *total_text_bytes = total_text_bytes
                .saturating_add(name.len())
                .saturating_add(path.len());
            if *total_text_bytes > MAX_RECURSIVE_DOWNLOAD_TEXT_BYTES {
                anyhow::bail!(
                    "remote download tree exceeds the {MAX_RECURSIVE_DOWNLOAD_TEXT_BYTES}-byte text limit"
                );
            }
            let mut child_components = local_components.to_vec();
            child_components.push(name);
            match kind {
                DiscoveryEntryKind::Directory => {
                    if depth >= MAX_RECURSIVE_DOWNLOAD_DEPTH {
                        anyhow::bail!(
                            "remote download tree exceeds the {MAX_RECURSIVE_DOWNLOAD_DEPTH}-level depth limit"
                        );
                    }
                    reserve_recursive_directory(directories)?;
                    pending.push_back((path, child_components, depth + 1));
                }
                DiscoveryEntryKind::RegularFile => {
                    if requests.len() >= MAX_RECURSIVE_DOWNLOAD_FILES {
                        anyhow::bail!(
                            "remote download tree exceeds the {MAX_RECURSIVE_DOWNLOAD_FILES}-file limit"
                        );
                    }
                    if size > root.max_file_bytes {
                        anyhow::bail!("remote file exceeds the configured SFTP file size limit");
                    }
                    requests.push(SftpDownloadRequest::for_local_download(
                        if requests.is_empty() {
                            root.transfer_id
                        } else {
                            Uuid::new_v4()
                        },
                        path,
                        root.local_directory.clone(),
                        child_components,
                        size,
                    )?);
                }
            }
        }
    }
    Ok(())
}

pub(super) fn reserve_recursive_entry(scanned_entries: &mut usize) -> Result<()> {
    *scanned_entries = scanned_entries.saturating_add(1);
    if *scanned_entries > MAX_RECURSIVE_DOWNLOAD_ENTRIES {
        anyhow::bail!(
            "remote download tree exceeds the {MAX_RECURSIVE_DOWNLOAD_ENTRIES}-entry scan limit"
        );
    }
    Ok(())
}

pub(super) fn reserve_recursive_directory(directories: &mut usize) -> Result<()> {
    *directories = directories.saturating_add(1);
    if *directories > MAX_RECURSIVE_DOWNLOAD_DIRECTORIES {
        anyhow::bail!(
            "remote download tree exceeds the {MAX_RECURSIVE_DOWNLOAD_DIRECTORIES}-directory limit"
        );
    }
    Ok(())
}

pub(super) enum DiscoveryEntryKind {
    Directory,
    RegularFile,
}

pub(super) fn bounded_discovery_entry(
    parent: &str,
    file: File,
) -> Option<(String, String, DiscoveryEntryKind, u64)> {
    let name = file.filename;
    if name == "."
        || name == ".."
        || name.is_empty()
        || name.chars().count() > MAX_NAME_CHARS
        || name.chars().any(char::is_control)
        || name.contains(['/', '\\'])
    {
        return None;
    }
    let path = join_remote_path(parent, &name);
    if validate_remote_path(&path).is_err() || file.attrs.is_symlink() {
        return None;
    }
    if file.attrs.is_dir() {
        return Some((name, path, DiscoveryEntryKind::Directory, 0));
    }
    if !file.attrs.is_regular() {
        return None;
    }
    let size = file.attrs.size?;
    Some((name, path, DiscoveryEntryKind::RegularFile, size))
}

pub(super) fn join_remote_path(parent: &str, child: &str) -> String {
    if parent == "/" {
        format!("/{child}")
    } else {
        format!("{parent}/{child}")
    }
}
