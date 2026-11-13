//! Transfer controls use pinned loopback hosts and synthetic local data only.
use super::*;
use crate::sftp::SftpTransferAction;

async fn control_ack(events: &mut mpsc::Receiver<SshSessionEvent>, id: Uuid, paused: bool) {
    until(events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::ControlState { transfer_id, paused: actual, .. }) if *transfer_id == id && *actual == paused)).await;
}

pub(super) async fn barrier(
    worker: &SshSessionHandle,
    events: &mut mpsc::Receiver<SshSessionEvent>,
) {
    worker
        .request_list_sftp(701, "/remote".into())
        .expect("barrier");
    until(events, |event| {
        matches!(
            event,
            SshSessionEvent::Sftp(SftpBrowserEvent::DirectoryPage {
                request_id: Some(701),
                ..
            })
        )
    })
    .await;
}

#[tokio::test]
async fn sftp_pause_all_covers_new_downloads_and_uploads_and_isolates_other_workers() {
    let fixture = Fixture::new(false).await;
    let (worker, mut events) = fixture.worker().await;
    until(&mut events, |event| {
        matches!(event, SshSessionEvent::Connected)
    })
    .await;
    worker
        .request_sftp_transfer_action(SftpTransferAction::Pause, true, Vec::new())
        .expect("pause page");
    barrier(&worker, &mut events).await;
    let download = Uuid::new_v4();
    worker
        .sftp_download_submitter()
        .submit(
            download,
            "/remote/file.txt".into(),
            fixture.directory.clone(),
            Vec::new(),
            false,
        )
        .await
        .expect("queued download");
    let source = fixture.directory.join("source.bin");
    std::fs::write(&source, CONTENT).expect("source");
    let upload = Uuid::new_v4();
    worker
        .sftp_upload_submitter()
        .submit(
            Uuid::new_v4(),
            vec![(
                upload,
                "/remote/new.bin".into(),
                source,
                CONTENT.len() as u64,
                Vec::new(),
            )],
        )
        .await
        .expect("queued upload");
    barrier(&worker, &mut events).await;
    assert_eq!(
        fixture.files.initializations.load(Ordering::Acquire),
        1,
        "paused arrivals must not open transfer sessions"
    );
    assert!(!fixture.directory.join("file.txt").exists());
    let other = Fixture::new(false).await;
    let (other_worker, mut other_events) = other.worker().await;
    until(&mut other_events, |event| {
        matches!(event, SshSessionEvent::Connected)
    })
    .await;
    session_reuse::download_selected_file(&other, &other_worker, &mut other_events, "unaffected")
        .await;
    other_worker.shutdown().await.expect("other shutdown");
    worker
        .request_sftp_transfer_action(SftpTransferAction::Resume, true, Vec::new())
        .expect("resume page");
    let mut completed = HashSet::new();
    while completed.len() < 2 {
        let event = until(&mut events, |event| {
            matches!(
                event,
                SshSessionEvent::SftpTransfer(SftpTransferEvent::Completed { .. })
            )
        })
        .await;
        if let SshSessionEvent::SftpTransfer(SftpTransferEvent::Completed { transfer_id, .. }) =
            event
        {
            completed.insert(transfer_id);
        }
    }
    assert_eq!(completed, HashSet::from([download, upload]));
    assert_eq!(
        std::fs::read(fixture.directory.join("file.txt")).expect("download bytes"),
        CONTENT
    );
    assert_eq!(
        fixture.files.files.lock().expect("files")["/remote/new.bin"],
        CONTENT
    );
    worker.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn sftp_prestart_pause_resume_and_late_cancel_do_not_open_channels() {
    let fixture = Fixture::new(false).await;
    let (worker, mut events) = fixture.worker().await;
    until(&mut events, |event| {
        matches!(event, SshSessionEvent::Connected)
    })
    .await;
    let id = Uuid::new_v4();
    worker
        .request_sftp_transfer_action(SftpTransferAction::Pause, false, vec![id])
        .expect("prestart pause");
    control_ack(&mut events, id, true).await;
    worker
        .sftp_download_submitter()
        .submit(
            id,
            "/remote/file.txt".into(),
            fixture.directory.clone(),
            Vec::new(),
            false,
        )
        .await
        .expect("paused request");
    barrier(&worker, &mut events).await;
    assert_eq!(fixture.files.initializations.load(Ordering::Acquire), 1);
    worker
        .request_resume_sftp_transfer(id)
        .expect("resume queued task");
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Completed { transfer_id, .. }) if *transfer_id == id)).await;
    let late = Uuid::new_v4();
    worker
        .request_sftp_transfer_action(SftpTransferAction::Cancel, true, vec![late])
        .expect("cancel all includes a not-yet-submitted row");
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Cancelled { transfer_id }) if *transfer_id == late)).await;
    worker
        .sftp_download_submitter()
        .submit(
            late,
            "/remote/slow.bin".into(),
            fixture.directory.clone(),
            Vec::new(),
            false,
        )
        .await
        .expect("late submission consumed");
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Cancelled { transfer_id }) if *transfer_id == late)).await;
    assert!(!fixture.directory.join("slow.bin").exists());
    session_reuse::download_selected_file(&fixture, &worker, &mut events, "after-cancel").await;
    worker.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn sftp_cancel_during_paused_handshake_releases_channel() {
    let fixture = Fixture::new(false).await;
    let (worker, mut events) = fixture.worker().await;
    until(&mut events, |event| {
        matches!(event, SshSessionEvent::Connected)
    })
    .await;
    // Discard the browser's notification before observing the file channel.
    let _ = timeout(
        Duration::from_millis(1),
        fixture.files.subsystem_requested.notified(),
    )
    .await;
    fixture.files.hold_subsystems.store(true, Ordering::Release);
    let id = Uuid::new_v4();
    worker
        .sftp_download_submitter()
        .submit(
            id,
            "/remote/file.txt".into(),
            fixture.directory.clone(),
            Vec::new(),
            false,
        )
        .await
        .expect("file accepted");
    timeout(
        Duration::from_secs(5),
        fixture.files.subsystem_requested.notified(),
    )
    .await
    .expect("file handshake started");
    worker
        .request_pause_sftp_transfer(id)
        .expect("pause handshake");
    control_ack(&mut events, id, true).await;
    worker
        .request_cancel_sftp_transfer(id)
        .expect("cancel paused handshake");
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Cancelled { transfer_id }) if *transfer_id == id)).await;
    assert!(!fixture.directory.join("file.txt").exists());
    worker.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn sftp_directory_pause_cascades_to_children_then_cancel_all_finishes_root_as_cancelled() {
    let fixture = Fixture::new(false).await;
    let (worker, mut events) = fixture.worker().await;
    until(&mut events, |event| {
        matches!(event, SshSessionEvent::Connected)
    })
    .await;
    let root = Uuid::new_v4();
    worker
        .sftp_download_submitter()
        .submit(
            root,
            "/remote".into(),
            fixture.directory.clone(),
            Vec::new(),
            true,
        )
        .await
        .expect("directory accepted");
    timeout(
        Duration::from_secs(5),
        fixture.files.read_started.notified(),
    )
    .await
    .expect("directory child is in flight");
    worker
        .request_pause_sftp_transfer(root)
        .expect("pause directory");
    control_ack(&mut events, root, true).await;
    fixture.files.read_gate.add_permits(64);
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(
        !fixture.directory.join("remote/slow.bin").exists(),
        "paused directory cannot publish its file"
    );
    worker
        .request_sftp_transfer_action(SftpTransferAction::Cancel, true, vec![root])
        .expect("cancel page");
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::DiscoveryFinished { transfer_id, cancelled: true, .. }) if *transfer_id == root)).await;
    assert!(!fixture.directory.join("remote/slow.bin").exists());
    worker.shutdown().await.expect("shutdown");
}

async fn paused_upload_fixture() -> (
    Fixture,
    SshSessionHandle,
    mpsc::Receiver<SshSessionEvent>,
    Uuid,
    Vec<u8>,
) {
    let fixture = Fixture::new(false).await;
    let (worker, mut events) = fixture.worker().await;
    until(&mut events, |event| {
        matches!(event, SshSessionEvent::Connected)
    })
    .await;
    fixture.files.hold_writes.store(true, Ordering::Release);
    let bytes = (0..2 * 1024 * 1024)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let local = fixture.directory.join("upload-source.bin");
    std::fs::write(&local, &bytes).expect("synthetic source");
    let id = Uuid::new_v4();
    worker
        .sftp_upload_submitter()
        .submit(
            Uuid::new_v4(),
            vec![(
                id,
                "/remote/control-upload.bin".into(),
                local,
                bytes.len() as u64,
                Vec::new(),
            )],
        )
        .await
        .expect("upload accepted");
    timeout(
        Duration::from_secs(5),
        fixture.files.write_started.notified(),
    )
    .await
    .expect("upload is in flight");
    worker
        .request_pause_sftp_transfer(id)
        .expect("pause upload");
    control_ack(&mut events, id, true).await;
    fixture.files.write_gate.add_permits(16);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        fixture.files.completed_writes.load(Ordering::Acquire) <= 16,
        "in-flight writes remain bounded while paused"
    );
    assert!(
        !fixture
            .files
            .files
            .lock()
            .expect("files")
            .contains_key("/remote/control-upload.bin")
    );
    (fixture, worker, events, id, bytes)
}

#[tokio::test]
async fn sftp_upload_pause_resume_preserves_bytes_and_blocks_publication() {
    let (fixture, worker, mut events, id, bytes) = paused_upload_fixture().await;
    worker
        .request_resume_sftp_transfer(id)
        .expect("resume upload");
    fixture.files.write_gate.add_permits(256);
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Completed { transfer_id, .. }) if *transfer_id == id)).await;
    assert_eq!(
        fixture.files.files.lock().expect("remote bytes")["/remote/control-upload.bin"],
        bytes
    );
    worker.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn sftp_upload_cancel_while_paused_removes_temporary_and_never_publishes() {
    let (fixture, worker, mut events, id, _) = paused_upload_fixture().await;
    // Let already-sent writes acknowledge before cancellation cleans up.
    fixture.files.write_gate.add_permits(256);
    worker
        .request_cancel_sftp_transfer(id)
        .expect("cancel upload");
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Cancelled { transfer_id }) if *transfer_id == id)).await;
    assert!(
        fixture
            .files
            .files
            .lock()
            .expect("remote files")
            .keys()
            .all(|path| !path.contains("control-upload")),
        "temporary and final upload paths are absent"
    );
    worker.shutdown().await.expect("shutdown");
}
