//! Comprehensive error handling and recovery tests
//!
//! Tests all error types, retry mechanisms, circuit breakers, and recovery strategies

#[cfg(test)]
mod error_type_tests {
    use adapteros_lora_mlx_ffi::error::{ErrorSeverity, MlxError};

    #[test]
    fn test_error_recoverability() {
        let errors = vec![
            (
                MlxError::GpuOomError {
                    requested_mb: 100.0,
                    available_mb: 50.0,
                    hint: "test".to_string(),
                },
                true,
            ),
            (
                MlxError::ShapeMismatch {
                    expected: vec![2, 2],
                    actual: vec![3, 3],
                    context: "test".to_string(),
                },
                false,
            ),
            (
                MlxError::Timeout {
                    operation: "test".to_string(),
                    timeout_ms: 1000,
                },
                true,
            ),
            (
                MlxError::ValidationError {
                    check: "test".to_string(),
                    reason: "invalid".to_string(),
                },
                false,
            ),
        ];

        for (error, expected_recoverable) in errors {
            assert_eq!(
                error.is_recoverable(),
                expected_recoverable,
                "Error {:?} recoverability mismatch",
                error
            );
        }
    }

    #[test]
    fn test_error_severity() {
        let critical = MlxError::Internal {
            message: "test".to_string(),
        };
        assert_eq!(critical.severity(), ErrorSeverity::Critical);

        let high = MlxError::GpuOomError {
            requested_mb: 100.0,
            available_mb: 50.0,
            hint: "test".to_string(),
        };
        assert_eq!(high.severity(), ErrorSeverity::High);

        let low = MlxError::ValidationError {
            check: "test".to_string(),
            reason: "test".to_string(),
        };
        assert_eq!(low.severity(), ErrorSeverity::Low);
    }

    #[test]
    fn test_error_recovery_hints() {
        let oom = MlxError::GpuOomError {
            requested_mb: 100.0,
            available_mb: 50.0,
            hint: "Custom hint".to_string(),
        };
        assert_eq!(oom.recovery_hint(), "Custom hint");

        let timeout = MlxError::Timeout {
            operation: "test".to_string(),
            timeout_ms: 1000,
        };
        assert!(timeout.recovery_hint().contains("Retry"));
    }

    #[test]
    fn test_error_to_aos_conversion() {
        let mlx_error = MlxError::AdapterNotFound { adapter_id: 42 };
        let aos_error = mlx_error.into_aos_error();

        let error_str = format!("{:?}", aos_error);
        assert!(error_str.contains("42"));
        assert!(error_str.contains("Lifecycle"));
    }
}

#[cfg(test)]
mod retry_tests {
    use adapteros_lora_mlx_ffi::error::MlxError;
    use adapteros_lora_mlx_ffi::retry::{retry_with_backoff_sync, RetryConfig};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_retry_eventual_success() {
        let config = RetryConfig::default();
        let attempt_counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&attempt_counter);

        let result = retry_with_backoff_sync(&config, "test_op", || {
            let count = counter_clone.fetch_add(1, Ordering::SeqCst) + 1;
            if count < 2 {
                Err(MlxError::GpuOomError {
                    requested_mb: 100.0,
                    available_mb: 50.0,
                    hint: "test".to_string(),
                })
            } else {
                Ok(42)
            }
        });

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempt_counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_retry_exhausted() {
        let config = RetryConfig {
            max_attempts: 2,
            initial_backoff_ms: 10,
            max_backoff_ms: 100,
            backoff_multiplier: 2.0,
            jitter: false,
        };

        let result = retry_with_backoff_sync(&config, "test_op", || {
            Err(MlxError::GpuOomError {
                requested_mb: 100.0,
                available_mb: 50.0,
                hint: "test".to_string(),
            })
        });

        assert!(matches!(result, Err(MlxError::RetryExhausted { .. })));
    }

    #[test]
    fn test_retry_non_recoverable_immediate_fail() {
        let config = RetryConfig::default();
        let attempt_counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&attempt_counter);

        let result = retry_with_backoff_sync(&config, "test_op", || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            Err(MlxError::ValidationError {
                check: "test".to_string(),
                reason: "invalid".to_string(),
            })
        });

        // Should fail immediately without retry
        assert!(matches!(result, Err(MlxError::ValidationError { .. })));
        assert_eq!(attempt_counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_retry_config_presets() {
        let transient = RetryConfig::transient();
        assert_eq!(transient.max_attempts, 5);
        assert!(transient.jitter);

        let resource = RetryConfig::resource_exhaustion();
        assert_eq!(resource.max_attempts, 3);
        assert_eq!(resource.initial_backoff_ms, 500);

        let model = RetryConfig::model_loading();
        assert_eq!(model.max_attempts, 2);
        assert_eq!(model.max_backoff_ms, 15000);
    }

    #[test]
    fn test_backoff_calculation() {
        let config = RetryConfig {
            max_attempts: 10,
            initial_backoff_ms: 100,
            max_backoff_ms: 5000,
            backoff_multiplier: 2.0,
            jitter: false,
        };

        let backoff1 = config.backoff_duration(1);
        let backoff2 = config.backoff_duration(2);
        let backoff3 = config.backoff_duration(3);

        // Should increase exponentially
        assert!(backoff2 > backoff1);
        assert!(backoff3 > backoff2);
        assert_eq!(backoff1.as_millis(), 100);
        assert_eq!(backoff2.as_millis(), 200);
        assert_eq!(backoff3.as_millis(), 400);

        // Should clamp to max
        let backoff_large = config.backoff_duration(100);
        assert_eq!(backoff_large.as_millis(), 5000);
    }
}

#[cfg(test)]
mod circuit_breaker_tests {
    use adapteros_lora_mlx_ffi::error::MlxError;
    use adapteros_lora_mlx_ffi::retry::CircuitBreaker;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn test_circuit_breaker_opens_after_failures() {
        let breaker = CircuitBreaker::new("test_op", 3, 1000);

        // Record 3 failures
        for _ in 0..3 {
            let _: Result<(), MlxError> = breaker.call(|| {
                Err(MlxError::Internal {
                    message: "test".to_string(),
                })
            });
        }

        // Circuit should be open
        let result: Result<(), MlxError> = breaker.call(|| Ok(()));
        assert!(matches!(result, Err(MlxError::CircuitBreakerOpen { .. })));
        assert_eq!(breaker.state(), "Open");
    }

    #[test]
    fn test_circuit_breaker_half_open_transition() {
        let breaker = CircuitBreaker::new("test_op", 2, 100);

        // Open the circuit
        for _ in 0..2 {
            let _: Result<(), MlxError> = breaker.call(|| {
                Err(MlxError::Internal {
                    message: "test".to_string(),
                })
            });
        }

        assert_eq!(breaker.state(), "Open");

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(150));

        // Next call should transition to half-open and allow execution
        let result: Result<i32, MlxError> = breaker.call(|| Ok(42));
        assert_eq!(result.unwrap(), 42);
        assert_eq!(breaker.state(), "HalfOpen");
    }

    #[test]
    fn test_circuit_breaker_closes_after_recovery() {
        let breaker = CircuitBreaker::new("test_op", 2, 100);

        // Open circuit
        for _ in 0..2 {
            let _: Result<(), MlxError> = breaker.call(|| {
                Err(MlxError::Internal {
                    message: "test".to_string(),
                })
            });
        }

        // Wait for half-open
        std::thread::sleep(Duration::from_millis(150));

        // Two successes should close circuit
        let _: Result<(), MlxError> = breaker.call(|| Ok(()));
        let _: Result<(), MlxError> = breaker.call(|| Ok(()));

        assert_eq!(breaker.state(), "Closed");
    }

    #[test]
    fn test_circuit_breaker_reopens_on_half_open_failure() {
        let breaker = CircuitBreaker::new("test_op", 2, 100);

        // Open circuit
        for _ in 0..2 {
            let _: Result<(), MlxError> = breaker.call(|| {
                Err(MlxError::Internal {
                    message: "test".to_string(),
                })
            });
        }

        // Wait for half-open
        std::thread::sleep(Duration::from_millis(150));

        // Failure in half-open should reopen
        let _: Result<(), MlxError> = breaker.call(|| {
            Err(MlxError::Internal {
                message: "test".to_string(),
            })
        });

        assert_eq!(breaker.state(), "Open");
    }

    #[test]
    fn test_circuit_breaker_manual_reset() {
        let breaker = CircuitBreaker::new("test_op", 2, 1000);

        // Open circuit
        for _ in 0..2 {
            let _: Result<(), MlxError> = breaker.call(|| {
                Err(MlxError::Internal {
                    message: "test".to_string(),
                })
            });
        }

        assert_eq!(breaker.state(), "Open");

        // Manual reset
        breaker.reset();

        assert_eq!(breaker.state(), "Closed");
        assert_eq!(breaker.failure_count(), 0);
    }

    #[test]
    fn test_circuit_breaker_success_resets_failure_count() {
        let breaker = CircuitBreaker::new("test_op", 3, 1000);

        // 2 failures
        for _ in 0..2 {
            let _: Result<(), MlxError> = breaker.call(|| {
                Err(MlxError::Internal {
                    message: "test".to_string(),
                })
            });
        }

        assert_eq!(breaker.failure_count(), 2);

        // Success should reset counter
        let _: Result<(), MlxError> = breaker.call(|| Ok(()));

        assert_eq!(breaker.failure_count(), 0);
        assert_eq!(breaker.state(), "Closed");
    }
}

#[cfg(test)]
mod validation_tests {
    use adapteros_lora_mlx_ffi::validation::*;

    #[test]
    fn test_shape_validation() {
        assert!(validate_shape(&[2, 3], &[2, 3], "test").is_ok());
        assert!(validate_shape(&[2, 3], &[2, 0], "test").is_ok()); // 0 means any

        assert!(validate_shape(&[2, 3], &[3, 3], "test").is_err());
        assert!(validate_shape(&[2, 3, 4], &[2, 3], "test").is_err());
    }

    #[test]
    fn test_matmul_validation() {
        assert!(validate_matmul_shapes(&[2, 3], &[3, 4], "test").is_ok());
        assert!(validate_matmul_shapes(&[10, 20], &[20, 30], "test").is_ok());

        assert!(validate_matmul_shapes(&[2, 3], &[2, 4], "test").is_err()); // Inner mismatch
        assert!(validate_matmul_shapes(&[2], &[3, 4], "test").is_err()); // Too few dims
    }

    #[test]
    fn test_broadcast_validation() {
        assert!(validate_broadcastable(&[2, 3], &[2, 3], "test").is_ok());
        assert!(validate_broadcastable(&[1, 3], &[2, 3], "test").is_ok());
        assert!(validate_broadcastable(&[2, 1], &[2, 3], "test").is_ok());
        assert!(validate_broadcastable(&[3], &[2, 3], "test").is_ok());

        assert!(validate_broadcastable(&[2, 3], &[3, 4], "test").is_err());
    }

    #[test]
    fn test_lora_config_validation() {
        assert!(validate_lora_config(8, 16.0, 0.1).is_ok());
        assert!(validate_lora_config(128, 256.0, 0.0).is_ok());

        assert!(validate_lora_config(0, 16.0, 0.1).is_err()); // rank=0
        assert!(validate_lora_config(300, 16.0, 0.1).is_err()); // rank too high
        assert!(validate_lora_config(8, -1.0, 0.1).is_err()); // negative alpha
        assert!(validate_lora_config(8, 16.0, 1.5).is_err()); // dropout > 1
        assert!(validate_lora_config(8, 16.0, -0.1).is_err()); // negative dropout
    }

    #[test]
    fn test_gates_validation() {
        assert!(validate_gates_q15(&[16384, 8192, 32767], 3).is_ok());

        assert!(validate_gates_q15(&[16384, 8192], 3).is_err()); // Count mismatch
        assert!(validate_gates_q15(&[32768], 1).is_err()); // Exceeds Q15 max
    }

    #[test]
    fn test_adapter_id_validation() {
        assert!(validate_adapter_id(1).is_ok());
        assert!(validate_adapter_id(100).is_ok());

        assert!(validate_adapter_id(0).is_err()); // Reserved
        assert!(validate_adapter_id(2000).is_err()); // Too high
    }

    #[test]
    fn test_model_config_validation() {
        assert!(validate_model_config(768, 12, 12, 32000).is_ok());
        assert!(validate_model_config(1024, 24, 16, 50000).is_ok());

        assert!(validate_model_config(0, 12, 12, 32000).is_err()); // hidden=0
        assert!(validate_model_config(768, 0, 12, 32000).is_err()); // layers=0
        assert!(validate_model_config(768, 12, 0, 32000).is_err()); // heads=0
        assert!(validate_model_config(768, 12, 13, 32000).is_err()); // Not divisible
    }

    #[test]
    fn test_token_ids_validation() {
        assert!(validate_token_ids(&[1, 2, 3, 99], 100).is_ok());

        assert!(validate_token_ids(&[1, 100, 3], 100).is_err()); // Token 100 >= vocab 100
        assert!(validate_token_ids(&[1, 2, 1000], 100).is_err()); // Way over
    }

    #[test]
    fn test_finite_validation() {
        assert!(validate_finite(1.0, "test").is_ok());
        assert!(validate_finite(0.0, "test").is_ok());
        assert!(validate_finite(-1.0, "test").is_ok());

        assert!(validate_finite(f32::NAN, "test").is_err());
        assert!(validate_finite(f32::INFINITY, "test").is_err());
        assert!(validate_finite(f32::NEG_INFINITY, "test").is_err());
    }

    #[test]
    fn test_all_finite_validation() {
        assert!(validate_all_finite(&[1.0, 2.0, 3.0], "test").is_ok());
        assert!(validate_all_finite(&[], "test").is_ok()); // Empty is OK

        assert!(validate_all_finite(&[1.0, f32::NAN, 3.0], "test").is_err());
        assert!(validate_all_finite(&[1.0, 2.0, f32::INFINITY], "test").is_err());
    }

    #[test]
    fn test_non_empty_validation() {
        assert!(validate_non_empty(&[1, 2, 3], "test").is_ok());

        assert!(validate_non_empty::<i32>(&[], "test").is_err());
    }
}

#[cfg(test)]
mod recovery_tests {
    use adapteros_lora_mlx_ffi::recovery::{RecoveryManager, RecoveryStrategy};
    use adapteros_lora_mlx_ffi::{memory, MLXFFIModel};
    use std::time::Duration;

    #[test]
    fn test_recovery_manager_creation() {
        let manager = RecoveryManager::new(4096.0);

        let stats = manager.memory_stats();
        assert_eq!(stats.max_mb, 4096.0);
        assert_eq!(stats.target_mb, 3072.0); // 75% of max
    }

    #[test]
    fn test_lru_adapter_tracking() {
        let manager = RecoveryManager::new(4096.0);

        manager.record_adapter_access(1);
        std::thread::sleep(Duration::from_millis(10));
        manager.record_adapter_access(2);
        std::thread::sleep(Duration::from_millis(10));
        manager.record_adapter_access(3);
        std::thread::sleep(Duration::from_millis(10));
        manager.record_adapter_access(1); // Access again, should be most recent

        let lru = manager.get_lru_adapters(2);
        assert_eq!(lru.len(), 2);
        // Oldest first
        assert_eq!(lru[0], 2);
        assert_eq!(lru[1], 3);
    }

    #[test]
    fn test_memory_stats_health() {
        let manager = RecoveryManager::new(2048.0);
        let stats = manager.memory_stats();

        // Stats should be returned
        assert!(stats.current_mb >= 0.0);
        assert_eq!(stats.max_mb, 2048.0);
        assert_eq!(stats.target_mb, 1536.0);

        // Low usage should be healthy
        if stats.usage_pct < 85.0 {
            assert!(stats.is_healthy());
            assert!(!stats.needs_recovery());
        }
    }

    #[test]
    fn test_garbage_collection_recovery() {
        let manager = RecoveryManager::new(4096.0);
        memory::reset(); // Start clean

        // Try GC recovery
        let result = manager.try_garbage_collection(100.0);
        assert!(result.is_ok());

        let recovery = result.unwrap();
        assert_eq!(recovery.strategy, RecoveryStrategy::GarbageCollect);
        assert!(recovery.freed_mb >= 0.0);
    }
}

#[cfg(test)]
mod integration_tests {
    use adapteros_lora_mlx_ffi::error::MlxError;
    use adapteros_lora_mlx_ffi::recovery::RecoveryManager;
    use adapteros_lora_mlx_ffi::retry::{retry_with_backoff_sync, CircuitBreaker, RetryConfig};
    use adapteros_lora_mlx_ffi::validation;

    #[test]
    fn test_retry_with_validation() {
        let config = RetryConfig::default();

        let result = retry_with_backoff_sync(&config, "validated_op", || {
            // Validate input
            validation::validate_shape(&[2, 3], &[2, 3], "input")?;

            // Simulate operation
            Ok(42)
        });

        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_circuit_breaker_with_recovery() {
        let breaker = CircuitBreaker::new("test_op_with_recovery", 3, 100);
        let recovery = RecoveryManager::new(2048.0);

        // Simulate failures
        for _ in 0..3 {
            let _: Result<(), MlxError> = breaker.call(|| {
                Err(MlxError::GpuOomError {
                    requested_mb: 100.0,
                    available_mb: 50.0,
                    hint: "test".to_string(),
                })
            });
        }

        // Circuit should be open
        assert_eq!(breaker.state(), "Open");

        // Recovery can still be attempted independently
        let stats = recovery.memory_stats();
        assert!(stats.current_mb >= 0.0);
    }

    #[test]
    fn test_validation_chain() {
        let result = || -> Result<(), MlxError> {
            // Validate multiple aspects
            validation::validate_non_empty(&[1, 2, 3], "input")?;
            validation::validate_shape(&[2, 3], &[2, 3], "tensor")?;
            validation::validate_lora_config(8, 16.0, 0.1)?;
            validation::validate_gates_q15(&[16384], 1)?;
            Ok(())
        }();

        assert!(result.is_ok());
    }
}
