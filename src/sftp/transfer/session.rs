//! Initialized sessions belong to one authenticated transport, never a profile.

use std::io;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::task::{Context as TaskContext, Poll};

use anyhow::{Context, Result};
use russh_sftp::client::{Config, RawSftpSession};
use russh_sftp::protocol::Version;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::Semaphore;
use tokio::time::{Instant, timeout};
use tracing::{debug, warn};

use crate::sftp::{MAX_PACKET_BYTES, PacketLimitedStream, REQUEST_TIMEOUT};

const MAX_IDLE_SESSIONS: usize = 2;

#[derive(Clone)]
pub(crate) struct SftpSessionCache(Arc<CacheInner>);

struct CacheInner {
    state: Mutex<CacheState>,
    slots: Arc<Semaphore>,
}

#[derive(Default)]
struct CacheState {
    idle: Vec<SftpSession>,
    closed: bool,
}

impl SftpSessionCache {
    pub(crate) fn new(slots: Arc<Semaphore>) -> Self {
        Self(Arc::new(CacheInner {
            state: Mutex::new(CacheState::default()),
            slots,
        }))
    }

    pub(crate) fn take(&self) -> Result<Option<SftpSession>> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("SFTP session cache lock poisoned"))?;
        if state.closed {
            anyhow::bail!("SFTP session cache is disconnected");
        }
        while let Some(mut session) = state.idle.pop() {
            if session.healthy.load(Ordering::Acquire) {
                session.reused = true;
                debug!(
                    version = session.version.version,
                    extensions = session.version.extensions.len(),
                    "reusing initialized SFTP session"
                );
                return Ok(Some(session));
            }
        }
        Ok(None)
    }

    pub(crate) async fn initialize<S>(&self, stream: S) -> Result<SftpSession>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let mut session = SftpSession::initialize(stream).await?;
        session.cache = Arc::downgrade(&self.0);
        Ok(session)
    }

    /// Releasing one idle stream makes room for a fresh upload/write/channel.
    pub(crate) fn evict_one(&self) -> Result<()> {
        let session = self
            .0
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("SFTP session cache lock poisoned"))?
            .idle
            .pop();
        drop(session);
        Ok(())
    }

    pub(crate) fn clear(&self) -> Result<()> {
        let idle = {
            let mut state = self
                .0
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("SFTP session cache lock poisoned"))?;
            state.closed = true;
            std::mem::take(&mut state.idle)
        };
        drop(idle);
        Ok(())
    }
}

pub(crate) struct SftpSession {
    raw: RawSftpSession,
    version: Version,
    healthy: Arc<AtomicBool>,
    cache: Weak<CacheInner>,
    reused: bool,
}

impl SftpSession {
    pub(crate) async fn initialize<S>(stream: S) -> Result<Self>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let started = Instant::now();
        let healthy = Arc::new(AtomicBool::new(true));
        let raw = RawSftpSession::new_with_config(
            PacketLimitedStream::new(ObservedStream {
                stream,
                healthy: healthy.clone(),
            }),
            Config {
                max_packet_len: MAX_PACKET_BYTES,
                max_concurrent_reads: 16,
                max_concurrent_writes: 1,
                max_write_packet_len: MAX_PACKET_BYTES,
                request_timeout_secs: REQUEST_TIMEOUT.as_secs(),
            },
        );
        let version = timeout(REQUEST_TIMEOUT, raw.init())
            .await
            .context("SFTP handshake timed out")?
            .context("SFTP handshake failed")?;
        debug!(
            version = version.version,
            extensions = version.extensions.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "SFTP session initialized"
        );
        Ok(Self {
            raw,
            version,
            healthy,
            cache: Weak::new(),
            reused: false,
        })
    }

    pub(super) fn invalidate(&self) {
        self.healthy.store(false, Ordering::Release);
    }

    pub(crate) fn was_reused(&self) -> bool {
        self.reused
    }

    /// Call only after success and after all remote handles have been closed.
    /// Errors and cancellation simply drop the lease, closing its subsystem.
    pub(super) fn recycle(self) {
        let Some(cache) = self.cache.upgrade() else {
            return;
        };
        if !self.healthy.load(Ordering::Acquire) {
            return;
        }
        let Ok(mut state) = cache.state.lock() else {
            warn!("cannot return SFTP session: cache lock poisoned");
            return;
        };
        // A waiting opener needs a released permit. Returning an idle session
        // while all admission is held could otherwise strand that opener.
        if !state.closed
            && state.idle.len() < MAX_IDLE_SESSIONS
            && cache.slots.available_permits() > 0
        {
            state.idle.push(self);
        }
    }
}

impl Deref for SftpSession {
    type Target = RawSftpSession;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

struct ObservedStream<S> {
    stream: S,
    healthy: Arc<AtomicBool>,
}

impl<S> Drop for ObservedStream<S> {
    fn drop(&mut self) {
        self.healthy.store(false, Ordering::Release);
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for ObservedStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let had_capacity = buf.remaining() > 0;
        let result = Pin::new(&mut self.stream).poll_read(cx, buf);
        if matches!(&result, Poll::Ready(Err(_)))
            || (had_capacity
                && matches!(&result, Poll::Ready(Ok(())))
                && buf.filled().len() == before)
        {
            self.healthy.store(false, Ordering::Release);
        }
        result
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for ObservedStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.stream).poll_write(cx, buf);
        if matches!(&result, Poll::Ready(Err(_)))
            || (!buf.is_empty() && matches!(&result, Poll::Ready(Ok(0))))
        {
            self.healthy.store(false, Ordering::Release);
        }
        result
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        let result = Pin::new(&mut self.stream).poll_flush(cx);
        if matches!(&result, Poll::Ready(Err(_))) {
            self.healthy.store(false, Ordering::Release);
        }
        result
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        self.healthy.store(false, Ordering::Release);
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests;
