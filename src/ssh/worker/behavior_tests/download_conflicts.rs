//! Real worker decisions against a pinned loopback peer and disposable files.

use super::*;
use crate::sftp::SftpConflictChoice;

async fn conflict(events: &mut mpsc::Receiver<SshSessionEvent>, id: Uuid) {
    until(events, |event| {
        matches!(event,
        SshSessionEvent::SftpTransfer(SftpTransferEvent::DownloadConflict { transfer_id, .. })
        if *transfer_id == id)
    })
    .await;
}

#[tokio::test]
async fn sftp_download_conflicts_support_three_choices_and_cancel() {
    let fixture = Fixture::new(false).await;
    let (worker, mut events) = fixture.worker().await;
    until(&mut events, |event| {
        matches!(event, SshSessionEvent::Connected)
    })
    .await;
    for choice in [
        SftpConflictChoice::Skip,
        SftpConflictChoice::KeepBoth,
        SftpConflictChoice::Overwrite,
    ] {
        let directory = fixture.directory.join(format!("{choice:?}"));
        std::fs::create_dir(&directory).expect("destination");
        let path = directory.join("file.txt");
        std::fs::write(&path, b"original").expect("original");
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
            .expect("submit");
        conflict(&mut events, id).await;
        assert_eq!(
            std::fs::read(&path).expect("waiting preserves original"),
            b"original"
        );
        worker
            .request_resolve_sftp_conflict(id, choice, false)
            .expect("decision");
        let event = until(&mut events, |event| matches!(event,
            SshSessionEvent::SftpTransfer(SftpTransferEvent::Completed { transfer_id, .. } | SftpTransferEvent::Skipped { transfer_id })
            if *transfer_id == id)).await;
        if choice == SftpConflictChoice::Skip {
            assert!(matches!(
                event,
                SshSessionEvent::SftpTransfer(SftpTransferEvent::Skipped { .. })
            ));
        } else {
            let target = if choice == SftpConflictChoice::KeepBoth {
                directory.join("file (1).txt")
            } else {
                path.clone()
            };
            assert_eq!(std::fs::read(target).expect("download"), CONTENT);
        }
        if choice != SftpConflictChoice::Overwrite {
            assert_eq!(std::fs::read(path).expect("original retained"), b"original");
        }
        assert!(std::fs::read_dir(directory).expect("files").all(|entry| {
            !entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .ends_with(".part")
        }));
    }
    let path = fixture.directory.join("file.txt");
    std::fs::write(&path, b"before prompt").expect("original");
    let changed = Uuid::new_v4();
    worker
        .sftp_download_submitter()
        .submit(
            changed,
            "/remote/file.txt".into(),
            fixture.directory.clone(),
            Vec::new(),
            false,
        )
        .await
        .expect("submit");
    conflict(&mut events, changed).await;
    std::fs::write(&path, b"changed while deciding").expect("local edit");
    worker
        .request_resolve_sftp_conflict(changed, SftpConflictChoice::Overwrite, false)
        .expect("decision");
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Failed { transfer_id, .. }) if *transfer_id == changed)).await;
    assert_eq!(
        std::fs::read(&path).expect("local edits retained"),
        b"changed while deciding"
    );
    std::fs::write(&path, b"cancelled original").expect("original");
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
        .expect("submit");
    conflict(&mut events, id).await;
    worker
        .request_cancel_sftp_transfer(id)
        .expect("cancel waiting decision");
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Cancelled { transfer_id }) if *transfer_id == id)).await;
    worker
        .request_resolve_sftp_conflict(id, SftpConflictChoice::Overwrite, true)
        .expect("late choice ignored");
    transfer_controls::barrier(&worker, &mut events).await;
    assert_eq!(
        std::fs::read(path).expect("original retained"),
        b"cancelled original"
    );
    worker.shutdown().await.expect("shutdown");
    assert_no_active_sftp_channels(&fixture).await;
}

#[tokio::test]
async fn sftp_download_batch_decision_covers_pending_and_future_files_only() {
    let fixture = Fixture::new(false).await;
    for name in ["second.txt", "third.txt", "fourth.txt"] {
        fixture
            .files
            .files
            .lock()
            .expect("remote files")
            .insert(format!("/remote/{name}"), CONTENT.to_vec());
    }
    for name in ["file.txt", "second.txt", "third.txt", "fourth.txt"] {
        std::fs::write(fixture.directory.join(name), b"original").expect("original");
    }
    let (worker, mut events) = fixture.worker().await;
    until(&mut events, |event| {
        matches!(event, SshSessionEvent::Connected)
    })
    .await;
    let batch_id = Uuid::new_v4();
    let submitter = worker.sftp_download_submitter().for_batch(Some(batch_id));
    let ids = [Uuid::new_v4(), Uuid::new_v4()];
    for (id, name) in ids.into_iter().zip(["file.txt", "second.txt"]) {
        submitter
            .submit(
                id,
                format!("/remote/{name}"),
                fixture.directory.clone(),
                Vec::new(),
                false,
            )
            .await
            .expect("submit");
    }
    let mut pending = HashSet::new();
    while pending.len() < 2 {
        let event = until(&mut events, |event| {
            matches!(
                event,
                SshSessionEvent::SftpTransfer(SftpTransferEvent::DownloadConflict { .. })
            )
        })
        .await;
        if let SshSessionEvent::SftpTransfer(SftpTransferEvent::DownloadConflict {
            transfer_id,
            batch_id: actual,
            ..
        }) = event
        {
            assert_eq!(actual, batch_id);
            pending.insert(transfer_id);
        }
    }
    worker
        .request_resolve_sftp_conflict(ids[0], SftpConflictChoice::Skip, true)
        .expect("batch decision");
    let mut skipped = HashSet::new();
    while skipped.len() < 2 {
        let event = until(&mut events, |event| {
            matches!(
                event,
                SshSessionEvent::SftpTransfer(SftpTransferEvent::Skipped { .. })
            )
        })
        .await;
        if let SshSessionEvent::SftpTransfer(SftpTransferEvent::Skipped { transfer_id }) = event {
            skipped.insert(transfer_id);
        }
    }
    assert_eq!(skipped, HashSet::from(ids));
    let third = Uuid::new_v4();
    submitter
        .submit(
            third,
            "/remote/third.txt".into(),
            fixture.directory.clone(),
            Vec::new(),
            false,
        )
        .await
        .expect("later file");
    let event = until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(
        SftpTransferEvent::Skipped { transfer_id } | SftpTransferEvent::DownloadConflict { transfer_id, .. }
    ) if *transfer_id == third)).await;
    assert!(matches!(
        event,
        SshSessionEvent::SftpTransfer(SftpTransferEvent::Skipped { .. })
    ));
    let fourth = Uuid::new_v4();
    worker
        .sftp_download_submitter()
        .submit(
            fourth,
            "/remote/fourth.txt".into(),
            fixture.directory.clone(),
            Vec::new(),
            false,
        )
        .await
        .expect("new batch");
    conflict(&mut events, fourth).await;
    worker.request_cancel_sftp_transfer(fourth).expect("cancel");
    for name in ["file.txt", "second.txt", "third.txt", "fourth.txt"] {
        assert_eq!(
            std::fs::read(fixture.directory.join(name)).expect("original"),
            b"original"
        );
    }
    worker.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn sftp_recursive_repeat_merges_directories_and_cancels_waiting_children() {
    let fixture = Fixture::new(false).await;
    fixture
        .files
        .files
        .lock()
        .expect("files")
        .remove("/remote/slow.bin");
    let directory = fixture.directory.join("remote");
    std::fs::create_dir(&directory).expect("existing directory");
    std::fs::write(directory.join("file.txt"), b"original").expect("original");
    let (worker, mut events) = fixture.worker().await;
    until(&mut events, |event| {
        matches!(event, SshSessionEvent::Connected)
    })
    .await;
    for cancel in [false, true] {
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
            .expect("repeat folder");
        let event = until(&mut events, |event| {
            matches!(
                event,
                SshSessionEvent::SftpTransfer(SftpTransferEvent::DownloadConflict { .. })
            )
        })
        .await;
        let SshSessionEvent::SftpTransfer(SftpTransferEvent::DownloadConflict {
            transfer_id, ..
        }) = event
        else {
            unreachable!()
        };
        if cancel {
            worker
                .request_cancel_sftp_transfer(root)
                .expect("cancel root");
        } else {
            worker
                .request_resolve_sftp_conflict(transfer_id, SftpConflictChoice::KeepBoth, true)
                .expect("keep both");
        }
        let event = until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::DiscoveryFinished { transfer_id, .. }) if *transfer_id == root)).await;
        let SshSessionEvent::SftpTransfer(SftpTransferEvent::DiscoveryFinished {
            cancelled,
            failed_files,
            ..
        }) = event
        else {
            unreachable!()
        };
        assert_eq!(cancelled, cancel);
        if !cancel {
            assert_eq!(failed_files, 0);
        }
        assert_eq!(
            std::fs::read(directory.join("file.txt")).expect("original"),
            b"original"
        );
        assert_eq!(
            std::fs::read(directory.join("file (1).txt")).expect("copy"),
            CONTENT
        );
    }
    worker.shutdown().await.expect("shutdown");
    assert_no_active_sftp_channels(&fixture).await;
}

#[tokio::test]
async fn sftp_directory_user_skip_is_separate_from_discovery_failures() {
    let fixture = Fixture::new(false).await;
    fixture
        .files
        .files
        .lock()
        .expect("files")
        .remove("/remote/slow.bin");
    let directory = fixture.directory.join("remote");
    std::fs::create_dir(&directory).expect("existing folder");
    std::fs::write(directory.join("file.txt"), b"original").expect("original");
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
        .expect("folder");
    let event = until(&mut events, |event| {
        matches!(
            event,
            SshSessionEvent::SftpTransfer(SftpTransferEvent::DownloadConflict { .. })
        )
    })
    .await;
    let SshSessionEvent::SftpTransfer(SftpTransferEvent::DownloadConflict { transfer_id, .. }) =
        event
    else {
        unreachable!()
    };
    worker
        .request_resolve_sftp_conflict(transfer_id, SftpConflictChoice::Skip, true)
        .expect("skip");
    let event = until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::DiscoveryFinished { transfer_id, .. }) if *transfer_id == root)).await;
    assert!(matches!(
        event,
        SshSessionEvent::SftpTransfer(SftpTransferEvent::DiscoveryFinished {
            discovered_files: 1,
            skipped_files: 1,
            skipped_entries: 0,
            failed_files: 0,
            cancelled: false,
            ..
        })
    ));
    assert_eq!(
        std::fs::read(directory.join("file.txt")).expect("kept original"),
        b"original"
    );
    worker.shutdown().await.expect("shutdown");
}
