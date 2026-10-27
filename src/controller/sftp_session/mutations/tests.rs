use super::super::transfer_state::tests::{finish, until, worker};
use super::*;
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

fn entry() -> SftpEntryView {
    SftpEntryView {
        name: "file".into(),
        path: "/remote/file".into(),
        is_dir: false,
        is_symlink: false,
        size: 3,
        modified: Some(1),
        icon: SftpIcon::GenericFile,
    }
}

#[tokio::test]
async fn names_paths_and_receipt_budget_are_validated_before_queue_admission() {
    let mut worker = worker().await;
    let (client, mut commands) = client(&worker);
    let entry = entry();
    for name in [
        "".into(),
        ".".into(),
        "..".into(),
        "a/b".into(),
        "a\\b".into(),
        "a\0b".into(),
        "界".repeat(513),
        "file".into(),
    ] {
        assert!(
            client
                .mutate_entry(0, &entry, SftpEntryAction::Rename(name))
                .is_err()
        );
    }
    for path in [
        "/".into(),
        "/remote/..".into(),
        "/remote/.".into(),
        "a\nb".into(),
        "a".repeat(4097),
    ] {
        assert!(
            client
                .mutate_entry(
                    0,
                    &SftpEntryView {
                        path,
                        ..entry.clone()
                    },
                    SftpEntryAction::Delete
                )
                .is_err()
        );
    }
    assert!(commands.try_recv().is_err());
    for _ in 0..COMMAND_CAPACITY {
        client.list("/remote".into()).unwrap();
    }
    assert!(
        client
            .mutate_entry(0, &entry, SftpEntryAction::Delete)
            .is_err()
    );
    assert!(worker.shared.lock().unwrap().mutation.is_none());
    while commands.try_recv().is_ok() {}
    let id = client
        .mutate_entry(0, &entry, SftpEntryAction::Rename("  新名  ".into()))
        .unwrap();
    assert!(
        client
            .mutate_entry(0, &entry, SftpEntryAction::Delete)
            .is_err()
    );
    assert!(client.take_entry_result(Uuid::new_v4()).unwrap().is_none());
    client.abandon_entry_result(id).unwrap();
    worker
        .dispatch_queued(commands.recv().await.unwrap())
        .await
        .unwrap();
    assert!(worker.shared.lock().unwrap().mutation.is_none());
    let id = client
        .mutate_entry(0, &entry, SftpEntryAction::Delete)
        .unwrap();
    worker
        .dispatch_queued(commands.recv().await.unwrap())
        .await
        .unwrap();
    // Async owner rejection remains retained until exactly its requester consumes it.
    assert!(
        client
            .mutate_entry(0, &entry, SftpEntryAction::Delete)
            .is_err()
    );
    assert!(client.take_entry_result(id).unwrap().unwrap().is_err());
    drop(commands);
    assert!(
        client
            .mutate_entry(0, &entry, SftpEntryAction::Delete)
            .is_err()
    );
    assert!(worker.shared.lock().unwrap().mutation.is_none());
    finish(&mut worker).await;
}

#[tokio::test]
async fn stale_generation_directory_and_entry_changes_reject_without_a_native_write() {
    let mut worker = worker().await;
    worker.connected = true;
    worker.path = "/remote".into();
    worker.entries = vec![entry()];
    let (client, mut commands) = client(&worker);
    for case in 0..7 {
        let original = entry();
        worker.entries = vec![original.clone()];
        worker.loading = false;
        let id = client
            .mutate_entry(worker.request_id, &original, SftpEntryAction::Delete)
            .unwrap();
        match case {
            0 => worker.generation += 1,
            1 => worker.request_id += 1,
            2 => worker.entries.clear(),
            3 => worker.entries[0].is_symlink = true,
            4 => worker.entries[0].is_dir = true,
            5 => worker.entries[0].modified = Some(2),
            _ => worker.loading = true,
        }
        worker
            .dispatch_queued(commands.recv().await.unwrap())
            .await
            .unwrap();
        assert!(client.take_entry_result(id).unwrap().unwrap().is_err());
        assert!(worker.mutation.is_none());
        // Keep the client bound to the original generation for case 0 only.
        worker.generation = client.generation.unwrap();
    }
    finish(&mut worker).await;
}

async fn connected() -> (SftpTestFixture, Worker) {
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
    (fixture, worker)
}

#[tokio::test]
async fn real_rename_collision_retry_delete_and_refresh_keep_exact_receipts() {
    let (fixture, mut worker) = connected().await;
    let source = worker
        .entries
        .iter()
        .find(|entry| !entry.is_dir && !entry.is_symlink)
        .unwrap()
        .clone();
    let expected = fixture.contents(&source.path);
    let (client, mut commands) = client(&worker);
    fixture.replace("/remote/collision", b"keep");
    let id = client
        .mutate_entry(
            worker.request_id,
            &source,
            SftpEntryAction::Rename("collision".into()),
        )
        .unwrap();
    worker
        .dispatch_queued(commands.recv().await.unwrap())
        .await
        .unwrap();
    assert!(client.take_entry_result(id).unwrap().is_none());
    until(&mut worker, |event| matches!(event, SshSessionEvent::SftpWrite(SftpWriteEvent::Failed { operation_id, .. }) if *operation_id==id)).await;
    assert!(client.take_entry_result(id).unwrap().unwrap().is_err());
    assert_eq!(fixture.contents(&source.path), expected);
    assert_eq!(fixture.contents("/remote/collision"), b"keep");
    let id = client
        .mutate_entry(
            worker.request_id,
            &source,
            SftpEntryAction::Rename("renamed".into()),
        )
        .unwrap();
    worker
        .dispatch_queued(commands.recv().await.unwrap())
        .await
        .unwrap();
    assert!(
        !worker
            .mutation_event(&SftpWriteEvent::Completed {
                operation_id: Uuid::new_v4(),
                path: "/remote/renamed".into()
            })
            .unwrap()
    );
    until(&mut worker, |event| matches!(event, SshSessionEvent::SftpWrite(SftpWriteEvent::Completed { operation_id, .. }) if *operation_id==id)).await;
    assert!(client.take_entry_result(id).unwrap().unwrap().is_ok());
    assert!(!fixture.contains(&source.path));
    assert_eq!(fixture.contents("/remote/renamed"), expected);
    assert!(worker.loading);
    until(&mut worker, |event| {
        matches!(
            event,
            SshSessionEvent::Sftp(SftpBrowserEvent::DirectoryPage { .. })
        )
    })
    .await;
    let renamed = worker
        .entries
        .iter()
        .find(|entry| entry.path == "/remote/renamed")
        .unwrap()
        .clone();
    let id = client
        .mutate_entry(worker.request_id, &renamed, SftpEntryAction::Delete)
        .unwrap();
    worker
        .dispatch_queued(commands.recv().await.unwrap())
        .await
        .unwrap();
    client.abandon_entry_result(id).unwrap();
    // Navigation while a write is in flight must not be redirected by its ack.
    worker.path = "/other".into();
    worker.request_id += 1;
    until(&mut worker, |event| matches!(event, SshSessionEvent::SftpWrite(SftpWriteEvent::Completed { operation_id, .. }) if *operation_id==id)).await;
    assert_eq!(worker.path, "/other");
    assert!(!worker.loading);
    assert!(!fixture.contains("/remote/renamed"));
    assert!(worker.shared.lock().unwrap().mutation.is_none());
    finish(&mut worker).await;
}

#[tokio::test]
async fn pending_failure_is_bounded_and_disconnect_or_duplicate_receipts_cannot_complete_a_new_slot()
 {
    let (fixture, mut worker) = connected().await;
    let source = worker
        .entries
        .iter()
        .find(|entry| !entry.is_dir && !entry.is_symlink)
        .unwrap()
        .clone();
    let (client, mut commands) = client(&worker);
    let id = client
        .mutate_entry(worker.request_id, &source, SftpEntryAction::Delete)
        .unwrap();
    worker
        .dispatch_queued(commands.recv().await.unwrap())
        .await
        .unwrap();
    worker
        .mutation_event(&SftpWriteEvent::Failed {
            operation_id: id,
            message: "界".repeat(600),
        })
        .unwrap();
    assert_eq!(
        client
            .take_entry_result(id)
            .unwrap()
            .unwrap()
            .unwrap_err()
            .chars()
            .count(),
        512
    );
    let next = client
        .mutate_entry(worker.request_id, &source, SftpEntryAction::Delete)
        .unwrap();
    assert!(
        !worker
            .mutation_event(&SftpWriteEvent::Completed {
                operation_id: id,
                path: source.path.clone()
            })
            .unwrap()
    );
    assert!(client.take_entry_result(next).unwrap().is_none());
    worker.dispatch(Command::Disconnect).await.unwrap();
    assert!(worker.shared.lock().unwrap().mutation.is_none());
    worker
        .dispatch_queued(commands.recv().await.unwrap())
        .await
        .unwrap();
    assert!(worker.mutation.is_none());
    finish(&mut worker).await;
    drop(fixture);
}

#[tokio::test]
async fn real_directory_delete_uses_nonrecursive_rmdir_and_preserves_nonempty_contents() {
    let fixture = SftpTestFixture::new(false).await;
    fixture.add_directory("/remote/empty");
    fixture.add_directory("/remote/nonempty");
    fixture.replace("/remote/nonempty/child", b"retain");
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
    let (client, mut commands) = client(&worker);
    for (path, success) in [("/remote/nonempty", false), ("/remote/empty", true)] {
        let source = worker
            .entries
            .iter()
            .find(|entry| entry.path == path)
            .unwrap()
            .clone();
        assert!(source.is_dir);
        let id = client
            .mutate_entry(worker.request_id, &source, SftpEntryAction::Delete)
            .unwrap();
        worker
            .dispatch_queued(commands.recv().await.unwrap())
            .await
            .unwrap();
        until(&mut worker,|event|matches!(event,SshSessionEvent::SftpWrite(SftpWriteEvent::Completed { operation_id,.. }|SftpWriteEvent::Failed { operation_id,.. }) if *operation_id==id)).await;
        assert_eq!(
            client.take_entry_result(id).unwrap().unwrap().is_ok(),
            success
        );
        assert_eq!(fixture.has_directory(path), !success);
    }
    assert_eq!(fixture.contents("/remote/nonempty/child"), b"retain");
    finish(&mut worker).await;
}
