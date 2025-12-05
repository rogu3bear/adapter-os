//! KV operation metrics tracking
//!
//! This module provides comprehensive metrics for KV backend operations:
//! - Read/write operation counts
//! - Operation latency (p50/p95/p99)
//! - SQL fallback counts
//! - Error tracking by type
//!
//! Metrics are collected using atomic counters for performance and can be
//! exported via the telemetry system.
//!
//! # Usage
//!
//! ## Basic Usage
//!
//! ```rust
//! use adapteros_db::{global_kv_metrics, KvOperationTimer, KvOperationType};
//!
//! // Manual timing
//! async fn my_kv_operation() {
//!     let start = std::time::Instant::now();
//!     // ... perform KV operation ...
//!     let metrics = global_kv_metrics();
//!     metrics.record_read(start.elapsed());
//! }
//!
//! // Automatic timing with RAII guard
//! async fn my_kv_write() {
//!     let _timer = KvOperationTimer::new(KvOperationType::Write);
//!     // ... perform KV operation ...
//!     // Timer automatically records duration on drop
//! }
//! ```
//!
//! ## Integration with KV Backend
//!
//! Add metrics to KV backend operations:
//!
//! ```rust,ignore
//! use adapteros_db::kv_metrics::{global_kv_metrics, KvErrorType};
//!
//! impl KvDb {
//!     pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
//!         let _timer = KvOperationTimer::new(KvOperationType::Read);
//!
//!         self.backend
//!             .get(key)
//!             .await
//!             .map_err(|e| {
//!                 global_kv_metrics().record_error(KvErrorType::Backend);
//!                 AosError::Database(format!("KV get failed: {}", e))
//!             })
//!     }
//!
//!     pub async fn set(&self, key: &str, value: Vec<u8>) -> Result<()> {
//!         let _timer = KvOperationTimer::new(KvOperationType::Write);
//!
//!         self.backend
//!             .set(key, value)
//!             .await
//!             .map_err(|e| {
//!                 global_kv_metrics().record_error(KvErrorType::Backend);
//!                 AosError::Database(format!("KV set failed: {}", e))
//!             })
//!     }
//! }
//! ```
//!
//! ## Tracking SQL Fallbacks
//!
//! In dual-write mode, track when operations fall back to SQL:
//!
//! ```rust,ignore
//! async fn dual_write_adapter(adapter: &Adapter) -> Result<()> {
//!     let metrics = global_kv_metrics();
//!
//!     // Try KV write first
//!     if let Err(e) = write_to_kv(adapter).await {
//!         warn!("KV write failed, falling back to SQL: {}", e);
//!         metrics.record_fallback_write();
//!         metrics.record_error(KvErrorType::Backend);
//!         write_to_sql(adapter).await?;
//!     }
//!     Ok(())
//! }
//! ```
//!
//! ## Exporting Metrics
//!
//! Get a snapshot for telemetry or REST API:
//!
//! ```rust
//! use adapteros_db::global_kv_metrics;
//!
//! async fn get_kv_metrics() -> serde_json::Value {
//!     let metrics = global_kv_metrics();
//!     let snapshot = metrics.snapshot();
//!     serde_json::to_value(&snapshot).unwrap()
//! }
//! ```

use adapteros_telemetry::alerting::{
    AlertComparator, AlertRecord, AlertRule, AlertSeverity, AlertingEngine, EscalationPolicy,
    NotificationChannel,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Global KV metrics instance
static KV_METRICS: once_cell::sync::Lazy<Arc<KvMetrics>> =
    once_cell::sync::Lazy::new(|| Arc::new(KvMetrics::new()));

/// Get the global KV metrics instance
pub fn global_kv_metrics() -> Arc<KvMetrics> {
    KV_METRICS.clone()
}

/// Metric keys used for KV alert rules.
pub const KV_ALERT_METRIC_FALLBACKS: &str = "kv.fallbacks_total";
pub const KV_ALERT_METRIC_ERRORS: &str = "kv.errors_total";
pub const KV_ALERT_METRIC_DRIFT: &str = "kv.drift_detections_total";
pub const KV_ALERT_METRIC_DEGRADATIONS: &str = "kv.degraded_events_total";

/// Default alert rules for KV degradation and drift detection.
pub fn kv_alert_rules() -> Vec<AlertRule> {
    let escalation = EscalationPolicy {
        repeat_interval: Duration::from_secs(300),
        channels: vec![NotificationChannel {
            channel_type: "log".to_string(),
            target: "kv-alerts".to_string(),
        }],
    };

    vec![
        AlertRule {
            name: "kv_fallbacks_detected".to_string(),
            metric: KV_ALERT_METRIC_FALLBACKS.to_string(),
            comparator: AlertComparator::GreaterThan,
            threshold: 0.0,
            severity: AlertSeverity::Warning,
            escalation: escalation.clone(),
        },
        AlertRule {
            name: "kv_backend_errors".to_string(),
            metric: KV_ALERT_METRIC_ERRORS.to_string(),
            comparator: AlertComparator::GreaterThan,
            threshold: 0.0,
            severity: AlertSeverity::Critical,
            escalation: escalation.clone(),
        },
        AlertRule {
            name: "kv_drift_detected".to_string(),
            metric: KV_ALERT_METRIC_DRIFT.to_string(),
            comparator: AlertComparator::GreaterThan,
            threshold: 0.0,
            severity: AlertSeverity::Warning,
            escalation: escalation.clone(),
        },
        AlertRule {
            name: "kv_degraded_events".to_string(),
            metric: KV_ALERT_METRIC_DEGRADATIONS.to_string(),
            comparator: AlertComparator::GreaterThan,
            threshold: 0.0,
            severity: AlertSeverity::Critical,
            escalation,
        },
    ]
}

/// Evaluate KV alert rules against the provided snapshot.
pub fn evaluate_kv_alerts(
    snapshot: &KvMetricsSnapshot,
    alerting: &mut AlertingEngine,
) -> Vec<AlertRecord> {
    let mut alerts = Vec::new();
    alerts.extend(alerting.evaluate_metric(
        KV_ALERT_METRIC_FALLBACKS,
        snapshot.fallback_operations_total as f64,
    ));
    alerts.extend(alerting.evaluate_metric(KV_ALERT_METRIC_ERRORS, snapshot.errors_total as f64));
    alerts.extend(alerting.evaluate_metric(
        KV_ALERT_METRIC_DRIFT,
        snapshot.drift_detections_total as f64,
    ));
    alerts.extend(alerting.evaluate_metric(
        KV_ALERT_METRIC_DEGRADATIONS,
        snapshot.degraded_events_total as f64,
    ));
    alerts
}

/// Evaluate KV alerts against the current global metrics snapshot.
pub fn evaluate_global_kv_alerts(alerting: &mut AlertingEngine) -> Vec<AlertRecord> {
    let snapshot = global_kv_metrics().snapshot();
    evaluate_kv_alerts(&snapshot, alerting)
}

/// KV operation metrics collector
///
/// Tracks all KV backend operations with atomic counters for thread-safe
/// concurrent access. Metrics include:
/// - Operation counts (reads, writes, deletes, scans)
/// - Latency tracking with histogram buckets
/// - SQL fallback counts
/// - Error counts by category
#[derive(Debug, Default)]
pub struct KvMetrics {
    // Operation counts
    reads_total: AtomicU64,
    writes_total: AtomicU64,
    deletes_total: AtomicU64,
    scans_total: AtomicU64,
    index_queries_total: AtomicU64,

    // Latency tracking (microseconds)
    // We use histogram buckets for percentile calculation
    read_latency_sum_us: AtomicU64,
    write_latency_sum_us: AtomicU64,
    delete_latency_sum_us: AtomicU64,
    scan_latency_sum_us: AtomicU64,

    // Latency buckets for percentile calculation (p50, p95, p99)
    // Buckets: <1ms, 1-5ms, 5-10ms, 10-50ms, 50-100ms, 100-500ms, >500ms
    read_latency_buckets: [AtomicU64; 7],
    write_latency_buckets: [AtomicU64; 7],
    delete_latency_buckets: [AtomicU64; 7],
    scan_latency_buckets: [AtomicU64; 7],

    // SQL fallback tracking
    fallback_reads_total: AtomicU64,
    fallback_writes_total: AtomicU64,
    fallback_deletes_total: AtomicU64,

    // Error tracking by category
    errors_not_found: AtomicU64,
    errors_serialization: AtomicU64,
    errors_backend: AtomicU64,
    errors_timeout: AtomicU64,
    errors_other: AtomicU64,

    // Drift/degradation tracking
    drift_detections_total: AtomicU64,
    degraded_events_total: AtomicU64,
}

impl KvMetrics {
    /// Create a new KV metrics collector
    pub fn new() -> Self {
        Self {
            reads_total: AtomicU64::new(0),
            writes_total: AtomicU64::new(0),
            deletes_total: AtomicU64::new(0),
            scans_total: AtomicU64::new(0),
            index_queries_total: AtomicU64::new(0),

            read_latency_sum_us: AtomicU64::new(0),
            write_latency_sum_us: AtomicU64::new(0),
            delete_latency_sum_us: AtomicU64::new(0),
            scan_latency_sum_us: AtomicU64::new(0),

            read_latency_buckets: Default::default(),
            write_latency_buckets: Default::default(),
            delete_latency_buckets: Default::default(),
            scan_latency_buckets: Default::default(),

            fallback_reads_total: AtomicU64::new(0),
            fallback_writes_total: AtomicU64::new(0),
            fallback_deletes_total: AtomicU64::new(0),

            errors_not_found: AtomicU64::new(0),
            errors_serialization: AtomicU64::new(0),
            errors_backend: AtomicU64::new(0),
            errors_timeout: AtomicU64::new(0),
            errors_other: AtomicU64::new(0),

            drift_detections_total: AtomicU64::new(0),
            degraded_events_total: AtomicU64::new(0),
        }
    }

    /// Record a KV read operation
    pub fn record_read(&self, duration: Duration) {
        self.reads_total.fetch_add(1, Ordering::Relaxed);
        let micros = duration.as_micros() as u64;
        self.read_latency_sum_us
            .fetch_add(micros, Ordering::Relaxed);
        self.increment_latency_bucket(&self.read_latency_buckets, duration);
    }

    /// Record a KV write operation
    pub fn record_write(&self, duration: Duration) {
        self.writes_total.fetch_add(1, Ordering::Relaxed);
        let micros = duration.as_micros() as u64;
        self.write_latency_sum_us
            .fetch_add(micros, Ordering::Relaxed);
        self.increment_latency_bucket(&self.write_latency_buckets, duration);
    }

    /// Record a KV delete operation
    pub fn record_delete(&self, duration: Duration) {
        self.deletes_total.fetch_add(1, Ordering::Relaxed);
        let micros = duration.as_micros() as u64;
        self.delete_latency_sum_us
            .fetch_add(micros, Ordering::Relaxed);
        self.increment_latency_bucket(&self.delete_latency_buckets, duration);
    }

    /// Record a KV scan operation
    pub fn record_scan(&self, duration: Duration) {
        self.scans_total.fetch_add(1, Ordering::Relaxed);
        let micros = duration.as_micros() as u64;
        self.scan_latency_sum_us
            .fetch_add(micros, Ordering::Relaxed);
        self.increment_latency_bucket(&self.scan_latency_buckets, duration);
    }

    /// Record a KV index query operation
    pub fn record_index_query(&self) {
        self.index_queries_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a SQL fallback read
    pub fn record_fallback_read(&self) {
        self.fallback_reads_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a SQL fallback write
    pub fn record_fallback_write(&self) {
        self.fallback_writes_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a SQL fallback delete
    pub fn record_fallback_delete(&self) {
        self.fallback_deletes_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an error by category
    pub fn record_error(&self, error_type: KvErrorType) {
        match error_type {
            KvErrorType::NotFound => self.errors_not_found.fetch_add(1, Ordering::Relaxed),
            KvErrorType::Serialization => self.errors_serialization.fetch_add(1, Ordering::Relaxed),
            KvErrorType::Backend => self.errors_backend.fetch_add(1, Ordering::Relaxed),
            KvErrorType::Timeout => self.errors_timeout.fetch_add(1, Ordering::Relaxed),
            KvErrorType::Other => self.errors_other.fetch_add(1, Ordering::Relaxed),
        };
    }

    /// Record a drift detection (SQL/KV mismatch or fallback)
    pub fn record_drift_detected(&self) {
        self.drift_detections_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a degradation event (KV guardrails triggered)
    pub fn record_degradation(&self) {
        self.degraded_events_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Get a snapshot of current metrics
    pub fn snapshot(&self) -> KvMetricsSnapshot {
        let reads = self.reads_total.load(Ordering::Relaxed);
        let writes = self.writes_total.load(Ordering::Relaxed);
        let deletes = self.deletes_total.load(Ordering::Relaxed);
        let scans = self.scans_total.load(Ordering::Relaxed);

        KvMetricsSnapshot {
            // Operation counts
            reads_total: reads,
            writes_total: writes,
            deletes_total: deletes,
            scans_total: scans,
            index_queries_total: self.index_queries_total.load(Ordering::Relaxed),
            operations_total: reads + writes + deletes + scans,

            // Average latencies
            read_avg_ms: self.calculate_avg_latency(&self.read_latency_sum_us, reads),
            write_avg_ms: self.calculate_avg_latency(&self.write_latency_sum_us, writes),
            delete_avg_ms: self.calculate_avg_latency(&self.delete_latency_sum_us, deletes),
            scan_avg_ms: self.calculate_avg_latency(&self.scan_latency_sum_us, scans),

            // Percentile latencies
            read_p50_ms: self.calculate_percentile(&self.read_latency_buckets, 50),
            read_p95_ms: self.calculate_percentile(&self.read_latency_buckets, 95),
            read_p99_ms: self.calculate_percentile(&self.read_latency_buckets, 99),
            write_p50_ms: self.calculate_percentile(&self.write_latency_buckets, 50),
            write_p95_ms: self.calculate_percentile(&self.write_latency_buckets, 95),
            write_p99_ms: self.calculate_percentile(&self.write_latency_buckets, 99),

            // SQL fallback counts
            fallback_reads_total: self.fallback_reads_total.load(Ordering::Relaxed),
            fallback_writes_total: self.fallback_writes_total.load(Ordering::Relaxed),
            fallback_deletes_total: self.fallback_deletes_total.load(Ordering::Relaxed),
            fallback_operations_total: self.fallback_reads_total.load(Ordering::Relaxed)
                + self.fallback_writes_total.load(Ordering::Relaxed)
                + self.fallback_deletes_total.load(Ordering::Relaxed),

            // Error counts
            errors_not_found: self.errors_not_found.load(Ordering::Relaxed),
            errors_serialization: self.errors_serialization.load(Ordering::Relaxed),
            errors_backend: self.errors_backend.load(Ordering::Relaxed),
            errors_timeout: self.errors_timeout.load(Ordering::Relaxed),
            errors_other: self.errors_other.load(Ordering::Relaxed),
            errors_total: self.errors_not_found.load(Ordering::Relaxed)
                + self.errors_serialization.load(Ordering::Relaxed)
                + self.errors_backend.load(Ordering::Relaxed)
                + self.errors_timeout.load(Ordering::Relaxed)
                + self.errors_other.load(Ordering::Relaxed),

            drift_detections_total: self.drift_detections_total.load(Ordering::Relaxed),
            degraded_events_total: self.degraded_events_total.load(Ordering::Relaxed),
        }
    }

    /// Reset all metrics (useful for testing)
    pub fn reset(&self) {
        self.reads_total.store(0, Ordering::Relaxed);
        self.writes_total.store(0, Ordering::Relaxed);
        self.deletes_total.store(0, Ordering::Relaxed);
        self.scans_total.store(0, Ordering::Relaxed);
        self.index_queries_total.store(0, Ordering::Relaxed);

        self.read_latency_sum_us.store(0, Ordering::Relaxed);
        self.write_latency_sum_us.store(0, Ordering::Relaxed);
        self.delete_latency_sum_us.store(0, Ordering::Relaxed);
        self.scan_latency_sum_us.store(0, Ordering::Relaxed);

        for bucket in &self.read_latency_buckets {
            bucket.store(0, Ordering::Relaxed);
        }
        for bucket in &self.write_latency_buckets {
            bucket.store(0, Ordering::Relaxed);
        }
        for bucket in &self.delete_latency_buckets {
            bucket.store(0, Ordering::Relaxed);
        }
        for bucket in &self.scan_latency_buckets {
            bucket.store(0, Ordering::Relaxed);
        }

        self.fallback_reads_total.store(0, Ordering::Relaxed);
        self.fallback_writes_total.store(0, Ordering::Relaxed);
        self.fallback_deletes_total.store(0, Ordering::Relaxed);

        self.errors_not_found.store(0, Ordering::Relaxed);
        self.errors_serialization.store(0, Ordering::Relaxed);
        self.errors_backend.store(0, Ordering::Relaxed);
        self.errors_timeout.store(0, Ordering::Relaxed);
        self.errors_other.store(0, Ordering::Relaxed);

        self.drift_detections_total.store(0, Ordering::Relaxed);
        self.degraded_events_total.store(0, Ordering::Relaxed);
    }

    // Internal helper: increment the appropriate latency bucket
    fn increment_latency_bucket(&self, buckets: &[AtomicU64; 7], duration: Duration) {
        let millis = duration.as_millis();
        let bucket_idx = match millis {
            0..=1 => 0,     // <1ms
            2..=5 => 1,     // 1-5ms
            6..=10 => 2,    // 5-10ms
            11..=50 => 3,   // 10-50ms
            51..=100 => 4,  // 50-100ms
            101..=500 => 5, // 100-500ms
            _ => 6,         // >500ms
        };
        buckets[bucket_idx].fetch_add(1, Ordering::Relaxed);
    }

    // Internal helper: calculate average latency in milliseconds
    fn calculate_avg_latency(&self, sum_us: &AtomicU64, count: u64) -> f64 {
        if count == 0 {
            0.0
        } else {
            (sum_us.load(Ordering::Relaxed) as f64) / (count as f64) / 1000.0
        }
    }

    // Internal helper: calculate percentile from histogram buckets
    // This is a simplified approximation using bucket boundaries
    fn calculate_percentile(&self, buckets: &[AtomicU64; 7], percentile: u8) -> f64 {
        let total: u64 = buckets.iter().map(|b| b.load(Ordering::Relaxed)).sum();
        if total == 0 {
            return 0.0;
        }

        let target = (total as f64 * percentile as f64 / 100.0) as u64;
        let mut cumulative = 0u64;

        // Bucket upper bounds in milliseconds
        let bucket_bounds = [1.0, 5.0, 10.0, 50.0, 100.0, 500.0, 1000.0];

        for (idx, bucket) in buckets.iter().enumerate() {
            cumulative += bucket.load(Ordering::Relaxed);
            if cumulative >= target {
                return bucket_bounds[idx];
            }
        }

        // If we get here, return the highest bucket
        bucket_bounds[6]
    }
}

/// KV error categories for tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KvErrorType {
    /// Key not found (not always an error)
    NotFound,
    /// Serialization/deserialization error
    Serialization,
    /// Backend storage error
    Backend,
    /// Operation timeout
    Timeout,
    /// Other/unknown error
    Other,
}

/// Snapshot of KV metrics at a point in time
///
/// This structure can be serialized and exported via telemetry or REST API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvMetricsSnapshot {
    // Operation counts
    pub reads_total: u64,
    pub writes_total: u64,
    pub deletes_total: u64,
    pub scans_total: u64,
    pub index_queries_total: u64,
    pub operations_total: u64,

    // Average latencies (milliseconds)
    pub read_avg_ms: f64,
    pub write_avg_ms: f64,
    pub delete_avg_ms: f64,
    pub scan_avg_ms: f64,

    // Percentile latencies (milliseconds)
    pub read_p50_ms: f64,
    pub read_p95_ms: f64,
    pub read_p99_ms: f64,
    pub write_p50_ms: f64,
    pub write_p95_ms: f64,
    pub write_p99_ms: f64,

    // SQL fallback counts
    pub fallback_reads_total: u64,
    pub fallback_writes_total: u64,
    pub fallback_deletes_total: u64,
    pub fallback_operations_total: u64,

    // Error counts by type
    pub errors_not_found: u64,
    pub errors_serialization: u64,
    pub errors_backend: u64,
    pub errors_timeout: u64,
    pub errors_other: u64,
    pub errors_total: u64,

    // Drift/degradation
    pub drift_detections_total: u64,
    pub degraded_events_total: u64,
}

/// RAII guard for automatic KV operation timing
///
/// Usage:
/// ```ignore
/// let _timer = KvOperationTimer::new(KvOperationType::Read);
/// // ... perform KV operation ...
/// // Timer automatically records duration on drop
/// ```
pub struct KvOperationTimer {
    start: Instant,
    operation_type: KvOperationType,
}

impl KvOperationTimer {
    /// Create a new operation timer
    pub fn new(operation_type: KvOperationType) -> Self {
        Self {
            start: Instant::now(),
            operation_type,
        }
    }

    /// Manually record the operation (consumes the timer)
    pub fn record(self) {
        // Drop will handle recording
    }
}

impl Drop for KvOperationTimer {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        let metrics = global_kv_metrics();

        match self.operation_type {
            KvOperationType::Read => metrics.record_read(duration),
            KvOperationType::Write => metrics.record_write(duration),
            KvOperationType::Delete => metrics.record_delete(duration),
            KvOperationType::Scan => metrics.record_scan(duration),
            KvOperationType::IndexQuery => metrics.record_index_query(),
        }
    }
}

/// KV operation types for timing
#[derive(Debug, Clone, Copy)]
pub enum KvOperationType {
    Read,
    Write,
    Delete,
    Scan,
    IndexQuery,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    fn empty_snapshot() -> KvMetricsSnapshot {
        KvMetricsSnapshot {
            reads_total: 0,
            writes_total: 0,
            deletes_total: 0,
            scans_total: 0,
            index_queries_total: 0,
            operations_total: 0,
            read_avg_ms: 0.0,
            write_avg_ms: 0.0,
            delete_avg_ms: 0.0,
            scan_avg_ms: 0.0,
            read_p50_ms: 0.0,
            read_p95_ms: 0.0,
            read_p99_ms: 0.0,
            write_p50_ms: 0.0,
            write_p95_ms: 0.0,
            write_p99_ms: 0.0,
            fallback_reads_total: 0,
            fallback_writes_total: 0,
            fallback_deletes_total: 0,
            fallback_operations_total: 0,
            errors_not_found: 0,
            errors_serialization: 0,
            errors_backend: 0,
            errors_timeout: 0,
            errors_other: 0,
            errors_total: 0,
            drift_detections_total: 0,
            degraded_events_total: 0,
        }
    }

    #[test]
    fn test_metrics_basic_operations() {
        let metrics = KvMetrics::new();

        // Record some operations
        metrics.record_read(Duration::from_micros(500));
        metrics.record_write(Duration::from_micros(1000));
        metrics.record_delete(Duration::from_micros(300));

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.reads_total, 1);
        assert_eq!(snapshot.writes_total, 1);
        assert_eq!(snapshot.deletes_total, 1);
        assert_eq!(snapshot.operations_total, 3);
    }

    #[test]
    fn test_metrics_latency_tracking() {
        let metrics = KvMetrics::new();

        // Record operations with known latencies
        metrics.record_read(Duration::from_millis(2));
        metrics.record_read(Duration::from_millis(5));
        metrics.record_read(Duration::from_millis(10));

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.reads_total, 3);
        assert!(snapshot.read_avg_ms > 0.0);
        assert!(snapshot.read_avg_ms < 20.0); // Should be around 5-6ms
    }

    #[test]
    fn test_metrics_fallback_tracking() {
        let metrics = KvMetrics::new();

        metrics.record_fallback_read();
        metrics.record_fallback_write();
        metrics.record_fallback_write();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.fallback_reads_total, 1);
        assert_eq!(snapshot.fallback_writes_total, 2);
        assert_eq!(snapshot.fallback_operations_total, 3);
    }

    #[test]
    fn test_metrics_error_tracking() {
        let metrics = KvMetrics::new();

        metrics.record_error(KvErrorType::NotFound);
        metrics.record_error(KvErrorType::Backend);
        metrics.record_error(KvErrorType::Backend);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.errors_not_found, 1);
        assert_eq!(snapshot.errors_backend, 2);
        assert_eq!(snapshot.errors_total, 3);
    }

    #[test]
    fn test_metrics_reset() {
        let metrics = KvMetrics::new();

        metrics.record_read(Duration::from_micros(100));
        metrics.record_write(Duration::from_micros(200));
        metrics.record_fallback_read();

        let snapshot = metrics.snapshot();
        assert!(snapshot.operations_total > 0);

        metrics.reset();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.operations_total, 0);
        assert_eq!(snapshot.fallback_operations_total, 0);
        assert_eq!(snapshot.errors_total, 0);
    }

    #[test]
    fn test_operation_timer() {
        // Use the global metrics instance since the timer uses it
        let metrics = global_kv_metrics();

        // Reset metrics to start fresh
        metrics.reset();

        {
            let _timer = KvOperationTimer::new(KvOperationType::Read);
            thread::sleep(Duration::from_micros(100));
        }

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.reads_total, 1);

        // Clean up for other tests
        metrics.reset();
    }

    #[test]
    fn test_concurrent_metrics() {
        let metrics = Arc::new(KvMetrics::new());
        let mut handles = vec![];

        for _ in 0..10 {
            let m = metrics.clone();
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    m.record_read(Duration::from_micros(100));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.reads_total, 1000);
    }

    #[test]
    fn kv_alert_rules_fire_on_fallbacks_and_drift() {
        let mut engine = AlertingEngine::new(10);
        for rule in kv_alert_rules() {
            engine.register_rule(rule);
        }

        let mut snapshot = empty_snapshot();
        snapshot.operations_total = 4;
        snapshot.fallback_reads_total = 1;
        snapshot.fallback_writes_total = 1;
        snapshot.fallback_operations_total = 2;
        snapshot.errors_backend = 1;
        snapshot.errors_total = 1;
        snapshot.drift_detections_total = 1;
        snapshot.degraded_events_total = 1;

        let alerts = evaluate_kv_alerts(&snapshot, &mut engine);
        assert!(alerts.iter().any(|a| a.metric == KV_ALERT_METRIC_FALLBACKS));
        assert!(alerts.iter().any(|a| a.metric == KV_ALERT_METRIC_ERRORS));
        assert!(alerts.iter().any(|a| a.metric == KV_ALERT_METRIC_DRIFT));
        assert!(alerts
            .iter()
            .any(|a| a.metric == KV_ALERT_METRIC_DEGRADATIONS));
    }
}
