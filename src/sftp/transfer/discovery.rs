//! SFTP discovery operations owned by the transfer worker.

use super::*;
use crate::sftp::SftpDiscoveredDownload;

pub(super) fn validate_download_path(path: &str) -> Result<&str> {
    validate_remote_path(path)?;
    let name = path.rsplit('/').next().unwrap_or_default();
    if name.is_empty() || name == "." || name == ".." {
        anyhow::bail!("remote download path must name a file");
    }
    Ok(name)
}

pub(crate) async fn discover_download_requests<S>(
    stream: S,
    root: SftpDownloadRoot,
    discovered_tx: mpsc::Sender<SftpDiscoveredDownload>,
) -> Result<SftpDiscoverySummary>
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
    let result = discover_initialized_download_requests(&session, &root, discovered_tx).await;
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
    discovered_tx: mpsc::Sender<SftpDiscoveredDownload>,
) -> Result<SftpDiscoverySummary> {
    if transfer_name_matches_filter(&root.name, root.filter_patterns()) {
        return Ok(SftpDiscoverySummary {
            discovered_files: 0,
            skipped_entries: 1,
            first_issue: Some("Excluded by transfer filter".to_owned()),
            is_directory: false,
        });
    }
    let attrs = timeout(REQUEST_TIMEOUT, session.lstat(root.remote_path.clone()))
        .await
        .context("inspecting recursive-download root timed out")?
        .with_context(|| format!("cannot inspect remote path {:?}", root.remote_path))?;
    if attrs.attrs.is_symlink() {
        anyhow::bail!("remote symbolic links cannot be downloaded");
    }
    if attrs.attrs.is_regular() {
        let total_bytes = validate_regular_metadata(&attrs.attrs)?.size;
        let request = SftpDownloadRequest::for_local_download(
            if root.expected_directory() {
                Uuid::new_v4()
            } else {
                root.transfer_id
            },
            root.remote_path.clone(),
            root.local_directory.clone(),
            vec![root.name.clone()],
            total_bytes,
        )?;
        discovered_tx
            .send(SftpDiscoveredDownload::Request {
                root_transfer_id: root.transfer_id,
                request,
            })
            .await
            .context("SFTP download receiver stopped during discovery")?;
        return Ok(SftpDiscoverySummary {
            discovered_files: 1,
            skipped_entries: 0,
            first_issue: None,
            is_directory: root.expected_directory(),
        });
    }
    if !attrs.attrs.is_dir() {
        anyhow::bail!("remote path is neither a regular file nor a directory");
    }

    let handle = timeout(REQUEST_TIMEOUT, session.opendir(root.remote_path.clone()))
        .await
        .context("opening remote download directory timed out")?
        .with_context(|| format!("cannot open remote directory {:?}", root.remote_path))?
        .handle;
    let mut pending = vec![DownloadDirectoryFrame {
        remote_directory: root.remote_path.clone(),
        local_components: vec![root.name.clone()],
        handle,
        entries: std::collections::VecDeque::new(),
    }];
    let mut summary = SftpDiscoverySummary {
        is_directory: true,
        ..SftpDiscoverySummary::default()
    };
    while !pending.is_empty() {
        let frame = pending
            .last_mut()
            .context("download directory stack is empty")?;
        if let Some(file) = frame.entries.pop_front() {
            let entry = download_entry(&frame.remote_directory, file);
            let (name, path, is_directory, size) = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    record_discovery_issue(&mut summary, error);
                    continue;
                }
            };
            if transfer_name_matches_filter(&name, root.filter_patterns()) {
                record_discovery_issue(&mut summary, "Excluded by transfer filter");
                continue;
            }
            let mut child_components = frame.local_components.clone();
            child_components.push(name.clone());
            if is_directory {
                match timeout(REQUEST_TIMEOUT, session.opendir(path.clone())).await {
                    Ok(Ok(response)) => pending.push(DownloadDirectoryFrame {
                        remote_directory: path,
                        local_components: child_components,
                        handle: response.handle,
                        entries: std::collections::VecDeque::new(),
                    }),
                    Ok(Err(error)) => {
                        record_discovery_issue(
                            &mut summary,
                            anyhow::Error::new(error)
                                .context("cannot open nested remote download directory"),
                        );
                    }
                    Err(_) => {
                        record_discovery_issue(
                            &mut summary,
                            "opening nested remote download directory timed out",
                        );
                    }
                }
                continue;
            }
            let request = match SftpDownloadRequest::for_local_download(
                Uuid::new_v4(),
                path,
                root.local_directory.clone(),
                child_components,
                size,
            ) {
                Ok(request) => request,
                Err(error) => {
                    record_discovery_issue(&mut summary, error);
                    continue;
                }
            };
            discovered_tx
                .send(SftpDiscoveredDownload::Request {
                    root_transfer_id: root.transfer_id,
                    request,
                })
                .await
                .context("SFTP download receiver stopped during discovery")?;
            summary.discovered_files = summary.discovered_files.saturating_add(1);
            continue;
        }

        let handle = frame.handle.clone();
        let response = timeout(REQUEST_TIMEOUT, session.readdir(handle.clone())).await;
        match response {
            Ok(Ok(names)) if !names.files.is_empty() => frame.entries = names.files.into(),
            Ok(Ok(_)) => {
                if let Some(finished) = pending.pop() {
                    close_remote_handle(session, finished.handle).await;
                }
            }
            Ok(Err(russh_sftp::client::error::Error::Status(status)))
                if status.status_code == StatusCode::Eof =>
            {
                if let Some(finished) = pending.pop() {
                    close_remote_handle(session, finished.handle).await;
                }
            }
            Ok(Err(error)) => {
                record_discovery_issue(
                    &mut summary,
                    anyhow::Error::new(error).context("cannot read remote download directory"),
                );
                if let Some(finished) = pending.pop() {
                    close_remote_handle(session, finished.handle).await;
                }
            }
            Err(_) => {
                record_discovery_issue(&mut summary, "reading remote download directory timed out");
                if let Some(finished) = pending.pop() {
                    close_remote_handle(session, finished.handle).await;
                }
            }
        }
    }
    Ok(summary)
}

fn record_discovery_issue(summary: &mut SftpDiscoverySummary, error: impl std::fmt::Display) {
    summary.skipped_entries = summary.skipped_entries.saturating_add(1);
    if summary.first_issue.is_none() {
        let text = error.to_string();
        let mut chars = text.chars();
        let mut issue = chars.by_ref().take(512).collect::<String>();
        if chars.next().is_some() {
            issue.push('…');
        }
        summary.first_issue = Some(issue);
    }
}

struct DownloadDirectoryFrame {
    remote_directory: String,
    local_components: Vec<String>,
    handle: String,
    entries: std::collections::VecDeque<File>,
}

pub(super) fn download_entry(parent: &str, file: File) -> Result<(String, String, bool, u64)> {
    let name = file.filename;
    if name == "."
        || name == ".."
        || name.is_empty()
        || name.chars().any(char::is_control)
        || name.contains(['/', '\\'])
    {
        anyhow::bail!("remote directory contains an unsafe entry name: {name:?}");
    }
    let path = join_remote_path(parent, &name);
    validate_remote_path(&path).context("remote download path is invalid")?;
    if file.attrs.is_symlink() {
        anyhow::bail!(
            "remote directory contains a symbolic link that cannot be downloaded: {path}"
        );
    }
    if file.attrs.is_dir() {
        return Ok((name, path, true, 0));
    }
    if !file.attrs.is_regular() {
        anyhow::bail!("remote directory contains an unsupported file type: {path}");
    }
    let size = file.attrs.size.context("remote file size is unavailable")?;
    Ok((name, path, false, size))
}

pub(super) fn join_remote_path(parent: &str, child: &str) -> String {
    if parent == "/" {
        format!("/{child}")
    } else {
        format!("{parent}/{child}")
    }
}
