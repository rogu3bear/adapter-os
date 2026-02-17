//! Shared graduated-drain phase and statistics primitives.

use std::time::Duration;

/// Phase durations for graduated drain escalation.
#[derive(Debug, Clone)]
pub struct DrainPhaseConfig {
    /// Duration of Phase 1: graceful silent wait.
    pub graceful: Duration,
    /// Duration of Phase 2: warning logging.
    pub warning: Duration,
    /// Duration of Phase 3: shutdown notification.
    pub notify: Duration,
}

impl Default for DrainPhaseConfig {
    fn default() -> Self {
        Self {
            graceful: Duration::from_secs(15),
            warning: Duration::from_secs(10),
            notify: Duration::from_secs(5),
        }
    }
}

impl DrainPhaseConfig {
    /// Total drain duration across all phases.
    pub fn total(&self) -> Duration {
        self.graceful + self.warning + self.notify
    }

    /// Cumulative elapsed-time boundaries for phase transitions.
    pub fn phase_boundaries(&self) -> (Duration, Duration) {
        let warning_start = self.graceful;
        let notify_start = warning_start + self.warning;
        (warning_start, notify_start)
    }
}

/// Drain phase state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainPhase {
    Graceful,
    Warning,
    Notify,
    Force,
}

impl DrainPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            DrainPhase::Graceful => "graceful",
            DrainPhase::Warning => "warning",
            DrainPhase::Notify => "notify",
            DrainPhase::Force => "force",
        }
    }
}

/// Aggregated request statistics sampled during drain.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DrainStats {
    pub sample_count: u64,
    pub total_in_flight: u64,
    pub peak_in_flight: usize,
}

impl DrainStats {
    pub fn record(&mut self, in_flight: usize) {
        self.sample_count += 1;
        self.total_in_flight += in_flight as u64;
        self.peak_in_flight = self.peak_in_flight.max(in_flight);
    }

    pub fn average_in_flight(&self) -> f64 {
        if self.sample_count == 0 {
            return 0.0;
        }
        self.total_in_flight as f64 / self.sample_count as f64
    }
}

/// Determine the current phase from elapsed time and phase boundaries.
pub fn phase_for_elapsed(
    elapsed: Duration,
    warning_start: Duration,
    notify_start: Duration,
) -> DrainPhase {
    if elapsed >= notify_start {
        DrainPhase::Notify
    } else if elapsed >= warning_start {
        DrainPhase::Warning
    } else {
        DrainPhase::Graceful
    }
}

/// Whether the current sample should emit warning logs (every ~20 samples).
pub fn should_emit_warning_sample(sample_count: u64) -> bool {
    sample_count > 0 && sample_count.is_multiple_of(20)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_phase_config_default_totals_30s() {
        let cfg = DrainPhaseConfig::default();
        assert_eq!(cfg.graceful, Duration::from_secs(15));
        assert_eq!(cfg.warning, Duration::from_secs(10));
        assert_eq!(cfg.notify, Duration::from_secs(5));
        assert_eq!(cfg.total(), Duration::from_secs(30));
    }

    #[test]
    fn drain_phase_boundaries_accumulate() {
        let cfg = DrainPhaseConfig {
            graceful: Duration::from_secs(5),
            warning: Duration::from_secs(3),
            notify: Duration::from_secs(2),
        };

        let (warning_start, notify_start) = cfg.phase_boundaries();
        assert_eq!(warning_start, Duration::from_secs(5));
        assert_eq!(notify_start, Duration::from_secs(8));
    }

    #[test]
    fn drain_phase_for_elapsed_transitions_in_order() {
        let cfg = DrainPhaseConfig::default();
        let (warning_start, notify_start) = cfg.phase_boundaries();

        assert_eq!(
            phase_for_elapsed(Duration::from_secs(0), warning_start, notify_start),
            DrainPhase::Graceful
        );
        assert_eq!(
            phase_for_elapsed(warning_start, warning_start, notify_start),
            DrainPhase::Warning
        );
        assert_eq!(
            phase_for_elapsed(notify_start, warning_start, notify_start),
            DrainPhase::Notify
        );
    }

    #[test]
    fn drain_stats_records_peak_and_average() {
        let mut stats = DrainStats::default();

        stats.record(3);
        stats.record(5);
        stats.record(1);

        assert_eq!(stats.sample_count, 3);
        assert_eq!(stats.total_in_flight, 9);
        assert_eq!(stats.peak_in_flight, 5);
        assert!((stats.average_in_flight() - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn drain_warning_sample_emits_every_twentieth() {
        assert!(!should_emit_warning_sample(1));
        assert!(!should_emit_warning_sample(19));
        assert!(should_emit_warning_sample(20));
        assert!(!should_emit_warning_sample(21));
        assert!(should_emit_warning_sample(40));
    }
}
