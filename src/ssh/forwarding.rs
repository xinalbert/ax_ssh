//! Session-owned port forwarding over an authenticated SSH transport.

mod socks;

#[cfg(unix)]
mod unix;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use russh::{Channel, ChannelOpenFailure, client};
use tokio::io::{AsyncRead, AsyncWrite, copy_bidirectional};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinSet;
use tokio::time::{Duration, timeout};

use super::SshConnection;

const MAX_RELAYS: usize = 16;
const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(4);
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const LOOPBACK: &str = "127.0.0.1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForwardListener {
    Tcp(u16),
    UnixSocket(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForwardTarget {
    Tcp { host: String, port: u16 },
    UnixSocket(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForwardRule {
    Local {
        listen: ForwardListener,
        target: ForwardTarget,
    },
    Dynamic {
        port: u16,
    },
    Remote {
        listen: ForwardListener,
        target: ForwardTarget,
    },
}

impl ForwardRule {
    pub fn validate(&self) -> Result<()> {
        let (listen, target, local_listener) = match self {
            Self::Local { listen, target } => (listen, target, true),
            Self::Remote { listen, target } => (listen, target, false),
            Self::Dynamic { .. } => return Ok(()),
        };
        validate_target(target, !local_listener)?;
        if let ForwardListener::UnixSocket(path) = listen {
            validate_socket(path, local_listener)?;
        }
        Ok(())
    }
}

pub(super) enum ForwardState {
    Starting,
    Active(ForwardRule),
    Stopped,
}

type Incoming = (Channel<client::Msg>, client::ChannelOpenHandle);

struct RemoteRoute {
    listen: ForwardListener,
    incoming: mpsc::Sender<Incoming>,
}

#[derive(Clone, Default)]
pub(super) struct RemoteDispatcher(Arc<Mutex<Vec<RemoteRoute>>>);

struct Registration {
    dispatcher: RemoteDispatcher,
    listen: ForwardListener,
}

impl RemoteDispatcher {
    pub(super) async fn incoming_tcp(
        &self,
        address: &str,
        port: u32,
        origin: &str,
        channel: Channel<client::Msg>,
        reply: client::ChannelOpenHandle,
    ) {
        // A server policy can widen the requested bind address. Refuse
        // non-loopback peer addresses reported by the verified SSH server.
        if address == LOOPBACK
            && origin
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.to_canonical().is_loopback())
            && let Ok(port) = u16::try_from(port)
        {
            self.incoming(ForwardListener::Tcp(port), channel, reply)
                .await;
        } else {
            reply
                .reject(ChannelOpenFailure::AdministrativelyProhibited)
                .await;
        }
    }

    pub(super) async fn incoming(
        &self,
        listen: ForwardListener,
        channel: Channel<client::Msg>,
        reply: client::ChannelOpenHandle,
    ) {
        let matched = self.0.lock().ok().and_then(|routes| {
            routes
                .iter()
                .find(|route| route.listen == listen)
                .map(|route| route.incoming.clone())
        });
        if let Some(sender) = matched {
            if sender.try_send((channel, reply)).is_err() {
                tracing::debug!("remote forwarding channel queue unavailable");
            }
        } else {
            reply
                .reject(ChannelOpenFailure::AdministrativelyProhibited)
                .await;
        }
    }

    fn register(
        &self,
        listen: ForwardListener,
        incoming: mpsc::Sender<Incoming>,
    ) -> Result<Registration> {
        let mut routes = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("remote forwarding state unavailable"))?;
        anyhow::ensure!(
            !routes.iter().any(|route| route.listen == listen),
            "a remote forward is already registered for this listener"
        );
        routes.push(RemoteRoute {
            listen: listen.clone(),
            incoming,
        });
        Ok(Registration {
            dispatcher: self.clone(),
            listen,
        })
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        match self.dispatcher.0.lock() {
            Ok(mut routes) => routes.retain(|route| route.listen != self.listen),
            Err(_) => tracing::warn!("remote forwarding state is poisoned"),
        }
    }
}

pub(super) async fn run(
    connection: SshConnection,
    dispatcher: RemoteDispatcher,
    rule: ForwardRule,
    state: watch::Sender<ForwardState>,
    mut stop: oneshot::Receiver<()>,
    started: impl FnOnce(Result<ForwardRule>) + Send,
) -> Result<()> {
    let failure = match rule {
        ForwardRule::Remote { listen, target } => {
            match prepare_remote(&connection, &dispatcher, listen, &target).await {
                Ok((listen, registration, incoming)) => {
                    publish(
                        &state,
                        started,
                        ForwardRule::Remote {
                            listen: listen.clone(),
                            target: target.clone(),
                        },
                    );
                    let mut relays = JoinSet::new();
                    relay_remote(incoming, target, &mut stop, &mut relays).await;
                    drop(registration);
                    state.send_replace(ForwardState::Stopped);
                    relays.shutdown().await;
                    return cancel_remote(&connection, listen).await;
                }
                Err(error) => error,
            }
        }
        ForwardRule::Local { listen, target } => match prepare_local(listen, Some(&target)).await {
            Ok((listen, listener)) => {
                publish(
                    &state,
                    started,
                    ForwardRule::Local {
                        listen,
                        target: target.clone(),
                    },
                );
                return relay_local(listener, connection, Some(target), stop).await;
            }
            Err(error) => error,
        },
        ForwardRule::Dynamic { port } => {
            match prepare_local(ForwardListener::Tcp(port), None).await {
                Ok((ForwardListener::Tcp(port), listener)) => {
                    publish(&state, started, ForwardRule::Dynamic { port });
                    return relay_local(listener, connection, None, stop).await;
                }
                Ok(_) => anyhow::anyhow!("dynamic forwarding requires a TCP listener"),
                Err(error) => error,
            }
        }
    };
    state.send_replace(ForwardState::Stopped);
    started(Err(failure));
    Ok(())
}

fn publish(
    state: &watch::Sender<ForwardState>,
    started: impl FnOnce(Result<ForwardRule>) + Send,
    rule: ForwardRule,
) {
    state.send_replace(ForwardState::Active(rule.clone()));
    started(Ok(rule));
}

fn validate_target(target: &ForwardTarget, local: bool) -> Result<()> {
    match target {
        ForwardTarget::Tcp { host, port } => {
            anyhow::ensure!(
                !host.is_empty()
                    && host.len() <= 512
                    && *port != 0
                    && !host.chars().any(|c| c.is_whitespace() || c.is_control()),
                "enter a destination host and a port between 1 and 65535"
            );
        }
        ForwardTarget::UnixSocket(path) => validate_socket(path, local)?,
    }
    Ok(())
}

fn validate_socket(path: &str, local: bool) -> Result<()> {
    anyhow::ensure!(
        !local || cfg!(unix),
        "local Unix sockets require macOS or Linux"
    );
    anyhow::ensure!(
        path.starts_with('/') && path.len() <= 4096 && !path.chars().any(char::is_control),
        "enter an absolute Unix socket path"
    );
    Ok(())
}

async fn prepare_local(
    requested: ForwardListener,
    target: Option<&ForwardTarget>,
) -> Result<(ForwardListener, LocalListener)> {
    if let Some(target) = target {
        validate_target(target, false)?;
    }
    if let ForwardListener::UnixSocket(path) = &requested {
        validate_socket(path, true)?;
    }
    match requested {
        ForwardListener::Tcp(port) => {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
                .await
                .context("cannot bind the local SSH forwarding port")?;
            Ok((
                ForwardListener::Tcp(listener.local_addr()?.port()),
                LocalListener::Tcp(listener),
            ))
        }
        #[cfg(unix)]
        ForwardListener::UnixSocket(path) => {
            let listener = unix::Listener::bind(path.clone()).await?;
            Ok((
                ForwardListener::UnixSocket(path),
                LocalListener::Unix(listener),
            ))
        }
        #[cfg(not(unix))]
        ForwardListener::UnixSocket(_) => {
            anyhow::bail!("local Unix sockets require macOS or Linux")
        }
    }
}

async fn prepare_remote(
    connection: &SshConnection,
    dispatcher: &RemoteDispatcher,
    requested: ForwardListener,
    target: &ForwardTarget,
) -> Result<(ForwardListener, Registration, mpsc::Receiver<Incoming>)> {
    validate_target(target, true)?;
    if let ForwardListener::UnixSocket(path) = &requested {
        validate_socket(path, false)?;
    }
    let (incoming, receiver) = mpsc::channel(MAX_RELAYS);
    let result = timeout(REQUEST_TIMEOUT, async {
        match &requested {
            ForwardListener::Tcp(port) => {
                connection
                    .handle
                    .tcpip_forward(LOOPBACK, u32::from(*port))
                    .await
            }
            ForwardListener::UnixSocket(path) => connection
                .handle
                .streamlocal_forward(path.clone())
                .await
                .map(|()| 0),
        }
    })
    .await;
    let reply = match result {
        Ok(Ok(reply)) => reply,
        Ok(Err(russh::Error::RequestDenied)) => {
            anyhow::bail!("the SSH server refused the remote forward")
        }
        Ok(Err(error)) => {
            disconnect_uncertain(connection).await;
            return Err(error).context("remote forwarding setup failed");
        }
        Err(_) => {
            disconnect_uncertain(connection).await;
            anyhow::bail!("remote forward confirmation timed out");
        }
    };
    let listen = match requested {
        ForwardListener::Tcp(0) => match u16::try_from(reply) {
            Ok(port) if port != 0 => ForwardListener::Tcp(port),
            _ => {
                disconnect_uncertain(connection).await;
                anyhow::bail!("the server did not return a valid listening port");
            }
        },
        listen => listen,
    };
    let registration = match dispatcher.register(listen.clone(), incoming) {
        Ok(registration) => registration,
        Err(error) => {
            disconnect_uncertain(connection).await;
            return Err(error);
        }
    };
    Ok((listen, registration, receiver))
}

async fn cancel_remote(connection: &SshConnection, listen: ForwardListener) -> Result<()> {
    if connection.is_closed() {
        return Ok(());
    }
    let result = timeout(REQUEST_TIMEOUT, async {
        match listen {
            ForwardListener::Tcp(port) => {
                connection
                    .handle
                    .cancel_tcpip_forward(LOOPBACK, u32::from(port))
                    .await
            }
            ForwardListener::UnixSocket(path) => {
                connection.handle.cancel_streamlocal_forward(path).await
            }
        }
    })
    .await;
    match result {
        Ok(Ok(())) => Ok(()),
        outcome => {
            disconnect_uncertain(connection).await;
            outcome
                .context("remote forward cancellation timed out")?
                .context("cannot cancel the remote forward")
        }
    }
}

async fn disconnect_uncertain(connection: &SshConnection) {
    // The server may have acted on a global request even if we lost its reply.
    // End the owning transport so the server can release its listener.
    if !matches!(
        timeout(DISCONNECT_TIMEOUT, connection.disconnect()).await,
        Ok(Ok(()))
    ) {
        tracing::warn!("could not request SSH disconnect after an uncertain forwarding operation");
    }
}

trait LocalIo: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> LocalIo for T {}

enum LocalListener {
    Tcp(TcpListener),
    #[cfg(unix)]
    Unix(unix::Listener),
}

impl LocalListener {
    async fn accept(&self) -> Result<(Box<dyn LocalIo>, SocketAddr)> {
        match self {
            Self::Tcp(listener) => {
                let (peer, addr) = listener.accept().await?;
                Ok((Box::new(peer), addr))
            }
            #[cfg(unix)]
            Self::Unix(listener) => {
                let peer = listener.accept().await?;
                Ok((Box::new(peer), SocketAddr::from((Ipv4Addr::LOCALHOST, 0))))
            }
        }
    }
}

async fn relay_local(
    listener: LocalListener,
    connection: SshConnection,
    target: Option<ForwardTarget>,
    mut stop: oneshot::Receiver<()>,
) -> Result<()> {
    let mut relays = JoinSet::new();
    let result = loop {
        tokio::select! {
            biased;
            _ = &mut stop => break Ok(()),
            completed = relays.join_next(), if !relays.is_empty() => log_relay(completed),
            accepted = listener.accept(), if relays.len() < MAX_RELAYS => {
                let (socket, peer) = match accepted {
                    Ok(accepted) => accepted,
                    Err(error) => break Err(error).context("local forwarding listener failed"),
                };
                relays.spawn(relay_outgoing(connection.clone(), socket, peer, target.clone()));
            }
        }
    };
    drop(listener);
    relays.shutdown().await;
    result
}

async fn relay_outgoing(
    connection: SshConnection,
    mut socket: Box<dyn LocalIo>,
    origin: SocketAddr,
    target: Option<ForwardTarget>,
) -> Result<()> {
    let target = match target {
        Some(target) => target,
        None => {
            let target = timeout(OPEN_TIMEOUT, socks::request(&mut socket))
                .await
                .context("SOCKS5 handshake timed out")??;
            let channel = open_channel(&connection, target, origin).await;
            let code = match &channel {
                Ok(_) => 0,
                Err(error)
                    if matches!(
                        error.downcast_ref::<russh::Error>(),
                        Some(russh::Error::ChannelOpenFailure(
                            ChannelOpenFailure::AdministrativelyProhibited
                        ))
                    ) =>
                {
                    2
                }
                Err(_) => 5,
            };
            let stream = channel.map(Channel::into_stream);
            // Convert first to ensure an opened SSH channel closes if the
            // local SOCKS client drops before the response.
            timeout(OPEN_TIMEOUT, socks::reply(&mut socket, code)).await??;
            copy_bidirectional(&mut socket, &mut stream?).await?;
            return Ok(());
        }
    };
    let mut stream = open_channel(&connection, target, origin)
        .await?
        .into_stream();
    copy_bidirectional(&mut socket, &mut stream).await?;
    Ok(())
}

async fn open_channel(
    connection: &SshConnection,
    target: ForwardTarget,
    origin: SocketAddr,
) -> Result<Channel<client::Msg>> {
    timeout(OPEN_TIMEOUT, async {
        match target {
            ForwardTarget::Tcp { host, port } => {
                connection
                    .handle
                    .channel_open_direct_tcpip(
                        host,
                        u32::from(port),
                        origin.ip().to_string(),
                        u32::from(origin.port()),
                    )
                    .await
            }
            ForwardTarget::UnixSocket(path) => {
                connection
                    .handle
                    .channel_open_direct_streamlocal(path)
                    .await
            }
        }
    })
    .await
    .context("SSH forwarding channel open timed out")?
    .context("cannot open SSH forwarding channel")
}

async fn relay_remote(
    mut incoming: mpsc::Receiver<Incoming>,
    target: ForwardTarget,
    stop: &mut oneshot::Receiver<()>,
    relays: &mut JoinSet<Result<()>>,
) {
    loop {
        tokio::select! {
            biased;
            _ = &mut *stop => break,
            completed = relays.join_next(), if !relays.is_empty() => log_relay(completed),
            request = incoming.recv() => {
                let Some((channel, reply)) = request else { break; };
                if relays.len() >= MAX_RELAYS {
                    reply.reject(ChannelOpenFailure::ResourceShortage).await;
                    continue;
                }
                relays.spawn(relay_incoming(channel, reply, target.clone()));
            }
        }
    }
}

async fn relay_incoming(
    channel: Channel<client::Msg>,
    reply: client::ChannelOpenHandle,
    target: ForwardTarget,
) -> Result<()> {
    let result = timeout(OPEN_TIMEOUT, connect_target(target)).await;
    match result {
        Ok(Ok(mut socket)) => {
            reply.accept().await;
            copy_bidirectional(&mut socket, &mut channel.into_stream()).await?;
            Ok(())
        }
        failure => {
            reply.reject(ChannelOpenFailure::ConnectFailed).await;
            match failure {
                Ok(result) => result.map(|_| ()),
                Err(error) => Err(error).context("local forwarding target connection timed out"),
            }
        }
    }
}

async fn connect_target(target: ForwardTarget) -> Result<Box<dyn LocalIo>> {
    match target {
        ForwardTarget::Tcp { host, port } => {
            Ok(Box::new(TcpStream::connect((host.as_str(), port)).await?))
        }
        #[cfg(unix)]
        ForwardTarget::UnixSocket(path) => {
            Ok(Box::new(tokio::net::UnixStream::connect(path).await?))
        }
        #[cfg(not(unix))]
        ForwardTarget::UnixSocket(_) => anyhow::bail!("local Unix sockets require macOS or Linux"),
    }
}

fn log_relay(completed: Option<Result<Result<()>, tokio::task::JoinError>>) {
    if matches!(completed, Some(Err(_) | Ok(Err(_)))) {
        tracing::debug!("SSH forwarding relay ended with an error");
    }
}
