//! H8: Inference Metrics Collection
//!
//! Tracks key performance indicators for inference:
//! - Tokens/sec throughput
//! - Latency percentiles (p50, p95, p99)
//! - Adapter selection decisions
//! - Request counts and error rates

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Inference performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceMetrics {
    /// Total requests processed
    pub total_requests: u64,

    /// Successful requests
    pub successful_requests: u64,

    /// Failed requests
    pub failed_requests: u64,

    /// Total tokens generated
    pub total_tokens: u64,

    /// Tokens per second (recent average)
    pub tokens_per_second: f64,

    /// Latency p50 in milliseconds
    pub latency_p50_ms: u64,

    /// Latency p95 in milliseconds
    pub latency_p95_ms: u64,

    /// Latency p99 in milliseconds
    pub latency_p99_ms: u64,

    /// Mean latency in milliseconds
    pub latency_mean_ms: f64,

    /// Adapter selection counts
    pub adapter_selections: HashMap<String, u64>,

    /// Stop reason counts (PRD: Hard Deterministic Stop Controller)
    /// Maps stop reason code to count (e.g., "LENGTH" -> 42, "BUDGET_MAX" -> 10)
    pub stop_reason_counts: HashMap<String, u64>,

    /// Output tokens by stop reason (for computing averages)
    /// Maps stop reason code to total tokens generated when that reason triggered
    pub output_tokens_by_stop_reason: HashMap<String, u64>,

    /// Timestamp of last update
    pub last_updated: u64,
}

impl Default for InferenceMetrics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            total_tokens: 0,
            tokens_per_second: 0.0,
            latency_p50_ms: 0,
            latency_p95_ms: 0,
            latency_p99_ms: 0,
            latency_mean_ms: 0.0,
            adapter_selections: HashMap::new(),
            stop_reason_counts: HashMap::new(),
            output_tokens_by_stop_reason: HashMap::new(),
            last_updated: 0,
        }
    }
}

/// Single inference measurement
#[derive(Debug, Clone)]
pub struct InferenceMeasurement {
    /// Request ID
    pub request_id: String,

    /// Latency in milliseconds
    pub latency_ms: u64,

    /// Tokens generated
    pub tokens: u64,

    /// Whether request succeeded
    pub success: bool,

    /// Adapters selected for this request
    pub adapters: Vec<String>,

    /// Stop reason code (PRD: Hard Deterministic Stop Controller)
    /// e.g., "LENGTH", "BUDGET_MAX", "COMPLETION_CONFIDENT", "REPETITION_GUARD"
    pub stop_reason: Option<String>,

    /// Time spent waiting in queue before inference starts (microseconds)
    /// Measures the gap between request arrival and actual inference execution
    pub queue_time_us: u64,

    /// Time spent in actual token generation (microseconds)
    /// Excludes queue wait time; pure inference duration
    pub generation_time_us: u64,

    /// Worker ID that processed this request
    pub worker_id: Option<String>,

    /// Timestamp
    pub timestamp: Instant,
}

/// Metrics collector for inference operations
pub struct InferenceMetricsCollector {
    /// Recent measurements (rolling window)
    measurements: Arc<RwLock<Vec<InferenceMeasurement>>>,

    /// Maximum measurements to keep
    max_measurements: usize,

    /// Aggregated metrics
    metrics: Arc<RwLock<InferenceMetrics>>,

    /// Start time for throughput calculation
    start_time: Instant,
}

impl InferenceMetricsCollector {
    /// Create new metrics collector
    pub fn new(max_measurements: usize) -> Self {
        Self {
            measurements: Arc::new(RwLock::new(Vec::with_capacity(max_measurements))),
            max_measurements,
            metrics: Arc::new(RwLock::new(InferenceMetrics::default())),
            start_time: Instant::now(),
        }
    }

    /// Record an inference operation
    pub async fn record_inference(
        &self,
        request_id: String,
        latency: Duration,
        tokens: u64,
        success: bool,
        adapters: Vec<String>,
    ) {
        self.record_inference_with_stop(request_id, latency, tokens, success, adapters, None)
            .await;
    }

    /// Record an inference operation with stop reason (PRD: Hard Deterministic Stop Controller)
    pub async fn record_inference_with_stop(
        &self,
        request_id: String,
        latency: Duration,
        tokens: u64,
        success: bool,
        adapters: Vec<String>,
        stop_reason: Option<String>,
    ) {
        self.record_inference_with_timing(
            request_id,
            latency,
            tokens,
            success,
            adapters,
            stop_reason,
            0,
            0,
            None,
        )
        .await;
    }

    /// Record an inference operation with full timing breakdown
    ///
    /// This is the comprehensive recording method that captures queue wait time
    /// and generation time separately for performance analysis.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_inference_with_timing(
        &self,
        request_id: String,
        latency: Duration,
        tokens: u64,
        success: bool,
        adapters: Vec<String>,
        stop_reason: Option<String>,
        queue_time_us: u64,
        generation_time_us: u64,
        worker_id: Option<String>,
    ) {
        let measurement = InferenceMeasurement {
            request_id,
            latency_ms: latency.as_millis() as u64,
            tokens,
            success,
            adapters: adapters.clone(),
            stop_reason: stop_reason.clone(),
            queue_time_us,
            generation_time_us,
            worker_id,
            timestamp: Instant::now(),
        };

        // Add measurement to rolling window
        {
            let mut measurements = self.measurements.write().await;
            measurements.push(measurement.clone());

            // Maintain window size
            if measurements.len() > self.max_measurements {
                measurements.remove(0);
            }
        }

        // Update aggregated metrics
        self.update_metrics(measurement, adapters, stop_reason)
            .await;
    }

    /// Update aggregated metrics
    async fn update_metrics(
        &self,
        measurement: InferenceMeasurement,
        adapters: Vec<String>,
        stop_reason: Option<String>,
    ) {
        {
            let mut metrics = self.metrics.write().await;

            // Update counters
            metrics.total_requests += 1;
            if measurement.success {
                metrics.successful_requests += 1;
            } else {
                metrics.failed_requests += 1;
            }

            metrics.total_tokens += measurement.tokens;

            // Update adapter selections
            for adapter in adapters {
                *metrics.adapter_selections.entry(adapter).or_insert(0) += 1;
            }

            // Update stop reason counts (PRD: Hard Deterministic Stop Controller)
            if let Some(reason) = stop_reason {
                *metrics
                    .stop_reason_counts
                    .entry(reason.clone())
                    .or_insert(0) += 1;
                *metrics
                    .output_tokens_by_stop_reason
                    .entry(reason)
                    .or_insert(0) += measurement.tokens;
            }
        }

        // Recalculate percentiles and throughput (uses separate locks)
        self.recalculate_metrics().await;

        // Update last_updated timestamp after recompute
        let mut metrics = self.metrics.write().await;
        metrics.last_updated = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Recalculate percentiles and throughput from recent measurements
    async fn recalculate_metrics(&self) {
        let measurements = self.measurements.read().await;
        if measurements.is_empty() {
            return;
        }

        // Collect latencies
        let mut latencies: Vec<u64> = measurements.iter().map(|m| m.latency_ms).collect();
        latencies.sort();

        let len = latencies.len();
        let p50 = latencies[len / 2];
        let p95 = latencies[(len * 95) / 100];
        let p99 = latencies[(len * 99) / 100];
        let mean = latencies.iter().sum::<u64>() as f64 / len as f64;

        // Calculate tokens/sec
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let total_tokens = measurements.iter().map(|m| m.tokens).sum::<u64>();
        let tokens_per_second = if elapsed > 0.0 {
            total_tokens as f64 / elapsed
        } else {
            0.0
        };

        // Update metrics
        let mut metrics = self.metrics.write().await;
        metrics.latency_p50_ms = p50;
        metrics.latency_p95_ms = p95;
        metrics.latency_p99_ms = p99;
        metrics.latency_mean_ms = mean;
        metrics.tokens_per_second = tokens_per_second;
    }

    /// Get current metrics snapshot
    pub async fn get_metrics(&self) -> InferenceMetrics {
        self.metrics.read().await.clone()
    }

    /// Get recent measurements
    pub async fn get_measurements(&self, limit: usize) -> Vec<InferenceMeasurement> {
        let measurements = self.measurements.read().await;
        let start = if measurements.len() > limit {
            measurements.len() - limit
        } else {
            0
        };
        measurements[start..].to_vec()
    }

    /// Reset all metrics
    pub async fn reset(&self) {
        let mut measurements = self.measurements.write().await;
        measurements.clear();

        let mut metrics = self.metrics.write().await;
        *metrics = InferenceMetrics::default();
    }

    /// Get adapter selection statistics
    pub async fn get_adapter_stats(&self) -> HashMap<String, AdapterStats> {
        let metrics = self.metrics.read().await;

        let total = metrics.adapter_selections.values().sum::<u64>() as f64;

        metrics
            .adapter_selections
            .iter()
            .map(|(adapter_id, count)| {
                let percentage = if total > 0.0 {
                    (*count as f64 / total) * 100.0
                } else {
                    0.0
                };

                (
                    adapter_id.clone(),
                    AdapterStats {
                        selection_count: *count,
                        selection_percentage: percentage,
                    },
                )
            })
            .collect()
    }
}

/// Adapter selection statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterStats {
    /// Number of times adapter was selected
    pub selection_count: u64,

    /// Percentage of total selections
    pub selection_percentage: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    const TEST_TIMEOUT_SECS: u64 = 10;

    async fn run_with_timeout<T, F>(fut: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        timeout(Duration::from_secs(TEST_TIMEOUT_SECS), fut)
            .await
            .expect("inference_metrics test exceeded timeout")
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_metrics_collector_basic() {
        run_with_timeout(async {
            let collector = InferenceMetricsCollector::new(100);

            // Record some inferences
            collector
                .record_inference(
                    "req1".to_string(),
                    Duration::from_millis(50),
                    100,
                    true,
                    vec!["adapter1".to_string()],
                )
                .await;

            collector
                .record_inference(
                    "req2".to_string(),
                    Duration::from_millis(75),
                    150,
                    true,
                    vec!["adapter2".to_string()],
                )
                .await;

            // Get metrics
            let metrics = collector.get_metrics().await;

            assert_eq!(metrics.total_requests, 2);
            assert_eq!(metrics.successful_requests, 2);
            assert_eq!(metrics.failed_requests, 0);
            assert_eq!(metrics.total_tokens, 250);
        })
        .await;
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_metrics_percentiles() {
        run_with_timeout(async {
            let collector = InferenceMetricsCollector::new(100);

            // Record inferences with known latencies
            let latencies = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];

            for (i, latency) in latencies.iter().enumerate() {
                collector
                    .record_inference(
                        format!("req{}", i),
                        Duration::from_millis(*latency),
                        50,
                        true,
                        vec!["adapter1".to_string()],
                    )
                    .await;
            }

            let metrics = collector.get_metrics().await;

            // p50 should be around 50ms
            assert!(
                metrics.latency_p50_ms >= 40 && metrics.latency_p50_ms <= 60,
                "p50: {}",
                metrics.latency_p50_ms
            );

            // p95 should be around 95ms
            assert!(
                metrics.latency_p95_ms >= 85 && metrics.latency_p95_ms <= 100,
                "p95: {}",
                metrics.latency_p95_ms
            );

            // p99 should be around 99-100ms
            assert!(
                metrics.latency_p99_ms >= 90 && metrics.latency_p99_ms <= 100,
                "p99: {}",
                metrics.latency_p99_ms
            );
        })
        .await;
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_adapter_selection_tracking() {
        run_with_timeout(async {
            let collector = InferenceMetricsCollector::new(100);

            // Record inferences with different adapters
            for i in 0..10 {
                let adapter = if i % 2 == 0 { "adapter1" } else { "adapter2" };
                collector
                    .record_inference(
                        format!("req{}", i),
                        Duration::from_millis(50),
                        100,
                        true,
                        vec![adapter.to_string()],
                    )
                    .await;
            }

            let stats = collector.get_adapter_stats().await;

            assert_eq!(stats.len(), 2);
            assert_eq!(stats.get("adapter1").unwrap().selection_count, 5);
            assert_eq!(stats.get("adapter2").unwrap().selection_count, 5);
            assert!(
                (stats.get("adapter1").unwrap().selection_percentage - 50.0).abs() < 0.1,
                "Percentage: {}",
                stats.get("adapter1").unwrap().selection_percentage
            );
        })
        .await;
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_rolling_window() {
        run_with_timeout(async {
            let collector = InferenceMetricsCollector::new(5); // Small window

            // Record 10 inferences (should only keep last 5)
            for i in 0..10 {
                collector
                    .record_inference(
                        format!("req{}", i),
                        Duration::from_millis(50),
                        100,
                        true,
                        vec!["adapter1".to_string()],
                    )
                    .await;
            }

            let measurements = collector.get_measurements(100).await;
            assert_eq!(measurements.len(), 5, "Should only keep 5 measurements");
        })
        .await;
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_success_failure_tracking() {
        run_with_timeout(async {
            let collector = InferenceMetricsCollector::new(100);

            // Record mix of success and failures
            for i in 0..10 {
                let success = i % 3 != 0; // Fail every 3rd request
                collector
                    .record_inference(
                        format!("req{}", i),
                        Duration::from_millis(50),
                        if success { 100 } else { 0 },
                        success,
                        vec!["adapter1".to_string()],
                    )
                    .await;
            }

            let metrics = collector.get_metrics().await;

            assert_eq!(metrics.total_requests, 10);
            assert_eq!(metrics.successful_requests, 6);
            assert_eq!(metrics.failed_requests, 4);
        })
        .await;
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_tokens_per_second() {
        run_with_timeout(async {
            let collector = InferenceMetricsCollector::new(100);

            // Record inferences with known token counts
            for i in 0..10 {
                collector
                    .record_inference(
                        format!("req{}", i),
                        Duration::from_millis(1),
                        100, // 100 tokens each
                        true,
                        vec!["adapter1".to_string()],
                    )
                    .await;
            }

            let metrics = collector.get_metrics().await;

            // Should have processed 1000 tokens total
            assert_eq!(metrics.total_tokens, 1000);

            // Tokens/sec should be positive
            assert!(
                metrics.tokens_per_second > 0.0,
                "Tokens/sec: {}",
                metrics.tokens_per_second
            );
        })
        .await;
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_reset() {
        run_with_timeout(async {
            let collector = InferenceMetricsCollector::new(100);

            // Record some data
            collector
                .record_inference(
                    "req1".to_string(),
                    Duration::from_millis(10),
                    100,
                    true,
                    vec!["adapter1".to_string()],
                )
                .await;

            // Reset
            collector.reset().await;

            // Verify everything is cleared
            let metrics = collector.get_metrics().await;
            assert_eq!(metrics.total_requests, 0);
            assert_eq!(metrics.total_tokens, 0);

            let measurements = collector.get_measurements(100).await;
            assert!(measurements.is_empty());
        })
        .await;
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_stop_reason_counts_tracked() {
        run_with_timeout(async {
            let collector = InferenceMetricsCollector::new(100);

            // Record inferences with known stop reasons
            collector
                .record_inference_with_stop(
                    "req1".to_string(),
                    Duration::from_millis(50),
                    100,
                    true,
                    vec!["adapter1".to_string()],
                    Some("LENGTH".to_string()),
                )
                .await;

            collector
                .record_inference_with_stop(
                    "req2".to_string(),
                    Duration::from_millis(60),
                    50,
                    true,
                    vec!["adapter1".to_string()],
                    Some("BUDGET_MAX".to_string()),
                )
                .await;

            collector
                .record_inference_with_stop(
                    "req3".to_string(),
                    Duration::from_millis(70),
                    200,
                    true,
                    vec!["adapter1".to_string()],
                    Some("LENGTH".to_string()),
                )
                .await;

            collector
                .record_inference_with_stop(
                    "req4".to_string(),
                    Duration::from_millis(40),
                    80,
                    true,
                    vec!["adapter1".to_string()],
                    None, // no stop reason
                )
                .await;

            let metrics = collector.get_metrics().await;

            // Verify stop_reason_counts
            assert_eq!(
                metrics.stop_reason_counts.get("LENGTH"),
                Some(&2),
                "LENGTH should appear twice"
            );
            assert_eq!(
                metrics.stop_reason_counts.get("BUDGET_MAX"),
                Some(&1),
                "BUDGET_MAX should appear once"
            );
            assert_eq!(
                metrics.stop_reason_counts.get("COMPLETION_CONFIDENT"),
                None,
                "COMPLETION_CONFIDENT should not appear"
            );

            // Verify output_tokens_by_stop_reason
            assert_eq!(
                metrics.output_tokens_by_stop_reason.get("LENGTH"),
                Some(&300), // 100 + 200
                "LENGTH output tokens should sum to 300"
            );
            assert_eq!(
                metrics.output_tokens_by_stop_reason.get("BUDGET_MAX"),
                Some(&50),
                "BUDGET_MAX output tokens should be 50"
            );

            // Requests without stop reason should not appear in the maps
            assert_eq!(metrics.stop_reason_counts.len(), 2);
            assert_eq!(metrics.output_tokens_by_stop_reason.len(), 2);
        })
        .await;
    }
}
