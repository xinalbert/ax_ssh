use super::*;
use tokio::sync::Semaphore;
use tokio::time::timeout;

struct DeviceLease {
    dropped: Arc<AtomicBool>,
    closing: Option<Arc<tokio::sync::Notify>>,
    close_gate: Option<sync::Receiver<()>>,
}
impl Drop for DeviceLease {
    fn drop(&mut self) {
        if let Some(closing) = &self.closing {
            closing.notify_one();
        }
        if let Some(gate) = &self.close_gate {
            gate.recv_timeout(Duration::from_secs(3))
                .expect("fixture device is allowed to close");
        }
        self.dropped.store(true, Ordering::Release);
    }
}

fn source(
    format: AudioFormat,
    rx: sync::Receiver<NativeBlock>,
    lease: DeviceLease,
    failed: Arc<AtomicBool>,
) -> Result<CaptureSource<DeviceLease>> {
    Ok(CaptureSource {
        stream: lease,
        native_rx: rx,
        failed,
        converter: PcmConverter::new(16_000, format)?,
    })
}
fn device_lease(dropped: Arc<AtomicBool>) -> DeviceLease {
    DeviceLease {
        dropped,
        closing: None,
        close_gate: None,
    }
}

#[tokio::test]
async fn capture_permission_failure_and_cancellation_never_open_the_device() {
    let gate = Arc::new(Semaphore::new(1));
    let (_stop, stopped) = watch::channel(false);
    let (pcm, _packets) = mpsc::channel(PCM_QUEUE_PACKETS);
    let (ready, completion) = oneshot::channel();
    let result = capture_with(
        stopped,
        AudioFormat::SPEECH,
        pcm,
        ready,
        gate.clone(),
        async { anyhow::bail!("injected permission rejection") },
        |_, _| -> Result<CaptureSource<()>> {
            panic!("permission rejection must precede device access")
        },
    )
    .await;
    assert!(
        result
            .expect_err("permission denial")
            .to_string()
            .contains("permission rejection")
    );
    assert!(completion.await.is_err());
    assert_eq!(gate.available_permits(), 1);
    let (stop, stopped) = watch::channel(false);
    let (pcm, _packets) = mpsc::channel(PCM_QUEUE_PACKETS);
    let (ready, _completion) = oneshot::channel();
    let task = tokio::spawn(capture_with(
        stopped,
        AudioFormat::SPEECH,
        pcm,
        ready,
        gate.clone(),
        std::future::pending::<Result<()>>(),
        |_, _| -> Result<CaptureSource<()>> {
            panic!("cancelled permission must not open the device")
        },
    ));
    stop.send_replace(true);
    timeout(Duration::from_secs(2), task)
        .await
        .expect("permission cancellation deadline")
        .expect("capture joins")
        .expect("cancelled before open");
    assert_eq!(gate.available_permits(), 1);
}

#[tokio::test]
async fn actual_capture_worker_converts_samples_and_stops_with_owned_device_lease() {
    let gate = Arc::new(Semaphore::new(1));
    let dropped = Arc::new(AtomicBool::new(false));
    let (native, rx) = sync::sync_channel(8);
    let (stop, stopped) = watch::channel(false);
    let (pcm, mut packets) = mpsc::channel(PCM_QUEUE_PACKETS);
    let (ready, completion) = oneshot::channel();
    let lease = device_lease(dropped.clone());
    let task = tokio::spawn(capture_with(
        stopped,
        AudioFormat::SPEECH,
        pcm,
        ready,
        gate.clone(),
        async { Ok(()) },
        move |_, format| source(format, rx, lease, Arc::new(AtomicBool::new(false))),
    ));
    completion
        .await
        .expect("startup signal")
        .expect("worker opened injected device");
    assert_eq!(gate.available_permits(), 0);
    native
        .send(NativeBlock {
            samples: [0.25; NATIVE_FRAMES * 8],
            len: NATIVE_FRAMES,
        })
        .expect("native callback block");
    let packet = timeout(Duration::from_secs(2), packets.recv())
        .await
        .expect("PCM deadline")
        .expect("converted PCM");
    assert_eq!(packet.len(), AudioFormat::SPEECH.packet_bytes());
    // The beginning contains the resampling filter's transient. Check settled
    // samples while preserving the production filter and packet boundary.
    let tail = packet.len() - 2;
    assert!((i16::from_le_bytes([packet[tail], packet[tail + 1]]) - 8192).abs() < 10);
    stop.send_replace(true);
    timeout(Duration::from_secs(2), task)
        .await
        .expect("capture shutdown deadline")
        .expect("capture joins")
        .expect("capture stops");
    assert!(dropped.load(Ordering::Acquire));
    assert_eq!(gate.available_permits(), 1);
}

#[tokio::test]
async fn capture_worker_reports_startup_device_eof_and_backpressure_errors() {
    let gate = Arc::new(Semaphore::new(1));
    let (_stop, stopped) = watch::channel(false);
    let (pcm, _packets) = mpsc::channel(PCM_QUEUE_PACKETS);
    let (ready, completion) = oneshot::channel();
    let task = tokio::spawn(capture_with(
        stopped,
        AudioFormat::SPEECH,
        pcm,
        ready,
        gate.clone(),
        async { Ok(()) },
        |_, _| -> Result<CaptureSource<()>> { anyhow::bail!("injected open failure") },
    ));
    assert!(completion.await.expect("startup error").is_err());
    assert!(task.await.expect("worker joins").is_err());
    for mode in 0..3 {
        let (native, rx) = sync::sync_channel(8);
        if mode == 2 {
            for _ in 0..2 {
                native
                    .send(NativeBlock {
                        samples: [0.0; NATIVE_FRAMES * 8],
                        len: NATIVE_FRAMES,
                    })
                    .expect("overflow block");
            }
        }
        let dropped = Arc::new(AtomicBool::new(false));
        let lease = device_lease(dropped.clone());
        let (_stop, stopped) = watch::channel(false);
        let (pcm, _packets) = mpsc::channel(1);
        let (ready, completion) = oneshot::channel();
        let failed = Arc::new(AtomicBool::new(mode == 0));
        let task = tokio::spawn(capture_with(
            stopped,
            AudioFormat::SPEECH,
            pcm,
            ready,
            gate.clone(),
            async { Ok(()) },
            move |_, format| source(format, rx, lease, failed),
        ));
        completion
            .await
            .expect("startup")
            .expect("injected device opened");
        if mode == 1 {
            drop(native);
        }
        let error = timeout(Duration::from_secs(2), task)
            .await
            .expect("device error deadline")
            .expect("worker joins")
            .expect_err("capture error");
        let expected = match mode {
            0 => "device error",
            1 => "stream ended",
            _ => "too slow",
        };
        assert!(format!("{error:#}").contains(expected));
        assert!(dropped.load(Ordering::Acquire));
        assert_eq!(gate.available_permits(), 1);
    }
}

#[tokio::test]
async fn dropping_capture_future_does_not_release_lease_before_the_device_closes() {
    let gate = Arc::new(Semaphore::new(1));
    let closing = Arc::new(tokio::sync::Notify::new());
    let dropped = Arc::new(AtomicBool::new(false));
    let (release, close_gate) = sync::sync_channel(1);
    let (_native, rx) = sync::sync_channel(8);
    let lease = DeviceLease {
        dropped: dropped.clone(),
        closing: Some(closing.clone()),
        close_gate: Some(close_gate),
    };
    let (_stop, stopped) = watch::channel(false);
    let (pcm, _packets) = mpsc::channel(PCM_QUEUE_PACKETS);
    let (ready, completion) = oneshot::channel();
    let task = tokio::spawn(capture_with(
        stopped,
        AudioFormat::SPEECH,
        pcm,
        ready,
        gate.clone(),
        async { Ok(()) },
        move |_, format| source(format, rx, lease, Arc::new(AtomicBool::new(false))),
    ));
    completion
        .await
        .expect("startup signal")
        .expect("injected device opened");
    task.abort();
    assert!(task.await.expect_err("caller cancelled").is_cancelled());
    timeout(Duration::from_secs(2), closing.notified())
        .await
        .expect("device is closing");
    assert!(!dropped.load(Ordering::Acquire));
    let (_stop, stopped) = watch::channel(false);
    let (pcm, _packets) = mpsc::channel(PCM_QUEUE_PACKETS);
    let (ready, _completion) = oneshot::channel();
    let result = capture_with(
        stopped,
        AudioFormat::SPEECH,
        pcm,
        ready,
        gate.clone(),
        async { Ok(()) },
        |_, _| -> Result<CaptureSource<()>> { panic!("an overlapping device must not open") },
    )
    .await;
    assert!(
        result
            .expect_err("device lease remains held")
            .to_string()
            .contains("already in use")
    );
    release
        .send(())
        .expect("allow real worker to finish closing");
    timeout(Duration::from_secs(2), async {
        while gate.available_permits() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("device lease released after close");
    assert!(dropped.load(Ordering::Acquire));
}
