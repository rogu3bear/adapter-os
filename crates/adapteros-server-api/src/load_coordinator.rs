//! Load coordinator for handling concurrent first-requests (thundering herd protection)
//!
//! When multiple requests arrive for an adapter that isn't loaded yet, only the first
//! request triggers the actual load operation. Subsequent requests wait for the load
//! to complete and receive the same result.
//!
//! ## Architecture
//!
//! - **DashMap**: Lock-free concurrent hash map for pending loads
//! - **LoadWaiter**: Coordination structure using tokio::sync primitives
//! - **OnceCell**: Ensures load result is set exactly once
//! - **Notify**: Wakes all waiting tasks when load completes
//!
//! ## Metrics
//!
//! Logs include:
//! - Number of waiters coalesced for each load
//! - Wait times for concurrent requests
//! - Load completion success/failure
//!
//! [source: crates/adapteros-server-api/src/load_coordinator.rs]
//! [related: CLAUDE.md#core-standards]

use adapteros_core::AosError;
use adapteros_db::Db;
use adapteros_lora_lifecycle::loader::AdapterHandle;
use dashmap::DashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{Notify, OnceCell};

/// Coordination structure for a single adapter load operation
struct LoadWaiter {
    /// Notify all waiters when load completes
    notify: Notify,
    /// Load result (set exactly once by first requester)
    result: OnceCell<Result<AdapterHandle, AosError>>,
    /// Number of concurrent waiters
    waiter_count: AtomicUsize,
    /// Timestamp when first request arrived
    first_request_at: std::time::Instant,
}

impl LoadWaiter {
    fn new() -> Self {
        Self {
            notify: Notify::new(),
            result: OnceCell::new(),
            waiter_count: AtomicUsize::new(0),
            first_request_at: std::time::Instant::now(),
        }
    }

    /// Increment waiter count and return new count
    fn add_waiter(&self) -> usize {
        self.waiter_count.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Get current waiter count
    fn get_waiter_count(&self) -> usize {
        self.waiter_count.load(Ordering::SeqCst)
    }

    /// Set the load result (can only be called once)
    fn set_result(&self, result: Result<AdapterHandle, AosError>) -> Result<(), ()> {
        self.result.set(result).map_err(|_| ())
    }

    /// Get the load result (blocking if not set yet)
    async fn get_result(&self) -> Result<AdapterHandle, AosError> {
        // Wait for notification that result is available
        self.notify.notified().await;

        // Clone the result from OnceCell
        match self.result.get() {
            Some(Ok(handle)) => {
                // Clone the handle for this waiter
                Ok(AdapterHandle {
                    adapter_id: handle.adapter_id,
                    path: handle.path.clone(),
                    memory_bytes: handle.memory_bytes,
                    metadata: handle.metadata.clone(),
                })
            }
            Some(Err(e)) => {
                // Clone the error
                Err(AosError::Lifecycle(format!("Load failed: {}", e)))
            }
            None => {
                // This should never happen if notify was called
                Err(AosError::Lifecycle(
                    "Load result not available despite notification".to_string(),
                ))
            }
        }
    }

    /// Notify all waiters that result is available
    fn notify_all(&self) {
        self.notify.notify_waiters();
    }

    /// Get elapsed time since first request
    fn elapsed(&self) -> std::time::Duration {
        self.first_request_at.elapsed()
    }
}

/// Coordinator for handling concurrent adapter load requests
///
/// Prevents "thundering herd" problems where multiple requests for the same
/// adapter trigger redundant load operations. Only the first request performs
/// the actual load, while subsequent requests wait for the result.
///
/// ## Example
///
/// ```ignore
/// let coordinator = LoadCoordinator::new();
///
/// // First request triggers load, subsequent requests wait
/// let handle = coordinator.load_or_wait("my-adapter", || async {
///     adapter_loader.load_adapter(42, "my-adapter")
/// }).await?;
/// ```
pub struct LoadCoordinator {
    /// Pending load operations by model_id
    pending_loads: DashMap<String, Arc<LoadWaiter>>,
}

impl LoadCoordinator {
    /// Create a new load coordinator
    pub fn new() -> Self {
        Self {
            pending_loads: DashMap::new(),
        }
    }

    /// Load model or wait for in-progress load
    ///
    /// The first caller triggers the load via `load_fn`, subsequent callers wait
    /// for the result. All callers receive the same result (success or error).
    ///
    /// ## Parameters
    ///
    /// - `model_id`: Unique identifier for the model/adapter
    /// - `load_fn`: Async function that performs the actual load operation
    ///
    /// ## Returns
    ///
    /// - `Ok(AdapterHandle)`: Successfully loaded or retrieved from concurrent load
    /// - `Err(AosError)`: Load failed (error is shared across all waiters)
    ///
    /// ## Metrics
    ///
    /// Logs info-level messages when multiple requests coalesce, including:
    /// - Number of waiters
    /// - Total wait time for waiters
    pub async fn load_or_wait<F, Fut>(
        &self,
        model_id: &str,
        load_fn: F,
    ) -> Result<AdapterHandle, AosError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<AdapterHandle, AosError>>,
    {
        let model_id_owned = model_id.to_string();

        // Try to insert a new waiter, or get existing one
        let waiter = self
            .pending_loads
            .entry(model_id_owned.clone())
            .or_insert_with(|| Arc::new(LoadWaiter::new()))
            .clone();

        let waiter_num = waiter.add_waiter();

        if waiter_num == 1 {
            // First request - we perform the load
            tracing::debug!(
                model_id = %model_id,
                "First request for adapter, triggering load"
            );

            let load_result = load_fn().await;
            let is_success = load_result.is_ok();

            // Store result for all waiters (convert error to string for sharing)
            let shared_result = match &load_result {
                Ok(handle) => Ok(AdapterHandle {
                    adapter_id: handle.adapter_id,
                    path: handle.path.clone(),
                    memory_bytes: handle.memory_bytes,
                    metadata: handle.metadata.clone(),
                }),
                Err(e) => Err(AosError::Lifecycle(format!("Load failed: {}", e))),
            };

            if waiter.set_result(shared_result).is_err() {
                // Result was already set by another task (race condition)
                tracing::warn!(
                    model_id = %model_id,
                    "Load result was set by another task (unexpected race)"
                );
            }

            // Notify all waiters
            waiter.notify_all();

            let elapsed = waiter.elapsed();
            let final_waiter_count = waiter.get_waiter_count();

            if final_waiter_count > 1 {
                tracing::info!(
                    model_id = %model_id,
                    waiters = final_waiter_count - 1,
                    load_time_ms = elapsed.as_millis(),
                    success = is_success,
                    "Load completed with {} waiting requests",
                    final_waiter_count - 1
                );
            } else {
                tracing::debug!(
                    model_id = %model_id,
                    load_time_ms = elapsed.as_millis(),
                    success = is_success,
                    "Load completed with no waiters"
                );
            }

            // Clean up entry from pending_loads
            self.pending_loads.remove(&model_id_owned);

            load_result
        } else {
            // Subsequent request - wait for first request to complete
            let wait_start = std::time::Instant::now();

            tracing::debug!(
                model_id = %model_id,
                waiter_position = waiter_num,
                "Waiting for in-progress load"
            );

            let result = waiter.get_result().await;

            let wait_time = wait_start.elapsed();
            tracing::info!(
                model_id = %model_id,
                wait_time_ms = wait_time.as_millis(),
                success = result.is_ok(),
                "Load wait completed"
            );

            result
        }
    }

    /// Load model or wait for in-progress load, with archive/purge safety check
    ///
    /// First verifies the adapter is loadable (not archived/purged), then
    /// proceeds with the standard load-or-wait logic. This prevents attempts
    /// to load adapters whose .aos files have been garbage collected.
    ///
    /// ## Parameters
    ///
    /// - `model_id`: Unique identifier for the model/adapter (adapter_id field)
    /// - `db`: Database connection for loadability check
    /// - `load_fn`: Async function that performs the actual load operation
    ///
    /// ## Returns
    ///
    /// - `Ok(AdapterHandle)`: Successfully loaded or retrieved from concurrent load
    /// - `Err(AosError::Lifecycle)`: Adapter is archived or purged, cannot be loaded
    /// - `Err(AosError)`: Load failed for other reasons
    pub async fn load_or_wait_with_check<F, Fut>(
        &self,
        model_id: &str,
        db: &Db,
        load_fn: F,
    ) -> Result<AdapterHandle, AosError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<AdapterHandle, AosError>>,
    {
        // Safety check: verify adapter is loadable (not archived/purged)
        match db.is_adapter_loadable(model_id).await {
            Ok(true) => {
                // Adapter is loadable, proceed with normal load
                self.load_or_wait(model_id, load_fn).await
            }
            Ok(false) => {
                // Adapter is archived or purged, reject load attempt
                tracing::warn!(
                    adapter_id = %model_id,
                    "Rejected load attempt for archived/purged adapter"
                );
                Err(AosError::Lifecycle(format!(
                    "Cannot load adapter '{}': adapter is archived or purged",
                    model_id
                )))
            }
            Err(e) => {
                // Database lookup failed - could be adapter not found or DB error
                tracing::error!(
                    adapter_id = %model_id,
                    error = %e,
                    "Failed to check adapter loadability"
                );
                Err(e)
            }
        }
    }

    /// Check if a model is currently being loaded
    ///
    /// Returns `true` if there's an active load operation for the given model_id.
    pub fn is_loading(&self, model_id: &str) -> bool {
        self.pending_loads.contains_key(model_id)
    }

    /// Get count of waiters for a model
    ///
    /// Returns the number of requests currently waiting for the model to load.
    /// Returns 0 if the model is not being loaded.
    pub fn waiter_count(&self, model_id: &str) -> usize {
        self.pending_loads
            .get(model_id)
            .map(|waiter| waiter.get_waiter_count())
            .unwrap_or(0)
    }

    /// Cancel a pending load
    ///
    /// Removes the load waiter from the pending map. This does not stop
    /// the actual load operation if it's in progress, but prevents new
    /// waiters from joining.
    ///
    /// All existing waiters will still receive the result when the load completes.
    pub fn cancel(&self, model_id: &str) {
        if let Some((_, waiter)) = self.pending_loads.remove(model_id) {
            let count = waiter.get_waiter_count();
            tracing::warn!(
                model_id = %model_id,
                waiters = count,
                "Cancelled pending load with {} waiters",
                count
            );
        }
    }

    /// Get metrics for monitoring
    ///
    /// Returns information about currently pending loads for observability.
    pub fn metrics(&self) -> LoadCoordinatorMetrics {
        let mut total_waiters = 0;
        let mut oldest_load_age_ms = 0u128;
        let pending_count = self.pending_loads.len();

        for entry in self.pending_loads.iter() {
            let waiter = entry.value();
            total_waiters += waiter.get_waiter_count();
            let age = waiter.elapsed().as_millis();
            if age > oldest_load_age_ms {
                oldest_load_age_ms = age;
            }
        }

        LoadCoordinatorMetrics {
            pending_loads: pending_count,
            total_waiters,
            oldest_load_age_ms,
        }
    }
}

impl Default for LoadCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// Metrics for load coordinator monitoring
#[derive(Debug, Clone, Copy)]
pub struct LoadCoordinatorMetrics {
    /// Number of models currently being loaded
    pub pending_loads: usize,
    /// Total number of requests waiting across all loads
    pub total_waiters: usize,
    /// Age of oldest pending load in milliseconds
    pub oldest_load_age_ms: u128,
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_lora_lifecycle::loader::AdapterMetadata;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicU32};

    fn create_test_handle(id: u16) -> AdapterHandle {
        AdapterHandle {
            adapter_id: id,
            path: PathBuf::from(format!("/test/adapter_{}.aos", id)),
            memory_bytes: 1024 * 1024,
            metadata: AdapterMetadata {
                num_parameters: 1000,
                rank: Some(8),
                target_modules: vec!["q_proj".to_string(), "v_proj".to_string()],
            },
        }
    }

    #[tokio::test]
    async fn test_single_request() {
        let coordinator = LoadCoordinator::new();
        let load_count = Arc::new(AtomicU32::new(0));
        let load_count_clone = load_count.clone();

        let result = coordinator
            .load_or_wait("test-adapter", || async move {
                load_count_clone.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                Ok(create_test_handle(42))
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(load_count.load(Ordering::SeqCst), 1);
        let handle = result.expect("Load should succeed");
        assert_eq!(handle.adapter_id, 42);
    }

    #[tokio::test]
    async fn test_concurrent_requests_coalesce() {
        let coordinator = Arc::new(LoadCoordinator::new());
        let load_count = Arc::new(AtomicU32::new(0));

        let mut handles = vec![];

        // Spawn 10 concurrent requests
        for _ in 0..10 {
            let coord = coordinator.clone();
            let count = load_count.clone();
            let handle = tokio::spawn(async move {
                coord
                    .load_or_wait("test-adapter", || async move {
                        count.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                        Ok(create_test_handle(42))
                    })
                    .await
            });
            handles.push(handle);
        }

        // Wait for all requests to complete
        let results: Vec<_> = futures::future::join_all(handles).await;

        // All should succeed
        for result in results {
            let join_result = result.expect("Task should not panic");
            let adapter = join_result.expect("Load should succeed");
            assert_eq!(adapter.adapter_id, 42);
        }

        // Load should only have been called once
        assert_eq!(load_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_error_propagation() {
        let coordinator = Arc::new(LoadCoordinator::new());

        let mut handles = vec![];

        // Spawn 5 concurrent requests that will fail
        for _ in 0..5 {
            let coord = coordinator.clone();
            let handle = tokio::spawn(async move {
                coord
                    .load_or_wait("failing-adapter", || async move {
                        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
                        Err(AosError::Lifecycle("Test failure".to_string()))
                    })
                    .await
            });
            handles.push(handle);
        }

        // Wait for all requests
        let results: Vec<_> = futures::future::join_all(handles).await;

        // All should receive the error
        for result in results {
            let join_result = result.expect("Task should not panic");
            let err = join_result.expect_err("Load should fail");
            assert!(matches!(err, AosError::Lifecycle(_)));
        }
    }

    #[tokio::test]
    async fn test_is_loading() {
        let coordinator = Arc::new(LoadCoordinator::new());
        let started = Arc::new(AtomicBool::new(false));
        let started_clone = started.clone();

        assert!(!coordinator.is_loading("test-adapter"));

        // Start a load in background
        let coord_clone = coordinator.clone();
        let load_handle = tokio::spawn(async move {
            coord_clone
                .load_or_wait("test-adapter", || async move {
                    started_clone.store(true, Ordering::SeqCst);
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    Ok(create_test_handle(42))
                })
                .await
        });

        // Wait for load to start
        while !started.load(Ordering::SeqCst) {
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        }

        assert!(coordinator.is_loading("test-adapter"));
        assert_eq!(coordinator.waiter_count("test-adapter"), 1);

        // Wait for load to complete
        let join_result = load_handle.await.expect("Task should not panic");
        join_result.expect("Load should succeed");

        assert!(!coordinator.is_loading("test-adapter"));
        assert_eq!(coordinator.waiter_count("test-adapter"), 0);
    }

    #[tokio::test]
    async fn test_cancel() {
        let coordinator = Arc::new(LoadCoordinator::new());

        // Start a load
        let coord_clone = coordinator.clone();
        let _load_handle = tokio::spawn(async move {
            coord_clone
                .load_or_wait("test-adapter", || async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    Ok(create_test_handle(42))
                })
                .await
        });

        // Wait a bit for load to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        assert!(coordinator.is_loading("test-adapter"));

        // Cancel the load
        coordinator.cancel("test-adapter");

        assert!(!coordinator.is_loading("test-adapter"));
    }

    #[tokio::test]
    async fn test_metrics() {
        let coordinator = Arc::new(LoadCoordinator::new());

        // No pending loads initially
        let metrics = coordinator.metrics();
        assert_eq!(metrics.pending_loads, 0);
        assert_eq!(metrics.total_waiters, 0);

        // Start multiple loads
        let coord_clone1 = coordinator.clone();
        let handle1 = tokio::spawn(async move {
            coord_clone1
                .load_or_wait("adapter-1", || async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    Ok(create_test_handle(1))
                })
                .await
        });

        let coord_clone2 = coordinator.clone();
        let handle2 = tokio::spawn(async move {
            coord_clone2
                .load_or_wait("adapter-2", || async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    Ok(create_test_handle(2))
                })
                .await
        });

        // Wait for loads to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let metrics = coordinator.metrics();
        assert_eq!(metrics.pending_loads, 2);
        assert_eq!(metrics.total_waiters, 2);
        assert!(metrics.oldest_load_age_ms > 0);

        // Wait for completion
        let join_result1 = handle1.await.expect("Task 1 should not panic");
        join_result1.expect("Load 1 should succeed");
        let join_result2 = handle2.await.expect("Task 2 should not panic");
        join_result2.expect("Load 2 should succeed");

        let metrics = coordinator.metrics();
        assert_eq!(metrics.pending_loads, 0);
        assert_eq!(metrics.total_waiters, 0);
    }

    #[tokio::test]
    async fn test_sequential_loads_same_model() {
        let coordinator = LoadCoordinator::new();
        let load_count = Arc::new(AtomicU32::new(0));

        // First load
        let count1 = load_count.clone();
        let result1 = coordinator
            .load_or_wait("test-adapter", || async move {
                count1.fetch_add(1, Ordering::SeqCst);
                Ok(create_test_handle(42))
            })
            .await;
        assert!(result1.is_ok());

        // Second load (not concurrent, should trigger new load)
        let count2 = load_count.clone();
        let result2 = coordinator
            .load_or_wait("test-adapter", || async move {
                count2.fetch_add(1, Ordering::SeqCst);
                Ok(create_test_handle(43))
            })
            .await;
        assert!(result2.is_ok());

        // Both loads should have been called
        assert_eq!(load_count.load(Ordering::SeqCst), 2);
        let handle2 = result2.expect("Second load should succeed");
        assert_eq!(handle2.adapter_id, 43);
    }
}
