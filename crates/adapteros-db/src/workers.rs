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
}
