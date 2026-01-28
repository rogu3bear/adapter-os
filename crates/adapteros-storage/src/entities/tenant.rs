//! Tenant entity KV schema
//!
//! This module defines the canonical tenant entity for key-value storage,
//! replacing the SQL `tenants` table.

use adapteros_types::tenants::Tenant;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Canonical tenant entity for KV storage
///
/// This struct represents the authoritative schema for tenant entities in the
/// key-value storage backend. It includes all fields from the SQL `tenants` table
/// with proper type conversions.
///
/// **Key Design:**
/// - Primary key: `tenant/{id}`
/// - Secondary indexes:
///   - `tenant-by-name/{name}` -> `{id}`
///   - `tenants-by-status/{status}` -> Set<{id}>
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TenantKv {
    // Core identity
    pub id: String,
    pub name: String,

    // Security
    pub itar_flag: bool,

    // Status
    pub status: String, // active | suspended | archived

    // Configuration
    pub default_stack_id: Option<String>,
    /// Default pinned adapter IDs for new chat sessions (JSON array)
    pub default_pinned_adapter_ids: Option<String>,

    // Quotas and limits
    pub max_adapters: Option<i32>,
    pub max_training_jobs: Option<i32>,
    pub max_storage_gb: Option<f64>,
    pub rate_limit_rpm: Option<i32>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TenantKv {
    /// Check if the tenant is active
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }

    /// Check if the tenant is suspended
    pub fn is_suspended(&self) -> bool {
        self.status == "suspended"
    }

    /// Check if the tenant is archived
    pub fn is_archived(&self) -> bool {
        self.status == "archived"
    }

    /// Check if a quota is exceeded
    pub fn is_over_quota(&self, current: i32, limit: Option<i32>) -> bool {
        if let Some(max) = limit {
            current >= max
        } else {
            false
        }
    }
}

/// Convert from SQL Tenant to KV TenantKv
impl From<Tenant> for TenantKv {
    fn from(tenant: Tenant) -> Self {
        TenantKv {
            id: tenant.id,
            name: tenant.name,
            itar_flag: tenant.itar_flag,
            created_at: chrono::DateTime::parse_from_rfc3339(&tenant.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            status: tenant.status.unwrap_or_else(|| "active".to_string()),
            updated_at: tenant
                .updated_at
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
            default_stack_id: tenant.default_stack_id,
            max_adapters: tenant.max_adapters,
            max_training_jobs: tenant.max_training_jobs,
            max_storage_gb: tenant.max_storage_gb,
            rate_limit_rpm: tenant.rate_limit_rpm,
            default_pinned_adapter_ids: tenant.default_pinned_adapter_ids,
        }
    }
}

/// Convert from KV TenantKv to SQL Tenant
impl From<TenantKv> for Tenant {
    fn from(kv: TenantKv) -> Self {
        Tenant {
            id: kv.id,
            name: kv.name,
            itar_flag: kv.itar_flag,
            created_at: kv.created_at.to_rfc3339(),
            status: Some(kv.status),
            updated_at: Some(kv.updated_at.to_rfc3339()),
            default_stack_id: kv.default_stack_id,
            max_adapters: kv.max_adapters,
            max_training_jobs: kv.max_training_jobs,
            max_storage_gb: kv.max_storage_gb,
            rate_limit_rpm: kv.rate_limit_rpm,
            default_pinned_adapter_ids: kv.default_pinned_adapter_ids,
            // KV quota fields - default to None for KV backend (not yet supported in KV)
            max_kv_cache_bytes: None,
            kv_residency_policy_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_status_checks() {
        let tenant = TenantKv {
            id: "tenant-1".to_string(),
            name: "Test Tenant".to_string(),
            itar_flag: false,
            status: "active".to_string(),
            default_stack_id: None,
            default_pinned_adapter_ids: None,
            max_adapters: Some(100),
            max_training_jobs: Some(10),
            max_storage_gb: Some(500.0),
            rate_limit_rpm: Some(1000),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert!(tenant.is_active());
        assert!(!tenant.is_suspended());
        assert!(!tenant.is_archived());
    }

    #[test]
    fn test_tenant_quota_check() {
        let tenant = TenantKv {
            id: "tenant-1".to_string(),
            name: "Test Tenant".to_string(),
            itar_flag: false,
            status: "active".to_string(),
            default_stack_id: None,
            default_pinned_adapter_ids: None,
            max_adapters: Some(100),
            max_training_jobs: Some(10),
            max_storage_gb: Some(500.0),
            rate_limit_rpm: Some(1000),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Under quota
        assert!(!tenant.is_over_quota(50, tenant.max_adapters));

        // At quota
        assert!(tenant.is_over_quota(100, tenant.max_adapters));

        // Over quota
        assert!(tenant.is_over_quota(150, tenant.max_adapters));

        // No quota set
        assert!(!tenant.is_over_quota(1000, None));
    }
}
