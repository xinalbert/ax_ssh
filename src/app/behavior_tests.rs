//! Drive generated callbacks into the production controller on an isolated UI thread.

use super::*;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{EventLoopProxy, Platform, WindowAdapter};

type UiEvent = Box<dyn FnOnce() + Send>;

fn isolated(test_name: &str, operation: impl FnOnce() + Send + 'static) {
    let full_name = format!("app::behavior_tests::{test_name}");
    if std::env::var("AXSSH_CALLBACK_TEST_CHILD").ok().as_deref() == Some(test_name) {
        std::thread::spawn(operation)
            .join()
            .expect("isolated UI thread");
        return;
    }
    // Slint's process-wide event-loop proxy must not point at a different
    // concurrent test's UI thread. Child processes also retain LLVM profiles.
    let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .env("AXSSH_CALLBACK_TEST_CHILD", test_name)
        .args([full_name.as_str(), "--exact", "--nocapture"])
        .output()
        .expect("isolated controller test process");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct UiProxy(std::sync::mpsc::SyncSender<UiEvent>);
impl EventLoopProxy for UiProxy {
    fn quit_event_loop(&self) -> std::result::Result<(), slint::EventLoopError> {
        Ok(())
    }
    fn invoke_from_event_loop(
        &self,
        event: UiEvent,
    ) -> std::result::Result<(), slint::EventLoopError> {
        self.0
            .try_send(event)
            .map_err(|_| slint::EventLoopError::EventLoopTerminated)
    }
}

struct HeadlessPlatform(
    Rc<MinimalSoftwareWindow>,
    std::sync::mpsc::SyncSender<UiEvent>,
);
impl Platform for HeadlessPlatform {
    fn create_window_adapter(
        &self,
    ) -> std::result::Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
    fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
        Some(Box::new(UiProxy(self.1.clone())))
    }
}

struct Harness {
    ui: AppWindow,
    state: Arc<Mutex<AppState>>,
    router: WindowRouter,
    _runtime: Runtime,
    profile: SessionProfile,
    directory: PathBuf,
    events: std::sync::mpsc::Receiver<UiEvent>,
}

impl Harness {
    fn new() -> Self {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        let (sender, events) = std::sync::mpsc::sync_channel(256);
        slint::platform::set_platform(Box::new(HeadlessPlatform(window, sender)))
            .expect("isolated headless platform");
        let ui = AppWindow::new().expect("generated component");
        let directory = std::env::temp_dir().join(format!("axssh-callbacks-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).expect("isolated controller files");
        let profile = SessionProfile::new("Fixture", "fixture.invalid", "fixture");
        let state = Arc::new(Mutex::new(AppState::new(
            ConfigStore::new(directory.join("config.json")),
            SessionStore {
                sessions: vec![profile.clone()],
                ..Default::default()
            },
        )));
        let router = WindowRouter::new(ui.as_weak());
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(2)
            .enable_all()
            .build()
            .expect("bounded test runtime");
        wire_callbacks(
            &ui,
            WindowCallbackContext {
                state: state.clone(),
                runtime: runtime.handle().clone(),
                font_registry: Arc::new(Mutex::new(FontRegistry::new())),
                log_directory: directory.join("logs"),
                window_router: router.clone(),
                window_id: MAIN_WINDOW_ID,
                detached_windows: Rc::new(RefCell::new(HashMap::new())),
            },
        );
        Self {
            ui,
            state,
            router,
            _runtime: runtime,
            profile,
            directory,
            events,
        }
    }
    fn status(&self) -> SharedString {
        while let Ok(event) = self.events.try_recv() {
            event();
        }
        self.ui.get_status()
    }

    fn wait_for(&self, predicate: impl Fn(&AppState) -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            self.status();
            if predicate(&self.state.lock().expect("controller state")) {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "controller transition deadline"
            );
            std::thread::yield_now();
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).expect("remove isolated controller files");
    }
}

#[test]
fn callbacks_manage_editor_tabs_respect_modal_guard_and_validate_identity() {
    isolated(
        "callbacks_manage_editor_tabs_respect_modal_guard_and_validate_identity",
        || {
            let harness = Harness::new();
            harness
                .ui
                .invoke_new_session_in_group("Fixture group".into());
            let editor = harness
                .state
                .lock()
                .expect("state")
                .active_tab_id()
                .expect("editor tab");
            assert_eq!(
                harness
                    .state
                    .lock()
                    .expect("state")
                    .active_editor_profile_id(),
                Some(None)
            );
            harness.ui.invoke_modal_state_changed(true);
            harness.ui.invoke_close_tab(editor.to_string().into());
            assert_eq!(
                harness.state.lock().expect("state").active_tab_id(),
                Some(editor)
            );
            harness.ui.invoke_modal_state_changed(false);
            harness
                .ui
                .invoke_edit_session(harness.profile.id.to_string().into());
            assert_eq!(
                harness
                    .state
                    .lock()
                    .expect("state")
                    .active_editor_profile_id(),
                Some(Some(harness.profile.id))
            );
            harness.ui.invoke_edit_session("bad UUID".into());
            assert!(harness.status().to_lowercase().contains("session"));
            harness
                .ui
                .invoke_copy_session(Uuid::new_v4().to_string().into());
            assert_eq!(harness.status(), "Session not found");
            harness.ui.invoke_copy_session("bad UUID".into());
            assert_eq!(harness.status(), "Session not found");
            harness.ui.invoke_cancel_session_dialog();
            assert!(
                harness
                    .state
                    .lock()
                    .expect("state")
                    .active_editor_profile_id()
                    .is_none()
            );
            // Preload the settings tab so selecting it does not enumerate system fonts.
            let settings = harness.state.lock().expect("state").open_settings_tab();
            harness.ui.invoke_open_settings();
            assert_eq!(
                harness.state.lock().expect("state").active_tab_id(),
                Some(settings)
            );
            harness.ui.invoke_new_session();
            let editor = harness
                .state
                .lock()
                .expect("state")
                .active_tab_id()
                .expect("new editor");
            harness.ui.invoke_activate_tab(settings.to_string().into());
            assert_eq!(
                harness.state.lock().expect("state").active_tab_id(),
                Some(settings)
            );
            harness.ui.invoke_cycle_tab(true);
            assert_eq!(
                harness.state.lock().expect("state").active_tab_id(),
                Some(editor)
            );
            harness.ui.invoke_move_tab(editor.to_string().into(), 0);
            assert_eq!(
                harness.state.lock().expect("state").tab_summaries()[0].id,
                editor
            );
            harness.ui.invoke_close_tab(editor.to_string().into());
            assert_eq!(
                harness.state.lock().expect("state").active_tab_id(),
                Some(settings)
            );
            assert!(
                std::fs::read_dir(&harness.directory)
                    .expect("isolated files")
                    .next()
                    .is_none()
            );
        },
    );
}

#[test]
fn callbacks_read_terminal_selection_and_reject_foreign_or_invalid_targets() {
    isolated(
        "callbacks_read_terminal_selection_and_reject_foreign_or_invalid_targets",
        || {
            let harness = Harness::new();
            let id = {
                let mut app = harness.state.lock().expect("state");
                let id = app.open_terminal_tab(&harness.profile);
                let terminal = app.terminal_mut(id).expect("terminal");
                terminal.terminal = Some(ax_ssh::terminal::TerminalModel::from_text(
                    "hello controller",
                    80,
                    24,
                    100,
                ));
                harness.router.activate_tab(MAIN_WINDOW_ID, id, &mut app);
                id.to_string()
            };
            assert_eq!(
                harness
                    .ui
                    .invoke_terminal_selection_text(id.clone().into(), 0, 0, 0, 4),
                "hello"
            );
            assert!(
                harness
                    .ui
                    .invoke_terminal_selection_text(Uuid::new_v4().to_string().into(), 0, 0, 0, 4)
                    .is_empty()
            );
            assert!(
                harness
                    .ui
                    .invoke_terminal_selection_text("invalid".into(), 0, 0, 0, 4)
                    .is_empty()
            );
            harness.ui.invoke_terminal_pane_focus(id.clone().into());
            harness.ui.invoke_scroll_terminal(id.clone().into(), 0);
            harness
                .ui
                .invoke_resize_terminal(id.clone().into(), -1, -1, 8.0, 16.0);
            harness.ui.invoke_activate_tab("invalid".into());
            assert_eq!(
                harness
                    .state
                    .lock()
                    .expect("state")
                    .active_tab_id()
                    .map(|id| id.to_string()),
                Some(id)
            );
            let actions = harness.ui.global::<MenuActions>().invoke_recent_items(
                ModelRc::new(VecModel::from(vec![SharedString::from("workspace.axssh")])),
                true,
                "Clear".into(),
            );
            assert_eq!(actions.row_count(), 3);
            assert_eq!(
                actions.row_data(0).expect("recent action").action_id,
                "open-recent-workspace"
            );
            assert_eq!(
                harness
                    .ui
                    .global::<MenuActions>()
                    .invoke_next_index(actions, 0, 1),
                2,
                "menu navigation skips separators"
            );
        },
    );
}

#[test]
fn callbacks_select_and_sort_sftp_rows_and_reject_stale_rename_and_workerless_navigation() {
    isolated(
        "callbacks_select_and_sort_sftp_rows_and_reject_stale_rename_and_workerless_navigation",
        || {
            let harness = Harness::new();
            let tab = {
                let mut app = harness.state.lock().expect("state");
                let id = app.open_sftp_tab(&harness.profile);
                let terminal = app.terminal_mut(id).expect("SFTP tab");
                terminal.connected = true;
                terminal.sftp.open = true;
                terminal.sftp.path = "/remote".into();
                terminal.sftp.entries = vec![
                    SftpEntry {
                        name: "b.txt".into(),
                        path: "/remote/b.txt".into(),
                        is_dir: false,
                        is_symlink: false,
                        size: 2,
                        modified: None,
                    },
                    SftpEntry {
                        name: "a.txt".into(),
                        path: "/remote/a.txt".into(),
                        is_dir: false,
                        is_symlink: false,
                        size: 1,
                        modified: None,
                    },
                ];
                harness.router.activate_tab(MAIN_WINDOW_ID, id, &mut app);
                id
            };
            harness
                .ui
                .invoke_toggle_remote_sftp_selection("/remote/b.txt".into(), true);
            assert!(
                harness
                    .state
                    .lock()
                    .expect("state")
                    .terminal(tab)
                    .expect("SFTP")
                    .sftp
                    .selected
                    .contains("/remote/b.txt")
            );
            harness.ui.invoke_select_all_remote_sftp(true);
            assert_eq!(
                harness
                    .state
                    .lock()
                    .expect("state")
                    .terminal(tab)
                    .expect("SFTP")
                    .sftp
                    .selected
                    .len(),
                2
            );
            harness.ui.invoke_select_all_remote_sftp(false);
            assert!(
                harness
                    .state
                    .lock()
                    .expect("state")
                    .terminal(tab)
                    .expect("SFTP")
                    .sftp
                    .selected
                    .is_empty()
            );
            harness.ui.invoke_sort_remote_sftp("size".into());
            harness.ui.invoke_sort_remote_sftp("unknown-column".into());
            assert!(harness.status().contains("Cannot sort"));
            harness
                .ui
                .invoke_toggle_remote_sftp_selection("/not-visible".into(), true);
            assert!(harness.status().contains("no longer visible"));
            harness
                .ui
                .invoke_toggle_remote_sftp_selection("/remote/a.txt".into(), true);
            harness.ui.invoke_rename_remote_sftp(
                tab.to_string().into(),
                "/remote/a.txt".into(),
                "../escape".into(),
            );
            assert!(harness.status().contains("name is invalid"));
            harness.ui.invoke_rename_remote_sftp(
                Uuid::new_v4().to_string().into(),
                "/remote/a.txt".into(),
                "new.txt".into(),
            );
            assert!(harness.status().contains("Cannot rename"));
            harness.ui.invoke_rename_remote_sftp(
                tab.to_string().into(),
                "/remote/a.txt".into(),
                "new.txt".into(),
            );
            assert!(harness.status().contains("worker"));
            harness.ui.invoke_list_sftp_directory("/other".into());
            assert!(harness.status().contains("Cannot"));
            let app = harness.state.lock().expect("state");
            let sftp = &app.terminal(tab).expect("SFTP").sftp;
            assert!(!sftp.loading);
            assert_eq!(sftp.path, "/remote");
            assert!(sftp.entries.iter().any(|entry| entry.name == "a.txt"));
        },
    );
}

#[test]
fn terminal_callbacks_connect_to_a_real_worker_and_forward_keys_paste_focus_and_resize() {
    isolated(
        "terminal_callbacks_connect_to_a_real_worker_and_forward_keys_paste_focus_and_resize",
        || {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let harness = Harness::new();
            let listener = harness
                ._runtime
                .block_on(tokio::net::TcpListener::bind((
                    std::net::Ipv4Addr::LOCALHOST,
                    0,
                )))
                .expect("loopback terminal listener");
            let mut profile = SessionProfile::new_telnet("Loopback", "127.0.0.1");
            if let ConnectionProfile::Telnet(config) = &mut profile.connection {
                config.port = listener.local_addr().expect("listener address").port();
            }
            harness.state.lock().expect("state").sessions.sessions = vec![profile.clone()];
            let (received, bytes_received) = oneshot::channel();
            let peer = harness._runtime.spawn(async move {
                let (mut stream, _) = listener.accept().await.expect("worker connects");
                let mut negotiation = [0; 9];
                stream
                    .read_exact(&mut negotiation)
                    .await
                    .expect("actual RFC 854 startup negotiation");
                assert_eq!(negotiation, [255, 251, 3, 255, 253, 3, 255, 251, 31]);
                stream
                    .write_all(b"\x1b[?2004h\x1b[?1004hhello worker\r\n")
                    .await
                    .expect("remote terminal modes and output");
                let expected = b"\x1b[IK\x1b[200~x\ry\x1b[201~\x03\x1b[O";
                let mut actual = Vec::new();
                while actual.len() < expected.len() {
                    let mut chunk = [0; 64];
                    match timeout(Duration::from_secs(2), stream.read(&mut chunk)).await {
                        Ok(Ok(0)) | Err(_) => break,
                        Ok(Ok(count)) => actual.extend_from_slice(&chunk[..count]),
                        Ok(Err(error)) => panic!("callback input read failed: {error}"),
                    }
                }
                assert_eq!(actual, expected);
                received.send(()).expect("input acknowledgement");
                let mut extra = [0];
                assert_eq!(
                    stream.read(&mut extra).await.expect("worker closes stream"),
                    0,
                    "foreign input and duplicate focus must not leak bytes"
                );
            });
            harness
                .ui
                .invoke_connect_session(profile.id.to_string().into());
            harness.router.set_window_active(MAIN_WINDOW_ID, true);
            harness.wait_for(|app| {
                app.active_tab_id()
                    .and_then(|id| app.terminal(id))
                    .is_some_and(|terminal| {
                        terminal.connected
                            && terminal
                                .terminal
                                .as_ref()
                                .is_some_and(|model| model.focus_reporting_active())
                    })
            });
            let tab = harness
                .state
                .lock()
                .expect("state")
                .active_tab_id()
                .expect("connected tab");
            let id: SharedString = tab.to_string().into();
            assert!(
                !harness.router.workspace_actions_locked(
                    MAIN_WINDOW_ID,
                    &harness.state.lock().expect("state")
                ),
                "input fixture must have an unlocked workspace"
            );
            harness.ui.invoke_terminal_focus_state(id.clone(), true);
            harness.ui.invoke_terminal_focus_state(id.clone(), true);
            harness.ui.invoke_terminal_key(TerminalKeyboardEvent {
                terminal_id: Uuid::new_v4().to_string().into(),
                event: KeyboardEvent {
                    text: "leak".into(),
                    ..Default::default()
                },
            });
            harness.ui.invoke_terminal_key(TerminalKeyboardEvent {
                terminal_id: id.clone(),
                event: KeyboardEvent {
                    text: "K".into(),
                    logical_key: "K".into(),
                    ..Default::default()
                },
            });
            harness.ui.invoke_terminal_key(TerminalKeyboardEvent {
                terminal_id: id.clone(),
                event: KeyboardEvent {
                    text: "x\ny".into(),
                    is_paste: true,
                    ..Default::default()
                },
            });
            // Emulate the native input hook's owned modifier snapshot; a
            // headless test must not consult the physical keyboard state.
            input::update_native_event_modifiers(false, true, false, false);
            harness.ui.invoke_terminal_key(TerminalKeyboardEvent {
                terminal_id: id.clone(),
                event: KeyboardEvent {
                    text: "c".into(),
                    logical_key: "c".into(),
                    control: true,
                    uses_native_modifiers: true,
                    ..Default::default()
                },
            });
            input::clear_native_event_modifiers();
            harness.ui.invoke_terminal_focus_state(id.clone(), false);
            harness
                .ui
                .invoke_resize_terminal(id.clone(), 104, 30, 8.0, 16.0);
            harness
                .ui
                .invoke_resize_terminal(id.clone(), 104, 30, 8.0, 16.0);
            assert_eq!(
                harness
                    .state
                    .lock()
                    .expect("state")
                    .terminal(tab)
                    .expect("terminal")
                    .terminal
                    .as_ref()
                    .expect("model")
                    .size()
                    .columns(),
                104
            );
            harness
                ._runtime
                .block_on(async { timeout(Duration::from_secs(5), bytes_received).await })
                .expect("socket input deadline")
                .expect("socket input acknowledged");
            harness.ui.invoke_disconnect_session();
            harness
                ._runtime
                .block_on(async { timeout(Duration::from_secs(5), peer).await })
                .expect("peer shutdown deadline")
                .expect("loopback peer joins");
            harness.wait_for(|app| {
                app.terminal(tab)
                    .is_some_and(|terminal| !terminal.worker_running && !terminal.connected)
            });
            let app = harness.state.lock().expect("state");
            let terminal = app.terminal(tab).expect("disconnect retains terminal tab");
            assert!(
                !terminal.reconnecting,
                "explicit disconnect cancels automatic reconnect"
            );
            assert_eq!(terminal.telnet_route(), Some((profile.id, None)));
        },
    );
}

#[cfg(target_os = "macos")]
#[test]
fn native_sftp_drag_errors_reach_the_sftp_status_without_creating_a_transfer() {
    isolated(
        "native_sftp_drag_errors_reach_the_sftp_status_without_creating_a_transfer",
        || {
            let harness = Harness::new();
            assert!(GLOBAL_WINDOW_ROUTER.set(harness.router.clone()).is_ok());
            let tab = {
                let mut app = harness.state.lock().expect("state");
                let tab = app.open_sftp_tab(&harness.profile);
                let terminal = app.terminal_mut(tab).expect("SFTP tab");
                terminal.connected = true;
                terminal.sftp.open = true;
                terminal.sftp.status = "Directory ready".into();
                harness.router.activate_tab(MAIN_WINDOW_ID, tab, &mut app);
                tab
            };
            harness
                .ui
                .invoke_begin_native_remote_file_drag_sftp("/missing.txt".into());
            harness.wait_for(|app| {
                app.terminal(tab).is_some_and(|terminal| {
                    terminal
                        .sftp
                        .status
                        .contains("remote entry is no longer visible")
                })
            });
            harness.status();
            assert!(
                harness
                    .ui
                    .get_sftp_status()
                    .contains("Cannot start native SFTP drag")
            );
            assert!(
                harness
                    .state
                    .lock()
                    .expect("state")
                    .terminal(tab)
                    .expect("SFTP tab")
                    .sftp
                    .transfers
                    .is_empty()
            );
        },
    );
}

#[test]
fn clipboard_consent_revalidates_identity_limits_and_permission_before_sending_bytes() {
    isolated(
        "clipboard_consent_revalidates_identity_limits_and_permission_before_sending_bytes",
        || {
            use tokio::io::AsyncReadExt;
            let harness = Harness::new();
            let (tab, mut stream, mut events) = harness._runtime.block_on(async {
                let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                    .await
                    .unwrap();
                let mut profile = SessionProfile::new_telnet("Clipboard fixture", "127.0.0.1");
                let ConnectionProfile::Telnet(config) = &mut profile.connection else {
                    panic!("Telnet config")
                };
                config.port = listener.local_addr().unwrap().port();
                let (worker, events) = TelnetSessionHandle::spawn(
                    harness._runtime.handle(),
                    Uuid::new_v4(),
                    config.clone(),
                    80,
                    24,
                );
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut negotiation = [0; 9];
                stream.read_exact(&mut negotiation).await.unwrap();
                let mut app = harness.state.lock().unwrap();
                let tab = app.open_terminal_tab(&profile);
                let terminal = app.terminal_mut(tab).unwrap();
                terminal.worker = Some(TerminalWorker::Telnet(worker));
                terminal.connected = true;
                terminal.worker_running = true;
                terminal
                    .terminal
                    .as_mut()
                    .unwrap()
                    .set_osc52_clipboard_enabled(true);
                harness.router.activate_tab(MAIN_WINDOW_ID, tab, &mut app);
                (tab, stream, events)
            });
            let offer = || {
                let mut app = harness.state.lock().unwrap();
                let terminal = app.terminal_mut(tab).unwrap();
                let model = terminal.terminal.as_mut().unwrap();
                model.set_osc52_clipboard_enabled(true);
                model.process(b"\x1b]52;c;?\x07");
                let formatter = model.take_clipboard_load().unwrap();
                terminal.offer_clipboard_read(formatter).unwrap();
            };
            let read = |operation| {
                terminal_bridge::handle_osc52_clipboard_read_with(
                    &harness.router,
                    MAIN_WINDOW_ID,
                    tab,
                    &harness.state,
                    &harness.ui,
                    operation,
                )
            };
            offer();
            assert!(!terminal_bridge::handle_osc52_clipboard_read_with(
                &harness.router,
                Uuid::new_v4(),
                tab,
                &harness.state,
                &harness.ui,
                || panic!("foreign window must not read clipboard"),
            ));
            assert!(!read(
                Box::new(|| Err(anyhow::anyhow!("injected clipboard error")))
                    as Box<dyn FnOnce() -> Result<String>>
            ));
            assert!(harness.status().contains("injected clipboard error"));
            assert!(
                harness
                    .state
                    .lock()
                    .unwrap()
                    .terminal(tab)
                    .unwrap()
                    .clipboard_read_key()
                    .is_none()
            );
            offer();
            assert!(!read(Box::new(|| Ok(
                "x".repeat(ax_ssh::terminal::MAX_OSC52_CLIPBOARD_BYTES + 1)
            ))));
            assert!(harness.status().contains("64 KiB"));
            offer();
            assert!(!read(Box::new(|| {
                harness
                    .state
                    .lock()
                    .unwrap()
                    .terminal_mut(tab)
                    .unwrap()
                    .terminal
                    .as_mut()
                    .unwrap()
                    .set_osc52_clipboard_enabled(false);
                Ok("must not leak".into())
            })));
            offer();
            assert!(!read(Box::new(|| {
                offer();
                Ok("stale consent".into())
            })));
            assert!(
                harness
                    .state
                    .lock()
                    .unwrap()
                    .terminal(tab)
                    .unwrap()
                    .clipboard_read_key()
                    .is_some()
            );
            assert!(harness.ui.invoke_terminal_pane_command(
                tab.to_string().into(),
                "deny-osc52-clipboard-read".into()
            ));
            offer();
            assert!(read(Box::new(|| Ok("private fixture".into()))));
            harness._runtime.block_on(async {
                let expected = b"\x1b]52;c;cHJpdmF0ZSBmaXh0dXJl\x07";
                let mut bytes = vec![0; expected.len()];
                timeout(Duration::from_secs(3), stream.read_exact(&mut bytes))
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(bytes, expected);
                assert!(
                    timeout(Duration::from_millis(50), stream.read_u8())
                        .await
                        .is_err(),
                    "rejected reads cannot leak bytes"
                );
                while events.try_recv().is_ok() {}
                let worker = harness
                    .state
                    .lock()
                    .unwrap()
                    .terminal_mut(tab)
                    .unwrap()
                    .worker
                    .take()
                    .unwrap();
                timeout(Duration::from_secs(3), worker.shutdown())
                    .await
                    .unwrap()
                    .unwrap();
            });
            assert!(!read(Box::new(|| panic!(
                "consent is consumed exactly once"
            ))));
        },
    );
}
