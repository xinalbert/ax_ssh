use super::*;
use std::collections::HashSet;

#[cfg(target_os = "macos")]
#[test]
fn native_download_rejects_a_retired_attempt_and_workerless_targets_before_queueing() {
    let mut app = test_state();
    let profile = SessionProfile::new("remote", "remote.example", "alice");
    let tab_id = app.open_sftp_tab(&profile);
    let old_attempt = Uuid::new_v4();
    let current_attempt = Uuid::new_v4();
    let terminal = app.terminal_mut(tab_id).expect("SFTP tab");
    terminal.connected = true;
    terminal.set_ssh_attempt(Some(current_attempt));
    let state = Arc::new(Mutex::new(app));
    for (attempt, expected) in [
        (old_attempt, "connection changed"),
        (current_attempt, "no worker"),
    ] {
        let error = queue_native_remote_download(
            &state,
            tab_id,
            attempt,
            Uuid::new_v4(),
            "/remote/file.txt".into(),
            1,
            std::env::temp_dir().join("unused-file.txt"),
        )
        .expect_err("invalid download owner");
        assert!(error.to_string().contains(expected));
        assert!(
            state
                .lock()
                .expect("state")
                .terminal(tab_id)
                .expect("tab")
                .sftp
                .transfers
                .is_empty()
        );
    }
}

fn test_state() -> AppState {
    AppState::new(
        ConfigStore::new(std::env::temp_dir().join(format!("ax-ssh-sftp-{}.json", Uuid::new_v4()))),
        SessionStore::default(),
    )
}

#[test]
fn sftp_rename_rejects_switched_tab_even_with_same_path() {
    let router = WindowRouter::new(slint::Weak::<AppWindow>::default());
    let mut app = test_state();
    let profile = SessionProfile::new("remote", "remote.example", "alice");
    let original = app.open_sftp_tab(&profile);
    let other = app.open_sftp_tab(&profile);
    assert!(router.activate_tab(MAIN_WINDOW_ID, other, &mut app));
    let state = Arc::new(Mutex::new(app));
    let error = with_window_sftp_rename_target::<()>(
        &state,
        &router,
        MAIN_WINDOW_ID,
        &original.to_string(),
        "/file.txt",
        |_, _| panic!("a switched target must never enqueue a rename"),
    )
    .expect_err("the original modal must not target the newly active tab");
    assert!(error.to_string().contains("no longer active"));
}

#[test]
fn sftp_rename_revalidates_selection_visibility_and_directory_readiness() {
    let router = WindowRouter::new(slint::Weak::<AppWindow>::default());
    let mut app = test_state();
    let profile = SessionProfile::new("remote", "remote.example", "alice");
    let tab_id = app.open_sftp_tab(&profile);
    assert!(router.activate_tab(MAIN_WINDOW_ID, tab_id, &mut app));
    let terminal = app.terminal_mut(tab_id).expect("SFTP tab");
    terminal.connected = true;
    terminal.sftp.open = true;
    terminal.sftp.entries = vec![ax_ssh::sftp::SftpEntry {
        name: "file.txt".to_owned(),
        path: "/file.txt".to_owned(),
        is_dir: false,
        is_symlink: false,
        size: 1,
        modified: None,
    }];
    terminal.sftp.selected = HashSet::from(["/file.txt".to_owned()]);
    let state = Arc::new(Mutex::new(app));
    let rename_target = || {
        with_window_sftp_rename_target(
            &state,
            &router,
            MAIN_WINDOW_ID,
            &tab_id.to_string(),
            "/file.txt",
            |_, entry| Ok(entry.path),
        )
    };
    assert_eq!(
        rename_target().expect("original single selection"),
        "/file.txt"
    );
    for selected in [
        HashSet::new(),
        HashSet::from(["/other.txt".to_owned()]),
        HashSet::from(["/file.txt".to_owned(), "/other.txt".to_owned()]),
    ] {
        state
            .lock()
            .expect("state lock")
            .terminal_mut(tab_id)
            .expect("SFTP tab")
            .sftp
            .selected = selected;
        assert!(
            rename_target().is_err(),
            "changed selection must reject rename"
        );
    }
    {
        let mut app = state.lock().expect("state lock");
        let sftp = &mut app.terminal_mut(tab_id).expect("SFTP tab").sftp;
        sftp.selected = HashSet::from(["/file.txt".to_owned()]);
        sftp.loading = true;
    }
    assert!(
        rename_target().is_err(),
        "loading directory must reject rename"
    );
    {
        let mut app = state.lock().expect("state lock");
        let sftp = &mut app.terminal_mut(tab_id).expect("SFTP tab").sftp;
        sftp.loading = false;
        sftp.entries.clear();
    }
    assert!(
        rename_target().is_err(),
        "vanished entry must reject rename"
    );
}

#[test]
fn window_sftp_context_ignores_global_active_tab() {
    let router = WindowRouter::new(slint::Weak::<AppWindow>::default());
    let mut app = test_state();
    let profile = SessionProfile::new("remote", "remote.example", "alice");
    let routed_tab = app.open_sftp_tab(&profile);
    let global_tab = app.open_sftp_tab(&profile);
    app.terminal_mut(routed_tab)
        .expect("routed SFTP tab")
        .connected = true;
    app.terminal_mut(global_tab)
        .expect("global SFTP tab")
        .connected = true;
    assert!(router.activate_tab(MAIN_WINDOW_ID, routed_tab, &mut app));
    assert!(app.activate_tab(global_tab));

    let state = Arc::new(Mutex::new(app));
    with_window_sftp_terminal(&state, &router, MAIN_WINDOW_ID, |terminal| {
        terminal.sftp.status = "routed".to_owned();
        Ok(())
    })
    .expect("window route should resolve an SFTP tab");
    let app = state.lock().expect("state lock");
    assert_eq!(app.active_tab_id(), Some(global_tab));
    assert_eq!(
        app.terminal(routed_tab).expect("routed tab").sftp.status,
        "routed"
    );
    assert_ne!(
        app.terminal(global_tab).expect("global tab").sftp.status,
        "routed"
    );
}

#[test]
fn active_sftp_upload_target_revalidates_live_readiness() {
    let router = WindowRouter::new(slint::Weak::<AppWindow>::default());
    let mut app = test_state();
    let profile = SessionProfile::new("remote", "remote.example", "alice");
    let tab_id = app.open_sftp_tab(&profile);
    assert!(router.activate_tab(MAIN_WINDOW_ID, tab_id, &mut app));
    let state = Arc::new(Mutex::new(app));

    let error = active_sftp_upload_target(&state, &router, MAIN_WINDOW_ID)
        .expect_err("a disconnected SFTP tab must reject an upload");
    assert!(error.to_string().contains("no longer connected"));

    {
        let mut app = state.lock().expect("state lock");
        let terminal = app.terminal_mut(tab_id).expect("SFTP tab");
        terminal.connected = true;
        terminal.sftp.path = "/remote/inbox".to_owned();
    }
    let error = active_sftp_upload_target(&state, &router, MAIN_WINDOW_ID)
        .expect_err("an unopened SFTP directory must reject an upload");
    assert!(error.to_string().contains("not stable"));

    {
        let mut app = state.lock().expect("state lock");
        let sftp = &mut app.terminal_mut(tab_id).expect("SFTP tab").sftp;
        sftp.open = true;
        sftp.loading = true;
    }
    assert_eq!(
        active_sftp_upload_target(&state, &router, MAIN_WINDOW_ID)
            .expect("refreshing the same directory should preserve the upload target"),
        (tab_id, "/remote/inbox".to_owned())
    );

    {
        let mut app = state.lock().expect("state lock");
        let sftp = &mut app.terminal_mut(tab_id).expect("SFTP tab").sftp;
        sftp.loading = false;
        sftp.begin_navigation(SftpNavigation::Direct, Some("/remote/other".to_owned()))
            .expect("navigation should start");
    }
    assert!(
        active_sftp_upload_target(&state, &router, MAIN_WINDOW_ID).is_err(),
        "uploading must wait when navigation changes the destination"
    );
}

#[test]
fn dropped_local_paths_parses_multiple_lines() {
    let paths =
        parse_dropped_local_paths("file:///tmp/a.txt\n/tmp/b.txt\nfile:///tmp/c%20d.txt").unwrap();
    assert_eq!(paths.len(), 3);
    assert_eq!(paths[0], PathBuf::from("/tmp/a.txt"));
    assert_eq!(paths[1], PathBuf::from("/tmp/b.txt"));
    assert_eq!(paths[2], PathBuf::from("/tmp/c d.txt"));
}

#[test]
fn dropped_local_paths_rejects_invalid_uri_escape() {
    assert!(parse_dropped_local_paths("file:///tmp/bad%2").is_err());
}

#[test]
fn dropped_local_paths_accepts_localhost_authority() {
    let paths = parse_dropped_local_paths("file://localhost/tmp/ok.txt").unwrap();
    assert_eq!(paths, vec![PathBuf::from("/tmp/ok.txt")]);
}

#[test]
fn dropped_local_paths_deduplicates() {
    let paths = parse_dropped_local_paths("/tmp/a.txt\n/tmp/a.txt\nfile:///tmp/a.txt").unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0], PathBuf::from("/tmp/a.txt"));
}

#[test]
fn dropped_local_paths_skips_blank_lines() {
    let paths = parse_dropped_local_paths("\n\n/tmp/x.txt\n\n").unwrap();
    assert_eq!(paths, vec![PathBuf::from("/tmp/x.txt")]);
}

#[test]
fn dropped_local_paths_rejects_empty() {
    assert!(parse_dropped_local_paths("\n\n  \n").is_err());
}

#[test]
fn dropped_local_paths_rejects_remote_host_in_uri_list() {
    assert!(parse_dropped_local_paths("file:///tmp/ok.txt\nfile://evil-host/tmp/bad.txt").is_err());
}

#[test]
fn dropped_local_paths_keeps_more_than_the_old_batch_limit() {
    let text = (0..600)
        .map(|index| format!("/tmp/file-{index}.txt"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(parse_dropped_local_paths(&text).unwrap().len(), 600);
}

#[test]
fn dropped_local_paths_accepts_large_payload() {
    let text = (0..620)
        .map(|index| format!("/tmp/file-{index}-{}.txt", "x".repeat(4000)))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.len() > 2 * 1024 * 1024);
    assert_eq!(parse_dropped_local_paths(&text).unwrap().len(), 620);
}

#[test]
fn sftp_drag_payload_distinguishes_local_and_remote_paths() {
    match parse_sftp_drag_payload(&format!("{LOCAL_DRAG_PREFIX}/tmp/a.txt")).unwrap() {
        SftpDragPayload::Local(paths) => assert_eq!(paths, vec![PathBuf::from("/tmp/a.txt")]),
        SftpDragPayload::Remote(_) => panic!("local drag payload was classified as remote"),
    }
    match parse_sftp_drag_payload(&format!("{REMOTE_DRAG_PREFIX}/var/log/a.txt")).unwrap() {
        SftpDragPayload::Remote(path) => assert_eq!(path, "/var/log/a.txt"),
        SftpDragPayload::Local(_) => panic!("remote drag payload was classified as local"),
    }
}

#[test]
fn local_drag_payload_contains_every_selected_root() {
    let data = local_file_drag_data(&[
        PathBuf::from("/tmp/one.txt"),
        PathBuf::from("/tmp/folder"),
        PathBuf::from("/tmp/three.txt"),
    ]);
    let text = data.plain_text().expect("drag text should exist");
    match parse_sftp_drag_payload(text.as_str()).expect("drag payload should parse") {
        SftpDragPayload::Local(paths) => assert_eq!(
            paths,
            [
                PathBuf::from("/tmp/one.txt"),
                PathBuf::from("/tmp/folder"),
                PathBuf::from("/tmp/three.txt"),
            ]
        ),
        SftpDragPayload::Remote(_) => panic!("local roots must remain local"),
    }
}
