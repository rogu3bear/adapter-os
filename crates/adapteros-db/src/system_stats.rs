//! System statistics database queries
//!
//! Provides methods for retrieving system-wide statistics including:
//! - Active session counts
//! - Active worker counts
//! - Active adapter counts
//! - Database health checks
//! - Table existence checks
//! - Row counting

use crate::{get_system_memory, Db};
use adapteros_core::validation::{Validator, ValidatorBuilder};
use adapteros_core::{AosError, Result};
use tracing::warn;

/// Validator for SQL table names to prevent injection attacks.
/// Table names must be alphanumeric with underscores only.
fn table_name_validator() -> Validator {
    ValidatorBuilder::new("table_name")
        .not_empty()
        .max_length(128)
        .with_chars("_") // Only underscores in addition to alphanumeric
        .starts_with_alphanumeric()
        .build()
}

fn is_schema_gap_error(message: &str) -> bool {
    message.contains("no such column") || message.contains("no such table")
}

impl Db {
    /// Count active chat sessions (sessions with activity in the last 24 hours)
    pub async fn count_active_chat_sessions(&self) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM chat_sessions WHERE last_activity_at > datetime('now', '-1 day')",
        )
        .fetch_one(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to count active chat sessions: {}", e)))?;

        Ok(count)
    }

    /// Count active workers (non-terminal lifecycle states)
    ///
    /// Includes 'pending' status to prevent race conditions where worker process
    /// has started but socket isn't bound yet (see WorkerStatus::Pending).
    pub async fn count_active_workers(&self) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM workers WHERE status IN ('pending','created','registered','healthy','draining')",
        )
        .fetch_one(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to count active workers: {}", e)))?;

        Ok(count)
    }

    /// Count active adapters (adapters with active = 1)
    pub async fn count_active_adapters(&self) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM adapters WHERE active = 1")
            .fetch_one(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(format!("Failed to count active adapters: {}", e)))?;

        Ok(count)
    }

    /// Check database health by executing a simple query
    ///
    /// Returns Ok(()) if database is responsive, Err otherwise.
    pub async fn check_database_health(&self) -> Result<()> {
        sqlx::query_scalar::<_, i64>("SELECT 1")
            .fetch_one(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(format!("Database health check failed: {}", e)))?;

        Ok(())
    }

    /// Check if a table exists in the database
    ///
    /// Returns true if the table exists, false otherwise.
    pub async fn table_exists(&self, table_name: &str) -> Result<bool> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
        )
        .bind(table_name)
        .fetch_one(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to check table existence: {}", e)))?;

        Ok(count > 0)
    }

    /// Count rows in a specific table
    ///
    /// # Safety
    /// This method uses string interpolation for the table name. Only use with
    /// validated/trusted table names to prevent SQL injection.
    pub async fn count_table_rows(&self, table_name: &str) -> Result<i64> {
        // Validate table name using centralized validator to prevent SQL injection
        table_name_validator()
            .validate(table_name)
            .map_err(|e| AosError::Validation(e.message))?;

        let query = format!("SELECT COUNT(*) FROM {}", table_name);
        let count = sqlx::query_scalar::<_, i64>(&query)
            .fetch_one(self.pool_result()?)
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to count rows in {}: {}", table_name, e))
            })?;

        Ok(count)
    }

    /// Count loaded models (adapters with ready current_state or legacy loaded state)
    pub async fn count_loaded_models(&self) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM adapters WHERE current_state IN ('warm', 'hot', 'resident') OR load_state IN ('loaded', 'warm')",
        )
        .fetch_one(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to count loaded models: {}", e)))?;

        Ok(count)
    }

    /// Count loaded adapters (same as count_loaded_models, for consistency)
    pub async fn count_loaded_adapters(&self) -> Result<i64> {
        self.count_loaded_models().await
    }

    /// Count active inference requests
    ///
    /// Note: This assumes an 'inference_requests' table exists with a 'status' column.
    /// Returns 0 if the table doesn't exist.
    pub async fn count_active_requests(&self) -> Result<i64> {
        // Check if table exists first
        let table_exists = self.table_exists("inference_requests").await?;
        if !table_exists {
            return Ok(0);
        }

        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM inference_requests WHERE status = 'active'",
        )
        .fetch_one(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to count active requests: {}", e)))?;

        Ok(count)
    }

    /// Count running training jobs
    pub async fn count_running_training_jobs(&self) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM training_jobs WHERE status = 'running'",
        )
        .fetch_one(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to count running training jobs: {}", e)))?;

        Ok(count)
    }

    /// Get adapter memory information for all active adapters
    pub async fn get_adapter_memory_info(&self) -> Result<Vec<AdapterMemoryInfo>> {
        // Primary query (newer schema)
        let primary = sqlx::query_as::<_, AdapterMemoryInfo>(
            "SELECT a.adapter_id, a.name, a.rank, a.current_state,
                    COALESCE(a.last_access_at, a.created_at) as last_access,
                    COALESCE(a.access_count, 0) as access_count,
                    CASE WHEN p.adapter_pk IS NOT NULL THEN 1 ELSE 0 END as pinned,
                    a.category
             FROM adapters a
             LEFT JOIN pinned_adapters p ON a.id = p.adapter_pk
             WHERE a.active = 1
             ORDER BY a.current_state DESC, a.last_access_at DESC",
        )
        .fetch_all(self.pool_result()?)
        .await;

        match primary {
            Ok(adapters) => Ok(adapters),
            Err(e) => {
                let err_str = e.to_string();
                let is_legacy_schema =
                    err_str.contains("no such column") || err_str.contains("no such table");

                if is_legacy_schema {
                    // Fallback for older SQLite schemas that lack last_access_at/access_count/pinned tables.
                    warn!(
                        error = %e,
                        "Falling back to legacy adapter memory query (schema missing columns)"
                    );

                    let fallback = sqlx::query_as::<_, AdapterMemoryInfo>(
                        "SELECT a.adapter_id,
                                a.name,
                                a.rank,
                                COALESCE(a.current_state, 'unknown') as current_state,
                                a.created_at as last_access,
                                0 as access_count,
                                0 as pinned,
                                a.category
                         FROM adapters a
                         WHERE a.active = 1
                         ORDER BY a.current_state DESC, a.created_at DESC",
                    )
                    .fetch_all(self.pool_result()?)
                    .await
                    .map_err(|fallback_err| {
                        AosError::Database(format!(
                            "Failed to get adapter memory info (legacy fallback): {}",
                            fallback_err
                        ))
                    })?;

                    Ok(fallback)
                } else {
                    Err(AosError::Database(format!(
                        "Failed to get adapter memory info: {}",
                        e
                    )))
                }
            }
        }
    }

    /// Get eviction candidates (adapters that could be evicted from memory)
    pub async fn get_eviction_candidates(&self, limit: i32) -> Result<Vec<EvictionCandidate>> {
        let candidates = sqlx::query_as::<_, EvictionCandidate>(
            "SELECT a.adapter_id, a.rank, a.current_state,
                    COALESCE(a.last_access_at, a.created_at) as last_access,
                    COALESCE(a.activation_pct, 0.0) as activation_rate
             FROM adapters a
             LEFT JOIN pinned_adapters p ON a.id = p.adapter_pk
             WHERE a.current_state IN ('warm', 'cold')
             AND p.adapter_pk IS NULL
             ORDER BY a.activation_pct ASC, a.last_access_at ASC
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get eviction candidates: {}", e)))?;

        Ok(candidates)
    }

    /// Get current memory usage percentage for a tenant from adapter records.
    ///
    /// Uses active adapter `memory_bytes` as the numerator and host memory as
    /// the denominator. Returns `0.0` when adapter schema is unavailable.
    pub async fn get_tenant_memory_usage_pct(&self, tenant_id: &str) -> Result<f64> {
        let total_adapter_bytes = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT COALESCE(SUM(COALESCE(memory_bytes, 0)), 0)
             FROM adapters
             WHERE tenant_id = ?
               AND active = 1
               AND current_state IN ('cold', 'warm', 'hot', 'resident')",
        )
        .bind(tenant_id)
        .fetch_one(self.pool_result()?)
        .await;

        let total_adapter_bytes = match total_adapter_bytes {
            Ok(bytes) => bytes.unwrap_or(0).max(0) as f64,
            Err(e) => {
                let err_str = e.to_string();
                if is_schema_gap_error(&err_str) {
                    warn!(
                        tenant_id = %tenant_id,
                        error = %e,
                        "Falling back to 0 tenant memory usage (schema gap)"
                    );
                    return Ok(0.0);
                }
                return Err(AosError::Database(format!(
                    "Failed to get tenant memory usage: {}",
                    e
                )));
            }
        };

        let system_memory = get_system_memory();
        let total_system_bytes = (system_memory.total_gb * 1024.0 * 1024.0 * 1024.0).max(1.0);
        let usage_pct = ((total_adapter_bytes / total_system_bytes) * 100.0).clamp(0.0, 100.0);
        Ok(usage_pct)
    }

    /// Get tenant-scoped eviction candidates ordered by policy preference:
    /// ephemeral category first, then colder states, then lower activation/LRU.
    pub async fn get_tenant_eviction_candidates(
        &self,
        tenant_id: &str,
        limit: i32,
    ) -> Result<Vec<TenantEvictionCandidate>> {
        let primary = sqlx::query_as::<_, TenantEvictionCandidate>(
            "SELECT a.adapter_id,
                    a.current_state,
                    a.category,
                    COALESCE(a.last_access_at, a.created_at) as last_access,
                    COALESCE(a.activation_pct, 0.0) as activation_rate
             FROM adapters a
             LEFT JOIN pinned_adapters p ON a.id = p.adapter_pk
             WHERE a.tenant_id = ?
               AND a.active = 1
               AND a.current_state IN ('cold', 'warm', 'hot', 'resident')
               AND p.adapter_pk IS NULL
             ORDER BY
               CASE WHEN COALESCE(a.category, '') = 'ephemeral' THEN 0 ELSE 1 END,
               CASE
                 WHEN a.current_state = 'cold' THEN 0
                 WHEN a.current_state = 'warm' THEN 1
                 WHEN a.current_state = 'hot' THEN 2
                 WHEN a.current_state = 'resident' THEN 3
                 ELSE 4
               END,
               COALESCE(a.activation_pct, 0.0) ASC,
               COALESCE(a.last_access_at, a.created_at) ASC
             LIMIT ?",
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(self.pool_result()?)
        .await;

        match primary {
            Ok(candidates) => Ok(candidates),
            Err(e) => {
                let err_str = e.to_string();
                if is_schema_gap_error(&err_str) {
                    warn!(
                        tenant_id = %tenant_id,
                        error = %e,
                        "Falling back to legacy tenant eviction candidate query"
                    );

                    let fallback = sqlx::query_as::<_, TenantEvictionCandidate>(
                        "SELECT a.adapter_id,
                                COALESCE(a.current_state, 'unknown') as current_state,
                                a.category,
                                a.created_at as last_access,
                                0.0 as activation_rate
                         FROM adapters a
                         WHERE a.tenant_id = ?
                           AND a.active = 1
                           AND COALESCE(a.current_state, 'unknown') IN ('cold', 'warm', 'hot', 'resident')
                         ORDER BY
                           CASE WHEN COALESCE(a.category, '') = 'ephemeral' THEN 0 ELSE 1 END,
                           CASE
                             WHEN COALESCE(a.current_state, 'unknown') = 'cold' THEN 0
                             WHEN COALESCE(a.current_state, 'unknown') = 'warm' THEN 1
                             WHEN COALESCE(a.current_state, 'unknown') = 'hot' THEN 2
                             WHEN COALESCE(a.current_state, 'unknown') = 'resident' THEN 3
                             ELSE 4
                           END,
                           a.created_at ASC
                         LIMIT ?",
                    )
                    .bind(tenant_id)
                    .bind(limit)
                    .fetch_all(self.pool_result()?)
                    .await
                    .map_err(|fallback_err| {
                        AosError::Database(format!(
                            "Failed to get tenant eviction candidates (legacy fallback): {}",
                            fallback_err
                        ))
                    })?;

                    Ok(fallback)
                } else {
                    Err(AosError::Database(format!(
                        "Failed to get tenant eviction candidates: {}",
                        e
                    )))
                }
            }
        }
    }

    /// Count adapters in various load states
    pub async fn count_adapters_by_load_state(&self) -> Result<Vec<(String, i64)>> {
        let counts = sqlx::query_as::<_, (String, i64)>(
            "SELECT COALESCE(load_state, 'unknown') as state, COUNT(*) as count
             FROM adapters
             WHERE active = 1
             GROUP BY load_state",
        )
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to count adapters by load state: {}", e))
        })?;

        Ok(counts)
    }
}

/// Adapter memory information record
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AdapterMemoryInfo {
    pub adapter_id: Option<String>,
    pub name: String,
    pub rank: i32,
    pub current_state: String,
    pub last_access: String,
    pub access_count: i64,
    pub pinned: i32,
    pub category: Option<String>,
}

/// Eviction candidate record
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EvictionCandidate {
    pub adapter_id: String,
    pub rank: i32,
    pub current_state: String,
    pub last_access: String,
    pub activation_rate: f32,
}

/// Tenant-scoped eviction candidate ordered for policy-driven memory pressure handling.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TenantEvictionCandidate {
    pub adapter_id: String,
    pub current_state: String,
    pub category: Option<String>,
    pub last_access: String,
    pub activation_rate: f32,
}
