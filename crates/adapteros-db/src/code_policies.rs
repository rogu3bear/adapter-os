use crate::new_id;
use crate::Db;
use adapteros_core::{AosError, Result};
use adapteros_id::IdPrefix;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CodePolicy {
    pub id: String,
    pub tenant_id: String,
    pub evidence_config_json: String,
    pub auto_apply_config_json: String,
    pub path_permissions_json: String,
    pub secret_patterns_json: String,
    pub patch_limits_json: String,
    pub active: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl Db {
    /// Get active code policy for tenant
    pub async fn get_code_policy(&self, tenant_id: &str) -> Result<Option<CodePolicy>> {
        let policy = sqlx::query_as::<_, CodePolicy>(
            "SELECT id, tenant_id, evidence_config_json, auto_apply_config_json, 
                    path_permissions_json, secret_patterns_json, patch_limits_json, 
                    active, created_at, updated_at 
             FROM code_policies WHERE tenant_id = ? AND active = 1",
        )
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(policy)
    }

    /// Save or update code policy for tenant
    pub async fn save_code_policy(
        &self,
        tenant_id: &str,
        evidence_config_json: &str,
        auto_apply_config_json: &str,
        path_permissions_json: &str,
        secret_patterns_json: &str,
        patch_limits_json: &str,
    ) -> Result<String> {
        // Deactivate existing policies for this tenant
        sqlx::query("UPDATE code_policies SET active = 0 WHERE tenant_id = ? AND active = 1")
            .bind(tenant_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Insert new policy
        let id = new_id(IdPrefix::Pol);
        sqlx::query(
            "INSERT INTO code_policies
             (id, tenant_id, evidence_config_json, auto_apply_config_json,
              path_permissions_json, secret_patterns_json, patch_limits_json, active)
             VALUES (?, ?, ?, ?, ?, ?, ?, 1)",
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(evidence_config_json)
        .bind(auto_apply_config_json)
        .bind(path_permissions_json)
        .bind(secret_patterns_json)
        .bind(patch_limits_json)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(id)
    }

    /// List all code policies for a tenant (including inactive)
    pub async fn list_code_policies(&self, tenant_id: &str) -> Result<Vec<CodePolicy>> {
        let policies = sqlx::query_as::<_, CodePolicy>(
            "SELECT id, tenant_id, evidence_config_json, auto_apply_config_json,
                    path_permissions_json, secret_patterns_json, patch_limits_json,
                    active, created_at, updated_at
             FROM code_policies WHERE tenant_id = ? ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(policies)
    }
}
