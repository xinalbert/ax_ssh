//! russh transport, trust policy, authentication, and worker lifetime.

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use russh::client;
use russh::keys::PublicKey;
use russh::{Channel, ChannelMsg};
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Duration, MissedTickBehavior, interval, timeout};
use tracing::{debug, info, warn};

use crate::config::{AuthMethod, SessionProfile};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const AUTH_TIMEOUT: Duration = Duration::from_secs(30);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(20);
const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(90);
const KEEPALIVE_MAX: usize = 3;
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(7);
const COMMAND_CAPACITY: usize = 8;
const EVENT_CAPACITY: usize = 8;
const MAX_ERROR_CHARS: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshEvent {
    Output(Vec<u8>),
    Disconnected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshSessionEvent {
    Connected,
    Disconnected,
    HostKeyRejected {
        expected: Option<String>,
        actual: String,
    },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshError {
    AuthenticationFailed,
    UnsupportedAuth,
    HostKeyRejected {
        expected: Option<String>,
        actual: String,
    },
}

impl std::fmt::Display for SshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthenticationFailed => write!(f, "SSH authentication failed"),
            Self::UnsupportedAuth => write!(f, "this authentication method is not implemented yet"),
            Self::HostKeyRejected {
                expected: Some(expected),
                actual,
            } => write!(
                f,
                "SSH host key mismatch: expected {expected}, received {actual}"
            ),
            Self::HostKeyRejected {
                expected: None,
                actual,
            } => write!(f, "SSH host key is not trusted: received {actual}"),
        }
    }
}

impl std::error::Error for SshError {}

#[derive(Clone, Default)]
struct FingerprintObservation(Arc<Mutex<Option<String>>>);

impl FingerprintObservation {
    fn record(&self, fingerprint: String) -> Result<()> {
        let mut observed = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("host-key observation lock poisoned"))?;
        *observed = Some(fingerprint);
        Ok(())
    }

    fn get(&self) -> Result<Option<String>> {
        self.0
            .lock()
            .map(|observed| observed.clone())
            .map_err(|_| anyhow::anyhow!("host-key observation lock poisoned"))
    }
}

struct ClientHandler {
    expected_fingerprint: Option<String>,
    observation: FingerprintObservation,
}

impl ClientHandler {
    fn new(expected_fingerprint: Option<String>, observation: FingerprintObservation) -> Self {
        Self {
            expected_fingerprint,
            observation,
        }
    }
}

impl client::Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        let actual = server_public_key
            .fingerprint(russh::keys::HashAlg::Sha256)
            .to_string();
        self.observation.record(actual.clone())?;
        let trusted = fingerprint_is_trusted(self.expected_fingerprint.as_deref(), &actual);
        if trusted {
            debug!(fingerprint = %actual, "SSH host key accepted");
        } else {
            warn!(
                expected = ?self.expected_fingerprint,
                fingerprint = %actual,
                "SSH host key rejected before authentication"
            );
        }
        Ok(trusted)
    }
}

fn fingerprint_is_trusted(expected: Option<&str>, actual: &str) -> bool {
    expected.is_some_and(|expected| expected == actual)
}

fn client_config() -> Arc<client::Config> {
    Arc::new(client::Config {
        inactivity_timeout: Some(INACTIVITY_TIMEOUT),
        keepalive_interval: Some(KEEPALIVE_INTERVAL),
        keepalive_max: KEEPALIVE_MAX,
        ..client::Config::default()
    })
}

async fn connect_transport(
    profile: &SessionProfile,
    expected_fingerprint: Option<String>,
    observation: FingerprintObservation,
) -> Result<client::Handle<ClientHandler>> {
    let handler = ClientHandler::new(expected_fingerprint.clone(), observation.clone());
    let result = timeout(
        CONNECT_TIMEOUT,
        client::connect(
            client_config(),
            (profile.host.as_str(), profile.port),
            handler,
        ),
    )
    .await
    .with_context(|| {
        format!(
            "timed out connecting to {}:{} during SSH key exchange",
            profile.host, profile.port
        )
    })?;

    match result {
        Ok(handle) => Ok(handle),
        Err(error) => {
            if let Some(actual) = observation.get()?
                && !fingerprint_is_trusted(expected_fingerprint.as_deref(), &actual)
            {
                return Err(SshError::HostKeyRejected {
                    expected: expected_fingerprint,
                    actual,
                }
                .into());
            }
            Err(error)
                .with_context(|| format!("failed to connect to {}:{}", profile.host, profile.port))
        }
    }
}

/// Reads a host fingerprint while still rejecting the untrusted transport.
pub async fn probe_host_key(profile: &SessionProfile) -> Result<String> {
    profile.validate()?;
    let observation = FingerprintObservation::default();
    let result = connect_transport(profile, None, observation.clone()).await;
    let observed = observation.get()?;

    match (result, observed) {
        (Err(_), Some(fingerprint)) => Ok(fingerprint),
        (Err(error), None) => Err(error).context("failed before the server host key was available"),
        (Ok(handle), _) => {
            if let Err(error) = handle
                .disconnect(
                    russh::Disconnect::ByApplication,
                    "AxSSH host-key probe completed",
                    "",
                )
                .await
            {
                warn!(%error, "failed to close unexpected host-key probe transport");
            }
            anyhow::bail!("host-key probe unexpectedly accepted an untrusted server")
        }
    }
}

pub struct SshConnection {
    handle: client::Handle<ClientHandler>,
}

impl SshConnection {
    pub async fn connect(profile: &SessionProfile, secret: String) -> Result<Self> {
        profile.validate()?;
        info!(
            session_id = %profile.id,
            host = %profile.host,
            port = profile.port,
            "starting SSH connection"
        );

        let observation = FingerprintObservation::default();
        let mut handle =
            connect_transport(profile, profile.host_key_fingerprint.clone(), observation).await?;

        let authenticated = match &profile.auth {
            AuthMethod::Password => timeout(
                AUTH_TIMEOUT,
                handle.authenticate_password(profile.username.clone(), secret),
            )
            .await
            .context("SSH password authentication timed out")?
            .context("SSH password authentication failed")?,
            AuthMethod::PrivateKey { .. } => return Err(SshError::UnsupportedAuth.into()),
        };
        if !authenticated.success() {
            if let Err(error) = handle
                .disconnect(
                    russh::Disconnect::NoMoreAuthMethodsAvailable,
                    "AxSSH authentication failed",
                    "",
                )
                .await
            {
                warn!(%error, "failed to close transport after authentication failure");
            }
            return Err(SshError::AuthenticationFailed.into());
        }

        info!(session_id = %profile.id, "SSH authentication succeeded");
        Ok(Self { handle })
    }

    pub fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.handle
            .disconnect(russh::Disconnect::ByApplication, "AxSSH session closed", "")
            .await
            .context("failed to send SSH disconnect")
    }

    pub async fn open_shell(self, columns: u32, rows: u32) -> Result<SshShell> {
        let channel = self.handle.channel_open_session().await?;
        channel
            .request_pty(true, "xterm-256color", columns, rows, 0, 0, &[])
            .await?;
        channel.request_shell(true).await?;
        Ok(SshShell {
            handle: self.handle,
            channel,
        })
    }
}

enum SshCommand {
    Disconnect,
}

/// UI-adjacent controller for one worker-owned SSH connection.
pub struct SshSessionHandle {
    command_tx: mpsc::Sender<SshCommand>,
    task: JoinHandle<()>,
}

impl SshSessionHandle {
    pub fn spawn(
        runtime: &Handle,
        profile: SessionProfile,
        secret: String,
    ) -> (Self, mpsc::Receiver<SshSessionEvent>) {
        let (command_tx, command_rx) = mpsc::channel(COMMAND_CAPACITY);
        let (event_tx, event_rx) = mpsc::channel(EVENT_CAPACITY);
        let task = runtime.spawn(run_session(profile, secret, command_rx, event_tx));
        (Self { command_tx, task }, event_rx)
    }

    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }

    pub fn request_disconnect(&self) -> Result<()> {
        self.command_tx
            .try_send(SshCommand::Disconnect)
            .map_err(|error| anyhow::anyhow!("cannot request SSH disconnect: {error}"))
    }

    pub async fn shutdown(mut self) -> Result<()> {
        if !self.task.is_finished() && self.command_tx.send(SshCommand::Disconnect).await.is_err() {
            debug!("SSH worker command receiver already closed during shutdown");
        }

        match timeout(WORKER_SHUTDOWN_TIMEOUT, &mut self.task).await {
            Ok(joined) => joined.context("SSH worker task failed during shutdown"),
            Err(_) => {
                self.task.abort();
                match self.task.await {
                    Err(error) if error.is_cancelled() => {
                        warn!("SSH worker exceeded shutdown timeout and was aborted");
                        Ok(())
                    }
                    Err(error) => Err(error).context("failed to abort SSH worker task"),
                    Ok(()) => Ok(()),
                }
            }
        }
    }
}

async fn run_session(
    profile: SessionProfile,
    secret: String,
    mut command_rx: mpsc::Receiver<SshCommand>,
    event_tx: mpsc::Sender<SshSessionEvent>,
) {
    let session_id = profile.id;
    let connect = SshConnection::connect(&profile, secret);
    tokio::pin!(connect);
    let connection_result = tokio::select! {
        result = &mut connect => Some(result),
        command = command_rx.recv() => {
            match command {
                Some(SshCommand::Disconnect) => {
                    info!(session_id = %session_id, "SSH connection attempt cancelled");
                }
                None => {
                    info!(session_id = %session_id, "SSH controller dropped during connection attempt");
                }
            }
            None
        }
    };
    let Some(connection_result) = connection_result else {
        send_event(&event_tx, SshSessionEvent::Disconnected, session_id).await;
        return;
    };
    let connection = match connection_result {
        Ok(connection) => connection,
        Err(error) => {
            if let Some(SshError::HostKeyRejected { expected, actual }) =
                error.downcast_ref::<SshError>()
            {
                send_event(
                    &event_tx,
                    SshSessionEvent::HostKeyRejected {
                        expected: expected.clone(),
                        actual: actual.clone(),
                    },
                    session_id,
                )
                .await;
            } else {
                warn!(session_id = %session_id, %error, "SSH worker failed to connect");
                send_event(
                    &event_tx,
                    SshSessionEvent::Failed(bounded_error_message(&error)),
                    session_id,
                )
                .await;
            }
            return;
        }
    };

    if !send_event(&event_tx, SshSessionEvent::Connected, session_id).await {
        close_connection(&connection, session_id).await;
        return;
    }

    let mut health_check = interval(Duration::from_secs(1));
    health_check.set_missed_tick_behavior(MissedTickBehavior::Skip);
    health_check.tick().await;
    loop {
        tokio::select! {
            command = command_rx.recv() => {
                match command {
                    Some(SshCommand::Disconnect) => {
                        info!(session_id = %session_id, "SSH disconnect requested");
                    }
                    None => {
                        info!(session_id = %session_id, "SSH controller dropped; disconnecting worker");
                    }
                }
                close_connection(&connection, session_id).await;
                break;
            }
            _ = health_check.tick() => {
                if connection.is_closed() {
                    info!(session_id = %session_id, "SSH transport closed by remote peer");
                    break;
                }
            }
        }
    }

    send_event(&event_tx, SshSessionEvent::Disconnected, session_id).await;
}

async fn close_connection(connection: &SshConnection, session_id: uuid::Uuid) {
    match timeout(DISCONNECT_TIMEOUT, connection.disconnect()).await {
        Ok(Ok(())) => info!(session_id = %session_id, "SSH disconnect sent"),
        Ok(Err(error)) => warn!(session_id = %session_id, %error, "SSH disconnect failed"),
        Err(_) => warn!(session_id = %session_id, "SSH disconnect timed out"),
    }
}

async fn send_event(
    event_tx: &mpsc::Sender<SshSessionEvent>,
    event: SshSessionEvent,
    session_id: uuid::Uuid,
) -> bool {
    if event_tx.send(event).await.is_err() {
        debug!(session_id = %session_id, "SSH event receiver dropped");
        false
    } else {
        true
    }
}

fn bounded_error_message(error: &anyhow::Error) -> String {
    let message = format!("{error:#}");
    let mut chars = message.chars();
    let mut bounded = chars.by_ref().take(MAX_ERROR_CHARS).collect::<String>();
    if chars.next().is_some() {
        bounded.push_str("...");
    }
    bounded
}

pub struct SshShell {
    handle: client::Handle<ClientHandler>,
    channel: Channel<russh::client::Msg>,
}

impl SshShell {
    pub async fn send(&self, data: impl Into<Vec<u8>>) -> Result<()> {
        self.channel.data_bytes(data.into()).await?;
        Ok(())
    }

    pub async fn resize(&self, columns: u32, rows: u32) -> Result<()> {
        self.channel.window_change(columns, rows, 0, 0).await?;
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.channel.close().await?;
        self.handle
            .disconnect(russh::Disconnect::ByApplication, "AxSSH session closed", "")
            .await?;
        Ok(())
    }

    pub async fn next_event(&mut self) -> Result<Option<SshEvent>> {
        loop {
            let Some(message) = timeout(Duration::from_secs(30), self.channel.wait()).await? else {
                return Ok(None);
            };
            match message {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                    return Ok(Some(SshEvent::Output(data.to_vec())));
                }
                ChannelMsg::Eof | ChannelMsg::Close => {
                    return Ok(Some(SshEvent::Disconnected));
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng as _;
    use rand::rngs::StdRng;
    use russh::server::{self, Auth};
    use tokio::net::TcpListener;

    const TEST_USER: &str = "ax-test-user";
    const TEST_PASSWORD: &str = "ax-test-password";

    #[derive(Clone, Copy)]
    struct PasswordServer;

    impl server::Handler for PasswordServer {
        type Error = russh::Error;

        async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
            if user == TEST_USER && password == TEST_PASSWORD {
                Ok(Auth::Accept)
            } else {
                Ok(Auth::reject())
            }
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
            for _ in 0..3 {
                let (stream, _) = listener
                    .accept()
                    .await
                    .expect("test SSH connection should be accepted");
                let session = server::run_stream(server_config.clone(), stream, PasswordServer)
                    .await
                    .expect("test SSH session should start");
                sessions.push(tokio::spawn(session));
            }
            sessions
        });

        let mut profile = SessionProfile::new("test", address.ip().to_string(), TEST_USER);
        profile.port = address.port();
        let fingerprint = probe_host_key(&profile)
            .await
            .expect("unknown host-key probe should return the rejected fingerprint");
        assert_eq!(fingerprint, expected_fingerprint);

        profile.host_key_fingerprint = Some(fingerprint);
        let connection = SshConnection::connect(&profile, TEST_PASSWORD.to_owned())
            .await
            .expect("trusted host with valid password should authenticate");
        assert!(!connection.is_closed());
        connection
            .disconnect()
            .await
            .expect("authenticated test connection should disconnect");

        let (worker, mut events) =
            SshSessionHandle::spawn(&Handle::current(), profile, TEST_PASSWORD.to_owned());
        let connected = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("SSH worker should report connection promptly");
        assert_eq!(connected, Some(SshSessionEvent::Connected));
        worker
            .request_disconnect()
            .expect("SSH worker should accept disconnect command");
        let disconnected = timeout(Duration::from_secs(2), events.recv())
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
}
