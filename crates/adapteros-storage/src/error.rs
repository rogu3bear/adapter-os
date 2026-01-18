//! Storage error types

use thiserror::Error;

/// Errors that can occur during storage operations
#[derive(Debug, Error)]
pub enum StorageError {
    /// Key not found in storage
    #[error("Key not found: {0}")]
    NotFound(String),

    /// Serialization/deserialization failed
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Backend-specific error
    #[error("Backend error: {0}")]
    BackendError(String),

    /// Transaction failed
    #[error("Transaction error: {0}")]
    TransactionError(String),

    /// Index operation failed
    #[error("Index error: {0}")]
    IndexError(String),

    /// I/O error
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Invalid operation
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    /// Database is read-only
    #[error("Database is read-only")]
    ReadOnly,

    /// Lock acquisition failed
    #[error("Lock error: {0}")]
    LockError(String),

    /// Data validation failed
    #[error("Invalid data: {0}")]
    InvalidData(String),

    /// Concurrent modification detected
    #[error("Conflict: {0}")]
    ConflictError(String),
}

// ============================================================================
// Error conversions using impl_error_from_for! macro
// ============================================================================
//
// These conversions use the macro from adapteros-core to reduce boilerplate.

// Manual bincode conversion (macro doesn't handle Box<T> generics well)
impl From<Box<bincode::ErrorKind>> for StorageError {
    fn from(err: Box<bincode::ErrorKind>) -> Self {
        StorageError::SerializationError(err.to_string())
    }
}
adapteros_core::impl_error_from_for!(StorageError: redb::Error => BackendError);
adapteros_core::impl_error_from_for!(StorageError: redb::TransactionError => TransactionError);
adapteros_core::impl_error_from_for!(StorageError: redb::TableError => BackendError);
adapteros_core::impl_error_from_for!(StorageError: redb::CommitError => TransactionError);
adapteros_core::impl_error_from_for!(StorageError: redb::StorageError => BackendError);
