//! Connection-owned background SFTP admission; a permit lives with its stream.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use russh::{ChannelOpenFailure, ChannelStream, client};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::OwnedSemaphorePermit;

// Four file transfers, one discovery and one ordered write. The browser and
// best-effort release probe have separate headroom below OpenSSH's default 10.
// Servers may impose a lower limit; a refusal is still only a channel failure.
pub(super) const BACKGROUND_CHANNELS: usize = 6;
pub(super) const TRANSFER_CHANNELS: usize = BACKGROUND_CHANNELS - 2;

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
