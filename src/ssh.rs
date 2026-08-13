//! russh transport, trust policy, authentication, and worker lifetime.

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use russh::client;
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::{AgentClient, AgentStream};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKey};
use russh::{Channel, ChannelMsg, ChannelOpenFailure, ChannelStream};
use tokio::net::TcpStream;
use tokio::time::{Duration, timeout};
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

use crate::config::{AuthMethod, SessionProfile};

pub use self::known_hosts::TrustDecision;
pub use self::private_keys::discover_private_keys;
pub use self::worker::{SshSessionEvent, SshSessionHandle};

pub fn append_confirmed_known_host(host: &str, port: u16, public_key: &str) -> Result<()> {
    known_hosts::Snapshot::append_confirmed_openssh(
        known_hosts::default_path()?,
        host,
        port,
        public_key,
    )
}

pub fn replace_confirmed_known_host(host: &str, port: u16, public_key: &str) -> Result<()> {
    known_hosts::Snapshot::replace_confirmed_openssh(
        known_hosts::default_path()?,
        host,
        port,
        public_key,
    )
}

pub fn remove_known_host(host: &str, port: u16, fingerprint: &str) -> Result<bool> {
    known_hosts::Snapshot::remove_key(known_hosts::default_path()?, host, port, fingerprint)
}

mod known_hosts;
mod private_keys;
mod worker;
mod x11;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const AUTH_TIMEOUT: Duration = Duration::from_secs(30);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(20);
const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(90);
const KEEPALIVE_MAX: usize = 3;
const MAX_SSH_AGENT_IDENTITIES: usize = 5;

type RuntimeAgentClient = AgentClient<Box<dyn AgentStream + Send + Unpin>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshEvent {
    Output(Vec<u8>),
    Disconnected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshError {
    AuthenticationFailed,
    PrivateKeyLoad(String),
    SshAgentUnavailable,
    SshAgentTimedOut,
    SshAgentNoIdentities,
    SshAgentOperationFailed,
    HostKeyRejected {
        expected: Option<String>,
        actual: String,
        public_key: Option<String>,
    },
    HostKeyRevoked {
        actual: String,
        public_key: Option<String>,
    },
}

impl std::fmt::Display for SshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthenticationFailed => write!(f, "SSH authentication failed"),
            Self::PrivateKeyLoad(message) => write!(f, "failed to load private key: {message}"),
            Self::SshAgentUnavailable => write!(f, "SSH agent is unavailable"),
            Self::SshAgentTimedOut => write!(f, "SSH agent authentication timed out"),
            Self::SshAgentNoIdentities => write!(f, "SSH agent has no identities"),
            Self::SshAgentOperationFailed => {
                write!(f, "SSH agent could not complete authentication")
            }
            Self::HostKeyRejected {
                expected: Some(expected),
                actual,
                ..
            } => write!(
                f,
                "SSH host key mismatch: expected {expected}, received {actual}"
            ),
            Self::HostKeyRejected {
                expected: None,
                actual,
                ..
            } => write!(f, "SSH host key is not trusted: received {actual}"),
            Self::HostKeyRevoked { actual, .. } => {
                write!(f, "SSH host key is revoked: received {actual}")
            }
        }
    }
}

impl std::error::Error for SshError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostKeyObservationDecision {
    Unknown,
    Trusted,
    Changed,
    Revoked,
}

#[derive(Clone, Default)]
struct FingerprintObservation {
    fingerprint: Arc<Mutex<Option<String>>>,
    public_key: Arc<Mutex<Option<String>>>,
    decision: Arc<Mutex<Option<HostKeyObservationDecision>>>,
    accepted: Arc<Mutex<Option<bool>>>,
}

impl FingerprintObservation {
    fn record(&self, fingerprint: String) -> Result<()> {
        let mut observed = self
            .fingerprint
            .lock()
            .map_err(|_| anyhow::anyhow!("host-key observation lock poisoned"))?;
        *observed = Some(fingerprint);
        Ok(())
    }

    fn record_public_key(&self, key: String) -> Result<()> {
        let mut observed = self
            .public_key
            .lock()
            .map_err(|_| anyhow::anyhow!("host-key observation lock poisoned"))?;
        *observed = Some(key);
        Ok(())
    }

    fn get_public_key(&self) -> Result<Option<String>> {
        self.public_key
            .lock()
            .map(|key| key.clone())
            .map_err(|_| anyhow::anyhow!("host-key observation lock poisoned"))
    }

    fn record_decision(&self, decision: HostKeyObservationDecision) -> Result<()> {
        let mut observed = self
            .decision
            .lock()
            .map_err(|_| anyhow::anyhow!("host-key observation lock poisoned"))?;
        *observed = Some(decision);
        Ok(())
    }

    fn get_decision(&self) -> Result<Option<HostKeyObservationDecision>> {
        self.decision
            .lock()
            .map(|decision| *decision)
            .map_err(|_| anyhow::anyhow!("host-key observation lock poisoned"))
    }

    fn record_accepted(&self, accepted: bool) -> Result<()> {
        let mut observed = self
            .accepted
            .lock()
            .map_err(|_| anyhow::anyhow!("host-key observation lock poisoned"))?;
        *observed = Some(accepted);
        Ok(())
    }

    fn was_accepted(&self) -> Result<Option<bool>> {
        self.accepted
            .lock()
            .map(|accepted| *accepted)
            .map_err(|_| anyhow::anyhow!("host-key observation lock poisoned"))
    }

    fn get(&self) -> Result<Option<String>> {
        self.fingerprint
            .lock()
            .map(|observed| observed.clone())
            .map_err(|_| anyhow::anyhow!("host-key observation lock poisoned"))
    }
}

struct ClientHandler {
    expected_fingerprint: Option<String>,
    known_hosts: known_hosts::Snapshot,
    host: String,
    port: u16,
    observation: FingerprintObservation,
    x11_dispatcher: Option<x11::X11Dispatcher>,
}

impl ClientHandler {
    fn new(
        expected_fingerprint: Option<String>,
        known_hosts: known_hosts::Snapshot,
        host: String,
        port: u16,
        observation: FingerprintObservation,
        x11_dispatcher: Option<x11::X11Dispatcher>,
    ) -> Self {
        Self {
            expected_fingerprint,
            known_hosts,
            host,
            port,
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
        self.observation
            .record_public_key(server_public_key.to_openssh()?)?;
        let decision = self
            .known_hosts
            .decision(&self.host, self.port, server_public_key);
        self.observation.record_decision(match decision {
            known_hosts::TrustDecision::Unknown => HostKeyObservationDecision::Unknown,
            known_hosts::TrustDecision::Trusted => HostKeyObservationDecision::Trusted,
            known_hosts::TrustDecision::Changed => HostKeyObservationDecision::Changed,
            known_hosts::TrustDecision::Revoked => HostKeyObservationDecision::Revoked,
        })?;
        if decision == known_hosts::TrustDecision::Revoked {
            self.observation.record_accepted(false)?;
            warn!(fingerprint = %actual, "SSH host key is revoked");
            return Ok(false);
        }
        let expected_matches =
            fingerprint_is_trusted(self.expected_fingerprint.as_deref(), &actual);
        if self.expected_fingerprint.is_some() {
            let accepted = expected_matches && decision != known_hosts::TrustDecision::Changed;
            self.observation.record_accepted(accepted)?;
            return Ok(accepted);
        }
        let trusted = matches!(decision, known_hosts::TrustDecision::Trusted);
        self.observation.record_accepted(trusted)?;
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
    let known_hosts = match known_hosts::Snapshot::load_default() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            warn!(%error, "unable to read system known_hosts; retaining profile-only trust");
            known_hosts::Snapshot::default()
        }
    };
    let handler = ClientHandler::new(
        expected_fingerprint.clone(),
        known_hosts.clone(),
        ssh.host.clone(),
        ssh.port,
        observation.clone(),
        x11_dispatcher,
    );
    // Keep TCP setup separate from russh's banner/KEX handshake.  Apart from
    // making Windows failures actionable, this prevents a refused or filtered
    // socket from being reported as if the server had rejected its host key.
    let socket = timeout(
        CONNECT_TIMEOUT,
        TcpStream::connect((ssh.host.as_str(), ssh.port)),
    )
    .await
    .with_context(|| format!("timed out connecting to {}:{}", ssh.host, ssh.port))?
    .with_context(|| format!("failed to connect to {}:{}", ssh.host, ssh.port))?;
    let config = client_config();
    if config.nodelay && socket.set_nodelay(true).is_err() {
        debug!(host = %ssh.host, port = ssh.port, "failed to enable TCP_NODELAY for SSH socket");
    }
    let result = timeout(
        CONNECT_TIMEOUT,
        client::connect_stream(config, socket, handler),
    )
    .await
    .with_context(|| {
        format!(
            "timed out during SSH banner or key exchange with {}:{}",
            ssh.host, ssh.port
        )
    })?;

    match result {
        Ok(handle) => Ok(handle),
        Err(error) => {
            debug!(
                host = %ssh.host,
                port = ssh.port,
                observed_fingerprint = ?observation.get()?,
                observed_decision = ?observation.get_decision()?,
                host_key_accepted = ?observation.was_accepted()?,
                error = ?error,
                "SSH transport handshake failed"
            );
            if let Some(actual) = observation.get()?
                && matches!(observation.was_accepted()?, Some(false))
            {
                if observation.get_decision()? == Some(HostKeyObservationDecision::Revoked) {
                    return Err(SshError::HostKeyRevoked {
                        actual,
                        public_key: observation.get_public_key()?,
                    }
                    .into());
                }
                return Err(SshError::HostKeyRejected {
                    expected: expected_fingerprint,
                    actual,
                    public_key: observation.get_public_key()?,
                }
                .into());
            }
            let phase = if observation.get()?.is_some() {
                "after the server host-key check"
            } else {
                "before the server presented a host key"
            };
            Err(error).with_context(|| {
                format!(
                    "SSH banner or key exchange with {}:{} ended {phase}",
                    ssh.host, ssh.port
                )
            })
        }
    }
}

#[cfg(unix)]
async fn connect_runtime_agent() -> std::result::Result<RuntimeAgentClient, SshError> {
    AgentClient::connect_env()
        .await
        .map(|agent| agent.dynamic())
        .map_err(|_| SshError::SshAgentUnavailable)
}

#[cfg(windows)]
async fn connect_runtime_agent() -> std::result::Result<RuntimeAgentClient, SshError> {
    const OPENSSH_AGENT_PIPE: &str = r"\\.\pipe\openssh-ssh-agent";

    let pipe = std::env::var_os("SSH_AUTH_SOCK")
        .unwrap_or_else(|| std::ffi::OsString::from(OPENSSH_AGENT_PIPE));
    AgentClient::connect_named_pipe(pipe)
        .await
        .map(|agent| agent.dynamic())
        .map_err(|_| SshError::SshAgentUnavailable)
}

#[cfg(not(any(unix, windows)))]
async fn connect_runtime_agent() -> std::result::Result<RuntimeAgentClient, SshError> {
    Err(SshError::SshAgentUnavailable)
}

const fn ssh_agent_attempt_count(identity_count: usize) -> usize {
    if identity_count < MAX_SSH_AGENT_IDENTITIES {
        identity_count
    } else {
        MAX_SSH_AGENT_IDENTITIES
    }
}

async fn authenticate_agent_identities<S>(
    handle: &mut client::Handle<ClientHandler>,
    username: &str,
    identities: Vec<AgentIdentity>,
    hash_alg: Option<HashAlg>,
    signer: &mut AgentClient<S>,
) -> std::result::Result<bool, SshError>
where
    S: AgentStream + Send + Unpin,
{
    let attempt_count = ssh_agent_attempt_count(identities.len());
    debug!(
        identity_count = identities.len(),
        attempt_count, "attempting bounded SSH agent authentication"
    );
    for identity in identities.into_iter().take(attempt_count) {
        let result = match identity {
            AgentIdentity::PublicKey { key, .. } => {
                handle
                    .authenticate_publickey_with(username, key, hash_alg, signer)
                    .await
            }
            AgentIdentity::Certificate { certificate, .. } => {
                handle
                    .authenticate_certificate_with(username, certificate, hash_alg, signer)
                    .await
            }
        }
        .map_err(|_| SshError::SshAgentOperationFailed)?;
        if result.success() {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn authenticate_with_runtime_agent(
    handle: &mut client::Handle<ClientHandler>,
    username: &str,
) -> std::result::Result<bool, SshError> {
    timeout(AUTH_TIMEOUT, async {
        let mut agent = connect_runtime_agent().await?;
        let identities = agent
            .request_identities()
            .await
            .map_err(|_| SshError::SshAgentOperationFailed)?;
        if identities.is_empty() {
            return Err(SshError::SshAgentNoIdentities);
        }

        let hash_alg = handle
            .best_supported_rsa_hash()
            .await
            .map_err(|_| SshError::SshAgentOperationFailed)?
            .flatten();
        authenticate_agent_identities(handle, username, identities, hash_alg, &mut agent).await
    })
    .await
    .map_err(|_| SshError::SshAgentTimedOut)?
}

/// Reads a host fingerprint while still rejecting the untrusted transport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostKeyProbe {
    pub fingerprint: String,
    pub decision: TrustDecision,
    pub public_key: Option<String>,
}

pub async fn probe_host_key(profile: &SessionProfile) -> Result<HostKeyProbe> {
    profile.validate()?;
    let observation = FingerprintObservation::default();
    let result = connect_transport(profile, None, observation.clone(), None).await;
    let observed = observation.get()?;

    match (result, observed) {
        (Err(error), Some(fingerprint)) => {
            let decision = match observation.get_decision()? {
                Some(HostKeyObservationDecision::Trusted) => TrustDecision::Trusted,
                Some(HostKeyObservationDecision::Changed) => TrustDecision::Changed,
                Some(HostKeyObservationDecision::Revoked) => TrustDecision::Revoked,
                _ => TrustDecision::Unknown,
            };
            match observation.was_accepted()? {
                Some(false) => {}
                Some(true) => {
                    return Err(error)
                        .context("SSH handshake failed after the server host key was accepted");
                }
                None => {
                    return Err(error)
                        .context("SSH host-key decision failed after observing the server key");
                }
            }
            Ok(HostKeyProbe {
                fingerprint,
                decision,
                public_key: observation.get_public_key()?,
            })
        }
        (Err(error), None) => Err(error),
        (Ok(handle), Some(fingerprint)) => {
            let decision = match observation.get_decision()? {
                Some(HostKeyObservationDecision::Trusted) => TrustDecision::Trusted,
                Some(HostKeyObservationDecision::Changed) => TrustDecision::Changed,
                Some(HostKeyObservationDecision::Revoked) => TrustDecision::Revoked,
                _ => TrustDecision::Unknown,
            };
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
            Ok(HostKeyProbe {
                fingerprint,
                decision,
                public_key: observation.get_public_key()?,
            })
        }
        (Ok(handle), None) => {
            let _ = handle
                .disconnect(
                    russh::Disconnect::ByApplication,
                    "AxSSH host-key probe completed",
                    "",
                )
                .await;
            anyhow::bail!("host-key probe completed without observing a server key")
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
            .context("SSH password authentication failed")?
            .success(),
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
                .success()
            }
            AuthMethod::SshAgent => {
                authenticate_with_runtime_agent(&mut handle, &ssh.username).await?
            }
        };
        if !authenticated {
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
