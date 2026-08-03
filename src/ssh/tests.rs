use super::*;
use rand::SeedableRng as _;
use rand::rngs::StdRng;
use russh::server::{self, Auth};
use tokio::net::TcpListener;
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::time::{advance, pause, resume};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::config::{X11ServerProvider, X11Settings};

use super::worker::{MAX_ERROR_CHARS, bounded_error_message};

const TEST_USER: &str = "ax-test-user";
const TEST_PASSWORD: &str = "ax-test-password";

fn test_profile(name: &str, host: String) -> SessionProfile {
    let mut profile = SessionProfile::new(name, host, TEST_USER);
    profile
        .ssh_mut()
        .expect("test profile should use SSH")
        .x11_forwarding = false;
    profile
}

#[test]
fn interactive_client_config_disables_nagle() {
    assert!(client_config().nodelay);
}

#[derive(Clone)]
struct TestServer {
    expected_public_key: Option<PublicKey>,
    send_initial_prompt: bool,
    x11_requests: Option<mpsc::UnboundedSender<()>>,
}

impl TestServer {
    fn password_only() -> Self {
        Self {
            expected_public_key: None,
            send_initial_prompt: true,
            x11_requests: None,
        }
    }

    fn silent_password_only() -> Self {
        Self {
            expected_public_key: None,
            send_initial_prompt: false,
            x11_requests: None,
        }
    }
}

impl server::Handler for TestServer {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if user == TEST_USER && password == TEST_PASSWORD {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        public_key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        if user == TEST_USER && self.expected_public_key.as_ref() == Some(public_key) {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<server::Msg>,
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

    async fn x11_request(
        &mut self,
        channel: russh::ChannelId,
        _single_connection: bool,
        _x11_auth_protocol: &str,
        _x11_auth_cookie: &str,
        _x11_screen_number: u32,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        if let Some(requests) = &self.x11_requests {
            let _ = requests.send(());
        }
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: russh::ChannelId,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        if self.send_initial_prompt {
            session.data(channel, b"ax-test$ ".to_vec())?;
        }
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        channel: russh::ChannelId,
        _columns: u32,
        _rows: u32,
        _pixel_width: u32,
        _pixel_height: u32,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: russh::ChannelId,
        data: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        let command = String::from_utf8_lossy(data).trim().to_owned();
        session.data(
            channel,
            format!("\r\necho: {command}\r\nax-test$ ").into_bytes(),
        )?;
        Ok(())
    }
}

#[tokio::test]
async fn idle_shell_does_not_disconnect_when_no_channel_data_arrives() {
    let mut rng = StdRng::seed_from_u64(6);
    let host_key = russh::keys::PrivateKey::random(&mut rng, russh::keys::Algorithm::Ed25519)
        .expect("test host key should be generated");
    let server_config = Arc::new(server::Config {
        auth_rejection_time: Duration::from_millis(1),
        auth_rejection_time_initial: Some(Duration::ZERO),
        keys: vec![host_key],
        ..server::Config::default()
    });
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test SSH listener should bind");
    let address = listener
        .local_addr()
        .expect("test SSH listener should have an address");
    let server_task = tokio::spawn(async move {
        let mut sessions = Vec::new();
        for _ in 0..2 {
            let (stream, _) = listener
                .accept()
                .await
                .expect("test SSH connection should be accepted");
            let session = server::run_stream(
                server_config.clone(),
                stream,
                TestServer::silent_password_only(),
            )
            .await
            .expect("test SSH session should start");
            sessions.push(tokio::spawn(session));
        }
        sessions
    });

    let mut profile = test_profile("idle-test", address.ip().to_string());
    profile.ssh_mut().expect("test profile should use SSH").port = address.port();
    profile
        .ssh_mut()
        .expect("test profile should use SSH")
        .host_key_fingerprint = Some(
        probe_host_key(&profile)
            .await
            .expect("unknown host-key probe should return the rejected fingerprint"),
    );

    let (worker, mut events) = SshSessionHandle::spawn(
        &Handle::current(),
        Uuid::new_v4(),
        profile,
        Zeroizing::new(TEST_PASSWORD.to_owned()),
        120,
        36,
    );
    let connected = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("silent SSH worker should connect promptly");
    assert_eq!(connected, Some(SshSessionEvent::Connected));

    // The SSH handshake needs real I/O time; only pause the post-connect idle interval.
    pause();
    advance(Duration::from_secs(31)).await;
    for _ in 0..3 {
        tokio::task::yield_now().await;
    }
    assert!(
        !worker.is_finished(),
        "a quiet shell must remain owned by its worker until transport liveness fails"
    );
    assert!(
        events.try_recv().is_err(),
        "a quiet shell must not report a synthetic terminal failure"
    );

    resume();
    worker
        .request_disconnect()
        .expect("idle SSH worker should accept disconnect command");
    let disconnected = timeout(Duration::from_secs(2), async {
        loop {
            match events.recv().await {
                Some(SshSessionEvent::Disconnected) => break Some(SshSessionEvent::Disconnected),
                Some(SshSessionEvent::Failed(message)) => {
                    panic!("idle SSH worker failed during disconnect: {message}")
                }
                Some(_) => {}
                None => break None,
            }
        }
    })
    .await
    .expect("idle SSH worker should disconnect promptly");
    assert_eq!(disconnected, Some(SshSessionEvent::Disconnected));
    worker
        .shutdown()
        .await
        .expect("idle SSH worker should join cleanly");

    let sessions = server_task
        .await
        .expect("test SSH accept task should finish");
    for session in sessions {
        session.abort();
        let _ = session.await;
    }
}

#[test]
fn host_key_policy_rejects_unknown_and_mismatched_keys() {
    let actual = "SHA256:known-key";
    assert!(!fingerprint_is_trusted(None, actual));
    assert!(!fingerprint_is_trusted(Some("SHA256:other-key"), actual));
    assert!(fingerprint_is_trusted(Some(actual), actual));
}

#[test]
fn host_key_error_distinguishes_unknown_and_changed_keys() {
    let unknown = SshError::HostKeyRejected {
        expected: None,
        actual: "SHA256:new".into(),
    };
    assert!(unknown.to_string().contains("not trusted"));

    let changed = SshError::HostKeyRejected {
        expected: Some("SHA256:old".into()),
        actual: "SHA256:new".into(),
    };
    let message = changed.to_string();
    assert!(message.contains("mismatch"));
    assert!(message.contains("SHA256:old"));
    assert!(message.contains("SHA256:new"));
}

#[test]
fn client_keepalive_bounds_idle_session_lifetime() {
    let config = client_config();
    assert_eq!(config.keepalive_interval, Some(KEEPALIVE_INTERVAL));
    assert_eq!(config.keepalive_max, KEEPALIVE_MAX);
    assert_eq!(config.inactivity_timeout, Some(INACTIVITY_TIMEOUT));
}

#[test]
fn worker_errors_are_bounded_on_character_boundaries() {
    let error = anyhow::anyhow!("{}", "界".repeat(MAX_ERROR_CHARS + 20));
    let message = bounded_error_message(&error);
    assert_eq!(message.chars().count(), MAX_ERROR_CHARS + 3);
    assert!(message.ends_with("..."));
}

#[tokio::test]
async fn probe_then_password_login_preserves_host_key_verification() {
    let mut rng = StdRng::seed_from_u64(42);
    let host_key = russh::keys::PrivateKey::random(&mut rng, russh::keys::Algorithm::Ed25519)
        .expect("test host key should be generated");
    let expected_fingerprint = host_key
        .public_key()
        .fingerprint(russh::keys::HashAlg::Sha256)
        .to_string();
    let server_config = Arc::new(server::Config {
        auth_rejection_time: Duration::from_millis(1),
        auth_rejection_time_initial: Some(Duration::ZERO),
        keys: vec![host_key],
        ..server::Config::default()
    });
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test SSH listener should bind");
    let address = listener
        .local_addr()
        .expect("test SSH listener should have an address");
    let server_task = tokio::spawn(async move {
        let mut sessions = Vec::new();
        for _ in 0..4 {
            let (stream, _) = listener
                .accept()
                .await
                .expect("test SSH connection should be accepted");
            let session =
                server::run_stream(server_config.clone(), stream, TestServer::password_only())
                    .await
                    .expect("test SSH session should start");
            sessions.push(tokio::spawn(session));
        }
        sessions
    });

    let mut profile = test_profile("test", address.ip().to_string());
    profile.ssh_mut().expect("test profile should use SSH").port = address.port();
    let fingerprint = probe_host_key(&profile)
        .await
        .expect("unknown host-key probe should return the rejected fingerprint");
    assert_eq!(fingerprint, expected_fingerprint);

    profile
        .ssh_mut()
        .expect("test profile should use SSH")
        .host_key_fingerprint = Some(fingerprint);
    let connection = SshConnection::connect(&profile, Zeroizing::new(TEST_PASSWORD.to_owned()))
        .await
        .expect("trusted host with valid password should authenticate");
    assert!(!connection.is_closed());
    connection
        .disconnect()
        .await
        .expect("authenticated test connection should disconnect");

    let (failed_worker, mut failed_events) = SshSessionHandle::spawn(
        &Handle::current(),
        Uuid::new_v4(),
        profile.clone(),
        Zeroizing::new("incorrect-password".to_owned()),
        120,
        36,
    );
    let authentication_failed = timeout(Duration::from_secs(2), failed_events.recv())
        .await
        .expect("SSH worker should report authentication failure promptly");
    assert_eq!(
        authentication_failed,
        Some(SshSessionEvent::AuthenticationFailed)
    );
    failed_worker
        .shutdown()
        .await
        .expect("failed authentication worker should join cleanly");

    let (worker, mut events) = SshSessionHandle::spawn(
        &Handle::current(),
        Uuid::new_v4(),
        profile,
        Zeroizing::new(TEST_PASSWORD.to_owned()),
        120,
        36,
    );
    let connected = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("SSH worker should report connection promptly");
    assert_eq!(connected, Some(SshSessionEvent::Connected));
    worker
        .request_resize(100, 30)
        .expect("SSH worker should accept terminal resize");
    let resized = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("SSH worker should report terminal resize promptly");
    assert_eq!(
        resized,
        Some(SshSessionEvent::Resized {
            columns: 100,
            rows: 30,
        })
    );
    worker
        .request_send(b"whoami\r".to_vec())
        .expect("SSH worker should accept terminal input");
    let output = timeout(Duration::from_secs(2), async {
        loop {
            match events.recv().await {
                Some(SshSessionEvent::Output { data, .. })
                    if String::from_utf8_lossy(&data).contains("echo: whoami") =>
                {
                    break data;
                }
                Some(SshSessionEvent::Failed(message)) => {
                    panic!("SSH worker failed while waiting for shell output: {message}")
                }
                Some(_) => {}
                None => panic!("SSH worker event stream closed before shell output"),
            }
        }
    })
    .await
    .expect("SSH worker should return shell output");
    assert!(String::from_utf8_lossy(&output).contains("echo: whoami"));
    worker
        .request_disconnect()
        .expect("SSH worker should accept disconnect command");
    let disconnected = timeout(Duration::from_secs(2), async {
        loop {
            match events.recv().await {
                Some(SshSessionEvent::Disconnected) => break Some(SshSessionEvent::Disconnected),
                Some(SshSessionEvent::Failed(message)) => {
                    panic!("SSH worker failed during disconnect: {message}")
                }
                Some(_) => {}
                None => break None,
            }
        }
    })
    .await
    .expect("SSH worker should report disconnection promptly");
    assert_eq!(disconnected, Some(SshSessionEvent::Disconnected));
    worker
        .shutdown()
        .await
        .expect("SSH worker should join cleanly after disconnect");

    let sessions = server_task
        .await
        .expect("test SSH accept task should finish");
    for session in sessions {
        session.abort();
        match session.await {
            Err(error) if error.is_cancelled() => {}
            Err(error) => panic!("test SSH server task failed: {error}"),
            Ok(Ok(())) => {}
            Ok(Err(_)) => {}
        }
    }
}

#[tokio::test]
async fn x11_request_does_not_prepare_a_local_server_before_a_remote_channel_opens() {
    let mut rng = StdRng::seed_from_u64(91);
    let host_key = russh::keys::PrivateKey::random(&mut rng, russh::keys::Algorithm::Ed25519)
        .expect("test host key should be generated");
    let expected_fingerprint = host_key
        .public_key()
        .fingerprint(russh::keys::HashAlg::Sha256)
        .to_string();
    let server_config = Arc::new(server::Config {
        auth_rejection_time: Duration::from_millis(1),
        auth_rejection_time_initial: Some(Duration::ZERO),
        keys: vec![host_key],
        ..server::Config::default()
    });
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test SSH listener should bind");
    let address = listener
        .local_addr()
        .expect("test SSH listener should have an address");
    let (x11_request_tx, mut x11_request_rx) = mpsc::unbounded_channel();
    let server_task = tokio::spawn(async move {
        for _ in 0..2 {
            let (stream, _) = listener
                .accept()
                .await
                .expect("test SSH connection should be accepted");
            let handler = TestServer {
                expected_public_key: None,
                send_initial_prompt: true,
                x11_requests: Some(x11_request_tx.clone()),
            };
            tokio::spawn(
                server::run_stream(server_config.clone(), stream, handler)
                    .await
                    .expect("test SSH session should start"),
            );
        }
    });

    let mut profile = test_profile("x11-lazy", address.ip().to_string());
    {
        let ssh = profile.ssh_mut().expect("test profile should use SSH");
        ssh.port = address.port();
        ssh.x11_forwarding = true;
    }
    let fingerprint = probe_host_key(&profile)
        .await
        .expect("unknown host-key probe should return the rejected fingerprint");
    assert_eq!(fingerprint, expected_fingerprint);
    profile
        .ssh_mut()
        .expect("test profile should use SSH")
        .host_key_fingerprint = Some(fingerprint);

    let x11_settings = X11Settings {
        provider: X11ServerProvider::Custom,
        app_path: "/definitely/not/an/axssh-x-server".to_owned(),
        launch_on_connect: true,
        allow_no_auth: false,
    };
    let (worker, mut events) = SshSessionHandle::spawn_with_x11_settings(
        &Handle::current(),
        Uuid::new_v4(),
        profile,
        Zeroizing::new(TEST_PASSWORD.to_owned()),
        120,
        36,
        x11_settings,
    );

    let connected = timeout(Duration::from_secs(2), async {
        loop {
            match events.recv().await {
                Some(SshSessionEvent::Connected) => break true,
                Some(SshSessionEvent::X11ForwardingEnabled) => {}
                Some(SshSessionEvent::Failed(message)) => {
                    panic!("SSH worker failed before opening its shell: {message}")
                }
                Some(_) => {}
                None => break false,
            }
        }
    })
    .await
    .expect("SSH worker should connect without a local X server");
    assert!(connected);
    timeout(Duration::from_secs(2), x11_request_rx.recv())
        .await
        .expect("server should receive the X11 forwarding request")
        .expect("X11 request sender should remain open");

    worker
        .request_disconnect()
        .expect("worker should accept a disconnect request");
    worker
        .shutdown()
        .await
        .expect("worker should join after disconnect");
    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn private_key_login_opens_interactive_shell() {
    let mut rng = StdRng::seed_from_u64(84);
    let host_key = russh::keys::PrivateKey::random(&mut rng, russh::keys::Algorithm::Ed25519)
        .expect("test host key should be generated");
    let user_key = russh::keys::PrivateKey::random(&mut rng, russh::keys::Algorithm::Ed25519)
        .expect("test user key should be generated");
    let expected_fingerprint = host_key
        .public_key()
        .fingerprint(russh::keys::HashAlg::Sha256)
        .to_string();
    let expected_public_key = user_key.public_key().clone();
    let key_path = std::env::temp_dir().join(format!("ax-ssh-user-key-{}", uuid::Uuid::new_v4()));
    std::fs::write(
        &key_path,
        user_key
            .to_openssh(russh::keys::ssh_key::LineEnding::LF)
            .expect("test private key should encode")
            .as_bytes(),
    )
    .expect("test private key should be written");

    let server_config = Arc::new(server::Config {
        auth_rejection_time: Duration::from_millis(1),
        auth_rejection_time_initial: Some(Duration::ZERO),
        keys: vec![host_key],
        ..server::Config::default()
    });
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test SSH listener should bind");
    let address = listener
        .local_addr()
        .expect("test SSH listener should have an address");
    let server_task = tokio::spawn(async move {
        let mut sessions = Vec::new();
        for _ in 0..2 {
            let (stream, _) = listener
                .accept()
                .await
                .expect("test SSH connection should be accepted");
            let session = server::run_stream(
                server_config.clone(),
                stream,
                TestServer {
                    expected_public_key: Some(expected_public_key.clone()),
                    send_initial_prompt: true,
                    x11_requests: None,
                },
            )
            .await
            .expect("test SSH session should start");
            sessions.push(tokio::spawn(session));
        }
        sessions
    });

    let mut profile = test_profile("key-test", address.ip().to_string());
    {
        let ssh = profile.ssh_mut().expect("test profile should use SSH");
        ssh.port = address.port();
        ssh.auth = AuthMethod::PrivateKey {
            path: key_path.clone(),
        };
    }
    let fingerprint = probe_host_key(&profile)
        .await
        .expect("unknown host-key probe should return the rejected fingerprint");
    assert_eq!(fingerprint, expected_fingerprint);
    profile
        .ssh_mut()
        .expect("test profile should use SSH")
        .host_key_fingerprint = Some(fingerprint);

    let (worker, mut events) = SshSessionHandle::spawn(
        &Handle::current(),
        Uuid::new_v4(),
        profile,
        Zeroizing::new(String::new()),
        120,
        36,
    );
    let connected = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("private-key worker should connect promptly");
    assert_eq!(connected, Some(SshSessionEvent::Connected));
    worker
        .request_send(b"id\r".to_vec())
        .expect("private-key shell should accept input");
    timeout(Duration::from_secs(2), async {
        loop {
            if let Some(SshSessionEvent::Output { data, .. }) = events.recv().await
                && String::from_utf8_lossy(&data).contains("echo: id")
            {
                break;
            }
        }
    })
    .await
    .expect("private-key shell should return output");
    worker
        .shutdown()
        .await
        .expect("private-key worker should shut down cleanly");

    let sessions = server_task
        .await
        .expect("test SSH accept task should finish");
    for session in sessions {
        session.abort();
        let _ = session.await;
    }
    std::fs::remove_file(key_path).expect("test private key should be removed");
}
