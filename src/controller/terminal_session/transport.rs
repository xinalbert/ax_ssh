//! One active transport and its bounded event receiver; no relay task or UI handles.

use super::*;
use crate::config::ConnectionProfile;
use crate::serial::{SerialSessionEvent, SerialSessionHandle};
use crate::ssh::{SshSessionEvent, SshSessionHandle, SshTerminalSettings};
use crate::telnet::{TelnetSessionEvent, TelnetSessionHandle};
use crate::terminal::TerminalOutputChunk;

pub(super) enum Transport {
    Probe(JoinHandle<()>),
    Local(LocalShellHandle),
    Ssh(SshSessionHandle),
    Telnet(TelnetSessionHandle),
    Serial(SerialSessionHandle),
}

pub(super) enum Events {
    Idle,
    Probe(tokio::sync::oneshot::Receiver<Result<crate::ssh::HostKeyProbe>>),
    Local(mpsc::Receiver<LocalShellEvent>),
    Ssh(mpsc::Receiver<SshSessionEvent>),
    Telnet(mpsc::Receiver<TelnetSessionEvent>),
    Serial(mpsc::Receiver<SerialSessionEvent>),
}

pub(super) enum Event {
    Probe(Result<crate::ssh::HostKeyProbe>),
    Connected,
    Output(TerminalOutputChunk),
    Ended(String),
    AuthenticationFailed,
    Notice(String),
    HostKey {
        expected: Option<String>,
        actual: String,
    },
    Ignore,
}

impl Transport {
    pub(super) fn probe(profile: SessionProfile) -> (Self, Events) {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let result = crate::ssh::probe_host_key(&profile).await;
            if sender.send(result).is_err() {
                tracing::trace!("terminal host-key probe recipient closed");
            }
        });
        (Self::Probe(task), Events::Probe(receiver))
    }
    pub(super) fn start(
        profile: &SessionProfile,
        secret: Zeroizing<String>,
        attempt: Uuid,
        size: TerminalSize,
        settings: SshTerminalSettings,
    ) -> (Self, Events, ConnectionIdentity) {
        let runtime = Handle::current();
        let profile_id = profile.id;
        let attempt_id = Some(attempt);
        match &profile.connection {
            ConnectionProfile::Ssh(_) => {
                let (worker, events) = SshSessionHandle::spawn_with_terminal_settings(
                    &runtime,
                    attempt,
                    profile.clone(),
                    secret,
                    size.columns(),
                    size.rows(),
                    settings,
                );
                (
                    Self::Ssh(worker),
                    Events::Ssh(events),
                    ConnectionIdentity::Ssh {
                        profile_id,
                        attempt_id,
                    },
                )
            }
            ConnectionProfile::Telnet(config) => {
                let (worker, events) = TelnetSessionHandle::spawn(
                    &runtime,
                    attempt,
                    config.clone(),
                    size.columns(),
                    size.rows(),
                );
                (
                    Self::Telnet(worker),
                    Events::Telnet(events),
                    ConnectionIdentity::Telnet {
                        profile_id,
                        attempt_id,
                    },
                )
            }
            ConnectionProfile::Serial(config) => {
                let (worker, events) =
                    SerialSessionHandle::spawn(&runtime, attempt, config.clone());
                (
                    Self::Serial(worker),
                    Events::Serial(events),
                    ConnectionIdentity::Serial {
                        profile_id,
                        attempt_id,
                    },
                )
            }
        }
    }

    pub(super) fn send(&self, data: Vec<u8>) -> Result<()> {
        match self {
            Self::Probe(_) => bail!("terminal is waiting for host-key verification"),
            Self::Local(worker) => worker.request_send(data),
            Self::Ssh(worker) => worker.request_send(data),
            Self::Telnet(worker) => worker.request_send(data),
            Self::Serial(worker) => worker.request_send(data),
        }
    }

    pub(super) fn paste(&self, data: Vec<u8>) -> Result<()> {
        match self {
            Self::Probe(_) => bail!("terminal is waiting for host-key verification"),
            Self::Local(worker) => worker.request_send_paste(data),
            Self::Ssh(worker) => worker.request_send_paste(data),
            Self::Telnet(worker) => worker.request_send_paste(data),
            Self::Serial(worker) => worker.request_send_paste(data),
        }
    }

    pub(super) fn motion(&self, data: Vec<u8>) -> Result<bool> {
        match self {
            Self::Probe(_) => bail!("terminal is waiting for host-key verification"),
            Self::Local(worker) => worker.request_send_motion(data),
            Self::Ssh(worker) => worker.request_send_motion(data),
            Self::Telnet(worker) => worker.request_send_motion(data),
            Self::Serial(worker) => worker.request_send_motion(data),
        }
    }

    pub(super) fn resize(&self, size: TerminalSize) -> Result<()> {
        match self {
            Self::Probe(_) => Ok(()),
            Self::Local(worker) => worker.request_resize_with_pixels(
                size.columns(),
                size.rows(),
                size.pixel_width(),
                size.pixel_height(),
            ),
            Self::Ssh(worker) => worker.request_resize_with_pixels(
                size.columns(),
                size.rows(),
                size.pixel_width(),
                size.pixel_height(),
            ),
            Self::Telnet(worker) => worker.request_resize(size.columns(), size.rows()),
            Self::Serial(_) => Ok(()),
        }
    }

    pub(super) async fn shutdown(self) -> Result<()> {
        match self {
            Self::Probe(task) => {
                task.abort();
                match task.await {
                    Ok(()) => Ok(()),
                    Err(error) if error.is_cancelled() => Ok(()),
                    Err(error) => Err(error).context("host-key probe task failed"),
                }
            }
            Self::Local(worker) => worker.shutdown().await,
            Self::Ssh(worker) => worker.shutdown().await,
            Self::Telnet(worker) => worker.shutdown().await,
            Self::Serial(worker) => worker.shutdown().await,
        }
    }
}

impl Events {
    pub(super) async fn next(&mut self) -> Option<Event> {
        Some(match self {
            Self::Idle => std::future::pending().await,
            Self::Probe(receiver) => Event::Probe(
                receiver
                    .await
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("host-key probe stopped"))),
            ),
            Self::Local(events) => match events.recv().await? {
                LocalShellEvent::Started { .. } => Event::Connected,
                LocalShellEvent::Output(chunk) => Event::Output(chunk),
                LocalShellEvent::Exited { status } | LocalShellEvent::Failed(status) => {
                    Event::Ended(status)
                }
                LocalShellEvent::Resized { .. } => Event::Ignore,
            },
            Self::Telnet(events) => match events.recv().await? {
                TelnetSessionEvent::Connected => Event::Connected,
                TelnetSessionEvent::Output(chunk) => Event::Output(chunk),
                TelnetSessionEvent::Disconnected => Event::Ended("Disconnected".into()),
                TelnetSessionEvent::Failed(status) => Event::Ended(status),
            },
            Self::Serial(events) => match events.recv().await? {
                SerialSessionEvent::Connected { .. } => Event::Connected,
                SerialSessionEvent::Output(chunk) => Event::Output(chunk),
                SerialSessionEvent::Disconnected => Event::Ended("Disconnected".into()),
                SerialSessionEvent::Failed(status) => Event::Ended(status),
            },
            Self::Ssh(events) => match events.recv().await? {
                SshSessionEvent::Connected => Event::Connected,
                SshSessionEvent::Output(chunk) => Event::Output(chunk),
                SshSessionEvent::X11ForwardingEnabled => {
                    Event::Notice("Connected · X11 forwarding enabled".into())
                }
                SshSessionEvent::X11ForwardingUnavailable(message) => {
                    Event::Notice(format!("Connected · X11 unavailable: {message}"))
                }
                SshSessionEvent::MicrophoneUnavailable(message) => {
                    Event::Notice(format!("Connected · Microphone unavailable: {message}"))
                }
                SshSessionEvent::HostKeyRejected {
                    expected, actual, ..
                } => Event::HostKey { expected, actual },
                SshSessionEvent::HostKeyRevoked { actual, .. } => Event::HostKey {
                    expected: None,
                    actual,
                },
                SshSessionEvent::AuthenticationFailed | SshSessionEvent::PrivateKeyFailed(_) => {
                    Event::AuthenticationFailed
                }
                SshSessionEvent::Failed(status) => Event::Ended(status),
                SshSessionEvent::Disconnected | SshSessionEvent::ShellExited => {
                    Event::Ended("Disconnected".into())
                }
                _ => Event::Ignore,
            },
        })
    }
}
