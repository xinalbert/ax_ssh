//! Verify wheel input at a real SSH peer, beyond callback and queue assertions.

use super::*;
use ax_ssh::config::X11ForwardingMode;
use ax_ssh::terminal::TerminalViewportMode;
use rand::{SeedableRng, rngs::StdRng};
use russh::server::{self, Auth};
use slint::platform::software_renderer::Rgb565Pixel;
use slint::platform::{Key, WindowEvent};
use std::time::Instant;
use zeroize::Zeroizing;

const DEADLINE: Duration = Duration::from_secs(5);
const RESET_MODES: &[u8] = b"\x1b[?1049l\x1b[?1000;1002;1003;1006l\x1b[?1007h\x1b[?1l";
const USER: &str = "wheel-fixture";
const PASSWORD: &str = "wheel-fixture-password";
const BARRIER: u8 = 0;

struct Receiver {
    input: mpsc::Sender<Vec<u8>>,
    ready: Option<oneshot::Sender<(server::Handle, russh::ChannelId)>>,
}

impl server::Handler for Receiver {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if user == USER && password == PASSWORD {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        _channel: russh::Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: russh::ChannelId,
        _term: &str,
        _columns: u32,
        _rows: u32,
        _pixel_width: u32,
        _pixel_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: russh::ChannelId,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        assert!(
            self.ready
                .take()
                .expect("one interactive shell")
                .send((session.handle(), channel))
                .is_ok()
        );
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: russh::ChannelId,
        _command: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_failure(channel)?;
        session.close(channel)?;
        Ok(())
    }

    async fn data(
        &mut self,
        _channel: russh::ChannelId,
        data: &[u8],
        _session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        assert!(data.len() <= 4096, "bounded fixture input");
        self.input
            .try_send(data.to_vec())
            .expect("bounded SSH receive queue");
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
struct Geometry {
    top_offset: f32,
    cell_width: f32,
    cell_height: f32,
}

struct Fixture {
    harness: Harness,
    tab: Uuid,
    received: mpsc::Receiver<Vec<u8>>,
    output: mpsc::Receiver<SshSessionEvent>,
    peer: server::Handle,
    channel: russh::ChannelId,
    task: tokio::task::JoinHandle<()>,
    geometry: Rc<RefCell<Option<Geometry>>>,
}

impl Fixture {
    fn new(detached: bool) -> Self {
        tracing_subscriber::fmt()
            .with_env_filter("ax_ssh::terminal_scroll=debug")
            .with_test_writer()
            .try_init()
            .expect("isolated wheel diagnostics");
        let mut harness = Harness::new();
        let (profile, output, received, ready, task) = harness._runtime.block_on(async {
            let mut random = StdRng::seed_from_u64(20261015);
            let key = russh::keys::PrivateKey::random(&mut random, russh::keys::Algorithm::Ed25519)
                .expect("deterministic fixture host key");
            let fingerprint = key
                .public_key()
                .fingerprint(russh::keys::HashAlg::Sha256)
                .to_string();
            let config = Arc::new(server::Config {
                keys: vec![key],
                auth_rejection_time: Duration::from_millis(1),
                auth_rejection_time_initial: Some(Duration::ZERO),
                ..Default::default()
            });
            let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                .await
                .expect("loopback SSH listener");
            let mut profile = SessionProfile::new("Wheel fixture", "127.0.0.1", USER);
            let ssh = profile.ssh_mut().expect("SSH profile");
            ssh.port = listener.local_addr().expect("loopback address").port();
            ssh.host_key_fingerprint = Some(fingerprint);
            ssh.x11_forwarding = X11ForwardingMode::Off;
            let (input, received) = mpsc::channel(16);
            let (ready, ready_rx) = oneshot::channel();
            let task = tokio::spawn(async move {
                let (stream, _) = timeout(DEADLINE, listener.accept())
                    .await
                    .expect("SSH accept deadline")
                    .expect("SSH client connects");
                server::run_stream(
                    config,
                    stream,
                    Receiver {
                        input,
                        ready: Some(ready),
                    },
                )
                .await
                .expect("SSH server starts")
                .await
                .expect("SSH server disconnects cleanly");
            });
            let (worker, output) = SshSessionHandle::spawn(
                &Handle::current(),
                Uuid::new_v4(),
                profile.clone(),
                Zeroizing::new(PASSWORD.to_owned()),
                80,
                24,
            );
            (profile, (worker, output), received, ready_rx, task)
        });
        let (worker, mut output) = output;
        harness._runtime.block_on(async {
            timeout(DEADLINE, async {
                loop {
                    match output.recv().await.expect("SSH startup event") {
                        SshSessionEvent::Connected => break,
                        SshSessionEvent::Failed(error) => panic!("SSH fixture startup: {error}"),
                        SshSessionEvent::HostKeyRejected { .. }
                        | SshSessionEvent::AuthenticationFailed => {
                            panic!("fixture must pass production trust and authentication")
                        }
                        _ => {}
                    }
                }
            })
            .await
            .expect("SSH startup deadline");
        });
        let (peer, channel) = harness._runtime.block_on(async {
            timeout(DEADLINE, ready)
                .await
                .expect("SSH shell deadline")
                .expect("SSH shell ready")
        });
        let tab = {
            let mut app = harness.state.lock().expect("state");
            let tab = app.open_terminal_tab(&profile);
            let terminal = app.terminal_mut(tab).expect("SSH terminal");
            terminal.worker = Some(TerminalWorker::Ssh(worker));
            terminal.connected = true;
            terminal.worker_running = true;
            harness.router.activate_tab(MAIN_WINDOW_ID, tab, &mut app);
            tab
        };
        if detached {
            harness.router = WindowRouter::new(slint::Weak::default());
            let window_id = Uuid::from_u128(1);
            harness.router.register_detached(
                window_id,
                harness.ui.as_weak(),
                WorkspaceTransfer {
                    source_window_id: MAIN_WINDOW_ID,
                    tab_ids: vec![tab],
                    active_tab_id: Some(tab),
                },
                Some(PaneTree::new(tab)),
            );
            terminal_bridge::wire_terminal(
                &harness.ui,
                harness.state.clone(),
                harness._runtime.handle().clone(),
                Arc::new(Mutex::new(FontRegistry::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
                harness.router.clone(),
                window_id,
            );
        }
        assert!(GLOBAL_WINDOW_ROUTER.set(harness.router.clone()).is_ok());
        harness.ui.set_detached_window(detached);
        harness.ui.set_apple_platform(cfg!(target_os = "macos"));
        apply_settings_to_component(&harness.ui, &AppSettings::default());
        let geometry = Rc::new(RefCell::new(None));
        let recorded = geometry.clone();
        harness
            .ui
            .on_terminal_geometry(move |id, _, _, _, _, _, _, _, height, cw, ch, _, rows| {
                assert_eq!(id.as_str(), tab.to_string());
                *recorded.borrow_mut() = Some(Geometry {
                    top_offset: height - rows as f32 * ch,
                    cell_width: cw,
                    cell_height: ch,
                });
            });
        harness.ui.show().expect("headless terminal visible");
        harness.window.set_size(slint::PhysicalSize::new(800, 600));
        let mut fixture = Self {
            harness,
            tab,
            received,
            output,
            peer,
            channel,
            task,
            geometry,
        };
        fixture.modes(b"");
        fixture
    }

    fn modes(&mut self, modes: &[u8]) {
        let mut bytes = RESET_MODES.to_vec();
        bytes.extend_from_slice(modes);
        self.harness._runtime.block_on(async {
            self.peer
                .data(self.channel, bytes.clone())
                .await
                .expect("fixture output");
            timeout(DEADLINE, async {
                let mut parsed = 0;
                while parsed < bytes.len() {
                    match self.output.recv().await.expect("SSH output event") {
                        SshSessionEvent::Output(output) => {
                            parsed += output.data.len();
                            let mut app = self.harness.state.lock().expect("state");
                            terminal_bridge::process_terminal_output(
                                app.terminal_mut(self.tab).expect("terminal"),
                                &output.data,
                            )
                            .expect("production terminal parser");
                        }
                        SshSessionEvent::Failed(error) => panic!("SSH output: {error}"),
                        _ => {}
                    }
                }
                assert_eq!(parsed, bytes.len(), "only fixture output is parsed");
            })
            .await
            .expect("SSH output deadline");
        });
        self.harness
            .state
            .lock()
            .expect("state")
            .terminal_mut(self.tab)
            .expect("terminal")
            .prepare_terminal_output_snapshot();
        refresh_workspace(&self.harness.ui.as_weak(), &self.harness.state);
        let deadline = Instant::now() + DEADLINE;
        let mut layout_ticks = 0;
        loop {
            self.harness.status();
            slint::platform::update_timers_and_animations();
            let size = self.harness.window.window().size();
            let mut pixels =
                vec![Rgb565Pixel::default(); size.width as usize * size.height as usize];
            self.harness.window.draw_if_needed(|renderer| {
                // Numerical layout only; no image is saved or inspected.
                renderer.render(&mut pixels, size.width as usize);
            });
            layout_ticks += 1;
            if layout_ticks >= 16
                && self.geometry.borrow().is_some()
                && self.harness.ui.get_terminal_panes().row_count() == 1
            {
                break;
            }
            assert!(Instant::now() < deadline, "terminal layout deadline");
            // Let the actual Slint clock advance through the pane's two
            // layout-settle ticks. This waits only inside the isolated test.
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn scroll(&self, horizontal: f32, vertical: f32, shift: bool) {
        let geometry = self.geometry.borrow().expect("measured terminal grid");
        assert!(
            geometry.cell_width > 1.0 && geometry.cell_height > 1.0,
            "{geometry:?}"
        );
        let pane = self
            .harness
            .ui
            .get_terminal_panes()
            .row_data(0)
            .expect("visible terminal pane");
        assert!(pane.terminal.connected, "fixture pane must accept input");
        assert!(
            !self.harness.ui.get_modal_open(),
            "fixture must not be modal"
        );
        let root = i_slint_core::item_tree::ItemRc::new_root(
            i_slint_core::window::WindowInner::from_pub(self.harness.ui.window()).component(),
        );
        let touch = root
            .visit_descendants(|item| {
                if item.is_visible() && item.downcast::<i_slint_core::items::TouchArea>().is_some()
                {
                    let size = item.geometry().size;
                    if size.width > 300.0 && size.height > 100.0 {
                        return std::ops::ControlFlow::Break(item.clone());
                    }
                }
                std::ops::ControlFlow::Continue(())
            })
            .expect("terminal body in the single-pane fixture");
        // Map from the input item so pane headers and desktop chrome cannot
        // accidentally put the synthetic wheel outside the terminal.
        let point = touch.map_to_window(i_slint_core::lengths::LogicalPoint::new(
            geometry.cell_width * 2.25,
            geometry.top_offset + geometry.cell_height * 1.25,
        ));
        let position = slint::LogicalPosition::new(point.x, point.y);
        let window = self.harness.ui.window();
        if shift {
            window.dispatch_event(WindowEvent::KeyPressed {
                text: Key::Shift.into(),
            });
        }
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerScrolled {
            position,
            delta_x: horizontal * geometry.cell_width,
            delta_y: vertical * geometry.cell_height,
        });
        if shift {
            window.dispatch_event(WindowEvent::KeyReleased {
                text: Key::Shift.into(),
            });
        }
    }

    fn received(&mut self, expected: &[u8]) {
        // FIFO barrier: prove that local/bypassed wheels sent no bytes without
        // treating a quiet socket or a sleep as evidence of success.
        self.harness
            .state
            .lock()
            .expect("state")
            .terminal(self.tab)
            .expect("terminal")
            .worker
            .as_ref()
            .expect("SSH worker")
            .request_send_kind(vec![BARRIER], state::terminal::TerminalInputKind::Key)
            .expect("receive barrier");
        let actual = self.harness._runtime.block_on(async {
            timeout(DEADLINE, async {
                let mut bytes = Vec::new();
                loop {
                    let chunk = self.received.recv().await.expect("SSH receive data");
                    bytes.extend_from_slice(&chunk);
                    assert!(bytes.len() <= 4096, "bounded received byte stream");
                    if bytes.last() == Some(&BARRIER) {
                        break bytes;
                    }
                }
            })
            .await
            .expect("SSH receive deadline")
        });
        let mut expected = expected.to_vec();
        expected.push(BARRIER);
        let pane = self
            .harness
            .ui
            .get_terminal_panes()
            .row_data(0)
            .expect("pane");
        assert_eq!(
            actual,
            expected,
            "SSH receive: geometry={:?}, window={:?}, reporting={}, notice={}",
            self.geometry.borrow(),
            self.harness.ui.window().size(),
            pane.terminal.mouse_wheel_reporting,
            pane.terminal.notice.visible
        );
    }

    fn offset(&self) -> usize {
        self.harness
            .state
            .lock()
            .expect("state")
            .terminal(self.tab)
            .expect("terminal")
            .terminal
            .as_ref()
            .expect("model")
            .display_offset()
    }

    fn finish(self) {
        let worker = self
            .harness
            .state
            .lock()
            .expect("state")
            .terminal_mut(self.tab)
            .expect("terminal")
            .worker
            .take()
            .expect("worker owner");
        self.harness._runtime.block_on(async {
            worker.shutdown().await.expect("explicit worker shutdown");
            timeout(DEADLINE, self.task)
                .await
                .expect("SSH peer shutdown deadline")
                .expect("SSH peer joins");
        });
        self.harness.ui.hide().expect("hide headless terminal");
    }
}

fn wheel_bytes(detached: bool) {
    let mut fixture = Fixture::new(detached);
    fixture.modes(b"\x1b[?1000;1006h");
    fixture.harness.ui.invoke_terminal_pointer_input(
        fixture.tab.to_string().into(),
        TerminalPointerInput {
            button: 3,
            repeat_count: 1,
            ..Default::default()
        },
    );
    fixture.received(b"\x1b[<64;1;1M");
    fixture.scroll(0.0, 3.0, false);
    fixture.received(&b"\x1b[<64;3;2M".repeat(3));
    let height = fixture.geometry.borrow().expect("grid").cell_height;
    fixture.scroll(0.0, 60.0 / height, false);
    fixture.received(&b"\x1b[<64;3;2M".repeat((60.0 / height).round() as usize));
    fixture.scroll(0.0, -60.0 / height, false);
    fixture.received(&b"\x1b[<65;3;2M".repeat((60.0 / height).round() as usize));
    fixture.scroll(0.0, 1000.0, false);
    fixture.received(&b"\x1b[<64;3;2M".repeat(256));
    fixture.scroll(0.0, -1.0, false);
    fixture.received(b"\x1b[<65;3;2M");
    fixture.scroll(-1.0, 0.0, false);
    fixture.received(b"\x1b[<66;3;2M");
    fixture.scroll(0.0, 0.2, false);
    fixture.scroll(0.0, 0.2, false);
    fixture.received(b"");
    fixture.scroll(0.0, 0.6, false);
    fixture.received(b"\x1b[<64;3;2M");

    fixture.modes(b"\x1b[?1000h");
    fixture.scroll(0.0, 1.0, false);
    fixture.received(b"\x1b[M`#\"");

    fixture.modes(b"\x1b[?1049h");
    fixture.scroll(0.0, 1.0, false);
    fixture.received(b"\x1b[A");
    fixture.modes(b"\x1b[?1049h\x1b[?1h");
    fixture.scroll(0.0, -2.0, false);
    fixture.received(&b"\x1bOB".repeat(2));
    fixture
        .harness
        .state
        .lock()
        .expect("state")
        .sessions
        .settings
        .terminal
        .alternate_scroll_to_home = true;
    fixture.scroll(0.0, 2.0, false);
    fixture.received(&b"\x1bOA\x1bOH".repeat(2));
    fixture.modes(b"\x1b[?1049h\x1b[?1000;1006h");
    fixture.scroll(0.0, 1.0, false);
    fixture.received(b"\x1b[<64;3;2M");

    fixture.modes(b"\x1b[?1049h\x1b[?1007l");
    fixture.scroll(0.0, 1.0, false);
    fixture.received(b"");
    assert_eq!(fixture.offset(), 0);
    assert_eq!(
        fixture
            .harness
            .state
            .lock()
            .expect("state")
            .terminal(fixture.tab)
            .expect("terminal")
            .terminal
            .as_ref()
            .expect("model")
            .viewport_mode(),
        TerminalViewportMode::AlternateScreen
    );

    let history = (0..80)
        .map(|i| format!("history {i}\r\n"))
        .collect::<String>();
    fixture.modes(history.as_bytes());
    fixture.scroll(0.0, 1.0, false);
    fixture.received(b"");
    assert_eq!(fixture.offset(), 1, "normal wheel moves local history");
    let mut reporting_history = b"\x1b[?1000;1006h".to_vec();
    reporting_history.extend_from_slice(history.as_bytes());
    fixture.modes(&reporting_history);
    let before = fixture.offset();
    fixture.scroll(0.0, 1.0, true);
    fixture.received(b"");
    assert_eq!(fixture.offset(), before + 1, "Shift bypass stays local");
    fixture.finish();
}

#[test]
fn main_window_wheel_bytes_reach_the_ssh_terminal() {
    isolated(
        "terminal_wheel::main_window_wheel_bytes_reach_the_ssh_terminal",
        || wheel_bytes(false),
    );
}

#[test]
fn detached_window_wheel_bytes_reach_the_ssh_terminal() {
    isolated(
        "terminal_wheel::detached_window_wheel_bytes_reach_the_ssh_terminal",
        || wheel_bytes(true),
    );
}
