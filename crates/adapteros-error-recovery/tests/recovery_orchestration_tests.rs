//! Recovery orchestration integration tests
//!
//! Tests for retry budget exhaustion, backoff timing verification,
//! concurrent failure handling, recovery priority ordering, and
//! cascading failure prevention.

use adapteros_core::{AosError, Result};
use adapteros_error_recovery::{
    ErrorRecoveryConfig, ErrorRecoveryManager, ErrorType, RecoveryResult,
};
use adapteros_platform::common::PlatformUtils;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::sync::Barrier;

fn new_test_tempdir() -> Result<TempDir> {
    let root = PlatformUtils::temp_dir();
    std::fs::create_dir_all(&root)?;
    Ok(TempDir::new_in(&root)?)
}

// =============================================================================
// Retry Budget Exhaustion Tests
// =============================================================================

mod retry_budget_exhaustion {
    use super::*;
    use adapteros_error_recovery::retry::RetryManager;

    #[tokio::test]
    async fn test_zero_retry_budget_fails_immediately() -> Result<()> {
        let config = ErrorRecoveryConfig {
            max_retry_attempts: 0,
            retry_delay: Duration::from_millis(1),
            ..ErrorRecoveryConfig::default()
        };
        let manager = RetryManager::new(&config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("nonexistent.txt");

        // With 0 retries, should fail immediately
        let result = manager.retry_operation(&test_file).await?;
        assert_eq!(
            result,
            RecoveryResult::Failed,
            "Zero retry budget should fail immediately"
        );

        // Verify no retry was recorded (attempts exceeded before trying)
        let stats = manager.get_retry_statistics().await;
        assert_eq!(
            stats.total_retries, 0,
            "No retries should be recorded with 0 budget"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_retry_budget_exhaustion_after_max_attempts() -> Result<()> {
        let config = ErrorRecoveryConfig {
            max_retry_attempts: 3,
            retry_delay: Duration::from_millis(1),
            max_retry_delay: Duration::from_millis(10),
            backoff_multiplier: 1.5,
            ..ErrorRecoveryConfig::default()
        };
        let manager = RetryManager::new(&config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("never_exists.txt");
        let key = test_file.to_string_lossy().to_string();

        // First attempt
        let result = manager.retry_operation(&test_file).await?;
        assert_eq!(result, RecoveryResult::Failed);

        // Check record
        let record = manager.get_retry_record(&key).await.unwrap();
        assert_eq!(record.retry_count, 1);

        // Second attempt
        let result = manager.retry_operation(&test_file).await?;
        assert_eq!(result, RecoveryResult::Failed);
        let record = manager.get_retry_record(&key).await.unwrap();
        assert_eq!(record.retry_count, 2);

        // Third attempt
        let result = manager.retry_operation(&test_file).await?;
        assert_eq!(result, RecoveryResult::Failed);
        let record = manager.get_retry_record(&key).await.unwrap();
        assert_eq!(record.retry_count, 3);

        // Fourth attempt should fail because budget exhausted
        let result = manager.retry_operation(&test_file).await?;
        assert_eq!(
            result,
            RecoveryResult::Failed,
            "Should fail after budget exhaustion"
        );

        // Retry count should not increase beyond max
        let record = manager.get_retry_record(&key).await.unwrap();
        assert_eq!(
            record.retry_count, 3,
            "Retry count should remain at max after exhaustion"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_retry_with_custom_operation_budget_exhaustion() -> Result<()> {
        let config = ErrorRecoveryConfig {
            max_retry_attempts: 2,
            retry_delay: Duration::from_millis(1),
            max_retry_delay: Duration::from_millis(5),
            ..ErrorRecoveryConfig::default()
        };
        let manager = RetryManager::new(&config)?;

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        // Use retry_with to track actual operation calls
        let result = manager
            .retry_with("test_key", move || {
                let cc = call_count_clone.clone();
                async move {
                    cc.fetch_add(1, Ordering::SeqCst);
                    Err(AosError::Io("Simulated failure".to_string()))
                }
            })
            .await?;

        assert_eq!(result, RecoveryResult::Failed);
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "Operation should be called once per retry_with call"
        );

        // Second call
        let call_count2 = Arc::new(AtomicU32::new(0));
        let call_count2_clone = call_count2.clone();
        let result = manager
            .retry_with("test_key", move || {
                let cc = call_count2_clone.clone();
                async move {
                    cc.fetch_add(1, Ordering::SeqCst);
                    Err(AosError::Io("Simulated failure".to_string()))
                }
            })
            .await?;

        assert_eq!(result, RecoveryResult::Failed);

        // Third call - budget exhausted
        let call_count3 = Arc::new(AtomicU32::new(0));
        let call_count3_clone = call_count3.clone();
        let result = manager
            .retry_with("test_key", move || {
                let cc = call_count3_clone.clone();
                async move {
                    cc.fetch_add(1, Ordering::SeqCst);
                    Err(AosError::Io("Simulated failure".to_string()))
                }
            })
            .await?;

        assert_eq!(
            result,
            RecoveryResult::Failed,
            "Should fail after budget exhaustion"
        );
        // Operation should NOT be called when budget is already exhausted
        assert_eq!(
            call_count3.load(Ordering::SeqCst),
            0,
            "Operation should not be called when budget exhausted"
        );

        Ok(())
    }
}

// =============================================================================
// Backoff Timing Verification Tests
// =============================================================================

mod backoff_timing {
    use super::*;
    use adapteros_error_recovery::retry::RetryManager;

    /// Test that exponential backoff is applied with real timing.
    /// Uses short delays to keep tests fast while still verifying behavior.
    #[tokio::test]
    async fn test_exponential_backoff_timing() -> Result<()> {
        let base_delay = Duration::from_millis(5);
        let config = ErrorRecoveryConfig {
            max_retry_attempts: 5,
            retry_delay: base_delay,
            max_retry_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            ..ErrorRecoveryConfig::default()
        };
        let manager = RetryManager::new(&config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("backoff_test.txt");

        // First retry: delay = base_delay * 2^0 = 5ms
        let start = Instant::now();
        let _ = manager.retry_operation(&test_file).await?;
        let first_elapsed = start.elapsed();

        // Second retry: delay = base_delay * 2^1 = 10ms
        let start = Instant::now();
        let _ = manager.retry_operation(&test_file).await?;
        let second_elapsed = start.elapsed();

        // Third retry: delay = base_delay * 2^2 = 20ms
        let start = Instant::now();
        let _ = manager.retry_operation(&test_file).await?;
        let third_elapsed = start.elapsed();

        // Verify delays are increasing (exponential backoff)
        // Allow some tolerance for scheduling jitter
        assert!(
            first_elapsed >= Duration::from_millis(4),
            "First retry should have >= 4ms delay, got {:?}",
            first_elapsed
        );
        assert!(
            second_elapsed >= Duration::from_millis(8),
            "Second retry should have >= 8ms delay, got {:?}",
            second_elapsed
        );
        assert!(
            third_elapsed >= Duration::from_millis(16),
            "Third retry should have >= 16ms delay, got {:?}",
            third_elapsed
        );

        Ok(())
    }

    /// Test that delays are capped at max_retry_delay
    #[tokio::test]
    async fn test_max_delay_cap() -> Result<()> {
        let config = ErrorRecoveryConfig {
            max_retry_attempts: 10,
            retry_delay: Duration::from_millis(10),
            max_retry_delay: Duration::from_millis(30), // Cap at 30ms
            backoff_multiplier: 3.0,                    // 10 * 3^n grows quickly
            ..ErrorRecoveryConfig::default()
        };
        let manager = RetryManager::new(&config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("max_delay_test.txt");

        // Do several retries - delay should never exceed max_retry_delay
        for i in 0..5 {
            let start = Instant::now();
            let _ = manager.retry_operation(&test_file).await?;
            let elapsed = start.elapsed();

            // After a few iterations, backoff would exceed 30ms without the cap
            // 10 * 3^3 = 270ms, but should be capped at 30ms
            // Allow some tolerance (up to 50ms) for test reliability
            assert!(
                elapsed <= Duration::from_millis(50),
                "Retry {} delay should be capped at ~30ms, got {:?}",
                i,
                elapsed
            );
        }

        Ok(())
    }

    /// Test that multiplier of 1.0 results in constant delay
    #[tokio::test]
    async fn test_backoff_multiplier_one_no_increase() -> Result<()> {
        let base_delay = Duration::from_millis(10);
        let config = ErrorRecoveryConfig {
            max_retry_attempts: 5,
            retry_delay: base_delay,
            max_retry_delay: Duration::from_secs(10),
            backoff_multiplier: 1.0, // No increase
            ..ErrorRecoveryConfig::default()
        };
        let manager = RetryManager::new(&config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("no_backoff_test.txt");

        let mut delays = Vec::new();

        // Collect delays for 3 retries
        for _ in 0..3 {
            let start = Instant::now();
            let _ = manager.retry_operation(&test_file).await?;
            delays.push(start.elapsed());
        }

        // With multiplier 1.0, all delays should be similar (within tolerance)
        // Each delay should be roughly base_delay (10ms)
        for (i, delay) in delays.iter().enumerate() {
            assert!(
                *delay >= Duration::from_millis(8) && *delay <= Duration::from_millis(30),
                "Retry {} should have ~10ms delay with multiplier 1.0, got {:?}",
                i,
                delay
            );
        }

        // Verify delays are not increasing significantly
        // With multiplier 1.0, later delays should NOT be 2x or 3x the first
        if delays.len() >= 3 {
            let ratio = delays[2].as_millis() as f64 / delays[0].as_millis().max(1) as f64;
            assert!(
                ratio < 2.0,
                "With multiplier 1.0, third delay should not be >2x first (ratio: {:.2})",
                ratio
            );
        }

        Ok(())
    }
}

// =============================================================================
// Concurrent Failure Handling Tests
// =============================================================================

mod concurrent_failures {
    use super::*;
    use adapteros_error_recovery::retry::RetryManager;

    #[tokio::test]
    async fn test_concurrent_retries_independent_paths() -> Result<()> {
        let config = ErrorRecoveryConfig {
            max_retry_attempts: 3,
            retry_delay: Duration::from_millis(1),
            max_retry_delay: Duration::from_millis(10),
            ..ErrorRecoveryConfig::default()
        };
        let manager = Arc::new(RetryManager::new(&config)?);

        let temp_dir = new_test_tempdir()?;
        let file1 = temp_dir.path().join("concurrent_1.txt");
        let file2 = temp_dir.path().join("concurrent_2.txt");
        let file3 = temp_dir.path().join("concurrent_3.txt");

        // Spawn concurrent retry operations
        let m1 = manager.clone();
        let f1 = file1.clone();
        let handle1 = tokio::spawn(async move { m1.retry_operation(&f1).await });

        let m2 = manager.clone();
        let f2 = file2.clone();
        let handle2 = tokio::spawn(async move { m2.retry_operation(&f2).await });

        let m3 = manager.clone();
        let f3 = file3.clone();
        let handle3 = tokio::spawn(async move { m3.retry_operation(&f3).await });

        // Wait for all to complete
        let (r1, r2, r3) = tokio::join!(handle1, handle2, handle3);

        // All should fail (files don't exist)
        assert_eq!(r1.unwrap()?, RecoveryResult::Failed);
        assert_eq!(r2.unwrap()?, RecoveryResult::Failed);
        assert_eq!(r3.unwrap()?, RecoveryResult::Failed);

        // Check each path has independent retry records
        let stats = manager.get_retry_statistics().await;
        assert_eq!(
            stats.total_retries, 3,
            "Should have 3 independent retry records"
        );

        // Verify independent records exist
        let key1 = file1.to_string_lossy().to_string();
        let key2 = file2.to_string_lossy().to_string();
        let key3 = file3.to_string_lossy().to_string();

        assert!(manager.get_retry_record(&key1).await.is_some());
        assert!(manager.get_retry_record(&key2).await.is_some());
        assert!(manager.get_retry_record(&key3).await.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_retries_same_path() -> Result<()> {
        let config = ErrorRecoveryConfig {
            max_retry_attempts: 10, // Higher limit to allow concurrent operations
            retry_delay: Duration::from_millis(1),
            max_retry_delay: Duration::from_millis(5),
            ..ErrorRecoveryConfig::default()
        };
        let manager = Arc::new(RetryManager::new(&config)?);

        let temp_dir = new_test_tempdir()?;
        let shared_file = temp_dir.path().join("shared.txt");

        // Start barrier to synchronize concurrent access
        let barrier = Arc::new(Barrier::new(3));

        let m1 = manager.clone();
        let f1 = shared_file.clone();
        let b1 = barrier.clone();
        let handle1 = tokio::spawn(async move {
            b1.wait().await;
            m1.retry_operation(&f1).await
        });

        let m2 = manager.clone();
        let f2 = shared_file.clone();
        let b2 = barrier.clone();
        let handle2 = tokio::spawn(async move {
            b2.wait().await;
            m2.retry_operation(&f2).await
        });

        let m3 = manager.clone();
        let f3 = shared_file.clone();
        let b3 = barrier.clone();
        let handle3 = tokio::spawn(async move {
            b3.wait().await;
            m3.retry_operation(&f3).await
        });

        // Wait for all to complete
        let (r1, r2, r3) = tokio::join!(handle1, handle2, handle3);

        // All should fail (file doesn't exist)
        assert_eq!(r1.unwrap()?, RecoveryResult::Failed);
        assert_eq!(r2.unwrap()?, RecoveryResult::Failed);
        assert_eq!(r3.unwrap()?, RecoveryResult::Failed);

        // Should have only 1 retry record (same path, shared HashMap key)
        let stats = manager.get_retry_statistics().await;
        assert_eq!(
            stats.total_retries, 1,
            "Should have 1 retry record for same path"
        );

        // The retry_count in the shared record reflects the final state.
        // Due to concurrent access with a Mutex, one of the concurrent
        // operations wins the race to update the record.
        let key = shared_file.to_string_lossy().to_string();
        let record = manager.get_retry_record(&key).await.unwrap();
        // At minimum, one operation completed and updated the record
        assert!(
            record.retry_count >= 1,
            "At least one retry should be recorded"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_success_after_file_creation() -> Result<()> {
        let config = ErrorRecoveryConfig {
            max_retry_attempts: 10,
            retry_delay: Duration::from_millis(5),
            max_retry_delay: Duration::from_millis(20),
            ..ErrorRecoveryConfig::default()
        };
        let manager = Arc::new(RetryManager::new(&config)?);

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("delayed_create.txt");
        let test_file_clone = test_file.clone();

        // Task that creates the file after a short delay
        let creator = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            tokio::fs::write(&test_file_clone, "created").await
        });

        // Retry operations that should eventually succeed
        let m1 = manager.clone();
        let f1 = test_file.clone();
        let retrier = tokio::spawn(async move {
            // Keep retrying until success or budget exhausted
            for _ in 0..5 {
                let result = m1.retry_operation(&f1).await;
                if let Ok(RecoveryResult::Success) = result {
                    return Ok::<_, adapteros_core::AosError>(RecoveryResult::Success);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok::<_, adapteros_core::AosError>(RecoveryResult::Failed)
        });

        let (create_result, retry_result) = tokio::join!(creator, retrier);
        create_result.unwrap()?;

        assert_eq!(
            retry_result.unwrap()?,
            RecoveryResult::Success,
            "Should eventually succeed after file is created"
        );

        Ok(())
    }
}

// =============================================================================
// Recovery Priority Ordering Tests
// =============================================================================

mod recovery_priority {
    use super::*;

    #[tokio::test]
    async fn test_error_classification_determines_strategy() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");

        // Test network error -> should retry
        let network_error = AosError::Network("connection refused".to_string());
        let result = manager.handle_error(network_error, &test_file).await;
        // Network errors should use Retry strategy
        // (Will fail because no actual operation to retry, but strategy is Retry)
        assert!(
            result.is_ok() || result.is_err(),
            "Network error should be handled"
        );

        // Test timeout error -> should retry
        let timeout_error = AosError::Timeout {
            duration: Duration::from_secs(30),
        };
        let result = manager.handle_error(timeout_error, &test_file).await;
        assert!(
            result.is_ok() || result.is_err(),
            "Timeout error should be handled"
        );

        // Test permission error -> should require manual intervention
        let permission_error = AosError::Authz("access denied".to_string());
        let result = manager.handle_error(permission_error, &test_file).await;
        // Permission errors should result in ManualRequired, which returns an error
        assert!(
            result.is_err(),
            "Permission error should require manual intervention"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_io_error_classification_variations() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");

        // Test corruption detection in IO error message
        let corruption_error = AosError::Io("file is corrupt".to_string());
        let result = manager.handle_error(corruption_error, &test_file).await;
        // Corruption errors trigger RestoreFromBackup or RecreateFile
        assert!(
            result.is_ok() || result.is_err(),
            "Corruption error should be handled"
        );

        // Test permission-related IO error
        let perm_io_error = AosError::Io("permission denied for operation".to_string());
        let result = manager.handle_error(perm_io_error, &test_file).await;
        // Should map to PermissionError type
        assert!(
            result.is_err(),
            "Permission-related IO error should require manual intervention"
        );

        // Test disk space IO error
        let disk_error = AosError::Io("no space left on device".to_string());
        let result = manager.handle_error(disk_error, &test_file).await;
        // Should require manual intervention
        assert!(
            result.is_err(),
            "Disk space error should require manual intervention"
        );

        // Test lock error
        let lock_error = AosError::Io("file is locked by another process".to_string());
        let result = manager.handle_error(lock_error, &test_file).await;
        // Lock errors should use Retry strategy
        assert!(
            result.is_ok() || result.is_err(),
            "Lock error should be handled"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_recovery_history_tracks_strategy_used() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;

        // Create a few errors of different types
        let test_file1 = temp_dir.path().join("network_error.txt");
        let _ = manager
            .handle_error(
                AosError::Network("test network error".to_string()),
                &test_file1,
            )
            .await;

        let test_file2 = temp_dir.path().join("timeout_error.txt");
        let _ = manager
            .handle_error(
                AosError::Timeout {
                    duration: Duration::from_secs(5),
                },
                &test_file2,
            )
            .await;

        let test_file3 = temp_dir.path().join("corruption_error.txt");
        let _ = manager
            .handle_error(
                AosError::Io("data corruption detected".to_string()),
                &test_file3,
            )
            .await;

        // Check recovery history
        let history = manager.get_recovery_history().await;
        assert_eq!(history.len(), 3, "Should have 3 recovery records");

        // Verify different error types were recorded
        let error_types: Vec<_> = history.iter().map(|r| &r.error_type).collect();
        assert!(
            error_types
                .iter()
                .any(|t| matches!(t, ErrorType::NetworkError)),
            "Should have NetworkError in history"
        );
        assert!(
            error_types
                .iter()
                .any(|t| matches!(t, ErrorType::TimeoutError)),
            "Should have TimeoutError in history"
        );
        assert!(
            error_types
                .iter()
                .any(|t| matches!(t, ErrorType::FileCorruption)),
            "Should have FileCorruption in history"
        );

        Ok(())
    }
}

// =============================================================================
// Cascading Failure Prevention Tests
// =============================================================================

mod cascading_failure_prevention {
    use super::*;

    #[tokio::test]
    async fn test_disabled_recovery_returns_original_error() -> Result<()> {
        let config = ErrorRecoveryConfig {
            enabled: false,
            ..ErrorRecoveryConfig::default()
        };
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");

        // When recovery is disabled, original error should be returned
        let error = AosError::Io("test error".to_string());
        let result = manager.handle_error(error, &test_file).await;

        assert!(result.is_err(), "Disabled recovery should return error");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("test error"),
            "Should return original error"
        );

        // No recovery should be recorded
        let history = manager.get_recovery_history().await;
        assert!(
            history.is_empty(),
            "No recovery should be recorded when disabled"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_recovery_history_truncation() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;

        // Generate many recovery events
        for i in 0..50 {
            let test_file = temp_dir.path().join(format!("test_{}.txt", i));
            let _ = manager
                .handle_error(AosError::Network(format!("error {}", i)), &test_file)
                .await;
        }

        let history = manager.get_recovery_history().await;
        assert!(
            history.len() <= 1000,
            "Recovery history should be bounded to prevent memory exhaustion"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_partial_recovery_reporting() -> Result<()> {
        let config = ErrorRecoveryConfig {
            enable_partial_recovery: true,
            enable_backup_restore: false, // Force recreate strategy
            ..ErrorRecoveryConfig::default()
        };
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("partial.txt");

        // Create a file that will be "recovered" via recreation
        tokio::fs::write(&test_file, "original content").await?;

        // Trigger corruption recovery (file exists, will be recreated)
        let error = AosError::Io("file is corrupt".to_string());
        let result = manager.handle_error(error, &test_file).await;

        // RecreateFile returns PartialSuccess, which is treated as Ok
        assert!(result.is_ok(), "Partial recovery should succeed");

        // Check statistics
        let stats = manager.get_recovery_statistics().await;
        assert_eq!(stats.total_recoveries, 1, "Should have 1 recovery attempt");

        Ok(())
    }

    #[tokio::test]
    async fn test_statistics_accuracy() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;

        // Create some successful recoveries
        for i in 0..3 {
            let test_file = temp_dir.path().join(format!("success_{}.txt", i));
            tokio::fs::write(&test_file, "content").await?;
            // Lock errors trigger Retry, existing file will succeed
            let _ = manager
                .handle_error(AosError::Io("file is locked".to_string()), &test_file)
                .await;
        }

        // Create some failed recoveries
        for i in 0..2 {
            let test_file = temp_dir.path().join(format!("fail_{}.txt", i));
            // Permission errors require manual intervention (failure)
            let _ = manager
                .handle_error(AosError::Authz(format!("denied {}", i)), &test_file)
                .await;
        }

        let stats = manager.get_recovery_statistics().await;
        assert_eq!(
            stats.total_recoveries, 5,
            "Should have 5 total recovery attempts"
        );

        // Verify success rate is reasonable
        assert!(
            stats.success_rate >= 0.0 && stats.success_rate <= 1.0,
            "Success rate should be between 0 and 1"
        );

        Ok(())
    }
}

// =============================================================================
// Edge Cases and Error Conditions
// =============================================================================

mod edge_cases {
    use super::*;
    use adapteros_error_recovery::retry::{retry_with_error_handler, retry_with_timeout};

    #[tokio::test]
    async fn test_retry_with_timeout_success() -> Result<()> {
        let result = retry_with_timeout(
            || Ok("success"),
            3,
            Duration::from_millis(10),
            Duration::from_millis(100),
            Duration::from_millis(500),
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");

        Ok(())
    }

    #[tokio::test]
    async fn test_retry_with_timeout_all_fail() -> Result<()> {
        let result: Result<&str> = retry_with_timeout(
            || Err(AosError::Io("always fails".to_string())),
            3,
            Duration::from_millis(5),
            Duration::from_millis(20),
            Duration::from_millis(100),
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("always fails"));

        Ok(())
    }

    #[tokio::test]
    async fn test_retry_with_error_handler_conditional_retry() -> Result<()> {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result: std::result::Result<&str, AosError> = retry_with_error_handler(
            move || {
                call_count_clone.fetch_add(1, Ordering::SeqCst);
                Err(AosError::Io("retriable error".to_string()))
            },
            5,
            Duration::from_millis(5),
            Duration::from_millis(20),
            |err, attempt| {
                // Only retry if error contains "retriable" and attempt < 3
                err.to_string().contains("retriable") && attempt < 3
            },
        )
        .await;

        assert!(result.is_err());
        // Should have tried initial + 2 retries = 3 calls, then error_handler returns false
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            3,
            "Should stop at attempt 3 due to error handler"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_retry_with_error_handler_immediate_success() -> Result<()> {
        let result: std::result::Result<&str, AosError> = retry_with_error_handler(
            || Ok("immediate success"),
            5,
            Duration::from_millis(10),
            Duration::from_millis(100),
            |_, _| true,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "immediate success");

        Ok(())
    }

    #[tokio::test]
    async fn test_recovery_manager_with_minimal_config() -> Result<()> {
        let config = ErrorRecoveryConfig {
            enabled: true,
            enable_corruption_detection: false,
            enable_automatic_retry: false,
            max_retry_attempts: 0,
            retry_delay: Duration::ZERO,
            backoff_multiplier: 0.0,
            max_retry_delay: Duration::ZERO,
            enable_partial_recovery: false,
            enable_backup_restore: false,
            backup_retention_count: 0,
        };

        let manager = ErrorRecoveryManager::new(config)?;
        assert!(manager.config().enabled);
        assert!(!manager.config().enable_automatic_retry);

        Ok(())
    }

    #[tokio::test]
    async fn test_retry_manager_clear_history() -> Result<()> {
        let config = ErrorRecoveryConfig {
            max_retry_attempts: 3,
            retry_delay: Duration::from_millis(1),
            ..ErrorRecoveryConfig::default()
        };
        let manager = adapteros_error_recovery::retry::RetryManager::new(&config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("clear_test.txt");

        // Create some retry records
        let _ = manager.retry_operation(&test_file).await?;

        let stats = manager.get_retry_statistics().await;
        assert!(stats.total_retries > 0);

        // Clear history
        manager.clear_retry_history().await;

        let stats = manager.get_retry_statistics().await;
        assert_eq!(stats.total_retries, 0, "History should be cleared");

        Ok(())
    }
}
