//! Predictive adapter scheduler for proactive resource management.
//!
//! Instead of reacting to memory pressure, adapter misses, and queue
//! imbalances after they happen, this module analyses trends in system
//! snapshots and produces scheduling decisions *before* conditions
//! become critical.
//!
//! Three prediction strategies:
//!
//! 1. **Pre-eviction** — when memory pressure is trending upward and has
//!    crossed the early-warning threshold, evict the coldest loaded adapter
//!    before the system hits critical pressure.
//!
//! 2. **Pre-loading** — when an unloaded adapter's heat score is rising and
//!    above the pre-load threshold, suggest loading it before a request
//!    actually arrives.
//!
//! 3. **Slot rebalancing** — when the training queue depth changes,
//!    dynamically redistribute inference/training semaphore permits so
//!    neither workload starves the other.
//!
//! The scheduler is **pure computation** — it takes snapshots in and
//! produces decisions out. All I/O (loading adapters, evicting, resizing
//! semaphores) is the caller's responsibility.
//!
//! Disabled by default (`PredictiveSchedulerConfig::enabled = false`).
//! When disabled, `evaluate` always returns `[SchedulingDecision::Hold]`.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use tracing::debug;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for predictive scheduling.
#[derive(Debug, Clone)]
pub struct PredictiveSchedulerConfig {
    /// Enable predictive scheduling (default: false — falls back to reactive).
    pub enabled: bool,
    /// How often to run prediction cycles (default: 5 s).
    pub prediction_interval: Duration,
    /// Number of historical samples to keep for trend analysis (default: 60).
    pub history_window: usize,
    /// Memory pressure threshold to trigger pre-eviction (0.0–1.0, default: 0.7).
    pub pre_eviction_pressure_threshold: f64,
    /// Memory pressure threshold considered critical (default: 0.9).
    pub critical_pressure_threshold: f64,
    /// Adapter heat score threshold for pre-loading (default: 0.6).
    pub pre_load_heat_threshold: f64,
    /// Minimum inference slots to always reserve (default: 2).
    pub min_inference_slots: usize,
    /// Minimum training slots to always reserve (default: 1).
    pub min_training_slots: usize,
    /// Total slots available for inference + training (default: 8).
    pub total_slots: usize,
}

impl Default for PredictiveSchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            prediction_interval: Duration::from_secs(5),
            history_window: 60,
            pre_eviction_pressure_threshold: 0.7,
            critical_pressure_threshold: 0.9,
            pre_load_heat_threshold: 0.6,
            min_inference_slots: 2,
            min_training_slots: 1,
            total_slots: 8,
        }
    }
}

// ---------------------------------------------------------------------------
// Snapshot / decision types
// ---------------------------------------------------------------------------

/// A snapshot of system state at a point in time.
#[derive(Debug, Clone)]
pub struct SystemSnapshot {
    pub timestamp: Instant,
    /// Memory pressure 0.0 (idle) to 1.0 (critical).
    pub memory_pressure: f64,
    /// Currently loaded adapter IDs.
    pub loaded_adapters: Vec<String>,
    /// Per-adapter heat scores (higher = more recently/frequently accessed).
    pub adapter_heat: HashMap<String, f64>,
    /// Number of pending training jobs.
    pub training_queue_depth: usize,
    /// Current inference concurrency.
    pub active_inference: usize,
    /// Current training concurrency.
    pub active_training: usize,
}

/// A scheduling decision produced by the predictor.
#[derive(Debug, Clone)]
pub enum SchedulingDecision {
    /// Pre-evict an adapter to free memory before pressure becomes critical.
    PreEvict {
        adapter_id: String,
        reason: String,
        predicted_pressure: f64,
    },
    /// Pre-load an adapter that is trending hot.
    PreLoad {
        adapter_id: String,
        heat_score: f64,
        /// Positive = warming up.
        heat_trend: f64,
    },
    /// Rebalance inference/training slot allocation.
    Rebalance {
        inference_slots: usize,
        training_slots: usize,
        reason: String,
    },
    /// No action needed.
    Hold,
}

/// Trend direction for a metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trend {
    Rising,
    Stable,
    Falling,
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

/// The predictive scheduler.
///
/// Feed it [`SystemSnapshot`]s via [`evaluate`](Self::evaluate) and it
/// returns zero or more [`SchedulingDecision`]s based on trend analysis.
pub struct PredictiveScheduler {
    config: PredictiveSchedulerConfig,
    /// Historical system snapshots for trend analysis.
    history: VecDeque<SystemSnapshot>,
    /// Current slot allocation — inference.
    inference_slots: usize,
    /// Current slot allocation — training.
    training_slots: usize,
}

impl PredictiveScheduler {
    pub fn new(config: PredictiveSchedulerConfig) -> Self {
        let inference_slots = config.total_slots.saturating_sub(config.min_training_slots);
        let training_slots = config.min_training_slots;
        Self {
            config,
            history: VecDeque::new(),
            inference_slots,
            training_slots,
        }
    }

    /// Record a new system snapshot and produce scheduling decisions.
    pub fn evaluate(&mut self, snapshot: SystemSnapshot) -> Vec<SchedulingDecision> {
        self.history.push_back(snapshot.clone());
        while self.history.len() > self.config.history_window {
            self.history.pop_front();
        }

        if !self.config.enabled || self.history.len() < 3 {
            return vec![SchedulingDecision::Hold];
        }

        let mut decisions = Vec::new();

        // 1. Memory pressure prediction
        if let Some(decision) = self.predict_memory_pressure(&snapshot) {
            decisions.push(decision);
        }

        // 2. Adapter heat prediction
        decisions.extend(self.predict_adapter_heat(&snapshot));

        // 3. Slot rebalancing
        if let Some(decision) = self.predict_slot_rebalance(&snapshot) {
            decisions.push(decision);
        }

        if decisions.is_empty() {
            decisions.push(SchedulingDecision::Hold);
        }

        decisions
    }

    /// Current slot allocation: `(inference, training)`.
    pub fn current_slots(&self) -> (usize, usize) {
        (self.inference_slots, self.training_slots)
    }

    // -----------------------------------------------------------------------
    // Prediction strategies
    // -----------------------------------------------------------------------

    /// Predict memory pressure trend and suggest pre-eviction.
    fn predict_memory_pressure(&self, current: &SystemSnapshot) -> Option<SchedulingDecision> {
        let trend = self.compute_trend(|s| s.memory_pressure);

        if trend == Trend::Rising
            && current.memory_pressure > self.config.pre_eviction_pressure_threshold
        {
            if let Some(coldest) = self.find_coldest_adapter(current) {
                let predicted = self.linear_extrapolate(|s| s.memory_pressure, 5);
                debug!(
                    adapter_id = %coldest,
                    current_pressure = current.memory_pressure,
                    predicted_pressure = predicted,
                    "Pre-eviction triggered"
                );
                return Some(SchedulingDecision::PreEvict {
                    adapter_id: coldest,
                    reason: format!(
                        "Memory pressure rising ({:.1}% -> predicted {:.1}%)",
                        current.memory_pressure * 100.0,
                        predicted * 100.0,
                    ),
                    predicted_pressure: predicted,
                });
            }
        }
        None
    }

    /// Predict which adapters are trending hot and suggest pre-loading.
    fn predict_adapter_heat(&self, current: &SystemSnapshot) -> Vec<SchedulingDecision> {
        let mut decisions = Vec::new();

        for (adapter_id, &heat) in &current.adapter_heat {
            if current.loaded_adapters.contains(adapter_id) {
                continue; // Already loaded
            }
            if heat < self.config.pre_load_heat_threshold {
                continue; // Not hot enough
            }

            let heat_trend = self.adapter_heat_trend(adapter_id);
            if heat_trend > 0.0 {
                debug!(
                    adapter_id = %adapter_id,
                    heat_score = heat,
                    heat_trend = heat_trend,
                    "Pre-load candidate detected"
                );
                decisions.push(SchedulingDecision::PreLoad {
                    adapter_id: adapter_id.clone(),
                    heat_score: heat,
                    heat_trend,
                });
            }
        }

        // Sort by heat descending for determinism, then truncate to one per
        // cycle to avoid thrashing.
        decisions.sort_by(|a, b| {
            let heat_a = match a {
                SchedulingDecision::PreLoad { heat_score, .. } => *heat_score,
                _ => 0.0,
            };
            let heat_b = match b {
                SchedulingDecision::PreLoad { heat_score, .. } => *heat_score,
                _ => 0.0,
            };
            heat_b
                .partial_cmp(&heat_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        decisions.truncate(1);
        decisions
    }

    /// Predict optimal slot allocation based on training queue depth.
    fn predict_slot_rebalance(&mut self, current: &SystemSnapshot) -> Option<SchedulingDecision> {
        let queue_trend = self.compute_trend(|s| s.training_queue_depth as f64);

        let headroom = self
            .config
            .total_slots
            .saturating_sub(self.config.min_inference_slots)
            .saturating_sub(self.config.min_training_slots);

        let ideal_training = if current.training_queue_depth > 0 {
            let extra = current.training_queue_depth.min(headroom);
            self.config.min_training_slots + extra
        } else {
            self.config.min_training_slots
        };

        let ideal_inference = self.config.total_slots.saturating_sub(ideal_training);

        // Only rebalance when the allocation actually changes and respects
        // minimum guarantees.
        if ideal_inference != self.inference_slots
            && ideal_inference >= self.config.min_inference_slots
            && ideal_training >= self.config.min_training_slots
        {
            let old_inference = self.inference_slots;
            let old_training = self.training_slots;
            self.inference_slots = ideal_inference;
            self.training_slots = ideal_training;

            debug!(
                old_inference,
                old_training,
                new_inference = ideal_inference,
                new_training = ideal_training,
                queue_depth = current.training_queue_depth,
                "Slot rebalance triggered"
            );

            return Some(SchedulingDecision::Rebalance {
                inference_slots: ideal_inference,
                training_slots: ideal_training,
                reason: format!(
                    "Training queue={}, trend={:?} (was {}i/{}t, now {}i/{}t)",
                    current.training_queue_depth,
                    queue_trend,
                    old_inference,
                    old_training,
                    ideal_inference,
                    ideal_training,
                ),
            });
        }

        None
    }

    // -----------------------------------------------------------------------
    // Trend / extrapolation helpers
    // -----------------------------------------------------------------------

    /// Compute trend direction for a metric over recent history.
    fn compute_trend(&self, extract: impl Fn(&SystemSnapshot) -> f64) -> Trend {
        if self.history.len() < 3 {
            return Trend::Stable;
        }

        let recent: Vec<f64> = self.history.iter().rev().take(5).map(extract).collect();

        // Compare newer half average to older half average.
        let mid = recent.len() / 2;
        if mid == 0 {
            return Trend::Stable;
        }
        let older_avg: f64 = recent[mid..].iter().sum::<f64>() / (recent.len() - mid) as f64;
        let newer_avg: f64 = recent[..mid].iter().sum::<f64>() / mid as f64;

        let delta = newer_avg - older_avg;
        if delta > 0.05 {
            Trend::Rising
        } else if delta < -0.05 {
            Trend::Falling
        } else {
            Trend::Stable
        }
    }

    /// Linear extrapolation of a metric N steps into the future.
    fn linear_extrapolate(&self, extract: impl Fn(&SystemSnapshot) -> f64, steps: usize) -> f64 {
        if self.history.len() < 2 {
            return self.history.back().map(&extract).unwrap_or(0.0);
        }

        let values: Vec<f64> = self.history.iter().map(extract).collect();
        let n = values.len() as f64;
        let sum_x: f64 = (0..values.len()).map(|i| i as f64).sum();
        let sum_y: f64 = values.iter().sum();
        let sum_xy: f64 = values.iter().enumerate().map(|(i, &y)| i as f64 * y).sum();
        let sum_x2: f64 = (0..values.len()).map(|i| (i as f64).powi(2)).sum();

        let denom = n * sum_x2 - sum_x.powi(2);
        if denom.abs() < 1e-10 {
            return values.last().copied().unwrap_or(0.0);
        }

        let slope = (n * sum_xy - sum_x * sum_y) / denom;
        let intercept = (sum_y - slope * sum_x) / n;

        let predicted = intercept + slope * (values.len() + steps) as f64;
        predicted.clamp(0.0, 1.0)
    }

    /// Find the coldest loaded adapter by heat score.
    fn find_coldest_adapter(&self, current: &SystemSnapshot) -> Option<String> {
        current
            .loaded_adapters
            .iter()
            .min_by(|a, b| {
                let heat_a = current.adapter_heat.get(*a).copied().unwrap_or(0.0);
                let heat_b = current.adapter_heat.get(*b).copied().unwrap_or(0.0);
                heat_a
                    .partial_cmp(&heat_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.cmp(b)) // deterministic tie-break
            })
            .cloned()
    }

    /// Compute heat trend for a specific adapter over recent history.
    fn adapter_heat_trend(&self, adapter_id: &str) -> f64 {
        if self.history.len() < 3 {
            return 0.0;
        }

        let recent: Vec<f64> = self
            .history
            .iter()
            .rev()
            .take(5)
            .map(|s| s.adapter_heat.get(adapter_id).copied().unwrap_or(0.0))
            .collect();

        if recent.len() < 2 {
            return 0.0;
        }

        let mid = recent.len() / 2;
        let older: f64 = recent[mid..].iter().sum::<f64>() / (recent.len() - mid) as f64;
        let newer: f64 = recent[..mid].iter().sum::<f64>() / mid as f64;

        newer - older
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- helpers --

    fn default_config() -> PredictiveSchedulerConfig {
        PredictiveSchedulerConfig::default()
    }

    fn enabled_config() -> PredictiveSchedulerConfig {
        PredictiveSchedulerConfig {
            enabled: true,
            ..default_config()
        }
    }

    fn snap(
        pressure: f64,
        loaded: Vec<&str>,
        heat: Vec<(&str, f64)>,
        queue: usize,
    ) -> SystemSnapshot {
        SystemSnapshot {
            timestamp: Instant::now(),
            memory_pressure: pressure,
            loaded_adapters: loaded.into_iter().map(String::from).collect(),
            adapter_heat: heat.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            training_queue_depth: queue,
            active_inference: 0,
            active_training: 0,
        }
    }

    // -- config defaults --

    #[test]
    fn default_config_values() {
        let cfg = default_config();
        assert!(!cfg.enabled);
        assert_eq!(cfg.prediction_interval, Duration::from_secs(5));
        assert_eq!(cfg.history_window, 60);
        assert!((cfg.pre_eviction_pressure_threshold - 0.7).abs() < f64::EPSILON);
        assert!((cfg.critical_pressure_threshold - 0.9).abs() < f64::EPSILON);
        assert!((cfg.pre_load_heat_threshold - 0.6).abs() < f64::EPSILON);
        assert_eq!(cfg.min_inference_slots, 2);
        assert_eq!(cfg.min_training_slots, 1);
        assert_eq!(cfg.total_slots, 8);
    }

    #[test]
    fn initial_slot_allocation() {
        let scheduler = PredictiveScheduler::new(default_config());
        // total_slots(8) - min_training_slots(1) = 7 inference, 1 training
        assert_eq!(scheduler.current_slots(), (7, 1));
    }

    // -- disabled / insufficient history --

    #[test]
    fn disabled_scheduler_returns_hold() {
        let mut scheduler = PredictiveScheduler::new(default_config());
        for _ in 0..5 {
            let decisions = scheduler.evaluate(snap(0.5, vec![], vec![], 0));
            assert!(matches!(decisions.as_slice(), [SchedulingDecision::Hold]));
        }
    }

    #[test]
    fn insufficient_history_returns_hold() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());
        // Only 2 snapshots — need at least 3
        let d1 = scheduler.evaluate(snap(0.5, vec![], vec![], 0));
        let d2 = scheduler.evaluate(snap(0.6, vec![], vec![], 0));
        assert!(matches!(d1.as_slice(), [SchedulingDecision::Hold]));
        assert!(matches!(d2.as_slice(), [SchedulingDecision::Hold]));
    }

    // -- trend computation --

    #[test]
    fn trend_rising() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());
        // Feed increasing pressure values
        for p in &[0.2, 0.3, 0.4, 0.5, 0.6] {
            scheduler.history.push_back(snap(*p, vec![], vec![], 0));
        }
        let trend = scheduler.compute_trend(|s| s.memory_pressure);
        assert_eq!(trend, Trend::Rising);
    }

    #[test]
    fn trend_falling() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());
        for p in &[0.8, 0.7, 0.6, 0.5, 0.4] {
            scheduler.history.push_back(snap(*p, vec![], vec![], 0));
        }
        let trend = scheduler.compute_trend(|s| s.memory_pressure);
        assert_eq!(trend, Trend::Falling);
    }

    #[test]
    fn trend_stable() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());
        for p in &[0.5, 0.51, 0.49, 0.50, 0.50] {
            scheduler.history.push_back(snap(*p, vec![], vec![], 0));
        }
        let trend = scheduler.compute_trend(|s| s.memory_pressure);
        assert_eq!(trend, Trend::Stable);
    }

    #[test]
    fn trend_with_fewer_than_3_samples_is_stable() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());
        scheduler.history.push_back(snap(0.1, vec![], vec![], 0));
        scheduler.history.push_back(snap(0.9, vec![], vec![], 0));
        assert_eq!(
            scheduler.compute_trend(|s| s.memory_pressure),
            Trend::Stable,
        );
    }

    // -- linear extrapolation --

    #[test]
    fn linear_extrapolation_increasing() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());
        // 0.1, 0.2, 0.3, 0.4, 0.5 — slope ~0.1 per step
        for i in 1..=5 {
            scheduler
                .history
                .push_back(snap(i as f64 * 0.1, vec![], vec![], 0));
        }
        // 5 steps ahead: predicted ≈ 0.5 + 5*0.1 = 1.0 (clamped)
        let predicted = scheduler.linear_extrapolate(|s| s.memory_pressure, 5);
        assert!(predicted > 0.5, "predicted={predicted}, expected > 0.5");
        assert!(predicted <= 1.0, "predicted={predicted}, must be <= 1.0");
    }

    #[test]
    fn linear_extrapolation_constant() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());
        for _ in 0..5 {
            scheduler.history.push_back(snap(0.5, vec![], vec![], 0));
        }
        let predicted = scheduler.linear_extrapolate(|s| s.memory_pressure, 10);
        assert!(
            (predicted - 0.5).abs() < 0.01,
            "predicted={predicted}, expected ~0.5"
        );
    }

    #[test]
    fn linear_extrapolation_clamped_to_zero() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());
        // Strongly decreasing
        for i in (0..5).rev() {
            scheduler
                .history
                .push_back(snap(i as f64 * 0.1, vec![], vec![], 0));
        }
        let predicted = scheduler.linear_extrapolate(|s| s.memory_pressure, 20);
        assert!(
            predicted >= 0.0,
            "predicted={predicted}, must be clamped >= 0"
        );
    }

    #[test]
    fn linear_extrapolation_single_sample() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());
        scheduler.history.push_back(snap(0.42, vec![], vec![], 0));
        let predicted = scheduler.linear_extrapolate(|s| s.memory_pressure, 5);
        assert!(
            (predicted - 0.42).abs() < f64::EPSILON,
            "single sample should return that value"
        );
    }

    #[test]
    fn linear_extrapolation_empty_history() {
        let scheduler = PredictiveScheduler::new(enabled_config());
        let predicted = scheduler.linear_extrapolate(|s| s.memory_pressure, 5);
        assert!((predicted - 0.0).abs() < f64::EPSILON);
    }

    // -- pre-eviction --

    #[test]
    fn pre_eviction_triggers_on_rising_pressure_above_threshold() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());

        // Build rising pressure history above the 0.7 threshold
        let pressures = [0.60, 0.65, 0.70, 0.75, 0.80];
        for &p in &pressures {
            scheduler.evaluate(snap(
                p,
                vec!["adapter-a", "adapter-b"],
                vec![("adapter-a", 0.9), ("adapter-b", 0.1)],
                0,
            ));
        }

        // The last evaluation should have produced a PreEvict for the cold adapter
        let decisions = scheduler.evaluate(snap(
            0.85,
            vec!["adapter-a", "adapter-b"],
            vec![("adapter-a", 0.9), ("adapter-b", 0.1)],
            0,
        ));

        let pre_evict = decisions
            .iter()
            .find(|d| matches!(d, SchedulingDecision::PreEvict { .. }));
        assert!(pre_evict.is_some(), "Expected PreEvict decision");

        if let Some(SchedulingDecision::PreEvict { adapter_id, .. }) = pre_evict {
            assert_eq!(adapter_id, "adapter-b", "Should evict coldest adapter");
        }
    }

    #[test]
    fn pre_eviction_does_not_trigger_below_threshold() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());

        // Pressure rising but staying below the 0.7 threshold
        for &p in &[0.3, 0.4, 0.5, 0.55, 0.6] {
            scheduler.evaluate(snap(p, vec!["adapter-a"], vec![("adapter-a", 0.5)], 0));
        }

        let decisions =
            scheduler.evaluate(snap(0.65, vec!["adapter-a"], vec![("adapter-a", 0.5)], 0));

        let has_evict = decisions
            .iter()
            .any(|d| matches!(d, SchedulingDecision::PreEvict { .. }));
        assert!(!has_evict, "Should not evict below threshold");
    }

    #[test]
    fn pre_eviction_no_loaded_adapters() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());

        for &p in &[0.60, 0.70, 0.75, 0.80, 0.85] {
            scheduler.evaluate(snap(p, vec![], vec![], 0));
        }

        let decisions = scheduler.evaluate(snap(0.90, vec![], vec![], 0));

        let has_evict = decisions
            .iter()
            .any(|d| matches!(d, SchedulingDecision::PreEvict { .. }));
        assert!(!has_evict, "Cannot evict when nothing is loaded");
    }

    // -- cold adapter selection --

    #[test]
    fn find_coldest_adapter_picks_lowest_heat() {
        let scheduler = PredictiveScheduler::new(enabled_config());
        let current = snap(
            0.5,
            vec!["hot", "warm", "cold"],
            vec![("hot", 0.9), ("warm", 0.5), ("cold", 0.1)],
            0,
        );
        assert_eq!(
            scheduler.find_coldest_adapter(&current),
            Some("cold".to_string()),
        );
    }

    #[test]
    fn find_coldest_adapter_deterministic_tie_break() {
        let scheduler = PredictiveScheduler::new(enabled_config());
        let current = snap(
            0.5,
            vec!["beta", "alpha"],
            vec![("beta", 0.5), ("alpha", 0.5)],
            0,
        );
        // Equal heat — tie-break by ID ascending
        assert_eq!(
            scheduler.find_coldest_adapter(&current),
            Some("alpha".to_string()),
        );
    }

    #[test]
    fn find_coldest_adapter_missing_heat_defaults_to_zero() {
        let scheduler = PredictiveScheduler::new(enabled_config());
        let current = snap(0.5, vec!["known", "unknown"], vec![("known", 0.3)], 0);
        // "unknown" has no heat entry → defaults to 0.0 → coldest
        assert_eq!(
            scheduler.find_coldest_adapter(&current),
            Some("unknown".to_string()),
        );
    }

    // -- pre-loading --

    #[test]
    fn pre_load_triggers_on_rising_heat_above_threshold() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());

        // adapter-x is not loaded, heat rising from 0.5 to 0.8
        for &h in &[0.50, 0.55, 0.60, 0.65, 0.70] {
            scheduler.evaluate(snap(
                0.3,
                vec!["other"],
                vec![("adapter-x", h), ("other", 0.5)],
                0,
            ));
        }

        let decisions = scheduler.evaluate(snap(
            0.3,
            vec!["other"],
            vec![("adapter-x", 0.80), ("other", 0.5)],
            0,
        ));

        let pre_load = decisions
            .iter()
            .find(|d| matches!(d, SchedulingDecision::PreLoad { .. }));
        assert!(pre_load.is_some(), "Expected PreLoad decision");

        if let Some(SchedulingDecision::PreLoad { adapter_id, .. }) = pre_load {
            assert_eq!(adapter_id, "adapter-x");
        }
    }

    #[test]
    fn pre_load_skips_already_loaded_adapters() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());

        for &h in &[0.50, 0.60, 0.70, 0.80, 0.90] {
            scheduler.evaluate(snap(
                0.3,
                vec!["adapter-x"], // already loaded
                vec![("adapter-x", h)],
                0,
            ));
        }

        let decisions =
            scheduler.evaluate(snap(0.3, vec!["adapter-x"], vec![("adapter-x", 0.95)], 0));

        let has_preload = decisions
            .iter()
            .any(|d| matches!(d, SchedulingDecision::PreLoad { .. }));
        assert!(!has_preload, "Should not pre-load already loaded adapter");
    }

    #[test]
    fn pre_load_skips_below_heat_threshold() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());

        // Heat stays below 0.6 threshold
        for &h in &[0.10, 0.20, 0.30, 0.40, 0.50] {
            scheduler.evaluate(snap(0.3, vec![], vec![("cool-adapter", h)], 0));
        }

        let decisions = scheduler.evaluate(snap(0.3, vec![], vec![("cool-adapter", 0.55)], 0));

        let has_preload = decisions
            .iter()
            .any(|d| matches!(d, SchedulingDecision::PreLoad { .. }));
        assert!(!has_preload, "Should not pre-load below heat threshold");
    }

    #[test]
    fn pre_load_limits_to_one_per_cycle() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());

        // Two hot unloaded adapters
        for _ in 0..5 {
            scheduler.evaluate(snap(0.3, vec![], vec![("a", 0.7), ("b", 0.8)], 0));
        }

        let decisions = scheduler.evaluate(snap(0.3, vec![], vec![("a", 0.75), ("b", 0.85)], 0));

        let preload_count = decisions
            .iter()
            .filter(|d| matches!(d, SchedulingDecision::PreLoad { .. }))
            .count();
        assert!(
            preload_count <= 1,
            "Should limit to one pre-load per cycle, got {preload_count}"
        );
    }

    // -- slot rebalancing --

    #[test]
    fn rebalance_allocates_more_training_slots_with_queue() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());
        assert_eq!(scheduler.current_slots(), (7, 1));

        // Feed snapshots with increasing training queue
        for q in &[0, 1, 2, 3, 4] {
            scheduler.evaluate(snap(0.3, vec![], vec![], *q));
        }

        let decisions = scheduler.evaluate(snap(0.3, vec![], vec![], 5));

        let rebalance = decisions
            .iter()
            .find(|d| matches!(d, SchedulingDecision::Rebalance { .. }));
        assert!(rebalance.is_some(), "Expected Rebalance decision");

        if let Some(SchedulingDecision::Rebalance { training_slots, .. }) = rebalance {
            assert!(
                *training_slots > 1,
                "Training slots should increase with queue depth"
            );
        }
    }

    #[test]
    fn rebalance_respects_minimum_inference_slots() {
        let cfg = PredictiveSchedulerConfig {
            enabled: true,
            min_inference_slots: 2,
            min_training_slots: 1,
            total_slots: 8,
            ..default_config()
        };
        let mut scheduler = PredictiveScheduler::new(cfg);

        // Very deep training queue — should not reduce inference below 2
        for _ in 0..5 {
            scheduler.evaluate(snap(0.3, vec![], vec![], 100));
        }
        let decisions = scheduler.evaluate(snap(0.3, vec![], vec![], 100));

        let (inf, train) = scheduler.current_slots();
        assert!(
            inf >= 2,
            "Inference slots must not drop below minimum, got {inf}"
        );
        assert!(
            train >= 1,
            "Training slots must not drop below minimum, got {train}"
        );
        assert_eq!(inf + train, 8, "Total slots must equal configured total");
    }

    #[test]
    fn rebalance_restores_when_queue_drains() {
        let cfg = PredictiveSchedulerConfig {
            enabled: true,
            ..default_config()
        };
        let mut scheduler = PredictiveScheduler::new(cfg);

        // Ramp up training queue
        for q in 0..6 {
            scheduler.evaluate(snap(0.3, vec![], vec![], q));
        }

        // Drain queue
        for q in (0..=5).rev() {
            scheduler.evaluate(snap(0.3, vec![], vec![], q));
        }

        let decisions = scheduler.evaluate(snap(0.3, vec![], vec![], 0));
        let (inf, train) = scheduler.current_slots();

        // With queue at 0, training should be at minimum
        assert_eq!(
            train, 1,
            "Training should return to minimum when queue is empty"
        );
        assert_eq!(inf, 7, "Inference should reclaim freed slots");
    }

    #[test]
    fn no_rebalance_when_allocation_unchanged() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());

        // Stable queue depth of 0 — no change from initial allocation
        for _ in 0..5 {
            scheduler.evaluate(snap(0.3, vec![], vec![], 0));
        }

        let decisions = scheduler.evaluate(snap(0.3, vec![], vec![], 0));

        let has_rebalance = decisions
            .iter()
            .any(|d| matches!(d, SchedulingDecision::Rebalance { .. }));
        assert!(!has_rebalance, "No rebalance when allocation is unchanged");
    }

    // -- history window --

    #[test]
    fn history_window_bounded() {
        let cfg = PredictiveSchedulerConfig {
            enabled: true,
            history_window: 5,
            ..default_config()
        };
        let mut scheduler = PredictiveScheduler::new(cfg);

        for i in 0..20 {
            scheduler.evaluate(snap(0.3, vec![], vec![], 0));
        }

        assert!(
            scheduler.history.len() <= 5,
            "History should be bounded to window size, got {}",
            scheduler.history.len()
        );
    }

    // -- hold when nothing to do --

    #[test]
    fn hold_when_all_stable() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());

        // Stable pressure, no unloaded hot adapters, no training queue
        for _ in 0..6 {
            scheduler.evaluate(snap(0.3, vec!["a"], vec![("a", 0.5)], 0));
        }

        let decisions = scheduler.evaluate(snap(0.3, vec!["a"], vec![("a", 0.5)], 0));

        assert!(
            decisions
                .iter()
                .any(|d| matches!(d, SchedulingDecision::Hold)),
            "Expected Hold when nothing needs action"
        );
    }

    // -- adapter heat trend --

    #[test]
    fn adapter_heat_trend_positive_when_warming() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());
        for &h in &[0.1, 0.2, 0.3, 0.4, 0.5] {
            scheduler
                .history
                .push_back(snap(0.3, vec![], vec![("x", h)], 0));
        }
        let trend = scheduler.adapter_heat_trend("x");
        assert!(trend > 0.0, "trend={trend}, expected positive");
    }

    #[test]
    fn adapter_heat_trend_negative_when_cooling() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());
        for &h in &[0.9, 0.7, 0.5, 0.3, 0.1] {
            scheduler
                .history
                .push_back(snap(0.3, vec![], vec![("x", h)], 0));
        }
        let trend = scheduler.adapter_heat_trend("x");
        assert!(trend < 0.0, "trend={trend}, expected negative");
    }

    #[test]
    fn adapter_heat_trend_zero_for_unknown_adapter() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());
        for _ in 0..5 {
            scheduler.history.push_back(snap(0.3, vec![], vec![], 0));
        }
        let trend = scheduler.adapter_heat_trend("nonexistent");
        assert!(
            trend.abs() < f64::EPSILON,
            "Unknown adapter should have zero trend"
        );
    }

    #[test]
    fn adapter_heat_trend_insufficient_history() {
        let mut scheduler = PredictiveScheduler::new(enabled_config());
        scheduler
            .history
            .push_back(snap(0.3, vec![], vec![("x", 0.5)], 0));
        let trend = scheduler.adapter_heat_trend("x");
        assert!(
            trend.abs() < f64::EPSILON,
            "Should return 0 with insufficient history"
        );
    }

    // -- combined scenario --

    #[test]
    fn combined_eviction_and_rebalance() {
        let cfg = PredictiveSchedulerConfig {
            enabled: true,
            ..default_config()
        };
        let mut scheduler = PredictiveScheduler::new(cfg);

        // Rising pressure + growing training queue
        for i in 0..5 {
            scheduler.evaluate(snap(
                0.60 + (i as f64 * 0.05),
                vec!["cold-one", "hot-one"],
                vec![("cold-one", 0.1), ("hot-one", 0.9)],
                i + 1,
            ));
        }

        let decisions = scheduler.evaluate(snap(
            0.85,
            vec!["cold-one", "hot-one"],
            vec![("cold-one", 0.1), ("hot-one", 0.9)],
            6,
        ));

        let has_evict = decisions
            .iter()
            .any(|d| matches!(d, SchedulingDecision::PreEvict { .. }));
        let has_rebalance = decisions
            .iter()
            .any(|d| matches!(d, SchedulingDecision::Rebalance { .. }));

        // At least one of these should fire given the conditions
        assert!(
            has_evict || has_rebalance,
            "Expected at least one proactive decision"
        );
    }
}
