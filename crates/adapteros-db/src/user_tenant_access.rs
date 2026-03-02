//! User tenant access management
//!
//! Manages which tenants admin users can access (for tenant isolation security)

use crate::{new_id, Db, Result};
use adapteros_id::IdPrefix;
use chrono::Utc;

/// Grant a user access to a tenant
///
/// This is used to give admin users explicit access to specific tenants,
/// fixing the tenant isolation bypass vulnerability.
pub async fn grant_user_tenant_access(
    db: &Db,
    user_id: &str,
    tenant_id: &str,
    granted_by: &str,
    reason: Option<&str>,
    expires_at: Option<&str>,
) -> Result<()> {
    let id = new_id(IdPrefix::Usr);
    let granted_at = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO user_tenant_access
         (id, user_id, tenant_id, granted_by, granted_at, reason, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(user_id, tenant_id) DO UPDATE SET
            granted_by = excluded.granted_by,
            granted_at = excluded.granted_at,
            reason = excluded.reason,
            expires_at = excluded.expires_at",
    )
    .bind(&id)
    .bind(user_id)
    .bind(tenant_id)
    .bind(granted_by)
    .bind(&granted_at)
    .bind(reason)
    .bind(expires_at)
    .execute(db.pool_result()?)
    .await?;

    Ok(())
}

/// Revoke a user's access to a tenant
pub async fn revoke_user_tenant_access(db: &Db, user_id: &str, tenant_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM user_tenant_access WHERE user_id = ? AND tenant_id = ?")
        .bind(user_id)
        .bind(tenant_id)
        .execute(db.pool_result()?)
        .await?;

    Ok(())
}

/// Get all tenants a user has access to
///
/// Returns tenant IDs only (for use in JWT claims)
pub async fn get_user_tenant_access(db: &Db, user_id: &str) -> Result<Vec<String>> {
    let now = Utc::now().to_rfc3339();

    let tenant_ids: Vec<String> = sqlx::query_scalar(
        "SELECT tenant_id FROM user_tenant_access
         WHERE user_id = ?
           AND (expires_at IS NULL OR expires_at > ?)",
    )
    .bind(user_id)
    .bind(&now)
    .fetch_all(db.pool_result()?)
    .await?;

    Ok(tenant_ids)
}

/// Get detailed tenant access info for a user (for admin UI)
pub async fn get_user_tenant_access_details(
    db: &Db,
    user_id: &str,
) -> Result<Vec<UserTenantAccess>> {
    let now = Utc::now().to_rfc3339();

    let access: Vec<UserTenantAccess> = sqlx::query_as(
        "SELECT id, user_id, tenant_id, granted_by, granted_at, reason, expires_at
         FROM user_tenant_access
         WHERE user_id = ?
           AND (expires_at IS NULL OR expires_at > ?)",
    )
    .bind(user_id)
    .bind(&now)
    .fetch_all(db.pool_result()?)
    .await?;

    Ok(access)
}

/// Clean up expired tenant access grants
pub async fn cleanup_expired_tenant_access(db: &Db) -> Result<usize> {
    let now = Utc::now().to_rfc3339();

    let result = sqlx::query(
        "DELETE FROM user_tenant_access WHERE expires_at IS NOT NULL AND expires_at <= ?",
    )
    .bind(&now)
    .execute(db.pool_result()?)
    .await?;

    Ok(result.rows_affected() as usize)
}

/// User tenant access record
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserTenantAccess {
    pub id: String,
    pub user_id: String,
    pub tenant_id: String,
    pub granted_by: Option<String>,
    pub granted_at: String,
    pub reason: Option<String>,
    pub expires_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_grant_and_revoke_access() {
        let db = Db::connect("sqlite::memory:")
            .await
            .expect("Failed to create test database");

        // Create the table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_tenant_access (
                id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT NOT NULL,
                tenant_id TEXT NOT NULL,
                granted_by TEXT NOT NULL,
                granted_at TEXT NOT NULL DEFAULT (datetime('now')),
                reason TEXT,
                expires_at TEXT,
                UNIQUE(user_id, tenant_id)
            )
            "#,
        )
        .execute(db.pool_result().unwrap())
        .await
        .expect("Failed to create table");

        // Grant access
        grant_user_tenant_access(
            &db,
            "user-1",
            "tenant-a",
            "admin-1",
            Some("test access"),
            None,
        )
        .await
        .expect("Failed to grant access");

        // Verify access
        let tenants = get_user_tenant_access(&db, "user-1")
            .await
            .expect("Failed to get access");
        assert_eq!(tenants, vec!["tenant-a"]);

        // Revoke access
        revoke_user_tenant_access(&db, "user-1", "tenant-a")
            .await
            .expect("Failed to revoke access");

        // Verify revoked
        let tenants = get_user_tenant_access(&db, "user-1")
            .await
            .expect("Failed to get access");
        assert!(tenants.is_empty());
    }
}
