//! ACL (Access Control List) inheritance and resolution
//!
//! This module implements ACL checking with inheritance from parent adapters.
//!
//! ## ACL Resolution Hierarchy
//!
//! 1. If adapter has explicit ACL (non-empty), use it
//! 2. If adapter has no ACL but has a parent, inherit from parent (recursive)
//! 3. If no ACL anywhere in chain, allow all (global access)

use crate::Db;
use adapteros_core::{AosError, Result};
use async_trait::async_trait;
use tracing::{debug, warn};

/// Trait for ACL resolution operations
#[async_trait]
pub trait AclResolver {
    /// Check if tenant has access to adapter (with ACL inheritance)
    async fn check_acl(&self, adapter_id: &str, tenant_id: &str) -> Result<bool>;

    /// Get effective ACL for an adapter (resolves inheritance)
    async fn get_effective_acl(&self, adapter_id: &str) -> Result<Vec<String>>;
}

#[async_trait]
impl AclResolver for Db {
    async fn check_acl(&self, adapter_id: &str, tenant_id: &str) -> Result<bool> {
        check_acl(self, adapter_id, tenant_id).await
    }

    async fn get_effective_acl(&self, adapter_id: &str) -> Result<Vec<String>> {
        get_effective_acl(self, adapter_id).await
    }
}

/// Check if adapter is allowed for tenant (with ACL inheritance)
///
/// ACL resolution follows this hierarchy:
/// 1. If adapter has explicit ACL (non-empty), use it
/// 2. If adapter has no ACL but has a parent, inherit from parent (recursive)
/// 3. If no ACL anywhere in chain, allow all (global access)
pub async fn check_acl(db: &Db, adapter_id: &str, tenant_id: &str) -> Result<bool> {
    // Get adapter ACL and parent info
    let row = sqlx::query_as::<_, AclRow>(
        r#"
        SELECT acl_json, parent_id FROM adapters WHERE adapter_id = ?1
        "#,
    )
    .bind(adapter_id)
    .fetch_optional(db.pool())
    .await
    .map_err(|e| AosError::Registry(format!("Failed to get adapter ACL: {}", e)))?;

    let row = row.ok_or_else(|| AosError::Registry(format!("Adapter '{}' not found", adapter_id)))?;

    // Parse ACL
    let acl: Vec<String> = row
        .acl_json
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    // Check direct ACL first
    if !acl.is_empty() {
        let allowed = acl.contains(&tenant_id.to_string());
        if allowed {
            debug!(
                event_type = "acl.allowed",
                adapter_id = %adapter_id,
                tenant_id = %tenant_id,
                reason = "direct_acl",
                "ACL check passed"
            );
        } else {
            warn!(
                event_type = "acl.denied",
                adapter_id = %adapter_id,
                tenant_id = %tenant_id,
                reason = "not_in_acl",
                "ACL check failed"
            );
        }
        return Ok(allowed);
    }

    // Empty ACL: inherit from parent if exists
    if let Some(parent_id) = &row.parent_id {
        debug!(
            event_type = "acl.inherited",
            adapter_id = %adapter_id,
            parent_id = %parent_id,
            tenant_id = %tenant_id,
            "Inheriting ACL from parent"
        );
        // Recursive call using Box::pin for async recursion
        return Box::pin(check_acl(db, parent_id, tenant_id)).await;
    }

    // No ACL and no parent: allow all (global access)
    debug!(
        event_type = "acl.allowed",
        adapter_id = %adapter_id,
        tenant_id = %tenant_id,
        reason = "global_access",
        "ACL check passed (global access)"
    );
    Ok(true)
}

/// Get effective ACL for an adapter (resolves inheritance)
pub async fn get_effective_acl(db: &Db, adapter_id: &str) -> Result<Vec<String>> {
    // Get adapter ACL and parent info
    let row = sqlx::query_as::<_, AclRow>(
        r#"
        SELECT acl_json, parent_id FROM adapters WHERE adapter_id = ?1
        "#,
    )
    .bind(adapter_id)
    .fetch_optional(db.pool())
    .await
    .map_err(|e| AosError::Registry(format!("Failed to get adapter ACL: {}", e)))?;

    let row = row.ok_or_else(|| AosError::Registry(format!("Adapter '{}' not found", adapter_id)))?;

    // Parse ACL
    let acl: Vec<String> = row
        .acl_json
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    // If ACL is non-empty, return it
    if !acl.is_empty() {
        return Ok(acl);
    }

    // Empty ACL: inherit from parent if exists
    if let Some(parent_id) = &row.parent_id {
        return Box::pin(get_effective_acl(db, parent_id)).await;
    }

    // No ACL anywhere: empty list (global access)
    Ok(vec![])
}

#[derive(sqlx::FromRow)]
struct AclRow {
    acl_json: Option<String>,
    parent_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_acl_global_access() {
        let db = Db::new_in_memory().await.unwrap();

        // Insert test adapter with empty ACL
        sqlx::query(
            r#"
            INSERT INTO adapters (id, adapter_id, tenant_id, name, hash_b3, tier, rank, alpha, targets_json)
            VALUES ('test-1', 'test-adapter', 'default', 'test', 'abc123', 'warm', 8, 32, '[]')
            "#,
        )
        .execute(db.pool())
        .await
        .unwrap();

        // Empty ACL should allow all
        let allowed = check_acl(&db, "test-adapter", "any-tenant").await.unwrap();
        assert!(allowed);
    }

    #[tokio::test]
    async fn test_acl_direct() {
        let db = Db::new_in_memory().await.unwrap();

        // Insert test adapter with specific ACL
        sqlx::query(
            r#"
            INSERT INTO adapters (id, adapter_id, tenant_id, name, hash_b3, tier, rank, alpha, targets_json, acl_json)
            VALUES ('test-1', 'test-adapter', 'default', 'test', 'abc123', 'warm', 8, 32, '[]', '["allowed-tenant"]')
            "#,
        )
        .execute(db.pool())
        .await
        .unwrap();

        // Allowed tenant should pass
        let allowed = check_acl(&db, "test-adapter", "allowed-tenant")
            .await
            .unwrap();
        assert!(allowed);

        // Other tenant should fail
        let allowed = check_acl(&db, "test-adapter", "other-tenant")
            .await
            .unwrap();
        assert!(!allowed);
    }
}
