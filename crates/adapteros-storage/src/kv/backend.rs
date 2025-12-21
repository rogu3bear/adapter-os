//! KV backend trait
//!
//! Defines the interface for key-value storage backends.
//! Implementations can use SQLite, RocksDB, redb, or other KV stores.

use crate::error::StorageError;
use async_trait::async_trait;

/// Key-value storage backend trait
///
/// This trait is designed to be object-safe (dyn-compatible) for use
/// with Arc<dyn KvBackend>.
#[async_trait]
pub trait KvBackend: Send + Sync {
    /// Get a value by key
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError>;

    /// Set a value for a key
    async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), StorageError>;

    /// Delete a key
    async fn delete(&self, key: &str) -> Result<bool, StorageError>;

    /// Check if a key exists
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;

    /// Get all keys matching a prefix
    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<String>, StorageError>;

    /// Get multiple values by keys (batch operation)
    async fn batch_get(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>, StorageError>;

    /// Set multiple key-value pairs (batch operation)
    async fn batch_set(&self, pairs: Vec<(String, Vec<u8>)>) -> Result<(), StorageError>;

    /// Delete multiple keys (batch operation)
    async fn batch_delete(&self, keys: &[String]) -> Result<usize, StorageError>;

    /// Add a member to a set (for secondary indexes)
    async fn set_add(&self, key: &str, member: &str) -> Result<(), StorageError>;

    /// Remove a member from a set
    async fn set_remove(&self, key: &str, member: &str) -> Result<(), StorageError>;

    /// Get all members of a set
    async fn set_members(&self, key: &str) -> Result<Vec<String>, StorageError>;

    /// Check if a member exists in a set
    async fn set_is_member(&self, key: &str, member: &str) -> Result<bool, StorageError>;
}

/// Batch operation builder (non-trait, used for optimization)
pub struct KvBatch {
    operations: Vec<BatchOp>,
}

/// Individual batch operation
pub enum BatchOp {
    Put { key: String, value: Vec<u8> },
    Delete { key: String },
    SetAdd { key: String, member: String },
    SetRemove { key: String, member: String },
}

impl KvBatch {
    /// Create a new batch
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }

    /// Add a put operation
    pub fn put(&mut self, key: &str, value: Vec<u8>) {
        self.operations.push(BatchOp::Put {
            key: key.to_string(),
            value,
        });
    }

    /// Add a delete operation
    pub fn delete(&mut self, key: &str) {
        self.operations.push(BatchOp::Delete {
            key: key.to_string(),
        });
    }

    /// Add a set-add operation
    pub fn set_add(&mut self, key: &str, member: &str) {
        self.operations.push(BatchOp::SetAdd {
            key: key.to_string(),
            member: member.to_string(),
        });
    }

    /// Add a set-remove operation
    pub fn set_remove(&mut self, key: &str, member: &str) {
        self.operations.push(BatchOp::SetRemove {
            key: key.to_string(),
            member: member.to_string(),
        });
    }

    /// Get the operations
    pub fn into_operations(self) -> Vec<BatchOp> {
        self.operations
    }
}

impl Default for KvBatch {
    fn default() -> Self {
        Self::new()
    }
}
