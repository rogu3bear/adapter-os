use crate::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Adapter {
    pub id: String,
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,
    pub rank: i32,
    pub tier: i32,
    pub languages_json: Option<String>,
    pub framework: Option<String>,

    // Code intelligence fields
    pub category: String,
    pub scope: String,
    pub framework_id: Option<String>,
    pub framework_version: Option<String>,
    pub repo_id: Option<String>,
    pub commit_sha: Option<String>,
    pub intent: Option<String>,

    // Lifecycle state management
    pub current_state: String,
    pub pinned: i32,
    pub memory_bytes: i64,
    pub last_activated: Option<String>,
    pub activation_count: i64,

    pub created_at: String,
    pub updated_at: String,
    pub active: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdapterActivation {
    pub id: String,
    pub adapter_id: String,
    pub request_id: Option<String>,
    pub gate_value: f64,
    pub selected: i32,
    pub created_at: String,
}

impl Db {
    /// Register a new adapter
    pub async fn register_adapter(
        &self,
        adapter_id: &str,
        name: &str,
        hash_b3: &str,
        rank: i32,
        tier: i32,
        languages_json: Option<&str>,
        framework: Option<&str>,
    ) -> Result<String> {
        self.register_adapter_extended(
            adapter_id,
            name,
            hash_b3,
            rank,
            tier,
            languages_json,
            framework,
            "code",
            "global",
            None,
            None,
            None,
            None,
            None,
        )
        .await
    }

    /// Register a new adapter with extended fields
    pub async fn register_adapter_extended(
        &self,
        adapter_id: &str,
        name: &str,
        hash_b3: &str,
        rank: i32,
        tier: i32,
        languages_json: Option<&str>,
        framework: Option<&str>,
        category: &str,
        scope: &str,
        framework_id: Option<&str>,
        framework_version: Option<&str>,
        repo_id: Option<&str>,
        commit_sha: Option<&str>,
        intent: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO adapters (
                id, adapter_id, name, hash_b3, rank, tier, languages_json, framework,
                category, scope, framework_id, framework_version, repo_id, commit_sha, intent,
                current_state, pinned, memory_bytes, activation_count
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(adapter_id)
        .bind(name)
        .bind(hash_b3)
        .bind(rank)
        .bind(tier)
        .bind(languages_json)
        .bind(framework)
        .bind(category)
        .bind(scope)
        .bind(framework_id)
        .bind(framework_version)
        .bind(repo_id)
        .bind(commit_sha)
        .bind(intent)
        .bind("unloaded")
        .bind(0)
        .bind(0)
        .bind(0)
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    /// List all adapters
    pub async fn list_adapters(&self) -> Result<Vec<Adapter>> {
        let adapters = sqlx::query_as::<_, Adapter>(
            "SELECT id, adapter_id, name, hash_b3, rank, tier, languages_json, framework,
                    category, scope, framework_id, framework_version, repo_id, commit_sha, intent,
                    current_state, pinned, memory_bytes, last_activated, activation_count,
                    created_at, updated_at, active 
             FROM adapters 
             WHERE active = 1 
             ORDER BY tier ASC, created_at DESC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(adapters)
    }

    /// Get adapter by ID
    pub async fn get_adapter(&self, adapter_id: &str) -> Result<Option<Adapter>> {
        let adapter = sqlx::query_as::<_, Adapter>(
            "SELECT id, adapter_id, name, hash_b3, rank, tier, languages_json, framework,
                    category, scope, framework_id, framework_version, repo_id, commit_sha, intent,
                    current_state, pinned, memory_bytes, last_activated, activation_count,
                    created_at, updated_at, active 
             FROM adapters 
             WHERE adapter_id = ?",
        )
        .bind(adapter_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(adapter)
    }

    /// Delete adapter (soft delete)
    pub async fn delete_adapter(&self, adapter_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE adapters SET active = 0, updated_at = datetime('now') WHERE adapter_id = ?",
        )
        .bind(adapter_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Record adapter activation
    pub async fn record_activation(
        &self,
        adapter_id: &str,
        request_id: Option<&str>,
        gate_value: f64,
        selected: bool,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO adapter_activations (id, adapter_id, request_id, gate_value, selected) 
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(adapter_id)
        .bind(request_id)
        .bind(gate_value)
        .bind(if selected { 1 } else { 0 })
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    /// Get adapter activations
    pub async fn get_adapter_activations(
        &self,
        adapter_id: &str,
        limit: i64,
    ) -> Result<Vec<AdapterActivation>> {
        let activations = sqlx::query_as::<_, AdapterActivation>(
            "SELECT id, adapter_id, request_id, gate_value, selected, created_at 
             FROM adapter_activations 
             WHERE adapter_id = ? 
             ORDER BY created_at DESC 
             LIMIT ?",
        )
        .bind(adapter_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok(activations)
    }

    /// Get adapter activation stats
    pub async fn get_adapter_stats(&self, adapter_id: &str) -> Result<(i64, i64, f64)> {
        let row = sqlx::query(
            "SELECT 
                COUNT(*) as total,
                SUM(selected) as selected_count,
                AVG(gate_value) as avg_gate
             FROM adapter_activations 
             WHERE adapter_id = ?",
        )
        .bind(adapter_id)
        .fetch_one(self.pool())
        .await?;

        let total: i64 = row.try_get("total")?;
        let selected: i64 = row.try_get("selected_count").unwrap_or(0);
        let avg_gate: f64 = row.try_get("avg_gate").unwrap_or(0.0);

        Ok((total, selected, avg_gate))
    }

    /// Update adapter state
    pub async fn update_adapter_state(
        &self,
        adapter_id: &str,
        state: &str,
        _reason: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE adapters SET current_state = ?, updated_at = datetime('now') WHERE adapter_id = ?"
        )
        .bind(state)
        .bind(adapter_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    // Pin/unpin functionality moved to pinned_adapters.rs

    /// Update adapter memory usage
    pub async fn update_adapter_memory(&self, adapter_id: &str, memory_bytes: i64) -> Result<()> {
        sqlx::query(
            "UPDATE adapters SET memory_bytes = ?, updated_at = datetime('now') WHERE adapter_id = ?"
        )
        .bind(memory_bytes)
        .bind(adapter_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// List adapters by category
    pub async fn list_adapters_by_category(&self, category: &str) -> Result<Vec<Adapter>> {
        let adapters = sqlx::query_as::<_, Adapter>(
            "SELECT id, adapter_id, name, hash_b3, rank, tier, languages_json, framework,
                    category, scope, framework_id, framework_version, repo_id, commit_sha, intent,
                    current_state, pinned, memory_bytes, last_activated, activation_count,
                    created_at, updated_at, active 
             FROM adapters 
             WHERE active = 1 AND category = ?
             ORDER BY activation_count DESC, created_at DESC",
        )
        .bind(category)
        .fetch_all(self.pool())
        .await?;
        Ok(adapters)
    }

    /// List adapters by scope
    pub async fn list_adapters_by_scope(&self, scope: &str) -> Result<Vec<Adapter>> {
        let adapters = sqlx::query_as::<_, Adapter>(
            "SELECT id, adapter_id, name, hash_b3, rank, tier, languages_json, framework,
                    category, scope, framework_id, framework_version, repo_id, commit_sha, intent,
                    current_state, pinned, memory_bytes, last_activated, activation_count,
                    created_at, updated_at, active 
             FROM adapters 
             WHERE active = 1 AND scope = ?
             ORDER BY activation_count DESC, created_at DESC",
        )
        .bind(scope)
        .fetch_all(self.pool())
        .await?;
        Ok(adapters)
    }

    /// List adapters by state
    pub async fn list_adapters_by_state(&self, state: &str) -> Result<Vec<Adapter>> {
        let adapters = sqlx::query_as::<_, Adapter>(
            "SELECT id, adapter_id, name, hash_b3, rank, tier, languages_json, framework,
                    category, scope, framework_id, framework_version, repo_id, commit_sha, intent,
                    current_state, pinned, memory_bytes, last_activated, activation_count,
                    created_at, updated_at, active 
             FROM adapters 
             WHERE active = 1 AND current_state = ?
             ORDER BY activation_count DESC, created_at DESC",
        )
        .bind(state)
        .fetch_all(self.pool())
        .await?;
        Ok(adapters)
    }

    /// Get adapter state summary
    pub async fn get_adapter_state_summary(
        &self,
    ) -> Result<Vec<(String, String, String, i64, i64, f64, Option<String>)>> {
        let summary = sqlx::query(
            "SELECT category, scope, current_state, COUNT(*) as count, 
                    SUM(memory_bytes) as total_memory_bytes, 
                    AVG(activation_count) as avg_activations,
                    MAX(last_activated) as most_recent_activation
             FROM adapters 
             WHERE active = 1
             GROUP BY category, scope, current_state
             ORDER BY category, scope, current_state",
        )
        .fetch_all(self.pool())
        .await?;

        let mut result = Vec::new();
        for row in summary {
            let category: String = row.try_get("category")?;
            let scope: String = row.try_get("scope")?;
            let state: String = row.try_get("current_state")?;
            let count: i64 = row.try_get("count")?;
            let total_memory: i64 = row.try_get("total_memory_bytes").unwrap_or(0);
            let avg_activations: f64 = row.try_get("avg_activations").unwrap_or(0.0);
            let most_recent: Option<String> = row.try_get("most_recent_activation").ok();

            result.push((
                category,
                scope,
                state,
                count,
                total_memory,
                avg_activations,
                most_recent,
            ));
        }

        Ok(result)
    }
}
