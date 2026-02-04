//! Adapter Activation Tracking
//!
//! Tracks which adapters are frequently used to support the hybrid
//! hot/cold adapter strategy. Adapters with high activation rates
//! are promoted to "hot" status and cached in the model server.

use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, info};

use crate::DEFAULT_HOT_ADAPTER_THRESHOLD;

/// Activation statistics for a single adapter
#[derive(Debug)]
pub struct AdapterStats {
    /// Adapter ID
    pub adapter_id: u32,

    /// Adapter name (for display)
    pub adapter_name: String,

    /// Total activation count (requests using this adapter)
    pub activation_count: AtomicU64,

    /// Last activation timestamp
    pub last_activated: parking_lot::RwLock<Instant>,

    /// Whether this adapter is currently "hot" (cached in model server)
    pub is_hot: parking_lot::RwLock<bool>,
}

impl AdapterStats {
    /// Create new stats for an adapter
    pub fn new(adapter_id: u32, adapter_name: String) -> Self {
        Self {
            adapter_id,
            adapter_name,
            activation_count: AtomicU64::new(0),
            last_activated: parking_lot::RwLock::new(Instant::now()),
            is_hot: parking_lot::RwLock::new(false),
        }
    }

    /// Record an activation
    pub fn record_activation(&self) {
        self.activation_count.fetch_add(1, Ordering::Relaxed);
        *self.last_activated.write() = Instant::now();
    }

    /// Get activation count
    pub fn count(&self) -> u64 {
        self.activation_count.load(Ordering::Relaxed)
    }

    /// Get seconds since last activation
    pub fn seconds_since_activation(&self) -> f64 {
        self.last_activated.read().elapsed().as_secs_f64()
    }

    /// Get hot status
    pub fn is_hot(&self) -> bool {
        *self.is_hot.read()
    }

    /// Set hot status
    pub fn set_hot(&self, hot: bool) {
        *self.is_hot.write() = hot;
    }
}

/// Tracks adapter activations and manages hot/cold promotion
pub struct ActivationTracker {
    /// Per-adapter statistics
    stats: DashMap<u32, AdapterStats>,

    /// Total request count for computing rates
    total_requests: AtomicU64,

    /// Threshold for hot adapter promotion (0.0-1.0)
    hot_threshold: f64,

    /// Cooldown period before demoting a hot adapter (seconds)
    demotion_cooldown_secs: f64,

    /// Window size for rate calculation
    #[allow(dead_code)] // Reserved for rate-based hot adapter promotion
    rate_window: Duration,

    /// Stats update timestamp
    last_recalc: parking_lot::RwLock<Instant>,
}

impl ActivationTracker {
    /// Create a new activation tracker
    pub fn new(hot_threshold: f64) -> Self {
        Self {
            stats: DashMap::new(),
            total_requests: AtomicU64::new(0),
            hot_threshold,
            demotion_cooldown_secs: 300.0,          // 5 minutes
            rate_window: Duration::from_secs(3600), // 1 hour
            last_recalc: parking_lot::RwLock::new(Instant::now()),
        }
    }

    /// Create with default threshold
    pub fn with_default_threshold() -> Self {
        Self::new(DEFAULT_HOT_ADAPTER_THRESHOLD)
    }

    /// Record activations for a set of adapters (from a single request)
    pub fn record_request(&self, adapter_ids: &[u32]) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);

        for &adapter_id in adapter_ids {
            if let Some(stats) = self.stats.get(&adapter_id) {
                stats.record_activation();
            }
        }
    }

    /// Register a new adapter
    pub fn register_adapter(&self, adapter_id: u32, adapter_name: String) {
        self.stats
            .entry(adapter_id)
            .or_insert_with(|| AdapterStats::new(adapter_id, adapter_name));
    }

    /// Unregister an adapter
    pub fn unregister_adapter(&self, adapter_id: u32) {
        self.stats.remove(&adapter_id);
    }

    /// Get activation rate for an adapter (0.0-1.0)
    pub fn activation_rate(&self, adapter_id: u32) -> f64 {
        let total = self.total_requests.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }

        if let Some(stats) = self.stats.get(&adapter_id) {
            stats.count() as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Check if an adapter should be hot based on activation rate
    pub fn should_be_hot(&self, adapter_id: u32) -> bool {
        self.activation_rate(adapter_id) >= self.hot_threshold
    }

    /// Recalculate hot/cold status for all adapters
    ///
    /// Returns: (newly_hot, newly_cold) - lists of adapter IDs that changed status
    pub fn recalculate_status(&self) -> (Vec<u32>, Vec<u32>) {
        let mut newly_hot = Vec::new();
        let mut newly_cold = Vec::new();

        let total = self.total_requests.load(Ordering::Relaxed);
        if total == 0 {
            return (newly_hot, newly_cold);
        }

        for entry in self.stats.iter() {
            let stats = entry.value();
            let rate = stats.count() as f64 / total as f64;
            let was_hot = stats.is_hot();
            let should_be_hot = rate >= self.hot_threshold;

            if !was_hot && should_be_hot {
                // Promote to hot
                stats.set_hot(true);
                newly_hot.push(stats.adapter_id);

                info!(
                    adapter_id = stats.adapter_id,
                    adapter_name = %stats.adapter_name,
                    rate = rate,
                    threshold = self.hot_threshold,
                    "Promoting adapter to HOT"
                );
            } else if was_hot && !should_be_hot {
                // Check cooldown before demoting
                if stats.seconds_since_activation() > self.demotion_cooldown_secs {
                    stats.set_hot(false);
                    newly_cold.push(stats.adapter_id);

                    info!(
                        adapter_id = stats.adapter_id,
                        adapter_name = %stats.adapter_name,
                        rate = rate,
                        threshold = self.hot_threshold,
                        cooldown_secs = stats.seconds_since_activation(),
                        "Demoting adapter to COLD"
                    );
                } else {
                    debug!(
                        adapter_id = stats.adapter_id,
                        seconds_since_activation = stats.seconds_since_activation(),
                        cooldown_required = self.demotion_cooldown_secs,
                        "Skipping demotion - within cooldown period"
                    );
                }
            }
        }

        *self.last_recalc.write() = Instant::now();
        (newly_hot, newly_cold)
    }

    /// Get all hot adapter IDs
    pub fn hot_adapters(&self) -> Vec<u32> {
        self.stats
            .iter()
            .filter(|e| e.value().is_hot())
            .map(|e| e.value().adapter_id)
            .collect()
    }

    /// Get all cold adapter IDs
    pub fn cold_adapters(&self) -> Vec<u32> {
        self.stats
            .iter()
            .filter(|e| !e.value().is_hot())
            .map(|e| e.value().adapter_id)
            .collect()
    }

    /// Get stats for a specific adapter
    pub fn get_stats(&self, adapter_id: u32) -> Option<AdapterStatsSnapshot> {
        self.stats.get(&adapter_id).map(|entry| {
            let stats = entry.value();
            let total = self.total_requests.load(Ordering::Relaxed);
            let rate = if total > 0 {
                stats.count() as f64 / total as f64
            } else {
                0.0
            };

            AdapterStatsSnapshot {
                adapter_id: stats.adapter_id,
                adapter_name: stats.adapter_name.clone(),
                activation_count: stats.count(),
                activation_rate: rate,
                is_hot: stats.is_hot(),
                seconds_since_activation: stats.seconds_since_activation(),
            }
        })
    }

    /// Get all adapter stats
    pub fn all_stats(&self) -> Vec<AdapterStatsSnapshot> {
        let total = self.total_requests.load(Ordering::Relaxed);

        self.stats
            .iter()
            .map(|entry| {
                let stats = entry.value();
                let rate = if total > 0 {
                    stats.count() as f64 / total as f64
                } else {
                    0.0
                };

                AdapterStatsSnapshot {
                    adapter_id: stats.adapter_id,
                    adapter_name: stats.adapter_name.clone(),
                    activation_count: stats.count(),
                    activation_rate: rate,
                    is_hot: stats.is_hot(),
                    seconds_since_activation: stats.seconds_since_activation(),
                }
            })
            .collect()
    }

    /// Get total request count
    pub fn total_requests(&self) -> u64 {
        self.total_requests.load(Ordering::Relaxed)
    }

    /// Reset all counters (for testing or periodic cleanup)
    pub fn reset(&self) {
        self.total_requests.store(0, Ordering::Relaxed);
        for entry in self.stats.iter() {
            entry.value().activation_count.store(0, Ordering::Relaxed);
        }
    }
}

/// Snapshot of adapter statistics
#[derive(Debug, Clone)]
pub struct AdapterStatsSnapshot {
    pub adapter_id: u32,
    pub adapter_name: String,
    pub activation_count: u64,
    pub activation_rate: f64,
    pub is_hot: bool,
    pub seconds_since_activation: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activation_recording() {
        let tracker = ActivationTracker::new(0.10); // 10% threshold

        tracker.register_adapter(1, "adapter-1".to_string());
        tracker.register_adapter(2, "adapter-2".to_string());

        // Record 10 requests - 8 use adapter 1, 2 use adapter 2
        for _ in 0..8 {
            tracker.record_request(&[1]);
        }
        for _ in 0..2 {
            tracker.record_request(&[2]);
        }

        assert_eq!(tracker.total_requests(), 10);
        assert!((tracker.activation_rate(1) - 0.8).abs() < 0.01);
        assert!((tracker.activation_rate(2) - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_hot_promotion() {
        let tracker = ActivationTracker::new(0.20); // 20% threshold

        tracker.register_adapter(1, "adapter-1".to_string());
        tracker.register_adapter(2, "adapter-2".to_string());

        // Record 100 requests: adapter 1 gets 30, adapter 2 gets 5
        // Adapter 1: 30% (above 20% threshold), adapter 2: 5% (below threshold)
        for _ in 0..30 {
            tracker.record_request(&[1]);
        }
        for _ in 0..5 {
            tracker.record_request(&[2]);
        }
        // Add 65 empty requests to bring total to 100
        for _ in 0..65 {
            tracker.record_request(&[]);
        }

        // Initial state - nothing is hot
        assert!(!tracker.stats.get(&1).unwrap().is_hot());
        assert!(!tracker.stats.get(&2).unwrap().is_hot());

        // Recalculate status
        let (newly_hot, _) = tracker.recalculate_status();

        // Only adapter 1 should be promoted (30% > 20%)
        assert!(newly_hot.contains(&1));
        assert!(!newly_hot.contains(&2));
        assert!(tracker.stats.get(&1).unwrap().is_hot());
        assert!(!tracker.stats.get(&2).unwrap().is_hot());
    }

    #[test]
    fn test_snapshot() {
        let tracker = ActivationTracker::new(0.10);
        tracker.register_adapter(1, "test-adapter".to_string());
        tracker.record_request(&[1]);

        let stats = tracker.get_stats(1).unwrap();
        assert_eq!(stats.adapter_id, 1);
        assert_eq!(stats.adapter_name, "test-adapter");
        assert_eq!(stats.activation_count, 1);
        assert!((stats.activation_rate - 1.0).abs() < 0.01);
    }
}
