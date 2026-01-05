//! Per-run event tracking with thread-safe counters.
//!
//! Tracks event counts and drop counts for each diagnostic run.
//! Uses DashMap for concurrent access from multiple emitters.

use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Summary of a completed diagnostic run.
#[derive(Debug, Clone, Default)]
pub struct RunSummary {
    /// Number of events successfully emitted for this run
    pub events_emitted: u64,
    /// Number of events dropped due to backpressure
    pub events_dropped: u64,
    /// Timestamp when run started (unix ms)
    pub started_at_ms: u64,
}

/// Per-run counters for tracking events and drops.
struct RunCounters {
    events: AtomicU64,
    drops: AtomicU64,
    started_at_ms: u64,
}

/// Thread-safe tracker for per-run event and drop counts.
///
/// Uses DashMap for concurrent access from multiple emitters.
/// Each run is identified by a string key (typically the run_id/trace_id).
pub struct RunTracker {
    runs: DashMap<String, RunCounters>,
}

impl RunTracker {
    /// Create a new empty RunTracker.
    pub fn new() -> Self {
        Self {
            runs: DashMap::new(),
        }
    }

    /// Start tracking a new run.
    ///
    /// Initializes counters for the given run_id.
    /// If the run already exists, this is a no-op.
    pub fn start_run(&self, run_id: &str) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        self.runs.entry(run_id.to_string()).or_insert(RunCounters {
            events: AtomicU64::new(0),
            drops: AtomicU64::new(0),
            started_at_ms: now_ms,
        });
    }

    /// Increment the event count for a run.
    ///
    /// If the run doesn't exist, this is a no-op.
    pub fn increment_events(&self, run_id: &str) {
        if let Some(counters) = self.runs.get(run_id) {
            counters.events.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Increment the drop count for a run.
    ///
    /// If the run doesn't exist, this is a no-op.
    pub fn increment_drops(&self, run_id: &str) {
        if let Some(counters) = self.runs.get(run_id) {
            counters.drops.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get the current event count for a run.
    ///
    /// Returns 0 if the run doesn't exist.
    pub fn event_count(&self, run_id: &str) -> u64 {
        self.runs
            .get(run_id)
            .map(|c| c.events.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Get the current drop count for a run.
    ///
    /// Returns 0 if the run doesn't exist.
    pub fn drop_count(&self, run_id: &str) -> u64 {
        self.runs
            .get(run_id)
            .map(|c| c.drops.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// End a run and return its summary.
    ///
    /// Removes the run from tracking and returns the final counts.
    /// Returns a default summary if the run doesn't exist.
    pub fn end_run(&self, run_id: &str) -> RunSummary {
        if let Some((_, counters)) = self.runs.remove(run_id) {
            RunSummary {
                events_emitted: counters.events.load(Ordering::Relaxed),
                events_dropped: counters.drops.load(Ordering::Relaxed),
                started_at_ms: counters.started_at_ms,
            }
        } else {
            RunSummary::default()
        }
    }

    /// Get the count of currently active runs.
    pub fn active_run_count(&self) -> usize {
        self.runs.len()
    }

    /// Check if a run is currently being tracked.
    pub fn has_run(&self, run_id: &str) -> bool {
        self.runs.contains_key(run_id)
    }
}

impl Default for RunTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_and_end_run() {
        let tracker = RunTracker::new();

        tracker.start_run("run-1");
        assert!(tracker.has_run("run-1"));
        assert_eq!(tracker.active_run_count(), 1);

        let summary = tracker.end_run("run-1");
        assert!(!tracker.has_run("run-1"));
        assert_eq!(tracker.active_run_count(), 0);
        assert_eq!(summary.events_emitted, 0);
        assert_eq!(summary.events_dropped, 0);
    }

    #[test]
    fn test_increment_events() {
        let tracker = RunTracker::new();
        tracker.start_run("run-1");

        tracker.increment_events("run-1");
        tracker.increment_events("run-1");
        tracker.increment_events("run-1");

        assert_eq!(tracker.event_count("run-1"), 3);

        let summary = tracker.end_run("run-1");
        assert_eq!(summary.events_emitted, 3);
    }

    #[test]
    fn test_increment_drops() {
        let tracker = RunTracker::new();
        tracker.start_run("run-1");

        tracker.increment_drops("run-1");
        tracker.increment_drops("run-1");

        assert_eq!(tracker.drop_count("run-1"), 2);

        let summary = tracker.end_run("run-1");
        assert_eq!(summary.events_dropped, 2);
    }

    #[test]
    fn test_multiple_runs() {
        let tracker = RunTracker::new();

        tracker.start_run("run-1");
        tracker.start_run("run-2");

        tracker.increment_events("run-1");
        tracker.increment_events("run-1");
        tracker.increment_events("run-2");
        tracker.increment_drops("run-2");

        assert_eq!(tracker.event_count("run-1"), 2);
        assert_eq!(tracker.event_count("run-2"), 1);
        assert_eq!(tracker.drop_count("run-2"), 1);

        let summary1 = tracker.end_run("run-1");
        assert_eq!(summary1.events_emitted, 2);
        assert_eq!(summary1.events_dropped, 0);

        // run-2 still active
        assert!(tracker.has_run("run-2"));

        let summary2 = tracker.end_run("run-2");
        assert_eq!(summary2.events_emitted, 1);
        assert_eq!(summary2.events_dropped, 1);
    }

    #[test]
    fn test_increment_on_nonexistent_run() {
        let tracker = RunTracker::new();

        // Should not panic
        tracker.increment_events("nonexistent");
        tracker.increment_drops("nonexistent");

        assert_eq!(tracker.event_count("nonexistent"), 0);
        assert_eq!(tracker.drop_count("nonexistent"), 0);
    }

    #[test]
    fn test_end_nonexistent_run() {
        let tracker = RunTracker::new();

        let summary = tracker.end_run("nonexistent");
        assert_eq!(summary.events_emitted, 0);
        assert_eq!(summary.events_dropped, 0);
    }
}
