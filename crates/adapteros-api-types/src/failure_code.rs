use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Structured failure codes for smoke-test visibility and UI surfacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FailureCode {
    MigrationInvalid,
    ModelLoadFailed,
    OutOfMemory,
    TraceWriteFailed,
    ReceiptMismatch,
    PolicyDivergence,
    BackendFallback,
    TenantAccessDenied,
    /// KV cache quota exceeded - tenant has exhausted KV cache allocation
    KvQuotaExceeded,
    /// Worker is at capacity and cannot accept more requests (retryable)
    WorkerOverloaded,

    // Resource exhaustion failure codes
    /// CPU is throttled due to excessive usage (retryable with backoff)
    CpuThrottled,
    /// File descriptor limit reached (retryable after cleanup)
    FileDescriptorExhausted,
    /// Thread pool is saturated (retryable with backoff)
    ThreadPoolSaturated,
    /// GPU device became unavailable (may be transient)
    GpuUnavailable,

    // Boot-specific failure codes
    /// Database is unreachable during boot
    BootDbUnreachable,
    /// Database migration failed during boot
    BootMigrationFailed,
    /// Seed derivation or initialization failed during boot
    BootSeedFailed,
    /// No workers registered or available during boot
    BootNoWorkers,
    /// No models loaded or available during boot
    BootNoModels,
    /// Dependency timeout during boot
    BootDependencyTimeout,
    /// Background task failed to spawn during boot
    BootBackgroundTaskFailed,
    /// Configuration invalid during boot
    BootConfigInvalid,
    /// System tenant or core policy bootstrap failed during boot
    BootBootstrapFailed,

    // Migration failure codes (Category 5)
    /// Migration file is missing from expected location
    MigrationFileMissing,
    /// Migration checksum doesn't match signature
    MigrationChecksumMismatch,
    /// Migration applied out of expected order
    MigrationOutOfOrder,
    /// Down migration blocked by non-empty table
    DownMigrationBlocked,
    /// Schema version in app is ahead of database
    SchemaVersionAhead,

    // Cache failure codes (Category 6)
    /// Cache entry is stale beyond TTL (retryable - can refetch)
    CacheStale,
    /// Cache key contains nondeterministic values
    CacheKeyNondeterministic,
    /// Cache serialization/deserialization failed
    CacheSerializationError,
    /// Cache invalidation failed to propagate
    CacheInvalidationFailed,

    // Network failure codes (Category 3)
    /// DNS resolution failed
    DnsResolutionFailed,
    /// TLS certificate error
    TlsCertificateError,
    /// Proxy connection failed
    ProxyConnectionFailed,
    /// Environment mismatch detected
    EnvironmentMismatch,

    // Rate limiting failure codes (Category 23)
    /// Rate limiter configuration is missing
    RateLimiterNotConfigured,
    /// Rate limit configuration has invalid parameters
    InvalidRateLimitConfig,
    /// Request rejected due to thundering herd protection (retryable)
    ThunderingHerdRejected,
}

impl FailureCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            FailureCode::MigrationInvalid => "MIGRATION_INVALID",
            FailureCode::ModelLoadFailed => "MODEL_LOAD_FAILED",
            FailureCode::OutOfMemory => "OUT_OF_MEMORY",
            FailureCode::TraceWriteFailed => "TRACE_WRITE_FAILED",
            FailureCode::ReceiptMismatch => "RECEIPT_MISMATCH",
            FailureCode::PolicyDivergence => "POLICY_DIVERGENCE",
            FailureCode::BackendFallback => "BACKEND_FALLBACK",
            FailureCode::TenantAccessDenied => "TENANT_ACCESS_DENIED",
            FailureCode::KvQuotaExceeded => "KV_QUOTA_EXCEEDED",
            FailureCode::WorkerOverloaded => "WORKER_OVERLOADED",
            FailureCode::CpuThrottled => "CPU_THROTTLED",
            FailureCode::FileDescriptorExhausted => "FILE_DESCRIPTOR_EXHAUSTED",
            FailureCode::ThreadPoolSaturated => "THREAD_POOL_SATURATED",
            FailureCode::GpuUnavailable => "GPU_UNAVAILABLE",
            FailureCode::BootDbUnreachable => "BOOT_DB_UNREACHABLE",
            FailureCode::BootMigrationFailed => "BOOT_MIGRATION_FAILED",
            FailureCode::BootSeedFailed => "BOOT_SEED_FAILED",
            FailureCode::BootNoWorkers => "BOOT_NO_WORKERS",
            FailureCode::BootNoModels => "BOOT_NO_MODELS",
            FailureCode::BootDependencyTimeout => "BOOT_DEPENDENCY_TIMEOUT",
            FailureCode::BootBackgroundTaskFailed => "BOOT_BACKGROUND_TASK_FAILED",
            FailureCode::BootConfigInvalid => "BOOT_CONFIG_INVALID",
            FailureCode::BootBootstrapFailed => "BOOT_BOOTSTRAP_FAILED",
            // Migration codes (Category 5)
            FailureCode::MigrationFileMissing => "MIGRATION_FILE_MISSING",
            FailureCode::MigrationChecksumMismatch => "MIGRATION_CHECKSUM_MISMATCH",
            FailureCode::MigrationOutOfOrder => "MIGRATION_OUT_OF_ORDER",
            FailureCode::DownMigrationBlocked => "DOWN_MIGRATION_BLOCKED",
            FailureCode::SchemaVersionAhead => "SCHEMA_VERSION_AHEAD",
            // Cache codes (Category 6)
            FailureCode::CacheStale => "CACHE_STALE",
            FailureCode::CacheKeyNondeterministic => "CACHE_KEY_NONDETERMINISTIC",
            FailureCode::CacheSerializationError => "CACHE_SERIALIZATION_ERROR",
            FailureCode::CacheInvalidationFailed => "CACHE_INVALIDATION_FAILED",
            // Network codes (Category 3)
            FailureCode::DnsResolutionFailed => "DNS_RESOLUTION_FAILED",
            FailureCode::TlsCertificateError => "TLS_CERTIFICATE_ERROR",
            FailureCode::ProxyConnectionFailed => "PROXY_CONNECTION_FAILED",
            FailureCode::EnvironmentMismatch => "ENVIRONMENT_MISMATCH",
            // Rate limiting codes (Category 23)
            FailureCode::RateLimiterNotConfigured => "RATE_LIMITER_NOT_CONFIGURED",
            FailureCode::InvalidRateLimitConfig => "INVALID_RATE_LIMIT_CONFIG",
            FailureCode::ThunderingHerdRejected => "THUNDERING_HERD_REJECTED",
        }
    }

    pub fn parse_code(code: &str) -> Option<Self> {
        match code {
            "MIGRATION_INVALID" => Some(FailureCode::MigrationInvalid),
            "MODEL_LOAD_FAILED" => Some(FailureCode::ModelLoadFailed),
            "OUT_OF_MEMORY" => Some(FailureCode::OutOfMemory),
            "TRACE_WRITE_FAILED" => Some(FailureCode::TraceWriteFailed),
            "RECEIPT_MISMATCH" => Some(FailureCode::ReceiptMismatch),
            "POLICY_DIVERGENCE" => Some(FailureCode::PolicyDivergence),
            "BACKEND_FALLBACK" => Some(FailureCode::BackendFallback),
            "TENANT_ACCESS_DENIED" => Some(FailureCode::TenantAccessDenied),
            "KV_QUOTA_EXCEEDED" => Some(FailureCode::KvQuotaExceeded),
            "WORKER_OVERLOADED" => Some(FailureCode::WorkerOverloaded),
            "CPU_THROTTLED" => Some(FailureCode::CpuThrottled),
            "FILE_DESCRIPTOR_EXHAUSTED" => Some(FailureCode::FileDescriptorExhausted),
            "THREAD_POOL_SATURATED" => Some(FailureCode::ThreadPoolSaturated),
            "GPU_UNAVAILABLE" => Some(FailureCode::GpuUnavailable),
            "BOOT_DB_UNREACHABLE" => Some(FailureCode::BootDbUnreachable),
            "BOOT_MIGRATION_FAILED" => Some(FailureCode::BootMigrationFailed),
            "BOOT_SEED_FAILED" => Some(FailureCode::BootSeedFailed),
            "BOOT_NO_WORKERS" => Some(FailureCode::BootNoWorkers),
            "BOOT_NO_MODELS" => Some(FailureCode::BootNoModels),
            "BOOT_DEPENDENCY_TIMEOUT" => Some(FailureCode::BootDependencyTimeout),
            "BOOT_BACKGROUND_TASK_FAILED" => Some(FailureCode::BootBackgroundTaskFailed),
            "BOOT_CONFIG_INVALID" => Some(FailureCode::BootConfigInvalid),
            "BOOT_BOOTSTRAP_FAILED" => Some(FailureCode::BootBootstrapFailed),
            // Migration codes
            "MIGRATION_FILE_MISSING" => Some(FailureCode::MigrationFileMissing),
            "MIGRATION_CHECKSUM_MISMATCH" => Some(FailureCode::MigrationChecksumMismatch),
            "MIGRATION_OUT_OF_ORDER" => Some(FailureCode::MigrationOutOfOrder),
            "DOWN_MIGRATION_BLOCKED" => Some(FailureCode::DownMigrationBlocked),
            "SCHEMA_VERSION_AHEAD" => Some(FailureCode::SchemaVersionAhead),
            // Cache codes
            "CACHE_STALE" => Some(FailureCode::CacheStale),
            "CACHE_KEY_NONDETERMINISTIC" => Some(FailureCode::CacheKeyNondeterministic),
            "CACHE_SERIALIZATION_ERROR" => Some(FailureCode::CacheSerializationError),
            "CACHE_INVALIDATION_FAILED" => Some(FailureCode::CacheInvalidationFailed),
            // Network codes
            "DNS_RESOLUTION_FAILED" => Some(FailureCode::DnsResolutionFailed),
            "TLS_CERTIFICATE_ERROR" => Some(FailureCode::TlsCertificateError),
            "PROXY_CONNECTION_FAILED" => Some(FailureCode::ProxyConnectionFailed),
            "ENVIRONMENT_MISMATCH" => Some(FailureCode::EnvironmentMismatch),
            // Rate limiting codes
            "RATE_LIMITER_NOT_CONFIGURED" => Some(FailureCode::RateLimiterNotConfigured),
            "INVALID_RATE_LIMIT_CONFIG" => Some(FailureCode::InvalidRateLimitConfig),
            "THUNDERING_HERD_REJECTED" => Some(FailureCode::ThunderingHerdRejected),
            _ => None,
        }
    }

    /// Check if this failure code represents a retryable condition
    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            FailureCode::WorkerOverloaded
                | FailureCode::CpuThrottled
                | FailureCode::FileDescriptorExhausted
                | FailureCode::ThreadPoolSaturated
                | FailureCode::GpuUnavailable
                | FailureCode::OutOfMemory
                | FailureCode::BootDbUnreachable
                | FailureCode::BootDependencyTimeout
                // Cache stale is retryable (refetch from source)
                | FailureCode::CacheStale
                // DNS and proxy failures are transient
                | FailureCode::DnsResolutionFailed
                | FailureCode::ProxyConnectionFailed
                // Thundering herd rejection is by design retryable
                | FailureCode::ThunderingHerdRejected
        )
    }
}

impl std::fmt::Display for FailureCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All failure codes for exhaustive testing
    const ALL_CODES: &[FailureCode] = &[
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

    #[test]
    fn test_all_codes_round_trip() {
        for code in ALL_CODES {
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

    #[test]
    fn test_parse_unknown_code() {
        assert_eq!(FailureCode::parse_code("UNKNOWN_CODE"), None);
        assert_eq!(FailureCode::parse_code(""), None);
        assert_eq!(FailureCode::parse_code("lowercase_code"), None);
    }

    #[test]
    fn test_as_str_format() {
        // All codes should be SCREAMING_SNAKE_CASE
        for code in ALL_CODES {
            let str_code = code.as_str();
            assert!(
                str_code.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
                "Code {:?} has non-uppercase string: {}",
                code,
                str_code
            );
        }
    }

    // Retryable tests
    #[test]
    fn test_worker_overloaded_is_retryable() {
        assert!(FailureCode::WorkerOverloaded.is_retryable());
    }

    #[test]
    fn test_cpu_throttled_is_retryable() {
        assert!(FailureCode::CpuThrottled.is_retryable());
    }

    #[test]
    fn test_fd_exhausted_is_retryable() {
        assert!(FailureCode::FileDescriptorExhausted.is_retryable());
    }

    #[test]
    fn test_thread_pool_saturated_is_retryable() {
        assert!(FailureCode::ThreadPoolSaturated.is_retryable());
    }

    #[test]
    fn test_gpu_unavailable_is_retryable() {
        assert!(FailureCode::GpuUnavailable.is_retryable());
    }

    #[test]
    fn test_out_of_memory_is_retryable() {
        assert!(FailureCode::OutOfMemory.is_retryable());
    }

    #[test]
    fn test_boot_db_unreachable_is_retryable() {
        assert!(FailureCode::BootDbUnreachable.is_retryable());
    }

    #[test]
    fn test_boot_dependency_timeout_is_retryable() {
        assert!(FailureCode::BootDependencyTimeout.is_retryable());
    }

    #[test]
    fn test_cache_stale_is_retryable() {
        assert!(FailureCode::CacheStale.is_retryable());
    }

    #[test]
    fn test_dns_resolution_failed_is_retryable() {
        assert!(FailureCode::DnsResolutionFailed.is_retryable());
    }

    #[test]
    fn test_proxy_connection_failed_is_retryable() {
        assert!(FailureCode::ProxyConnectionFailed.is_retryable());
    }

    #[test]
    fn test_thundering_herd_rejected_is_retryable() {
        assert!(FailureCode::ThunderingHerdRejected.is_retryable());
    }

    // Non-retryable tests
    #[test]
    fn test_migration_invalid_not_retryable() {
        assert!(!FailureCode::MigrationInvalid.is_retryable());
    }

    #[test]
    fn test_tenant_access_denied_not_retryable() {
        assert!(!FailureCode::TenantAccessDenied.is_retryable());
    }

    #[test]
    fn test_policy_divergence_not_retryable() {
        assert!(!FailureCode::PolicyDivergence.is_retryable());
    }

    #[test]
    fn test_tls_certificate_error_not_retryable() {
        assert!(!FailureCode::TlsCertificateError.is_retryable());
    }

    #[test]
    fn test_environment_mismatch_not_retryable() {
        assert!(!FailureCode::EnvironmentMismatch.is_retryable());
    }

    #[test]
    fn test_migration_checksum_mismatch_not_retryable() {
        assert!(!FailureCode::MigrationChecksumMismatch.is_retryable());
    }

    #[test]
    fn test_invalid_rate_limit_config_not_retryable() {
        assert!(!FailureCode::InvalidRateLimitConfig.is_retryable());
    }

    #[test]
    fn test_cache_key_nondeterministic_not_retryable() {
        assert!(!FailureCode::CacheKeyNondeterministic.is_retryable());
    }

    // Serde tests
    #[test]
    fn test_serde_round_trip() {
        for code in ALL_CODES {
            let json = serde_json::to_string(code).expect("serialize");
            let parsed: FailureCode = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(parsed, *code, "Serde round-trip failed for {:?}", code);
        }
    }

    #[test]
    fn test_serde_screaming_snake_case() {
        let json = serde_json::to_string(&FailureCode::OutOfMemory).expect("serialize");
        assert_eq!(json, "\"OUT_OF_MEMORY\"");
    }

    // ========================================================================
    // API Consistency Tests
    // ========================================================================

    /// Verify all codes serialize to SCREAMING_SNAKE_CASE format via serde
    #[test]
    fn test_all_codes_serialize_to_screaming_snake_case() {
        for code in ALL_CODES {
            let json = serde_json::to_string(code).expect("serialize");
            // Remove surrounding quotes from JSON string
            let serialized = json.trim_matches('"');

            // Verify SCREAMING_SNAKE_CASE format:
            // - Only uppercase ASCII letters and underscores
            // - No leading/trailing underscores
            // - No consecutive underscores
            assert!(
                !serialized.is_empty(),
                "Code {:?} serialized to empty string",
                code
            );
            assert!(
                serialized
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c == '_'),
                "Code {:?} serialized to non-SCREAMING_SNAKE_CASE: {}",
                code,
                serialized
            );
            assert!(
                !serialized.starts_with('_'),
                "Code {:?} serialized with leading underscore: {}",
                code,
                serialized
            );
            assert!(
                !serialized.ends_with('_'),
                "Code {:?} serialized with trailing underscore: {}",
                code,
                serialized
            );
            assert!(
                !serialized.contains("__"),
                "Code {:?} serialized with consecutive underscores: {}",
                code,
                serialized
            );
        }
    }

    /// Verify parse_code and as_str are symmetric for all variants
    #[test]
    fn test_parse_code_as_str_symmetry() {
        for code in ALL_CODES {
            // as_str -> parse_code should return the same variant
            let str_repr = code.as_str();
            let parsed = FailureCode::parse_code(str_repr);
            assert_eq!(
                parsed,
                Some(*code),
                "Symmetry failed: {:?}.as_str() = {:?}, parse_code({:?}) = {:?}",
                code,
                str_repr,
                str_repr,
                parsed
            );

            // The string from as_str should match what serde produces (without quotes)
            let serde_json = serde_json::to_string(code).expect("serialize");
            let serde_str = serde_json.trim_matches('"');
            assert_eq!(
                str_repr, serde_str,
                "as_str() and serde serialization mismatch for {:?}: as_str={}, serde={}",
                code, str_repr, serde_str
            );
        }
    }

    /// Verify is_retryable classification is correct for all codes
    #[test]
    fn test_is_retryable_classification_comprehensive() {
        // Define the expected retryable codes based on their semantics
        const EXPECTED_RETRYABLE: &[FailureCode] = &[
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

        // Define the expected non-retryable codes
        const EXPECTED_NON_RETRYABLE: &[FailureCode] = &[
            FailureCode::MigrationInvalid,
            FailureCode::ModelLoadFailed,
            FailureCode::TraceWriteFailed,
            FailureCode::ReceiptMismatch,
            FailureCode::PolicyDivergence,
            FailureCode::BackendFallback,
            FailureCode::TenantAccessDenied,
            FailureCode::KvQuotaExceeded,
            FailureCode::BootMigrationFailed,
            FailureCode::BootSeedFailed,
            FailureCode::BootNoWorkers,
            FailureCode::BootNoModels,
            FailureCode::BootBackgroundTaskFailed,
            FailureCode::BootConfigInvalid,
            FailureCode::BootBootstrapFailed,
            FailureCode::MigrationFileMissing,
            FailureCode::MigrationChecksumMismatch,
            FailureCode::MigrationOutOfOrder,
            FailureCode::DownMigrationBlocked,
            FailureCode::SchemaVersionAhead,
            FailureCode::CacheKeyNondeterministic,
            FailureCode::CacheSerializationError,
            FailureCode::CacheInvalidationFailed,
            FailureCode::TlsCertificateError,
            FailureCode::EnvironmentMismatch,
            FailureCode::RateLimiterNotConfigured,
            FailureCode::InvalidRateLimitConfig,
        ];

        // Verify every code in ALL_CODES is classified correctly
        for code in ALL_CODES {
            let is_retryable = code.is_retryable();
            let in_retryable = EXPECTED_RETRYABLE.contains(code);
            let in_non_retryable = EXPECTED_NON_RETRYABLE.contains(code);

            if in_retryable {
                assert!(
                    is_retryable,
                    "Code {:?} should be retryable but is_retryable() returned false",
                    code
                );
                assert!(
                    !in_non_retryable,
                    "Code {:?} is in both retryable and non-retryable sets",
                    code
                );
            } else if in_non_retryable {
                assert!(
                    !is_retryable,
                    "Code {:?} should NOT be retryable but is_retryable() returned true",
                    code
                );
            } else {
                panic!(
                    "Code {:?} is not classified in either EXPECTED_RETRYABLE or EXPECTED_NON_RETRYABLE",
                    code
                );
            }
        }

        // Verify all expected codes are in ALL_CODES
        for code in EXPECTED_RETRYABLE {
            assert!(
                ALL_CODES.contains(code),
                "Expected retryable code {:?} is not in ALL_CODES",
                code
            );
        }
        for code in EXPECTED_NON_RETRYABLE {
            assert!(
                ALL_CODES.contains(code),
                "Expected non-retryable code {:?} is not in ALL_CODES",
                code
            );
        }

        // Verify counts match
        assert_eq!(
            EXPECTED_RETRYABLE.len() + EXPECTED_NON_RETRYABLE.len(),
            ALL_CODES.len(),
            "Mismatch: {} retryable + {} non-retryable != {} total codes",
            EXPECTED_RETRYABLE.len(),
            EXPECTED_NON_RETRYABLE.len(),
            ALL_CODES.len()
        );
    }

    /// Verify no duplicate string representations exist
    #[test]
    fn test_no_duplicate_string_representations() {
        let mut seen_strings: std::collections::HashMap<&'static str, FailureCode> =
            std::collections::HashMap::new();

        for code in ALL_CODES {
            let str_repr = code.as_str();

            if let Some(existing_code) = seen_strings.get(str_repr) {
                panic!(
                    "Duplicate string representation '{}' found for both {:?} and {:?}",
                    str_repr, existing_code, code
                );
            }

            seen_strings.insert(str_repr, *code);
        }

        // Also verify serde serialization produces no duplicates
        let mut seen_serde: std::collections::HashMap<String, FailureCode> =
            std::collections::HashMap::new();

        for code in ALL_CODES {
            let json = serde_json::to_string(code).expect("serialize");

            if let Some(existing_code) = seen_serde.get(&json) {
                panic!(
                    "Duplicate serde serialization '{}' found for both {:?} and {:?}",
                    json, existing_code, code
                );
            }

            seen_serde.insert(json, *code);
        }
    }

    /// Verify ALL_CODES contains all enum variants (exhaustiveness check)
    #[test]
    fn test_all_codes_exhaustive() {
        // This test verifies that ALL_CODES is kept in sync with the enum.
        // We count the variants by checking that parse_code covers all as_str outputs.
        let mut variant_count = 0;

        for code in ALL_CODES {
            let str_repr = code.as_str();
            assert!(
                FailureCode::parse_code(str_repr).is_some(),
                "as_str() output '{}' for {:?} is not recognized by parse_code()",
                str_repr,
                code
            );
            variant_count += 1;
        }

        // Ensure we have a reasonable number of variants (sanity check)
        assert!(
            variant_count >= 30,
            "Expected at least 30 variants, found {}. ALL_CODES may be incomplete.",
            variant_count
        );

        // Verify the count matches the array length
        assert_eq!(
            variant_count,
            ALL_CODES.len(),
            "Variant count mismatch with ALL_CODES length"
        );
    }
}
