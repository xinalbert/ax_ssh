//! russh transport, trust policy, authentication, and worker lifetime.

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use russh::client;
use russh::keys::{PrivateKeyWithHashAlg, PublicKey};
use russh::{Channel, ChannelMsg};
use tokio::time::{Duration, timeout};
use tracing::{debug, info, warn};

use crate::config::{AuthMethod, SessionProfile};

pub use self::private_keys::discover_private_keys;
pub use self::worker::{SshSessionEvent, SshSessionHandle};

mod private_keys;
mod worker;

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
                        profile.username.clone(),
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
mod tests;
