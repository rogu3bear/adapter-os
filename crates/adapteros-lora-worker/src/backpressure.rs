//! Worker backpressure gate for fast-fail overload protection
//!
//! Implements bounded concurrency control using a semaphore with `try_acquire()`
//! to immediately reject requests when the worker is at capacity.
//!
//! # Resource Partitioning
//!
//! The [`PartitionedBackpressureGate`] splits the total concurrency budget into
//! separate inference and training pools so that long-running training jobs
//! cannot starve latency-sensitive inference requests.
//!
//! Default split: 6 inference + 2 training = 8 total.
//!
//! Configure via environment variables:
//! - `AOS_WORKER_MAX_INFERENCE` — inference pool size (default: 6)
//! - `AOS_WORKER_MAX_TRAINING` — training pool size (default: 2)
//! - `AOS_WORKER_MAX_CONCURRENT` — legacy single-pool size (default: 8)
//!
//! # Usage
//!
//! ```ignore
//! use adapteros_lora_worker::backpressure::{
//!     PartitionedBackpressureGate, WorkloadKind,
//! };
//!
//! let gate = Arc::new(PartitionedBackpressureGate::from_env());
//!
//! // Inference request
//! if let Some(permit) = gate.try_acquire(WorkloadKind::Inference) {
//!     do_inference().await;
//! } else {
//!     return Err(WorkerOverloaded);
//! }
//!
//! // Training request
//! if let Some(permit) = gate.try_acquire(WorkloadKind::Training) {
//!     do_training().await;
//! } else {
//!     return Err(TrainingQueueFull);
//! }
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{Semaphore, TryAcquireError};
use tracing::{info, warn};

/// Default maximum concurrent inference requests
pub const DEFAULT_MAX_CONCURRENT: usize = 8;

/// Default inference pool size in partitioned mode
pub const DEFAULT_MAX_INFERENCE: usize = 6;

/// Default training pool size in partitioned mode
pub const DEFAULT_MAX_TRAINING: usize = 2;

// ---------------------------------------------------------------------------
// WorkloadKind
// ---------------------------------------------------------------------------

/// Distinguishes request types for resource partitioning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkloadKind {
    /// Latency-sensitive inference request.
    Inference,
    /// Long-running training job.
    Training,
}

impl std::fmt::Display for WorkloadKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inference => f.write_str("inference"),
            Self::Training => f.write_str("training"),
        }
    }
}

// ---------------------------------------------------------------------------
// BackpressureStats (unchanged)
// ---------------------------------------------------------------------------

/// Backpressure statistics for observability
#[derive(Debug, Clone, Default)]
pub struct BackpressureStats {
    /// Current number of in-flight requests
    pub in_flight: u64,
    /// Maximum allowed concurrent requests
    pub max_concurrent: u64,
    /// Total requests rejected due to overload
    pub rejected_count: u64,
    /// Total requests successfully admitted
    pub admitted_count: u64,
}

impl BackpressureStats {
    /// Calculate utilization percentage (0-100)
    pub fn utilization_percent(&self) -> f64 {
        if self.max_concurrent == 0 {
            return 0.0;
        }
        (self.in_flight as f64 / self.max_concurrent as f64) * 100.0
    }

    /// Calculate rejection rate percentage
    pub fn rejection_rate_percent(&self) -> f64 {
        let total = self.admitted_count + self.rejected_count;
        if total == 0 {
            return 0.0;
        }
        (self.rejected_count as f64 / total as f64) * 100.0
    }
}

// ---------------------------------------------------------------------------
// BackpressurePermit (unchanged interface)
// ---------------------------------------------------------------------------

/// RAII guard that releases the semaphore permit when dropped
pub struct BackpressurePermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    gate: Arc<BackpressureGate>,
}

impl Drop for BackpressurePermit {
    fn drop(&mut self) {
        self.gate.in_flight.fetch_sub(1, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// BackpressureGate (backward-compatible single-pool)
// ---------------------------------------------------------------------------

/// Backpressure gate for controlling concurrent request admission
///
/// Uses a semaphore with `try_acquire()` for non-blocking admission control.
/// When the gate is at capacity, requests are immediately rejected (fast-fail)
/// rather than queuing indefinitely.
use crate::limiter::ThunderingHerdConfig;

pub struct BackpressureGate {
    semaphore: Arc<Semaphore>,
    max_concurrent: usize,
    in_flight: AtomicU64,
    rejected_count: AtomicU64,
    admitted_count: AtomicU64,
    config: ThunderingHerdConfig,
}

impl BackpressureGate {
    /// Create a new backpressure gate with the specified maximum concurrent requests
    pub fn new(max_concurrent: usize) -> Self {
        Self::new_with_config(max_concurrent, ThunderingHerdConfig::default())
    }

    /// Create with explicit config
    pub fn new_with_config(max_concurrent: usize, config: ThunderingHerdConfig) -> Self {
        info!(max_concurrent, "Creating backpressure gate");
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
            in_flight: AtomicU64::new(0),
            rejected_count: AtomicU64::new(0),
            admitted_count: AtomicU64::new(0),
            config,
        }
    }

    /// Create a backpressure gate from environment configuration
    ///
    /// Reads `AOS_WORKER_MAX_CONCURRENT` env var, falls back to default if not set or invalid.
    pub fn from_env() -> Self {
        let max_concurrent = std::env::var("AOS_WORKER_MAX_CONCURRENT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_MAX_CONCURRENT);
        Self::new_with_config(max_concurrent, ThunderingHerdConfig::from_env())
    }

    /// Try to acquire a permit without waiting (fast-fail)
    ///
    /// Returns `Some(BackpressurePermit)` if a slot is available, `None` if at capacity.
    /// The permit is automatically released when dropped.
    pub fn try_acquire(self: &Arc<Self>) -> Option<BackpressurePermit> {
        match self.semaphore.clone().try_acquire_owned() {
            Ok(permit) => {
                self.in_flight.fetch_add(1, Ordering::Relaxed);
                self.admitted_count.fetch_add(1, Ordering::Relaxed);
                Some(BackpressurePermit {
                    _permit: permit,
                    gate: Arc::clone(self),
                })
            }
            Err(TryAcquireError::NoPermits) => {
                self.rejected_count.fetch_add(1, Ordering::Relaxed);
                warn!(
                    in_flight = self.in_flight.load(Ordering::Relaxed),
                    max_concurrent = self.max_concurrent,
                    "Backpressure gate rejecting request - at capacity"
                );
                None
            }
            Err(TryAcquireError::Closed) => {
                // Semaphore closed - should never happen in normal operation
                warn!("Backpressure semaphore unexpectedly closed");
                None
            }
        }
    }

    /// Get current statistics
    pub fn stats(&self) -> BackpressureStats {
        BackpressureStats {
            in_flight: self.in_flight.load(Ordering::Relaxed),
            max_concurrent: self.max_concurrent as u64,
            rejected_count: self.rejected_count.load(Ordering::Relaxed),
            admitted_count: self.admitted_count.load(Ordering::Relaxed),
        }
    }

    /// Get current number of in-flight requests
    pub fn in_flight(&self) -> u64 {
        self.in_flight.load(Ordering::Relaxed)
    }

    /// Get the configured maximum concurrent requests
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }

    /// Compute suggested retry delay in milliseconds based on current load
    ///
    /// Returns a delay that increases with load to provide natural backoff behavior.
    pub fn suggested_retry_ms(&self) -> u64 {
        let base_ms = self.config.base_retry_hint_ms;
        let load_factor = if self.max_concurrent > 0 {
            self.in_flight.load(Ordering::Relaxed) as f64 / self.max_concurrent as f64
        } else {
            1.0
        };
        let jitter =
            (load_factor * self.config.max_retry_hint_ms as f64 * self.config.jitter_factor) as u64;
        base_ms + jitter
    }
}

impl Default for BackpressureGate {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CONCURRENT)
    }
}

// ---------------------------------------------------------------------------
// PartitionedBackpressureGate (inference + training pools)
// ---------------------------------------------------------------------------

/// RAII guard for a partitioned permit. Decrements the correct pool counter on drop.
pub struct PartitionedPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    gate: Arc<PartitionedBackpressureGate>,
    kind: WorkloadKind,
}

impl Drop for PartitionedPermit {
    fn drop(&mut self) {
        match self.kind {
            WorkloadKind::Inference => {
                self.gate
                    .inference_in_flight
                    .fetch_sub(1, Ordering::Relaxed);
            }
            WorkloadKind::Training => {
                self.gate.training_in_flight.fetch_sub(1, Ordering::Relaxed);
            }
        }
    }
}

/// Partitioned backpressure gate with separate inference and training pools.
///
/// Prevents long-running training jobs from starving inference by reserving
/// dedicated concurrency budgets for each workload type.
pub struct PartitionedBackpressureGate {
    inference_semaphore: Arc<Semaphore>,
    training_semaphore: Arc<Semaphore>,
    max_inference: usize,
    max_training: usize,

    inference_in_flight: AtomicU64,
    training_in_flight: AtomicU64,

    inference_rejected: AtomicU64,
    training_rejected: AtomicU64,

    inference_admitted: AtomicU64,
    training_admitted: AtomicU64,
}

impl PartitionedBackpressureGate {
    /// Create a partitioned gate with explicit pool sizes.
    pub fn new(max_inference: usize, max_training: usize) -> Self {
        info!(
            max_inference,
            max_training,
            total = max_inference + max_training,
            "Creating partitioned backpressure gate"
        );
        Self {
            inference_semaphore: Arc::new(Semaphore::new(max_inference)),
            training_semaphore: Arc::new(Semaphore::new(max_training)),
            max_inference,
            max_training,
            inference_in_flight: AtomicU64::new(0),
            training_in_flight: AtomicU64::new(0),
            inference_rejected: AtomicU64::new(0),
            training_rejected: AtomicU64::new(0),
            inference_admitted: AtomicU64::new(0),
            training_admitted: AtomicU64::new(0),
        }
    }

    /// Create from environment variables.
    ///
    /// Reads `AOS_WORKER_MAX_INFERENCE` and `AOS_WORKER_MAX_TRAINING`.
    /// Falls back to [`DEFAULT_MAX_INFERENCE`] and [`DEFAULT_MAX_TRAINING`].
    pub fn from_env() -> Self {
        let max_inference = std::env::var("AOS_WORKER_MAX_INFERENCE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_MAX_INFERENCE);
        let max_training = std::env::var("AOS_WORKER_MAX_TRAINING")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_MAX_TRAINING);
        Self::new(max_inference, max_training)
    }

    /// Try to acquire a permit for the given workload type (fast-fail).
    ///
    /// Returns `None` when the pool for `kind` is exhausted.
    pub fn try_acquire(self: &Arc<Self>, kind: WorkloadKind) -> Option<PartitionedPermit> {
        let (semaphore, in_flight, admitted, rejected, max, label) = match kind {
            WorkloadKind::Inference => (
                &self.inference_semaphore,
                &self.inference_in_flight,
                &self.inference_admitted,
                &self.inference_rejected,
                self.max_inference,
                "inference",
            ),
            WorkloadKind::Training => (
                &self.training_semaphore,
                &self.training_in_flight,
                &self.training_admitted,
                &self.training_rejected,
                self.max_training,
                "training",
            ),
        };

        match semaphore.clone().try_acquire_owned() {
            Ok(permit) => {
                in_flight.fetch_add(1, Ordering::Relaxed);
                admitted.fetch_add(1, Ordering::Relaxed);
                Some(PartitionedPermit {
                    _permit: permit,
                    gate: Arc::clone(self),
                    kind,
                })
            }
            Err(TryAcquireError::NoPermits) => {
                rejected.fetch_add(1, Ordering::Relaxed);
                warn!(
                    workload = label,
                    in_flight = in_flight.load(Ordering::Relaxed),
                    max = max,
                    "Backpressure gate rejecting {} request - pool at capacity",
                    label
                );
                None
            }
            Err(TryAcquireError::Closed) => {
                warn!(
                    workload = label,
                    "Backpressure semaphore unexpectedly closed"
                );
                None
            }
        }
    }

    /// Statistics for a specific workload pool.
    pub fn stats_for(&self, kind: WorkloadKind) -> BackpressureStats {
        match kind {
            WorkloadKind::Inference => BackpressureStats {
                in_flight: self.inference_in_flight.load(Ordering::Relaxed),
                max_concurrent: self.max_inference as u64,
                rejected_count: self.inference_rejected.load(Ordering::Relaxed),
                admitted_count: self.inference_admitted.load(Ordering::Relaxed),
            },
            WorkloadKind::Training => BackpressureStats {
                in_flight: self.training_in_flight.load(Ordering::Relaxed),
                max_concurrent: self.max_training as u64,
                rejected_count: self.training_rejected.load(Ordering::Relaxed),
                admitted_count: self.training_admitted.load(Ordering::Relaxed),
            },
        }
    }

    /// Aggregate statistics across both pools.
    pub fn stats(&self) -> BackpressureStats {
        BackpressureStats {
            in_flight: self.inference_in_flight.load(Ordering::Relaxed)
                + self.training_in_flight.load(Ordering::Relaxed),
            max_concurrent: (self.max_inference + self.max_training) as u64,
            rejected_count: self.inference_rejected.load(Ordering::Relaxed)
                + self.training_rejected.load(Ordering::Relaxed),
            admitted_count: self.inference_admitted.load(Ordering::Relaxed)
                + self.training_admitted.load(Ordering::Relaxed),
        }
    }

    /// Total in-flight across both pools.
    pub fn in_flight(&self) -> u64 {
        self.inference_in_flight.load(Ordering::Relaxed)
            + self.training_in_flight.load(Ordering::Relaxed)
    }

    /// Pool size for inference.
    pub fn max_inference(&self) -> usize {
        self.max_inference
    }

    /// Pool size for training.
    pub fn max_training(&self) -> usize {
        self.max_training
    }

    /// Total capacity across both pools.
    pub fn max_concurrent(&self) -> usize {
        self.max_inference + self.max_training
    }

    /// Suggested retry delay, considering the specific pool's load.
    pub fn suggested_retry_ms(&self, kind: WorkloadKind) -> u64 {
        let base_ms = 100;
        let (in_flight, max) = match kind {
            WorkloadKind::Inference => (
                self.inference_in_flight.load(Ordering::Relaxed),
                self.max_inference,
            ),
            WorkloadKind::Training => (
                self.training_in_flight.load(Ordering::Relaxed),
                self.max_training,
            ),
        };
        let load_factor = if max > 0 {
            in_flight as f64 / max as f64
        } else {
            1.0
        };
        let jitter = (load_factor * 200.0) as u64;
        base_ms + jitter
    }
}

impl Default for PartitionedBackpressureGate {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_INFERENCE, DEFAULT_MAX_TRAINING)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- BackpressureGate (single-pool, backward-compat) tests ---

    #[test]
    fn test_gate_creation() {
        let gate = BackpressureGate::new(4);
        assert_eq!(gate.max_concurrent(), 4);
        assert_eq!(gate.in_flight(), 0);

        let stats = gate.stats();
        assert_eq!(stats.max_concurrent, 4);
        assert_eq!(stats.in_flight, 0);
        assert_eq!(stats.admitted_count, 0);
        assert_eq!(stats.rejected_count, 0);
    }

    #[test]
    fn test_default_max_concurrent() {
        assert_eq!(DEFAULT_MAX_CONCURRENT, 8);
    }

    #[tokio::test]
    async fn test_permit_acquisition() {
        let gate = Arc::new(BackpressureGate::new(2));

        // Acquire first permit
        let permit1 = gate.try_acquire().expect("Should acquire first permit");
        assert_eq!(gate.in_flight(), 1);

        // Acquire second permit
        let permit2 = gate.try_acquire().expect("Should acquire second permit");
        assert_eq!(gate.in_flight(), 2);

        // Third should be rejected
        assert!(
            gate.try_acquire().is_none(),
            "Third permit should be rejected"
        );
        assert_eq!(gate.stats().rejected_count, 1);

        // Drop one permit
        drop(permit1);
        assert_eq!(gate.in_flight(), 1);

        // Now should be able to acquire again
        let _permit3 = gate.try_acquire().expect("Should acquire after release");
        assert_eq!(gate.in_flight(), 2);

        drop(permit2);
        drop(_permit3);
        assert_eq!(gate.in_flight(), 0);
    }

    #[tokio::test]
    async fn test_stats_accuracy() {
        let gate = Arc::new(BackpressureGate::new(4));

        // Initial state
        let stats = gate.stats();
        assert_eq!(stats.in_flight, 0);
        assert_eq!(stats.max_concurrent, 4);
        assert_eq!(stats.admitted_count, 0);
        assert_eq!(stats.rejected_count, 0);
        assert_eq!(stats.utilization_percent(), 0.0);

        // Acquire some permits
        let _p1 = gate.try_acquire();
        let _p2 = gate.try_acquire();

        let stats = gate.stats();
        assert_eq!(stats.in_flight, 2);
        assert_eq!(stats.admitted_count, 2);
        assert_eq!(stats.utilization_percent(), 50.0);

        // Fill up and trigger rejection
        let _p3 = gate.try_acquire();
        let _p4 = gate.try_acquire();
        let _ = gate.try_acquire(); // Should be rejected

        let stats = gate.stats();
        assert_eq!(stats.in_flight, 4);
        assert_eq!(stats.admitted_count, 4);
        assert_eq!(stats.rejected_count, 1);
        assert_eq!(stats.utilization_percent(), 100.0);
    }

    #[test]
    fn test_suggested_retry_ms_scaling() {
        let gate = Arc::new(BackpressureGate::new(10));

        // At zero load - base delay
        let retry_low = gate.suggested_retry_ms();
        assert!(retry_low >= 100, "Base delay should be at least 100ms");
        assert!(retry_low < 150, "At zero load, delay should be near base");

        // Simulate high load by acquiring permits
        let mut permits = Vec::new();
        for _ in 0..10 {
            if let Some(p) = gate.try_acquire() {
                permits.push(p);
            }
        }

        let retry_high = gate.suggested_retry_ms();
        assert!(
            retry_high > retry_low,
            "Retry delay should increase with load"
        );
        let expected_max = gate.config.base_retry_hint_ms
            + (gate.config.max_retry_hint_ms as f64 * gate.config.jitter_factor) as u64;
        assert!(
            retry_high <= expected_max,
            "Max retry should respect configured cap"
        );
    }

    #[test]
    fn test_rejection_rate_percent() {
        let stats = BackpressureStats {
            in_flight: 4,
            max_concurrent: 8,
            rejected_count: 10,
            admitted_count: 90,
        };
        assert_eq!(stats.rejection_rate_percent(), 10.0);

        let empty_stats = BackpressureStats::default();
        assert_eq!(empty_stats.rejection_rate_percent(), 0.0);
    }

    // --- PartitionedBackpressureGate tests ---

    #[test]
    fn test_partitioned_defaults() {
        assert_eq!(DEFAULT_MAX_INFERENCE, 6);
        assert_eq!(DEFAULT_MAX_TRAINING, 2);
        assert_eq!(
            DEFAULT_MAX_INFERENCE + DEFAULT_MAX_TRAINING,
            DEFAULT_MAX_CONCURRENT
        );
    }

    #[test]
    fn test_partitioned_creation() {
        let gate = PartitionedBackpressureGate::new(4, 2);
        assert_eq!(gate.max_inference(), 4);
        assert_eq!(gate.max_training(), 2);
        assert_eq!(gate.max_concurrent(), 6);
        assert_eq!(gate.in_flight(), 0);
    }

    #[tokio::test]
    async fn test_partitioned_inference_pool() {
        let gate = Arc::new(PartitionedBackpressureGate::new(2, 1));

        let p1 = gate
            .try_acquire(WorkloadKind::Inference)
            .expect("Should acquire inference permit 1");
        let p2 = gate
            .try_acquire(WorkloadKind::Inference)
            .expect("Should acquire inference permit 2");

        // Inference pool exhausted
        assert!(
            gate.try_acquire(WorkloadKind::Inference).is_none(),
            "Inference pool should be full"
        );

        let inf_stats = gate.stats_for(WorkloadKind::Inference);
        assert_eq!(inf_stats.in_flight, 2);
        assert_eq!(inf_stats.rejected_count, 1);
        assert_eq!(inf_stats.admitted_count, 2);

        // Training pool should still be available
        let _t1 = gate
            .try_acquire(WorkloadKind::Training)
            .expect("Training pool should be independent");

        let train_stats = gate.stats_for(WorkloadKind::Training);
        assert_eq!(train_stats.in_flight, 1);
        assert_eq!(train_stats.admitted_count, 1);

        // Total in-flight
        assert_eq!(gate.in_flight(), 3);

        drop(p1);
        drop(p2);
        assert_eq!(gate.stats_for(WorkloadKind::Inference).in_flight, 0);
    }

    #[tokio::test]
    async fn test_partitioned_training_pool() {
        let gate = Arc::new(PartitionedBackpressureGate::new(3, 1));

        let _t1 = gate
            .try_acquire(WorkloadKind::Training)
            .expect("Should acquire training permit");

        // Training pool exhausted
        assert!(
            gate.try_acquire(WorkloadKind::Training).is_none(),
            "Training pool should be full"
        );

        let train_stats = gate.stats_for(WorkloadKind::Training);
        assert_eq!(train_stats.in_flight, 1);
        assert_eq!(train_stats.rejected_count, 1);

        // Inference should still work
        let _i1 = gate
            .try_acquire(WorkloadKind::Inference)
            .expect("Inference should be independent of training");

        assert_eq!(gate.in_flight(), 2);
    }

    #[tokio::test]
    async fn test_partitioned_pools_isolated() {
        let gate = Arc::new(PartitionedBackpressureGate::new(2, 2));

        // Fill inference
        let _i1 = gate.try_acquire(WorkloadKind::Inference);
        let _i2 = gate.try_acquire(WorkloadKind::Inference);
        assert!(gate.try_acquire(WorkloadKind::Inference).is_none());

        // Training still has both slots
        let _t1 = gate.try_acquire(WorkloadKind::Training);
        let _t2 = gate.try_acquire(WorkloadKind::Training);
        assert!(gate.try_acquire(WorkloadKind::Training).is_none());

        assert_eq!(gate.in_flight(), 4);

        let agg = gate.stats();
        assert_eq!(agg.in_flight, 4);
        assert_eq!(agg.max_concurrent, 4);
        assert_eq!(agg.admitted_count, 4);
        assert_eq!(agg.rejected_count, 2);
    }

    #[tokio::test]
    async fn test_partitioned_permit_release() {
        let gate = Arc::new(PartitionedBackpressureGate::new(1, 1));

        let p = gate
            .try_acquire(WorkloadKind::Inference)
            .expect("Should acquire");
        assert_eq!(gate.stats_for(WorkloadKind::Inference).in_flight, 1);

        drop(p);
        assert_eq!(gate.stats_for(WorkloadKind::Inference).in_flight, 0);

        // Can acquire again after release
        let _p2 = gate
            .try_acquire(WorkloadKind::Inference)
            .expect("Should acquire after release");
        assert_eq!(gate.stats_for(WorkloadKind::Inference).in_flight, 1);
    }

    #[test]
    fn test_partitioned_suggested_retry_ms() {
        let gate = Arc::new(PartitionedBackpressureGate::new(4, 2));

        let retry_inference = gate.suggested_retry_ms(WorkloadKind::Inference);
        assert!(retry_inference >= 100);

        let retry_training = gate.suggested_retry_ms(WorkloadKind::Training);
        assert!(retry_training >= 100);
    }

    #[test]
    fn test_workload_kind_display() {
        assert_eq!(WorkloadKind::Inference.to_string(), "inference");
        assert_eq!(WorkloadKind::Training.to_string(), "training");
    }

    #[tokio::test]
    async fn test_partitioned_stats_for_accuracy() {
        let gate = Arc::new(PartitionedBackpressureGate::new(3, 2));

        let _i1 = gate.try_acquire(WorkloadKind::Inference);
        let _i2 = gate.try_acquire(WorkloadKind::Inference);
        let _t1 = gate.try_acquire(WorkloadKind::Training);

        let inf = gate.stats_for(WorkloadKind::Inference);
        assert_eq!(inf.in_flight, 2);
        assert_eq!(inf.max_concurrent, 3);
        assert_eq!(inf.admitted_count, 2);

        let train = gate.stats_for(WorkloadKind::Training);
        assert_eq!(train.in_flight, 1);
        assert_eq!(train.max_concurrent, 2);
        assert_eq!(train.admitted_count, 1);

        let agg = gate.stats();
        assert_eq!(agg.in_flight, 3);
        assert_eq!(agg.max_concurrent, 5);
    }

    #[test]
    fn test_partitioned_default() {
        let gate = PartitionedBackpressureGate::default();
        assert_eq!(gate.max_inference(), DEFAULT_MAX_INFERENCE);
        assert_eq!(gate.max_training(), DEFAULT_MAX_TRAINING);
    }
}
