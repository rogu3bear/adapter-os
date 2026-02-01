//! Unified Progress Service for adapterOS
//!
//! Centralizes progress tracking across all operations (adapter loading, training,
//! background tasks, etc.) with standardized event emission, filtering, and persistence.

use adapteros_core::{AosError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};
use uuid::Uuid;

/// Progress event emitted via broadcast channel for SSE streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub event_id: String,
    pub operation_id: String,
    pub tenant_id: String,
    pub event_type: ProgressEventType,
    pub progress_pct: f64,
    pub status: ProgressStatus,
    pub message: Option<String>,
    pub started_at: String,
    pub updated_at: String,
    pub elapsed_secs: f64,
    pub metadata: Option<serde_json::Value>,
}

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
#[derive(Debug, Clone, Deserialize, Default)]
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
pub struct ProgressService {
    db: Arc<adapteros_db::Db>,
    event_tx: broadcast::Sender<ProgressEvent>,
    config: ProgressConfig,
    metrics: Option<ProgressMetrics>,
}

/// Progress tracking metrics
#[allow(dead_code)]
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
    /// Create a new progress service with database storage
    pub fn new(db: Arc<adapteros_db::Db>) -> Self {
        Self::with_database_and_config(db, ProgressConfig::default())
    }

    /// Create a new progress service with database storage
    pub fn with_database(db: Arc<adapteros_db::Db>) -> Self {
        Self::with_database_and_config(db, ProgressConfig::default())
    }

    /// Create a new progress service with database storage and custom configuration
    pub fn with_database_and_config(db: Arc<adapteros_db::Db>, config: ProgressConfig) -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        let metrics = if config.enable_metrics {
            Some(Self::create_metrics())
        } else {
            None
        };

        Self {
            db,
            event_tx,
            config,
            metrics,
        }
    }

    /// Create Prometheus metrics for progress tracking
    fn create_metrics() -> ProgressMetrics {
        use prometheus::{CounterVec, GaugeVec, HistogramVec, Opts};

        let operations_started_total = CounterVec::new(
            Opts::new(
                "progress_operations_started_total",
                "Total number of operations started",
            )
            .namespace("adapteros"),
            &["event_type", "tenant_id"],
        )
        .expect("Failed to create Prometheus metric");

        let operations_completed_total = CounterVec::new(
            Opts::new(
                "progress_operations_completed_total",
                "Total number of operations completed",
            )
            .namespace("adapteros"),
            &["event_type", "tenant_id"],
        )
        .expect("Failed to create Prometheus metric");

        let operations_failed_total = CounterVec::new(
            Opts::new(
                "progress_operations_failed_total",
                "Total number of operations failed",
            )
            .namespace("adapteros"),
            &["event_type", "tenant_id"],
        )
        .expect("Failed to create Prometheus metric");

        let operations_cancelled_total = CounterVec::new(
            Opts::new(
                "progress_operations_cancelled_total",
                "Total number of operations cancelled",
            )
            .namespace("adapteros"),
            &["event_type", "tenant_id"],
        )
        .expect("Failed to create Prometheus metric");

        let active_operations = GaugeVec::new(
            Opts::new(
                "progress_active_operations",
                "Number of currently active operations",
            )
            .namespace("adapteros"),
            &["event_type", "tenant_id"],
        )
        .expect("Failed to create Prometheus metric");

        let operation_duration_seconds = HistogramVec::new(
            prometheus::HistogramOpts::new(
                "progress_operation_duration_seconds",
                "Duration of completed operations in seconds",
            )
            .namespace("adapteros")
            .buckets(vec![
                1.0, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0,
            ]),
            &["event_type", "status"],
        )
        .expect("Failed to create Prometheus metric");

        let db_operation_duration_seconds = HistogramVec::new(
            prometheus::HistogramOpts::new(
                "progress_db_operation_duration_seconds",
                "Duration of database operations in seconds",
            )
            .namespace("adapteros")
            .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]),
            &["operation"],
        )
        .expect("Failed to create Prometheus metric");

        let query_duration_seconds = HistogramVec::new(
            prometheus::HistogramOpts::new(
                "progress_query_duration_seconds",
                "Duration of progress queries in seconds",
            )
            .namespace("adapteros")
            .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0]),
            &[],
        )
        .expect("Failed to create Prometheus metric");

        let events_emitted_total = CounterVec::new(
            Opts::new(
                "progress_events_emitted_total",
                "Total number of progress events emitted",
            )
            .namespace("adapteros"),
            &["event_type"],
        )
        .expect("Failed to create Prometheus metric");

        let sse_subscribers = prometheus::Gauge::new(
            "progress_sse_subscribers",
            "Number of active SSE subscribers",
        )
        .expect("Failed to create Prometheus metric");

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

        let event_type_str = match &event_type {
            ProgressEventType::Operation(s) => format!("operation:{}", s),
            ProgressEventType::Training(s) => format!("training:{}", s),
            ProgressEventType::Background(s) => format!("background:{}", s),
            ProgressEventType::Custom(s) => format!("custom:{}", s),
        };

        // Store operation in database
        self.db
            .create_progress_event(
                &operation_id,
                &tenant_id,
                &event_type_str,
                0.0,
                "running",
                Some("Operation started"),
                None,
            )
            .await?;

        // Record metrics
        let metric_type = Self::event_type_string(&event_type);
        self.record_operation_started(metric_type, &tenant_id);

        // Emit progress event
        self.emit_progress_event(
            operation_id.clone(),
            tenant_id.clone(),
            event_type.clone(),
            0.0,
            ProgressStatus::Running,
            Some("Operation started".to_string()),
        )
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

        // Fetch operation from database
        let query = adapteros_db::progress_events::ProgressEventQuery {
            operation_id: Some(operation_id.clone()),
            limit: Some(1),
            ..Default::default()
        };

        let records = self.db.get_progress_events(query).await?;
        let record = records
            .first()
            .ok_or_else(|| AosError::NotFound(format!("Operation not found: {}", operation_id)))?;

        // Update in database
        self.db
            .update_progress_event(
                &operation_id,
                &record.tenant_id,
                progress_pct,
                "running",
                message.as_deref(),
            )
            .await?;

        let event_type = Self::parse_event_type(&record.event_type);

        // Emit progress event
        self.emit_progress_event(
            operation_id,
            record.tenant_id.clone(),
            event_type,
            progress_pct,
            ProgressStatus::Running,
            message,
        )
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
        let final_status = if success {
            ProgressStatus::Completed
        } else {
            ProgressStatus::Failed
        };
        let message = if success {
            "Operation completed successfully"
        } else {
            "Operation failed"
        };

        // Fetch operation from database
        let query = adapteros_db::progress_events::ProgressEventQuery {
            operation_id: Some(operation_id.clone()),
            limit: Some(1),
            ..Default::default()
        };
        let records = self.db.get_progress_events(query).await?;
        let record = records
            .first()
            .ok_or_else(|| AosError::NotFound(format!("Operation not found: {}", operation_id)))?;

        let final_status_str = final_status.to_string();
        self.db
            .update_progress_event(
                &operation_id,
                &record.tenant_id,
                100.0,
                final_status_str.as_str(),
                Some(message),
            )
            .await?;

        let event_type = Self::parse_event_type(&record.event_type);
        let created_at = record.created_at_datetime()?;
        let duration_secs = (Utc::now() - created_at).num_milliseconds() as f64 / 1000.0;

        // Record metrics
        let event_type_str = Self::event_type_string(&event_type);
        if success {
            self.record_operation_completed(event_type_str, &record.tenant_id, duration_secs);
        } else {
            self.record_operation_failed(event_type_str, &record.tenant_id, duration_secs);
        }

        // Emit final progress event
        self.emit_progress_event(
            operation_id.clone(),
            record.tenant_id.clone(),
            event_type,
            100.0,
            final_status,
            Some(message.to_string()),
        )
        .await?;

        info!(
            operation_id = %operation_id,
            success = success,
            "Completed progress tracking"
        );

        Ok(())
    }

    /// Get current progress for an operation
    pub async fn get_progress(
        &self,
        operation_id: impl Into<String>,
    ) -> Result<Option<ProgressOperation>> {
        let operation_id = operation_id.into();

        let query = adapteros_db::progress_events::ProgressEventQuery {
            operation_id: Some(operation_id.clone()),
            limit: Some(1),
            ..Default::default()
        };

        match self.db.get_progress_events(query).await {
            Ok(mut records) if !records.is_empty() => {
                let record = records.remove(0);
                let event_type = Self::parse_event_type(&record.event_type);
                let status = Self::parse_status(&record.status);
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

    /// Query operations with filtering
    pub async fn query_operations(&self, filter: ProgressFilter) -> Result<Vec<ProgressOperation>> {
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

        match self.db.get_progress_events(query).await {
            Ok(records) => {
                let operations: Vec<ProgressOperation> = records
                    .into_iter()
                    .map(|record| {
                        let event_type = Self::parse_event_type(&record.event_type);
                        let status = Self::parse_status(&record.status);
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
    }

    /// Get active operations count
    pub async fn active_operations_count(&self) -> Result<usize> {
        match self.db.count_active_operations(None).await {
            Ok(count) => Ok(count as usize),
            Err(e) => {
                warn!("Failed to count active operations from database: {}", e);
                Ok(0) // Return 0 on error
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
            let _ = metrics
                .operations_started_total
                .with_label_values(&[event_type, tenant_id])
                .inc();
        }
    }

    /// Record operation completion metric
    fn record_operation_completed(&self, event_type: &str, tenant_id: &str, duration_secs: f64) {
        if let Some(ref metrics) = self.metrics {
            let _ = metrics
                .operations_completed_total
                .with_label_values(&[event_type, tenant_id])
                .inc();
            let _ = metrics
                .operation_duration_seconds
                .with_label_values(&[event_type, "completed"])
                .observe(duration_secs);
        }
    }

    /// Record operation failure metric
    fn record_operation_failed(&self, event_type: &str, tenant_id: &str, duration_secs: f64) {
        if let Some(ref metrics) = self.metrics {
            let _ = metrics
                .operations_failed_total
                .with_label_values(&[event_type, tenant_id])
                .inc();
            let _ = metrics
                .operation_duration_seconds
                .with_label_values(&[event_type, "failed"])
                .observe(duration_secs);
        }
    }

    /// Record operation cancellation metric
    #[allow(dead_code)]
    fn record_operation_cancelled(&self, event_type: &str, tenant_id: &str) {
        if let Some(ref metrics) = self.metrics {
            let _ = metrics
                .operations_cancelled_total
                .with_label_values(&[event_type, tenant_id])
                .inc();
        }
    }

    /// Update active operations gauge
    #[allow(dead_code)]
    fn update_active_operations(&self, event_type: &str, tenant_id: &str, count: f64) {
        if let Some(ref metrics) = self.metrics {
            let _ = metrics
                .active_operations
                .with_label_values(&[event_type, tenant_id])
                .set(count);
        }
    }

    /// Record event emission
    fn record_event_emitted(&self, event_type: &str) {
        if let Some(ref metrics) = self.metrics {
            let _ = metrics
                .events_emitted_total
                .with_label_values(&[event_type])
                .inc();
        }
    }

    /// Record database operation latency
    #[allow(dead_code)]
    fn record_db_operation(&self, operation: &str, duration_secs: f64) {
        if let Some(ref metrics) = self.metrics {
            let _ = metrics
                .db_operation_duration_seconds
                .with_label_values(&[operation])
                .observe(duration_secs);
        }
    }

    /// Record query operation latency
    #[allow(dead_code)]
    fn record_query_operation(&self, duration_secs: f64) {
        if let Some(ref metrics) = self.metrics {
            let empty: &[&str] = &[];
            let _ = metrics
                .query_duration_seconds
                .with_label_values(empty)
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

        // Get started_at from database to calculate elapsed time
        let (started_at, elapsed_secs) = {
            let query = adapteros_db::progress_events::ProgressEventQuery {
                operation_id: Some(operation_id.clone()),
                limit: Some(1),
                ..Default::default()
            };
            if let Ok(records) = self.db.get_progress_events(query).await {
                if let Some(record) = records.first() {
                    match record.created_at_datetime() {
                        Ok(created_at) => {
                            let elapsed =
                                (Utc::now() - created_at).num_milliseconds() as f64 / 1000.0;
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
        let removed = self.db.delete_old_progress_events(cutoff).await?;
        if removed > 0 {
            info!("Cleaned up {} expired operations from database", removed);
        }
        Ok(removed as usize)
    }

    /// Parse event type from database string
    fn parse_event_type(event_type: &str) -> ProgressEventType {
        if let Some(suffix) = event_type.strip_prefix("operation:") {
            ProgressEventType::Operation(suffix.to_string())
        } else if let Some(suffix) = event_type.strip_prefix("training:") {
            ProgressEventType::Training(suffix.to_string())
        } else if let Some(suffix) = event_type.strip_prefix("background:") {
            ProgressEventType::Background(suffix.to_string())
        } else if let Some(suffix) = event_type.strip_prefix("custom:") {
            ProgressEventType::Custom(suffix.to_string())
        } else {
            ProgressEventType::Custom(event_type.to_string())
        }
    }

    /// Parse status from database string
    fn parse_status(status: &str) -> ProgressStatus {
        match status {
            "running" => ProgressStatus::Running,
            "completed" => ProgressStatus::Completed,
            "failed" => ProgressStatus::Failed,
            "cancelled" => ProgressStatus::Cancelled,
            _ => ProgressStatus::Running,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_service() -> ProgressService {
        let db = adapteros_db::Db::new_in_memory()
            .await
            .expect("Failed to create in-memory test database");
        ProgressService::new(Arc::new(db))
    }

    #[tokio::test]
    async fn test_progress_service_basic_operations() {
        let service = create_test_service().await;

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
        let service = create_test_service().await;

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
