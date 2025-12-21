use crate::adapters::Adapter; // assume
use crate::kv_backend::KvBackend;
use crate::tenants_kv::{CreateTenantParams, TenantKvOps, TenantKvRepository};
use crate::Db;
use adapteros_core::error_helpers::DbErrorExt;
use adapteros_core::tenant_snapshot::{AdapterInfo, PolicyInfo, StackInfo, TenantStateSnapshot};
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_storage::entities::tenant::TenantKv;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::collections::BTreeMap;
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Core policies enabled by default for new tenants
const CORE_POLICIES: &[&str] = &["egress", "determinism", "isolation", "evidence"];

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
    /// Default pinned adapter IDs for new chat sessions (JSON array)
    #[sqlx(default)]
    pub default_pinned_adapter_ids: Option<String>,
    /// KV cache quota in bytes (None = unlimited)
    #[sqlx(default)]
    pub max_kv_cache_bytes: Option<i64>,
    /// KV residency policy ID for cache management
    #[sqlx(default)]
    pub kv_residency_policy_id: Option<String>,
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

impl From<TenantKv> for Tenant {
    fn from(kv: TenantKv) -> Self {
        Self {
            id: kv.id,
            name: kv.name,
            itar_flag: kv.itar_flag,
            created_at: kv.created_at.to_rfc3339(),
            status: Some(kv.status),
            updated_at: Some(kv.updated_at.to_rfc3339()),
            default_stack_id: kv.default_stack_id,
            max_adapters: kv.max_adapters,
            max_training_jobs: kv.max_training_jobs,
            max_storage_gb: kv.max_storage_gb,
            rate_limit_rpm: kv.rate_limit_rpm,
            default_pinned_adapter_ids: kv.default_pinned_adapter_ids,
            // KV quota fields - default to None for KV backend (not yet supported in KV)
            max_kv_cache_bytes: None,
            kv_residency_policy_id: None,
        }
    }
}

impl Db {
    /// Get a TenantKvRepository if KV reads/writes are enabled
    fn get_tenant_kv_repo(&self) -> Option<TenantKvRepository> {
        if self.storage_mode().write_to_kv() || self.storage_mode().read_from_kv() {
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

        // SQL write if enabled
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, ?)")
                    .bind(&id)
                    .bind(name)
                    .bind(itar_flag)
                    .execute(pool)
                    .await
                    .map_err(|e| AosError::Database(e.to_string()))?;
            } else if !self.storage_mode().write_to_kv() {
                return Err(AosError::Database(
                    "SQL backend unavailable for create_tenant".to_string(),
                ));
            }
        }

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            let params = CreateTenantParams {
                name: name.to_string(),
                itar_flag,
            };
            if let Err(e) = repo.create_tenant_kv(&params).await {
                self.record_kv_write_fallback("tenants.create");
                warn!(error = %e, tenant_id = %id, "Failed to write tenant to KV backend (dual-write)");
            } else {
                debug!(tenant_id = %id, "Tenant written to both SQL and KV backends");
            }
        }

        // Initialize default policy bindings for new tenant (KV and/or SQL)
        if let Err(e) = self.initialize_tenant_policy_bindings(&id, "system").await {
            warn!(error = %e, tenant_id = %id, "Failed to initialize policy bindings for new tenant");
            // Non-fatal: tenant is created, bindings can be added later via migration or API
        }

        Ok(id)
    }

    /// Initialize default policy bindings for a tenant
    ///
    /// Creates policy bindings for all 24 canonical policies:
    /// - Core policies (egress, determinism, isolation, evidence) = enabled
    /// - All other policies = disabled
    ///
    /// This is called automatically when a new tenant is created.
    pub async fn initialize_tenant_policy_bindings(
        &self,
        tenant_id: &str,
        created_by: &str,
    ) -> Result<()> {
        // All 24 canonical policies from AGENTS.md
        let all_policies = [
            "egress",
            "determinism",
            "router",
            "evidence",
            "refusal",
            "numeric",
            "rag",
            "isolation",
            "telemetry",
            "retention",
            "performance",
            "memory",
            "artifacts",
            "secrets",
            "build_release",
            "compliance",
            "incident",
            "output",
            "adapters",
            "deterministic_io",
            "drift",
            "mplora",
            "naming",
            "dependency_security",
        ];

        let now = Utc::now().to_rfc3339();

        // KV path (supports kv_only / kv_primary)
        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_policy_binding_kv_repo() {
                for policy_id in all_policies {
                    let enabled = CORE_POLICIES.contains(&policy_id);
                    let binding = crate::tenant_policy_bindings_kv::TenantPolicyBindingKv {
                        id: Uuid::now_v7().to_string(),
                        tenant_id: tenant_id.to_string(),
                        policy_pack_id: policy_id.to_string(),
                        scope: "global".to_string(),
                        enabled,
                        created_at: Utc::now().to_string(),
                        created_by: created_by.to_string(),
                        updated_at: Utc::now().to_string(),
                        updated_by: Some(created_by.to_string()),
                    };
                    repo.upsert_binding(binding).await?;
                }
            }
        }

        // SQL path when available (dual-write)
        if self.storage_mode().write_to_sql() {
            if let Some(tx) = self.pool_opt().map(|p| p.begin()) {
                let mut tx = tx.await.map_err(|e| {
                    AosError::Database(format!("Failed to begin transaction: {}", e))
                })?;

                for policy_id in all_policies {
                    let id = Uuid::new_v4().to_string();
                    let enabled = CORE_POLICIES.contains(&policy_id);

                    sqlx::query(
                        r#"
                        INSERT INTO tenant_policy_bindings
                        (id, tenant_id, policy_pack_id, scope, enabled, created_at, created_by, updated_at)
                        VALUES (?, ?, ?, 'global', ?, ?, ?, ?)
                        "#,
                    )
                    .bind(&id)
                    .bind(tenant_id)
                    .bind(policy_id)
                    .bind(if enabled { 1 } else { 0 })
                    .bind(&now)
                    .bind(created_by)
                    .bind(&now)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| {
                        AosError::Database(format!(
                            "Failed to initialize policy binding for {}: {}",
                            policy_id, e
                        ))
                    })?;
                }

                tx.commit().await.map_err(|e| {
                    AosError::Database(format!("Failed to commit transaction: {}", e))
                })?;
            } else if !self.storage_mode().write_to_kv() {
                return Err(AosError::Database(
                    "Policy binding init failed: SQL unavailable and KV disabled".to_string(),
                ));
            }
        }

        info!(
            tenant_id = %tenant_id,
            created_by = %created_by,
            total_policies = all_policies.len(),
            core_enabled = CORE_POLICIES.len(),
            mode = %self.storage_mode(),
            "Initialized tenant policy bindings"
        );

        Ok(())
    }

    pub async fn get_tenant(&self, id: &str) -> Result<Option<Tenant>> {
        // KV primary path
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_tenant_kv_repo() {
                match repo.get_tenant_kv(id).await {
                    Ok(Some(kv)) => return Ok(Some(Tenant::from(kv))),
                    Ok(None) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("tenants.get.miss");
                    }
                    Ok(None) => return Ok(None),
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("tenants.get.error");
                        warn!(error = %e, tenant_id = %id, "KV tenant read failed, falling back to SQL");
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        let pool = self.pool_opt().ok_or_else(|| {
            AosError::Database("SQL backend unavailable for get_tenant".to_string())
        })?;

        let tenant = sqlx::query_as::<_, Tenant>(
            "SELECT id, name, itar_flag, created_at, status, updated_at, default_stack_id,
                    max_adapters, max_training_jobs, max_storage_gb, rate_limit_rpm,
                    default_pinned_adapter_ids, max_kv_cache_bytes, kv_residency_policy_id
             FROM tenants WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(tenant)
    }

    pub async fn list_tenants(&self) -> Result<Vec<Tenant>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_tenant_kv_repo() {
                match repo.list_tenants_kv().await {
                    Ok(mut kv_tenants) => {
                        if kv_tenants.is_empty() && self.storage_mode().sql_fallback_enabled() {
                            self.record_kv_read_fallback("tenants.list.empty");
                        } else {
                            kv_tenants.sort_by(|a, b| {
                                b.created_at
                                    .cmp(&a.created_at)
                                    .then_with(|| a.id.cmp(&b.id))
                            });
                            return Ok(kv_tenants.into_iter().map(Tenant::from).collect());
                        }
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("tenants.list.error");
                        warn!(error = %e, "KV tenant list failed, falling back to SQL");
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        let pool = self.pool_opt().ok_or_else(|| {
            AosError::Database("SQL backend unavailable for list_tenants".to_string())
        })?;

        let tenants = sqlx::query_as::<_, Tenant>(
            "SELECT id, name, itar_flag, created_at, status, updated_at, default_stack_id,
                    max_adapters, max_training_jobs, max_storage_gb, rate_limit_rpm,
                    default_pinned_adapter_ids, max_kv_cache_bytes, kv_residency_policy_id
             FROM tenants ORDER BY created_at DESC",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(tenants)
    }

    /// List tenants with pagination
    pub async fn list_tenants_paginated(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Tenant>, i64)> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_tenant_kv_repo() {
                match repo.list_tenants_kv().await {
                    Ok(mut kv_tenants) => {
                        if kv_tenants.is_empty() && self.storage_mode().sql_fallback_enabled() {
                            self.record_kv_read_fallback("tenants.list_paginated.empty");
                        } else {
                            kv_tenants.sort_by(|a, b| {
                                b.created_at
                                    .cmp(&a.created_at)
                                    .then_with(|| a.id.cmp(&b.id))
                            });

                            let total = kv_tenants.len() as i64;
                            let start = offset.max(0) as usize;
                            let end = (start + limit.max(0) as usize).min(kv_tenants.len());
                            let window = if start < end {
                                kv_tenants[start..end].to_vec()
                            } else {
                                Vec::new()
                            };

                            return Ok((window.into_iter().map(Tenant::from).collect(), total));
                        }
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("tenants.list_paginated.error");
                        warn!(error = %e, "KV tenant pagination failed, falling back to SQL");
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        let pool = self.pool_opt().ok_or_else(|| {
            AosError::Database("SQL backend unavailable for list_tenants_paginated".to_string())
        })?;

        // Get total count
        let total = sqlx::query("SELECT COUNT(*) as cnt FROM tenants")
            .fetch_one(pool)
            .await
            .db_err("count tenants")?
            .get::<i64, _>(0);

        // Get paginated results
        let tenants = sqlx::query_as::<_, Tenant>(
            "SELECT id, name, itar_flag, created_at, status, updated_at, default_stack_id,
                    max_adapters, max_training_jobs, max_storage_gb, rate_limit_rpm,
                    default_pinned_adapter_ids, max_kv_cache_bytes, kv_residency_policy_id
             FROM tenants ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .db_err("list tenants paginated")?;

        Ok((tenants, total))
    }

    /// Ensure the system tenant exists across storage backends.
    pub async fn ensure_system_tenant(&self) -> Result<()> {
        if self.get_tenant("system").await?.is_some() {
            return Ok(());
        }

        // KV creation when allowed
        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_tenant_kv_repo() {
                let params = CreateTenantParams {
                    name: "System".to_string(),
                    itar_flag: false,
                };
                // Ignore already-exists errors (best effort)
                if let Err(e) = repo.create_tenant_kv_with_id("system", &params).await {
                    if !self.storage_mode().write_to_sql() {
                        return Err(e);
                    } else {
                        warn!(error = %e, "KV system tenant creation failed; will attempt SQL");
                    }
                }
            }
        }

        // SQL creation when available
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                sqlx::query(
                    "INSERT OR IGNORE INTO tenants (id, name, itar_flag, created_at) VALUES ('system', 'System', 0, datetime('now'))",
                )
                .execute(pool)
                .await
                .map_err(|e| AosError::Database(format!("Failed to create system tenant: {}", e)))?;
            } else if !self.storage_mode().write_to_kv() {
                return Err(AosError::Database(
                    "SQL backend unavailable for system tenant creation".to_string(),
                ));
            }
        }

        // Initialize default policy bindings (covers KV + SQL)
        self.initialize_tenant_policy_bindings("system", "system")
            .await?;
        Ok(())
    }

    /// Rename a tenant
    pub async fn rename_tenant(&self, id: &str, new_name: &str) -> Result<()> {
        sqlx::query("UPDATE tenants SET name = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(new_name)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            if let Err(e) = repo.rename_tenant_kv(id, new_name).await {
                self.record_kv_write_fallback("tenants.rename");
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
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            if let Err(e) = repo.update_tenant_itar_flag_kv(id, itar_flag).await {
                self.record_kv_write_fallback("tenants.update_itar");
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
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            if let Err(e) = repo.pause_tenant_kv(id).await {
                self.record_kv_write_fallback("tenants.pause");
                warn!(error = %e, tenant_id = %id, "Failed to pause tenant in KV backend (dual-write)");
            }
        }

        Ok(())
    }

    /// Archive a tenant and cascade to their adapters
    ///
    /// This operation:
    /// 1. Archives all active adapters belonging to the tenant (sets `archived_at`)
    /// 2. Sets the tenant status to 'archived'
    ///
    /// Both operations happen in a single transaction to ensure consistency.
    /// The .aos files are NOT deleted - that's handled by GC based on age policy.
    pub async fn archive_tenant(&self, id: &str) -> Result<()> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Archive all adapters for this tenant
        let adapter_result = sqlx::query(
            "UPDATE adapters
             SET archived_at = datetime('now'),
                 archived_by = 'system',
                 archive_reason = 'tenant_archived',
                 updated_at = datetime('now')
             WHERE tenant_id = ?
               AND archived_at IS NULL
               AND active = 1",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(format!("Failed to archive tenant adapters: {}", e)))?;

        let adapters_archived = adapter_result.rows_affected();

        // Archive the tenant itself
        sqlx::query(
            "UPDATE tenants SET status = 'archived', updated_at = datetime('now') WHERE id = ?",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(format!("Failed to archive tenant: {}", e)))?;

        tx.commit()
            .await
            .map_err(|e| AosError::Database(format!("Failed to commit transaction: {}", e)))?;

        info!(
            tenant_id = %id,
            adapters_archived = adapters_archived,
            "Archived tenant with adapter cascade"
        );

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            if let Err(e) = repo.archive_tenant_kv(id).await {
                self.record_kv_write_fallback("tenants.archive");
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
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            if let Err(e) = repo.activate_tenant_kv(id).await {
                self.record_kv_write_fallback("tenants.activate");
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
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update tenant limits: {}", e)))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            if let Err(e) = repo
                .update_tenant_limits_kv(
                    id,
                    max_adapters,
                    max_training_jobs,
                    max_storage_gb,
                    rate_limit_rpm,
                )
                .await
            {
                self.record_kv_write_fallback("tenants.update_limits");
                warn!(error = %e, tenant_id = %id, "Failed to update tenant limits in KV backend (dual-write)");
            }
        }

        Ok(())
    }

    /// Update tenant KV cache quota and residency policy
    ///
    /// Pass `None` to clear/disable quota enforcement (unlimited).
    pub async fn update_tenant_kv_quota(
        &self,
        id: &str,
        max_kv_cache_bytes: Option<i64>,
        kv_residency_policy_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE tenants
             SET max_kv_cache_bytes = ?, kv_residency_policy_id = ?,
                 updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(max_kv_cache_bytes)
        .bind(kv_residency_policy_id)
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update tenant KV quota: {}", e)))?;

        info!(
            tenant_id = %id,
            max_kv_cache_bytes = ?max_kv_cache_bytes,
            kv_residency_policy_id = ?kv_residency_policy_id,
            "Updated tenant KV cache quota"
        );

        Ok(())
    }

    /// Get tenant's default pinned adapter IDs
    ///
    /// Returns the parsed list of adapter IDs, or None if not set.
    pub async fn get_tenant_default_pinned_adapters(
        &self,
        tenant_id: &str,
    ) -> Result<Option<Vec<String>>> {
        let json: Option<String> =
            sqlx::query_scalar("SELECT default_pinned_adapter_ids FROM tenants WHERE id = ?")
                .bind(tenant_id)
                .fetch_optional(self.pool())
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to get tenant pinned adapters: {}", e))
                })?
                .flatten();

        match json {
            Some(s) => {
                let ids: Vec<String> = serde_json::from_str(&s).map_err(|e| {
                    AosError::Validation(format!("Invalid pinned adapter IDs JSON: {}", e))
                })?;
                Ok(Some(ids))
            }
            None => Ok(None),
        }
    }

    /// Set tenant's default pinned adapter IDs
    ///
    /// Pass `None` to clear the default pinned adapters.
    /// Pass `Some(&[])` to explicitly set an empty list.
    pub async fn set_tenant_default_pinned_adapters(
        &self,
        tenant_id: &str,
        adapter_ids: Option<&[String]>,
    ) -> Result<()> {
        let json = adapter_ids.map(|ids| serde_json::to_string(ids).unwrap());

        sqlx::query(
            "UPDATE tenants SET default_pinned_adapter_ids = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(&json)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to set tenant pinned adapters: {}", e))
        })?;

        debug!(tenant_id = %tenant_id, "Updated tenant default pinned adapters");
        Ok(())
    }

    /// Get tenant usage statistics
    pub async fn get_tenant_usage(&self, tenant_id: &str) -> Result<TenantUsage> {
        // Count active adapters
        let adapter_count =
            sqlx::query("SELECT COUNT(*) as cnt FROM adapters WHERE tenant_id = ? AND active = 1")
                .bind(tenant_id)
                .fetch_one(self.pool())
                .await
                .db_err("count adapters")?
                .get::<i32, _>(0);

        // Count running training jobs
        let training_jobs_count = sqlx::query(
            "SELECT COUNT(*) as cnt FROM training_jobs WHERE tenant_id = ? AND status = 'running'",
        )
        .bind(tenant_id)
        .fetch_one(self.pool())
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
        .fetch_optional(self.pool())
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
            .fetch_all(self.pool())
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
        .fetch_all(self.pool())
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

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            if let Err(e) = repo.set_default_stack_kv(tenant_id, stack_id).await {
                self.record_kv_write_fallback("tenants.set_default_stack");
                warn!(error = %e, tenant_id = %tenant_id, "Failed to set default stack in KV backend (dual-write)");
            }
        }

        Ok(())
    }

    /// Clear default stack for a tenant
    pub async fn clear_default_stack(&self, tenant_id: &str) -> Result<()> {
        sqlx::query("UPDATE tenants SET default_stack_id = NULL WHERE id = ?")
            .bind(tenant_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to clear default stack: {}", e)))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_tenant_kv_repo() {
            if let Err(e) = repo.clear_default_stack_kv(tenant_id).await {
                self.record_kv_write_fallback("tenants.clear_default_stack");
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
        .execute(self.pool())
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
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to assign adapter to tenant: {}", e)))?;
        Ok(())
    }
}
