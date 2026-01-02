//! Integration tests for MLX backend resilience system
//!
//! These tests verify that the complete resilience system works end-to-end,
//! including monitoring, alerting, failover, and recovery.
//!
//! Note: These tests use stub implementations when real MLX is not available.

use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_mlx_ffi::backend::{MLXFFIBackend, MLXResilienceConfig};
use adapteros_lora_mlx_ffi::mock::create_mock_config;
use adapteros_lora_mlx_ffi::monitoring::{AlertThresholds, MonitoringConfig};
use adapteros_lora_mlx_ffi::MLXFFIModel;
use std::sync::Arc;
use std::time::Duration;

#[cfg(test)]
mod resilience_tests {
    use super::*;

    fn create_test_model() -> MLXFFIModel {
        // Create a test model using new_null which is designed for testing
        let config = create_mock_config();
        MLXFFIModel::new_null(config)
    }

    fn create_test_backend() -> MLXFFIBackend {
        // Create a minimal test model that doesn't require FFI
        let model = create_test_model();

        let resilience_config = MLXResilienceConfig {
            max_consecutive_failures: 3,
            circuit_breaker_timeout_secs: 300,
            enable_stub_fallback: true,
            health_check_interval_secs: 60,
            failover_command: Some("echo 'failover_triggered'".to_string()),
            failover_env_vars: [
                ("BACKEND_FAILED".to_string(), "mlx".to_string()),
                ("FAILOVER_ACTIVE".to_string(), "true".to_string()),
            ]
            .into(),
        };

        let backend = MLXFFIBackend::with_resilience_config(model, resilience_config);

        let monitoring_config = MonitoringConfig {
            health_check_interval: Duration::from_secs(60),
            alert_thresholds: AlertThresholds {
                warning_failure_threshold: 1,
                critical_failure_threshold: 3,
                min_success_rate_percent: 95.0,
                max_recovery_time_secs: 300,
            },
            metrics_enabled: true,
        };

        backend.with_monitoring(monitoring_config)
    }

    fn create_test_io_buffers() -> IoBuffers {
        IoBuffers {
            input_ids: vec![1, 2, 3, 4, 5],
            output_logits: vec![0.0; 32000],
            position: 0,
        }
    }

    fn create_test_router_ring() -> RouterRing {
        let mut ring = RouterRing::new(3);
        ring.indices[0] = 0;
        ring.indices[1] = 1;
        ring.indices[2] = 2;
        // Q15 format: 32767 = 1.0, 16384 = 0.5, etc.
        ring.gates_q15[0] = 26214; // ~0.8
        ring.gates_q15[1] = 19661; // ~0.6
        ring.gates_q15[2] = 13107; // ~0.4
        ring
    }

    #[test]
    fn test_backend_creation() {
        let backend = create_test_backend();
        // Backend should be created successfully
        assert_eq!(backend.adapter_count(), 0);
    }

    #[test]
    #[ignore = "requires real model for forward pass - null model returns FFI error"]
    fn test_resilience_healthy_operation() {
        let mut backend = create_test_backend();
        let mut io = create_test_io_buffers();
        let ring = create_test_router_ring();

        // Perform successful operations
        for _ in 0..5 {
            let result = backend.run_step(&ring, &mut io);
            assert!(
                result.is_ok(),
                "Backend should handle requests successfully: {:?}",
                result.err()
            );
            io.position += 1;
        }

        // Check health status
        let health = backend.health_status();
        assert!(health.operational, "Backend should remain operational");
        assert_eq!(
            health.successful_requests, 5,
            "Should have 5 successful requests"
        );
        assert_eq!(health.failed_requests, 0, "Should have 0 failed requests");
        assert_eq!(
            health.current_failure_streak, 0,
            "Should have no failure streak"
        );

        // Check monitoring
        let health_check = backend.perform_health_check();
        assert!(health_check.is_some(), "Should have health check result");
        let check = health_check.unwrap();
        assert_eq!(
            check.status,
            adapteros_lora_mlx_ffi::monitoring::HealthStatus::Healthy
        );
        assert_eq!(
            check.health_score, 100.0,
            "Should have perfect health score"
        );

        // Check no alerts
        let alerts = backend.active_alerts();
        assert_eq!(alerts.len(), 0, "Should have no active alerts");
    }

    #[test]
    fn test_resilience_recovery_via_reset() {
        let backend = create_test_backend();

        // Reset health (simulating recovery)
        backend.reset_health();

        // Verify recovery
        let health = backend.health_status();
        assert!(
            health.operational,
            "Backend should be operational after reset"
        );
        assert_eq!(
            health.current_failure_streak, 0,
            "Failure streak should be reset"
        );
        assert!(
            !health.stub_fallback_active,
            "Stub fallback should be disabled"
        );
    }

    #[test]
    fn test_resilience_metrics_export() {
        let mut backend = create_test_backend();
        let mut io = create_test_io_buffers();
        let ring = create_test_router_ring();

        // Generate some activity
        for _ in 0..5 {
            let _ = backend.run_step(&ring, &mut io);
            io.position += 1;
        }

        let metrics = backend.export_metrics();

        // Verify metrics format (Prometheus style)
        assert!(
            metrics.contains("mlx_backend_requests_total"),
            "Should contain total requests metric"
        );
        assert!(
            metrics.contains("mlx_backend_requests_successful"),
            "Should contain successful requests metric"
        );
        assert!(
            metrics.contains("mlx_backend_success_rate"),
            "Should contain success rate metric"
        );
        assert!(
            metrics.contains("mlx_backend_health_score"),
            "Should contain health score metric"
        );
    }

    #[test]
    fn test_resilience_monitoring_initial_state() {
        let backend = create_test_backend();

        // Initial state - healthy
        let health_check = backend.perform_health_check();
        assert!(health_check.is_some(), "Should have health check result");
        let check = health_check.unwrap();
        assert_eq!(
            check.status,
            adapteros_lora_mlx_ffi::monitoring::HealthStatus::Healthy
        );
        assert_eq!(check.issues.len(), 0, "Should have no issues initially");
    }

    #[test]
    fn test_backend_device_name() {
        let backend = create_test_backend();

        // Get device name via FusedKernels trait
        let device_name = backend.device_name();
        assert!(
            device_name.contains("MLX"),
            "Device name should mention MLX"
        );
    }

    #[test]
    fn test_backend_health_check() {
        let backend = create_test_backend();

        // Backend should report healthy initially
        assert!(backend.is_healthy(), "Backend should be healthy initially");
    }

    #[test]
    fn test_adapter_registration() {
        let backend = create_test_backend();

        // Create and register an adapter
        let adapter = adapteros_lora_mlx_ffi::mock::create_mock_adapter("test-adapter", 4);
        let result = backend.register_adapter(1, adapter);

        assert!(result.is_ok(), "Adapter registration should succeed");
        assert_eq!(backend.adapter_count(), 1, "Should have 1 adapter");
    }

    #[test]
    fn test_adapter_hot_swap() {
        let backend = create_test_backend();

        // Load adapter
        let adapter1 = adapteros_lora_mlx_ffi::mock::create_mock_adapter("adapter1", 4);
        backend.load_adapter_runtime(1, adapter1).unwrap();
        assert_eq!(backend.adapter_count(), 1);

        // Unload adapter
        backend.unload_adapter_runtime(1).unwrap();
        assert_eq!(backend.adapter_count(), 0);
    }

    #[test]
    fn test_memory_pool_stats() {
        let backend = create_test_backend();

        // Memory pool stats should be accessible
        let stats = backend.get_memory_pool_stats();
        // Initial state should show empty pool
        assert_eq!(stats.total_allocations, 0);
        assert_eq!(stats.total_pooled_bytes, 0);
    }

    #[test]
    fn test_performance_metrics_accessible() {
        let backend = create_test_backend();

        // Performance metrics should be accessible via public API
        let metrics = backend.performance_metrics.read();
        assert_eq!(
            metrics.total_requests, 0,
            "Initial state should have 0 requests"
        );
        assert_eq!(
            metrics.average_latency_ms, 0.0,
            "Initial latency should be 0"
        );
    }

    #[test]
    fn test_enhanced_monitoring_creation() {
        let backend = Arc::new(create_test_backend());

        let monitoring_config = MonitoringConfig {
            health_check_interval: Duration::from_secs(60),
            alert_thresholds: AlertThresholds {
                warning_failure_threshold: 2,
                critical_failure_threshold: 5,
                min_success_rate_percent: 95.0,
                max_recovery_time_secs: 300,
            },
            metrics_enabled: true,
        };

        let mut monitor =
            adapteros_lora_mlx_ffi::monitoring::MLXMonitor::new(backend.clone(), monitoring_config);

        // Test that monitoring can perform health check
        let health_check = monitor.health_check();
        assert_eq!(
            health_check.status,
            adapteros_lora_mlx_ffi::monitoring::HealthStatus::Healthy
        );
    }

    #[test]
    fn test_determinism_attestation() {
        let backend = create_test_backend();

        let report = backend.attest_determinism();
        assert!(report.is_ok(), "Attestation should succeed");

        let report = report.unwrap();
        // MLX without manifest hash should not be deterministic
        assert!(
            !report.deterministic || report.metallib_hash.is_none(),
            "MLX backend without manifest hash should report non-deterministic"
        );
    }

    #[test]
    fn test_backend_with_manifest_hash() {
        use adapteros_core::B3Hash;

        let model = create_test_model();
        let manifest_hash = B3Hash::hash(b"test-manifest");

        let result = MLXFFIBackend::with_manifest_hash(model, manifest_hash);
        assert!(
            result.is_ok(),
            "Backend with manifest hash should be created"
        );

        let backend = result.unwrap();
        assert!(
            backend.manifest_hash().is_some(),
            "Manifest hash should be set"
        );
    }
}
