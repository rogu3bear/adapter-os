use crate::new_id;
use crate::plans_kv::{kv_to_plan, plan_to_kv, PlanKvRepository};
use crate::{models::Plan, Db, StorageMode};
use adapteros_core::{AosError, Result};
use adapteros_id::IdPrefix;
use chrono::Utc;

impl Db {
    fn get_plan_kv_repo(&self) -> Option<PlanKvRepository> {
        if (self.storage_mode().write_to_kv() || self.storage_mode().read_from_kv())
            && self.has_kv_backend()
        {
            self.kv_backend()
                .map(|kv| PlanKvRepository::new(kv.backend().clone()))
        } else {
            None
        }
    }

    pub async fn create_plan(
        &self,
        _id: &str,
        tenant_id: &str,
        plan_id_b3: &str,
        manifest_hash_b3: &str,
        kernel_hashes_json: &str,
        layout_hash_b3: &str,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Pln);
        let now = Utc::now().to_rfc3339();

        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                sqlx::query(
                    "INSERT INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(&id)
                .bind(tenant_id)
                .bind(plan_id_b3)
                .bind(manifest_hash_b3)
                .bind(kernel_hashes_json)
                .bind(layout_hash_b3)
                .bind(&now)
                .execute(pool)
                .await
                .map_err(|e| AosError::Database(format!("Failed to create plan: {}", e)))?;
            } else if !self.storage_mode().write_to_kv() {
                return Err(AosError::Database(
                    "SQL backend unavailable for create_plan".to_string(),
                ));
            }
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_plan_kv_repo() {
                let kv = plan_to_kv(&Plan {
                    id: id.clone(),
                    tenant_id: tenant_id.to_string(),
                    plan_id_b3: plan_id_b3.to_string(),
                    manifest_hash_b3: manifest_hash_b3.to_string(),
                    kernel_hashes_json: kernel_hashes_json.to_string(),
                    metallib_hash_b3: None,
                    created_at: now.clone(),
                })?;
                repo.put_plan(kv).await?;
            }
        }
        Ok(id)
    }

    pub async fn get_plan(&self, id: &str) -> Result<Option<Plan>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_plan_kv_repo() {
                // We need tenant id to build key; scan all tenants
                let all = repo.list_all().await?;
                if let Some(plan) = all.into_iter().find(|p| p.id == id) {
                    return Ok(Some(kv_to_plan(&plan)));
                }
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(None);
            }
        }

        let pool = match self.pool_opt() {
            Some(p) => p,
            None => return Ok(None),
        };
        let plan = sqlx::query_as::<_, Plan>(
            "SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, metallib_hash_b3, created_at FROM plans WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get plan: {}", e)))?;
        Ok(plan)
    }

    /// Get plan by plan_id_b3 (the logical plan identifier, e.g., "dev")
    /// This is distinct from the primary key `id` which is a UUID
    pub async fn get_plan_by_plan_id(&self, plan_id_b3: &str) -> Result<Option<Plan>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_plan_kv_repo() {
                let all = repo.list_all().await?;
                if let Some(plan) = all.into_iter().find(|p| p.plan_id_b3 == plan_id_b3) {
                    return Ok(Some(kv_to_plan(&plan)));
                }
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(None);
            }
        }

        let pool = match self.pool_opt() {
            Some(p) => p,
            None => return Ok(None),
        };
        let plan = sqlx::query_as::<_, Plan>(
            "SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, metallib_hash_b3, created_at FROM plans WHERE plan_id_b3 = ?"
        )
        .bind(plan_id_b3)
        .fetch_optional(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get plan by plan_id: {}", e)))?;
        Ok(plan)
    }

    pub async fn list_plans_by_tenant(&self, tenant_id: &str) -> Result<Vec<Plan>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_plan_kv_repo() {
                let plans = repo
                    .list_plans(tenant_id)
                    .await?
                    .into_iter()
                    .map(|p| kv_to_plan(&p))
                    .collect();
                return Ok(plans);
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(Vec::new());
            }
        }

        let pool = match self.pool_opt() {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };
        let plans = sqlx::query_as::<_, Plan>(
            "SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, metallib_hash_b3, created_at
             FROM plans WHERE tenant_id = ? ORDER BY created_at DESC"
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list plans by tenant: {}", e)))?;
        Ok(plans)
    }

    pub async fn list_all_plans(&self) -> Result<Vec<Plan>> {
        if self.storage_mode().read_from_kv() && !self.storage_mode().sql_fallback_enabled() {
            if let Some(repo) = self.get_plan_kv_repo() {
                return Ok(repo
                    .list_all()
                    .await?
                    .into_iter()
                    .map(|p| kv_to_plan(&p))
                    .collect());
            }
        }

        let pool = match self.pool_opt() {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };
        let plans = sqlx::query_as::<_, Plan>(
            "SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, metallib_hash_b3, created_at
             FROM plans ORDER BY created_at DESC"
        )
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list all plans: {}", e)))?;
        Ok(plans)
    }

    pub async fn delete_plan(&self, id: &str) -> Result<bool> {
        let mut deleted = false;

        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                let result = sqlx::query("DELETE FROM plans WHERE id = ?")
                    .bind(id)
                    .execute(pool)
                    .await
                    .map_err(|e| AosError::Database(format!("Failed to delete plan: {}", e)))?;
                deleted |= result.rows_affected() > 0;
            }
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_plan_kv_repo() {
                // We need tenant; scan all
                if let Some(plan) = repo.list_all().await?.into_iter().find(|p| p.id == id) {
                    deleted |= repo.delete_plan(&plan.tenant_id, id).await?;
                }
            }
        }

        Ok(deleted)
    }
}
