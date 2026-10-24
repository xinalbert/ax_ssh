//! A single cancellable deadline consumes the shared five-attempt retry budget.

use super::*;

pub(super) async fn wait(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

impl Worker {
    pub(super) async fn failed(&mut self, status: String) -> Result<()> {
        self.connected = false;
        self.pending_secret = None;
        self.stop_transport().await?;
        self.status = status.chars().take(400).collect();
        self.reconnect.finish_attempt(self.reconnect.generation());
        if self.profile.is_some() && self.prompt.is_none() {
            if let Some((generation, attempt)) = self.reconnect.begin(false) {
                let delay = Duration::from_secs(1 << (attempt - 1));
                self.retry_deadline = Some((tokio::time::Instant::now() + delay, generation));
                self.status
                    .push_str(&format!(" · Retry {attempt}/5 in {}s", delay.as_secs()));
            } else if self.reconnect.enabled() {
                self.status.push_str(" · Automatic retry limit reached");
            }
        }
        self.publish()
    }

    pub(super) async fn retry(&mut self) -> Result<()> {
        let Some((_, generation)) = self.retry_deadline.take() else {
            return Ok(());
        };
        if !self.reconnect.current(generation) || self.transport.is_some() {
            return Ok(());
        }
        self.reset_model()?;
        let needs_secret = self
            .profile
            .as_ref()
            .and_then(SessionProfile::ssh)
            .is_some_and(|config| !matches!(config.auth, crate::config::AuthMethod::SshAgent));
        if needs_secret {
            self.prompt(TerminalPromptKind::Authentication)?;
            self.status = "Reconnect requires fresh SSH credentials".into();
        } else {
            self.start_remote(Zeroizing::new(String::new()))?;
        }
        self.publish()
    }
}
