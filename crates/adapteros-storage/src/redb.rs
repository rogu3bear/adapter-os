//! Redb backend implementation for KvBackend trait
//!
//! Provides a high-performance embedded key-value store using redb.
//! Redb is a simple, portable, high-performance, ACID, embedded key-value database.

use crate::error::StorageError;
use crate::kv::backend::KvBackend;

// Re-export for convenience
pub use crate::error::StorageError as RedbError;
use async_trait::async_trait;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::path::Path;
use std::sync::Arc;

/// Table definitions
const DATA_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("data");
const SETS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("sets");

/// Redb storage backend
///
/// This backend uses redb for persistent key-value storage with:
/// - ACID transactions
/// - Zero-copy reads
/// - Crash recovery
/// - Multi-threaded access
pub struct RedbBackend {
    db: Arc<Database>,
}

impl RedbBackend {
    /// Open a redb database at the given path
    ///
    /// This will create the database file if it doesn't exist and
    /// initialize the required tables.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let db = Database::create(path)
            .map_err(|e| StorageError::BackendError(format!("Failed to create database: {}", e)))?;

        // Initialize tables
        let write_txn = db.begin_write().map_err(|e| {
            StorageError::BackendError(format!("Failed to begin write transaction: {}", e))
        })?;
        {
            write_txn.open_table(DATA_TABLE).map_err(|e| {
                StorageError::BackendError(format!("Failed to open data table: {}", e))
            })?;
            write_txn.open_table(SETS_TABLE).map_err(|e| {
                StorageError::BackendError(format!("Failed to open sets table: {}", e))
            })?;
        }
        write_txn.commit().map_err(|e| {
            StorageError::BackendError(format!("Failed to commit table creation: {}", e))
        })?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Open an in-memory database for testing
    ///
    /// This creates a temporary database that exists only in memory.
    /// Useful for unit tests and temporary storage.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let db = Database::builder()
            .create_with_backend(redb::backends::InMemoryBackend::new())
            .map_err(|e| {
                StorageError::BackendError(format!("Failed to create in-memory database: {}", e))
            })?;

        // Initialize tables
        let write_txn = db.begin_write().map_err(|e| {
            StorageError::BackendError(format!("Failed to begin write transaction: {}", e))
        })?;
        {
            write_txn.open_table(DATA_TABLE).map_err(|e| {
                StorageError::BackendError(format!("Failed to open data table: {}", e))
            })?;
            write_txn.open_table(SETS_TABLE).map_err(|e| {
                StorageError::BackendError(format!("Failed to open sets table: {}", e))
            })?;
        }
        write_txn.commit().map_err(|e| {
            StorageError::BackendError(format!("Failed to commit table creation: {}", e))
        })?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Get set member key
    ///
    /// For set operations, we store members as composite keys in the format:
    /// "set:{set_key}::{member}"
    fn set_member_key(set_key: &str, member: &str) -> String {
        format!("set:{}::{}", set_key, member)
    }

    /// Get set prefix for scanning
    fn set_prefix(set_key: &str) -> String {
        format!("set:{}::", set_key)
    }

    /// Scan prefix with values (internal helper for advanced queries)
    pub async fn scan_prefix_with_values(
        &self,
        prefix: &str,
        limit: usize,
    ) -> Result<Vec<(String, Vec<u8>)>, StorageError> {
        let db = self.db.clone();
        let prefix_owned = prefix.to_string();

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read().map_err(|e| {
                StorageError::BackendError(format!("Failed to begin read transaction: {}", e))
            })?;

            let table = read_txn.open_table(DATA_TABLE).map_err(|e| {
                StorageError::BackendError(format!("Failed to open data table: {}", e))
            })?;

            let mut results = Vec::new();
            let mut count = 0;

            let iter = table.iter().map_err(|e| {
                StorageError::BackendError(format!("Failed to iterate table: {}", e))
            })?;

            for item in iter {
                let (key, value) = item.map_err(|e| {
                    StorageError::BackendError(format!("Failed to read item: {}", e))
                })?;

                let key_str = key.value();
                if key_str.starts_with(&prefix_owned) {
                    results.push((key_str.to_string(), value.value().to_vec()));
                    count += 1;

                    if count >= limit {
                        break;
                    }
                }
            }

            Ok(results)
        })
        .await
        .map_err(|e| StorageError::BackendError(format!("Task join error: {}", e)))?
    }

    /// Scan range (internal helper for advanced queries)
    pub async fn scan_range(
        &self,
        start: &str,
        end: &str,
        limit: usize,
    ) -> Result<Vec<(String, Vec<u8>)>, StorageError> {
        let db = self.db.clone();
        let start_owned = start.to_string();
        let end_owned = end.to_string();

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read().map_err(|e| {
                StorageError::BackendError(format!("Failed to begin read transaction: {}", e))
            })?;

            let table = read_txn.open_table(DATA_TABLE).map_err(|e| {
                StorageError::BackendError(format!("Failed to open data table: {}", e))
            })?;

            let mut results = Vec::new();
            let mut count = 0;

            let iter = table
                .range(start_owned.as_str()..end_owned.as_str())
                .map_err(|e| {
                    StorageError::BackendError(format!("Failed to create range iterator: {}", e))
                })?;

            for item in iter {
                let (key, value) = item.map_err(|e| {
                    StorageError::BackendError(format!("Failed to read item: {}", e))
                })?;

                results.push((key.value().to_string(), value.value().to_vec()));
                count += 1;

                if count >= limit {
                    break;
                }
            }

            Ok(results)
        })
        .await
        .map_err(|e| StorageError::BackendError(format!("Task join error: {}", e)))?
    }
}

#[async_trait]
impl KvBackend for RedbBackend {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let db = self.db.clone();
        let key_owned = key.to_string();

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read().map_err(|e| {
                StorageError::BackendError(format!("Failed to begin read transaction: {}", e))
            })?;

            let table = read_txn.open_table(DATA_TABLE).map_err(|e| {
                StorageError::BackendError(format!("Failed to open data table: {}", e))
            })?;

            match table.get(key_owned.as_str()) {
                Ok(Some(value)) => {
                    let bytes = value.value().to_vec();
                    Ok(Some(bytes))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(StorageError::BackendError(format!(
                    "Failed to get value: {}",
                    e
                ))),
            }
        })
        .await
        .map_err(|e| StorageError::BackendError(format!("Task join error: {}", e)))?
    }

    async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), StorageError> {
        let db = self.db.clone();
        let key_owned = key.to_string();

        tokio::task::spawn_blocking(move || {
            let write_txn = db.begin_write().map_err(|e| {
                StorageError::BackendError(format!("Failed to begin write transaction: {}", e))
            })?;

            {
                let mut table = write_txn.open_table(DATA_TABLE).map_err(|e| {
                    StorageError::BackendError(format!("Failed to open data table: {}", e))
                })?;

                table
                    .insert(key_owned.as_str(), value.as_slice())
                    .map_err(|e| {
                        StorageError::BackendError(format!("Failed to insert value: {}", e))
                    })?;
            }

            write_txn.commit().map_err(|e| {
                StorageError::BackendError(format!("Failed to commit transaction: {}", e))
            })?;

            Ok(())
        })
        .await
        .map_err(|e| StorageError::BackendError(format!("Task join error: {}", e)))?
    }

    async fn delete(&self, key: &str) -> Result<bool, StorageError> {
        let db = self.db.clone();
        let key_owned = key.to_string();

        tokio::task::spawn_blocking(move || {
            let write_txn = db.begin_write().map_err(|e| {
                StorageError::BackendError(format!("Failed to begin write transaction: {}", e))
            })?;

            let existed = {
                let mut table = write_txn.open_table(DATA_TABLE).map_err(|e| {
                    StorageError::BackendError(format!("Failed to open data table: {}", e))
                })?;

                let removed = table.remove(key_owned.as_str()).map_err(|e| {
                    StorageError::BackendError(format!("Failed to remove value: {}", e))
                })?;
                removed.is_some()
            };

            write_txn.commit().map_err(|e| {
                StorageError::BackendError(format!("Failed to commit transaction: {}", e))
            })?;

            Ok(existed)
        })
        .await
        .map_err(|e| StorageError::BackendError(format!("Task join error: {}", e)))?
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        let result = self.get(key).await?;
        Ok(result.is_some())
    }

    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let db = self.db.clone();
        let prefix_owned = prefix.to_string();

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read().map_err(|e| {
                StorageError::BackendError(format!("Failed to begin read transaction: {}", e))
            })?;

            let table = read_txn.open_table(DATA_TABLE).map_err(|e| {
                StorageError::BackendError(format!("Failed to open data table: {}", e))
            })?;

            let mut results = Vec::new();

            let iter = table.iter().map_err(|e| {
                StorageError::BackendError(format!("Failed to iterate table: {}", e))
            })?;

            for item in iter {
                let (key, _value) = item.map_err(|e| {
                    StorageError::BackendError(format!("Failed to read item: {}", e))
                })?;

                let key_str = key.value();
                if key_str.starts_with(&prefix_owned) {
                    results.push(key_str.to_string());
                }
            }

            Ok(results)
        })
        .await
        .map_err(|e| StorageError::BackendError(format!("Task join error: {}", e)))?
    }

    async fn batch_get(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>, StorageError> {
        let mut results = Vec::with_capacity(keys.len());
        for key in keys {
            results.push(self.get(key).await?);
        }
        Ok(results)
    }

    async fn batch_set(&self, pairs: Vec<(String, Vec<u8>)>) -> Result<(), StorageError> {
        if pairs.is_empty() {
            return Ok(());
        }

        let db = self.db.clone();

        tokio::task::spawn_blocking(move || {
            let write_txn = db.begin_write().map_err(|e| {
                StorageError::BackendError(format!("Failed to begin write transaction: {}", e))
            })?;

            {
                let mut table = write_txn.open_table(DATA_TABLE).map_err(|e| {
                    StorageError::BackendError(format!("Failed to open data table: {}", e))
                })?;

                for (key, value) in pairs {
                    table.insert(key.as_str(), value.as_slice()).map_err(|e| {
                        StorageError::BackendError(format!("Failed to insert value: {}", e))
                    })?;
                }
            }

            write_txn.commit().map_err(|e| {
                StorageError::BackendError(format!("Failed to commit batch: {}", e))
            })?;

            Ok(())
        })
        .await
        .map_err(|e| StorageError::BackendError(format!("Task join error: {}", e)))?
    }

    async fn batch_delete(&self, keys: &[String]) -> Result<usize, StorageError> {
        if keys.is_empty() {
            return Ok(0);
        }

        let db = self.db.clone();
        let keys_owned: Vec<String> = keys.to_vec();

        tokio::task::spawn_blocking(move || {
            let write_txn = db.begin_write().map_err(|e| {
                StorageError::BackendError(format!("Failed to begin write transaction: {}", e))
            })?;

            let mut deleted_count = 0;

            {
                let mut table = write_txn.open_table(DATA_TABLE).map_err(|e| {
                    StorageError::BackendError(format!("Failed to open data table: {}", e))
                })?;

                for key in keys_owned {
                    if table
                        .remove(key.as_str())
                        .map_err(|e| {
                            StorageError::BackendError(format!("Failed to remove value: {}", e))
                        })?
                        .is_some()
                    {
                        deleted_count += 1;
                    }
                }
            }

            write_txn.commit().map_err(|e| {
                StorageError::BackendError(format!("Failed to commit batch delete: {}", e))
            })?;

            Ok(deleted_count)
        })
        .await
        .map_err(|e| StorageError::BackendError(format!("Task join error: {}", e)))?
    }

    async fn set_add(&self, key: &str, member: &str) -> Result<(), StorageError> {
        let db = self.db.clone();
        let composite_key = Self::set_member_key(key, member);

        tokio::task::spawn_blocking(move || {
            let write_txn = db.begin_write().map_err(|e| {
                StorageError::BackendError(format!("Failed to begin write transaction: {}", e))
            })?;

            {
                let mut table = write_txn.open_table(SETS_TABLE).map_err(|e| {
                    StorageError::BackendError(format!("Failed to open sets table: {}", e))
                })?;

                table.insert(composite_key.as_str(), "1").map_err(|e| {
                    StorageError::BackendError(format!("Failed to add set member: {}", e))
                })?;
            }

            write_txn.commit().map_err(|e| {
                StorageError::BackendError(format!("Failed to commit transaction: {}", e))
            })?;

            Ok(())
        })
        .await
        .map_err(|e| StorageError::BackendError(format!("Task join error: {}", e)))?
    }

    async fn set_remove(&self, key: &str, member: &str) -> Result<(), StorageError> {
        let db = self.db.clone();
        let composite_key = Self::set_member_key(key, member);

        tokio::task::spawn_blocking(move || {
            let write_txn = db.begin_write().map_err(|e| {
                StorageError::BackendError(format!("Failed to begin write transaction: {}", e))
            })?;

            {
                let mut table = write_txn.open_table(SETS_TABLE).map_err(|e| {
                    StorageError::BackendError(format!("Failed to open sets table: {}", e))
                })?;

                table.remove(composite_key.as_str()).map_err(|e| {
                    StorageError::BackendError(format!("Failed to remove set member: {}", e))
                })?;
            }

            write_txn.commit().map_err(|e| {
                StorageError::BackendError(format!("Failed to commit transaction: {}", e))
            })?;

            Ok(())
        })
        .await
        .map_err(|e| StorageError::BackendError(format!("Task join error: {}", e)))?
    }

    async fn set_members(&self, key: &str) -> Result<Vec<String>, StorageError> {
        let db = self.db.clone();
        let prefix = Self::set_prefix(key);
        let prefix_len = prefix.len();

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read().map_err(|e| {
                StorageError::BackendError(format!("Failed to begin read transaction: {}", e))
            })?;

            let table = read_txn.open_table(SETS_TABLE).map_err(|e| {
                StorageError::BackendError(format!("Failed to open sets table: {}", e))
            })?;

            let mut members = Vec::new();

            let iter = table.iter().map_err(|e| {
                StorageError::BackendError(format!("Failed to iterate sets table: {}", e))
            })?;

            for item in iter {
                let (key_guard, _) = item.map_err(|e| {
                    StorageError::BackendError(format!("Failed to read item: {}", e))
                })?;

                let key_str = key_guard.value();
                if key_str.starts_with(&prefix) {
                    // Extract member from composite key
                    let member = &key_str[prefix_len..];
                    members.push(member.to_string());
                }
            }

            Ok(members)
        })
        .await
        .map_err(|e| StorageError::BackendError(format!("Task join error: {}", e)))?
    }

    async fn set_is_member(&self, key: &str, member: &str) -> Result<bool, StorageError> {
        let db = self.db.clone();
        let composite_key = Self::set_member_key(key, member);

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read().map_err(|e| {
                StorageError::BackendError(format!("Failed to begin read transaction: {}", e))
            })?;

            let table = read_txn.open_table(SETS_TABLE).map_err(|e| {
                StorageError::BackendError(format!("Failed to open sets table: {}", e))
            })?;

            let exists = table
                .get(composite_key.as_str())
                .map_err(|e| StorageError::BackendError(format!("Failed to check member: {}", e)))?
                .is_some();

            Ok(exists)
        })
        .await
        .map_err(|e| StorageError::BackendError(format!("Task join error: {}", e)))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_operations() -> Result<(), StorageError> {
        let backend = RedbBackend::open_in_memory()?;

        // Test set and get
        backend.set("test_key", b"test_value".to_vec()).await?;
        let value = backend.get("test_key").await?;
        assert_eq!(value, Some(b"test_value".to_vec()));

        // Test exists
        assert!(backend.exists("test_key").await?);
        assert!(!backend.exists("nonexistent").await?);

        // Test delete
        assert!(backend.delete("test_key").await?);
        assert!(!backend.exists("test_key").await?);
        assert!(!backend.delete("test_key").await?);

        Ok(())
    }

    #[tokio::test]
    async fn test_scan_prefix() -> Result<(), StorageError> {
        let backend = RedbBackend::open_in_memory()?;

        // Insert multiple keys with same prefix
        backend.set("user:1", b"alice".to_vec()).await?;
        backend.set("user:2", b"bob".to_vec()).await?;
        backend.set("user:3", b"charlie".to_vec()).await?;
        backend.set("admin:1", b"admin".to_vec()).await?;

        // Scan with prefix - returns keys only
        let results = backend.scan_prefix("user:").await?;
        assert_eq!(results.len(), 3);

        // Scan with prefix and values
        let results = backend.scan_prefix_with_values("user:", 10).await?;
        assert_eq!(results.len(), 3);

        // Scan with limit
        let results = backend.scan_prefix_with_values("user:", 2).await?;
        assert_eq!(results.len(), 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_scan_range() -> Result<(), StorageError> {
        let backend = RedbBackend::open_in_memory()?;

        backend.set("key:1", b"value1".to_vec()).await?;
        backend.set("key:2", b"value2".to_vec()).await?;
        backend.set("key:3", b"value3".to_vec()).await?;
        backend.set("key:5", b"value5".to_vec()).await?;

        let results = backend.scan_range("key:2", "key:5", 10).await?;
        assert_eq!(results.len(), 2); // key:2 and key:3

        Ok(())
    }

    #[tokio::test]
    async fn test_batch_operations() -> Result<(), StorageError> {
        let backend = RedbBackend::open_in_memory()?;

        // Test batch_set
        let pairs = vec![
            ("key1".to_string(), b"value1".to_vec()),
            ("key2".to_string(), b"value2".to_vec()),
            ("key3".to_string(), b"value3".to_vec()),
        ];
        backend.batch_set(pairs).await?;

        // Verify all values were written
        assert_eq!(backend.get("key1").await?, Some(b"value1".to_vec()));
        assert_eq!(backend.get("key2").await?, Some(b"value2".to_vec()));
        assert_eq!(backend.get("key3").await?, Some(b"value3".to_vec()));

        // Test batch_delete
        let keys = vec!["key1".to_string(), "key2".to_string()];
        let deleted = backend.batch_delete(&keys).await?;
        assert_eq!(deleted, 2);

        assert!(!backend.exists("key1").await?);
        assert!(!backend.exists("key2").await?);
        assert!(backend.exists("key3").await?);

        Ok(())
    }

    #[tokio::test]
    async fn test_set_operations() -> Result<(), StorageError> {
        let backend = RedbBackend::open_in_memory()?;

        // Test set_add
        backend.set_add("myset", "member1").await?;
        backend.set_add("myset", "member2").await?;
        backend.set_add("myset", "member3").await?;

        // Test set_is_member
        assert!(backend.set_is_member("myset", "member1").await?);
        assert!(!backend.set_is_member("myset", "member4").await?);

        // Test set_members
        let members = backend.set_members("myset").await?;
        assert_eq!(members.len(), 3);

        // Test set_remove
        backend.set_remove("myset", "member2").await?;
        let members = backend.set_members("myset").await?;
        assert_eq!(members.len(), 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_batch_get() -> Result<(), StorageError> {
        let backend = RedbBackend::open_in_memory()?;

        backend.set("key1", b"value1".to_vec()).await?;
        backend.set("key2", b"value2".to_vec()).await?;

        let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
        let values = backend.batch_get(&keys).await?;

        assert_eq!(values.len(), 3);
        assert_eq!(values[0], Some(b"value1".to_vec()));
        assert_eq!(values[1], Some(b"value2".to_vec()));
        assert_eq!(values[2], None);

        Ok(())
    }
}
