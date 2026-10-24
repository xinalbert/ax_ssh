//! Real loopback peers exercise trust, authentication and the terminal byte chain.

use super::*;
use rand::{SeedableRng, rngs::StdRng};
use russh::{Channel, ChannelId, keys::PrivateKey, server};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::net::TcpListener;
use tokio::task::JoinSet;

const SECRET: &str = "fixture-ephemeral-input";

#[derive(Default)]
struct Observed {
    authentications: AtomicUsize,
    execs: AtomicUsize,
    input: Mutex<Vec<u8>>,
    sizes: Mutex<Vec<(u32, u32, u32, u32)>>,
    shell: Mutex<Option<server::Handle>>,
}

struct Peer(Arc<Observed>);

impl server::Handler for Peer {
    type Error = russh::Error;

    async fn auth_password(
        &mut self,
        user: &str,
        secret: &str,
    ) -> Result<server::Auth, Self::Error> {
        self.0.authentications.fetch_add(1, Ordering::SeqCst);
        Ok(if user == "fixture" && secret == SECRET {
            server::Auth::Accept
        } else {
            server::Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        _: Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        id: ChannelId,
        _: &str,
        columns: u32,
        rows: u32,
        width: u32,
        height: u32,
        _: &[(russh::Pty, u32)],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        self.0
            .sizes
            .lock()
            .unwrap()
            .push((columns, rows, width, height));
        session.channel_success(id)?;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        _: ChannelId,
        columns: u32,
        rows: u32,
        width: u32,
        height: u32,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        self.0
            .sizes
            .lock()
            .unwrap()
            .push((columns, rows, width, height));
        Ok(())
    }

    async fn shell_request(
        &mut self,
        id: ChannelId,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        *self.0.shell.lock().unwrap() = Some(session.handle());
        session.channel_success(id)?;
        session.data(id, b"E4_READY\r\n\x1b[6n".to_vec())?;
        Ok(())
    }

    async fn data(
        &mut self,
        id: ChannelId,
        data: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        let mut input = self.0.input.lock().unwrap();
        assert!(input.len() + data.len() < 8192);
        input.extend_from_slice(data);
        session.data(id, data.to_vec())?;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        id: ChannelId,
        _: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        self.0.execs.fetch_add(1, Ordering::SeqCst);
        session.channel_failure(id)?;
        session.close(id)?;
        Ok(())
    }
}

struct Fixture {
    profile: SessionProfile,
    observed: Arc<Observed>,
    stop: tokio::sync::oneshot::Sender<()>,
    task: JoinHandle<()>,
}

impl Fixture {
    async fn new(pin: bool, rotate_at: usize) -> Self {
        let mut rng = StdRng::seed_from_u64(404);
        let key = PrivateKey::random(&mut rng, russh::keys::Algorithm::Ed25519).unwrap();
        let other_key = PrivateKey::random(&mut rng, russh::keys::Algorithm::Ed25519).unwrap();
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let mut profile = SessionProfile::new("E4 fixture", address.ip().to_string(), "fixture");
        let ssh = profile.ssh_mut().unwrap();
        ssh.port = address.port();
        ssh.x11_forwarding = crate::config::X11ForwardingMode::Off;
        if pin {
            ssh.host_key_fingerprint = Some(
                key.public_key()
                    .fingerprint(russh::keys::HashAlg::Sha256)
                    .to_string(),
            );
        }
        let observed = Arc::new(Observed::default());
        let state = observed.clone();
        let (stop, mut stopped) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            let mut accepts = 0;
            loop {
                tokio::select! {
                    _ = &mut stopped => break,
                    result = connections.join_next(), if !connections.is_empty() => {
                        // A rejected probe intentionally ends the unauthenticated handshake.
                        assert!(!matches!(result, Some(Err(_))), "fixture task panicked");
                    }
                    accepted = listener.accept() => {
                        let (stream, _) = accepted.unwrap();
                        let config = Arc::new(server::Config {
                            keys: vec![if accepts >= rotate_at { other_key.clone() } else { key.clone() }],
                            auth_rejection_time: Duration::ZERO,
                            auth_rejection_time_initial: Some(Duration::ZERO),
                            ..Default::default()
                        });
                        accepts += 1;
                        let peer = Peer(state.clone());
                        connections.spawn(async move {
                            if let Ok(session) = server::run_stream(config, stream, peer).await {
                                session.await
                            } else { Ok(()) }
                        });
                    }
                }
            }
            connections.abort_all();
            while let Some(result) = connections.join_next().await {
                assert!(!matches!(result, Err(error) if error.is_panic()));
            }
        });
        Self {
            profile,
            observed,
            stop,
            task,
        }
    }

    async fn finish(self) {
        self.stop.send(()).unwrap();
        self.task.await.unwrap();
    }
}

fn terminal() -> TerminalSession {
    TerminalSession::spawn(
        &Handle::current(),
        "axssh-no-such-fixture-shell".into(),
        Arc::new(|| {}),
    )
}

async fn wait(client: &TerminalClient, predicate: impl Fn(&TerminalView) -> bool) -> TerminalView {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let view = client.view().unwrap();
            if predicate(&view) {
                return view;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("controller transition should complete")
}

fn text(view: &TerminalView) -> String {
    view.terminal
        .lines
        .iter()
        .flat_map(|line| &line.runs)
        .map(|run| run.text.as_str())
        .collect()
}

#[tokio::test]
async fn unknown_key_requires_exact_confirmation_before_authentication_and_echoes_bytes() {
    let fixture = Fixture::new(false, usize::MAX).await;
    let terminal = terminal();
    let client = terminal.client();
    client
        .connect(fixture.profile.clone(), Zeroizing::new(SECRET.into()))
        .unwrap();
    let view = wait(&client, |view| view.prompt.is_some()).await;
    let prompt = view.prompt.unwrap();
    assert!(matches!(
        prompt.kind,
        TerminalPromptKind::HostKey {
            confirmable: true,
            ..
        }
    ));
    assert_eq!(fixture.observed.authentications.load(Ordering::SeqCst), 0);
    assert!(!text(&client.view().unwrap()).contains(SECRET));
    client
        .respond(Uuid::new_v4(), Zeroizing::new(SECRET.into()))
        .unwrap();
    client
        .respond(prompt.id, Zeroizing::new(SECRET.into()))
        .unwrap();
    wait(&client, |view| {
        view.connected && text(view).contains("E4_READY")
    })
    .await;
    client
        .resize(TerminalSize::model_with_pixels(92, 31, 920, 620), 10, 20)
        .unwrap();
    client
        .key(
            TerminalKey::Text("REMOTE_E4".into()),
            TerminalModifiers::default(),
        )
        .unwrap();
    wait(&client, |view| {
        text(view).contains("REMOTE_E4") && view.terminal.max_columns == 92
    })
    .await;
    assert_eq!(fixture.observed.authentications.load(Ordering::SeqCst), 1);
    let input = fixture.observed.input.lock().unwrap().clone();
    assert!(input.windows(6).any(|bytes| bytes == b"\x1b[2;1R"));
    tokio::time::timeout(Duration::from_secs(5), async {
        while !fixture
            .observed
            .sizes
            .lock()
            .unwrap()
            .contains(&(92, 31, 920, 620))
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(fixture.observed.execs.load(Ordering::SeqCst), 0);
    client.disconnect().unwrap();
    wait(&client, |view| view.status == "Disconnected").await;
    terminal.shutdown().await.unwrap();
    fixture.finish().await;
}

#[tokio::test]
async fn fresh_authentication_retry_and_replacement_reject_old_terminal_input() {
    let fixture = Fixture::new(true, usize::MAX).await;
    let terminal = terminal();
    let client = terminal.client();
    client
        .connect(
            fixture.profile.clone(),
            Zeroizing::new("wrong-fixture-input".into()),
        )
        .unwrap();
    let view = wait(&client, |view| {
        view.prompt
            .as_ref()
            .is_some_and(|prompt| prompt.kind == TerminalPromptKind::Authentication)
    })
    .await;
    client
        .respond(view.prompt.unwrap().id, Zeroizing::new(SECRET.into()))
        .unwrap();
    let before = wait(&client, |view| {
        view.connected && text(view).contains("E4_READY")
    })
    .await;
    assert_eq!(fixture.observed.authentications.load(Ordering::SeqCst), 2);
    let old = client.for_view(&before);
    client
        .connect(fixture.profile.clone(), Zeroizing::new(SECRET.into()))
        .unwrap();
    old.key(
        TerminalKey::Text("STALE_E4".into()),
        TerminalModifiers::default(),
    )
    .unwrap();
    wait(&client, |view| {
        view.generation != before.generation && view.connected && text(view).contains("E4_READY")
    })
    .await;
    client
        .key(
            TerminalKey::Text("CURRENT_E4".into()),
            TerminalModifiers::default(),
        )
        .unwrap();
    wait(&client, |view| text(view).contains("CURRENT_E4")).await;
    assert!(
        !fixture
            .observed
            .input
            .lock()
            .unwrap()
            .windows(8)
            .any(|bytes| bytes == b"STALE_E4")
    );
    terminal.shutdown().await.unwrap();
    fixture.finish().await;
}

#[tokio::test]
async fn key_change_between_confirmed_probe_and_authentication_is_blocked() {
    let fixture = Fixture::new(false, 2).await;
    let terminal = terminal();
    let client = terminal.client();
    client
        .connect(fixture.profile.clone(), Zeroizing::new(SECRET.into()))
        .unwrap();
    let first = wait(&client, |view| view.prompt.is_some()).await;
    let first_id = first.prompt.unwrap().id;
    client
        .respond(first_id, Zeroizing::new(SECRET.into()))
        .unwrap();
    let blocked = wait(&client, |view| {
        view.prompt
            .as_ref()
            .is_some_and(|prompt| prompt.id != first_id)
    })
    .await;
    let blocked_prompt = blocked.prompt.unwrap();
    assert!(matches!(
        blocked_prompt.kind,
        TerminalPromptKind::HostKey {
            confirmable: false,
            ..
        }
    ));
    assert!(!blocked.connected);
    assert_eq!(fixture.observed.authentications.load(Ordering::SeqCst), 0);
    // Even a forged UI confirmation cannot override the blocked prompt.
    client
        .respond(blocked_prompt.id, Zeroizing::new(SECRET.into()))
        .unwrap();
    client.disconnect().unwrap();
    wait(&client, |view| view.status == "Disconnected").await;
    assert_eq!(fixture.observed.authentications.load(Ordering::SeqCst), 0);
    terminal.shutdown().await.unwrap();
    fixture.finish().await;
}

#[tokio::test]
async fn cancelling_a_stalled_probe_joins_it_and_rejects_late_prompt_responses() {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let mut profile = SessionProfile::new("Stalled fixture", "127.0.0.1", "fixture");
    profile.ssh_mut().unwrap().port = listener.local_addr().unwrap().port();
    let terminal = terminal();
    let client = terminal.client();
    client
        .connect(profile, Zeroizing::new(SECRET.into()))
        .unwrap();
    let (mut stream, _) = listener.accept().await.unwrap();
    let view = wait(&client, |view| view.status.starts_with("Checking SSH")).await;
    let old = client.for_view(&view);
    client.disconnect().unwrap();
    wait(&client, |view| view.status == "Disconnected").await;
    old.respond(Uuid::new_v4(), Zeroizing::new(SECRET.into()))
        .unwrap();
    use tokio::io::AsyncReadExt as _;
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    terminal.shutdown().await.unwrap();
}

#[tokio::test]
async fn telnet_uses_the_existing_worker_and_serial_open_failure_remains_recoverable() {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let mut profile = SessionProfile::new_telnet("Telnet fixture", "127.0.0.1");
    if let crate::config::ConnectionProfile::Telnet(config) = &mut profile.connection {
        config.port = listener.local_addr().unwrap().port();
    }
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"TELNET_READY\r\n").await.unwrap();
        let mut input = [0; 512];
        loop {
            let count = stream.read(&mut input).await.unwrap();
            if count == 0 {
                break;
            }
            stream.write_all(&input[..count]).await.unwrap();
        }
    });
    let terminal = terminal();
    let client = terminal.client();
    client
        .connect(profile, Zeroizing::new(String::new()))
        .unwrap();
    wait(&client, |view| {
        view.connected && text(view).contains("TELNET_READY")
    })
    .await;
    client
        .key(
            TerminalKey::Text("TELNET_E4".into()),
            TerminalModifiers::default(),
        )
        .unwrap();
    wait(&client, |view| text(view).contains("TELNET_E4")).await;
    client
        .connect(
            SessionProfile::new_serial("Serial fixture", "axssh-no-such-serial-port"),
            Zeroizing::new(String::new()),
        )
        .unwrap();
    wait(&client, |view| {
        matches!(view.identity, ConnectionIdentity::Serial { .. })
            && !view.connected
            && !view.status.starts_with("Connecting")
    })
    .await;
    client.disconnect().unwrap();
    wait(&client, |view| view.status == "Disconnected").await;
    terminal.shutdown().await.unwrap();
    peer.await.unwrap();
}

#[tokio::test]
async fn lost_ssh_connection_requests_fresh_credentials_and_cancel_revokes_retry() {
    let fixture = Fixture::new(true, usize::MAX).await;
    let terminal = terminal();
    let client = terminal.client();
    client
        .connect_with_reconnect(fixture.profile.clone(), Zeroizing::new(SECRET.into()), true)
        .unwrap();
    wait(&client, |view| {
        view.connected && text(view).contains("E4_READY")
    })
    .await;
    let shell = fixture.observed.shell.lock().unwrap().take().unwrap();
    shell
        .disconnect(
            russh::Disconnect::ByApplication,
            "fixture closed".into(),
            String::new(),
        )
        .await
        .unwrap();
    let view = wait(&client, |view| {
        view.prompt
            .as_ref()
            .is_some_and(|prompt| prompt.kind == TerminalPromptKind::Authentication)
    })
    .await;
    assert_eq!(fixture.observed.authentications.load(Ordering::SeqCst), 1);
    assert!(!view.connected);
    client.disconnect().unwrap();
    wait(&client, |view| view.status == "Disconnected").await;
    client
        .respond(view.prompt.unwrap().id, Zeroizing::new(SECRET.into()))
        .unwrap();
    terminal.shutdown().await.unwrap();
    assert_eq!(fixture.observed.authentications.load(Ordering::SeqCst), 1);
    fixture.finish().await;
}

#[tokio::test]
async fn telnet_reconnects_after_transport_loss_and_old_disconnect_cannot_close_replacement() {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let mut profile = SessionProfile::new_telnet("Retry fixture", "127.0.0.1");
    if let crate::config::ConnectionProfile::Telnet(config) = &mut profile.connection {
        config.port = listener.local_addr().unwrap().port();
    }
    let (close, closed) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut first, _) = listener.accept().await.unwrap();
        first.write_all(b"FIRST_READY\r\n").await.unwrap();
        closed.await.unwrap();
        drop(first);
        let (mut second, _) = listener.accept().await.unwrap();
        second.write_all(b"RETRY_READY\r\n").await.unwrap();
        let mut input = [0; 512];
        loop {
            let count = second.read(&mut input).await.unwrap();
            if count == 0 {
                break;
            }
            second.write_all(&input[..count]).await.unwrap();
        }
    });
    let terminal = terminal();
    let client = terminal.client();
    client
        .connect_with_reconnect(profile, Zeroizing::new(String::new()), true)
        .unwrap();
    let first = wait(&client, |view| {
        view.connected && text(view).contains("FIRST_READY")
    })
    .await;
    close.send(()).unwrap();
    wait(&client, |view| {
        view.connected && view.generation != first.generation && text(view).contains("RETRY_READY")
    })
    .await;
    client.for_view(&first).disconnect().unwrap();
    client
        .key(
            TerminalKey::Text("RETRY_ECHO".into()),
            TerminalModifiers::default(),
        )
        .unwrap();
    wait(&client, |view| {
        view.connected && text(view).contains("RETRY_ECHO")
    })
    .await;
    client.disconnect().unwrap();
    wait(&client, |view| view.status == "Disconnected").await;
    terminal.shutdown().await.unwrap();
    peer.await.unwrap();
}

#[tokio::test]
async fn disconnect_during_backoff_revokes_the_pending_deadline() {
    let terminal = terminal();
    let client = terminal.client();
    client
        .connect_with_reconnect(
            SessionProfile::new_serial("Retry fixture", "axssh-no-such-serial-port"),
            Zeroizing::new(String::new()),
            true,
        )
        .unwrap();
    wait(&client, |view| view.status.contains("Retry 1/5")).await;
    client.disconnect().unwrap();
    let disconnected = wait(&client, |view| view.status == "Disconnected").await;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let after = client.view().unwrap();
    assert_eq!(after.generation, disconnected.generation);
    assert_eq!(after.status, "Disconnected");
    assert!(after.prompt.is_none());
    assert!(!after.connected);
    terminal.shutdown().await.unwrap();
}
