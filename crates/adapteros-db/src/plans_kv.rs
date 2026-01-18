//! KV storage for plans
//!
//! Provides full KV-based operations for Metal execution plans, enabling dual-write
//! consistency during SQL-to-KV migration.
//!
//! Keys:
//! - `tenant/{tenant_id}/plan/{id}` -> PlanKv (JSON)
//! - `tenant/{tenant_id}/plans` -> Vec<plan_id> for deterministic ordering (created_at DESC)
//! - `plan-lookup/{id}` -> tenant_id (cross-tenant lookup for efficient get by id)

use crate::models::Plan;
use adapteros_core::{AosError, Result};
use adapteros_storage::KvBackend;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanKv {
    pub id: String,
    pub tenant_id: String,
    pub plan_id_b3: String,
    pub manifest_hash_b3: String,
    pub kernel_hashes_json: String,
    pub metallib_hash_b3: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct PlanKvRepository {
    backend: Arc<dyn KvBackend>,
}

impl PlanKvRepository {
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    fn primary_key(tenant_id: &str, id: &str) -> String {
        format!("tenant/{}/plan/{}", tenant_id, id)
    }

    fn tenant_index_key(tenant_id: &str) -> String {
        format!("tenant/{}/plans", tenant_id)
    }

    /// Reverse lookup key for cross-tenant plan lookups by ID
    fn lookup_key(id: &str) -> String {
        format!("plan-lookup/{}", id)
    }

    fn serialize(plan: &PlanKv) -> Result<Vec<u8>> {
        serde_json::to_vec(plan).map_err(AosError::Serialization)
    }

    fn deserialize(bytes: &[u8]) -> Result<PlanKv> {
        serde_json::from_slice(bytes)
            .map_err(|e| AosError::Database(format!("Failed to deserialize plan: {}", e)))
    }

    async fn append_to_tenant_index(&self, tenant_id: &str, plan_id: &str) -> Result<()> {
        let key = Self::tenant_index_key(tenant_id);
        let existing = self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to read plan index: {}", e)))?;

        let mut ids: Vec<String> = match existing {
            Some(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| AosError::Database(format!("Failed to decode plan index: {}", e)))?,
            None => Vec::new(),
        };

        if !ids.contains(&plan_id.to_string()) {
            ids.push(plan_id.to_string());
            let payload = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
            self.backend
                .set(&key, payload)
                .await
                .map_err(|e| AosError::Database(format!("Failed to update plan index: {}", e)))?;
        }
        Ok(())
    }

    async fn remove_from_tenant_index(&self, tenant_id: &str, plan_id: &str) -> Result<()> {
        let key = Self::tenant_index_key(tenant_id);
        if let Some(bytes) = self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to read plan index: {}", e)))?
        {
            let mut ids: Vec<String> = serde_json::from_slice(&bytes)
                .map_err(|e| AosError::Database(format!("Failed to decode plan index: {}", e)))?;
            ids.retain(|id| id != plan_id);
            if ids.is_empty() {
                let _ = self.backend.delete(&key).await;
            } else {
                let payload = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
                self.backend.set(&key, payload).await.map_err(|e| {
                    AosError::Database(format!("Failed to update plan index: {}", e))
                })?;
            }
        }
        Ok(())
    }

    pub async fn put_plan(&self, plan: PlanKv) -> Result<()> {
        let key = Self::primary_key(&plan.tenant_id, &plan.id);
        let payload = Self::serialize(&plan)?;
        self.backend
            .set(&key, payload)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store plan: {}", e)))?;

        // Set reverse lookup index for cross-tenant queries
        self.backend
            .set(&Self::lookup_key(&plan.id), plan.tenant_id.as_bytes().to_vec())
            .await
            .map_err(|e| AosError::Database(format!("Failed to store plan lookup: {}", e)))?;

        self.append_to_tenant_index(&plan.tenant_id, &plan.id)
            .await?;
        Ok(())
    }

    pub async fn get_plan(&self, tenant_id: &str, id: &str) -> Result<Option<PlanKv>> {
        let key = Self::primary_key(tenant_id, id);
        let Some(bytes) = self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to fetch plan: {}", e)))?
        else {
            return Ok(None);
        };
        Ok(Some(Self::deserialize(&bytes)?))
    }

    /// Get plan by ID using reverse lookup (cross-tenant efficient)
    pub async fn get_plan_by_id(&self, id: &str) -> Result<Option<PlanKv>> {
        // First, look up the tenant_id from the reverse index
        let Some(tenant_bytes) = self
            .backend
            .get(&Self::lookup_key(id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to read plan lookup: {}", e)))?
        else {
            return Ok(None);
        };
        let tenant_id = String::from_utf8(tenant_bytes)
            .map_err(|e| AosError::Database(format!("Invalid tenant_id in lookup: {}", e)))?;

        // Now fetch the plan with the known tenant_id
        self.get_plan(&tenant_id, id).await
    }

    pub async fn delete_plan(&self, tenant_id: &str, id: &str) -> Result<bool> {
        let key = Self::primary_key(tenant_id, id);
        let deleted = self
            .backend
            .delete(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete plan: {}", e)))?;
        if deleted {
            self.remove_from_tenant_index(tenant_id, id).await?;
            // Clean up reverse lookup index
            let _ = self.backend.delete(&Self::lookup_key(id)).await;
        }
        Ok(deleted)
    }

    pub async fn list_plans(&self, tenant_id: &str) -> Result<Vec<PlanKv>> {
        let key = Self::tenant_index_key(tenant_id);
        let Some(bytes) = self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to read plan index: {}", e)))?
        else {
            return Ok(Vec::new());
        };

        let ids: Vec<String> = serde_json::from_slice(&bytes)
            .map_err(|e| AosError::Database(format!("Failed to decode plan index: {}", e)))?;

        let mut plans = Vec::new();
        for id in ids {
            if let Some(bytes) = self
                .backend
                .get(&Self::primary_key(tenant_id, &id))
                .await
                .map_err(|e| AosError::Database(format!("Failed to load plan: {}", e)))?
            {
                if let Ok(plan) = Self::deserialize(&bytes) {
                    plans.push(plan);
                }
            }
        }

        // Deterministic ordering: created_at DESC then id ASC
        plans.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        Ok(plans)
    }

    pub async fn list_all(&self) -> Result<Vec<PlanKv>> {
        let mut plans = Vec::new();
        let keys = self
            .backend
            .scan_prefix("tenant/")
            .await
            .map_err(|e| AosError::Database(format!("Failed to scan plans: {}", e)))?;

        for key in keys {
            if key.contains("/plan/") {
                if let Some(bytes) = self
                    .backend
                    .get(&key)
                    .await
                    .map_err(|e| AosError::Database(format!("Failed to read plan: {}", e)))?
                {
                    if let Ok(plan) = Self::deserialize(&bytes) {
                        plans.push(plan);
                    }
                }
            }
        }

        plans.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(plans)
    }
}

/// Convert SQL Plan to KV representation
pub fn plan_to_kv(plan: &Plan) -> Result<PlanKv> {
    let created_at = DateTime::parse_from_rfc3339(&plan.created_at)
        .or_else(|_| {
            // handle sqlite format
            chrono::NaiveDateTime::parse_from_str(&plan.created_at, "%Y-%m-%d %H:%M:%S")
                .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
                .map(|dt| dt.into())
        })
        .map_err(|e| AosError::Parse(format!("invalid created_at: {}", e)))?
        .with_timezone(&Utc);

    Ok(PlanKv {
        id: plan.id.clone(),
        tenant_id: plan.tenant_id.clone(),
        plan_id_b3: plan.plan_id_b3.clone(),
        manifest_hash_b3: plan.manifest_hash_b3.clone(),
        kernel_hashes_json: plan.kernel_hashes_json.clone(),
        metallib_hash_b3: plan.metallib_hash_b3.clone(),
        created_at,
    })
}

/// Convert KV Plan to SQL Plan
pub fn kv_to_plan(kv: &PlanKv) -> Plan {
    Plan {
        id: kv.id.clone(),
        tenant_id: kv.tenant_id.clone(),
        plan_id_b3: kv.plan_id_b3.clone(),
        manifest_hash_b3: kv.manifest_hash_b3.clone(),
        kernel_hashes_json: kv.kernel_hashes_json.clone(),
        metallib_hash_b3: kv.metallib_hash_b3.clone(),
        created_at: kv.created_at.to_rfc3339(),
    }
}
