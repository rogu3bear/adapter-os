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
        // IMPORTANT: Include trailing ':' to ensure exact prefix matching
        // Without it, "test-adapter-1" would falsely match "test-adapter-10"
        let prefix = format!("index:{}:{}:", index_name, index_value);
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

    /// Index name for querying adapters by content hash (global deduplication key)
    pub const BY_CONTENT_HASH: &str = "adapters_by_content_hash";

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

    /// Index name for tenant-scoped state queries (compound: tenant_id:current_state)
    ///
    /// This index enables O(1) lookups for adapters by both tenant and state,
    /// avoiding the need to load all adapters for a given state and filter
    /// by tenant in application code. Critical for multi-tenant deployments.
    ///
    /// Index key format: `index:adapters_by_tenant_state:{tenant_id}:{state}:{entity_id}`
    pub const BY_TENANT_STATE: &str = "adapters_by_tenant_state";
}

/// Index definitions for telemetry
pub mod telemetry_indexes {
    /// Index telemetry events by tenant and event type
    /// index value format: "{tenant_id}:{event_type}"
    pub const EVENTS_BY_TENANT_TYPE: &str = "telemetry_events_by_tenant_type";
}

/// Index definitions for replay artifacts
pub mod replay_indexes {
    /// Index replay metadata by tenant
    pub const META_BY_TENANT: &str = "replay_meta_by_tenant";
    /// Index replay metadata by inference id (global lookup)
    pub const META_BY_INFERENCE: &str = "replay_meta_by_inference";
    /// Index replay metadata by record id
    pub const META_BY_ID: &str = "replay_meta_by_id";
    /// Index replay executions by tenant
    pub const EXEC_BY_TENANT: &str = "replay_exec_by_tenant";
    /// Index replay executions by original inference id
    pub const EXEC_BY_INFERENCE: &str = "replay_exec_by_inference";
    /// Index replay executions by execution id
    pub const EXEC_BY_ID: &str = "replay_exec_by_id";
    /// Index replay sessions by tenant
    pub const SESSIONS_BY_TENANT: &str = "replay_sessions_by_tenant";
    /// Index replay sessions by id
    pub const SESSIONS_BY_ID: &str = "replay_sessions_by_id";
}

/// Index definitions for datasets
pub mod dataset_indexes {
    /// Index datasets by tenant
    pub const BY_TENANT: &str = "datasets_by_tenant";
    /// Index datasets by validation status
    pub const BY_VALIDATION_STATUS: &str = "datasets_by_validation_status";
    /// Index datasets by content hash
    pub const BY_HASH: &str = "datasets_by_hash";

    /// Index dataset versions by tenant
    pub const VERSION_BY_TENANT: &str = "dataset_versions_by_tenant";
    /// Index dataset versions by parent dataset
    pub const VERSION_BY_DATASET: &str = "dataset_versions_by_dataset";
    /// Index dataset versions by trust state
    pub const VERSION_BY_TRUST_STATE: &str = "dataset_versions_by_trust_state";
    /// Index dataset versions by validation status
    pub const VERSION_BY_VALIDATION_STATUS: &str = "dataset_versions_by_validation_status";
    /// Index dataset versions by content hash
    pub const VERSION_BY_HASH: &str = "dataset_versions_by_hash";
}

/// Index definitions for adapter versions (git-style versioning)
pub mod adapter_version_indexes {
    /// Index versions by content hash (primary lookup)
    pub const BY_HASH: &str = "adapter_versions_by_hash";

    /// Index versions by adapter name (list all versions of an adapter)
    pub const BY_NAME: &str = "adapter_versions_by_name";

    /// Index versions by parent hash (lineage traversal)
    pub const BY_PARENT: &str = "adapter_versions_by_parent";

    /// Index versions by tenant
    pub const BY_TENANT: &str = "adapter_versions_by_tenant";

    /// Compound index: tenant + adapter name
    pub const BY_TENANT_NAME: &str = "adapter_versions_by_tenant_name";

    /// Index versions by ref name (current, previous, draft, v1, etc.)
    pub const BY_REF: &str = "adapter_versions_by_ref";
}
