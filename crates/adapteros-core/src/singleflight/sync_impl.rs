//! Sync SingleFlight implementation using parking_lot primitives.
//!
//! Ensures that concurrent requests for the same key trigger only one
//! actual load operation. Subsequent requests wait for and share the result.

use parking_lot::{Condvar, Mutex};
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use super::metrics::{NoOpMetrics, SharedMetrics, SingleFlightStats};

use std::sync::atomic::AtomicBool;

/// In-flight entry tracking a single load operation (sync version).
struct InFlightEntrySync<V, E>
where
    V: Clone + Send,
    E: Clone + Send,
{
    /// The result, protected by mutex.
    result: Mutex<Option<Result<V, E>>>,
    /// Condvar for waiters.
    condvar: Condvar,
    /// Number of concurrent waiters (including leader).
    waiter_count: AtomicUsize,
    /// Timestamp when first request arrived.
    first_request_at: Instant,
    /// Set to true if leader panicked.
    leader_panicked: AtomicBool,
}

impl<V, E> InFlightEntrySync<V, E>
where
    V: Clone + Send,
    E: Clone + Send,
{
    fn new() -> Self {
        Self {
            result: Mutex::new(None),
            condvar: Condvar::new(),
            waiter_count: AtomicUsize::new(0),
            first_request_at: Instant::now(),
            leader_panicked: AtomicBool::new(false),
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

    /// Set the result and notify all waiters.
    fn set_result(&self, result: Result<V, E>) {
        let mut guard = self.result.lock();
        *guard = Some(result);
        self.condvar.notify_all();
    }

    /// Mark leader as panicked and wake all waiters.
    fn mark_panicked(&self) {
        self.leader_panicked.store(true, Ordering::SeqCst);
        self.condvar.notify_all();
    }

    /// Wait for the result to be set and clone it.
    /// Panics if leader panicked before setting result.
    fn wait(&self) -> Result<V, E> {
        let mut guard = self.result.lock();
        loop {
            if let Some(ref result) = *guard {
                return result.clone();
            }
            if self.leader_panicked.load(Ordering::SeqCst) {
                panic!(
                    "SingleFlightSync leader panicked before setting result. \
                    Caller should retry the operation."
                );
            }
            self.condvar.wait(&mut guard);
        }
    }
}

/// Sync single-flight deduplication utility.
///
/// Ensures that concurrent requests for the same key trigger only one
/// actual load operation. Subsequent requests wait for and share the result.
///
/// This is the synchronous version using `parking_lot::Condvar`.
///
/// # Type Parameters
///
/// - `K`: Key type (must be `Eq + Hash + Clone + Send`)
/// - `V`: Value type (must be `Clone + Send`)
/// - `E`: Error type (must be `Clone + Send`)
///
/// # Thread Safety
///
/// All operations are thread-safe and can be called from multiple threads.
///
/// # Example
///
/// ```ignore
/// let sf = SingleFlightSync::<String, Data, MyError>::new("my_operation");
///
/// // Multiple concurrent calls with the same key will dedupe
/// let result = sf.get_or_load("key".to_string(), || {
///     expensive_load()
/// });
/// ```
pub struct SingleFlightSync<K, V, E>
where
    K: Eq + Hash + Clone + Send + 'static,
    V: Clone + Send + 'static,
    E: Clone + Send + 'static,
{
    /// In-flight operations keyed by request key.
    in_flight: Mutex<HashMap<K, Arc<InFlightEntrySync<V, E>>>>,
    /// Operation label for metrics.
    operation_label: &'static str,
    /// Optional metrics collector.
    metrics: SharedMetrics,
}

impl<K, V, E> SingleFlightSync<K, V, E>
where
    K: Eq + Hash + Clone + Send + 'static,
    V: Clone + Send + 'static,
    E: Clone + Send + 'static,
{
    /// Create a new SingleFlightSync instance without metrics.
    pub fn new(operation_label: &'static str) -> Self {
        Self {
            in_flight: Mutex::new(HashMap::new()),
            operation_label,
            metrics: Arc::new(NoOpMetrics),
        }
    }

    /// Create a new SingleFlightSync instance with metrics collector.
    pub fn with_metrics(operation_label: &'static str, metrics: SharedMetrics) -> Self {
        Self {
            in_flight: Mutex::new(HashMap::new()),
            operation_label,
            metrics,
        }
    }

    /// Execute load function or wait for in-flight result.
    ///
    /// # Arguments
    ///
    /// * `key` - The deduplication key
    /// * `load_fn` - Function to execute on cache miss
    ///
    /// # Returns
    ///
    /// * `Ok(V)` - The loaded value (either fresh or from concurrent load)
    /// * `Err(E)` - Error from loader (propagated to all waiters)
    pub fn get_or_load<F>(&self, key: K, load_fn: F) -> Result<V, E>
    where
        F: FnOnce() -> Result<V, E>,
    {
        // Get or insert in-flight entry
        let (entry, is_leader) = {
            let mut in_flight = self.in_flight.lock();
            if let Some(existing) = in_flight.get(&key) {
                let entry = existing.clone();
                let waiter_num = entry.add_waiter();
                (entry, waiter_num == 1)
            } else {
                let entry = Arc::new(InFlightEntrySync::new());
                entry.add_waiter(); // Count starts at 1 for leader
                in_flight.insert(key.clone(), entry.clone());
                (entry, true)
            }
        };

        if is_leader {
            // Leader path: execute load
            self.metrics.record_leader(self.operation_label);

            tracing::debug!(
                operation = %self.operation_label,
                "SingleFlightSync: first request, triggering load"
            );

            // Use a guard to ensure cleanup on panic
            let cleanup_guard = SyncCleanupGuard {
                in_flight: &self.in_flight,
                key: key.clone(),
                entry: entry.clone(),
            };

            let result = load_fn();

            // Prevent cleanup guard from running (we'll clean up manually)
            std::mem::forget(cleanup_guard);

            // Store result and notify waiters
            entry.set_result(result.clone());

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
                    "SingleFlightSync: load completed with waiters"
                );
            }

            // Cleanup: remove from in-flight map
            {
                let mut in_flight = self.in_flight.lock();
                in_flight.remove(&key);
            }

            result
        } else {
            // Waiter path: wait for leader to complete
            self.metrics.record_waiter(self.operation_label);

            tracing::debug!(
                operation = %self.operation_label,
                "SingleFlightSync: waiting for in-progress load"
            );

            let wait_start = Instant::now();

            let result = entry.wait();

            let wait_time_ms = wait_start.elapsed().as_millis();

            tracing::debug!(
                operation = %self.operation_label,
                wait_time_ms = wait_time_ms,
                success = result.is_ok(),
                "SingleFlightSync: wait completed"
            );

            result
        }
    }

    /// Check if a key is currently being loaded.
    pub fn is_loading(&self, key: &K) -> bool {
        self.in_flight.lock().contains_key(key)
    }

    /// Get current waiter count for a key.
    ///
    /// Returns 0 if the key is not being loaded.
    pub fn waiter_count(&self, key: &K) -> usize {
        self.in_flight
            .lock()
            .get(key)
            .map(|entry| entry.get_waiter_count())
            .unwrap_or(0)
    }

    /// Get metrics snapshot.
    pub fn stats(&self) -> SingleFlightStats {
        let in_flight = self.in_flight.lock();
        let mut total_waiters = 0;
        let mut oldest_load_age_ms = 0u128;
        let pending_count = in_flight.len();

        for entry in in_flight.values() {
            total_waiters += entry.get_waiter_count();
            let age = entry.elapsed_ms();
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

impl<K, V, E> Debug for SingleFlightSync<K, V, E>
where
    K: Eq + Hash + Clone + Send + 'static,
    V: Clone + Send + 'static,
    E: Clone + Send + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pending = self.in_flight.lock().len();
        f.debug_struct("SingleFlightSync")
            .field("operation_label", &self.operation_label)
            .field("pending_loads", &pending)
            .finish()
    }
}

/// RAII guard to ensure in-flight entry is cleaned up on panic.
///
/// If the leader panics during load, this guard will:
/// 1. Mark the entry as panicked (so waiters know to propagate the panic)
/// 2. Remove the entry from the in-flight map
struct SyncCleanupGuard<'a, K, V, E>
where
    K: Eq + Hash + Clone + Send + 'static,
    V: Clone + Send + 'static,
    E: Clone + Send + 'static,
{
    in_flight: &'a Mutex<HashMap<K, Arc<InFlightEntrySync<V, E>>>>,
    key: K,
    entry: Arc<InFlightEntrySync<V, E>>,
}

impl<K, V, E> Drop for SyncCleanupGuard<'_, K, V, E>
where
    K: Eq + Hash + Clone + Send + 'static,
    V: Clone + Send + 'static,
    E: Clone + Send + 'static,
{
    fn drop(&mut self) {
        // This only runs on panic (normal path uses mem::forget)
        // Mark entry as panicked so waiters know to propagate
        self.entry.mark_panicked();
        // Remove from in-flight map
        let mut in_flight = self.in_flight.lock();
        in_flight.remove(&self.key);
    }
}
