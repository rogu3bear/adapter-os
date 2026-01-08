//! Client error persistence and querying
//!
//! This module provides database operations for client-side error events
//! reported from the WASM UI, along with alert rules and alert history.

use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// =============================================================================
// Client Error Types
// =============================================================================

/// A client-side error event stored in the database
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ClientError {
    pub id: String,
    pub tenant_id: String,
    pub user_id: Option<String>,
    pub error_type: String,
    pub message: String,
    pub code: Option<String>,
    pub failure_code: Option<String>,
    pub http_status: Option<i32>,
    pub page: Option<String>,
    pub user_agent: String,
    pub client_timestamp: String,
    pub details_json: Option<String>,
    pub ip_address: Option<String>,
    pub session_id: Option<String>,
    pub created_at: String,
}

/// Query parameters for filtering client errors
#[derive(Debug, Clone, Default)]
pub struct ClientErrorQuery {
    pub tenant_id: String,
    pub error_type: Option<String>,
    pub http_status: Option<i32>,
    pub page_pattern: Option<String>,
    pub user_id: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Aggregated statistics for client errors
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientErrorStats {
    pub total_count: i64,
    pub error_type_counts: HashMap<String, i64>,
    pub http_status_counts: HashMap<i32, i64>,
    pub errors_per_hour: Vec<ErrorsPerHour>,
}

/// Error count per hour bucket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorsPerHour {
    pub hour: String,
    pub count: i64,
}

// =============================================================================
// Alert Rule Types
// =============================================================================

/// An alert rule configuration
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ErrorAlertRule {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub error_type_pattern: Option<String>,
    pub http_status_pattern: Option<String>,
    pub page_pattern: Option<String>,
    pub threshold_count: i32,
    pub threshold_window_minutes: i32,
    pub cooldown_minutes: i32,
    pub severity: String,
    pub is_active: i32,
    pub notification_channels_json: Option<String>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Parameters for creating a new alert rule
#[derive(Debug, Clone)]
pub struct CreateAlertRuleParams {
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub error_type_pattern: Option<String>,
    pub http_status_pattern: Option<String>,
    pub page_pattern: Option<String>,
    pub threshold_count: i32,
    pub threshold_window_minutes: i32,
    pub cooldown_minutes: i32,
    pub severity: String,
    pub notification_channels_json: Option<String>,
    pub created_by: Option<String>,
}

/// A triggered alert record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ErrorAlertHistory {
    pub id: String,
    pub rule_id: String,
    pub tenant_id: String,
    pub triggered_at: String,
    pub error_count: i32,
    pub sample_error_ids_json: Option<String>,
    pub acknowledged_at: Option<String>,
    pub acknowledged_by: Option<String>,
    pub resolved_at: Option<String>,
    pub resolution_note: Option<String>,
}

// =============================================================================
// Client Error Operations
// =============================================================================

impl Db {
    /// Insert a new client error record
    pub async fn insert_client_error(&self, error: &ClientError) -> Result<String> {
        let id = if error.id.is_empty() {
            Uuid::now_v7().to_string()
        } else {
            error.id.clone()
        };

        sqlx::query(
            r#"
            INSERT INTO client_errors (
                id, tenant_id, user_id, error_type, message, code, failure_code,
                http_status, page, user_agent, client_timestamp, details_json,
                ip_address, session_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&error.tenant_id)
        .bind(&error.user_id)
        .bind(&error.error_type)
        .bind(&error.message)
        .bind(&error.code)
        .bind(&error.failure_code)
        .bind(error.http_status)
        .bind(&error.page)
        .bind(&error.user_agent)
        .bind(&error.client_timestamp)
        .bind(&error.details_json)
        .bind(&error.ip_address)
        .bind(&error.session_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(id)
    }

    /// Get a single client error by ID
    pub async fn get_client_error(&self, id: &str) -> Result<Option<ClientError>> {
        let error = sqlx::query_as::<_, ClientError>(
            r#"
            SELECT id, tenant_id, user_id, error_type, message, code, failure_code,
                   http_status, page, user_agent, client_timestamp, details_json,
                   ip_address, session_id, created_at
            FROM client_errors
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(error)
    }

    /// List client errors with filtering
    pub async fn list_client_errors(&self, query: &ClientErrorQuery) -> Result<Vec<ClientError>> {
        let mut sql = String::from(
            r#"
            SELECT id, tenant_id, user_id, error_type, message, code, failure_code,
                   http_status, page, user_agent, client_timestamp, details_json,
                   ip_address, session_id, created_at
            FROM client_errors
            WHERE tenant_id = ?
            "#,
        );

        // Build dynamic WHERE clauses
        if query.error_type.is_some() {
            sql.push_str(" AND error_type = ?");
        }
        if query.http_status.is_some() {
            sql.push_str(" AND http_status = ?");
        }
        if query.page_pattern.is_some() {
            sql.push_str(" AND page LIKE ?");
        }
        if query.user_id.is_some() {
            sql.push_str(" AND user_id = ?");
        }
        if query.since.is_some() {
            sql.push_str(" AND created_at >= ?");
        }
        if query.until.is_some() {
            sql.push_str(" AND created_at <= ?");
        }

        sql.push_str(" ORDER BY created_at DESC");

        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = query.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        let mut query_builder = sqlx::query_as::<_, ClientError>(&sql).bind(&query.tenant_id);

        if let Some(ref error_type) = query.error_type {
            query_builder = query_builder.bind(error_type);
        }
        if let Some(http_status) = query.http_status {
            query_builder = query_builder.bind(http_status);
        }
        if let Some(ref page_pattern) = query.page_pattern {
            // Convert glob to SQL LIKE pattern
            let like_pattern = page_pattern.replace('*', "%").replace('?', "_");
            query_builder = query_builder.bind(like_pattern);
        }
        if let Some(ref user_id) = query.user_id {
            query_builder = query_builder.bind(user_id);
        }
        if let Some(ref since) = query.since {
            query_builder = query_builder.bind(since);
        }
        if let Some(ref until) = query.until {
            query_builder = query_builder.bind(until);
        }

        let errors = query_builder
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(errors)
    }

    /// Get aggregated statistics for client errors
    pub async fn get_client_error_stats(
        &self,
        tenant_id: &str,
        since: Option<&str>,
    ) -> Result<ClientErrorStats> {
        // Total count
        let since_clause = if since.is_some() {
            " AND created_at >= ?"
        } else {
            ""
        };

        let total_count: (i64,) = {
            let sql = format!(
                "SELECT COUNT(*) FROM client_errors WHERE tenant_id = ?{}",
                since_clause
            );
            let mut q = sqlx::query_as(&sql).bind(tenant_id);
            if let Some(s) = since {
                q = q.bind(s);
            }
            q.fetch_one(self.pool())
                .await
                .map_err(|e| AosError::Database(e.to_string()))?
        };

        // Error type counts
        let error_type_counts: Vec<(String, i64)> = {
            let sql = format!(
                r#"
                SELECT error_type, COUNT(*) as count
                FROM client_errors
                WHERE tenant_id = ?{}
                GROUP BY error_type
                ORDER BY count DESC
                "#,
                since_clause
            );
            let mut q = sqlx::query_as(&sql).bind(tenant_id);
            if let Some(s) = since {
                q = q.bind(s);
            }
            q.fetch_all(self.pool())
                .await
                .map_err(|e| AosError::Database(e.to_string()))?
        };

        // HTTP status counts
        let http_status_counts: Vec<(i32, i64)> = {
            let sql = format!(
                r#"
                SELECT http_status, COUNT(*) as count
                FROM client_errors
                WHERE tenant_id = ?{} AND http_status IS NOT NULL
                GROUP BY http_status
                ORDER BY count DESC
                "#,
                since_clause
            );
            let mut q = sqlx::query_as(&sql).bind(tenant_id);
            if let Some(s) = since {
                q = q.bind(s);
            }
            q.fetch_all(self.pool())
                .await
                .map_err(|e| AosError::Database(e.to_string()))?
        };

        // Errors per hour (last 24 hours)
        let errors_per_hour: Vec<(String, i64)> = {
            let sql = format!(
                r#"
                SELECT strftime('%Y-%m-%dT%H:00:00Z', created_at) as hour, COUNT(*) as count
                FROM client_errors
                WHERE tenant_id = ?{}
                GROUP BY hour
                ORDER BY hour DESC
                LIMIT 24
                "#,
                since_clause
            );
            let mut q = sqlx::query_as(&sql).bind(tenant_id);
            if let Some(s) = since {
                q = q.bind(s);
            }
            q.fetch_all(self.pool())
                .await
                .map_err(|e| AosError::Database(e.to_string()))?
        };

        Ok(ClientErrorStats {
            total_count: total_count.0,
            error_type_counts: error_type_counts.into_iter().collect(),
            http_status_counts: http_status_counts.into_iter().collect(),
            errors_per_hour: errors_per_hour
                .into_iter()
                .map(|(hour, count)| ErrorsPerHour { hour, count })
                .collect(),
        })
    }

    /// Count errors matching criteria within a time window (for alert evaluation)
    pub async fn count_errors_in_window(
        &self,
        tenant_id: &str,
        error_type_pattern: Option<&str>,
        http_status_pattern: Option<&str>,
        page_pattern: Option<&str>,
        window_minutes: i64,
    ) -> Result<i64> {
        let mut sql = String::from(
            r#"
            SELECT COUNT(*)
            FROM client_errors
            WHERE tenant_id = ?
              AND created_at >= datetime('now', ? || ' minutes')
            "#,
        );

        if error_type_pattern.is_some() {
            sql.push_str(" AND error_type = ?");
        }
        if http_status_pattern.is_some() {
            // Handle patterns like '4xx', '5xx', or specific codes
            sql.push_str(" AND (CAST(http_status AS TEXT) LIKE ? OR http_status = CAST(? AS INTEGER))");
        }
        if page_pattern.is_some() {
            sql.push_str(" AND page LIKE ?");
        }

        let mut q = sqlx::query_as::<_, (i64,)>(&sql)
            .bind(tenant_id)
            .bind(format!("-{}", window_minutes));

        if let Some(error_type) = error_type_pattern {
            q = q.bind(error_type);
        }
        if let Some(status_pattern) = http_status_pattern {
            // Convert '4xx' to '4%', '5xx' to '5%' for LIKE matching
            let like_pattern = status_pattern
                .replace("xx", "%")
                .replace("XX", "%");
            q = q.bind(like_pattern).bind(status_pattern.to_string());
        }
        if let Some(page) = page_pattern {
            let like_pattern = page.replace('*', "%").replace('?', "_");
            q = q.bind(like_pattern);
        }

        let (count,) = q
            .fetch_one(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(count)
    }

    /// List recent errors since a timestamp (for SSE delta streaming)
    pub async fn list_client_errors_since(
        &self,
        tenant_id: &str,
        since_timestamp: &str,
        limit: Option<i64>,
    ) -> Result<Vec<ClientError>> {
        let limit_clause = limit.map(|l| format!(" LIMIT {}", l)).unwrap_or_default();

        let sql = format!(
            r#"
            SELECT id, tenant_id, user_id, error_type, message, code, failure_code,
                   http_status, page, user_agent, client_timestamp, details_json,
                   ip_address, session_id, created_at
            FROM client_errors
            WHERE tenant_id = ? AND created_at > ?
            ORDER BY created_at ASC
            {}
            "#,
            limit_clause
        );

        let errors = sqlx::query_as::<_, ClientError>(&sql)
            .bind(tenant_id)
            .bind(since_timestamp)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(errors)
    }

    /// Delete old client errors (retention cleanup)
    pub async fn cleanup_old_client_errors(&self, retention_days: i64) -> Result<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM client_errors
            WHERE created_at < datetime('now', ? || ' days')
            "#,
        )
        .bind(format!("-{}", retention_days))
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(result.rows_affected())
    }

    // =========================================================================
    // Alert Rule Operations
    // =========================================================================

    /// List alert rules for a tenant
    pub async fn list_error_alert_rules(&self, tenant_id: &str) -> Result<Vec<ErrorAlertRule>> {
        let rules = sqlx::query_as::<_, ErrorAlertRule>(
            r#"
            SELECT id, tenant_id, name, description, error_type_pattern, http_status_pattern,
                   page_pattern, threshold_count, threshold_window_minutes, cooldown_minutes,
                   severity, is_active, notification_channels_json, created_by,
                   created_at, updated_at
            FROM error_alert_rules
            WHERE tenant_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(rules)
    }

    /// List only active alert rules for a tenant
    pub async fn list_active_error_alert_rules(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<ErrorAlertRule>> {
        let rules = sqlx::query_as::<_, ErrorAlertRule>(
            r#"
            SELECT id, tenant_id, name, description, error_type_pattern, http_status_pattern,
                   page_pattern, threshold_count, threshold_window_minutes, cooldown_minutes,
                   severity, is_active, notification_channels_json, created_by,
                   created_at, updated_at
            FROM error_alert_rules
            WHERE tenant_id = ? AND is_active = 1
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(rules)
    }

    /// Get a single alert rule by ID
    pub async fn get_error_alert_rule(&self, id: &str) -> Result<Option<ErrorAlertRule>> {
        let rule = sqlx::query_as::<_, ErrorAlertRule>(
            r#"
            SELECT id, tenant_id, name, description, error_type_pattern, http_status_pattern,
                   page_pattern, threshold_count, threshold_window_minutes, cooldown_minutes,
                   severity, is_active, notification_channels_json, created_by,
                   created_at, updated_at
            FROM error_alert_rules
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(rule)
    }

    /// Create a new alert rule
    pub async fn create_error_alert_rule(&self, params: &CreateAlertRuleParams) -> Result<String> {
        let id = Uuid::now_v7().to_string();

        sqlx::query(
            r#"
            INSERT INTO error_alert_rules (
                id, tenant_id, name, description, error_type_pattern, http_status_pattern,
                page_pattern, threshold_count, threshold_window_minutes, cooldown_minutes,
                severity, notification_channels_json, created_by
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&params.tenant_id)
        .bind(&params.name)
        .bind(&params.description)
        .bind(&params.error_type_pattern)
        .bind(&params.http_status_pattern)
        .bind(&params.page_pattern)
        .bind(params.threshold_count)
        .bind(params.threshold_window_minutes)
        .bind(params.cooldown_minutes)
        .bind(&params.severity)
        .bind(&params.notification_channels_json)
        .bind(&params.created_by)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(id)
    }

    /// Update an existing alert rule
    pub async fn update_error_alert_rule(&self, rule: &ErrorAlertRule) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE error_alert_rules SET
                name = ?,
                description = ?,
                error_type_pattern = ?,
                http_status_pattern = ?,
                page_pattern = ?,
                threshold_count = ?,
                threshold_window_minutes = ?,
                cooldown_minutes = ?,
                severity = ?,
                is_active = ?,
                notification_channels_json = ?
            WHERE id = ?
            "#,
        )
        .bind(&rule.name)
        .bind(&rule.description)
        .bind(&rule.error_type_pattern)
        .bind(&rule.http_status_pattern)
        .bind(&rule.page_pattern)
        .bind(rule.threshold_count)
        .bind(rule.threshold_window_minutes)
        .bind(rule.cooldown_minutes)
        .bind(&rule.severity)
        .bind(rule.is_active)
        .bind(&rule.notification_channels_json)
        .bind(&rule.id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// Delete an alert rule
    pub async fn delete_error_alert_rule(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM error_alert_rules WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// Toggle alert rule active status
    pub async fn set_error_alert_rule_active(&self, id: &str, is_active: bool) -> Result<()> {
        sqlx::query("UPDATE error_alert_rules SET is_active = ? WHERE id = ?")
            .bind(if is_active { 1 } else { 0 })
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    // =========================================================================
    // Alert History Operations
    // =========================================================================

    /// Insert a triggered alert record
    pub async fn insert_error_alert_history(
        &self,
        rule_id: &str,
        tenant_id: &str,
        error_count: i32,
        sample_error_ids: Option<&[String]>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let sample_ids_json = sample_error_ids.map(|ids| serde_json::to_string(ids).ok()).flatten();

        sqlx::query(
            r#"
            INSERT INTO error_alert_history (id, rule_id, tenant_id, error_count, sample_error_ids_json)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(rule_id)
        .bind(tenant_id)
        .bind(error_count)
        .bind(sample_ids_json)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(id)
    }

    /// List alert history for a tenant
    pub async fn list_error_alert_history(
        &self,
        tenant_id: &str,
        limit: Option<i64>,
        unresolved_only: bool,
    ) -> Result<Vec<ErrorAlertHistory>> {
        let mut sql = String::from(
            r#"
            SELECT id, rule_id, tenant_id, triggered_at, error_count, sample_error_ids_json,
                   acknowledged_at, acknowledged_by, resolved_at, resolution_note
            FROM error_alert_history
            WHERE tenant_id = ?
            "#,
        );

        if unresolved_only {
            sql.push_str(" AND resolved_at IS NULL");
        }

        sql.push_str(" ORDER BY triggered_at DESC");

        if let Some(l) = limit {
            sql.push_str(&format!(" LIMIT {}", l));
        }

        let history = sqlx::query_as::<_, ErrorAlertHistory>(&sql)
            .bind(tenant_id)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(history)
    }

    /// Acknowledge an alert
    pub async fn acknowledge_error_alert(&self, id: &str, acknowledged_by: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE error_alert_history
            SET acknowledged_at = datetime('now'), acknowledged_by = ?
            WHERE id = ?
            "#,
        )
        .bind(acknowledged_by)
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// Resolve an alert
    pub async fn resolve_error_alert(&self, id: &str, resolution_note: Option<&str>) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE error_alert_history
            SET resolved_at = datetime('now'), resolution_note = ?
            WHERE id = ?
            "#,
        )
        .bind(resolution_note)
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_error_query_defaults() {
        let query = ClientErrorQuery::default();
        assert!(query.tenant_id.is_empty());
        assert!(query.error_type.is_none());
        assert!(query.limit.is_none());
    }

    #[test]
    fn test_error_stats_default() {
        let stats = ClientErrorStats::default();
        assert_eq!(stats.total_count, 0);
        assert!(stats.error_type_counts.is_empty());
    }
}
