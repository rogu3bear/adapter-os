//! Error Handling Integration Tests
//!
//! Cross-crate integration tests verifying:
//! 1. Error propagation from adapteros-db to adapteros-server-api
//! 2. FailureCode consistency across crates
//! 3. Error serialization round-trip through API
//! 4. CLI error code formatting
//!
//! These tests ensure error types flow correctly across crate boundaries
//! and maintain consistency in error reporting throughout the system.

#![allow(unused_imports)]
#![allow(deprecated)]
#![allow(clippy::needless_borrows_for_generic_args)]

use adapteros_api_types::{ErrorResponse, FailureCode};
use adapteros_cli::error_codes::{all_error_codes, find_by_code, ExitCode};
use adapteros_core::errors::storage::AosStorageError;
use adapteros_core::AosError;
use adapteros_db::error_classification::{classify_sqlx_error, DatabaseBackend, DbErrorClass};

// ============================================================================
// Section 1: Error Propagation from adapteros-db to adapteros-server-api
// ============================================================================

mod error_propagation {
    use super::*;

    /// Test that AosError variants properly convert to API error responses
    #[test]
    fn test_aos_error_to_api_response_conversion() {
        // Database error should map to appropriate status
        let db_err = AosError::Database("connection failed".to_string());
        let api_response = ErrorResponse::new(&db_err.to_string()).with_code("DATABASE_ERROR");
        assert_eq!(api_response.code, "DATABASE_ERROR");
        assert!(api_response.message.contains("connection failed"));
    }

    /// Test that storage errors maintain context through conversion
    #[test]
    fn test_storage_error_context_preservation() {
        let storage_err = AosStorageError::HostUnreachable {
            host: "db.example.com".to_string(),
            reason: "connection refused".to_string(),
            error_code: Some("08001".to_string()),
        };

        let msg = storage_err.to_string();
        assert!(msg.contains("db.example.com"), "Host should be preserved");
        assert!(
            msg.contains("connection refused"),
            "Reason should be preserved"
        );
    }

    /// Test migration error propagation
    #[test]
    fn test_migration_error_propagation() {
        let migration_err = AosStorageError::MigrationChecksumMismatch {
            filename: "V001__init.sql".to_string(),
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };

        let msg = migration_err.to_string();
        assert!(msg.contains("V001__init.sql"));
        assert!(msg.contains("abc123"));
        assert!(msg.contains("def456"));

        // Verify API response can capture this context
        let api_response = ErrorResponse::new(&msg).with_code("MIGRATION_CHECKSUM_MISMATCH");
        assert_eq!(api_response.code, "MIGRATION_CHECKSUM_MISMATCH");
    }

    /// Test cache error propagation
    #[test]
    fn test_cache_error_propagation() {
        let cache_err = AosStorageError::CacheStale {
            key: "model:abc123".to_string(),
            expired_secs: 300,
            ttl_secs: 3600,
        };

        let msg = cache_err.to_string();
        assert!(msg.contains("model:abc123"));
        assert!(msg.contains("300"));
        assert!(msg.contains("3600"));
    }

    /// Test that dual-write inconsistency errors are properly formatted
    #[test]
    fn test_dual_write_error_propagation() {
        let dual_write_err = AosStorageError::DualWriteInconsistency {
            entity_type: "adapter".to_string(),
            entity_id: "test-adapter-123".to_string(),
            reason: "KV write failed after SQL commit".to_string(),
        };

        let msg = dual_write_err.to_string();
        assert!(msg.contains("adapter"));
        assert!(msg.contains("test-adapter-123"));
        assert!(msg.contains("KV write failed"));
    }

    /// Test database error classification flows correctly
    #[test]
    fn test_db_error_classification_consistency() {
        // Pool timeout should be retriable
        let pool_err = sqlx::Error::PoolTimedOut;
        let class = classify_sqlx_error(&pool_err, DatabaseBackend::Sqlite);
        assert_eq!(class, DbErrorClass::PoolExhausted);
        assert!(class.is_retryable());

        // Pool closed should not be retriable
        let closed_err = sqlx::Error::PoolClosed;
        let class = classify_sqlx_error(&closed_err, DatabaseBackend::Sqlite);
        assert_eq!(class, DbErrorClass::Other);
        assert!(!class.is_retryable());
    }
}

// ============================================================================
// Section 2: FailureCode Consistency Across Crates
// ============================================================================

mod failure_code_consistency {
    use super::*;

    /// All FailureCode variants for exhaustive testing
    pub(crate) const ALL_FAILURE_CODES: &[FailureCode] = &[
        FailureCode::MigrationInvalid,
        FailureCode::ModelLoadFailed,
        FailureCode::OutOfMemory,
        FailureCode::TraceWriteFailed,
        FailureCode::ReceiptMismatch,
        FailureCode::PolicyDivergence,
        FailureCode::BackendFallback,
        FailureCode::TenantAccessDenied,
        FailureCode::KvQuotaExceeded,
        FailureCode::WorkerOverloaded,
        FailureCode::CpuThrottled,
        FailureCode::FileDescriptorExhausted,
        FailureCode::ThreadPoolSaturated,
        FailureCode::GpuUnavailable,
        FailureCode::BootDbUnreachable,
        FailureCode::BootMigrationFailed,
        FailureCode::BootSeedFailed,
        FailureCode::BootNoWorkers,
        FailureCode::BootNoModels,
        FailureCode::BootDependencyTimeout,
        FailureCode::BootBackgroundTaskFailed,
        FailureCode::BootConfigInvalid,
        FailureCode::BootBootstrapFailed,
        FailureCode::MigrationFileMissing,
        FailureCode::MigrationChecksumMismatch,
        FailureCode::MigrationOutOfOrder,
        FailureCode::DownMigrationBlocked,
        FailureCode::SchemaVersionAhead,
        FailureCode::CacheStale,
        FailureCode::CacheKeyNondeterministic,
        FailureCode::CacheSerializationError,
        FailureCode::CacheInvalidationFailed,
        FailureCode::DnsResolutionFailed,
        FailureCode::TlsCertificateError,
        FailureCode::ProxyConnectionFailed,
        FailureCode::EnvironmentMismatch,
        FailureCode::RateLimiterNotConfigured,
        FailureCode::InvalidRateLimitConfig,
        FailureCode::ThunderingHerdRejected,
    ];

    /// Test that all FailureCode variants round-trip through as_str/parse_code
    #[test]
    fn test_failure_code_round_trip() {
        for code in ALL_FAILURE_CODES {
            let str_code = code.as_str();
            let parsed = FailureCode::parse_code(str_code);
            assert_eq!(
                parsed,
                Some(*code),
                "Round-trip failed for {:?} -> {} -> {:?}",
                code,
                str_code,
                parsed
            );
        }
    }

    /// Test that all FailureCode string representations are SCREAMING_SNAKE_CASE
    #[test]
    fn test_failure_code_format() {
        for code in ALL_FAILURE_CODES {
            let str_code = code.as_str();
            assert!(
                str_code.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
                "Code {:?} has non-uppercase string: {}",
                code,
                str_code
            );
        }
    }

    /// Test that retryable codes are consistent with their semantics
    #[test]
    fn test_failure_code_retryable_consistency() {
        // These should all be retryable
        let retryable_codes = [
            FailureCode::WorkerOverloaded,
            FailureCode::CpuThrottled,
            FailureCode::FileDescriptorExhausted,
            FailureCode::ThreadPoolSaturated,
            FailureCode::GpuUnavailable,
            FailureCode::OutOfMemory,
            FailureCode::BootDbUnreachable,
            FailureCode::BootDependencyTimeout,
            FailureCode::CacheStale,
            FailureCode::DnsResolutionFailed,
            FailureCode::ProxyConnectionFailed,
            FailureCode::ThunderingHerdRejected,
        ];

        for code in &retryable_codes {
            assert!(code.is_retryable(), "{:?} should be retryable", code);
        }

        // These should NOT be retryable
        let non_retryable_codes = [
            FailureCode::MigrationInvalid,
            FailureCode::TenantAccessDenied,
            FailureCode::PolicyDivergence,
            FailureCode::TlsCertificateError,
            FailureCode::EnvironmentMismatch,
            FailureCode::MigrationChecksumMismatch,
            FailureCode::InvalidRateLimitConfig,
            FailureCode::CacheKeyNondeterministic,
        ];

        for code in &non_retryable_codes {
            assert!(!code.is_retryable(), "{:?} should NOT be retryable", code);
        }
    }

    /// Test FailureCode integration with ErrorResponse
    #[test]
    fn test_failure_code_in_error_response() {
        let response = ErrorResponse::new("Worker is overloaded")
            .with_code("WORKER_OVERLOADED")
            .with_failure_code(FailureCode::WorkerOverloaded);

        assert_eq!(response.code, "WORKER_OVERLOADED");
        assert_eq!(response.failure_code, Some(FailureCode::WorkerOverloaded));
        assert!(response.failure_code.unwrap().is_retryable());
    }

    /// Test that ErrorResponse auto-parses failure code from code string
    #[test]
    fn test_error_response_auto_parse_failure_code() {
        let response = ErrorResponse::new("Out of memory").with_code("OUT_OF_MEMORY");

        // ErrorResponse.with_code should auto-parse recognized codes
        assert_eq!(response.failure_code, Some(FailureCode::OutOfMemory));
    }
}

// ============================================================================
// Section 3: Error Serialization Round-Trip Through API
// ============================================================================

mod error_serialization {
    use super::*;

    /// Test ErrorResponse JSON serialization round-trip
    #[test]
    fn test_error_response_json_round_trip() {
        let original = ErrorResponse::new("Test error message")
            .with_code("TEST_ERROR")
            .with_string_details("Additional context");

        let json = serde_json::to_string(&original).expect("Failed to serialize");
        let parsed: ErrorResponse = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(parsed.message, original.message);
        assert_eq!(parsed.code, original.code);
        assert_eq!(parsed.details, original.details);
    }

    /// Test ErrorResponse with FailureCode serialization
    #[test]
    fn test_error_response_with_failure_code_round_trip() {
        let original = ErrorResponse::new("Memory pressure detected")
            .with_code("OUT_OF_MEMORY")
            .with_failure_code(FailureCode::OutOfMemory);

        let json = serde_json::to_string(&original).expect("Failed to serialize");
        let parsed: ErrorResponse = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(parsed.failure_code, Some(FailureCode::OutOfMemory));
    }

    /// Test that FailureCode serializes as SCREAMING_SNAKE_CASE
    #[test]
    fn test_failure_code_serde_format() {
        let code = FailureCode::OutOfMemory;
        let json = serde_json::to_string(&code).expect("Failed to serialize");

        // Should serialize as "OUT_OF_MEMORY" (with quotes)
        assert_eq!(json, "\"OUT_OF_MEMORY\"");

        // Should deserialize back
        let parsed: FailureCode = serde_json::from_str(&json).expect("Failed to deserialize");
        assert_eq!(parsed, FailureCode::OutOfMemory);
    }

    /// Test ErrorResponse schema_version is included
    #[test]
    fn test_error_response_schema_version() {
        let response = ErrorResponse::new("Test error");
        let json = serde_json::to_string(&response).expect("Failed to serialize");

        // Should contain schema_version field
        assert!(
            json.contains("schema_version"),
            "JSON should contain schema_version"
        );

        // Parse and verify
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("schema_version").is_some());
    }

    /// Test error details can be structured JSON
    #[test]
    fn test_error_response_structured_details() {
        let details = serde_json::json!({
            "resource": "adapter",
            "id": "test-123",
            "retry_after_ms": 5000
        });

        let response = ErrorResponse::new("Resource temporarily unavailable")
            .with_code("SERVICE_UNAVAILABLE")
            .with_details(details.clone());

        let json = serde_json::to_string(&response).expect("Failed to serialize");
        let parsed: ErrorResponse = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(parsed.details, Some(details));
    }

    /// Test that all FailureCode variants serialize/deserialize correctly
    #[test]
    fn test_all_failure_codes_serde() {
        use super::failure_code_consistency::ALL_FAILURE_CODES;

        for code in ALL_FAILURE_CODES {
            let json = serde_json::to_string(code).expect("Failed to serialize");
            let parsed: FailureCode = serde_json::from_str(&json).expect("Failed to deserialize");
            assert_eq!(parsed, *code, "Serde round-trip failed for {:?}", code);
        }
    }
}

// ============================================================================
// Section 4: CLI Error Code Formatting
// ============================================================================

mod cli_error_codes {
    use super::*;

    /// Test that all CLI error codes are unique
    #[test]
    fn test_cli_error_codes_unique() {
        let codes = all_error_codes();
        let mut seen = std::collections::HashSet::new();
        for code in codes {
            assert!(
                seen.insert(code.code),
                "Duplicate error code: {}",
                code.code
            );
        }
    }

    /// Test CLI error code lookup by code string
    #[test]
    #[allow(deprecated)] // Testing the deprecated function intentionally
    fn test_cli_error_code_lookup() {
        // Known codes should be found
        assert!(find_by_code("E1001").is_some());
        assert!(find_by_code("E2001").is_some());
        assert!(find_by_code("E3001").is_some());
        assert!(find_by_code("E9001").is_some());

        // Unknown codes should return None
        assert!(find_by_code("E9999").is_none());
        assert!(find_by_code("UNKNOWN").is_none());
    }

    /// Test compile-time checked error code mapping via AosError::ecode()
    ///
    /// This test verifies that the unified error registry properly maps
    /// AosError variants to ECode values at compile-time.
    ///
    /// Uses the hierarchical AosError from adapteros_core::errors module
    /// which wraps categorical sub-enums for type-safe error handling.
    #[test]
    fn test_aos_error_ecode_mapping() {
        use adapteros_core::errors::{
            crypto::AosCryptoError, policy::AosPolicyError, resource::AosResourceError,
            AosError as HierarchicalError, ECode, HasECode,
        };

        // Crypto errors
        let hash_err = HierarchicalError::Crypto(AosCryptoError::InvalidHash("bad".to_string()));
        assert_eq!(hash_err.ecode(), ECode::E1004);

        // Policy errors
        let policy_err =
            HierarchicalError::Policy(AosPolicyError::Violation("test violation".to_string()));
        assert_eq!(policy_err.ecode(), ECode::E2002);

        let determinism_err = HierarchicalError::Policy(AosPolicyError::DeterminismViolation(
            "seed mismatch".to_string(),
        ));
        assert_eq!(determinism_err.ecode(), ECode::E2001);

        // Resource exhaustion errors
        let cpu_err = HierarchicalError::Resource(AosResourceError::CpuThrottled {
            reason: "high load".to_string(),
            usage_percent: 95.0,
            limit_percent: 80.0,
            backoff_ms: 1000,
        });
        assert_eq!(cpu_err.ecode(), ECode::E9005);

        let oom_err = HierarchicalError::Resource(AosResourceError::OutOfMemory {
            reason: "heap exhausted".to_string(),
            used_mb: 8000,
            limit_mb: 8192,
            restart_imminent: true,
        });
        assert_eq!(oom_err.ecode(), ECode::E9006);

        let fd_err = HierarchicalError::Resource(AosResourceError::FileDescriptorExhausted {
            current: 1024,
            limit: 1024,
            suggestion: "increase ulimit".to_string(),
        });
        assert_eq!(fd_err.ecode(), ECode::E9007);

        let thread_err = HierarchicalError::Resource(AosResourceError::ThreadPoolSaturated {
            active: 64,
            max: 64,
            queued: 100,
            estimated_wait_ms: 500,
        });
        assert_eq!(thread_err.ecode(), ECode::E9008);

        let gpu_err = HierarchicalError::Resource(AosResourceError::GpuUnavailable {
            reason: "device busy".to_string(),
            device_id: Some("gpu:0".to_string()),
            cpu_fallback_available: true,
            is_transient: true,
        });
        assert_eq!(gpu_err.ecode(), ECode::E9009);
    }

    /// Test that CLI error codes have proper category prefixes
    #[test]
    fn test_cli_error_code_categories() {
        let codes = all_error_codes();
        for code in codes {
            let prefix = &code.code[0..2];
            match prefix {
                "E1" => assert_eq!(code.category, "Crypto/Signing"),
                "E2" => assert_eq!(code.category, "Policy/Determinism"),
                "E3" => assert_eq!(code.category, "Kernels/Build/Manifest"),
                "E4" => assert_eq!(code.category, "Telemetry/Chain"),
                "E5" => assert_eq!(code.category, "Artifacts/CAS"),
                "E6" => assert_eq!(code.category, "Adapters/DIR"),
                "E7" => assert_eq!(code.category, "Node/Cluster"),
                "E8" => assert_eq!(code.category, "CLI/Config"),
                "E9" => assert_eq!(code.category, "OS/Environment"),
                _ => panic!("Invalid code prefix: {}", prefix),
            }
        }
    }

    /// Test ExitCode categories are consistent
    #[test]
    fn test_exit_code_categories() {
        assert_eq!(ExitCode::Success.category(), "Success");
        assert_eq!(ExitCode::GeneralError.category(), "General");
        assert_eq!(ExitCode::Config.category(), "Configuration");
        assert_eq!(ExitCode::Database.category(), "Database");
        assert_eq!(ExitCode::Network.category(), "Network");
        assert_eq!(ExitCode::Crypto.category(), "Crypto");
        assert_eq!(ExitCode::PolicyViolation.category(), "Policy");
        assert_eq!(ExitCode::Validation.category(), "Validation");
        assert_eq!(ExitCode::Auth.category(), "Auth");
        assert_eq!(ExitCode::Worker.category(), "Worker/Job");
        assert_eq!(ExitCode::Io.category(), "Subsystem");
        assert_eq!(ExitCode::Telemetry.category(), "Domain");
    }

    /// Test ExitCode conversion from AosError
    #[test]
    fn test_exit_code_from_aos_error() {
        // Config error
        let config_err = AosError::Config("test".to_string());
        assert_eq!(ExitCode::from(&config_err), ExitCode::Config);

        // Database error
        let db_err = AosError::Database("test".to_string());
        assert_eq!(ExitCode::from(&db_err), ExitCode::Database);

        // Policy error
        let policy_err = AosError::PolicyViolation("test".to_string());
        assert_eq!(ExitCode::from(&policy_err), ExitCode::PolicyViolation);

        // Crypto error
        let crypto_err = AosError::Crypto("test".to_string());
        assert_eq!(ExitCode::from(&crypto_err), ExitCode::Crypto);

        // Validation error
        let validation_err = AosError::Validation("test".to_string());
        assert_eq!(ExitCode::from(&validation_err), ExitCode::Validation);

        // Rate limiting errors
        let rate_limit_err = AosError::RateLimiterNotConfigured {
            reason: "missing config".to_string(),
            limiter_name: "api".to_string(),
        };
        assert_eq!(ExitCode::from(&rate_limit_err), ExitCode::RateLimitConfig);

        // Thundering herd
        let herd_err = AosError::ThunderingHerdRejected {
            reason: "too many requests".to_string(),
            retry_after_ms: 5000,
        };
        assert_eq!(ExitCode::from(&herd_err), ExitCode::ResourceExhaustion);
    }

    /// Test ExitCode numeric ranges are correct
    #[test]
    fn test_exit_code_numeric_ranges() {
        // Success is 0
        assert_eq!(ExitCode::Success as u8, 0);

        // General errors (1-9)
        assert!((1..=9).contains(&(ExitCode::GeneralError as u8)));
        assert!((1..=9).contains(&(ExitCode::InternalError as u8)));
        assert!((1..=9).contains(&(ExitCode::NotFound as u8)));
        assert!((1..=9).contains(&(ExitCode::Timeout as u8)));

        // Configuration errors (10-19)
        assert!((10..=19).contains(&(ExitCode::Config as u8)));
        assert!((10..=19).contains(&(ExitCode::DeprecatedFlag as u8)));
        assert!((10..=19).contains(&(ExitCode::OutputFormat as u8)));
        assert!((10..=19).contains(&(ExitCode::RateLimitConfig as u8)));

        // Database errors (20-29)
        assert!((20..=29).contains(&(ExitCode::Database as u8)));
        assert!((20..=29).contains(&(ExitCode::Sqlite as u8)));
        assert!((20..=29).contains(&(ExitCode::Sqlx as u8)));

        // Network errors (30-39)
        assert!((30..=39).contains(&(ExitCode::Network as u8)));
        assert!((30..=39).contains(&(ExitCode::Http as u8)));
        assert!((30..=39).contains(&(ExitCode::CircuitBreakerOpen as u8)));

        // Crypto errors (40-49)
        assert!((40..=49).contains(&(ExitCode::Crypto as u8)));
        assert!((40..=49).contains(&(ExitCode::InvalidHash as u8)));

        // Policy errors (50-59)
        assert!((50..=59).contains(&(ExitCode::PolicyViolation as u8)));
        assert!((50..=59).contains(&(ExitCode::DeterminismViolation as u8)));

        // Validation errors (60-69)
        assert!((60..=69).contains(&(ExitCode::Validation as u8)));
        assert!((60..=69).contains(&(ExitCode::InvalidCPID as u8)));

        // Auth errors (70-79)
        assert!((70..=79).contains(&(ExitCode::Auth as u8)));
        assert!((70..=79).contains(&(ExitCode::Authz as u8)));

        // Worker/Job errors (80-89)
        assert!((80..=89).contains(&(ExitCode::Worker as u8)));
        assert!((80..=89).contains(&(ExitCode::Job as u8)));

        // Subsystem errors (90-99)
        assert!((90..=99).contains(&(ExitCode::Io as u8)));
        assert!((90..=99).contains(&(ExitCode::Memory as u8)));
        assert!((90..=99).contains(&(ExitCode::Kernel as u8)));

        // Domain errors (100-119)
        assert!((100..=119).contains(&(ExitCode::Telemetry as u8)));
        assert!((100..=119).contains(&(ExitCode::Artifact as u8)));
        assert!((100..=119).contains(&(ExitCode::Training as u8)));

        // Model Hub errors (120-129)
        assert!((120..=129).contains(&(ExitCode::DownloadFailed as u8)));
        assert!((120..=129).contains(&(ExitCode::ModelNotFound as u8)));
    }

    /// Test ExitCode to i32 conversion
    #[test]
    fn test_exit_code_to_i32() {
        let code = ExitCode::Config;
        let value: i32 = code.into();
        assert_eq!(value, 10);

        let code = ExitCode::Success;
        let value: i32 = code.into();
        assert_eq!(value, 0);
    }

    /// Test error code display formatting contains required fields
    #[test]
    #[allow(deprecated)] // Testing the deprecated function intentionally
    fn test_cli_error_code_display_format() {
        if let Some(code) = find_by_code("E3001") {
            let display = format!("{}", code);

            // Should contain all required fields
            assert!(
                display.contains("Error Code:"),
                "Should contain 'Error Code:'"
            );
            assert!(display.contains("Category:"), "Should contain 'Category:'");
            assert!(display.contains("Cause:"), "Should contain 'Cause:'");
            assert!(display.contains("Fix:"), "Should contain 'Fix:'");
        }
    }
}

// ============================================================================
// Section 5: Cross-Crate Error Type Compatibility
// ============================================================================

mod cross_crate_compatibility {
    use super::*;

    /// Test that AosError can be converted to string and back via ErrorResponse
    #[test]
    fn test_aos_error_to_error_response_cycle() {
        let original = AosError::Database("test database error".to_string());
        let response = ErrorResponse::new(&original.to_string()).with_code("DATABASE_ERROR");

        // The error message should be preserved
        assert!(response.message.contains("test database error"));
        assert_eq!(response.code, "DATABASE_ERROR");
    }

    /// Test that storage errors can be used with ErrorResponse
    #[test]
    fn test_storage_error_to_error_response() {
        let storage_err = AosStorageError::QueryTimeout {
            timeout_ms: 30000,
            query_context: "SELECT * FROM large_table".to_string(),
        };

        let response = ErrorResponse::new(&storage_err.to_string()).with_code("QUERY_TIMEOUT");

        assert!(response.message.contains("30000ms"));
        assert!(response.message.contains("large_table"));
    }

    /// Test DbErrorClass consistency with FailureCode semantics
    #[test]
    fn test_db_error_class_and_failure_code_alignment() {
        // Both should agree on what's retryable

        // Pool exhausted should be retryable in both systems
        assert!(DbErrorClass::PoolExhausted.is_retryable());
        assert!(FailureCode::WorkerOverloaded.is_retryable());

        // Query timeout should be retryable
        assert!(DbErrorClass::QueryTimeout.is_retryable());
        assert!(FailureCode::BootDependencyTimeout.is_retryable());

        // Lock contention should be retryable
        assert!(DbErrorClass::LockContention.is_retryable());
        assert!(FailureCode::ThunderingHerdRejected.is_retryable());

        // Authentication failures should NOT be retryable
        assert!(!DbErrorClass::AuthenticationFailed.is_retryable());
        assert!(!FailureCode::TenantAccessDenied.is_retryable());

        // Schema conflicts should NOT be retryable
        assert!(!DbErrorClass::SchemaVersionConflict.is_retryable());
        assert!(!FailureCode::MigrationChecksumMismatch.is_retryable());
    }

    /// Test that error context is preserved through the error chain
    #[test]
    fn test_error_context_preservation() {
        // Create a nested error with context
        let base_err: adapteros_core::Result<()> =
            Err(AosError::Internal("base error".to_string()));

        // Use the ResultExt trait for context
        use adapteros_core::ResultExt;
        let with_context = base_err.context("while processing request");

        match with_context {
            Err(AosError::WithContext { context, source }) => {
                assert_eq!(context, "while processing request");
                assert!(matches!(source.as_ref(), AosError::Internal(_)));
            }
            _ => panic!("Expected WithContext error"),
        }
    }

    /// Test that FailureCode can be embedded in ErrorResponse and extracted
    #[test]
    fn test_failure_code_embedding_and_extraction() {
        // Create response with failure code
        let response = ErrorResponse::new("Cache is stale")
            .with_code("CACHE_STALE")
            .with_failure_code(FailureCode::CacheStale);

        // Serialize and deserialize
        let json = serde_json::to_string(&response).unwrap();
        let parsed: ErrorResponse = serde_json::from_str(&json).unwrap();

        // Verify failure code is preserved
        assert_eq!(parsed.failure_code, Some(FailureCode::CacheStale));

        // Verify retryable status
        assert!(parsed.failure_code.unwrap().is_retryable());
    }
}

// ============================================================================
// Section 6: Error Message Quality
// ============================================================================

mod error_message_quality {
    use super::*;

    /// Test that error messages follow capitalization standards
    #[test]
    fn test_error_message_capitalization() {
        // AosError variants should produce messages starting with capital letters
        let errors = [
            AosError::Database("connection failed".to_string()),
            AosError::Validation("field required".to_string()),
            AosError::Config("invalid value".to_string()),
            AosError::PolicyViolation("egress denied".to_string()),
        ];

        for err in &errors {
            let msg = err.to_string();
            // The error message template (from thiserror) starts with a capital letter
            let first_char = msg.chars().next().unwrap();
            assert!(
                first_char.is_uppercase() || first_char.is_numeric(),
                "Error message should start with capital letter: {}",
                msg
            );
        }
    }

    /// Test that error messages don't end with periods
    #[test]
    fn test_error_messages_no_trailing_period() {
        let errors = [
            AosError::Database("connection failed".to_string()),
            AosError::Validation("field required".to_string()),
            AosError::Config("invalid value".to_string()),
        ];

        for err in &errors {
            let msg = err.to_string();
            assert!(
                !msg.ends_with('.'),
                "Error message should not end with period: {}",
                msg
            );
        }
    }

    /// Test that FailureCode as_str matches expected format
    #[test]
    fn test_failure_code_str_format_quality() {
        // All codes should be non-empty
        for code in failure_code_consistency::ALL_FAILURE_CODES {
            let str_code = code.as_str();
            assert!(!str_code.is_empty(), "Code string should not be empty");

            // Should not start or end with underscore
            assert!(!str_code.starts_with('_'));
            assert!(!str_code.ends_with('_'));

            // Should not have consecutive underscores
            assert!(!str_code.contains("__"));
        }
    }

    /// Test CLI error codes have meaningful fix instructions
    #[test]
    fn test_cli_error_code_fix_quality() {
        let codes = all_error_codes();
        for code in codes {
            // Fix should not be empty
            assert!(
                !code.fix.is_empty(),
                "Fix should not be empty for {}",
                code.code
            );

            // Fix should contain actionable instructions (numbered steps or commands)
            assert!(
                code.fix.contains("1.")
                    || code.fix.contains("aosctl")
                    || code.fix.contains("cargo"),
                "Fix should contain actionable steps for {}: {}",
                code.code,
                code.fix
            );
        }
    }
}
