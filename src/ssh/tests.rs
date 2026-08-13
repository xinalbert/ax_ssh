use super::*;
use rand::SeedableRng as _;
use rand::rngs::StdRng;
use russh::server::{self, Auth};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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

#[tokio::test]
async fn probe_reports_disconnect_before_server_host_key_as_handshake_failure() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test SSH listener should bind");
    let address = listener
        .local_addr()
        .expect("test SSH listener should have an address");
    let server_task = tokio::spawn(async move {
        let (mut stream, _) = listener
            .accept()
            .await
            .expect("test SSH connection should be accepted");
        let mut client_identification = Vec::new();
        while client_identification.len() < 256 {
            let byte = stream
                .read_u8()
                .await
                .expect("client identification should be readable");
            client_identification.push(byte);
            if byte == b'\n' {
                break;
            }
        }
        assert!(client_identification.starts_with(b"SSH-2.0-"));
        assert_eq!(client_identification.last(), Some(&b'\n'));
        stream
            .write_all(b"SSH-2.0-AxSSH-test\r\n")
            .await
            .expect("server identification should be writable");
        stream
            .shutdown()
            .await
            .expect("test SSH stream should close");
    });
    let mut profile = test_profile("early-disconnect", address.ip().to_string());
    profile.ssh_mut().expect("test profile should use SSH").port = address.port();

    let error = probe_host_key(&profile)
        .await
        .expect_err("probe should fail before observing a server host key");
    let message = format!("{error:#}");
    assert!(message.contains("SSH banner or key exchange"));
    assert!(message.contains("before the server presented a host key"));
    server_task.await.expect("test SSH server should join");
}

#[test]
fn interactive_pty_requests_crlf_output_mode() {
    assert_eq!(
        INTERACTIVE_TERMINAL_MODES,
        &[(russh::Pty::OPOST, 1), (russh::Pty::ONLCR, 1)]
    );
}

#[test]
fn ssh_agent_identity_attempts_are_bounded() {
    assert_eq!(ssh_agent_attempt_count(0), 0);
    assert_eq!(ssh_agent_attempt_count(3), 3);
    assert_eq!(
        ssh_agent_attempt_count(MAX_SSH_AGENT_IDENTITIES + 20),
        MAX_SSH_AGENT_IDENTITIES
    );
}

#[test]
fn ssh_agent_errors_do_not_expose_runtime_details() {
    assert_eq!(
        SshError::SshAgentUnavailable.to_string(),
        "SSH agent is unavailable"
    );
    assert_eq!(
        SshError::SshAgentOperationFailed.to_string(),
        "SSH agent could not complete authentication"
    );
}

fn append_agent_string(frame: &mut Vec<u8>, value: &[u8]) {
    let length = u32::try_from(value.len()).expect("test agent string should fit in a u32");
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(value);
}

fn read_agent_string<'a>(frame: &'a [u8], offset: &mut usize) -> &'a [u8] {
    let length_end = offset.checked_add(4).expect("test agent offset should fit");
    let length_bytes: [u8; 4] = frame
        .get(*offset..length_end)
        .expect("test agent string length should be present")
        .try_into()
        .expect("test agent string length should contain four bytes");
    let length = u32::from_be_bytes(length_bytes) as usize;
    let value_end = length_end
        .checked_add(length)
        .expect("test agent string length should fit");
    let value = frame
        .get(length_end..value_end)
        .expect("test agent string should be present");
    *offset = value_end;
    value
}

async fn read_agent_frame(stream: &mut tokio::io::DuplexStream) -> Vec<u8> {
    let mut length = [0_u8; 4];
    stream
        .read_exact(&mut length)
        .await
        .expect("test agent frame length should be readable");
    let length = u32::from_be_bytes(length) as usize;
    assert!(length <= 256 * 1024);
    let mut frame = vec![0_u8; length];
    stream
        .read_exact(&mut frame)
        .await
        .expect("test agent frame should be readable");
    frame
}

async fn write_agent_frame(stream: &mut tokio::io::DuplexStream, frame: &[u8]) {
    let length = u32::try_from(frame.len()).expect("test agent frame should fit in a u32");
    stream
        .write_all(&length.to_be_bytes())
        .await
        .expect("test agent frame length should be writable");
    stream
        .write_all(frame)
        .await
        .expect("test agent frame should be writable");
    stream.flush().await.expect("test agent frame should flush");
}

#[tokio::test]
async fn external_agent_signer_authenticates_against_a_trusted_server() {
    let mut rng = StdRng::seed_from_u64(108);
    let host_key = russh::keys::PrivateKey::random(&mut rng, russh::keys::Algorithm::Ed25519)
        .expect("test host key should be generated");
    let user_key = russh::keys::PrivateKey::random(&mut rng, russh::keys::Algorithm::Ed25519)
        .expect("test user key should be generated");
    let expected_fingerprint = host_key
        .public_key()
        .fingerprint(HashAlg::Sha256)
        .to_string();
    let user_public_key = user_key.public_key().clone();
    let expected_public_key = user_public_key.clone();
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
        let (stream, _) = listener
            .accept()
            .await
            .expect("test SSH connection should be accepted");
        let session = server::run_stream(
            server_config,
            stream,
            TestServer {
                expected_public_key: Some(expected_public_key),
                send_initial_prompt: false,
                x11_requests: None,
                pty_modes: None,
            },
        )
        .await
        .expect("test SSH session should start");
        let _ = session.await;
    });

    let mut profile = test_profile("agent-test", address.ip().to_string());
    {
        let ssh = profile.ssh_mut().expect("test profile should use SSH");
        ssh.port = address.port();
        ssh.auth = AuthMethod::SshAgent;
        ssh.host_key_fingerprint = Some(expected_fingerprint.clone());
    }
    let observation = FingerprintObservation::default();
    let mut handle = connect_transport(&profile, Some(expected_fingerprint), observation, None)
        .await
        .expect("trusted test transport should connect");
    let (agent_client_stream, mut agent_server_stream) = tokio::io::duplex(16 * 1024);
    let public_key_blob = user_public_key
        .to_bytes()
        .expect("test public key should encode");
    let expected_key_blob = public_key_blob.clone();
    let agent_task = tokio::spawn(async move {
        let identity_request = read_agent_frame(&mut agent_server_stream).await;
        assert_eq!(identity_request.as_slice(), [11]);
        let mut identity_response = vec![12];
        identity_response.extend_from_slice(&1_u32.to_be_bytes());
        append_agent_string(&mut identity_response, &public_key_blob);
        append_agent_string(&mut identity_response, b"test identity");
        write_agent_frame(&mut agent_server_stream, &identity_response).await;

        let sign_request = read_agent_frame(&mut agent_server_stream).await;
        assert_eq!(sign_request.first(), Some(&13));
        let mut offset = 1;
        let requested_key = read_agent_string(&sign_request, &mut offset);
        let signed_data = read_agent_string(&sign_request, &mut offset);
        let flags_end = offset.checked_add(4).expect("test agent flags should fit");
        let flags: [u8; 4] = sign_request
            .get(offset..flags_end)
            .expect("test agent flags should be present")
            .try_into()
            .expect("test agent flags should contain four bytes");
        assert_eq!(requested_key, expected_key_blob);
        assert_eq!(u32::from_be_bytes(flags), 0);

        let signature = russh::keys::signature::Signer::try_sign(user_key.key_data(), signed_data)
            .expect("test agent should sign the authentication request");
        let encoded_signature =
            Vec::<u8>::try_from(signature).expect("test agent signature should encode");
        let mut sign_response = vec![14];
        append_agent_string(&mut sign_response, &encoded_signature);
        write_agent_frame(&mut agent_server_stream, &sign_response).await;
    });
    let mut agent = AgentClient::connect(agent_client_stream);
    let identities = agent
        .request_identities()
        .await
        .expect("test agent should list its identity");

    assert!(
        authenticate_agent_identities(&mut handle, TEST_USER, identities, None, &mut agent)
            .await
            .expect("external signer authentication should complete")
    );
    agent_task
        .await
        .expect("test agent protocol task should complete");
    handle
        .disconnect(russh::Disconnect::ByApplication, "test complete", "")
        .await
        .expect("test transport should disconnect");
    server_task.abort();
    let _ = server_task.await;
}

#[derive(Clone)]
struct TestServer {
    expected_public_key: Option<PublicKey>,
    send_initial_prompt: bool,
    x11_requests: Option<mpsc::UnboundedSender<()>>,
    pty_modes: Option<mpsc::UnboundedSender<Vec<(russh::Pty, u32)>>>,
}

impl TestServer {
    fn password_only() -> Self {
        Self {
            expected_public_key: None,
            send_initial_prompt: true,
            x11_requests: None,
            pty_modes: None,
        }
    }

    fn silent_password_only_with_pty_modes(
        pty_modes: mpsc::UnboundedSender<Vec<(russh::Pty, u32)>>,
    ) -> Self {
        Self {
            expected_public_key: None,
            send_initial_prompt: false,
            x11_requests: None,
            pty_modes: Some(pty_modes),
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
        modes: &[(russh::Pty, u32)],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        if let Some(pty_modes) = &self.pty_modes {
            let _ = pty_modes.send(modes.to_vec());
        }
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
    let (pty_modes_tx, mut pty_modes_rx) = mpsc::unbounded_channel();
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
                TestServer::silent_password_only_with_pty_modes(pty_modes_tx.clone()),
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
            .expect("unknown host-key probe should return the rejected fingerprint")
            .fingerprint,
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
    let pty_modes = timeout(Duration::from_secs(2), pty_modes_rx.recv())
        .await
        .expect("interactive SSH shell should request PTY modes")
        .expect("PTY mode capture channel should remain open");
    assert!(pty_modes.contains(&(russh::Pty::OPOST, 1)));
    assert!(pty_modes.contains(&(russh::Pty::ONLCR, 1)));

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

#[tokio::test]
async fn commands_queued_during_authentication_do_not_cancel_the_worker() {
    let mut rng = StdRng::seed_from_u64(7);
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
            let session =
                server::run_stream(server_config.clone(), stream, TestServer::password_only())
                    .await
                    .expect("test SSH session should start");
            sessions.push(tokio::spawn(session));
        }
        sessions
    });

    let mut profile = test_profile("pre-auth-command", address.ip().to_string());
    profile.ssh_mut().expect("test profile should use SSH").port = address.port();
    profile
        .ssh_mut()
        .expect("test profile should use SSH")
        .host_key_fingerprint = Some(
        probe_host_key(&profile)
            .await
            .expect("unknown host-key probe should return the rejected fingerprint")
            .fingerprint,
    );

    let (worker, mut events) = SshSessionHandle::spawn(
        &Handle::current(),
        Uuid::new_v4(),
        profile,
        Zeroizing::new(TEST_PASSWORD.to_owned()),
        120,
        36,
    );
    worker
        .request_send(b"ignored-before-auth\r".to_vec())
        .expect("connecting worker should accept bounded terminal input");
    worker
        .request_list_sftp("/ignored-before-auth".to_owned())
        .expect("connecting worker should accept bounded SFTP input");

    let connected = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("pre-auth commands must not prevent SSH connection");
    assert_eq!(connected, Some(SshSessionEvent::Connected));

    worker
        .request_disconnect()
        .expect("connected worker should accept disconnect command");
    let disconnected = timeout(Duration::from_secs(2), async {
        loop {
            match events.recv().await {
                Some(SshSessionEvent::Disconnected) => break,
                Some(SshSessionEvent::Failed(message)) => {
                    panic!("SSH worker failed during disconnect: {message}")
                }
                Some(_) => {}
                None => panic!("SSH worker event stream closed before disconnect"),
            }
        }
    })
    .await;
    assert!(
        disconnected.is_ok(),
        "SSH worker should disconnect promptly"
    );
    worker
        .shutdown()
        .await
        .expect("SSH worker should join cleanly after disconnect");

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
        public_key: None,
    };
    assert!(unknown.to_string().contains("not trusted"));

    let changed = SshError::HostKeyRejected {
        expected: Some("SHA256:old".into()),
        actual: "SHA256:new".into(),
        public_key: None,
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
        .expect("unknown host-key probe should return the rejected fingerprint")
        .fingerprint;
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
                pty_modes: None,
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
        .expect("unknown host-key probe should return the rejected fingerprint")
        .fingerprint;
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
                    pty_modes: None,
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
        .expect("unknown host-key probe should return the rejected fingerprint")
        .fingerprint;
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
