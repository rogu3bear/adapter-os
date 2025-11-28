//! Operation Deduplication Tracker
//!
//! Prevents concurrent operations on the same model by tracking ongoing operations
//! across all clients and API requests. Uses in-memory storage with proper locking.
//!
//! # Citations
//! - Evidence tracker pattern: [source: crates/adapteros-policy/src/evidence_tracker.rs L172-L187]
//! - Concurrent filesystem tracker: [source: crates/adapteros-concurrent-fs/src/manager.rs L30-L37]
//! - Progress broadcasting: [source: crates/adapteros-server-api/src/state.rs L428-L429]

use crate::types::OperationProgressEvent;
use adapteros_core::AosError;
use chrono::Utc;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

#[cfg(feature = "metrics")]
use adapteros_metrics_exporter::MetricsCollector;

#[cfg(feature = "redis")]
use redis::{AsyncCommands, Client as RedisClient};

/// Type of model operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelOperationType {
    Load,
    Unload,
}

/// Type of adapter operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdapterOperationType {
    Load,
    Unload,
}

/// Represents an ongoing operation
#[derive(Debug, Clone)]
pub struct OngoingOperation {
    pub operation_type: OperationType,
    pub started_at: Instant,
    pub tenant_id: String,
    pub progress_pct: f64,
    pub last_progress_update: Instant,
    pub cancellation_token: Arc<CancellationToken>,
}

/// Type of operation (model or adapter)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationType {
    Model(ModelOperationType),
    Adapter(AdapterOperationType),
}

/// Storage backend for operation tracking
#[derive(Debug)]
pub enum OperationStorage {
    /// In-memory storage (single server)
    InMemory(Arc<RwLock<HashMap<(String, String), OngoingOperation>>>),
    /// Redis-based distributed storage (multi-server)
    #[cfg(feature = "redis")]
    Redis {
        client: RedisClient,
        key_prefix: String,
        fallback_to_memory: bool,
    },
}

/// Tracks ongoing operations to prevent duplicates
#[derive(Debug)]
pub struct OperationTracker {
    storage: OperationStorage,
    default_timeout: Duration,
    #[cfg(feature = "metrics")]
    metrics: Option<Arc<MetricsCollector>>,
    /// Broadcast channel for progress updates
    progress_tx: Option<broadcast::Sender<OperationProgressEvent>>,
}

impl OperationTracker {
    /// Create a new in-memory operation tracker
    pub fn new(default_timeout: Duration) -> Self {
        Self {
            storage: OperationStorage::InMemory(Arc::new(RwLock::new(HashMap::new()))),
            default_timeout,
            #[cfg(feature = "metrics")]
            metrics: None,
            progress_tx: None,
        }
    }

    /// Create a new operation tracker with progress broadcasting
    pub fn new_with_progress(
        default_timeout: Duration,
        progress_tx: broadcast::Sender<OperationProgressEvent>,
    ) -> Self {
        Self {
            storage: OperationStorage::InMemory(Arc::new(RwLock::new(HashMap::new()))),
            default_timeout,
            #[cfg(feature = "metrics")]
            metrics: None,
            progress_tx: Some(progress_tx),
        }
    }

    /// Create a new in-memory operation tracker with metrics
    #[cfg(feature = "metrics")]
    pub fn new_with_metrics(default_timeout: Duration, metrics: Arc<MetricsCollector>) -> Self {
        Self {
            storage: OperationStorage::InMemory(Arc::new(RwLock::new(HashMap::new()))),
            default_timeout,
            metrics: Some(metrics),
        }
    }

    /// Create with default 5-minute timeout
    pub fn new_default() -> Self {
        Self::new(Duration::from_secs(300)) // 5 minutes
    }

    /// Create a Redis-based distributed operation tracker
    #[cfg(feature = "redis")]
    pub fn new_redis(
        redis_url: &str,
        key_prefix: &str,
        default_timeout: Duration,
    ) -> Result<Self, redis::RedisError> {
        let client = RedisClient::open(redis_url)?;
        Ok(Self {
            storage: OperationStorage::Redis {
                client,
                key_prefix: key_prefix.to_string(),
                fallback_to_memory: false,
            },
            default_timeout,
        })
    }

    /// Get a reference to the in-memory operations HashMap for read access
    fn get_operations_read(
        &self,
    ) -> Result<Arc<RwLock<HashMap<(String, String), OngoingOperation>>>, AosError> {
        match &self.storage {
            OperationStorage::InMemory(ops) => Ok(ops.clone()),
            #[cfg(feature = "redis")]
            OperationStorage::Redis { .. } => {
                // Redis implementation not yet complete
                Err(AosError::Config("Redis storage not yet implemented".into()))
            }
        }
    }

    /// Get a reference to the in-memory operations HashMap for write access
    fn get_operations_write(
        &self,
    ) -> Result<Arc<RwLock<HashMap<(String, String), OngoingOperation>>>, AosError> {
        match &self.storage {
            OperationStorage::InMemory(ops) => Ok(ops.clone()),
            #[cfg(feature = "redis")]
            OperationStorage::Redis { .. } => {
                // Redis implementation not yet complete
                Err(AosError::Config("Redis storage not yet implemented".into()))
            }
        }
    }

    /// Attempt to start a model operation, returns true if started successfully
    pub async fn start_model_operation(
        &self,
        model_id: &str,
        tenant_id: &str,
        operation_type: ModelOperationType,
    ) -> Result<(), AosError> {
        self.start_operation(model_id, tenant_id, OperationType::Model(operation_type))
            .await
    }

    /// Attempt to start an adapter operation, returns true if started successfully
    pub async fn start_adapter_operation(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        operation_type: AdapterOperationType,
    ) -> Result<(), AosError> {
        self.start_operation(
            adapter_id,
            tenant_id,
            OperationType::Adapter(operation_type),
        )
        .await
    }

    /// Attempt to start an operation, returns true if started successfully
    pub async fn start_operation(
        &self,
        resource_id: &str,
        tenant_id: &str,
        operation_type: OperationType,
    ) -> Result<(), AosError> {
        let key = (resource_id.to_string(), tenant_id.to_string());
        let operations_lock = self.get_operations_write()?;
        let mut operations = operations_lock.write().await;

        // Clean up expired operations first
        self.cleanup_expired(&mut operations);

        // Check if operation already exists
        if let Some(existing) = operations.get(&key) {
            warn!(
                resource_id = %resource_id,
                tenant_id = %tenant_id,
                conflicting_operation = ?existing.operation_type,
                remaining_time_secs = %self.default_timeout.saturating_sub(existing.started_at.elapsed()).as_secs(),
                "Operation conflict detected"
            );
            return Err(AosError::Validation(format!(
                "Operation conflict: existing {:?} for resource {}, tenant {}",
                existing.operation_type, resource_id, tenant_id
            )));
        }

        // Start new operation
        let now = Instant::now();
        let operation = OngoingOperation {
            operation_type,
            started_at: now,
            tenant_id: tenant_id.to_string(),
            progress_pct: 0.0,
            last_progress_update: now,
            cancellation_token: Arc::new(CancellationToken::new()),
        };

        operations.insert(key.clone(), operation.clone());

        // Emit initial progress event
        if let Some(ref tx) = self.progress_tx {
            let (operation_type_str, _resource_type) = match operation_type {
                OperationType::Model(ModelOperationType::Load) => ("load", "model"),
                OperationType::Model(ModelOperationType::Unload) => ("unload", "model"),
                OperationType::Adapter(AdapterOperationType::Load) => ("load", "adapter"),
                OperationType::Adapter(AdapterOperationType::Unload) => ("unload", "adapter"),
            };

            let _ = tx.send(OperationProgressEvent {
                operation_id: format!("{}:{}", resource_id, tenant_id),
                model_id: resource_id.to_string(),
                operation: operation_type_str.to_string(),
                status: "started".to_string(),
                progress_percent: Some(0),
                duration_ms: None,
                error_message: None,
                created_at: Utc::now(),
            });
        }

        #[cfg(feature = "metrics")]
        if let Some(metrics) = &self.metrics {
            let operation_type_str = match operation_type {
                OperationType::Model(mt) => format!("model_{:?}", mt),
                OperationType::Adapter(at) => format!("adapter_{:?}", at),
            };
            let _ = metrics.record_counter(
                "operation_tracker.operations_started",
                1,
                &[("operation_type", &operation_type_str)],
            );
        }

        debug!(
            resource_id = %resource_id,
            tenant_id = %tenant_id,
            operation = ?operation_type,
            "Started operation"
        );

        Ok(())
    }

    /// Update model operation progress
    pub async fn update_model_progress(
        &self,
        model_id: &str,
        tenant_id: &str,
        progress_pct: f64,
        message: Option<String>,
    ) {
        self.update_progress(model_id, tenant_id, progress_pct, message)
            .await
    }

    /// Update adapter operation progress
    pub async fn update_adapter_progress(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        progress_pct: f64,
        message: Option<String>,
    ) {
        self.update_progress(adapter_id, tenant_id, progress_pct, message)
            .await
    }

    pub async fn update_progress(
        &self,
        resource_id: &str,
        tenant_id: &str,
        progress_pct: f64,
        _message: Option<String>,
    ) {
        let key = (resource_id.to_string(), tenant_id.to_string());
        let operations_lock = match self.get_operations_write() {
            Ok(lock) => lock,
            Err(e) => {
                warn!(error = %e, "Failed to get operations lock for progress update");
                return;
            }
        };
        let mut operations = operations_lock.write().await;

        if let Some(op) = operations.get_mut(&key) {
            op.progress_pct = progress_pct.clamp(0.0, 100.0);
            op.last_progress_update = Instant::now();

            // Emit progress event
            if let Some(ref tx) = self.progress_tx {
                let elapsed = op.started_at.elapsed().as_secs_f64();
                let operation_type_str = match op.operation_type {
                    OperationType::Model(ModelOperationType::Load) => "load",
                    OperationType::Model(ModelOperationType::Unload) => "unload",
                    OperationType::Adapter(AdapterOperationType::Load) => "load",
                    OperationType::Adapter(AdapterOperationType::Unload) => "unload",
                };

                let _ = tx.send(OperationProgressEvent {
                    operation_id: format!("{}:{}", resource_id, tenant_id),
                    model_id: resource_id.to_string(),
                    operation: operation_type_str.to_string(),
                    status: "in_progress".to_string(),
                    progress_percent: Some(op.progress_pct as u8),
                    duration_ms: Some(elapsed as u64 * 1000),
                    error_message: None,
                    created_at: Utc::now(),
                });
            }
        }
    }

    /// Complete a model operation
    pub async fn complete_model_operation(
        &self,
        model_id: &str,
        tenant_id: &str,
        operation_type: ModelOperationType,
        success: bool,
    ) {
        self.complete_operation(
            model_id,
            tenant_id,
            OperationType::Model(operation_type),
            success,
        )
        .await
    }

    /// Complete an adapter operation
    pub async fn complete_adapter_operation(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        operation_type: AdapterOperationType,
        success: bool,
    ) {
        self.complete_operation(
            adapter_id,
            tenant_id,
            OperationType::Adapter(operation_type),
            success,
        )
        .await
    }

    pub async fn complete_operation(
        &self,
        resource_id: &str,
        tenant_id: &str,
        operation_type: OperationType,
        success: bool,
    ) {
        let key = (resource_id.to_string(), tenant_id.to_string());
        let operations_lock = match self.get_operations_write() {
            Ok(lock) => lock,
            Err(e) => {
                warn!(error = %e, "Failed to get operations lock for completion");
                return;
            }
        };
        let mut operations = operations_lock.write().await;

        if let Some(op) = operations.remove(&key) {
            let duration_ms = op.started_at.elapsed().as_millis() as f64;

            // Emit completion event
            if let Some(ref tx) = self.progress_tx {
                let elapsed = op.started_at.elapsed().as_secs_f64();
                let (operation_type_str, resource_type) = match operation_type {
                    OperationType::Model(ModelOperationType::Load) => ("load", "model"),
                    OperationType::Model(ModelOperationType::Unload) => ("unload", "model"),
                    OperationType::Adapter(AdapterOperationType::Load) => ("load", "adapter"),
                    OperationType::Adapter(AdapterOperationType::Unload) => ("unload", "adapter"),
                };

                let _ = tx.send(OperationProgressEvent {
                    operation_id: format!("{}:{}", resource_id, tenant_id),
                    model_id: resource_id.to_string(),
                    operation: operation_type_str.to_string(),
                    status: if success {
                        "completed".to_string()
                    } else {
                        "failed".to_string()
                    },
                    progress_percent: Some(100),
                    duration_ms: Some(elapsed as u64 * 1000),
                    error_message: if success {
                        None
                    } else {
                        Some(format!("{} operation failed", resource_type))
                    },
                    created_at: Utc::now(),
                });
            }

            #[cfg(feature = "metrics")]
            if let Some(metrics) = &self.metrics {
                let operation_type_str = match operation_type {
                    OperationType::Model(mt) => format!("model_{:?}", mt),
                    OperationType::Model(ModelOperationType::Unload) => format!("model_unload",),
                    OperationType::Adapter(AdapterOperationType::Load) => format!("adapter_load"),
                    OperationType::Adapter(AdapterOperationType::Unload) => {
                        format!("adapter_unload")
                    }
                };
                let _ = metrics.record_histogram(
                    "operation_tracker.operation_duration_ms",
                    duration_ms,
                    &[("operation_type", &operation_type_str)],
                );
                let _ = metrics.record_counter(
                    "operation_tracker.operations_completed",
                    1,
                    &[("operation_type", &operation_type_str)],
                );
            }

            debug!(
                resource_id = %resource_id,
                tenant_id = %tenant_id,
                operation = ?operation_type,
                duration_ms = %duration_ms,
                "Completed operation"
            );
        } else {
            #[cfg(feature = "metrics")]
            if let Some(metrics) = &self.metrics {
                let _ = metrics.record_counter(
                    "operation_tracker.operations_completed_unknown",
                    1,
                    &[("operation_type", &format!("{:?}", operation_type))],
                );
            }

            warn!(
                resource_id = %resource_id,
                tenant_id = %tenant_id,
                operation = ?operation_type,
                "Attempted to complete unknown operation"
            );
        }
    }

    /// Cancel an ongoing model operation
    pub async fn cancel_model_operation(
        &self,
        model_id: &str,
        tenant_id: &str,
    ) -> Result<(), OperationCancellationError> {
        self.cancel_operation(model_id, tenant_id).await
    }

    /// Cancel an ongoing adapter operation
    pub async fn cancel_adapter_operation(
        &self,
        adapter_id: &str,
        tenant_id: &str,
    ) -> Result<(), OperationCancellationError> {
        self.cancel_operation(adapter_id, tenant_id).await
    }

    pub async fn cancel_operation(
        &self,
        resource_id: &str,
        tenant_id: &str,
    ) -> Result<(), OperationCancellationError> {
        let key = (resource_id.to_string(), tenant_id.to_string());
        let operations_lock = self
            .get_operations_write()
            .map_err(|_| OperationCancellationError::OperationNotFound)?;
        let operations = operations_lock.read().await;

        if let Some(op) = operations.get(&key) {
            // Cancel the operation using the cancellation token
            op.cancellation_token.cancel();

            // Emit cancellation event
            if let Some(ref tx) = self.progress_tx {
                let elapsed = op.started_at.elapsed().as_secs_f64();
                let _ = tx.send(OperationProgressEvent {
                    operation_id: format!("{}:{}", resource_id, tenant_id),
                    model_id: resource_id.to_string(),
                    operation: match op.operation_type {
                        OperationType::Model(ModelOperationType::Load) => "load",
                        OperationType::Model(ModelOperationType::Unload) => "unload",
                        OperationType::Adapter(_) => "adapter_operation", // Generic fallback
                    }
                    .to_string(),
                    status: "cancelled".to_string(),
                    progress_percent: Some(op.progress_pct as u8),
                    duration_ms: Some(elapsed as u64 * 1000),
                    error_message: Some("Operation cancelled by user".to_string()),
                    created_at: Utc::now(),
                });
            }

            debug!(
                resource_id = %resource_id,
                tenant_id = %tenant_id,
                operation = ?op.operation_type,
                "Cancelled operation"
            );

            Ok(())
        } else {
            Err(OperationCancellationError::OperationNotFound)
        }
    }

    /// Check whether an operation has been cancelled
    pub async fn is_operation_cancelled(&self, resource_id: &str, tenant_id: &str) -> Option<bool> {
        let key = (resource_id.to_string(), tenant_id.to_string());
        let operations_lock = self.get_operations_read().ok()?;
        let operations = operations_lock.read().await;

        operations
            .get(&key)
            .map(|op| op.cancellation_token.is_cancelled())
    }

    /// Check if a model operation is currently running
    pub async fn is_model_operation_running(
        &self,
        model_id: &str,
        tenant_id: &str,
    ) -> Option<OngoingOperation> {
        self.is_operation_running(model_id, tenant_id).await
    }

    /// Check if an adapter operation is currently running
    pub async fn is_adapter_operation_running(
        &self,
        adapter_id: &str,
        tenant_id: &str,
    ) -> Option<OngoingOperation> {
        self.is_operation_running(adapter_id, tenant_id).await
    }

    async fn is_operation_running(
        &self,
        resource_id: &str,
        tenant_id: &str,
    ) -> Option<OngoingOperation> {
        let operations_lock = self.get_operations_read().ok()?;
        let operations = operations_lock.read().await;
        let key = (resource_id.to_string(), tenant_id.to_string());

        operations.get(&key).cloned()
    }

    /// Get all ongoing operations (for monitoring/debugging)
    pub async fn get_ongoing_operations(&self) -> HashMap<(String, String), OngoingOperation> {
        let operations_lock = match self.get_operations_read() {
            Ok(lock) => lock,
            Err(_) => return HashMap::new(),
        };
        let operations = operations_lock.read().await;
        operations.clone()
    }

    /// Clean up expired operations
    fn cleanup_expired(&self, operations: &mut HashMap<(String, String), OngoingOperation>) {
        let mut to_remove = Vec::new();

        for (key, op) in operations.iter() {
            if op.started_at.elapsed() > self.default_timeout {
                warn!(
                    model_id = %key.0,
                    tenant_id = %key.1,
                    operation = ?op.operation_type,
                    elapsed_secs = %op.started_at.elapsed().as_secs(),
                    "Cleaning up expired operation"
                );
                to_remove.push(key.clone());
            }
        }

        for key in to_remove {
            operations.remove(&key);
        }
    }

    /// Force cleanup of all operations (for testing/emergency use)
    pub async fn force_cleanup(&self) {
        let operations_lock = match self.get_operations_write() {
            Ok(lock) => lock,
            Err(e) => {
                warn!(error = %e, "Failed to get operations lock for force cleanup");
                return;
            }
        };
        let mut operations = operations_lock.write().await;
        let count = operations.len();
        operations.clear();
        warn!("Force cleaned up {} operations", count);
    }

    /// Get the default timeout duration
    pub fn default_timeout(&self) -> Duration {
        self.default_timeout
    }
}

/// Error type for operation cancellation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationCancellationError {
    OperationNotFound,
    OperationAlreadyCompleted,
}

impl std::fmt::Display for OperationCancellationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationCancellationError::OperationNotFound => {
                write!(f, "No ongoing operation found")
            }
            OperationCancellationError::OperationAlreadyCompleted => {
                write!(f, "Operation already completed")
            }
        }
    }
}

impl std::error::Error for OperationCancellationError {}

/// Represents a conflict with an ongoing operation
#[derive(Debug, Clone)]
pub struct OperationConflict {
    pub model_id: String, // Used for both models and adapters for compatibility
    pub tenant_id: String,
    pub conflicting_operation: OperationType,
    pub remaining_time: Duration,
}

impl std::fmt::Display for OperationConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (resource_type, operation_desc) = match self.conflicting_operation {
            OperationType::Model(ModelOperationType::Load) => ("Model", "loading"),
            OperationType::Model(ModelOperationType::Unload) => ("Model", "unloading"),
            OperationType::Adapter(AdapterOperationType::Load) => ("Adapter", "loading"),
            OperationType::Adapter(AdapterOperationType::Unload) => ("Adapter", "unloading"),
        };

        write!(
            f,
            "{} '{}' is currently {} ({} remaining)",
            resource_type,
            self.model_id,
            operation_desc,
            format_duration(self.remaining_time)
        )
    }
}

impl std::error::Error for OperationConflict {}

impl OperationTracker {
    /// Get the status of a specific operation
    pub async fn get_operation_status(
        &self,
        resource_id: &str,
        tenant_id: &str,
    ) -> Option<OperationProgressEvent> {
        let operations_lock = self.get_operations_read().ok()?;
        let operations = operations_lock.read().await;
        let key = (resource_id.to_string(), tenant_id.to_string());

        operations.get(&key).map(|op| {
            let elapsed = op.started_at.elapsed().as_secs_f64();
            let operation_type_str = match op.operation_type {
                OperationType::Model(ModelOperationType::Load) => "load",
                OperationType::Model(ModelOperationType::Unload) => "unload",
                OperationType::Adapter(AdapterOperationType::Load) => "load",
                OperationType::Adapter(AdapterOperationType::Unload) => "unload",
            };

            OperationProgressEvent {
                operation_id: format!("{}:{}", resource_id, tenant_id),
                model_id: resource_id.to_string(),
                operation: operation_type_str.to_string(),
                status: "in_progress".to_string(),
                progress_percent: Some(op.progress_pct as u8),
                duration_ms: Some(elapsed as u64 * 1000),
                error_message: None,
                created_at: Utc::now(),
            }
        })
    }
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, secs % 3600 / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_operation_conflict() {
        let tracker = OperationTracker::new(Duration::from_secs(1));

        // Start load operation
        tracker
            .start_operation(
                "model1",
                "tenant1",
                OperationType::Model(ModelOperationType::Load),
            )
            .await
            .expect("Operation should succeed");

        // Try to start unload operation - should conflict
        let conflict = tracker
            .start_operation(
                "model1",
                "tenant1",
                OperationType::Model(ModelOperationType::Unload),
            )
            .await;
        assert!(conflict.is_err());

        // Try to start same operation again - should conflict
        let conflict = tracker
            .start_operation(
                "model1",
                "tenant1",
                OperationType::Model(ModelOperationType::Load),
            )
            .await;
        assert!(conflict.is_err());

        // Complete operation
        tracker
            .complete_operation(
                "model1",
                "tenant1",
                OperationType::Model(ModelOperationType::Load),
                true,
            )
            .await;

        // Now unload should work
        tracker
            .start_operation(
                "model1",
                "tenant1",
                OperationType::Model(ModelOperationType::Unload),
            )
            .await
            .expect("Operation should succeed");
    }

    #[tokio::test]
    async fn test_operation_timeout() {
        let tracker = OperationTracker::new(Duration::from_millis(50));

        // Start operation
        tracker
            .start_operation(
                "model1",
                "tenant1",
                OperationType::Model(ModelOperationType::Load),
            )
            .await
            .expect("Operation should succeed");

        // Wait for timeout
        sleep(Duration::from_millis(100)).await;

        // Should allow new operation after cleanup
        tracker
            .start_operation(
                "model1",
                "tenant1",
                OperationType::Model(ModelOperationType::Unload),
            )
            .await
            .expect("Operation should succeed");
    }
}
