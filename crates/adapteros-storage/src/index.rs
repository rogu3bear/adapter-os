//! Secondary index management for key-value storage
//!
//! Provides index definition, management, and querying capabilities
//! for entities stored in the KV backend. Indexes enable efficient
//! lookups by non-primary-key fields (e.g., tenant_id, state, hash).
//!
//! # Design
//!
//! - **Index Definitions**: Configurable key extractors for flexible indexing
//! - **Automatic Maintenance**: Indexes updated on create/update/delete
//! - **Efficient Queries**: Direct index lookups avoid full table scans
//! - **Rebuild Support**: Index recovery and migration capabilities
//!
//! # Architecture
//!
//! Indexes are stored using a prefix scheme in the KV backend:
//! - Entity: `entity:{type}:{id}` → entity_data (JSON)
//! - Index: `{index_name}:{value}` → JSON array of entity IDs
//!
//! # Example
//!
//! ```ignore
//! use adapteros_storage::index::{IndexManager, IndexDef};
//! use std::sync::Arc;
//!
//! // Create index manager
//! let manager = IndexManager::new(backend);
//!
//! // Register an index
//! let index = IndexDef::new(
//!     "idx:adapter:tenant",
//!     "Index adapters by tenant_id",
//!     Arc::new(|data| {
//!         let json: Value = serde_json::from_slice(data)?;
//!         let tenant_id = json["tenant_id"].as_str()
//!             .ok_or(StorageError::SerializationError("missing tenant_id".into()))?;
//!         Ok(vec![tenant_id.to_string()])
//!     })
//! );
//! manager.register_index("adapter", index).await;
//!
//! // Create an entity (indexes updated automatically)
//! let adapter_data = serde_json::json!({
//!     "id": "adapter-1",
//!     "tenant_id": "default",
//!     "name": "My Adapter"
//! });
//! let data_bytes = serde_json::to_vec(&adapter_data)?;
//! manager.on_create("adapter", "adapter-1", &data_bytes).await?;
//!
//! // Query the index
//! let adapter_ids = manager.query_index("idx:adapter:tenant", "default").await?;
//! ```

use crate::error::StorageError;
use crate::kv::backend::KvBackend;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Key extractor function type
///
/// Takes raw entity value bytes and returns a vector of index keys.
/// Multiple keys allow multi-value indexes (e.g., tags, languages).
///
/// # Arguments
/// * `data` - Raw entity data as bytes (typically JSON)
///
/// # Returns
/// * `Ok(keys)` - Vector of index key values
/// * `Err(StorageError)` - Failed to extract keys (invalid data, missing field, etc.)
pub type KeyExtractor = Arc<dyn Fn(&[u8]) -> Result<Vec<String>, StorageError> + Send + Sync>;

/// Index definition for a specific field or computed value
///
/// Defines how to extract index keys from entity data and metadata about the index.
#[derive(Clone)]
pub struct IndexDef {
    /// Unique index name (e.g., "idx:adapter:tenant")
    pub name: String,
    /// Function to extract index keys from entity value
    pub key_extractor: KeyExtractor,
    /// Human-readable description
    pub description: String,
}

impl IndexDef {
    /// Create a new index definition
    ///
    /// # Arguments
    /// * `name` - Unique index name (e.g., "idx:adapter:tenant")
    /// * `description` - Human-readable description of the index
    /// * `key_extractor` - Function to extract index keys from entity data
    ///
    /// # Example
    /// ```ignore
    /// let index = IndexDef::new(
    ///     "idx:adapter:tier",
    ///     "Index adapters by tier",
    ///     Arc::new(|data| {
    ///         let json: Value = serde_json::from_slice(data)?;
    ///         let tier = json["tier"].as_str()
    ///             .ok_or(StorageError::SerializationError("missing tier".into()))?;
    ///         Ok(vec![tier.to_string()])
    ///     })
    /// );
    /// ```
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        key_extractor: KeyExtractor,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            key_extractor,
        }
    }
}

impl std::fmt::Debug for IndexDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexDef")
            .field("name", &self.name)
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}

/// Index manager for maintaining secondary indexes
///
/// Manages index registration, updates, and queries across entity types.
/// Indexes are stored in the KV backend using a prefix scheme:
/// - Entity: `entity:{type}:{id}` → entity_data
/// - Index: `{index_name}:{value}` → JSON array of entity IDs
///
/// # Thread Safety
///
/// IndexManager is thread-safe and can be shared across tasks using Arc.
/// The internal index registry uses RwLock for concurrent access.
pub struct IndexManager {
    /// KV backend for storage
    backend: Arc<dyn KvBackend>,
    /// Registered indexes by entity type
    indexes: Arc<RwLock<HashMap<String, Vec<IndexDef>>>>,
}

impl IndexManager {
    /// Create a new index manager
    ///
    /// # Arguments
    /// * `backend` - KV backend for index storage
    ///
    /// # Example
    /// ```ignore
    /// let backend = Arc::new(RedbBackend::new(&db_path)?);
    /// let manager = IndexManager::new(backend);
    /// ```
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self {
            backend,
            indexes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register an index for an entity type
    ///
    /// Indexes must be registered before entities are created/updated.
    /// Registering a duplicate index name will replace the existing index.
    ///
    /// # Arguments
    /// * `entity_type` - Type of entity (e.g., "adapter", "stack")
    /// * `index` - Index definition
    ///
    /// # Example
    /// ```ignore
    /// manager.register_index("adapter", IndexDef::new(
    ///     "idx:adapter:tenant",
    ///     "Index adapters by tenant_id",
    ///     Arc::new(|data| {
    ///         let json: Value = serde_json::from_slice(data)?;
    ///         let tenant_id = json["tenant_id"].as_str()
    ///             .ok_or(StorageError::SerializationError("missing tenant_id".into()))?;
    ///         Ok(vec![tenant_id.to_string()])
    ///     })
    /// )).await;
    /// ```
    pub async fn register_index(&self, entity_type: &str, index: IndexDef) {
        let mut indexes = self.indexes.write().await;
        let entry = indexes.entry(entity_type.to_string()).or_default();

        // Check for duplicate index names
        if entry.iter().any(|i| i.name == index.name) {
            warn!(
                entity_type = %entity_type,
                index_name = %index.name,
                "Index already registered, replacing"
            );
            entry.retain(|i| i.name != index.name);
        }

        info!(
            entity_type = %entity_type,
            index_name = %index.name,
            description = %index.description,
            "Registered index"
        );
        entry.push(index);
    }

    /// Called after entity creation - updates all indexes
    ///
    /// This method should be called immediately after storing a new entity.
    /// It updates all registered indexes for the entity type.
    ///
    /// # Arguments
    /// * `entity_type` - Type of entity
    /// * `key` - Entity primary key (ID)
    /// * `value` - Entity data bytes
    ///
    /// # Returns
    /// * `Ok(())` - All indexes updated successfully
    /// * `Err(StorageError)` - Index update failed
    ///
    /// # Example
    /// ```ignore
    /// let adapter_data = serde_json::to_vec(&adapter)?;
    /// backend.put(&format!("entity:adapter:{}", id), &adapter_data).await?;
    /// manager.on_create("adapter", &id, &adapter_data).await?;
    /// ```
    pub async fn on_create(
        &self,
        entity_type: &str,
        key: &str,
        value: &[u8],
    ) -> Result<(), StorageError> {
        debug!(entity_type = %entity_type, key = %key, "Indexing new entity");

        let indexes = self.indexes.read().await;
        let entity_indexes = match indexes.get(entity_type) {
            Some(idxs) => idxs,
            None => {
                debug!(entity_type = %entity_type, "No indexes registered for entity type");
                return Ok(());
            }
        };

        // Update each index
        for index in entity_indexes {
            if let Err(e) = self.update_index_internal(index, key, value, None).await {
                error!(
                    entity_type = %entity_type,
                    index_name = %index.name,
                    key = %key,
                    error = %e,
                    "Failed to update index on create"
                );
                return Err(e);
            }
        }

        Ok(())
    }

    /// Called after entity update - updates changed indexes
    ///
    /// This method should be called immediately after updating an entity.
    /// It intelligently updates only changed index entries.
    ///
    /// # Arguments
    /// * `entity_type` - Type of entity
    /// * `key` - Entity primary key (ID)
    /// * `old` - Old entity data bytes
    /// * `new` - New entity data bytes
    ///
    /// # Returns
    /// * `Ok(())` - All indexes updated successfully
    /// * `Err(StorageError)` - Index update failed
    ///
    /// # Example
    /// ```ignore
    /// let old_data = backend.get(&format!("entity:adapter:{}", id)).await?.unwrap();
    /// let new_data = serde_json::to_vec(&updated_adapter)?;
    /// backend.put(&format!("entity:adapter:{}", id), &new_data).await?;
    /// manager.on_update("adapter", &id, &old_data, &new_data).await?;
    /// ```
    pub async fn on_update(
        &self,
        entity_type: &str,
        key: &str,
        old: &[u8],
        new: &[u8],
    ) -> Result<(), StorageError> {
        debug!(entity_type = %entity_type, key = %key, "Updating entity indexes");

        let indexes = self.indexes.read().await;
        let entity_indexes = match indexes.get(entity_type) {
            Some(idxs) => idxs,
            None => {
                debug!(entity_type = %entity_type, "No indexes registered for entity type");
                return Ok(());
            }
        };

        // Update each index
        for index in entity_indexes {
            if let Err(e) = self.update_index_internal(index, key, new, Some(old)).await {
                error!(
                    entity_type = %entity_type,
                    index_name = %index.name,
                    key = %key,
                    error = %e,
                    "Failed to update index on update"
                );
                return Err(e);
            }
        }

        Ok(())
    }

    /// Called before entity deletion - removes from indexes
    ///
    /// This method should be called before deleting an entity.
    /// It removes the entity from all registered indexes.
    ///
    /// # Arguments
    /// * `entity_type` - Type of entity
    /// * `key` - Entity primary key (ID)
    /// * `value` - Entity data bytes
    ///
    /// # Returns
    /// * `Ok(())` - Entity removed from all indexes
    /// * `Err(StorageError)` - Index update failed
    ///
    /// # Example
    /// ```ignore
    /// let data = backend.get(&format!("entity:adapter:{}", id)).await?.unwrap();
    /// manager.on_delete("adapter", &id, &data).await?;
    /// backend.delete(&format!("entity:adapter:{}", id)).await?;
    /// ```
    pub async fn on_delete(
        &self,
        entity_type: &str,
        key: &str,
        value: &[u8],
    ) -> Result<(), StorageError> {
        debug!(entity_type = %entity_type, key = %key, "Removing entity from indexes");

        let indexes = self.indexes.read().await;
        let entity_indexes = match indexes.get(entity_type) {
            Some(idxs) => idxs,
            None => {
                debug!(entity_type = %entity_type, "No indexes registered for entity type");
                return Ok(());
            }
        };

        // Remove from each index
        for index in entity_indexes {
            if let Err(e) = self.remove_from_index_internal(index, key, value).await {
                error!(
                    entity_type = %entity_type,
                    index_name = %index.name,
                    key = %key,
                    error = %e,
                    "Failed to remove from index on delete"
                );
                return Err(e);
            }
        }

        Ok(())
    }

    /// Query by index
    ///
    /// Returns a list of entity IDs that match the index value.
    ///
    /// # Arguments
    /// * `index_name` - Name of the index (e.g., "idx:adapter:tenant")
    /// * `value` - Index value to query (e.g., "tenant-123")
    ///
    /// # Returns
    /// * `Ok(ids)` - Vector of entity IDs matching the index value
    /// * `Err(StorageError)` - Query failed
    ///
    /// # Example
    /// ```ignore
    /// // Get all adapters for a tenant
    /// let adapter_ids = manager.query_index("idx:adapter:tenant", "default").await?;
    ///
    /// // Get all adapters in warm tier for a tenant
    /// let warm_adapters = manager.query_index("idx:adapter:tier", "default:warm").await?;
    /// ```
    pub async fn query_index(
        &self,
        index_name: &str,
        index_value: &str,
    ) -> Result<Vec<String>, StorageError> {
        let index_key = format!("{}:{}", index_name, index_value);

        debug!(index_name = %index_name, value = %index_value, "Querying index");

        match self.backend.get(&index_key).await? {
            Some(data) => {
                let ids: Vec<String> = serde_json::from_slice(&data).map_err(|e| {
                    StorageError::SerializationError(format!(
                        "Failed to deserialize index data: {}",
                        e
                    ))
                })?;
                debug!(index_name = %index_name, value = %index_value, count = ids.len(), "Index query result");
                Ok(ids)
            }
            None => {
                debug!(index_name = %index_name, value = %index_value, "Index key not found");
                Ok(Vec::new())
            }
        }
    }

    /// Update an index for a specific entity
    ///
    /// If old_value is provided, removes old index entries before adding new ones.
    async fn update_index_internal(
        &self,
        index: &IndexDef,
        entity_id: &str,
        new_value: &[u8],
        old_value: Option<&[u8]>,
    ) -> Result<(), StorageError> {
        // Extract new index keys
        let new_keys = (index.key_extractor)(new_value)?;

        // Extract old index keys if updating
        let old_keys = if let Some(old) = old_value {
            match (index.key_extractor)(old) {
                Ok(keys) => Some(keys),
                Err(e) => {
                    warn!(
                        index_name = %index.name,
                        error = %e,
                        "Failed to extract old index keys, skipping removal"
                    );
                    None
                }
            }
        } else {
            None
        };

        // Remove from old index entries (if updating and keys changed)
        if let Some(old_keys) = old_keys {
            for old_key in old_keys {
                if !new_keys.contains(&old_key) {
                    self.remove_entity_from_index_key(&index.name, &old_key, entity_id)
                        .await?;
                }
            }
        }

        // Add to new index entries
        for new_key in new_keys {
            self.add_entity_to_index_key(&index.name, &new_key, entity_id)
                .await?;
        }

        Ok(())
    }

    /// Remove entity from index
    async fn remove_from_index_internal(
        &self,
        index: &IndexDef,
        entity_id: &str,
        value: &[u8],
    ) -> Result<(), StorageError> {
        let keys = (index.key_extractor)(value)?;

        for key in keys {
            self.remove_entity_from_index_key(&index.name, &key, entity_id)
                .await?;
        }

        Ok(())
    }

    /// Add entity ID to an index key's value set
    async fn add_entity_to_index_key(
        &self,
        index_name: &str,
        index_value: &str,
        entity_id: &str,
    ) -> Result<(), StorageError> {
        let index_key = format!("{}:{}", index_name, index_value);

        // Get current index value set
        let mut ids: Vec<String> = match self.backend.get(&index_key).await? {
            Some(data) => serde_json::from_slice(&data).map_err(|e| {
                StorageError::SerializationError(format!("Failed to deserialize index data: {}", e))
            })?,
            None => Vec::new(),
        };

        // Add entity ID if not already present
        if !ids.contains(&entity_id.to_string()) {
            ids.push(entity_id.to_string());
            let data = serde_json::to_vec(&ids).map_err(|e| {
                StorageError::SerializationError(format!("Failed to serialize index data: {}", e))
            })?;
            self.backend.set(&index_key, data).await?;
        }

        Ok(())
    }

    /// Remove entity ID from an index key's value set
    async fn remove_entity_from_index_key(
        &self,
        index_name: &str,
        index_value: &str,
        entity_id: &str,
    ) -> Result<(), StorageError> {
        let index_key = format!("{}:{}", index_name, index_value);

        // Get current index value set
        let mut ids: Vec<String> = match self.backend.get(&index_key).await? {
            Some(data) => serde_json::from_slice(&data).map_err(|e| {
                StorageError::SerializationError(format!("Failed to deserialize index data: {}", e))
            })?,
            None => return Ok(()), // Index key doesn't exist, nothing to remove
        };

        // Remove entity ID
        ids.retain(|id| id != entity_id);

        // Update or delete index key
        if ids.is_empty() {
            self.backend.delete(&index_key).await?;
        } else {
            let data = serde_json::to_vec(&ids).map_err(|e| {
                StorageError::SerializationError(format!("Failed to serialize index data: {}", e))
            })?;
            self.backend.set(&index_key, data).await?;
        }

        Ok(())
    }

    /// Get all registered indexes for an entity type
    ///
    /// # Arguments
    /// * `entity_type` - Type of entity
    ///
    /// # Returns
    /// Vector of index definitions for the entity type
    pub async fn list_indexes(&self, entity_type: &str) -> Vec<IndexDef> {
        let indexes = self.indexes.read().await;
        indexes
            .get(entity_type)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Get all registered entity types
    ///
    /// # Returns
    /// Vector of entity type names that have registered indexes
    pub async fn list_entity_types(&self) -> Vec<String> {
        let indexes = self.indexes.read().await;
        indexes.keys().cloned().collect()
    }

    /// Rebuild an index from scratch
    ///
    /// Useful for index migrations or corruption recovery.
    /// Scans all entities of the given type and rebuilds the index.
    ///
    /// # Arguments
    /// * `entity_type` - Type of entity (e.g., "adapter")
    /// * `index_name` - Name of the index to rebuild
    ///
    /// # Returns
    /// * `Ok(())` - Index rebuilt successfully
    /// * `Err(StorageError)` - Rebuild failed
    ///
    /// # Example
    /// ```ignore
    /// // Rebuild the tenant index for adapters
    /// manager.rebuild_index("adapter", "idx:adapter:tenant").await?;
    /// ```
    pub async fn rebuild_index(
        &self,
        entity_type: &str,
        index_name: &str,
    ) -> Result<(), StorageError> {
        info!(entity_type = %entity_type, index_name = %index_name, "Rebuilding index");

        // Find the index definition
        let indexes = self.indexes.read().await;
        let entity_indexes = indexes.get(entity_type).ok_or_else(|| {
            StorageError::NotFound(format!(
                "No indexes registered for entity type: {}",
                entity_type
            ))
        })?;

        let index = entity_indexes
            .iter()
            .find(|i| i.name == index_name)
            .ok_or_else(|| StorageError::NotFound(format!("Index not found: {}", index_name)))?;

        // Clear existing index entries
        let prefix = format!("{}:", index_name);
        let index_keys = self.backend.scan_prefix(&prefix).await?;

        for key in index_keys {
            self.backend.delete(&key).await?;
        }

        // Scan all entities of this type
        let entity_prefix = format!("entity:{}:", entity_type);
        let entity_keys = self.backend.scan_prefix(&entity_prefix).await?;

        // Rebuild index for each entity
        for entity_key in entity_keys {
            // Get entity data
            let data = match self.backend.get(&entity_key).await? {
                Some(d) => d,
                None => continue, // Entity was deleted between scan and get
            };

            // Extract entity ID from key (entity:{type}:{id})
            let entity_id = entity_key
                .strip_prefix(&entity_prefix)
                .unwrap_or(&entity_key);

            if let Err(e) = self
                .update_index_internal(index, entity_id, &data, None)
                .await
            {
                error!(
                    entity_type = %entity_type,
                    index_name = %index_name,
                    entity_id = %entity_id,
                    error = %e,
                    "Failed to rebuild index entry"
                );
            }
        }

        info!(entity_type = %entity_type, index_name = %index_name, "Index rebuild complete");
        Ok(())
    }
}

/// Predefined adapter indexes
///
/// These indexes are commonly used for adapter queries and should be
/// registered by default when using IndexManager with adapters.
pub mod adapter_indexes {
    use super::*;

    /// Create index by tenant_id
    ///
    /// Index key: `idx:adapter:tenant:{tenant_id}`
    ///
    /// # Example
    /// ```ignore
    /// manager.register_index("adapter", adapter_indexes::index_by_tenant()).await;
    /// let adapters = manager.query_index("idx:adapter:tenant", "default").await?;
    /// ```
    pub fn index_by_tenant() -> IndexDef {
        IndexDef::new(
            "idx:adapter:tenant",
            "Index adapters by tenant_id",
            Arc::new(|data| {
                let json: Value = serde_json::from_slice(data).map_err(|e| {
                    StorageError::SerializationError(format!("Failed to parse adapter JSON: {}", e))
                })?;
                let tenant_id =
                    json.get("tenant_id")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            StorageError::SerializationError("tenant_id field missing".into())
                        })?;
                Ok(vec![tenant_id.to_string()])
            }),
        )
    }

    /// Create index by state and tenant
    ///
    /// Index key: `idx:adapter:state:{tenant_id}:{lifecycle_state}`
    ///
    /// # Example
    /// ```ignore
    /// manager.register_index("adapter", adapter_indexes::index_by_state()).await;
    /// let active_adapters = manager.query_index("idx:adapter:state", "default:active").await?;
    /// ```
    pub fn index_by_state() -> IndexDef {
        IndexDef::new(
            "idx:adapter:state",
            "Index adapters by tenant_id and lifecycle_state",
            Arc::new(|data| {
                let json: Value = serde_json::from_slice(data).map_err(|e| {
                    StorageError::SerializationError(format!("Failed to parse adapter JSON: {}", e))
                })?;
                let tenant_id =
                    json.get("tenant_id")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            StorageError::SerializationError("tenant_id field missing".into())
                        })?;
                let state = json
                    .get("lifecycle_state")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        StorageError::SerializationError("lifecycle_state field missing".into())
                    })?;
                Ok(vec![format!("{}:{}", tenant_id, state)])
            }),
        )
    }

    /// Create index by hash_b3
    ///
    /// Index key: `idx:adapter:hash:{hash_b3}`
    ///
    /// # Example
    /// ```ignore
    /// manager.register_index("adapter", adapter_indexes::index_by_hash()).await;
    /// let adapters = manager.query_index("idx:adapter:hash", "abc123...").await?;
    /// ```
    pub fn index_by_hash() -> IndexDef {
        IndexDef::new(
            "idx:adapter:hash",
            "Index adapters by BLAKE3 hash",
            Arc::new(|data| {
                let json: Value = serde_json::from_slice(data).map_err(|e| {
                    StorageError::SerializationError(format!("Failed to parse adapter JSON: {}", e))
                })?;
                let hash = json
                    .get("hash_b3")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        StorageError::SerializationError("hash_b3 field missing".into())
                    })?;
                Ok(vec![hash.to_string()])
            }),
        )
    }

    /// Create index by tier and tenant
    ///
    /// Index key: `idx:adapter:tier:{tenant_id}:{tier}`
    ///
    /// # Example
    /// ```ignore
    /// manager.register_index("adapter", adapter_indexes::index_by_tier()).await;
    /// let warm_adapters = manager.query_index("idx:adapter:tier", "default:warm").await?;
    /// ```
    pub fn index_by_tier() -> IndexDef {
        IndexDef::new(
            "idx:adapter:tier",
            "Index adapters by tenant_id and tier",
            Arc::new(|data| {
                let json: Value = serde_json::from_slice(data).map_err(|e| {
                    StorageError::SerializationError(format!("Failed to parse adapter JSON: {}", e))
                })?;
                let tenant_id =
                    json.get("tenant_id")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            StorageError::SerializationError("tenant_id field missing".into())
                        })?;
                let tier = json
                    .get("tier")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| StorageError::SerializationError("tier field missing".into()))?;
                Ok(vec![format!("{}:{}", tenant_id, tier)])
            }),
        )
    }

    /// Register all predefined adapter indexes
    ///
    /// Convenience function to register all standard adapter indexes.
    ///
    /// # Example
    /// ```ignore
    /// adapter_indexes::register_all(&manager).await;
    /// ```
    pub async fn register_all(manager: &IndexManager) {
        manager.register_index("adapter", index_by_tenant()).await;
        manager.register_index("adapter", index_by_state()).await;
        manager.register_index("adapter", index_by_hash()).await;
        manager.register_index("adapter", index_by_tier()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap as StdHashMap;
    use tokio::sync::RwLock as TokioRwLock;

    /// In-memory KV backend for testing
    struct MemoryBackend {
        data: Arc<TokioRwLock<StdHashMap<String, Vec<u8>>>>,
    }

    impl MemoryBackend {
        fn new() -> Self {
            Self {
                data: Arc::new(TokioRwLock::new(StdHashMap::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl KvBackend for MemoryBackend {
        async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
            let data = self.data.read().await;
            Ok(data.get(key).cloned())
        }

        async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), StorageError> {
            let mut data = self.data.write().await;
            data.insert(key.to_string(), value);
            Ok(())
        }

        async fn delete(&self, key: &str) -> Result<bool, StorageError> {
            let mut data = self.data.write().await;
            Ok(data.remove(key).is_some())
        }

        async fn exists(&self, key: &str) -> Result<bool, StorageError> {
            let data = self.data.read().await;
            Ok(data.contains_key(key))
        }

        async fn scan_prefix(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
            let data = self.data.read().await;
            Ok(data
                .keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect())
        }

        async fn batch_get(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>, StorageError> {
            let data = self.data.read().await;
            Ok(keys.iter().map(|k| data.get(k).cloned()).collect())
        }

        async fn batch_set(&self, pairs: Vec<(String, Vec<u8>)>) -> Result<(), StorageError> {
            let mut data = self.data.write().await;
            for (key, value) in pairs {
                data.insert(key, value);
            }
            Ok(())
        }

        async fn batch_delete(&self, keys: &[String]) -> Result<usize, StorageError> {
            let mut data = self.data.write().await;
            let mut count = 0;
            for key in keys {
                if data.remove(key).is_some() {
                    count += 1;
                }
            }
            Ok(count)
        }

        async fn set_add(&self, key: &str, member: &str) -> Result<(), StorageError> {
            let composite_key = format!("set:{}::{}", key, member);
            let mut data = self.data.write().await;
            data.insert(composite_key, vec![1]);
            Ok(())
        }

        async fn set_remove(&self, key: &str, member: &str) -> Result<(), StorageError> {
            let composite_key = format!("set:{}::{}", key, member);
            let mut data = self.data.write().await;
            data.remove(&composite_key);
            Ok(())
        }

        async fn set_members(&self, key: &str) -> Result<Vec<String>, StorageError> {
            let prefix = format!("set:{}::", key);
            let data = self.data.read().await;
            Ok(data
                .keys()
                .filter(|k| k.starts_with(&prefix))
                .map(|k| k[prefix.len()..].to_string())
                .collect())
        }

        async fn set_is_member(&self, key: &str, member: &str) -> Result<bool, StorageError> {
            let composite_key = format!("set:{}::{}", key, member);
            let data = self.data.read().await;
            Ok(data.contains_key(&composite_key))
        }
    }

    #[tokio::test]
    async fn test_index_create_and_query() {
        let backend = Arc::new(MemoryBackend::new());
        let manager = IndexManager::new(backend);

        // Register a simple index
        let index = IndexDef::new(
            "idx:test:tenant",
            "Test index by tenant",
            Arc::new(|data| {
                let json: Value = serde_json::from_slice(data)
                    .map_err(|e| StorageError::SerializationError(e.to_string()))?;
                let tenant = json["tenant_id"]
                    .as_str()
                    .ok_or_else(|| StorageError::SerializationError("missing tenant_id".into()))?;
                Ok(vec![tenant.to_string()])
            }),
        );
        manager.register_index("test", index).await;

        // Create an entity
        let entity_data = serde_json::json!({
            "id": "test-1",
            "tenant_id": "tenant-a",
            "name": "Test Entity"
        });
        let entity_bytes = serde_json::to_vec(&entity_data).unwrap();

        manager
            .on_create("test", "test-1", &entity_bytes)
            .await
            .unwrap();

        // Query the index
        let results = manager
            .query_index("idx:test:tenant", "tenant-a")
            .await
            .unwrap();
        assert_eq!(results, vec!["test-1"]);
    }

    #[tokio::test]
    async fn test_index_update() {
        let backend = Arc::new(MemoryBackend::new());
        let manager = IndexManager::new(backend);

        let index = IndexDef::new(
            "idx:test:tenant",
            "Test index by tenant",
            Arc::new(|data| {
                let json: Value = serde_json::from_slice(data)
                    .map_err(|e| StorageError::SerializationError(e.to_string()))?;
                let tenant = json["tenant_id"]
                    .as_str()
                    .ok_or_else(|| StorageError::SerializationError("missing tenant_id".into()))?;
                Ok(vec![tenant.to_string()])
            }),
        );
        manager.register_index("test", index).await;

        // Create entity
        let old_data = serde_json::json!({"tenant_id": "tenant-a"});
        let old_bytes = serde_json::to_vec(&old_data).unwrap();
        manager
            .on_create("test", "test-1", &old_bytes)
            .await
            .unwrap();

        // Update entity to different tenant
        let new_data = serde_json::json!({"tenant_id": "tenant-b"});
        let new_bytes = serde_json::to_vec(&new_data).unwrap();
        manager
            .on_update("test", "test-1", &old_bytes, &new_bytes)
            .await
            .unwrap();

        // Old index should be empty
        let old_results = manager
            .query_index("idx:test:tenant", "tenant-a")
            .await
            .unwrap();
        assert!(old_results.is_empty());

        // New index should contain entity
        let new_results = manager
            .query_index("idx:test:tenant", "tenant-b")
            .await
            .unwrap();
        assert_eq!(new_results, vec!["test-1"]);
    }

    #[tokio::test]
    async fn test_index_delete() {
        let backend = Arc::new(MemoryBackend::new());
        let manager = IndexManager::new(backend);

        let index = IndexDef::new(
            "idx:test:tenant",
            "Test index by tenant",
            Arc::new(|data| {
                let json: Value = serde_json::from_slice(data)
                    .map_err(|e| StorageError::SerializationError(e.to_string()))?;
                let tenant = json["tenant_id"]
                    .as_str()
                    .ok_or_else(|| StorageError::SerializationError("missing tenant_id".into()))?;
                Ok(vec![tenant.to_string()])
            }),
        );
        manager.register_index("test", index).await;

        // Create entity
        let entity_data = serde_json::json!({"tenant_id": "tenant-a"});
        let entity_bytes = serde_json::to_vec(&entity_data).unwrap();
        manager
            .on_create("test", "test-1", &entity_bytes)
            .await
            .unwrap();

        // Delete entity
        manager
            .on_delete("test", "test-1", &entity_bytes)
            .await
            .unwrap();

        // Index should be empty
        let results = manager
            .query_index("idx:test:tenant", "tenant-a")
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_adapter_indexes() {
        let backend = Arc::new(MemoryBackend::new());
        let manager = IndexManager::new(backend);

        // Register adapter indexes
        adapter_indexes::register_all(&manager).await;

        // Create adapter entity
        let adapter_data = serde_json::json!({
            "id": "adapter-1",
            "tenant_id": "default",
            "hash_b3": "abc123",
            "tier": "warm",
            "lifecycle_state": "active"
        });
        let adapter_bytes = serde_json::to_vec(&adapter_data).unwrap();

        manager
            .on_create("adapter", "adapter-1", &adapter_bytes)
            .await
            .unwrap();

        // Query by tenant
        let by_tenant = manager
            .query_index("idx:adapter:tenant", "default")
            .await
            .unwrap();
        assert_eq!(by_tenant, vec!["adapter-1"]);

        // Query by hash
        let by_hash = manager
            .query_index("idx:adapter:hash", "abc123")
            .await
            .unwrap();
        assert_eq!(by_hash, vec!["adapter-1"]);

        // Query by state
        let by_state = manager
            .query_index("idx:adapter:state", "default:active")
            .await
            .unwrap();
        assert_eq!(by_state, vec!["adapter-1"]);

        // Query by tier
        let by_tier = manager
            .query_index("idx:adapter:tier", "default:warm")
            .await
            .unwrap();
        assert_eq!(by_tier, vec!["adapter-1"]);
    }
}
