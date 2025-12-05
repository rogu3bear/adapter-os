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

        // Check if adapter already exists
        if self.backend.exists(&key).await? {
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

        // adapter_id should be unique, so take the first match
        let internal_id = ids.first().map(|s| s.as_str());

        // Fetch by internal UUID if present
        let key = internal_id.map(|id| format!("adapter:{}", id));

        // Fallback: also attempt direct adapter_id key to handle historical mismatches
        // where the primary key may have been stored using adapter_id instead of UUID.
        let fallback_key = format!("adapter:{}", adapter_id);

        let bytes = match key {
            Some(k) => match self.backend.get(&k).await? {
                Some(b) => b,
                None => match self.backend.get(&fallback_key).await? {
                    Some(b) => b,
                    None => return Ok(None),
                },
            },
            None => match self.backend.get(&fallback_key).await? {
                Some(b) => b,
                None => return Ok(None),
            },
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

        // Get old adapter for index updates
        let old_bytes = self
            .backend
            .get(&key)
            .await?
            .ok_or_else(|| StorageError::NotFound(adapter.id.clone()))?;

        let old_adapter: AdapterKv = bincode::deserialize(&old_bytes)?;

        // Serialize new adapter
        let new_value = bincode::serialize(&adapter)?;

        // Update in storage
        self.backend.set(&key, new_value).await?;

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

        // Delete from storage
        let deleted = self.backend.delete(&key).await?;

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
        let id = &adapter.id;

        // Tenant index
        if old_adapter.is_none() {
            self.index_manager
                .add_to_index(adapter_indexes::BY_TENANT, &adapter.tenant_id, id)
                .await?;
        }

        // State index
        let old_state = old_adapter.map(|a| a.current_state.as_str());
        self.index_manager
            .update_index(
                adapter_indexes::BY_STATE,
                old_state,
                &adapter.current_state,
                id,
            )
            .await?;

        // Tier index
        let old_tier = old_adapter.map(|a| a.tier.as_str());
        self.index_manager
            .update_index(adapter_indexes::BY_TIER, old_tier, &adapter.tier, id)
            .await?;

        // Hash index
        if old_adapter.is_none() {
            self.index_manager
                .add_to_index(adapter_indexes::BY_HASH, &adapter.hash_b3, id)
                .await?;
        }

        // Adapter ID index (external adapter_id -> internal id mapping)
        if old_adapter.is_none() {
            if let Some(adapter_id) = &adapter.adapter_id {
                self.index_manager
                    .add_to_index(adapter_indexes::BY_ADAPTER_ID, adapter_id, id)
                    .await?;
            }
        }

        // Lifecycle state index
        let old_lifecycle = old_adapter.map(|a| a.lifecycle_state.as_str());
        self.index_manager
            .update_index(
                adapter_indexes::BY_LIFECYCLE_STATE,
                old_lifecycle,
                &adapter.lifecycle_state,
                id,
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
                id,
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
                id,
            )
            .await?;

        // Parent index (for lineage queries)
        if let Some(parent_id) = &adapter.parent_id {
            let old_parent = old_adapter.and_then(|a| a.parent_id.as_deref());
            self.index_manager
                .update_index(adapter_indexes::BY_PARENT, old_parent, parent_id, id)
                .await?;
        }

        Ok(())
    }

    /// Remove adapter from all indexes
    async fn remove_from_indexes(&self, adapter: &AdapterKv) -> Result<(), StorageError> {
        let id = &adapter.id;

        self.index_manager
            .remove_from_index(adapter_indexes::BY_TENANT, &adapter.tenant_id, id)
            .await?;

        self.index_manager
            .remove_from_index(adapter_indexes::BY_STATE, &adapter.current_state, id)
            .await?;

        self.index_manager
            .remove_from_index(adapter_indexes::BY_TIER, &adapter.tier, id)
            .await?;

        self.index_manager
            .remove_from_index(adapter_indexes::BY_HASH, &adapter.hash_b3, id)
            .await?;

        // Remove adapter_id index
        if let Some(adapter_id) = &adapter.adapter_id {
            self.index_manager
                .remove_from_index(adapter_indexes::BY_ADAPTER_ID, adapter_id, id)
                .await?;
        }

        self.index_manager
            .remove_from_index(
                adapter_indexes::BY_LIFECYCLE_STATE,
                &adapter.lifecycle_state,
                id,
            )
            .await?;

        self.index_manager
            .remove_from_index(adapter_indexes::BY_ACTIVE, &adapter.active.to_string(), id)
            .await?;

        self.index_manager
            .remove_from_index(adapter_indexes::BY_PINNED, &adapter.pinned.to_string(), id)
            .await?;

        if let Some(parent_id) = &adapter.parent_id {
            self.index_manager
                .remove_from_index(adapter_indexes::BY_PARENT, parent_id, id)
                .await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // Tests would go here - testing CRUD operations, lineage traversal,
    // pagination, index management, etc.
}
