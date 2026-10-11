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
