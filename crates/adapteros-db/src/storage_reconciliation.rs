use crate::query_helpers::db_err;
use crate::Db;
use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use crate::new_id;
use adapteros_id::IdPrefix;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StorageReconciliationIssue {
    pub id: String,
    pub tenant_id: Option<String>,
    pub owner_type: String,
    pub owner_id: Option<String>,
    pub version_id: Option<String>,
    pub issue_type: String,
    pub severity: String,
    pub path: String,
    pub expected_hash: Option<String>,
    pub actual_hash: Option<String>,
    pub message: Option<String>,
    pub detected_at: String,
    pub resolved_at: Option<String>,
}

pub struct StorageIssueParams<'a> {
    pub tenant_id: Option<&'a str>,
    pub owner_type: &'a str,
    pub owner_id: Option<&'a str>,
    pub version_id: Option<&'a str>,
    pub issue_type: &'a str,
    pub severity: &'a str,
    pub path: &'a str,
    pub expected_hash: Option<&'a str>,
    pub actual_hash: Option<&'a str>,
    pub message: Option<&'a str>,
}

impl Db {
    pub async fn record_storage_reconciliation_issue(
        &self,
        params: StorageIssueParams<'_>,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Fil);
        sqlx::query(
            r#"
            INSERT INTO storage_reconciliation_issues (
                id, tenant_id, owner_type, owner_id, version_id,
                issue_type, severity, path, expected_hash, actual_hash, message
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(params.tenant_id)
        .bind(params.owner_type)
        .bind(params.owner_id)
        .bind(params.version_id)
        .bind(params.issue_type)
        .bind(params.severity)
        .bind(params.path)
        .bind(params.expected_hash)
        .bind(params.actual_hash)
        .bind(params.message)
        .execute(self.pool())
        .await
        .map_err(db_err("insert storage reconciliation issue"))?;
        Ok(id)
    }

    pub async fn list_storage_reconciliation_issues(
        &self,
        limit: i64,
    ) -> Result<Vec<StorageReconciliationIssue>> {
        let issues = sqlx::query_as::<_, StorageReconciliationIssue>(
            r#"
            SELECT id, tenant_id, owner_type, owner_id, version_id, issue_type, severity,
                   path, expected_hash, actual_hash, message, detected_at, resolved_at
            FROM storage_reconciliation_issues
            ORDER BY detected_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list storage reconciliation issues"))?;
        Ok(issues)
    }

    pub async fn resolve_storage_reconciliation_issue(&self, issue_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE storage_reconciliation_issues SET resolved_at = datetime('now') WHERE id = ?",
        )
        .bind(issue_id)
        .execute(self.pool())
        .await
        .map_err(db_err("resolve storage issue"))?;
        Ok(())
    }
}
