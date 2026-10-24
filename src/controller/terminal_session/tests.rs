use super::*;
use crate::terminal::{TerminalMouseButton, TerminalMouseEventKind, TerminalMouseModifiers};

#[cfg(unix)]
fn two_line_reader_script() -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt as _;

    let path = std::env::temp_dir().join(format!("axssh-egui-input-{}.sh", uuid::Uuid::new_v4()));
    std::fs::write(
        &path,
        "#!/bin/sh\nstty -echo\nprintf 'AX_EGUI_READY\\n'\nIFS= read -r first\nprintf 'AX_EGUI_FIRST:%s\\n' \"$first\"\nIFS= read -r second\nprintf 'AX_EGUI_SECOND:%s\\n' \"$second\"\n",
    )
    .expect("test shell script should be written");
    let mut permissions = std::fs::metadata(&path)
        .expect("test shell script metadata should be readable")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&path, permissions).expect("test shell script should be executable");
    path
}

fn terminal_text(view: &TerminalView) -> String {
    view.terminal
        .lines
        .iter()
        .flat_map(|line| &line.runs)
        .map(|run| run.text.as_str())
        .collect()
}

#[test]
fn facade_reports_overload_and_rejects_oversize_paste() {
    let (commands, _receiver) = mpsc::channel(1);
    let shared = Arc::new(Mutex::new(Shared {
        view: TerminalView {
            terminal: TerminalModel::new(80, 24, 10).snapshot(),
            status: String::new(),
            connected: false,
            selection_revision: 0,
            generation: 0,
            identity: ConnectionIdentity::Local,
            prompt: None,
        },
        clipboard: None,
        selection: None,
    }));
    let client = TerminalClient {
        commands,
        shared,
        generation: None,
    };
    client.scroll(1).unwrap();
    assert!(client.scroll(2).is_err());
    assert!(
        client
            .paste("x".repeat(MAX_COMMAND_TEXT_BYTES + 1))
            .is_err()
    );
}

#[tokio::test]
async fn controller_reports_start_failure_and_joins_its_worker() {
    let terminal = TerminalSession::spawn(
        &Handle::current(),
        "axssh-no-such-test-shell".to_owned(),
        Arc::new(|| {}),
    );
    let client = terminal.client();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let view = client.view().unwrap();
            if !view.status.starts_with("Starting") {
                assert!(!view.connected);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    terminal.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn real_local_pty_preserves_bytes_and_resizes_before_shutdown() {
    let terminal =
        TerminalSession::spawn(&Handle::current(), "/bin/sh".to_owned(), Arc::new(|| {}));
    let client = terminal.client();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !client.view().unwrap().connected {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    client
        .resize(TerminalSize::model_with_pixels(90, 30, 900, 600), 10, 20)
        .unwrap();
    client
        .paste("printf 'AX_EGUI_%s\\n' OK\n".to_owned())
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let view = client.view().unwrap();
            let text = view
                .terminal
                .lines
                .iter()
                .flat_map(|line| &line.runs)
                .map(|run| run.text.as_str())
                .collect::<String>();
            if text.contains("AX_EGUI_OK") {
                assert_eq!(view.terminal.max_columns, 90);
                assert_eq!(view.terminal.lines.len(), 30);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    terminal.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn one_return_stays_one_pty_line_submission() {
    let script = two_line_reader_script();
    let terminal = TerminalSession::spawn(
        &Handle::current(),
        script.to_string_lossy().into_owned(),
        Arc::new(|| {}),
    );
    let client = terminal.client();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !client.view().unwrap().connected {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if terminal_text(&client.view().unwrap()).contains("AX_EGUI_READY") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    client
        .key(
            TerminalKey::Text("once".to_owned()),
            TerminalModifiers::default(),
        )
        .unwrap();
    client
        .key(TerminalKey::Return, TerminalModifiers::default())
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let view = client.view().unwrap();
            let text = terminal_text(&view);
            if text.contains("AX_EGUI_FIRST:once") {
                assert!(!text.contains("AX_EGUI_SECOND"));
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    client
        .key(TerminalKey::Return, TerminalModifiers::default())
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let view = client.view().unwrap();
            let text = terminal_text(&view);
            if text.contains("AX_EGUI_SECOND:") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    terminal.shutdown().await.unwrap();
    std::fs::remove_file(script).expect("test shell script should be removed");
}

#[test]
fn skipped_snapshots_keep_all_changed_rows_by_revision() {
    let mut model = TerminalModel::new(80, 24, 10);
    let first = model.snapshot();
    model.process(b"\x1b[1;1Hfirst");
    let intermediate = model.snapshot();
    model.process(b"\x1b[2;1Hsecond");
    let latest = model.snapshot();
    assert_ne!(first.lines[0].revision, latest.lines[0].revision);
    assert_ne!(first.lines[1].revision, latest.lines[1].revision);
    assert_eq!(latest.lines[0].revision, intermediate.lines[0].revision);
    assert!(!latest.dirty_rows.contains(&0));
}

#[test]
fn sgr_pixel_mouse_reports_keep_physical_and_cell_coordinates_separate() {
    let mut model = TerminalModel::new(80, 24, 10);
    model.set_window_size(80, 24, 10, 20);
    model.process(b"\x1b[?1000;1006;1016h");
    let event = TerminalMouseEvent {
        kind: TerminalMouseEventKind::Press,
        button: TerminalMouseButton::Left,
        column: 4,
        row: 3,
        modifiers: TerminalMouseModifiers::default(),
    };
    assert_eq!(
        mouse_bytes(&model, event, 31, 43, 1).unwrap(),
        Some(b"\x1b[<0;31;43M".to_vec())
    );
    assert_eq!(mouse_bytes(&model, event, 31, 43, 2).unwrap(), None);
}

#[test]
fn wheel_batches_use_live_model_modes_for_cursor_and_horizontal_reports() {
    let mut model = TerminalModel::new(80, 24, 10);
    let mut event = TerminalMouseEvent {
        kind: TerminalMouseEventKind::Press,
        button: TerminalMouseButton::WheelUp,
        row: 3,
        column: 4,
        modifiers: TerminalMouseModifiers::default(),
    };
    assert_eq!(mouse_bytes(&model, event, 1, 1, 100).unwrap(), None);
    model.process(b"\x1b[?1049;1007h");
    assert_eq!(
        mouse_bytes(&model, event, 1, 1, 100).unwrap(),
        Some(b"\x1b[A".repeat(100))
    );
    model.process(b"\x1b[?1h");
    assert_eq!(
        mouse_bytes(&model, event, 1, 1, 100).unwrap(),
        Some(b"\x1bOA".repeat(100))
    );
    event.button = TerminalMouseButton::WheelLeft;
    assert_eq!(mouse_bytes(&model, event, 1, 1, 2).unwrap(), None);
    model.process(b"\x1b[?1000;1006h");
    assert_eq!(
        mouse_bytes(&model, event, 1, 1, 2).unwrap(),
        Some(b"\x1b[<66;5;4M".repeat(2))
    );
}

#[cfg(unix)]
#[tokio::test]
async fn real_pty_resize_rejects_stale_copy_and_selection_requests() {
    let script = two_line_reader_script();
    let terminal = TerminalSession::spawn(
        &Handle::current(),
        script.to_string_lossy().into_owned(),
        Arc::new(|| {}),
    );
    let client = terminal.client();
    let before = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let view = client.view().unwrap();
            if view.connected && terminal_text(&view).contains("AX_EGUI_READY") {
                break view;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let range = TerminalSelectionRange {
        start_row: 0,
        start_column: 0,
        end_row: 0,
        end_column: 5,
    };
    client.copy(range, before.selection_revision).unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(text) = client.take_clipboard().unwrap() {
                assert!(!text.is_empty());
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    client.resize(TerminalSize::model(90, 30), 10, 20).unwrap();
    client.copy(range, before.selection_revision).unwrap();
    client
        .select(SelectionKind::Line, 0, 0, 1, before.selection_revision)
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(selection) = client
                .take_selection_matching(1, before.selection_revision)
                .unwrap()
            {
                assert!(selection.range.is_none());
                assert!(client.take_clipboard().unwrap().is_none());
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let view = client.view().unwrap();
            if view.selection_revision != before.selection_revision {
                assert_eq!(
                    (view.terminal.max_columns, view.terminal.lines.len()),
                    (90, 30)
                );
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    terminal.shutdown().await.unwrap();
    std::fs::remove_file(script).unwrap();
}

#[tokio::test]
async fn rejected_transport_resize_can_be_retried_without_changing_the_viewport() {
    let (pty, mut events) = LocalShellHandle::spawn("axssh-no-such-shell".to_owned(), 80, 24);
    tokio::time::timeout(Duration::from_secs(10), async {
        while events.recv().await.is_some() {}
    })
    .await
    .unwrap();
    let mut model = TerminalModel::new(80, 24, 100);
    let shared = Arc::new(Mutex::new(Shared {
        view: TerminalView {
            terminal: model.snapshot(),
            status: String::new(),
            connected: false,
            selection_revision: 0,
            generation: 0,
            identity: ConnectionIdentity::Local,
            prompt: None,
        },
        clipboard: None,
        selection: None,
    }));
    let mut worker = Worker {
        model,
        transport: Some(Transport::Local(pty)),
        events: Events::Idle,
        reconnect: super::super::ReconnectState::new(false),
        retry_deadline: None,
        profile: None,
        pending_secret: None,
        prompt: None,
        generation: 0,
        identity: ConnectionIdentity::Local,
        shared,
        wake: Arc::new(|| {}),
        status: String::new(),
        connected: false,
        last_resize: None,
        viewport: TerminalViewport::default(),
    };
    for _ in 0..2 {
        assert!(
            worker
                .command(Command::Resize(TerminalSize::model(90, 30), 10, 20))
                .is_err()
        );
        assert_eq!(worker.model.size(), TerminalSize::model(80, 24));
        assert_eq!(worker.viewport.selection_revision(), 0);
    }
    worker.stop_transport().await.unwrap();
}
