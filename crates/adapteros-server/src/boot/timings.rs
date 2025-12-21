//! Boot phase timing tracker.
//!
//! Tracks the duration of each boot phase for observability and debugging.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::info;

/// Tracks timing information for boot phases.
#[derive(Debug)]
pub struct BootTimings {
    /// Start time of the entire boot sequence
    boot_start: Instant,
    /// Per-phase timing data
    phases: HashMap<String, PhaseTiming>,
    /// Order in which phases were started
    phase_order: Vec<String>,
    /// Currently active phase (if any)
    current_phase: Option<String>,
}

#[derive(Debug, Clone)]
struct PhaseTiming {
    start: Instant,
    end: Option<Instant>,
    duration: Option<Duration>,
}

impl BootTimings {
    /// Create a new boot timing tracker.
    pub fn new() -> Self {
        Self {
            boot_start: Instant::now(),
            phases: HashMap::new(),
            phase_order: Vec::new(),
            current_phase: None,
        }
    }

    /// Start timing a new phase.
    ///
    /// If there's a current phase in progress, it will be automatically ended.
    pub fn start_phase(&mut self, name: &str) {
        // Auto-end previous phase if one is in progress
        if let Some(current) = self.current_phase.take() {
            self.end_phase(&current);
        }

        let timing = PhaseTiming {
            start: Instant::now(),
            end: None,
            duration: None,
        };

        self.phases.insert(name.to_string(), timing);
        self.phase_order.push(name.to_string());
        self.current_phase = Some(name.to_string());
    }

    /// End timing for a specific phase.
    pub fn end_phase(&mut self, name: &str) {
        if let Some(timing) = self.phases.get_mut(name) {
            let end = Instant::now();
            timing.end = Some(end);
            timing.duration = Some(end.duration_since(timing.start));
        }

        if self.current_phase.as_deref() == Some(name) {
            self.current_phase = None;
        }
    }

    /// Get the duration of a specific phase.
    pub fn phase_duration(&self, name: &str) -> Option<Duration> {
        self.phases.get(name).and_then(|t| t.duration)
    }

    /// Get the total boot duration so far.
    pub fn total_duration(&self) -> Duration {
        self.boot_start.elapsed()
    }

    /// Log a summary of all phase timings.
    pub fn log_summary(&self) {
        let total = self.total_duration();

        info!(
            target: "boot",
            total_ms = total.as_millis() as u64,
            "Boot timing summary"
        );

        for phase_name in &self.phase_order {
            if let Some(timing) = self.phases.get(phase_name) {
                if let Some(duration) = timing.duration {
                    let percentage = (duration.as_secs_f64() / total.as_secs_f64()) * 100.0;
                    info!(
                        target: "boot",
                        phase = %phase_name,
                        duration_ms = duration.as_millis() as u64,
                        percentage = format!("{:.1}%", percentage),
                        "Phase timing"
                    );
                }
            }
        }
    }

    /// Get phase durations as a map (for boot report).
    pub fn as_duration_map(&self) -> HashMap<String, u64> {
        self.phases
            .iter()
            .filter_map(|(name, timing)| {
                timing
                    .duration
                    .map(|d| (name.clone(), d.as_millis() as u64))
            })
            .collect()
    }
}

impl Default for BootTimings {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_phase_timing() {
        let mut timings = BootTimings::new();

        timings.start_phase("config");
        sleep(Duration::from_millis(10));
        timings.end_phase("config");

        let duration = timings.phase_duration("config").unwrap();
        assert!(duration.as_millis() >= 10);
    }

    #[test]
    fn test_auto_end_previous_phase() {
        let mut timings = BootTimings::new();

        timings.start_phase("phase1");
        sleep(Duration::from_millis(5));
        timings.start_phase("phase2"); // Should auto-end phase1
        sleep(Duration::from_millis(5));
        timings.end_phase("phase2");

        // Both phases should have durations
        assert!(timings.phase_duration("phase1").is_some());
        assert!(timings.phase_duration("phase2").is_some());
    }

    #[test]
    fn test_duration_map() {
        let mut timings = BootTimings::new();

        timings.start_phase("a");
        timings.end_phase("a");
        timings.start_phase("b");
        timings.end_phase("b");

        let map = timings.as_duration_map();
        assert!(map.contains_key("a"));
        assert!(map.contains_key("b"));
    }
}
