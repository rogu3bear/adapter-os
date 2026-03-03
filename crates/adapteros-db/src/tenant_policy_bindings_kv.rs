// Minimal KV stubs for tenant policy bindings to satisfy build during KV parity tests.
use adapteros_core::Result;
use adapteros_storage::kv::KvBackend;
use std::sync::Arc;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    backend: Arc<dyn KvBackend>,
}

impl PolicyBindingKvRepository {
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    fn key(tenant_id: &str, policy_pack_id: &str) -> String {
        format!("tenant/{}/policy_binding/{}", tenant_id, policy_pack_id)
    }

    fn prefix(tenant_id: &str) -> String {
        format!("tenant/{}/policy_binding/", tenant_id)
    }

    pub async fn get_active_policy_ids(&self, tenant_id: &str) -> Result<Vec<String>> {
        let bindings = self.list_bindings(tenant_id).await?;
        Ok(bindings
            .into_iter()
            .filter(|b| b.enabled)
            .map(|b| b.policy_pack_id)
            .collect())
    }

    pub async fn upsert_enabled(
        &self,
        tenant_id: &str,
        policy_pack_id: &str,
        enabled: bool,
        updated_by: &str,
    ) -> Result<bool> {
        let key = Self::key(tenant_id, policy_pack_id);
        let mut binding = if let Some(bytes) = self
            .backend
            .get(&key)
            .await
            .map_err(|e| adapteros_core::AosError::Database(e.to_string()))?
        {
            serde_json::from_slice::<TenantPolicyBindingKv>(&bytes)
                .map_err(adapteros_core::AosError::Serialization)?
        } else {
            TenantPolicyBindingKv {
                id: crate::new_id(adapteros_id::IdPrefix::Pol),
                tenant_id: tenant_id.to_string(),
                policy_pack_id: policy_pack_id.to_string(),
                scope: "global".to_string(),
                enabled,
                created_at: chrono::Utc::now().to_rfc3339(),
                created_by: updated_by.to_string(),
                updated_at: chrono::Utc::now().to_rfc3339(),
                updated_by: Some(updated_by.to_string()),
            }
        };

        let previous = binding.enabled;
        binding.enabled = enabled;
        binding.updated_at = chrono::Utc::now().to_rfc3339();
        binding.updated_by = Some(updated_by.to_string());

        let bytes =
            serde_json::to_vec(&binding).map_err(adapteros_core::AosError::Serialization)?;
        self.backend
            .set(&key, bytes)
            .await
            .map_err(|e| adapteros_core::AosError::Database(e.to_string()))?;
        Ok(previous)
    }

    pub async fn list_all(&self) -> Result<Vec<TenantPolicyBindingKv>> {
        let keys = self
            .backend
            .scan_prefix("tenant/")
            .await
            .map_err(|e| adapteros_core::AosError::Database(e.to_string()))?;
        let binding_keys: Vec<String> = keys
            .into_iter()
            .filter(|k| k.contains("/policy_binding/"))
            .collect();
        if binding_keys.is_empty() {
            return Ok(vec![]);
        }
        let values = self
            .backend
            .batch_get(&binding_keys)
            .await
            .map_err(|e| adapteros_core::AosError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for val in values.into_iter().flatten() {
            if let Ok(b) = serde_json::from_slice::<TenantPolicyBindingKv>(&val) {
                results.push(b);
            }
        }
        Ok(results)
    }

    pub async fn list_bindings(&self, tenant_id: &str) -> Result<Vec<TenantPolicyBindingKv>> {
        let prefix = Self::prefix(tenant_id);
        let keys = self
            .backend
            .scan_prefix(&prefix)
            .await
            .map_err(|e| adapteros_core::AosError::Database(e.to_string()))?;
        if keys.is_empty() {
            return Ok(vec![]);
        }
        let values = self
            .backend
            .batch_get(&keys)
            .await
            .map_err(|e| adapteros_core::AosError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for val in values.into_iter().flatten() {
            if let Ok(b) = serde_json::from_slice::<TenantPolicyBindingKv>(&val) {
                results.push(b);
            }
        }
        Ok(results)
    }

    pub async fn get_binding(
        &self,
        tenant_id: &str,
        policy_pack_id: &str,
    ) -> Result<Option<TenantPolicyBindingKv>> {
        let key = Self::key(tenant_id, policy_pack_id);
        if let Some(bytes) = self
            .backend
            .get(&key)
            .await
            .map_err(|e| adapteros_core::AosError::Database(e.to_string()))?
        {
            let binding =
                serde_json::from_slice(&bytes).map_err(adapteros_core::AosError::Serialization)?;
            Ok(Some(binding))
        } else {
            Ok(None)
        }
    }

    pub async fn put_binding(&self, binding: TenantPolicyBindingKv) -> Result<()> {
        let key = Self::key(&binding.tenant_id, &binding.policy_pack_id);
        let bytes =
            serde_json::to_vec(&binding).map_err(adapteros_core::AosError::Serialization)?;
        self.backend
            .set(&key, bytes)
            .await
            .map_err(|e| adapteros_core::AosError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn upsert_binding(&self, binding: TenantPolicyBindingKv) -> Result<()> {
        self.put_binding(binding).await
    }

    /// Delete all policy bindings for a tenant (used for rollback in atomic dual-write)
    pub async fn delete_all_bindings(&self, tenant_id: &str) -> Result<()> {
        let prefix = Self::prefix(tenant_id);
        let keys = self
            .backend
            .scan_prefix(&prefix)
            .await
            .map_err(|e| adapteros_core::AosError::Database(e.to_string()))?;
        if !keys.is_empty() {
            self.backend
                .batch_delete(&keys)
                .await
                .map_err(|e| adapteros_core::AosError::Database(e.to_string()))?;
        }
        Ok(())
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
