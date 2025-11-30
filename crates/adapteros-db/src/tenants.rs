use crate::adapters::Adapter; // assume
use crate::kv_backend::KvBackend;
use crate::tenants_kv::{CreateTenantParams, TenantKvOps, TenantKvRepository};
use crate::Db;
use adapteros_core::error_helpers::DbErrorExt;
use adapteros_core::tenant_snapshot::{AdapterInfo, PolicyInfo, StackInfo, TenantStateSnapshot};
use adapteros_core::{AosError, B3Hash, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::collections::BTreeMap;
use std::sync::Arc;
use tracing::{debug, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Tenant {
    pub id: String,
    pub name: String,
    pub itar_flag: bool,
    pub created_at: String,
    #[sqlx(default)]
    pub status: Option<String>,
    #[sqlx(default)]
    pub updated_at: Option<String>,
    #[sqlx(default)]
    pub default_stack_id: Option<String>,
    #[sqlx(default)]
    pub max_adapters: Option<i32>,
    #[sqlx(default)]
    pub max_training_jobs: Option<i32>,
    #[sqlx(default)]
    pub max_storage_gb: Option<f64>,
    #[sqlx(default)]
    pub rate_limit_rpm: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantUsage {
    pub tenant_id: String,
    pub active_adapters_count: i32,
    pub running_training_jobs: i32,
    pub inference_count_24h: i64,
    pub storage_used_gb: f64,
    pub cpu_usage_pct: f64,
    pub gpu_usage_pct: f64,
    pub memory_used_gb: f64,
    pub memory_total_gb: f64,
}

impl Db {
    /// Get a TenantKvRepository if KV writes are enabled
    fn get_tenant_kv_repo(&self) -> Option<TenantKvRepository> {
        if self.storage_mode().write_to_kv() {
            self.kv_backend().map(|kv| {
                let kv_backend: Arc<dyn KvBackend> = kv.clone();
                TenantKvRepository::new(kv_backend)
            })
        } else {
            None
        }
    }

    pub async fn create_tenant(&self, name: &str, itar_flag: bool) -> Result<String> {
        let id = Uuid::now_v7().to_string();

        // SQL write (always happens)
        sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, ?)")
            .bind(&id)
            .bind(name)
            .bind(itar_flag)
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            let params = CreateTenantParams {
                name: name.to_string(),
                itar_flag,
            };
            if let Err(e) = repo.create_tenant_kv(&params).await {
                warn!(error = %e, tenant_id = %id, "Failed to write tenant to KV backend (dual-write)");
            } else {
                debug!(tenant_id = %id, "Tenant written to both SQL and KV backends");
            }
        }

        Ok(id)
    }

    pub async fn get_tenant(&self, id: &str) -> Result<Option<Tenant>> {
        let tenant = sqlx::query_as::<_, Tenant>(
            "SELECT id, name, itar_flag, created_at, status, updated_at, default_stack_id,
                    max_adapters, max_training_jobs, max_storage_gb, rate_limit_rpm
             FROM tenants WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(tenant)
    }

    pub async fn list_tenants(&self) -> Result<Vec<Tenant>> {
        let tenants = sqlx::query_as::<_, Tenant>(
            "SELECT id, name, itar_flag, created_at, status, updated_at, default_stack_id,
                    max_adapters, max_training_jobs, max_storage_gb, rate_limit_rpm
             FROM tenants ORDER BY created_at DESC",
        )
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(tenants)
    }

    /// List tenants with pagination
    pub async fn list_tenants_paginated(&self, limit: i64, offset: i64) -> Result<(Vec<Tenant>, i64)> {
        // Get total count
        let total = sqlx::query("SELECT COUNT(*) as cnt FROM tenants")
            .fetch_one(&*self.pool())
            .await
            .db_err("count tenants")?
            .get::<i64, _>(0);

        // Get paginated results
        let tenants = sqlx::query_as::<_, Tenant>(
            "SELECT id, name, itar_flag, created_at, status, updated_at, default_stack_id,
                    max_adapters, max_training_jobs, max_storage_gb, rate_limit_rpm
             FROM tenants ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool())
        .await
        .db_err("list tenants paginated")?;

        Ok((tenants, total))
    }

    /// Rename a tenant
    pub async fn rename_tenant(&self, id: &str, new_name: &str) -> Result<()> {
        sqlx::query("UPDATE tenants SET name = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(new_name)
            .bind(id)
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            if let Err(e) = repo.rename_tenant_kv(id, new_name).await {
                warn!(error = %e, tenant_id = %id, "Failed to rename tenant in KV backend (dual-write)");
            }
        }

        Ok(())
    }

    /// Update tenant ITAR flag
    pub async fn update_tenant_itar_flag(&self, id: &str, itar_flag: bool) -> Result<()> {
        sqlx::query("UPDATE tenants SET itar_flag = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(itar_flag)
            .bind(id)
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            if let Err(e) = repo.update_tenant_itar_flag_kv(id, itar_flag).await {
                warn!(error = %e, tenant_id = %id, "Failed to update tenant ITAR flag in KV backend (dual-write)");
            }
        }

        Ok(())
    }

    /// Pause a tenant
    pub async fn pause_tenant(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE tenants SET status = 'paused', updated_at = datetime('now') WHERE id = ?",
        )
        .bind(id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            if let Err(e) = repo.pause_tenant_kv(id).await {
                warn!(error = %e, tenant_id = %id, "Failed to pause tenant in KV backend (dual-write)");
            }
        }

        Ok(())
    }

    /// Archive a tenant
    pub async fn archive_tenant(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE tenants SET status = 'archived', updated_at = datetime('now') WHERE id = ?",
        )
        .bind(id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            if let Err(e) = repo.archive_tenant_kv(id).await {
                warn!(error = %e, tenant_id = %id, "Failed to archive tenant in KV backend (dual-write)");
            }
        }

        Ok(())
    }

    /// Reactivate a paused or archived tenant
    pub async fn activate_tenant(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE tenants SET status = 'active', updated_at = datetime('now') WHERE id = ?",
        )
        .bind(id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            if let Err(e) = repo.activate_tenant_kv(id).await {
                warn!(error = %e, tenant_id = %id, "Failed to activate tenant in KV backend (dual-write)");
            }
        }

        Ok(())
    }

    /// Update tenant resource limits
    pub async fn update_tenant_limits(
        &self,
        id: &str,
        max_adapters: Option<i32>,
        max_training_jobs: Option<i32>,
        max_storage_gb: Option<f64>,
        rate_limit_rpm: Option<i32>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE tenants
             SET max_adapters = ?, max_training_jobs = ?, max_storage_gb = ?, rate_limit_rpm = ?,
                 updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(max_adapters)
        .bind(max_training_jobs)
        .bind(max_storage_gb)
        .bind(rate_limit_rpm)
        .bind(id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update tenant limits: {}", e)))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            if let Err(e) = repo.update_tenant_limits_kv(id, max_adapters, max_training_jobs, max_storage_gb, rate_limit_rpm).await {
                warn!(error = %e, tenant_id = %id, "Failed to update tenant limits in KV backend (dual-write)");
            }
        }

        Ok(())
    }

    /// Get tenant usage statistics
    pub async fn get_tenant_usage(&self, tenant_id: &str) -> Result<TenantUsage> {
        // Count active adapters
        let adapter_count =
            sqlx::query("SELECT COUNT(*) as cnt FROM adapters WHERE tenant_id = ? AND active = 1")
                .bind(tenant_id)
                .fetch_one(&*self.pool())
                .await
                .db_err("count adapters")?
                .get::<i32, _>(0);

        // Count running training jobs
        let training_jobs_count = sqlx::query(
            "SELECT COUNT(*) as cnt FROM training_jobs WHERE tenant_id = ? AND status = 'running'",
        )
        .bind(tenant_id)
        .fetch_one(&*self.pool())
        .await
        .db_err("count training jobs")?
        .get::<i32, _>(0);

        // Count inference operations in last 24h
        let inference_count_24h = sqlx::query(
            "SELECT COUNT(*) as cnt FROM audit_logs
             WHERE tenant_id = ? AND action = 'inference.execute'
             AND created_at >= datetime('now', '-24 hours')",
        )
        .bind(tenant_id)
        .fetch_optional(&*self.pool())
        .await
        .db_err("count inference operations")?
        .map(|row| row.get::<i32, _>(0))
        .unwrap_or(0);

        Ok(TenantUsage {
            tenant_id: tenant_id.to_string(),
            active_adapters_count: adapter_count,
            running_training_jobs: training_jobs_count,
            inference_count_24h: inference_count_24h as i64,
            storage_used_gb: 0.0, // TODO: calculate from artifacts
            cpu_usage_pct: 0.0,   // TODO: from system metrics
            gpu_usage_pct: 0.0,   // TODO: from system metrics
            memory_used_gb: 0.0,  // TODO: from system metrics
            memory_total_gb: 0.0, // TODO: from system metrics
        })
    }

    pub async fn store_tenant_snapshot_hash(
        &self,
        tenant_id: &str,
        state_hash: &B3Hash,
    ) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO tenant_snapshots (tenant_id, state_hash, created_at) VALUES (?, ?, datetime('now'))")
            .bind(tenant_id)
            .bind(state_hash.to_hex())
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn get_tenant_snapshot_hash(&self, tenant_id: &str) -> Result<Option<B3Hash>> {
        let hash_str = sqlx::query("SELECT state_hash FROM tenant_snapshots WHERE tenant_id = ? ORDER BY created_at DESC LIMIT 1")
            .bind(tenant_id)
            .fetch_optional(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?
            .map(|row| row.get::<String, _>(0));
        Ok(hash_str.and_then(|s| B3Hash::from_hex(&s).ok()))
    }

    pub async fn build_tenant_snapshot(&self, tenant_id: &str) -> Result<TenantStateSnapshot> {
        // Adapters - use system-level API for snapshot building
        let all_adapters = self.list_all_adapters_system().await?;
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
            .fetch_all(&*self.pool())
            .await
            .db_err("query policies")?;

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
        .fetch_all(&*self.pool())
        .await
        .db_err("query configs")?;

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
            .fetch_optional(&*self.pool())
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
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to set default stack: {}", e)))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            if let Err(e) = repo.set_default_stack_kv(tenant_id, stack_id).await {
                warn!(error = %e, tenant_id = %tenant_id, "Failed to set default stack in KV backend (dual-write)");
            }
        }

        Ok(())
    }

    /// Clear default stack for a tenant
    pub async fn clear_default_stack(&self, tenant_id: &str) -> Result<()> {
        sqlx::query("UPDATE tenants SET default_stack_id = NULL WHERE id = ?")
            .bind(tenant_id)
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to clear default stack: {}", e)))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            if let Err(e) = repo.clear_default_stack_kv(tenant_id).await {
                warn!(error = %e, tenant_id = %tenant_id, "Failed to clear default stack in KV backend (dual-write)");
            }
        }

        Ok(())
    }

    /// Assign a policy to a tenant
    pub async fn assign_policy_to_tenant(
        &self,
        tenant_id: &str,
        policy_id: &str,
        assigned_by: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO tenant_policies (tenant_id, cpid, assigned_by, assigned_at)
             VALUES (?, ?, ?, datetime('now'))",
        )
        .bind(tenant_id)
        .bind(policy_id)
        .bind(assigned_by)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to assign policy to tenant: {}", e)))?;
        Ok(())
    }

    /// Assign an adapter to a tenant
    pub async fn assign_adapter_to_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        assigned_by: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO tenant_adapters (tenant_id, adapter_id, assigned_by, assigned_at)
             VALUES (?, ?, ?, datetime('now'))"
        )
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(assigned_by)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to assign adapter to tenant: {}", e)))?;
        Ok(())
    }
}
