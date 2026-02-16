//! Operation Deduplication Tracker
//!
//! Prevents concurrent operations on the same model by tracking ongoing operations
//! across all clients and API requests. Uses in-memory storage with proper locking.
//!
//! # Citations
//! - Evidence tracker pattern: [source: crates/adapteros-policy/src/evidence_tracker.rs L172-L187]
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
use redis::{AsyncCommands, Client as RedisClient, Script};
#[cfg(feature = "redis")]
use serde::{Deserialize, Serialize};

type OperationMap = Arc<RwLock<HashMap<(String, String), OngoingOperation>>>;

/// Type of model operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "redis", derive(Serialize, Deserialize))]
pub enum ModelOperationType {
    Load,
    Unload,
}

/// Type of adapter operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "redis", derive(Serialize, Deserialize))]
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
#[cfg_attr(feature = "redis", derive(Serialize, Deserialize))]
pub enum OperationType {
    Model(ModelOperationType),
    Adapter(AdapterOperationType),
}

#[cfg(feature = "redis")]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RedisOperationRecord {
    operation_type: OperationType,
    tenant_id: String,
    progress_pct: f64,
    started_at_ms: i64,
    last_progress_update_ms: i64,
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
    local_operations: OperationMap,
    default_timeout: Duration,
    #[cfg(feature = "metrics")]
    metrics: Option<Arc<MetricsCollector>>,
    /// Broadcast channel for progress updates
    progress_tx: Option<broadcast::Sender<OperationProgressEvent>>,
}

impl OperationTracker {
    /// Create a new in-memory operation tracker
    pub fn new(default_timeout: Duration) -> Self {
        let operations = Arc::new(RwLock::new(HashMap::new()));
        Self {
            storage: OperationStorage::InMemory(operations.clone()),
            local_operations: operations,
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
        let operations = Arc::new(RwLock::new(HashMap::new()));
        Self {
            storage: OperationStorage::InMemory(operations.clone()),
            local_operations: operations,
            default_timeout,
            #[cfg(feature = "metrics")]
            metrics: None,
            progress_tx: Some(progress_tx),
        }
    }

    /// Create a new in-memory operation tracker with metrics
    #[cfg(feature = "metrics")]
    pub fn new_with_metrics(default_timeout: Duration, metrics: Arc<MetricsCollector>) -> Self {
        let operations = Arc::new(RwLock::new(HashMap::new()));
        Self {
            storage: OperationStorage::InMemory(operations.clone()),
            local_operations: operations,
            default_timeout,
            metrics: Some(metrics),
            progress_tx: None,
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
        let local_operations = Arc::new(RwLock::new(HashMap::new()));
        Ok(Self {
            storage: OperationStorage::Redis {
                client,
                key_prefix: key_prefix.to_string(),
                fallback_to_memory: false,
            },
            local_operations,
            default_timeout,
            #[cfg(feature = "metrics")]
            metrics: None,
            progress_tx: None,
        })
    }

    /// Get a reference to the in-memory operations HashMap for read access
    fn get_operations_read(&self) -> Result<OperationMap, AosError> {
        Ok(self.local_operations.clone())
    }

    /// Get a reference to the in-memory operations HashMap for write access
    fn get_operations_write(&self) -> Result<OperationMap, AosError> {
        Ok(self.local_operations.clone())
    }

    #[cfg(feature = "redis")]
    fn redis_ttl_secs(&self) -> u64 {
        self.default_timeout.as_secs().max(1)
    }

    #[cfg(feature = "redis")]
    fn elapsed_ms_from_epoch(started_at_ms: i64) -> u64 {
        let now_ms = Utc::now().timestamp_millis();
        now_ms.saturating_sub(started_at_ms).max(0) as u64
    }

    #[cfg(feature = "redis")]
    fn redis_record_to_ongoing(record: RedisOperationRecord) -> OngoingOperation {
        let now_instant = Instant::now();
        let started_at = now_instant
            .checked_sub(Duration::from_millis(Self::elapsed_ms_from_epoch(
                record.started_at_ms,
            )))
            .unwrap_or(now_instant);
        let last_progress_update = now_instant
            .checked_sub(Duration::from_millis(Self::elapsed_ms_from_epoch(
                record.last_progress_update_ms,
            )))
            .unwrap_or(now_instant);

        OngoingOperation {
            operation_type: record.operation_type,
            started_at,
            tenant_id: record.tenant_id,
            progress_pct: record.progress_pct.clamp(0.0, 100.0),
            last_progress_update,
            cancellation_token: Arc::new(CancellationToken::new()),
        }
    }

    #[cfg(feature = "redis")]
    fn redis_operation_key(&self, resource_id: &str, tenant_id: &str) -> Result<String, AosError> {
        match &self.storage {
            OperationStorage::Redis { key_prefix, .. } => Ok(format!(
                "{}:operations:{}:{}",
                key_prefix, tenant_id, resource_id
            )),
            _ => Err(AosError::Config(
                "Redis operation tracking is not configured".to_string(),
            )),
        }
    }

    #[cfg(feature = "redis")]
    fn redis_operation_scan_pattern(&self) -> Result<String, AosError> {
        match &self.storage {
            OperationStorage::Redis { key_prefix, .. } => {
                Ok(format!("{}:operations:*", key_prefix))
            }
            _ => Err(AosError::Config(
                "Redis operation tracking is not configured".to_string(),
            )),
        }
    }

    #[cfg(feature = "redis")]
    fn map_redis_error(error: redis::RedisError) -> AosError {
        AosError::Network(format!("Operation tracker Redis error: {error}"))
    }

    #[cfg(feature = "redis")]
    async fn redis_connection(&self) -> Result<redis::aio::MultiplexedConnection, AosError> {
        match &self.storage {
            OperationStorage::Redis { client, .. } => client
                .get_multiplexed_async_connection()
                .await
                .map_err(Self::map_redis_error),
            _ => Err(AosError::Config(
                "Redis operation tracking is not configured".to_string(),
            )),
        }
    }

    #[cfg(feature = "redis")]
    fn parse_redis_record(raw: &str) -> Result<RedisOperationRecord, AosError> {
        serde_json::from_str(raw).map_err(Into::into)
    }

    #[cfg(feature = "redis")]
    async fn start_operation_redis(
        &self,
        resource_id: &str,
        tenant_id: &str,
        operation_type: OperationType,
    ) -> Result<(), AosError> {
        let key = self.redis_operation_key(resource_id, tenant_id)?;
        let now_ms = Utc::now().timestamp_millis();
        let record = RedisOperationRecord {
            operation_type,
            tenant_id: tenant_id.to_string(),
            progress_pct: 0.0,
            started_at_ms: now_ms,
            last_progress_update_ms: now_ms,
        };
        let payload = serde_json::to_string(&record)?;
        let mut conn = self.redis_connection().await?;

        let set_result: Option<String> = redis::cmd("SET")
            .arg(&key)
            .arg(payload)
            .arg("NX")
            .arg("EX")
            .arg(self.redis_ttl_secs())
            .query_async(&mut conn)
            .await
            .map_err(Self::map_redis_error)?;

        if set_result.is_some() {
            return Ok(());
        }

        let existing_record = match conn.get::<_, Option<String>>(&key).await {
            Ok(Some(raw)) => Self::parse_redis_record(&raw).ok(),
            Ok(None) => None,
            Err(_) => None,
        };
        let remaining_ttl_secs = conn.ttl::<_, i64>(&key).await.unwrap_or_default().max(0) as u64;
        let conflicting_operation = existing_record
            .as_ref()
            .map(|record| record.operation_type)
            .unwrap_or(operation_type);
        warn!(
            resource_id = %resource_id,
            tenant_id = %tenant_id,
            conflicting_operation = ?conflicting_operation,
            remaining_time_secs = %remaining_ttl_secs,
            "Operation conflict detected"
        );
        Err(AosError::Validation(format!(
            "Operation conflict: existing {:?} for resource {}, tenant {}",
            conflicting_operation, resource_id, tenant_id
        )))
    }

    #[cfg(feature = "redis")]
    async fn update_progress_redis(
        &self,
        resource_id: &str,
        tenant_id: &str,
        progress_pct: f64,
    ) -> Result<Option<RedisOperationRecord>, AosError> {
        let key = self.redis_operation_key(resource_id, tenant_id)?;
        let mut conn = self.redis_connection().await?;
        let now_ms = Utc::now().timestamp_millis();
        let script = Script::new(
            r#"
            local key = KEYS[1]
            local payload = redis.call('GET', key)
            if not payload then
                return nil
            end
            local record = cjson.decode(payload)
            record.progress_pct = tonumber(ARGV[1])
            record.last_progress_update_ms = tonumber(ARGV[2])
            local encoded = cjson.encode(record)
            redis.call('SET', key, encoded, 'XX', 'KEEPTTL')
            return encoded
            "#,
        );

        let updated: Option<String> = script
            .key(&key)
            .arg(progress_pct.clamp(0.0, 100.0))
            .arg(now_ms)
            .invoke_async(&mut conn)
            .await
            .map_err(Self::map_redis_error)?;

        match updated {
            Some(raw) => Ok(Some(Self::parse_redis_record(&raw)?)),
            None => Ok(None),
        }
    }

    #[cfg(feature = "redis")]
    async fn complete_operation_redis(
        &self,
        resource_id: &str,
        tenant_id: &str,
    ) -> Result<Option<RedisOperationRecord>, AosError> {
        let key = self.redis_operation_key(resource_id, tenant_id)?;
        let mut conn = self.redis_connection().await?;
        let script = Script::new(
            r#"
            local key = KEYS[1]
            local payload = redis.call('GET', key)
            if payload then
                redis.call('DEL', key)
            end
            return payload
            "#,
        );
        let removed: Option<String> = script
            .key(&key)
            .invoke_async(&mut conn)
            .await
            .map_err(Self::map_redis_error)?;

        match removed {
            Some(raw) => Ok(Some(Self::parse_redis_record(&raw)?)),
            None => Ok(None),
        }
    }

    #[cfg(feature = "redis")]
    async fn get_operation_status_redis(
        &self,
        resource_id: &str,
        tenant_id: &str,
    ) -> Result<Option<RedisOperationRecord>, AosError> {
        let key = self.redis_operation_key(resource_id, tenant_id)?;
        let mut conn = self.redis_connection().await?;
        let payload: Option<String> = conn.get(&key).await.map_err(Self::map_redis_error)?;
        match payload {
            Some(raw) => Ok(Some(Self::parse_redis_record(&raw)?)),
            None => Ok(None),
        }
    }

    #[cfg(feature = "redis")]
    async fn force_cleanup_redis(&self) -> Result<usize, AosError> {
        let mut conn = self.redis_connection().await?;
        let pattern = self.redis_operation_scan_pattern()?;
        let mut keys = Vec::new();
        {
            let mut iter: redis::AsyncIter<String> = conn
                .scan_match(pattern)
                .await
                .map_err(Self::map_redis_error)?;
            while let Some(key) = iter.next_item().await {
                keys.push(key);
            }
        }

        if keys.is_empty() {
            return Ok(0);
        }

        conn.del(keys).await.map_err(Self::map_redis_error)
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
        let now = Instant::now();
        let operation = OngoingOperation {
            operation_type,
            started_at: now,
            tenant_id: tenant_id.to_string(),
            progress_pct: 0.0,
            last_progress_update: now,
            cancellation_token: Arc::new(CancellationToken::new()),
        };

        match &self.storage {
            OperationStorage::InMemory(_) => {
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

                operations.insert(key.clone(), operation.clone());
            }
            #[cfg(feature = "redis")]
            OperationStorage::Redis { .. } => {
                self.start_operation_redis(resource_id, tenant_id, operation_type)
                    .await?;
                let operations_lock = self.get_operations_write()?;
                let mut operations = operations_lock.write().await;
                self.cleanup_expired(&mut operations);
                operations.insert(key.clone(), operation.clone());
            }
        }

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
        let clamped_progress = progress_pct.clamp(0.0, 100.0);
        match &self.storage {
            OperationStorage::InMemory(_) => {
                let operations_lock = match self.get_operations_write() {
                    Ok(lock) => lock,
                    Err(e) => {
                        warn!(error = %e, "Failed to get operations lock for progress update");
                        return;
                    }
                };
                let mut operations = operations_lock.write().await;

                if let Some(op) = operations.get_mut(&key) {
                    op.progress_pct = clamped_progress;
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
            #[cfg(feature = "redis")]
            OperationStorage::Redis { .. } => {
                let redis_record = match self
                    .update_progress_redis(resource_id, tenant_id, clamped_progress)
                    .await
                {
                    Ok(record) => record,
                    Err(e) => {
                        warn!(error = %e, "Failed to update Redis operation progress");
                        None
                    }
                };

                let operations_lock = match self.get_operations_write() {
                    Ok(lock) => lock,
                    Err(e) => {
                        warn!(error = %e, "Failed to get operations lock for progress update");
                        return;
                    }
                };
                let mut operations = operations_lock.write().await;
                if let Some(op) = operations.get_mut(&key) {
                    op.progress_pct = clamped_progress;
                    op.last_progress_update = Instant::now();
                }

                if let (Some(tx), Some(record)) = (&self.progress_tx, redis_record) {
                    let elapsed_ms = Self::elapsed_ms_from_epoch(record.started_at_ms);
                    let operation_type_str = match record.operation_type {
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
                        progress_percent: Some(record.progress_pct.clamp(0.0, 100.0) as u8),
                        duration_ms: Some(elapsed_ms),
                        error_message: None,
                        created_at: Utc::now(),
                    });
                }
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
        #[cfg(feature = "redis")]
        let redis_completion: Option<(u64, OperationType)> = match &self.storage {
            OperationStorage::InMemory(_) => None,
            OperationStorage::Redis { .. } => {
                match self.complete_operation_redis(resource_id, tenant_id).await {
                    Ok(Some(record)) => Some((
                        Self::elapsed_ms_from_epoch(record.started_at_ms),
                        record.operation_type,
                    )),
                    Ok(None) => None,
                    Err(e) => {
                        warn!(error = %e, "Failed to complete operation in Redis");
                        None
                    }
                }
            }
        };
        #[cfg(not(feature = "redis"))]
        let redis_completion: Option<(u64, OperationType)> = None;

        let mut operations = operations_lock.write().await;
        let local_op = operations.remove(&key);
        drop(operations);

        if local_op.is_some() || redis_completion.is_some() {
            let duration_ms = local_op
                .as_ref()
                .map(|op| op.started_at.elapsed().as_millis() as f64)
                .unwrap_or_else(|| {
                    redis_completion
                        .map(|(elapsed_ms, _)| elapsed_ms as f64)
                        .unwrap_or(0.0)
                });

            // Emit completion event
            if let Some(ref tx) = self.progress_tx {
                let elapsed_ms = local_op
                    .as_ref()
                    .map(|op| op.started_at.elapsed().as_millis() as u64)
                    .unwrap_or_else(|| {
                        redis_completion
                            .map(|(elapsed_ms, _)| elapsed_ms)
                            .unwrap_or(0)
                    });
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
                    duration_ms: Some(elapsed_ms),
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
        let key = (resource_id.to_string(), tenant_id.to_string());
        let operations_lock = self.get_operations_read().ok()?;
        let operations = operations_lock.read().await;
        if let Some(operation) = operations.get(&key) {
            return Some(operation.clone());
        }
        drop(operations);

        match &self.storage {
            OperationStorage::InMemory(_) => None,
            #[cfg(feature = "redis")]
            OperationStorage::Redis { .. } => {
                match self
                    .get_operation_status_redis(resource_id, tenant_id)
                    .await
                {
                    Ok(Some(record)) => Some(Self::redis_record_to_ongoing(record)),
                    Ok(None) => None,
                    Err(e) => {
                        warn!(error = %e, "Failed to query Redis operation status");
                        None
                    }
                }
            }
        }
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
        #[cfg(feature = "redis")]
        let mut redis_count = 0usize;
        #[cfg(feature = "redis")]
        if matches!(self.storage, OperationStorage::Redis { .. }) {
            match self.force_cleanup_redis().await {
                Ok(count) => redis_count = count,
                Err(e) => {
                    warn!(error = %e, "Failed to force cleanup Redis operations");
                }
            }
        }

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
        #[cfg(feature = "redis")]
        warn!(
            "Force cleaned up {} local operations and {} Redis operations",
            count, redis_count
        );
        #[cfg(not(feature = "redis"))]
        warn!("Force cleaned up {} operations", count);
    }

    /// Get the default timeout duration
    pub fn default_timeout(&self) -> Duration {
        self.default_timeout
    }
}

/// Error type for operation cancellation
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OperationCancellationError {
    #[error("No ongoing operation found")]
    OperationNotFound,
    #[error("Operation already completed")]
    OperationAlreadyCompleted,
}

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

        if let Some(op) = operations.get(&key) {
            let elapsed = op.started_at.elapsed().as_secs_f64();
            let operation_type_str = match op.operation_type {
                OperationType::Model(ModelOperationType::Load) => "load",
                OperationType::Model(ModelOperationType::Unload) => "unload",
                OperationType::Adapter(AdapterOperationType::Load) => "load",
                OperationType::Adapter(AdapterOperationType::Unload) => "unload",
            };

            return Some(OperationProgressEvent {
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
        drop(operations);

        match &self.storage {
            OperationStorage::InMemory(_) => None,
            #[cfg(feature = "redis")]
            OperationStorage::Redis { .. } => match self
                .get_operation_status_redis(resource_id, tenant_id)
                .await
            {
                Ok(Some(record)) => {
                    let elapsed_ms = Self::elapsed_ms_from_epoch(record.started_at_ms);
                    let operation_type_str = match record.operation_type {
                        OperationType::Model(ModelOperationType::Load) => "load",
                        OperationType::Model(ModelOperationType::Unload) => "unload",
                        OperationType::Adapter(AdapterOperationType::Load) => "load",
                        OperationType::Adapter(AdapterOperationType::Unload) => "unload",
                    };

                    Some(OperationProgressEvent {
                        operation_id: format!("{}:{}", resource_id, tenant_id),
                        model_id: resource_id.to_string(),
                        operation: operation_type_str.to_string(),
                        status: "in_progress".to_string(),
                        progress_percent: Some(record.progress_pct.clamp(0.0, 100.0) as u8),
                        duration_ms: Some(elapsed_ms),
                        error_message: None,
                        created_at: Utc::now(),
                    })
                }
                Ok(None) => None,
                Err(e) => {
                    warn!(error = %e, "Failed to query Redis operation status");
                    None
                }
            },
        }
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
