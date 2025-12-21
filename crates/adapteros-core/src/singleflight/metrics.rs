//! Metrics trait for SingleFlight operations.
//!
//! Implementors can record leader/waiter/error counts for observability.

use std::sync::Arc;

/// Metrics interface for SingleFlight operations.
///
/// Implementors record leader/waiter/error counts. This trait allows
/// SingleFlight to be used without direct dependency on Prometheus.
pub trait SingleFlightMetrics: Send + Sync {
    /// Record that a request became the leader (triggered the load).
    fn record_leader(&self, operation: &str);

    /// Record that a request became a waiter (waiting for leader).
    fn record_waiter(&self, operation: &str);

    /// Update the current waiter gauge for an operation.
    fn set_waiter_gauge(&self, operation: &str, count: usize);

    /// Record an error from a load operation.
    fn record_error(&self, operation: &str, error_type: &str);
}

/// Statistics snapshot for SingleFlight monitoring.
#[derive(Debug, Clone, Default)]
pub struct SingleFlightStats {
    /// Number of keys currently being loaded.
    pub pending_loads: usize,
    /// Total number of waiters across all pending loads.
    pub total_waiters: usize,
    /// Age of the oldest pending load in milliseconds.
    pub oldest_load_age_ms: u128,
}

/// No-op metrics implementation for when metrics are disabled.
#[derive(Debug, Clone, Default)]
pub struct NoOpMetrics;

impl SingleFlightMetrics for NoOpMetrics {
    fn record_leader(&self, _operation: &str) {}
    fn record_waiter(&self, _operation: &str) {}
    fn set_waiter_gauge(&self, _operation: &str, _count: usize) {}
    fn record_error(&self, _operation: &str, _error_type: &str) {}
}

/// Wrapper to use `Arc<dyn SingleFlightMetrics>` conveniently.
pub type SharedMetrics = Arc<dyn SingleFlightMetrics>;
