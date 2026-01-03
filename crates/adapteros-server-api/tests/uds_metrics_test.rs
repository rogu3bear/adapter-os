//! PRD-11: UDS latency metrics verification test
//!
//! This test ensures that UDS phase timings are properly recorded
//! and can be retrieved from the metrics registry.

use adapteros_server_api::telemetry::MetricsRegistry;
use adapteros_server_api::uds_client::UdsPhaseTimings;
use adapteros_server_api::uds_metrics::{metric_names, record_uds_timings};

#[tokio::test]
async fn test_uds_metrics_are_recorded() {
    // Create a fresh metrics registry
    let registry = MetricsRegistry::new();

    // Simulate realistic UDS phase timings
    let timings = UdsPhaseTimings {
        connect_secs: 0.0005, // 0.5ms connect
        write_secs: 0.001,    // 1ms write
        read_secs: 0.025,     // 25ms read (includes inference)
    };

    // Record the timings
    record_uds_timings(
        &registry,
        &timings,
        Some("/inference"),
        Some("worker-test-001"),
        true,
    )
    .await;

    // Verify all four metrics were recorded
    let series = registry.list_series_async().await;

    // Check that RTT metric exists
    let has_rtt = series
        .iter()
        .any(|s| s.starts_with(metric_names::UDS_RTT_MS));
    assert!(
        has_rtt,
        "UDS RTT metric should be recorded. Found: {:?}",
        series
    );

    // Check that connect metric exists
    let has_connect = series
        .iter()
        .any(|s| s.starts_with(metric_names::UDS_CONNECT_MS));
    assert!(has_connect, "UDS connect metric should be recorded");

    // Check that write metric exists
    let has_write = series
        .iter()
        .any(|s| s.starts_with(metric_names::UDS_WRITE_MS));
    assert!(has_write, "UDS write metric should be recorded");

    // Check that read metric exists
    let has_read = series
        .iter()
        .any(|s| s.starts_with(metric_names::UDS_READ_MS));
    assert!(has_read, "UDS read metric should be recorded");
}

#[tokio::test]
async fn test_uds_metrics_values_are_correct() {
    let registry = MetricsRegistry::new();

    let timings = UdsPhaseTimings {
        connect_secs: 0.001, // 1ms
        write_secs: 0.002,   // 2ms
        read_secs: 0.003,    // 3ms
    };

    record_uds_timings(&registry, &timings, Some("/test"), Some("w1"), true).await;

    // Verify RTT is the sum of all phases
    let rtt_series = registry
        .get_series_async(&format!("{}._test.w1.ok", metric_names::UDS_RTT_MS))
        .await;

    assert!(rtt_series.is_some(), "RTT series should exist");
    let points = rtt_series.unwrap().get_points(None, None);
    assert!(!points.is_empty(), "RTT series should have points");

    // RTT should be approximately 6ms
    let rtt_value = points[0].value;
    assert!(
        (rtt_value - 6.0).abs() < 0.1,
        "RTT should be ~6ms, got {}",
        rtt_value
    );
}

#[tokio::test]
async fn test_uds_metrics_failure_tag() {
    let registry = MetricsRegistry::new();

    let timings = UdsPhaseTimings {
        connect_secs: 0.001,
        write_secs: 0.0,
        read_secs: 0.0,
    };

    // Record a failed connection (only connect phase completed)
    record_uds_timings(&registry, &timings, Some("/inference"), Some("w2"), false).await;

    let series = registry.list_series_async().await;

    // Should have .err suffix
    let has_err_suffix = series.iter().any(|s| s.contains(".err"));
    assert!(has_err_suffix, "Failed request should have .err suffix");
}
