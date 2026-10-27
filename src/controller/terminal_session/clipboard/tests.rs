use super::*;

pub(in crate::controller::terminal_session) fn fixture(enabled: bool) -> (Worker, TerminalClient) {
    let settings = TerminalSettings {
        osc52_clipboard: enabled,
        ..Default::default()
    };
    let mut model = new_model_with_settings(TerminalSize::model(80, 24), &settings);
    let shared = Arc::new(Mutex::new(Shared {
        view: TerminalView {
            semantic: Vec::new(),
            terminal: model.snapshot(),
            status: String::new(),
            connected: true,
            selection_revision: 0,
            generation: 1,
            identity: ConnectionIdentity::Local,
            prompt: None,
        },
        clipboard: None,
        selection: None,
        remote: SharedClipboard::default(),
        image: None,
    }));
    let (commands, _receiver) = mpsc::channel(64);
    let (focus, focus_changes) = watch::channel(false);
    let client = TerminalClient {
        commands,
        focus,
        generation: Some(1),
        shared: shared.clone(),
    };
    let worker = Worker {
        presentation: refresh::Presentation::default(),
        focus: focus_changes,
        semantic: crate::controller::semantic::Cache::default(),
        audio_format: AudioSettings::default().format(),
        microphone_forwarding: false,
        x11_settings: X11Settings::default(),
        model,
        settings,
        transport: None,
        events: Events::Idle,
        profile: None,
        pending_secret: None,
        prompt: None,
        generation: 1,
        identity: ConnectionIdentity::Local,
        reconnect: super::super::super::ReconnectState::new(false),
        retry_deadline: None,
        shared,
        wake: Arc::new(|| {}),
        status: String::new(),
        connected: true,
        last_resize: None,
        viewport: TerminalViewport::default(),
        remote_read: None,
        attachments: Default::default(),
    };
    (worker, client)
}

fn output(worker: &mut Worker, bytes: &[u8]) {
    let mut effects = TerminalOutput::process(&mut worker.model, bytes);
    worker.offer_clipboard(&mut effects).unwrap();
}

#[tokio::test]
async fn coalesced_disable_and_reenable_revoke_a_queued_answer_before_dispatch() {
    let (mut worker, client) = fixture(true);
    output(&mut worker, b"\x1b]52;c;?\x07");
    let token = client.clipboard_read().unwrap().unwrap();
    let mut settings = SessionSettings {
        refresh: refresh::Rates::new(20, 2),
        terminal: worker.settings.clone(),
        x11: worker.x11_settings.clone(),
        audio_format: AudioFormat {
            sample_rate: 96000,
            channels: 4,
        },
    };
    let (updates, receiver) = watch::channel(settings.clone());
    settings.terminal.osc52_clipboard = false;
    updates.send(settings.clone()).unwrap();
    settings.terminal.osc52_clipboard = true;
    settings.x11.provider = crate::config::X11ServerProvider::Custom;
    settings.x11.app_path = "/fixture/x-server".into();
    let expected_x11 = settings.x11.clone();
    updates.send(settings).unwrap();
    let (commands, queued) = mpsc::channel(1);
    commands
        .try_send(QueuedCommand {
            generation: worker.generation,
            command: Command::Clipboard(token, Some("revoked contents".into()), 0),
        })
        .unwrap_or_else(|_| panic!("fixture queue full"));
    let (stop, cancelled) = watch::channel(false);
    let task = tokio::spawn(async move {
        let result = worker.run(queued, cancelled, receiver).await;
        (worker, result)
    });
    while client.clipboard_read().unwrap().is_some() {
        tokio::task::yield_now().await;
    }
    tokio::task::yield_now().await;
    stop.send_replace(true);
    let (worker, result) = task.await.unwrap();
    result.unwrap();
    assert_eq!(worker.x11_settings, expected_x11);
    assert_eq!(worker.presentation.rates, refresh::Rates::new(20, 2));
    assert_eq!(
        worker.audio_format,
        AudioFormat {
            sample_rate: 96000,
            channels: 4
        }
    );
    // If the old answer reached transport, the fixture's missing transport
    // would produce an error status instead of silently discarding the answer.
    assert!(worker.status.is_empty());
    assert!(worker.remote_read.is_none());
}

#[tokio::test(start_paused = true)]
async fn disabled_and_revoked_access_never_publish_clipboard_effects() {
    let (mut worker, client) = fixture(false);
    output(&mut worker, b"\x1b]52;c;SGVsbG8=\x07\x1b]52;c;?\x07");
    assert!(client.clipboard_read().unwrap().is_none());
    assert!(client.take_remote_clipboard().unwrap().is_none());
    let mut settings = worker.settings.clone();
    settings.osc52_clipboard = true;
    worker.configure(settings.clone()).unwrap();
    output(&mut worker, b"\x1b]52;c;SGVsbG8=\x07\x1b]52;c;?\x07");
    let token = client.clipboard_read().unwrap().unwrap();
    settings.osc52_clipboard = false;
    worker.configure(settings.clone()).unwrap();
    settings.osc52_clipboard = true;
    worker.configure(settings).unwrap();
    worker
        .answer_clipboard(token, Some("never sent".into()), 0)
        .unwrap();
    assert!(client.take_remote_clipboard().unwrap().is_none());
    assert!(client.clipboard_read().unwrap().is_none());
}

#[tokio::test(start_paused = true)]
async fn requests_are_latest_only_expire_and_deny_once_without_transport_access() {
    let (mut worker, client) = fixture(true);
    output(&mut worker, b"\x1b]52;c;?\x07");
    let old = client.clipboard_read().unwrap().unwrap();
    output(&mut worker, b"\x1b]52;c;?\x07");
    let current = client.clipboard_read().unwrap().unwrap();
    assert_ne!(old, current);
    worker
        .answer_clipboard(old, Some("not authorized".into()), 0)
        .unwrap();
    assert_eq!(client.clipboard_read().unwrap(), Some(current));
    worker.answer_clipboard(current, None, 0).unwrap();
    worker
        .answer_clipboard(current, Some("already denied".into()), 0)
        .unwrap();
    assert!(client.clipboard_read().unwrap().is_none());
    output(&mut worker, b"\x1b]52;c;?\x07");
    let timed = client.clipboard_read().unwrap().unwrap();
    tokio::time::advance(READ_TIMEOUT).await;
    assert!(client.clipboard_read().unwrap().is_none());
    worker
        .answer_clipboard(timed, Some("expired".into()), 0)
        .unwrap();
    assert!(worker.remote_read.is_none());
    worker.model.process(&b"history\r\n".repeat(100));
    output(&mut worker, b"\x1b]52;c;?\x07");
    let moved = client.clipboard_read().unwrap().unwrap();
    worker.command(Command::Scroll(1)).unwrap();
    assert_eq!(client.clipboard_read().unwrap(), Some(moved));
    worker
        .answer_clipboard(moved, Some("old viewport".into()), 0)
        .unwrap();
    assert!(client.clipboard_read().unwrap().is_none());
    assert!(
        client
            .respond_clipboard(timed, Some("x".repeat(MAX_OSC52_CLIPBOARD_BYTES + 1)), 0)
            .is_err()
    );
}

#[tokio::test]
async fn writes_are_bounded_one_shot_and_old_connections_cannot_consume_them() {
    let (mut worker, client) = fixture(true);
    output(&mut worker, b"\x1b]52;p;SGVsbG8=\x07");
    assert!(client.take_remote_clipboard().unwrap().is_none());
    output(&mut worker, b"\x1b]52;c;SGVsbG8=\x07");
    let old = client.clone();
    worker.reset_model().unwrap();
    worker.publish().unwrap();
    let client = client.for_view(&client.view().unwrap());
    output(&mut worker, b"\x1b]52;c;SGVsbG8=\x07");
    assert!(old.take_remote_clipboard().unwrap().is_none());
    assert_eq!(
        client.take_remote_clipboard().unwrap().as_deref(),
        Some("Hello")
    );
    assert!(client.take_remote_clipboard().unwrap().is_none());
    use base64::Engine as _;
    for length in [MAX_OSC52_CLIPBOARD_BYTES, MAX_OSC52_CLIPBOARD_BYTES + 1] {
        let encoded = base64::engine::general_purpose::STANDARD.encode("x".repeat(length));
        output(&mut worker, format!("\x1b]52;c;{encoded}\x07").as_bytes());
        assert_eq!(
            client
                .take_remote_clipboard()
                .unwrap()
                .map(|text| text.len()),
            (length == MAX_OSC52_CLIPBOARD_BYTES).then_some(length)
        );
    }
}
