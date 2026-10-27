use super::*;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("axssh-local-open-{}", Uuid::new_v4()));
        std::fs::create_dir_all(path.join("files")).unwrap();
        Self(std::fs::canonicalize(path).unwrap())
    }
    fn files(&self) -> PathBuf {
        self.0.join("files")
    }
    fn cache(&self) -> PathBuf {
        self.0.join("cache")
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        if self.0.exists() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

async fn wait(client: &SftpClient, mut ready: impl FnMut(&SftpView) -> bool) -> SftpView {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let view = client.view().unwrap();
            if ready(&view) {
                return view;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "local open test timed out: {}",
            client.view().unwrap().status
        )
    })
}

async fn listing(client: &SftpClient, path: PathBuf) -> SftpView {
    let revision = client.view().unwrap().local_revision;
    client.list_local(path).unwrap();
    wait(client, |view| {
        view.local_revision > revision && !view.local_loading
    })
    .await
}

fn session(root: &Root, mut opening: Opening, limit_mib: u32) -> SftpSession {
    opening.root = Some(root.cache());
    SftpSession::start(
        &Handle::current(),
        SftpTransferSettings {
            local_open_max_file_mib: limit_mib,
            ..Default::default()
        },
        SftpTransferFilterSettings::default(),
        Arc::new(|| {}),
        opening,
    )
}

#[tokio::test]
async fn local_open_uses_an_isolated_private_copy_and_the_saved_limit() {
    let root = Root::new();
    let file = root.files().join("notes.txt");
    std::fs::write(&file, b"original contents").unwrap();
    let opened = Arc::new(Mutex::new(Vec::<PathBuf>::new()));
    let observed = opened.clone();
    let opening = Opening {
        opener: Arc::new(move |path| {
            observed.lock().unwrap().push(path.into());
            Ok(())
        }),
        ..Default::default()
    };
    let session = session(&root, opening, 1);
    let client = session.client();
    let view = listing(&client, root.files()).await;
    assert!(!view.connected);
    client
        .open_local(file.display().to_string(), view.local_revision)
        .unwrap();
    wait(&client, |view| view.status.starts_with("Opened local file")).await;
    let copy = opened.lock().unwrap()[0].clone();
    assert!(copy.starts_with(root.cache()));
    assert_ne!(copy, file);
    assert_eq!(std::fs::read(&copy).unwrap(), b"original contents");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(root.cache())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&copy).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let large = root.files().join("large.bin");
    std::fs::write(&large, vec![1; 2 * 1024 * 1024]).unwrap();
    let view = listing(&client, root.files()).await;
    client
        .open_local(large.display().to_string(), view.local_revision)
        .unwrap();
    wait(&client, |view| view.status.contains("saved open limit")).await;
    assert_eq!(opened.lock().unwrap().len(), 1);
    session.configure(
        SftpTransferSettings {
            local_open_max_file_mib: 0,
            ..Default::default()
        },
        SftpTransferFilterSettings::default(),
    );
    client
        .open_local(large.display().to_string(), view.local_revision)
        .unwrap();
    wait(&client, |view| {
        view.status.starts_with("Opened local file large")
    })
    .await;
    assert_eq!(opened.lock().unwrap().len(), 2);
    assert_eq!(std::fs::read(&file).unwrap(), b"original contents");
    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn old_directories_and_replaced_regular_files_never_reach_the_opener() {
    let root = Root::new();
    let file = root.files().join("notes.txt");
    std::fs::write(&file, b"original").unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let session = session(
        &root,
        Opening {
            opener: Arc::new(move |_| {
                observed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
            ..Default::default()
        },
        1,
    );
    let client = session.client();
    let old = listing(&client, root.files()).await;
    let current = listing(&client, root.files()).await;
    client
        .open_local(file.display().to_string(), old.local_revision)
        .unwrap();
    wait(&client, |view| {
        view.status.contains("local directory changed")
    })
    .await;
    std::fs::remove_file(&file).unwrap();
    std::fs::write(&file, b"replaced contents").unwrap();
    client
        .open_local(file.display().to_string(), current.local_revision)
        .unwrap();
    wait(&client, |view| {
        view.status.contains("changed after it was listed")
    })
    .await;
    client
        .open_local(
            root.0.join("outside.txt").display().to_string(),
            current.local_revision,
        )
        .unwrap();
    wait(&client, |view| {
        view.status
            .contains("no longer in the local directory snapshot")
    })
    .await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!root.cache().exists());
    session.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_link_replacing_a_listed_file_is_rejected_before_copying() {
    let root = Root::new();
    let file = root.files().join("notes.txt");
    let outside = root.0.join("outside.txt");
    std::fs::write(&file, b"original").unwrap();
    std::fs::write(&outside, b"outside").unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let session = session(
        &root,
        Opening {
            opener: Arc::new(move |_| {
                observed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
            ..Default::default()
        },
        1,
    );
    let client = session.client();
    let view = listing(&client, root.files()).await;
    std::fs::remove_file(&file).unwrap();
    std::os::unix::fs::symlink(&outside, &file).unwrap();
    client
        .open_local(file.display().to_string(), view.local_revision)
        .unwrap();
    wait(&client, |view| view.status.contains("symbolic link")).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!root.cache().exists());
    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancelled_copies_are_removed_and_shutdown_retains_the_job_join() {
    for mode in 0..4 {
        let root = Root::new();
        let file = root.files().join("notes.txt");
        std::fs::write(&file, b"original").unwrap();
        let (started, began) = tokio::sync::oneshot::channel();
        let started = Mutex::new(Some(started));
        let (cancelled, cancelled_ack) = tokio::sync::oneshot::channel();
        let cancelled = Mutex::new(Some(cancelled));
        let (release, held) = std::sync::mpsc::sync_channel(1);
        let held = Mutex::new(held);
        let default_copy = Opening::default().copier;
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let opening = Opening {
            copier: Arc::new(move |file, root, limit, stop| {
                let path = default_copy(file, root, limit, stop)?;
                started
                    .lock()
                    .unwrap()
                    .take()
                    .unwrap()
                    .send(path.clone())
                    .unwrap();
                loop {
                    if stop.load(Ordering::Acquire)
                        && let Some(sender) = cancelled.lock().unwrap().take()
                    {
                        sender.send(()).unwrap();
                    }
                    match held.lock().unwrap().recv_timeout(Duration::from_millis(5)) {
                        Ok(()) => break,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                        Err(error) => panic!("release channel failed: {error}"),
                    }
                }
                Ok(path)
            }),
            opener: Arc::new(move |_| {
                observed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
            ..Default::default()
        };
        let session = session(&root, opening, 1);
        let client = session.client();
        let view = listing(&client, root.files()).await;
        client
            .open_local(file.display().to_string(), view.local_revision)
            .unwrap();
        let copy = tokio::time::timeout(Duration::from_secs(10), began)
            .await
            .unwrap()
            .unwrap();
        match mode {
            0 => {
                listing(&client, root.files()).await;
            }
            1 => session.configure(
                SftpTransferSettings {
                    local_open_max_file_mib: 2,
                    ..Default::default()
                },
                SftpTransferFilterSettings::default(),
            ),
            2 => client.disconnect().unwrap(),
            _ => {
                session.cancel();
            }
        }
        tokio::time::timeout(Duration::from_secs(10), cancelled_ack)
            .await
            .unwrap()
            .unwrap();
        assert!(copy.exists());
        if mode == 3 {
            let task = tokio::spawn(session.shutdown());
            tokio::task::yield_now().await;
            assert!(!task.is_finished());
            release.send(()).unwrap();
            task.await.unwrap().unwrap();
        } else {
            release.send(()).unwrap();
            wait(&client, |view| {
                !copy.exists() && !view.status.starts_with("Opening local file")
            })
            .await;
            if mode == 2 {
                assert_eq!(client.view().unwrap().status, "Disconnected");
            }
            session.shutdown().await.unwrap();
        }
        assert!(!copy.exists());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn opener_errors_are_visible_and_remove_the_unused_copy() {
    let root = Root::new();
    let file = root.files().join("notes.txt");
    std::fs::write(&file, b"original").unwrap();
    let session = session(
        &root,
        Opening {
            opener: Arc::new(|_| anyhow::bail!("fixture opener unavailable")),
            ..Default::default()
        },
        1,
    );
    let client = session.client();
    let view = listing(&client, root.files()).await;
    client
        .open_local(file.display().to_string(), view.local_revision)
        .unwrap();
    wait(&client, |view| {
        view.status.contains("fixture opener unavailable")
    })
    .await;
    assert_eq!(std::fs::read_dir(root.cache()).unwrap().count(), 0);
    session.shutdown().await.unwrap();
}
