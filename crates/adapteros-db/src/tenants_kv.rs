//! Tenant KV operations
//!
//! This module provides KV-based operations for tenant management,
//! enabling dual-write and eventual migration from SQL to KV storage.

use adapteros_core::{AosError, Result};
use adapteros_storage::entities::tenant::TenantKv;
use adapteros_storage::KvBackend;
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

use crate::tenants::Tenant;

/// Tenant operations trait for KV backend
#[async_trait]
pub trait TenantKvOps {
    /// Create a new tenant
    async fn create_tenant_kv(&self, params: &CreateTenantParams) -> Result<String>;
    /// Create a tenant with a specific ID (used for system tenant bootstrap)
    async fn create_tenant_kv_with_id(
        &self,
        id: &str,
        params: &CreateTenantParams,
    ) -> Result<String>;

    /// Get a tenant by ID
    async fn get_tenant_kv(&self, id: &str) -> Result<Option<TenantKv>>;

    /// List all tenants
    async fn list_tenants_kv(&self) -> Result<Vec<TenantKv>>;

    /// List tenants with pagination
    async fn list_tenants_paginated_kv(
        &self,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<(Vec<TenantKv>, Option<String>)>;

    /// Update tenant name
    async fn rename_tenant_kv(&self, id: &str, new_name: &str) -> Result<()>;

    /// Update tenant ITAR flag
    async fn update_tenant_itar_flag_kv(&self, id: &str, itar_flag: bool) -> Result<()>;

    /// Pause a tenant
    async fn pause_tenant_kv(&self, id: &str) -> Result<()>;

    /// Archive a tenant
    async fn archive_tenant_kv(&self, id: &str) -> Result<()>;

    /// Reactivate a tenant
    async fn activate_tenant_kv(&self, id: &str) -> Result<()>;

    /// Update tenant resource limits
    async fn update_tenant_limits_kv(
        &self,
        id: &str,
        max_adapters: Option<i32>,
        max_training_jobs: Option<i32>,
        max_storage_gb: Option<f64>,
        rate_limit_rpm: Option<i32>,
    ) -> Result<()>;

    /// Set default stack for a tenant
    async fn set_default_stack_kv(&self, tenant_id: &str, stack_id: &str) -> Result<()>;

    /// Clear default stack for a tenant
    async fn clear_default_stack_kv(&self, tenant_id: &str) -> Result<()>;
}

/// Parameters for creating a new tenant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTenantParams {
    pub name: String,
    pub itar_flag: bool,
}

/// KV backend implementation for tenant operations
pub struct TenantKvRepository {
    backend: Arc<dyn KvBackend>,
}

impl TenantKvRepository {
    /// Create a new tenant KV repository
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    /// Build primary key for a tenant
    fn primary_key(id: &str) -> String {
        format!("tenant:{}", id)
    }

    /// Build secondary index key for tenant by name
    fn name_index_key(name: &str) -> String {
        format!("tenant-by-name:{}", name)
    }

    /// Idempotent upsert used by migration/repair paths.
    pub async fn put_tenant(&self, tenant: &TenantKv) -> Result<()> {
        let existing = self.load_tenant(&tenant.id).await?;
        self.store_tenant(tenant).await?;
        self.update_indexes(tenant, existing.as_ref()).await
    }

    /// Build secondary index key for tenants by status
    fn status_index_key(status: &str, id: &str) -> String {
        format!("tenant-by-status:{}:{}", status, id)
    }

    /// Store a tenant in KV
    async fn store_tenant(&self, tenant: &TenantKv) -> Result<()> {
        let key = Self::primary_key(&tenant.id);
        let value = bincode::serialize(tenant)
            .map_err(|e| AosError::Database(format!("Failed to serialize tenant: {}", e)))?;

        self.backend
            .set(&key, value)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store tenant: {}", e)))?;

        // Update secondary indexes
        self.update_indexes(tenant, None).await?;

        Ok(())
    }

    /// Load a tenant from KV
    async fn load_tenant(&self, id: &str) -> Result<Option<TenantKv>> {
        let key = Self::primary_key(id);

        let bytes = match self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get tenant: {}", e)))?
        {
            Some(b) => b,
            None => return Ok(None),
        };

        let tenant: TenantKv = bincode::deserialize(&bytes)
            .map_err(|e| AosError::Database(format!("Failed to deserialize tenant: {}", e)))?;

        Ok(Some(tenant))
    }

    /// Update secondary indexes
    async fn update_indexes(&self, tenant: &TenantKv, old_tenant: Option<&TenantKv>) -> Result<()> {
        // Name index (only on create, names should be unique)
        if old_tenant.is_none() {
            let name_key = Self::name_index_key(&tenant.name);
            self.backend
                .set(&name_key, tenant.id.as_bytes().to_vec())
                .await
                .map_err(|e| AosError::Database(format!("Failed to update name index: {}", e)))?;
        } else if let Some(old) = old_tenant {
            // If name changed, update index
            if old.name != tenant.name {
                // Remove old name index
                let old_name_key = Self::name_index_key(&old.name);
                self.backend.delete(&old_name_key).await.map_err(|e| {
                    AosError::Database(format!("Failed to delete old name index: {}", e))
                })?;

                // Add new name index
                let new_name_key = Self::name_index_key(&tenant.name);
                self.backend
                    .set(&new_name_key, tenant.id.as_bytes().to_vec())
                    .await
                    .map_err(|e| {
                        AosError::Database(format!("Failed to update name index: {}", e))
                    })?;
            }
        }

        // Status index
        if let Some(old) = old_tenant {
            if old.status != tenant.status {
                // Remove from old status index
                let old_status_key = Self::status_index_key(&old.status, &tenant.id);
                self.backend.delete(&old_status_key).await.map_err(|e| {
                    AosError::Database(format!("Failed to delete old status index: {}", e))
                })?;
            }
        }

        // Add to current status index
        let status_key = Self::status_index_key(&tenant.status, &tenant.id);
        self.backend
            .set(&status_key, tenant.id.as_bytes().to_vec())
            .await
            .map_err(|e| AosError::Database(format!("Failed to update status index: {}", e)))?;

        Ok(())
    }

    /// Remove from indexes (used when deleting tenants)
    #[allow(dead_code)]
    async fn remove_from_indexes(&self, tenant: &TenantKv) -> Result<()> {
        // Remove name index
        let name_key = Self::name_index_key(&tenant.name);
        self.backend
            .delete(&name_key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete name index: {}", e)))?;

        // Remove status index
        let status_key = Self::status_index_key(&tenant.status, &tenant.id);
        self.backend
            .delete(&status_key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete status index: {}", e)))?;

        Ok(())
    }

    /// Load multiple tenants by scanning prefix
    async fn load_all_tenants(&self) -> Result<Vec<TenantKv>> {
        let keys = self
            .backend
            .scan_prefix("tenant:")
            .await
            .map_err(|e| AosError::Database(format!("Failed to scan tenants: {}", e)))?;

        let mut tenants = Vec::new();
        for key in keys {
            // Skip index keys
            if key.contains("by-name") || key.contains("by-status") {
                continue;
            }

            let bytes = match self
                .backend
                .get(&key)
                .await
                .map_err(|e| AosError::Database(format!("Failed to get tenant: {}", e)))?
            {
                Some(b) => b,
                None => continue,
            };

            match bincode::deserialize::<TenantKv>(&bytes) {
                Ok(tenant) => tenants.push(tenant),
                Err(e) => {
                    warn!(key = %key, error = %e, "Failed to deserialize tenant");
                }
            }
        }

        // Sort by created_at descending (most recent first)
        tenants.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(tenants)
    }
}

#[async_trait]
impl TenantKvOps for TenantKvRepository {
    async fn create_tenant_kv(&self, params: &CreateTenantParams) -> Result<String> {
        let id = uuid::Uuid::now_v7().to_string();

        self.create_tenant_kv_with_id(&id, params).await?;
        Ok(id)
    }

    async fn create_tenant_kv_with_id(
        &self,
        id: &str,
        params: &CreateTenantParams,
    ) -> Result<String> {
        let tenant = TenantKv {
            id: id.to_string(),
            name: params.name.clone(),
            itar_flag: params.itar_flag,
            status: "active".to_string(),
            default_stack_id: None,
            default_pinned_adapter_ids: None,
            max_adapters: None,
            max_training_jobs: None,
            max_storage_gb: None,
            rate_limit_rpm: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        self.store_tenant(&tenant).await?;

        info!(tenant_id = %id, name = %params.name, "Tenant created in KV");
        Ok(id.to_string())
    }

    async fn get_tenant_kv(&self, id: &str) -> Result<Option<TenantKv>> {
        self.load_tenant(id).await
    }

    async fn list_tenants_kv(&self) -> Result<Vec<TenantKv>> {
        self.load_all_tenants().await
    }

    async fn list_tenants_paginated_kv(
        &self,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<(Vec<TenantKv>, Option<String>)> {
        let all_tenants = self.load_all_tenants().await?;

        // Find cursor position
        let start_index = if let Some(cursor_id) = cursor {
            all_tenants
                .iter()
                .position(|t| t.id == cursor_id)
                .map(|pos| pos + 1)
                .unwrap_or(0)
        } else {
            0
        };

        // Get page
        let page_tenants: Vec<TenantKv> = all_tenants
            .into_iter()
            .skip(start_index)
            .take(limit)
            .collect();

        let next_cursor = if page_tenants.len() == limit {
            page_tenants.last().map(|t| t.id.clone())
        } else {
            None
        };

        Ok((page_tenants, next_cursor))
    }

    async fn rename_tenant_kv(&self, id: &str, new_name: &str) -> Result<()> {
        let mut tenant = self
            .load_tenant(id)
            .await?
            .ok_or_else(|| AosError::Database(format!("Tenant not found: {}", id)))?;

        let old_tenant = tenant.clone();
        tenant.name = new_name.to_string();
        tenant.updated_at = Utc::now();

        self.store_tenant(&tenant).await?;
        self.update_indexes(&tenant, Some(&old_tenant)).await?;

        info!(tenant_id = %id, new_name = %new_name, "Tenant renamed in KV");
        Ok(())
    }

    async fn update_tenant_itar_flag_kv(&self, id: &str, itar_flag: bool) -> Result<()> {
        let mut tenant = self
            .load_tenant(id)
            .await?
            .ok_or_else(|| AosError::Database(format!("Tenant not found: {}", id)))?;

        tenant.itar_flag = itar_flag;
        tenant.updated_at = Utc::now();

        self.store_tenant(&tenant).await?;

        info!(tenant_id = %id, itar_flag = %itar_flag, "Tenant ITAR flag updated in KV");
        Ok(())
    }

    async fn pause_tenant_kv(&self, id: &str) -> Result<()> {
        let mut tenant = self
            .load_tenant(id)
            .await?
            .ok_or_else(|| AosError::Database(format!("Tenant not found: {}", id)))?;

        let old_tenant = tenant.clone();
        tenant.status = "paused".to_string();
        tenant.updated_at = Utc::now();

        self.store_tenant(&tenant).await?;
        self.update_indexes(&tenant, Some(&old_tenant)).await?;

        info!(tenant_id = %id, "Tenant paused in KV");
        Ok(())
    }

    async fn archive_tenant_kv(&self, id: &str) -> Result<()> {
        let mut tenant = self
            .load_tenant(id)
            .await?
            .ok_or_else(|| AosError::Database(format!("Tenant not found: {}", id)))?;

        let old_tenant = tenant.clone();
        tenant.status = "archived".to_string();
        tenant.updated_at = Utc::now();

        self.store_tenant(&tenant).await?;
        self.update_indexes(&tenant, Some(&old_tenant)).await?;

        info!(tenant_id = %id, "Tenant archived in KV");
        Ok(())
    }

    async fn activate_tenant_kv(&self, id: &str) -> Result<()> {
        let mut tenant = self
            .load_tenant(id)
            .await?
            .ok_or_else(|| AosError::Database(format!("Tenant not found: {}", id)))?;

        let old_tenant = tenant.clone();
        tenant.status = "active".to_string();
        tenant.updated_at = Utc::now();

        self.store_tenant(&tenant).await?;
        self.update_indexes(&tenant, Some(&old_tenant)).await?;

        info!(tenant_id = %id, "Tenant activated in KV");
        Ok(())
    }

    async fn update_tenant_limits_kv(
        &self,
        id: &str,
        max_adapters: Option<i32>,
        max_training_jobs: Option<i32>,
        max_storage_gb: Option<f64>,
        rate_limit_rpm: Option<i32>,
    ) -> Result<()> {
        let mut tenant = self
            .load_tenant(id)
            .await?
            .ok_or_else(|| AosError::Database(format!("Tenant not found: {}", id)))?;

        tenant.max_adapters = max_adapters;
        tenant.max_training_jobs = max_training_jobs;
        tenant.max_storage_gb = max_storage_gb;
        tenant.rate_limit_rpm = rate_limit_rpm;
        tenant.updated_at = Utc::now();

        self.store_tenant(&tenant).await?;

        info!(tenant_id = %id, "Tenant limits updated in KV");
        Ok(())
    }

    async fn set_default_stack_kv(&self, tenant_id: &str, stack_id: &str) -> Result<()> {
        let mut tenant = self
            .load_tenant(tenant_id)
            .await?
            .ok_or_else(|| AosError::Database(format!("Tenant not found: {}", tenant_id)))?;

        tenant.default_stack_id = Some(stack_id.to_string());
        tenant.updated_at = Utc::now();

        self.store_tenant(&tenant).await?;

        info!(tenant_id = %tenant_id, stack_id = %stack_id, "Default stack set in KV");
        Ok(())
    }

    async fn clear_default_stack_kv(&self, tenant_id: &str) -> Result<()> {
        let mut tenant = self
            .load_tenant(tenant_id)
            .await?
            .ok_or_else(|| AosError::Database(format!("Tenant not found: {}", tenant_id)))?;

        tenant.default_stack_id = None;
        tenant.updated_at = Utc::now();

        self.store_tenant(&tenant).await?;

        info!(tenant_id = %tenant_id, "Default stack cleared in KV");
        Ok(())
    }
}

// ============================================================================
// Conversion Functions
// ============================================================================

/// Convert SQL Tenant to KV TenantKv
impl From<Tenant> for TenantKv {
    fn from(sql_tenant: Tenant) -> Self {
        use chrono::DateTime;

        // Parse timestamps, default to now if parsing fails
        let created_at = DateTime::parse_from_rfc3339(&sql_tenant.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let updated_at = sql_tenant
            .updated_at
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        Self {
            id: sql_tenant.id,
            name: sql_tenant.name,
            itar_flag: sql_tenant.itar_flag,
            status: sql_tenant.status.unwrap_or_else(|| "active".to_string()),
            default_stack_id: sql_tenant.default_stack_id,
            default_pinned_adapter_ids: sql_tenant.default_pinned_adapter_ids,
            max_adapters: sql_tenant.max_adapters,
            max_training_jobs: sql_tenant.max_training_jobs,
            max_storage_gb: sql_tenant.max_storage_gb,
            rate_limit_rpm: sql_tenant.rate_limit_rpm,
            created_at,
            updated_at,
        }
    }
}

/// Helper function to create TenantKv from CreateTenantParams (used in tests)
#[allow(dead_code)]
fn tenant_kv_from_params(params: &CreateTenantParams, id: &str) -> TenantKv {
    let now = Utc::now();
    TenantKv {
        id: id.to_string(),
        name: params.name.clone(),
        itar_flag: params.itar_flag,
        status: "active".to_string(),
        default_stack_id: None,
        default_pinned_adapter_ids: None,
        max_adapters: None,
        max_training_jobs: None,
        max_storage_gb: None,
        rate_limit_rpm: None,
        created_at: now,
        updated_at: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_storage::redb::RedbBackend;
    use std::sync::Arc;

    #[test]
    fn test_sql_to_kv_conversion() {
        let sql_tenant = Tenant {
            id: "tenant-1".to_string(),
            name: "Test Tenant".to_string(),
            itar_flag: true,
            created_at: "2025-01-01T00:00:00Z".to_string(),
            status: Some("active".to_string()),
            updated_at: Some("2025-01-02T00:00:00Z".to_string()),
            default_stack_id: Some("stack-1".to_string()),
            default_pinned_adapter_ids: None,
            max_adapters: Some(100),
            max_training_jobs: Some(10),
            max_storage_gb: Some(500.0),
            rate_limit_rpm: Some(1000),
            max_kv_cache_bytes: None,
            kv_residency_policy_id: None,
        };

        let kv_tenant: TenantKv = sql_tenant.clone().into();

        assert_eq!(kv_tenant.id, sql_tenant.id);
        assert_eq!(kv_tenant.name, sql_tenant.name);
        assert_eq!(kv_tenant.itar_flag, sql_tenant.itar_flag);
        assert_eq!(kv_tenant.status, "active");
        assert_eq!(kv_tenant.default_stack_id, sql_tenant.default_stack_id);
        assert_eq!(kv_tenant.max_adapters, sql_tenant.max_adapters);
    }

    #[test]
    fn test_kv_to_sql_conversion() {
        let kv_tenant = TenantKv {
            id: "tenant-1".to_string(),
            name: "Test Tenant".to_string(),
            itar_flag: false,
            status: "paused".to_string(),
            default_stack_id: None,
            default_pinned_adapter_ids: None,
            max_adapters: Some(50),
            max_training_jobs: Some(5),
            max_storage_gb: Some(100.0),
            rate_limit_rpm: Some(500),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let sql_tenant: Tenant = kv_tenant.clone().into();

        assert_eq!(sql_tenant.id, kv_tenant.id);
        assert_eq!(sql_tenant.name, kv_tenant.name);
        assert_eq!(sql_tenant.itar_flag, kv_tenant.itar_flag);
        assert_eq!(sql_tenant.status, Some("paused".to_string()));
        assert_eq!(sql_tenant.max_adapters, kv_tenant.max_adapters);
    }

    #[test]
    fn test_from_params() {
        let params = CreateTenantParams {
            name: "New Tenant".to_string(),
            itar_flag: true,
        };

        let tenant = tenant_kv_from_params(&params, "test-id");

        assert_eq!(tenant.id, "test-id");
        assert_eq!(tenant.name, "New Tenant");
        assert_eq!(tenant.itar_flag, true);
        assert_eq!(tenant.status, "active");
        assert!(tenant.default_stack_id.is_none());
    }

    #[tokio::test]
    async fn test_create_and_list_tenants_kv() {
        let backend = Arc::new(adapteros_storage::redb::RedbBackend::open_in_memory().unwrap());
        let repo = TenantKvRepository::new(backend);

        let params = CreateTenantParams {
            name: "TenantA".into(),
            itar_flag: false,
        };

        let tenant_id = repo
            .create_tenant_kv(&params)
            .await
            .expect("kv write should succeed");
        let tenants = repo
            .list_tenants_kv()
            .await
            .expect("kv list should succeed");

        assert_eq!(tenants.len(), 1);
        assert_eq!(tenants[0].id, tenant_id);
        assert_eq!(tenants[0].name, "TenantA");
    }
}
