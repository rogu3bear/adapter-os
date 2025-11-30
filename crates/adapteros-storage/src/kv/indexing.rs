//! Indexing infrastructure for KV storage
//!
//! Provides secondary indexes to enable efficient queries beyond primary key lookups.
//! Replaces SQL indexes like idx_adapters_state, idx_adapters_tier, etc.

use crate::error::StorageError;
use crate::kv::backend::KvBackend;
use std::sync::Arc;

/// Index manager for maintaining secondary indexes
pub struct IndexManager {
    backend: Arc<dyn KvBackend>,
}

impl IndexManager {
    /// Create a new index manager
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    /// Add an entry to a secondary index
    ///
    /// # Arguments
    /// * `index_name` - Name of the index (e.g., "adapters_by_state")
    /// * `index_value` - Value to index by (e.g., "warm")
    /// * `entity_id` - ID of the entity being indexed
    pub async fn add_to_index(
        &self,
        index_name: &str,
        index_value: &str,
        entity_id: &str,
    ) -> Result<(), StorageError> {
        let index_key = format!("index:{}:{}:{}", index_name, index_value, entity_id);
        self.backend.set(&index_key, vec![1]).await?;
        Ok(())
    }

    /// Remove an entry from a secondary index
    pub async fn remove_from_index(
        &self,
        index_name: &str,
        index_value: &str,
        entity_id: &str,
    ) -> Result<(), StorageError> {
        let index_key = format!("index:{}:{}:{}", index_name, index_value, entity_id);
        self.backend.delete(&index_key).await?;
        Ok(())
    }

    /// Query an index to get all entity IDs matching a value
    pub async fn query_index(
        &self,
        index_name: &str,
        index_value: &str,
    ) -> Result<Vec<String>, StorageError> {
        let prefix = format!("index:{}:{}", index_name, index_value);
        let keys = self.backend.scan_prefix(&prefix).await?;

        // Extract entity IDs from index keys
        // Format: "index:{index_name}:{index_value}:{entity_id}"
        let entity_ids: Vec<String> = keys
            .iter()
            .filter_map(|key| {
                let parts: Vec<&str> = key.split(':').collect();
                if parts.len() >= 4 {
                    Some(parts[3..].join(":"))
                } else {
                    None
                }
            })
            .collect();

        Ok(entity_ids)
    }

    /// Update an index entry (remove old, add new)
    pub async fn update_index(
        &self,
        index_name: &str,
        old_value: Option<&str>,
        new_value: &str,
        entity_id: &str,
    ) -> Result<(), StorageError> {
        if let Some(old) = old_value {
            if old != new_value {
                self.remove_from_index(index_name, old, entity_id).await?;
            }
        }
        self.add_to_index(index_name, new_value, entity_id).await?;
        Ok(())
    }

    /// Remove all index entries for an entity across an index
    pub async fn remove_all_from_index(
        &self,
        index_name: &str,
        entity_id: &str,
    ) -> Result<(), StorageError> {
        // Scan for all entries with this entity_id
        let prefix = format!("index:{}", index_name);
        let all_keys = self.backend.scan_prefix(&prefix).await?;

        let keys_to_delete: Vec<String> = all_keys
            .into_iter()
            .filter(|key| key.ends_with(&format!(":{}", entity_id)))
            .collect();

        if !keys_to_delete.is_empty() {
            self.backend.batch_delete(&keys_to_delete).await?;
        }

        Ok(())
    }
}

/// Index definitions for adapters
pub mod adapter_indexes {
    /// Index name for querying adapters by state
    pub const BY_STATE: &str = "adapters_by_state";

    /// Index name for querying adapters by tier
    pub const BY_TIER: &str = "adapters_by_tier";

    /// Index name for querying adapters by tenant
    pub const BY_TENANT: &str = "adapters_by_tenant";

    /// Index name for querying adapters by hash
    pub const BY_HASH: &str = "adapters_by_hash";

    /// Index name for querying adapters by lifecycle_state
    pub const BY_LIFECYCLE_STATE: &str = "adapters_by_lifecycle_state";

    /// Index name for active adapters
    pub const BY_ACTIVE: &str = "adapters_by_active";

    /// Index name for pinned adapters
    pub const BY_PINNED: &str = "adapters_by_pinned";

    /// Index name for parent-child relationships (lineage)
    pub const BY_PARENT: &str = "adapters_by_parent";

    /// Index name for querying adapters by external adapter_id
    pub const BY_ADAPTER_ID: &str = "adapters_by_adapter_id";
}
