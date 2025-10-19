use crate::{models::Worker, Db};
use anyhow::Result;

impl Db {
    pub async fn list_workers_by_tenant(&self, tenant_id: &str) -> Result<Vec<Worker>> {
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at FROM workers WHERE tenant_id = ?"
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await?;
        Ok(workers)
    }

    pub async fn list_all_workers(&self) -> Result<Vec<Worker>> {
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at FROM workers ORDER BY started_at DESC"
        )
        .fetch_all(self.pool())
        .await?;
        Ok(workers)
    }

    pub async fn list_workers_by_node(&self, node_id: &str) -> Result<Vec<Worker>> {
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at FROM workers WHERE node_id = ? ORDER BY started_at DESC",
        )
        .bind(node_id)
        .fetch_all(self.pool())
        .await?;
        Ok(workers)
    }

    pub async fn update_worker_status(&self, worker_id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE workers SET status = ?, last_seen_at = datetime('now') WHERE id = ?")
            .bind(status)
            .bind(worker_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Insert a worker record
    pub async fn insert_worker(
        &self,
        id: &str,
        tenant_id: &str,
        node_id: &str,
        plan_id: &str,
        uds_path: &str,
        pid: Option<i32>,
        status: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO workers (id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(id)
        .bind(tenant_id)
        .bind(node_id)
        .bind(plan_id)
        .bind(uds_path)
        .bind(pid)
        .bind(status)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Update worker heartbeat and optionally status
    pub async fn update_worker_heartbeat(&self, id: &str, status: Option<&str>) -> Result<()> {
        if let Some(st) = status {
            sqlx::query(
                "UPDATE workers SET status = ?, last_seen_at = datetime('now') WHERE id = ?",
            )
            .bind(st)
            .bind(id)
            .execute(self.pool())
            .await?;
        } else {
            sqlx::query("UPDATE workers SET last_seen_at = datetime('now') WHERE id = ?")
                .bind(id)
                .execute(self.pool())
                .await?;
        }
        Ok(())
    }
}
