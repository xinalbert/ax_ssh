//! Reconnect policy without timers, workers, UI values or credentials.

/// One connection's retry budget and cancellation generation.
///
/// The application owner still cancels dependent media/clipboard work and owns
/// its worker. Scheduling and authentication remain outside this state machine.
pub struct ReconnectState {
    generation: u64,
    attempt: u8,
    reconnecting: bool,
    enabled: bool,
}

impl ReconnectState {
    /// Maximum automatic retries between a successful connection or manual retry.
    pub const MAX_ATTEMPTS: u8 = 5;

    /// Creates a fresh retry budget; local terminals normally pass false.
    pub fn new(enabled: bool) -> Self {
        Self {
            generation: 0,
            attempt: 0,
            reconnecting: false,
            enabled,
        }
    }

    /// Identity used to reject delayed work after cancellation or manual retry.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Number of automatic retries already admitted in the current budget.
    pub fn attempt(&self) -> u8 {
        self.attempt
    }

    /// Whether an automatic reconnect has been admitted and not yet completed.
    pub fn reconnecting(&self) -> bool {
        self.reconnecting
    }

    /// Whether the owner currently permits automatic reconnect.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Admits one retry only when enabled, idle, worker-free and within budget.
    pub fn begin(&mut self, worker_present: bool) -> Option<(u64, u8)> {
        if !self.enabled || self.reconnecting || worker_present {
            return None;
        }
        let attempt = self.attempt.checked_add(1)?;
        if attempt > Self::MAX_ATTEMPTS {
            return None;
        }
        self.attempt = attempt;
        self.reconnecting = true;
        Some((self.generation, attempt))
    }

    /// Whether delayed reconnect work still belongs to the active retry.
    pub fn current(&self, generation: u64) -> bool {
        self.enabled && self.generation == generation && self.reconnecting
    }

    /// Completes a retry without letting an older generation stop its replacement.
    pub fn finish_attempt(&mut self, generation: u64) -> bool {
        if self.generation != generation {
            return false;
        }
        self.reconnecting = false;
        true
    }

    /// Resets the retry budget when the current generation connects successfully.
    pub fn mark_connected(&mut self, generation: u64) -> bool {
        if self.generation != generation {
            return false;
        }
        self.attempt = 0;
        self.reconnecting = false;
        self.enabled = true;
        true
    }

    /// Revokes delayed work and disables automatic retries.
    pub fn cancel(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.reconnecting = false;
        self.enabled = false;
    }

    /// Starts a new manual retry generation and budget under the owner's policy.
    pub fn prepare_manual_retry(&mut self, enabled: bool) {
        self.generation = self.generation.wrapping_add(1);
        self.attempt = 0;
        self.reconnecting = false;
        self.enabled = enabled;
    }

    /// Enables automatic retries without changing the generation or budget.
    pub fn enable(&mut self) {
        self.enabled = true;
    }
}

#[cfg(test)]
mod tests;
