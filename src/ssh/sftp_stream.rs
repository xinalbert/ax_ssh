//! Connection-owned background SFTP admission; a permit lives with its stream.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::{Context as _, Result};
use russh::{Channel, ChannelMsg, ChannelOpenFailure, ChannelStream, client};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::OwnedSemaphorePermit;
use tokio::time::{Duration, Instant, timeout_at};
use tracing::{debug, warn};

// Four file transfers, one discovery and one ordered write. The browser and
// best-effort release probe have separate headroom below OpenSSH's default 10.
// Servers may impose a lower limit; a refusal is still only a channel failure.
pub(super) const BACKGROUND_CHANNELS: usize = 6;
pub(super) const TRANSFER_CHANNELS: usize = BACKGROUND_CHANNELS - 2;

// Raw Channel does not close on drop; ChannelStream does. Preserve that close
// behavior while waiting for confirmation, including cancellation by the owner.
struct PendingSubsystem(Option<Channel<client::Msg>>);

impl Drop for PendingSubsystem {
    fn drop(&mut self) {
        if let Some(channel) = self.0.take() {
            drop(channel.into_stream());
        }
    }
}

pub(super) async fn request_subsystem(
    channel: Channel<client::Msg>,
    request_timeout: Duration,
) -> Result<ChannelStream<client::Msg>> {
    let channel_id = channel.id();
    let started = Instant::now();
    let deadline = started + request_timeout;
    let mut pending = PendingSubsystem(Some(channel));
    let result = async {
        let channel = pending
            .0
            .as_mut()
            .context("SFTP startup channel is missing")?;
        // russh only queues this request; Ok does not mean the server accepted it.
        timeout_at(deadline, channel.request_subsystem(true, "sftp"))
            .await
            .context("timed out sending the SFTP subsystem request")?
            .context("cannot send the SFTP subsystem request")?;
        debug!(%channel_id, "SFTP subsystem request sent; awaiting confirmation");
        timeout_at(deadline, async {
            loop {
                match channel.wait().await {
                    Some(ChannelMsg::Success) => return Ok(()),
                    Some(ChannelMsg::Failure) => {
                        anyhow::bail!("server rejected the SFTP subsystem");
                    }
                    Some(ChannelMsg::Eof | ChannelMsg::Close) | None => {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "SFTP channel closed before subsystem confirmation",
                        )
                        .into());
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        anyhow::bail!(
                            "SFTP subsystem exited before confirmation (status {exit_status})"
                        );
                    }
                    // stderr is not SFTP protocol data and may contain private
                    // server output. Do not copy its contents into errors/logs.
                    Some(ChannelMsg::ExtendedData { .. } | ChannelMsg::WindowAdjusted { .. }) => {}
                    Some(_) => {
                        anyhow::bail!("unexpected SSH message before SFTP subsystem confirmation")
                    }
                }
            }
        })
        .await
        .context("timed out waiting for SFTP subsystem confirmation")?
    }
    .await;
    if let Err(error) = result {
        warn!(%channel_id, elapsed_ms = started.elapsed().as_millis(), %error, "SFTP subsystem startup failed");
        return Err(error);
    }
    debug!(%channel_id, elapsed_ms = started.elapsed().as_millis(), "SFTP subsystem confirmed");
    Ok(pending
        .0
        .take()
        .context("confirmed SFTP channel is missing")?
        .into_stream())
}

pub(super) struct SftpStream {
    // Drop the SSH stream (which sends Close) before releasing admission.
    stream: ChannelStream<client::Msg>,
    _permit: OwnedSemaphorePermit,
}

impl SftpStream {
    pub(super) fn new(stream: ChannelStream<client::Msg>, permit: OwnedSemaphorePermit) -> Self {
        Self {
            stream,
            _permit: permit,
        }
    }
}

impl AsyncRead for SftpStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for SftpStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

pub(super) fn channel_open_can_retry(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<russh::Error>(),
            Some(russh::Error::ChannelOpenFailure(
                ChannelOpenFailure::AdministrativelyProhibited
                    | ChannelOpenFailure::ResourceShortage
            ))
        )
    })
}
