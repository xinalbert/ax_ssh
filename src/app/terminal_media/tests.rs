use super::*;
use tokio::time::timeout;

#[test]
fn multiline_dictation_does_not_submit_in_a_terminal_without_bracketed_paste() {
    let terminal = ax_ssh::terminal::TerminalModel::new(80, 3, 10);
    let text = single_line_transcript("请分析项目\r\n并解释结果\n保留配置\r继续");
    let encoded = terminal.encode_paste(&text).unwrap();
    assert_eq!(
        String::from_utf8(encoded.clone()).unwrap(),
        "请分析项目 并解释结果 保留配置 继续"
    );
    assert!(!encoded.contains(&b'\r'));
    assert!(!encoded.contains(&b'\n'));
}

#[test]
fn attachment_paths_cannot_inject_shell_commands() {
    assert_eq!(attachment_text("/tmp/a.png"), "/tmp/a.png");
    assert_eq!(attachment_text("/tmp/a b's.png"), "'/tmp/a b'\\''s.png'");
    assert_eq!(
        attachment_text("/tmp/$(whoami).png"),
        "'/tmp/$(whoami).png'"
    );
}
#[test]
fn dropping_media_job_revokes_capture() {
    let (stop, stopped) = watch::channel(false);
    drop(MediaJob {
        id: Uuid::new_v4(),
        mode: "microphone",
        stop,
        task: None,
    });
    assert!(*stopped.borrow());
}

struct Fixture {
    state: Arc<Mutex<AppState>>,
    tab: Uuid,
    router: WindowRouter,
}

impl Fixture {
    fn new() -> Self {
        let mut app = AppState::new(
            ConfigStore::new(
                std::env::temp_dir().join(format!("axssh-media-fixture-{}.json", Uuid::new_v4())),
            ),
            SessionStore::default(),
        );
        let tab = app.open_local_shell_tab();
        let router = WindowRouter::new(slint::Weak::default());
        assert!(router.activate_tab(MAIN_WINDOW_ID, tab, &mut app));
        Self {
            state: Arc::new(Mutex::new(app)),
            tab,
            router,
        }
    }

    fn command(&self, command: &str) -> bool {
        handle_command(
            self.tab,
            command,
            &self.state,
            &slint::Weak::default(),
            &Handle::current(),
            &self.router,
            MAIN_WINDOW_ID,
        )
    }
}

#[tokio::test]
async fn media_commands_reject_foreign_disconnected_and_workerless_targets() {
    let fixture = Fixture::new();
    assert!(!handle_command(
        fixture.tab,
        "discard-transcript",
        &fixture.state,
        &slint::Weak::default(),
        &Handle::current(),
        &fixture.router,
        Uuid::new_v4()
    ));
    for command in ["microphone", "insert-transcript\ntext"] {
        assert!(!fixture.command(command));
    }
    assert!(
        paste_text(
            &fixture.state,
            fixture.tab,
            &"x".repeat(4 * 1024 * 1024 + 1)
        )
        .is_err()
    );
    assert!(paste_text(&fixture.state, Uuid::new_v4(), "text").is_err());
    {
        let mut app = fixture.state.lock().unwrap();
        let terminal = app.terminal_mut(fixture.tab).unwrap();
        terminal.connected = true;
        terminal.media.transcript = "kept on failed insertion".into();
    }
    assert!(!fixture.command("microphone"));
    assert!(paste_text(&fixture.state, fixture.tab, "").is_err());
    assert!(!fixture.command("insert-transcript\ntext"));
    assert_eq!(
        fixture
            .state
            .lock()
            .unwrap()
            .terminal(fixture.tab)
            .unwrap()
            .media
            .transcript,
        "kept on failed insertion"
    );
    assert!(fixture.command("discard-transcript"));
    assert!(
        fixture
            .state
            .lock()
            .unwrap()
            .terminal(fixture.tab)
            .unwrap()
            .media
            .transcript
            .is_empty()
    );
}

#[tokio::test]
async fn media_stop_and_cancel_revoke_jobs_and_ignore_stale_status_updates() {
    let fixture = Fixture::new();
    let id = Uuid::new_v4();
    let (stop, stopped) = watch::channel(false);
    let task = tokio::spawn(std::future::pending());
    let abort = task.abort_handle();
    {
        let mut app = fixture.state.lock().unwrap();
        let media = &mut app.terminal_mut(fixture.tab).unwrap().media;
        media.job = Some(MediaJob {
            id,
            mode: "microphone",
            stop,
            task: Some(task),
        });
        media.image_token = Some(Uuid::new_v4());
        media.transcript = "discard on disconnect".into();
    }
    audio_status(
        &fixture.state,
        &slint::Weak::default(),
        fixture.tab,
        Uuid::new_v4(),
        "stale",
    );
    assert!(
        fixture
            .state
            .lock()
            .unwrap()
            .terminal(fixture.tab)
            .unwrap()
            .media
            .status
            .is_empty()
    );
    audio_status(
        &fixture.state,
        &slint::Weak::default(),
        fixture.tab,
        id,
        "live",
    );
    assert_eq!(
        fixture
            .state
            .lock()
            .unwrap()
            .terminal(fixture.tab)
            .unwrap()
            .media
            .snapshot()
            .mode,
        "microphone"
    );
    assert!(fixture.command("microphone"));
    assert!(*stopped.borrow());
    assert_eq!(
        fixture
            .state
            .lock()
            .unwrap()
            .terminal(fixture.tab)
            .unwrap()
            .media
            .status,
        "Stopping microphone…"
    );
    fixture
        .state
        .lock()
        .unwrap()
        .terminal_mut(fixture.tab)
        .unwrap()
        .media
        .cancel();
    tokio::task::yield_now().await;
    assert!(abort.is_finished());
    let mut app = fixture.state.lock().unwrap();
    let media = &mut app.terminal_mut(fixture.tab).unwrap().media;
    assert!(
        !media.has_job()
            && media.image_token.is_none()
            && media.transcript.is_empty()
            && media.status.is_empty()
    );
    let (stop, stopped) = watch::channel(false);
    media.job = Some(MediaJob {
        id,
        mode: "transcribing",
        stop,
        task: None,
    });
    drop(app);
    assert!(fixture.command("dictation"));
    assert!(*stopped.borrow());
    assert_eq!(
        fixture
            .state
            .lock()
            .unwrap()
            .terminal(fixture.tab)
            .unwrap()
            .media
            .status,
        "Transcription cancelled"
    );
}

#[tokio::test]
async fn clipboard_text_and_transcript_use_real_transport_without_submitting_multiline_commands() {
    use tokio::io::AsyncReadExt;
    let fixture = Fixture::new();
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let mut profile = SessionProfile::new_telnet("Media", "127.0.0.1");
    let ConnectionProfile::Telnet(config) = &mut profile.connection else {
        panic!("Telnet config")
    };
    config.port = listener.local_addr().unwrap().port();
    let (worker, _events) =
        TelnetSessionHandle::spawn(&Handle::current(), fixture.tab, config.clone(), 80, 24);
    let (mut stream, _) = listener.accept().await.unwrap();
    stream.read_exact(&mut [0; 9]).await.unwrap();
    {
        let mut app = fixture.state.lock().unwrap();
        let terminal = app.terminal_mut(fixture.tab).unwrap();
        terminal.worker = Some(TerminalWorker::Telnet(worker));
        terminal.connected = true;
        terminal.media.transcript = "review".into();
    }
    assert!(fixture.command("insert-transcript\nfirst\r\nsecond\nthird"));
    assert!(
        fixture
            .state
            .lock()
            .unwrap()
            .terminal(fixture.tab)
            .unwrap()
            .media
            .transcript
            .is_empty()
    );
    assert!(paste_clipboard_with(
        fixture.tab,
        &fixture.state,
        &slint::Weak::default(),
        &Handle::current(),
        || Ok(ClipboardContent::Text(" paste".into()))
    ));
    assert!(!paste_clipboard_with(
        fixture.tab,
        &fixture.state,
        &slint::Weak::default(),
        &Handle::current(),
        || Err(anyhow::anyhow!("clipboard unavailable"))
    ));
    let expected = b"first second third paste";
    let mut bytes = vec![0; expected.len()];
    timeout(Duration::from_secs(3), stream.read_exact(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bytes, expected);
    // Telnet cannot accept an image; do not attempt filesystem attachment creation.
    assert!(!paste_clipboard_with(
        fixture.tab,
        &fixture.state,
        &slint::Weak::default(),
        &Handle::current(),
        || Ok(ClipboardContent::Image(pixel()))
    ));
    let worker = fixture
        .state
        .lock()
        .unwrap()
        .terminal_mut(fixture.tab)
        .unwrap()
        .worker
        .take()
        .unwrap();
    timeout(Duration::from_secs(3), worker.shutdown())
        .await
        .unwrap()
        .unwrap();
}

fn pixel() -> arboard::ImageData<'static> {
    arboard::ImageData {
        width: 1,
        height: 1,
        bytes: std::borrow::Cow::Owned(vec![255; 4]),
    }
}

#[tokio::test]
async fn image_preparation_rejects_invalid_pixels_closed_tabs_and_cancelled_identity() {
    let fixture = Fixture::new();
    let token = Uuid::new_v4();
    let invalid = arboard::ImageData {
        width: 1,
        height: 1,
        bytes: std::borrow::Cow::Owned(Vec::new()),
    };
    assert!(
        prepare_image(&fixture.state, fixture.tab, 0, token, invalid)
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("pixels")
    );
    assert!(
        prepare_image(&fixture.state, Uuid::new_v4(), 0, token, pixel())
            .await
            .is_err()
    );
    assert!(
        prepare_image(&fixture.state, fixture.tab, 0, token, pixel())
            .await
            .is_err()
    );
    {
        let mut app = fixture.state.lock().unwrap();
        let terminal = app.terminal_mut(fixture.tab).unwrap();
        terminal.media.image_token = Some(token);
        terminal.connected = true;
    }
    assert!(
        prepare_image(&fixture.state, fixture.tab, 1, token, pixel())
            .await
            .is_err()
    );
    assert!(
        prepare_image(&fixture.state, fixture.tab, 0, Uuid::new_v4(), pixel())
            .await
            .is_err()
    );
    assert!(
        prepare_image(&fixture.state, fixture.tab, 0, token, pixel())
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("worker stopped")
    );
}

#[cfg(not(windows))]
#[tokio::test]
async fn local_image_files_are_private_and_deleted_by_the_background_owner_on_cancellation() {
    let fixture = Fixture::new();
    let token = Uuid::new_v4();
    let (worker, _events) =
        LocalShellHandle::spawn(ax_ssh::local_shell::SYSTEM_SHELL.into(), 80, 24);
    {
        let mut app = fixture.state.lock().unwrap();
        let terminal = app.terminal_mut(fixture.tab).unwrap();
        terminal.worker = Some(TerminalWorker::Local(worker));
        terminal.connected = true;
        terminal.media.image_token = Some(token);
    }
    let (path, owner) = prepare_image(&fixture.state, fixture.tab, 0, token, pixel())
        .await
        .unwrap();
    let owner = owner.unwrap();
    assert!(
        std::fs::read(&path)
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    {
        let mut app = fixture.state.lock().unwrap();
        let media = &mut app.terminal_mut(fixture.tab).unwrap().media;
        media.files.push(BackgroundFile {
            file: Some(owner),
            runtime: Handle::current(),
        });
        media.cancel();
    }
    timeout(Duration::from_secs(3), async {
        while PathBuf::from(&path).exists() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let worker = fixture
        .state
        .lock()
        .unwrap()
        .terminal_mut(fixture.tab)
        .unwrap()
        .worker
        .take()
        .unwrap();
    timeout(Duration::from_secs(8), worker.shutdown())
        .await
        .unwrap()
        .unwrap();
}
