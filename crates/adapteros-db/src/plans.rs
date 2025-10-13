use crate::{models::Plan, Db};
use anyhow::Result;
use uuid::Uuid;

impl Db {
    pub async fn create_plan(
        &self,
        _id: &str,
        tenant_id: &str,
        plan_id_b3: &str,
        manifest_hash_b3: &str,
        kernel_hashes_json: &str,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(plan_id_b3)
        .bind(manifest_hash_b3)
        .bind(kernel_hashes_json)
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    pub async fn get_plan(&self, id: &str) -> Result<Option<Plan>> {
        let plan = sqlx::query_as::<_, Plan>(
            "SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, metallib_hash_b3, created_at FROM plans WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?;
        Ok(plan)
    }

    pub async fn list_plans_by_tenant(&self, tenant_id: &str) -> Result<Vec<Plan>> {
        let plans = sqlx::query_as::<_, Plan>(
            "SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, metallib_hash_b3, created_at 
             FROM plans WHERE tenant_id = ? ORDER BY created_at DESC"
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await?;
        Ok(plans)
    }

    pub async fn list_all_plans(&self) -> Result<Vec<Plan>> {
        let plans = sqlx::query_as::<_, Plan>(
            "SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, metallib_hash_b3, created_at 
             FROM plans ORDER BY created_at DESC"
        )
        .fetch_all(self.pool())
        .await?;
        Ok(plans)
    }
}
