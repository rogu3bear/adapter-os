use crate::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

/// Builder for creating adapter registration parameters
#[derive(Debug, Default)]
pub struct AdapterRegistrationBuilder {
    adapter_id: Option<String>,
    name: Option<String>,
    hash_b3: Option<String>,
    rank: Option<i32>,
    tier: Option<i32>,
    languages_json: Option<String>,
    framework: Option<String>,
    category: Option<String>,
    scope: Option<String>,
    framework_id: Option<String>,
    framework_version: Option<String>,
    repo_id: Option<String>,
    commit_sha: Option<String>,
    intent: Option<String>,
    expires_at: Option<String>,
}

/// Parameters for adapter registration
#[derive(Debug, Clone)]
pub struct AdapterRegistrationParams {
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,
    pub rank: i32,
    pub tier: i32,
    pub languages_json: Option<String>,
    pub framework: Option<String>,
    pub category: String,
    pub scope: String,
    pub framework_id: Option<String>,
    pub framework_version: Option<String>,
    pub repo_id: Option<String>,
    pub commit_sha: Option<String>,
    pub intent: Option<String>,
    pub expires_at: Option<String>,
}

impl AdapterRegistrationBuilder {
    /// Create a new adapter registration builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the adapter ID (required)
    pub fn adapter_id(mut self, adapter_id: impl Into<String>) -> Self {
        self.adapter_id = Some(adapter_id.into());
        self
    }

    /// Set the adapter name (required)
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the B3 hash (required)
    pub fn hash_b3(mut self, hash_b3: impl Into<String>) -> Self {
        self.hash_b3 = Some(hash_b3.into());
        self
    }

    /// Set the rank (required)
    pub fn rank(mut self, rank: i32) -> Self {
        self.rank = Some(rank);
        self
    }

    /// Set the tier (required)
    pub fn tier(mut self, tier: i32) -> Self {
        self.tier = Some(tier);
        self
    }

    /// Set the languages JSON (optional)
    pub fn languages_json(mut self, languages_json: Option<impl Into<String>>) -> Self {
        self.languages_json = languages_json.map(|s| s.into());
        self
    }

    /// Set the framework (optional)
    pub fn framework(mut self, framework: Option<impl Into<String>>) -> Self {
        self.framework = framework.map(|s| s.into());
        self
    }

    /// Set the category (defaults to `"code"` when omitted)
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Set the scope (defaults to `"global"` when omitted)
    pub fn scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }

    /// Set the framework ID (optional)
    pub fn framework_id(mut self, framework_id: Option<impl Into<String>>) -> Self {
        self.framework_id = framework_id.map(|s| s.into());
        self
    }

    /// Set the framework version (optional)
    pub fn framework_version(mut self, framework_version: Option<impl Into<String>>) -> Self {
        self.framework_version = framework_version.map(|s| s.into());
        self
    }

    /// Set the repository ID (optional)
    pub fn repo_id(mut self, repo_id: Option<impl Into<String>>) -> Self {
        self.repo_id = repo_id.map(|s| s.into());
        self
    }

    /// Set the commit SHA (optional)
    pub fn commit_sha(mut self, commit_sha: Option<impl Into<String>>) -> Self {
        self.commit_sha = commit_sha.map(|s| s.into());
        self
    }

    /// Set the intent (optional)
    pub fn intent(mut self, intent: Option<impl Into<String>>) -> Self {
        self.intent = intent.map(|s| s.into());
        self
    }

    /// Set the expiration date (optional)
    pub fn expires_at(mut self, expires_at: Option<impl Into<String>>) -> Self {
        self.expires_at = expires_at.map(|s| s.into());
        self
    }

    /// Build the adapter registration parameters
    pub fn build(self) -> Result<AdapterRegistrationParams> {
        Ok(AdapterRegistrationParams {
            adapter_id: self
                .adapter_id
                .ok_or_else(|| anyhow::anyhow!("adapter_id is required"))?,
            name: self
                .name
                .ok_or_else(|| anyhow::anyhow!("name is required"))?,
            hash_b3: self
                .hash_b3
                .ok_or_else(|| anyhow::anyhow!("hash_b3 is required"))?,
            rank: self
                .rank
                .ok_or_else(|| anyhow::anyhow!("rank is required"))?,
            tier: self
                .tier
                .ok_or_else(|| anyhow::anyhow!("tier is required"))?,
            category: self.category.unwrap_or_else(|| "code".to_string()),
            scope: self.scope.unwrap_or_else(|| "global".to_string()),
            languages_json: self.languages_json,
            framework: self.framework,
            framework_id: self.framework_id,
            framework_version: self.framework_version,
            repo_id: self.repo_id,
            commit_sha: self.commit_sha,
            intent: self.intent,
            expires_at: self.expires_at,
        })
    }
}

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
    pub expires_at: Option<String>,

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
    ///
    /// Construct parameters using [`AdapterRegistrationBuilder`] to ensure required
    /// fields are provided and validated:
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let params = adapteros_db::AdapterRegistrationBuilder::new()
    ///     .adapter_id("adapter-123")
    ///     .name("My Adapter")
    ///     .hash_b3("b3:0123")
    ///     .rank(8)
    ///     .tier(2)
    ///     .build()?;
    /// db.register_adapter(params).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn register_adapter(&self, params: AdapterRegistrationParams) -> Result<String> {
        self.register_adapter_extended(params).await
    }

    /// Register a new adapter with extended fields
    ///
    /// Use [`AdapterRegistrationBuilder`] to construct complex parameter sets:
    /// ```no_run
    /// use adapteros_db::adapters::AdapterRegistrationBuilder;
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) {
    /// let params = AdapterRegistrationBuilder::new()
    ///     .adapter_id("adapter-123")
    ///     .name("My Adapter")
    ///     .hash_b3("abc123...")
    ///     .rank(1)
    ///     .tier(2)
    ///     .category("classification")
    ///     .scope("general")
    ///     .build()
    ///     .expect("required fields");
    /// db.register_adapter_extended(params)
    ///     .await
    ///     .expect("registration succeeds");
    /// # }
    /// ```
    pub async fn register_adapter_extended(
        &self,
        params: AdapterRegistrationParams,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO adapters (id, adapter_id, name, hash_b3, rank, tier, languages_json, framework, category, scope, framework_id, framework_version, repo_id, commit_sha, intent, expires_at, current_state, pinned, memory_bytes, activation_count, active)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, 'unloaded', 0, 0, 0, 1)"
        )
        .bind(&id)
        .bind(&params.adapter_id)
        .bind(&params.name)
        .bind(&params.hash_b3)
        .bind(params.rank)
        .bind(params.tier)
        .bind(&params.languages_json)
        .bind(&params.framework)
        .bind(&params.category)
        .bind(&params.scope)
        .bind(&params.framework_id)
        .bind(&params.framework_version)
        .bind(&params.repo_id)
        .bind(&params.commit_sha)
        .bind(&params.intent)
        .bind(&params.expires_at)
        .execute(self.pool())
        .await?;

        Ok(id)
    }

    /// Find all expired adapters
    pub async fn find_expired_adapters(&self) -> Result<Vec<Adapter>> {
        let adapters = sqlx::query_as::<_, Adapter>(
            "SELECT * FROM adapters WHERE expires_at IS NOT NULL AND expires_at < datetime('now')",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(adapters)
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

    /// Delete an adapter by its ID
    pub async fn delete_adapter(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM adapters WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
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
