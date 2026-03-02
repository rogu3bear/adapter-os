use crate::new_id;
use crate::Db;
use adapteros_core::{AosError, Result};
use adapteros_id::IdPrefix;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
    pub payload_json: String,
    pub status: String,
    pub result_json: Option<String>,
    pub logs_path: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

impl Db {
    pub async fn create_job(
        &self,
        kind: &str,
        tenant_id: Option<&str>,
        user_id: Option<&str>,
        payload_json: &str,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Job);
        sqlx::query(
            "INSERT INTO jobs (id, kind, tenant_id, user_id, payload_json, status) VALUES (?, ?, ?, ?, ?, 'queued')"
        )
        .bind(&id)
        .bind(kind)
        .bind(tenant_id)
        .bind(user_id)
        .bind(payload_json)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create job: {}", e)))?;

        // Count queued jobs for metrics (simple implementation - could be optimized)
        if let Ok(queue_depth) = self.count_queued_jobs().await {
            // Note: This would need access to metrics collector, which isn't available here
            // In a real implementation, we'd emit an event or use a callback
            tracing::debug!("Job queue depth: {}", queue_depth);
        }

        Ok(id)
    }

    /// Count queued jobs for metrics
    pub async fn count_queued_jobs(&self) -> Result<i64> {
        let result = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status = 'queued'")
            .fetch_one(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to count queued jobs: {}", e)))?;
        Ok(result)
    }

    pub async fn update_job_status(
        &self,
        id: &str,
        status: &str,
        result_json: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE jobs SET status = ?, result_json = ?, finished_at = CASE WHEN ? IN ('finished','failed','cancelled') THEN datetime('now') ELSE finished_at END WHERE id = ?"
        )
        .bind(status)
        .bind(result_json)
        .bind(status)
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update job status: {}", e)))?;
        Ok(())
    }

    pub async fn get_job(&self, id: &str) -> Result<Option<Job>> {
        let job = sqlx::query_as::<_, Job>(
            "SELECT id, kind, tenant_id, user_id, payload_json, status, result_json, logs_path, created_at, started_at, finished_at FROM jobs WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get job: {}", e)))?;
        Ok(job)
    }

    pub async fn list_jobs(&self, tenant_id: Option<&str>) -> Result<Vec<Job>> {
        let jobs = if let Some(tid) = tenant_id {
            sqlx::query_as::<_, Job>(
                "SELECT id, kind, tenant_id, user_id, payload_json, status, result_json, logs_path, created_at, started_at, finished_at FROM jobs WHERE tenant_id = ? ORDER BY created_at DESC LIMIT 100"
            )
            .bind(tid)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to list jobs: {}", e)))?
        } else {
            sqlx::query_as::<_, Job>(
                "SELECT id, kind, tenant_id, user_id, payload_json, status, result_json, logs_path, created_at, started_at, finished_at FROM jobs ORDER BY created_at DESC LIMIT 100"
            )
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to list jobs: {}", e)))?
        };
        Ok(jobs)
    }
}
