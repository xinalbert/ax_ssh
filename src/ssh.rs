//! russh transport, trust policy, authentication, and worker lifetime.

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use russh::client;
use russh::keys::{PrivateKeyWithHashAlg, PublicKey};
use russh::{Channel, ChannelMsg, ChannelOpenFailure, ChannelStream};
use tokio::time::{Duration, timeout};
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

use crate::config::{AuthMethod, SessionProfile};

pub use self::private_keys::discover_private_keys;
pub use self::worker::{SshSessionEvent, SshSessionHandle};

mod private_keys;
mod worker;
mod x11;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const AUTH_TIMEOUT: Duration = Duration::from_secs(30);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(20);
const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(90);
const KEEPALIVE_MAX: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshEvent {
    Output(Vec<u8>),
    Disconnected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshError {
    AuthenticationFailed,
    PrivateKeyLoad(String),
    HostKeyRejected {
        expected: Option<String>,
        actual: String,
    },
}

impl std::fmt::Display for SshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthenticationFailed => write!(f, "SSH authentication failed"),
            Self::PrivateKeyLoad(message) => write!(f, "failed to load private key: {message}"),
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
    x11_dispatcher: Option<x11::X11Dispatcher>,
}

impl ClientHandler {
    fn new(
        expected_fingerprint: Option<String>,
        observation: FingerprintObservation,
        x11_dispatcher: Option<x11::X11Dispatcher>,
    ) -> Self {
        Self {
            expected_fingerprint,
            observation,
            x11_dispatcher,
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

    fn server_channel_open_x11(
        &mut self,
        channel: Channel<client::Msg>,
        _originator_address: &str,
        _originator_port: u32,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        let dispatcher = self.x11_dispatcher.clone();
        async move {
            match dispatcher {
                Some(dispatcher) => dispatcher.dispatch(channel, reply).await,
                None => {
                    reply
                        .reject(ChannelOpenFailure::AdministrativelyProhibited)
                        .await;
                }
            }
            Ok(())
        }
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
        // Interactive terminal input is usually only a few bytes. Send it
        // immediately instead of waiting for Nagle aggregation.
        nodelay: true,
        ..client::Config::default()
    })
}

async fn connect_transport(
    profile: &SessionProfile,
    expected_fingerprint: Option<String>,
    observation: FingerprintObservation,
    x11_dispatcher: Option<x11::X11Dispatcher>,
) -> Result<client::Handle<ClientHandler>> {
    let ssh = profile
        .ssh()
        .context("SSH transport requires an SSH session profile")?;
    let handler = ClientHandler::new(
        expected_fingerprint.clone(),
        observation.clone(),
        x11_dispatcher,
    );
    let result = timeout(
        CONNECT_TIMEOUT,
        client::connect(client_config(), (ssh.host.as_str(), ssh.port), handler),
    )
    .await
    .with_context(|| {
        format!(
            "timed out connecting to {}:{} during SSH key exchange",
            ssh.host, ssh.port
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
            Err(error).with_context(|| format!("failed to connect to {}:{}", ssh.host, ssh.port))
        }
    }
}

/// Reads a host fingerprint while still rejecting the untrusted transport.
pub async fn probe_host_key(profile: &SessionProfile) -> Result<String> {
    profile.validate()?;
    let observation = FingerprintObservation::default();
    let result = connect_transport(profile, None, observation.clone(), None).await;
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

#[derive(Clone)]
pub struct SshConnection {
    handle: Arc<client::Handle<ClientHandler>>,
}

impl SshConnection {
    #[cfg(test)]
    pub(crate) async fn connect(
        profile: &SessionProfile,
        secret: Zeroizing<String>,
    ) -> Result<Self> {
        Self::connect_with_x11(profile, secret, None).await
    }

    async fn connect_with_x11(
        profile: &SessionProfile,
        secret: Zeroizing<String>,
        x11_dispatcher: Option<x11::X11Dispatcher>,
    ) -> Result<Self> {
        profile.validate()?;
        let ssh = profile
            .ssh()
            .context("SSH connection requires an SSH session profile")?;
        info!(
            session_id = %profile.id,
            host = %ssh.host,
            port = ssh.port,
            "starting SSH connection"
        );

        let observation = FingerprintObservation::default();
        let mut handle = connect_transport(
            profile,
            ssh.host_key_fingerprint.clone(),
            observation,
            x11_dispatcher,
        )
        .await?;

        let authenticated = match &ssh.auth {
            AuthMethod::Password => timeout(
                AUTH_TIMEOUT,
                handle.authenticate_password(ssh.username.clone(), secret.as_str()),
            )
            .await
            .context("SSH password authentication timed out")?
            .context("SSH password authentication failed")?,
            AuthMethod::PrivateKey { path } => {
                let private_key = private_keys::load_private_key(path.clone(), secret)
                    .await
                    .map_err(|error| SshError::PrivateKeyLoad(error.to_string()))?;
                let hash_alg = timeout(AUTH_TIMEOUT, handle.best_supported_rsa_hash())
                    .await
                    .context("SSH private-key algorithm negotiation timed out")?
                    .context("SSH private-key algorithm negotiation failed")?
                    .flatten();
                timeout(
                    AUTH_TIMEOUT,
                    handle.authenticate_publickey(
                        ssh.username.clone(),
                        PrivateKeyWithHashAlg::new(Arc::new(private_key), hash_alg),
                    ),
                )
                .await
                .context("SSH private-key authentication timed out")?
                .context("SSH private-key authentication failed")?
            }
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
        Ok(Self {
            handle: Arc::new(handle),
        })
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

    async fn open_shell(
        &self,
        columns: u32,
        rows: u32,
        x11: Option<&x11::X11Forwarding>,
    ) -> Result<(SshShell, X11RequestStatus)> {
        let channel = self.handle.channel_open_session().await?;
        channel
            .request_pty(true, "xterm-256color", columns, rows, 0, 0, &[])
            .await?;
        let x11_status = match x11 {
            Some(x11) => {
                let fake_cookie = x11.fake_cookie_hex();
                match timeout(
                    CONNECT_TIMEOUT,
                    channel.request_x11(
                        true,
                        false,
                        x11::X11_AUTH_PROTOCOL,
                        fake_cookie.as_str(),
                        x11.screen(),
                    ),
                )
                .await
                {
                    Ok(Ok(())) => X11RequestStatus::Enabled,
                    Ok(Err(error)) => {
                        debug!(%error, "SSH server rejected the X11 forwarding request");
                        X11RequestStatus::Rejected
                    }
                    Err(_) => {
                        debug!("SSH X11 forwarding request timed out");
                        X11RequestStatus::Rejected
                    }
                }
            }
            None => X11RequestStatus::NotRequested,
        };
        channel.request_shell(true).await?;
        Ok((SshShell { channel }, x11_status))
    }

    pub(crate) async fn open_sftp_stream(&self) -> Result<ChannelStream<client::Msg>> {
        let channel = timeout(CONNECT_TIMEOUT, self.handle.channel_open_session())
            .await
            .context("timed out opening the SFTP SSH channel")?
            .context("failed to open SFTP SSH channel")?;
        timeout(CONNECT_TIMEOUT, channel.request_subsystem(true, "sftp"))
            .await
            .context("timed out requesting the SFTP subsystem")?
            .context("server rejected the SFTP subsystem")?;
        Ok(channel.into_stream())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum X11RequestStatus {
    NotRequested,
    Enabled,
    Rejected,
}

pub struct SshShell {
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
        Ok(())
    }

    pub async fn next_event(&mut self) -> Result<Option<SshEvent>> {
        loop {
            // Transport keepalive/inactivity owns liveness. A quiet interactive shell is valid.
            let Some(message) = self.channel.wait().await else {
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
mod tests;
