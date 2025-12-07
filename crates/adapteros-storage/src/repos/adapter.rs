//! Adapter repository
//!
//! Replaces SQL queries with KV-based operations for adapter management.
//! Implements all CRUD operations, queries, and lineage traversal.

use crate::error::StorageError;
use crate::kv::backend::KvBackend;
use crate::kv::indexing::{adapter_indexes, IndexManager};
use crate::models::AdapterKv;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use tracing::{error, info, warn};

/// Paginated query result
#[derive(Debug, Clone)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

/// Adapter repository for KV storage
pub struct AdapterRepository {
    backend: Arc<dyn KvBackend>,
    index_manager: Arc<IndexManager>,
}

impl AdapterRepository {
    /// Create a new adapter repository
    pub fn new(backend: Arc<dyn KvBackend>, index_manager: Arc<IndexManager>) -> Self {
        Self {
            backend,
            index_manager,
        }
    }

    // ============================================================================
    // CRUD Operations
    // ============================================================================

    /// Create a new adapter
    ///
    /// This operation:
    /// 1. Serializes the adapter to bytes
    /// 2. Stores it with primary key
    /// 3. Updates all secondary indexes
    pub async fn create(&self, adapter: AdapterKv) -> Result<String, StorageError> {
        let id = adapter.id.clone();
        let key = adapter.primary_key();
        let legacy_key = adapter.legacy_primary_key();

        // Check if adapter already exists
        let exists_primary = self.backend.exists(&key).await?;
        let exists_legacy = if legacy_key != key {
            self.backend.exists(&legacy_key).await?
        } else {
            false
        };
        if exists_primary || exists_legacy {
            return Err(StorageError::ConflictError(format!(
                "Adapter already exists: {}",
                id
            )));
        }

        // Serialize adapter
        let value = bincode::serialize(&adapter)?;

        // Store adapter
        self.backend.set(&key, value).await?;

        // Update all indexes
        self.update_indexes(&adapter, None).await?;

        info!(adapter_id = %id, "Adapter created");
        Ok(id)
    }

    /// Get an adapter by external adapter_id
    ///
    /// Uses the adapter_id index to find the internal UUID, then fetches the adapter.
    pub async fn get(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<Option<AdapterKv>, StorageError> {
        // Look up internal ID via adapter_id index
        let ids = self
            .index_manager
            .query_index(adapter_indexes::BY_ADAPTER_ID, adapter_id)
            .await?;

        // Build candidate keys: prefer index-resolved id if present, then canonical adapter_id key
        let mut candidate_keys: Vec<String> = Vec::new();
        if let Some(first) = ids.first() {
            candidate_keys.push(format!("adapter:{}", first));
        }
        let adapter_key = format!("adapter:{}", adapter_id);
        if !candidate_keys.contains(&adapter_key) {
            candidate_keys.push(adapter_key);
        }

        // Attempt candidates in order
        let mut bytes_opt = None;
        for key in candidate_keys {
            if let Some(bytes) = self.backend.get(&key).await? {
                bytes_opt = Some(bytes);
                break;
            }
        }

        let bytes = match bytes_opt {
            Some(b) => b,
            None => return Ok(None),
        };

        let adapter: AdapterKv = bincode::deserialize(&bytes)?;

        // Verify tenant ownership
        if adapter.tenant_id != tenant_id {
            return Ok(None);
        }

        Ok(Some(adapter))
    }

    /// Update an existing adapter
    pub async fn update(&self, adapter: AdapterKv) -> Result<(), StorageError> {
        let key = adapter.primary_key();
        let legacy_key = adapter.legacy_primary_key();

        // Get old adapter for index updates (support legacy UUID-keyed entries)
        let (stored_key, old_bytes) = match self.backend.get(&key).await? {
            Some(bytes) => (key.clone(), bytes),
            None => {
                if legacy_key != key {
                    match self.backend.get(&legacy_key).await? {
                        Some(bytes) => (legacy_key.clone(), bytes),
                        None => return Err(StorageError::NotFound(adapter.id.clone())),
                    }
                } else {
                    return Err(StorageError::NotFound(adapter.id.clone()));
                }
            }
        };

        let old_adapter: AdapterKv = bincode::deserialize(&old_bytes)?;

        // Serialize new adapter
        let new_value = bincode::serialize(&adapter)?;

        // Update in storage
        self.backend.set(&key, new_value).await?;

        // If we updated a legacy key, remove the old entry to avoid drift
        if stored_key != key {
            let _ = self.backend.delete(&stored_key).await?;
        }

        // Update indexes (comparing old vs new values)
        self.update_indexes(&adapter, Some(&old_adapter)).await?;

        info!(adapter_id = %adapter.id, "Adapter updated");
        Ok(())
    }

    /// Delete an adapter
    pub async fn delete(&self, tenant_id: &str, adapter_id: &str) -> Result<bool, StorageError> {
        // Get adapter to verify tenant and clean up indexes
        let adapter = match self.get(tenant_id, adapter_id).await? {
            Some(a) => a,
            None => return Ok(false),
        };

        let key = adapter.primary_key();
        let legacy_key = adapter.legacy_primary_key();

        // Delete from storage
        let mut deleted = self.backend.delete(&key).await?;
        // Also remove legacy key if it exists to prevent drift
        if legacy_key != key {
            let legacy_deleted = self.backend.delete(&legacy_key).await?;
            deleted = deleted || legacy_deleted;
        }

        if deleted {
            // Remove from all indexes
            self.remove_from_indexes(&adapter).await?;
            info!(adapter_id = %adapter_id, "Adapter deleted");
        }

        Ok(deleted)
    }

    // ============================================================================
    // Query Operations
    // ============================================================================

    /// List all adapters for a tenant
    pub async fn list_by_tenant(&self, tenant_id: &str) -> Result<Vec<AdapterKv>, StorageError> {
        let adapter_ids = self
            .index_manager
            .query_index(adapter_indexes::BY_TENANT, tenant_id)
            .await?;

        self.load_adapters(&adapter_ids).await
    }

    /// List adapters by state
    pub async fn list_by_state(
        &self,
        tenant_id: &str,
        state: &str,
    ) -> Result<Vec<AdapterKv>, StorageError> {
        let adapter_ids = self
            .index_manager
            .query_index(adapter_indexes::BY_STATE, state)
            .await?;

        // Filter by tenant
        let adapters = self.load_adapters(&adapter_ids).await?;
        Ok(adapters
            .into_iter()
            .filter(|a| a.tenant_id == tenant_id)
            .collect())
    }

    /// List adapters by tier
    pub async fn list_by_tier(
        &self,
        tenant_id: &str,
        tier: &str,
    ) -> Result<Vec<AdapterKv>, StorageError> {
        let adapter_ids = self
            .index_manager
            .query_index(adapter_indexes::BY_TIER, tier)
            .await?;

        // Filter by tenant
        let adapters = self.load_adapters(&adapter_ids).await?;
        Ok(adapters
            .into_iter()
            .filter(|a| a.tenant_id == tenant_id)
            .collect())
    }

    /// Find adapter by content hash
    pub async fn find_by_hash(&self, hash: &str) -> Result<Option<AdapterKv>, StorageError> {
        let adapter_ids = self
            .index_manager
            .query_index(adapter_indexes::BY_HASH, hash)
            .await?;

        if adapter_ids.is_empty() {
            return Ok(None);
        }

        // Should only be one adapter per hash due to UNIQUE constraint
        let adapters = self.load_adapters(&adapter_ids).await?;
        Ok(adapters.into_iter().next())
    }

    // ============================================================================
    // Lineage Queries (Replaces SQL Recursive CTEs)
    // ============================================================================

    /// Get all ancestors of an adapter
    ///
    /// This replaces the SQL recursive CTE for walking up the parent chain.
    /// Uses iterative breadth-first traversal to avoid stack overflow.
    pub async fn get_ancestors(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<Vec<AdapterKv>, StorageError> {
        let mut ancestors = Vec::new();
        let mut visited = HashSet::new();
        let mut current_id = adapter_id.to_string();

        // Walk up the parent chain
        loop {
            // Prevent infinite loops from circular references
            if visited.contains(&current_id) {
                warn!(
                    adapter_id = %adapter_id,
                    current_id = %current_id,
                    "Circular reference detected in adapter lineage"
                );
                break;
            }

            visited.insert(current_id.clone());

            // Get current adapter
            let adapter = match self.get(tenant_id, &current_id).await? {
                Some(a) => a,
                None => break,
            };

            // Check if there's a parent
            match adapter.parent_id.clone() {
                Some(parent_id) => {
                    ancestors.push(adapter);
                    current_id = parent_id;
                }
                None => {
                    // Reached root of lineage
                    ancestors.push(adapter);
                    break;
                }
            }

            // Safety limit to prevent runaway queries
            if ancestors.len() > 100 {
                error!(
                    adapter_id = %adapter_id,
                    "Ancestor chain exceeds safety limit of 100"
                );
                break;
            }
        }

        Ok(ancestors)
    }

    /// Get all descendants of an adapter
    ///
    /// This replaces the SQL recursive CTE for walking down the parent_id references.
    /// Uses iterative breadth-first search to find all children, grandchildren, etc.
    pub async fn get_descendants(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<Vec<AdapterKv>, StorageError> {
        let mut descendants = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        // First, resolve the external adapter_id to internal UUID
        // The BY_PARENT index stores parent_id as internal UUID, not external adapter_id
        let start_adapter = match self.get(tenant_id, adapter_id).await? {
            Some(a) => a,
            None => return Ok(descendants), // Adapter not found
        };
        let start_internal_id = start_adapter.id.clone();

        queue.push_back(start_internal_id.clone());
        visited.insert(start_internal_id);

        // Breadth-first search for all descendants
        while let Some(current_id) = queue.pop_front() {
            // Find all adapters that have current_id as their parent (using internal UUID)
            let children_ids = self
                .index_manager
                .query_index(adapter_indexes::BY_PARENT, &current_id)
                .await?;

            for child_id in children_ids {
                // Prevent infinite loops
                if visited.contains(&child_id) {
                    warn!(
                        adapter_id = %adapter_id,
                        child_id = %child_id,
                        "Circular reference detected in adapter lineage"
                    );
                    continue;
                }

                visited.insert(child_id.clone());

                // Load child adapter by internal UUID directly
                let key = format!("adapter:{}", child_id);
                if let Some(bytes) = self.backend.get(&key).await? {
                    match bincode::deserialize::<AdapterKv>(&bytes) {
                        Ok(child) => {
                            // Verify tenant ownership
                            if child.tenant_id == tenant_id {
                                let child_internal_id = child.id.clone();
                                descendants.push(child);
                                queue.push_back(child_internal_id);
                            }
                        }
                        Err(e) => {
                            error!(child_id = %child_id, error = %e, "Failed to deserialize child adapter");
                        }
                    }
                }

                // Safety limit
                if descendants.len() > 1000 {
                    error!(
                        adapter_id = %adapter_id,
                        "Descendant tree exceeds safety limit of 1000"
                    );
                    return Ok(descendants);
                }
            }
        }

        Ok(descendants)
    }

    // ============================================================================
    // Paginated Queries
    // ============================================================================

    /// List adapters with pagination
    ///
    /// # Arguments
    /// * `tenant_id` - Tenant to query
    /// * `cursor` - Optional cursor for pagination (adapter ID to start after)
    /// * `limit` - Maximum number of results to return
    pub async fn list_paginated(
        &self,
        tenant_id: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<PaginatedResult<AdapterKv>, StorageError> {
        let mut all_ids = self
            .index_manager
            .query_index(adapter_indexes::BY_TENANT, tenant_id)
            .await?;

        // Sort IDs for consistent pagination
        all_ids.sort();

        // Find cursor position
        let start_index = if let Some(cursor_id) = cursor {
            all_ids
                .iter()
                .position(|id| id == cursor_id)
                .map(|pos| pos + 1)
                .unwrap_or(0)
        } else {
            0
        };

        // Get page of IDs
        let page_ids: Vec<String> = all_ids
            .iter()
            .skip(start_index)
            .take(limit)
            .cloned()
            .collect();

        let has_more = start_index + page_ids.len() < all_ids.len();
        let next_cursor = if has_more {
            page_ids.last().cloned()
        } else {
            None
        };

        // Load adapters
        let items = self.load_adapters(&page_ids).await?;

        Ok(PaginatedResult {
            items,
            next_cursor,
            has_more,
        })
    }

    // ============================================================================
    // Internal Helper Methods
    // ============================================================================

    /// Load multiple adapters by ID
    async fn load_adapters(&self, adapter_ids: &[String]) -> Result<Vec<AdapterKv>, StorageError> {
        let keys: Vec<String> = adapter_ids
            .iter()
            .map(|id| format!("adapter:{}", id))
            .collect();

        let values = self.backend.batch_get(&keys).await?;

        let mut adapters = Vec::new();
        for (id, value_opt) in adapter_ids.iter().zip(values.iter()) {
            if let Some(bytes) = value_opt {
                match bincode::deserialize::<AdapterKv>(bytes) {
                    Ok(adapter) => adapters.push(adapter),
                    Err(e) => {
                        error!(adapter_id = %id, error = %e, "Failed to deserialize adapter");
                    }
                }
            }
        }

        Ok(adapters)
    }

    /// Update all secondary indexes for an adapter
    async fn update_indexes(
        &self,
        adapter: &AdapterKv,
        old_adapter: Option<&AdapterKv>,
    ) -> Result<(), StorageError> {
        let entity_id = adapter.key_id().to_string();
        let old_entity_id = old_adapter.map(|a| a.key_id().to_string());
        let id_changed = old_entity_id
            .as_ref()
            .map(|old| old != &entity_id)
            .unwrap_or(false);

        // If the storage key changed (legacy UUID -> adapter_id), drop old index entries first.
        if let Some(old) = old_adapter {
            if id_changed {
                self.remove_from_indexes(old).await?;
            }
        }

        let should_add_new = old_adapter.is_none() || id_changed;

        // Tenant index
        if should_add_new {
            self.index_manager
                .add_to_index(adapter_indexes::BY_TENANT, &adapter.tenant_id, &entity_id)
                .await?;
        }

        // State index
        let old_state = old_adapter.map(|a| a.current_state.as_str());
        self.index_manager
            .update_index(
                adapter_indexes::BY_STATE,
                old_state,
                &adapter.current_state,
                &entity_id,
            )
            .await?;

        // Tier index
        let old_tier = old_adapter.map(|a| a.tier.as_str());
        self.index_manager
            .update_index(
                adapter_indexes::BY_TIER,
                old_tier,
                &adapter.tier,
                &entity_id,
            )
            .await?;

        // Hash index
        if should_add_new {
            self.index_manager
                .add_to_index(adapter_indexes::BY_HASH, &adapter.hash_b3, &entity_id)
                .await?;
        }

        // Adapter ID index (external adapter_id -> storage key mapping)
        if let Some(adapter_id) = &adapter.adapter_id {
            if should_add_new {
                self.index_manager
                    .add_to_index(adapter_indexes::BY_ADAPTER_ID, adapter_id, &entity_id)
                    .await?;
            } else if let Some(old) = old_adapter {
                if old.adapter_id.as_deref() != Some(adapter_id.as_str()) {
                    if let Some(old_adapter_id) = old.adapter_id.as_ref() {
                        let old_entity = old_entity_id.as_deref().unwrap_or_else(|| old.key_id());
                        self.index_manager
                            .remove_from_index(
                                adapter_indexes::BY_ADAPTER_ID,
                                old_adapter_id,
                                old_entity,
                            )
                            .await?;
                    }
                    self.index_manager
                        .add_to_index(adapter_indexes::BY_ADAPTER_ID, adapter_id, &entity_id)
                        .await?;
                }
            }
        }

        // Lifecycle state index
        let old_lifecycle = old_adapter.map(|a| a.lifecycle_state.as_str());
        self.index_manager
            .update_index(
                adapter_indexes::BY_LIFECYCLE_STATE,
                old_lifecycle,
                &adapter.lifecycle_state,
                &entity_id,
            )
            .await?;

        // Active index
        let active_str = adapter.active.to_string();
        let old_active = old_adapter.map(|a| a.active.to_string());
        self.index_manager
            .update_index(
                adapter_indexes::BY_ACTIVE,
                old_active.as_deref(),
                &active_str,
                &entity_id,
            )
            .await?;

        // Pinned index
        let pinned_str = adapter.pinned.to_string();
        let old_pinned = old_adapter.map(|a| a.pinned.to_string());
        self.index_manager
            .update_index(
                adapter_indexes::BY_PINNED,
                old_pinned.as_deref(),
                &pinned_str,
                &entity_id,
            )
            .await?;

        // Parent index (for lineage queries)
        if let Some(parent_id) = &adapter.parent_id {
            let old_parent = old_adapter.and_then(|a| a.parent_id.as_deref());
            self.index_manager
                .update_index(
                    adapter_indexes::BY_PARENT,
                    old_parent,
                    parent_id,
                    &entity_id,
                )
                .await?;
        } else if let Some(old_parent) = old_adapter.and_then(|a| a.parent_id.as_deref()) {
            let old_entity = old_entity_id.as_deref().unwrap_or_else(|| adapter.key_id());
            self.index_manager
                .remove_from_index(adapter_indexes::BY_PARENT, old_parent, old_entity)
                .await?;
        }

        Ok(())
    }

    /// Remove adapter from all indexes
    async fn remove_from_indexes(&self, adapter: &AdapterKv) -> Result<(), StorageError> {
        for entity_id in adapter.index_entity_ids() {
            self.index_manager
                .remove_from_index(adapter_indexes::BY_TENANT, &adapter.tenant_id, &entity_id)
                .await?;

            self.index_manager
                .remove_from_index(
                    adapter_indexes::BY_STATE,
                    &adapter.current_state,
                    &entity_id,
                )
                .await?;

            self.index_manager
                .remove_from_index(adapter_indexes::BY_TIER, &adapter.tier, &entity_id)
                .await?;

            self.index_manager
                .remove_from_index(adapter_indexes::BY_HASH, &adapter.hash_b3, &entity_id)
                .await?;

            // Remove adapter_id index
            if let Some(adapter_id) = &adapter.adapter_id {
                self.index_manager
                    .remove_from_index(adapter_indexes::BY_ADAPTER_ID, adapter_id, &entity_id)
                    .await?;
            }

            self.index_manager
                .remove_from_index(
                    adapter_indexes::BY_LIFECYCLE_STATE,
                    &adapter.lifecycle_state,
                    &entity_id,
                )
                .await?;

            self.index_manager
                .remove_from_index(
                    adapter_indexes::BY_ACTIVE,
                    &adapter.active.to_string(),
                    &entity_id,
                )
                .await?;

            self.index_manager
                .remove_from_index(
                    adapter_indexes::BY_PINNED,
                    &adapter.pinned.to_string(),
                    &entity_id,
                )
                .await?;

            if let Some(parent_id) = &adapter.parent_id {
                self.index_manager
                    .remove_from_index(adapter_indexes::BY_PARENT, parent_id, &entity_id)
                    .await?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::indexing::IndexManager;
    use crate::redb::RedbBackend;
    use chrono::Utc;

    fn sample_adapter(adapter_id: &str, id: &str, tenant_id: &str) -> AdapterKv {
        let now = Utc::now().to_rfc3339();
        AdapterKv {
            id: id.to_string(),
            adapter_id: Some(adapter_id.to_string()),
            tenant_id: tenant_id.to_string(),
            name: "Test Adapter".to_string(),
            tier: "warm".to_string(),
            hash_b3: "b3:test".to_string(),
            rank: 8,
            alpha: 16.0,
            targets_json: "[]".to_string(),
            acl_json: None,
            languages_json: None,
            framework: None,
            active: 1,
            category: "code".to_string(),
            scope: "global".to_string(),
            framework_id: None,
            framework_version: None,
            repo_id: None,
            commit_sha: None,
            intent: None,
            current_state: "unloaded".to_string(),
            pinned: 0,
            memory_bytes: 0,
            last_activated: None,
            activation_count: 0,
            expires_at: None,
            load_state: "cold".to_string(),
            last_loaded_at: None,
            aos_file_path: None,
            aos_file_hash: None,
            adapter_name: None,
            tenant_namespace: None,
            domain: None,
            purpose: None,
            revision: None,
            parent_id: None,
            fork_type: None,
            fork_reason: None,
            version: "1.0.0".to_string(),
            lifecycle_state: "active".to_string(),
            archived_at: None,
            archived_by: None,
            archive_reason: None,
            purged_at: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    fn repo_in_memory() -> (AdapterRepository, Arc<dyn KvBackend>, Arc<IndexManager>) {
        let backend = Arc::new(RedbBackend::open_in_memory().unwrap());
        let index_manager = Arc::new(IndexManager::new(backend.clone()));
        let repo = AdapterRepository::new(backend.clone(), index_manager.clone());
        (repo, backend, index_manager)
    }

    #[tokio::test]
    async fn create_and_read_use_adapter_id_key() {
        let (repo, backend, _indexes) = repo_in_memory();
        let adapter = sample_adapter("adapter-123", "uuid-1", "tenant-a");

        repo.create(adapter.clone()).await.unwrap();

        // Stored under adapter_id key
        assert!(backend.get("adapter:adapter-123").await.unwrap().is_some());

        let fetched = repo
            .get("tenant-a", "adapter-123")
            .await
            .unwrap()
            .expect("adapter readable");
        assert_eq!(fetched.id, "uuid-1");
        assert_eq!(fetched.adapter_id.as_deref(), Some("adapter-123"));
    }

    #[tokio::test]
    async fn update_migrates_legacy_uuid_key() {
        let (repo, backend, indexes) = repo_in_memory();
        let mut legacy = sample_adapter("legacy-adapter", "legacy-uuid", "tenant-a");

        // Store legacy key (UUID) and minimal index entry to simulate old layout
        let bytes = bincode::serialize(&legacy).unwrap();
        backend
            .set(&legacy.legacy_primary_key(), bytes)
            .await
            .unwrap();
        indexes
            .add_to_index(
                adapter_indexes::BY_ADAPTER_ID,
                legacy.adapter_id.as_ref().unwrap(),
                &legacy.id,
            )
            .await
            .unwrap();

        // Update state -> should rewrite under adapter_id key and drop legacy key
        legacy.current_state = "loaded".to_string();
        repo.update(legacy.clone()).await.unwrap();

        assert!(backend
            .get("adapter:legacy-adapter")
            .await
            .unwrap()
            .is_some());
        assert!(backend.get("adapter:legacy-uuid").await.unwrap().is_none());

        let fetched = repo
            .get("tenant-a", "legacy-adapter")
            .await
            .unwrap()
            .expect("adapter readable after migration");
        assert_eq!(fetched.current_state, "loaded");
    }

    #[tokio::test]
    async fn delete_removes_both_keys() {
        let (repo, backend, _indexes) = repo_in_memory();
        let adapter = sample_adapter("delete-me", "uuid-del", "tenant-a");

        repo.create(adapter.clone()).await.unwrap();

        let deleted = repo.delete("tenant-a", "delete-me").await.unwrap();
        assert!(deleted);

        assert!(backend.get("adapter:delete-me").await.unwrap().is_none());
        assert!(backend.get("adapter:uuid-del").await.unwrap().is_none());
    }
}
