use super::*;
use russh_sftp::protocol::{File, Handle, Name, Status, StatusCode};
use std::sync::{Arc, Mutex as StdMutex};

#[derive(Default)]
struct UploadState {
    path: String,
    bytes: Vec<u8>,
    closed: bool,
    fail_write: bool,
}
struct UploadServer(Arc<StdMutex<UploadState>>);
fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: String::new(),
        language_tag: String::new(),
    }
}
impl russh_sftp::server::Handler for UploadServer {
    type Error = StatusCode;
    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }
    async fn realpath(&mut self, id: u32, _path: String) -> std::result::Result<Name, StatusCode> {
        Ok(Name {
            id,
            files: vec![File::new("/home/test", FileAttributes::default())],
        })
    }
    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        attrs: FileAttributes,
    ) -> std::result::Result<Status, StatusCode> {
        if !path.starts_with("/home/test/.axssh-attachment-") || attrs.permissions != Some(0o700) {
            return Err(StatusCode::PermissionDenied);
        }
        Ok(ok(id))
    }
    async fn setstat(
        &mut self,
        id: u32,
        _path: String,
        attrs: FileAttributes,
    ) -> std::result::Result<Status, StatusCode> {
        if attrs.permissions != Some(0o700) {
            return Err(StatusCode::PermissionDenied);
        }
        Ok(ok(id))
    }
    async fn open(
        &mut self,
        id: u32,
        filename: String,
        flags: OpenFlags,
        attrs: FileAttributes,
    ) -> std::result::Result<Handle, StatusCode> {
        if !flags.contains(OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE)
            || attrs.permissions != Some(0o600)
        {
            return Err(StatusCode::PermissionDenied);
        }
        self.0.lock().map_err(|_| StatusCode::Failure)?.path = filename;
        Ok(Handle {
            id,
            handle: "image".into(),
        })
    }
    async fn fsetstat(
        &mut self,
        id: u32,
        _handle: String,
        attrs: FileAttributes,
    ) -> std::result::Result<Status, StatusCode> {
        if attrs.permissions != Some(0o600) {
            return Err(StatusCode::PermissionDenied);
        }
        Ok(ok(id))
    }
    async fn write(
        &mut self,
        id: u32,
        _handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> std::result::Result<Status, StatusCode> {
        let mut state = self.0.lock().map_err(|_| StatusCode::Failure)?;
        if state.fail_write {
            return Err(StatusCode::PermissionDenied);
        }
        if offset != state.bytes.len() as u64 {
            return Err(StatusCode::BadMessage);
        }
        state.bytes.extend(data);
        Ok(ok(id))
    }
    async fn close(&mut self, id: u32, _handle: String) -> std::result::Result<Status, StatusCode> {
        self.0.lock().map_err(|_| StatusCode::Failure)?.closed = true;
        Ok(ok(id))
    }
}

#[tokio::test]
async fn attachment_upload_acknowledges_every_chunk_and_private_permissions() {
    let state = Arc::new(StdMutex::new(UploadState::default()));
    let (client, server) = tokio::io::duplex(64 * 1024);
    let task = tokio::spawn(russh_sftp::server::run(server, UploadServer(state.clone())));
    let directories = Arc::new(Mutex::new(Vec::new()));
    let bytes = vec![123; 100_003]; // crosses several SFTP packet boundaries
    let path = upload_image_stream(client, bytes.clone(), directories.clone())
        .await
        .unwrap();
    {
        let uploaded = state.lock().unwrap();
        assert!(uploaded.closed);
        assert_eq!(uploaded.bytes, bytes);
        assert_eq!(uploaded.path, path);
    }
    assert_eq!(directories.lock().await.len(), 1);
    task.abort();
}

#[tokio::test]
async fn failed_attachment_keeps_cleanup_target_and_returns_no_path() {
    let state = Arc::new(StdMutex::new(UploadState {
        fail_write: true,
        ..UploadState::default()
    }));
    let (client, server) = tokio::io::duplex(64 * 1024);
    let task = tokio::spawn(russh_sftp::server::run(server, UploadServer(state)));
    let directories = Arc::new(Mutex::new(Vec::new()));
    assert!(
        upload_image_stream(client, vec![123; 100], directories.clone())
            .await
            .is_err()
    );
    assert_eq!(directories.lock().await.len(), 1);
    task.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn microphone_helper_streams_pcm_and_restores_source_on_eof() {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::AsyncWriteExt;
    let root = std::env::temp_dir().join(format!("axssh-audio-test-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(root.join("bin")).unwrap();
    std::fs::write(root.join("default"), "original").unwrap();
    let uname = "#!/bin/sh\necho Linux\n";
    let pactl = r#"#!/bin/sh
case "$1" in
get-default-source)
    if [ -f "$AXSSH_TEST_ROOT/external-change" ] && [ "$(cat "$AXSSH_TEST_ROOT/default")" = axssh_fixture ]; then
        printf external > "$AXSSH_TEST_ROOT/default"
    fi
    cat "$AXSSH_TEST_ROOT/default"
    ;;
set-default-source) printf '%s' "$2" > "$AXSSH_TEST_ROOT/default" ;;
load-module)
    printf '%s\n' "$@" > "$AXSSH_TEST_ROOT/arguments"
    for argument in "$@"; do case "$argument" in file=*) fifo=${argument#file=} ;; esac; done
    cat "$fifo" > "$AXSSH_TEST_ROOT/received" &
    echo 123
    ;;
unload-module) echo "$2" > "$AXSSH_TEST_ROOT/unloaded" ;;
esac
"#;
    for (name, script) in [("uname", uname), ("pactl", pactl)] {
        let path = root.join("bin").join(name);
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    for (rate, channels) in [(8000, 1), (44100, 2), (96000, 6), (192000, 8)] {
        if channels == 8 {
            std::fs::write(root.join("external-change"), "").unwrap();
        }
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![root.join("bin")];
        paths.extend(std::env::split_paths(&path));
        let script =
            format!("source=axssh_fixture\nrate={rate}\nchannels={channels}\n{AUDIO_HELPER}");
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", &script])
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("TMPDIR", &root)
            .env("AXSSH_TEST_ROOT", &root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let data = vec![0x5a; 4096];
        child.stdin.take().unwrap().write_all(&data).await.unwrap();
        let output = tokio::time::timeout(Duration::from_secs(5), child.wait_with_output())
            .await
            .unwrap()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"AXSSH_AUDIO_READY\n");
        assert_eq!(std::fs::read(root.join("received")).unwrap(), data);
        assert_eq!(
            std::fs::read_to_string(root.join("default")).unwrap(),
            if channels == 8 {
                "external"
            } else {
                "original"
            }
        );
        let arguments = std::fs::read_to_string(root.join("arguments")).unwrap();
        assert!(arguments.contains(&format!("rate={rate}")));
        assert!(arguments.contains(&format!("channels={channels}")));
        assert_eq!(
            std::fs::read_to_string(root.join("unloaded"))
                .unwrap()
                .trim(),
            "123"
        );
        assert!(!std::fs::read_dir(&root).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("axssh-audio.")
        }));
    }
    std::fs::remove_dir_all(root).unwrap();
}
