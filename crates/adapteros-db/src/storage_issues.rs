//! Storage reconciler issue tracking.

use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StorageIssue {
    pub id: String,
    pub tenant_id: Option<String>,
    pub owner_type: String,
    pub owner_id: String,
    pub version_id: Option<String>,
    pub issue_type: String,
    pub severity: String,
    pub location: String,
    pub details: Option<String>,
    pub detected_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewStorageIssue<'a> {
    pub tenant_id: Option<&'a str>,
    pub owner_type: &'a str,
    pub owner_id: &'a str,
    pub version_id: Option<&'a str>,
    pub issue_type: &'a str,
    pub severity: &'a str,
    pub location: &'a str,
    pub details: Option<&'a str>,
}

impl Db {
    /// Record a new storage integrity issue.
    pub async fn record_storage_issue(&self, issue: NewStorageIssue<'_>) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let pool = self.pool_opt().ok_or_else(|| {
            AosError::Database("SQL pool not configured for storage issue recording".to_string())
        })?;

        sqlx::query(
            r#"
            INSERT INTO storage_issues (
                id, tenant_id, owner_type, owner_id, version_id,
                issue_type, severity, location, details, detected_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
            "#,
        )
        .bind(&id)
        .bind(issue.tenant_id)
        .bind(issue.owner_type)
        .bind(issue.owner_id)
        .bind(issue.version_id)
        .bind(issue.issue_type)
        .bind(issue.severity)
        .bind(issue.location)
        .bind(issue.details)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert storage issue: {}", e)))?;

        Ok(id)
    }

    /// List unresolved storage issues.
    pub async fn list_unresolved_storage_issues(&self) -> Result<Vec<StorageIssue>> {
        let pool = match self.pool_opt() {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };

        let rows = sqlx::query_as::<_, StorageIssue>(
            r#"
            SELECT id, tenant_id, owner_type, owner_id, version_id,
                   issue_type, severity, location, details,
                   detected_at, resolved_at
            FROM storage_issues
            WHERE resolved_at IS NULL
            ORDER BY detected_at DESC
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list storage issues: {}", e)))?;

        Ok(rows)
    }

    /// Mark an issue as resolved.
    pub async fn resolve_storage_issue(&self, id: &str) -> Result<()> {
        let pool = self.pool_opt().ok_or_else(|| {
            AosError::Database("SQL pool not configured for storage issue resolution".to_string())
        })?;
        sqlx::query(
            r#"
            UPDATE storage_issues
            SET resolved_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to resolve storage issue: {}", e)))?;
        Ok(())
    }
}
