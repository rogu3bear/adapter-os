//! KV storage for plugin configurations and tenant overrides.
//!
//! Provides a KV-backed mirror of `plugin_configs` and `plugin_tenant_enables`
//! for dual-write and KV-primary modes. Keys are namespaced by plugin and tenant
//! to preserve tenant isolation invariants.

use adapteros_core::{AosError, Result};
use adapteros_storage::KvBackend;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::new_id;
use adapteros_id::IdPrefix;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginConfigKv {
    pub id: String,
    pub plugin_name: String,
    pub enabled: bool,
    pub config_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginTenantEnableKv {
    pub id: String,
    pub plugin_name: String,
    pub tenant_id: String,
    pub enabled: bool,
    pub config_override_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct PluginConfigKvRepository {
    backend: Arc<dyn KvBackend>,
}

impl PluginConfigKvRepository {
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    fn config_key(plugin_name: &str) -> String {
        format!("plugin_config:{plugin_name}")
    }

    fn tenant_key(tenant_id: &str, plugin_name: &str) -> String {
        format!("plugin_tenant:{tenant_id}:{plugin_name}")
    }

    fn tenant_prefix(tenant_id: &str) -> String {
        format!("plugin_tenant:{tenant_id}:")
    }

    pub async fn get_plugin_config(&self, plugin_name: &str) -> Result<Option<PluginConfigKv>> {
        let key = Self::config_key(plugin_name);
        let Some(bytes) = self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("KV get plugin config failed: {e}")))?
        else {
            return Ok(None);
        };

        serde_json::from_slice(&bytes)
            .map_err(AosError::Serialization)
            .map(Some)
    }

    pub async fn upsert_plugin_config(&self, desired: PluginConfigKv) -> Result<PluginConfigKv> {
        let bytes = serde_json::to_vec(&desired).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::config_key(&desired.plugin_name), bytes)
            .await
            .map_err(|e| AosError::Database(format!("KV upsert plugin config failed: {e}")))?;
        Ok(desired)
    }

    pub async fn list_plugin_configs(&self) -> Result<Vec<PluginConfigKv>> {
        let keys = self
            .backend
            .scan_prefix("plugin_config:")
            .await
            .map_err(|e| AosError::Database(format!("KV scan plugin configs failed: {e}")))?;

        let mut configs = Vec::new();
        for key in keys {
            if let Some(bytes) = self
                .backend
                .get(&key)
                .await
                .map_err(|e| AosError::Database(format!("KV read plugin config failed: {e}")))?
            {
                if let Ok(cfg) = serde_json::from_slice::<PluginConfigKv>(&bytes) {
                    configs.push(cfg);
                }
            }
        }

        // Deterministic ordering by plugin_name ASC
        configs.sort_by(|a, b| a.plugin_name.cmp(&b.plugin_name));
        Ok(configs)
    }

    pub async fn get_tenant_enable(
        &self,
        tenant_id: &str,
        plugin_name: &str,
    ) -> Result<Option<PluginTenantEnableKv>> {
        let key = Self::tenant_key(tenant_id, plugin_name);
        let Some(bytes) = self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("KV get tenant plugin failed: {e}")))?
        else {
            return Ok(None);
        };

        serde_json::from_slice(&bytes)
            .map_err(AosError::Serialization)
            .map(Some)
    }

    pub async fn upsert_tenant_enable(
        &self,
        desired: PluginTenantEnableKv,
    ) -> Result<PluginTenantEnableKv> {
        let bytes = serde_json::to_vec(&desired).map_err(AosError::Serialization)?;
        self.backend
            .set(
                &Self::tenant_key(&desired.tenant_id, &desired.plugin_name),
                bytes,
            )
            .await
            .map_err(|e| AosError::Database(format!("KV upsert tenant plugin failed: {e}")))?;
        Ok(desired)
    }

    pub async fn list_tenant_enables(&self, tenant_id: &str) -> Result<Vec<PluginTenantEnableKv>> {
        let prefix = Self::tenant_prefix(tenant_id);
        let keys = self
            .backend
            .scan_prefix(&prefix)
            .await
            .map_err(|e| AosError::Database(format!("KV scan tenant plugins failed: {e}")))?;

        let mut entries = Vec::new();
        for key in keys {
            if let Some(bytes) = self
                .backend
                .get(&key)
                .await
                .map_err(|e| AosError::Database(format!("KV read tenant plugin failed: {e}")))?
            {
                if let Ok(entry) = serde_json::from_slice::<PluginTenantEnableKv>(&bytes) {
                    entries.push(entry);
                }
            }
        }

        // Deterministic ordering by plugin_name ASC
        entries.sort_by(|a, b| a.plugin_name.cmp(&b.plugin_name));
        Ok(entries)
    }

    pub fn now_ts() -> String {
        Utc::now().to_rfc3339()
    }

    pub fn new_config_record(
        &self,
        plugin_name: &str,
        enabled: bool,
        config_json: Option<String>,
        existing: Option<PluginConfigKv>,
    ) -> PluginConfigKv {
        let now = Self::now_ts();
        let (id, created_at) = if let Some(existing) = existing {
            (existing.id, existing.created_at)
        } else {
            (new_id(IdPrefix::Pol), now.clone())
        };

        PluginConfigKv {
            id,
            plugin_name: plugin_name.to_string(),
            enabled,
            config_json,
            created_at,
            updated_at: now,
        }
    }

    pub fn new_tenant_record(
        &self,
        tenant_id: &str,
        plugin_name: &str,
        enabled: bool,
        config_override_json: Option<String>,
        existing: Option<PluginTenantEnableKv>,
    ) -> PluginTenantEnableKv {
        let now = Self::now_ts();
        let (id, created_at) = if let Some(existing) = existing {
            (existing.id, existing.created_at)
        } else {
            (new_id(IdPrefix::Pol), now.clone())
        };

        PluginTenantEnableKv {
            id,
            plugin_name: plugin_name.to_string(),
            tenant_id: tenant_id.to_string(),
            enabled,
            config_override_json,
            created_at,
            updated_at: now,
        }
    }
}
