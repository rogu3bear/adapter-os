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
