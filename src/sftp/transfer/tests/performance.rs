//! Real protocol peers that withhold replies until a full window arrives.

use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

async fn request(peer: &mut DuplexStream) -> Option<Packet> {
    let length = match peer.read_u32().await {
        Ok(length) => length,
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return None,
        Err(error) => panic!("request read failed: {error}"),
    };
    assert!(length <= MAX_PACKET_BYTES);
    let mut payload = vec![0; length as usize];
    peer.read_exact(&mut payload)
        .await
        .expect("request payload");
    // Vec serialization supplies a length prefix; Packet expects just the
    // type and payload. This obtains Bytes without another direct dependency.
    let bytes = russh_sftp::ser::to_bytes(&payload).expect("payload bytes");
    let mut payload = bytes.slice(4..);
    Some(Packet::try_from(&mut payload).expect("valid request"))
}

async fn reply<T: serde::Serialize>(peer: &mut DuplexStream, kind: u8, value: &T) {
    let body = russh_sftp::ser::to_bytes(value).expect("response payload");
    peer.write_u32(body.len() as u32 + 1)
        .await
        .expect("response length");
    peer.write_u8(kind).await.expect("response kind");
    peer.write_all(&body).await.expect("response body");
}

async fn data_reply(peer: &mut DuplexStream, read: russh_sftp::protocol::Read, content: &[u8]) {
    let start = read.offset as usize;
    // Force short replies, including for the first request in each range.
    let end = (start + (read.len as usize).min(17_000)).min(content.len());
    reply(
        peer,
        103,
        &Data {
            id: read.id,
            data: content[start..end].to_vec(),
        },
    )
    .await;
}

async fn delayed_peer(
    mut peer: DuplexStream,
    content: Vec<u8>,
    upload: bool,
    change_metadata: bool,
) -> Vec<u8> {
    let window = content
        .len()
        .div_ceil(DOWNLOAD_CHUNK_BYTES as usize)
        .min(REQUEST_WINDOW);
    let mut first_window = true;
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    let mut uploaded = Vec::new();
    while let Some(packet) = request(&mut peer).await {
        let id = packet.get_request_id();
        match packet {
            Packet::Init(_) => reply(&mut peer, 2, &Version::new()).await,
            Packet::Lstat(_) | Packet::Fstat(_) if !upload => {
                let mut attrs = FileAttributes::empty();
                attrs.set_regular(true);
                attrs.size = Some(content.len() as u64);
                attrs.mtime = Some(if change_metadata && !first_window {
                    5678
                } else {
                    1234
                });
                reply(&mut peer, 105, &Attrs { id, attrs }).await;
            }
            Packet::Lstat(_) => {
                reply(
                    &mut peer,
                    101,
                    &Status {
                        id,
                        status_code: StatusCode::NoSuchFile,
                        error_message: String::new(),
                        language_tag: String::new(),
                    },
                )
                .await
            }
            Packet::Open(_) => {
                reply(
                    &mut peer,
                    102,
                    &RemoteHandle {
                        id,
                        handle: "file".into(),
                    },
                )
                .await
            }
            Packet::Read(read) if read.offset as usize >= content.len() => {
                reply(
                    &mut peer,
                    101,
                    &Status {
                        id,
                        status_code: StatusCode::Eof,
                        error_message: String::new(),
                        language_tag: String::new(),
                    },
                )
                .await;
            }
            Packet::Read(read) if first_window => {
                assert_eq!(
                    read.offset,
                    reads.len() as u64 * u64::from(DOWNLOAD_CHUNK_BYTES)
                );
                reads.push(read);
                if reads.len() == window {
                    // A serial implementation cannot reach this point before
                    // its request deadline. Replies deliberately arrive backwards.
                    first_window = false;
                    for read in reads.drain(..).rev() {
                        data_reply(&mut peer, read, &content).await;
                    }
                }
            }
            Packet::Read(read) => data_reply(&mut peer, read, &content).await,
            Packet::Write(write) => {
                assert!(write.data.len() <= DOWNLOAD_CHUNK_BYTES as usize);
                assert_eq!(write.offset as usize, uploaded.len());
                uploaded.extend_from_slice(&write.data);
                if first_window {
                    writes.push(id);
                    if writes.len() == window {
                        first_window = false;
                        for id in writes.drain(..).rev() {
                            reply(&mut peer, 101, &upload_test_status(id)).await;
                        }
                    }
                } else {
                    reply(&mut peer, 101, &upload_test_status(id)).await;
                }
            }
            Packet::Close(_) => reply(&mut peer, 101, &upload_test_status(id)).await,
            Packet::Rename(_) => {
                assert!(writes.is_empty());
                assert_eq!(uploaded, content, "publish follows all acknowledged writes");
                reply(&mut peer, 101, &upload_test_status(id)).await;
            }
            packet => panic!("unexpected request: {packet:?}"),
        }
    }
    uploaded
}

fn content() -> Vec<u8> {
    (0..DOWNLOAD_CHUNK_BYTES as usize * (REQUEST_WINDOW + 2) + 17)
        .map(|index| ((index / 997) % 251) as u8)
        .collect()
}

#[tokio::test]
async fn download_pipelines_real_requests_and_repairs_out_of_order_short_replies() {
    let expected = content();
    let (client, peer) = tokio::io::duplex(128 * 1024);
    let server = tokio::spawn(delayed_peer(peer, expected.clone(), false, false));
    let root = TestCacheDir::new();
    let request =
        SftpDownloadRequest::new(Uuid::new_v4(), "/srv/report.bin".into()).expect("request");
    let (events, _receiver) = mpsc::channel(SFTP_TRANSFER_EVENT_CAPACITY);
    let result = timeout(
        Duration::from_secs(5),
        download_to_cache(
            client,
            &request,
            &TransferCancellation::new(),
            &events,
            root.path(),
        ),
    )
    .await
    .expect("full window before timeout")
    .expect("download");
    assert_eq!(fs::read(result.0).expect("published download"), expected);
    server.await.expect("peer joined");
}

#[tokio::test]
async fn upload_pipelines_real_requests_and_waits_for_reversed_acknowledgements() {
    let expected = content();
    let path = std::env::temp_dir().join(format!("ax-ssh-pipeline-{}.bin", Uuid::new_v4()));
    fs::write(&path, &expected).expect("fixture");
    let request = SftpUploadRequest::from_local_file(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "/srv/report.bin".into(),
        path.clone(),
        expected.len() as u64,
        Vec::new(),
    )
    .expect("request");
    let (client, peer) = tokio::io::duplex(128 * 1024);
    let server = tokio::spawn(delayed_peer(peer, expected.clone(), true, false));
    let (events, _receiver) = mpsc::channel(SFTP_TRANSFER_EVENT_CAPACITY);
    let result = timeout(
        Duration::from_secs(5),
        upload_initialized(client, &request, &TransferCancellation::new(), &events),
    )
    .await
    .expect("full window before timeout")
    .expect("upload");
    assert!(matches!(result, UploadOutcome::Published));
    assert_eq!(server.await.expect("peer joined"), expected);
    fs::remove_file(path).expect("cleanup");
}

#[tokio::test]
async fn failed_final_metadata_check_never_publishes_a_full_download() {
    let (client, peer) = tokio::io::duplex(128 * 1024);
    let server = tokio::spawn(delayed_peer(peer, content(), false, true));
    let root = TestCacheDir::new();
    let request =
        SftpDownloadRequest::new(Uuid::new_v4(), "/srv/report.bin".into()).expect("request");
    let (events, _receiver) = mpsc::channel(SFTP_TRANSFER_EVENT_CAPACITY);
    let result = timeout(
        Duration::from_secs(5),
        download_to_cache(
            client,
            &request,
            &TransferCancellation::new(),
            &events,
            root.path(),
        ),
    )
    .await
    .expect("bounded transfer");
    assert!(result.is_err());
    assert_eq!(
        fs::read_dir(root.path()).expect("cache directory").count(),
        0
    );
    server.await.expect("peer joined");
}

#[tokio::test]
async fn interrupted_download_reopens_saved_partial_and_rejects_changed_prefix() {
    let root = TestCacheDir::new();
    fs::create_dir(root.path()).expect("local destination");
    let journal = root.path().join("journal");
    let bytes = content();
    let offset = u64::from(DOWNLOAD_CHUNK_BYTES);
    let mut transfer = SftpDownloadRequest::for_local_download(
        Uuid::new_v4(),
        "/srv/report.bin".into(),
        root.path().to_owned(),
        vec!["report.bin".into()],
        bytes.len() as u64,
    )
    .expect("download request");
    let store = ResumeStore::open(journal.clone(), "fixture-server".into()).expect("journal");
    transfer.enable_resume(Some(&store));
    let (client, mut peer) = tokio::io::duplex(128 * 1024);
    let server_content = bytes.clone();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        while let Some(packet) = request(&mut peer).await {
            let id = packet.get_request_id();
            match packet {
                Packet::Init(_) => reply(&mut peer, 2, &Version::new()).await,
                Packet::Lstat(_) | Packet::Fstat(_) => {
                    let attrs = resume_test_attrs(server_content.len() as u64);
                    reply(&mut peer, 105, &Attrs { id, attrs }).await;
                }
                Packet::Open(_) => {
                    reply(
                        &mut peer,
                        102,
                        &RemoteHandle {
                            id,
                            handle: "file".into(),
                        },
                    )
                    .await
                }
                Packet::Read(read) if read.offset == 0 => {
                    tokio::time::sleep(PROGRESS_INTERVAL).await;
                    reply(
                        &mut peer,
                        103,
                        &Data {
                            id,
                            data: server_content[..offset as usize].to_vec(),
                        },
                    )
                    .await;
                    stopped.await.expect("client received first chunk");
                    return;
                }
                packet => panic!("unexpected SFTP request: {packet:?}"),
            }
        }
    });
    let (events, mut receiver) = mpsc::channel(SFTP_TRANSFER_EVENT_CAPACITY);
    let progress = tokio::spawn(async move {
        while let Some(event) = receiver.recv().await {
            if matches!(event, SftpTransferEvent::Progress { downloaded_bytes, .. } if downloaded_bytes == offset)
            {
                stop.send(()).expect("peer awaits interruption");
                return;
            }
        }
        panic!("no progress observed before interruption");
    });
    timeout(
        Duration::from_secs(5),
        download_to_local(client, &transfer, &TransferCancellation::new(), &events),
    )
    .await
    .expect("interruption settles")
    .expect_err("connection interrupted");
    server.await.expect("peer completes");
    progress.await.expect("progress observed");
    assert!(!root.path().join("report.bin").exists());
    assert_eq!(
        store.recoveries().expect("interrupted record")[0].offset,
        offset
    );
    transfer.resume = None;
    drop(store);

    let store = ResumeStore::open(journal, "fixture-server".into()).expect("restart opens journal");
    transfer.enable_resume(Some(&store));
    let (events, mut receiver) = mpsc::channel(SFTP_TRANSFER_EVENT_CAPACITY);
    let reads = Arc::new(Mutex::new(Vec::new()));
    let mut changed = bytes.clone();
    changed[0] ^= 1;
    let (client, peer) = tokio::io::duplex(128 * 1024);
    russh_sftp::server::run(
        peer,
        DownloadTestServer {
            reported_size: changed.len() as u64,
            content: Arc::new(changed),
            reads: reads.clone(),
        },
    )
    .await;
    let error = download_to_local(client, &transfer, &TransferCancellation::new(), &events)
        .await
        .expect_err("same-size change rejected");
    assert!(error.to_string().contains("does not match"), "{error:#}");
    assert!(!root.path().join("report.bin").exists());
    reads.lock().expect("read log").clear();

    let (client, peer) = tokio::io::duplex(128 * 1024);
    russh_sftp::server::run(
        peer,
        DownloadTestServer {
            reported_size: bytes.len() as u64,
            content: Arc::new(bytes.clone()),
            reads: reads.clone(),
        },
    )
    .await;
    let (published, _) =
        download_to_local(client, &transfer, &TransferCancellation::new(), &events)
            .await
            .expect("validated download resumed");
    assert_eq!(fs::read(published).expect("downloaded data"), bytes);
    assert!(std::iter::from_fn(|| receiver.try_recv().ok()).any(|event| {
        matches!(event, SftpTransferEvent::ContinuedFromPartial { offset: resumed, .. } if resumed == offset)
    }));
    assert_eq!(
        reads
            .lock()
            .expect("read log")
            .iter()
            .filter(|(start, _)| *start == 0)
            .count(),
        1
    );
    assert!(
        store
            .recoveries()
            .expect("published journal removed")
            .is_empty()
    );
}

fn resume_test_attrs(size: u64) -> FileAttributes {
    let mut attrs = FileAttributes::empty();
    attrs.set_regular(true);
    attrs.size = Some(size);
    attrs.mtime = Some(10);
    attrs
}

#[tokio::test]
async fn upload_resumes_at_confirmed_prefix_and_discards_unconfirmed_tail() {
    let root = TestCacheDir::new();
    fs::create_dir(root.path()).expect("source directory");
    let bytes = content();
    let prefix = DOWNLOAD_CHUNK_BYTES as usize;
    let local = root.path().join("upload.bin");
    fs::write(&local, &bytes).expect("upload source");
    let mut transfer = SftpUploadRequest::from_local_file(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "/srv/upload.bin".into(),
        local.clone(),
        bytes.len() as u64,
        Vec::new(),
    )
    .expect("upload request");
    let store =
        ResumeStore::open(root.path().join("journal"), "fixture-server".into()).expect("journal");
    transfer.enable_resume(Some(&store));
    let key = ResumeKey {
        transfer_id: transfer.transfer_id,
        direction: TransferDirection::Upload,
        remote_path: transfer.remote_path.clone(),
        local_path: local,
    };
    let mut record = store
        .create(key, bytes.len() as u64)
        .expect("plan record")
        .expect("space");
    record.offset = prefix as u64;
    record.digest = Sha256::digest(&bytes[..prefix]).into();
    record.local_source = Some(transfer.local_identity.as_ref().clone());
    store.save(&record).expect("persist checkpoint");
    let expected = bytes.clone();
    let (client, mut peer) = tokio::io::duplex(128 * 1024);
    let server = tokio::spawn(async move {
        let mut remote = expected[..prefix].to_vec();
        remote.extend_from_slice(b"unconfirmed tail");
        let mut truncated = false;
        let mut published = false;
        while let Some(packet) = request(&mut peer).await {
            let id = packet.get_request_id();
            match packet {
                Packet::Init(_) => reply(&mut peer, 2, &Version::new()).await,
                Packet::Lstat(query) if query.path == "/srv/upload.bin" => {
                    reply(
                        &mut peer,
                        101,
                        &Status {
                            id,
                            status_code: StatusCode::NoSuchFile,
                            error_message: String::new(),
                            language_tag: String::new(),
                        },
                    )
                    .await
                }
                Packet::Lstat(_) | Packet::Fstat(_) => {
                    reply(
                        &mut peer,
                        105,
                        &Attrs {
                            id,
                            attrs: resume_test_attrs(remote.len() as u64),
                        },
                    )
                    .await
                }
                Packet::Open(open) => {
                    assert!(!open.pflags.contains(OpenFlags::TRUNCATE));
                    reply(
                        &mut peer,
                        102,
                        &RemoteHandle {
                            id,
                            handle: "partial".into(),
                        },
                    )
                    .await;
                }
                Packet::Read(read) => {
                    assert!(read.offset as usize + read.len as usize <= prefix);
                    data_reply(&mut peer, read, &remote).await;
                }
                Packet::FSetStat(set) => {
                    assert_eq!(set.attrs.size, Some(prefix as u64));
                    remote.truncate(prefix);
                    truncated = true;
                    reply(&mut peer, 101, &upload_test_status(id)).await;
                }
                Packet::Write(write) => {
                    assert!(truncated);
                    assert_eq!(write.offset as usize, remote.len());
                    remote.extend_from_slice(&write.data);
                    reply(&mut peer, 101, &upload_test_status(id)).await;
                }
                Packet::Close(_) => reply(&mut peer, 101, &upload_test_status(id)).await,
                Packet::Rename(_) => {
                    assert_eq!(remote, expected);
                    published = true;
                    reply(&mut peer, 101, &upload_test_status(id)).await;
                }
                packet => panic!("unexpected SFTP request: {packet:?}"),
            }
        }
        assert!(published);
    });
    let (events, mut receiver) = mpsc::channel(SFTP_TRANSFER_EVENT_CAPACITY);
    let outcome = timeout(
        Duration::from_secs(5),
        upload_initialized(client, &transfer, &TransferCancellation::new(), &events),
    )
    .await
    .expect("upload settles")
    .expect("resumed upload");
    assert!(matches!(outcome, UploadOutcome::Published));
    server.await.expect("server saw verified publication");
    assert!(matches!(
        receiver.try_recv(),
        Ok(SftpTransferEvent::CheckingPartial { .. })
    ));
    assert!(
        matches!(receiver.try_recv(), Ok(SftpTransferEvent::ContinuedFromPartial { offset, .. }) if offset == prefix as u64)
    );
    assert!(
        store
            .recoveries()
            .expect("published record removed")
            .is_empty()
    );
}
