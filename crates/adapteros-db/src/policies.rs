use crate::Db;
use adapteros_core::{AosError, DriftPolicy, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::debug;

/// Policy configuration for a tenant
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TenantPolicies {
    pub drift: DriftPolicy,
}

impl Db {
    /// Get policies for a tenant
    pub async fn get_policies(&self, tenant_id: &str) -> Result<TenantPolicies> {
        // Query the active policy for this tenant from the policies table
        let row = sqlx::query(
            "SELECT body_json FROM policies WHERE tenant_id = ? AND active = 1 LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to query tenant policies: {}", e)))?;

        match row {
            Some(row) => {
                let body_json: String = row.get("body_json");
                let policies: TenantPolicies = serde_json::from_str(&body_json).map_err(|e| {
                    AosError::Database(format!("Failed to parse policy JSON: {}", e))
                })?;
                debug!(tenant_id = %tenant_id, "Loaded tenant policies from database");
                Ok(policies)
            }
            None => {
                // No policy found, return defaults
                debug!(tenant_id = %tenant_id, "No policies found, using defaults");
                Ok(TenantPolicies::default())
            }
        }
    }

    /// Store policies for a tenant
    pub async fn store_policies(&self, tenant_id: &str, policies: &TenantPolicies) -> Result<()> {
        let body_json = serde_json::to_string(policies)
            .map_err(|e| AosError::Database(format!("Failed to serialize policies: {}", e)))?;

        // Compute hash using BLAKE3
        let hash_b3 = blake3::hash(body_json.as_bytes()).to_hex().to_string();
        let id = uuid::Uuid::now_v7().to_string();

        // Deactivate existing policies for this tenant
        sqlx::query("UPDATE policies SET active = 0 WHERE tenant_id = ?")
            .bind(tenant_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to deactivate old policies: {}", e)))?;

        // Insert new active policy
        sqlx::query(
            "INSERT INTO policies (id, tenant_id, hash_b3, body_json, active) VALUES (?, ?, ?, ?, 1)"
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(&hash_b3)
        .bind(&body_json)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to store tenant policies: {}", e)))?;

        debug!(tenant_id = %tenant_id, policy_id = %id, "Stored tenant policies");
        Ok(())
    }
}
