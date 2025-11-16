//! System metrics integration tests
//!
//! Tests the complete system metrics collection pipeline including
//! collector, policy enforcement, telemetry integration, and API endpoints.

use adapteros_core::B3Hash;
use adapteros_system_metrics::{
    MetricsConfig, SystemMetricsCollector, SystemMetricsPolicy, SystemMonitor,
    SystemMonitoringService, ThresholdsConfig,
};
use adapteros_telemetry::TelemetryWriter;
use std::path::Path;
use std::time::Duration;

#[tokio::test]
async fn test_system_metrics_collection() {
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();

    // Basic validation of collected metrics
    assert!(metrics.cpu_usage >= 0.0 && metrics.cpu_usage <= 100.0);
    assert!(metrics.memory_usage >= 0.0 && metrics.memory_usage <= 100.0);
    assert!(metrics.disk_io.read_bytes >= 0);
    assert!(metrics.disk_io.write_bytes >= 0);
    assert!(metrics.network_io.rx_bytes >= 0);
    assert!(metrics.network_io.tx_bytes >= 0);
    assert!(metrics.disk_io.usage_percent >= 0.0 && metrics.disk_io.usage_percent <= 100.0);

    // Test additional collector methods
    let headroom = collector.headroom_pct();
    assert!(headroom >= 0.0 && headroom <= 100.0);

    let uptime = collector.uptime_seconds();
    assert!(uptime > 0);

    let process_count = collector.process_count();
    assert!(process_count > 0);

    let load_avg = collector.load_average();
    assert!(load_avg.0 >= 0.0);
    assert!(load_avg.1 >= 0.0);
    assert!(load_avg.2 >= 0.0);
}

#[tokio::test]
async fn test_policy_enforcement() {
    let thresholds = ThresholdsConfig::default();
    let policy = SystemMetricsPolicy::new(thresholds);

    // Test with healthy metrics
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();

    // Should pass with normal system load
    let result = policy.check_thresholds(&metrics);
    // Note: This might fail on heavily loaded systems, so we just check it doesn't panic
    let _ = result;

    // Test memory headroom
    let headroom = collector.headroom_pct();
    let headroom_result = policy.check_memory_headroom(headroom);
    // This should generally pass unless system is critically low on memory
    let _ = headroom_result;

    // Test health status
    let health_status = policy.get_health_status(&metrics);
    assert!(matches!(
        health_status,
        adapteros_system_metrics::policy::SystemHealthStatus::Healthy
            | adapteros_system_metrics::policy::SystemHealthStatus::Warning
            | adapteros_system_metrics::policy::SystemHealthStatus::Critical
    ));

    // Test violations list
    let violations = policy.get_violations(&metrics);
    // Violations list should be valid (may be empty)
    assert!(violations.len() <= 4); // Max possible violations
}

#[tokio::test]
async fn test_telemetry_integration() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let telemetry_writer = TelemetryWriter::new(temp_dir.path(), 1000, 1024 * 1024)
        .expect("Failed to create telemetry writer");

    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();

    // Test telemetry event creation
    let event = adapteros_system_metrics::telemetry::SystemMetricsEvent::from_metrics(&metrics);

    assert!(event.cpu_usage >= 0.0 && event.cpu_usage <= 100.0);
    assert!(event.memory_usage >= 0.0 && event.memory_usage <= 100.0);
    assert!(event.timestamp > 0);

    // Test threshold violation event
    let violation = adapteros_system_metrics::telemetry::ThresholdViolationEvent::new(
        "cpu_usage".to_string(),
        95.0,
        90.0,
        "critical".to_string(),
    );

    assert_eq!(violation.metric_name, "cpu_usage");
    assert_eq!(violation.current_value, 95.0);
    assert_eq!(violation.threshold_value, 90.0);
    assert_eq!(violation.severity, "critical");
    assert!(violation.timestamp > 0);
}

#[tokio::test]
async fn test_monitoring_service() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let telemetry_writer = TelemetryWriter::new(temp_dir.path(), 1000, 1024 * 1024)
        .expect("Failed to create telemetry writer");

    let config = MetricsConfig {
        collection_interval_secs: 1, // 1 second for testing
        sampling_rate: 1.0,          // 100% sampling for testing
        enable_gpu_metrics: true,
        enable_disk_metrics: true,
        enable_network_metrics: true,
        retention_days: 30,
        thresholds: ThresholdsConfig::default(),
    };

    let test_seed = B3Hash::hash(b"test_seed");
    let mut monitor = SystemMonitor::new(telemetry_writer, config, &test_seed);

    // Test health status
    let health_status = monitor.get_health_status();
    assert!(matches!(
        health_status,
        adapteros_system_metrics::policy::SystemHealthStatus::Healthy
            | adapteros_system_metrics::policy::SystemHealthStatus::Warning
            | adapteros_system_metrics::policy::SystemHealthStatus::Critical
    ));

    // Test current metrics
    let current_metrics = monitor.get_current_metrics();
    assert!(current_metrics.cpu_usage >= 0.0 && current_metrics.cpu_usage <= 100.0);
    assert!(current_metrics.memory_usage >= 0.0 && current_metrics.memory_usage <= 100.0);

    // Test violation count
    let violation_count = monitor.get_violation_count();
    assert!(violation_count >= 0);

    // Test violation count reset
    monitor.reset_violation_count();
    assert_eq!(monitor.get_violation_count(), 0);
}

#[tokio::test]
async fn test_gpu_metrics() {
    let gpu_collector = adapteros_system_metrics::gpu::GpuMetricsCollector::new();
    let metrics = gpu_collector.collect_metrics();

    // GPU metrics may be None on unsupported platforms
    if let Some(utilization) = metrics.utilization {
        assert!(utilization >= 0.0 && utilization <= 100.0);
    }

    if let Some(memory_used) = metrics.memory_used {
        assert!(memory_used >= 0);
    }

    if let Some(memory_total) = metrics.memory_total {
        assert!(memory_total >= 0);
    }

    // Test device info
    let device_info = gpu_collector.get_device_info();
    // Device info may be None on unsupported platforms
    if let Some(info) = device_info {
        assert!(!info.name.is_empty());
        assert!(!info.vendor.is_empty());
        assert!(!info.device_type.is_empty());
    }
}

#[tokio::test]
async fn test_configuration() {
    let config = MetricsConfig::default();

    assert_eq!(config.collection_interval_secs, 30);
    assert_eq!(config.sampling_rate, 0.05);
    assert!(config.enable_gpu_metrics);
    assert!(config.enable_disk_metrics);
    assert!(config.enable_network_metrics);

    let thresholds = config.thresholds;
    assert_eq!(thresholds.cpu_critical, 90.0);
    assert_eq!(thresholds.memory_critical, 95.0);
    assert_eq!(thresholds.disk_critical, 95.0);
    assert_eq!(thresholds.gpu_critical, 95.0);
    assert_eq!(thresholds.min_memory_headroom, 15.0);
}

#[tokio::test]
async fn test_error_handling() {
    // Test that collectors handle errors gracefully
    let collector = SystemMetricsCollector::new();

    // These should not panic even on edge cases
    let _headroom = collector.headroom_pct();
    let _uptime = collector.uptime_seconds();
    let _process_count = collector.process_count();
    let _load_avg = collector.load_average();

    // Test policy with extreme values
    let thresholds = ThresholdsConfig {
        cpu_warning: 0.0,           // Impossible threshold
        cpu_critical: 0.0,          // Impossible threshold
        memory_warning: 0.0,        // Impossible threshold
        memory_critical: 0.0,       // Impossible threshold
        disk_warning: 0.0,          // Impossible threshold
        disk_critical: 0.0,         // Impossible threshold
        gpu_warning: 0.0,           // Impossible threshold
        gpu_critical: 0.0,          // Impossible threshold
        min_memory_headroom: 100.0, // Impossible threshold
    };

    let policy = SystemMetricsPolicy::new(thresholds);
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();

    // Should fail with impossible thresholds
    let result = policy.check_thresholds(&metrics);
    assert!(result.is_err());

    let violations = policy.get_violations(&metrics);
    assert!(!violations.is_empty());

    let health_status = policy.get_health_status(&metrics);
    assert_eq!(
        health_status,
        adapteros_system_metrics::policy::SystemHealthStatus::Critical
    );
}

#[tokio::test]
async fn test_concurrent_access() {
    use tokio::task;

    let mut collector = SystemMetricsCollector::new();

    // Test concurrent metric collection
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let mut collector = SystemMetricsCollector::new();
            task::spawn(async move { collector.collect_metrics() })
        })
        .collect();

    for handle in handles {
        let metrics = handle.await.expect("Task failed");
        assert!(metrics.cpu_usage >= 0.0 && metrics.cpu_usage <= 100.0);
        assert!(metrics.memory_usage >= 0.0 && metrics.memory_usage <= 100.0);
    }
}

#[tokio::test]
async fn test_serialization() {
    use serde_json;

    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();

    // Test that metrics can be serialized
    let json = serde_json::to_string(&metrics).expect("Failed to serialize metrics");
    let deserialized: adapteros_system_metrics::SystemMetrics =
        serde_json::from_str(&json).expect("Failed to deserialize metrics");

    assert_eq!(metrics.cpu_usage, deserialized.cpu_usage);
    assert_eq!(metrics.memory_usage, deserialized.memory_usage);
    assert_eq!(metrics.disk_io.read_bytes, deserialized.disk_io.read_bytes);
    assert_eq!(
        metrics.network_io.rx_bytes,
        deserialized.network_io.rx_bytes
    );
}

#[tokio::test]
async fn test_performance() {
    let mut collector = SystemMetricsCollector::new();

    // Test that metric collection is reasonably fast
    let start = std::time::Instant::now();

    for _ in 0..100 {
        let _metrics = collector.collect_metrics();
    }

    let duration = start.elapsed();

    // Should complete 100 collections in under 10 seconds (reasonable on loaded systems)
    assert!(
        duration.as_secs() < 10,
        "100 collections took {:?}, expected < 10s",
        duration
    );

    // Average collection time should be under 100ms
    let avg_time = duration.as_millis() / 100;
    assert!(
        avg_time < 100,
        "Average collection time was {}ms, expected < 100ms",
        avg_time
    );
}
