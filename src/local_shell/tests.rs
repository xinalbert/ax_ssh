use std::sync::atomic::AtomicUsize;

use super::*;

struct CountingMasterPty {
    resize_calls: AtomicUsize,
}

impl portable_pty::MasterPty for CountingMasterPty {
    fn resize(&self, _size: PtySize) -> Result<()> {
        self.resize_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn get_size(&self) -> Result<PtySize> {
        anyhow::bail!("resize deduplication must not query the platform PTY")
    }

    fn try_clone_reader(&self) -> Result<Box<dyn Read + Send>> {
        Ok(Box::new(std::io::empty()))
    }

    fn take_writer(&self) -> Result<Box<dyn Write + Send>> {
        Ok(Box::new(std::io::sink()))
    }

    #[cfg(unix)]
    fn process_group_leader(&self) -> Option<i32> {
        None
    }

    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<portable_pty::unix::RawFd> {
        None
    }

    #[cfg(unix)]
    fn tty_name(&self) -> Option<PathBuf> {
        None
    }
}

#[test]
fn system_shell_and_explicit_shell_resolve() {
    assert!(resolve_shell(SYSTEM_SHELL).is_ok());
    let shells = discover_shells();
    assert!(!shells.is_empty());
    assert!(shells.iter().all(|shell| resolve_shell(shell).is_ok()));
}

#[cfg(unix)]
#[test]
fn local_pty_output_modes_translate_linefeeds_before_shell_start() {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 3,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("test PTY should open");
    let fd = pair
        .master
        .as_raw_fd()
        .expect("native test PTY should expose a terminal descriptor");
    let mut termios = local_pty_termios(fd).expect("test PTY termios should be readable");
    termios.c_oflag &= !(libc::OPOST | libc::ONLCR);
    set_local_pty_termios(fd, &termios).expect("test PTY output modes should be configurable");

    configure_local_pty_output_modes(pair.master.as_ref())
        .expect("local PTY output modes should be enabled");
    let termios = local_pty_termios(fd).expect("configured PTY termios should be readable");
    assert_ne!(termios.c_oflag & libc::OPOST, 0);
    assert_ne!(termios.c_oflag & libc::ONLCR, 0);

    let mut command = CommandBuilder::new("/bin/sh");
    command.arg("-c");
    command.arg("printf 'first\\nsecond\\n'");
    let mut child = pair
        .slave
        .spawn_command(command)
        .expect("test shell should start in PTY");
    drop(pair.slave);

    set_local_pty_nonblocking(fd).expect("test PTY should support nonblocking reads");
    let mut reader = pair
        .master
        .try_clone_reader()
        .expect("test PTY output should be readable");
    let expected = b"first\r\nsecond\r\n";
    let output = match read_local_pty_output_with_timeout(reader.as_mut(), expected.len()) {
        Ok(output) => output,
        Err(error) => {
            let _ = stop_local_pty_test_child(child.as_mut());
            panic!("test PTY output should arrive before the deadline: {error:#}");
        }
    };
    if let Err(error) = wait_for_local_pty_test_child_exit(child.as_mut()) {
        let _ = stop_local_pty_test_child(child.as_mut());
        panic!("test shell should exit before the deadline: {error:#}");
    }
    assert_eq!(output, expected);
}

#[test]
fn invalid_shell_is_rejected_without_fallback() {
    assert!(resolve_shell("axssh-shell-that-does-not-exist").is_err());
    assert!(resolve_shell("bad\nshell").is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn zsh_shell_detection_uses_the_executable_name() {
    assert!(is_zsh_shell(Path::new("/bin/zsh")));
    assert!(is_zsh_shell(Path::new("ZSH")));
    assert!(!is_zsh_shell(Path::new("/bin/bash")));
    assert!(!is_zsh_shell(Path::new("/bin/zsh-wrapper")));
}

#[cfg(target_os = "macos")]
#[test]
fn macos_zsh_uses_login_startup_files() {
    let mut command = CommandBuilder::new("/bin/zsh");
    configure_local_shell_command(&mut command, Path::new("/bin/zsh"));
    assert_eq!(
        command.get_argv(),
        &vec![OsString::from("/bin/zsh"), OsString::from("-l")]
    );
}

#[cfg(unix)]
#[test]
fn local_shell_replaces_a_non_utf8_locale() {
    let mut command = CommandBuilder::new("sh");
    command.env("LANG", "");
    command.env("LC_CTYPE", "C");
    command.env("LC_ALL", "C");

    configure_local_shell_locale(&mut command);

    #[cfg(target_os = "macos")]
    let expected = MACOS_UTF8_LOCALE;
    #[cfg(not(target_os = "macos"))]
    let expected = UNIX_UTF8_LOCALE;
    let locale = |key| command.get_env(key).and_then(OsStr::to_str);
    assert_eq!(locale("LANG"), Some(expected));
    assert_eq!(locale("LC_CTYPE"), Some(expected));
    assert_eq!(locale("LC_ALL"), Some(expected));
}

#[cfg(unix)]
#[test]
fn local_shell_keeps_an_existing_utf8_locale() {
    let mut command = CommandBuilder::new("sh");
    command.env("LANG", "zh_CN.UTF-8");
    command.env("LC_CTYPE", "C");
    command.env("LC_ALL", "zh_CN.UTF-8");

    configure_local_shell_locale(&mut command);

    let locale = |key| command.get_env(key).and_then(OsStr::to_str);
    assert_eq!(locale("LANG"), Some("zh_CN.UTF-8"));
    assert_eq!(locale("LC_CTYPE"), Some("C"));
    assert_eq!(locale("LC_ALL"), Some("zh_CN.UTF-8"));
}

#[cfg(not(target_os = "macos"))]
#[test]
fn non_macos_shells_keep_native_startup_arguments() {
    let mut command = CommandBuilder::new("sh");
    configure_local_shell_command(&mut command, Path::new("sh"));
    assert_eq!(command.get_argv(), &vec![OsString::from("sh")]);
}

#[test]
fn default_shell_fallback_matches_the_platform() {
    #[cfg(windows)]
    assert_eq!(default_shell_fallback(), Some("cmd.exe"));
    #[cfg(target_os = "macos")]
    assert_eq!(default_shell_fallback(), Some("/bin/zsh"));
    #[cfg(all(not(windows), not(target_os = "macos")))]
    assert_eq!(default_shell_fallback(), Some("sh"));
}

#[test]
fn local_pty_size_includes_physical_terminal_dimensions() {
    let size = pty_size(TerminalSize::backend_with_pixels(80, 24, 720, 432));
    assert_eq!((size.cols, size.rows), (80, 24));
    assert_eq!((size.pixel_width, size.pixel_height), (720, 432));
}

#[cfg(target_os = "macos")]
#[test]
fn homebrew_path_entries_are_prepended_once() {
    let current_path =
        env::join_paths(["/usr/bin", "/opt/homebrew/bin"]).expect("test path should be valid");
    let updated = prepend_unique_path_entries(
        Some(current_path.as_os_str()),
        [
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/opt/homebrew/sbin"),
            PathBuf::from("/opt/homebrew/sbin"),
        ],
    )
    .expect("missing Homebrew path entry should be added");

    assert_eq!(
        env::split_paths(&updated).collect::<Vec<_>>(),
        vec![
            PathBuf::from("/opt/homebrew/sbin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/opt/homebrew/bin"),
        ]
    );
}

#[cfg(target_os = "macos")]
#[test]
fn no_homebrew_path_entries_leave_the_child_path_unset() {
    let current_path =
        env::join_paths(["/usr/bin", "/opt/homebrew/bin"]).expect("test path should be valid");
    assert_eq!(
        prepend_unique_path_entries(
            Some(current_path.as_os_str()),
            [
                PathBuf::from("/usr/bin"),
                PathBuf::from("/opt/homebrew/bin")
            ],
        ),
        None
    );
}

#[test]
fn repeated_local_pty_size_does_not_call_platform_resize() {
    let master = CountingMasterPty {
        resize_calls: AtomicUsize::new(0),
    };
    let initial_size = TerminalSize::backend(80, 24);
    let pending_resize = Arc::new(Mutex::new(Some(initial_size)));
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let (event_tx, mut event_rx) = mpsc::channel(1);
    let mut applied_size = initial_size;

    apply_pending_resize(
        &master,
        &pending_resize,
        &mut applied_size,
        &shutdown_requested,
        &event_tx,
    )
    .expect("same-size resize should be ignored");
    assert_eq!(master.resize_calls.load(Ordering::SeqCst), 0);
    assert!(event_rx.try_recv().is_err());

    *pending_resize.lock().expect("resize state should lock") =
        Some(TerminalSize::backend(100, 30));
    apply_pending_resize(
        &master,
        &pending_resize,
        &mut applied_size,
        &shutdown_requested,
        &event_tx,
    )
    .expect("changed size should reach the platform PTY");
    assert_eq!(master.resize_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        event_rx.try_recv(),
        Ok(LocalShellEvent::Resized {
            columns: 100,
            rows: 30,
        })
    );
}

#[test]
fn cancellation_does_not_enqueue_pending_local_pty_events() {
    let shutdown_requested = Arc::new(AtomicBool::new(true));
    let (event_tx, mut event_rx) = mpsc::channel(1);

    assert!(
        send_event_with_cancellation(
            &event_tx,
            LocalShellEvent::Output(crate::terminal::TerminalOutputChunk::new(
                b"discard on shutdown".to_vec(),
            )),
            &shutdown_requested,
        )
        .is_err()
    );
    assert!(event_rx.try_recv().is_err());
}

#[test]
fn output_reader_notifies_owner_after_eof() {
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let (event_tx, mut event_rx) = mpsc::channel(1);
    let (command_tx, command_rx) = sync_channel(1);

    read_output(
        Box::new(std::io::Cursor::new(b"done".to_vec())),
        &event_tx,
        &shutdown_requested,
        &command_tx,
    );

    let event = event_rx.try_recv().expect("output event should be queued");
    match event {
        LocalShellEvent::Output(output) => assert_eq!(output.data, b"done"),
        other => panic!("expected output event, got {other:?}"),
    }
    assert!(matches!(
        command_rx.try_recv(),
        Ok(LocalShellCommand::ReaderClosed)
    ));
}

#[cfg(not(windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_pty_accepts_input_and_streams_output() {
    let (worker, mut events) = LocalShellHandle::spawn("sh".into(), 80, 24);
    let output = timeout(Duration::from_secs(5), async {
        let mut output = Vec::new();
        let mut sent = false;
        while let Some(event) = events.recv().await {
            match event {
                LocalShellEvent::Started { .. } => {
                    worker
                        .request_send(b"printf 'AXSSH_LOCAL_PTY_OK\\n'; exit\n".to_vec())
                        .expect("local PTY should accept input");
                    sent = true;
                }
                LocalShellEvent::Output(data) => output.extend(data.data),
                LocalShellEvent::Exited { .. } => break,
                LocalShellEvent::Failed(message) => {
                    panic!("local PTY failed: {message}");
                }
                LocalShellEvent::Resized { .. } => {}
            }
        }
        assert!(sent, "local shell should report startup");
        output
    })
    .await
    .expect("local PTY should finish within the test timeout");
    worker
        .shutdown()
        .await
        .expect("finished local PTY should join cleanly");
    let output = String::from_utf8_lossy(&output);
    assert!(
        output.contains("AXSSH_LOCAL_PTY_OK"),
        "output was {output:?}"
    );
}

#[cfg(not(windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_terminates_a_running_shell_with_a_full_event_queue() {
    let (worker, mut events) = LocalShellHandle::spawn("sh".into(), 80, 24);
    let started = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("local shell should start promptly");
    assert!(matches!(started, Some(LocalShellEvent::Started { .. })));
    worker
        .request_send(b"while :; do echo AXSSH_LOCAL_PTY_BUSY; done\n".to_vec())
        .expect("local PTY should accept the output loop");
    tokio::time::sleep(Duration::from_millis(100)).await;

    timeout(
        WORKER_SHUTDOWN_TIMEOUT.saturating_add(Duration::from_secs(1)),
        worker.shutdown(),
    )
    .await
    .expect("shutdown must not leave a detached blocking join")
    .expect("running local PTY should shut down cleanly");
}

#[cfg(not(windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_handle_terminates_the_owned_shell() {
    let (worker, mut events) = LocalShellHandle::spawn("sh".into(), 80, 24);
    let started = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("local shell should start promptly");
    assert!(matches!(started, Some(LocalShellEvent::Started { .. })));

    drop(worker);

    timeout(Duration::from_secs(5), async {
        while events.recv().await.is_some() {}
    })
    .await
    .expect("dropping the owner must terminate the local PTY worker");
}
