//! System statistics database queries
//!
//! Provides methods for retrieving system-wide statistics including:
//! - Active session counts
//! - Active worker counts
//! - Active adapter counts
//! - Database health checks
//! - Table existence checks
//! - Row counting

use crate::Db;
use adapteros_core::{AosError, Result};

impl Db {
    /// Count active chat sessions (sessions with activity in the last 24 hours)
    pub async fn count_active_chat_sessions(&self) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM chat_sessions WHERE last_activity_at > datetime('now', '-1 day')",
        )
        .fetch_one(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to count active chat sessions: {}", e))
        })?;

        Ok(count)
    }

    /// Count active workers (workers with status 'serving' or 'starting')
    pub async fn count_active_workers(&self) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM workers WHERE status IN ('serving', 'starting')",
        )
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to count active workers: {}", e)))?;

        Ok(count)
    }

    /// Count active adapters (adapters with active = 1)
    pub async fn count_active_adapters(&self) -> Result<i64> {
        let count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM adapters WHERE active = 1")
                .fetch_one(self.pool())
                .await
                .map_err(|e| AosError::Database(format!("Failed to count active adapters: {}", e)))?;

        Ok(count)
    }

    /// Check database health by executing a simple query
    ///
    /// Returns Ok(()) if database is responsive, Err otherwise.
    pub async fn check_database_health(&self) -> Result<()> {
        sqlx::query_scalar::<_, i64>("SELECT 1")
            .fetch_one(self.pool())
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
        .fetch_one(self.pool())
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
        // Validate table name to prevent SQL injection
        if !table_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_')
        {
            return Err(AosError::Validation(format!(
                "Invalid table name: {}",
                table_name
            )));
        }

        let query = format!("SELECT COUNT(*) FROM {}", table_name);
        let count = sqlx::query_scalar::<_, i64>(&query)
            .fetch_one(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to count rows in {}: {}", table_name, e))
            })?;

        Ok(count)
    }

    /// Count loaded models (adapters with load_state = 'loaded')
    pub async fn count_loaded_models(&self) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM adapters WHERE load_state = 'loaded'",
        )
        .fetch_one(self.pool())
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
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to count active requests: {}", e)))?;

        Ok(count)
    }

    /// Count running training jobs
    pub async fn count_running_training_jobs(&self) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM training_jobs WHERE status = 'running'",
        )
        .fetch_one(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to count running training jobs: {}", e))
        })?;

        Ok(count)
    }

    /// Get adapter memory information for all active adapters
    pub async fn get_adapter_memory_info(&self) -> Result<Vec<AdapterMemoryInfo>> {
        let adapters = sqlx::query_as::<_, AdapterMemoryInfo>(
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
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get adapter memory info: {}", e)))?;

        Ok(adapters)
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
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get eviction candidates: {}", e)))?;

        Ok(candidates)
    }

    /// Count adapters in various load states
    pub async fn count_adapters_by_load_state(&self) -> Result<Vec<(String, i64)>> {
        let counts = sqlx::query_as::<_, (String, i64)>(
            "SELECT COALESCE(load_state, 'unknown') as state, COUNT(*) as count
             FROM adapters
             WHERE active = 1
             GROUP BY load_state",
        )
        .fetch_all(self.pool())
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
