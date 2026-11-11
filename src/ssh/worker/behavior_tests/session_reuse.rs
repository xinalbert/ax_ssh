//! Worker-level session reuse retains fresh file metadata and strict SSH trust.

use super::*;

pub(super) async fn download_selected_file(
    fixture: &Fixture,
    worker: &SshSessionHandle,
    events: &mut mpsc::Receiver<SshSessionEvent>,
    destination: &str,
) -> PathBuf {
    let directory = fixture.directory.join(destination);
    std::fs::create_dir(&directory).expect("local directory");
    let id = Uuid::new_v4();
    worker
        .sftp_download_submitter()
        .submit(
            id,
            "/remote/file.txt".into(),
            directory.clone(),
            Vec::new(),
            false,
        )
        .await
        .expect("selected file accepted");
    until(events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Completed { transfer_id, .. }) if *transfer_id == id)).await;
    directory.join("file.txt")
}

#[tokio::test]
async fn sftp_selected_files_skip_discovery_and_reuse_one_initialized_session_with_fresh_sizes() {
    let fixture = Fixture::new(false).await;
    let (worker, mut events) = fixture.worker().await;
    until(&mut events, |event| {
        matches!(event, SshSessionEvent::Connected)
    })
    .await;
    let first = download_selected_file(&fixture, &worker, &mut events, "first").await;
    assert_eq!(std::fs::read(first).expect("first file"), CONTENT);
    assert_eq!(
        fixture.files.initializations.load(Ordering::Acquire),
        2,
        "one browser INIT and one transfer INIT, with no discovery INIT"
    );

    let changed = b"new size and fresh content";
    fixture
        .files
        .files
        .lock()
        .expect("files")
        .insert("/remote/file.txt".into(), changed.to_vec());
    let second = download_selected_file(&fixture, &worker, &mut events, "second").await;
    assert_eq!(std::fs::read(second).expect("updated file"), changed);
    assert_eq!(
        fixture.files.initializations.load(Ordering::Acquire),
        2,
        "second download must use the initialized session"
    );
    assert_eq!(fixture.files.authentications.load(Ordering::Acquire), 1);
    worker.shutdown().await.expect("shutdown");
    assert_no_active_sftp_channels(&fixture).await;
}

#[tokio::test]
async fn sftp_closed_cached_session_rebuilds_without_reauthentication() {
    let fixture = Fixture::new(false).await;
    let (worker, mut events) = fixture.worker().await;
    until(&mut events, |event| {
        matches!(event, SshSessionEvent::Connected)
    })
    .await;
    download_selected_file(&fixture, &worker, &mut events, "first").await;
    fixture.close_sftp_channel(1).await;
    let second = download_selected_file(&fixture, &worker, &mut events, "second").await;
    assert_eq!(std::fs::read(second).expect("rebuilt download"), CONTENT);
    assert_eq!(fixture.files.initializations.load(Ordering::Acquire), 3);
    assert_eq!(fixture.files.authentications.load(Ordering::Acquire), 1);
    worker.shutdown().await.expect("shutdown");
    assert_no_active_sftp_channels(&fixture).await;
}

#[test]
fn sftp_single_file_fast_path_preserves_filters_exclusions_and_directory_intents() {
    let directory = std::env::temp_dir();
    let file = SftpDownloadRoot::new(
        Uuid::new_v4(),
        "/remote/file.txt".into(),
        directory.clone(),
        Vec::new(),
        false,
    )
    .expect("file root");
    let request = file
        .single_file_request()
        .expect("safe local target")
        .expect("direct request");
    assert_eq!(request.remote_path(), "/remote/file.txt");
    assert_eq!(request.local_path(), Some(directory.join("file.txt")));
    let filtered = SftpDownloadRoot::new(
        Uuid::new_v4(),
        "/remote/file.txt".into(),
        directory.clone(),
        vec!["*.txt".into()],
        false,
    )
    .expect("filtered root");
    assert!(
        filtered
            .single_file_request()
            .expect("filter check")
            .is_none()
    );
    let excluded = SftpDownloadRoot::new_with_existing_directories(
        Uuid::new_v4(),
        "/remote/file.txt".into(),
        directory.clone(),
        Vec::new(),
        false,
        HashSet::from(["/remote/file.txt".into()]),
    )
    .expect("excluded root");
    assert!(
        excluded
            .single_file_request()
            .expect("exclusion check")
            .is_none()
    );
    let folder = SftpDownloadRoot::new(
        Uuid::new_v4(),
        "/remote".into(),
        directory,
        Vec::new(),
        true,
    )
    .expect("directory root");
    assert!(
        folder
            .single_file_request()
            .expect("directory scan")
            .is_none()
    );
}

#[tokio::test]
async fn sftp_idle_cache_yields_to_ordered_writes_under_a_lower_server_session_limit() {
    let fixture = Fixture::new(false).await;
    let (worker, mut events) = fixture.worker().await;
    until(&mut events, |event| {
        matches!(event, SshSessionEvent::Connected)
    })
    .await;
    download_selected_file(&fixture, &worker, &mut events, "first").await;
    // Browser plus one idle transfer already occupy this server's limit.
    fixture.files.channel_limit.store(2, Ordering::Release);
    let operation = Uuid::new_v4();
    worker
        .request_sftp_write(
            operation,
            SftpWriteOperation::Rename {
                old_path: "/remote/file.txt".into(),
                new_path: "/remote/renamed.txt".into(),
            },
        )
        .expect("ordered write");
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpWrite(SftpWriteEvent::Completed { operation_id, .. }) if *operation_id == operation)).await;
    assert!(fixture.files.rejected_channels.load(Ordering::Acquire) > 0);
    assert!(
        fixture
            .files
            .files
            .lock()
            .expect("files")
            .contains_key("/remote/renamed.txt")
    );
    assert_eq!(fixture.files.authentications.load(Ordering::Acquire), 1);
    worker.shutdown().await.expect("shutdown");
    assert_no_active_sftp_channels(&fixture).await;
}
