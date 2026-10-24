//! Connection transitions stay serialized with terminal input in the model owner.

use super::*;
use crate::ssh::{HostKeyProbe, TrustDecision};

impl Worker {
    pub(super) fn transport(&self) -> Result<&Transport> {
        self.transport.as_ref().context("terminal is disconnected")
    }

    pub(super) async fn stop_transport(&mut self) -> Result<()> {
        // Drop the receiver before joining: a full event queue must not block cleanup.
        self.events = Events::Idle;
        if let Some(transport) = self.transport.take() {
            transport.shutdown().await?;
        }
        Ok(())
    }

    async fn replace(&mut self) -> Result<()> {
        self.connected = false;
        self.prompt = None;
        self.pending_secret = None;
        self.profile = None;
        self.reconnect.cancel();
        self.retry_deadline = None;
        self.status = "Disconnecting…".into();
        self.publish()?;
        self.stop_transport().await?;
        self.reset_model()
    }

    pub(super) fn reset_model(&mut self) -> Result<()> {
        self.generation = self
            .generation
            .checked_add(1)
            .context("connection generation exhausted")?;
        let size = self.model.size();
        self.model = new_model(size);
        self.viewport.reset();
        self.last_resize = None;
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal view lock poisoned"))?;
        shared.clipboard = None;
        shared.selection = None;
        Ok(())
    }

    pub(super) async fn dispatch(&mut self, queued: QueuedCommand) -> Result<()> {
        match queued.command {
            Command::Connect(profile, secret, automatic) => {
                self.replace().await?;
                self.reconnect.prepare_manual_retry(automatic);
                self.profile = Some(*profile);
                self.start_remote(secret)?;
                self.publish()?;
            }
            Command::Local(shell) => {
                self.replace().await?;
                let size = self.model.size();
                let (worker, events) = LocalShellHandle::spawn(shell, size.columns(), size.rows());
                self.transport = Some(Transport::Local(worker));
                self.events = Events::Local(events);
                self.identity = ConnectionIdentity::Local;
                self.status = "Starting local shell…".into();
                self.publish()?;
            }
            _ if queued.generation != self.generation => {}
            Command::Disconnect => {
                self.replace().await?;
                self.status = "Disconnected".into();
                self.publish()?;
            }
            Command::Respond(id, secret) => {
                let Some(prompt) = self.prompt.as_ref().filter(|prompt| prompt.id == id) else {
                    return Ok(());
                };
                match &prompt.kind {
                    TerminalPromptKind::HostKey {
                        actual,
                        confirmable: true,
                        ..
                    } => {
                        let config = self
                            .profile
                            .as_mut()
                            .and_then(SessionProfile::ssh_mut)
                            .context("missing SSH prompt target")?;
                        config.host_key_fingerprint = Some(actual.clone());
                    }
                    TerminalPromptKind::HostKey {
                        confirmable: false, ..
                    } => return Ok(()),
                    TerminalPromptKind::Authentication => {}
                }
                self.prompt = None;
                self.retry_deadline = None;
                self.reconnect
                    .prepare_manual_retry(self.reconnect.enabled());
                // Re-probe on every manual retry. Authentication never precedes trust.
                self.start_remote(secret)?;
                self.publish()?;
            }
            command => self.command(command)?,
        }
        Ok(())
    }

    pub(super) fn start_remote(&mut self, secret: Zeroizing<String>) -> Result<()> {
        let profile = self.profile.as_ref().context("missing temporary profile")?;
        self.connected = false;
        if profile.ssh().is_some() {
            self.status = "Checking SSH host key…".into();
            self.identity = ConnectionIdentity::Ssh {
                profile_id: profile.id,
                attempt_id: Some(Uuid::new_v4()),
            };
            let (transport, events) = Transport::probe(profile.clone());
            self.transport = Some(transport);
            self.events = events;
            self.pending_secret = Some(secret);
        } else {
            self.start_authenticated(secret)?;
        }
        Ok(())
    }

    fn start_authenticated(&mut self, secret: Zeroizing<String>) -> Result<()> {
        let profile = self.profile.as_ref().context("missing temporary profile")?;
        let (transport, events, identity) =
            Transport::start(profile, secret, Uuid::new_v4(), self.model.size());
        self.transport = Some(transport);
        self.events = events;
        self.identity = identity;
        self.status = "Connecting…".into();
        // Replay physical metrics to the new worker even if the grid did not change.
        if let Some((size, _, _)) = self.last_resize {
            self.transport()?.resize(size)?;
        }
        Ok(())
    }

    pub(super) fn prompt(&mut self, kind: TerminalPromptKind) -> Result<()> {
        let config = self
            .profile
            .as_ref()
            .and_then(SessionProfile::ssh)
            .context("missing SSH prompt target")?;
        self.prompt = Some(TerminalPrompt {
            id: Uuid::new_v4(),
            host: config.host.clone(),
            port: config.port,
            username: config.username.clone(),
            kind,
        });
        Ok(())
    }

    pub(super) async fn event(&mut self, event: Option<Event>) -> Result<()> {
        match event {
            Some(Event::Probe(result)) => {
                self.stop_transport().await?;
                let result = match result {
                    Ok(result) => result,
                    Err(error) => {
                        self.pending_secret = None;
                        return Err(error.context("SSH host-key probe failed"));
                    }
                };
                let config = self
                    .profile
                    .as_ref()
                    .and_then(SessionProfile::ssh)
                    .context("missing SSH probe target")?;
                if let Some(kind) = trust_prompt(&result, config.host_key_fingerprint.as_deref()) {
                    self.pending_secret = None;
                    self.prompt(kind)?;
                    self.status = "SSH host-key confirmation required".into();
                } else {
                    let secret = self
                        .pending_secret
                        .take()
                        .context("missing ephemeral authentication input")?;
                    // Pin this observation too: a key change between probe and worker is rejected.
                    if let Some(config) = self.profile.as_mut().and_then(SessionProfile::ssh_mut) {
                        config.host_key_fingerprint = Some(result.fingerprint);
                    }
                    self.start_authenticated(secret)?;
                }
                self.publish()?;
            }
            Some(Event::Connected) => {
                self.connected = true;
                self.status = "Connected".into();
                self.prompt = None;
                if self.reconnect.enabled() {
                    self.reconnect.mark_connected(self.reconnect.generation());
                }
                self.publish()?;
            }
            Some(Event::Output(chunk)) => {
                let effects = TerminalOutput::process(&mut self.model, &chunk.data);
                for response in effects.responses {
                    self.transport()?.send(response)?;
                }
                // Title and OSC 52 actions remain disabled in the experimental frontend.
                self.viewport.defer_output();
            }
            Some(Event::AuthenticationFailed) => {
                self.connected = false;
                self.stop_transport().await?;
                self.pending_secret = None;
                self.reconnect.finish_attempt(self.reconnect.generation());
                self.prompt(TerminalPromptKind::Authentication)?;
                self.status = "SSH authentication failed; retry with fresh credentials".into();
                self.publish()?;
            }
            Some(Event::HostKey { expected, actual }) => {
                self.connected = false;
                self.stop_transport().await?;
                self.pending_secret = None;
                self.prompt(TerminalPromptKind::HostKey {
                    expected,
                    actual,
                    confirmable: false,
                })?;
                self.status = "SSH host key rejected; start a new connection to check again".into();
                self.publish()?;
            }
            Some(Event::Ended(status)) => self.failed(status).await?,
            None => self.failed("Disconnected".into()).await?,
            Some(Event::Ignore) => {}
        }
        Ok(())
    }
}

fn trust_prompt(probe: &HostKeyProbe, expected: Option<&str>) -> Option<TerminalPromptKind> {
    let blocked = matches!(
        probe.decision,
        TrustDecision::Changed | TrustDecision::Revoked
    );
    let matches = expected == Some(probe.fingerprint.as_str());
    if !blocked && (matches || (expected.is_none() && probe.decision == TrustDecision::Trusted)) {
        return None;
    }
    Some(TerminalPromptKind::HostKey {
        expected: expected.map(str::to_owned),
        actual: probe.fingerprint.clone(),
        confirmable: !blocked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_decision_preserves_pins_and_never_overrides_changed_or_revoked_keys() {
        for decision in [
            TrustDecision::Unknown,
            TrustDecision::Trusted,
            TrustDecision::Changed,
            TrustDecision::Revoked,
        ] {
            for expected in [None, Some("fixture-key"), Some("different-key")] {
                let probe = HostKeyProbe {
                    fingerprint: "fixture-key".into(),
                    decision,
                    public_key: None,
                };
                let prompt = trust_prompt(&probe, expected);
                let blocked = matches!(decision, TrustDecision::Changed | TrustDecision::Revoked);
                if blocked {
                    assert!(matches!(
                        prompt,
                        Some(TerminalPromptKind::HostKey {
                            confirmable: false,
                            ..
                        })
                    ));
                } else {
                    assert_eq!(
                        prompt.is_none(),
                        expected == Some("fixture-key")
                            || (expected.is_none() && decision == TrustDecision::Trusted)
                    );
                }
            }
        }
    }
}
