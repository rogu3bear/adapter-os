//! Database error classification for retry and error handling
//!
//! Provides unified error detection for both SQLite and PostgreSQL backends,
//! classifying errors into specific categories for appropriate handling.

use adapteros_core::errors::storage::AosStorageError;

/// Trait for errors that can indicate whether they are retriable
///
/// Implement this trait for error types that may be used with the `with_retry`
/// function to enable intelligent retry behavior based on error classification.
///
/// # Examples
///
/// ```rust
/// use adapteros_db::error_classification::Retriable;
///
/// impl Retriable for MyError {
///     fn is_retriable(&self) -> bool {
///         matches!(self, MyError::Timeout | MyError::TemporaryFailure)
///     }
/// }
/// ```
pub trait Retriable {
    /// Returns true if this error is transient and may succeed on retry.
    ///
    /// Non-retriable errors include:
    /// - Authentication failures (wrong credentials won't fix themselves)
    /// - Permission denied (won't change without external action)
    /// - Invalid input (same input will always fail)
    /// - Resource not found (unless it might appear later)
    fn is_retriable(&self) -> bool;
}

/// Database backend type for error classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseBackend {
    /// SQLite embedded database
    Sqlite,
    /// PostgreSQL client-server database
    Postgres,
}

/// Classification of database errors for handling decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbErrorClass {
    /// Host unreachable (network failure, connection refused)
    HostUnreachable,
    /// Invalid credentials (authentication failed)
    AuthenticationFailed,
    /// Connection pool exhausted (no available connections)
    PoolExhausted,
    /// Query timeout (statement or connection timeout)
    QueryTimeout,
    /// Migration table locked (concurrent migration)
    MigrationLocked,
    /// Transient lock contention (SQLITE_BUSY, deadlock)
    LockContention,
    /// Schema version conflict (app/db version mismatch)
    SchemaVersionConflict,
    /// Migration integrity error (checksum mismatch, out of order)
    MigrationIntegrity,
    /// Other database error (not specifically classified)
    Other,
}

impl DbErrorClass {
    /// Check if this error class is retryable
    ///
    /// Retryable errors are transient conditions that may succeed on retry.
    /// Non-retryable errors require human intervention or code changes.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            DbErrorClass::PoolExhausted
                | DbErrorClass::QueryTimeout
                | DbErrorClass::MigrationLocked
                | DbErrorClass::LockContention
        )
        // Note: SchemaVersionConflict and MigrationIntegrity are NOT retryable
        // as they require human intervention to fix
    }

    /// Check if this error requires operator attention
    ///
    /// These errors indicate issues that require manual intervention,
    /// configuration changes, or deployment fixes.
    pub fn requires_operator_attention(&self) -> bool {
        matches!(
            self,
            DbErrorClass::AuthenticationFailed
                | DbErrorClass::SchemaVersionConflict
                | DbErrorClass::MigrationIntegrity
        )
    }

    /// Get the recommended initial retry delay for this error class
    pub fn recommended_delay_ms(&self) -> u64 {
        match self {
            DbErrorClass::PoolExhausted => 100,
            DbErrorClass::QueryTimeout => 500,
            DbErrorClass::MigrationLocked => 1000,
            DbErrorClass::LockContention => 50,
            // Non-retryable errors - delay doesn't matter
            DbErrorClass::HostUnreachable => 1000,
            DbErrorClass::AuthenticationFailed => 0,
            DbErrorClass::SchemaVersionConflict => 0,
            DbErrorClass::MigrationIntegrity => 0,
            DbErrorClass::Other => 100,
        }
    }

    /// Get a human-readable description of the error class
    pub fn description(&self) -> &'static str {
        match self {
            DbErrorClass::HostUnreachable => "Database host is unreachable",
            DbErrorClass::AuthenticationFailed => "Database authentication failed",
            DbErrorClass::PoolExhausted => "Connection pool exhausted",
            DbErrorClass::QueryTimeout => "Query timed out",
            DbErrorClass::MigrationLocked => "Migration table is locked",
            DbErrorClass::LockContention => "Database lock contention",
            DbErrorClass::SchemaVersionConflict => {
                "Schema version mismatch between app and database"
            }
            DbErrorClass::MigrationIntegrity => "Migration integrity check failed",
            DbErrorClass::Other => "Unknown database error",
        }
    }
}

/// Classify a sqlx error into a specific error class
///
/// # Arguments
/// * `err` - The sqlx error to classify
/// * `backend` - The database backend type for backend-specific detection
///
/// # Returns
/// The classified error type
pub fn classify_sqlx_error(err: &sqlx::Error, backend: DatabaseBackend) -> DbErrorClass {
    match err {
        // Pool-level errors
        sqlx::Error::PoolTimedOut => DbErrorClass::PoolExhausted,
        sqlx::Error::PoolClosed => DbErrorClass::Other,

        // Database-specific errors
        sqlx::Error::Database(db_err) => classify_database_error(db_err.as_ref(), backend),

        // IO/connection errors
        sqlx::Error::Io(io_err) => {
            if is_connection_error(io_err) {
                DbErrorClass::HostUnreachable
            } else {
                DbErrorClass::Other
            }
        }

        // TLS errors indicate connection issues
        sqlx::Error::Tls(_) => DbErrorClass::HostUnreachable,

        // Protocol errors might indicate timeout or connection issues
        sqlx::Error::Protocol(msg) => {
            let msg_lower = msg.to_lowercase();
            if msg_lower.contains("timeout") || msg_lower.contains("timed out") {
                DbErrorClass::QueryTimeout
            } else if msg_lower.contains("connection") {
                DbErrorClass::HostUnreachable
            } else {
                DbErrorClass::Other
            }
        }

        _ => DbErrorClass::Other,
    }
}

/// Classify a database-specific error
fn classify_database_error(
    db_err: &dyn sqlx::error::DatabaseError,
    backend: DatabaseBackend,
) -> DbErrorClass {
    let code = db_err.code().map(|c| c.to_string());
    let message = db_err.message().to_lowercase();

    match backend {
        DatabaseBackend::Postgres => classify_postgres_error(code.as_deref(), &message),
        DatabaseBackend::Sqlite => classify_sqlite_error(code.as_deref(), &message),
    }
}

/// Classify PostgreSQL errors using SQLSTATE codes
///
/// Reference: https://www.postgresql.org/docs/current/errcodes-appendix.html
fn classify_postgres_error(code: Option<&str>, message: &str) -> DbErrorClass {
    match code {
        // Class 08 - Connection Exception
        Some(c) if c.starts_with("08") => DbErrorClass::HostUnreachable,

        // Class 28 - Invalid Authorization Specification
        Some("28000") | Some("28P01") => DbErrorClass::AuthenticationFailed,

        // Class 53 - Insufficient Resources
        Some(c) if c.starts_with("53") => DbErrorClass::PoolExhausted,

        // 57014 - query_canceled (statement timeout)
        Some("57014") => DbErrorClass::QueryTimeout,

        // Lock-related errors
        Some("55P03") => DbErrorClass::MigrationLocked, // lock_not_available
        Some("40P01") => DbErrorClass::LockContention,  // deadlock_detected
        Some("40001") => DbErrorClass::LockContention,  // serialization_failure

        // 55006 - object_in_use (could be migration table)
        Some("55006") => DbErrorClass::MigrationLocked,

        _ => {
            // Fallback to message-based detection
            if message.contains("connection refused") || message.contains("could not connect") {
                DbErrorClass::HostUnreachable
            } else if message.contains("password authentication failed")
                || message.contains("authentication failed")
            {
                DbErrorClass::AuthenticationFailed
            } else if message.contains("timeout") || message.contains("timed out") {
                DbErrorClass::QueryTimeout
            } else if message.contains("locked") || message.contains("deadlock") {
                DbErrorClass::LockContention
            } else {
                DbErrorClass::Other
            }
        }
    }
}

/// Classify SQLite errors using error codes
fn classify_sqlite_error(code: Option<&str>, message: &str) -> DbErrorClass {
    match code {
        // SQLITE_BUSY (5)
        Some("5") | Some("SQLITE_BUSY") => DbErrorClass::LockContention,

        // SQLITE_LOCKED (6)
        Some("6") | Some("SQLITE_LOCKED") => DbErrorClass::LockContention,

        // SQLITE_IOERR (10) - might indicate connection/file issues
        Some("10") | Some("SQLITE_IOERR") => {
            if message.contains("timeout") {
                DbErrorClass::QueryTimeout
            } else {
                DbErrorClass::HostUnreachable
            }
        }

        // SQLITE_CANTOPEN (14)
        Some("14") | Some("SQLITE_CANTOPEN") => DbErrorClass::HostUnreachable,

        // SQLITE_AUTH (23)
        Some("23") | Some("SQLITE_AUTH") => DbErrorClass::AuthenticationFailed,

        _ => {
            // Fallback to message-based detection
            if message.contains("database is locked") || message.contains("busy") {
                DbErrorClass::LockContention
            } else if message.contains("timeout") {
                DbErrorClass::QueryTimeout
            } else if message.contains("unable to open") || message.contains("cannot open") {
                DbErrorClass::HostUnreachable
            } else {
                DbErrorClass::Other
            }
        }
    }
}

/// Check if an IO error indicates a connection failure
fn is_connection_error(io_err: &std::io::Error) -> bool {
    matches!(
        io_err.kind(),
        std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::AddrNotAvailable
            | std::io::ErrorKind::TimedOut
    )
}

/// Convert a sqlx error to an AosStorageError with proper classification
///
/// # Arguments
/// * `err` - The sqlx error to convert
/// * `backend` - The database backend type
/// * `context` - Context string describing the operation that failed
///
/// # Returns
/// A classified AosStorageError
pub fn sqlx_to_storage_error(
    err: sqlx::Error,
    backend: DatabaseBackend,
    context: &str,
) -> AosStorageError {
    let class = classify_sqlx_error(&err, backend);

    match class {
        DbErrorClass::HostUnreachable => AosStorageError::HostUnreachable {
            host: extract_host_from_error(&err).unwrap_or_else(|| "unknown".to_string()),
            reason: err.to_string(),
            error_code: extract_error_code(&err),
        },
        DbErrorClass::AuthenticationFailed => AosStorageError::AuthenticationFailed {
            user: extract_user_from_error(&err).unwrap_or_else(|| "unknown".to_string()),
            reason: err.to_string(),
            error_code: extract_error_code(&err),
        },
        DbErrorClass::PoolExhausted => AosStorageError::PoolExhausted {
            reason: format!("{}: {}", context, err),
            pool_size: None,
        },
        DbErrorClass::QueryTimeout => AosStorageError::QueryTimeout {
            timeout_ms: 0, // Actual timeout not available from error
            query_context: context.to_string(),
        },
        DbErrorClass::MigrationLocked => AosStorageError::MigrationLocked {
            reason: err.to_string(),
            lock_holder: None,
        },
        DbErrorClass::SchemaVersionConflict => AosStorageError::SchemaVersionMismatch {
            app_version: 0, // Not available from raw sqlx error
            db_version: 0,
            direction: "unknown".to_string(),
        },
        DbErrorClass::MigrationIntegrity => {
            // Map to generic database error as specific migration info not available
            AosStorageError::Database(format!("Migration integrity error: {}: {}", context, err))
        }
        DbErrorClass::LockContention | DbErrorClass::Other => {
            AosStorageError::Sqlx(format!("{}: {}", context, err))
        }
    }
}

/// Extract error code from sqlx error if available
fn extract_error_code(err: &sqlx::Error) -> Option<String> {
    match err {
        sqlx::Error::Database(db_err) => db_err.code().map(|c| c.to_string()),
        _ => None,
    }
}

/// Extract host from error message (best effort)
fn extract_host_from_error(err: &sqlx::Error) -> Option<String> {
    let msg = err.to_string();
    // Try to extract host from common error message patterns
    if let Some(start) = msg.find("host \"") {
        if let Some(end) = msg[start + 6..].find('"') {
            return Some(msg[start + 6..start + 6 + end].to_string());
        }
    }
    if let Some(start) = msg.find("connect to ") {
        let rest = &msg[start + 11..];
        if let Some(end) = rest.find(|c: char| c.is_whitespace() || c == ':') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// Extract username from error message (best effort)
fn extract_user_from_error(err: &sqlx::Error) -> Option<String> {
    let msg = err.to_string();
    // Try to extract user from common error message patterns
    if let Some(start) = msg.find("user \"") {
        if let Some(end) = msg[start + 6..].find('"') {
            return Some(msg[start + 6..start + 6 + end].to_string());
        }
    }
    if let Some(start) = msg.find("for user ") {
        let rest = &msg[start + 9..];
        if let Some(end) = rest.find(|c: char| c.is_whitespace()) {
            return Some(rest[..end].trim_matches('"').to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // DbErrorClass Variant Classification Tests
    // ==========================================================================

    mod db_error_class_variants {
        use super::*;

        #[test]
        fn test_all_variants_have_unique_discrimination() {
            // Ensure all variants are distinct
            let variants = [
                DbErrorClass::HostUnreachable,
                DbErrorClass::AuthenticationFailed,
                DbErrorClass::PoolExhausted,
                DbErrorClass::QueryTimeout,
                DbErrorClass::MigrationLocked,
                DbErrorClass::LockContention,
                DbErrorClass::SchemaVersionConflict,
                DbErrorClass::MigrationIntegrity,
                DbErrorClass::Other,
            ];

            // Check each variant is not equal to any other
            for (i, v1) in variants.iter().enumerate() {
                for (j, v2) in variants.iter().enumerate() {
                    if i == j {
                        assert_eq!(v1, v2);
                    } else {
                        assert_ne!(v1, v2, "Variants at {} and {} should differ", i, j);
                    }
                }
            }
        }

        #[test]
        fn test_error_class_clone_and_copy() {
            let class = DbErrorClass::LockContention;
            let cloned = class.clone();
            let copied = class;

            assert_eq!(class, cloned);
            assert_eq!(class, copied);
        }

        #[test]
        fn test_error_class_debug_impl() {
            let class = DbErrorClass::HostUnreachable;
            let debug_str = format!("{:?}", class);
            assert!(debug_str.contains("HostUnreachable"));
        }

        #[test]
        fn test_database_backend_variants() {
            assert_ne!(DatabaseBackend::Sqlite, DatabaseBackend::Postgres);

            let sqlite = DatabaseBackend::Sqlite;
            let postgres = DatabaseBackend::Postgres;

            assert_eq!(sqlite, DatabaseBackend::Sqlite);
            assert_eq!(postgres, DatabaseBackend::Postgres);
        }

        #[test]
        fn test_database_backend_clone_copy() {
            let backend = DatabaseBackend::Sqlite;
            let cloned = backend.clone();
            let copied = backend;

            assert_eq!(backend, cloned);
            assert_eq!(backend, copied);
        }
    }

    // ==========================================================================
    // Retryable Error Detection Tests
    // ==========================================================================

    mod retryable_detection {
        use super::*;

        #[test]
        fn test_error_class_is_retryable() {
            // Retryable errors - transient conditions
            assert!(DbErrorClass::PoolExhausted.is_retryable());
            assert!(DbErrorClass::QueryTimeout.is_retryable());
            assert!(DbErrorClass::MigrationLocked.is_retryable());
            assert!(DbErrorClass::LockContention.is_retryable());

            // Non-retryable errors - require intervention
            assert!(!DbErrorClass::HostUnreachable.is_retryable());
            assert!(!DbErrorClass::AuthenticationFailed.is_retryable());
            assert!(!DbErrorClass::SchemaVersionConflict.is_retryable());
            assert!(!DbErrorClass::MigrationIntegrity.is_retryable());
            assert!(!DbErrorClass::Other.is_retryable());
        }

        #[test]
        fn test_retryable_count() {
            // Verify exactly 4 retryable error classes
            let all_classes = [
                DbErrorClass::HostUnreachable,
                DbErrorClass::AuthenticationFailed,
                DbErrorClass::PoolExhausted,
                DbErrorClass::QueryTimeout,
                DbErrorClass::MigrationLocked,
                DbErrorClass::LockContention,
                DbErrorClass::SchemaVersionConflict,
                DbErrorClass::MigrationIntegrity,
                DbErrorClass::Other,
            ];

            let retryable_count = all_classes.iter().filter(|c| c.is_retryable()).count();
            assert_eq!(
                retryable_count, 4,
                "Expected exactly 4 retryable error classes"
            );
        }

        #[test]
        fn test_requires_operator_attention() {
            // Errors requiring operator attention
            assert!(DbErrorClass::AuthenticationFailed.requires_operator_attention());
            assert!(DbErrorClass::SchemaVersionConflict.requires_operator_attention());
            assert!(DbErrorClass::MigrationIntegrity.requires_operator_attention());

            // Errors that may auto-resolve
            assert!(!DbErrorClass::PoolExhausted.requires_operator_attention());
            assert!(!DbErrorClass::QueryTimeout.requires_operator_attention());
            assert!(!DbErrorClass::LockContention.requires_operator_attention());
            assert!(!DbErrorClass::HostUnreachable.requires_operator_attention());
            assert!(!DbErrorClass::MigrationLocked.requires_operator_attention());
            assert!(!DbErrorClass::Other.requires_operator_attention());
        }

        #[test]
        fn test_retryable_vs_operator_attention_exclusive() {
            // No error should be both retryable AND require operator attention
            let all_classes = [
                DbErrorClass::HostUnreachable,
                DbErrorClass::AuthenticationFailed,
                DbErrorClass::PoolExhausted,
                DbErrorClass::QueryTimeout,
                DbErrorClass::MigrationLocked,
                DbErrorClass::LockContention,
                DbErrorClass::SchemaVersionConflict,
                DbErrorClass::MigrationIntegrity,
                DbErrorClass::Other,
            ];

            for class in all_classes.iter() {
                if class.is_retryable() && class.requires_operator_attention() {
                    panic!(
                        "Error class {:?} should not be both retryable and require operator attention",
                        class
                    );
                }
            }
        }

        #[test]
        fn test_recommended_delay() {
            // Fastest retry for lock contention (brief locks)
            assert_eq!(DbErrorClass::LockContention.recommended_delay_ms(), 50);

            // Medium delay for pool exhaustion
            assert_eq!(DbErrorClass::PoolExhausted.recommended_delay_ms(), 100);
            assert_eq!(DbErrorClass::Other.recommended_delay_ms(), 100);

            // Longer delay for timeouts
            assert_eq!(DbErrorClass::QueryTimeout.recommended_delay_ms(), 500);

            // Longest delay for migration locks
            assert_eq!(DbErrorClass::MigrationLocked.recommended_delay_ms(), 1000);
            assert_eq!(DbErrorClass::HostUnreachable.recommended_delay_ms(), 1000);

            // Non-retryable errors have 0 delay
            assert_eq!(DbErrorClass::AuthenticationFailed.recommended_delay_ms(), 0);
            assert_eq!(
                DbErrorClass::SchemaVersionConflict.recommended_delay_ms(),
                0
            );
            assert_eq!(DbErrorClass::MigrationIntegrity.recommended_delay_ms(), 0);
        }

        #[test]
        fn test_retriable_trait_for_sqlx_error() {
            // Pool timeout is retriable
            let err = sqlx::Error::PoolTimedOut;
            assert!(err.is_retriable());

            // Pool closed is not retriable
            let err = sqlx::Error::PoolClosed;
            assert!(!err.is_retriable());
        }
    }

    // ==========================================================================
    // SQLite Error Code Mapping Tests
    // ==========================================================================

    mod sqlite_error_mapping {
        use super::*;

        #[test]
        fn test_sqlite_busy_error_code() {
            // SQLITE_BUSY (5) - database is locked
            let class = classify_sqlite_error(Some("5"), "");
            assert_eq!(class, DbErrorClass::LockContention);
            assert!(class.is_retryable());

            // Alternative representation
            let class = classify_sqlite_error(Some("SQLITE_BUSY"), "");
            assert_eq!(class, DbErrorClass::LockContention);
        }

        #[test]
        fn test_sqlite_locked_error_code() {
            // SQLITE_LOCKED (6) - table is locked
            let class = classify_sqlite_error(Some("6"), "");
            assert_eq!(class, DbErrorClass::LockContention);
            assert!(class.is_retryable());

            let class = classify_sqlite_error(Some("SQLITE_LOCKED"), "");
            assert_eq!(class, DbErrorClass::LockContention);
        }

        #[test]
        fn test_sqlite_ioerr_error_code() {
            // SQLITE_IOERR (10) - I/O error
            let class = classify_sqlite_error(Some("10"), "");
            assert_eq!(class, DbErrorClass::HostUnreachable);

            let class = classify_sqlite_error(Some("SQLITE_IOERR"), "");
            assert_eq!(class, DbErrorClass::HostUnreachable);

            // IOERR with timeout message becomes QueryTimeout
            // Note: message must contain "timeout" (exact substring match)
            let class = classify_sqlite_error(Some("10"), "operation timeout exceeded");
            assert_eq!(class, DbErrorClass::QueryTimeout);
        }

        #[test]
        fn test_sqlite_cantopen_error_code() {
            // SQLITE_CANTOPEN (14) - unable to open database file
            let class = classify_sqlite_error(Some("14"), "");
            assert_eq!(class, DbErrorClass::HostUnreachable);

            let class = classify_sqlite_error(Some("SQLITE_CANTOPEN"), "");
            assert_eq!(class, DbErrorClass::HostUnreachable);
        }

        #[test]
        fn test_sqlite_auth_error_code() {
            // SQLITE_AUTH (23) - authorization denied
            let class = classify_sqlite_error(Some("23"), "");
            assert_eq!(class, DbErrorClass::AuthenticationFailed);
            assert!(!class.is_retryable());

            let class = classify_sqlite_error(Some("SQLITE_AUTH"), "");
            assert_eq!(class, DbErrorClass::AuthenticationFailed);
        }

        #[test]
        fn test_sqlite_message_based_fallback() {
            // Fallback to message-based detection when code is unknown
            let class = classify_sqlite_error(None, "database is locked by another process");
            assert_eq!(class, DbErrorClass::LockContention);

            let class = classify_sqlite_error(None, "the database is busy");
            assert_eq!(class, DbErrorClass::LockContention);

            let class = classify_sqlite_error(None, "operation timeout exceeded");
            assert_eq!(class, DbErrorClass::QueryTimeout);

            let class = classify_sqlite_error(None, "unable to open database file");
            assert_eq!(class, DbErrorClass::HostUnreachable);

            let class = classify_sqlite_error(None, "cannot open file");
            assert_eq!(class, DbErrorClass::HostUnreachable);
        }

        #[test]
        fn test_sqlite_unknown_code_defaults_to_other() {
            let class = classify_sqlite_error(Some("999"), "some unknown error");
            assert_eq!(class, DbErrorClass::Other);
            assert!(!class.is_retryable());
        }
    }

    // ==========================================================================
    // PostgreSQL Error Code Mapping Tests
    // ==========================================================================

    mod postgres_error_mapping {
        use super::*;

        #[test]
        fn test_postgres_connection_exception_class_08() {
            // Class 08 - Connection Exception
            let class = classify_postgres_error(Some("08000"), "");
            assert_eq!(class, DbErrorClass::HostUnreachable);

            let class = classify_postgres_error(Some("08003"), "");
            assert_eq!(class, DbErrorClass::HostUnreachable);

            let class = classify_postgres_error(Some("08006"), "");
            assert_eq!(class, DbErrorClass::HostUnreachable);
        }

        #[test]
        fn test_postgres_auth_errors() {
            // Invalid Authorization Specification
            let class = classify_postgres_error(Some("28000"), "");
            assert_eq!(class, DbErrorClass::AuthenticationFailed);

            let class = classify_postgres_error(Some("28P01"), "");
            assert_eq!(class, DbErrorClass::AuthenticationFailed);
        }

        #[test]
        fn test_postgres_insufficient_resources_class_53() {
            // Class 53 - Insufficient Resources
            let class = classify_postgres_error(Some("53000"), "");
            assert_eq!(class, DbErrorClass::PoolExhausted);

            let class = classify_postgres_error(Some("53100"), "");
            assert_eq!(class, DbErrorClass::PoolExhausted);

            let class = classify_postgres_error(Some("53200"), "");
            assert_eq!(class, DbErrorClass::PoolExhausted);
        }

        #[test]
        fn test_postgres_query_canceled() {
            // 57014 - query_canceled (statement timeout)
            let class = classify_postgres_error(Some("57014"), "");
            assert_eq!(class, DbErrorClass::QueryTimeout);
            assert!(class.is_retryable());
        }

        #[test]
        fn test_postgres_lock_errors() {
            // 55P03 - lock_not_available
            let class = classify_postgres_error(Some("55P03"), "");
            assert_eq!(class, DbErrorClass::MigrationLocked);

            // 40P01 - deadlock_detected
            let class = classify_postgres_error(Some("40P01"), "");
            assert_eq!(class, DbErrorClass::LockContention);

            // 40001 - serialization_failure
            let class = classify_postgres_error(Some("40001"), "");
            assert_eq!(class, DbErrorClass::LockContention);

            // 55006 - object_in_use
            let class = classify_postgres_error(Some("55006"), "");
            assert_eq!(class, DbErrorClass::MigrationLocked);
        }

        #[test]
        fn test_postgres_message_based_fallback() {
            // Fallback to message-based detection
            let class = classify_postgres_error(None, "connection refused to host");
            assert_eq!(class, DbErrorClass::HostUnreachable);

            let class = classify_postgres_error(None, "could not connect to server");
            assert_eq!(class, DbErrorClass::HostUnreachable);

            let class = classify_postgres_error(None, "password authentication failed");
            assert_eq!(class, DbErrorClass::AuthenticationFailed);

            let class = classify_postgres_error(None, "statement timeout");
            assert_eq!(class, DbErrorClass::QueryTimeout);

            let class = classify_postgres_error(None, "query timed out");
            assert_eq!(class, DbErrorClass::QueryTimeout);

            let class = classify_postgres_error(None, "table locked");
            assert_eq!(class, DbErrorClass::LockContention);

            let class = classify_postgres_error(None, "deadlock detected");
            assert_eq!(class, DbErrorClass::LockContention);
        }

        #[test]
        fn test_postgres_unknown_code_defaults_to_other() {
            let class = classify_postgres_error(Some("XX999"), "some unknown error");
            assert_eq!(class, DbErrorClass::Other);
        }
    }

    // ==========================================================================
    // Connection vs Query Error Distinction Tests
    // ==========================================================================

    mod connection_vs_query_errors {
        use super::*;
        use std::io::{Error as IoError, ErrorKind};

        #[test]
        fn test_connection_io_errors() {
            // Connection refused
            let io_err = IoError::new(ErrorKind::ConnectionRefused, "connection refused");
            assert!(is_connection_error(&io_err));

            // Connection reset
            let io_err = IoError::new(ErrorKind::ConnectionReset, "connection reset");
            assert!(is_connection_error(&io_err));

            // Connection aborted
            let io_err = IoError::new(ErrorKind::ConnectionAborted, "connection aborted");
            assert!(is_connection_error(&io_err));

            // Not connected
            let io_err = IoError::new(ErrorKind::NotConnected, "not connected");
            assert!(is_connection_error(&io_err));

            // Address not available
            let io_err = IoError::new(ErrorKind::AddrNotAvailable, "address not available");
            assert!(is_connection_error(&io_err));

            // Timed out
            let io_err = IoError::new(ErrorKind::TimedOut, "timed out");
            assert!(is_connection_error(&io_err));
        }

        #[test]
        fn test_non_connection_io_errors() {
            // Permission denied - not a connection error
            let io_err = IoError::new(ErrorKind::PermissionDenied, "permission denied");
            assert!(!is_connection_error(&io_err));

            // Not found
            let io_err = IoError::new(ErrorKind::NotFound, "not found");
            assert!(!is_connection_error(&io_err));

            // Already exists
            let io_err = IoError::new(ErrorKind::AlreadyExists, "already exists");
            assert!(!is_connection_error(&io_err));

            // Invalid input
            let io_err = IoError::new(ErrorKind::InvalidInput, "invalid input");
            assert!(!is_connection_error(&io_err));

            // Other
            let io_err = IoError::new(ErrorKind::Other, "other error");
            assert!(!is_connection_error(&io_err));
        }

        #[test]
        fn test_io_error_classification_through_sqlx() {
            // Connection refused IO error should classify as HostUnreachable
            let io_err = IoError::new(ErrorKind::ConnectionRefused, "connection refused");
            let sqlx_err = sqlx::Error::Io(io_err);
            let class = classify_sqlx_error(&sqlx_err, DatabaseBackend::Sqlite);
            assert_eq!(class, DbErrorClass::HostUnreachable);
            assert!(!class.is_retryable());
        }

        #[test]
        fn test_non_connection_io_error_classification() {
            // Non-connection IO error should classify as Other
            let io_err = IoError::new(ErrorKind::PermissionDenied, "permission denied");
            let sqlx_err = sqlx::Error::Io(io_err);
            let class = classify_sqlx_error(&sqlx_err, DatabaseBackend::Sqlite);
            assert_eq!(class, DbErrorClass::Other);
        }

        #[test]
        fn test_tls_error_is_connection_error() {
            // TLS errors indicate connection issues
            let tls_err = Box::new(IoError::new(ErrorKind::Other, "TLS handshake failed"));
            let sqlx_err = sqlx::Error::Tls(tls_err);
            let class = classify_sqlx_error(&sqlx_err, DatabaseBackend::Postgres);
            assert_eq!(class, DbErrorClass::HostUnreachable);
        }

        #[test]
        fn test_protocol_timeout_is_query_error() {
            // Protocol timeout should be QueryTimeout
            let sqlx_err = sqlx::Error::Protocol("statement timeout exceeded".to_string());
            let class = classify_sqlx_error(&sqlx_err, DatabaseBackend::Postgres);
            assert_eq!(class, DbErrorClass::QueryTimeout);
            assert!(class.is_retryable());

            let sqlx_err = sqlx::Error::Protocol("query timed out".to_string());
            let class = classify_sqlx_error(&sqlx_err, DatabaseBackend::Postgres);
            assert_eq!(class, DbErrorClass::QueryTimeout);
        }

        #[test]
        fn test_protocol_connection_error() {
            // Protocol connection errors should be HostUnreachable
            let sqlx_err = sqlx::Error::Protocol("connection lost".to_string());
            let class = classify_sqlx_error(&sqlx_err, DatabaseBackend::Postgres);
            assert_eq!(class, DbErrorClass::HostUnreachable);
        }

        #[test]
        fn test_protocol_other_error() {
            // Other protocol errors
            let sqlx_err = sqlx::Error::Protocol("unknown protocol error".to_string());
            let class = classify_sqlx_error(&sqlx_err, DatabaseBackend::Postgres);
            assert_eq!(class, DbErrorClass::Other);
        }
    }

    // ==========================================================================
    // Pool Error Classification Tests
    // ==========================================================================

    mod pool_errors {
        use super::*;

        #[test]
        fn test_pool_timeout_classification() {
            let err = sqlx::Error::PoolTimedOut;

            // Same classification regardless of backend
            assert_eq!(
                classify_sqlx_error(&err, DatabaseBackend::Sqlite),
                DbErrorClass::PoolExhausted
            );
            assert_eq!(
                classify_sqlx_error(&err, DatabaseBackend::Postgres),
                DbErrorClass::PoolExhausted
            );

            // Pool exhaustion is retryable
            assert!(DbErrorClass::PoolExhausted.is_retryable());
        }

        #[test]
        fn test_pool_closed_not_retryable() {
            let err = sqlx::Error::PoolClosed;
            let class = classify_sqlx_error(&err, DatabaseBackend::Sqlite);
            assert_eq!(class, DbErrorClass::Other);
            assert!(!class.is_retryable());
        }

        #[test]
        fn test_pool_exhausted_delay() {
            // Pool exhausted should have short delay
            assert_eq!(DbErrorClass::PoolExhausted.recommended_delay_ms(), 100);
        }
    }

    // ==========================================================================
    // Error Description Tests
    // ==========================================================================

    mod error_descriptions {
        use super::*;

        #[test]
        fn test_error_class_descriptions() {
            assert!(!DbErrorClass::HostUnreachable.description().is_empty());
            assert!(!DbErrorClass::AuthenticationFailed.description().is_empty());
            assert!(!DbErrorClass::PoolExhausted.description().is_empty());
            assert!(!DbErrorClass::QueryTimeout.description().is_empty());
            assert!(!DbErrorClass::MigrationLocked.description().is_empty());
            assert!(!DbErrorClass::LockContention.description().is_empty());
            assert!(!DbErrorClass::SchemaVersionConflict.description().is_empty());
            assert!(!DbErrorClass::MigrationIntegrity.description().is_empty());
            assert!(!DbErrorClass::Other.description().is_empty());
        }

        #[test]
        fn test_descriptions_are_human_readable() {
            // Descriptions should contain meaningful content
            assert!(DbErrorClass::HostUnreachable
                .description()
                .contains("unreachable"));
            assert!(DbErrorClass::AuthenticationFailed
                .description()
                .contains("authentication"));
            assert!(DbErrorClass::PoolExhausted.description().contains("pool"));
            // Note: description is "Query timed out" (contains "timed")
            assert!(DbErrorClass::QueryTimeout.description().contains("timed"));
            assert!(DbErrorClass::LockContention.description().contains("lock"));
            assert!(DbErrorClass::SchemaVersionConflict
                .description()
                .contains("version"));
        }

        #[test]
        fn test_all_descriptions_unique() {
            let descriptions = [
                DbErrorClass::HostUnreachable.description(),
                DbErrorClass::AuthenticationFailed.description(),
                DbErrorClass::PoolExhausted.description(),
                DbErrorClass::QueryTimeout.description(),
                DbErrorClass::MigrationLocked.description(),
                DbErrorClass::LockContention.description(),
                DbErrorClass::SchemaVersionConflict.description(),
                DbErrorClass::MigrationIntegrity.description(),
                DbErrorClass::Other.description(),
            ];

            for (i, d1) in descriptions.iter().enumerate() {
                for (j, d2) in descriptions.iter().enumerate() {
                    if i != j {
                        assert_ne!(d1, d2, "Descriptions at {} and {} should differ", i, j);
                    }
                }
            }
        }
    }

    // ==========================================================================
    // Cross-Backend Consistency Tests
    // ==========================================================================

    mod cross_backend_consistency {
        use super::*;

        #[test]
        fn test_pool_timeout_consistent_across_backends() {
            let err = sqlx::Error::PoolTimedOut;
            let sqlite_class = classify_sqlx_error(&err, DatabaseBackend::Sqlite);
            let postgres_class = classify_sqlx_error(&err, DatabaseBackend::Postgres);
            assert_eq!(sqlite_class, postgres_class);
        }

        #[test]
        fn test_pool_closed_consistent_across_backends() {
            let err = sqlx::Error::PoolClosed;
            let sqlite_class = classify_sqlx_error(&err, DatabaseBackend::Sqlite);
            let postgres_class = classify_sqlx_error(&err, DatabaseBackend::Postgres);
            assert_eq!(sqlite_class, postgres_class);
        }

        #[test]
        fn test_tls_error_consistent_across_backends() {
            let tls_err = Box::new(std::io::Error::new(std::io::ErrorKind::Other, "TLS error"));
            let err = sqlx::Error::Tls(tls_err);
            let sqlite_class = classify_sqlx_error(&err, DatabaseBackend::Sqlite);

            let tls_err = Box::new(std::io::Error::new(std::io::ErrorKind::Other, "TLS error"));
            let err = sqlx::Error::Tls(tls_err);
            let postgres_class = classify_sqlx_error(&err, DatabaseBackend::Postgres);

            assert_eq!(sqlite_class, postgres_class);
            assert_eq!(sqlite_class, DbErrorClass::HostUnreachable);
        }
    }
}

// Implement Retriable for sqlx::Error using database-agnostic classification
impl Retriable for sqlx::Error {
    fn is_retriable(&self) -> bool {
        // Use SQLite classification as default since it's more conservative
        // The specific backend should use with_db_retry for backend-aware classification
        classify_sqlx_error(self, DatabaseBackend::Sqlite).is_retryable()
    }
}
