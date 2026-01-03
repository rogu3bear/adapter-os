//! UDS (Unix Domain Socket) latency metrics for PRD-11 observability.
//!
//! This module provides explicit metrics for UDS communication phases:
//! - `uds.connect_ms`: Time to establish the Unix socket connection
//! - `uds.write_ms`: Time to send the request over the socket
//! - `uds.read_ms`: Time to receive the response (includes worker inference time)
//! - `uds.rtt_ms`: Total round-trip time
//!
//! These metrics are tagged with:
//! - `endpoint`: The UDS endpoint path
//! - `worker_id`: Worker identifier (if available)
//! - `success`: Whether the operation succeeded

use crate::telemetry::MetricsRegistry;
use crate::uds_client::UdsPhaseTimings;
use std::sync::Arc;

/// Metric name constants for UDS phase timings
pub mod metric_names {
    pub const UDS_CONNECT_MS: &str = "uds.connect_ms";
    pub const UDS_WRITE_MS: &str = "uds.write_ms";
    pub const UDS_READ_MS: &str = "uds.read_ms";
    pub const UDS_RTT_MS: &str = "uds.rtt_ms";
}

/// Record UDS phase timings to the metrics registry.
///
/// This function records all four UDS metrics for a single request:
/// - connect, write, read phase timings
/// - total round-trip time
///
/// # Arguments
/// * `registry` - The metrics registry to record to
/// * `timings` - The UDS phase timings captured during the request
/// * `endpoint` - Optional endpoint path for tagging (e.g., "/inference")
/// * `worker_id` - Optional worker ID for tagging
/// * `success` - Whether the operation succeeded
pub async fn record_uds_timings(
    registry: &MetricsRegistry,
    timings: &UdsPhaseTimings,
    endpoint: Option<&str>,
    worker_id: Option<&str>,
    success: bool,
) {
    let suffix = format_metric_suffix(endpoint, worker_id, success);

    // Record individual phase metrics (in milliseconds)
    let connect_ms = timings.connect_secs * 1000.0;
    let write_ms = timings.write_secs * 1000.0;
    let read_ms = timings.read_secs * 1000.0;
    let rtt_ms = timings.total_ms() as f64;

    registry
        .record_metric(
            format!("{}{}", metric_names::UDS_CONNECT_MS, suffix),
            connect_ms,
        )
        .await;
    registry
        .record_metric(
            format!("{}{}", metric_names::UDS_WRITE_MS, suffix),
            write_ms,
        )
        .await;
    registry
        .record_metric(format!("{}{}", metric_names::UDS_READ_MS, suffix), read_ms)
        .await;
    registry
        .record_metric(format!("{}{}", metric_names::UDS_RTT_MS, suffix), rtt_ms)
        .await;

    // Also log for observability
    tracing::debug!(
        target: "uds_metrics",
        connect_ms = connect_ms,
        write_ms = write_ms,
        read_ms = read_ms,
        rtt_ms = rtt_ms,
        endpoint = endpoint.unwrap_or("unknown"),
        worker_id = worker_id.unwrap_or("unknown"),
        success = success,
        "UDS phase timings recorded"
    );
}

/// Format metric suffix with labels
fn format_metric_suffix(endpoint: Option<&str>, worker_id: Option<&str>, success: bool) -> String {
    let endpoint_label = endpoint.unwrap_or("unknown").replace('/', "_");
    let worker_label = worker_id.unwrap_or("unknown");
    let success_label = if success { "ok" } else { "err" };
    format!(".{}.{}.{}", endpoint_label, worker_label, success_label)
}

/// Convenience wrapper for recording timings from an Arc<MetricsRegistry>
pub async fn record_uds_timings_arc(
    registry: &Arc<MetricsRegistry>,
    timings: &UdsPhaseTimings,
    endpoint: Option<&str>,
    worker_id: Option<&str>,
    success: bool,
) {
    record_uds_timings(registry.as_ref(), timings, endpoint, worker_id, success).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_uds_timings() {
        let registry = MetricsRegistry::new();

        let timings = UdsPhaseTimings {
            connect_secs: 0.001,
            write_secs: 0.002,
            read_secs: 0.050,
        };

        record_uds_timings(
            &registry,
            &timings,
            Some("/inference"),
            Some("worker-001"),
            true,
        )
        .await;

        // Verify metrics were recorded
        let series_list = registry.list_series_async().await;
        assert!(
            series_list.iter().any(|s| s.contains("uds.rtt_ms")),
            "Expected uds.rtt_ms metric to be recorded, got: {:?}",
            series_list
        );
        assert!(
            series_list.iter().any(|s| s.contains("uds.connect_ms")),
            "Expected uds.connect_ms metric to be recorded"
        );
    }

    #[tokio::test]
    async fn test_uds_phase_timings_total() {
        let timings = UdsPhaseTimings {
            connect_secs: 0.001,
            write_secs: 0.002,
            read_secs: 0.003,
        };

        assert!((timings.total_secs() - 0.006).abs() < 0.0001);
        assert_eq!(timings.total_ms(), 6);
    }

    #[tokio::test]
    async fn test_metric_suffix_formatting() {
        let suffix = format_metric_suffix(Some("/inference"), Some("worker-001"), true);
        assert_eq!(suffix, "._inference.worker-001.ok");

        let suffix_fail = format_metric_suffix(None, None, false);
        assert_eq!(suffix_fail, ".unknown.unknown.err");
    }
}
