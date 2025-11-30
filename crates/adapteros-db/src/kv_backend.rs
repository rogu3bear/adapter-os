//! KV storage backend integration for adapteros-db
//!
//! This module provides integration between the adapteros-storage KV backend
//! and the database layer, enabling dual-write and migration scenarios.

use adapteros_core::{AosError, Result};
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

// Re-export KV types from adapteros-storage
pub use adapteros_storage::kv::KvBackend;
pub use adapteros_storage::kv::IndexManager;
pub use adapteros_storage::redb::RedbBackend;
pub use adapteros_storage::StorageError;

/// Wrapper around KV backend that includes index management
///
/// This provides a unified interface for KV operations with automatic
/// index maintenance for common query patterns.
#[derive(Clone)]
pub struct KvDb {
    /// The underlying KV backend (trait object)
    backend: Arc<dyn KvBackend>,
    /// Index manager for secondary indexes
    index_manager: Arc<IndexManager>,
}

impl KvDb {
    /// Create a new KvDb with the given backend and index manager
    pub fn new(backend: Arc<dyn KvBackend>, index_manager: Arc<IndexManager>) -> Self {
        Self {
            backend,
            index_manager,
        }
    }

    /// Initialize a new KvDb with a redb backend at the given path
    ///
    /// This creates both the backend and index manager, initializing
    /// all necessary indexes for adapter storage.
    pub fn init_redb(path: &Path) -> Result<Self> {
        // Create the redb backend
        let backend = RedbBackend::open(path)
            .map_err(|e| AosError::Database(format!("Failed to open redb backend: {}", e)))?;

        let backend = Arc::new(backend) as Arc<dyn KvBackend>;
        let index_manager = Arc::new(IndexManager::new(backend.clone()));

        Ok(Self {
            backend,
            index_manager,
        })
    }

    /// Initialize an in-memory KvDb for testing
    pub fn init_in_memory() -> Result<Self> {
        // Create in-memory backend
        let backend = RedbBackend::open_in_memory()
            .map_err(|e| AosError::Database(format!("Failed to create in-memory backend: {}", e)))?;

        let backend = Arc::new(backend) as Arc<dyn KvBackend>;
        let index_manager = Arc::new(IndexManager::new(backend.clone()));

        Ok(Self {
            backend,
            index_manager,
        })
    }

    /// Get the underlying KV backend
    pub fn backend(&self) -> &Arc<dyn KvBackend> {
        &self.backend
    }

    /// Get the index manager
    pub fn index_manager(&self) -> &Arc<IndexManager> {
        &self.index_manager
    }

    /// Get a value by key
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.backend
            .get(key)
            .await
            .map_err(|e| AosError::Database(format!("KV get failed: {}", e)))
    }

    /// Set a value for a key
    pub async fn set(&self, key: &str, value: Vec<u8>) -> Result<()> {
        self.backend
            .set(key, value)
            .await
            .map_err(|e| AosError::Database(format!("KV set failed: {}", e)))
    }

    /// Delete a key
    pub async fn delete(&self, key: &str) -> Result<bool> {
        self.backend
            .delete(key)
            .await
            .map_err(|e| AosError::Database(format!("KV delete failed: {}", e)))
    }

    /// Check if a key exists
    pub async fn exists(&self, key: &str) -> Result<bool> {
        self.backend
            .exists(key)
            .await
            .map_err(|e| AosError::Database(format!("KV exists failed: {}", e)))
    }

    /// Scan keys with a prefix
    pub async fn scan_prefix(&self, prefix: &str) -> Result<Vec<String>> {
        self.backend
            .scan_prefix(prefix)
            .await
            .map_err(|e| AosError::Database(format!("KV scan_prefix failed: {}", e)))
    }

    /// Query keys using an index
    pub async fn query_by_index(&self, index_name: &str, index_value: &str) -> Result<Vec<String>> {
        self.index_manager
            .query_index(index_name, index_value)
            .await
            .map_err(|e| AosError::Database(format!("Index query failed: {}", e)))
    }

    /// Add an entry to a secondary index
    pub async fn add_to_index(
        &self,
        index_name: &str,
        index_value: &str,
        entity_id: &str,
    ) -> Result<()> {
        self.index_manager
            .add_to_index(index_name, index_value, entity_id)
            .await
            .map_err(|e| AosError::Database(format!("Index add failed: {}", e)))
    }

    /// Remove an entry from a secondary index
    pub async fn remove_from_index(
        &self,
        index_name: &str,
        index_value: &str,
        entity_id: &str,
    ) -> Result<()> {
        self.index_manager
            .remove_from_index(index_name, index_value, entity_id)
            .await
            .map_err(|e| AosError::Database(format!("Index remove failed: {}", e)))
    }

    /// Update an index entry (remove old, add new)
    pub async fn update_index(
        &self,
        index_name: &str,
        old_value: Option<&str>,
        new_value: &str,
        entity_id: &str,
    ) -> Result<()> {
        self.index_manager
            .update_index(index_name, old_value, new_value, entity_id)
            .await
            .map_err(|e| AosError::Database(format!("Index update failed: {}", e)))
    }
}

/// Implement KvBackend for KvDb by delegating to the inner backend
///
/// This allows KvDb to be used wherever a KvBackend is expected,
/// making it compatible with generic storage operations.
#[async_trait]
impl KvBackend for KvDb {
    async fn get(&self, key: &str) -> std::result::Result<Option<Vec<u8>>, StorageError> {
        self.backend.get(key).await
    }

    async fn set(&self, key: &str, value: Vec<u8>) -> std::result::Result<(), StorageError> {
        self.backend.set(key, value).await
    }

    async fn delete(&self, key: &str) -> std::result::Result<bool, StorageError> {
        self.backend.delete(key).await
    }

    async fn exists(&self, key: &str) -> std::result::Result<bool, StorageError> {
        self.backend.exists(key).await
    }

    async fn scan_prefix(&self, prefix: &str) -> std::result::Result<Vec<String>, StorageError> {
        self.backend.scan_prefix(prefix).await
    }

    async fn batch_get(&self, keys: &[String]) -> std::result::Result<Vec<Option<Vec<u8>>>, StorageError> {
        self.backend.batch_get(keys).await
    }

    async fn batch_set(&self, pairs: Vec<(String, Vec<u8>)>) -> std::result::Result<(), StorageError> {
        self.backend.batch_set(pairs).await
    }

    async fn batch_delete(&self, keys: &[String]) -> std::result::Result<usize, StorageError> {
        self.backend.batch_delete(keys).await
    }

    async fn set_add(&self, key: &str, member: &str) -> std::result::Result<(), StorageError> {
        self.backend.set_add(key, member).await
    }

    async fn set_remove(&self, key: &str, member: &str) -> std::result::Result<(), StorageError> {
        self.backend.set_remove(key, member).await
    }

    async fn set_members(&self, key: &str) -> std::result::Result<Vec<String>, StorageError> {
        self.backend.set_members(key).await
    }

    async fn set_is_member(&self, key: &str, member: &str) -> std::result::Result<bool, StorageError> {
        self.backend.set_is_member(key, member).await
    }
}
