use super::*;
use russh_sftp::protocol::{File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode};

#[derive(Default)]
pub(super) struct Upload {
    bytes: Mutex<Vec<u8>>,
    path: Mutex<String>,
    closed: AtomicBool,
    removed: AtomicBool,
    fail: AtomicBool,
}
pub(super) struct FilePeer(pub(super) Arc<Upload>);

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: String::new(),
        language_tag: String::new(),
    }
}

impl russh_sftp::server::Handler for FilePeer {
    type Error = StatusCode;
    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }
    async fn realpath(&mut self, id: u32, _: String) -> Result<Name, StatusCode> {
        Ok(Name {
            id,
            files: vec![File::new(
                "/home/it's $(literal)",
                FileAttributes::default(),
            )],
        })
    }
    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        attrs: FileAttributes,
    ) -> Result<Status, StatusCode> {
        assert!(path.starts_with("/home/it's $(literal)/.axssh-attachment-"));
        assert_eq!(attrs.permissions, Some(0o700));
        Ok(ok(id))
    }
    async fn setstat(
        &mut self,
        id: u32,
        _: String,
        attrs: FileAttributes,
    ) -> Result<Status, StatusCode> {
        assert_eq!(attrs.permissions, Some(0o700));
        Ok(ok(id))
    }
    async fn open(
        &mut self,
        id: u32,
        path: String,
        flags: OpenFlags,
        attrs: FileAttributes,
    ) -> Result<Handle, StatusCode> {
        assert_eq!(attrs.permissions, Some(0o600));
        assert!(flags.contains(OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE));
        *self.0.path.lock().unwrap() = path;
        Ok(Handle {
            id,
            handle: "png".into(),
        })
    }
    async fn fsetstat(
        &mut self,
        id: u32,
        _: String,
        attrs: FileAttributes,
    ) -> Result<Status, StatusCode> {
        assert_eq!(attrs.permissions, Some(0o600));
        Ok(ok(id))
    }
    async fn write(
        &mut self,
        id: u32,
        _: String,
        offset: u64,
        bytes: Vec<u8>,
    ) -> Result<Status, StatusCode> {
        if self.0.fail.load(Ordering::SeqCst) {
            return Err(StatusCode::PermissionDenied);
        }
        let mut data = self.0.bytes.lock().unwrap();
        assert_eq!(offset, data.len() as u64);
        data.extend(bytes);
        Ok(ok(id))
    }
    async fn close(&mut self, id: u32, _: String) -> Result<Status, StatusCode> {
        self.0.closed.store(true, Ordering::SeqCst);
        Ok(ok(id))
    }
    async fn remove(&mut self, id: u32, path: String) -> Result<Status, StatusCode> {
        assert_eq!(path, *self.0.path.lock().unwrap());
        self.0.removed.store(true, Ordering::SeqCst);
        Ok(ok(id))
    }
    async fn rmdir(&mut self, id: u32, _: String) -> Result<Status, StatusCode> {
        Ok(ok(id))
    }
}

#[tokio::test]
async fn ssh_image_uploads_on_the_authenticated_connection_and_only_confirmed_path_reaches_pty() {
    let fixture = Fixture::new(true, usize::MAX).await;
    let upload = Arc::new(Upload::default());
    *fixture.observed.upload.lock().unwrap() = Some(upload.clone());
    let terminal = terminal();
    let client = terminal.client();
    client
        .connect(fixture.profile.clone(), Zeroizing::new(SECRET.into()))
        .unwrap();
    let view = wait(&client, |view| view.connected).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while !fixture.observed.input.lock().unwrap().ends_with(b"R") {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let before = fixture.observed.input.lock().unwrap().clone();
    let png = crate::media::encode_clipboard_png(1, 1, &[23, 45, 67, 255]).unwrap();
    let token = Uuid::new_v4();
    client
        .for_view(&view)
        .prepare_image(token, png.clone(), view.selection_revision)
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while client.image_ready(token).unwrap() != Some(true) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(*fixture.observed.input.lock().unwrap(), before);
    assert_eq!(*upload.bytes.lock().unwrap(), png);
    assert!(upload.closed.load(Ordering::SeqCst));
    assert_eq!(fixture.observed.authentications.load(Ordering::SeqCst), 1);
    client.finish_image(token, true).unwrap();
    wait(&client, |view| text(view).contains("clipboard.png")).await;
    let expected = super::super::attachments::attachment_text(&upload.path.lock().unwrap());
    assert_eq!(
        &fixture.observed.input.lock().unwrap()[before.len()..],
        expected.as_bytes()
    );
    terminal.shutdown().await.unwrap();
    assert!(upload.removed.load(Ordering::SeqCst));
    fixture.finish().await;
}

#[tokio::test]
async fn ssh_image_failure_never_inserts_path_and_keeps_shell_usable() {
    let fixture = Fixture::new(true, usize::MAX).await;
    let upload = Arc::new(Upload::default());
    upload.fail.store(true, Ordering::SeqCst);
    *fixture.observed.upload.lock().unwrap() = Some(upload.clone());
    let terminal = terminal();
    let client = terminal.client();
    client
        .connect(fixture.profile.clone(), Zeroizing::new(SECRET.into()))
        .unwrap();
    let view = wait(&client, |view| view.connected).await;
    let token = Uuid::new_v4();
    client
        .prepare_image(
            token,
            crate::media::encode_clipboard_png(1, 1, &[0, 0, 0, 255]).unwrap(),
            view.selection_revision,
        )
        .unwrap();
    wait(&client, |view| {
        view.status.contains("cannot upload clipboard image")
    })
    .await;
    assert!(client.image_ready(token).unwrap().is_none());
    assert!(
        !String::from_utf8_lossy(&fixture.observed.input.lock().unwrap()).contains("clipboard.png")
    );
    client.paste("shell-still-usable".into()).unwrap();
    wait(&client, |view| text(view).contains("shell-still-usable")).await;
    terminal.shutdown().await.unwrap();
    assert!(upload.removed.load(Ordering::SeqCst));
    fixture.finish().await;
}
