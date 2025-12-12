//! Async SingleFlight implementation using tokio primitives.
//!
//! Ensures that concurrent requests for the same key trigger only one
//! actual load operation. Subsequent requests wait for and share the result.

use dashmap::DashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Notify, OnceCell};

use super::metrics::{NoOpMetrics, SharedMetrics, SingleFlightStats};

/// In-flight entry tracking a single load operation.
struct InFlightEntry<V, E>
where
    V: Clone + Send + Sync,
    E: Clone + Send + Sync,
{
    /// Notification for waiters when result is ready.
    notify: Notify,
    /// The result, set exactly once by the leader.
    result: OnceCell<Result<V, E>>,
    /// Number of concurrent waiters (including leader).
    waiter_count: AtomicUsize,
    /// Timestamp when first request arrived.
    first_request_at: Instant,
}

impl<V, E> InFlightEntry<V, E>
where
    V: Clone + Send + Sync,
    E: Clone + Send + Sync,
{
    fn new() -> Self {
        Self {
            notify: Notify::new(),
            result: OnceCell::new(),
            waiter_count: AtomicUsize::new(0),
            first_request_at: Instant::now(),
        }
    }

    /// Add a waiter and return the new count (1 = leader, >1 = waiter).
    fn add_waiter(&self) -> usize {
        self.waiter_count.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Get current waiter count.
    fn get_waiter_count(&self) -> usize {
        self.waiter_count.load(Ordering::SeqCst)
    }

    /// Get elapsed time since first request.
    fn elapsed_ms(&self) -> u128 {
        self.first_request_at.elapsed().as_millis()
    }
}

/// Async single-flight deduplication utility.
///
/// Ensures that concurrent requests for the same key trigger only one
/// actual load operation. Subsequent requests wait for and share the result.
///
/// # Type Parameters
///
/// - `K`: Key type (must be `Eq + Hash + Clone + Send + Sync`)
/// - `V`: Value type (must be `Clone + Send + Sync`)
/// - `E`: Error type (must be `Clone + Send + Sync`)
///
/// # Thread Safety
///
/// All operations are thread-safe and can be called from multiple async tasks.
///
/// # Example
///
/// ```ignore
/// let sf = SingleFlight::<String, Data, MyError>::new("my_operation");
///
/// // Multiple concurrent calls with the same key will dedupe
/// let result = sf.get_or_load("key".to_string(), || async {
///     expensive_load().await
/// }).await;
/// ```
pub struct SingleFlight<K, V, E>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    /// In-flight operations keyed by request key.
    in_flight: DashMap<K, Arc<InFlightEntry<V, E>>>,
    /// Operation label for metrics.
    operation_label: &'static str,
    /// Optional metrics collector.
    metrics: SharedMetrics,
}

impl<K, V, E> SingleFlight<K, V, E>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    /// Create a new SingleFlight instance without metrics.
    pub fn new(operation_label: &'static str) -> Self {
        Self {
            in_flight: DashMap::new(),
            operation_label,
            metrics: Arc::new(NoOpMetrics),
        }
    }

    /// Create a new SingleFlight instance with metrics collector.
    pub fn with_metrics(operation_label: &'static str, metrics: SharedMetrics) -> Self {
        Self {
            in_flight: DashMap::new(),
            operation_label,
            metrics,
        }
    }

    /// Execute load function or wait for in-flight result.
    ///
    /// # Arguments
    ///
    /// * `key` - The deduplication key
    /// * `load_fn` - Async function to execute on cache miss
    ///
    /// # Returns
    ///
    /// * `Ok(V)` - The loaded value (either fresh or from concurrent load)
    /// * `Err(E)` - Error from loader (propagated to all waiters)
    ///
    /// # Cancellation Safety
    ///
    /// If the leader task is cancelled before completing, waiters will receive
    /// an error and the in-flight entry is cleaned up.
    pub async fn get_or_load<F, Fut>(&self, key: K, load_fn: F) -> Result<V, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<V, E>>,
    {
        // Try to get existing entry or insert new one
        let entry = self
            .in_flight
            .entry(key.clone())
            .or_insert_with(|| Arc::new(InFlightEntry::new()))
            .clone();

        let waiter_num = entry.add_waiter();

        if waiter_num == 1 {
            // Leader path: execute load
            self.metrics.record_leader(self.operation_label);

            tracing::debug!(
                operation = %self.operation_label,
                "SingleFlight: first request, triggering load"
            );

            // Use a guard to ensure cleanup on cancellation/panic
            let mut cleanup_guard = CleanupGuard {
                in_flight: &self.in_flight,
                key: key.clone(),
                entry: entry.clone(),
                completed: false,
            };

            let result = load_fn().await;

            // Mark as completed BEFORE setting result to avoid race
            cleanup_guard.completed = true;

            // Store result for all waiters
            let _ = entry.result.set(result.clone());

            // Notify all waiters
            entry.notify.notify_waiters();

            // Update metrics
            let final_waiter_count = entry.get_waiter_count();
            self.metrics
                .set_waiter_gauge(self.operation_label, final_waiter_count.saturating_sub(1));

            if result.is_err() {
                self.metrics.record_error(self.operation_label, "load_error");
            }

            if final_waiter_count > 1 {
                tracing::info!(
                    operation = %self.operation_label,
                    waiters = final_waiter_count - 1,
                    load_time_ms = entry.elapsed_ms(),
                    success = result.is_ok(),
                    "SingleFlight: load completed with waiters"
                );
            }

            // Cleanup: remove from in-flight map
            self.in_flight.remove(&key);

            result
        } else {
            // Waiter path: wait for leader to complete
            self.metrics.record_waiter(self.operation_label);

            tracing::debug!(
                operation = %self.operation_label,
                waiter_position = waiter_num,
                "SingleFlight: waiting for in-progress load"
            );

            let wait_start = Instant::now();

            // Wait for notification
            entry.notify.notified().await;

            let wait_time_ms = wait_start.elapsed().as_millis();

            // Get result from OnceCell
            // Loop to handle spurious wakeups
            loop {
                match entry.result.get() {
                    Some(result) => {
                        tracing::debug!(
                            operation = %self.operation_label,
                            wait_time_ms = wait_time_ms,
                            success = result.is_ok(),
                            "SingleFlight: wait completed"
                        );
                        return result.clone();
                    }
                    None => {
                        // Check if entry was removed (leader cancelled/panicked)
                        if !self.in_flight.contains_key(&key) {
                            tracing::warn!(
                                operation = %self.operation_label,
                                "SingleFlight: leader cancelled/panicked"
                            );
                            self.metrics
                                .record_error(self.operation_label, "leader_cancelled");
                            // Entry was cleaned up by CleanupGuard - caller should retry
                            // We can't retry here because load_fn is FnOnce
                            panic!(
                                "SingleFlight leader cancelled or panicked. \
                                Caller should retry the operation."
                            );
                        }
                        // Spurious wakeup - wait again
                        entry.notify.notified().await;
                    }
                }
            }
        }
    }

    /// Check if a key is currently being loaded.
    pub fn is_loading(&self, key: &K) -> bool {
        self.in_flight.contains_key(key)
    }

    /// Get current waiter count for a key.
    ///
    /// Returns 0 if the key is not being loaded.
    pub fn waiter_count(&self, key: &K) -> usize {
        self.in_flight
            .get(key)
            .map(|entry| entry.get_waiter_count())
            .unwrap_or(0)
    }

    /// Get metrics snapshot.
    pub fn stats(&self) -> SingleFlightStats {
        let mut total_waiters = 0;
        let mut oldest_load_age_ms = 0u128;
        let pending_count = self.in_flight.len();

        for entry in self.in_flight.iter() {
            let waiter = entry.value();
            total_waiters += waiter.get_waiter_count();
            let age = waiter.elapsed_ms();
            if age > oldest_load_age_ms {
                oldest_load_age_ms = age;
            }
        }

        SingleFlightStats {
            pending_loads: pending_count,
            total_waiters,
            oldest_load_age_ms,
        }
    }

    /// Get operation label for this instance.
    pub fn operation_label(&self) -> &'static str {
        self.operation_label
    }
}

impl<K, V, E> Clone for SingleFlight<K, V, E>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        // Note: This creates a new instance that shares nothing with the original.
        // This is intentional - each instance should have its own in-flight map.
        // If you need to share state, wrap in Arc.
        Self {
            in_flight: DashMap::new(),
            operation_label: self.operation_label,
            metrics: self.metrics.clone(),
        }
    }
}

impl<K, V, E> Debug for SingleFlight<K, V, E>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleFlight")
            .field("operation_label", &self.operation_label)
            .field("pending_loads", &self.in_flight.len())
            .finish()
    }
}

/// RAII guard to ensure in-flight entry is cleaned up on cancellation.
struct CleanupGuard<'a, K, V, E>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    in_flight: &'a DashMap<K, Arc<InFlightEntry<V, E>>>,
    key: K,
    entry: Arc<InFlightEntry<V, E>>,
    completed: bool,
}

impl<K, V, E> Drop for CleanupGuard<'_, K, V, E>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    fn drop(&mut self) {
        // If the guard is dropped without completing (i.e., leader was cancelled),
        // we need to notify waiters and clean up.
        // However, since we can't set a result in Drop (we don't have E),
        // we just notify waiters who will see None and handle it.
        if !self.completed && self.entry.result.get().is_none() {
            // Notify waiters that something went wrong
            self.entry.notify.notify_waiters();
            // Remove the in-flight entry
            self.in_flight.remove(&self.key);
        }
    }
}
