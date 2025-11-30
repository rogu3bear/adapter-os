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

impl From<bincode::Error> for StorageError {
    fn from(err: bincode::Error) -> Self {
        StorageError::SerializationError(err.to_string())
    }
}

impl From<redb::Error> for StorageError {
    fn from(err: redb::Error) -> Self {
        StorageError::BackendError(err.to_string())
    }
}

impl From<redb::TransactionError> for StorageError {
    fn from(err: redb::TransactionError) -> Self {
        StorageError::TransactionError(err.to_string())
    }
}

impl From<redb::TableError> for StorageError {
    fn from(err: redb::TableError) -> Self {
        StorageError::BackendError(err.to_string())
    }
}

impl From<redb::CommitError> for StorageError {
    fn from(err: redb::CommitError) -> Self {
        StorageError::TransactionError(err.to_string())
    }
}

impl From<redb::StorageError> for StorageError {
    fn from(err: redb::StorageError) -> Self {
        StorageError::BackendError(err.to_string())
    }
}
