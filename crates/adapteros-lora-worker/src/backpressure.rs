//! Worker backpressure gate for fast-fail overload protection
//!
//! Implements bounded concurrency control using a semaphore with `try_acquire()`
//! to immediately reject requests when the worker is at capacity.
//!
//! # Configuration
//!
//! Set `AOS_WORKER_MAX_CONCURRENT` environment variable to control the maximum
//! number of concurrent inference requests. Default is 8.
//!
//! # Usage
//!
//! ```ignore
//! use adapteros_lora_worker::backpressure::{BackpressureGate, DEFAULT_MAX_CONCURRENT};
//!
//! let gate = Arc::new(BackpressureGate::new(DEFAULT_MAX_CONCURRENT));
//!
//! // Try to acquire a permit (fast-fail if at capacity)
//! if let Some(permit) = gate.try_acquire() {
//!     // Process request while holding permit
//!     do_inference().await;
//!     // Permit is released when dropped
//! } else {
//!     // Return 503 immediately
//!     return Err(WorkerOverloaded);
//! }
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{Semaphore, TryAcquireError};
use tracing::{info, warn};

/// Default maximum concurrent inference requests
pub const DEFAULT_MAX_CONCURRENT: usize = 8;

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

/// Backpressure gate for controlling concurrent request admission
///
/// Uses a semaphore with `try_acquire()` for non-blocking admission control.
/// When the gate is at capacity, requests are immediately rejected (fast-fail)
/// rather than queuing indefinitely.
pub struct BackpressureGate {
    semaphore: Arc<Semaphore>,
    max_concurrent: usize,
    in_flight: AtomicU64,
    rejected_count: AtomicU64,
    admitted_count: AtomicU64,
}

impl BackpressureGate {
    /// Create a new backpressure gate with the specified maximum concurrent requests
    pub fn new(max_concurrent: usize) -> Self {
        info!(max_concurrent, "Creating backpressure gate");
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
            in_flight: AtomicU64::new(0),
            rejected_count: AtomicU64::new(0),
            admitted_count: AtomicU64::new(0),
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
        Self::new(max_concurrent)
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
    /// Base delay is 100ms, scaling up to ~300ms at full capacity.
    pub fn suggested_retry_ms(&self) -> u64 {
        let base_ms = 100;
        let load_factor =
            self.in_flight.load(Ordering::Relaxed) as f64 / self.max_concurrent as f64;
        let jitter = (load_factor * 200.0) as u64;
        base_ms + jitter
    }
}

impl Default for BackpressureGate {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CONCURRENT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(retry_high <= 300, "Max retry should be around 300ms");
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
}
