//! Output snapshot deadlines; parsing and protocol replies never wait here.

use std::time::Duration;

use tokio::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rates {
    focused: u16,
    unfocused: u16,
}

impl Rates {
    pub(super) fn new(focused: u16, unfocused: u16) -> Self {
        Self {
            focused: focused.clamp(1, 120),
            unfocused: unfocused.clamp(1, 120),
        }
    }

    fn interval(self, focused: bool) -> Duration {
        Duration::from_secs(1)
            / u32::from(if focused {
                self.focused
            } else {
                self.unfocused
            })
    }
}

impl Default for Rates {
    fn default() -> Self {
        let settings = crate::config::AppearanceSettings::default();
        Self::new(
            settings.focused_terminal_refresh_fps,
            settings.unfocused_terminal_refresh_fps,
        )
    }
}

#[derive(Default)]
pub(super) struct Presentation {
    pub(super) rates: Rates,
    pub(super) focused: bool,
    last_publication: Option<Instant>,
}

impl Presentation {
    pub(super) fn deadline(&self, dirty: bool, sync_hold: Option<Duration>) -> Option<Instant> {
        if !dirty {
            return None;
        }
        let now = Instant::now();
        let due = self
            .last_publication
            .map_or(now, |last| last + self.rates.interval(self.focused))
            .max(now);
        Some(sync_hold.map_or(due, |hold| due.max(now + hold)))
    }

    pub(super) fn published(&mut self) {
        // Missed deadlines do not accumulate a burst of catch-up snapshots.
        self.last_publication = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests;
