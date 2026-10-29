use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

async fn read_test_packet(stream: &mut DuplexStream) -> Vec<u8> {
    let mut length = [0_u8; 4];
    stream
        .read_exact(&mut length)
        .await
        .expect("SFTP request length should arrive");
    let mut packet = vec![0_u8; u32::from_be_bytes(length) as usize];
    stream
        .read_exact(&mut packet)
        .await
        .expect("SFTP request body should arrive");
    packet
}

async fn write_test_packet(stream: &mut DuplexStream, packet: &[u8]) {
    stream
        .write_all(&(packet.len() as u32).to_be_bytes())
        .await
        .expect("SFTP response length should write");
    stream
        .write_all(packet)
        .await
        .expect("SFTP response body should write");
}

fn push_test_string(packet: &mut Vec<u8>, value: &str) {
    packet.extend_from_slice(&(value.len() as u32).to_be_bytes());
    packet.extend_from_slice(value.as_bytes());
}

#[test]
fn browser_reconnects_only_after_transport_or_protocol_failure() {
    use russh_sftp::protocol::{Status, StatusCode};

    let permission_denied = anyhow::Error::from(SftpClientError::Status(Status {
        id: 1,
        status_code: StatusCode::PermissionDenied,
        error_message: "denied".to_owned(),
        language_tag: String::new(),
    }));
    assert!(!browser_transport_failed(&permission_denied));
    let connection_lost = anyhow::Error::from(SftpClientError::Status(Status {
        id: 2,
        status_code: StatusCode::ConnectionLost,
        error_message: "lost".to_owned(),
        language_tag: String::new(),
    }));
    assert!(browser_transport_failed(&connection_lost));
    assert!(!browser_transport_failed(&anyhow::anyhow!("invalid path")));
    assert!(browser_transport_failed(&anyhow::Error::from(
        SftpClientError::Timeout
    )));
    assert!(browser_transport_failed(&anyhow::Error::from(
        SftpClientError::UnexpectedBehavior("session closed".to_owned())
    )));
}

#[tokio::test]
async fn directory_page_does_not_wait_for_another_batch_or_close_ack() {
    use russh_sftp::protocol::{Name, Status};

    let (stream, mut peer) = tokio::io::duplex(4096);
    let (allow_close, close_allowed) = oneshot::channel();
    let server = tokio::spawn(async move {
        read_test_packet(&mut peer).await;
        write_test_packet(&mut peer, &[2, 0, 0, 0, 3]).await;
        let request = read_test_packet(&mut peer).await;
        assert_eq!(request[0], 12);
        let id = u32::from_be_bytes(request[1..5].try_into().expect("request id"));
        let mut attrs = FileAttributes::empty();
        attrs.set_regular(true);
        let name = Name {
            id,
            files: vec![File::new("first.txt", attrs)],
        };
        let mut packet = vec![104];
        packet.extend_from_slice(&russh_sftp::ser::to_bytes(&name).expect("name payload"));
        write_test_packet(&mut peer, &packet).await;
        // No EOF response is available until the caller requests More.
        let request = read_test_packet(&mut peer).await;
        assert_eq!(request[0], 12);
        let id = u32::from_be_bytes(request[1..5].try_into().expect("request id"));
        let mut packet = vec![101];
        packet.extend_from_slice(
            &russh_sftp::ser::to_bytes(&Status {
                id,
                status_code: StatusCode::Eof,
                error_message: String::new(),
                language_tag: String::new(),
            })
            .expect("EOF payload"),
        );
        write_test_packet(&mut peer, &packet).await;
        let request = read_test_packet(&mut peer).await;
        assert_eq!(request[0], 4);
        close_allowed.await.expect("test permits CLOSE response");
        let mut packet = vec![101];
        packet.extend_from_slice(&request[1..5]);
        packet.extend_from_slice(&0_u32.to_be_bytes());
        push_test_string(&mut packet, "");
        push_test_string(&mut packet, "");
        write_test_packet(&mut peer, &packet).await;
        let mut byte = [0];
        assert_eq!(peer.read(&mut byte).await.expect("session EOF"), 0);
    });
    let session = RawSftpSession::new(stream);
    session.init().await.expect("handshake");
    let mut cursor = DirectoryCursor {
        path: "/srv".into(),
        handle: "directory".into(),
        pending: VecDeque::new(),
        done: false,
        truncated: false,
    };
    let page = timeout(Duration::from_secs(1), read_page(&session, &mut cursor))
        .await
        .expect("first page before EOF")
        .expect("page");
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].name, "first.txt");
    assert!(!cursor.done);
    let (events, mut receiver) = mpsc::channel(2);
    let emitter = tokio::spawn(async move {
        emit_page(&session, &mut cursor, Some(1), true, &events)
            .await
            .expect("final page");
        session.close_session().expect("close session");
    });
    let event = timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("page before CLOSE ack")
        .expect("event");
    assert!(matches!(
        event,
        SftpBrowserEvent::DirectoryPage {
            has_more: false,
            ..
        }
    ));
    allow_close.send(()).expect("release server");
    emitter.await.expect("emitter joined");
    server.await.expect("server joined");
}

#[tokio::test]
async fn idle_browser_reports_a_closed_sftp_channel() {
    let (stream, mut server) = tokio::io::duplex(1024);
    let (_command_tx, command_rx) = mpsc::channel(COMMAND_CAPACITY);
    let (event_tx, mut events) = mpsc::channel(8);
    let browser = tokio::spawn(run_browser(stream, ".".to_owned(), command_rx, event_tx));
    let server_task = tokio::spawn(async move {
        assert_eq!(read_test_packet(&mut server).await[0], 1); // SSH_FXP_INIT
        write_test_packet(&mut server, &[2, 0, 0, 0, 3]).await; // VERSION 3

        let realpath = read_test_packet(&mut server).await;
        assert_eq!(realpath[0], 16);
        let mut name = vec![104]; // SSH_FXP_NAME
        name.extend_from_slice(&realpath[1..5]);
        name.extend_from_slice(&1_u32.to_be_bytes());
        push_test_string(&mut name, "/home/alice");
        push_test_string(&mut name, "/home/alice");
        name.extend_from_slice(&0_u32.to_be_bytes()); // empty attributes
        write_test_packet(&mut server, &name).await;

        let opendir = read_test_packet(&mut server).await;
        assert_eq!(opendir[0], 11);
        let mut handle = vec![102]; // SSH_FXP_HANDLE
        handle.extend_from_slice(&opendir[1..5]);
        push_test_string(&mut handle, "directory");
        write_test_packet(&mut server, &handle).await;

        let readdir = read_test_packet(&mut server).await;
        assert_eq!(readdir[0], 12);
        let mut eof = vec![101]; // SSH_FXP_STATUS
        eof.extend_from_slice(&readdir[1..5]);
        eof.extend_from_slice(&1_u32.to_be_bytes()); // EOF
        push_test_string(&mut eof, "");
        push_test_string(&mut eof, "");
        write_test_packet(&mut server, &eof).await;

        let close = read_test_packet(&mut server).await;
        assert_eq!(close[0], 4);
        let mut ok = vec![101];
        ok.extend_from_slice(&close[1..5]);
        ok.extend_from_slice(&0_u32.to_be_bytes());
        push_test_string(&mut ok, "");
        push_test_string(&mut ok, "");
        write_test_packet(&mut server, &ok).await;
        // Dropping this peer simulates a closed SFTP channel with no command pending.
    });

    let opened = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("SFTP home should arrive")
        .expect("browser should still run");
    assert!(matches!(opened, SftpBrowserEvent::Opened { .. }));
    let page = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("initial page should arrive")
        .expect("browser should still run");
    assert!(matches!(page, SftpBrowserEvent::DirectoryPage { .. }));
    server_task.await.expect("test SFTP peer should finish");

    let failed = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("connection failure should arrive")
        .expect("browser should report failure");
    assert!(matches!(failed, SftpBrowserEvent::Failed { .. }));
    let closed = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("closed event should arrive")
        .expect("browser should report closure");
    assert!(matches!(closed, SftpBrowserEvent::Closed));
    browser.await.expect("browser task should exit");
}

#[tokio::test]
async fn packet_limiter_preserves_fragmented_valid_frames() {
    let (reader, mut writer) = tokio::io::duplex(32);
    let writer_task = tokio::spawn(async move {
        writer
            .write_all(&[0, 0])
            .await
            .expect("first header fragment should write");
        writer
            .write_all(&[0, 3, b'a'])
            .await
            .expect("second frame fragment should write");
        writer
            .write_all(b"bc")
            .await
            .expect("remaining payload should write");
    });
    let mut limited = PacketLimitedStream::new(reader);
    let mut packet = [0_u8; 7];

    limited
        .read_exact(&mut packet)
        .await
        .expect("valid fragmented frame should pass through");
    writer_task.await.expect("writer task should finish");

    assert_eq!(packet, [0, 0, 0, 3, b'a', b'b', b'c']);
}

#[tokio::test]
async fn packet_limiter_supports_tiny_reads_and_consecutive_frames() {
    let (reader, mut writer) = tokio::io::duplex(32);
    writer
        .write_all(&[0, 0, 0, 1, b'a', 0, 0, 0, 2, b'b', b'c'])
        .await
        .expect("two frames should write");
    drop(writer);
    let mut limited = PacketLimitedStream::new(reader);
    let mut bytes = Vec::new();
    let mut byte = [0_u8; 1];

    while limited
        .read(&mut byte)
        .await
        .expect("tiny packet read should succeed")
        != 0
    {
        bytes.push(byte[0]);
    }

    assert_eq!(bytes, [0, 0, 0, 1, b'a', 0, 0, 0, 2, b'b', b'c']);
}

#[tokio::test]
async fn packet_limiter_rejects_oversized_frames_before_payload_allocation() {
    let (reader, mut writer) = tokio::io::duplex(32);
    writer
        .write_all(&(MAX_PACKET_BYTES + 1).to_be_bytes())
        .await
        .expect("oversized header should write");
    let mut limited = PacketLimitedStream::new(reader);
    let mut byte = [0_u8; 1];

    let error = limited
        .read_exact(&mut byte)
        .await
        .expect_err("oversized frame should be rejected");

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("exceeds"));
    assert_eq!(
        limited
            .read(&mut byte)
            .await
            .expect("failed limiter should become EOF"),
        0
    );
}

#[tokio::test]
async fn packet_limiter_rejects_truncated_frames() {
    let (reader, mut writer) = tokio::io::duplex(32);
    writer
        .write_all(&[0, 0, 0, 3, b'a'])
        .await
        .expect("partial frame should write");
    drop(writer);
    let mut limited = PacketLimitedStream::new(reader);
    let mut packet = [0_u8; 7];

    let error = limited
        .read_exact(&mut packet)
        .await
        .expect_err("truncated frame should be rejected");

    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    assert_eq!(
        limited
            .read(&mut packet[..1])
            .await
            .expect("truncated limiter should become EOF"),
        0
    );
}

#[test]
fn remote_paths_resolve_against_current_and_home() {
    assert_eq!(
        resolve_remote_path("/srv/app/current", "/home/alice", "../logs")
            .expect("relative path should resolve"),
        "/srv/app/logs"
    );
    assert_eq!(
        resolve_remote_path("/srv/app", "/home/alice", "~/notes")
            .expect("home path should resolve"),
        "/home/alice/notes"
    );
    assert_eq!(
        resolve_remote_path("/srv/app", "/home/alice", "notes ")
            .expect("valid trailing spaces should be preserved"),
        "/srv/app/notes "
    );
}

#[test]
fn remote_paths_reject_control_characters_but_accept_long_paths() {
    assert!(validate_remote_path("/tmp/line\nbreak").is_err());
    assert!(validate_remote_path(&format!("/{}", "a".repeat(5000))).is_ok());
}

#[test]
fn entries_reject_ambiguous_or_unsafe_names() {
    let attrs = russh_sftp::protocol::FileAttributes::empty();
    assert!(bounded_entry("/tmp", File::new("..", attrs.clone())).is_none());
    assert!(bounded_entry("/tmp", File::new("bad/name", attrs.clone())).is_none());
    assert!(bounded_entry("/tmp", File::new("line\nbreak", attrs.clone())).is_none());
    assert!(bounded_entry("/tmp", File::new("n".repeat(600), attrs)).is_some());
}
