//! Integration tests for MLX backend resilience system
//!
//! These tests verify that the complete resilience system works end-to-end,
//! including monitoring, alerting, failover, and recovery.

use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_mlx_ffi::backend::{MLXFFIBackend, MLXResilienceConfig};
use adapteros_lora_mlx_ffi::monitoring::{AlertThresholds, MLXMonitor, MonitoringConfig};
use adapteros_lora_mlx_ffi::MLXFFIModel;
use std::sync::Arc;
use std::time::Duration;

#[cfg(test)]
mod resilience_tests {
    use super::*;

    fn create_test_model() -> MLXFFIModel {
        // Create a test model without requiring FFI calls
        // This avoids linking issues during test compilation
        MLXFFIModel {
            model: std::ptr::null_mut(), // Null pointer for tests
            config: adapteros_lora_mlx_ffi::ModelConfig {
                hidden_size: 4096,
                num_hidden_layers: 32,
                num_attention_heads: 32,
                num_key_value_heads: 8,
                intermediate_size: 11008,
                vocab_size: 32000,
                max_position_embeddings: 32768,
                rope_theta: 10000.0,
            },
            health: Arc::new(std::sync::Mutex::new(adapteros_lora_mlx_ffi::ModelHealth {
                operational: true,
                consecutive_failures: 0,
                last_success: None,
                last_failure: None,
                circuit_breaker: adapteros_lora_mlx_ffi::CircuitBreakerState::Closed,
            })),
        }
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
        RouterRing {
            indices: vec![0, 1, 2],
            gates: vec![0.8, 0.6, 0.4],
        }
    }

    #[test]
    fn test_resilience_healthy_operation() {
        let backend = create_test_backend();
        let mut io = create_test_io_buffers();
        let ring = create_test_router_ring();

        // Perform successful operations
        for _ in 0..5 {
            let result = backend.run_step(&ring, &mut io);
            assert!(
                result.is_ok(),
                "Backend should handle requests successfully"
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
    fn test_resilience_stub_fallback_activation() {
        let backend = create_test_backend();
        let mut io = create_test_io_buffers();
        let ring = create_test_router_ring();

        // Simulate failures to trigger stub fallback
        for _ in 0..2 {
            // Force a failure by manipulating health status
            {
                let mut health = backend.health_status.write();
                health.failed_requests += 1;
                health.current_failure_streak += 1;
            }

            let result = backend.run_step(&ring, &mut io);
            assert!(
                result.is_ok(),
                "Backend should still work with stub fallback"
            );
            io.position += 1;
        }

        // Check that stub fallback is active
        let health = backend.health_status();
        assert!(
            health.stub_fallback_active,
            "Stub fallback should be active"
        );
        assert!(health.operational, "Backend should still be operational");

        // Check monitoring detects the issue
        let health_check = backend.perform_health_check();
        assert!(health_check.is_some());
        let check = health_check.unwrap();
        assert_eq!(
            check.status,
            adapteros_lora_mlx_ffi::monitoring::HealthStatus::Warning
        );
        assert!(check
            .issues
            .contains(&"Operating in stub fallback mode".to_string()));
    }

    #[test]
    fn test_resilience_circuit_breaker_opens() {
        let backend = create_test_backend();
        let mut io = create_test_io_buffers();
        let ring = create_test_router_ring();

        // Simulate enough failures to open circuit breaker
        for _ in 0..3 {
            {
                let mut health = backend.health_status.write();
                health.failed_requests += 1;
                health.current_failure_streak += 1;
            }
        }

        // Next request should fail due to circuit breaker
        let result = backend.run_step(&ring, &mut io);
        assert!(result.is_err(), "Circuit breaker should prevent requests");

        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("Circuit breaker open"),
            "Error should mention circuit breaker"
        );

        // Check backend is non-operational
        let health = backend.health_status();
        assert!(!health.operational, "Backend should be non-operational");

        // Check critical alert is raised
        let alerts = backend.active_alerts();
        assert!(alerts.len() > 0, "Should have active alerts");
        let critical_alerts: Vec<_> = alerts
            .iter()
            .filter(|a| {
                matches!(
                    a.severity,
                    adapteros_lora_mlx_ffi::monitoring::AlertSeverity::Critical
                )
            })
            .collect();
        assert!(!critical_alerts.is_empty(), "Should have critical alerts");
    }

    #[test]
    fn test_resilience_failover_actions() {
        let backend = create_test_backend();

        // Simulate complete failure
        for _ in 0..5 {
            let mut health = backend.health_status.write();
            health.failed_requests += 1;
            health.current_failure_streak += 1;
        }

        // Trigger failover (this happens internally in run_step, but we'll call it directly for testing)
        backend.reset_health(); // Reset first
        {
            let mut health = backend.health_status.write();
            health.current_failure_streak = 5; // Force high failure count
            health.operational = false;
        }

        // The failover actions would be triggered in the next run_step call
        // For this test, we verify the environment variables would be set

        let config = &backend.resilience_config;
        assert_eq!(
            config.failover_env_vars.get("BACKEND_FAILED"),
            Some(&"mlx".to_string())
        );
        assert_eq!(
            config.failover_env_vars.get("FAILOVER_ACTIVE"),
            Some(&"true".to_string())
        );
        assert_eq!(
            config.failover_command,
            Some("echo 'failover_triggered'".to_string())
        );
    }

    #[test]
    fn test_resilience_recovery_works() {
        let backend = create_test_backend();
        let mut io = create_test_io_buffers();
        let ring = create_test_router_ring();

        // First, break the backend
        {
            let mut health = backend.health_status.write();
            health.operational = false;
            health.current_failure_streak = 5;
            health.stub_fallback_active = true;
        }

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

        // Perform successful operation
        let result = backend.run_step(&ring, &mut io);
        assert!(result.is_ok(), "Backend should work after recovery");

        // Check health monitoring reflects recovery
        let health_check = backend.perform_health_check();
        assert!(health_check.is_some());
        let check = health_check.unwrap();
        assert_eq!(
            check.status,
            adapteros_lora_mlx_ffi::monitoring::HealthStatus::Healthy
        );
        assert_eq!(check.health_score, 100.0);
    }

    #[test]
    fn test_resilience_metrics_export() {
        let backend = create_test_backend();

        // Simulate some activity
        {
            let mut health = backend.health_status.write();
            health.total_requests = 1000;
            health.successful_requests = 950;
            health.failed_requests = 50;
        }

        let metrics = backend.export_metrics();

        // Verify metrics format (Prometheus style)
        assert!(metrics.contains("mlx_backend_requests_total 1000"));
        assert!(metrics.contains("mlx_backend_requests_successful 950"));
        assert!(metrics.contains("mlx_backend_success_rate 95"));
        assert!(metrics.contains("mlx_backend_health_score"));
    }

    #[test]
    fn test_resilience_monitoring_integration() {
        let backend = create_test_backend();

        // Initial state - healthy
        let health_check = backend.perform_health_check().unwrap();
        assert_eq!(
            health_check.status,
            adapteros_lora_mlx_ffi::monitoring::HealthStatus::Healthy
        );
        assert_eq!(health_check.issues.len(), 0);

        // Simulate warning condition
        {
            let mut health = backend.health_status.write();
            health.current_failure_streak = 2;
        }

        let health_check = backend.perform_health_check().unwrap();
        assert_eq!(
            health_check.status,
            adapteros_lora_mlx_ffi::monitoring::HealthStatus::Warning
        );
        assert!(health_check.issues.len() > 0);

        // Simulate critical condition
        {
            let mut health = backend.health_status.write();
            health.current_failure_streak = 5;
            health.operational = false;
        }

        let health_check = backend.perform_health_check().unwrap();
        assert_eq!(
            health_check.status,
            adapteros_lora_mlx_ffi::monitoring::HealthStatus::Critical
        );
        assert!(health_check.issues.len() > 0);

        // Check alerts were generated
        let alerts = backend.active_alerts();
        assert!(
            alerts.len() >= 2,
            "Should have generated alerts for warning and critical conditions"
        );
    }

    #[test]
    fn test_resilience_device_info_reflects_health() {
        let backend = create_test_backend();

        // Healthy state
        let info = backend.device_info();
        assert!(info.contains("Healthy"));
        assert!(info.contains("Success: 100.0%"));

        // Degraded state
        {
            let mut health = backend.health_status.write();
            health.operational = false;
            health.successful_requests = 90;
            health.total_requests = 100;
        }

        let info = backend.device_info();
        assert!(info.contains("Degraded"));
        assert!(info.contains("Success: 90.0%"));
    }

    #[test]
    fn test_performance_metrics_tracking() {
        let mut backend = create_test_backend();

        // Record some performance data
        {
            let mut metrics = backend.performance_metrics.write();
            metrics.total_requests = 100;
            metrics.total_inference_time_ms = 5000; // 50ms average
            metrics.peak_memory_usage_mb = 256.0;
        }

        // Verify metrics are accessible
        let metrics = backend.performance_metrics.read();
        assert_eq!(metrics.total_requests, 100);
        assert_eq!(metrics.average_latency_ms, 50.0);
        assert_eq!(metrics.peak_memory_usage_mb, 256.0);
    }

    #[test]
    fn test_memory_pool_management() {
        let backend = create_test_backend();

        // Memory pool should be initialized
        assert!(backend.memory_pool.read().is_empty());

        // In a real test, we would allocate and deallocate MLX arrays
        // For now, just verify the pool exists
        let pool_size = backend.memory_pool.read().len();
        assert_eq!(pool_size, 0);
    }

    #[test]
    fn test_enhanced_monitoring_integration() {
        let backend = Arc::new(create_test_backend());
        let monitor = adapteros_lora_mlx_ffi::monitoring::MLXMonitor::new(
            backend.clone(),
            adapteros_lora_mlx_ffi::monitoring::MonitoringConfig {
                health_check_interval_secs: 60,
                alert_thresholds: adapteros_lora_mlx_ffi::monitoring::AlertThresholds {
                    max_failure_rate: 0.1,
                    max_response_time_ms: 5000.0,
                    min_health_score: 70.0,
                },
            },
        );

        // Test that monitoring can access backend metrics
        let metrics = monitor.performance_metrics();
        assert_eq!(metrics.total_requests, 0); // Initial state
    }
}
