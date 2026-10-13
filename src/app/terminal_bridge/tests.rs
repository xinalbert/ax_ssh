use super::*;
#[cfg(not(windows))]
use ax_ssh::local_shell::{LocalShellEvent, LocalShellHandle};

fn wheel_event(button: TerminalMouseButton) -> TerminalMouseEvent {
    TerminalMouseEvent {
        kind: TerminalMouseEventKind::Press,
        button,
        column: 0,
        row: 0,
        modifiers: TerminalMouseModifiers::default(),
    }
}

#[test]
fn alternate_scroll_to_home_appends_home_after_each_bounded_wheel_step() {
    let mut model = TerminalModel::new(80, 24, 10);
    model.process(b"\x1b[?1049h");
    for (mode, up, down, home) in [
        (b"\x1b[?1l", b"\x1b[A", b"\x1b[B", b"\x1b[H"),
        (b"\x1b[?1h", b"\x1bOA", b"\x1bOB", b"\x1bOH"),
    ] {
        model.process(mode);
        for (button, direction) in [
            (TerminalMouseButton::WheelUp, up),
            (TerminalMouseButton::WheelDown, down),
        ] {
            let event = wheel_event(button);
            for (count, expected_count) in [(0, 1), (1, 1), (3, 3), (i32::MAX, 256)] {
                let arrows = direction.repeat(expected_count);
                assert_eq!(
                    encode_terminal_mouse_input(&model, event, 1, 1, count, false),
                    Some(arrows.clone())
                );
                let mut expected = Vec::new();
                for _ in 0..expected_count {
                    expected.extend(direction);
                    expected.extend(home);
                }
                assert_eq!(
                    encode_terminal_mouse_input(&model, event, 1, 1, count, true),
                    Some(expected)
                );
            }
        }
    }
}

#[test]
fn alternate_scroll_to_home_preserves_mouse_reporting_and_other_events() {
    let mut model = TerminalModel::new(80, 24, 10);
    model.process(b"\x1b[?1049h\x1b[?1h\x1b[?1000h\x1b[?1006h");
    let event = wheel_event(TerminalMouseButton::WheelUp);
    assert_eq!(
        encode_terminal_mouse_input(&model, event, 1, 1, 3, true),
        Some(b"\x1b[<64;1;1M".repeat(3))
    );
    for button in [
        TerminalMouseButton::WheelDown,
        TerminalMouseButton::WheelLeft,
        TerminalMouseButton::WheelRight,
        TerminalMouseButton::Left,
    ] {
        let event = wheel_event(button);
        assert_eq!(
            encode_terminal_mouse_input(&model, event, 1, 1, 1, true),
            model.encode_mouse_event(event)
        );
    }
    assert_eq!(
        encode_terminal_mouse_input(
            &model,
            wheel_event(TerminalMouseButton::Left),
            1,
            1,
            2,
            true
        ),
        None
    );
    model.process(b"\x1b[?1000l");
    for button in [TerminalMouseButton::WheelLeft, TerminalMouseButton::Left] {
        assert_eq!(
            encode_terminal_mouse_input(&model, wheel_event(button), 1, 1, 1, true),
            None
        );
    }
    model.process(b"\x1b[?1007l");
    assert_eq!(
        encode_terminal_mouse_input(&model, event, 1, 1, 1, true),
        None
    );
    model.process(b"\x1b[?1007h\x1b[?1049l");
    assert_eq!(
        encode_terminal_mouse_input(&model, event, 1, 1, 1, true),
        None
    );
}

#[test]
fn terminal_target_uses_slint_primary_shortcut_modifier() {
    assert!(terminal_target_modifier_held(true, false));
    assert!(!terminal_target_modifier_held(false, true));
}

#[test]
fn semantic_selection_prefers_complete_url_and_omits_trailing_punctuation() {
    let mut terminal = TerminalModel::new(80, 3, 10);
    terminal.process(b"See https://example.test/releases/v1.2).\r\n");

    let range = terminal_url_selection_range(&terminal, 0, 12)
        .expect("URL should provide a bounded selection range");
    assert_eq!(
        terminal.selection_text(
            range.start_row,
            range.start_column,
            range.end_row,
            range.end_column,
        ),
        "https://example.test/releases/v1.2"
    );
}

#[test]
fn semantic_selection_keeps_soft_wrapped_url_in_one_range() {
    let mut terminal = TerminalModel::new(24, 3, 10);
    terminal.process(b"https://example.test/very-long/path?q=1");

    let range = terminal_url_selection_range(&terminal, 1, 2)
        .expect("soft-wrapped URL should provide a bounded selection range");
    assert_eq!(
        terminal.selection_text(
            range.start_row,
            range.start_column,
            range.end_row,
            range.end_column,
        ),
        "https://example.test/very-long/path?q=1"
    );
}

#[test]
fn url_selection_excludes_user_reported_chinese_punctuation_and_http_suffix() {
    let url = "https://www.rfc-editor.org/rfc/rfc9110.html#section-15";
    for columns in [40, 60, 80, 100, 120] {
        let mut terminal = TerminalModel::new(columns, 8, 10);
        terminal.process(
            format!("规则依据 HTTP 状态类别（{url})。HTTP 状态码必须有明确上下文").as_bytes(),
        );
        let mut matched_cells = 0;
        for row in 0..8 {
            for column in 0..columns {
                let Some(range) = terminal_url_selection_range(&terminal, row, column) else {
                    continue;
                };
                assert_eq!(
                    terminal.selection_text(
                        range.start_row,
                        range.start_column,
                        range.end_row,
                        range.end_column,
                    ),
                    url,
                    "columns={columns}, row={row}, column={column}"
                );
                matched_cells += 1;
            }
        }
        assert_eq!(matched_cells, url.len(), "columns={columns}");
    }
}

#[test]
fn url_selection_maps_unicode_characters_back_to_terminal_cells() {
    let url = "HTTPS://例子.测试/中文路径?q=测试#片段";
    let mut terminal = TerminalModel::new(28, 8, 10);
    terminal.process(format!("链接：{url}。后文").as_bytes());
    let range = terminal_url_selection_range(&terminal, 0, 8).expect("Unicode URL selection");
    assert_eq!(
        terminal.selection_text(
            range.start_row,
            range.start_column,
            range.end_row,
            range.end_column,
        ),
        url
    );
}

#[test]
fn terminal_geometry_quantization_is_stable_for_diagnostics() {
    assert_eq!(quantize_logical(12.34), 123);
    assert_eq!(quantize_logical(12.36), 124);
    assert_eq!(quantize_logical(f32::NAN), i32::MIN);
    assert_eq!(quantize_scale(2.0), 2_000);
    assert_eq!(quantize_scale(f64::NAN), 0);
}

#[test]
fn consecutive_native_file_events_share_one_upload_batch() {
    let mut pointer = NativeFileDropPointer::default();
    let first = pointer.upload_batch_id();
    pointer.complete_external_file_drop();
    assert_eq!(pointer.upload_batch_id(), first);
    pointer.clear();
    assert_ne!(pointer.upload_batch_id(), first);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn native_file_drop_requires_current_external_hover_and_coordinates() {
    let mut pointer = NativeFileDropPointer::default();
    pointer.record_cursor_position(80.0, 48.0);
    assert_eq!(pointer.logical_position(2.0), None);

    pointer.begin_external_file_hover();
    assert_eq!(pointer.logical_position(2.0), None);
    pointer.record_cursor_position(80.0, 48.0);
    assert_eq!(pointer.logical_position(2.0), Some((40.0, 24.0)));
    assert_eq!(pointer.logical_position(0.0), None);

    pointer.begin_external_file_hover();
    pointer.complete_external_file_drop();
    assert_eq!(pointer.logical_position(2.0), Some((40.0, 24.0)));
    pointer.complete_external_file_drop();
    assert_eq!(pointer.logical_position(2.0), None);

    pointer.begin_external_file_hover();
    pointer.clear();
    assert_eq!(pointer.logical_position(2.0), None);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn native_file_drop_rejects_non_finite_pointer_coordinates() {
    let mut pointer = NativeFileDropPointer::default();
    pointer.begin_external_file_hover();
    pointer.record_cursor_position(f64::NAN, 12.0);
    assert_eq!(pointer.logical_position(1.0), None);
}

#[cfg(target_os = "macos")]
#[test]
fn macos_file_drop_uses_appkit_position_without_hover_state() {
    assert_eq!(macos_file_drop_position(|| None), None);
    assert_eq!(
        macos_file_drop_position(|| Some((30.0, 20.0))),
        Some((30.0, 20.0))
    );
}

#[test]
fn terminal_output_preserves_local_selection_revision() {
    let mut app = AppState::new(
        ConfigStore::new(
            std::env::temp_dir().join(format!("axssh-output-revision-{}.json", Uuid::new_v4())),
        ),
        SessionStore::default(),
    );
    let tab_id = app.open_local_shell_tab();
    let terminal = app
        .terminal_mut(tab_id)
        .expect("local terminal should exist");
    let before = terminal.selection_revision;

    process_terminal_output(terminal, b"output").expect("terminal output should be processed");

    assert_eq!(terminal.selection_revision, before);

    process_terminal_output(terminal, b"").expect("empty output should be accepted");

    assert_eq!(terminal.selection_revision, before);
}

#[cfg(not(windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn finishing_local_terminal_returns_its_worker_for_explicit_shutdown() {
    let state = Arc::new(Mutex::new(AppState::new(
        ConfigStore::new(
            std::env::temp_dir().join(format!("axssh-local-finish-{}.json", Uuid::new_v4())),
        ),
        SessionStore::default(),
    )));
    let (tab_id, mut events) = {
        let mut app = state.lock().expect("state should lock");
        let tab_id = app.open_local_shell_tab();
        let (worker, events) =
            LocalShellHandle::spawn(ax_ssh::local_shell::SYSTEM_SHELL.into(), 80, 24);
        app.terminal_mut(tab_id)
            .expect("local terminal should exist")
            .worker = Some(TerminalWorker::Local(worker));
        (tab_id, events)
    };

    let started = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("local shell should report startup");
    assert!(matches!(started, Some(LocalShellEvent::Started { .. })));

    let finished = finish_local_terminal(&state, tab_id, "finished")
        .expect("local terminal should transition to finished");
    let worker = finished
        .worker
        .expect("finished transition must preserve the worker owner");
    {
        let app = state.lock().expect("state should lock");
        let terminal = app.terminal(tab_id).expect("local terminal should remain");
        assert!(terminal.worker.is_none());
        assert!(!terminal.connected);
        assert!(!terminal.worker_running);
        assert_eq!(terminal.status, "finished");
    }
    tokio::time::timeout(Duration::from_secs(5), worker.shutdown())
        .await
        .expect("worker shutdown must remain bounded")
        .expect("finished local worker should shut down cleanly");
}

#[cfg(not(windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_shell_exit_closes_its_workspace_tab_and_selects_following_tab() {
    let state = Arc::new(Mutex::new(AppState::new(
        ConfigStore::new(
            std::env::temp_dir().join(format!("axssh-local-exit-{}.json", Uuid::new_v4())),
        ),
        SessionStore::default(),
    )));
    let (exited_tab_id, following_tab_id, events) = {
        let mut app = state.lock().expect("state should lock");
        let _preceding_tab_id = app.open_local_shell_tab();
        let exited_tab_id = app.open_local_shell_tab();
        let following_tab_id = app.open_local_shell_tab();
        assert!(app.activate_tab(exited_tab_id));

        let (worker, events) =
            LocalShellHandle::spawn(ax_ssh::local_shell::SYSTEM_SHELL.into(), 80, 24);
        worker
            .request_send(b"exit\n".to_vec())
            .expect("exit command should queue");
        app.terminal_mut(exited_tab_id)
            .expect("local terminal should exist")
            .worker = Some(TerminalWorker::Local(worker));
        (exited_tab_id, following_tab_id, events)
    };

    spawn_local_shell_monitor(
        &Handle::current(),
        state.clone(),
        slint::Weak::<AppWindow>::default(),
        exited_tab_id,
        events,
    );

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let closed_and_focused = state.lock().is_ok_and(|app| {
                app.terminal(exited_tab_id).is_none()
                    && app.active_tab_id() == Some(following_tab_id)
            });
            if closed_and_focused {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("local exit should close its tab and focus the following tab");
}
