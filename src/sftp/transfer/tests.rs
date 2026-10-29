use super::*;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use russh_sftp::protocol::{Attrs, Data, Handle as RemoteHandle, Status, Version};

mod performance;

#[tokio::test(start_paused = true)]
async fn connection_rate_limiter_spaces_upload_and_download_chunks() {
    let limiter = Arc::new(TransferRateLimiter::new(Some(1024 * 1024)));
    let cancellation = TransferCancellation::new();
    limiter
        .reserve(64 * 1024, &cancellation)
        .await
        .expect("first chunk");
    let limiter_for_next = limiter.clone();
    let cancellation_for_next = cancellation.clone();
    let next = tokio::spawn(async move {
        limiter_for_next
            .reserve(64 * 1024, &cancellation_for_next)
            .await
    });
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(50)).await;
    assert!(
        !next.is_finished(),
        "shared limiter should delay the next chunk"
    );
    tokio::time::advance(Duration::from_millis(20)).await;
    next.await.expect("rate task").expect("second chunk");

    let cancelled = TransferCancellation::new();
    cancelled.cancel();
    assert!(limiter.reserve(64 * 1024, &cancelled).await.is_err());
}
#[test]
fn transfer_filter_patterns_match_exact_names_and_wildcards() {
    let patterns = vec![".DS_Store".to_owned(), "._*".to_owned(), "*.tmp".to_owned()];
    assert!(transfer_name_matches_filter(".DS_Store", &patterns));
    assert!(transfer_name_matches_filter("._notes", &patterns));
    assert!(transfer_name_matches_filter("report.tmp", &patterns));
    assert!(!transfer_name_matches_filter("report.txt", &patterns));
}

#[test]
fn upload_limiter_tracks_active_permits_and_limit_changes() {
    let limiter = Arc::new(UploadLimiter::new(2));
    let first = limiter
        .try_acquire()
        .expect("first permit should be available");
    let second = limiter
        .try_acquire()
        .expect("second permit should be available");
    assert!(limiter.try_acquire().is_err());

    limiter.set_limit(1);
    drop(first);
    assert!(limiter.try_acquire().is_err());
    drop(second);

    let permit = limiter
        .try_acquire()
        .expect("a permit should be available after active uploads finish");
    limiter.set_limit(3);
    let _second = limiter
        .try_acquire()
        .expect("raising the limit should allow another upload");
    drop(permit);
}

#[tokio::test]
async fn local_upload_reads_bounded_chunks_and_rejects_size_changes() {
    let path = std::env::temp_dir().join(format!("ax-ssh-upload-{}.bin", Uuid::new_v4()));
    let contents = vec![0x5a; (DOWNLOAD_CHUNK_BYTES as usize) + 17];
    fs::write(&path, &contents).expect("upload fixture should write");
    let request = SftpUploadRequest::from_local_file(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "/srv/file.bin".into(),
        path.clone(),
        contents.len() as u64,
        Vec::new(),
    )
    .expect("request");
    let cancellation = TransferCancellation::new();
    let (mut chunks, reader) = start_local_upload_reader(&request, &cancellation);
    let first = chunks
        .recv()
        .await
        .expect("first chunk")
        .expect("read success");
    assert_eq!(first.len(), DOWNLOAD_CHUNK_BYTES as usize);
    let last = chunks
        .recv()
        .await
        .expect("last chunk")
        .expect("read success");
    assert_eq!(last.len(), 17);
    assert!(chunks.recv().await.is_none());
    reader
        .await
        .expect("reader joined")
        .expect("reader success");

    fs::write(&path, b"changed").expect("changed upload fixture should write");
    let (mut chunks, reader) = start_local_upload_reader(&request, &cancellation);
    let error = chunks
        .recv()
        .await
        .expect("error event")
        .expect_err("changed source rejected");
    assert!(error.to_string().contains("changed"));
    assert!(reader.await.expect("reader joined").is_err());

    fs::remove_file(path).expect("upload fixture should be removed");
}

#[tokio::test]
async fn dropping_upload_consumer_releases_a_blocked_reader() {
    let path = std::env::temp_dir().join(format!("ax-ssh-reader-{}.bin", Uuid::new_v4()));
    fs::write(&path, vec![7; DOWNLOAD_CHUNK_BYTES as usize * 8]).expect("fixture");
    let request = SftpUploadRequest::from_local_file(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "/srv/file.bin".into(),
        path.clone(),
        u64::from(DOWNLOAD_CHUNK_BYTES) * 8,
        Vec::new(),
    )
    .expect("request");
    let (mut chunks, reader) = start_local_upload_reader(&request, &TransferCancellation::new());
    chunks
        .recv()
        .await
        .expect("first chunk")
        .expect("read success");
    drop(chunks);
    assert!(
        timeout(Duration::from_secs(1), reader)
            .await
            .expect("reader stopped")
            .expect("reader joined")
            .is_err()
    );
    fs::remove_file(path).expect("cleanup");
}

#[tokio::test]
async fn cancelling_upload_while_opening_a_channel_releases_its_slot() {
    let path = std::env::temp_dir().join(format!("ax-ssh-opening-{}.bin", Uuid::new_v4()));
    fs::write(&path, b"content").expect("fixture");
    let request = SftpUploadRequest::from_local_file(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "/srv/file.bin".into(),
        path.clone(),
        7,
        Vec::new(),
    )
    .expect("request");
    let (events, mut receiver) = mpsc::channel(4);
    let worker = SftpUploadHandle::spawn(
        &Handle::current(),
        std::future::pending::<Result<tokio::io::DuplexStream>>(),
        request,
        events,
        SftpUploadHandle::reserve_global_slot().expect("slot"),
        Arc::new(TransferRateLimiter::new(None)),
    );
    worker.cancel();
    timeout(Duration::from_secs(1), worker.shutdown())
        .await
        .expect("prompt cancellation")
        .expect("join");
    assert!(matches!(
        receiver.recv().await,
        Some(SftpTransferEvent::Cancelled { .. })
    ));
    fs::remove_file(path).expect("cleanup");
}

#[test]
fn local_upload_request_stores_a_path_and_bounded_size_only() {
    let path = std::env::temp_dir().join(format!("ax-ssh-upload-request-{}.bin", Uuid::new_v4()));
    fs::write(&path, vec![0_u8; 64 * 1024]).expect("request fixture should write");
    let request = SftpUploadRequest::from_local_file(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "/remote/example.bin".to_owned(),
        path.clone(),
        64 * 1024,
        Vec::new(),
    )
    .expect("local upload request should validate");

    assert_eq!(request.local_path, path);
    assert_eq!(request.total_bytes(), 64 * 1024);
    fs::remove_file(path).expect("request fixture should be removed");
}

#[derive(Clone, Copy)]
enum UploadClosePolicy {
    Accept,
    Reject,
    Cancel,
    CancelBeforePublish,
}

struct UploadTestServer {
    close_policy: UploadClosePolicy,
    cancellation: TransferCancellation,
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    directories: Arc<Mutex<HashSet<String>>>,
}

fn upload_test_status(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".to_owned(),
        language_tag: "en-US".to_owned(),
    }
}

impl russh_sftp::server::Handler for UploadTestServer {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> std::result::Result<Version, Self::Error> {
        let mut version = Version::new();
        version
            .extensions
            .insert(POSIX_RENAME_EXTENSION.to_owned(), "1".to_owned());
        Ok(version)
    }

    async fn lstat(&mut self, id: u32, path: String) -> std::result::Result<Attrs, Self::Error> {
        let files = self.files.lock().map_err(|_| StatusCode::Failure)?;
        if matches!(self.close_policy, UploadClosePolicy::CancelBeforePublish)
            && files.keys().any(|path| path.contains(".axssh-upload-"))
        {
            self.cancellation.cancel();
        }
        let mut attrs = FileAttributes::empty();
        if let Some(content) = files.get(&path) {
            attrs.set_regular(true);
            attrs.size = Some(content.len() as u64);
            attrs.mtime = Some(1234);
        } else if self
            .directories
            .lock()
            .map_err(|_| StatusCode::Failure)?
            .contains(&path)
        {
            attrs.set_dir(true);
        } else {
            return Err(StatusCode::NoSuchFile);
        }
        Ok(Attrs { id, attrs })
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> std::result::Result<Status, Self::Error> {
        let mut directories = self.directories.lock().map_err(|_| StatusCode::Failure)?;
        let parent = path
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("");
        if !directories.contains(parent) || !directories.insert(path) {
            return Err(StatusCode::Failure);
        }
        Ok(upload_test_status(id))
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        flags: OpenFlags,
        _attrs: FileAttributes,
    ) -> std::result::Result<RemoteHandle, Self::Error> {
        if !flags.contains(OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE) {
            return Err(StatusCode::PermissionDenied);
        }
        let mut files = self.files.lock().map_err(|_| StatusCode::Failure)?;
        if files.contains_key(&filename) {
            return Err(StatusCode::Failure);
        }
        files.insert(filename.clone(), Vec::new());
        Ok(RemoteHandle {
            id,
            handle: filename,
        })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> std::result::Result<Status, Self::Error> {
        let mut files = self.files.lock().map_err(|_| StatusCode::Failure)?;
        let file = files.get_mut(&handle).ok_or(StatusCode::NoSuchFile)?;
        if file.len() != offset as usize {
            return Err(StatusCode::BadMessage);
        }
        file.extend(data);
        Ok(upload_test_status(id))
    }

    async fn close(
        &mut self,
        id: u32,
        _handle: String,
    ) -> std::result::Result<Status, Self::Error> {
        match self.close_policy {
            UploadClosePolicy::Reject => return Err(StatusCode::Failure),
            UploadClosePolicy::Cancel => self.cancellation.cancel(),
            UploadClosePolicy::Accept | UploadClosePolicy::CancelBeforePublish => {}
        }
        Ok(upload_test_status(id))
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> std::result::Result<Status, Self::Error> {
        let mut files = self.files.lock().map_err(|_| StatusCode::Failure)?;
        if files.contains_key(&newpath) {
            return Err(StatusCode::Failure);
        }
        let content = files.remove(&oldpath).ok_or(StatusCode::NoSuchFile)?;
        files.insert(newpath, content);
        Ok(upload_test_status(id))
    }

    async fn extended(
        &mut self,
        id: u32,
        request: String,
        data: Vec<u8>,
    ) -> std::result::Result<Packet, Self::Error> {
        if request != POSIX_RENAME_EXTENSION || data.is_empty() {
            return Err(StatusCode::OpUnsupported);
        }
        let mut files = self.files.lock().map_err(|_| StatusCode::Failure)?;
        let temporary = files
            .keys()
            .find(|path| path.starts_with("/srv/.report.txt.axssh-upload-"))
            .cloned()
            .ok_or(StatusCode::NoSuchFile)?;
        let content = files.remove(&temporary).ok_or(StatusCode::NoSuchFile)?;
        files.insert("/srv/report.txt".to_owned(), content);
        Ok(Packet::Status(upload_test_status(id)))
    }

    async fn remove(
        &mut self,
        id: u32,
        filename: String,
    ) -> std::result::Result<Status, Self::Error> {
        self.files
            .lock()
            .map_err(|_| StatusCode::Failure)?
            .remove(&filename);
        Ok(upload_test_status(id))
    }
}

async fn run_upload_test(
    request: &SftpUploadRequest,
    files: &Arc<Mutex<HashMap<String, Vec<u8>>>>,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
) -> Result<UploadOutcome> {
    run_upload_test_with_close_policy(
        request,
        files,
        cancellation,
        event_tx,
        UploadClosePolicy::Accept,
    )
    .await
}

async fn run_upload_test_with_close_policy(
    request: &SftpUploadRequest,
    files: &Arc<Mutex<HashMap<String, Vec<u8>>>>,
    cancellation: &TransferCancellation,
    event_tx: &mpsc::Sender<SftpTransferEvent>,
    close_policy: UploadClosePolicy,
) -> Result<UploadOutcome> {
    let (client, server) = tokio::io::duplex(64 * 1024);
    russh_sftp::server::run(
        server,
        UploadTestServer {
            close_policy,
            cancellation: cancellation.clone(),
            files: files.clone(),
            directories: Arc::new(Mutex::new(HashSet::from(["/srv".to_owned()]))),
        },
    )
    .await;
    upload_initialized(client, request, cancellation, event_tx).await
}

#[tokio::test]
async fn upload_close_failure_or_prepublication_cancellation_cleans_up_without_publishing() {
    let local = std::env::temp_dir().join(format!("axssh-close-failure-{}", Uuid::new_v4()));
    let contents = b"complete upload fixture";
    fs::write(&local, contents).expect("local upload fixture");
    for policy in [
        UploadClosePolicy::Reject,
        UploadClosePolicy::Cancel,
        UploadClosePolicy::CancelBeforePublish,
    ] {
        let request = SftpUploadRequest::from_local_file(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "/srv/report.txt".into(),
            local.clone(),
            contents.len() as u64,
            Vec::new(),
        )
        .expect("request");
        let files = Arc::new(Mutex::new(HashMap::new()));
        let (events, _receiver) = mpsc::channel(32);
        let error = run_upload_test_with_close_policy(
            &request,
            &files,
            &TransferCancellation::new(),
            &events,
            policy,
        )
        .await
        .err()
        .expect("close failure or cancellation must prevent publication");
        match policy {
            UploadClosePolicy::Reject => {
                assert!(format!("{error:#}").contains("cannot close remote upload file"))
            }
            UploadClosePolicy::Cancel | UploadClosePolicy::CancelBeforePublish => {
                assert!(is_cancelled(&error))
            }
            UploadClosePolicy::Accept => {
                unreachable!("success is covered by the existing upload tests")
            }
        }
        assert!(files.lock().expect("remote files").is_empty());
    }
    fs::remove_file(local).expect("fixture cleanup");
}

#[tokio::test]
async fn upload_conflict_choices_preserve_or_publish_the_expected_remote_file() {
    let path = std::env::temp_dir().join(format!("ax-ssh-upload-conflict-{}.txt", Uuid::new_v4()));
    fs::write(&path, b"new content").expect("local upload fixture should write");
    let files = Arc::new(Mutex::new(HashMap::from([(
        "/srv/report.txt".to_owned(),
        b"old".to_vec(),
    )])));
    let request = SftpUploadRequest::from_local_file(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "/srv/report.txt".to_owned(),
        path.clone(),
        11,
        Vec::new(),
    )
    .expect("upload request should validate");
    let cancellation = TransferCancellation::new();
    let (event_tx, _events) = mpsc::channel(SFTP_TRANSFER_EVENT_CAPACITY);

    assert!(matches!(
        run_upload_test(&request, &files, &cancellation, &event_tx)
            .await
            .expect("conflict should be reported"),
        UploadOutcome::Conflict(_)
    ));
    let mut skip = request.clone();
    skip.set_conflict_choice(SftpUploadConflictChoice::Skip);
    assert!(matches!(
        run_upload_test(&skip, &files, &cancellation, &event_tx)
            .await
            .expect("skip should finish"),
        UploadOutcome::Skipped
    ));
    assert_eq!(
        files.lock().expect("remote files should lock")["/srv/report.txt"],
        b"old"
    );

    let mut keep_both = request.clone();
    keep_both.set_conflict_choice(SftpUploadConflictChoice::KeepBoth);
    assert!(matches!(
        run_upload_test(&keep_both, &files, &cancellation, &event_tx)
            .await
            .expect("second copy should publish"),
        UploadOutcome::Published
    ));
    assert_eq!(
        files.lock().expect("remote files should lock")["/srv/report (1).txt"],
        b"new content"
    );

    let mut overwrite = request;
    overwrite.set_conflict_choice(SftpUploadConflictChoice::Overwrite);
    overwrite.set_expected_remote(Some(4), Some(1234));
    assert!(
        run_upload_test(&overwrite, &files, &cancellation, &event_tx)
            .await
            .is_err()
    );
    assert_eq!(
        files.lock().expect("remote files should lock")["/srv/report.txt"],
        b"old"
    );
    overwrite.set_expected_remote(Some(3), Some(1234));
    assert!(matches!(
        run_upload_test(&overwrite, &files, &cancellation, &event_tx)
            .await
            .expect("overwrite should publish"),
        UploadOutcome::Published
    ));
    assert_eq!(
        files.lock().expect("remote files should lock")["/srv/report.txt"],
        b"new content"
    );
    fs::remove_file(path).expect("local upload fixture should be removed");
}

#[tokio::test]
async fn upload_creates_nested_remote_directories_before_publishing() {
    let path = std::env::temp_dir().join(format!("ax-ssh-nested-upload-{}.txt", Uuid::new_v4()));
    fs::write(&path, b"nested").expect("nested upload fixture should write");
    let files = Arc::new(Mutex::new(HashMap::new()));
    let request = SftpUploadRequest::from_local_file(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "/srv/folder/nested/file.txt".to_owned(),
        path.clone(),
        6,
        vec!["/srv/folder".to_owned(), "/srv/folder/nested".to_owned()],
    )
    .expect("nested upload request should validate");
    let cancellation = TransferCancellation::new();
    let (event_tx, _events) = mpsc::channel(SFTP_TRANSFER_EVENT_CAPACITY);
    assert!(matches!(
        run_upload_test(&request, &files, &cancellation, &event_tx)
            .await
            .expect("nested upload should publish"),
        UploadOutcome::Published
    ));
    assert_eq!(
        files.lock().expect("remote files should lock")["/srv/folder/nested/file.txt"],
        b"nested"
    );
    fs::remove_file(path).expect("nested upload fixture should be removed");
}

struct TestCacheDir(PathBuf);

impl TestCacheDir {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("axssh-sftp-cache-{}", Uuid::new_v4())))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestCacheDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct DownloadTestServer {
    content: Arc<Vec<u8>>,
    reported_size: u64,
    reads: Arc<Mutex<Vec<(u64, u32)>>>,
}

impl DownloadTestServer {
    fn attrs(&self, id: u32) -> Attrs {
        let mut attrs = FileAttributes::empty();
        attrs.set_regular(true);
        attrs.size = Some(self.reported_size);
        attrs.mtime = Some(1234);
        Attrs { id, attrs }
    }
}

impl russh_sftp::server::Handler for DownloadTestServer {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        flags: OpenFlags,
        _attrs: FileAttributes,
    ) -> std::result::Result<RemoteHandle, Self::Error> {
        if filename != "/srv/report.bin" || !flags.contains(OpenFlags::READ) {
            return Err(StatusCode::PermissionDenied);
        }
        Ok(RemoteHandle {
            id,
            handle: "test-download".to_owned(),
        })
    }

    async fn close(&mut self, id: u32, handle: String) -> std::result::Result<Status, Self::Error> {
        if handle != "test-download" {
            return Err(StatusCode::BadMessage);
        }
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: "Ok".to_owned(),
            language_tag: "en-US".to_owned(),
        })
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> std::result::Result<Data, Self::Error> {
        if handle != "test-download" {
            return Err(StatusCode::BadMessage);
        }
        self.reads
            .lock()
            .expect("read observations should lock")
            .push((offset, len));
        let start = usize::try_from(offset).map_err(|_| StatusCode::BadMessage)?;
        if start >= self.content.len() {
            return Err(StatusCode::Eof);
        }
        let end = start.saturating_add(len as usize).min(self.content.len());
        Ok(Data {
            id,
            data: self.content[start..end].to_vec(),
        })
    }

    async fn lstat(&mut self, id: u32, path: String) -> std::result::Result<Attrs, Self::Error> {
        if path != "/srv/report.bin" {
            return Err(StatusCode::NoSuchFile);
        }
        Ok(self.attrs(id))
    }

    async fn fstat(&mut self, id: u32, handle: String) -> std::result::Result<Attrs, Self::Error> {
        if handle != "test-download" {
            return Err(StatusCode::BadMessage);
        }
        Ok(self.attrs(id))
    }
}

#[test]
fn download_paths_accept_long_file_names_and_reject_invalid_paths() {
    assert_eq!(
        validate_download_path("/srv/report.txt").expect("file path should validate"),
        "report.txt"
    );
    assert!(validate_download_path("/srv/").is_err());
    assert!(validate_download_path("..").is_err());
    assert!(validate_download_path("/srv/line\nbreak").is_err());
    assert!(validate_download_path(&format!("/srv/{}", "a".repeat(600))).is_ok());
}

#[test]
fn cache_basename_is_portable_and_preserves_extension() {
    assert_eq!(sanitize_basename("../bad\\name?.pdf"), "_bad_name_.pdf");
    assert_eq!(sanitize_basename("..."), "remote-file");
    assert_eq!(sanitize_basename(" report.txt. "), "report.txt");

    let long = format!("{}.json", "界".repeat(100));
    let sanitized = sanitize_basename(&long);
    assert!(sanitized.len() <= MAX_CACHE_BASENAME_BYTES);
    assert!(sanitized.ends_with(".json"));
    assert!(sanitized.is_char_boundary(sanitized.len()));
}

#[test]
fn cache_target_stays_directly_inside_namespace() {
    let root = Path::new("/private/cache/sftp-open");
    let id =
        Uuid::parse_str("12345678-1234-4234-8234-123456789abc").expect("fixture UUID should parse");
    let target = CacheTarget::new(root, id, "../../report?.txt");

    assert_eq!(target.final_path.parent(), Some(root));
    assert_eq!(target.part_path.parent(), Some(root));
    assert_eq!(
        target.final_path.file_name().and_then(|name| name.to_str()),
        Some("12345678-1234-4234-8234-123456789abc-_.._report_.txt")
    );
    assert_eq!(
        managed_cache_name("12345678-1234-4234-8234-123456789abc-report.txt"),
        Some(false)
    );
    assert_eq!(
        managed_cache_name(".12345678-1234-4234-8234-123456789abc-report.txt.part"),
        Some(true)
    );
    assert_eq!(managed_cache_name("notes.txt"), None);
}

#[test]
fn metadata_rejects_links_directories_and_unknown_types_without_fixed_size_cap() {
    let mut regular = FileAttributes::empty();
    regular.set_regular(true);
    regular.size = Some(2 * 1024 * 1024 * 1024 * 1024);
    assert_eq!(
        validate_regular_metadata(&regular)
            .expect("regular file above 1 TiB should pass")
            .size,
        2 * 1024 * 1024 * 1024 * 1024
    );

    let mut symlink = FileAttributes::empty();
    symlink.set_symlink(true);
    symlink.size = Some(1);
    assert!(validate_regular_metadata(&symlink).is_err());

    let mut directory = FileAttributes::empty();
    directory.set_dir(true);
    directory.size = Some(1);
    assert!(validate_regular_metadata(&directory).is_err());

    let mut unknown = FileAttributes::empty();
    unknown.size = Some(1);
    assert!(validate_regular_metadata(&unknown).is_err());

    regular.size = None;
    assert!(validate_regular_metadata(&regular).is_err());
}

#[test]
fn recursive_discovery_entries_validate_names_types_and_unlimited_file_sizes() {
    let mut regular = FileAttributes::empty();
    regular.set_regular(true);
    regular.size = Some(7);
    let regular = File::new("report.txt", regular);
    assert!(matches!(
        download_entry("/srv", regular),
        Ok((name, path, false, 7))
            if name == "report.txt" && path == "/srv/report.txt"
    ));

    let mut large = FileAttributes::empty();
    large.set_regular(true);
    large.size = Some(2 * 1024 * 1024 * 1024 * 1024);
    assert!(matches!(
        download_entry("/srv", File::new("large.bin", large)),
        Ok((_, _, false, 2_199_023_255_552))
    ));

    let mut directory = FileAttributes::empty();
    directory.set_dir(true);
    assert!(matches!(
        download_entry("/srv", File::new("nested", directory)),
        Ok((name, path, true, 0))
            if name == "nested" && path == "/srv/nested"
    ));

    let mut symlink = FileAttributes::empty();
    symlink.set_symlink(true);
    assert!(download_entry("/srv", File::new("link", symlink)).is_err());
    assert!(download_entry("/srv", File::new("../escape", FileAttributes::empty())).is_err());
    assert!(download_entry("/srv", File::new("nested\\file", FileAttributes::empty())).is_err());
    let mut long_name = FileAttributes::empty();
    long_name.set_regular(true);
    long_name.size = Some(1);
    assert!(download_entry("/srv", File::new("n".repeat(600), long_name)).is_ok());
}

#[test]
fn recursive_discovery_accepts_entries_without_count_or_size_budgets() {
    let mut entries = FileAttributes::empty();
    entries.set_regular(true);
    entries.size = Some(1);
    assert!(download_entry("/srv", File::new("file.txt", entries)).is_ok());
}

#[tokio::test]
async fn loopback_download_reads_bounded_chunks_and_atomically_publishes() {
    let content = Arc::new(
        (0..150_000)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>(),
    );
    let reads = Arc::new(Mutex::new(Vec::new()));
    let server = DownloadTestServer {
        content: content.clone(),
        reported_size: content.len() as u64,
        reads: reads.clone(),
    };
    let (client_stream, server_stream) = tokio::io::duplex(512 * 1024);
    russh_sftp::server::run(server_stream, server).await;
    let test_root = TestCacheDir::new();
    let request = SftpDownloadRequest::new(
        Uuid::parse_str("12345678-1234-4234-8234-123456789abc").expect("fixture UUID should parse"),
        "/srv/report.bin".to_owned(),
    )
    .expect("download request should validate");
    let cancellation = TransferCancellation::new();
    let (event_tx, mut event_rx) = mpsc::channel(SFTP_TRANSFER_EVENT_CAPACITY);

    let (local_path, total_bytes) = download_to_cache(
        client_stream,
        &request,
        &cancellation,
        &event_tx,
        test_root.path(),
    )
    .await
    .expect("loopback download should complete");

    assert_eq!(total_bytes, content.len() as u64);
    assert_eq!(
        fs::read(&local_path).expect("downloaded cache file should read"),
        *content
    );
    assert!(
        fs::read_dir(test_root.path())
            .expect("cache directory should read")
            .all(|entry| !entry
                .expect("cache entry should read")
                .file_name()
                .to_string_lossy()
                .ends_with(".part"))
    );
    let observed = reads.lock().expect("read observations should lock");
    assert_eq!(
        observed.as_slice(),
        &[
            (0, DOWNLOAD_CHUNK_BYTES),
            (u64::from(DOWNLOAD_CHUNK_BYTES), DOWNLOAD_CHUNK_BYTES),
            (2 * u64::from(DOWNLOAD_CHUNK_BYTES), 18_928),
            (150_000, 1),
        ]
    );
    drop(observed);
    assert!(matches!(
        event_rx.try_recv(),
        Ok(SftpTransferEvent::Started {
            total_bytes: 150_000,
            ..
        })
    ));
    assert!(matches!(
        event_rx.try_recv(),
        Ok(SftpTransferEvent::Progress {
            downloaded_bytes: 150_000,
            total_bytes: 150_000,
            ..
        })
    ));
}

#[tokio::test]
async fn loopback_download_publishes_into_the_selected_local_directory() {
    let content = Arc::new(b"downloaded locally".to_vec());
    let server = DownloadTestServer {
        content: content.clone(),
        reported_size: content.len() as u64,
        reads: Arc::new(Mutex::new(Vec::new())),
    };
    let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
    russh_sftp::server::run(server_stream, server).await;
    let test_root = TestCacheDir::new();
    fs::create_dir(test_root.path()).expect("local directory fixture should be created");
    let request = SftpDownloadRequest::for_local_download(
        Uuid::new_v4(),
        "/srv/report.bin".to_owned(),
        test_root.path().to_owned(),
        vec!["selected".to_owned(), "report.bin".to_owned()],
        content.len() as u64,
    )
    .expect("local download request should validate");
    let cancellation = TransferCancellation::new();
    let (event_tx, mut event_rx) = mpsc::channel(SFTP_TRANSFER_EVENT_CAPACITY);

    let (local_path, total_bytes) =
        download_to_local(client_stream, &request, &cancellation, &event_tx)
            .await
            .expect("loopback local download should complete");

    assert_eq!(total_bytes, content.len() as u64);
    let canonical_root = fs::canonicalize(test_root.path())
        .expect("local directory fixture should resolve canonically");
    assert_eq!(local_path, canonical_root.join("selected/report.bin"));
    assert_eq!(
        fs::read(&local_path).expect("local download should read"),
        *content
    );
    assert!(
        fs::read_dir(
            local_path
                .parent()
                .expect("local path should have a parent")
        )
        .expect("local download directory should read")
        .all(|entry| !entry
            .expect("local directory entry should read")
            .file_name()
            .to_string_lossy()
            .ends_with(".part"))
    );
    assert!(matches!(
        event_rx.try_recv(),
        Ok(SftpTransferEvent::Started { .. })
    ));
    assert!(matches!(
        event_rx.try_recv(),
        Ok(SftpTransferEvent::Progress { .. })
    ));
}

#[test]
fn explicit_local_path_download_uses_the_existing_local_target_validation() {
    let local_path = std::env::temp_dir()
        .join(format!("axssh-native-drag-{}", Uuid::new_v4()))
        .join("report.bin");
    let request = SftpDownloadRequest::for_explicit_local_path(
        Uuid::new_v4(),
        "/srv/report.bin".to_owned(),
        local_path,
        17,
    )
    .expect("absolute file target should be decomposed into a safe local target");

    assert_eq!(request.name(), "report.bin");
    assert_eq!(request.total_bytes(), 17);
    assert!(
        SftpDownloadRequest::for_explicit_local_path(
            Uuid::new_v4(),
            "/srv/report.bin".to_owned(),
            PathBuf::from("report.bin"),
            17,
        )
        .is_err(),
        "a bare file name must not silently select a process working directory"
    );
    assert!(
        SftpDownloadRequest::for_explicit_local_path(
            Uuid::new_v4(),
            "/srv/report.bin".to_owned(),
            PathBuf::from("relative/report.bin"),
            17,
        )
        .is_err(),
        "a relative nested path must not silently select a process working directory"
    );
}

#[tokio::test]
async fn truncated_loopback_download_never_publishes_a_cache_file() {
    let content = Arc::new(b"short".to_vec());
    let server = DownloadTestServer {
        content,
        reported_size: 10,
        reads: Arc::new(Mutex::new(Vec::new())),
    };
    let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
    russh_sftp::server::run(server_stream, server).await;
    let test_root = TestCacheDir::new();
    let request = SftpDownloadRequest::new(Uuid::new_v4(), "/srv/report.bin".to_owned())
        .expect("download request should validate");
    let cancellation = TransferCancellation::new();
    let (event_tx, _event_rx) = mpsc::channel(SFTP_TRANSFER_EVENT_CAPACITY);

    let error = download_to_cache(
        client_stream,
        &request,
        &cancellation,
        &event_tx,
        test_root.path(),
    )
    .await
    .expect_err("truncated download should fail");

    assert!(error.to_string().contains("ended after 5 of 10 bytes"));
    assert_eq!(
        fs::read_dir(test_root.path())
            .expect("cache directory should read")
            .count(),
        0
    );
}

#[tokio::test]
async fn cancellation_interrupts_a_pending_step() {
    let cancellation = TransferCancellation::new();
    let trigger = cancellation.clone();
    let cancel_task = tokio::spawn(async move {
        tokio::task::yield_now().await;
        trigger.cancel();
    });
    let result = await_step(
        &cancellation,
        Instant::now() + Duration::from_secs(1),
        Duration::from_secs(1),
        "pending test operation",
        std::future::pending::<()>(),
    )
    .await;

    let error = result.expect_err("cancelled step should fail");
    assert!(is_cancelled(&error));
    cancel_task.await.expect("cancel task should finish");
}

#[tokio::test(start_paused = true)]
async fn pending_protocol_step_obeys_its_timeout() {
    let result = await_step(
        &TransferCancellation::new(),
        Instant::now() + Duration::from_secs(60),
        Duration::from_secs(2),
        "test protocol request",
        std::future::pending::<()>(),
    )
    .await;

    assert_eq!(
        result
            .expect_err("pending step should time out")
            .to_string(),
        "test protocol request timed out"
    );
}

#[test]
fn stale_policy_distinguishes_partial_and_completed_files() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(2 * 24 * 60 * 60);
    let two_hours_old = now - Duration::from_secs(2 * 60 * 60);
    assert!(cache_entry_is_stale(two_hours_old, now, true));
    assert!(!cache_entry_is_stale(two_hours_old, now, false));
}

#[test]
fn cache_writer_publishes_only_a_complete_file() {
    let test_root = TestCacheDir::new();
    ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
    let target = CacheTarget::new(test_root.path(), Uuid::new_v4(), "notes.txt");
    let part_path = target.part_path.clone();
    let pending = PendingCacheFile::create(target, 7).expect("private part file should open");
    let (chunk_tx, chunk_rx) = mpsc::channel(WRITER_QUEUE_CAPACITY);
    chunk_tx
        .blocking_send(b"bounded".to_vec())
        .expect("test chunk should queue");
    drop(chunk_tx);

    let path = write_cache_file(pending, chunk_rx, 7, &TransferCancellation::new())
        .expect("complete cache file should publish");

    assert_eq!(
        fs::read(&path).expect("published file should read"),
        b"bounded"
    );
    assert!(!part_path.exists());
}

#[test]
fn cancelled_cache_writer_removes_its_partial_file() {
    let test_root = TestCacheDir::new();
    ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
    let target = CacheTarget::new(test_root.path(), Uuid::new_v4(), "notes.txt");
    let part_path = target.part_path.clone();
    let pending = PendingCacheFile::create(target, 1).expect("private part file should open");
    let (chunk_tx, chunk_rx) = mpsc::channel(WRITER_QUEUE_CAPACITY);
    drop(chunk_tx);
    let cancellation = TransferCancellation::new();
    cancellation.cancel();

    let error = write_cache_file(pending, chunk_rx, 1, &cancellation)
        .expect_err("cancelled cache write should fail");

    assert!(is_cancelled(&error));
    assert!(!part_path.exists());
}

#[test]
fn stale_cleanup_leaves_unrecognized_files_and_directories() {
    let test_root = TestCacheDir::new();
    ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
    let managed = test_root
        .path()
        .join(format!("{}-notes.txt", Uuid::new_v4()));
    let unrecognized = test_root.path().join("user-notes.txt");
    let directory = test_root
        .path()
        .join(format!("{}-directory", Uuid::new_v4()));
    fs::write(&managed, b"managed").expect("managed fixture should write");
    fs::write(&unrecognized, b"unrecognized").expect("unrecognized fixture should write");
    fs::create_dir(&directory).expect("directory fixture should create");
    let future = SystemTime::now() + CACHE_STALE_AFTER + Duration::from_secs(1);

    let removed =
        cleanup_stale_cache_at(test_root.path(), future).expect("stale cleanup should finish");

    assert_eq!(removed, 1);
    assert!(!managed.exists());
    assert!(unrecognized.exists());
    assert!(directory.exists());
}

#[test]
fn stale_cleanup_stops_after_the_bounded_scan_budget() {
    let test_root = TestCacheDir::new();
    ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
    let id = Uuid::new_v4();
    for index in 0..=MAX_CACHE_SCAN_ENTRIES {
        fs::write(
            test_root.path().join(format!("{id}-entry-{index}")),
            b"stale",
        )
        .expect("managed cache fixture should write");
    }

    let future = SystemTime::now() + CACHE_STALE_AFTER + Duration::from_secs(1);
    let removed =
        cleanup_stale_cache_at(test_root.path(), future).expect("bounded cleanup should finish");

    assert_eq!(removed, MAX_CACHE_SCAN_ENTRIES);
    assert_eq!(
        fs::read_dir(test_root.path())
            .expect("cache directory should read")
            .count(),
        1
    );
}

#[test]
fn cache_quota_evicts_completed_files_before_rejecting_a_new_reservation() {
    let test_root = TestCacheDir::new();
    ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
    let target = CacheTarget::new(test_root.path(), Uuid::new_v4(), "old.bin");
    let file = LocalFile::create(&target.final_path).expect("completed fixture should create");
    file.set_len(MAX_CACHE_BYTES)
        .expect("completed fixture should reserve quota");

    enforce_cache_quota(test_root.path(), 1, MAX_CACHE_BYTES, SystemTime::now())
        .expect("old completed file should be evicted");

    assert!(!target.final_path.exists());
}

#[test]
fn cache_quota_never_evicts_an_active_partial_file() {
    let test_root = TestCacheDir::new();
    ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
    let target = CacheTarget::new(test_root.path(), Uuid::new_v4(), "active.bin");
    let file = LocalFile::create(&target.part_path).expect("partial fixture should create");
    file.set_len(MAX_CACHE_BYTES)
        .expect("partial fixture should reserve quota");

    let error = enforce_cache_quota(test_root.path(), 1, MAX_CACHE_BYTES, SystemTime::now())
        .expect_err("active partial file should block an over-quota reservation");

    assert!(error.to_string().contains("quota"));
    assert!(target.part_path.exists());
}

#[test]
fn larger_local_open_limit_can_reserve_above_the_default_cache_quota() {
    let test_root = TestCacheDir::new();
    ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
    let incoming = MAX_CACHE_BYTES + 1;
    assert!(
        enforce_cache_quota(
            test_root.path(),
            incoming,
            MAX_CACHE_BYTES,
            SystemTime::now()
        )
        .is_err()
    );
    enforce_cache_quota(test_root.path(), incoming, incoming, SystemTime::now())
        .expect("configured local open limit should raise the cache quota");
}

#[test]
fn unlimited_local_open_cache_does_not_evict_for_byte_quota() {
    let test_root = TestCacheDir::new();
    ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
    let target = CacheTarget::new(test_root.path(), Uuid::new_v4(), "old.bin");
    let file = LocalFile::create(&target.final_path).expect("completed fixture should create");
    file.set_len(MAX_CACHE_BYTES)
        .expect("completed fixture should reserve the default quota");

    enforce_cache_quota(test_root.path(), 1, u64::MAX, SystemTime::now())
        .expect("unlimited local-open quota should permit another byte");

    assert!(target.final_path.exists());
}

#[test]
fn local_open_snapshot_copies_from_the_validated_handle() {
    let test_root = TestCacheDir::new();
    let cancellation = AtomicBool::new(false);
    let source_path = test_root.path().with_extension("source");
    fs::write(&source_path, b"validated contents").expect("source fixture should write");
    let mut source = LocalFile::open(&source_path).expect("source fixture should open");

    let snapshot = snapshot_local_file_for_open_at(
        &mut source,
        "notes.txt",
        test_root.path(),
        MAX_CACHE_OPEN_BYTES,
        &cancellation,
    )
    .expect("validated handle should publish a private snapshot");

    assert_eq!(
        fs::read(&snapshot).expect("snapshot should remain readable"),
        b"validated contents"
    );
    assert_eq!(
        snapshot.parent(),
        Some(test_root.path()),
        "snapshot must stay directly inside the private namespace"
    );
    fs::remove_file(source_path).expect("source fixture should be removed");
}

#[test]
fn local_open_snapshot_rejects_files_over_its_configured_limit() {
    let test_root = TestCacheDir::new();
    let cancellation = AtomicBool::new(false);
    ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
    let source_path = test_root.path().with_extension("source");
    fs::write(&source_path, b"12345").expect("source fixture should write");
    let mut source = LocalFile::open(&source_path).expect("source fixture should open");

    assert!(
        snapshot_local_file_for_open_at(
            &mut source,
            "notes.txt",
            test_root.path(),
            4,
            &cancellation,
        )
        .is_err()
    );
    assert!(
        snapshot_local_file_for_open_at(
            &mut source,
            "notes.txt",
            test_root.path(),
            0,
            &cancellation,
        )
        .is_err()
    );
    assert_eq!(
        fs::read_dir(test_root.path())
            .expect("cache directory should read")
            .count(),
        0
    );
    let unlimited_snapshot = snapshot_local_file_for_open_at(
        &mut source,
        "notes.txt",
        test_root.path(),
        u64::MAX,
        &cancellation,
    )
    .expect("unlimited local-open limit should allow the validated file");
    assert_eq!(
        fs::read(unlimited_snapshot).expect("unlimited snapshot should be readable"),
        b"12345"
    );
    cancellation.store(true, Ordering::Release);
    assert!(
        snapshot_local_file_for_open_at(
            &mut source,
            "notes.txt",
            test_root.path(),
            u64::MAX,
            &cancellation,
        )
        .is_err()
    );
    assert_eq!(
        fs::read_dir(test_root.path())
            .expect("cancelled cache directory should read")
            .count(),
        1,
        "cancelled copy should remove its partial file"
    );
    fs::remove_file(source_path).expect("source fixture should be removed");
}

#[cfg(unix)]
#[test]
fn cache_files_and_namespace_use_private_unix_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let test_root = TestCacheDir::new();
    ensure_cache_namespace(test_root.path()).expect("private namespace should be created");
    let target = CacheTarget::new(test_root.path(), Uuid::new_v4(), "notes.txt");
    let pending = PendingCacheFile::create(target, 0).expect("private part file should open");
    let directory_mode = fs::metadata(test_root.path())
        .expect("namespace metadata should exist")
        .permissions()
        .mode()
        & 0o777;
    let file_mode = fs::metadata(&pending.target.part_path)
        .expect("part metadata should exist")
        .permissions()
        .mode()
        & 0o777;

    assert_eq!(directory_mode, 0o700);
    assert_eq!(file_mode, 0o600);
}

#[cfg(unix)]
#[test]
fn cache_namespace_rejects_a_symbolic_link() {
    use std::os::unix::fs::symlink;

    let test_root = TestCacheDir::new();
    let real_directory = test_root.path().join("real");
    let linked_namespace = test_root.path().join("sftp-open");
    fs::create_dir_all(&real_directory).expect("real cache fixture should create");
    symlink(&real_directory, &linked_namespace).expect("cache symlink fixture should create");

    let error = ensure_cache_namespace(&linked_namespace)
        .expect_err("cache namespace symlink should be rejected");

    assert!(error.to_string().contains("not a private directory"));
}
