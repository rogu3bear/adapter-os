//! Tenant Policy Bindings - Database operations
//!
//! Manages per-tenant policy pack enable/disable state with full audit trail.

#![allow(clippy::borrow_deref_ref)]

use crate::policy_audit::is_audit_chain_divergence;
use crate::tenant_policy_bindings_kv::{
    kv_to_binding, PolicyBindingKvRepository, TenantPolicyBindingKv,
};
use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::{debug, info};
use crate::new_id;
use adapteros_id::IdPrefix;

/// Tenant policy binding record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantPolicyBinding {
    pub id: String,
    pub tenant_id: String,
    pub policy_pack_id: String,
    pub scope: String,
    pub enabled: bool,
    pub created_at: String,
    pub created_by: String,
    pub updated_at: String,
    pub updated_by: Option<String>,
}

/// Core policies enabled by default for new tenants
pub const CORE_POLICIES: &[&str] = &["egress", "determinism", "isolation", "evidence"];

/// All 24 canonical policies from AGENTS.md
pub const ALL_POLICIES: &[&str] = &[
    "egress",
    "determinism",
    "router",
    "evidence",
    "refusal",
    "numeric",
    "rag",
    "isolation",
    "telemetry",
    "retention",
    "performance",
    "memory",
    "artifacts",
    "secrets",
    "build_release",
    "compliance",
    "incident",
    "output",
    "adapters",
    "deterministic_io",
    "drift",
    "mplora",
    "naming",
    "dependency_security",
];

impl Db {
    pub(crate) fn get_policy_binding_kv_repo(&self) -> Option<PolicyBindingKvRepository> {
        if self.storage_mode().write_to_kv() || self.storage_mode().read_from_kv() {
            self.kv_backend()
                .map(|kv| PolicyBindingKvRepository::new(kv.backend().clone()))
        } else {
            None
        }
    }

    /// Get active (enabled) policy pack IDs for a tenant
    ///
    /// Returns a list of policy_pack_id strings for all enabled policies.
    pub async fn get_active_policies_for_tenant(&self, tenant_id: &str) -> Result<Vec<String>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_policy_binding_kv_repo() {
                match repo.get_active_policy_ids(tenant_id).await {
                    Ok(ids) => {
                        // KV-only bootstrap: default to core policies if KV store is empty
                        if ids.is_empty() && self.storage_mode().is_kv_only() {
                            return Ok(CORE_POLICIES.iter().map(|p| p.to_string()).collect());
                        }
                        return Ok(ids);
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("tenant_policy_bindings.get_active.error");
                        debug!(error = %e, tenant_id = %tenant_id, "KV get_active_policies failed; falling back to SQL");
                    }
                    Err(e) => return Err(e),
                }
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(Vec::new());
            }
        }

        let rows = sqlx::query(
            r#"
            SELECT policy_pack_id
            FROM tenant_policy_bindings
            WHERE tenant_id = ? AND enabled = 1
            ORDER BY policy_pack_id ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to get active policies for tenant {}: {}",
                tenant_id, e
            ))
        })?;

        let policy_ids: Vec<String> = rows
            .iter()
            .map(|row| row.get::<String, _>("policy_pack_id"))
            .collect();

        debug!(
            tenant_id = %tenant_id,
            policy_count = policy_ids.len(),
            "Retrieved active policies for tenant"
        );

        Ok(policy_ids)
    }

    /// Toggle a policy pack on/off for a tenant
    ///
    /// Updates the `enabled` flag for the specified tenant/policy combination.
    /// Creates a new binding if one doesn't exist.
    /// **Also writes an audit record to policy_audit_decisions for compliance.**
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    /// * `policy_pack_id` - The policy pack ID (e.g., "egress", "determinism")
    /// * `enabled` - True to enable, false to disable
    /// * `updated_by` - User/system performing the update
    ///
    /// # Returns
    /// The previous enabled state (for audit purposes)
    pub async fn toggle_tenant_policy(
        &self,
        tenant_id: &str,
        policy_pack_id: &str,
        enabled: bool,
        updated_by: &str,
    ) -> Result<bool> {
        let now = chrono::Utc::now().to_rfc3339();

        // KV path
        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_policy_binding_kv_repo() {
                let previous = repo
                    .upsert_enabled(tenant_id, policy_pack_id, enabled, updated_by)
                    .await?;
                // Best-effort audit: if SQL available keep logging, otherwise warn
                if self.storage_mode().write_to_sql() && self.pool_opt().is_some() {
                    // fall through to SQL path to record audit row too
                } else {
                    info!(
                        tenant_id = %tenant_id,
                        policy_pack_id = %policy_pack_id,
                        previous_enabled = %previous,
                        new_enabled = %enabled,
                        updated_by = %updated_by,
                        "Toggled tenant policy binding (KV-only)"
                    );
                    return Ok(previous);
                }
            }
        }

        // Get the previous state for audit
        let previous_enabled = sqlx::query(
            r#"
            SELECT enabled
            FROM tenant_policy_bindings
            WHERE tenant_id = ? AND policy_pack_id = ? AND scope = 'global'
            "#,
        )
        .bind(tenant_id)
        .bind(policy_pack_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get current state: {}", e)))?
        .map(|row| row.get::<i32, _>("enabled") != 0)
        .unwrap_or(false);

        // Try to update existing binding first
        let result = sqlx::query(
            r#"
            UPDATE tenant_policy_bindings
            SET enabled = ?, updated_at = ?, updated_by = ?
            WHERE tenant_id = ? AND policy_pack_id = ? AND scope = 'global'
            "#,
        )
        .bind(if enabled { 1 } else { 0 })
        .bind(&now)
        .bind(updated_by)
        .bind(tenant_id)
        .bind(policy_pack_id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to update tenant policy binding: {}", e))
        })?;

        // If no rows were updated, insert a new binding
        if result.rows_affected() == 0 {
            let id = new_id(IdPrefix::Pol);
            sqlx::query(
                r#"
                INSERT INTO tenant_policy_bindings
                (id, tenant_id, policy_pack_id, scope, enabled, created_at, created_by, updated_at)
                VALUES (?, ?, ?, 'global', ?, ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(tenant_id)
            .bind(policy_pack_id)
            .bind(if enabled { 1 } else { 0 })
            .bind(&now)
            .bind(updated_by)
            .bind(&now)
            .execute(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to create tenant policy binding: {}", e))
            })?;
        }

        // Write audit record to policy_audit_decisions table
        // This creates a cryptographic audit trail via Merkle-chain
        let decision = if enabled { "allow" } else { "deny" };
        let reason = format!(
            "Policy {} {} by {} (was: {})",
            policy_pack_id,
            if enabled { "enabled" } else { "disabled" },
            updated_by,
            if previous_enabled {
                "enabled"
            } else {
                "disabled"
            }
        );

        if let Err(e) = self
            .log_policy_decision(
                tenant_id,
                policy_pack_id,
                "toggle", // hook = toggle for policy state changes
                decision,
                Some(&reason),
                None, // request_id
                Some(updated_by),
                Some("policy_binding"),
                Some(policy_pack_id),
                Some(
                    &serde_json::json!({
                        "previous_enabled": previous_enabled,
                        "new_enabled": enabled,
                        "action": "toggle"
                    })
                    .to_string(),
                ),
            )
            .await
        {
            if is_audit_chain_divergence(&e) {
                return Err(e);
            }
            // Log but don't fail for non-divergence cases - audit should not block operations
            tracing::error!(
                tenant_id = %tenant_id,
                policy_pack_id = %policy_pack_id,
                error = %e,
                "Failed to write policy toggle audit record"
            );
        }

        info!(
            tenant_id = %tenant_id,
            policy_pack_id = %policy_pack_id,
            previous_enabled = %previous_enabled,
            new_enabled = %enabled,
            updated_by = %updated_by,
            "Toggled tenant policy binding"
        );

        Ok(previous_enabled)
    }

    /// List all policy bindings for a tenant
    ///
    /// Returns complete list of all policy bindings (both enabled and disabled)
    /// for visibility and management purposes.
    pub async fn list_tenant_policy_bindings(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<TenantPolicyBinding>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_policy_binding_kv_repo() {
                match repo.list_bindings(tenant_id).await {
                    Ok(kv_bindings) => {
                        let bindings = kv_bindings.iter().map(kv_to_binding).collect();
                        return Ok(bindings);
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("tenant_policy_bindings.list.error");
                        debug!(error = %e, tenant_id = %tenant_id, "KV list bindings failed; falling back to SQL");
                    }
                    Err(e) => return Err(e),
                }
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(Vec::new());
            }
        }

        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, policy_pack_id, scope, enabled,
                   created_at, created_by, updated_at, updated_by
            FROM tenant_policy_bindings
            WHERE tenant_id = ?
            ORDER BY policy_pack_id ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list tenant policy bindings: {}", e)))?;

        let bindings: Vec<TenantPolicyBinding> = rows
            .iter()
            .map(|row| TenantPolicyBinding {
                id: row.get("id"),
                tenant_id: row.get("tenant_id"),
                policy_pack_id: row.get("policy_pack_id"),
                scope: row.get("scope"),
                enabled: row.get::<i32, _>("enabled") != 0,
                created_at: row.get("created_at"),
                created_by: row.get("created_by"),
                updated_at: row.get("updated_at"),
                updated_by: row.get("updated_by"),
            })
            .collect();

        debug!(
            tenant_id = %tenant_id,
            binding_count = bindings.len(),
            "Listed tenant policy bindings"
        );

        Ok(bindings)
    }

    /// Check if a specific policy is enabled for a tenant
    pub async fn is_policy_enabled_for_tenant(
        &self,
        tenant_id: &str,
        policy_pack_id: &str,
    ) -> Result<bool> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_policy_binding_kv_repo() {
                match repo.get_binding(tenant_id, policy_pack_id).await {
                    Ok(Some(binding)) => return Ok(binding.enabled),
                    Ok(None) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("tenant_policy_bindings.get.miss");
                    }
                    Ok(None) => return Ok(false),
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("tenant_policy_bindings.get.error");
                        debug!(error = %e, tenant_id = %tenant_id, "KV policy enabled check failed; falling back to SQL");
                    }
                    Err(e) => return Err(e),
                }
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(false);
            }
        }

        let result = sqlx::query(
            r#"
            SELECT enabled
            FROM tenant_policy_bindings
            WHERE tenant_id = ? AND policy_pack_id = ? AND scope = 'global'
            "#,
        )
        .bind(tenant_id)
        .bind(policy_pack_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to check policy enabled status: {}", e)))?;

        Ok(result
            .map(|row| row.get::<i32, _>("enabled") != 0)
            .unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_toggle_tenant_policy() -> Result<()> {
        let db = Db::new_in_memory().await?;

        // Create test tenant first
        sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES ('test-tenant', 'Test', 0)")
            .execute(&*db.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Initialize bindings via the tenants.rs method
        db.initialize_tenant_policy_bindings("test-tenant", "test-user")
            .await?;

        // Telemetry should be disabled by default
        let active_before = db.get_active_policies_for_tenant("test-tenant").await?;
        assert!(
            !active_before.contains(&"telemetry".to_string()),
            "telemetry should be disabled by default"
        );

        // Enable telemetry - should return previous state (false)
        let previous = db
            .toggle_tenant_policy("test-tenant", "telemetry", true, "admin")
            .await?;
        assert!(!previous, "previous state should be false");

        // Verify it's now enabled
        let active_after = db.get_active_policies_for_tenant("test-tenant").await?;
        assert!(
            active_after.contains(&"telemetry".to_string()),
            "telemetry should now be enabled"
        );

        // Disable it again - should return previous state (true)
        let previous = db
            .toggle_tenant_policy("test-tenant", "telemetry", false, "admin")
            .await?;
        assert!(previous, "previous state should be true");

        // Verify it's disabled
        let active_final = db.get_active_policies_for_tenant("test-tenant").await?;
        assert!(
            !active_final.contains(&"telemetry".to_string()),
            "telemetry should be disabled again"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_list_tenant_policy_bindings() -> Result<()> {
        let db = Db::new_in_memory().await?;

        // Create test tenant
        sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES ('test-tenant', 'Test', 0)")
            .execute(&*db.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Initialize bindings
        db.initialize_tenant_policy_bindings("test-tenant", "test-user")
            .await?;

        // List all bindings
        let bindings = db.list_tenant_policy_bindings("test-tenant").await?;

        assert_eq!(bindings.len(), 24, "Should have 24 policy bindings");

        // Check that core policies are enabled
        let enabled_count = bindings.iter().filter(|b| b.enabled).count();
        assert_eq!(enabled_count, 4, "Should have 4 enabled policies");

        // Verify a specific binding
        let egress_binding = bindings
            .iter()
            .find(|b| b.policy_pack_id == "egress")
            .expect("Should find egress binding");
        assert!(egress_binding.enabled);
        assert_eq!(egress_binding.scope, "global");
        assert_eq!(egress_binding.created_by, "test-user");

        Ok(())
    }

    #[tokio::test]
    async fn test_is_policy_enabled() -> Result<()> {
        let db = Db::new_in_memory().await?;

        // Create test tenant
        sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES ('test-tenant', 'Test', 0)")
            .execute(&*db.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Initialize bindings
        db.initialize_tenant_policy_bindings("test-tenant", "test-user")
            .await?;

        // Core policy should be enabled
        assert!(
            db.is_policy_enabled_for_tenant("test-tenant", "egress")
                .await?
        );

        // Non-core policy should be disabled
        assert!(
            !db.is_policy_enabled_for_tenant("test-tenant", "telemetry")
                .await?
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_toggle_creates_audit_record() -> Result<()> {
        let db = Db::new_in_memory().await?;

        // Create test tenant
        sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES ('test-tenant', 'Test', 0)")
            .execute(&*db.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Initialize bindings
        db.initialize_tenant_policy_bindings("test-tenant", "test-user")
            .await?;

        // Toggle a policy
        db.toggle_tenant_policy("test-tenant", "telemetry", true, "admin")
            .await?;

        // Verify audit record was created
        let audit_count = sqlx::query(
            r#"
            SELECT COUNT(*) as cnt
            FROM policy_audit_decisions
            WHERE tenant_id = 'test-tenant'
              AND policy_pack_id = 'telemetry'
              AND hook = 'toggle'
            "#,
        )
        .fetch_one(&*db.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?
        .get::<i32, _>("cnt");

        assert!(audit_count > 0, "Should have audit record for toggle");

        Ok(())
    }
}
