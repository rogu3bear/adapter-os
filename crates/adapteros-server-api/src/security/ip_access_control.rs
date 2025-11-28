///! IP address allowlisting and denylisting for security
///!
///! Provides granular control over which IP addresses can access the system.
use adapteros_core::Result;
use adapteros_db::Db;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IpAccessRule {
    pub id: String,
    pub ip_address: String,
    pub ip_range: Option<String>,
    pub list_type: String, // 'allow' or 'deny'
    pub tenant_id: Option<String>,
    pub active: bool,
    pub created_at: String,
    pub created_by: String,
    pub expires_at: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessDecision {
    Allow,
    Deny,
}

/// Check if an IP address is allowed access
///
/// Logic:
/// 1. Check deny list first (deny wins)
/// 2. If allowlist is enabled and IP not on allowlist, deny
/// 3. Otherwise, allow
pub async fn check_ip_access(db: &Db, ip: &str, tenant_id: Option<&str>) -> Result<AccessDecision> {
    let now = Utc::now().to_rfc3339();

    // Check deny list (global and tenant-specific)
    let denied = if let Some(tid) = tenant_id {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM ip_access_control
             WHERE (ip_address = ? OR ? LIKE ip_range || '%')
               AND list_type = 'deny'
               AND active = 1
               AND (expires_at IS NULL OR expires_at > ?)
               AND (tenant_id IS NULL OR tenant_id = ?)",
        )
        .bind(ip)
        .bind(ip)
        .bind(&now)
        .bind(tid)
        .fetch_one(db.pool())
        .await?
    } else {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM ip_access_control
             WHERE (ip_address = ? OR ? LIKE ip_range || '%')
               AND list_type = 'deny'
               AND active = 1
               AND (expires_at IS NULL OR expires_at > ?)
               AND tenant_id IS NULL",
        )
        .bind(ip)
        .bind(ip)
        .bind(&now)
        .fetch_one(db.pool())
        .await?
    };

    if denied > 0 {
        warn!(ip = %ip, tenant_id = ?tenant_id, "IP address denied by denylist");
        return Ok(AccessDecision::Deny);
    }

    // Check if allowlist is enabled
    let allowlist_enabled = if let Some(tid) = tenant_id {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM ip_access_control
             WHERE list_type = 'allow'
               AND active = 1
               AND (tenant_id IS NULL OR tenant_id = ?)",
        )
        .bind(tid)
        .fetch_one(db.pool())
        .await?
    } else {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM ip_access_control
             WHERE list_type = 'allow'
               AND active = 1
               AND tenant_id IS NULL",
        )
        .fetch_one(db.pool())
        .await?
    };

    if allowlist_enabled > 0 {
        // Check if IP is on allowlist
        let allowed = if let Some(tid) = tenant_id {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM ip_access_control
                 WHERE (ip_address = ? OR ? LIKE ip_range || '%')
                   AND list_type = 'allow'
                   AND active = 1
                   AND (expires_at IS NULL OR expires_at > ?)
                   AND (tenant_id IS NULL OR tenant_id = ?)",
            )
            .bind(ip)
            .bind(ip)
            .bind(&now)
            .bind(tid)
            .fetch_one(db.pool())
            .await?
        } else {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM ip_access_control
                 WHERE (ip_address = ? OR ? LIKE ip_range || '%')
                   AND list_type = 'allow'
                   AND active = 1
                   AND (expires_at IS NULL OR expires_at > ?)
                   AND tenant_id IS NULL",
            )
            .bind(ip)
            .bind(ip)
            .bind(&now)
            .fetch_one(db.pool())
            .await?
        };

        if allowed == 0 {
            warn!(ip = %ip, tenant_id = ?tenant_id, "IP address not on allowlist");
            return Ok(AccessDecision::Deny);
        }
    }

    Ok(AccessDecision::Allow)
}

/// Add an IP address to allow or deny list
pub async fn add_ip_rule(
    db: &Db,
    ip_address: &str,
    ip_range: Option<&str>,
    list_type: &str,
    tenant_id: Option<&str>,
    created_by: &str,
    reason: Option<&str>,
    expires_at: Option<&str>,
) -> Result<String> {
    let id = Uuid::now_v7().to_string();
    let created_at = Utc::now().to_rfc3339();

    if list_type != "allow" && list_type != "deny" {
        return Err(anyhow::anyhow!("list_type must be 'allow' or 'deny'").into());
    }

    sqlx::query(
        "INSERT INTO ip_access_control
         (id, ip_address, ip_range, list_type, tenant_id, active, created_at, created_by, expires_at, reason)
         VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(ip_address)
    .bind(ip_range)
    .bind(list_type)
    .bind(tenant_id)
    .bind(&created_at)
    .bind(created_by)
    .bind(expires_at)
    .bind(reason)
    .execute(db.pool())
    .await?;

    info!(
        id = %id,
        ip_address = %ip_address,
        list_type = %list_type,
        tenant_id = ?tenant_id,
        "Added IP access rule"
    );

    Ok(id)
}

/// Remove an IP access rule
pub async fn remove_ip_rule(db: &Db, rule_id: &str) -> Result<()> {
    sqlx::query("UPDATE ip_access_control SET active = 0 WHERE id = ?")
        .bind(rule_id)
        .execute(db.pool())
        .await?;

    info!(rule_id = %rule_id, "Removed IP access rule");

    Ok(())
}

/// List all IP access rules for a tenant
pub async fn list_ip_rules(
    db: &Db,
    tenant_id: Option<&str>,
    list_type: Option<&str>,
) -> Result<Vec<IpAccessRule>> {
    let mut query = String::from(
        "SELECT id, ip_address, ip_range, list_type, tenant_id, active, created_at, created_by, expires_at, reason
         FROM ip_access_control
         WHERE active = 1"
    );

    let mut params: Vec<String> = Vec::new();

    if let Some(tid) = tenant_id {
        query.push_str(" AND (tenant_id IS NULL OR tenant_id = ?)");
        params.push(tid.to_string());
    } else {
        query.push_str(" AND tenant_id IS NULL");
    }

    if let Some(lt) = list_type {
        query.push_str(" AND list_type = ?");
        params.push(lt.to_string());
    }

    query.push_str(" ORDER BY created_at DESC");

    let mut q = sqlx::query_as::<_, IpAccessRule>(&query);
    for param in &params {
        q = q.bind(param);
    }

    let rules = q.fetch_all(db.pool()).await?;
    Ok(rules)
}

/// Cleanup expired IP rules
pub async fn cleanup_expired_ip_rules(db: &Db) -> Result<usize> {
    let now = Utc::now().to_rfc3339();

    let result = sqlx::query(
        "UPDATE ip_access_control SET active = 0
         WHERE expires_at IS NOT NULL AND expires_at < ?",
    )
    .bind(&now)
    .execute(db.pool())
    .await?;

    let count = result.rows_affected() as usize;

    if count > 0 {
        info!(count = %count, "Cleaned up expired IP access rules");
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn init_test_schema(db: &Db) {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS ip_access_control (
                id TEXT PRIMARY KEY,
                ip_address TEXT NOT NULL,
                ip_range TEXT,
                list_type TEXT NOT NULL CHECK(list_type IN ('allow', 'deny')),
                tenant_id TEXT,
                active INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                created_by TEXT NOT NULL,
                expires_at TEXT,
                reason TEXT
            )",
        )
        .execute(db.pool())
        .await
        .expect("Failed to create ip_access_control table");
    }

    #[tokio::test]
    async fn test_ip_denylist() {
        let db = Db::connect("sqlite::memory:")
            .await
            .expect("Failed to create test database");
        init_test_schema(&db).await;

        // Add to denylist
        add_ip_rule(
            &db,
            "192.168.1.100",
            None,
            "deny",
            Some("tenant-a"),
            "admin",
            Some("suspicious activity"),
            None,
        )
        .await
        .expect("IP access control operation failed");

        // Check denied
        let decision = check_ip_access(&db, "192.168.1.100", Some("tenant-a"))
            .await
            .expect("IP access control operation failed");
        assert_eq!(decision, AccessDecision::Deny);

        // Different IP allowed
        let decision = check_ip_access(&db, "192.168.1.101", Some("tenant-a"))
            .await
            .expect("IP access control operation failed");
        assert_eq!(decision, AccessDecision::Allow);
    }

    #[tokio::test]
    async fn test_ip_allowlist() {
        let db = Db::connect("sqlite::memory:")
            .await
            .expect("Failed to create test database");
        init_test_schema(&db).await;

        // Add to allowlist
        add_ip_rule(
            &db,
            "10.0.0.1",
            None,
            "allow",
            Some("tenant-b"),
            "admin",
            Some("corporate office"),
            None,
        )
        .await
        .expect("IP access control operation failed");

        // Allowlisted IP allowed
        let decision = check_ip_access(&db, "10.0.0.1", Some("tenant-b"))
            .await
            .expect("IP access control operation failed");
        assert_eq!(decision, AccessDecision::Allow);

        // Non-allowlisted IP denied when allowlist exists
        let decision = check_ip_access(&db, "10.0.0.2", Some("tenant-b"))
            .await
            .expect("IP access control operation failed");
        assert_eq!(decision, AccessDecision::Deny);
    }
}
