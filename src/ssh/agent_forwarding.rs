//! Session-scoped forwarding to a local SSH agent.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use russh::keys::agent::AgentSessionBinding;
use russh::{Channel, ChannelMsg, ChannelOpenFailure, client};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

use super::{SshConnection, connect_runtime_agent};

pub(super) const MAX_RELAYS: usize = 4;
const MAX_FRAME: usize = 256 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const AGENT_RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

pub(super) type Incoming = (Channel<client::Msg>, client::ChannelOpenHandle);

#[derive(Clone, Default)]
pub(super) struct Dispatcher {
    sender: Option<mpsc::Sender<Incoming>>,
    enabled: Arc<AtomicBool>,
}

pub(super) struct AgentForwarding {
    enabled: Arc<AtomicBool>,
    binding: AgentSessionBinding,
}

impl Dispatcher {
    pub(super) fn channel() -> (Self, mpsc::Receiver<Incoming>) {
        let (sender, receiver) = mpsc::channel(MAX_RELAYS);
        (
            Self {
                sender: Some(sender),
                ..Self::default()
            },
            receiver,
        )
    }

    pub(super) fn prepare(&self, connection: &SshConnection) -> Option<AgentForwarding> {
        self.sender.as_ref()?;
        Some(AgentForwarding {
            enabled: self.enabled.clone(),
            binding: connection.handle.agent_session_binding()?.clone(),
        })
    }

    pub(super) async fn dispatch(
        &self,
        channel: Channel<client::Msg>,
        reply: client::ChannelOpenHandle,
    ) {
        if self.enabled.load(Ordering::Acquire)
            && let Some(sender) = &self.sender
        {
            // An exhausted or closed queue drops the request and rejects it.
            if sender.try_send((channel, reply)).is_err() {
                tracing::debug!("SSH agent relay queue unavailable");
            }
        } else {
            reply
                .reject(ChannelOpenFailure::AdministrativelyProhibited)
                .await;
        }
    }
}

impl AgentForwarding {
    pub(super) async fn request(&self, channel: &mut Channel<client::Msg>) {
        let confirmation = timeout(REQUEST_TIMEOUT, async {
            channel.agent_forward(true).await?;
            loop {
                match channel.wait().await {
                    Some(ChannelMsg::Success) => return Ok(true),
                    Some(ChannelMsg::Failure | ChannelMsg::Close | ChannelMsg::Eof) | None => {
                        return Ok(false);
                    }
                    Some(_) => {}
                }
            }
        })
        .await;
        if matches!(confirmation, Ok(Ok::<_, russh::Error>(true))) {
            self.enabled.store(true, Ordering::Release);
        } else {
            tracing::warn!("SSH server did not confirm agent forwarding");
        }
    }

    pub(super) fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub(super) async fn relay(
        binding: AgentSessionBinding,
        (channel, reply): Incoming,
    ) -> Result<()> {
        let agent = timeout(REQUEST_TIMEOUT, async {
            let mut agent = connect_runtime_agent().await?;
            agent
                .bind_session(&binding, true)
                .await
                .context("cannot bind the local SSH agent")?;
            Ok::<_, anyhow::Error>(agent)
        })
        .await;
        let Ok(Ok(agent)) = agent else {
            reply
                .reject(ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            anyhow::bail!("local SSH agent could not establish a bound forwarding session");
        };
        reply.accept().await;
        forward_requests(channel.into_stream(), agent.into_inner()).await
    }

    pub(super) fn binding(&self) -> AgentSessionBinding {
        self.binding.clone()
    }
}

impl Drop for AgentForwarding {
    fn drop(&mut self) {
        self.enabled.store(false, Ordering::Release);
    }
}

async fn forward_requests(
    mut remote: impl AsyncRead + AsyncWrite + Unpin,
    mut agent: impl AsyncRead + AsyncWrite + Unpin,
) -> Result<()> {
    while let Some(request) = read_frame(&mut remote).await? {
        if !permitted_request(&request) {
            write_frame(&mut remote, &[5]).await?;
            continue;
        }
        let response = timeout(AGENT_RESPONSE_TIMEOUT, async {
            write_frame(&mut agent, &request).await?;
            read_frame(&mut agent)
                .await?
                .context("local SSH agent disconnected")
        })
        .await
        .context("local SSH agent response timed out")??;
        write_frame(&mut remote, &response).await?;
    }
    remote
        .shutdown()
        .await
        .context("cannot close forwarded SSH agent channel")
}

fn permitted_request(request: &[u8]) -> bool {
    const BIND_SESSION: &[u8] = b"session-bind@openssh.com";
    match request {
        [11] | [13, ..] => true, // list identities or request a signature
        [27, 0, 0, 0, length, tail @ ..] => {
            usize::from(*length) == BIND_SESSION.len() && tail.starts_with(BIND_SESSION)
        }
        _ => false,
    }
}

async fn read_frame(stream: &mut (impl AsyncRead + Unpin)) -> Result<Option<Vec<u8>>> {
    let mut length = [0; 4];
    if stream.read(&mut length[..1]).await? == 0 {
        return Ok(None);
    }
    stream.read_exact(&mut length[1..]).await?;
    let length = u32::from_be_bytes(length) as usize;
    anyhow::ensure!(
        (1..=MAX_FRAME).contains(&length),
        "invalid SSH agent frame size"
    );
    let mut data = vec![0; length];
    stream.read_exact(&mut data).await?;
    Ok(Some(data))
}

async fn write_frame(stream: &mut (impl AsyncWrite + Unpin), data: &[u8]) -> Result<()> {
    stream.write_u32(data.len() as u32).await?;
    stream.write_all(data).await?;
    stream.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn forwarding_rejects_mutation_and_relays_identity_requests() {
        let (mut remote, channel) = tokio::io::duplex(64);
        let (mut agent, local) = tokio::io::duplex(64);
        let relay = tokio::spawn(forward_requests(channel, local));
        write_frame(&mut remote, &[19]).await.unwrap();
        assert_eq!(read_frame(&mut remote).await.unwrap(), Some(vec![5]));
        write_frame(&mut remote, &[11]).await.unwrap();
        assert_eq!(read_frame(&mut agent).await.unwrap(), Some(vec![11]));
        write_frame(&mut agent, &[12, 0, 0, 0, 0]).await.unwrap();
        assert_eq!(
            read_frame(&mut remote).await.unwrap(),
            Some(vec![12, 0, 0, 0, 0])
        );
        remote.shutdown().await.unwrap();
        relay.await.unwrap().unwrap();
    }
}
