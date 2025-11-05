use crate::Db;
use adapteros_core::DriftPolicy;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Policy configuration for a tenant
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TenantPolicies {
    pub drift: DriftPolicy,
}

impl Db {
    /// Get policies for a tenant
    pub async fn get_policies(&self, tenant_id: &str) -> Result<TenantPolicies> {
        let row = sqlx::query!(
            "SELECT body_json FROM policies WHERE tenant_id = ? AND active = 1",
            tenant_id
        )
        .fetch_optional(self.pool())
        .await?;

        match row {
            Some(record) => {
                let policies: TenantPolicies = serde_json::from_str(&record.body_json)
                    .map_err(|e| {
                        anyhow::anyhow!("Failed to deserialize tenant policies for {}: {}", tenant_id, e)
                    })?;
                Ok(policies)
            }
            None => {
                // No policies found, return defaults
                tracing::debug!(tenant_id = %tenant_id, "No policies found, using defaults");
                Ok(TenantPolicies::default())
            }
        }
    }

    /// Save or update policies for a tenant
    pub async fn save_policies(&self, tenant_id: &str, policies: &TenantPolicies) -> Result<String> {
        let body_json = serde_json::to_string(policies)
            .map_err(|e| anyhow::anyhow!("Failed to serialize tenant policies: {}", e))?;

        // Calculate hash for integrity
        let hash_b3 = format!("{:x}", Sha256::digest(body_json.as_bytes()));

        // Deactivate existing policies for this tenant
        sqlx::query!(
            "UPDATE policies SET active = 0 WHERE tenant_id = ? AND active = 1",
            tenant_id
        )
        .execute(self.pool())
        .await?;

        // Insert new active policy
        let policy_id = format!("policy_{}_{}", tenant_id, hash_b3[..8].to_string());

        sqlx::query!(
            "INSERT INTO policies (id, tenant_id, hash_b3, body_json, active) VALUES (?, ?, ?, ?, 1)",
            policy_id,
            tenant_id,
            hash_b3,
            body_json
        )
        .execute(self.pool())
        .await?;

        tracing::info!(
            tenant_id = %tenant_id,
            policy_id = %policy_id,
            hash_b3 = %hash_b3,
            "Saved tenant policies to database"
        );

        Ok(policy_id)
    }

    /// Get policy history for a tenant (for auditing)
    pub async fn get_policy_history(&self, tenant_id: &str, limit: Option<i64>) -> Result<Vec<PolicyHistoryEntry>> {
        let limit = limit.unwrap_or(10);

        let rows = sqlx::query!(
            "SELECT id, hash_b3, body_json, created_at FROM policies
             WHERE tenant_id = ? ORDER BY created_at DESC LIMIT ?",
            tenant_id,
            limit
        )
        .fetch_all(self.pool())
        .await?;

        let history = rows
            .into_iter()
            .filter_map(|row| {
                Some(PolicyHistoryEntry {
                    id: row.id?,
                    hash_b3: row.hash_b3,
                    created_at: row.created_at,
                })
            })
            .collect();

        Ok(history)
    }
}

/// Policy history entry for auditing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyHistoryEntry {
    pub id: String,
    pub hash_b3: String,
    pub created_at: String,
}
