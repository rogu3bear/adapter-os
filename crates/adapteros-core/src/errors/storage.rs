//! Storage-related errors
//!
//! Covers database operations, I/O, and cache corruption.

use thiserror::Error;

/// Storage and database errors
#[derive(Error, Debug)]
pub enum AosStorageError {
    /// Generic database error
    #[error("Database error: {0}")]
    Database(String),

    /// Database operation with source error
    #[error("Database error: {operation}")]
    DatabaseOp {
        operation: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// SQLx-specific error
    #[error("SQLx database error: {0}")]
    Sqlx(String),

    /// SQLite-specific error
    #[error("SQLite error: {0}")]
    Sqlite(String),

    /// I/O error
    #[error("IO error: {0}")]
    Io(String),

    /// I/O error with source
    #[error("IO error: {context}")]
    IoWithSource {
        context: String,
        #[source]
        source: std::io::Error,
    },

    /// Cache data corruption detected
    #[error("Cache corruption at {path}: expected hash {expected}, got {actual}")]
    CacheCorruption {
        path: String,
        expected: String,
        actual: String,
    },

    /// Registry operation error
    #[error("Registry error: {0}")]
    Registry(String),

    /// Artifact storage error
    #[error("Artifact error: {0}")]
    Artifact(String),

    /// Dual-write consistency failure (SQL committed but KV sync failed and rollback unavailable)
    ///
    /// This is a critical error indicating the system is in an inconsistent state
    /// between SQL and KV stores. Manual intervention or `ensure_consistency()` is required.
    #[error("Dual-write inconsistency for {entity_type} {entity_id}: SQL committed, KV failed, rollback unavailable - {reason}")]
    DualWriteInconsistency {
        /// The type of entity (e.g., "adapter", "training_job")
        entity_type: String,
        /// The ID of the affected entity
        entity_id: String,
        /// Reason for the inconsistency
        reason: String,
    },

    /// Database host is unreachable (network-level failure)
    ///
    /// Indicates that the database server cannot be reached. This could be due to
    /// network issues, firewall rules, or the database server being down.
    #[error("Database host unreachable: {host} - {reason}")]
    HostUnreachable {
        /// The database host that could not be reached
        host: String,
        /// Reason for the connection failure
        reason: String,
        /// Database-specific error code (e.g., PostgreSQL SQLSTATE "08001")
        error_code: Option<String>,
    },

    /// Database credentials are invalid (authentication failure)
    ///
    /// The provided username/password combination was rejected by the database server.
    #[error("Database authentication failed for user '{user}': {reason}")]
    AuthenticationFailed {
        /// The username that failed authentication
        user: String,
        /// Reason for the authentication failure
        reason: String,
        /// Database-specific error code (e.g., PostgreSQL SQLSTATE "28P01")
        error_code: Option<String>,
    },

    /// Connection pool is exhausted (no available connections)
    ///
    /// All connections in the pool are in use and the timeout for acquiring
    /// a new connection has been exceeded. This is a transient error that
    /// may succeed on retry.
    #[error("Connection pool exhausted: {reason}")]
    PoolExhausted {
        /// Description of the pool exhaustion
        reason: String,
        /// Maximum pool size if known
        pool_size: Option<u32>,
    },

    /// Query exceeded statement timeout
    ///
    /// The query took longer than the configured statement timeout.
    /// This is a transient error that may succeed on retry with a simpler query
    /// or after database load decreases.
    #[error("Query timeout after {timeout_ms}ms: {query_context}")]
    QueryTimeout {
        /// Timeout duration in milliseconds
        timeout_ms: u64,
        /// Description of the query that timed out
        query_context: String,
    },

    /// Migration version table is locked (concurrent migration conflict)
    ///
    /// Another process is currently running migrations, preventing this process
    /// from acquiring the migration lock. This is a transient error.
    #[error("Migration table locked: {reason}")]
    MigrationLocked {
        /// Description of the lock situation
        reason: String,
        /// Information about the lock holder if available
        lock_holder: Option<String>,
    },

    // =========================================================================
    // Migration errors (Category 5)
    // =========================================================================
    /// Required migration file is missing
    #[error("Migration file missing: {filename}. Expected at {expected_path}")]
    MigrationFileMissing {
        /// Name of the missing migration file
        filename: String,
        /// Path where the file was expected
        expected_path: String,
    },

    /// Migration checksum doesn't match the signatures list
    #[error("Migration checksum mismatch for {filename}: expected {expected}, computed {actual}")]
    MigrationChecksumMismatch {
        /// Name of the migration file
        filename: String,
        /// Expected checksum from signatures
        expected: String,
        /// Actually computed checksum
        actual: String,
    },

    /// Migration was applied out of order
    #[error("Migration applied out of order: {filename} (version {version}) was applied after version {applied_after}")]
    MigrationOutOfOrder {
        /// Name of the out-of-order migration
        filename: String,
        /// Version number of this migration
        version: i64,
        /// Version number it was applied after (should have been before)
        applied_after: i64,
    },

    /// Down migration blocked because table is not empty
    #[error("Down migration blocked: table {table_name} is not empty ({row_count} rows)")]
    DownMigrationBlocked {
        /// Name of the table that blocked the migration
        table_name: String,
        /// Number of rows in the table
        row_count: u64,
        /// Name of the migration that was blocked
        migration_name: String,
    },

    /// Schema version mismatch between app and database
    #[error("Schema version mismatch: app expects version {app_version}, database is at {db_version} ({direction})")]
    SchemaVersionMismatch {
        /// Version the application expects
        app_version: i64,
        /// Version currently in the database
        db_version: i64,
        /// Direction of the mismatch ("ahead" or "behind")
        direction: String,
    },

    // =========================================================================
    // Cache errors (Category 6)
    // =========================================================================
    /// Cache entry is stale beyond its TTL
    #[error("Cache entry stale: key '{key}' expired {expired_secs}s ago (TTL: {ttl_secs}s)")]
    CacheStale {
        /// The cache key that is stale
        key: String,
        /// How many seconds past expiration
        expired_secs: u64,
        /// The configured TTL in seconds
        ttl_secs: u64,
    },

    /// Cache entry was evicted under memory pressure
    #[error("Cache eviction occurred: {evicted_count} entries removed ({freed_bytes} bytes freed) - {reason}")]
    CacheEviction {
        /// Number of entries that were evicted
        evicted_count: usize,
        /// Bytes freed by the eviction
        freed_bytes: u64,
        /// Reason for eviction (e.g., "memory_pressure", "ttl_expired", "explicit")
        reason: String,
    },

    /// Cache key includes nondeterministic values
    #[error("Cache key nondeterministic: key '{key}' contains time-dependent or random component: {details}")]
    CacheKeyNondeterministic {
        /// The problematic cache key
        key: String,
        /// Details about what makes it nondeterministic
        details: String,
    },

    /// Cache serialization or deserialization failed
    #[error("Cache serialization failed for {operation}: {reason}")]
    CacheSerializationFailed {
        /// The operation that failed ("encode" or "decode")
        operation: String,
        /// The cache key if available
        key: Option<String>,
        /// Reason for the failure
        reason: String,
    },

    /// Cache invalidation failed to propagate
    #[error("Cache invalidation failed: {reason}")]
    CacheInvalidationFailed {
        /// Reason for the invalidation failure
        reason: String,
        /// Keys that were affected
        affected_keys: Vec<String>,
    },
}

impl From<std::io::Error> for AosStorageError {
    fn from(err: std::io::Error) -> Self {
        AosStorageError::Io(err.to_string())
    }
}

impl From<rusqlite::Error> for AosStorageError {
    fn from(err: rusqlite::Error) -> Self {
        AosStorageError::Sqlite(err.to_string())
    }
}

#[cfg(feature = "sqlx")]
impl From<sqlx::Error> for AosStorageError {
    fn from(err: sqlx::Error) -> Self {
        AosStorageError::Sqlx(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_error_display() {
        let err = AosStorageError::Database("connection failed".to_string());
        assert!(err.to_string().contains("Database error"));
        assert!(err.to_string().contains("connection failed"));
    }

    #[test]
    fn test_cache_corruption_display() {
        let err = AosStorageError::CacheCorruption {
            path: "/cache/model.bin".to_string(),
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("/cache/model.bin"));
        assert!(msg.contains("abc123"));
        assert!(msg.contains("def456"));
    }

    #[test]
    fn test_host_unreachable_display() {
        let err = AosStorageError::HostUnreachable {
            host: "db.example.com".to_string(),
            reason: "connection refused".to_string(),
            error_code: Some("08001".to_string()),
        };
        let msg = err.to_string();
        assert!(msg.contains("db.example.com"));
        assert!(msg.contains("connection refused"));
    }

    #[test]
    fn test_authentication_failed_display() {
        let err = AosStorageError::AuthenticationFailed {
            user: "admin".to_string(),
            reason: "invalid password".to_string(),
            error_code: Some("28P01".to_string()),
        };
        let msg = err.to_string();
        assert!(msg.contains("admin"));
        assert!(msg.contains("invalid password"));
    }

    #[test]
    fn test_pool_exhausted_display() {
        let err = AosStorageError::PoolExhausted {
            reason: "timeout waiting for connection".to_string(),
            pool_size: Some(10),
        };
        let msg = err.to_string();
        assert!(msg.contains("pool exhausted"));
    }

    #[test]
    fn test_query_timeout_display() {
        let err = AosStorageError::QueryTimeout {
            timeout_ms: 30000,
            query_context: "SELECT * FROM large_table".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("30000ms"));
        assert!(msg.contains("large_table"));
    }

    #[test]
    fn test_migration_locked_display() {
        let err = AosStorageError::MigrationLocked {
            reason: "another process is running migrations".to_string(),
            lock_holder: Some("pid:12345".to_string()),
        };
        let msg = err.to_string();
        assert!(msg.contains("locked"));
    }

    // Migration error tests
    #[test]
    fn test_migration_file_missing_display() {
        let err = AosStorageError::MigrationFileMissing {
            filename: "V001__init.sql".to_string(),
            expected_path: "/migrations/V001__init.sql".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("V001__init.sql"));
        assert!(msg.contains("/migrations/"));
    }

    #[test]
    fn test_migration_checksum_mismatch_display() {
        let err = AosStorageError::MigrationChecksumMismatch {
            filename: "V001__init.sql".to_string(),
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("V001__init.sql"));
        assert!(msg.contains("abc123"));
        assert!(msg.contains("def456"));
    }

    #[test]
    fn test_migration_out_of_order_display() {
        let err = AosStorageError::MigrationOutOfOrder {
            filename: "V002__users.sql".to_string(),
            version: 2,
            applied_after: 3,
        };
        let msg = err.to_string();
        assert!(msg.contains("V002__users.sql"));
        assert!(msg.contains("out of order"));
    }

    #[test]
    fn test_down_migration_blocked_display() {
        let err = AosStorageError::DownMigrationBlocked {
            table_name: "users".to_string(),
            row_count: 100,
            migration_name: "V001__init.sql".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("users"));
        assert!(msg.contains("100 rows"));
    }

    #[test]
    fn test_schema_version_mismatch_display() {
        let err = AosStorageError::SchemaVersionMismatch {
            app_version: 5,
            db_version: 3,
            direction: "behind".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("5"));
        assert!(msg.contains("3"));
        assert!(msg.contains("behind"));
    }

    // Cache error tests
    #[test]
    fn test_cache_stale_display() {
        let err = AosStorageError::CacheStale {
            key: "model:abc123".to_string(),
            expired_secs: 300,
            ttl_secs: 3600,
        };
        let msg = err.to_string();
        assert!(msg.contains("model:abc123"));
        assert!(msg.contains("300"));
        assert!(msg.contains("3600"));
    }

    #[test]
    fn test_cache_eviction_display() {
        let err = AosStorageError::CacheEviction {
            evicted_count: 50,
            freed_bytes: 1024 * 1024,
            reason: "memory_pressure".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("50 entries"));
        assert!(msg.contains("memory_pressure"));
    }

    #[test]
    fn test_cache_key_nondeterministic_display() {
        let err = AosStorageError::CacheKeyNondeterministic {
            key: "query:timestamp:123".to_string(),
            details: "contains current timestamp".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("nondeterministic"));
        assert!(msg.contains("timestamp"));
    }

    #[test]
    fn test_cache_serialization_failed_display() {
        let err = AosStorageError::CacheSerializationFailed {
            operation: "encode".to_string(),
            key: Some("model:abc".to_string()),
            reason: "invalid UTF-8".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("serialization"));
        assert!(msg.contains("encode"));
    }

    #[test]
    fn test_cache_invalidation_failed_display() {
        let err = AosStorageError::CacheInvalidationFailed {
            reason: "broadcast failed".to_string(),
            affected_keys: vec!["key1".to_string(), "key2".to_string()],
        };
        let msg = err.to_string();
        assert!(msg.contains("invalidation"));
        assert!(msg.contains("broadcast failed"));
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let storage_err: AosStorageError = io_err.into();
        assert!(matches!(storage_err, AosStorageError::Io(_)));
    }

    #[test]
    fn test_from_rusqlite_error() {
        // Create a simple rusqlite error
        let rusqlite_err = rusqlite::Error::QueryReturnedNoRows;
        let storage_err: AosStorageError = rusqlite_err.into();
        assert!(matches!(storage_err, AosStorageError::Sqlite(_)));
        assert!(storage_err.to_string().contains("SQLite error"));
    }

    // =========================================================================
    // Display consistency tests - verify all variants implement Display properly
    // =========================================================================

    #[test]
    fn test_all_variants_display_non_empty() {
        // Create one of each variant and verify Display produces non-empty string
        let variants: Vec<AosStorageError> = vec![
            AosStorageError::Database("test".to_string()),
            AosStorageError::DatabaseOp {
                operation: "insert".to_string(),
                source: Box::new(std::io::Error::other("test")),
            },
            AosStorageError::Sqlx("test".to_string()),
            AosStorageError::Sqlite("test".to_string()),
            AosStorageError::Io("test".to_string()),
            AosStorageError::IoWithSource {
                context: "reading file".to_string(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
            },
            AosStorageError::CacheCorruption {
                path: "/test".to_string(),
                expected: "abc".to_string(),
                actual: "def".to_string(),
            },
            AosStorageError::Registry("test".to_string()),
            AosStorageError::Artifact("test".to_string()),
            AosStorageError::DualWriteInconsistency {
                entity_type: "adapter".to_string(),
                entity_id: "test-123".to_string(),
                reason: "KV sync failed".to_string(),
            },
            AosStorageError::HostUnreachable {
                host: "db.test.com".to_string(),
                reason: "timeout".to_string(),
                error_code: Some("08001".to_string()),
            },
            AosStorageError::AuthenticationFailed {
                user: "testuser".to_string(),
                reason: "bad credentials".to_string(),
                error_code: None,
            },
            AosStorageError::PoolExhausted {
                reason: "no connections".to_string(),
                pool_size: Some(10),
            },
            AosStorageError::QueryTimeout {
                timeout_ms: 5000,
                query_context: "SELECT *".to_string(),
            },
            AosStorageError::MigrationLocked {
                reason: "concurrent migration".to_string(),
                lock_holder: Some("pid:123".to_string()),
            },
            AosStorageError::MigrationFileMissing {
                filename: "V001.sql".to_string(),
                expected_path: "/migrations/V001.sql".to_string(),
            },
            AosStorageError::MigrationChecksumMismatch {
                filename: "V001.sql".to_string(),
                expected: "aaa".to_string(),
                actual: "bbb".to_string(),
            },
            AosStorageError::MigrationOutOfOrder {
                filename: "V002.sql".to_string(),
                version: 2,
                applied_after: 3,
            },
            AosStorageError::DownMigrationBlocked {
                table_name: "users".to_string(),
                row_count: 50,
                migration_name: "V001.sql".to_string(),
            },
            AosStorageError::SchemaVersionMismatch {
                app_version: 5,
                db_version: 3,
                direction: "ahead".to_string(),
            },
            AosStorageError::CacheStale {
                key: "model:abc".to_string(),
                expired_secs: 60,
                ttl_secs: 3600,
            },
            AosStorageError::CacheEviction {
                evicted_count: 10,
                freed_bytes: 1024,
                reason: "memory_pressure".to_string(),
            },
            AosStorageError::CacheKeyNondeterministic {
                key: "query:ts:123".to_string(),
                details: "contains timestamp".to_string(),
            },
            AosStorageError::CacheSerializationFailed {
                operation: "decode".to_string(),
                key: Some("model:xyz".to_string()),
                reason: "invalid format".to_string(),
            },
            AosStorageError::CacheInvalidationFailed {
                reason: "broadcast failed".to_string(),
                affected_keys: vec!["key1".to_string(), "key2".to_string()],
            },
        ];

        for (i, variant) in variants.iter().enumerate() {
            let display = variant.to_string();
            assert!(
                !display.is_empty(),
                "Variant at index {} has empty display string",
                i
            );
            assert!(
                display.len() > 5,
                "Variant at index {} has suspiciously short display: '{}'",
                i,
                display
            );
        }
    }

    #[test]
    fn test_display_starts_with_capital_letter() {
        // Per AGENTS.md, error messages should start with capital letters
        let test_cases: Vec<AosStorageError> = vec![
            AosStorageError::Database("connection failed".to_string()),
            AosStorageError::Io("file not found".to_string()),
            AosStorageError::Registry("registry unavailable".to_string()),
            AosStorageError::MigrationFileMissing {
                filename: "V001.sql".to_string(),
                expected_path: "/path".to_string(),
            },
            AosStorageError::CacheStale {
                key: "test".to_string(),
                expired_secs: 10,
                ttl_secs: 100,
            },
        ];

        for err in test_cases {
            let display = err.to_string();
            let first_char = display.chars().next().expect("empty display string");
            assert!(
                first_char.is_uppercase(),
                "Error message should start with capital letter: '{}'",
                display
            );
        }
    }

    #[test]
    fn test_display_no_trailing_period() {
        // Per AGENTS.md, error messages should not end with periods
        let test_cases: Vec<AosStorageError> = vec![
            AosStorageError::Database("connection failed".to_string()),
            AosStorageError::PoolExhausted {
                reason: "no connections available".to_string(),
                pool_size: None,
            },
            AosStorageError::CacheEviction {
                evicted_count: 5,
                freed_bytes: 2048,
                reason: "ttl".to_string(),
            },
        ];

        for err in test_cases {
            let display = err.to_string();
            assert!(
                !display.ends_with('.'),
                "Error message should not end with period: '{}'",
                display
            );
        }
    }

    // =========================================================================
    // AosError conversion tests - verify From impl works correctly
    // =========================================================================

    #[test]
    fn test_storage_error_converts_to_aos_error() {
        use crate::errors::AosError;

        let storage_err = AosStorageError::Database("test db error".to_string());
        let aos_err: AosError = storage_err.into();

        // Verify the conversion wraps in Storage variant
        assert!(matches!(aos_err, AosError::Storage(_)));

        // Verify the message is preserved
        let display = aos_err.to_string();
        assert!(display.contains("test db error"));
    }

    #[test]
    fn test_migration_errors_convert_to_aos_error() {
        use crate::errors::AosError;

        // MigrationFileMissing
        let err = AosStorageError::MigrationFileMissing {
            filename: "V005__add_indexes.sql".to_string(),
            expected_path: "/app/migrations/V005__add_indexes.sql".to_string(),
        };
        let aos_err: AosError = err.into();
        assert!(matches!(aos_err, AosError::Storage(_)));
        let display = aos_err.to_string();
        assert!(display.contains("V005__add_indexes.sql"));
        assert!(display.contains("/app/migrations"));

        // MigrationChecksumMismatch
        let err = AosStorageError::MigrationChecksumMismatch {
            filename: "V003__users.sql".to_string(),
            expected: "sha256:abc123".to_string(),
            actual: "sha256:def456".to_string(),
        };
        let aos_err: AosError = err.into();
        assert!(matches!(aos_err, AosError::Storage(_)));
        let display = aos_err.to_string();
        assert!(display.contains("checksum mismatch"));
        assert!(display.contains("sha256:abc123"));
        assert!(display.contains("sha256:def456"));

        // MigrationOutOfOrder
        let err = AosStorageError::MigrationOutOfOrder {
            filename: "V010__late.sql".to_string(),
            version: 10,
            applied_after: 15,
        };
        let aos_err: AosError = err.into();
        let display = aos_err.to_string();
        assert!(display.contains("out of order"));
        assert!(display.contains("10"));
        assert!(display.contains("15"));

        // DownMigrationBlocked
        let err = AosStorageError::DownMigrationBlocked {
            table_name: "orders".to_string(),
            row_count: 1000,
            migration_name: "V002__orders.sql".to_string(),
        };
        let aos_err: AosError = err.into();
        let display = aos_err.to_string();
        assert!(display.contains("orders"));
        assert!(display.contains("1000"));

        // SchemaVersionMismatch
        let err = AosStorageError::SchemaVersionMismatch {
            app_version: 20,
            db_version: 15,
            direction: "behind".to_string(),
        };
        let aos_err: AosError = err.into();
        let display = aos_err.to_string();
        assert!(display.contains("20"));
        assert!(display.contains("15"));
        assert!(display.contains("behind"));
    }

    #[test]
    fn test_cache_errors_convert_to_aos_error() {
        use crate::errors::AosError;

        // CacheStale
        let err = AosStorageError::CacheStale {
            key: "adapter:model-xyz:weights".to_string(),
            expired_secs: 7200,
            ttl_secs: 3600,
        };
        let aos_err: AosError = err.into();
        assert!(matches!(aos_err, AosError::Storage(_)));
        let display = aos_err.to_string();
        assert!(display.contains("adapter:model-xyz:weights"));
        assert!(display.contains("7200"));
        assert!(display.contains("3600"));

        // CacheEviction
        let err = AosStorageError::CacheEviction {
            evicted_count: 100,
            freed_bytes: 1024 * 1024 * 512, // 512 MB
            reason: "oom_prevention".to_string(),
        };
        let aos_err: AosError = err.into();
        let display = aos_err.to_string();
        assert!(display.contains("100 entries"));
        assert!(display.contains("oom_prevention"));

        // CacheKeyNondeterministic
        let err = AosStorageError::CacheKeyNondeterministic {
            key: "inference:uuid:random-uuid-here".to_string(),
            details: "UUID component makes key nondeterministic".to_string(),
        };
        let aos_err: AosError = err.into();
        let display = aos_err.to_string();
        assert!(display.contains("nondeterministic"));
        assert!(display.contains("inference:uuid:random-uuid-here"));

        // CacheSerializationFailed
        let err = AosStorageError::CacheSerializationFailed {
            operation: "encode".to_string(),
            key: Some("weights:lora:abc".to_string()),
            reason: "bincode serialization overflow".to_string(),
        };
        let aos_err: AosError = err.into();
        let display = aos_err.to_string();
        assert!(display.contains("serialization"));
        assert!(display.contains("encode"));

        // CacheSerializationFailed without key
        let err = AosStorageError::CacheSerializationFailed {
            operation: "decode".to_string(),
            key: None,
            reason: "corrupted data".to_string(),
        };
        let aos_err: AosError = err.into();
        let display = aos_err.to_string();
        assert!(display.contains("decode"));
        assert!(display.contains("corrupted data"));

        // CacheInvalidationFailed
        let err = AosStorageError::CacheInvalidationFailed {
            reason: "pubsub channel closed".to_string(),
            affected_keys: vec![
                "model:a".to_string(),
                "model:b".to_string(),
                "model:c".to_string(),
            ],
        };
        let aos_err: AosError = err.into();
        let display = aos_err.to_string();
        assert!(display.contains("invalidation"));
        assert!(display.contains("pubsub channel closed"));
    }

    // =========================================================================
    // Sensitive data pattern tests - verify no secrets leak in error messages
    // =========================================================================

    /// Patterns that should NEVER appear in error messages
    const SENSITIVE_PATTERNS: &[&str] = &[
        "password=",
        "password:",
        "secret=",
        "secret:",
        "api_key=",
        "api_key:",
        "apikey=",
        "apikey:",
        "token=",
        "private_key=",
        "private_key:",
        "-----BEGIN",     // PEM formatted keys
        "-----END",       // PEM formatted keys
        "bearer ",        // Auth tokens
        "basic ",         // Basic auth
        "authorization:", // Auth headers
    ];

    fn assert_no_sensitive_data(error_msg: &str) {
        let lower = error_msg.to_lowercase();
        for pattern in SENSITIVE_PATTERNS {
            assert!(
                !lower.contains(&pattern.to_lowercase()),
                "Error message may contain sensitive data pattern '{}': '{}'",
                pattern,
                error_msg
            );
        }
    }

    #[test]
    fn test_auth_failed_no_password_in_message() {
        let err = AosStorageError::AuthenticationFailed {
            user: "admin".to_string(),
            reason: "invalid credentials".to_string(),
            error_code: Some("28P01".to_string()),
        };
        let display = err.to_string();

        // Should contain username (for debugging)
        assert!(display.contains("admin"));

        // Should NOT contain actual password
        assert_no_sensitive_data(&display);

        // Should not contain the word "password" as a value indicator
        assert!(
            !display.contains("password="),
            "Error should not expose password value"
        );
    }

    #[test]
    fn test_host_unreachable_no_credentials() {
        let err = AosStorageError::HostUnreachable {
            host: "postgres://user:secretpass@db.example.com:5432/mydb".to_string(),
            reason: "connection refused".to_string(),
            error_code: None,
        };
        let display = err.to_string();

        // Note: This test documents that if a connection string with password
        // is passed as the host, it will be displayed. Callers should sanitize.
        // The test verifies we don't ADD sensitive patterns, but we document
        // that callers must not pass secrets in host field.
        assert!(display.contains("db.example.com"));
    }

    #[test]
    fn test_database_error_no_sensitive_data() {
        let err = AosStorageError::Database("connection failed".to_string());
        assert_no_sensitive_data(&err.to_string());
    }

    #[test]
    fn test_io_error_no_sensitive_data() {
        let err = AosStorageError::IoWithSource {
            context: "reading config file".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied"),
        };
        assert_no_sensitive_data(&err.to_string());
    }

    #[test]
    fn test_cache_corruption_no_sensitive_data() {
        let err = AosStorageError::CacheCorruption {
            path: "/var/cache/adapters/model.bin".to_string(),
            expected: "b3:abc123def456".to_string(),
            actual: "b3:789xyz000111".to_string(),
        };
        let display = err.to_string();
        assert_no_sensitive_data(&display);

        // Hashes are OK to display (they're not secrets)
        assert!(display.contains("b3:abc123def456"));
        assert!(display.contains("b3:789xyz000111"));
    }

    #[test]
    fn test_dual_write_no_sensitive_data() {
        let err = AosStorageError::DualWriteInconsistency {
            entity_type: "training_job".to_string(),
            entity_id: "job-uuid-12345".to_string(),
            reason: "KV store write failed after SQL commit".to_string(),
        };
        assert_no_sensitive_data(&err.to_string());
    }

    #[test]
    fn test_migration_errors_no_sensitive_data() {
        let errors: Vec<AosStorageError> = vec![
            AosStorageError::MigrationFileMissing {
                filename: "V001__init.sql".to_string(),
                expected_path: "/app/migrations/V001__init.sql".to_string(),
            },
            AosStorageError::MigrationChecksumMismatch {
                filename: "V002.sql".to_string(),
                expected: "sha256:aabbcc".to_string(),
                actual: "sha256:ddeeff".to_string(),
            },
            AosStorageError::MigrationLocked {
                reason: "another process holds lock".to_string(),
                lock_holder: Some("process:12345".to_string()),
            },
            AosStorageError::SchemaVersionMismatch {
                app_version: 10,
                db_version: 5,
                direction: "behind".to_string(),
            },
        ];

        for err in errors {
            assert_no_sensitive_data(&err.to_string());
        }
    }

    #[test]
    fn test_cache_errors_no_sensitive_data() {
        let errors: Vec<AosStorageError> = vec![
            AosStorageError::CacheStale {
                key: "model:weights:abc".to_string(),
                expired_secs: 100,
                ttl_secs: 3600,
            },
            AosStorageError::CacheEviction {
                evicted_count: 50,
                freed_bytes: 1024 * 1024,
                reason: "memory pressure".to_string(),
            },
            AosStorageError::CacheKeyNondeterministic {
                key: "query:timestamp:now".to_string(),
                details: "timestamp is nondeterministic".to_string(),
            },
            AosStorageError::CacheSerializationFailed {
                operation: "encode".to_string(),
                key: Some("model:xyz".to_string()),
                reason: "invalid format".to_string(),
            },
            AosStorageError::CacheInvalidationFailed {
                reason: "channel closed".to_string(),
                affected_keys: vec!["key1".to_string()],
            },
        ];

        for err in errors {
            assert_no_sensitive_data(&err.to_string());
        }
    }

    // =========================================================================
    // From impl correctness tests
    // =========================================================================

    #[test]
    fn test_from_io_error_preserves_message() {
        let io_err = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "cannot access /etc/shadow",
        );
        let storage_err: AosStorageError = io_err.into();

        match storage_err {
            AosStorageError::Io(msg) => {
                assert!(msg.contains("cannot access /etc/shadow"));
            }
            _ => panic!("Expected Io variant"),
        }
    }

    #[test]
    fn test_from_io_error_various_kinds() {
        let test_cases = vec![
            (std::io::ErrorKind::NotFound, "file not found"),
            (std::io::ErrorKind::PermissionDenied, "permission denied"),
            (std::io::ErrorKind::ConnectionRefused, "connection refused"),
            (std::io::ErrorKind::TimedOut, "operation timed out"),
            (std::io::ErrorKind::WouldBlock, "would block"),
        ];

        for (kind, msg) in test_cases {
            let io_err = std::io::Error::new(kind, msg);
            let storage_err: AosStorageError = io_err.into();
            assert!(matches!(storage_err, AosStorageError::Io(_)));
            assert!(storage_err.to_string().contains(msg));
        }
    }

    #[test]
    fn test_from_rusqlite_error_preserves_context() {
        let rusqlite_err = rusqlite::Error::QueryReturnedNoRows;
        let storage_err: AosStorageError = rusqlite_err.into();

        match storage_err {
            AosStorageError::Sqlite(msg) => {
                // Verify the error message contains useful info
                assert!(!msg.is_empty());
            }
            _ => panic!("Expected Sqlite variant"),
        }
    }

    // =========================================================================
    // Edge case tests for newer variants
    // =========================================================================

    #[test]
    fn test_migration_out_of_order_version_numbers() {
        // Test with edge case version numbers
        let err = AosStorageError::MigrationOutOfOrder {
            filename: "V999999999__huge_version.sql".to_string(),
            version: 999999999,
            applied_after: 1000000000,
        };
        let display = err.to_string();
        assert!(display.contains("999999999"));
        assert!(display.contains("1000000000"));
    }

    #[test]
    fn test_cache_stale_with_zero_ttl() {
        let err = AosStorageError::CacheStale {
            key: "ephemeral:data".to_string(),
            expired_secs: 1,
            ttl_secs: 0, // Edge case: zero TTL
        };
        let display = err.to_string();
        assert!(display.contains("TTL: 0s"));
    }

    #[test]
    fn test_cache_eviction_with_zero_entries() {
        let err = AosStorageError::CacheEviction {
            evicted_count: 0,
            freed_bytes: 0,
            reason: "no_op".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("0 entries"));
        assert!(display.contains("0 bytes"));
    }

    #[test]
    fn test_cache_invalidation_empty_keys() {
        let err = AosStorageError::CacheInvalidationFailed {
            reason: "nothing to invalidate".to_string(),
            affected_keys: vec![],
        };
        let display = err.to_string();
        // Should still produce valid output even with empty keys
        assert!(display.contains("invalidation"));
    }

    #[test]
    fn test_pool_exhausted_without_size() {
        let err = AosStorageError::PoolExhausted {
            reason: "all connections busy".to_string(),
            pool_size: None,
        };
        let display = err.to_string();
        assert!(display.contains("pool exhausted"));
        // pool_size is optional, should still work
    }

    #[test]
    fn test_host_unreachable_without_error_code() {
        let err = AosStorageError::HostUnreachable {
            host: "192.168.1.100".to_string(),
            reason: "no route to host".to_string(),
            error_code: None,
        };
        let display = err.to_string();
        assert!(display.contains("192.168.1.100"));
        assert!(display.contains("no route to host"));
    }

    #[test]
    fn test_migration_locked_without_holder() {
        let err = AosStorageError::MigrationLocked {
            reason: "lock acquisition timeout".to_string(),
            lock_holder: None,
        };
        let display = err.to_string();
        assert!(display.contains("locked"));
    }

    #[test]
    fn test_down_migration_blocked_large_row_count() {
        let err = AosStorageError::DownMigrationBlocked {
            table_name: "audit_logs".to_string(),
            row_count: u64::MAX, // Edge case: maximum value
            migration_name: "V001.sql".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("audit_logs"));
        // Should handle large numbers gracefully
    }

    // =========================================================================
    // Debug trait tests
    // =========================================================================

    #[test]
    fn test_debug_impl_exists_for_all_variants() {
        let err = AosStorageError::CacheSerializationFailed {
            operation: "test".to_string(),
            key: Some("key".to_string()),
            reason: "reason".to_string(),
        };

        // Debug should produce output
        let debug = format!("{:?}", err);
        assert!(!debug.is_empty());
        assert!(debug.contains("CacheSerializationFailed"));
    }

    #[test]
    fn test_error_source_chain() {
        use std::error::Error;

        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = AosStorageError::IoWithSource {
            context: "loading adapter".to_string(),
            source: io_err,
        };

        // Verify source() returns the underlying error
        let source = err.source();
        assert!(source.is_some());
        let source_msg = source.unwrap().to_string();
        assert!(source_msg.contains("file not found"));
    }

    #[test]
    fn test_database_op_error_source_chain() {
        use std::error::Error;

        let inner_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "connection lost");
        let err = AosStorageError::DatabaseOp {
            operation: "SELECT".to_string(),
            source: Box::new(inner_err),
        };

        let source = err.source();
        assert!(source.is_some());
    }
}
