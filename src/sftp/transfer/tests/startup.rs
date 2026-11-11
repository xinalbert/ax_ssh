//! Initialization retries stop before discovery can publish any child requests.

use super::*;
use tokio::io::DuplexStream;
use tokio::task::JoinSet;

fn discovery_root(local: &TestCacheDir) -> SftpDownloadRoot {
    SftpDownloadRoot::new(
        Uuid::new_v4(),
        "/srv/root".to_owned(),
        local.path().to_owned(),
        Vec::new(),
        true,
    )
    .expect("valid discovery root")
}

async fn join_servers(servers: &mut JoinSet<()>) {
    timeout(REQUEST_TIMEOUT, async {
        while let Some(result) = servers.join_next().await {
            result.expect("test server should join");
        }
    })
    .await
    .expect("all discovery streams should close");
}

#[tokio::test(start_paused = true)]
async fn discovery_handshake_timeout_reopens_a_fresh_stream_and_recovers() {
    let local = TestCacheDir::new();
    fs::create_dir(local.path()).expect("local directory");
    let root = discovery_root(&local);
    let (discovered_tx, mut discovered_rx) = mpsc::channel(4);
    let mut servers = JoinSet::new();
    let mut attempts = 0;
    let result = discover_download_requests(
        || {
            attempts += 1;
            let (client, mut server) = tokio::io::duplex(64 * 1024);
            if attempts == 1 {
                servers.spawn(async move {
                    tokio::io::copy(&mut server, &mut tokio::io::sink())
                        .await
                        .expect("silent peer should observe stream close");
                });
            } else {
                servers.spawn(russh_sftp::server::run(
                    server,
                    DirectoryDiscoveryServer {
                        directories: HashMap::from([("/srv/root".to_owned(), Vec::new())]),
                    },
                ));
            }
            std::future::ready(Ok(client))
        },
        root,
        discovered_tx,
    )
    .await
    .expect("second handshake should recover");
    assert_eq!(attempts, 2);
    assert_eq!(result.discovered_files, 0);
    assert!(local.path().join("root").is_dir());
    assert!(discovered_rx.recv().await.is_none());
    join_servers(&mut servers).await;
}

#[tokio::test(start_paused = true)]
async fn discovery_startup_exhausts_three_attempts_with_one_and_two_second_backoff() {
    let local = TestCacheDir::new();
    let (discovered_tx, mut discovered_rx) = mpsc::channel(4);
    let started = Instant::now();
    let mut attempts = Vec::new();
    let error = discover_download_requests(
        || {
            attempts.push(started.elapsed());
            std::future::ready(Err::<DuplexStream, _>(
                io::Error::from(io::ErrorKind::TimedOut).into(),
            ))
        },
        discovery_root(&local),
        discovered_tx,
    )
    .await
    .expect_err("transient failures should exhaust their budget");
    assert!(transfer_error_is_recoverable(&error));
    assert_eq!(
        attempts,
        [
            Duration::ZERO,
            Duration::from_secs(1),
            Duration::from_secs(3)
        ]
    );
    assert!(discovered_rx.recv().await.is_none());
    assert!(!local.path().exists());
}

#[tokio::test(start_paused = true)]
async fn discovery_does_not_retry_explicit_subsystem_rejection() {
    let local = TestCacheDir::new();
    let (discovered_tx, _) = mpsc::channel(4);
    let mut attempts = 0;
    let error = discover_download_requests(
        || {
            attempts += 1;
            std::future::ready(Err::<DuplexStream, _>(anyhow::anyhow!(
                "server rejected the SFTP subsystem"
            )))
        },
        discovery_root(&local),
        discovered_tx,
    )
    .await
    .expect_err("explicit rejection should fail immediately");
    assert_eq!(attempts, 1);
    assert!(!transfer_error_is_recoverable(&error));
}

#[tokio::test(start_paused = true)]
async fn discovery_cancellation_during_backoff_does_not_reopen() {
    let local = TestCacheDir::new();
    let root = discovery_root(&local);
    let (discovered_tx, _) = mpsc::channel(4);
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_for_task = attempts.clone();
    let discovery = tokio::spawn(discover_download_requests(
        move || {
            attempts_for_task.fetch_add(1, Ordering::AcqRel);
            std::future::ready(Err::<DuplexStream, _>(
                io::Error::from(io::ErrorKind::TimedOut).into(),
            ))
        },
        root,
        discovered_tx,
    ));
    tokio::task::yield_now().await;
    assert_eq!(attempts.load(Ordering::Acquire), 1);
    discovery.abort();
    assert!(
        discovery
            .await
            .expect_err("cancelled discovery")
            .is_cancelled()
    );
    tokio::time::advance(Duration::from_secs(5)).await;
    assert_eq!(attempts.load(Ordering::Acquire), 1);
    assert!(!local.path().exists());
}

struct StalledMetadataServer;

impl russh_sftp::server::Handler for StalledMetadataServer {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn lstat(&mut self, _: u32, _: String) -> std::result::Result<Attrs, Self::Error> {
        std::future::pending().await
    }
}

#[tokio::test(start_paused = true)]
async fn discovery_does_not_replay_after_a_successful_handshake() {
    let local = TestCacheDir::new();
    let (discovered_tx, mut discovered_rx) = mpsc::channel(4);
    let mut servers = JoinSet::new();
    let mut attempts = 0;
    let error = discover_download_requests(
        || {
            attempts += 1;
            let (client, server) = tokio::io::duplex(64 * 1024);
            servers.spawn(russh_sftp::server::run(server, StalledMetadataServer));
            std::future::ready(Ok(client))
        },
        discovery_root(&local),
        discovered_tx,
    )
    .await
    .expect_err("metadata request should time out");
    assert!(transfer_error_is_recoverable(&error));
    assert!(format!("{error:#}").contains("inspect"), "{error:#}");
    assert_eq!(attempts, 1, "discovery must not replay once initialized");
    assert!(discovered_rx.recv().await.is_none());
    assert!(!local.path().exists());
    // This peer deliberately blocks inside its metadata handler. Its owner
    // aborts and joins it instead of leaving a detached test task.
    servers.shutdown().await;
}
