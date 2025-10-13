use crate::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO jobs (id, kind, tenant_id, user_id, payload_json, status) VALUES (?, ?, ?, ?, ?, 'queued')"
        )
        .bind(&id)
        .bind(kind)
        .bind(tenant_id)
        .bind(user_id)
        .bind(payload_json)
        .execute(self.pool())
        .await?;
        Ok(id)
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
        .await?;
        Ok(())
    }

    pub async fn get_job(&self, id: &str) -> Result<Option<Job>> {
        let job = sqlx::query_as::<_, Job>(
            "SELECT id, kind, tenant_id, user_id, payload_json, status, result_json, logs_path, created_at, started_at, finished_at FROM jobs WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?;
        Ok(job)
    }

    pub async fn list_jobs(&self, tenant_id: Option<&str>) -> Result<Vec<Job>> {
        let jobs = if let Some(tid) = tenant_id {
            sqlx::query_as::<_, Job>(
                "SELECT id, kind, tenant_id, user_id, payload_json, status, result_json, logs_path, created_at, started_at, finished_at FROM jobs WHERE tenant_id = ? ORDER BY created_at DESC LIMIT 100"
            )
            .bind(tid)
            .fetch_all(self.pool())
            .await?
        } else {
            sqlx::query_as::<_, Job>(
                "SELECT id, kind, tenant_id, user_id, payload_json, status, result_json, logs_path, created_at, started_at, finished_at FROM jobs ORDER BY created_at DESC LIMIT 100"
            )
            .fetch_all(self.pool())
            .await?
        };
        Ok(jobs)
    }
}
