//! Adapter operations with KV storage backend
//!
//! This module provides the KV-based implementation of adapter operations,
//! replacing SQL queries with key-value operations and Rust-based lineage traversal.

use crate::adapters::{Adapter, AdapterRegistrationParams};
use adapteros_core::{AosError, Result};
// Use models::AdapterKv which matches what AdapterRepository uses
use adapteros_storage::repos::adapter::AdapterRepository;
use adapteros_storage::AdapterKv;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Trait for adapter operations in KV mode
///
/// This trait defines all adapter operations that can be performed
/// against the KV storage backend. It mirrors the SQL operations
/// from the `Db` impl but uses KV storage primitives.
#[async_trait::async_trait]
pub trait AdapterKvOps {
    /// Register a new adapter
    async fn register_adapter_kv(&self, params: AdapterRegistrationParams) -> Result<String>;

    /// Register a new adapter with a specific ID (for dual-write consistency with SQL)
    async fn register_adapter_kv_with_id(
        &self,
        id: &str,
        params: AdapterRegistrationParams,
    ) -> Result<String>;

    /// Get adapter by ID
    async fn get_adapter_kv(&self, adapter_id: &str) -> Result<Option<Adapter>>;

    /// List adapters for a tenant
    async fn list_adapters_for_tenant_kv(&self, tenant_id: &str) -> Result<Vec<Adapter>>;

    /// Delete an adapter
    async fn delete_adapter_kv(&self, id: &str) -> Result<()>;

    /// Update adapter state
    async fn update_adapter_state_kv(
        &self,
        adapter_id: &str,
        state: &str,
        reason: &str,
    ) -> Result<()>;

    /// Update adapter memory
    async fn update_adapter_memory_kv(&self, adapter_id: &str, memory_bytes: i64) -> Result<()>;

    /// Update both state and memory atomically
    async fn update_adapter_state_and_memory_kv(
        &self,
        adapter_id: &str,
        state: &str,
        memory_bytes: i64,
        reason: &str,
    ) -> Result<()>;

    /// Get adapter lineage (ancestors + descendants)
    async fn get_adapter_lineage_kv(&self, adapter_id: &str) -> Result<Vec<Adapter>>;

    /// Get direct children of an adapter
    async fn get_adapter_children_kv(&self, adapter_id: &str) -> Result<Vec<Adapter>>;

    /// Get lineage path from root to adapter
    async fn get_lineage_path_kv(&self, adapter_id: &str) -> Result<Vec<Adapter>>;

    /// Find adapter by hash
    async fn find_adapter_by_hash_kv(&self, hash_b3: &str) -> Result<Option<Adapter>>;

    /// List adapters by category
    async fn list_adapters_by_category_kv(&self, category: &str) -> Result<Vec<Adapter>>;

    /// List adapters by scope
    async fn list_adapters_by_scope_kv(&self, scope: &str) -> Result<Vec<Adapter>>;

    /// List adapters by state
    async fn list_adapters_by_state_kv(&self, state: &str) -> Result<Vec<Adapter>>;

    /// Update adapter tier
    async fn update_adapter_tier_kv(&self, adapter_id: &str, tier: &str) -> Result<()>;

    /// Increment adapter activation count
    async fn increment_adapter_activation_kv(&self, adapter_id: &str) -> Result<()>;

    /// Archive an adapter in KV backend
    async fn archive_adapter_kv(
        &self,
        adapter_id: &str,
        archived_by: &str,
        reason: &str,
    ) -> Result<()>;

    /// Mark adapter as purged in KV backend
    async fn mark_adapter_purged_kv(&self, adapter_id: &str) -> Result<()>;

    /// Unarchive an adapter in KV backend
    async fn unarchive_adapter_kv(&self, adapter_id: &str) -> Result<()>;
}

/// KV adapter service that wraps the repository
pub struct AdapterKvRepository {
    repo: Arc<AdapterRepository>,
    // We need to track the tenant_id for multi-tenant operations
    // In practice, this might come from request context
    default_tenant: String,
    // Mutex to serialize concurrent increments for each adapter
    increment_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl AdapterKvRepository {
    /// Create a new KV adapter service
    pub fn new(repo: Arc<AdapterRepository>, default_tenant: String) -> Self {
        Self {
            repo,
            default_tenant,
            increment_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new KV adapter service with shared locks
    pub fn new_with_locks(
        repo: Arc<AdapterRepository>,
        default_tenant: String,
        increment_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    ) -> Self {
        Self {
            repo,
            default_tenant,
            increment_locks,
        }
    }
}

// ============================================================================
// Type Conversions: SQL Adapter <-> KV AdapterKv
// ============================================================================

/// Convert SQL Adapter to KV AdapterKv
/// Note: models::AdapterKv uses String timestamps and i32 for booleans
impl From<Adapter> for AdapterKv {
    fn from(adapter: Adapter) -> Self {
        AdapterKv {
            id: adapter.id,
            tenant_id: adapter.tenant_id,
            adapter_id: adapter.adapter_id,
            name: adapter.name,
            hash_b3: adapter.hash_b3,
            rank: adapter.rank,
            alpha: adapter.alpha,
            tier: adapter.tier,
            targets_json: adapter.targets_json,
            acl_json: adapter.acl_json,
            languages_json: adapter.languages_json,
            category: adapter.category,
            scope: adapter.scope,
            framework: adapter.framework,
            framework_id: adapter.framework_id,
            framework_version: adapter.framework_version,
            lifecycle_state: adapter.lifecycle_state,
            current_state: adapter.current_state,
            load_state: adapter.load_state,
            version: adapter.version,
            active: adapter.active,
            adapter_name: adapter.adapter_name,
            tenant_namespace: adapter.tenant_namespace,
            domain: adapter.domain,
            purpose: adapter.purpose,
            revision: adapter.revision,
            parent_id: adapter.parent_id,
            fork_type: adapter.fork_type,
            fork_reason: adapter.fork_reason,
            repo_id: adapter.repo_id,
            commit_sha: adapter.commit_sha,
            intent: adapter.intent,
            memory_bytes: adapter.memory_bytes,
            activation_count: adapter.activation_count,
            last_activated: adapter.last_activated,
            last_loaded_at: adapter.last_loaded_at,
            pinned: adapter.pinned,
            expires_at: adapter.expires_at,
            aos_file_path: adapter.aos_file_path,
            aos_file_hash: adapter.aos_file_hash,
            archived_at: adapter.archived_at,
            archived_by: adapter.archived_by,
            archive_reason: adapter.archive_reason,
            purged_at: adapter.purged_at,
            created_at: adapter.created_at,
            updated_at: adapter.updated_at,
        }
    }
}

/// Convert KV AdapterKv to SQL Adapter
impl From<AdapterKv> for Adapter {
    fn from(kv: AdapterKv) -> Self {
        Adapter {
            id: kv.id,
            tenant_id: kv.tenant_id,
            name: kv.name,
            tier: kv.tier,
            hash_b3: kv.hash_b3,
            rank: kv.rank,
            alpha: kv.alpha,
            targets_json: kv.targets_json,
            acl_json: kv.acl_json,
            adapter_id: kv.adapter_id,
            languages_json: kv.languages_json,
            framework: kv.framework,
            active: kv.active,
            category: kv.category,
            scope: kv.scope,
            framework_id: kv.framework_id,
            framework_version: kv.framework_version,
            repo_id: kv.repo_id,
            commit_sha: kv.commit_sha,
            intent: kv.intent,
            current_state: kv.current_state,
            pinned: kv.pinned,
            memory_bytes: kv.memory_bytes,
            last_activated: kv.last_activated,
            activation_count: kv.activation_count,
            expires_at: kv.expires_at,
            load_state: kv.load_state,
            last_loaded_at: kv.last_loaded_at,
            aos_file_path: kv.aos_file_path,
            aos_file_hash: kv.aos_file_hash,
            adapter_name: kv.adapter_name,
            tenant_namespace: kv.tenant_namespace,
            domain: kv.domain,
            purpose: kv.purpose,
            revision: kv.revision,
            parent_id: kv.parent_id,
            fork_type: kv.fork_type,
            fork_reason: kv.fork_reason,
            created_at: kv.created_at,
            updated_at: kv.updated_at,
            version: kv.version,
            lifecycle_state: kv.lifecycle_state,
            // Archive/GC fields from KV
            archived_at: kv.archived_at,
            archived_by: kv.archived_by,
            archive_reason: kv.archive_reason,
            purged_at: kv.purged_at,
            // Base model and artifact hardening (not stored in KV yet)
            base_model_id: None,
            manifest_schema_version: None,
            content_hash_b3: None,
            provenance_json: None,
        }
    }
}

// ============================================================================
// AdapterKvRepository Implementation
// ============================================================================

#[async_trait::async_trait]
impl AdapterKvOps for AdapterKvRepository {
    async fn register_adapter_kv(&self, params: AdapterRegistrationParams) -> Result<String> {
        let id = uuid::Uuid::now_v7().to_string();
        self.register_adapter_kv_with_id(&id, params).await
    }

    async fn register_adapter_kv_with_id(
        &self,
        id: &str,
        params: AdapterRegistrationParams,
    ) -> Result<String> {
        let now = Utc::now().to_rfc3339();

        // Create KV adapter entity (models::AdapterKv uses String timestamps and i32 for booleans)
        let adapter_kv = AdapterKv {
            id: id.to_string(),
            tenant_id: params.tenant_id.clone(),
            adapter_id: Some(params.adapter_id.clone()),
            name: params.name.clone(),
            hash_b3: params.hash_b3.clone(),
            rank: params.rank,
            alpha: params.alpha,
            tier: params.tier.clone(),
            targets_json: params.targets_json.clone(),
            acl_json: params.acl_json.clone(),
            languages_json: params.languages_json.clone(),
            category: params.category.clone(),
            scope: params.scope.clone(),
            framework: params.framework.clone(),
            framework_id: params.framework_id.clone(),
            framework_version: params.framework_version.clone(),
            lifecycle_state: "active".to_string(),
            current_state: "unloaded".to_string(),
            load_state: "cold".to_string(),
            version: "1.0.0".to_string(),
            active: 1,
            adapter_name: params.adapter_name.clone(),
            tenant_namespace: params.tenant_namespace.clone(),
            domain: params.domain.clone(),
            purpose: params.purpose.clone(),
            revision: params.revision.clone(),
            parent_id: params.parent_id.clone(),
            fork_type: params.fork_type.clone(),
            fork_reason: params.fork_reason.clone(),
            repo_id: params.repo_id.clone(),
            commit_sha: params.commit_sha.clone(),
            intent: params.intent.clone(),
            memory_bytes: 0,
            activation_count: 0,
            last_activated: None,
            last_loaded_at: None,
            pinned: 0,
            expires_at: params.expires_at.clone(),
            aos_file_path: params.aos_file_path.clone(),
            aos_file_hash: params.aos_file_hash.clone(),
            created_at: now.clone(),
            updated_at: now,
            // Archive/GC fields default to None for new adapters
            archived_at: None,
            archived_by: None,
            archive_reason: None,
            purged_at: None,
        };

        self.repo
            .create(adapter_kv)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create adapter: {}", e)))?;

        debug!(adapter_id = %params.adapter_id, tenant_id = %params.tenant_id, id = %id, "Adapter registered in KV storage");
        Ok(id.to_string())
    }

    async fn get_adapter_kv(&self, adapter_id: &str) -> Result<Option<Adapter>> {
        let adapter_kv = self
            .repo
            .get(&self.default_tenant, adapter_id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get adapter: {}", e)))?;

        Ok(adapter_kv.map(|kv| kv.into()))
    }

    async fn list_adapters_for_tenant_kv(&self, tenant_id: &str) -> Result<Vec<Adapter>> {
        let adapters_kv = self
            .repo
            .list_by_tenant(tenant_id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to list adapters: {}", e)))?;

        Ok(adapters_kv.into_iter().map(|kv| kv.into()).collect())
    }

    async fn delete_adapter_kv(&self, id: &str) -> Result<()> {
        // Note: In KV mode, we need to get the adapter first to extract adapter_id
        // The SQL version takes the internal `id` field, but we need to map it to adapter_id
        let deleted = self
            .repo
            .delete(&self.default_tenant, id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete adapter: {}", e)))?;

        if !deleted {
            return Err(AosError::NotFound(format!("Adapter not found: {}", id)));
        }

        debug!(adapter_id = %id, tenant_id = %self.default_tenant, "Adapter deleted from KV storage");
        Ok(())
    }

    async fn update_adapter_state_kv(
        &self,
        adapter_id: &str,
        state: &str,
        reason: &str,
    ) -> Result<()> {
        debug!(adapter_id = %adapter_id, state = %state, reason = %reason,
               "Updating adapter state (KV)");

        // Get current adapter
        let mut adapter_kv = self
            .repo
            .get(&self.default_tenant, adapter_id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get adapter: {}", e)))?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        // Update state
        adapter_kv.current_state = state.to_string();
        adapter_kv.updated_at = Utc::now().to_rfc3339();

        // Save
        self.repo
            .update(adapter_kv)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update adapter state: {}", e)))?;

        Ok(())
    }

    async fn update_adapter_memory_kv(&self, adapter_id: &str, memory_bytes: i64) -> Result<()> {
        debug!(adapter_id = %adapter_id, memory_bytes = %memory_bytes,
               "Updating adapter memory (KV)");

        // Get current adapter
        let mut adapter_kv = self
            .repo
            .get(&self.default_tenant, adapter_id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get adapter: {}", e)))?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        // Update memory
        adapter_kv.memory_bytes = memory_bytes;
        adapter_kv.updated_at = Utc::now().to_rfc3339();

        // Save
        self.repo
            .update(adapter_kv)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update adapter memory: {}", e)))?;

        Ok(())
    }

    async fn update_adapter_state_and_memory_kv(
        &self,
        adapter_id: &str,
        state: &str,
        memory_bytes: i64,
        reason: &str,
    ) -> Result<()> {
        debug!(
            adapter_id = %adapter_id,
            state = %state,
            memory_bytes = %memory_bytes,
            reason = %reason,
            "Updating adapter state and memory atomically (KV)"
        );

        // Get current adapter
        let mut adapter_kv = self
            .repo
            .get(&self.default_tenant, adapter_id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get adapter: {}", e)))?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        // Update both fields
        adapter_kv.current_state = state.to_string();
        adapter_kv.memory_bytes = memory_bytes;
        adapter_kv.updated_at = Utc::now().to_rfc3339();

        // Save atomically
        self.repo
            .update(adapter_kv)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update adapter: {}", e)))?;

        Ok(())
    }

    async fn get_adapter_lineage_kv(&self, adapter_id: &str) -> Result<Vec<Adapter>> {
        // Get ancestors
        let ancestors = self
            .repo
            .get_ancestors(&self.default_tenant, adapter_id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get ancestors: {}", e)))?;

        // Get descendants
        let descendants = self
            .repo
            .get_descendants(&self.default_tenant, adapter_id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get descendants: {}", e)))?;

        // Combine and deduplicate
        let mut lineage: Vec<AdapterKv> = ancestors;
        lineage.extend(descendants);

        // Deduplicate by ID
        lineage.sort_by(|a, b| a.id.cmp(&b.id));
        lineage.dedup_by(|a, b| a.id == b.id);

        // Sort by created_at
        lineage.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        // Convert to SQL Adapter type
        Ok(lineage.into_iter().map(|kv| kv.into()).collect())
    }

    async fn get_adapter_children_kv(&self, adapter_id: &str) -> Result<Vec<Adapter>> {
        // Get all descendants, then filter to direct children
        let descendants = self
            .repo
            .get_descendants(&self.default_tenant, adapter_id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get descendants: {}", e)))?;

        // Filter to direct children only
        let children: Vec<Adapter> = descendants
            .into_iter()
            .filter(|d| d.parent_id.as_ref() == Some(&adapter_id.to_string()))
            .map(|kv| kv.into())
            .collect();

        Ok(children)
    }

    async fn get_lineage_path_kv(&self, adapter_id: &str) -> Result<Vec<Adapter>> {
        // Get ancestors (which includes the adapter itself)
        let ancestors = self
            .repo
            .get_ancestors(&self.default_tenant, adapter_id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get ancestors: {}", e)))?;

        // Reverse to get root-to-leaf order
        let mut path: Vec<Adapter> = ancestors.into_iter().map(|kv| kv.into()).collect();
        path.reverse();

        Ok(path)
    }

    async fn find_adapter_by_hash_kv(&self, hash_b3: &str) -> Result<Option<Adapter>> {
        let adapter_kv =
            self.repo.find_by_hash(hash_b3).await.map_err(|e| {
                AosError::Database(format!("Failed to find adapter by hash: {}", e))
            })?;

        Ok(adapter_kv.map(|kv| kv.into()))
    }

    async fn list_adapters_by_category_kv(&self, category: &str) -> Result<Vec<Adapter>> {
        // Get all adapters for tenant, then filter by category
        let adapters = self
            .repo
            .list_by_tenant(&self.default_tenant)
            .await
            .map_err(|e| AosError::Database(format!("Failed to list adapters: {}", e)))?;

        let filtered: Vec<Adapter> = adapters
            .into_iter()
            .filter(|a| a.category == category)
            .map(|kv| kv.into())
            .collect();

        Ok(filtered)
    }

    async fn list_adapters_by_scope_kv(&self, scope: &str) -> Result<Vec<Adapter>> {
        // Get all adapters for tenant, then filter by scope
        let adapters = self
            .repo
            .list_by_tenant(&self.default_tenant)
            .await
            .map_err(|e| AosError::Database(format!("Failed to list adapters: {}", e)))?;

        let filtered: Vec<Adapter> = adapters
            .into_iter()
            .filter(|a| a.scope == scope)
            .map(|kv| kv.into())
            .collect();

        Ok(filtered)
    }

    async fn list_adapters_by_state_kv(&self, state: &str) -> Result<Vec<Adapter>> {
        let adapters = self
            .repo
            .list_by_state(&self.default_tenant, state)
            .await
            .map_err(|e| AosError::Database(format!("Failed to list adapters by state: {}", e)))?;

        Ok(adapters.into_iter().map(|kv| kv.into()).collect())
    }

    async fn update_adapter_tier_kv(&self, adapter_id: &str, tier: &str) -> Result<()> {
        // Validate tier
        if !["persistent", "warm", "ephemeral"].contains(&tier) {
            return Err(AosError::Validation(format!(
                "Invalid tier: {}. Must be 'persistent', 'warm', or 'ephemeral'",
                tier
            )));
        }

        // Get current adapter
        let mut adapter_kv = self
            .repo
            .get(&self.default_tenant, adapter_id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get adapter: {}", e)))?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        // Update tier
        adapter_kv.tier = tier.to_string();
        adapter_kv.updated_at = Utc::now().to_rfc3339();

        // Save
        self.repo
            .update(adapter_kv)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update adapter tier: {}", e)))?;

        Ok(())
    }

    async fn increment_adapter_activation_kv(&self, adapter_id: &str) -> Result<()> {
        debug!(adapter_id = %adapter_id, "Incrementing adapter activation (KV)");

        // Get or create a per-adapter lock to serialize concurrent increments
        let lock = {
            let mut locks = self.increment_locks.lock().await;
            locks
                .entry(adapter_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };

        // Hold the lock while doing read-modify-write
        let _guard = lock.lock().await;

        // Get current adapter
        let mut adapter_kv = self
            .repo
            .get(&self.default_tenant, adapter_id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get adapter: {}", e)))?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        // Increment activation count
        adapter_kv.activation_count += 1;
        adapter_kv.last_activated = Some(Utc::now().to_rfc3339());
        adapter_kv.updated_at = Utc::now().to_rfc3339();

        // Save
        self.repo.update(adapter_kv).await.map_err(|e| {
            AosError::Database(format!("Failed to increment adapter activation: {}", e))
        })?;

        Ok(())
    }

    async fn archive_adapter_kv(
        &self,
        adapter_id: &str,
        archived_by: &str,
        reason: &str,
    ) -> Result<()> {
        debug!(adapter_id = %adapter_id, archived_by = %archived_by, "Archiving adapter (KV)");

        // Get current adapter
        let mut adapter_kv = self
            .repo
            .get(&self.default_tenant, adapter_id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get adapter: {}", e)))?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        // Check not already archived
        if adapter_kv.archived_at.is_some() {
            return Err(AosError::Validation(format!(
                "Adapter {} is already archived",
                adapter_id
            )));
        }

        // Set archive fields
        adapter_kv.archived_at = Some(Utc::now().to_rfc3339());
        adapter_kv.archived_by = Some(archived_by.to_string());
        adapter_kv.archive_reason = Some(reason.to_string());
        adapter_kv.updated_at = Utc::now().to_rfc3339();

        // Save
        self.repo
            .update(adapter_kv)
            .await
            .map_err(|e| AosError::Database(format!("Failed to archive adapter: {}", e)))?;

        Ok(())
    }

    async fn mark_adapter_purged_kv(&self, adapter_id: &str) -> Result<()> {
        debug!(adapter_id = %adapter_id, "Marking adapter as purged (KV)");

        // Get current adapter
        let mut adapter_kv = self
            .repo
            .get(&self.default_tenant, adapter_id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get adapter: {}", e)))?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        // Check adapter is archived
        if adapter_kv.archived_at.is_none() {
            return Err(AosError::Validation(format!(
                "Cannot purge adapter {} that is not archived",
                adapter_id
            )));
        }

        // Set purge fields
        adapter_kv.purged_at = Some(Utc::now().to_rfc3339());
        adapter_kv.aos_file_path = None;
        adapter_kv.updated_at = Utc::now().to_rfc3339();

        // Save
        self.repo
            .update(adapter_kv)
            .await
            .map_err(|e| AosError::Database(format!("Failed to mark adapter purged: {}", e)))?;

        Ok(())
    }

    async fn unarchive_adapter_kv(&self, adapter_id: &str) -> Result<()> {
        debug!(adapter_id = %adapter_id, "Unarchiving adapter (KV)");

        // Get current adapter
        let mut adapter_kv = self
            .repo
            .get(&self.default_tenant, adapter_id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get adapter: {}", e)))?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        // Check adapter is archived but not purged
        if adapter_kv.archived_at.is_none() {
            return Err(AosError::Validation(format!(
                "Adapter {} is not archived",
                adapter_id
            )));
        }
        if adapter_kv.purged_at.is_some() {
            return Err(AosError::Validation(format!(
                "Cannot unarchive adapter {} that has been purged",
                adapter_id
            )));
        }

        // Clear archive fields
        adapter_kv.archived_at = None;
        adapter_kv.archived_by = None;
        adapter_kv.archive_reason = None;
        adapter_kv.updated_at = Utc::now().to_rfc3339();

        // Save
        self.repo
            .update(adapter_kv)
            .await
            .map_err(|e| AosError::Database(format!("Failed to unarchive adapter: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_sql_to_kv_conversion() {
        let sql_adapter = Adapter {
            id: "adapter-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            adapter_id: Some("external-id-1".to_string()),
            name: "Test Adapter".to_string(),
            hash_b3: "abc123".to_string(),
            rank: 8,
            alpha: 16.0,
            tier: "warm".to_string(),
            targets_json: r#"["q_proj","v_proj"]"#.to_string(),
            acl_json: Some(r#"["user-1","user-2"]"#.to_string()),
            languages_json: Some(r#"["rust"]"#.to_string()),
            framework: Some("pytorch".to_string()),
            active: 1,
            category: "code".to_string(),
            scope: "global".to_string(),
            framework_id: None,
            framework_version: None,
            repo_id: None,
            commit_sha: None,
            intent: None,
            current_state: "warm".to_string(),
            pinned: 0,
            memory_bytes: 1048576,
            last_activated: Some("2025-01-01T00:00:00Z".to_string()),
            activation_count: 42,
            expires_at: None,
            load_state: "loaded".to_string(),
            last_loaded_at: None,
            aos_file_path: Some("/var/adapters/test.aos".to_string()),
            aos_file_hash: None,
            adapter_name: None,
            tenant_namespace: None,
            domain: None,
            purpose: None,
            revision: None,
            parent_id: None,
            fork_type: None,
            fork_reason: None,
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
            version: "1.0.0".to_string(),
            lifecycle_state: "active".to_string(),
            archived_at: None,
            archived_by: None,
            archive_reason: None,
            purged_at: None,
        };

        // Convert to KV (models::AdapterKv uses same types as SQL Adapter)
        let kv_adapter: AdapterKv = sql_adapter.clone().into();

        // Verify key fields - models::AdapterKv uses i32 for active/pinned
        assert_eq!(kv_adapter.id, "adapter-1");
        assert_eq!(kv_adapter.name, "Test Adapter");
        assert_eq!(kv_adapter.tier, "warm");
        assert_eq!(kv_adapter.targets_json, r#"["q_proj","v_proj"]"#);
        assert_eq!(kv_adapter.active, 1);
        assert_eq!(kv_adapter.pinned, 0);

        // Convert back to SQL
        let sql_adapter_2: Adapter = kv_adapter.into();

        // Verify round-trip conversion
        assert_eq!(sql_adapter_2.id, sql_adapter.id);
        assert_eq!(sql_adapter_2.name, sql_adapter.name);
        assert_eq!(sql_adapter_2.tier, sql_adapter.tier);
        assert_eq!(sql_adapter_2.targets_json, sql_adapter.targets_json);
    }
}
