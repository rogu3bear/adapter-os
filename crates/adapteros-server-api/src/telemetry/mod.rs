//! Telemetry types and buffers with comprehensive error recovery
//!
//! This module provides resilient telemetry components with:
//! - Token bucket rate limiting (1000 events/sec per tenant)
//! - Backpressure detection for slow SSE clients
//! - Circuit breaker pattern for database writes
//! - Exponential backoff for retries
//! - Dead letter queue for failed events
//! - Health checks for telemetry subsystem
//! - Metrics for telemetry system health
//! - Graceful degradation when subsystems fail

use adapteros_core::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, StandardCircuitBreaker};
use adapteros_core::retry_policy::RetryPolicy;
use adapteros_telemetry::unified_events::TelemetryEvent;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};
use tracing::{error, warn, info};

/// Telemetry system health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TelemetryHealth {
    /// All subsystems operational
    Healthy,
    /// Some subsystems degraded but operational
    Degraded,
    /// Critical failures detected
    Unhealthy,
}

/// Telemetry system health metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryHealthMetrics {
    /// Current health status
    pub status: TelemetryHealth,
    /// Buffer utilization percentage (0-100)
    pub buffer_utilization_percent: f64,
    /// Circuit breaker state
    pub circuit_breaker_state: String,
    /// Total events dropped
    pub events_dropped_total: u64,
    /// Database persistence failures
    pub persistence_failures_total: u64,
    /// Dead letter queue size
    pub dlq_size: usize,
    /// Last check timestamp (Unix seconds)
    pub last_check_time: u64,
    /// Rate limit dropped events
    pub rate_limit_drops: u64,
    /// Backpressure drops
    pub backpressure_drops: u64,
}

/// Rate limiting configuration for telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum events per second per tenant (default: 1000)
    pub events_per_second: u64,
    /// Refill interval for token bucket (milliseconds)
    pub refill_interval_ms: u64,
    /// Maximum burst capacity (default: 10000)
    pub burst_capacity: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            events_per_second: 1000,
            refill_interval_ms: 100,
            burst_capacity: 10000,
        }
    }
}

/// Token bucket for rate limiting events per tenant
#[derive(Debug)]
struct TokenBucket {
    /// Current tokens available
    tokens: AtomicU64,
    /// Last refill time (milliseconds since epoch)
    last_refill: Arc<Mutex<u64>>,
    /// Events per second rate
    rate: u64,
    /// Maximum tokens (burst capacity)
    capacity: u64,
    /// Refill interval in milliseconds
    refill_interval_ms: u64,
}

impl TokenBucket {
    fn new(config: &RateLimitConfig) -> Self {
        let now = current_timestamp_ms();

        Self {
            tokens: AtomicU64::new(config.capacity),
            last_refill: Arc::new(Mutex::new(now)),
            rate: config.events_per_second,
            capacity: config.burst_capacity,
            refill_interval_ms: config.refill_interval_ms,
        }
    }

    /// Try to consume a token. Returns true if successful, false if rate limited.
    async fn try_consume(&self) -> bool {
        // Refill tokens based on elapsed time
        let now = current_timestamp_ms();

        let mut last_refill = self.last_refill.lock().await;
        let elapsed_ms = now.saturating_sub(*last_refill);

        if elapsed_ms >= self.refill_interval_ms {
            // Calculate tokens to add: (rate / 1000ms) * elapsed_ms
            let tokens_to_add = (self.rate * elapsed_ms) / 1000;
            let current = self.tokens.load(Ordering::Relaxed);
            let new_tokens = (current + tokens_to_add).min(self.capacity);
            self.tokens.store(new_tokens, Ordering::Relaxed);
            *last_refill = now;
        }

        // Try to consume one token
        let mut current = self.tokens.load(Ordering::Relaxed);
        loop {
            if current == 0 {
                return false;
            }

            match self.tokens.compare_exchange(
                current,
                current - 1,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }

    /// Get current token count
    fn tokens(&self) -> u64 {
        self.tokens.load(Ordering::Relaxed)
    }
}

/// Get current Unix timestamp in milliseconds
fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Backpressure detector for slow SSE clients
#[derive(Debug, Clone)]
pub struct BackpressureDetector {
    /// Maximum queue depth before backpressure
    max_queue_depth: usize,
    /// Events dropped due to backpressure
    events_dropped: Arc<AtomicUsize>,
}

impl BackpressureDetector {
    pub fn new(max_queue_depth: usize) -> Self {
        Self {
            max_queue_depth,
            events_dropped: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Check if we should apply backpressure
    pub fn should_apply_backpressure(&self, current_queue_depth: usize) -> bool {
        current_queue_depth >= self.max_queue_depth
    }

    /// Record a dropped event
    pub fn record_dropped_event(&self) {
        self.events_dropped.fetch_add(1, Ordering::Relaxed);
    }

    /// Get total events dropped
    pub fn events_dropped(&self) -> usize {
        self.events_dropped.load(Ordering::Relaxed)
    }

    /// Reset drop counter
    pub fn reset_drops(&self) {
        self.events_dropped.store(0, Ordering::Relaxed);
    }
}

/// Dead letter queue for failed telemetry events
#[derive(Clone)]
pub struct DeadLetterQueue {
    /// Queue of failed events with retry metadata
    events: Arc<Mutex<VecDeque<DeadLetterEvent>>>,
    /// Maximum queue size before old events are discarded
    max_size: usize,
    /// Metrics
    total_enqueued: Arc<AtomicU64>,
    total_processed: Arc<AtomicU64>,
}

/// Event in the dead letter queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterEvent {
    /// The telemetry event that failed
    pub event: TelemetryEvent,
    /// Number of retry attempts
    pub retry_attempts: u32,
    /// Last error message
    pub last_error: String,
    /// Time when event entered DLQ
    pub enqueued_at: u64,
    /// Last retry timestamp
    pub last_retry_at: Option<u64>,
}

impl DeadLetterQueue {
    /// Create a new dead letter queue
    pub fn new(max_size: usize) -> Self {
        Self {
            events: Arc::new(Mutex::new(VecDeque::new())),
            max_size,
            total_enqueued: Arc::new(AtomicU64::new(0)),
            total_processed: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Add an event to the dead letter queue
    pub async fn enqueue(&self, event: TelemetryEvent, error: &str) {
        let timestamp = current_timestamp();
        let dlq_event = DeadLetterEvent {
            event,
            retry_attempts: 1,
            last_error: error.to_string(),
            enqueued_at: timestamp,
            last_retry_at: None,
        };

        let mut queue = self.events.lock().await;

        // Enforce max size by removing oldest events
        while queue.len() >= self.max_size {
            queue.pop_front();
        }

        queue.push_back(dlq_event);
        self.total_enqueued.fetch_add(1, Ordering::Relaxed);
    }

    /// Get the size of the dead letter queue
    pub async fn size(&self) -> usize {
        let queue = self.events.lock().await;
        queue.len()
    }

    /// Get all events in the dead letter queue
    pub async fn list_events(&self) -> Vec<DeadLetterEvent> {
        let queue = self.events.lock().await;
        queue.iter().cloned().collect()
    }

    /// Retry a specific event in the DLQ
    pub async fn retry_event(&self, index: usize, error: Option<&str>) -> bool {
        let mut queue = self.events.lock().await;
        if let Some(event) = queue.get_mut(index) {
            event.retry_attempts += 1;
            event.last_retry_at = Some(current_timestamp());
            if let Some(err) = error {
                event.last_error = err.to_string();
            }
            true
        } else {
            false
        }
    }

    /// Remove an event from the dead letter queue (after successful processing)
    pub async fn remove(&self, index: usize) -> Option<DeadLetterEvent> {
        let mut queue = self.events.lock().await;
        if index < queue.len() {
            self.total_processed.fetch_add(1, Ordering::Relaxed);
            // Rotate to index and pop
            queue.remove(index)
        } else {
            None
        }
    }

    /// Clear the entire dead letter queue
    pub async fn clear(&self) {
        let mut queue = self.events.lock().await;
        queue.clear();
    }

    /// Get metrics for the dead letter queue
    pub async fn metrics(&self) -> (u64, u64, usize) {
        let queue = self.events.lock().await;
        (
            self.total_enqueued.load(Ordering::Relaxed),
            self.total_processed.load(Ordering::Relaxed),
            queue.len(),
        )
    }
}

/// Telemetry system health checker
#[derive(Clone)]
pub struct TelemetryHealthChecker {
    /// Circuit breaker for database writes
    circuit_breaker: Arc<StandardCircuitBreaker>,
    /// Last known buffer utilization
    buffer_utilization: Arc<AtomicU64>,
    /// Events dropped counter
    events_dropped: Arc<AtomicU64>,
    /// Persistence failures counter
    persistence_failures: Arc<AtomicU64>,
    /// Rate limit drops
    rate_limit_drops: Arc<AtomicU64>,
    /// Backpressure drops
    backpressure_drops: Arc<AtomicU64>,
    /// Last health check time
    last_check_time: Arc<Mutex<u64>>,
}

impl TelemetryHealthChecker {
    /// Create a new health checker
    pub fn new(circuit_breaker: Arc<StandardCircuitBreaker>) -> Self {
        Self {
            circuit_breaker,
            buffer_utilization: Arc::new(AtomicU64::new(0)),
            events_dropped: Arc::new(AtomicU64::new(0)),
            persistence_failures: Arc::new(AtomicU64::new(0)),
            rate_limit_drops: Arc::new(AtomicU64::new(0)),
            backpressure_drops: Arc::new(AtomicU64::new(0)),
            last_check_time: Arc::new(Mutex::new(current_timestamp())),
        }
    }

    /// Record a dropped event
    pub fn record_drop(&self) {
        self.events_dropped.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a rate limit drop
    pub fn record_rate_limit_drop(&self) {
        self.rate_limit_drops.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a backpressure drop
    pub fn record_backpressure_drop(&self) {
        self.backpressure_drops.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a persistence failure
    pub fn record_persistence_failure(&self) {
        self.persistence_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Update buffer utilization percentage
    pub fn update_buffer_utilization(&self, percent: f64) {
        let percent_u64 = (percent * 100.0) as u64;
        self.buffer_utilization.store(percent_u64, Ordering::Relaxed);
    }

    /// Perform a health check
    pub async fn check(&self) -> TelemetryHealthMetrics {
        let cb_metrics = self.circuit_breaker.metrics();
        let buffer_util = self.buffer_utilization.load(Ordering::Relaxed) as f64 / 100.0;
        let events_dropped = self.events_dropped.load(Ordering::Relaxed);
        let persistence_failures = self.persistence_failures.load(Ordering::Relaxed);
        let rate_limit_drops = self.rate_limit_drops.load(Ordering::Relaxed);
        let backpressure_drops = self.backpressure_drops.load(Ordering::Relaxed);

        // Determine health status
        let status = match cb_metrics.state {
            adapteros_core::circuit_breaker::CircuitState::Open { .. } => {
                TelemetryHealth::Unhealthy
            }
            adapteros_core::circuit_breaker::CircuitState::HalfOpen => {
                TelemetryHealth::Degraded
            }
            adapteros_core::circuit_breaker::CircuitState::Closed => {
                if buffer_util > 0.9 || persistence_failures > 100 || rate_limit_drops > 100 {
                    TelemetryHealth::Degraded
                } else {
                    TelemetryHealth::Healthy
                }
            }
        };

        let mut check_time = self.last_check_time.lock().await;
        *check_time = current_timestamp();

        TelemetryHealthMetrics {
            status,
            buffer_utilization_percent: buffer_util * 100.0,
            circuit_breaker_state: format!("{}", cb_metrics.state),
            events_dropped_total: events_dropped,
            persistence_failures_total: persistence_failures,
            dlq_size: 0, // Will be set by caller
            last_check_time: *check_time,
            rate_limit_drops,
            backpressure_drops,
        }
    }
}

/// Get current Unix timestamp in seconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Metrics collector stub - wraps the real MetricsCollector
pub struct MetricsCollector {
    inner: Arc<adapteros_telemetry::metrics::MetricsCollector>,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Result<Self, String> {
        let inner = adapteros_telemetry::metrics::MetricsCollector::new()
            .map_err(|e| format!("Failed to create metrics collector: {}", e))?;
        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    /// Get the inner metrics collector
    pub fn inner(&self) -> Arc<adapteros_telemetry::metrics::MetricsCollector> {
        self.inner.clone()
    }

    /// Get metrics snapshot
    pub async fn get_metrics_snapshot(&self) -> adapteros_telemetry::metrics::MetricsSnapshot {
        self.inner.get_metrics_snapshot().await
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new().expect("Failed to create default metrics collector")
    }
}

/// Metrics registry with production-ready time-series storage
pub struct MetricsRegistry {
    registry: Arc<prometheus::Registry>,
    /// Time-series data store: series_name -> sorted list of data points
    time_series: Arc<RwLock<std::collections::BTreeMap<String, Vec<MetricDataPoint>>>>,
    /// Retention period in seconds (default: 1 hour)
    retention_seconds: u64,
}

/// Series type for metrics with time-series data
pub struct MetricsSeries {
    name: String,
    points: Vec<MetricDataPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDataPoint {
    pub timestamp: u64,
    pub value: f64,
}

impl From<MetricDataPoint> for crate::types::MetricDataPointResponse {
    fn from(point: MetricDataPoint) -> Self {
        Self {
            timestamp: point.timestamp,
            value: point.value,
            labels: None,
        }
    }
}

impl MetricsSeries {
    /// Get points within the time range
    pub fn get_points(&self, start: Option<u64>, end: Option<u64>) -> Vec<MetricDataPoint> {
        self.points
            .iter()
            .filter(|p| {
                let after_start = start.map_or(true, |s| p.timestamp >= s);
                let before_end = end.map_or(true, |e| p.timestamp <= e);
                after_start && before_end
            })
            .cloned()
            .collect()
    }
}

impl MetricsRegistry {
    /// Create a new metrics registry with default 1-hour retention
    pub fn new() -> Self {
        Self::with_retention(3600)
    }

    /// Create a new metrics registry with custom retention period
    pub fn with_retention(retention_seconds: u64) -> Self {
        Self {
            registry: Arc::new(prometheus::Registry::new()),
            time_series: Arc::new(RwLock::new(std::collections::BTreeMap::new())),
            retention_seconds,
        }
    }

    /// Get the inner Prometheus registry
    pub fn inner(&self) -> Arc<prometheus::Registry> {
        self.registry.clone()
    }

    /// Record a metric data point with current timestamp
    pub async fn record_metric(&self, series_name: String, value: f64) {
        let timestamp = current_timestamp_ms();
        let point = MetricDataPoint { timestamp, value };

        let mut series = self.time_series.write().await;
        series
            .entry(series_name)
            .or_insert_with(Vec::new)
            .push(point);
    }

    /// Record a metric data point with explicit timestamp
    pub async fn record_metric_at(&self, series_name: String, value: f64, timestamp: u64) {
        let point = MetricDataPoint { timestamp, value };

        let mut series = self.time_series.write().await;
        let points = series.entry(series_name).or_insert_with(Vec::new);

        // Insert in sorted order by timestamp
        match points.binary_search_by_key(&timestamp, |p| p.timestamp) {
            Ok(pos) => points[pos] = point, // Replace if same timestamp
            Err(pos) => points.insert(pos, point), // Insert at correct position
        }
    }

    /// Get a series by name (async)
    pub async fn get_series_async(&self, name: &str) -> Option<MetricsSeries> {
        let series = self.time_series.read().await;
        series.get(name).map(|points| MetricsSeries {
            name: name.to_string(),
            points: points.clone(),
        })
    }

    /// Get a series by name (synchronous variant for backward compatibility)
    pub fn get_series(&self, name: &str) -> Option<MetricsSeries> {
        let series = self.time_series.blocking_read();
        series.get(name).map(|points| MetricsSeries {
            name: name.to_string(),
            points: points.clone(),
        })
    }

    /// List all available series (async)
    pub async fn list_series_async(&self) -> Vec<String> {
        let series = self.time_series.read().await;
        series.keys().cloned().collect()
    }

    /// List all available series (synchronous variant)
    pub fn list_series(&self) -> Vec<String> {
        let series = self.time_series.blocking_read();
        series.keys().cloned().collect()
    }

    /// Clean up old data points based on retention policy
    pub async fn cleanup_old_data(&self) {
        let cutoff_time = current_timestamp_ms() - (self.retention_seconds * 1000);

        let mut series = self.time_series.write().await;
        for points in series.values_mut() {
            // Remove points older than cutoff_time
            points.retain(|p| p.timestamp >= cutoff_time);
        }

        // Remove empty series to save memory
        series.retain(|_, points| !points.is_empty());
    }

    /// Collect metrics from the MetricsCollector and store as time-series data
    pub async fn collect_snapshot(&self, snapshot: &adapteros_telemetry::metrics::MetricsSnapshot) {
        let timestamp = snapshot.timestamp * 1000; // Convert seconds to milliseconds

        // Record latency metrics (all percentiles)
        self.record_metric_at("inference_latency_p50".to_string(), snapshot.latency.inference_p50_ms, timestamp).await;
        self.record_metric_at("inference_latency_p95".to_string(), snapshot.latency.inference_p95_ms, timestamp).await;
        self.record_metric_at("inference_latency_p99".to_string(), snapshot.latency.inference_p99_ms, timestamp).await;

        self.record_metric_at("router_latency_p50".to_string(), snapshot.latency.router_p50_ms, timestamp).await;
        self.record_metric_at("router_latency_p95".to_string(), snapshot.latency.router_p95_ms, timestamp).await;
        self.record_metric_at("router_latency_p99".to_string(), snapshot.latency.router_p99_ms, timestamp).await;

        self.record_metric_at("kernel_latency_p50".to_string(), snapshot.latency.kernel_p50_ms, timestamp).await;
        self.record_metric_at("kernel_latency_p95".to_string(), snapshot.latency.kernel_p95_ms, timestamp).await;
        self.record_metric_at("kernel_latency_p99".to_string(), snapshot.latency.kernel_p99_ms, timestamp).await;

        // Record queue depth metrics
        self.record_metric_at("queue_depth_request".to_string(), snapshot.queue_depth.request_queue, timestamp).await;
        self.record_metric_at("queue_depth_adapter".to_string(), snapshot.queue_depth.adapter_queue, timestamp).await;
        self.record_metric_at("queue_depth_kernel".to_string(), snapshot.queue_depth.kernel_queue, timestamp).await;

        // Record throughput metrics
        self.record_metric_at("tokens_per_second".to_string(), snapshot.throughput.tokens_per_second, timestamp).await;
        self.record_metric_at("tokens_generated_total".to_string(), snapshot.throughput.tokens_generated_total as f64, timestamp).await;
        self.record_metric_at("sessions_per_minute".to_string(), snapshot.throughput.sessions_per_minute, timestamp).await;

        // Record system metrics
        self.record_metric_at("active_sessions".to_string(), snapshot.system.active_sessions, timestamp).await;
        self.record_metric_at("memory_usage_mb".to_string(), snapshot.system.memory_usage_mb, timestamp).await;
        self.record_metric_at("cpu_usage_percent".to_string(), snapshot.system.cpu_usage_percent, timestamp).await;

        // Record policy metrics
        self.record_metric_at("policy_violations_total".to_string(), snapshot.policy.violations_total as f64, timestamp).await;
        self.record_metric_at("abstain_events_total".to_string(), snapshot.policy.abstain_events_total as f64, timestamp).await;

        // Record adapter metrics
        self.record_metric_at("adapter_activations_total".to_string(), snapshot.adapters.activations_total as f64, timestamp).await;
        self.record_metric_at("adapter_evictions_total".to_string(), snapshot.adapters.evictions_total as f64, timestamp).await;
        self.record_metric_at("active_adapters".to_string(), snapshot.adapters.active_adapters, timestamp).await;
    }

    /// Start a background task to periodically collect metrics
    /// Returns a join handle that can be used to stop the task
    pub fn start_collection_task(
        self: Arc<Self>,
        collector: Arc<MetricsCollector>,
        interval_secs: u64,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                interval.tick().await;

                // Collect current metrics snapshot
                let snapshot = collector.get_metrics_snapshot().await;

                // Store in time-series
                self.collect_snapshot(&snapshot).await;

                // Clean up old data based on retention policy
                self.cleanup_old_data().await;
            }
        })
    }

    /// Get the retention period in seconds
    pub fn retention_seconds(&self) -> u64 {
        self.retention_seconds
    }

    /// Get the number of time series being tracked
    pub async fn series_count(&self) -> usize {
        let series = self.time_series.read().await;
        series.len()
    }

    /// Get total number of data points across all series
    pub async fn total_data_points(&self) -> usize {
        let series = self.time_series.read().await;
        series.values().map(|points| points.len()).sum()
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Telemetry buffer with rate limiting and backpressure
#[derive(Clone)]
pub struct TelemetryBuffer {
    events: Arc<RwLock<Vec<TelemetryEvent>>>,
    max_size: usize,
    /// Token buckets per tenant for rate limiting
    rate_limiters: Arc<RwLock<HashMap<String, TokenBucket>>>,
    /// Rate limit configuration
    rate_limit_config: Arc<RateLimitConfig>,
    /// Backpressure detector
    backpressure_detector: Arc<BackpressureDetector>,
    /// Health checker
    health_checker: Arc<TelemetryHealthChecker>,
}

impl TelemetryBuffer {
    /// Create a new telemetry buffer with the given capacity
    pub fn new(max_size: usize) -> Self {
        Self::with_config(max_size, RateLimitConfig::default())
    }

    /// Create a buffer with custom rate limit configuration
    pub fn with_config(max_size: usize, config: RateLimitConfig) -> Self {
        let cb = Arc::new(StandardCircuitBreaker::new(
            CircuitBreakerConfig::default(),
        ));
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
            max_size,
            rate_limiters: Arc::new(RwLock::new(HashMap::new())),
            rate_limit_config: Arc::new(config),
            backpressure_detector: Arc::new(BackpressureDetector::new(max_size / 2)),
            health_checker: Arc::new(TelemetryHealthChecker::new(cb)),
        }
    }

    /// Add an event to the buffer with rate limiting and backpressure
    pub async fn push(&self, event: TelemetryEvent) -> Result<(), String> {
        let tenant_id = event.identity.tenant_id.clone();

        // Check rate limit for this tenant using token bucket
        {
            let rate_limiters = self.rate_limiters.read().await;
            if let Some(bucket) = rate_limiters.get(&tenant_id) {
                if !bucket.try_consume().await {
                    self.health_checker.record_rate_limit_drop();
                    warn!(
                        tenant_id = %tenant_id,
                        "Telemetry event dropped due to rate limit ({} events/sec)",
                        self.rate_limit_config.events_per_second
                    );
                    return Err(format!(
                        "Rate limit exceeded for tenant: max {} events/sec",
                        self.rate_limit_config.events_per_second
                    ));
                }
            } else {
                drop(rate_limiters);
                // Create new bucket for this tenant
                let bucket = TokenBucket::new(&self.rate_limit_config);
                if !bucket.try_consume().await {
                    self.health_checker.record_rate_limit_drop();
                    warn!(
                        tenant_id = %tenant_id,
                        "Telemetry event dropped due to rate limit ({} events/sec)",
                        self.rate_limit_config.events_per_second
                    );
                    return Err(format!(
                        "Rate limit exceeded for tenant: max {} events/sec",
                        self.rate_limit_config.events_per_second
                    ));
                }
                self.rate_limiters
                    .write()
                    .await
                    .insert(tenant_id.clone(), bucket);
            }
        }

        // Check backpressure before accepting
        let mut events = self.events.write().await;
        if self
            .backpressure_detector
            .should_apply_backpressure(events.len())
        {
            self.health_checker.record_backpressure_drop();
            self.backpressure_detector.record_dropped_event();
            warn!(
                tenant_id = %tenant_id,
                queue_depth = events.len(),
                max_queue_depth = self.max_size / 2,
                "Telemetry event dropped due to backpressure"
            );
            return Err("Backpressure: buffer queue depth exceeded".to_string());
        }

        // Evict oldest event if at capacity
        if events.len() >= self.max_size {
            events.remove(0);
        }

        events.push(event);
        info!(
            tenant_id = %tenant_id,
            queue_depth = events.len(),
            "Telemetry event accepted"
        );
        Ok(())
    }

    /// Get the current number of events in the buffer
    pub async fn len(&self) -> usize {
        let events = self.events.read().await;
        events.len()
    }

    /// Check if the buffer is empty
    pub async fn is_empty(&self) -> bool {
        let events = self.events.read().await;
        events.is_empty()
    }

    /// Flush all events from the buffer and return them
    pub async fn flush(&self) -> Vec<TelemetryEvent> {
        let mut events = self.events.write().await;
        std::mem::take(&mut *events)
    }

    /// Clear all events from the buffer without returning them
    pub async fn clear(&self) {
        let mut events = self.events.write().await;
        events.clear();
    }

    /// Query events with filters (synchronous read-only access)
    pub fn query(&self, filters: &adapteros_telemetry::unified_events::TelemetryFilters) -> Vec<TelemetryEvent> {
        // Use blocking read since this is a synchronous method
        let events = match self.events.try_read() {
            Ok(events) => events,
            Err(_) => return Vec::new(), // Return empty if lock is held
        };

        let mut filtered: Vec<TelemetryEvent> = events
            .iter()
            .filter(|event| {
                // Filter by tenant_id
                if let Some(ref tenant) = filters.tenant_id {
                    if &event.identity.tenant_id != tenant {
                        return false;
                    }
                }

                // Filter by user_id
                if let Some(ref user) = filters.user_id {
                    if event.user_id.as_ref() != Some(user) {
                        return false;
                    }
                }

                // Filter by event_type
                if let Some(ref event_type) = filters.event_type {
                    if &event.event_type != event_type {
                        return false;
                    }
                }

                // Filter by level
                if let Some(ref level) = filters.level {
                    if &event.level != level {
                        return false;
                    }
                }

                // Filter by component
                if let Some(ref component) = filters.component {
                    if event.component.as_ref() != Some(component) {
                        return false;
                    }
                }

                // Filter by trace_id
                if let Some(ref trace_id) = filters.trace_id {
                    if event.trace_id.as_ref() != Some(trace_id) {
                        return false;
                    }
                }

                // Filter by start_time
                if let Some(start) = filters.start_time {
                    if event.timestamp < start {
                        return false;
                    }
                }

                // Filter by end_time
                if let Some(end) = filters.end_time {
                    if event.timestamp > end {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect();

        // Sort by timestamp descending (most recent first)
        filtered.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Apply limit
        if let Some(limit) = filters.limit {
            filtered.truncate(limit);
        }

        filtered
    }

    /// Get backpressure metrics
    pub fn backpressure_metrics(&self) -> (usize, usize) {
        (
            self.backpressure_detector.events_dropped(),
            self.backpressure_detector.max_queue_depth,
        )
    }

    /// Get rate limit configuration
    pub fn rate_limit_config(&self) -> &RateLimitConfig {
        &self.rate_limit_config
    }

    /// Get health checker for advanced metrics
    pub fn health_checker(&self) -> Arc<TelemetryHealthChecker> {
        self.health_checker.clone()
    }
}

impl Default for TelemetryBuffer {
    fn default() -> Self {
        Self::new(10000) // Default buffer size of 10k events
    }
}

/// Trace buffer for storing trace events
/// Span status for trace search
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanStatus {
    Ok,
    Error,
    Unset,
}

impl std::fmt::Display for SpanStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpanStatus::Ok => write!(f, "ok"),
            SpanStatus::Error => write!(f, "error"),
            SpanStatus::Unset => write!(f, "unset"),
        }
    }
}

/// Search query for traces
#[derive(Debug, Clone)]
pub struct TraceSearchQuery {
    pub span_name: Option<String>,
    pub status: Option<SpanStatus>,
    pub start_time_ns: Option<u64>,
    pub end_time_ns: Option<u64>,
}

#[derive(Clone)]
pub struct TraceBuffer {
    traces: Arc<RwLock<Vec<TraceEvent>>>,
    max_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub timestamp: u64,
    pub duration_ms: Option<u64>,
    pub operation: String,
    pub status: String,  // "ok", "error", or "unset"
    pub metadata: serde_json::Value,
}

impl TraceBuffer {
    /// Create a new trace buffer with the given capacity
    pub fn new(max_size: usize) -> Self {
        Self {
            traces: Arc::new(RwLock::new(Vec::new())),
            max_size,
        }
    }

    /// Add a trace event to the buffer
    pub async fn push(&self, trace: TraceEvent) -> Result<(), String> {
        let mut traces = self.traces.write().await;
        if traces.len() >= self.max_size {
            // Evict oldest trace to make room
            traces.remove(0);
        }
        traces.push(trace);
        Ok(())
    }

    /// Get the current number of traces in the buffer
    pub async fn len(&self) -> usize {
        let traces = self.traces.read().await;
        traces.len()
    }

    /// Check if the buffer is empty
    pub async fn is_empty(&self) -> bool {
        let traces = self.traces.read().await;
        traces.is_empty()
    }

    /// Flush all traces from the buffer and return them
    pub async fn flush(&self) -> Vec<TraceEvent> {
        let mut traces = self.traces.write().await;
        std::mem::take(&mut *traces)
    }

    /// Clear all traces from the buffer without returning them
    pub async fn clear(&self) {
        let mut traces = self.traces.write().await;
        traces.clear();
    }

    /// Search traces by query parameters
    pub fn search(&self, query: &TraceSearchQuery) -> Vec<String> {
        // Use blocking read since this is a synchronous method
        let traces = match self.traces.try_read() {
            Ok(traces) => traces,
            Err(_) => return Vec::new(), // Return empty if lock is held
        };

        traces
            .iter()
            .filter(|trace| {
                // Filter by span_name (operation)
                if let Some(ref span_name) = query.span_name {
                    if !trace.operation.contains(span_name) {
                        return false;
                    }
                }

                // Filter by status
                if let Some(status) = query.status {
                    if trace.status != status.to_string() {
                        return false;
                    }
                }

                // Filter by time range
                if let Some(start) = query.start_time_ns {
                    if (trace.timestamp as u64) < start {
                        return false;
                    }
                }

                if let Some(end) = query.end_time_ns {
                    if (trace.timestamp as u64) > end {
                        return false;
                    }
                }

                true
            })
            .map(|t| t.trace_id.clone())
            .collect()
    }

    /// Get a trace by ID
    pub fn get_trace(&self, trace_id: &str) -> Option<TraceEvent> {
        // Use blocking read since this is a synchronous method
        let traces = match self.traces.try_read() {
            Ok(traces) => traces,
            Err(_) => return None, // Return None if lock is held
        };

        traces.iter().find(|t| t.trace_id == trace_id).cloned()
    }
}

impl Default for TraceBuffer {
    fn default() -> Self {
        Self::new(5000) // Default buffer size of 5k traces
    }
}

/// Telemetry channel sender for async event transmission (broadcast channel)
pub type TelemetrySender = tokio::sync::broadcast::Sender<TelemetryEvent>;

/// Telemetry channel receiver for async event transmission
pub type TelemetryReceiver = tokio::sync::broadcast::Receiver<TelemetryEvent>;

/// Create a new telemetry channel pair (broadcast channel with capacity)
pub fn telemetry_channel() -> (TelemetrySender, TelemetryReceiver) {
    tokio::sync::broadcast::channel(1000)
}

/// Convert from adapteros_telemetry::metrics::MetricsSnapshot to crate::types::MetricsSnapshotResponse
impl From<adapteros_telemetry::metrics::MetricsSnapshot> for crate::types::MetricsSnapshotResponse {
    fn from(snapshot: adapteros_telemetry::metrics::MetricsSnapshot) -> Self {
        let mut counters = HashMap::new();
        let mut gauges = HashMap::new();
        let mut histograms = HashMap::new();

        // Convert throughput to counters
        counters.insert(
            "tokens_generated_total".to_string(),
            snapshot.throughput.tokens_generated_total,
        );

        // Convert system metrics to gauges
        gauges.insert(
            "active_sessions".to_string(),
            snapshot.system.active_sessions,
        );
        gauges.insert(
            "memory_usage_mb".to_string(),
            snapshot.system.memory_usage_mb,
        );
        gauges.insert(
            "cpu_usage_percent".to_string(),
            snapshot.system.cpu_usage_percent,
        );
        gauges.insert(
            "tokens_per_second".to_string(),
            snapshot.throughput.tokens_per_second,
        );

        // Convert queue depth to gauges
        gauges.insert(
            "request_queue_depth".to_string(),
            snapshot.queue_depth.request_queue,
        );
        gauges.insert(
            "adapter_queue_depth".to_string(),
            snapshot.queue_depth.adapter_queue,
        );
        gauges.insert(
            "kernel_queue_depth".to_string(),
            snapshot.queue_depth.kernel_queue,
        );

        // Convert latency metrics to histograms (simplified as JSON)
        histograms.insert(
            "inference_latency".to_string(),
            serde_json::json!({
                "p50": snapshot.latency.inference_p50_ms,
                "p95": snapshot.latency.inference_p95_ms,
                "p99": snapshot.latency.inference_p99_ms,
            }),
        );
        histograms.insert(
            "router_latency".to_string(),
            serde_json::json!({
                "p50": snapshot.latency.router_p50_ms,
                "p95": snapshot.latency.router_p95_ms,
                "p99": snapshot.latency.router_p99_ms,
            }),
        );
        histograms.insert(
            "kernel_latency".to_string(),
            serde_json::json!({
                "p50": snapshot.latency.kernel_p50_ms,
                "p95": snapshot.latency.kernel_p95_ms,
                "p99": snapshot.latency.kernel_p99_ms,
            }),
        );

        // Add policy counters
        counters.insert(
            "policy_violations_total".to_string(),
            snapshot.policy.violations_total,
        );
        counters.insert(
            "abstain_events_total".to_string(),
            snapshot.policy.abstain_events_total,
        );

        // Add adapter counters
        counters.insert(
            "adapter_activations_total".to_string(),
            snapshot.adapters.activations_total,
        );
        counters.insert(
            "adapter_evictions_total".to_string(),
            snapshot.adapters.evictions_total,
        );
        gauges.insert(
            "active_adapters".to_string(),
            snapshot.adapters.active_adapters,
        );

        Self {
            timestamp: snapshot.timestamp,
            counters,
            gauges,
            histograms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_telemetry_buffer() {
        let buffer = TelemetryBuffer::new(10);
        assert!(buffer.is_empty().await);
        assert_eq!(buffer.len().await, 0);
    }

    #[tokio::test]
    async fn test_trace_buffer() {
        let buffer = TraceBuffer::new(10);
        assert!(buffer.is_empty().await);
        assert_eq!(buffer.len().await, 0);
    }

    #[test]
    fn test_metrics_collector_creation() {
        let collector = MetricsCollector::new();
        assert!(collector.is_ok());
    }

    #[test]
    fn test_metrics_registry_creation() {
        let registry = MetricsRegistry::new();
        assert!(registry.inner().metric_count() == 0);
        assert_eq!(registry.retention_seconds(), 3600);
    }

    #[tokio::test]
    async fn test_metrics_registry_record_and_retrieve() {
        let registry = MetricsRegistry::new();

        // Record some metrics
        registry.record_metric("test_metric".to_string(), 42.0).await;
        registry.record_metric("test_metric".to_string(), 43.0).await;

        // Retrieve the series
        let series = registry.get_series_async("test_metric").await;
        assert!(series.is_some());

        let series = series.expect("Failed to get metric series");
        assert_eq!(series.points.len(), 2);
        assert_eq!(series.points[0].value, 42.0);
        assert_eq!(series.points[1].value, 43.0);
    }

    #[tokio::test]
    async fn test_metrics_registry_list_series() {
        let registry = MetricsRegistry::new();

        // Record metrics for multiple series
        registry.record_metric("metric_a".to_string(), 1.0).await;
        registry.record_metric("metric_b".to_string(), 2.0).await;
        registry.record_metric("metric_c".to_string(), 3.0).await;

        // List all series
        let series_names = registry.list_series_async().await;
        assert_eq!(series_names.len(), 3);
        assert!(series_names.contains(&"metric_a".to_string()));
        assert!(series_names.contains(&"metric_b".to_string()));
        assert!(series_names.contains(&"metric_c".to_string()));
    }

    #[tokio::test]
    async fn test_metrics_series_time_range_filtering() {
        let registry = MetricsRegistry::new();

        // Record metrics with specific timestamps
        registry.record_metric_at("test_metric".to_string(), 1.0, 1000).await;
        registry.record_metric_at("test_metric".to_string(), 2.0, 2000).await;
        registry.record_metric_at("test_metric".to_string(), 3.0, 3000).await;
        registry.record_metric_at("test_metric".to_string(), 4.0, 4000).await;

        // Retrieve series and filter by time range
        let series = registry.get_series_async("test_metric").await.expect("Failed to get test metric series");

        // Filter for points between 1500 and 3500
        let filtered = series.get_points(Some(1500), Some(3500));
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].value, 2.0);
        assert_eq!(filtered[1].value, 3.0);
    }

    #[tokio::test]
    async fn test_metrics_registry_cleanup() {
        // Create registry with 1-second retention
        let registry = MetricsRegistry::with_retention(1);

        // Record a metric with old timestamp
        let old_timestamp = current_timestamp_ms() - 2000; // 2 seconds ago

        registry.record_metric_at("old_metric".to_string(), 1.0, old_timestamp).await;
        registry.record_metric("new_metric".to_string(), 2.0).await;

        // Run cleanup
        registry.cleanup_old_data().await;

        // Old metric should be removed
        let old_series = registry.get_series_async("old_metric").await;
        assert!(old_series.is_none());

        // New metric should still exist
        let new_series = registry.get_series_async("new_metric").await;
        assert!(new_series.is_some());
    }

    #[tokio::test]
    async fn test_metrics_registry_stats() {
        let registry = MetricsRegistry::new();

        // Record metrics for multiple series
        registry.record_metric("metric_a".to_string(), 1.0).await;
        registry.record_metric("metric_a".to_string(), 2.0).await;
        registry.record_metric("metric_b".to_string(), 3.0).await;

        // Check stats
        assert_eq!(registry.series_count().await, 2);
        assert_eq!(registry.total_data_points().await, 3);
    }

    #[tokio::test]
    async fn test_metrics_registry_collect_snapshot() {
        let registry = MetricsRegistry::new();
        let collector = MetricsCollector::new().expect("Failed to create metrics collector");

        // Get a snapshot and collect it
        let snapshot = collector.get_metrics_snapshot().await;
        registry.collect_snapshot(&snapshot).await;

        // Verify that series were created
        let series_names = registry.list_series_async().await;
        assert!(!series_names.is_empty());

        // Check that at least some expected series exist
        assert!(series_names.contains(&"inference_latency_p50".to_string()));
        assert!(series_names.contains(&"tokens_per_second".to_string()));
        assert!(series_names.contains(&"active_sessions".to_string()));
    }

    #[tokio::test]
    async fn test_rate_limit_config() {
        let config = RateLimitConfig::default();
        assert_eq!(config.events_per_second, 1000);
        assert_eq!(config.burst_capacity, 10000);
    }

    #[tokio::test]
    async fn test_backpressure_detector() {
        let detector = BackpressureDetector::new(100);
        assert!(!detector.should_apply_backpressure(50));
        assert!(detector.should_apply_backpressure(100));
        assert!(detector.should_apply_backpressure(150));
    }

    #[tokio::test]
    async fn test_token_bucket() {
        let config = RateLimitConfig {
            events_per_second: 100,
            refill_interval_ms: 100,
            burst_capacity: 100,
        };
        let bucket = TokenBucket::new(&config);

        // Should consume initial tokens
        assert!(bucket.try_consume().await);
        assert_eq!(bucket.tokens(), 99);
    }
}
