use super::*;
use crate::ssh::SftpTestFixture;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("axssh-companion-edit-{}", Uuid::new_v4())))
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        if self.0.exists() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

async fn owner(fixture: &SftpTestFixture, root: &Root, opener: Opener) -> SftpSession {
    let (transport, events) = fixture.worker().await;
    let template = SftpSession::idle(&Handle::current(), Arc::new(|| {}));
    let view = template.client().view().unwrap();
    template.shutdown().await.unwrap();
    let shared = Arc::new(Mutex::new(Shared {
        view,
        mutation: None,
    }));
    let (commands, receiver) = mpsc::channel(COMMAND_CAPACITY);
    let (stop, cancelled) = watch::channel(false);
    let (settings, updates) = watch::channel(Settings::default());
    let client = SftpClient {
        commands,
        generation: None,
        shared: shared.clone(),
    };
    let root = root.0.clone();
    let task = tokio::spawn(async move {
        let mut worker = Worker {
            runtime: Handle::current(),
            commands: receiver,
            stop: cancelled,
            settings: Settings::default(),
            updates,
            transport: Some(transport),
            events: Some(events),
            profile: None,
            probe: None,
            probe_task: None,
            pending_secret: None,
            prompt: None,
            generation: 1,
            identity: ConnectionIdentity::Sftp {
                profile_id: Uuid::new_v4(),
                attempt_id: Some(Uuid::new_v4()),
            },
            connected: false,
            status: String::new(),
            home: String::new(),
            path: String::new(),
            loading: false,
            has_more: false,
            truncated: false,
            request_id: 0,
            entries: Vec::new(),
            local_path: String::new(),
            local_loading: false,
            local_entries: Vec::new(),
            local_revision: 0,
            local_files: HashMap::new(),
            local_open: super::super::local_open::Opening::default(),
            preparation: super::super::transfers::Preparation::default(),
            icon_task: None,
            icons_dirty: false,
            transfers: BTreeMap::new(),
            last_batch: None,
            last_download_batch: None,
            mutation: None,
            editing: Editing {
                root: Some(root),
                opener,
                ..Default::default()
            },
            shared,
            wake: Arc::new(|| {}),
        };
        let result = worker.run().await;
        let cleanup = worker.stop_transport().await;
        worker.preparation.finish().await?;
        worker.editing.finish().await?;
        if let Some(task) = worker.icon_task.take() {
            task.await.unwrap();
        }
        result.and(cleanup)
    });
    let session = SftpSession {
        client,
        stop,
        settings,
        task,
    };
    wait(&session.client(), |view| {
        view.connected && !view.entries.is_empty()
    })
    .await;
    session
}

async fn wait(client: &SftpClient, predicate: impl Fn(&SftpView) -> bool) -> SftpView {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let view = client.view().unwrap();
            if predicate(&view) {
                break view;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn real_companion_roundtrips_binary_saves_and_retains_copies_after_stop() {
    let fixture = SftpTestFixture::new(false).await;
    let root = Root::new();
    let opened = Arc::new(AtomicUsize::new(0));
    let count = opened.clone();
    let session = owner(
        &fixture,
        &root,
        Arc::new(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }),
    )
    .await;
    let client = session.client();
    client.edit("/remote/file.txt".into()).unwrap();
    let view = wait(&client, |view| {
        view.edits
            .first()
            .is_some_and(|edit| edit.status == "Editing in system application")
    })
    .await;
    let edit = &view.edits[0];
    let id = edit.id;
    let path = edit.local_path.clone().unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        fixture.contents("/remote/file.txt")
    );
    assert_eq!(opened.load(Ordering::SeqCst), 1);
    std::fs::write(&path, [0, 255, 128, 10]).unwrap();
    wait(&client, |view| view.edits[0].status == "Saved to remote").await;
    assert_eq!(fixture.contents("/remote/file.txt"), [0, 255, 128, 10]);
    client.auto_upload_edits(false).unwrap();
    wait(&client, |view| !view.edit_auto_upload).await;
    std::fs::write(&path, b"manual save").unwrap();
    wait(&client, |view| view.edits[0].changed).await;
    assert_eq!(fixture.contents("/remote/file.txt"), [0, 255, 128, 10]);
    client.upload_edit(id).unwrap();
    wait(&client, |view| view.edits[0].status == "Saved to remote").await;
    assert_eq!(fixture.contents("/remote/file.txt"), b"manual save");
    client.stop_edit(id).unwrap();
    wait(&client, |view| view.edits.is_empty()).await;
    std::fs::write(&path, b"retained locally").unwrap();
    session.shutdown().await.unwrap();
    assert_eq!(std::fs::read(path).unwrap(), b"retained locally");
    assert_eq!(fixture.contents("/remote/file.txt"), b"manual save");
}

#[tokio::test]
async fn remote_conflicts_preserve_local_copy_and_opener_failures_remain_observable() {
    let fixture = SftpTestFixture::new(false).await;
    let root = Root::new();
    let session = owner(
        &fixture,
        &root,
        Arc::new(|_| anyhow::bail!("fixture opener unavailable")),
    )
    .await;
    let client = session.client();
    client.edit("/remote/file.txt".into()).unwrap();
    let view = wait(&client, |view| {
        view.edits
            .first()
            .is_some_and(|edit| edit.status.contains("fixture opener unavailable"))
    })
    .await;
    let path = view.edits[0].local_path.clone().unwrap();
    fixture.replace("/remote/file.txt", b"someone else changed remote contents");
    std::fs::write(&path, b"local changes").unwrap();
    wait(&client, |view| view.edits[0].failed).await;
    assert_eq!(
        fixture.contents("/remote/file.txt"),
        b"someone else changed remote contents"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"local changes");
    client.disconnect().unwrap();
    wait(&client, |view| !view.connected && view.edits.is_empty()).await;
    session.shutdown().await.unwrap();
    assert!(path.exists());
}

#[tokio::test]
async fn shutdown_joins_pending_copy_creation_without_publishing_or_opening_it() {
    let root = Root::new();
    let path = root.0.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let task = tokio::task::spawn_blocking(move || {
        receiver.recv().unwrap();
        let local = create_copy_in(
            &path,
            "/remote/file.txt",
            b"unpublished",
            SftpTransferSettings::default().remote_edit_max_file_bytes(),
        )
        .unwrap();
        JobResult {
            generation: 1,
            id: Uuid::new_v4(),
            result: Ok(Job::Copy {
                path: local,
                stamp: content_stamp(b"unpublished"),
                size: 11,
                modified: None,
            }),
        }
    });
    let mut editing = Editing {
        task: Some(task),
        ..Default::default()
    };
    let finish = tokio::spawn(async move { editing.finish().await });
    tokio::task::yield_now().await;
    assert!(!finish.is_finished());
    sender.send(()).unwrap();
    finish.await.unwrap().unwrap();
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 0);
}

#[tokio::test]
async fn stopped_remote_download_cannot_open_or_publish_a_late_edit_copy() {
    let fixture = SftpTestFixture::new(false).await;
    let root = Root::new();
    let opened = Arc::new(AtomicUsize::new(0));
    let count = opened.clone();
    let session = owner(
        &fixture,
        &root,
        Arc::new(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }),
    )
    .await;
    let client = session.client();
    client.edit("/remote/slow.bin".into()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), fixture.slow_read_started())
        .await
        .unwrap();
    let view = wait(&client, |view| !view.edits.is_empty()).await;
    client.stop_edit(view.edits[0].id).unwrap();
    wait(&client, |view| view.edits.is_empty()).await;
    fixture.release_slow_read();
    // A later normal edit is ordered behind the stopped read by the real worker.
    client.edit("/remote/file.txt".into()).unwrap();
    wait(&client, |view| {
        view.edits
            .first()
            .is_some_and(|edit| edit.status == "Editing in system application")
    })
    .await;
    assert_eq!(opened.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 1);
    session.shutdown().await.unwrap();
}

#[test]
fn stop_revokes_an_admitted_but_not_yet_started_editor_launch() {
    let mut editing = Editing::default();
    let (id, _, _) = editing.edits.open("/remote/file.txt".into()).unwrap();
    let permit = Arc::new(AtomicBool::new(true));
    editing.permits.insert(id, permit.clone());
    editing.stop(id);
    assert!(!permit.load(Ordering::Acquire));
    assert!(editing.edits.snapshots().is_empty());
}

#[tokio::test]
async fn drag_transfers_preserve_recursive_paths_and_reject_stale_directory_ids() {
    let fixture = SftpTestFixture::new(false).await;
    let root = Root::new();
    let local = root.0.join("local");
    std::fs::create_dir_all(local.join("folder/nested")).unwrap();
    std::fs::write(local.join("folder/nested/child.bin"), [0, 255, 128]).unwrap();
    std::fs::write(local.join("plain.txt"), b"upload root").unwrap();
    let session = owner(&fixture, &root, Arc::new(|_| Ok(()))).await;
    let client = session.client();
    client.list_local(local.clone()).unwrap();
    let view = wait(&client, |view| !view.local_entries.is_empty()).await;
    client
        .for_view(&view)
        .upload_local_drop(
            local.join("plain.txt").display().to_string(),
            view.local_revision + 1,
            view.path.clone(),
            view.request_id,
        )
        .unwrap();
    wait(&client, |view| view.status.contains("snapshot changed")).await;
    assert!(!fixture.contains("/remote/plain.txt"));
    client
        .for_view(&view)
        .upload_roots(
            vec![local.join("folder"), local.join("plain.txt")],
            view.path.clone(),
            view.request_id,
        )
        .unwrap();
    wait(&client, |view| {
        view.transfers
            .iter()
            .filter(|transfer| transfer.status == "Completed")
            .count()
            == 2
    })
    .await;
    assert_eq!(fixture.contents("/remote/plain.txt"), b"upload root");
    assert_eq!(
        fixture.contents("/remote/folder/nested/child.bin"),
        [0, 255, 128]
    );
    let view = client.view().unwrap();
    client
        .for_view(&view)
        .download_drop(
            "/remote/file.txt".into(),
            view.request_id,
            view.local_path.clone(),
            view.local_revision,
        )
        .unwrap();
    wait(&client, |view| {
        view.transfers
            .iter()
            .filter(|transfer| transfer.status == "Completed")
            .count()
            == 3
    })
    .await;
    assert_eq!(
        std::fs::read(local.join("file.txt")).unwrap(),
        fixture.contents("/remote/file.txt")
    );
    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn navigation_and_cancel_discard_preparation_before_transport_admission() {
    let fixture = SftpTestFixture::new(false).await;
    let root = Root::new();
    std::fs::create_dir_all(&root.0).unwrap();
    let source = root.0.join("discarded.txt");
    std::fs::write(&source, b"never sent").unwrap();
    let session = owner(&fixture, &root, Arc::new(|_| Ok(()))).await;
    let client = session.client();
    let view = client.view().unwrap();
    // Both commands are queued before yielding to the single-thread test runtime.
    client
        .for_view(&view)
        .upload_roots(vec![source.clone()], view.path.clone(), view.request_id)
        .unwrap();
    client.list(view.path.clone()).unwrap();
    wait(&client, |current| {
        current.request_id != view.request_id && !current.upload_preparing && !current.loading
    })
    .await;
    assert!(!fixture.contains("/remote/discarded.txt"));
    let view = wait(&client, |view| !view.loading).await;
    client
        .for_view(&view)
        .upload_roots(vec![source], view.path.clone(), view.request_id)
        .unwrap();
    client.cancel_upload_preparation().unwrap();
    wait(&client, |view| {
        view.status == "Upload preparation cancelled" && !view.upload_preparing
    })
    .await;
    assert!(!fixture.contains("/remote/discarded.txt"));
    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn saved_filters_and_edit_size_apply_to_a_connected_companion() {
    let fixture = SftpTestFixture::new(false).await;
    fixture.replace("/remote/large.txt", &vec![b'x'; 2 * 1024 * 1024]);
    let root = Root::new();
    let local = root.0.join("upload");
    std::fs::create_dir_all(&local).unwrap();
    std::fs::write(local.join("keep.txt"), b"keep").unwrap();
    std::fs::write(local.join("skip.tmp"), b"omit").unwrap();
    let opened = Arc::new(AtomicUsize::new(0));
    let observed = opened.clone();
    let session = owner(
        &fixture,
        &root,
        Arc::new(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }),
    )
    .await;
    let client = session.client();
    let mut transfers = SftpTransferSettings {
        remote_edit_max_file_mib: 1,
        ..Default::default()
    };
    let filters = SftpTransferFilterSettings::normalized(true, false, "*.tmp");
    session.configure(transfers.clone(), filters.clone());
    let view = client.view().unwrap();
    client
        .upload_roots(vec![local], view.path, view.request_id)
        .unwrap();
    wait(&client, |view| {
        view.transfers
            .iter()
            .any(|transfer| transfer.status == "Completed")
    })
    .await;
    assert_eq!(fixture.contents("/remote/upload/keep.txt"), b"keep");
    assert!(!fixture.contains("/remote/upload/skip.tmp"));
    client.edit("/remote/large.txt".into()).unwrap();
    wait(&client, |view| {
        view.status.contains("within the edit limit")
    })
    .await;
    assert_eq!(opened.load(Ordering::SeqCst), 0);
    transfers.remote_edit_max_file_mib = 3;
    session.configure(transfers, filters);
    client.edit("/remote/large.txt".into()).unwrap();
    wait(&client, |view| {
        view.edits.iter().any(|edit| edit.local_path.is_some())
    })
    .await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while opened.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    session.shutdown().await.unwrap();
}
