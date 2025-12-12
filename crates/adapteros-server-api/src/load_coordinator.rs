//! Load coordinator for handling concurrent first-requests (thundering herd protection)
//!
//! When multiple requests arrive for an adapter that isn't loaded yet, only the first
//! request triggers the actual load operation. Subsequent requests wait for the load
//! to complete and receive the same result.
//!
//! ## Architecture
//!
//! Uses the unified `SingleFlight` utility from `adapteros_core::singleflight` for
//! deduplication. This ensures consistent behavior across model loads, adapter loads,
//! and prefix KV builds.
//!
//! ## Metrics
//!
//! Prometheus metrics are recorded via `SingleFlightMetrics`:
//! - `singleflight_leader_count_total{operation="adapter_load"}` - Requests that triggered loads
//! - `singleflight_waiter_count{operation="adapter_load"}` - Current waiters
//! - `singleflight_error_count_total{operation="adapter_load"}` - Load errors
//!
//! [source: crates/adapteros-server-api/src/load_coordinator.rs]
//! [related: CLAUDE.md#core-standards]

use adapteros_core::singleflight::{SingleFlight, SingleFlightMetrics, SingleFlightStats};
use adapteros_core::AosError;
use adapteros_db::Db;
use adapteros_lora_lifecycle::loader::AdapterHandle;
use std::sync::Arc;

/// Operation label for SingleFlight metrics
const OPERATION_LABEL: &str = "adapter_load";

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
    /// SingleFlight for deduplication
    /// Uses String error type since AosError is not Clone
    singleflight: Arc<SingleFlight<String, AdapterHandle, String>>,
}

impl LoadCoordinator {
    /// Create a new load coordinator without metrics
    pub fn new() -> Self {
        Self {
            singleflight: Arc::new(SingleFlight::new(OPERATION_LABEL)),
        }
    }

    /// Create a new load coordinator with metrics
    pub fn with_metrics(metrics: Arc<dyn SingleFlightMetrics>) -> Self {
        Self {
            singleflight: Arc::new(SingleFlight::with_metrics(OPERATION_LABEL, metrics)),
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
    /// Records leader/waiter/error counts via Prometheus metrics when configured.
    pub async fn load_or_wait<F, Fut>(
        &self,
        model_id: &str,
        load_fn: F,
    ) -> Result<AdapterHandle, AosError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<AdapterHandle, AosError>>,
    {
        // Delegate to SingleFlight, converting errors at boundaries
        self.singleflight
            .get_or_load(model_id.to_string(), || async move {
                // Run the load function, converting AosError to String for sharing
                load_fn()
                    .await
                    .map_err(|e| format!("Load failed: {}", e))
            })
            .await
            .map_err(|e| AosError::Lifecycle(e))
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
        self.singleflight.is_loading(&model_id.to_string())
    }

    /// Get count of waiters for a model
    ///
    /// Returns the number of requests currently waiting for the model to load.
    /// Returns 0 if the model is not being loaded.
    pub fn waiter_count(&self, model_id: &str) -> usize {
        self.singleflight.waiter_count(&model_id.to_string())
    }

    /// Get metrics for monitoring
    ///
    /// Returns information about currently pending loads for observability.
    pub fn metrics(&self) -> LoadCoordinatorMetrics {
        let stats = self.singleflight.stats();
        LoadCoordinatorMetrics {
            pending_loads: stats.pending_loads,
            total_waiters: stats.total_waiters,
            oldest_load_age_ms: stats.oldest_load_age_ms,
        }
    }

    /// Get the underlying SingleFlight stats directly
    pub fn singleflight_stats(&self) -> SingleFlightStats {
        self.singleflight.stats()
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
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    fn create_test_handle(id: u16) -> AdapterHandle {
        AdapterHandle {
            adapter_id: id,
            path: PathBuf::from(format!("/test/adapter_{}.aos", id)),
            memory_bytes: 1024 * 1024,
            metadata: AdapterMetadata {
                num_parameters: 1000,
                rank: Some(8),
                target_modules: vec!["q_proj".to_string(), "v_proj".to_string()],
                ..Default::default()
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
