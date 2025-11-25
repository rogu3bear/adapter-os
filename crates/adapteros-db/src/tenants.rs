use crate::adapters::Adapter; // assume
use crate::Db;
use adapteros_core::tenant_snapshot::{AdapterInfo, PolicyInfo, StackInfo, TenantStateSnapshot};
use adapteros_core::{AosError, B3Hash, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Tenant {
    pub id: String,
    pub name: String,
    pub itar_flag: bool,
    pub created_at: String,
}

impl Db {
    pub async fn create_tenant(&self, name: &str, itar_flag: bool) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, ?)")
            .bind(&id)
            .bind(name)
            .bind(itar_flag)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(id)
    }

    pub async fn get_tenant(&self, id: &str) -> Result<Option<Tenant>> {
        let tenant = sqlx::query_as::<_, Tenant>(
            "SELECT id, name, itar_flag, created_at FROM tenants WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(tenant)
    }

    pub async fn list_tenants(&self) -> Result<Vec<Tenant>> {
        let tenants = sqlx::query_as::<_, Tenant>(
            "SELECT id, name, itar_flag, created_at FROM tenants ORDER BY created_at DESC",
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(tenants)
    }

    /// Rename a tenant
    pub async fn rename_tenant(&self, id: &str, new_name: &str) -> Result<()> {
        sqlx::query("UPDATE tenants SET name = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(new_name)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    /// Update tenant ITAR flag
    pub async fn update_tenant_itar_flag(&self, id: &str, itar_flag: bool) -> Result<()> {
        sqlx::query("UPDATE tenants SET itar_flag = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(itar_flag)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    /// Pause a tenant
    pub async fn pause_tenant(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE tenants SET status = 'paused', updated_at = datetime('now') WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    /// Archive a tenant
    pub async fn archive_tenant(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE tenants SET status = 'archived', updated_at = datetime('now') WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn store_tenant_snapshot_hash(
        &self,
        tenant_id: &str,
        state_hash: &B3Hash,
    ) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO tenant_snapshots (tenant_id, state_hash, created_at) VALUES (?, ?, datetime('now'))")
            .bind(tenant_id)
            .bind(state_hash.to_hex())
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn get_tenant_snapshot_hash(&self, tenant_id: &str) -> Result<Option<B3Hash>> {
        let hash_str = sqlx::query("SELECT state_hash FROM tenant_snapshots WHERE tenant_id = ? ORDER BY created_at DESC LIMIT 1")
            .bind(tenant_id)
            .fetch_optional(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?
            .map(|row| row.get::<String, _>(0));
        Ok(hash_str.and_then(|s| B3Hash::from_hex(&s).ok()))
    }

    pub async fn build_tenant_snapshot(&self, tenant_id: &str) -> Result<TenantStateSnapshot> {
        // Adapters
        let all_adapters = self.list_adapters().await?;
        let adapters: Vec<&Adapter> = all_adapters
            .iter()
            .filter(|a| a.tenant_id == tenant_id)
            .collect();
        let adapter_infos: Vec<AdapterInfo> = adapters
            .iter()
            .map(|a| AdapterInfo {
                id: a.id.clone(), // assume String
                name: a.name.clone(),
                rank: a.rank as u32,
                version: "1.0".to_string(), // since no version
            })
            .collect();

        // Stacks
        let stacks = self.list_stacks_for_tenant(tenant_id).await?;
        let stack_infos: Vec<StackInfo> = stacks
            .iter()
            .map(|s| {
                let adapter_ids: Vec<String> =
                    serde_json::from_str(&s.adapter_ids_json).unwrap_or_default();
                StackInfo {
                    name: s.name.clone(),
                    adapter_ids,
                }
            })
            .collect();

        // Router policies
        let mut policies_rs = Vec::new();
        let rows = sqlx::query("SELECT name, rules_json FROM router_policies WHERE tenant_id = ?")
            .bind(tenant_id)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to query policies: {}", e)))?;

        for row in rows {
            let name: String = row.get(0);
            let rules_json: String = row.get(1);
            policies_rs.push((name, rules_json));
        }

        let router_policies: Vec<PolicyInfo> = policies_rs
            .iter()
            .map(|(name, rules_json)| {
                let rules: Vec<String> = serde_json::from_str(rules_json).unwrap_or_default();
                PolicyInfo {
                    name: name.clone(),
                    rules,
                }
            })
            .collect();

        // Configs
        let mut config_rs = Vec::new();
        let config_rows = sqlx::query(
            "SELECT key, value_json FROM tenant_configs WHERE tenant_id = ? ORDER BY key",
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to query configs: {}", e)))?;

        for row in config_rows {
            let key: String = row.get(0);
            let value_json: String = row.get(1);
            config_rs.push((key, value_json));
        }

        let mut configs = BTreeMap::new();
        let mut plugin_configs = BTreeMap::new();
        let mut feature_flags = BTreeMap::new();

        for (key, value_json) in config_rs {
            let value: Value =
                serde_json::from_str(&value_json).map_err(|e| AosError::Serialization(e))?;
            if key.starts_with("plugin.") {
                let sub_key = key[7..].to_string();
                plugin_configs.insert(sub_key, value.clone());
            } else if key.starts_with("flag.") {
                if let Some(enabled) = value.as_bool() {
                    let flag_key = key[5..].to_string();
                    feature_flags.insert(flag_key, enabled);
                } else {
                    configs.insert(key.clone(), value.clone());
                }
            } else {
                configs.insert(key, value);
            }
        }

        let snapshot_timestamp = Utc::now();

        Ok(TenantStateSnapshot {
            tenant_id: tenant_id.to_string(),
            adapters: adapter_infos,
            stacks: stack_infos,
            router_policies,
            plugin_configs,
            feature_flags,
            configs,
            snapshot_timestamp,
        })
    }

    /// Get default stack ID for a tenant
    pub async fn get_default_stack(&self, tenant_id: &str) -> Result<Option<String>> {
        let stack_id = sqlx::query("SELECT default_stack_id FROM tenants WHERE id = ?")
            .bind(tenant_id)
            .fetch_optional(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to get default stack: {}", e)))?
            .and_then(|row| row.get::<Option<String>, _>(0));

        Ok(stack_id)
    }

    /// Set default stack for a tenant
    pub async fn set_default_stack(&self, tenant_id: &str, stack_id: &str) -> Result<()> {
        // Verify stack exists and belongs to tenant
        let stack = self.get_stack(tenant_id, stack_id).await?;
        if stack.is_none() {
            return Err(AosError::Database(format!(
                "Stack {} not found for tenant {}",
                stack_id, tenant_id
            )));
        }

        sqlx::query("UPDATE tenants SET default_stack_id = ? WHERE id = ?")
            .bind(stack_id)
            .bind(tenant_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to set default stack: {}", e)))?;

        Ok(())
    }

    /// Clear default stack for a tenant
    pub async fn clear_default_stack(&self, tenant_id: &str) -> Result<()> {
        sqlx::query("UPDATE tenants SET default_stack_id = NULL WHERE id = ?")
            .bind(tenant_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to clear default stack: {}", e)))?;

        Ok(())
    }

    /// Assign a policy to a tenant
    pub async fn assign_policy_to_tenant(&self, tenant_id: &str, policy_id: &str, assigned_by: &str) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO tenant_policies (tenant_id, cpid, assigned_by, assigned_at)
             VALUES (?, ?, ?, datetime('now'))"
        )
        .bind(tenant_id)
        .bind(policy_id)
        .bind(assigned_by)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to assign policy to tenant: {}", e)))?;
        Ok(())
    }

    /// Assign an adapter to a tenant
    pub async fn assign_adapter_to_tenant(&self, tenant_id: &str, adapter_id: &str, assigned_by: &str) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO tenant_adapters (tenant_id, adapter_id, assigned_by, assigned_at)
             VALUES (?, ?, ?, datetime('now'))"
        )
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(assigned_by)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to assign adapter to tenant: {}", e)))?;
        Ok(())
    }
}
