//! Loopback SSH/SFTP peers exercise the worker without system trust or devices.

use super::*;
use crate::ssh::{ClientHandler, FingerprintObservation, client_config, known_hosts};
use rand::{SeedableRng, rngs::StdRng};
use russh::{Channel, ChannelId, keys::PrivateKey, server};
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle as FileHandle, Name, OpenFlags, Status, StatusCode,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, Semaphore};
use tokio::task::JoinSet;

const CONTENT: &[u8] = b"worker download preserves every byte\n";

struct RemoteFiles {
    files: Mutex<HashMap<String, Vec<u8>>>,
    directories: Mutex<std::collections::HashSet<String>>,
    read_started: Notify,
    read_gate: Semaphore,
}

struct Peer {
    channels: HashMap<ChannelId, Channel<server::Msg>>,
    tasks: JoinSet<()>,
    files: Arc<RemoteFiles>,
    reject_subsystem: bool,
}

impl server::Handler for Peer {
    type Error = russh::Error;

    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<server::Auth, Self::Error> {
        Ok(if user == "fixture" && password == "fixture-ephemeral" {
            server::Auth::Accept
        } else {
            server::Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        self.channels.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        id: ChannelId,
        name: &str,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        if name != "sftp" || self.reject_subsystem {
            session.channel_failure(id)?;
            session.close(id)?;
        } else {
            let channel = self.channels.remove(&id).expect("accepted fixture channel");
            session.channel_success(id)?;
            let files = self.files.clone();
            self.tasks.spawn(async move {
                russh_sftp::server::run(
                    channel.into_stream(),
                    FilePeer {
                        files,
                        listed: false,
                    },
                )
                .await;
            });
        }
        Ok(())
    }

    async fn exec_request(
        &mut self,
        id: ChannelId,
        _: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_failure(id)?;
        session.close(id)?;
        Ok(())
    }
}

struct FilePeer {
    files: Arc<RemoteFiles>,
    listed: bool,
}

fn attributes(path: &str, files: &RemoteFiles) -> Result<FileAttributes, StatusCode> {
    let mut attrs = FileAttributes::empty();
    if path == "/remote" || files.directories.lock().unwrap().contains(path) {
        attrs.set_dir(true);
    } else {
        let files = files.files.lock().expect("fixture files");
        let bytes = files.get(path).ok_or(StatusCode::NoSuchFile)?;
        attrs.set_regular(true);
        attrs.size = Some(bytes.len() as u64);
    }
    attrs.mtime = Some(1234);
    Ok(attrs)
}

fn success(id: u32) -> Status {
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
    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _: FileAttributes,
    ) -> Result<Status, StatusCode> {
        self.files.directories.lock().unwrap().insert(path);
        Ok(success(id))
    }

    async fn init(
        &mut self,
        _: u32,
        _: HashMap<String, String>,
    ) -> Result<russh_sftp::protocol::Version, StatusCode> {
        let mut version = russh_sftp::protocol::Version::new();
        version
            .extensions
            .insert("posix-rename@openssh.com".into(), "1".into());
        Ok(version)
    }

    async fn extended(
        &mut self,
        id: u32,
        name: String,
        data: Vec<u8>,
    ) -> Result<russh_sftp::protocol::Packet, StatusCode> {
        if name != "posix-rename@openssh.com" {
            return Err(StatusCode::OpUnsupported);
        }
        let encoded = russh_sftp::ser::to_bytes(&data).map_err(|_| StatusCode::BadMessage)?;
        let mut payload = encoded.slice(4..);
        let (old, new): (String, String) =
            russh_sftp::de::from_bytes(&mut payload).map_err(|_| StatusCode::BadMessage)?;
        Ok(russh_sftp::protocol::Packet::Status(
            self.rename(id, old, new).await?,
        ))
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, StatusCode> {
        let path = if path == "." || path == "~" {
            "/remote".to_owned()
        } else {
            path
        };
        Ok(Name {
            id,
            files: vec![File::dummy(&path)],
        })
    }
    async fn opendir(&mut self, id: u32, path: String) -> Result<FileHandle, StatusCode> {
        if path != "/remote" && !self.files.directories.lock().unwrap().contains(&path) {
            return Err(StatusCode::PermissionDenied);
        }
        self.listed = false;
        Ok(FileHandle { id, handle: path })
    }
    async fn readdir(&mut self, id: u32, directory: String) -> Result<Name, StatusCode> {
        if self.listed {
            return Err(StatusCode::Eof);
        }
        self.listed = true;
        let mut names = self
            .files
            .files
            .lock()
            .expect("fixture files")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        names.extend(self.files.directories.lock().unwrap().iter().cloned());
        Ok(Name {
            id,
            files: names
                .iter()
                .filter(|path| {
                    path.rsplit_once('/')
                        .is_some_and(|(parent, _)| parent == directory)
                })
                .map(|path| {
                    File::new(
                        path.rsplit('/').next().expect("filename"),
                        attributes(path, &self.files).expect("file metadata"),
                    )
                })
                .collect(),
        })
    }
    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        Ok(Attrs {
            id,
            attrs: attributes(&path, &self.files)?,
        })
    }
    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        self.lstat(id, path).await
    }
    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, StatusCode> {
        self.lstat(id, handle).await
    }
    async fn open(
        &mut self,
        id: u32,
        path: String,
        flags: OpenFlags,
        _: FileAttributes,
    ) -> Result<FileHandle, StatusCode> {
        let mut files = self.files.files.lock().expect("fixture files");
        if flags.contains(OpenFlags::CREATE) {
            files.entry(path.clone()).or_default();
        }
        let bytes = files.get_mut(&path).ok_or(StatusCode::NoSuchFile)?;
        if flags.contains(OpenFlags::TRUNCATE) {
            bytes.clear();
        }
        Ok(FileHandle { id, handle: path })
    }
    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, StatusCode> {
        if handle.ends_with("slow.bin") {
            self.files.read_started.notify_one();
            self.files
                .read_gate
                .acquire()
                .await
                .expect("read gate")
                .forget();
        }
        let files = self.files.files.lock().expect("fixture files");
        let bytes = files.get(&handle).ok_or(StatusCode::NoSuchFile)?;
        let start = offset as usize;
        if start >= bytes.len() {
            return Err(StatusCode::Eof);
        }
        Ok(Data {
            id,
            data: bytes[start..(start + len as usize).min(bytes.len())].to_vec(),
        })
    }
    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, StatusCode> {
        let mut files = self.files.files.lock().expect("fixture files");
        let bytes = files.get_mut(&handle).ok_or(StatusCode::NoSuchFile)?;
        let start = offset as usize;
        bytes.resize(bytes.len().max(start + data.len()), 0);
        bytes[start..start + data.len()].copy_from_slice(&data);
        Ok(success(id))
    }
    async fn fsetstat(
        &mut self,
        id: u32,
        _: String,
        _: FileAttributes,
    ) -> Result<Status, StatusCode> {
        Ok(success(id))
    }
    async fn close(&mut self, id: u32, _: String) -> Result<Status, StatusCode> {
        Ok(success(id))
    }
    async fn rename(&mut self, id: u32, old: String, new: String) -> Result<Status, StatusCode> {
        let mut files = self.files.files.lock().expect("fixture files");
        let bytes = files.remove(&old).ok_or(StatusCode::NoSuchFile)?;
        files.insert(new, bytes);
        Ok(success(id))
    }
    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, StatusCode> {
        let prefix = format!("{path}/");
        if self
            .files
            .files
            .lock()
            .unwrap()
            .keys()
            .any(|item| item.starts_with(&prefix))
        {
            return Err(StatusCode::Failure);
        }
        let mut directories = self.files.directories.lock().unwrap();
        if directories.iter().any(|item| item.starts_with(&prefix)) {
            return Err(StatusCode::Failure);
        }
        if !directories.remove(&path) {
            return Err(StatusCode::NoSuchFile);
        }
        Ok(success(id))
    }

    async fn remove(&mut self, id: u32, path: String) -> Result<Status, StatusCode> {
        self.files
            .files
            .lock()
            .expect("fixture files")
            .remove(&path)
            .ok_or(StatusCode::NoSuchFile)?;
        Ok(success(id))
    }
}

pub(crate) struct Fixture {
    profile: SessionProfile,
    files: Arc<RemoteFiles>,
    server: JoinHandle<Result<(), russh::Error>>,
    directory: PathBuf,
}

impl Fixture {
    pub(crate) async fn new(reject_subsystem: bool) -> Self {
        let mut rng = StdRng::seed_from_u64(601);
        let key = PrivateKey::random(&mut rng, russh::keys::Algorithm::Ed25519)
            .expect("fixture host key");
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("loopback listener");
        let address = listener.local_addr().expect("listener address");
        let mut profile = SessionProfile::new("Fixture", address.ip().to_string(), "fixture");
        let ssh = profile.ssh_mut().expect("SSH profile");
        ssh.port = address.port();
        ssh.host_key_fingerprint = Some(
            key.public_key()
                .fingerprint(russh::keys::HashAlg::Sha256)
                .to_string(),
        );
        ssh.sftp_remote_path = "/remote".into();
        let files = Arc::new(RemoteFiles {
            directories: Mutex::new(std::collections::HashSet::new()),
            files: Mutex::new(HashMap::from([
                ("/remote/file.txt".into(), CONTENT.to_vec()),
                ("/remote/slow.bin".into(), vec![42; 256 * 1024]),
            ])),
            read_started: Notify::new(),
            read_gate: Semaphore::new(0),
        });
        let server_files = files.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("loopback accept");
            let config = Arc::new(server::Config {
                keys: vec![key],
                auth_rejection_time: Duration::ZERO,
                auth_rejection_time_initial: Some(Duration::ZERO),
                ..Default::default()
            });
            let session = server::run_stream(
                config,
                stream,
                Peer {
                    channels: HashMap::new(),
                    tasks: JoinSet::new(),
                    files: server_files,
                    reject_subsystem,
                },
            )
            .await?;
            session.await
        });
        let directory = std::env::temp_dir().join(format!("axssh-worker-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).expect("isolated directory");
        Self {
            profile,
            files,
            server,
            directory,
        }
    }

    #[cfg(feature = "egui-ui")]
    pub(crate) fn add_directory(&self, path: &str) {
        self.files.directories.lock().unwrap().insert(path.into());
    }
    #[cfg(feature = "egui-ui")]
    pub(crate) fn has_directory(&self, path: &str) -> bool {
        self.files.directories.lock().unwrap().contains(path)
    }
    #[cfg(feature = "egui-ui")]
    pub(crate) fn contains(&self, path: &str) -> bool {
        self.files.files.lock().unwrap().contains_key(path)
    }
    #[cfg(feature = "egui-ui")]
    pub(crate) fn contents(&self, path: &str) -> Vec<u8> {
        self.files.files.lock().unwrap()[path].clone()
    }
    #[cfg(feature = "egui-ui")]
    pub(crate) fn replace(&self, path: &str, data: &[u8]) {
        self.files
            .files
            .lock()
            .unwrap()
            .insert(path.into(), data.to_vec());
    }
    #[cfg(feature = "egui-ui")]
    pub(crate) async fn slow_read_started(&self) {
        self.files.read_started.notified().await;
    }
    #[cfg(feature = "egui-ui")]
    pub(crate) fn release_slow_read(&self) {
        self.files.read_gate.add_permits(64);
    }

    pub(crate) async fn worker(&self) -> (SshSessionHandle, mpsc::Receiver<SshSessionEvent>) {
        // Supply a known-hosts snapshot, preserving the real Handler trust check.
        let ssh = self.profile.ssh().expect("SSH profile");
        let socket = TcpStream::connect((ssh.host.as_str(), ssh.port))
            .await
            .expect("loopback connect");
        let handler = ClientHandler::new(
            ssh.host_key_fingerprint.clone(),
            known_hosts::Snapshot::default(),
            ssh.host.clone(),
            ssh.port,
            FingerprintObservation::default(),
            None,
        );
        let mut transport = russh::client::connect_stream(client_config(), socket, handler)
            .await
            .expect("pinned host handshake");
        assert!(
            transport
                .authenticate_password("fixture", "fixture-ephemeral")
                .await
                .expect("fixture authentication")
                .success()
        );
        let connection = SshConnection {
            handle: Arc::new(transport),
        };
        let (command_tx, command_rx) = mpsc::channel(COMMAND_CAPACITY);
        let (event_tx, event_rx) = mpsc::channel(EVENT_CAPACITY);
        let (latency_foreground_tx, latency_rx) = watch::channel(false);
        let policy = crate::config::SftpTransferSettings::default().policy(None);
        let id = Uuid::new_v4();
        let task = tokio::spawn(run_sftp_session(
            connection,
            id,
            "/remote".into(),
            command_rx,
            event_tx,
            policy,
            latency_rx,
        ));
        (
            SshSessionHandle {
                session_id: id,
                command_tx,
                resize_tx: watch::channel(TerminalSize::backend(1, 1)).0,
                latency_foreground_tx,
                next_input_sequence: AtomicU64::new(1),
                task,
                sftp_max_file_bytes: policy.max_file_bytes(),
            },
            event_rx,
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
        std::fs::remove_dir_all(&self.directory).expect("remove isolated directory");
    }
}

async fn until(
    events: &mut mpsc::Receiver<SshSessionEvent>,
    predicate: impl Fn(&SshSessionEvent) -> bool,
) -> SshSessionEvent {
    timeout(Duration::from_secs(10), async {
        loop {
            let event = events.recv().await.expect("worker event stream");
            if predicate(&event) {
                return event;
            }
            assert!(
                !matches!(
                    event,
                    SshSessionEvent::Failed(_)
                        | SshSessionEvent::SftpTransfer(SftpTransferEvent::Failed { .. })
                        | SshSessionEvent::SftpWrite(SftpWriteEvent::Failed { .. })
                ),
                "unexpected worker failure: {event:?}"
            );
        }
    })
    .await
    .expect("worker should make progress")
}

#[tokio::test]
async fn sftp_worker_lists_downloads_uploads_and_executes_writes() {
    let fixture = Fixture::new(false).await;
    let (worker, mut events) = fixture.worker().await;
    until(&mut events, |event| {
        matches!(
            event,
            SshSessionEvent::Sftp(SftpBrowserEvent::DirectoryPage { .. })
        )
    })
    .await;
    worker
        .request_list_sftp(71, "/remote".into())
        .expect("navigate");
    let page = until(&mut events, |event| {
        matches!(
            event,
            SshSessionEvent::Sftp(SftpBrowserEvent::DirectoryPage {
                request_id: Some(71),
                ..
            })
        )
    })
    .await;
    assert!(
        matches!(page, SshSessionEvent::Sftp(SftpBrowserEvent::DirectoryPage { entries, .. }) if entries.iter().any(|entry| entry.name == "file.txt"))
    );
    let download = Uuid::new_v4();
    worker
        .request_open_sftp_file_at_local_path(
            download,
            "/remote/file.txt".into(),
            fixture.directory.join("download.txt"),
            CONTENT.len() as u64,
        )
        .expect("download");
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Completed { transfer_id, .. }) if *transfer_id == download)).await;
    assert_eq!(
        std::fs::read(fixture.directory.join("download.txt")).expect("download bytes"),
        CONTENT
    );
    let local = fixture.directory.join("upload.txt");
    std::fs::write(&local, b"uploaded bytes").expect("fixture upload source");
    let upload = Uuid::new_v4();
    worker
        .request_open_sftp_upload_batch(
            Uuid::new_v4(),
            vec![(upload, "/remote/upload.txt".into(), local, 14, vec![])],
        )
        .expect("upload");
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Completed { transfer_id, .. }) if *transfer_id == upload)).await;
    assert_eq!(
        fixture
            .files
            .files
            .lock()
            .expect("remote files")
            .get("/remote/upload.txt")
            .expect("published remote file"),
        b"uploaded bytes"
    );
    let operation = Uuid::new_v4();
    worker
        .request_sftp_write(
            operation,
            SftpWriteOperation::Rename {
                old_path: "/remote/upload.txt".into(),
                new_path: "/remote/renamed.txt".into(),
            },
        )
        .expect("rename");
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpWrite(SftpWriteEvent::Completed { operation_id, .. }) if *operation_id == operation)).await;
    assert!(
        !fixture
            .files
            .files
            .lock()
            .expect("remote files")
            .contains_key("/remote/upload.txt")
    );
    let operation = Uuid::new_v4();
    worker
        .request_sftp_write(
            operation,
            SftpWriteOperation::Remove {
                path: "/remote/renamed.txt".into(),
                directory: false,
            },
        )
        .expect("remove");
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpWrite(SftpWriteEvent::Completed { operation_id, .. }) if *operation_id == operation)).await;
    assert!(
        !fixture
            .files
            .files
            .lock()
            .expect("remote files")
            .contains_key("/remote/renamed.txt")
    );
    worker.request_system_release().expect("release probe");
    until(&mut events, |event| {
        matches!(event, SshSessionEvent::SystemRelease(None))
    })
    .await;
    worker.shutdown().await.expect("worker shutdown");
    assert!(matches!(
        until(&mut events, |event| matches!(
            event,
            SshSessionEvent::Disconnected
        ))
        .await,
        SshSessionEvent::Disconnected
    ));
}

#[tokio::test]
async fn sftp_worker_cancels_in_flight_reads_and_cleans_partial_files() {
    let fixture = Fixture::new(false).await;
    let (worker, mut events) = fixture.worker().await;
    until(&mut events, |event| {
        matches!(event, SshSessionEvent::Connected)
    })
    .await;
    let id = Uuid::new_v4();
    worker
        .request_open_sftp_file_at_local_path(
            id,
            "/remote/slow.bin".into(),
            fixture.directory.join("slow.bin"),
            256 * 1024,
        )
        .expect("blocked download");
    timeout(
        Duration::from_secs(10),
        fixture.files.read_started.notified(),
    )
    .await
    .expect("real read request started");
    worker
        .request_cancel_sftp_transfer(id)
        .expect("cancel active read");
    // Commands are FIFO. An independent exec reply proves cancellation was
    // consumed before we allow the outstanding read/close requests to finish.
    worker
        .request_system_release()
        .expect("cancellation barrier");
    until(&mut events, |event| {
        matches!(event, SshSessionEvent::SystemRelease(None))
    })
    .await;
    fixture.files.read_gate.add_permits(64);
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Cancelled { transfer_id }) if *transfer_id == id)).await;
    worker
        .shutdown()
        .await
        .expect("worker and transfer should join");
    assert_eq!(
        std::fs::read_dir(&fixture.directory)
            .expect("isolated directory")
            .count(),
        0
    );
}

#[tokio::test]
async fn sftp_worker_reports_subsystem_rejection_without_connected_event() {
    let fixture = Fixture::new(true).await;
    let (worker, mut events) = fixture.worker().await;
    assert!(matches!(
        events.recv().await,
        Some(SshSessionEvent::Authenticated)
    ));
    timeout(Duration::from_secs(10), async {
        loop {
            let event = events.recv().await.expect("rejected subsystem event");
            assert!(!matches!(event, SshSessionEvent::Connected));
            if matches!(event, SshSessionEvent::Failed(_)) {
                break;
            }
        }
    })
    .await
    .expect("rejection reaches the worker");
    worker.shutdown().await.expect("failed worker joins");
}

#[tokio::test]
async fn sftp_worker_conflict_decisions_and_guarded_edits_preserve_remote_content() {
    let fixture = Fixture::new(false).await;
    let (worker, mut events) = fixture.worker().await;
    until(&mut events, |event| {
        matches!(event, SshSessionEvent::Connected)
    })
    .await;
    let local = fixture.directory.join("upload.txt");
    std::fs::write(&local, b"replacement").expect("isolated upload bytes");
    for choice in [
        SftpUploadConflictChoice::Skip,
        SftpUploadConflictChoice::Overwrite,
        SftpUploadConflictChoice::KeepBoth,
    ] {
        let id = Uuid::new_v4();
        worker
            .request_open_sftp_upload_batch(
                Uuid::new_v4(),
                vec![(id, "/remote/file.txt".into(), local.clone(), 11, vec![])],
            )
            .expect("conflicting upload");
        until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::UploadConflict { transfer_id, .. }) if *transfer_id == id)).await;
        worker
            .request_resolve_sftp_upload_conflict(id, choice, true)
            .expect("explicit conflict decision");
        let outcome = until(&mut events, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Completed { transfer_id, .. } | SftpTransferEvent::Skipped { transfer_id }) if *transfer_id == id)).await;
        if choice == SftpUploadConflictChoice::Skip {
            assert!(matches!(
                outcome,
                SshSessionEvent::SftpTransfer(SftpTransferEvent::Skipped { .. })
            ));
            assert_eq!(
                fixture
                    .files
                    .files
                    .lock()
                    .expect("remote files")
                    .get("/remote/file.txt")
                    .expect("original file"),
                CONTENT
            );
        } else {
            assert!(matches!(
                outcome,
                SshSessionEvent::SftpTransfer(SftpTransferEvent::Completed { .. })
            ));
            assert_eq!(
                fixture
                    .files
                    .files
                    .lock()
                    .expect("remote files")
                    .get("/remote/file.txt")
                    .expect("published file"),
                b"replacement"
            );
        }
    }
    assert!(
        fixture
            .files
            .files
            .lock()
            .expect("remote files")
            .keys()
            .any(|path| path.contains("file (1)")),
        "keep-both publishes a separate name"
    );
    let id = Uuid::new_v4();
    worker
        .request_sftp_write(
            id,
            SftpWriteOperation::ReadEditFile {
                path: "/remote/file.txt".into(),
                max_bytes: 4 * 1024 * 1024,
            },
        )
        .expect("read edit copy");
    let edit = until(&mut events, |event| matches!(event, SshSessionEvent::SftpWrite(SftpWriteEvent::EditFile { operation_id, .. }) if *operation_id == id)).await;
    assert!(
        matches!(edit, SshSessionEvent::SftpWrite(SftpWriteEvent::EditFile { data, expected_size: 11, .. }) if data == b"replacement")
    );
    let id = Uuid::new_v4();
    worker
        .request_sftp_write(
            id,
            SftpWriteOperation::WriteEditFile {
                path: "/remote/file.txt".into(),
                max_bytes: 4 * 1024 * 1024,
                data: b"unsafe overwrite".to_vec(),
                expected_size: 999,
                expected_modified: Some(1234),
            },
        )
        .expect("stale edit request");
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpWrite(SftpWriteEvent::Failed { operation_id, .. }) if *operation_id == id)).await;
    assert_eq!(
        fixture
            .files
            .files
            .lock()
            .expect("remote files")
            .get("/remote/file.txt")
            .expect("protected file"),
        b"replacement"
    );
    let id = Uuid::new_v4();
    worker
        .request_sftp_write(
            id,
            SftpWriteOperation::WriteEditFile {
                path: "/remote/file.txt".into(),
                max_bytes: 4 * 1024 * 1024,
                data: b"edited safely".to_vec(),
                expected_size: 11,
                expected_modified: Some(1234),
            },
        )
        .expect("guarded atomic edit");
    until(&mut events, |event| matches!(event, SshSessionEvent::SftpWrite(SftpWriteEvent::Updated { operation_id, size: 13, .. }) if *operation_id == id)).await;
    assert_eq!(
        fixture
            .files
            .files
            .lock()
            .expect("remote files")
            .get("/remote/file.txt")
            .expect("edited remote file"),
        b"edited safely"
    );
    worker
        .request_list_sftp(88, "/denied".into())
        .expect("denied navigation");
    until(&mut events, |event| {
        matches!(
            event,
            SshSessionEvent::Sftp(SftpBrowserEvent::Failed {
                request_id: Some(88),
                ..
            })
        )
    })
    .await;
    worker
        .shutdown()
        .await
        .expect("worker remains available for shutdown after errors");
}

#[tokio::test]
async fn failed_worker_rejects_commands_and_preserves_request_limits() {
    let profile = SessionProfile::new("Invalid", "", "fixture");
    let policy = crate::config::SftpTransferSettings::default().policy(None);
    let (worker, mut events) = SshSessionHandle::spawn_sftp(
        &Handle::current(),
        Uuid::new_v4(),
        profile,
        Zeroizing::new(String::new()),
        policy,
    );
    assert!(matches!(
        events.recv().await,
        Some(SshSessionEvent::Failed(_))
    ));
    assert!(events.recv().await.is_none());
    assert!(worker.is_finished());
    assert_eq!(worker.sftp_max_file_bytes(), policy.max_file_bytes());
    assert!(worker.request_resize(80, 24).is_err());
    assert!(worker.request_resize(0, 24).is_err());
    assert!(worker.request_send(Vec::new()).is_ok());
    assert!(worker.request_send_paste(Vec::new()).is_ok());
    assert!(
        worker
            .request_send_motion(Vec::new())
            .expect("empty motion")
    );
    assert!(worker.request_send(vec![0; MAX_INPUT_BYTES + 1]).is_err());
    assert!(
        worker
            .request_send_paste(vec![0; TERMINAL_PASTE_MAX_BYTES + 1])
            .is_err()
    );
    assert!(
        worker
            .request_send_motion(vec![0; MAX_INPUT_BYTES + 1])
            .is_err()
    );
    assert!(worker.request_send(vec![1]).is_err());
    assert!(worker.request_send_paste(vec![1]).is_err());
    assert!(worker.request_send_motion(vec![1]).is_err());
    assert!(worker.request_clipboard_image(b"not PNG".to_vec()).is_err());
    assert!(worker.request_list_sftp(1, "\0".into()).is_err());
    assert!(worker.request_disconnect().is_err());
    worker.shutdown().await.expect("failed worker joins");
}

#[tokio::test]
async fn real_host_key_handler_denies_unknown_mismatched_changed_and_revoked_keys() {
    use russh::client::Handler as _;
    let mut rng = StdRng::seed_from_u64(609);
    let key = PrivateKey::random(&mut rng, russh::keys::Algorithm::Ed25519).expect("fixture key");
    let other =
        PrivateKey::random(&mut rng, russh::keys::Algorithm::Ed25519).expect("other fixture key");
    let actual = key
        .public_key()
        .fingerprint(russh::keys::HashAlg::Sha256)
        .to_string();
    let root = std::env::temp_dir().join(format!("axssh-trust-handler-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).expect("isolated trust directory");
    let path = root.join("known_hosts");
    let public = key.public_key().to_openssh().expect("fixture public key");
    let changed = other.public_key().to_openssh().expect("changed public key");
    for (expected, record, accepted) in [
        (None, String::new(), false),
        (Some("SHA256:wrong".to_owned()), String::new(), false),
        (Some(actual.clone()), String::new(), true),
        (None, format!("fixture.invalid {public}\n"), true),
        (
            Some(actual.clone()),
            format!("@revoked fixture.invalid {public}\n"),
            false,
        ),
        (
            Some(actual.clone()),
            format!("fixture.invalid {changed}\n"),
            false,
        ),
    ] {
        std::fs::write(&path, record).expect("isolated public trust record");
        let snapshot =
            known_hosts::Snapshot::load_from_path(&path).expect("deterministic trust snapshot");
        let observation = FingerprintObservation::default();
        let mut handler = ClientHandler::new(
            expected,
            snapshot,
            "fixture.invalid".into(),
            22,
            observation.clone(),
            None,
        );
        assert_eq!(
            handler
                .check_server_key(&key.public_key().clone().into())
                .await
                .expect("real trust decision"),
            accepted
        );
        assert_eq!(
            observation.was_accepted().expect("acceptance observation"),
            Some(accepted)
        );
    }
    std::fs::remove_dir_all(root).expect("remove isolated trust records");
}
