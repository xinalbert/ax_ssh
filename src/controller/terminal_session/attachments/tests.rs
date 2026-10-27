use super::*;

fn png() -> Vec<u8> {
    crate::media::encode_clipboard_png(1, 1, &[23, 45, 67, 255]).unwrap()
}

#[tokio::test]
async fn image_admission_rejects_old_connections_unsupported_transports_and_session_limit() {
    let (mut worker, client) = super::super::clipboard::tests::fixture(false);
    let view = client.view().unwrap();
    let stale = client.for_view(&view);
    client.shared.lock().unwrap().view.generation = 2;
    assert!(stale.prepare_image(Uuid::new_v4(), png(), 0).is_err());
    client.shared.lock().unwrap().view.generation = 1;
    client.shared.lock().unwrap().view.identity = ConnectionIdentity::Telnet {
        profile_id: Uuid::new_v4(),
        attempt_id: None,
    };
    assert!(client.prepare_image(Uuid::new_v4(), png(), 0).is_err());
    client.shared.lock().unwrap().view.identity = ConnectionIdentity::Local;
    assert!(
        client
            .prepare_image(Uuid::new_v4(), vec![0; MAX_IMAGE_BYTES + 1], 0)
            .is_err()
    );
    let files = tokio::task::spawn_blocking(|| {
        (0..MAX_ATTACHMENTS)
            .map(|_| {
                let (owner, file) = PrivateMediaFile::create("png").unwrap();
                drop(file);
                owner
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap();
    let paths = files
        .iter()
        .map(PrivateMediaFile::output_path)
        .collect::<Vec<_>>();
    worker.attachments.files = files;
    let id = Uuid::new_v4();
    client.shared.lock().unwrap().image = Some(Ticket {
        id,
        generation: 1,
        revision: 0,
        ready: false,
        cancelled: false,
    });
    assert!(
        worker
            .prepare_image(id, png(), 0)
            .unwrap_err()
            .to_string()
            .contains("limit reached")
    );
    assert!(client.image_ready(id).unwrap().is_none());
    worker.stop_images().await.unwrap();
    assert!(paths.iter().all(|path| !path.exists()));
}

async fn ready(client: &TerminalClient, id: Uuid) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while client.image_ready(id).unwrap() != Some(true) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn local_image_waits_for_confirmation_and_retains_private_file_until_disconnect() {
    use std::os::unix::fs::PermissionsExt;
    let script = super::super::tests::two_line_reader_script();
    let session = TerminalSession::spawn(
        &Handle::current(),
        script.to_string_lossy().into_owned(),
        Arc::new(|| {}),
    );
    let client = session.client();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !super::super::tests::terminal_text(&client.view().unwrap()).contains("AX_EGUI_READY")
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let view = client.view().unwrap();
    let id = Uuid::new_v4();
    client
        .for_view(&view)
        .prepare_image(id, png(), view.selection_revision)
        .unwrap();
    assert!(
        client
            .prepare_image(Uuid::new_v4(), png(), view.selection_revision)
            .is_err()
    );
    ready(&client, id).await;
    assert!(
        !super::super::tests::terminal_text(&client.view().unwrap()).contains("AX_EGUI_FIRST:")
    );
    client.finish_image(id, true).unwrap();
    // A later newline is an independent user action; image paste must never submit it.
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(
        !super::super::tests::terminal_text(&client.view().unwrap()).contains("AX_EGUI_FIRST:")
    );
    client
        .key(TerminalKey::Return, TerminalModifiers::default())
        .unwrap();
    let text = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let text = super::super::tests::terminal_text(&client.view().unwrap());
            if text.contains("AX_EGUI_FIRST:") {
                break text;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let path = text
        .split("AX_EGUI_FIRST:")
        .nth(1)
        .unwrap()
        .split("input.png")
        .next()
        .unwrap()
        .to_owned()
        + "input.png";
    let path = path.trim_matches('\'');
    assert_eq!(std::fs::read(path).unwrap(), png());
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(std::path::Path::new(path).parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    client.disconnect().unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while std::path::Path::new(path).exists() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    session.shutdown().await.unwrap();
    std::fs::remove_file(script).unwrap();
}

#[tokio::test]
async fn cancellation_revokes_queued_confirmation_and_cleans_prepared_file() {
    let (mut worker, client) = super::super::clipboard::tests::fixture(false);
    let (file, handle) = tokio::task::spawn_blocking(|| PrivateMediaFile::create("png").unwrap())
        .await
        .unwrap();
    drop(handle);
    let path = file.output_path();
    let id = Uuid::new_v4();
    let ticket = Ticket {
        id,
        generation: 1,
        revision: 0,
        ready: true,
        cancelled: false,
    };
    client.shared.lock().unwrap().image = Some(ticket);
    worker.attachments.ticket = Some(ticket);
    worker.attachments.prepared = Some(Prepared {
        path: path.to_string_lossy().into_owned(),
        file: Some(file),
    });
    // Revoke synchronously even though the bounded queue can no longer accept an intent.
    assert!(client.finish_image(id, false).is_err());
    worker.finish_image(id, true).await.unwrap();
    assert!(!path.exists());
    assert!(client.image_ready(id).unwrap().is_none());
}

#[tokio::test]
async fn viewport_change_discards_late_file_and_shutdown_joins_running_file_job() {
    let (mut worker, client) = super::super::clipboard::tests::fixture(false);
    let id = Uuid::new_v4();
    let ticket = Ticket {
        id,
        generation: 1,
        revision: 0,
        ready: false,
        cancelled: false,
    };
    client.shared.lock().unwrap().image = Some(ticket);
    worker.attachments.ticket = Some(ticket);
    worker
        .viewport
        .resize(&mut worker.model, TerminalSize::model(90, 30));
    let (owner, file) = tokio::task::spawn_blocking(|| PrivateMediaFile::create("png").unwrap())
        .await
        .unwrap();
    drop(file);
    let path = owner.output_path();
    worker
        .image_prepared(Ok(Prepared {
            path: path.to_string_lossy().into_owned(),
            file: Some(owner),
        }))
        .await
        .unwrap();
    assert!(!path.exists());
    assert!(client.image_ready(id).unwrap().is_none());
    let (started, start) = tokio::sync::oneshot::channel();
    let (release, gate) = std::sync::mpsc::channel();
    let (path_tx, path_rx) = tokio::sync::oneshot::channel();
    worker.attachments.job = Some(Job::Local(tokio::task::spawn_blocking(move || {
        started.send(()).unwrap();
        gate.recv().unwrap();
        let (owner, file) = PrivateMediaFile::create("png")?;
        drop(file);
        path_tx.send(owner.output_path()).unwrap();
        Ok(Prepared {
            path: owner.path().to_string_lossy().into_owned(),
            file: Some(owner),
        })
    })));
    start.await.unwrap();
    let cleanup = tokio::spawn(async move { worker.stop_images().await });
    tokio::task::yield_now().await;
    assert!(!cleanup.is_finished());
    release.send(()).unwrap();
    let path = path_rx.await.unwrap();
    cleanup.await.unwrap().unwrap();
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn quoted_attachment_paths_are_one_literal_shell_argument() {
    let path = "/tmp/a 'quoted' $(touch SHOULD_NOT_EXIST);\n.png";
    let output = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(format!(
            "set -- {}; [ \"$#\" = 1 ] && printf '%s' \"$1\"",
            attachment_text(path)
        ))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, path.as_bytes());
}
