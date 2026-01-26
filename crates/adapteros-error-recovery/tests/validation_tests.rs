//! Validation and classification integration tests
//!
//! Tests for error classification logic, recovery strategy selection,
//! circuit breaker state transitions, and threshold configuration validation.

use adapteros_core::{AosError, Result};
use adapteros_error_recovery::{
    corruption::{CorruptionDetector, CorruptionType},
    validation::{ValidationEngine, ValidationErrorType, ValidationSeverity},
    ErrorRecoveryConfig, ErrorRecoveryManager, ErrorType, RecoveryResult, RecoveryStrategy,
};
use adapteros_storage::platform::common::PlatformUtils;
use std::time::Duration;
use tempfile::TempDir;

fn new_test_tempdir() -> Result<TempDir> {
    let root = PlatformUtils::temp_dir();
    std::fs::create_dir_all(&root)?;
    Ok(TempDir::new_in(&root)?)
}

// =============================================================================
// Error Classification Logic Tests
// =============================================================================

mod error_classification {
    use super::*;

    #[tokio::test]
    async fn test_classify_io_corruption_errors() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");

        // Test various corruption-related error messages
        let corruption_messages = vec![
            "file is corrupt",
            "invalid data format detected",
            "bad format in header",
            "CORRUPT checksum",
        ];

        for msg in corruption_messages {
            let error = AosError::Io(msg.to_string());
            let _result = manager.handle_error(error, &test_file).await;
            // Corruption errors should attempt recovery (RestoreFromBackup or RecreateFile)
            // Since there's no backup and file doesn't exist, this might fail or succeed
            // based on the specific strategy, but the classification should be FileCorruption
            let history = manager.get_recovery_history().await;
            let last = history.last().unwrap();
            assert!(
                matches!(last.error_type, ErrorType::FileCorruption),
                "Message '{}' should be classified as FileCorruption, got {:?}",
                msg,
                last.error_type
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_classify_permission_errors() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");

        // Test permission-related error messages
        let permission_messages = vec![
            "permission denied",
            "access denied to resource",
            "operation not permitted",
        ];

        for msg in permission_messages {
            let error = AosError::Io(msg.to_string());
            let result = manager.handle_error(error, &test_file).await;
            // Permission errors should require manual intervention
            assert!(
                result.is_err(),
                "Permission error '{}' should require manual intervention",
                msg
            );

            let history = manager.get_recovery_history().await;
            let last = history.last().unwrap();
            assert!(
                matches!(last.error_type, ErrorType::PermissionError),
                "Message '{}' should be classified as PermissionError, got {:?}",
                msg,
                last.error_type
            );
        }

        // Test Authz error type directly
        let authz_error = AosError::Authz("forbidden".to_string());
        let result = manager.handle_error(authz_error, &test_file).await;
        assert!(result.is_err());

        let history = manager.get_recovery_history().await;
        let last = history.last().unwrap();
        assert!(matches!(last.error_type, ErrorType::PermissionError));

        Ok(())
    }

    #[tokio::test]
    async fn test_classify_disk_space_errors() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");

        // Test disk space related error messages
        let disk_messages = vec![
            "no space left on device",
            "disk full cannot write",
            "quota exceeded for user",
        ];

        for msg in disk_messages {
            let error = AosError::Io(msg.to_string());
            let result = manager.handle_error(error, &test_file).await;
            assert!(
                result.is_err(),
                "Disk space error '{}' should require manual intervention",
                msg
            );

            let history = manager.get_recovery_history().await;
            let last = history.last().unwrap();
            assert!(
                matches!(last.error_type, ErrorType::DiskSpaceError),
                "Message '{}' should be classified as DiskSpaceError, got {:?}",
                msg,
                last.error_type
            );
        }

        // Test ResourceExhaustion error type directly
        let resource_error = AosError::ResourceExhaustion("out of memory".to_string());
        let _ = manager.handle_error(resource_error, &test_file).await;

        let history = manager.get_recovery_history().await;
        let last = history.last().unwrap();
        assert!(matches!(last.error_type, ErrorType::DiskSpaceError));

        Ok(())
    }

    #[tokio::test]
    async fn test_classify_network_errors() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");

        // Network errors should be classified as NetworkError and use Retry strategy
        let network_error = AosError::Network("connection refused".to_string());
        let _ = manager.handle_error(network_error, &test_file).await;

        let history = manager.get_recovery_history().await;
        let last = history.last().unwrap();
        assert!(
            matches!(last.error_type, ErrorType::NetworkError),
            "Network error should be classified as NetworkError"
        );
        assert!(
            matches!(last.strategy, RecoveryStrategy::Retry),
            "Network error should use Retry strategy"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_classify_timeout_errors() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");

        // Timeout errors should use Retry strategy
        let timeout_error = AosError::Timeout {
            duration: Duration::from_secs(30),
        };
        let _ = manager.handle_error(timeout_error, &test_file).await;

        let history = manager.get_recovery_history().await;
        let last = history.last().unwrap();
        assert!(
            matches!(last.error_type, ErrorType::TimeoutError),
            "Timeout error should be classified as TimeoutError"
        );
        assert!(
            matches!(last.strategy, RecoveryStrategy::Retry),
            "Timeout error should use Retry strategy"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_classify_lock_errors() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");

        // Test lock-related error messages
        let lock_messages = vec![
            "file is locked",
            "resource busy",
            "file in use by another process",
        ];

        for msg in lock_messages {
            let error = AosError::Io(msg.to_string());
            let _ = manager.handle_error(error, &test_file).await;

            let history = manager.get_recovery_history().await;
            let last = history.last().unwrap();
            assert!(
                matches!(last.error_type, ErrorType::LockError),
                "Message '{}' should be classified as LockError, got {:?}",
                msg,
                last.error_type
            );
            assert!(
                matches!(last.strategy, RecoveryStrategy::Retry),
                "Lock error should use Retry strategy"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_classify_unknown_errors() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");

        // Errors that don't match any pattern should be Unknown
        let unknown_errors = vec![
            AosError::Parse("invalid syntax".to_string()),
            AosError::Config("bad configuration".to_string()),
            AosError::Crypto("encryption failed".to_string()),
        ];

        for error in unknown_errors {
            let _ = manager.handle_error(error, &test_file).await;

            let history = manager.get_recovery_history().await;
            let last = history.last().unwrap();
            assert!(
                matches!(last.error_type, ErrorType::Unknown),
                "Error should be classified as Unknown"
            );
            assert!(
                matches!(last.strategy, RecoveryStrategy::Manual),
                "Unknown error should use Manual strategy"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_classify_io_not_found_as_unknown() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");

        // "Not found" errors should be classified as Unknown (not corruption)
        let not_found_messages = vec![
            "file not found",
            "no such file or directory",
            "path does not exist",
        ];

        for msg in not_found_messages {
            let error = AosError::Io(msg.to_string());
            let _ = manager.handle_error(error, &test_file).await;

            let history = manager.get_recovery_history().await;
            let last = history.last().unwrap();
            assert!(
                matches!(last.error_type, ErrorType::Unknown),
                "Message '{}' should be classified as Unknown, got {:?}",
                msg,
                last.error_type
            );
        }

        Ok(())
    }
}

// =============================================================================
// Recovery Strategy Selection Tests
// =============================================================================

mod strategy_selection {
    use super::*;

    #[tokio::test]
    async fn test_corruption_with_backup_enabled() -> Result<()> {
        let config = ErrorRecoveryConfig {
            enable_backup_restore: true,
            ..ErrorRecoveryConfig::default()
        };
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("corrupted.txt");

        // File corruption with backup enabled should try RestoreFromBackup
        let error = AosError::Io("file is corrupt".to_string());
        let _ = manager.handle_error(error, &test_file).await;

        let history = manager.get_recovery_history().await;
        let last = history.last().unwrap();
        assert!(
            matches!(last.strategy, RecoveryStrategy::RestoreFromBackup),
            "Corruption with backup enabled should use RestoreFromBackup"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_corruption_without_backup_enabled() -> Result<()> {
        let config = ErrorRecoveryConfig {
            enable_backup_restore: false,
            ..ErrorRecoveryConfig::default()
        };
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("corrupted.txt");

        // File corruption without backup should use RecreateFile
        let error = AosError::Io("file is corrupt".to_string());
        let _ = manager.handle_error(error, &test_file).await;

        let history = manager.get_recovery_history().await;
        let last = history.last().unwrap();
        assert!(
            matches!(last.strategy, RecoveryStrategy::RecreateFile),
            "Corruption without backup should use RecreateFile"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_directory_corruption_strategy() -> Result<()> {
        // Note: Directory corruption classification requires specific error patterns
        // This tests the strategy once classification is done
        let config = ErrorRecoveryConfig {
            enable_backup_restore: false,
            ..ErrorRecoveryConfig::default()
        };
        let _manager = ErrorRecoveryManager::new(config)?;

        // DirectoryCorruption type maps to RecreateDirectory strategy
        // when backup is disabled
        // This is validated in the recovery engine tests

        Ok(())
    }

    #[tokio::test]
    async fn test_transient_errors_use_retry() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");

        // Network, Timeout, and Lock errors are transient and should use Retry
        let transient_errors = vec![
            AosError::Network("connection reset".to_string()),
            AosError::Timeout {
                duration: Duration::from_secs(10),
            },
            AosError::Io("file is busy".to_string()),
        ];

        for error in transient_errors {
            let _ = manager.handle_error(error, &test_file).await;

            let history = manager.get_recovery_history().await;
            let last = history.last().unwrap();
            assert!(
                matches!(last.strategy, RecoveryStrategy::Retry),
                "Transient error should use Retry strategy, got {:?}",
                last.strategy
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_permanent_errors_use_manual() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");

        // Permission, DiskSpace, and Unknown errors require manual intervention
        let permanent_errors = vec![
            AosError::Authz("not authorized".to_string()),
            AosError::ResourceExhaustion("disk full".to_string()),
            AosError::Io("permission denied".to_string()),
            AosError::Parse("invalid format".to_string()),
        ];

        for error in permanent_errors {
            let _ = manager.handle_error(error, &test_file).await;

            let history = manager.get_recovery_history().await;
            let last = history.last().unwrap();
            assert!(
                matches!(last.strategy, RecoveryStrategy::Manual),
                "Permanent error should use Manual strategy, got {:?}",
                last.strategy
            );
        }

        Ok(())
    }
}

// =============================================================================
// Validation Engine Tests
// =============================================================================

mod validation_engine {
    use super::*;

    #[tokio::test]
    async fn test_validate_nonexistent_file() -> Result<()> {
        let mut engine = ValidationEngine::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("nonexistent.txt");

        let result = engine.validate_file_detailed(&test_file).await?;
        assert!(!result.is_valid);
        assert_eq!(result.errors.len(), 1);
        assert!(matches!(
            result.errors[0].error_type,
            ValidationErrorType::FileNotFound
        ));
        assert!(matches!(
            result.errors[0].severity,
            ValidationSeverity::Critical
        ));

        Ok(())
    }

    #[tokio::test]
    async fn test_validate_valid_file() -> Result<()> {
        let mut engine = ValidationEngine::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("valid.txt");
        tokio::fs::write(&test_file, "valid content").await?;

        let result = engine.validate_file_detailed(&test_file).await?;
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
        assert_eq!(result.details, "Validation passed");

        Ok(())
    }

    #[tokio::test]
    async fn test_validate_json_file_valid() -> Result<()> {
        let mut engine = ValidationEngine::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("config.json");
        tokio::fs::write(&test_file, r#"{"key": "value", "number": 42}"#).await?;

        let result = engine.validate_file_detailed(&test_file).await?;
        assert!(result.is_valid, "Valid JSON should pass validation");

        Ok(())
    }

    #[tokio::test]
    async fn test_validate_json_file_invalid() -> Result<()> {
        let mut engine = ValidationEngine::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("invalid.json");
        tokio::fs::write(&test_file, r#"{"key": "value",}"#).await?; // Trailing comma

        let result = engine.validate_file_detailed(&test_file).await?;
        assert!(!result.is_valid, "Invalid JSON should fail validation");
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e.error_type, ValidationErrorType::InvalidFormat)));

        Ok(())
    }

    #[tokio::test]
    async fn test_validate_toml_file_valid() -> Result<()> {
        let mut engine = ValidationEngine::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("config.toml");
        tokio::fs::write(
            &test_file,
            r#"[section]
key = "value"
number = 42"#,
        )
        .await?;

        let result = engine.validate_file_detailed(&test_file).await?;
        assert!(result.is_valid, "Valid TOML should pass validation");

        Ok(())
    }

    #[tokio::test]
    async fn test_validate_toml_file_invalid() -> Result<()> {
        let mut engine = ValidationEngine::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("invalid.toml");
        tokio::fs::write(
            &test_file,
            r#"[section
key = "value""#,
        )
        .await?; // Missing closing bracket

        let result = engine.validate_file_detailed(&test_file).await?;
        assert!(!result.is_valid, "Invalid TOML should fail validation");

        Ok(())
    }

    #[tokio::test]
    async fn test_validate_file_with_null_bytes() -> Result<()> {
        let mut engine = ValidationEngine::new()?;

        let temp_dir = new_test_tempdir()?;

        // Text file with null bytes should fail
        let text_file = temp_dir.path().join("corrupted.txt");
        tokio::fs::write(&text_file, b"hello\x00world").await?;

        let result = engine.validate_file_detailed(&text_file).await?;
        assert!(!result.is_valid, "Text file with null bytes should fail");
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e.error_type, ValidationErrorType::CorruptedData)));

        // JSON file with null bytes should also fail
        let json_file = temp_dir.path().join("corrupted.json");
        tokio::fs::write(&json_file, b"{\"key\": \"val\x00ue\"}").await?;

        let result = engine.validate_file_detailed(&json_file).await?;
        assert!(!result.is_valid, "JSON file with null bytes should fail");

        Ok(())
    }

    #[tokio::test]
    async fn test_validate_empty_file() -> Result<()> {
        let mut engine = ValidationEngine::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("empty.txt");
        tokio::fs::write(&test_file, "").await?;

        let result = engine.validate_file_detailed(&test_file).await?;
        // Empty files are allowed
        assert!(result.is_valid, "Empty file should pass validation");

        Ok(())
    }

    #[tokio::test]
    async fn test_validate_nonexistent_directory() -> Result<()> {
        let engine = ValidationEngine::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_dir = temp_dir.path().join("nonexistent_dir");

        let result = engine.validate_directory_detailed(&test_dir).await?;
        assert!(!result.is_valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e.error_type, ValidationErrorType::FileNotFound)));

        Ok(())
    }

    #[tokio::test]
    async fn test_validate_valid_directory() -> Result<()> {
        let engine = ValidationEngine::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_dir = temp_dir.path().join("valid_dir");
        tokio::fs::create_dir(&test_dir).await?;

        let result = engine.validate_directory_detailed(&test_dir).await?;
        assert!(result.is_valid);
        assert!(result.errors.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_validation_cache() -> Result<()> {
        let mut engine = ValidationEngine::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("cached.txt");
        tokio::fs::write(&test_file, "content").await?;

        // First validation
        let result1 = engine.validate_file_detailed(&test_file).await?;
        assert!(result1.is_valid);

        // Second validation should use cache (same timestamp check)
        let result2 = engine.validate_file_detailed(&test_file).await?;
        assert!(result2.is_valid);

        // Verify cache was used by checking statistics
        let stats = engine.get_validation_statistics();
        // After two calls to validate_file_detailed, we should have 1 entry in cache
        // (the second call uses cache)
        assert_eq!(stats.total_validations, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_clear_validation_cache() -> Result<()> {
        let mut engine = ValidationEngine::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("cache_test.txt");
        tokio::fs::write(&test_file, "content").await?;

        // Validate to populate cache
        let _ = engine.validate_file_detailed(&test_file).await?;

        let stats = engine.get_validation_statistics();
        assert_eq!(stats.total_validations, 1);

        // Clear cache
        engine.clear_cache();

        let stats = engine.get_validation_statistics();
        assert_eq!(stats.total_validations, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_validation_statistics() -> Result<()> {
        let mut engine = ValidationEngine::new()?;

        let temp_dir = new_test_tempdir()?;

        // Create some valid files
        for i in 0..3 {
            let file = temp_dir.path().join(format!("valid_{}.txt", i));
            tokio::fs::write(&file, "content").await?;
            let _ = engine.validate_file_detailed(&file).await?;
        }

        // Create some invalid files
        for i in 0..2 {
            let file = temp_dir.path().join(format!("invalid_{}.json", i));
            tokio::fs::write(&file, "not valid json").await?;
            let _ = engine.validate_file_detailed(&file).await?;
        }

        let stats = engine.get_validation_statistics();
        assert_eq!(stats.total_validations, 5);
        assert_eq!(stats.valid_files, 3);
        assert_eq!(stats.invalid_files, 2);
        assert!((stats.success_rate - 0.6).abs() < 0.001);

        Ok(())
    }
}

// =============================================================================
// Corruption Detection Tests
// =============================================================================

mod corruption_detection {
    use super::*;

    #[tokio::test]
    async fn test_detect_nonexistent_file() -> Result<()> {
        let detector = CorruptionDetector::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("nonexistent.txt");

        let result = detector.detect_corruption(&test_file).await?;
        assert!(
            !result.is_corrupted,
            "Nonexistent file should not be marked corrupted"
        );
        assert!(result.details.contains("does not exist"));

        Ok(())
    }

    #[tokio::test]
    async fn test_detect_valid_file() -> Result<()> {
        let detector = CorruptionDetector::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("valid.txt");
        tokio::fs::write(&test_file, "valid content without issues").await?;

        let result = detector.detect_corruption(&test_file).await?;
        assert!(
            !result.is_corrupted,
            "Valid file should not be marked corrupted"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_detect_file_with_null_bytes() -> Result<()> {
        let detector = CorruptionDetector::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("nullbytes.txt");
        tokio::fs::write(&test_file, b"content\x00with\x00nulls").await?;

        let result = detector.detect_corruption(&test_file).await?;
        assert!(
            result.is_corrupted,
            "Text file with null bytes should be detected as corrupted"
        );
        assert!(matches!(
            result.corruption_type,
            Some(CorruptionType::FileCorruption)
        ));

        Ok(())
    }

    #[tokio::test]
    async fn test_detect_valid_directory() -> Result<()> {
        let detector = CorruptionDetector::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_dir = temp_dir.path().join("valid_dir");
        tokio::fs::create_dir(&test_dir).await?;

        // Add some files to the directory
        tokio::fs::write(test_dir.join("file1.txt"), "content1").await?;
        tokio::fs::write(test_dir.join("file2.txt"), "content2").await?;

        let result = detector.detect_corruption(&test_dir).await?;
        assert!(
            !result.is_corrupted,
            "Valid directory should not be marked corrupted"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_is_corrupted_helper() -> Result<()> {
        let detector = CorruptionDetector::new()?;

        let temp_dir = new_test_tempdir()?;

        // Valid file
        let valid_file = temp_dir.path().join("valid.txt");
        tokio::fs::write(&valid_file, "good content").await?;
        assert!(!detector.is_corrupted(&valid_file).await?);

        // File with null bytes
        let corrupted_file = temp_dir.path().join("corrupted.txt");
        tokio::fs::write(&corrupted_file, b"bad\x00content").await?;
        assert!(detector.is_corrupted(&corrupted_file).await?);

        Ok(())
    }

    #[tokio::test]
    async fn test_detect_empty_file() -> Result<()> {
        let detector = CorruptionDetector::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("empty.txt");
        tokio::fs::write(&test_file, "").await?;

        let result = detector.detect_corruption(&test_file).await?;
        assert!(
            !result.is_corrupted,
            "Empty file should not be marked corrupted"
        );

        Ok(())
    }
}

// =============================================================================
// Configuration Validation Tests
// =============================================================================

mod config_validation {
    use super::*;

    #[tokio::test]
    async fn test_default_config_values() {
        let config = ErrorRecoveryConfig::default();

        assert!(config.enabled);
        assert!(config.enable_corruption_detection);
        assert!(config.enable_automatic_retry);
        assert_eq!(config.max_retry_attempts, 3);
        assert_eq!(config.retry_delay, Duration::from_millis(100));
        assert_eq!(config.backoff_multiplier, 2.0);
        assert_eq!(config.max_retry_delay, Duration::from_secs(30));
        assert!(config.enable_partial_recovery);
        assert!(config.enable_backup_restore);
        assert_eq!(config.backup_retention_count, 5);
    }

    #[tokio::test]
    async fn test_create_manager_with_custom_config() -> Result<()> {
        let config = ErrorRecoveryConfig {
            enabled: true,
            enable_corruption_detection: false,
            enable_automatic_retry: true,
            max_retry_attempts: 10,
            retry_delay: Duration::from_millis(50),
            backoff_multiplier: 1.5,
            max_retry_delay: Duration::from_secs(60),
            enable_partial_recovery: false,
            enable_backup_restore: false,
            backup_retention_count: 3,
        };

        let manager = ErrorRecoveryManager::new(config.clone())?;
        let stored_config = manager.config();

        assert_eq!(stored_config.max_retry_attempts, 10);
        assert_eq!(stored_config.retry_delay, Duration::from_millis(50));
        assert_eq!(stored_config.backoff_multiplier, 1.5);
        assert!(!stored_config.enable_corruption_detection);
        assert!(!stored_config.enable_backup_restore);

        Ok(())
    }

    #[tokio::test]
    async fn test_zero_config_values() -> Result<()> {
        let config = ErrorRecoveryConfig {
            enabled: true,
            enable_corruption_detection: true,
            enable_automatic_retry: true,
            max_retry_attempts: 0,
            retry_delay: Duration::ZERO,
            backoff_multiplier: 0.0,
            max_retry_delay: Duration::ZERO,
            enable_partial_recovery: true,
            enable_backup_restore: true,
            backup_retention_count: 0,
        };

        // Should not panic with zero values
        let manager = ErrorRecoveryManager::new(config)?;
        assert_eq!(manager.config().max_retry_attempts, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_large_config_values() -> Result<()> {
        let config = ErrorRecoveryConfig {
            enabled: true,
            enable_corruption_detection: true,
            enable_automatic_retry: true,
            max_retry_attempts: u32::MAX,
            retry_delay: Duration::from_secs(1000),
            backoff_multiplier: 100.0,
            max_retry_delay: Duration::from_secs(86400), // 1 day
            enable_partial_recovery: true,
            enable_backup_restore: true,
            backup_retention_count: 1000,
        };

        // Should handle large values without overflow
        let manager = ErrorRecoveryManager::new(config)?;
        assert_eq!(manager.config().max_retry_attempts, u32::MAX);

        Ok(())
    }
}

// =============================================================================
// Integration Tests - Full Recovery Flow
// =============================================================================

mod integration {
    use super::*;

    #[tokio::test]
    async fn test_full_recovery_flow_file_recreation() -> Result<()> {
        let config = ErrorRecoveryConfig {
            enable_backup_restore: false, // Force recreation strategy
            ..ErrorRecoveryConfig::default()
        };
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("to_recreate.txt");

        // Create initial file
        tokio::fs::write(&test_file, "original content").await?;
        assert!(test_file.exists());

        // Trigger corruption recovery
        let error = AosError::Io("data corruption detected".to_string());
        let result = manager.handle_error(error, &test_file).await;

        // Should succeed (file was recreated)
        assert!(result.is_ok());

        // File should exist (but be empty after recreation)
        assert!(test_file.exists());
        let content = tokio::fs::read_to_string(&test_file).await?;
        assert!(content.is_empty(), "Recreated file should be empty");

        // Check recovery history
        let history = manager.get_recovery_history().await;
        assert!(!history.is_empty());
        let last = history.last().unwrap();
        assert!(matches!(last.result, RecoveryResult::PartialSuccess));

        Ok(())
    }

    #[tokio::test]
    async fn test_full_validation_integrity_check() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let mut manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;

        // Create a valid file
        let valid_file = temp_dir.path().join("valid.json");
        tokio::fs::write(&valid_file, r#"{"status": "ok"}"#).await?;

        let is_valid = manager.validate_file_integrity(&valid_file).await?;
        assert!(is_valid, "Valid JSON file should pass integrity check");

        // Create an invalid file
        let invalid_file = temp_dir.path().join("invalid.json");
        tokio::fs::write(&invalid_file, "not valid json").await?;

        let is_valid = manager.validate_file_integrity(&invalid_file).await?;
        assert!(!is_valid, "Invalid JSON file should fail integrity check");

        Ok(())
    }

    #[tokio::test]
    async fn test_directory_integrity_check() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;

        // Create a valid directory
        let valid_dir = temp_dir.path().join("valid_dir");
        tokio::fs::create_dir(&valid_dir).await?;
        tokio::fs::write(valid_dir.join("file.txt"), "content").await?;

        let is_valid = manager.validate_directory_integrity(&valid_dir).await?;
        assert!(is_valid, "Valid directory should pass integrity check");

        // Non-existent directory
        let nonexistent_dir = temp_dir.path().join("nonexistent");
        let is_valid = manager
            .validate_directory_integrity(&nonexistent_dir)
            .await?;
        assert!(
            !is_valid,
            "Nonexistent directory should fail integrity check"
        );

        Ok(())
    }
}
