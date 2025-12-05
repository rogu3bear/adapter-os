use adapteros_core::Result;
use adapteros_storage::kv::KvBackend;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct TenantPolicyBindingKv {
    pub id: String,
    pub tenant_id: String,
    pub policy_pack_id: String,
    pub scope: String,
    pub enabled: bool,
    pub created_at: String,
    pub created_by: String,
    pub updated_at: String,
    pub updated_by: Option<String>,
}

pub struct PolicyBindingKvRepository {
    #[allow(dead_code)]
    backend: Arc<dyn KvBackend>,
}

impl PolicyBindingKvRepository {
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    pub async fn get_active_policy_ids(&self, _tenant_id: &str) -> Result<Vec<String>> {
        Ok(vec![])
    }

    pub async fn upsert_binding(&self, _binding: TenantPolicyBindingKv) -> Result<()> {
        Ok(())
    }

    pub async fn list_all(&self) -> Result<Vec<TenantPolicyBindingKv>> {
        Ok(vec![])
    }
}

pub fn kv_to_binding(
    kv: &TenantPolicyBindingKv,
) -> crate::tenant_policy_bindings::TenantPolicyBinding {
    crate::tenant_policy_bindings::TenantPolicyBinding {
        id: kv.id.clone(),
        tenant_id: kv.tenant_id.clone(),
        policy_pack_id: kv.policy_pack_id.clone(),
        scope: kv.scope.clone(),
        enabled: kv.enabled,
        created_at: kv.created_at.clone(),
        created_by: kv.created_by.clone(),
        updated_at: kv.updated_at.clone(),
        updated_by: kv.updated_by.clone(),
    }
}
//! KV storage for tenant policy bindings.
//!
//! Keys:
//! - `tenant/{tenant_id}/policy_binding/{policy_pack_id}` -> TenantPolicyBindingKv (JSON)
//! - `tenant/{tenant_id}/policy_bindings` -> Vec<policy_pack_id> (kept sorted)
use adapteros_core::{AosError, Result};
use adapteros_storage::KvBackend;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::tenant_policy_bindings::TenantPolicyBinding;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantPolicyBindingKv {
    pub id: String,
    pub tenant_id: String,
    pub policy_pack_id: String,
    pub scope: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<String>,
}

pub struct PolicyBindingKvRepository {
    backend: Arc<dyn KvBackend>,
}

impl PolicyBindingKvRepository {
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    fn primary_key(tenant_id: &str, policy_pack_id: &str) -> String {
        format!(
            "tenant/{tenant_id}/policy_binding/{policy_pack_id}",
            tenant_id = tenant_id,
            policy_pack_id = policy_pack_id
        )
    }

    fn tenant_index_key(tenant_id: &str) -> String {
        format!("tenant/{tenant_id}/policy_bindings", tenant_id = tenant_id)
    }

    fn serialize(binding: &TenantPolicyBindingKv) -> Result<Vec<u8>> {
        serde_json::to_vec(binding).map_err(AosError::Serialization)
    }

    fn deserialize(bytes: &[u8]) -> Result<TenantPolicyBindingKv> {
        serde_json::from_slice(bytes)
            .map_err(|e| AosError::Database(format!("Failed to deserialize policy binding: {}", e)))
    }

    /// Keep the tenant index sorted for deterministic ordering.
    async fn upsert_index(&self, tenant_id: &str, policy_pack_id: &str) -> Result<()> {
        let key = Self::tenant_index_key(tenant_id);
        let existing = self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to read policy index: {}", e)))?;

        let mut ids: Vec<String> = match existing {
            Some(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| AosError::Database(format!("Failed to decode policy index: {}", e)))?,
            None => Vec::new(),
        };

        if !ids.contains(&policy_pack_id.to_string()) {
            ids.push(policy_pack_id.to_string());
            ids.sort(); // deterministic: policy_pack_id ASC
            let payload = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
            self.backend
                .set(&key, payload)
                .await
                .map_err(|e| AosError::Database(format!("Failed to update policy index: {}", e)))?;
        }
        Ok(())
    }

    async fn remove_from_index(&self, tenant_id: &str, policy_pack_id: &str) -> Result<()> {
        let key = Self::tenant_index_key(tenant_id);
        if let Some(bytes) = self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to read policy index: {}", e)))?
        {
            let mut ids: Vec<String> = serde_json::from_slice(&bytes)
                .map_err(|e| AosError::Database(format!("Failed to decode policy index: {}", e)))?;
            ids.retain(|id| id != policy_pack_id);
            if ids.is_empty() {
                let _ = self.backend.delete(&key).await;
            } else {
                ids.sort();
                let payload = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
                self.backend
                    .set(&key, payload)
                    .await
                    .map_err(|e| AosError::Database(format!("Failed to update policy index: {}", e)))?;
            }
        }
        Ok(())
    }

    pub async fn put_binding(&self, binding: TenantPolicyBindingKv) -> Result<()> {
        let key = Self::primary_key(&binding.tenant_id, &binding.policy_pack_id);
        let payload = Self::serialize(&binding)?;
        self.backend
            .set(&key, payload)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store policy binding: {}", e)))?;
        self.upsert_index(&binding.tenant_id, &binding.policy_pack_id)
            .await?;
        Ok(())
    }

    pub async fn get_binding(
        &self,
        tenant_id: &str,
        policy_pack_id: &str,
    ) -> Result<Option<TenantPolicyBindingKv>> {
        let key = Self::primary_key(tenant_id, policy_pack_id);
        let Some(bytes) = self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to fetch policy binding: {}", e)))?
        else {
            return Ok(None);
        };
        Ok(Some(Self::deserialize(&bytes)?))
    }

    pub async fn list_bindings(&self, tenant_id: &str) -> Result<Vec<TenantPolicyBindingKv>> {
        let key = Self::tenant_index_key(tenant_id);
        let Some(bytes) = self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to read policy index: {}", e)))?
        else {
            return Ok(Vec::new());
        };

        let ids: Vec<String> = serde_json::from_slice(&bytes)
            .map_err(|e| AosError::Database(format!("Failed to decode policy index: {}", e)))?;

        let mut bindings = Vec::new();
        for policy_id in ids {
            if let Some(bytes) = self
                .backend
                .get(&Self::primary_key(tenant_id, &policy_id))
                .await
                .map_err(|e| AosError::Database(format!("Failed to load policy binding: {}", e)))?
            {
                if let Ok(binding) = Self::deserialize(&bytes) {
                    bindings.push(binding);
                }
            }
        }

        // Deterministic ordering: policy_pack_id ASC
        bindings.sort_by(|a, b| a.policy_pack_id.cmp(&b.policy_pack_id).then_with(|| a.id.cmp(&b.id)));
        Ok(bindings)
    }

    /// Upsert a binding's enabled flag, returning previous state.
    pub async fn upsert_enabled(
        &self,
        tenant_id: &str,
        policy_pack_id: &str,
        enabled: bool,
        actor: &str,
    ) -> Result<bool> {
        let existing = self.get_binding(tenant_id, policy_pack_id).await?;
        let previous = existing.as_ref().map(|b| b.enabled).unwrap_or(false);
        let now = Utc::now();
        let binding = existing.map(|mut b| {
            b.enabled = enabled;
            b.updated_at = now;
            b.updated_by = Some(actor.to_string());
            b
        }).unwrap_or_else(|| TenantPolicyBindingKv {
            id: uuid::Uuid::now_v7().to_string(),
            tenant_id: tenant_id.to_string(),
            policy_pack_id: policy_pack_id.to_string(),
            scope: "global".to_string(),
            enabled,
            created_at: now,
            created_by: actor.to_string(),
            updated_at: now,
            updated_by: Some(actor.to_string()),
        });

        self.put_binding(binding).await?;
        Ok(previous)
    }

    pub async fn get_active_policy_ids(&self, tenant_id: &str) -> Result<Vec<String>> {
        let bindings = self.list_bindings(tenant_id).await?;
        Ok(bindings
            .into_iter()
            .filter(|b| b.enabled)
            .map(|b| b.policy_pack_id)
            .collect())
    }
}

/// Convert KV binding to API struct
pub fn kv_to_binding(kv: &TenantPolicyBindingKv) -> TenantPolicyBinding {
    TenantPolicyBinding {
        id: kv.id.clone(),
        tenant_id: kv.tenant_id.clone(),
        policy_pack_id: kv.policy_pack_id.clone(),
        scope: kv.scope.clone(),
        enabled: kv.enabled,
        created_at: kv.created_at.to_rfc3339(),
        created_by: kv.created_by.clone(),
        updated_at: kv.updated_at.to_rfc3339(),
        updated_by: kv.updated_by.clone(),
    }
}

