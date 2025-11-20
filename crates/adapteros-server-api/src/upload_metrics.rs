//! Upload performance metrics and telemetry for AdapterOS
//!
//! Comprehensive metrics collection for upload operations including:
//! - Upload duration histograms (streaming, database registration)
//! - Success/failure rate counters per tenant
//! - File size distribution metrics
//! - Per-tenant upload volume tracking
//! - Queue depth and processing time metrics
//! - Cleanup operation monitoring
//! - Integration with adapteros-telemetry system

use adapteros_core::identity::IdentityEnvelope;
use adapteros_telemetry::{
    unified_events::{EventType, LogLevel},
    TelemetryEventBuilder, TelemetryWriter,
};
use prometheus::{CounterVec, Encoder, GaugeVec, HistogramOpts, HistogramVec, Opts, Registry};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Upload metrics collector with Prometheus integration
pub struct UploadMetricsCollector {
    registry: Registry,
    // Upload duration metrics (in seconds)
    upload_duration_streaming: HistogramVec,
    upload_duration_database: HistogramVec,
    upload_duration_total: HistogramVec,
    // File size metrics (in bytes)
    file_size_bytes: HistogramVec,
    // Success/failure counters
    uploads_successful_total: CounterVec,
    uploads_failed_total: CounterVec,
    uploads_rate_limited_total: CounterVec,
    uploads_aborted_total: CounterVec,
    // Per-tenant metrics
    uploads_per_tenant_total: GaugeVec,
    bytes_uploaded_per_tenant_total: CounterVec,
    // Queue depth metrics
    upload_queue_depth: GaugeVec,
    pending_cleanup_total: CounterVec,
    // Cleanup operation metrics
    cleanup_operations_total: CounterVec,
    cleanup_duration: HistogramVec,
    temp_files_deleted_total: CounterVec,
    cleanup_errors_total: CounterVec,
    // Rate limiter metrics
    rate_limit_tokens_available: GaugeVec,
    rate_limit_refills_total: CounterVec,
    // Error categorization
    upload_errors_by_type: CounterVec,
    // Metrics cache for snapshots
    metrics_cache: Arc<RwLock<UploadMetricsSnapshot>>,
    // Telemetry writer for structured events
    telemetry_writer: Option<TelemetryWriter>,
}

/// Snapshot of current upload metrics for JSON export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadMetricsSnapshot {
    pub timestamp: u64,
    pub upload_durations: UploadDurationMetrics,
    pub file_size: FileSizeMetrics,
    pub success_rates: SuccessRateMetrics,
    pub tenant_metrics: TenantUploadMetrics,
    pub queue_metrics: QueueMetrics,
    pub cleanup_metrics: CleanupMetrics,
    pub rate_limit_metrics: RateLimitMetrics,
}

/// Upload duration statistics (in milliseconds)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadDurationMetrics {
    pub streaming_p50_ms: f64,
    pub streaming_p95_ms: f64,
    pub streaming_p99_ms: f64,
    pub database_p50_ms: f64,
    pub database_p95_ms: f64,
    pub database_p99_ms: f64,
    pub total_p50_ms: f64,
    pub total_p95_ms: f64,
    pub total_p99_ms: f64,
}

/// File size distribution metrics (in bytes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSizeMetrics {
    pub small_files_total: u64,  // < 10MB
    pub medium_files_total: u64, // 10-100MB
    pub large_files_total: u64,  // 100-500MB
    pub xlarge_files_total: u64, // > 500MB
    pub avg_file_size_bytes: f64,
    pub max_file_size_bytes: u64,
}

/// Upload success and failure rates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessRateMetrics {
    pub successful_uploads_total: u64,
    pub failed_uploads_total: u64,
    pub rate_limited_total: u64,
    pub aborted_total: u64,
    pub success_rate_percent: f64,
}

/// Per-tenant upload metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantUploadMetrics {
    pub uploads_per_tenant: HashMap<String, f64>,
    pub bytes_per_tenant: HashMap<String, u64>,
    pub top_uploading_tenants: Vec<(String, u64)>,
}

/// Queue depth and processing time metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMetrics {
    pub current_queue_depth: f64,
    pub max_queue_depth: f64,
    pub pending_cleanup_items: f64,
}

/// Cleanup operation metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupMetrics {
    pub cleanup_operations_total: u64,
    pub cleanup_duration_p50_ms: f64,
    pub cleanup_duration_p95_ms: f64,
    pub cleanup_duration_p99_ms: f64,
    pub temp_files_deleted_total: u64,
    pub cleanup_errors_total: u64,
}

/// Rate limiter status metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitMetrics {
    pub tokens_available_per_tenant: HashMap<String, f64>,
    pub refills_total: u64,
}

impl UploadMetricsCollector {
    /// Create a new upload metrics collector
    pub fn new() -> Result<Self, String> {
        let registry = Registry::new();

        // Define histogram buckets for different metrics
        let duration_buckets = vec![
            0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0,
        ];
        let file_size_buckets = vec![
            1_000_000.0,     // 1MB
            10_000_000.0,    // 10MB
            50_000_000.0,    // 50MB
            100_000_000.0,   // 100MB
            500_000_000.0,   // 500MB
            1_000_000_000.0, // 1GB
        ];

        // Upload duration metrics (in seconds)
        let upload_duration_streaming = HistogramVec::new(
            HistogramOpts::new(
                "adapteros_upload_duration_streaming_seconds",
                "Upload streaming duration in seconds",
            )
            .buckets(duration_buckets.clone()),
            &["tenant_id", "tier"],
        )
        .map_err(|e| format!("Failed to create streaming duration histogram: {}", e))?;
        registry
            .register(Box::new(upload_duration_streaming.clone()))
            .map_err(|e| format!("Failed to register streaming duration histogram: {}", e))?;

        let upload_duration_database = HistogramVec::new(
            HistogramOpts::new(
                "adapteros_upload_duration_database_seconds",
                "Upload database registration duration in seconds",
            )
            .buckets(duration_buckets.clone()),
            &["tenant_id"],
        )
        .map_err(|e| format!("Failed to create database duration histogram: {}", e))?;
        registry
            .register(Box::new(upload_duration_database.clone()))
            .map_err(|e| format!("Failed to register database duration histogram: {}", e))?;

        let upload_duration_total = HistogramVec::new(
            HistogramOpts::new(
                "adapteros_upload_duration_total_seconds",
                "Total upload duration in seconds (streaming + database)",
            )
            .buckets(duration_buckets.clone()),
            &["tenant_id", "tier"],
        )
        .map_err(|e| format!("Failed to create total duration histogram: {}", e))?;
        registry
            .register(Box::new(upload_duration_total.clone()))
            .map_err(|e| format!("Failed to register total duration histogram: {}", e))?;

        // File size metrics (in bytes)
        let file_size_bytes = HistogramVec::new(
            HistogramOpts::new(
                "adapteros_upload_file_size_bytes",
                "Uploaded file size in bytes",
            )
            .buckets(file_size_buckets),
            &["tenant_id", "tier", "category"],
        )
        .map_err(|e| format!("Failed to create file size histogram: {}", e))?;
        registry
            .register(Box::new(file_size_bytes.clone()))
            .map_err(|e| format!("Failed to register file size histogram: {}", e))?;

        // Success/failure counters
        let uploads_successful_total = CounterVec::new(
            Opts::new(
                "adapteros_uploads_successful_total",
                "Total successful uploads",
            ),
            &["tenant_id", "tier"],
        )
        .map_err(|e| format!("Failed to create successful uploads counter: {}", e))?;
        registry
            .register(Box::new(uploads_successful_total.clone()))
            .map_err(|e| format!("Failed to register successful uploads counter: {}", e))?;

        let uploads_failed_total = CounterVec::new(
            Opts::new("adapteros_uploads_failed_total", "Total failed uploads"),
            &["tenant_id", "reason"],
        )
        .map_err(|e| format!("Failed to create failed uploads counter: {}", e))?;
        registry
            .register(Box::new(uploads_failed_total.clone()))
            .map_err(|e| format!("Failed to register failed uploads counter: {}", e))?;

        let uploads_rate_limited_total = CounterVec::new(
            Opts::new(
                "adapteros_uploads_rate_limited_total",
                "Total uploads rejected by rate limiter",
            ),
            &["tenant_id"],
        )
        .map_err(|e| format!("Failed to create rate limited uploads counter: {}", e))?;
        registry
            .register(Box::new(uploads_rate_limited_total.clone()))
            .map_err(|e| format!("Failed to register rate limited uploads counter: {}", e))?;

        let uploads_aborted_total = CounterVec::new(
            Opts::new(
                "adapteros_uploads_aborted_total",
                "Total uploads aborted by client",
            ),
            &["tenant_id"],
        )
        .map_err(|e| format!("Failed to create aborted uploads counter: {}", e))?;
        registry
            .register(Box::new(uploads_aborted_total.clone()))
            .map_err(|e| format!("Failed to register aborted uploads counter: {}", e))?;

        // Per-tenant metrics
        let uploads_per_tenant_total = GaugeVec::new(
            Opts::new(
                "adapteros_uploads_per_tenant_total",
                "Total uploads per tenant",
            ),
            &["tenant_id"],
        )
        .map_err(|e| format!("Failed to create uploads per tenant gauge: {}", e))?;
        registry
            .register(Box::new(uploads_per_tenant_total.clone()))
            .map_err(|e| format!("Failed to register uploads per tenant gauge: {}", e))?;

        let bytes_uploaded_per_tenant_total = CounterVec::new(
            Opts::new(
                "adapteros_bytes_uploaded_per_tenant_total",
                "Total bytes uploaded per tenant",
            ),
            &["tenant_id"],
        )
        .map_err(|e| format!("Failed to create bytes per tenant counter: {}", e))?;
        registry
            .register(Box::new(bytes_uploaded_per_tenant_total.clone()))
            .map_err(|e| format!("Failed to register bytes per tenant counter: {}", e))?;

        // Queue depth metrics
        let upload_queue_depth = GaugeVec::new(
            Opts::new(
                "adapteros_upload_queue_depth",
                "Current depth of upload queue",
            ),
            &["queue_type"],
        )
        .map_err(|e| format!("Failed to create queue depth gauge: {}", e))?;
        registry
            .register(Box::new(upload_queue_depth.clone()))
            .map_err(|e| format!("Failed to register queue depth gauge: {}", e))?;

        let pending_cleanup_total = CounterVec::new(
            Opts::new(
                "adapteros_pending_cleanup_items",
                "Number of items pending cleanup",
            ),
            &["cleanup_type"],
        )
        .map_err(|e| format!("Failed to create pending cleanup counter: {}", e))?;
        registry
            .register(Box::new(pending_cleanup_total.clone()))
            .map_err(|e| format!("Failed to register pending cleanup counter: {}", e))?;

        // Cleanup operation metrics
        let cleanup_operations_total = CounterVec::new(
            Opts::new(
                "adapteros_cleanup_operations_total",
                "Total cleanup operations performed",
            ),
            &["cleanup_type", "result"],
        )
        .map_err(|e| format!("Failed to create cleanup operations counter: {}", e))?;
        registry
            .register(Box::new(cleanup_operations_total.clone()))
            .map_err(|e| format!("Failed to register cleanup operations counter: {}", e))?;

        let cleanup_duration = HistogramVec::new(
            HistogramOpts::new(
                "adapteros_cleanup_duration_seconds",
                "Cleanup operation duration in seconds",
            )
            .buckets(duration_buckets.clone()),
            &["cleanup_type"],
        )
        .map_err(|e| format!("Failed to create cleanup duration histogram: {}", e))?;
        registry
            .register(Box::new(cleanup_duration.clone()))
            .map_err(|e| format!("Failed to register cleanup duration histogram: {}", e))?;

        let temp_files_deleted_total = CounterVec::new(
            Opts::new(
                "adapteros_temp_files_deleted_total",
                "Total temporary files deleted during cleanup",
            ),
            &["cleanup_type"],
        )
        .map_err(|e| format!("Failed to create temp files deleted counter: {}", e))?;
        registry
            .register(Box::new(temp_files_deleted_total.clone()))
            .map_err(|e| format!("Failed to register temp files deleted counter: {}", e))?;

        let cleanup_errors_total = CounterVec::new(
            Opts::new(
                "adapteros_cleanup_errors_total",
                "Total errors during cleanup operations",
            ),
            &["cleanup_type", "error_type"],
        )
        .map_err(|e| format!("Failed to create cleanup errors counter: {}", e))?;
        registry
            .register(Box::new(cleanup_errors_total.clone()))
            .map_err(|e| format!("Failed to register cleanup errors counter: {}", e))?;

        // Rate limiter metrics
        let rate_limit_tokens_available = GaugeVec::new(
            Opts::new(
                "adapteros_rate_limit_tokens_available",
                "Available rate limit tokens per tenant",
            ),
            &["tenant_id"],
        )
        .map_err(|e| format!("Failed to create rate limit tokens gauge: {}", e))?;
        registry
            .register(Box::new(rate_limit_tokens_available.clone()))
            .map_err(|e| format!("Failed to register rate limit tokens gauge: {}", e))?;

        let rate_limit_refills_total = CounterVec::new(
            Opts::new(
                "adapteros_rate_limit_refills_total",
                "Total rate limit token refills",
            ),
            &["tenant_id"],
        )
        .map_err(|e| format!("Failed to create rate limit refills counter: {}", e))?;
        registry
            .register(Box::new(rate_limit_refills_total.clone()))
            .map_err(|e| format!("Failed to register rate limit refills counter: {}", e))?;

        // Error categorization
        let upload_errors_by_type = CounterVec::new(
            Opts::new(
                "adapteros_upload_errors_by_type",
                "Upload errors categorized by type",
            ),
            &["error_type"],
        )
        .map_err(|e| format!("Failed to create upload errors counter: {}", e))?;
        registry
            .register(Box::new(upload_errors_by_type.clone()))
            .map_err(|e| format!("Failed to register upload errors counter: {}", e))?;

        Ok(Self {
            registry,
            upload_duration_streaming,
            upload_duration_database,
            upload_duration_total,
            file_size_bytes,
            uploads_successful_total,
            uploads_failed_total,
            uploads_rate_limited_total,
            uploads_aborted_total,
            uploads_per_tenant_total,
            bytes_uploaded_per_tenant_total,
            upload_queue_depth,
            pending_cleanup_total,
            cleanup_operations_total,
            cleanup_duration,
            temp_files_deleted_total,
            cleanup_errors_total,
            rate_limit_tokens_available,
            rate_limit_refills_total,
            upload_errors_by_type,
            metrics_cache: Arc::new(RwLock::new(UploadMetricsSnapshot::default())),
            telemetry_writer: None,
        })
    }

    /// Set optional telemetry writer for structured event logging
    pub fn with_telemetry_writer(mut self, writer: TelemetryWriter) -> Self {
        self.telemetry_writer = Some(writer);
        self
    }

    /// Record a successful upload
    pub fn record_upload_success(
        &self,
        tenant_id: &str,
        tier: &str,
        file_size: u64,
        streaming_duration: Duration,
        database_duration: Duration,
    ) {
        let total_duration = streaming_duration + database_duration;

        // Convert durations to seconds (f64)
        let streaming_secs = streaming_duration.as_secs_f64();
        let database_secs = database_duration.as_secs_f64();
        let total_secs = total_duration.as_secs_f64();

        // Record histograms
        self.upload_duration_streaming
            .with_label_values(&[tenant_id, tier])
            .observe(streaming_secs);
        self.upload_duration_database
            .with_label_values(&[tenant_id])
            .observe(database_secs);
        self.upload_duration_total
            .with_label_values(&[tenant_id, tier])
            .observe(total_secs);

        // Record file size histogram (convert to category)
        let category = categorize_file_size(file_size);
        self.file_size_bytes
            .with_label_values(&[tenant_id, tier, category])
            .observe(file_size as f64);

        // Increment success counter
        self.uploads_successful_total
            .with_label_values(&[tenant_id, tier])
            .inc();

        // Update per-tenant metrics
        self.uploads_per_tenant_total
            .with_label_values(&[tenant_id])
            .inc();
        self.bytes_uploaded_per_tenant_total
            .with_label_values(&[tenant_id])
            .inc_by(file_size);

        info!(
            tenant_id = %tenant_id,
            tier = %tier,
            file_size = file_size,
            streaming_ms = streaming_secs * 1000.0,
            database_ms = database_secs * 1000.0,
            total_ms = total_secs * 1000.0,
            "Upload completed successfully"
        );

        // Log structured telemetry event if available
        if let Some(writer) = &self.telemetry_writer {
            let identity = IdentityEnvelope::new(
                tenant_id.to_string(),
                "upload".to_string(),
                "success".to_string(),
                "1.0".to_string(),
            );
            let event = TelemetryEventBuilder::new(
                EventType::Custom("upload.success".to_string()),
                LogLevel::Info,
                format!(
                    "Upload completed: {} bytes in {:.2}s",
                    file_size, total_secs
                ),
                identity,
            )
            .metadata(serde_json::json!({
                "file_size": file_size,
                "tier": tier,
                "streaming_ms": (streaming_secs * 1000.0) as u64,
                "database_ms": (database_secs * 1000.0) as u64,
                "total_ms": (total_secs * 1000.0) as u64,
            }))
            .build();
            let _ = writer.log_event(event);
        }
    }

    /// Record a failed upload
    pub fn record_upload_failure(&self, tenant_id: &str, reason: &str, error_type: &str) {
        self.uploads_failed_total
            .with_label_values(&[tenant_id, reason])
            .inc();

        self.upload_errors_by_type
            .with_label_values(&[error_type])
            .inc();

        warn!(
            tenant_id = %tenant_id,
            reason = %reason,
            error_type = %error_type,
            "Upload failed"
        );

        // Log structured telemetry event
        if let Some(writer) = &self.telemetry_writer {
            let identity = IdentityEnvelope::new(
                tenant_id.to_string(),
                "upload".to_string(),
                "failure".to_string(),
                "1.0".to_string(),
            );
            let event = TelemetryEventBuilder::new(
                EventType::Custom("upload.failure".to_string()),
                LogLevel::Warn,
                format!("Upload failed: {}", reason),
                identity,
            )
            .metadata(serde_json::json!({
                "reason": reason,
                "error_type": error_type,
            }))
            .build();
            let _ = writer.log_event(event);
        }
    }

    /// Record a rate-limited upload attempt
    pub fn record_rate_limited(&self, tenant_id: &str) {
        self.uploads_rate_limited_total
            .with_label_values(&[tenant_id])
            .inc();

        warn!(tenant_id = %tenant_id, "Upload rate limited");

        if let Some(writer) = &self.telemetry_writer {
            let identity = IdentityEnvelope::new(
                tenant_id.to_string(),
                "upload".to_string(),
                "rate_limited".to_string(),
                "1.0".to_string(),
            );
            let event = TelemetryEventBuilder::new(
                EventType::Custom("upload.rate_limited".to_string()),
                LogLevel::Warn,
                "Upload request rate limited".to_string(),
                identity,
            )
            .build();
            let _ = writer.log_event(event);
        }
    }

    /// Record an aborted upload
    pub fn record_upload_aborted(&self, tenant_id: &str) {
        self.uploads_aborted_total
            .with_label_values(&[tenant_id])
            .inc();

        debug!(tenant_id = %tenant_id, "Upload aborted by client");
    }

    /// Update queue depth metric
    pub fn set_queue_depth(&self, queue_type: &str, depth: f64) {
        self.upload_queue_depth
            .with_label_values(&[queue_type])
            .set(depth);
    }

    /// Update cleanup operation metrics
    pub fn record_cleanup_operation(
        &self,
        cleanup_type: &str,
        duration: Duration,
        result: &str,
        items_deleted: u64,
    ) {
        let duration_secs = duration.as_secs_f64();

        self.cleanup_operations_total
            .with_label_values(&[cleanup_type, result])
            .inc();

        self.cleanup_duration
            .with_label_values(&[cleanup_type])
            .observe(duration_secs);

        self.temp_files_deleted_total
            .with_label_values(&[cleanup_type])
            .inc_by(items_deleted);

        info!(
            cleanup_type = %cleanup_type,
            duration_ms = duration_secs * 1000.0,
            result = %result,
            items_deleted = items_deleted,
            "Cleanup operation completed"
        );

        if let Some(writer) = &self.telemetry_writer {
            let identity = IdentityEnvelope::new(
                "system".to_string(),
                "cleanup".to_string(),
                cleanup_type.to_string(),
                "1.0".to_string(),
            );
            let event = TelemetryEventBuilder::new(
                EventType::Custom("cleanup.completed".to_string()),
                LogLevel::Info,
                format!("Cleanup operation completed: {} items", items_deleted),
                identity,
            )
            .metadata(serde_json::json!({
                "cleanup_type": cleanup_type,
                "duration_ms": (duration_secs * 1000.0) as u64,
                "result": result,
                "items_deleted": items_deleted,
            }))
            .build();
            let _ = writer.log_event(event);
        }
    }

    /// Record cleanup error
    pub fn record_cleanup_error(&self, cleanup_type: &str, error_type: &str) {
        self.cleanup_errors_total
            .with_label_values(&[cleanup_type, error_type])
            .inc();

        warn!(
            cleanup_type = %cleanup_type,
            error_type = %error_type,
            "Cleanup error occurred"
        );
    }

    /// Update rate limit token availability
    pub fn set_rate_limit_tokens(&self, tenant_id: &str, tokens: f64) {
        self.rate_limit_tokens_available
            .with_label_values(&[tenant_id])
            .set(tokens);
    }

    /// Record rate limit refill
    pub fn record_rate_limit_refill(&self, tenant_id: &str) {
        self.rate_limit_refills_total
            .with_label_values(&[tenant_id])
            .inc();
    }

    /// Get metrics snapshot for JSON export
    pub async fn get_metrics_snapshot(&self) -> UploadMetricsSnapshot {
        let timestamp = current_timestamp();

        // Extract metrics from Prometheus collectors
        let snapshot = UploadMetricsSnapshot {
            timestamp,
            upload_durations: UploadDurationMetrics {
                streaming_p50_ms: 0.0, // Would need to query histograms
                streaming_p95_ms: 0.0,
                streaming_p99_ms: 0.0,
                database_p50_ms: 0.0,
                database_p95_ms: 0.0,
                database_p99_ms: 0.0,
                total_p50_ms: 0.0,
                total_p95_ms: 0.0,
                total_p99_ms: 0.0,
            },
            file_size: FileSizeMetrics {
                small_files_total: 0,
                medium_files_total: 0,
                large_files_total: 0,
                xlarge_files_total: 0,
                avg_file_size_bytes: 0.0,
                max_file_size_bytes: 0,
            },
            success_rates: SuccessRateMetrics {
                successful_uploads_total: 0,
                failed_uploads_total: 0,
                rate_limited_total: 0,
                aborted_total: 0,
                success_rate_percent: 0.0,
            },
            tenant_metrics: TenantUploadMetrics {
                uploads_per_tenant: HashMap::new(),
                bytes_per_tenant: HashMap::new(),
                top_uploading_tenants: Vec::new(),
            },
            queue_metrics: QueueMetrics {
                current_queue_depth: 0.0,
                max_queue_depth: 0.0,
                pending_cleanup_items: 0.0,
            },
            cleanup_metrics: CleanupMetrics {
                cleanup_operations_total: 0,
                cleanup_duration_p50_ms: 0.0,
                cleanup_duration_p95_ms: 0.0,
                cleanup_duration_p99_ms: 0.0,
                temp_files_deleted_total: 0,
                cleanup_errors_total: 0,
            },
            rate_limit_metrics: RateLimitMetrics {
                tokens_available_per_tenant: HashMap::new(),
                refills_total: 0,
            },
        };

        let mut cache = self.metrics_cache.write().await;
        *cache = snapshot.clone();

        snapshot
    }

    /// Get Prometheus text format metrics
    pub fn get_prometheus_metrics(&self) -> Result<String, String> {
        let encoder = prometheus::TextEncoder::new();
        encoder
            .encode(&self.registry.gather(), &mut Vec::new())
            .map(|bytes| String::from_utf8(bytes).unwrap_or_default())
            .map_err(|e| format!("Failed to encode metrics: {}", e))
    }

    /// Get the Prometheus registry (for advanced usage)
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

impl Default for UploadMetricsSnapshot {
    fn default() -> Self {
        Self {
            timestamp: current_timestamp(),
            upload_durations: UploadDurationMetrics {
                streaming_p50_ms: 0.0,
                streaming_p95_ms: 0.0,
                streaming_p99_ms: 0.0,
                database_p50_ms: 0.0,
                database_p95_ms: 0.0,
                database_p99_ms: 0.0,
                total_p50_ms: 0.0,
                total_p95_ms: 0.0,
                total_p99_ms: 0.0,
            },
            file_size: FileSizeMetrics {
                small_files_total: 0,
                medium_files_total: 0,
                large_files_total: 0,
                xlarge_files_total: 0,
                avg_file_size_bytes: 0.0,
                max_file_size_bytes: 0,
            },
            success_rates: SuccessRateMetrics {
                successful_uploads_total: 0,
                failed_uploads_total: 0,
                rate_limited_total: 0,
                aborted_total: 0,
                success_rate_percent: 0.0,
            },
            tenant_metrics: TenantUploadMetrics {
                uploads_per_tenant: HashMap::new(),
                bytes_per_tenant: HashMap::new(),
                top_uploading_tenants: Vec::new(),
            },
            queue_metrics: QueueMetrics {
                current_queue_depth: 0.0,
                max_queue_depth: 0.0,
                pending_cleanup_items: 0.0,
            },
            cleanup_metrics: CleanupMetrics {
                cleanup_operations_total: 0,
                cleanup_duration_p50_ms: 0.0,
                cleanup_duration_p95_ms: 0.0,
                cleanup_duration_p99_ms: 0.0,
                temp_files_deleted_total: 0,
                cleanup_errors_total: 0,
            },
            rate_limit_metrics: RateLimitMetrics {
                tokens_available_per_tenant: HashMap::new(),
                refills_total: 0,
            },
        }
    }
}

/// Categorize file size for metrics
fn categorize_file_size(bytes: u64) -> &'static str {
    match bytes {
        0..=10_485_760 => "small",            // < 10MB
        10_485_761..=104_857_600 => "medium", // 10-100MB
        104_857_601..=524_288_000 => "large", // 100-500MB
        _ => "xlarge",                        // > 500MB
    }
}

/// Get current Unix timestamp in seconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Helper struct for tracking upload timing
pub struct UploadTimer {
    start_time: Instant,
    streaming_duration: Option<Duration>,
}

impl UploadTimer {
    /// Create a new upload timer
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            streaming_duration: None,
        }
    }

    /// Mark when streaming is complete and database registration starts
    pub fn mark_streaming_complete(&mut self) {
        self.streaming_duration = Some(self.start_time.elapsed());
    }

    /// Get streaming duration
    pub fn streaming_duration(&self) -> Option<Duration> {
        self.streaming_duration
    }

    /// Get total elapsed time since creation
    pub fn total_elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Get database registration duration (total - streaming)
    pub fn database_duration(&self) -> Option<Duration> {
        self.streaming_duration
            .map(|streaming| self.total_elapsed() - streaming)
    }
}

impl Default for UploadTimer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upload_metrics_collector_creation() {
        let collector = UploadMetricsCollector::new();
        assert!(collector.is_ok());
    }

    #[test]
    fn test_categorize_file_size() {
        assert_eq!(categorize_file_size(1_000_000), "small"); // 1MB
        assert_eq!(categorize_file_size(10_485_760), "small"); // 10MB
        assert_eq!(categorize_file_size(50_000_000), "medium"); // 50MB
        assert_eq!(categorize_file_size(100_000_000), "large"); // 100MB
        assert_eq!(categorize_file_size(500_000_000), "large"); // 500MB
        assert_eq!(categorize_file_size(1_000_000_000), "xlarge"); // 1GB
    }

    #[test]
    fn test_upload_timer() {
        let mut timer = UploadTimer::new();
        std::thread::sleep(Duration::from_millis(10));
        timer.mark_streaming_complete();
        std::thread::sleep(Duration::from_millis(10));

        let streaming = timer.streaming_duration();
        let database = timer.database_duration();
        let total = timer.total_elapsed();

        assert!(streaming.is_some());
        assert!(database.is_some());
        assert!(total > streaming.unwrap());
        assert!(total > database.unwrap());
    }

    #[test]
    fn test_metrics_snapshot_default() {
        let snapshot = UploadMetricsSnapshot::default();
        assert_eq!(snapshot.success_rates.successful_uploads_total, 0);
        assert_eq!(snapshot.cleanup_metrics.cleanup_operations_total, 0);
    }
}
