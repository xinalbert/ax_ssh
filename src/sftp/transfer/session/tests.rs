use super::*;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;

use russh_sftp::protocol::{Attrs, FileAttributes, StatusCode};
use tokio::io::DuplexStream;
use tokio::sync::OwnedSemaphorePermit;
use tokio::task::JoinHandle;
use tokio::time::Duration;

struct Peer {
    initializations: Arc<AtomicUsize>,
    generation: String,
}

impl russh_sftp::server::Handler for Peer {
    type Error = StatusCode;

    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _: u32,
        _: HashMap<String, String>,
    ) -> std::result::Result<Version, StatusCode> {
        self.initializations.fetch_add(1, Ordering::AcqRel);
        let mut version = Version::new();
        version
            .extensions
            .insert("fixture-generation".into(), self.generation.clone());
        Ok(version)
    }

    async fn lstat(&mut self, id: u32, _: String) -> std::result::Result<Attrs, StatusCode> {
        Ok(Attrs {
            id,
            attrs: FileAttributes::empty(),
        })
    }
}

struct AdmittedStream {
    stream: DuplexStream,
    _permit: OwnedSemaphorePermit,
}

impl AsyncRead for AdmittedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for AdmittedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

fn cache() -> SftpSessionCache {
    SftpSessionCache::new(Arc::new(Semaphore::new(6)))
}

async fn initialize(
    cache: &SftpSessionCache,
    initializations: Arc<AtomicUsize>,
    generation: &str,
) -> (SftpSession, JoinHandle<()>) {
    let permit = cache
        .0
        .slots
        .clone()
        .acquire_owned()
        .await
        .expect("admission");
    let (stream, mut bridge_client) = tokio::io::duplex(64 * 1024);
    let (mut bridge_server, peer) = tokio::io::duplex(64 * 1024);
    russh_sftp::server::run(
        peer,
        Peer {
            initializations,
            generation: generation.into(),
        },
    )
    .await;
    // The library server launcher returns before its detached I/O task ends.
    // Owning this bridge gives the fixture a real close/join/abort boundary.
    let server = tokio::spawn(async move {
        tokio::io::copy_bidirectional(&mut bridge_client, &mut bridge_server)
            .await
            .expect("fixture bridge");
    });
    (
        cache
            .initialize(AdmittedStream {
                stream,
                _permit: permit,
            })
            .await
            .expect("initialize session"),
        server,
    )
}

async fn joined(server: JoinHandle<()>) {
    timeout(Duration::from_secs(2), server)
        .await
        .expect("session should close")
        .expect("server joined");
}

#[tokio::test]
async fn sftp_session_cache_reuses_version_and_requests_without_a_second_init() {
    let cache = cache();
    let count = Arc::new(AtomicUsize::new(0));
    let (session, server) = initialize(&cache, count.clone(), "first").await;
    session
        .lstat("/fixture")
        .await
        .expect("first metadata request");
    session.recycle();
    let session = cache.take().expect("cache").expect("ready session");
    assert_eq!(session.version.extensions["fixture-generation"], "first");
    session
        .lstat("/fixture")
        .await
        .expect("second metadata request");
    assert_eq!(count.load(Ordering::Acquire), 1);
    drop(session);
    joined(server).await;
}

#[tokio::test]
async fn sftp_session_cache_discards_dead_idle_sessions_and_refreshes_capabilities() {
    let cache = cache();
    let count = Arc::new(AtomicUsize::new(0));
    let (session, server) = initialize(&cache, count.clone(), "old").await;
    let healthy = session.healthy.clone();
    session.recycle();
    server.abort();
    assert!(server.await.expect_err("stopped peer").is_cancelled());
    timeout(Duration::from_secs(2), async {
        while healthy.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("observe remote EOF");
    assert!(cache.take().expect("cache").is_none());
    let (session, server) = initialize(&cache, count.clone(), "new").await;
    assert_eq!(session.version.extensions["fixture-generation"], "new");
    assert_eq!(count.load(Ordering::Acquire), 2);
    drop(session);
    joined(server).await;
}

#[tokio::test]
async fn sftp_session_cache_bounds_idle_entries_and_clears_checked_out_leases_on_disconnect() {
    let cache = cache();
    let count = Arc::new(AtomicUsize::new(0));
    let mut servers = Vec::new();
    for _ in 0..3 {
        let (session, server) = initialize(&cache, count.clone(), "bounded").await;
        session.recycle();
        servers.push(server);
    }
    assert_eq!(cache.0.state.lock().expect("state").idle.len(), 2);
    let lease = cache.take().expect("cache").expect("session");
    cache.clear().expect("disconnect cache");
    lease.recycle();
    assert!(cache.0.state.lock().expect("state").idle.is_empty());
    assert!(cache.take().is_err());
    for server in servers {
        joined(server).await;
    }
    let released = timeout(
        Duration::from_secs(2),
        cache.0.slots.clone().acquire_many_owned(6),
    )
    .await
    .expect("all streams release admission")
    .expect("slots");
    drop(released);
    let new_connection_cache = self::cache();
    assert!(
        new_connection_cache
            .take()
            .expect("new connection cache")
            .is_none()
    );
}

#[tokio::test]
async fn sftp_session_cache_evicts_idle_and_drops_returns_when_admission_is_full() {
    let cache = cache();
    let count = Arc::new(AtomicUsize::new(0));
    let (session, server) = initialize(&cache, count.clone(), "idle").await;
    session.recycle();
    let held = cache
        .0
        .slots
        .clone()
        .acquire_many_owned(5)
        .await
        .expect("remaining slots");
    cache
        .evict_one()
        .expect("release idle session for a writer");
    let admitted = timeout(
        Duration::from_secs(2),
        cache.0.slots.clone().acquire_owned(),
    )
    .await
    .expect("writer admission")
    .expect("slot");
    joined(server).await;
    drop(admitted);
    let (active, server) = initialize(&cache, count, "active").await;
    let slots = cache.0.slots.clone();
    let waiter = tokio::spawn(async move { slots.acquire_owned().await });
    tokio::task::yield_now().await;
    assert!(!waiter.is_finished());
    active.recycle();
    let admitted = timeout(Duration::from_secs(2), waiter)
        .await
        .expect("return must unblock waiting opener")
        .expect("waiter joined")
        .expect("slot");
    assert!(cache.take().expect("cache").is_none());
    joined(server).await;
    drop((admitted, held));
}

#[tokio::test]
async fn sftp_session_cache_does_not_recycle_failed_handle_cleanup_or_aborted_leases() {
    let cache = cache();
    let count = Arc::new(AtomicUsize::new(0));
    let (session, server) = initialize(&cache, count.clone(), "cleanup").await;
    super::super::download::close_remote_handle(&session, "fixture".into()).await;
    session.recycle();
    assert!(cache.take().expect("cache").is_none());
    joined(server).await;

    let (session, server) = initialize(&cache, count, "cancel").await;
    let task = tokio::spawn(async move {
        let _lease = session;
        std::future::pending::<()>().await;
    });
    tokio::task::yield_now().await;
    task.abort();
    assert!(task.await.expect_err("cancelled lease").is_cancelled());
    assert!(cache.take().expect("cache").is_none());
    joined(server).await;
}

#[tokio::test]
async fn sftp_session_cache_cancelled_initialization_closes_stream_and_releases_admission() {
    let cache = cache();
    let permit = cache
        .0
        .slots
        .clone()
        .acquire_owned()
        .await
        .expect("admission");
    let (stream, mut silent_peer) = tokio::io::duplex(1024);
    let cache_for_task = cache.clone();
    let initializing = tokio::spawn(async move {
        cache_for_task
            .initialize(AdmittedStream {
                stream,
                _permit: permit,
            })
            .await
    });
    let peer = tokio::spawn(async move {
        tokio::io::copy(&mut silent_peer, &mut tokio::io::sink())
            .await
            .expect("silent peer EOF");
    });
    tokio::task::yield_now().await;
    initializing.abort();
    assert!(matches!(initializing.await, Err(error) if error.is_cancelled()));
    joined(peer).await;
    assert!(cache.take().expect("cache").is_none());
    let released = timeout(
        Duration::from_secs(2),
        cache.0.slots.clone().acquire_many_owned(6),
    )
    .await
    .expect("initialization releases permit")
    .expect("slots");
    drop(released);
}
