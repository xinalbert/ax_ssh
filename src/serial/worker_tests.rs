use super::*;
use std::io;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};
use tokio::io::{AsyncRead, AsyncWrite, DuplexStream, ReadBuf};

fn pair(
    capacity: usize,
) -> (
    SerialSessionHandle,
    mpsc::Receiver<SerialSessionEvent>,
    DuplexStream,
) {
    let (port, peer) = tokio::io::duplex(capacity);
    let (worker, events) = SerialSessionHandle::spawn_with_port(
        &Handle::current(),
        Uuid::new_v4(),
        SerialConfig {
            port_name: "injected-port".into(),
            ..Default::default()
        },
        move |_| Ok(port),
    );
    (worker, events, peer)
}

async fn next(events: &mut mpsc::Receiver<SerialSessionEvent>) -> SerialSessionEvent {
    timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("worker event deadline")
        .expect("worker event")
}

#[tokio::test]
async fn serial_worker_preserves_paste_and_timestamps_bounded_output() {
    let (worker, mut events, mut peer) = pair(128);
    assert!(
        matches!(next(&mut events).await, SerialSessionEvent::Connected { port_name } if port_name == "injected-port")
    );
    let paste = (0..128 * 1024).map(|value| value as u8).collect::<Vec<_>>();
    worker.request_send(Vec::new()).expect("empty input");
    worker
        .request_send_paste(paste.clone())
        .expect("large paste");
    let reader = tokio::spawn(async move {
        let mut received = vec![0; paste.len()];
        peer.read_exact(&mut received).await.expect("paste read");
        assert_eq!(received, paste);
        let before = std::time::Instant::now();
        peer.write_all(b"device output\r\n")
            .await
            .expect("device output");
        (peer, before)
    });
    let (peer, before) = timeout(Duration::from_secs(2), reader)
        .await
        .expect("paste deadline")
        .expect("peer joins");
    let mut output = Vec::new();
    while output.len() < b"device output\r\n".len() {
        let SerialSessionEvent::Output(chunk) = next(&mut events).await else {
            panic!("expected device output");
        };
        assert!(chunk.data.len() <= MAX_OUTPUT_BATCH_BYTES);
        assert!(chunk.received_at >= before);
        output.extend(chunk.data);
    }
    assert_eq!(output, b"device output\r\n");
    worker.request_disconnect().expect("disconnect intent");
    assert_eq!(next(&mut events).await, SerialSessionEvent::Disconnected);
    worker.shutdown().await.expect("real worker joins");
    drop(peer);
}

#[tokio::test]
async fn serial_worker_eof_and_dropped_observer_release_the_stream() {
    let (worker, mut events, peer) = pair(1);
    assert!(matches!(
        next(&mut events).await,
        SerialSessionEvent::Connected { .. }
    ));
    drop(peer);
    assert_eq!(next(&mut events).await, SerialSessionEvent::Disconnected);
    worker.shutdown().await.expect("EOF joins");
    let (worker, events, mut peer) = pair(1);
    drop(events);
    worker.shutdown().await.expect("observer drop joins");
    assert_eq!(
        peer.read_u8()
            .await
            .expect_err("stream was released")
            .kind(),
        io::ErrorKind::UnexpectedEof
    );
}

// Failure streams are boundary implementations; the real serial worker consumes them.
struct BrokenPort {
    read_error: bool,
}
impl AsyncRead for BrokenPort {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut TaskContext<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.read_error {
            Poll::Ready(Err(io::Error::other("read failed")))
        } else {
            Poll::Pending
        }
    }
}
impl AsyncWrite for BrokenPort {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut TaskContext<'_>,
        _: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Err(io::Error::other("write failed")))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::other("shutdown failed")))
    }
}

#[tokio::test]
async fn serial_worker_reports_open_read_and_write_errors_without_false_disconnect() {
    let (worker, mut events) = SerialSessionHandle::spawn_with_port::<BrokenPort, _>(
        &Handle::current(),
        Uuid::new_v4(),
        SerialConfig::default(),
        |_| anyhow::bail!("{}", "故".repeat(MAX_ERROR_CHARS + 20)),
    );
    let SerialSessionEvent::Failed(message) = next(&mut events).await else {
        panic!("open failure event");
    };
    assert_eq!(message.chars().count(), MAX_ERROR_CHARS);
    worker.shutdown().await.expect("failed open joins");
    assert!(events.recv().await.is_none());
    for read_error in [true, false] {
        let (worker, mut events) = SerialSessionHandle::spawn_with_port(
            &Handle::current(),
            Uuid::new_v4(),
            SerialConfig::default(),
            move |_| Ok(BrokenPort { read_error }),
        );
        assert!(matches!(
            next(&mut events).await,
            SerialSessionEvent::Connected { .. }
        ));
        if !read_error {
            worker
                .request_send(b"input".to_vec())
                .expect("write request");
        }
        let expected = if read_error {
            "read failed"
        } else {
            "write failed"
        };
        assert!(
            matches!(next(&mut events).await, SerialSessionEvent::Failed(message) if message == expected)
        );
        worker.shutdown().await.expect("failed I/O joins");
        assert!(
            events.recv().await.is_none(),
            "failure must not also announce normal disconnect"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn real_serial_worker_backpressure_is_bounded_and_shutdown_aborts_stalled_write() {
    let (worker, mut events, mut peer) = pair(1);
    assert!(matches!(
        next(&mut events).await,
        SerialSessionEvent::Connected { .. }
    ));
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
    assert!(
        worker
            .request_send_motion(Vec::new())
            .expect("empty motion")
    );
    worker
        .request_send(vec![42; MAX_INPUT_BYTES])
        .expect("stalled write");
    assert_eq!(peer.read_u8().await.expect("write has started"), 42);
    for _ in 0..COMMAND_CAPACITY {
        worker.request_send(vec![1]).expect("fill bounded queue");
    }
    assert!(
        !worker
            .request_send_motion(vec![2])
            .expect("motion is lossy under pressure")
    );
    assert!(worker.request_send(vec![3]).is_err());
    assert!(worker.request_disconnect().is_err());
    let started = tokio::time::Instant::now();
    worker
        .shutdown()
        .await
        .expect("stalled real worker is aborted and joined");
    assert!(tokio::time::Instant::now() - started >= WORKER_SHUTDOWN_TIMEOUT);
}
