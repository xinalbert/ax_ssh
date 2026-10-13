//! Shared transfer cancellation, pacing and upload concurrency.

use super::*;

static GLOBAL_UPLOAD_LIMITER: OnceLock<Arc<UploadLimiter>> = OnceLock::new();

/// Shared by all transfers in one SFTP connection. Zero rate is represented by None.
pub(crate) struct TransferRateLimiter {
    bytes_per_second: Option<u64>,
    next_slot: AsyncMutex<Instant>,
}

impl TransferRateLimiter {
    pub(crate) fn new(bytes_per_second: Option<u64>) -> Self {
        Self {
            bytes_per_second,
            next_slot: AsyncMutex::new(Instant::now()),
        }
    }

    pub(super) async fn reserve(
        &self,
        bytes: u64,
        cancellation: &TransferCancellation,
    ) -> Result<()> {
        let Some(rate) = self.bytes_per_second else {
            return Ok(());
        };
        let delay = Duration::from_secs_f64(bytes as f64 / rate as f64);
        let ready_at = {
            let mut next_slot = self.next_slot.lock().await;
            let ready_at = (*next_slot).max(Instant::now());
            *next_slot = ready_at + delay;
            ready_at
        };
        tokio::select! {
            _ = cancellation.cancelled() => Err(cancelled_error()),
            _ = tokio::time::sleep_until(ready_at) => Ok(()),
        }
    }
}

/// Update the process-wide cap for concurrently active SFTP uploads.
///
/// The new limit applies to uploads started after the update. Uploads that
/// are already active keep their permits until they finish.
pub fn configure_global_upload_limit(limit: usize) {
    global_upload_limiter().set_limit(limit);
}

pub(super) fn global_upload_limiter() -> &'static Arc<UploadLimiter> {
    GLOBAL_UPLOAD_LIMITER.get_or_init(|| {
        Arc::new(UploadLimiter::new(usize::from(
            crate::config::DEFAULT_SFTP_GLOBAL_UPLOADS,
        )))
    })
}

pub(super) struct UploadLimiter {
    active: AtomicUsize,
    limit: AtomicUsize,
}

impl UploadLimiter {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            active: AtomicUsize::new(0),
            limit: AtomicUsize::new(limit.max(1)),
        }
    }

    pub(super) fn set_limit(&self, limit: usize) {
        self.limit.store(limit.max(1), Ordering::Release);
    }

    pub(super) fn try_acquire(self: &Arc<Self>) -> Result<UploadPermit> {
        loop {
            let active = self.active.load(Ordering::Acquire);
            let limit = self.limit.load(Ordering::Acquire);
            if active >= limit {
                anyhow::bail!("the global SFTP upload limit has been reached");
            }
            if self
                .active
                .compare_exchange(active, active + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(UploadPermit {
                    limiter: Arc::clone(self),
                });
            }
        }
    }
}

pub(crate) struct UploadPermit {
    limiter: Arc<UploadLimiter>,
}

impl Drop for UploadPermit {
    fn drop(&mut self) {
        self.limiter.active.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Clone)]
pub(super) struct TransferCancellation {
    inner: Arc<TransferCancellationInner>,
}

struct TransferCancellationInner {
    cancelled: AtomicBool,
    paused: AtomicBool,
    notify: Notify,
    rate_limiter: Arc<TransferRateLimiter>,
}

impl TransferCancellation {
    #[cfg(test)]
    pub(super) fn new() -> Self {
        Self::with_limiter(Arc::new(TransferRateLimiter::new(None)))
    }

    pub(super) fn with_limiter(rate_limiter: Arc<TransferRateLimiter>) -> Self {
        Self {
            inner: Arc::new(TransferCancellationInner {
                cancelled: AtomicBool::new(false),
                paused: AtomicBool::new(false),
                notify: Notify::new(),
                rate_limiter,
            }),
        }
    }

    pub(super) async fn limit(&self, bytes: u64) -> Result<()> {
        self.inner.rate_limiter.reserve(bytes, self).await
    }

    pub(super) fn cancel(&self) {
        if !self.inner.cancelled.swap(true, Ordering::AcqRel) {
            self.inner.notify.notify_waiters();
        }
    }

    pub(super) fn pause(&self) {
        if !self.is_cancelled() && !self.inner.paused.swap(true, Ordering::AcqRel) {
            self.inner.notify.notify_waiters();
        }
    }

    pub(super) fn resume(&self) {
        if self.inner.paused.swap(false, Ordering::AcqRel) {
            self.inner.notify.notify_waiters();
        }
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Acquire)
    }

    pub(super) fn is_paused(&self) -> bool {
        self.inner.paused.load(Ordering::Acquire)
    }

    pub(super) async fn wait_until_running(&self) -> Result<()> {
        loop {
            if self.is_cancelled() {
                return Err(cancelled_error());
            }
            if !self.is_paused() {
                return Ok(());
            }
            let notified = self.inner.notify.notified();
            if self.is_cancelled() {
                return Err(cancelled_error());
            }
            if !self.is_paused() {
                return Ok(());
            }
            tokio::select! {
                _ = self.cancelled() => return Err(cancelled_error()),
                _ = notified => {}
            }
        }
    }

    pub(super) async fn cancelled(&self) {
        loop {
            let notified = self.inner.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

#[derive(Debug)]
struct TransferCancelled;

impl fmt::Display for TransferCancelled {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SFTP download cancelled")
    }
}

impl StdError for TransferCancelled {}

pub(super) fn cancelled_error() -> anyhow::Error {
    TransferCancelled.into()
}

pub(super) fn is_cancelled(error: &anyhow::Error) -> bool {
    error.downcast_ref::<TransferCancelled>().is_some()
}
