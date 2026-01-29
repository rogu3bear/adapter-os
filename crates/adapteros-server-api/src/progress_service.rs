//! Unified Progress Service for adapterOS
//!
//! Centralizes progress tracking across all operations (adapter loading, training,
//! background tasks, etc.) with standardized event emission, filtering, and persistence.

use crate::types::{OperationProgressEvent, ProgressEvent};
// ProgressStatus is now defined locally in this module
use adapteros_core::{AosError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Unified progress event types across all operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProgressEventType {
    /// Adapter/Model operations (load, unload, etc.)
    Operation(String),
    /// Training job progress
    Training(String),
    /// Background tasks and maintenance
    Background(String),
    /// Custom application-specific progress
    Custom(String),
}

impl Default for ProgressEventType {
    fn default() -> Self {
        ProgressEventType::Custom("unknown".to_string())
    }
}

/// Progress operation status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProgressStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for ProgressStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProgressStatus::Running => write!(f, "running"),
            ProgressStatus::Completed => write!(f, "completed"),
            ProgressStatus::Failed => write!(f, "failed"),
            ProgressStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Progress operation metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressOperation {
    pub operation_id: String,
    pub tenant_id: String,
    pub event_type: ProgressEventType,
    pub started_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
    pub progress_pct: f64,
    pub status: ProgressStatus,
    pub message: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Progress filter for querying operations
#[derive(Debug, Clone, Deserialize)]
#[derive(Default)]
pub struct ProgressFilter {
    pub tenant_id: Option<String>,
    pub event_type: Option<String>,
    pub operation_id: Option<String>,
    pub status: Option<ProgressStatus>,
    pub min_progress: Option<f64>,
    pub max_progress: Option<f64>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

/// Progress storage backend
#[derive(Debug)]
pub enum ProgressStorage {
    /// In-memory storage for development/testing
    Memory(HashMap<String, ProgressOperation>),
    /// Database-backed storage for production
    Database(Arc<adapteros_db::Db>),
}

/// Configuration for progress service
#[derive(Debug, Clone)]
pub struct ProgressConfig {
    pub max_operations: usize,
    pub retention_hours: i64,
    pub cleanup_interval_secs: u64,
    pub enable_metrics: bool,
}

impl Default for ProgressConfig {
    fn default() -> Self {
        Self {
            max_operations: 1000,
            retention_hours: 24,
            cleanup_interval_secs: 300, // 5 minutes
            enable_metrics: true,
        }
    }
}

/// Centralized progress tracking service
#[derive(Debug)]
pub struct ProgressService {
    storage: Arc<RwLock<ProgressStorage>>,
    event_tx: broadcast::Sender<ProgressEvent>,
    config: ProgressConfig,
    metrics: Option<ProgressMetrics>,
}

/// Progress tracking metrics
#[derive(Debug)]
pub struct ProgressMetrics {
    /// Total operations started
    operations_started_total: prometheus::CounterVec,
    /// Total operations completed
    operations_completed_total: prometheus::CounterVec,
    /// Total operations failed
    operations_failed_total: prometheus::CounterVec,
    /// Total operations cancelled
    operations_cancelled_total: prometheus::CounterVec,
    /// Current active operations
    active_operations: prometheus::GaugeVec,
    /// Operation duration histogram
    operation_duration_seconds: prometheus::HistogramVec,
    /// Database operation latency
    db_operation_duration_seconds: prometheus::HistogramVec,
    /// Query duration histogram
    query_duration_seconds: prometheus::HistogramVec,
    /// Event emission rate
    events_emitted_total: prometheus::CounterVec,
    /// SSE subscriber count
    sse_subscribers: prometheus::Gauge,
}

impl ProgressService {
    /// Create a new progress service with in-memory storage
    pub fn new() -> Self {
        Self::with_config(ProgressConfig::default())
    }

    /// Create a new progress service with database storage
    pub fn with_database(db: Arc<adapteros_db::Db>) -> Self {
        Self::with_database_and_config(db, ProgressConfig::default())
    }

    /// Create a new progress service with custom configuration
    pub fn with_config(config: ProgressConfig) -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        let storage = Arc::new(RwLock::new(ProgressStorage::Memory(HashMap::new())));
        let metrics = if config.enable_metrics {
            Some(Self::create_metrics())
        } else {
            None
        };

        Self {
            storage,
            event_tx,
            config,
            metrics,
        }
    }

    /// Create a new progress service with database storage and custom configuration
    pub fn with_database_and_config(db: Arc<adapteros_db::Db>, config: ProgressConfig) -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        let storage = Arc::new(RwLock::new(ProgressStorage::Database(db)));
        let metrics = if config.enable_metrics {
            Some(Self::create_metrics())
        } else {
            None
        };

        Self {
            storage,
            event_tx,
            config,
            metrics,
        }
    }

    /// Create Prometheus metrics for progress tracking
    fn create_metrics() -> ProgressMetrics {
        use prometheus::{CounterVec, GaugeVec, HistogramVec, Opts};

        let operations_started_total = CounterVec::new(
            Opts::new("progress_operations_started_total", "Total number of operations started")
                .namespace("adapteros"),
            &["event_type", "tenant_id"],
        ).expect("Failed to create Prometheus metric");

        let operations_completed_total = CounterVec::new(
            Opts::new("progress_operations_completed_total", "Total number of operations completed")
                .namespace("adapteros"),
            &["event_type", "tenant_id"],
        ).expect("Failed to create Prometheus metric");

        let operations_failed_total = CounterVec::new(
            Opts::new("progress_operations_failed_total", "Total number of operations failed")
                .namespace("adapteros"),
            &["event_type", "tenant_id"],
        ).expect("Failed to create Prometheus metric");

        let operations_cancelled_total = CounterVec::new(
            Opts::new("progress_operations_cancelled_total", "Total number of operations cancelled")
                .namespace("adapteros"),
            &["event_type", "tenant_id"],
        ).expect("Failed to create Prometheus metric");

        let active_operations = GaugeVec::new(
            Opts::new("progress_active_operations", "Number of currently active operations")
                .namespace("adapteros"),
            &["event_type", "tenant_id"],
        ).expect("Failed to create Prometheus metric");

        let operation_duration_seconds = HistogramVec::new(
            prometheus::HistogramOpts::new(
                "progress_operation_duration_seconds",
                "Duration of completed operations in seconds"
            )
            .namespace("adapteros")
            .buckets(vec![1.0, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0]),
            &["event_type", "status"],
        ).expect("Failed to create Prometheus metric");

        let db_operation_duration_seconds = HistogramVec::new(
            prometheus::HistogramOpts::new(
                "progress_db_operation_duration_seconds",
                "Duration of database operations in seconds"
            )
            .namespace("adapteros")
            .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]),
            &["operation"],
        ).expect("Failed to create Prometheus metric");

        let query_duration_seconds = HistogramVec::new(
            prometheus::HistogramOpts::new(
                "progress_query_duration_seconds",
                "Duration of progress queries in seconds"
            )
            .namespace("adapteros")
            .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0]),
            &[],
        ).expect("Failed to create Prometheus metric");

        let events_emitted_total = CounterVec::new(
            Opts::new("progress_events_emitted_total", "Total number of progress events emitted")
                .namespace("adapteros"),
            &["event_type"],
        ).expect("Failed to create Prometheus metric");

        let sse_subscribers = prometheus::Gauge::new(
            "progress_sse_subscribers",
            "Number of active SSE subscribers"
        ).expect("Failed to create Prometheus metric");

        ProgressMetrics {
            operations_started_total,
            operations_completed_total,
            operations_failed_total,
            operations_cancelled_total,
            active_operations,
            operation_duration_seconds,
            db_operation_duration_seconds,
            query_duration_seconds,
            events_emitted_total,
            sse_subscribers,
        }
    }

    /// Start tracking a new operation
    pub async fn start_operation(
        &self,
        operation_id: impl Into<String>,
        tenant_id: impl Into<String>,
        event_type: ProgressEventType,
    ) -> Result<()> {
        let operation_id = operation_id.into();
        let tenant_id = tenant_id.into();
        let now = Utc::now();

        let operation = ProgressOperation {
            operation_id: operation_id.clone(),
            tenant_id: tenant_id.clone(),
            event_type: event_type.clone(),
            started_at: now,
            last_updated: now,
            progress_pct: 0.0,
            status: ProgressStatus::Running,
            message: Some("Operation started".to_string()),
            metadata: None,
        };

        // Store operation
        {
            let mut storage = self.storage.write().await;
            match *storage {
                ProgressStorage::Memory(ref mut ops) => {
                    ops.insert(operation_id.clone(), operation);
                }
                ProgressStorage::Database(ref db) => {
                    let event_type_str = match &operation.event_type {
                        ProgressEventType::Operation(s) => format!("operation:{}", s),
                        ProgressEventType::Training(s) => format!("training:{}", s),
                        ProgressEventType::Background(s) => format!("background:{}", s),
                        ProgressEventType::Custom(s) => format!("custom:{}", s),
                    };

                    let metadata = operation.metadata.as_ref().map(|v| v.to_string());

                    // Create new progress event record (this is correct for starting an operation)
                    db.create_progress_event(
                        &operation.operation_id,
                        &operation.tenant_id,
                        &event_type_str,
                        operation.progress_pct,
                        &operation.status.to_string().to_lowercase(),
                        operation.message.as_deref(),
                        metadata.as_deref(),
                    )
                    .await?;
                }
            }
        }

        // Record metrics
        let event_type_str = Self::event_type_string(&event_type);
        self.record_operation_started(event_type_str, &tenant_id);

        // Emit progress event
        self.emit_progress_event(operation_id.clone(), tenant_id.clone(), event_type.clone(), 0.0, ProgressStatus::Running, Some("Operation started".to_string()))
            .await?;

        info!(
            operation_id = %operation_id,
            tenant_id = %tenant_id,
            event_type = ?event_type,
            "Started progress tracking"
        );

        Ok(())
    }

    /// Update progress for an operation
    pub async fn update_progress(
        &self,
        operation_id: impl Into<String>,
        progress_pct: f64,
        message: Option<String>,
    ) -> Result<()> {
        let operation_id = operation_id.into();
        let progress_pct = progress_pct.clamp(0.0, 100.0);
        let now = Utc::now();

        // Update stored operation
        let (tenant_id, event_type) = {
            let mut storage = self.storage.write().await;
            match *storage {
                ProgressStorage::Memory(ref mut ops) => {
                    if let Some(op) = ops.get_mut(&operation_id) {
                        op.last_updated = now;
                        op.progress_pct = progress_pct;
                        op.message = message.clone();
                        op.metadata = Some(serde_json::json!({
                            "updated_at": now.to_rfc3339(),
                            "progress_pct": progress_pct
                        }));
                        (op.tenant_id.clone(), op.event_type.clone())
                    } else {
                        return Err(AosError::NotFound(format!("Operation not found: {}", operation_id)));
                    }
                }
                ProgressStorage::Database(ref db) => {
                    let query = adapteros_db::progress_events::ProgressEventQuery {
                        operation_id: Some(operation_id.clone()),
                        limit: Some(1),
                        ..Default::default()
                    };
                    
                    let records = db.get_progress_events(query).await?;
                    let record = records.first().ok_or_else(|| {
                        AosError::NotFound(format!("Operation not found: {}", operation_id))
                    })?;

                    db.update_progress_event(
                        &operation_id,
                        progress_pct,
                        "running",
                        message.as_deref(),
                    )
                    .await?;

                    let event_type = if record.event_type.starts_with("operation:") {
                        ProgressEventType::Operation(record.event_type[10..].to_string())
                    } else if record.event_type.starts_with("training:") {
                        ProgressEventType::Training(record.event_type[9..].to_string())
                    } else if record.event_type.starts_with("background:") {
                        ProgressEventType::Background(record.event_type[11..].to_string())
                    } else if record.event_type.starts_with("custom:") {
                        ProgressEventType::Custom(record.event_type[7..].to_string())
                    } else {
                        ProgressEventType::Custom(record.event_type.clone())
                    };
                    (record.tenant_id.clone(), event_type)
                }
            }
        };

        // Emit progress event
        self.emit_progress_event(operation_id, tenant_id, event_type, progress_pct, ProgressStatus::Running, message)
            .await?;

        Ok(())
    }

    /// Complete an operation
    pub async fn complete_operation(
        &self,
        operation_id: impl Into<String>,
        success: bool,
    ) -> Result<()> {
        let operation_id = operation_id.into();
        let final_status = if success { ProgressStatus::Completed } else { ProgressStatus::Failed };
        let message = if success { "Operation completed successfully" } else { "Operation failed" };

        // Update stored operation
        let (tenant_id, event_type, duration_secs) = {
            let mut storage = self.storage.write().await;
            match *storage {
                ProgressStorage::Memory(ref mut ops) => {
                    if let Some(op) = ops.get_mut(&operation_id) {
                        let duration_secs = (Utc::now() - op.started_at).num_milliseconds() as f64 / 1000.0;
                        op.last_updated = Utc::now();
                        op.status = final_status.clone();
                        op.message = Some(message.to_string());
                        (op.tenant_id.clone(), op.event_type.clone(), duration_secs)
                    } else {
                        return Err(AosError::NotFound(format!("Operation not found: {}", operation_id)));
                    }
                }
                ProgressStorage::Database(ref db) => {
                    let query = adapteros_db::progress_events::ProgressEventQuery {
                        operation_id: Some(operation_id.clone()),
                        limit: Some(1),
                        ..Default::default()
                    };
                    let records = db.get_progress_events(query).await?;
                    let record = records.first().ok_or_else(|| {
                        AosError::NotFound(format!("Operation not found: {}", operation_id))
                    })?;

                    let final_status_str = final_status.to_string();
                    db.update_progress_event(
                        &operation_id,
                        100.0,
                        final_status_str.as_str(),
                        Some(message),
                    )
                    .await?;

                    let event_type = if record.event_type.starts_with("operation:") {
                        ProgressEventType::Operation(record.event_type[10..].to_string())
                    } else if record.event_type.starts_with("training:") {
                        ProgressEventType::Training(record.event_type[9..].to_string())
                    } else if record.event_type.starts_with("background:") {
                        ProgressEventType::Background(record.event_type[11..].to_string())
                    } else if record.event_type.starts_with("custom:") {
                        ProgressEventType::Custom(record.event_type[7..].to_string())
                    } else {
                        ProgressEventType::Custom(record.event_type.clone())
                    };

                    let created_at = record.created_at_datetime()?;
                    let duration_secs = (Utc::now() - created_at).num_milliseconds() as f64 / 1000.0;
                    (record.tenant_id.clone(), event_type, duration_secs)
                }
            }
        };

        // Record metrics
        let event_type_str = Self::event_type_string(&event_type);
        if success {
            self.record_operation_completed(event_type_str, &tenant_id, duration_secs);
        } else {
            self.record_operation_failed(event_type_str, &tenant_id, duration_secs);
        }

        // Emit final progress event
        self.emit_progress_event(operation_id.clone(), tenant_id, event_type, 100.0, final_status, Some(message.to_string()))
            .await?;

        // Remove from active tracking after a delay (keep for history)
        let storage_clone = Arc::clone(&self.storage);
        let operation_id_clone = operation_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(300)).await; // Keep for 5 minutes
            let mut storage = storage_clone.write().await;
            if let ProgressStorage::Memory(ref mut ops) = *storage {
                ops.remove(&operation_id_clone);
                debug!("Cleaned up completed operation: {}", operation_id_clone);
            }
        });

        info!(
            operation_id = %operation_id,
            success = success,
            "Completed progress tracking"
        );

        Ok(())
    }

    /// Get current progress for an operation
    pub async fn get_progress(&self, operation_id: impl Into<String>) -> Result<Option<ProgressOperation>> {
        let operation_id = operation_id.into();
        let storage = self.storage.read().await;

        match *storage {
            ProgressStorage::Memory(ref ops) => Ok(ops.get(&operation_id).cloned()),
            ProgressStorage::Database(ref db) => {
                // Try database first
                let query = adapteros_db::progress_events::ProgressEventQuery {
                    operation_id: Some(operation_id.clone()),
                    limit: Some(1),
                    ..Default::default()
                };
                match db.get_progress_events(query).await {
                    Ok(mut records) if !records.is_empty() => {
                        let record = records.remove(0);
                        // Convert database record to ProgressOperation
                        let event_type = if record.event_type.starts_with("operation:") {
                            ProgressEventType::Operation(record.event_type[10..].to_string())
                        } else if record.event_type.starts_with("training:") {
                            ProgressEventType::Training(record.event_type[9..].to_string())
                        } else if record.event_type.starts_with("background:") {
                            ProgressEventType::Background(record.event_type[11..].to_string())
                        } else if record.event_type.starts_with("custom:") {
                            ProgressEventType::Custom(record.event_type[7..].to_string())
                        } else {
                            ProgressEventType::Custom(record.event_type)
                        };

                        let status = match record.status.as_str() {
                            "running" => ProgressStatus::Running,
                            "completed" => ProgressStatus::Completed,
                            "failed" => ProgressStatus::Failed,
                            "cancelled" => ProgressStatus::Cancelled,
                            _ => ProgressStatus::Running,
                        };

                        let started_at = record.created_at_datetime()?;
                        let last_updated = record.updated_at_datetime()?;

                        let operation = ProgressOperation {
                            operation_id: record.operation_id,
                            tenant_id: record.tenant_id,
                            event_type,
                            started_at,
                            last_updated,
                            progress_pct: record.progress_pct,
                            status,
                            message: record.message,
                            metadata: record.metadata.and_then(|s| serde_json::from_str(&s).ok()),
                        };

                        Ok(Some(operation))
                    }
                    Ok(_) => Ok(None),
                    Err(e) => {
                        warn!("Failed to get progress from database: {}", e);
                        Ok(None)
                    }
                }
            }
        }
    }

    /// Query operations with filtering
    pub async fn query_operations(&self, filter: ProgressFilter) -> Result<Vec<ProgressOperation>> {
        let storage = self.storage.read().await;

        match *storage {
            ProgressStorage::Memory(ref ops) => {
                let mut results: Vec<ProgressOperation> = ops.values().cloned().collect();

                // Apply filters
                results.retain(|op| {
                    if let Some(ref tenant_id) = filter.tenant_id {
                        if op.tenant_id != *tenant_id {
                            return false;
                        }
                    }
                    if let Some(ref event_type_filter) = filter.event_type {
                        let op_event_type = match &op.event_type {
                            ProgressEventType::Operation(s) => format!("operation:{}", s),
                            ProgressEventType::Training(s) => format!("training:{}", s),
                            ProgressEventType::Background(s) => format!("background:{}", s),
                            ProgressEventType::Custom(s) => format!("custom:{}", s),
                        };
                        if op_event_type != *event_type_filter {
                            return false;
                        }
                    }
                    if let Some(ref operation_id_filter) = filter.operation_id {
                        if op.operation_id != *operation_id_filter {
                            return false;
                        }
                    }
                    if let Some(ref status_filter) = filter.status {
                        if op.status != *status_filter {
                            return false;
                        }
                    }
                    if let Some(min_progress) = filter.min_progress {
                        if op.progress_pct < min_progress {
                            return false;
                        }
                    }
                    if let Some(max_progress) = filter.max_progress {
                        if op.progress_pct > max_progress {
                            return false;
                        }
                    }
                    if let Some(since) = filter.since {
                        if op.started_at < since {
                            return false;
                        }
                    }
                    if let Some(until) = filter.until {
                        if op.started_at > until {
                            return false;
                        }
                    }
                    true
                });

                Ok(results)
            }
            ProgressStorage::Database(ref db) => {
                // Query database
                let query = adapteros_db::progress_events::ProgressEventQuery {
                    tenant_id: filter.tenant_id,
                    operation_id: filter.operation_id,
                    event_type: filter.event_type,
                    status: filter.status.as_ref().map(|s| s.to_string().to_lowercase()),
                    min_progress: filter.min_progress,
                    max_progress: filter.max_progress,
                    since: filter.since,
                    until: filter.until,
                    limit: Some(1000), // Default limit for API queries
                    offset: None,
                };

                match db.get_progress_events(query.clone()).await {
                    Ok(records) => {
                        // Convert database records to ProgressOperation
                        let operations: Vec<ProgressOperation> = records
                            .into_iter()
                            .map(|record| {
                                let event_type = if record.event_type.starts_with("operation:") {
                                    ProgressEventType::Operation(record.event_type[10..].to_string())
                                } else if record.event_type.starts_with("training:") {
                                    ProgressEventType::Training(record.event_type[9..].to_string())
                                } else if record.event_type.starts_with("background:") {
                                    ProgressEventType::Background(record.event_type[11..].to_string())
                                } else if record.event_type.starts_with("custom:") {
                                    ProgressEventType::Custom(record.event_type[7..].to_string())
                                } else {
                                    ProgressEventType::Custom(record.event_type)
                                };

                                let status = match record.status.as_str() {
                                    "running" => ProgressStatus::Running,
                                    "completed" => ProgressStatus::Completed,
                                    "failed" => ProgressStatus::Failed,
                                    "cancelled" => ProgressStatus::Cancelled,
                                    _ => ProgressStatus::Running,
                                };

                                let started_at = record.created_at_datetime()?;
                                let last_updated = record.updated_at_datetime()?;

                                Ok(ProgressOperation {
                                    operation_id: record.operation_id,
                                    tenant_id: record.tenant_id,
                                    event_type,
                                    started_at,
                                    last_updated,
                                    progress_pct: record.progress_pct,
                                    status,
                                    message: record.message,
                                    metadata: record.metadata.and_then(|s| serde_json::from_str(&s).ok()),
                                })
                            })
                            .collect::<Result<Vec<_>>>()?;

                        Ok(operations)
                    }
                    Err(e) => {
                        warn!("Failed to query progress events from database: {}", e);
                        Err(AosError::Io(format!("Database query failed: {}", e)))
                    }
                }
            },
        }
    }

    /// Get active operations count
    pub async fn active_operations_count(&self) -> Result<usize> {
        let storage = self.storage.read().await;

        match *storage {
            ProgressStorage::Memory(ref ops) => Ok(ops.len()),
            ProgressStorage::Database(ref db) => {
                match db.count_active_operations(None).await {
                    Ok(count) => Ok(count as usize),
                    Err(e) => {
                        warn!("Failed to count active operations from database: {}", e);
                        Ok(0) // Return 0 on error
                    }
                }
            }
        }
    }

    /// Subscribe to progress events
    pub fn subscribe_events(&self) -> broadcast::Receiver<ProgressEvent> {
        // Record SSE subscriber metric
        if let Some(ref metrics) = self.metrics {
            let _ = metrics.sse_subscribers.inc();
        }
        self.event_tx.subscribe()
    }

    /// Record operation start metric
    fn record_operation_started(&self, event_type: &str, tenant_id: &str) {
        if let Some(ref metrics) = self.metrics {
            let _ = metrics.operations_started_total
                .with_label_values(&[event_type, tenant_id])
                .inc();
        }
    }

    /// Record operation completion metric
    fn record_operation_completed(&self, event_type: &str, tenant_id: &str, duration_secs: f64) {
        if let Some(ref metrics) = self.metrics {
            let _ = metrics.operations_completed_total
                .with_label_values(&[event_type, tenant_id])
                .inc();
            let _ = metrics.operation_duration_seconds
                .with_label_values(&[event_type, "completed"])
                .observe(duration_secs);
        }
    }

    /// Record operation failure metric
    fn record_operation_failed(&self, event_type: &str, tenant_id: &str, duration_secs: f64) {
        if let Some(ref metrics) = self.metrics {
            let _ = metrics.operations_failed_total
                .with_label_values(&[event_type, tenant_id])
                .inc();
            let _ = metrics.operation_duration_seconds
                .with_label_values(&[event_type, "failed"])
                .observe(duration_secs);
        }
    }

    /// Record operation cancellation metric
    fn record_operation_cancelled(&self, event_type: &str, tenant_id: &str) {
        if let Some(ref metrics) = self.metrics {
            let _ = metrics.operations_cancelled_total
                .with_label_values(&[event_type, tenant_id])
                .inc();
        }
    }

    /// Update active operations gauge
    fn update_active_operations(&self, event_type: &str, tenant_id: &str, count: f64) {
        if let Some(ref metrics) = self.metrics {
            let _ = metrics.active_operations
                .with_label_values(&[event_type, tenant_id])
                .set(count);
        }
    }

    /// Record event emission
    fn record_event_emitted(&self, event_type: &str) {
        if let Some(ref metrics) = self.metrics {
            let _ = metrics.events_emitted_total
                .with_label_values(&[event_type])
                .inc();
        }
    }

    /// Record database operation latency
    fn record_db_operation(&self, operation: &str, duration_secs: f64) {
        if let Some(ref metrics) = self.metrics {
            let _ = metrics.db_operation_duration_seconds
                .with_label_values(&[operation])
                .observe(duration_secs);
        }
    }

    /// Record query operation latency
    fn record_query_operation(&self, duration_secs: f64) {
        if let Some(ref metrics) = self.metrics {
            let _ = metrics.query_duration_seconds
                .with_label_values(&[])
                .observe(duration_secs);
        }
    }

    /// Get event type string for metrics
    fn event_type_string(event_type: &ProgressEventType) -> &'static str {
        match event_type {
            ProgressEventType::Operation(_) => "operation",
            ProgressEventType::Training(_) => "training",
            ProgressEventType::Background(_) => "background",
            ProgressEventType::Custom(_) => "custom",
        }
    }

    /// Emit a progress event
    async fn emit_progress_event(
        &self,
        operation_id: String,
        tenant_id: String,
        event_type: ProgressEventType,
        progress_pct: f64,
        status: ProgressStatus,
        message: Option<String>,
    ) -> Result<()> {
        // Record event emission metric
        let event_type_str = Self::event_type_string(&event_type);
        self.record_event_emitted(event_type_str);

        // Get started_at from storage to calculate elapsed time
        let (started_at, elapsed_secs) = {
            let storage = self.storage.read().await;
            match &*storage {
                ProgressStorage::Memory(ops) => {
                    if let Some(op) = ops.get(&operation_id) {
                        let elapsed = (Utc::now() - op.started_at).num_milliseconds() as f64 / 1000.0;
                        (op.started_at.to_rfc3339(), elapsed)
                    } else {
                        (Utc::now().to_rfc3339(), 0.0)
                    }
                }
                ProgressStorage::Database(db) => {
                    let query = adapteros_db::progress_events::ProgressEventQuery {
                        operation_id: Some(operation_id.clone()),
                        limit: Some(1),
                        ..Default::default()
                    };
                    if let Ok(records) = db.get_progress_events(query).await {
                        if let Some(record) = records.first() {
                            match record.created_at_datetime() {
                                Ok(created_at) => {
                                    let elapsed = (Utc::now() - created_at).num_milliseconds() as f64 / 1000.0;
                                    (created_at.to_rfc3339(), elapsed)
                                }
                                Err(e) => {
                                    warn!("Invalid progress event timestamp: {}", e);
                                    (Utc::now().to_rfc3339(), 0.0)
                                }
                            }
                        } else {
                            (Utc::now().to_rfc3339(), 0.0)
                        }
                    } else {
                        (Utc::now().to_rfc3339(), 0.0)
                    }
                }
            }
        };

        let event = ProgressEvent {
            event_id: Uuid::new_v4().to_string(),
            operation_id,
            tenant_id,
            event_type,
            progress_pct,
            status,
            message,
            started_at,
            updated_at: Utc::now().to_rfc3339(),
            elapsed_secs,
            metadata: None,
        };

        let _ = self.event_tx.send(event);
        Ok(())
    }

    /// Cleanup old operations (called periodically)
    pub async fn cleanup_expired_operations(&self) -> Result<usize> {
        let cutoff = Utc::now() - chrono::Duration::hours(self.config.retention_hours);
        let mut storage = self.storage.write().await;

        match *storage {
            ProgressStorage::Memory(ref mut ops) => {
                let initial_count = ops.len();
                ops.retain(|_, op| op.last_updated > cutoff);
                let removed_count = initial_count - ops.len();

                if removed_count > 0 {
                    info!("Cleaned up {} expired operations", removed_count);
                }

                Ok(removed_count)
            }
            ProgressStorage::Database(ref db) => {
                let removed = db.delete_old_progress_events(cutoff).await?;
                if removed > 0 {
                    info!("Cleaned up {} expired operations in database", removed);
                }
                Ok(removed as usize)
            }
        }
    }
}

impl Default for ProgressService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_progress_service_basic_operations() {
        let service = ProgressService::new();

        // Test starting an operation
        service
            .start_operation(
                "test-op-1",
                "test-tenant",
                ProgressEventType::Operation("load".to_string()),
            )
            .await
            .expect("Failed to start operation");

        // Test getting progress
        let progress = service
            .get_progress("test-op-1")
            .await
            .expect("Failed to get progress")
            .expect("Operation should exist");

        assert_eq!(progress.operation_id, "test-op-1");
        assert_eq!(progress.tenant_id, "test-tenant");
        assert_eq!(progress.progress_pct, 0.0);
        assert_eq!(progress.status, ProgressStatus::Running);

        // Test updating progress
        service
            .update_progress("test-op-1", 50.0, Some("Halfway done".to_string()))
            .await
            .expect("Failed to update progress");

        let updated_progress = service
            .get_progress("test-op-1")
            .await
            .expect("Failed to get updated progress")
            .expect("Operation should still exist");

        assert_eq!(updated_progress.progress_pct, 50.0);
        assert_eq!(updated_progress.message, Some("Halfway done".to_string()));

        // Test completing operation
        service
            .complete_operation("test-op-1", true)
            .await
            .expect("Failed to complete operation");

        // Operation should still exist immediately after completion
        let completed_progress = service
            .get_progress("test-op-1")
            .await
            .expect("Failed to get completed progress")
            .expect("Operation should still exist");

        assert_eq!(completed_progress.status, ProgressStatus::Completed);
    }

    #[tokio::test]
    async fn test_progress_service_filtering() {
        let service = ProgressService::new();

        // Start multiple operations
        service
            .start_operation(
                "op1",
                "tenant1",
                ProgressEventType::Operation("load".to_string()),
            )
            .await
            .expect("Failed to start operation");

        service
            .start_operation(
                "op2",
                "tenant2",
                ProgressEventType::Training("train".to_string()),
            )
            .await
            .expect("Failed to start operation");

        // Test filtering by tenant
        let filter = ProgressFilter {
            tenant_id: Some("tenant1".to_string()),
            ..Default::default()
        };

        let results = service
            .query_operations(filter)
            .await
            .expect("Failed to query operations");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].operation_id, "op1");

        // Test filtering by event type
        let filter = ProgressFilter {
            event_type: Some("training:train".to_string()),
            ..Default::default()
        };

        let results = service
            .query_operations(filter)
            .await
            .expect("Failed to query operations");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].operation_id, "op2");
    }
}
