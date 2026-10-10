//! Volatile latency presentation; never part of a server profile or workspace file.

use std::time::{Duration, Instant, SystemTime};

const SAMPLE_LIFETIME: Duration = Duration::from_secs(90);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Measuring,
    Measured,
    TimedOut,
    Stale,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Quality {
    Good,
    Moderate,
    High,
    Poor,
}

impl Quality {
    fn for_micros(micros: u64) -> Self {
        match micros {
            0..100_000 => Self::Good,
            100_000..250_000 => Self::Moderate,
            250_000..500_000 => Self::High,
            _ => Self::Poor,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Moderate => "moderate",
            Self::High => "high",
            Self::Poor => "poor",
        }
    }
}

#[derive(Default)]
pub(in crate::app) struct ConnectionLatency {
    phase: Phase,
    smoothed_micros: Option<u64>,
    quality: Option<Quality>,
    candidate: Option<Quality>,
    measured_at: Option<Instant>,
    observed_at: Option<Instant>,
    updated_at: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::app) struct ConnectionLatencySnapshot {
    pub(in crate::app) state: &'static str,
    pub(in crate::app) milliseconds: i32,
    pub(in crate::app) updated_at: String,
}

impl ConnectionLatency {
    pub(in crate::app) fn record_sample(&mut self, sample: Duration, measured_at: Instant) {
        let now = Instant::now();
        if self
            .measured_at
            .is_some_and(|previous| measured_at <= previous)
        {
            return;
        }
        if now.saturating_duration_since(measured_at) >= SAMPLE_LIFETIME {
            self.phase = Phase::Stale;
            return;
        }
        let sample = sample.as_micros().min(u64::MAX as u128) as u64;
        let fresh = self.phase == Phase::Measured
            && self
                .observed_at
                .is_some_and(|at| now.saturating_duration_since(at) < SAMPLE_LIFETIME);
        let smoothed = if fresh {
            self.smoothed_micros.map_or(sample, |previous| {
                ((u128::from(previous) * 4 + u128::from(sample)) / 5) as u64
            })
        } else {
            sample
        };
        let quality = Quality::for_micros(smoothed);
        // A new colour requires two consecutive smoothed samples in that band.
        if !fresh || self.quality == Some(quality) || self.candidate == Some(quality) {
            self.quality = Some(quality);
            self.candidate = None;
        } else {
            self.candidate = Some(quality);
        }
        self.smoothed_micros = Some(smoothed);
        self.measured_at = Some(measured_at);
        self.observed_at = Some(measured_at);
        self.phase = Phase::Measured;
        let wall_time = SystemTime::now()
            .checked_sub(now.saturating_duration_since(measured_at))
            .unwrap_or(SystemTime::UNIX_EPOCH);
        self.updated_at = chrono::DateTime::<chrono::Local>::from(wall_time)
            .format("%H:%M:%S")
            .to_string();
    }

    pub(in crate::app) fn record_timeout(&mut self) {
        self.phase = Phase::TimedOut;
        self.observed_at = Some(Instant::now());
        self.candidate = None;
    }

    pub(in crate::app) fn expire(&mut self) {
        self.phase = Phase::Stale;
    }
    pub(in crate::app) fn unavailable(&mut self) {
        self.phase = Phase::Unavailable;
    }

    pub(in crate::app) fn snapshot(&self, now: Instant) -> ConnectionLatencySnapshot {
        let expired = matches!(self.phase, Phase::Measured | Phase::TimedOut)
            && self
                .observed_at
                .is_some_and(|at| now.saturating_duration_since(at) >= SAMPLE_LIFETIME);
        let state = if expired {
            "stale"
        } else {
            match self.phase {
                Phase::Measuring => "measuring",
                Phase::Measured => self.quality.map_or("measuring", Quality::name),
                Phase::TimedOut => "timeout",
                Phase::Stale => "stale",
                Phase::Unavailable => "unavailable",
            }
        };
        ConnectionLatencySnapshot {
            state,
            milliseconds: self
                .smoothed_micros
                .unwrap_or(0)
                .div_ceil(1000)
                .min(i32::MAX as u64) as i32,
            updated_at: self.updated_at.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_bands_include_their_lower_boundary() {
        for (ms, name) in [
            (0, "good"),
            (99, "good"),
            (100, "moderate"),
            (249, "moderate"),
            (250, "high"),
            (499, "high"),
            (500, "poor"),
        ] {
            let mut latency = ConnectionLatency::default();
            latency.record_sample(Duration::from_millis(ms), Instant::now());
            assert_eq!(latency.snapshot(Instant::now()).state, name);
        }
    }

    #[test]
    fn smoothed_colour_requires_two_samples_and_recovers_after_timeout() {
        let mut latency = ConnectionLatency::default();
        let start = Instant::now();
        latency.record_sample(Duration::from_millis(90), start);
        latency.record_sample(Duration::from_millis(190), start + Duration::from_nanos(1));
        assert_eq!(latency.snapshot(start).milliseconds, 110);
        assert_eq!(latency.snapshot(start).state, "good");
        latency.record_sample(Duration::from_millis(110), start + Duration::from_nanos(2));
        assert_eq!(latency.snapshot(start).state, "moderate");
        latency.record_timeout();
        assert_eq!(latency.snapshot(start).state, "timeout");
        latency.record_sample(Duration::from_millis(700), start + Duration::from_nanos(3));
        assert_eq!(latency.snapshot(start).state, "poor");
        assert_eq!(latency.snapshot(start).milliseconds, 700);
    }

    #[test]
    fn stale_or_reordered_samples_cannot_revive_old_green_data() {
        let mut latency = ConnectionLatency::default();
        let now = Instant::now();
        latency.record_sample(Duration::from_millis(600), now);
        latency.record_sample(Duration::from_millis(10), now - Duration::from_secs(1));
        assert_eq!(latency.snapshot(now).state, "poor");
        assert_eq!(latency.snapshot(now + SAMPLE_LIFETIME).state, "stale");
        let mut queued = ConnectionLatency::default();
        queued.record_sample(Duration::from_millis(10), now - SAMPLE_LIFETIME);
        assert_eq!(queued.snapshot(now).state, "stale");
    }

    #[test]
    fn latency_display_bounds_large_samples_and_preserves_submillisecond_values() {
        let mut latency = ConnectionLatency::default();
        latency.record_sample(Duration::MAX, Instant::now());
        assert_eq!(latency.snapshot(Instant::now()).milliseconds, i32::MAX);
        let mut latency = ConnectionLatency::default();
        latency.record_sample(Duration::from_micros(1), Instant::now());
        assert_eq!(latency.snapshot(Instant::now()).milliseconds, 1);
    }
}
