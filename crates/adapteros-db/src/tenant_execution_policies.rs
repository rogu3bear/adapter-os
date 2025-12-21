//! Tenant execution policies for determinism and routing policy enforcement
//!
//! Provides hierarchical policy management:
//! - Determinism policy: allowed modes, seed requirements, fallback behavior
//! - Routing policy: allowed stacks/adapters, pin enforcement
//! - Golden policy: golden-run verification configuration
//!
//! Uses default permissive approach: tenants without explicit policy get
//! a permissive default allowing all modes and no restrictions.

use crate::query_helpers::db_err;
use crate::Db;
use adapteros_api_types::{
    CreateExecutionPolicyRequest, DeterminismPolicy, GoldenPolicy, RoutingPolicy,
    TenantExecutionPolicy,
};
use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Database row for tenant_execution_policies table
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExecutionPolicyRow {
    pub id: String,
    pub tenant_id: String,
    pub version: i64,
    pub determinism_policy_json: String,
    pub routing_policy_json: Option<String>,
    pub golden_policy_json: Option<String>,
    pub require_signed_adapters: Option<i32>,
    pub active: i32,
    pub created_at: String,
    pub updated_at: String,
    pub created_by: Option<String>,
}

impl ExecutionPolicyRow {
    /// Convert database row to API type
    pub fn into_policy(self) -> Result<TenantExecutionPolicy> {
        let determinism: DeterminismPolicy = serde_json::from_str(&self.determinism_policy_json)
            .map_err(|e| {
                adapteros_core::AosError::Validation(format!(
                    "Failed to parse determinism policy: {}",
                    e
                ))
            })?;

        let routing: Option<RoutingPolicy> = self
            .routing_policy_json
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(|e| {
                adapteros_core::AosError::Validation(format!(
                    "Failed to parse routing policy: {}",
                    e
                ))
            })?;

        let golden: Option<GoldenPolicy> = self
            .golden_policy_json
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(|e| {
                adapteros_core::AosError::Validation(format!(
                    "Failed to parse golden policy: {}",
                    e
                ))
            })?;

        Ok(TenantExecutionPolicy {
            id: self.id,
            tenant_id: self.tenant_id,
            version: self.version,
            determinism,
            routing,
            golden,
            require_signed_adapters: self.require_signed_adapters.unwrap_or(0) != 0,
            active: self.active != 0,
            is_implicit: false,
            created_at: Some(self.created_at),
            updated_at: Some(self.updated_at),
            created_by: self.created_by,
        })
    }
}

impl Db {
    /// Get the active execution policy for a tenant.
    ///
    /// Returns None if no explicit policy is configured.
    /// Callers should use `get_execution_policy_or_default()` for inference.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    ///
    /// # Returns
    /// The active policy if one exists, None otherwise
    pub async fn get_execution_policy(
        &self,
        tenant_id: &str,
    ) -> Result<Option<TenantExecutionPolicy>> {
        debug!(tenant_id = %tenant_id, "Getting execution policy");

        let result = sqlx::query_as::<_, ExecutionPolicyRow>(
            r#"
            SELECT id, tenant_id, version, determinism_policy_json, routing_policy_json,
                   golden_policy_json, require_signed_adapters, active, created_at, updated_at, created_by
            FROM tenant_execution_policies
            WHERE tenant_id = ? AND active = 1
            "#,
        )
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get execution policy"))?;

        match result {
            Some(row) => Ok(Some(row.into_policy()?)),
            None => Ok(None),
        }
    }

    /// Get execution policy for a tenant, falling back to permissive default.
    ///
    /// This is the primary method for inference - always returns a policy.
    /// If no explicit policy exists, returns a permissive default that allows
    /// all determinism modes and has no routing restrictions.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    ///
    /// # Returns
    /// The active policy or a permissive default
    pub async fn get_execution_policy_or_default(
        &self,
        tenant_id: &str,
    ) -> Result<TenantExecutionPolicy> {
        match self.get_execution_policy(tenant_id).await? {
            Some(policy) => Ok(policy),
            None => {
                debug!(
                    tenant_id = %tenant_id,
                    "No explicit execution policy, using permissive default"
                );
                Ok(TenantExecutionPolicy::permissive_default(tenant_id))
            }
        }
    }

    /// Create a new execution policy for a tenant.
    ///
    /// If an active policy already exists, it will be deactivated and a new
    /// one created with incremented version number.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    /// * `request` - The policy configuration
    /// * `created_by` - Optional user who created the policy
    ///
    /// # Returns
    /// The ID of the newly created policy
    pub async fn create_execution_policy(
        &self,
        tenant_id: &str,
        request: CreateExecutionPolicyRequest,
        created_by: Option<&str>,
    ) -> Result<String> {
        debug!(tenant_id = %tenant_id, "Creating execution policy");

        let id = uuid::Uuid::new_v4().to_string();

        // Get the next version number
        let version = self.get_next_policy_version(tenant_id).await?;

        // Serialize policy components to JSON
        let determinism_json = serde_json::to_string(&request.determinism).map_err(|e| {
            adapteros_core::AosError::Validation(format!(
                "Failed to serialize determinism policy: {}",
                e
            ))
        })?;

        let routing_json = request
            .routing
            .map(|r| serde_json::to_string(&r))
            .transpose()
            .map_err(|e| {
                adapteros_core::AosError::Validation(format!(
                    "Failed to serialize routing policy: {}",
                    e
                ))
            })?;

        let golden_json = request
            .golden
            .map(|g| serde_json::to_string(&g))
            .transpose()
            .map_err(|e| {
                adapteros_core::AosError::Validation(format!(
                    "Failed to serialize golden policy: {}",
                    e
                ))
            })?;

        // Deactivate any existing active policy
        sqlx::query(
            r#"
            UPDATE tenant_execution_policies
            SET active = 0, updated_at = datetime('now')
            WHERE tenant_id = ? AND active = 1
            "#,
        )
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(db_err("deactivate existing policy"))?;

        // Insert new policy
        sqlx::query(
            r#"
            INSERT INTO tenant_execution_policies (
                id, tenant_id, version, determinism_policy_json,
                routing_policy_json, golden_policy_json, require_signed_adapters, active, created_by
            ) VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?)
            "#,
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(version)
        .bind(&determinism_json)
        .bind(&routing_json)
        .bind(&golden_json)
        .bind(if request.require_signed_adapters {
            1i32
        } else {
            0i32
        })
        .bind(created_by)
        .execute(self.pool())
        .await
        .map_err(db_err("create execution policy"))?;

        info!(
            tenant_id = %tenant_id,
            policy_id = %id,
            version = version,
            "Execution policy created"
        );

        Ok(id)
    }

    /// Update an existing execution policy.
    ///
    /// Creates a new version of the policy while deactivating the old one.
    /// This ensures audit trail is preserved.
    ///
    /// # Arguments
    /// * `policy_id` - The ID of the policy to update
    /// * `request` - The new policy configuration
    ///
    /// # Returns
    /// The ID of the newly created policy version
    pub async fn update_execution_policy(
        &self,
        policy_id: &str,
        request: CreateExecutionPolicyRequest,
    ) -> Result<String> {
        debug!(policy_id = %policy_id, "Updating execution policy");

        // Get existing policy to find tenant_id and created_by
        let existing = sqlx::query_as::<_, ExecutionPolicyRow>(
            r#"
            SELECT id, tenant_id, version, determinism_policy_json, routing_policy_json,
                   golden_policy_json, require_signed_adapters, active, created_at, updated_at, created_by
            FROM tenant_execution_policies
            WHERE id = ?
            "#,
        )
        .bind(policy_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get policy for update"))?
        .ok_or_else(|| {
            adapteros_core::AosError::NotFound(format!("Policy not found: {}", policy_id))
        })?;

        // Create new version
        self.create_execution_policy(&existing.tenant_id, request, existing.created_by.as_deref())
            .await
    }

    /// Deactivate an execution policy.
    ///
    /// # Arguments
    /// * `policy_id` - The ID of the policy to deactivate
    ///
    /// # Returns
    /// true if the policy was deactivated, false if not found
    pub async fn deactivate_execution_policy(&self, policy_id: &str) -> Result<bool> {
        debug!(policy_id = %policy_id, "Deactivating execution policy");

        let result = sqlx::query(
            r#"
            UPDATE tenant_execution_policies
            SET active = 0, updated_at = datetime('now')
            WHERE id = ? AND active = 1
            "#,
        )
        .bind(policy_id)
        .execute(self.pool())
        .await
        .map_err(db_err("deactivate execution policy"))?;

        let deactivated = result.rows_affected() > 0;
        if deactivated {
            info!(policy_id = %policy_id, "Execution policy deactivated");
        } else {
            warn!(policy_id = %policy_id, "Policy not found or already inactive");
        }
        Ok(deactivated)
    }

    /// Get policy history for a tenant (all versions, ordered by version desc).
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    /// * `limit` - Maximum number of policies to return
    ///
    /// # Returns
    /// List of policies, most recent first
    pub async fn get_execution_policy_history(
        &self,
        tenant_id: &str,
        limit: i64,
    ) -> Result<Vec<TenantExecutionPolicy>> {
        debug!(tenant_id = %tenant_id, limit = limit, "Getting execution policy history");

        let rows = sqlx::query_as::<_, ExecutionPolicyRow>(
            r#"
            SELECT id, tenant_id, version, determinism_policy_json, routing_policy_json,
                   golden_policy_json, require_signed_adapters, active, created_at, updated_at, created_by
            FROM tenant_execution_policies
            WHERE tenant_id = ?
            ORDER BY version DESC
            LIMIT ?
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("get execution policy history"))?;

        rows.into_iter().map(|row| row.into_policy()).collect()
    }

    /// Get the next version number for a tenant's policy.
    async fn get_next_policy_version(&self, tenant_id: &str) -> Result<i64> {
        let max_version = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(version), 0) FROM tenant_execution_policies WHERE tenant_id = ?",
        )
        .bind(tenant_id)
        .fetch_one(self.pool())
        .await
        .map_err(db_err("get max policy version"))?;

        Ok(max_version + 1)
    }

    /// Delete all execution policies for a tenant.
    ///
    /// Used for tenant cleanup. In production, prefer deactivation for audit trail.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    ///
    /// # Returns
    /// Number of policies deleted
    pub async fn delete_execution_policies_for_tenant(&self, tenant_id: &str) -> Result<u64> {
        debug!(tenant_id = %tenant_id, "Deleting all execution policies");

        let result = sqlx::query("DELETE FROM tenant_execution_policies WHERE tenant_id = ?")
            .bind(tenant_id)
            .execute(self.pool())
            .await
            .map_err(db_err("delete execution policies"))?;

        let deleted = result.rows_affected();
        if deleted > 0 {
            info!(tenant_id = %tenant_id, count = deleted, "Execution policies deleted");
        }
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permissive_default_policy() {
        let policy = TenantExecutionPolicy::permissive_default("test-tenant");
        assert_eq!(policy.tenant_id, "test-tenant");
        assert!(policy.is_implicit);
        assert_eq!(policy.version, 0);
        assert_eq!(
            policy.determinism.allowed_modes,
            vec!["strict", "besteffort", "relaxed"]
        );
        assert_eq!(policy.determinism.default_mode, "besteffort");
        assert!(policy.routing.is_none());
        assert!(policy.golden.is_none());
    }

    #[test]
    fn test_determinism_policy_default() {
        let policy = DeterminismPolicy::default();
        assert_eq!(
            policy.allowed_modes,
            vec!["strict", "besteffort", "relaxed"]
        );
        assert_eq!(policy.default_mode, "besteffort");
        assert!(!policy.require_seed);
        assert!(policy.allow_fallback);
        assert_eq!(policy.replay_mode, "approximate");
    }

    #[test]
    fn test_routing_policy_default() {
        let policy = RoutingPolicy::default();
        assert!(policy.allowed_stack_ids.is_none());
        assert!(policy.allowed_adapter_ids.is_none());
        assert_eq!(policy.pin_enforcement, "warn");
        assert!(!policy.require_stack);
        assert!(!policy.require_pins);
    }

    #[test]
    fn test_golden_policy_default() {
        let policy = GoldenPolicy::default();
        assert!(!policy.fail_on_drift);
        assert!(policy.golden_baseline_id.is_none());
        assert!((policy.epsilon_threshold - 1e-6).abs() < 1e-12);
    }
}
