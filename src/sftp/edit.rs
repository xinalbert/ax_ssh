//! Byte-preserving, bounded external edits with guarded atomic publication.

use super::*;
use russh_sftp::protocol::{Packet, Version};

const POSIX_RENAME: &str = "posix-rename@openssh.com";

async fn attributes(session: &RawSftpSession, path: &str) -> Result<FileAttributes> {
    validate_remote_path(path)?;
    let attrs = timeout(REQUEST_TIMEOUT, session.lstat(path.to_owned()))
        .await
        .context("SFTP edit metadata check timed out")?
        .context("cannot inspect remote edit file")?
        .attrs;
    validate_attributes(&attrs)?;
    Ok(attrs)
}

fn validate_attributes(attrs: &FileAttributes) -> Result<()> {
    if attrs.is_symlink() || !attrs.is_regular() {
        anyhow::bail!("external editing requires a regular remote file");
    }
    attrs
        .size
        .context("server did not provide the edit file size")?;
    Ok(())
}

fn require_fingerprint(attrs: &FileAttributes, size: u64, modified: Option<u32>) -> Result<()> {
    validate_attributes(attrs)?;
    if attrs.size != Some(size) || attrs.mtime != modified {
        anyhow::bail!("remote file changed since it was opened; edit was rejected");
    }
    Ok(())
}

fn validate_edit_limit(max_bytes: u64) -> Result<()> {
    let maximum = u64::from(crate::config::MAX_SFTP_REMOTE_EDIT_MAX_FILE_MIB) * 1024 * 1024;
    if !(1024 * 1024..=maximum).contains(&max_bytes) {
        anyhow::bail!("remote edit size limit is outside the supported 1–64 MiB range");
    }
    Ok(())
}

pub(super) async fn read_edit_file(
    session: &RawSftpSession,
    path: String,
    max_bytes: u64,
) -> Result<SftpWriteResult> {
    validate_edit_limit(max_bytes)?;
    let initial = attributes(session, &path).await?;
    let size = initial.size.context("missing edit size")?;
    if size > max_bytes {
        anyhow::bail!("remote file exceeds the {max_bytes}-byte edit limit");
    }
    let handle = timeout(
        REQUEST_TIMEOUT,
        session.open(path.clone(), OpenFlags::READ, FileAttributes::empty()),
    )
    .await
    .context("SFTP edit open timed out")?
    .context("cannot open remote edit file")?
    .handle;
    let result = async {
        let opened = timeout(REQUEST_TIMEOUT, session.fstat(handle.clone()))
            .await
            .context("SFTP edit handle check timed out")??;
        require_fingerprint(&opened.attrs, size, initial.mtime)?;
        use super::transfer::pipeline::{REQUEST_WINDOW, RequestWindow};
        let mut data = Vec::with_capacity(size as usize);
        let mut queued = 0_u64;
        let mut requests = RequestWindow::new();
        let remote_handle = handle.as_str();
        while (data.len() as u64) < size {
            while requests.len() < REQUEST_WINDOW && queued < size {
                let start = queued;
                let length = (size - queued).min(64 * 1024) as usize;
                requests
                    .push(async move {
                        let mut chunk = Vec::with_capacity(length);
                        while chunk.len() < length {
                            let remaining = length - chunk.len();
                            let packet = timeout(
                                REQUEST_TIMEOUT,
                                session.read(
                                    remote_handle.to_owned(),
                                    start + chunk.len() as u64,
                                    remaining as u32,
                                ),
                            )
                            .await
                            .context("SFTP edit read timed out")?
                            .context("remote edit file ended before its validated size")?;
                            if packet.data.is_empty() || packet.data.len() > remaining {
                                anyhow::bail!("remote edit file returned an invalid read length");
                            }
                            chunk.extend_from_slice(&packet.data);
                        }
                        Ok(chunk)
                    })
                    .await?;
                queued += length as u64;
            }
            data.extend(
                requests
                    .next()
                    .await
                    .context("missing SFTP edit read reply")??,
            );
        }
        match timeout(REQUEST_TIMEOUT, session.read(handle.clone(), size, 1)).await {
            Ok(Err(SftpClientError::Status(status))) if status.status_code == StatusCode::Eof => {}
            _ => anyhow::bail!("remote edit file did not end at its validated size"),
        }
        let finished = timeout(REQUEST_TIMEOUT, session.fstat(handle.clone()))
            .await
            .context("SFTP edit final handle check timed out")??;
        require_fingerprint(&finished.attrs, size, initial.mtime)?;
        require_fingerprint(&attributes(session, &path).await?, size, initial.mtime)?;
        Ok::<_, anyhow::Error>(SftpWriteResult::EditFile {
            path,
            data,
            expected_size: size,
            expected_modified: initial.mtime,
        })
    }
    .await;
    let closed = timeout(REQUEST_TIMEOUT, session.close(handle))
        .await
        .context("closing remote edit file timed out")?
        .context("cannot close remote edit file");
    result.and_then(|value| closed.map(|_| value))
}

pub(super) async fn write_edit_file(
    session: &RawSftpSession,
    version: &Version,
    path: String,
    data: Vec<u8>,
    max_bytes: u64,
    expected_size: u64,
    expected_modified: Option<u32>,
) -> Result<SftpWriteResult> {
    validate_edit_limit(max_bytes)?;
    if data.len() as u64 > max_bytes {
        anyhow::bail!("edited file exceeds the {max_bytes}-byte edit limit");
    }
    let original = attributes(session, &path).await?;
    require_fingerprint(&original, expected_size, expected_modified)?;
    if version.extensions.get(POSIX_RENAME).map(String::as_str) != Some("1") {
        anyhow::bail!("server does not support atomic SFTP overwrite; local edit is retained");
    }
    let (parent, _) = path
        .rsplit_once('/')
        .context("remote edit path must be absolute")?;
    let temporary = format!("{parent}/.axssh-edit-{}", Uuid::new_v4());
    let handle = timeout(
        REQUEST_TIMEOUT,
        session.open(
            temporary.clone(),
            OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
            FileAttributes {
                permissions: Some(0o600),
                ..FileAttributes::empty()
            },
        ),
    )
    .await
    .context("SFTP edit temporary open timed out")?
    .context("cannot create temporary remote edit file")?
    .handle;
    let written = async {
        write_remote_bytes(session, &handle, &data).await?;
        if let Some(permissions) = original.permissions {
            timeout(
                REQUEST_TIMEOUT,
                session.fsetstat(
                    handle.clone(),
                    FileAttributes {
                        permissions: Some(permissions & 0o777),
                        ..FileAttributes::empty()
                    },
                ),
            )
            .await
            .context("SFTP edit permission update timed out")?
            .context("cannot preserve remote edit permissions")?;
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let closed = timeout(REQUEST_TIMEOUT, session.close(handle))
        .await
        .context("SFTP edit temporary close timed out")
        .and_then(|result| result.context("cannot close temporary remote edit file"));
    let published = async {
        written.and(closed)?;
        let current = attributes(session, &path).await?;
        require_fingerprint(&current, expected_size, expected_modified)?;
        if current.permissions != original.permissions {
            anyhow::bail!("remote permissions changed before edit publication");
        }
        let payload = russh_sftp::ser::to_bytes(&(temporary.as_str(), path.as_str()))
            .context("cannot encode atomic edit publication")?;
        match timeout(
            REQUEST_TIMEOUT,
            session.extended(POSIX_RENAME, payload.to_vec()),
        )
        .await
        .context("SFTP edit publication timed out")?
        .context("cannot publish edited remote file")?
        {
            Packet::Status(status) if status.status_code == StatusCode::Ok => {}
            _ => anyhow::bail!("atomic edit publication failed"),
        }
        let saved = attributes(session, &path).await?;
        if saved.size != Some(data.len() as u64) {
            anyhow::bail!("remote edit size changed after publication");
        }
        Ok(SftpWriteResult::Updated {
            path,
            size: data.len() as u64,
            modified: saved.mtime,
        })
    }
    .await;
    if published.is_err()
        && let Err(error) = timeout(REQUEST_TIMEOUT, session.remove(temporary))
            .await
            .context("temporary edit cleanup timed out")
            .and_then(|result| result.context("cannot remove temporary edit file"))
    {
        debug!(%error, "temporary remote edit cleanup failed");
    }
    published
}

#[cfg(test)]
mod tests;
