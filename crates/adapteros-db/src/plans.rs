use crate::{models::Plan, Db};
use adapteros_core::{AosError, Result};
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
        .await
        .map_err(|e| AosError::Database(format!("Failed to create plan: {}", e)))?;
        Ok(id)
    }

    pub async fn get_plan(&self, id: &str) -> Result<Option<Plan>> {
        let plan = sqlx::query_as::<_, Plan>(
            "SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, metallib_hash_b3, created_at FROM plans WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get plan: {}", e)))?;
        Ok(plan)
    }

    pub async fn list_plans_by_tenant(&self, tenant_id: &str) -> Result<Vec<Plan>> {
        let plans = sqlx::query_as::<_, Plan>(
            "SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, metallib_hash_b3, created_at
             FROM plans WHERE tenant_id = ? ORDER BY created_at DESC"
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list plans by tenant: {}", e)))?;
        Ok(plans)
    }

    pub async fn list_all_plans(&self) -> Result<Vec<Plan>> {
        let plans = sqlx::query_as::<_, Plan>(
            "SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, metallib_hash_b3, created_at
             FROM plans ORDER BY created_at DESC"
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list all plans: {}", e)))?;
        Ok(plans)
    }

    pub async fn delete_plan(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM plans WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete plan: {}", e)))?;
        Ok(result.rows_affected() > 0)
    }
}
