//! Adapter operations with KV storage backend
//!
//! This module provides the KV-based implementation of adapter operations,
//! replacing SQL queries with key-value operations and Rust-based lineage traversal.

use crate::adapters::{Adapter, AdapterRegistrationParams};
use adapteros_core::{AosError, Result};
use adapteros_storage::entities::AdapterKv;
use adapteros_storage::repos::adapter::AdapterRepository;
use chrono::{DateTime, Utc};
use std::sync::Arc;
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
}

/// KV adapter service that wraps the repository
pub struct KvAdapterService {
    repo: Arc<AdapterRepository>,
    // We need to track the tenant_id for multi-tenant operations
    // In practice, this might come from request context
    default_tenant: String,
}

impl KvAdapterService {
    /// Create a new KV adapter service
    pub fn new(repo: Arc<AdapterRepository>, default_tenant: String) -> Self {
        Self {
            repo,
            default_tenant,
        }
    }
}

// ============================================================================
// Type Conversions: SQL Adapter <-> KV AdapterKv
// ============================================================================

/// Convert SQL Adapter to KV AdapterKv
impl From<Adapter> for AdapterKv {
    fn from(adapter: Adapter) -> Self {
        // Parse JSON fields
        let targets = serde_json::from_str::<Vec<String>>(&adapter.targets_json)
            .unwrap_or_default();

        let acl = adapter.acl_json
            .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok());

        let languages = adapter.languages_json
            .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok());

        // Parse timestamps
        let parse_timestamp = |s: Option<String>| -> Option<DateTime<Utc>> {
            s.and_then(|ts| DateTime::parse_from_rfc3339(&ts).ok())
                .map(|dt| dt.with_timezone(&Utc))
        };

        let created_at = parse_timestamp(Some(adapter.created_at))
            .unwrap_or_else(Utc::now);
        let updated_at = parse_timestamp(Some(adapter.updated_at))
            .unwrap_or_else(Utc::now);

        AdapterKv {
            id: adapter.id,
            tenant_id: adapter.tenant_id,
            adapter_id: adapter.adapter_id,
            name: adapter.name,
            hash_b3: adapter.hash_b3,
            rank: adapter.rank,
            alpha: adapter.alpha,
            tier: adapter.tier,
            targets,
            acl,
            languages,
            category: adapter.category,
            scope: adapter.scope,
            framework: adapter.framework,
            framework_id: adapter.framework_id,
            framework_version: adapter.framework_version,
            lifecycle_state: adapter.lifecycle_state,
            current_state: adapter.current_state,
            load_state: adapter.load_state,
            version: adapter.version,
            active: adapter.active != 0,
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
            last_activated: parse_timestamp(adapter.last_activated),
            last_loaded_at: parse_timestamp(adapter.last_loaded_at),
            pinned: adapter.pinned != 0,
            expires_at: parse_timestamp(adapter.expires_at),
            aos_file_path: adapter.aos_file_path,
            aos_file_hash: adapter.aos_file_hash,
            created_at,
            updated_at,
        }
    }
}

/// Convert KV AdapterKv to SQL Adapter
impl From<AdapterKv> for Adapter {
    fn from(kv: AdapterKv) -> Self {
        // Serialize JSON fields
        let targets_json = serde_json::to_string(&kv.targets)
            .unwrap_or_else(|_| "[]".to_string());

        let acl_json = kv.acl
            .and_then(|acl| serde_json::to_string(&acl).ok());

        let languages_json = kv.languages
            .and_then(|langs| serde_json::to_string(&langs).ok());

        // Format timestamps as RFC3339
        let format_timestamp = |dt: Option<DateTime<Utc>>| -> Option<String> {
            dt.map(|t| t.to_rfc3339())
        };

        Adapter {
            id: kv.id,
            tenant_id: kv.tenant_id,
            name: kv.name,
            tier: kv.tier,
            hash_b3: kv.hash_b3,
            rank: kv.rank,
            alpha: kv.alpha,
            targets_json,
            acl_json,
            adapter_id: kv.adapter_id,
            languages_json,
            framework: kv.framework,
            active: if kv.active { 1 } else { 0 },
            category: kv.category,
            scope: kv.scope,
            framework_id: kv.framework_id,
            framework_version: kv.framework_version,
            repo_id: kv.repo_id,
            commit_sha: kv.commit_sha,
            intent: kv.intent,
            current_state: kv.current_state,
            pinned: if kv.pinned { 1 } else { 0 },
            memory_bytes: kv.memory_bytes,
            last_activated: format_timestamp(kv.last_activated),
            activation_count: kv.activation_count,
            expires_at: format_timestamp(kv.expires_at),
            load_state: kv.load_state,
            last_loaded_at: format_timestamp(kv.last_loaded_at),
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
            created_at: kv.created_at.to_rfc3339(),
            updated_at: kv.updated_at.to_rfc3339(),
            version: kv.version,
            lifecycle_state: kv.lifecycle_state,
        }
    }
}

// ============================================================================
// KvAdapterService Implementation
// ============================================================================

#[async_trait::async_trait]
impl AdapterKvOps for KvAdapterService {
    async fn register_adapter_kv(&self, params: AdapterRegistrationParams) -> Result<String> {
        let id = uuid::Uuid::now_v7().to_string();

        // Create KV adapter entity
        let adapter_kv = AdapterKv {
            id: id.clone(),
            tenant_id: params.tenant_id.clone(),
            adapter_id: Some(params.adapter_id.clone()),
            name: params.name.clone(),
            hash_b3: params.hash_b3.clone(),
            rank: params.rank,
            alpha: params.alpha,
            tier: params.tier.clone(),
            targets: serde_json::from_str(&params.targets_json).unwrap_or_default(),
            acl: params.acl_json
                .and_then(|json| serde_json::from_str(&json).ok()),
            languages: params.languages_json
                .and_then(|json| serde_json::from_str(&json).ok()),
            category: params.category.clone(),
            scope: params.scope.clone(),
            framework: params.framework.clone(),
            framework_id: params.framework_id.clone(),
            framework_version: params.framework_version.clone(),
            lifecycle_state: "active".to_string(),
            current_state: "unloaded".to_string(),
            load_state: "cold".to_string(),
            version: "1.0.0".to_string(),
            active: true,
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
            pinned: false,
            expires_at: params.expires_at
                .and_then(|ts| DateTime::parse_from_rfc3339(&ts).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            aos_file_path: params.aos_file_path.clone(),
            aos_file_hash: params.aos_file_hash.clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        self.repo.create(adapter_kv).await
            .map_err(|e| AosError::Database(format!("Failed to create adapter: {}", e)))?;

        info!(adapter_id = %params.adapter_id, "Adapter registered in KV storage");
        Ok(id)
    }

    async fn get_adapter_kv(&self, adapter_id: &str) -> Result<Option<Adapter>> {
        let adapter_kv = self.repo.get(&self.default_tenant, adapter_id).await
            .map_err(|e| AosError::Database(format!("Failed to get adapter: {}", e)))?;

        Ok(adapter_kv.map(|kv| kv.into()))
    }

    async fn list_adapters_for_tenant_kv(&self, tenant_id: &str) -> Result<Vec<Adapter>> {
        let adapters_kv = self.repo.list_by_tenant(tenant_id).await
            .map_err(|e| AosError::Database(format!("Failed to list adapters: {}", e)))?;

        Ok(adapters_kv.into_iter().map(|kv| kv.into()).collect())
    }

    async fn delete_adapter_kv(&self, id: &str) -> Result<()> {
        // Note: In KV mode, we need to get the adapter first to extract adapter_id
        // The SQL version takes the internal `id` field, but we need to map it to adapter_id
        let deleted = self.repo.delete(&self.default_tenant, id).await
            .map_err(|e| AosError::Database(format!("Failed to delete adapter: {}", e)))?;

        if !deleted {
            return Err(AosError::NotFound(format!("Adapter not found: {}", id)));
        }

        info!(adapter_id = %id, "Adapter deleted from KV storage");
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
        let mut adapter_kv = self.repo.get(&self.default_tenant, adapter_id).await
            .map_err(|e| AosError::Database(format!("Failed to get adapter: {}", e)))?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        // Update state
        adapter_kv.current_state = state.to_string();
        adapter_kv.updated_at = Utc::now();

        // Save
        self.repo.update(adapter_kv).await
            .map_err(|e| AosError::Database(format!("Failed to update adapter state: {}", e)))?;

        Ok(())
    }

    async fn update_adapter_memory_kv(&self, adapter_id: &str, memory_bytes: i64) -> Result<()> {
        debug!(adapter_id = %adapter_id, memory_bytes = %memory_bytes,
               "Updating adapter memory (KV)");

        // Get current adapter
        let mut adapter_kv = self.repo.get(&self.default_tenant, adapter_id).await
            .map_err(|e| AosError::Database(format!("Failed to get adapter: {}", e)))?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        // Update memory
        adapter_kv.memory_bytes = memory_bytes;
        adapter_kv.updated_at = Utc::now();

        // Save
        self.repo.update(adapter_kv).await
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
        let mut adapter_kv = self.repo.get(&self.default_tenant, adapter_id).await
            .map_err(|e| AosError::Database(format!("Failed to get adapter: {}", e)))?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        // Update both fields
        adapter_kv.current_state = state.to_string();
        adapter_kv.memory_bytes = memory_bytes;
        adapter_kv.updated_at = Utc::now();

        // Save atomically
        self.repo.update(adapter_kv).await
            .map_err(|e| AosError::Database(format!("Failed to update adapter: {}", e)))?;

        Ok(())
    }

    async fn get_adapter_lineage_kv(&self, adapter_id: &str) -> Result<Vec<Adapter>> {
        // Get ancestors
        let ancestors = self.repo.get_ancestors(&self.default_tenant, adapter_id).await
            .map_err(|e| AosError::Database(format!("Failed to get ancestors: {}", e)))?;

        // Get descendants
        let descendants = self.repo.get_descendants(&self.default_tenant, adapter_id).await
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
        let descendants = self.repo.get_descendants(&self.default_tenant, adapter_id).await
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
        let ancestors = self.repo.get_ancestors(&self.default_tenant, adapter_id).await
            .map_err(|e| AosError::Database(format!("Failed to get ancestors: {}", e)))?;

        // Reverse to get root-to-leaf order
        let mut path: Vec<Adapter> = ancestors.into_iter().map(|kv| kv.into()).collect();
        path.reverse();

        Ok(path)
    }

    async fn find_adapter_by_hash_kv(&self, hash_b3: &str) -> Result<Option<Adapter>> {
        let adapter_kv = self.repo.find_by_hash(hash_b3).await
            .map_err(|e| AosError::Database(format!("Failed to find adapter by hash: {}", e)))?;

        Ok(adapter_kv.map(|kv| kv.into()))
    }

    async fn list_adapters_by_category_kv(&self, category: &str) -> Result<Vec<Adapter>> {
        // Get all adapters for tenant, then filter by category
        let adapters = self.repo.list_by_tenant(&self.default_tenant).await
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
        let adapters = self.repo.list_by_tenant(&self.default_tenant).await
            .map_err(|e| AosError::Database(format!("Failed to list adapters: {}", e)))?;

        let filtered: Vec<Adapter> = adapters
            .into_iter()
            .filter(|a| a.scope == scope)
            .map(|kv| kv.into())
            .collect();

        Ok(filtered)
    }

    async fn list_adapters_by_state_kv(&self, state: &str) -> Result<Vec<Adapter>> {
        let adapters = self.repo.list_by_state(&self.default_tenant, state).await
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
        let mut adapter_kv = self.repo.get(&self.default_tenant, adapter_id).await
            .map_err(|e| AosError::Database(format!("Failed to get adapter: {}", e)))?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        // Update tier
        adapter_kv.tier = tier.to_string();
        adapter_kv.updated_at = Utc::now();

        // Save
        self.repo.update(adapter_kv).await
            .map_err(|e| AosError::Database(format!("Failed to update adapter tier: {}", e)))?;

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
        };

        // Convert to KV
        let kv_adapter: AdapterKv = sql_adapter.clone().into();

        // Verify key fields
        assert_eq!(kv_adapter.id, "adapter-1");
        assert_eq!(kv_adapter.name, "Test Adapter");
        assert_eq!(kv_adapter.tier, "warm");
        assert_eq!(kv_adapter.targets, vec!["q_proj", "v_proj"]);
        assert!(kv_adapter.active);
        assert!(!kv_adapter.pinned);

        // Convert back to SQL
        let sql_adapter_2: Adapter = kv_adapter.into();

        // Verify round-trip conversion
        assert_eq!(sql_adapter_2.id, sql_adapter.id);
        assert_eq!(sql_adapter_2.name, sql_adapter.name);
        assert_eq!(sql_adapter_2.tier, sql_adapter.tier);
    }
}
