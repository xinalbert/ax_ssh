use super::super::transfer_state::tests::{finish, until, worker};
use super::*;
use crate::ssh::SftpTestFixture;

fn entry(path: &str, directory: bool, link: bool) -> SftpEntryView {
    SftpEntryView {
        path: path.into(),
        name: path.rsplit('/').next().unwrap().into(),
        is_dir: directory,
        is_symlink: link,
        size: 1,
        modified: Some(1),
        icon: SftpIcon::GenericFile,
    }
}
fn client(worker: &Worker) -> (SftpClient, mpsc::Receiver<QueuedCommand>) {
    let (commands, receiver) = mpsc::channel(COMMAND_CAPACITY);
    (
        SftpClient {
            commands,
            generation: Some(worker.generation),
            shared: worker.shared.clone(),
        },
        receiver,
    )
}

#[tokio::test]
async fn malformed_paths_and_full_or_closed_queue_do_not_admit_and_partial_sends_continue() {
    let mut worker = worker().await;
    let (client, mut commands) = client(&worker);
    for paths in [
        vec![],
        vec!["/a".into(), "/a".into()],
        vec!["/".into()],
        vec!["/a\0b".into()],
        (0..65).map(|n| format!("/a{n}")).collect(),
        (0..64)
            .map(|n| format!("/{}/{n}", "界".repeat(3000)))
            .collect(),
    ] {
        assert!(
            client
                .download_selected(paths, 0, "/local".into(), 0)
                .is_err()
        );
        assert!(commands.try_recv().is_err());
    }
    for _ in 0..COMMAND_CAPACITY {
        client.list("/remote".into()).unwrap();
    }
    assert!(
        client
            .download_selected(vec!["/a".into()], 0, "/local".into(), 0)
            .is_err()
    );
    while commands.try_recv().is_ok() {}
    drop(commands);
    assert!(
        client
            .download_selected(vec!["/a".into()], 0, "/local".into(), 0)
            .is_err()
    );
    assert!(worker.last_download_batch.is_none());
    let paths = vec!["/a".into(), "/skip".into(), "/b".into(), "/c".into()];
    let mut sent = vec![];
    let result = admit(
        &paths,
        |path| path != "/skip",
        |path| {
            sent.push(path.to_owned());
            if path == "/b" {
                anyhow::bail!("{}", "界".repeat(600));
            }
            Ok(())
        },
    );
    assert_eq!(sent, ["/a", "/b", "/c"]);
    assert_eq!(
        (
            result.requested,
            result.queued,
            result.skipped,
            result.failures.len()
        ),
        (4, 2, 1, 1)
    );
    assert_eq!(result.failures[0].path, "/b");
    assert_eq!(result.failures[0].message.chars().count(), 512);
    finish(&mut worker).await;
}

#[tokio::test]
async fn stale_source_or_target_never_sends_and_ineligible_entries_are_skipped() {
    let mut worker = worker().await;
    worker.path = "/remote".into();
    worker.local_path = "/local".into();
    worker.local_revision = 1;
    worker.entries = vec![
        entry("/remote/a", false, false),
        entry("/remote/link", false, true),
        entry("/remote/filter.tmp", false, false),
    ];
    let paths = vec!["/remote/a".into()];
    assert!(
        worker
            .download_selected(paths.clone(), 0, "/local".into(), 1)
            .is_err()
    );
    worker.connected = true;
    for (request, path, revision) in [(1, "/local", 1), (0, "/other", 1), (0, "/local", 2)] {
        assert!(
            worker
                .download_selected(paths.clone(), request, path.into(), revision)
                .is_err()
        );
    }
    let (client, mut commands) = client(&worker);
    client
        .download_selected(paths, 0, "/local".into(), 1)
        .unwrap();
    worker.generation += 1;
    worker
        .dispatch_queued(commands.recv().await.unwrap())
        .await
        .unwrap();
    assert!(worker.last_download_batch.is_none());
    assert!(worker.transfers.is_empty());
    worker.settings.filters = SftpTransferFilterSettings::normalized(true, false, "*.tmp");
    worker
        .download_selected(
            vec![
                "/remote/a".into(),
                "/remote/link".into(),
                "/remote/missing".into(),
                "/remote/filter.tmp".into(),
            ],
            0,
            "/local".into(),
            1,
        )
        .unwrap();
    let result = worker.last_download_batch.as_ref().unwrap();
    assert_eq!(
        (result.queued, result.skipped, result.failures.len()),
        (0, 3, 1)
    );
    assert!(worker.transfers.is_empty());
    worker.next_request();
    assert!(worker.last_download_batch.is_none());
    finish(&mut worker).await;
}

#[tokio::test]
async fn real_mixed_batch_preserves_recursive_bytes_filters_and_reports_empty_directory_failure() {
    let fixture = SftpTestFixture::new(false).await;
    fixture.add_directory("/remote/tree");
    fixture.add_directory("/remote/tree/sub");
    fixture.add_directory("/remote/empty");
    fixture.replace("/remote/tree/a", b"first\0file");
    fixture.replace("/remote/tree/sub/b", b"nested\nfile");
    fixture.replace("/remote/tree/skip.tmp", b"skip");
    let directory = std::env::temp_dir().join(format!("axssh-selected-{}", Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let mut worker = worker().await;
    let (transport, events) = fixture.worker().await;
    worker.transport = Some(transport);
    worker.events = Some(events);
    until(&mut worker, |event| {
        matches!(
            event,
            SshSessionEvent::Sftp(SftpBrowserEvent::DirectoryPage { .. })
        )
    })
    .await;
    worker.local_path = directory.to_str().unwrap().into();
    worker.local_revision = 1;
    worker.settings.filters = SftpTransferFilterSettings::normalized(true, false, "*.tmp");
    // Use one existing regular file plus a nested tree and an empty directory.
    let file = worker
        .entries
        .iter()
        .find(|entry| !entry.is_dir && entry.name != "slow.bin")
        .unwrap()
        .clone();
    let expected = fixture.contents(&file.path);
    worker
        .download_selected(
            vec![
                file.path.clone(),
                "/remote/tree".into(),
                "/remote/empty".into(),
            ],
            worker.request_id,
            worker.local_path.clone(),
            1,
        )
        .unwrap();
    assert_eq!(worker.last_download_batch.as_ref().unwrap().queued, 3);
    assert_eq!(worker.transfers.len(), 3);
    let empty_id = worker
        .transfers
        .values()
        .find(|item| item.name == "empty")
        .unwrap()
        .id;
    let mut empty_failed = false;
    let mut completed = HashSet::new();
    until(&mut worker, |event| {
        if let SshSessionEvent::SftpTransfer(SftpTransferEvent::Completed { transfer_id, .. }) =
            event
        {
            completed.insert(*transfer_id);
        }
        if let SshSessionEvent::SftpTransfer(SftpTransferEvent::DiscoveryFailed {
            transfer_id, ..
        }) = event
            && *transfer_id == empty_id
        {
            empty_failed = true;
        }
        completed.len() == 3 && empty_failed
    })
    .await;
    assert_eq!(std::fs::read(directory.join(&file.name)).unwrap(), expected);
    assert_eq!(
        std::fs::read(directory.join("tree/a")).unwrap(),
        b"first\0file"
    );
    assert_eq!(
        std::fs::read(directory.join("tree/sub/b")).unwrap(),
        b"nested\nfile"
    );
    assert!(!directory.join("tree/skip.tmp").exists());
    assert!(!directory.join("empty").exists());
    assert_eq!(worker.transfers.len(), 4);
    assert!(worker.transfers.values().all(|item| item.phase.finished()));
    worker.publish().unwrap();
    assert_eq!(
        worker.shared.lock().unwrap().view.last_download_batch,
        worker.last_download_batch
    );
    finish(&mut worker).await;
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn real_batch_cancel_cleans_partial_files_and_disconnect_clears_summary() {
    let fixture = SftpTestFixture::new(false).await;
    let directory = std::env::temp_dir().join(format!("axssh-selected-cancel-{}", Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let mut worker = worker().await;
    let (transport, events) = fixture.worker().await;
    worker.transport = Some(transport);
    worker.events = Some(events);
    until(&mut worker, |event| {
        matches!(
            event,
            SshSessionEvent::Sftp(SftpBrowserEvent::DirectoryPage { .. })
        )
    })
    .await;
    worker.local_path = directory.to_str().unwrap().into();
    worker
        .download_selected(
            vec!["/remote/slow.bin".into()],
            worker.request_id,
            worker.local_path.clone(),
            0,
        )
        .unwrap();
    let id = *worker.transfers.keys().next().unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        fixture.slow_read_started(),
    )
    .await
    .unwrap();
    until(&mut worker,|event|matches!(event,SshSessionEvent::SftpTransfer(SftpTransferEvent::Started { transfer_id,.. }) if *transfer_id==id)).await;
    worker.dispatch(Command::Cancel(id)).await.unwrap();
    fixture.release_slow_read();
    until(&mut worker,|event|matches!(event,SshSessionEvent::SftpTransfer(SftpTransferEvent::Cancelled { transfer_id }) if *transfer_id==id)).await;
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
    worker.dispatch(Command::Disconnect).await.unwrap();
    assert!(worker.last_download_batch.is_none());
    finish(&mut worker).await;
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn real_batch_never_overwrites_existing_local_files_and_keeps_other_downloads_running() {
    let fixture = SftpTestFixture::new(false).await;
    fixture.replace("/remote/other", b"server bytes");
    let directory =
        std::env::temp_dir().join(format!("axssh-selected-conflict-{}", Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("other"), b"keep local").unwrap();
    let mut worker = worker().await;
    let (transport, events) = fixture.worker().await;
    worker.transport = Some(transport);
    worker.events = Some(events);
    until(&mut worker, |event| {
        matches!(
            event,
            SshSessionEvent::Sftp(SftpBrowserEvent::DirectoryPage { .. })
        )
    })
    .await;
    let file = worker
        .entries
        .iter()
        .find(|entry| !entry.is_dir && entry.name != "slow.bin" && entry.name != "other")
        .unwrap()
        .clone();
    worker.local_path = directory.to_str().unwrap().into();
    worker
        .download_selected(
            vec!["/remote/other".into(), file.path.clone()],
            worker.request_id,
            worker.local_path.clone(),
            0,
        )
        .unwrap();
    let conflict = worker
        .transfers
        .values()
        .find(|entry| entry.name == "other")
        .unwrap()
        .id;
    let (mut failed, mut completed) = (false, false);
    until(&mut worker, |event| {
        match event {
            SshSessionEvent::SftpTransfer(SftpTransferEvent::Failed { transfer_id, .. })
                if *transfer_id == conflict =>
            {
                failed = true
            }
            SshSessionEvent::SftpTransfer(SftpTransferEvent::Completed { .. }) => completed = true,
            _ => {}
        }
        failed && completed
    })
    .await;
    assert_eq!(
        std::fs::read(directory.join("other")).unwrap(),
        b"keep local"
    );
    assert_eq!(
        std::fs::read(directory.join(&file.name)).unwrap(),
        fixture.contents(&file.path)
    );
    assert_eq!(worker.last_download_batch.as_ref().unwrap().queued, 2);
    assert!(
        worker
            .transfers
            .values()
            .all(|entry| entry.phase.finished())
    );
    finish(&mut worker).await;
    std::fs::remove_dir_all(directory).unwrap();
}
