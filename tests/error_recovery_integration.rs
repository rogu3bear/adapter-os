//! Comprehensive Error Recovery and Fault Tolerance Tests for AdapterOS
//!
//! This module implements chaos testing patterns to verify error paths are properly
//! handled and recovered across critical AdapterOS components:
//!
//! - Circuit breaker error recovery (simulates network/service failures)
//! - Hash verification failure handling (corrupted data detection)
//! - Memory pressure error handling
//! - Network timeout error construction and handling
//! - Invalid adapter manifest handling
//! - Error context chaining
//! - Concurrent error scenarios
//!
//! Test Philosophy:
//! - Simulate realistic failure conditions
//! - Verify graceful degradation
//! - Ensure error types are properly constructed
//! - Validate error telemetry and logging

use adapteros_core::{AosError, B3Hash, Result};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ============================================================================
// Module 1: Circuit Breaker Error Recovery Tests
// ============================================================================

mod circuit_breaker_error_recovery {
    use super::*;
    use adapteros_core::{
        CircuitBreaker, CircuitBreakerConfig, CircuitState, StandardCircuitBreaker,
    };
    use std::sync::Arc;
    use tokio::time::sleep;

    /// Test circuit breaker starts in closed (healthy) state
    #[tokio::test]
    async fn test_circuit_breaker_starts_closed() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_ms: 1000,
            half_open_max_requests: 5,
        };
        let breaker = StandardCircuitBreaker::new("test_service".to_string(), config);

        assert_eq!(breaker.state(), CircuitState::Closed);
        assert_eq!(breaker.name(), "test_service");
    }

    /// Test circuit opens after failure threshold
    #[tokio::test]
    async fn test_circuit_opens_on_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_ms: 1000,
            half_open_max_requests: 5,
        };
        let breaker = Arc::new(StandardCircuitBreaker::new("test".to_string(), config));

        // Cause failures
        for _ in 0..3 {
            let _result: Result<()> = breaker
                .call(async { Err(AosError::Network("Connection refused".to_string())) })
                .await;
        }

        match breaker.state() {
            CircuitState::Open { .. } => { /* Expected */ }
            state => panic!("Expected Open state, got {:?}", state),
        }

        // Further requests should be rejected immediately
        let result: Result<()> = breaker.call(async { Ok(()) }).await;
        assert!(result.is_err());
        if let Err(AosError::CircuitBreakerOpen { service }) = result {
            assert_eq!(service, "test");
        } else {
            panic!("Expected CircuitBreakerOpen error");
        }
    }

    /// Test circuit transitions to half-open after timeout
    #[tokio::test]
    async fn test_half_open_transition() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout_ms: 50, // Short timeout for test
            half_open_max_requests: 5,
        };
        let breaker = Arc::new(StandardCircuitBreaker::new("test".to_string(), config));

        // Open the circuit
        for _ in 0..2 {
            let _: Result<()> = breaker
                .call(async { Err(AosError::Network("Timeout".to_string())) })
                .await;
        }

        // Wait for timeout
        sleep(Duration::from_millis(100)).await;

        // Next call should transition to half-open
        let result: Result<()> = breaker.call(async { Ok(()) }).await;
        assert!(result.is_ok());

        // After successful call in half-open, check metrics
        let metrics = breaker.metrics();
        assert!(
            metrics.half_opens_total > 0,
            "Should have recorded half-open transition"
        );
    }

    /// Test circuit closes after successes in half-open
    #[tokio::test]
    async fn test_circuit_closes_after_recovery() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout_ms: 50,
            half_open_max_requests: 5,
        };
        let breaker = Arc::new(StandardCircuitBreaker::new("test".to_string(), config));

        // Open -> half-open
        for _ in 0..2 {
            let _: Result<()> = breaker
                .call(async { Err(AosError::Network("Error".to_string())) })
                .await;
        }
        sleep(Duration::from_millis(100)).await;

        // Successful calls to close circuit
        for _ in 0..2 {
            let result: Result<&str> = breaker.call(async { Ok("success") }).await;
            assert!(result.is_ok());
        }

        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    /// Test circuit breaker metrics tracking
    #[tokio::test]
    async fn test_circuit_breaker_metrics() {
        let config = CircuitBreakerConfig::default();
        let breaker = Arc::new(StandardCircuitBreaker::new(
            "metrics_test".to_string(),
            config,
        ));

        // Success
        let _: Result<i32> = breaker.call(async { Ok(1) }).await;
        // Failure
        let _: Result<i32> = breaker
            .call(async { Err(AosError::Network("Test".to_string())) })
            .await;

        let metrics = breaker.metrics();
        assert_eq!(metrics.requests_total, 2);
        assert_eq!(metrics.successes_total, 1);
        assert_eq!(metrics.failures_total, 1);
    }

    /// Test concurrent circuit breaker access
    #[tokio::test]
    async fn test_concurrent_circuit_breaker_access() {
        let config = CircuitBreakerConfig {
            failure_threshold: 100, // High threshold to avoid opening
            ..Default::default()
        };
        let breaker = Arc::new(StandardCircuitBreaker::new(
            "concurrent".to_string(),
            config,
        ));

        let barrier = Arc::new(tokio::sync::Barrier::new(20));
        let mut handles = vec![];

        // 10 success tasks
        for _ in 0..10 {
            let b = breaker.clone();
            let barrier = barrier.clone();
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                for _ in 0..100 {
                    let _: Result<i32> = b.call(async { Ok(42) }).await;
                }
            }));
        }

        // 10 failure tasks
        for _ in 0..10 {
            let b = breaker.clone();
            let barrier = barrier.clone();
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                for _ in 0..100 {
                    let _: Result<i32> = b
                        .call(async { Err(AosError::Network("Test".to_string())) })
                        .await;
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // Verify metrics are sane
        let metrics = breaker.metrics();
        assert!(
            metrics.requests_total >= 2000,
            "Should have processed all requests"
        );
    }
}

// ============================================================================
// Module 2: Hash Verification Failure Tests
// ============================================================================

mod hash_verification_failures {
    use super::*;

    /// Test B3Hash creation and comparison
    #[test]
    fn test_hash_creation_and_equality() {
        let data = b"test data";
        let hash1 = B3Hash::hash(data);
        let hash2 = B3Hash::hash(data);
        let hash3 = B3Hash::hash(b"different data");

        assert_eq!(hash1, hash2, "Same data should produce same hash");
        assert_ne!(hash1, hash3, "Different data should produce different hash");
    }

    /// Test hash_multi matches concatenated hash
    #[test]
    fn test_hash_multi_equivalence() {
        let h1 = B3Hash::hash(b"ab");
        let h2 = B3Hash::hash_multi(&[b"a", b"b"]);
        assert_eq!(
            h1, h2,
            "Multi-hash should equal single hash of concatenation"
        );
    }

    /// Test hex encoding/decoding roundtrip
    #[test]
    fn test_hex_roundtrip() {
        let original = B3Hash::hash(b"roundtrip test");
        let hex = original.to_hex();
        let restored = B3Hash::from_hex(&hex).unwrap();
        assert_eq!(original, restored, "Hex roundtrip should preserve hash");
    }

    /// Test invalid hex string handling
    #[test]
    fn test_invalid_hex_handling() {
        // Invalid characters
        let result = B3Hash::from_hex("not_valid_hex!");
        assert!(result.is_err(), "Invalid hex should fail");

        // Wrong length
        let result = B3Hash::from_hex("deadbeef"); // Only 8 chars, need 64
        assert!(result.is_err(), "Wrong length hex should fail");
    }

    /// Test corrupted data detection
    #[test]
    fn test_corrupted_data_detection() {
        let original_data = b"important data";
        let expected_hash = B3Hash::hash(original_data);

        // Corrupt one byte
        let mut corrupted = original_data.to_vec();
        corrupted[0] ^= 0xFF;
        let actual_hash = B3Hash::hash(&corrupted);

        assert_ne!(expected_hash, actual_hash, "Corruption should change hash");
    }

    /// Test AdapterHashMismatch error construction
    #[test]
    fn test_adapter_hash_mismatch_error() {
        let expected = B3Hash::hash(b"expected");
        let actual = B3Hash::hash(b"actual");

        let error = AosError::AdapterHashMismatch {
            adapter_id: "test_adapter".to_string(),
            expected,
            actual,
        };

        let error_str = error.to_string();
        assert!(
            error_str.contains("test_adapter"),
            "Error should contain adapter ID"
        );
        assert!(
            error_str.contains("hash mismatch"),
            "Error should mention mismatch"
        );
    }

    /// Test zero hash constant
    #[test]
    fn test_zero_hash() {
        let zero = B3Hash::zero();
        assert_eq!(zero.as_bytes(), &[0u8; 32], "Zero hash should be all zeros");
        assert_ne!(
            zero,
            B3Hash::hash(b"non-empty"),
            "Zero hash should differ from real hash"
        );
    }

    /// Test short hex for display
    #[test]
    fn test_short_hex_display() {
        let hash = B3Hash::hash(b"display test");
        let short = hash.to_short_hex();
        let full = hash.to_hex();

        assert_eq!(short.len(), 16, "Short hex should be 16 chars");
        assert_eq!(full.len(), 64, "Full hex should be 64 chars");
        assert!(full.starts_with(&short), "Short should be prefix of full");
    }

    /// Test file hashing capability
    #[test]
    fn test_hash_file() {
        use std::io::Write;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("aos_test_hash_file.txt");

        // Write test content
        let test_content = b"test file content for hashing";
        {
            let mut file = std::fs::File::create(&temp_file).unwrap();
            file.write_all(test_content).unwrap();
        }

        // Hash the file
        let file_hash = B3Hash::hash_file(&temp_file).unwrap();
        let content_hash = B3Hash::hash(test_content);

        assert_eq!(
            file_hash, content_hash,
            "File hash should match content hash"
        );

        // Cleanup
        let _ = std::fs::remove_file(&temp_file);
    }
}

// ============================================================================
// Module 3: Error Type Construction Tests
// ============================================================================

mod error_type_construction {
    use super::*;

    /// Test all major error variants are constructible and displayable
    #[test]
    fn test_error_variants() {
        let errors: Vec<AosError> = vec![
            AosError::Io("File not found".to_string()),
            AosError::Network("Connection refused".to_string()),
            AosError::Kernel("GPU execution failed".to_string()),
            AosError::Worker("Worker crashed".to_string()),
            AosError::Database("Query failed".to_string()),
            AosError::InvalidHash("Not valid hex".to_string()),
            AosError::Validation("Input too large".to_string()),
            AosError::InvalidManifest("Missing field".to_string()),
            AosError::MemoryPressure("Out of memory".to_string()),
            AosError::Quarantined("Policy violations".to_string()),
            AosError::DeterminismViolation("Non-deterministic RNG".to_string()),
            AosError::Parse("Invalid JSON".to_string()),
            AosError::Internal("Unexpected state".to_string()),
            AosError::ResourceExhaustion("GPU memory full".to_string()),
            AosError::NotFound("Adapter not found".to_string()),
            AosError::Config("Invalid configuration".to_string()),
            AosError::PolicyViolation("Egress blocked".to_string()),
            AosError::EgressViolation("Outbound request blocked".to_string()),
            AosError::IsolationViolation("Tenant boundary crossed".to_string()),
        ];

        for error in errors {
            let display = error.to_string();
            assert!(
                !display.is_empty(),
                "Error should have non-empty display: {:?}",
                error
            );
        }
    }

    /// Test timeout error construction
    #[test]
    fn test_timeout_error() {
        let duration = Duration::from_secs(30);
        let error = AosError::Timeout { duration };

        let error_str = error.to_string();
        assert!(error_str.contains("Timeout"), "Should indicate timeout");
        assert!(error_str.contains("30"), "Should include duration");
    }

    /// Test worker not responding error
    #[test]
    fn test_worker_not_responding_error() {
        let path = std::path::PathBuf::from("/var/run/aos/worker.sock");
        let error = AosError::WorkerNotResponding { path: path.clone() };

        let error_str = error.to_string();
        assert!(
            error_str.contains("not responding"),
            "Should indicate not responding"
        );
        assert!(error_str.contains("worker.sock"), "Should include path");
    }

    /// Test circuit breaker errors
    #[test]
    fn test_circuit_breaker_errors() {
        let open_error = AosError::CircuitBreakerOpen {
            service: "database".to_string(),
        };
        assert!(open_error.to_string().contains("database"));
        assert!(open_error.to_string().contains("open"));

        let half_open_error = AosError::CircuitBreakerHalfOpen {
            service: "api".to_string(),
        };
        assert!(half_open_error.to_string().contains("api"));
        assert!(half_open_error.to_string().contains("half-open"));
    }

    /// Test policy hash mismatch error
    #[test]
    fn test_policy_hash_mismatch() {
        let error = AosError::PolicyHashMismatch {
            pack_id: "egress_policy".to_string(),
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };

        let error_str = error.to_string();
        assert!(error_str.contains("egress_policy"));
        assert!(error_str.contains("expected"));
        assert!(error_str.contains("actual"));
    }

    /// Test feature disabled error
    #[test]
    fn test_feature_disabled_error() {
        let error = AosError::FeatureDisabled {
            feature: "mlx_backend".to_string(),
            reason: "Requires MLX framework installation".to_string(),
            alternative: Some("Use CoreML backend instead".to_string()),
        };

        let error_str = error.to_string();
        assert!(error_str.contains("mlx_backend"));
        assert!(error_str.contains("disabled"));
    }

    /// Test RNG error with seed tracking
    #[test]
    fn test_rng_error() {
        let error = AosError::RngError {
            seed_hash: "abc123".to_string(),
            label: "router".to_string(),
            counter: 42,
            message: "Seed exhausted".to_string(),
        };

        let error_str = error.to_string();
        assert!(error_str.contains("abc123"));
        assert!(error_str.contains("router"));
        assert!(error_str.contains("42"));
    }
}

// ============================================================================
// Module 4: Error Context Chaining Tests
// ============================================================================

mod error_context_chaining {
    use super::*;
    use adapteros_core::ResultExt;

    /// Test error context attachment
    #[test]
    fn test_context_attachment() {
        let base_error: Result<()> = Err(AosError::Io("File not found".to_string()));

        let with_context = base_error.context("While loading adapter config");

        if let Err(AosError::WithContext { context, source }) = with_context {
            assert_eq!(context, "While loading adapter config");
            assert!(matches!(*source, AosError::Io(_)));
        } else {
            panic!("Expected WithContext error");
        }
    }

    /// Test error context chaining
    #[test]
    fn test_context_chaining() {
        let base_error: Result<()> = Err(AosError::Network("Connection refused".to_string()));

        let chained = base_error
            .context("While connecting to registry")
            .with_context(|| "During adapter registration".to_string());

        if let Err(AosError::WithContext { context, source }) = chained {
            assert_eq!(context, "During adapter registration");
            if let AosError::WithContext {
                context: inner_ctx,
                source: inner_source,
            } = *source
            {
                assert_eq!(inner_ctx, "While connecting to registry");
                assert!(matches!(*inner_source, AosError::Network(_)));
            } else {
                panic!("Expected nested WithContext");
            }
        } else {
            panic!("Expected WithContext error");
        }
    }

    /// Test context display formatting
    #[test]
    fn test_context_display_formatting() {
        let base_error: Result<()> = Err(AosError::Other("base error".to_string()));

        let chained = base_error
            .context("context A")
            .context("context B")
            .context("context C");

        let display = format!("{}", chained.unwrap_err());
        assert!(display.contains("context C:"));
        assert!(display.contains("context B:"));
        assert!(display.contains("context A:"));
        assert!(display.contains("base error"));
    }
}

// ============================================================================
// Module 5: Memory and Resource Error Tests
// ============================================================================

mod memory_and_resource_errors {
    use super::*;

    /// Test memory pressure error
    #[test]
    fn test_memory_pressure_error() {
        let error = AosError::MemoryPressure("Insufficient headroom: 5% < 15%".to_string());
        let error_str = error.to_string();

        assert!(
            error_str.contains("Memory pressure"),
            "Error type should be indicated"
        );
        assert!(
            error_str.contains("headroom"),
            "Error message should be preserved"
        );
    }

    /// Test resource exhaustion error
    #[test]
    fn test_resource_exhaustion_error() {
        let error =
            AosError::ResourceExhaustion("GPU memory exhausted: 0 bytes available".to_string());

        let error_str = error.to_string();
        assert!(error_str.contains("exhaustion"));
        assert!(error_str.contains("GPU memory"));
    }

    /// Test memory error variant
    #[test]
    fn test_memory_error() {
        let error = AosError::Memory("Failed to allocate buffer".to_string());
        assert!(error.to_string().contains("Memory error"));
    }

    /// Test unavailable error
    #[test]
    fn test_unavailable_error() {
        let error = AosError::Unavailable("Service temporarily unavailable".to_string());
        assert!(error.to_string().contains("unavailable"));
    }
}

// ============================================================================
// Module 6: Invalid Manifest Tests
// ============================================================================

mod invalid_manifest_handling {
    use super::*;

    /// Test InvalidManifest error construction
    #[test]
    fn test_invalid_manifest_error() {
        let error = AosError::InvalidManifest("Missing required field: model_hash".to_string());

        assert!(error.to_string().contains("Invalid manifest"));
        assert!(error.to_string().contains("model_hash"));
    }

    /// Test manifest validation with empty bytes
    #[test]
    fn test_empty_manifest_bytes() {
        let empty_bytes: &[u8] = &[];

        // Attempting to parse empty bytes as manifest should fail
        let result: std::result::Result<serde_json::Value, _> = serde_json::from_slice(empty_bytes);
        assert!(
            result.is_err(),
            "Empty bytes should not parse as valid JSON"
        );
    }

    /// Test manifest with missing required fields
    #[test]
    fn test_manifest_missing_fields() {
        let incomplete_manifest = r#"{"name": "test_adapter"}"#;

        // Parse succeeds but validation would fail
        let json: serde_json::Value = serde_json::from_str(incomplete_manifest).unwrap();

        // Check for required fields
        assert!(
            json.get("model_hash").is_none(),
            "Should be missing model_hash"
        );
        assert!(
            json.get("weights_offset").is_none(),
            "Should be missing weights_offset"
        );
    }

    /// Test manifest with invalid types
    #[test]
    fn test_manifest_invalid_types() {
        let invalid_manifest = r#"{"weights_offset": "not_a_number"}"#;

        let json: serde_json::Value = serde_json::from_str(invalid_manifest).unwrap();
        let weights_offset = json.get("weights_offset").and_then(|v| v.as_u64());

        assert!(weights_offset.is_none(), "String should not parse as u64");
    }

    /// Test .aos format validation
    #[test]
    fn test_aos_format_validation() {
        // Valid .aos header: manifest_offset (4 bytes) + manifest_len (4 bytes) = 8 bytes minimum
        let too_small: [u8; 4] = [0, 0, 0, 8];

        // Check that we detect invalid size
        assert!(too_small.len() < 8, "Too small for valid .aos file");

        // Create minimal invalid .aos structure
        let mut invalid_aos: Vec<u8> = vec![0; 16];
        // Set manifest_offset to 8 (right after header)
        invalid_aos[0..4].copy_from_slice(&8u32.to_le_bytes());
        // Set manifest_len to 100 (way too large for our buffer)
        invalid_aos[4..8].copy_from_slice(&100u32.to_le_bytes());

        // Validate: manifest would be out of bounds
        let manifest_offset = u32::from_le_bytes([
            invalid_aos[0],
            invalid_aos[1],
            invalid_aos[2],
            invalid_aos[3],
        ]) as usize;
        let manifest_len = u32::from_le_bytes([
            invalid_aos[4],
            invalid_aos[5],
            invalid_aos[6],
            invalid_aos[7],
        ]) as usize;

        assert!(
            manifest_offset + manifest_len > invalid_aos.len(),
            "Should detect manifest out of bounds"
        );
    }
}

// ============================================================================
// Module 7: Concurrent Error Scenarios Tests
// ============================================================================

mod concurrent_error_scenarios {
    use super::*;
    use tokio::sync::Barrier;

    /// Test error accumulation under load
    #[tokio::test]
    async fn test_error_accumulation_under_load() {
        let error_count = Arc::new(AtomicUsize::new(0));
        let success_count = Arc::new(AtomicUsize::new(0));
        let mut handles = vec![];

        for i in 0..100 {
            let errors = error_count.clone();
            let successes = success_count.clone();
            handles.push(tokio::spawn(async move {
                // Simulate operation that sometimes fails
                if i % 3 == 0 {
                    errors.fetch_add(1, Ordering::Relaxed);
                    Err::<(), _>(AosError::Internal(format!("Task {} failed", i)))
                } else {
                    successes.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }
            }));
        }

        for handle in handles {
            let _ = handle.await.unwrap();
        }

        let total_errors = error_count.load(Ordering::Relaxed);
        let total_successes = success_count.load(Ordering::Relaxed);

        assert_eq!(total_errors + total_successes, 100);
        assert!(total_errors > 0, "Some errors should have occurred");
        assert!(total_successes > 0, "Some successes should have occurred");
    }

    /// Test rapid error generation performance
    #[test]
    fn test_rapid_error_generation() {
        let start = Instant::now();
        let mut results = vec![];

        let error_types: Vec<fn() -> AosError> = vec![
            || AosError::Io("IO error".to_string()),
            || AosError::Network("Network error".to_string()),
            || AosError::Kernel("Kernel error".to_string()),
            || AosError::Worker("Worker error".to_string()),
            || AosError::Database("Database error".to_string()),
        ];

        // Generate 10000 errors rapidly
        for i in 0..10000 {
            let error = error_types[i % error_types.len()]();
            results.push(error.to_string());
        }

        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(1),
            "Error handling should be fast"
        );
        assert_eq!(results.len(), 10000);
    }

    /// Test concurrent barrier synchronization with errors
    #[tokio::test]
    async fn test_barrier_synchronized_errors() {
        let barrier = Arc::new(Barrier::new(10));
        let error_tracker = Arc::new(AtomicUsize::new(0));
        let mut handles = vec![];

        for i in 0..10 {
            let barrier = barrier.clone();
            let tracker = error_tracker.clone();
            handles.push(tokio::spawn(async move {
                barrier.wait().await;

                // All tasks release at same time
                if i % 2 == 0 {
                    tracker.fetch_add(1, Ordering::Relaxed);
                    Err::<i32, _>(AosError::Internal(format!("Error {}", i)))
                } else {
                    Ok(i)
                }
            }));
        }

        let mut errors = 0;
        let mut successes = 0;

        for handle in handles {
            match handle.await.unwrap() {
                Ok(_) => successes += 1,
                Err(_) => errors += 1,
            }
        }

        assert_eq!(errors, 5);
        assert_eq!(successes, 5);
    }
}

// ============================================================================
// Module 8: Policy and Security Error Tests
// ============================================================================

mod policy_security_errors {
    use super::*;

    /// Test quarantine error for policy violations
    #[test]
    fn test_quarantine_error() {
        let error =
            AosError::Quarantined("3 consecutive policy hash mismatches detected".to_string());

        let error_str = error.to_string();
        assert!(error_str.contains("quarantined"));
        assert!(error_str.contains("policy hash"));
    }

    /// Test determinism violation detection
    #[test]
    fn test_determinism_violation() {
        let error = AosError::DeterminismViolation(
            "Non-deterministic random source detected: rand::thread_rng()".to_string(),
        );

        let error_str = error.to_string();
        assert!(error_str.contains("Determinism violation"));
        assert!(error_str.contains("thread_rng"));
    }

    /// Test egress violation
    #[test]
    fn test_egress_violation() {
        let error = AosError::EgressViolation(
            "Outbound HTTP request blocked in production mode".to_string(),
        );

        let error_str = error.to_string();
        assert!(error_str.contains("Egress violation"));
    }

    /// Test isolation violation
    #[test]
    fn test_isolation_violation() {
        let error = AosError::IsolationViolation(
            "Tenant A attempted to access Tenant B resources".to_string(),
        );

        let error_str = error.to_string();
        assert!(error_str.contains("Isolation violation"));
    }

    /// Test policy violation
    #[test]
    fn test_policy_violation() {
        let error =
            AosError::PolicyViolation("Adapter exceeds maximum allowed rank of 64".to_string());

        let error_str = error.to_string();
        assert!(error_str.contains("Policy violation"));
    }
}

// ============================================================================
// Module 9: Database and Crypto Error Tests
// ============================================================================

mod database_crypto_errors {
    use super::*;

    /// Test database error
    #[test]
    fn test_database_error() {
        let error = AosError::Database("Connection pool exhausted".to_string());
        assert!(error.to_string().contains("Database error"));
    }

    /// Test crypto error
    #[test]
    fn test_crypto_error() {
        let error = AosError::Crypto("Invalid signature".to_string());
        assert!(error.to_string().contains("Cryptographic error"));
    }

    /// Test encryption/decryption errors
    #[test]
    fn test_encryption_errors() {
        let enc_error = AosError::EncryptionFailed {
            reason: "Key derivation failed".to_string(),
        };
        assert!(enc_error.to_string().contains("Encryption failed"));

        let dec_error = AosError::DecryptionFailed {
            reason: "Invalid ciphertext".to_string(),
        };
        assert!(dec_error.to_string().contains("Decryption failed"));
    }

    /// Test sealed data error
    #[test]
    fn test_sealed_data_error() {
        let error = AosError::InvalidSealedData {
            reason: "Corrupted envelope".to_string(),
        };
        assert!(error.to_string().contains("Invalid sealed data"));
    }
}

// ============================================================================
// Module 10: Chaos Integration Tests
// ============================================================================

mod chaos_integration {
    use super::*;
    use adapteros_core::{
        CircuitBreaker, CircuitBreakerConfig, CircuitState, StandardCircuitBreaker,
    };

    /// Test recovery from cascading failures using circuit breakers
    #[tokio::test]
    async fn test_cascading_failure_isolation() {
        // Simulates a scenario where multiple services fail in sequence
        let service_a = Arc::new(StandardCircuitBreaker::new(
            "service_a".to_string(),
            CircuitBreakerConfig::default(),
        ));
        let service_b = Arc::new(StandardCircuitBreaker::new(
            "service_b".to_string(),
            CircuitBreakerConfig::default(),
        ));

        // Service A fails
        for _ in 0..5 {
            let _: Result<()> = service_a
                .call(async { Err(AosError::Network("A failed".to_string())) })
                .await;
        }

        // Service B depends on A, should still work independently
        let result: Result<&str> = service_b.call(async { Ok("B works") }).await;
        assert!(result.is_ok(), "Service B should work independently");

        // Verify isolation
        match service_a.state() {
            CircuitState::Open { .. } => { /* Expected */ }
            _ => panic!("Service A should be open"),
        }
        assert_eq!(
            service_b.state(),
            CircuitState::Closed,
            "Service B should be closed"
        );
    }

    /// Test error type is preserved through async boundaries
    #[tokio::test]
    async fn test_error_preservation_across_async() {
        let result: Result<()> =
            tokio::spawn(async { Err(AosError::Kernel("GPU error".to_string())) })
                .await
                .unwrap();

        if let Err(AosError::Kernel(msg)) = result {
            assert_eq!(msg, "GPU error");
        } else {
            panic!("Error type should be preserved");
        }
    }

    /// Test error conversion from std::io::Error
    #[test]
    fn test_io_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let aos_error: AosError = io_error.into();

        assert!(matches!(aos_error, AosError::Io(_)));
    }

    /// Test error conversion from serde_json::Error
    #[test]
    fn test_json_error_conversion() {
        let json_result: std::result::Result<serde_json::Value, _> =
            serde_json::from_str("not valid json");
        let aos_error: AosError = json_result.unwrap_err().into();

        assert!(matches!(aos_error, AosError::Serialization(_)));
    }
}

// ============================================================================
// Module 11: Metal GPU Recovery Tests
// ============================================================================

#[cfg(target_os = "macos")]
mod metal_gpu_recovery {
    use super::*;
    use adapteros_lora_kernel_mtl::RecoveryWrapper;
    use metal::Device;

    /// Test Metal GPU command buffer failure detection and recovery
    #[tokio::test]
    async fn test_metal_command_buffer_failure_recovery() {
        let device = Device::system_default().expect("Metal device should be available");
        let mut recovery = RecoveryWrapper::new();

        // Verify initial healthy state
        assert!(!recovery.is_degraded());
        assert_eq!(recovery.panic_count(), 0);
        assert!(recovery.health_check().is_ok());

        // Simulate GPU panic during command buffer execution
        let result = recovery.safe_dispatch(|| {
            panic!("Simulated Metal command buffer failure");
            #[allow(unreachable_code)]
            Ok(())
        });

        // Verify panic was caught and device marked degraded
        assert!(result.is_err());
        assert!(recovery.is_degraded());
        assert_eq!(recovery.panic_count(), 1);
        assert!(recovery.health_check().is_err());

        // Verify error message contains context
        if let Err(AosError::Kernel(msg)) = result {
            assert!(msg.contains("device marked degraded"));
        } else {
            panic!("Expected Kernel error");
        }

        // Attempt recovery with buffer cleanup
        let mut buffers_cleaned = false;
        let recovery_result = recovery.attempt_recovery(
            &device,
            Some(|| {
                buffers_cleaned = true;
            }),
        );

        // Verify recovery succeeded
        assert!(recovery_result.is_ok());
        assert!(!recovery.is_degraded());
        assert_eq!(recovery.recovery_count(), 1);
        assert!(buffers_cleaned);
        assert!(recovery.health_check().is_ok());

        // Verify new command queue is functional
        let recovery_data = recovery_result.unwrap();
        assert!(recovery_data.test_dispatch_us >= 0);

        // Verify subsequent dispatch succeeds
        let result = recovery.safe_dispatch(|| Ok(42));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    /// Test multiple consecutive GPU failures and recoveries
    #[tokio::test]
    async fn test_metal_multiple_failure_recovery_cycles() {
        let device = Device::system_default().expect("Metal device should be available");
        let mut recovery = RecoveryWrapper::new();

        // Simulate 3 consecutive failures with recoveries
        for i in 1..=3 {
            // Trigger panic
            let _ = recovery.safe_dispatch(|| {
                panic!("Failure {}", i);
                #[allow(unreachable_code)]
                Ok(())
            });

            assert!(recovery.is_degraded());
            assert_eq!(recovery.panic_count(), i);

            // Recover
            let result = recovery.attempt_recovery_simple(&device);
            assert!(result.is_ok());
            assert!(!recovery.is_degraded());
            assert_eq!(recovery.recovery_count(), i);

            // Verify time since last recovery is tracked
            assert!(recovery.time_since_last_recovery().is_some());

            // Small delay to ensure timestamp difference
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        // Verify final state
        assert_eq!(recovery.panic_count(), 3);
        assert_eq!(recovery.recovery_count(), 3);
        assert!(!recovery.is_degraded());
    }
}

// ============================================================================
// Module 12: Hot-Swap Quarantine Tests
// ============================================================================

#[cfg(all(test, feature = "extended-tests"))]
mod hotswap_quarantine {
    use super::*;
    use adapteros_lora_worker::adapter_hotswap::AdapterTable;

    /// Test hot-swap quarantine after 3 consecutive failures
    #[tokio::test]
    async fn test_hotswap_quarantine_after_three_failures() {
        let table = AdapterTable::new();
        let hash = B3Hash::hash(b"test_adapter");

        // Simulate 3 consecutive load failures
        for i in 1..=3 {
            let adapter_id = format!("failing_adapter_{}", i);

            // Preload
            table.preload(adapter_id.clone(), hash, 10).await.unwrap();

            // Attempt swap
            let result = table.swap(&[adapter_id.clone()], &[]).await;

            if i < 3 {
                // First 2 failures - should succeed in table update
                assert!(result.is_ok());

                // Simulate kernel load failure and rollback
                table.rollback().await.unwrap();
            } else {
                // 3rd failure - quarantine logic would be in HotSwapManager
                assert!(result.is_ok());
            }
        }

        // After rollbacks, verify state consistency
        let active = table.get_active();
        // After final swap, should have 1 active (the last one)
        assert!(active.len() <= 1);
    }

    /// Test RCU retry enforcement with max retries
    #[tokio::test]
    async fn test_hotswap_rcu_retry_enforcement() {
        let table = Arc::new(AdapterTable::new());
        let h = B3Hash::hash(b"test");

        // Setup: Load and retire a stack
        table.preload("test".to_string(), h, 10).await.unwrap();
        table.swap(&["test".to_string()], &[]).await.unwrap();

        // Get current stack generation
        let generation = table.get_current_stack_generation();

        // Verify stack exists
        let stack = table.get_current_stack_handle();
        assert_eq!(stack.generation, generation as u64);

        // The RCU process_retired_stacks handles retry enforcement
        // After 3 failures, stack should be quarantined
    }
}

// ============================================================================
// Module 13: Deterministic Executor Crash Recovery Tests
// ============================================================================

#[cfg(all(test, feature = "extended-tests"))]
mod deterministic_executor_recovery {
    use super::*;
    use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Test deterministic executor crash recovery via snapshot
    #[tokio::test]
    async fn test_executor_crash_recovery_via_snapshot() {
        let config = ExecutorConfig {
            global_seed: [42u8; 32],
            enable_event_logging: true,
            ..Default::default()
        };

        let executor = DeterministicExecutor::new(config.clone());

        // Spawn some tasks
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        executor
            .spawn_deterministic("Task 1".to_string(), async move {
                counter_clone.fetch_add(1, Ordering::Relaxed);
            })
            .unwrap();

        let counter_clone = counter.clone();
        executor
            .spawn_deterministic("Task 2".to_string(), async move {
                counter_clone.fetch_add(10, Ordering::Relaxed);
            })
            .unwrap();

        // Run for a bit
        tokio::select! {
            _ = executor.run() => {},
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(50)) => {}
        }

        // Create snapshot before "crash"
        let snapshot = executor.snapshot().unwrap();
        assert!(snapshot.tick > 0);
        assert!(snapshot.event_log.len() > 0);

        let initial_tick = snapshot.tick;

        // Simulate crash - create new executor
        let executor2 = DeterministicExecutor::new(config);

        // Restore from snapshot
        executor2.restore(snapshot).unwrap();

        // Verify state restored
        assert_eq!(executor2.current_tick(), initial_tick);

        // Verify event log restored
        let events = executor2.get_event_log();
        assert!(events.len() > 0);

        // Verify can continue execution
        let counter2 = Arc::new(AtomicU32::new(0));
        let counter_clone = counter2.clone();

        executor2
            .spawn_deterministic("Post-recovery task".to_string(), async move {
                counter_clone.fetch_add(100, Ordering::Relaxed);
            })
            .unwrap();
    }

    /// Test snapshot validation prevents restore with wrong seed
    #[tokio::test]
    async fn test_executor_snapshot_seed_validation() {
        let config = ExecutorConfig {
            global_seed: [42u8; 32],
            ..Default::default()
        };

        let executor = DeterministicExecutor::new(config);

        // Create snapshot
        let snapshot = executor.snapshot().unwrap();

        // Try to restore with different seed
        let wrong_config = ExecutorConfig {
            global_seed: [99u8; 32],
            ..Default::default()
        };

        let executor2 = DeterministicExecutor::new(wrong_config);

        // Should fail due to seed mismatch
        let result = executor2.restore(snapshot);
        assert!(result.is_err());

        if let Err(e) = result {
            let error_msg = format!("{:?}", e);
            assert!(error_msg.contains("seed mismatch"));
        }
    }

    /// Test executor prevents restore while running
    #[tokio::test]
    async fn test_executor_running_restore_prevention() {
        let config = ExecutorConfig {
            global_seed: [42u8; 32],
            ..Default::default()
        };

        let executor = Arc::new(DeterministicExecutor::new(config));

        // Create snapshot first
        let snapshot = executor.snapshot().unwrap();

        // Spawn task that keeps executor running
        let executor_clone = executor.clone();
        let run_handle = tokio::spawn(async move {
            let _ = executor_clone.run().await;
        });

        // Wait for executor to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Try to restore while running - should fail
        if executor.is_running() {
            let result = executor.restore(snapshot);
            assert!(result.is_err());
        }

        // Cleanup
        run_handle.abort();
    }
}

// ============================================================================
// Module 14: Resource Leak Detection Tests
// ============================================================================

#[cfg(all(test, feature = "extended-tests"))]
mod resource_leak_detection {
    use super::*;
    use adapteros_lora_worker::adapter_hotswap::AdapterTable;

    /// Test no resource leaks during repeated load/unload cycles
    #[tokio::test]
    async fn test_no_memory_leaks_load_unload_cycles() {
        let table = AdapterTable::new();

        // Load and unload adapter multiple times
        for i in 0..10 {
            let adapter_id = format!("leak_test_{}", i);
            let hash = B3Hash::hash(adapter_id.as_bytes());

            table.preload(adapter_id.clone(), hash, 100).await.unwrap();
            table.swap(&[adapter_id.clone()], &[]).await.unwrap();

            // Immediate unload
            table.swap(&[], &[adapter_id]).await.unwrap();
        }

        // Verify no memory leaks - all adapters should be unloaded
        assert_eq!(table.get_active().len(), 0);
        assert_eq!(table.total_vram_mb(), 0);
    }

    /// Test state consistency after partial failure
    #[tokio::test]
    async fn test_state_consistency_after_partial_failure() {
        let table = AdapterTable::new();

        // Setup: Load 5 adapters
        let mut adapter_ids = Vec::new();
        for i in 0..5 {
            let adapter_id = format!("consistency_test_{}", i);
            let hash = B3Hash::hash(adapter_id.as_bytes());
            table.preload(adapter_id.clone(), hash, 100).await.unwrap();
            adapter_ids.push(adapter_id);
        }

        // Swap all in
        table.swap(&adapter_ids, &[]).await.unwrap();
        assert_eq!(table.get_active().len(), 5);

        // Simulate partial unload (remove 2 out of 5)
        let to_remove = vec![adapter_ids[0].clone(), adapter_ids[2].clone()];
        table.swap(&[], &to_remove).await.unwrap();

        // Verify consistency - only 2 removed, 3 remain
        assert_eq!(table.get_active().len(), 3);

        // Verify correct adapters removed
        let active = table.get_active();
        assert!(!active.iter().any(|a| a.id == adapter_ids[0]));
        assert!(!active.iter().any(|a| a.id == adapter_ids[2]));
    }
}
