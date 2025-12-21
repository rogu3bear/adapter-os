// Minimal KV stubs for tenant policy bindings to satisfy build during KV parity tests.
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

    pub async fn upsert_enabled(
        &self,
        _tenant_id: &str,
        _policy_pack_id: &str,
        _enabled: bool,
        _updated_by: &str,
    ) -> Result<bool> {
        Ok(false)
    }

    pub async fn list_bindings(&self, _tenant_id: &str) -> Result<Vec<TenantPolicyBindingKv>> {
        Ok(vec![])
    }

    pub async fn get_binding(
        &self,
        _tenant_id: &str,
        _policy_pack_id: &str,
    ) -> Result<Option<TenantPolicyBindingKv>> {
        Ok(None)
    }

    pub async fn put_binding(&self, _binding: TenantPolicyBindingKv) -> Result<()> {
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
