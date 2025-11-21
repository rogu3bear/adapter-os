//! Audit logging for RBAC compliance and security
//!
//! All sensitive operations are logged to the audit_logs table for compliance review.
//! Audit logs are immutable and queryable for compliance officers and administrators.

use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditLog {
    pub id: String,
    pub timestamp: String,
    pub user_id: String,
    pub user_role: String,
    pub tenant_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub ip_address: Option<String>,
    pub metadata_json: Option<String>,
}

impl Db {
    /// Log an audit event
    ///
    /// # Arguments
    /// * `user_id` - User who performed the action
    /// * `user_role` - Role of the user (admin, operator, sre, compliance, viewer)
    /// * `tenant_id` - Tenant context
    /// * `action` - Action performed (e.g., "adapter.register", "training.start")
    /// * `resource_type` - Type of resource (e.g., "adapter", "policy", "tenant")
    /// * `resource_id` - ID of the resource acted upon
    /// * `status` - "success" or "failure"
    /// * `error_message` - Error details if status = "failure"
    /// * `ip_address` - Client IP address (optional)
    /// * `metadata_json` - Additional context as JSON (optional)
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// db.log_audit(
    ///     "user-123",
    ///     "admin",
    ///     "tenant-a",
    ///     "adapter.delete",
    ///     "adapter",
    ///     Some("adapter-xyz"),
    ///     "success",
    ///     None,
    ///     Some("192.168.1.100"),
    ///     None,
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn log_audit(
        &self,
        user_id: &str,
        user_role: &str,
        tenant_id: &str,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        status: &str,
        error_message: Option<&str>,
        ip_address: Option<&str>,
        metadata_json: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO audit_logs
             (id, timestamp, user_id, user_role, tenant_id, action, resource_type, resource_id,
              status, error_message, ip_address, metadata_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&timestamp)
        .bind(user_id)
        .bind(user_role)
        .bind(tenant_id)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(status)
        .bind(error_message)
        .bind(ip_address)
        .bind(metadata_json)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(id)
    }

    /// Query audit logs with filters
    ///
    /// # Arguments
    /// * `user_id` - Filter by user (optional)
    /// * `action` - Filter by action (optional)
    /// * `resource_type` - Filter by resource type (optional)
    /// * `start_date` - Filter by start date in RFC3339 format (optional)
    /// * `end_date` - Filter by end date in RFC3339 format (optional)
    /// * `limit` - Maximum number of results (default: 100, max: 1000)
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let logs = db.query_audit_logs(
    ///     Some("user-123"),
    ///     Some("adapter.delete"),
    ///     None,
    ///     Some("2025-01-01T00:00:00Z"),
    ///     Some("2025-12-31T23:59:59Z"),
    ///     100,
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn query_audit_logs(
        &self,
        user_id: Option<&str>,
        action: Option<&str>,
        resource_type: Option<&str>,
        start_date: Option<&str>,
        end_date: Option<&str>,
        limit: i64,
    ) -> Result<Vec<AuditLog>> {
        // Enforce maximum limit
        let limit = limit.min(1000);

        let mut query = String::from(
            "SELECT id, timestamp, user_id, user_role, tenant_id, action, resource_type,
                    resource_id, status, error_message, ip_address, metadata_json
             FROM audit_logs WHERE 1=1",
        );
        let mut params: Vec<String> = Vec::new();

        if let Some(uid) = user_id {
            query.push_str(" AND user_id = ?");
            params.push(uid.to_string());
        }

        if let Some(act) = action {
            query.push_str(" AND action = ?");
            params.push(act.to_string());
        }

        if let Some(rt) = resource_type {
            query.push_str(" AND resource_type = ?");
            params.push(rt.to_string());
        }

        if let Some(start) = start_date {
            query.push_str(" AND timestamp >= ?");
            params.push(start.to_string());
        }

        if let Some(end) = end_date {
            query.push_str(" AND timestamp <= ?");
            params.push(end.to_string());
        }

        query.push_str(" ORDER BY timestamp DESC LIMIT ?");
        params.push(limit.to_string());

        // Build query dynamically
        let mut q = sqlx::query_as::<_, AuditLog>(&query);
        for param in &params {
            q = q.bind(param);
        }

        let logs = q.fetch_all(self.pool()).await.map_err(|e| AosError::Database(e.to_string()))?;
        Ok(logs)
    }

    /// Get audit logs for a specific resource
    ///
    /// # Arguments
    /// * `resource_type` - Type of resource (e.g., "adapter", "policy")
    /// * `resource_id` - ID of the resource
    /// * `limit` - Maximum number of results
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let logs = db.get_resource_audit_trail("adapter", "adapter-xyz", 50).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_resource_audit_trail(
        &self,
        resource_type: &str,
        resource_id: &str,
        limit: i64,
    ) -> Result<Vec<AuditLog>> {
        let limit = limit.min(1000);

        let logs = sqlx::query_as::<_, AuditLog>(
            "SELECT id, timestamp, user_id, user_role, tenant_id, action, resource_type,
                    resource_id, status, error_message, ip_address, metadata_json
             FROM audit_logs
             WHERE resource_type = ? AND resource_id = ?
             ORDER BY timestamp DESC
             LIMIT ?",
        )
        .bind(resource_type)
        .bind(resource_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(logs)
    }

    /// Get audit log count by action (for compliance dashboard)
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let stats = db.get_audit_stats_by_action(
    ///     Some("2025-01-01T00:00:00Z"),
    ///     Some("2025-12-31T23:59:59Z"),
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_audit_stats_by_action(
        &self,
        start_date: Option<&str>,
        end_date: Option<&str>,
    ) -> Result<Vec<(String, i64)>> {
        let mut query = String::from(
            "SELECT action, COUNT(*) as count
             FROM audit_logs
             WHERE 1=1",
        );
        let mut params: Vec<String> = Vec::new();

        if let Some(start) = start_date {
            query.push_str(" AND timestamp >= ?");
            params.push(start.to_string());
        }

        if let Some(end) = end_date {
            query.push_str(" AND timestamp <= ?");
            params.push(end.to_string());
        }

        query.push_str(" GROUP BY action ORDER BY count DESC");

        let mut q = sqlx::query_as::<_, (String, i64)>(&query);
        for param in &params {
            q = q.bind(param);
        }

        let stats = q.fetch_all(self.pool()).await.map_err(|e| AosError::Database(e.to_string()))?;
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_audit_log_creation() {
        let db = Db::connect("sqlite::memory:").await.unwrap();

        let id = db
            .log_audit(
                "user-123",
                "admin",
                "tenant-a",
                "adapter.register",
                "adapter",
                Some("adapter-xyz"),
                "success",
                None,
                Some("192.168.1.100"),
                Some(r#"{"extra":"metadata"}"#),
            )
            .await
            .unwrap();

        assert!(!id.is_empty());

        // Query back
        let logs = db
            .query_audit_logs(Some("user-123"), None, None, None, None, 10)
            .await
            .unwrap();

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].action, "adapter.register");
        assert_eq!(logs[0].user_role, "admin");
        assert_eq!(logs[0].status, "success");
    }

    #[tokio::test]
    async fn test_resource_audit_trail() {
        let db = Db::connect("sqlite::memory:").await.unwrap();

        // Create multiple audit logs for same resource
        db.log_audit(
            "user-1",
            "admin",
            "tenant-a",
            "adapter.register",
            "adapter",
            Some("adapter-xyz"),
            "success",
            None,
            None,
            None,
        )
        .await
        .unwrap();

        db.log_audit(
            "user-2",
            "operator",
            "tenant-a",
            "adapter.load",
            "adapter",
            Some("adapter-xyz"),
            "success",
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let trail = db
            .get_resource_audit_trail("adapter", "adapter-xyz", 10)
            .await
            .unwrap();

        assert_eq!(trail.len(), 2);
    }
}
