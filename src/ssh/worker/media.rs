//! Auxiliary channels share authenticated transport, never the terminal PTY.

use super::*;
use russh::ChannelMsg;
use russh_sftp::{
    client::{Config, rawsession::RawSftpSession},
    protocol::{FileAttributes, OpenFlags},
};
use std::sync::Arc;
use tokio::sync::Mutex;

pub(super) type AttachmentDirectories = Arc<Mutex<Vec<String>>>;

pub(super) async fn upload_image(
    connection: SshConnection,
    png: Vec<u8>,
    directories: AttachmentDirectories,
) -> Result<String> {
    timeout(Duration::from_secs(30), async {
        let stream = connection.open_sftp_stream().await?;
        upload_image_stream(stream, png, directories).await
    })
    .await
    .context("clipboard image upload timed out")?
}

async fn upload_image_stream<S>(
    stream: S,
    png: Vec<u8>,
    directories: AttachmentDirectories,
) -> Result<String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    timeout(Duration::from_secs(30), async {
        let session = RawSftpSession::new_with_config(
            stream,
            Config {
                request_timeout_secs: 10,
                max_concurrent_writes: 1,
                max_concurrent_reads: 1,
                max_packet_len: 64 * 1024,
                max_write_packet_len: 64 * 1024,
            },
        );
        session
            .init()
            .await
            .context("cannot initialize attachment SFTP")?;
        let home = session
            .realpath(".")
            .await?
            .files
            .into_iter()
            .next()
            .context("SFTP home directory missing")?
            .filename;
        anyhow::ensure!(
            home.starts_with('/') && home.len() <= 3000 && !home.chars().any(char::is_control),
            "unsupported remote SFTP home directory"
        );
        let directory = format!(
            "{}/.axssh-attachment-{}",
            home.trim_end_matches('/'),
            Uuid::new_v4()
        );
        // Track before creation: cancellation while waiting for a reply must
        // still leave a known, unique cleanup target.
        {
            let mut directories = directories.lock().await;
            anyhow::ensure!(
                directories.len() < crate::media::MAX_ATTACHMENTS,
                "session attachment limit reached (32)"
            );
            directories.push(directory.clone());
        }
        session
            .mkdir(
                directory.clone(),
                FileAttributes {
                    permissions: Some(0o700),
                    ..FileAttributes::default()
                },
            )
            .await
            .context("cannot create private remote attachment directory")?;
        session
            .setstat(
                directory.clone(),
                FileAttributes {
                    permissions: Some(0o700),
                    ..FileAttributes::default()
                },
            )
            .await?;
        let path = format!("{directory}/clipboard.png");
        let file = session
            .open(
                path.clone(),
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE,
                FileAttributes {
                    permissions: Some(0o600),
                    ..FileAttributes::default()
                },
            )
            .await
            .context("cannot create remote attachment")?;
        session
            .fsetstat(
                file.handle.clone(),
                FileAttributes {
                    permissions: Some(0o600),
                    ..FileAttributes::default()
                },
            )
            .await?;
        for (index, chunk) in png.chunks(32 * 1024).enumerate() {
            session
                .write(
                    file.handle.clone(),
                    (index * 32 * 1024) as u64,
                    chunk.to_vec(),
                )
                .await
                .context("cannot upload clipboard image")?;
        }
        session
            .close(file.handle)
            .await
            .context("cannot finish remote attachment")?;
        Ok(path)
    })
    .await
    .context("clipboard image upload timed out")?
}

pub(super) async fn cleanup_attachments(
    connection: &SshConnection,
    directories: &AttachmentDirectories,
) -> Result<()> {
    let directories = std::mem::take(&mut *directories.lock().await);
    if directories.is_empty() {
        return Ok(());
    }
    timeout(Duration::from_secs(5), async {
        let session = RawSftpSession::new(connection.open_sftp_stream().await?);
        session.init().await?;
        for directory in directories {
            for result in [
                session.remove(format!("{directory}/clipboard.png")).await,
                session.rmdir(directory).await,
            ] {
                if let Err(error) = result {
                    tracing::warn!(%error, "remote attachment cleanup failed");
                }
            }
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("remote attachment cleanup timed out")?
}

// Fixed shell source; only a generated UUID source name is substituted.
// No terminal input, profile value, path or secret is interpolated.
const AUDIO_HELPER: &str = r#"set -eu
export LC_ALL=C
[ "$(uname -s)" = Linux ] || { echo 'Remote microphone requires Linux' >&2; exit 1; }
command -v pactl >/dev/null || { echo 'Install pactl and run PulseAudio or pipewire-pulse for this SSH user' >&2; exit 1; }
umask 077
directory=$(mktemp -d "${TMPDIR:-/tmp}/axssh-audio.XXXXXXXX")
module=''
old=''
cleanup() {
    trap - EXIT HUP INT TERM
    if [ -n "$old" ] && [ "$(pactl get-default-source 2>/dev/null || true)" = "$source" ]; then
        pactl set-default-source "$old" || true
    fi
    if [ -n "$module" ]; then pactl unload-module "$module" || true; fi
    rm -f "$directory/input.pcm"
    rmdir "$directory" || true
}
trap cleanup EXIT
trap 'exit 0' HUP INT TERM
mkfifo -m 600 "$directory/input.pcm"
old=$(pactl get-default-source)
module=$(pactl load-module module-pipe-source file="$directory/input.pcm" source_name="$source" format=s16le rate="$rate" channels="$channels" source_properties=device.description=AxSSH_Microphone)
pactl set-default-source "$source"
printf 'AXSSH_AUDIO_READY\n'
cat > "$directory/input.pcm"
"#;

pub(super) async fn forward_microphone(
    connection: SshConnection,
    format: crate::audio::AudioFormat,
    mut pcm: mpsc::Receiver<Vec<u8>>,
    mut stop: watch::Receiver<bool>,
    ready: oneshot::Sender<Result<String>>,
) -> Result<()> {
    format.validate()?;
    let source = format!("axssh_{}", Uuid::new_v4().simple());
    let mut ready = Some(ready);
    let result = async {
        if *stop.borrow() { return Ok(()); }
        let mut channel = connection.open_auxiliary_channel().await?;
        if *stop.borrow() {
            if let Err(error) = channel.close().await {
                tracing::warn!(%error, "cannot close cancelled microphone channel");
            }
            return Ok(());
        }
        let command = format!("source='{source}'\nrate={}\nchannels={}\n{AUDIO_HELPER}", format.sample_rate, format.channels);
        let command = format!("exec sh -c '{}'", command.replace('\'', "'\\''"));
        let started = timeout(Duration::from_secs(2), channel.exec(true, command))
            .await.context("remote microphone command timed out")
            .and_then(|result| result.context("remote rejected microphone helper"));
        if let Err(error) = started {
            close_audio_channel(&mut channel).await;
            return Err(error);
        }
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let startup = timeout(Duration::from_secs(10), async {
            loop {
                tokio::select! {
                    _ = crate::audio::wait_for_stop(&mut stop) => anyhow::bail!("microphone startup cancelled"),
                    message = channel.wait() => match message {
                        Some(ChannelMsg::Data { data }) => {
                            anyhow::ensure!(stdout.len() + data.len() <= 1024, "invalid microphone handshake");
                            stdout.extend_from_slice(&data);
                            if stdout == b"AXSSH_AUDIO_READY\n" { break; }
                        }
                        Some(ChannelMsg::ExtendedData { data, .. }) => extend_error(&mut stderr, &data),
                        Some(ChannelMsg::ExitStatus { exit_status }) => anyhow::bail!("remote microphone exited ({exit_status}): {}", String::from_utf8_lossy(&stderr)),
                        Some(ChannelMsg::Eof | ChannelMsg::Close) | None => anyhow::bail!("remote microphone ended: {}", String::from_utf8_lossy(&stderr)),
                        _ => {}
                    }
                }
            }
            Ok::<_, anyhow::Error>(())
        }).await.context("remote microphone startup timed out");
        if let Err(error) = startup.and_then(|result| result) {
            close_audio_channel(&mut channel).await;
            return Err(error);
        }
        if ready.take().is_some_and(|reply| reply.send(Ok(source.clone())).is_err()) {
            close_audio_channel(&mut channel).await;
            return Ok(());
        }
        let outcome = loop {
            tokio::select! {
                _ = crate::audio::wait_for_stop(&mut stop) => break Ok(()),
                packet = pcm.recv() => {
                    let Some(packet) = packet else { break Ok(()); };
                    if packet.len() != format.packet_bytes() { break Err(anyhow::anyhow!("invalid PCM packet")); }
                    match timeout(Duration::from_secs(2), channel.data(packet.as_slice())).await {
                        Ok(Ok(())) => {},
                        Ok(Err(error)) => break Err(error.into()),
                        Err(_) => break Err(anyhow::anyhow!("remote audio receiver is too slow")),
                    }
                }
                message = channel.wait() => match message {
                    Some(ChannelMsg::ExtendedData { data, .. }) => extend_error(&mut stderr, &data),
                    Some(ChannelMsg::ExitStatus { exit_status }) => break Err(anyhow::anyhow!("remote microphone stopped ({exit_status}): {}", String::from_utf8_lossy(&stderr))),
                    Some(ChannelMsg::Eof | ChannelMsg::Close) | None => break Err(anyhow::anyhow!("remote microphone channel closed")),
                    _ => {}
                }
            }
        };
        close_audio_channel(&mut channel).await;
        outcome
    }.await;
    if let Some(ready) = ready {
        let message = result
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_else(|| "microphone cancelled".into());
        if ready.send(Err(anyhow::anyhow!(message))).is_err() {
            tracing::debug!("microphone requester cancelled");
        }
    }
    result
}

fn extend_error(output: &mut Vec<u8>, data: &[u8]) {
    output.extend_from_slice(&data[..data.len().min(512usize.saturating_sub(output.len()))]);
}

async fn close_audio_channel(channel: &mut russh::Channel<russh::client::Msg>) {
    if let Err(error) = channel.eof().await {
        tracing::warn!(%error, "cannot send microphone EOF");
    }
    let exited = timeout(Duration::from_secs(3), async {
        while let Some(message) = channel.wait().await {
            if matches!(message, ChannelMsg::ExitStatus { .. } | ChannelMsg::Close) {
                break;
            }
        }
    })
    .await;
    if exited.is_err() {
        tracing::warn!("remote microphone cleanup timed out");
    }
    if let Err(error) = channel.close().await {
        tracing::warn!(%error, "cannot close microphone channel");
    }
}

#[cfg(test)]
mod tests;
