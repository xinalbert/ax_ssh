use super::*;
use crate::controller::sftp_session::transfer_state::tests::{finish, until, worker};
use crate::ssh::SftpTestFixture;

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
fn root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("axssh-local-select-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    root
}
async fn connected(root: &Path) -> (SftpTestFixture, Worker) {
    let fixture = SftpTestFixture::new(false).await;
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
    worker
        .dispatch(Command::ListLocal(root.to_path_buf()))
        .await
        .unwrap();
    (fixture, worker)
}
async fn prepared(worker: &mut Worker) -> Prepared {
    worker.preparation.task.take().unwrap().await.unwrap()
}

#[tokio::test]
async fn selected_input_limits_duplicate_paths_and_closed_or_full_queue_fail_before_preparation() {
    let mut worker = worker().await;
    let (client, mut commands) = client(&worker);
    for paths in [
        vec![],
        vec!["/a".into(), "/a".into()],
        vec!["bad\npath".into()],
        vec!["x".repeat(4097)],
        (0..65).map(|n| format!("/a{n}")).collect(),
    ] {
        assert!(
            client
                .upload_selected(paths, 0, "/remote".into(), 0)
                .is_err()
        );
        assert!(commands.try_recv().is_err());
    }
    for _ in 0..COMMAND_CAPACITY {
        client.list("/remote".into()).unwrap();
    }
    assert!(
        client
            .upload_selected(vec!["/file".into()], 0, "/remote".into(), 0)
            .is_err()
    );
    while commands.try_recv().is_ok() {}
    drop(commands);
    assert!(
        client
            .upload_selected(vec!["/file".into()], 0, "/remote".into(), 0)
            .is_err()
    );
    assert!(!worker.preparation.active());
    finish(&mut worker).await;
}

#[tokio::test]
async fn stale_sources_and_targets_and_changed_files_never_reach_transport() {
    let root = root();
    std::fs::write(root.join("file"), b"old").unwrap();
    let (fixture, mut worker) = connected(&root).await;
    let path = worker.local_entries[0].path.clone();
    assert!(
        worker
            .prepare_selected_upload(
                vec![path.clone()],
                worker.local_revision + 1,
                worker.path.clone(),
                worker.request_id
            )
            .is_err()
    );
    assert!(
        worker
            .prepare_selected_upload(
                vec!["/missing".into()],
                worker.local_revision,
                worker.path.clone(),
                worker.request_id
            )
            .is_err()
    );
    assert!(
        worker
            .prepare_selected_upload(
                vec![path.clone()],
                worker.local_revision,
                "/other".into(),
                worker.request_id
            )
            .is_err()
    );
    let (client, mut commands) = client(&worker);
    client
        .upload_selected(
            vec![path.clone()],
            worker.local_revision,
            worker.path.clone(),
            worker.request_id,
        )
        .unwrap();
    worker.generation += 1;
    worker
        .dispatch_queued(commands.recv().await.unwrap())
        .await
        .unwrap();
    assert!(!worker.preparation.active());
    worker
        .prepare_selected_upload(
            vec![path.clone()],
            worker.local_revision,
            worker.path.clone(),
            worker.request_id,
        )
        .unwrap();
    let result = prepared(&mut worker).await;
    assert!(result.result.is_ok());
    worker.local_revision += 1;
    assert!(worker.complete_preparation(result).is_err());
    std::fs::write(&path, b"changed bytes").unwrap();
    worker
        .prepare_selected_upload(
            vec![path],
            worker.local_revision,
            worker.path.clone(),
            worker.request_id,
        )
        .unwrap();
    let result = prepared(&mut worker).await;
    assert!(result.result.is_err());
    assert!(worker.complete_preparation(result).is_err());
    assert!(worker.transfers.is_empty());
    assert!(!fixture.contains("/remote/file"));
    finish(&mut worker).await;
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn selected_root_validation_rejects_directory_type_changes_and_file_replacement() {
    let root = root();
    std::fs::write(root.join("file"), b"same").unwrap();
    std::fs::create_dir(root.join("folder")).unwrap();
    let listing = read_local_directory(root.clone()).unwrap();
    let file = listing
        .files
        .values()
        .find(|entry| !entry.is_dir)
        .unwrap()
        .clone();
    let folder = listing
        .files
        .values()
        .find(|entry| entry.is_dir)
        .unwrap()
        .clone();
    let file_source = LocalSource::Selection {
        directory: listing.path.clone(),
        revision: 0,
        entries: vec![file],
    };
    let dir_source = LocalSource::Selection {
        directory: listing.path,
        revision: 0,
        entries: vec![folder],
    };
    let cancelled = AtomicBool::new(false);
    file_source.validate(&cancelled).unwrap();
    dir_source.validate(&cancelled).unwrap();
    std::fs::rename(root.join("file"), root.join("old")).unwrap();
    std::fs::write(root.join("file"), b"same").unwrap();
    assert!(file_source.validate(&cancelled).is_err());
    std::fs::remove_dir(root.join("folder")).unwrap();
    std::fs::write(root.join("folder"), b"ordinary file").unwrap();
    assert!(dir_source.validate(&cancelled).is_err());
    #[cfg(unix)]
    {
        std::fs::remove_file(root.join("folder")).unwrap();
        std::os::unix::fs::symlink(root.join("old"), root.join("folder")).unwrap();
        assert!(dir_source.validate(&cancelled).is_err());
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn real_selected_file_and_directory_uploads_preserve_bytes_filters_and_one_batch_identity() {
    let root = root();
    std::fs::write(root.join("top.bin"), b"top\0bytes").unwrap();
    std::fs::create_dir_all(root.join("folder/sub")).unwrap();
    std::fs::write(root.join("folder/sub/data.bin"), b"nested\nbytes").unwrap();
    std::fs::write(root.join("folder/skip.tmp"), b"skip").unwrap();
    let (fixture, mut worker) = connected(&root).await;
    worker.settings.filters = SftpTransferFilterSettings::normalized(true, false, "*.tmp");
    let paths = worker
        .local_entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect();
    worker
        .prepare_selected_upload(
            paths,
            worker.local_revision,
            worker.path.clone(),
            worker.request_id,
        )
        .unwrap();
    assert!(worker.preparation.active());
    assert!(worker.transfers.is_empty());
    let result = prepared(&mut worker).await;
    let requests = result.result.as_ref().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].batch_id(), requests[1].batch_id());
    worker.complete_preparation(result).unwrap();
    assert_eq!(worker.transfers.len(), 2);
    let mut completed = HashSet::new();
    until(&mut worker, |event| {
        if let SshSessionEvent::SftpTransfer(SftpTransferEvent::Completed { transfer_id, .. }) =
            event
        {
            completed.insert(*transfer_id);
        }
        completed.len() == 2
    })
    .await;
    assert_eq!(fixture.contents("/remote/top.bin"), b"top\0bytes");
    assert_eq!(
        fixture.contents("/remote/folder/sub/data.bin"),
        b"nested\nbytes"
    );
    assert!(!fixture.contains("/remote/folder/skip.tmp"));
    finish(&mut worker).await;
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn cancelled_or_navigated_preparations_are_joined_without_publishing_and_conflicts_require_a_choice()
 {
    let root = root();
    std::fs::write(root.join("file.txt"), b"replacement").unwrap();
    let (fixture, mut worker) = connected(&root).await;
    let paths = vec![worker.local_entries[0].path.clone()];
    for action in 0..3 {
        worker
            .prepare_selected_upload(
                paths.clone(),
                worker.local_revision,
                worker.path.clone(),
                worker.request_id,
            )
            .unwrap();
        let result = prepared(&mut worker).await;
        match action {
            0 => worker.dispatch(Command::CancelPreparation).await.unwrap(),
            1 => worker
                .dispatch(Command::ListLocal(root.clone()))
                .await
                .unwrap(),
            _ => {
                worker
                    .dispatch(Command::List("/remote".into()))
                    .await
                    .unwrap();
                until(&mut worker, |event| {
                    matches!(
                        event,
                        SshSessionEvent::Sftp(SftpBrowserEvent::DirectoryPage { .. })
                    )
                })
                .await;
            }
        }
        worker.complete_preparation(result).unwrap();
        assert!(worker.transfers.is_empty());
    }
    let before = fixture.contents("/remote/file.txt");
    worker
        .prepare_selected_upload(
            paths,
            worker.local_revision,
            worker.path.clone(),
            worker.request_id,
        )
        .unwrap();
    let result = prepared(&mut worker).await;
    worker.complete_preparation(result).unwrap();
    let id = *worker.transfers.keys().next().unwrap();
    until(&mut worker,|event|matches!(event,SshSessionEvent::SftpTransfer(SftpTransferEvent::UploadConflict { transfer_id,.. }) if *transfer_id==id)).await;
    assert_eq!(fixture.contents("/remote/file.txt"), before);
    worker
        .dispatch(Command::ResolveConflict {
            transfer_id: id,
            choice: SftpUploadConflictChoice::Overwrite,
            apply_to_batch: false,
        })
        .await
        .unwrap();
    until(&mut worker,|event|matches!(event,SshSessionEvent::SftpTransfer(SftpTransferEvent::Completed { transfer_id,.. }) if *transfer_id==id)).await;
    assert_eq!(fixture.contents("/remote/file.txt"), b"replacement");
    finish(&mut worker).await;
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn selected_directory_with_more_than_64_files_rejects_the_whole_preparation() {
    let root = root();
    std::fs::create_dir(root.join("tree")).unwrap();
    for index in 0..65 {
        std::fs::write(root.join("tree").join(format!("file{index}")), b"byte").unwrap();
    }
    let (fixture, mut worker) = connected(&root).await;
    let directory = worker.local_entries[0].path.clone();
    assert!(
        worker
            .open_local(directory.clone(), worker.local_revision)
            .is_err()
    );
    worker
        .prepare_selected_upload(
            vec![directory],
            worker.local_revision,
            worker.path.clone(),
            worker.request_id,
        )
        .unwrap();
    let result = prepared(&mut worker).await;
    assert!(result.result.is_err());
    assert!(worker.complete_preparation(result).is_err());
    assert!(worker.transfers.is_empty());
    assert!(!fixture.contains("/remote/tree/file0"));
    finish(&mut worker).await;
    std::fs::remove_dir_all(root).unwrap();
}
