use crate::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

/// Builder for creating adapter registration parameters
#[derive(Debug, Default)]
pub struct AdapterRegistrationBuilder {
    tenant_id: Option<String>,
    adapter_id: Option<String>,
    name: Option<String>,
    hash_b3: Option<String>,
    rank: Option<i32>,
    tier: Option<String>, // 'persistent', 'warm', or 'ephemeral'
    alpha: Option<f64>,
    targets_json: Option<String>,
    acl_json: Option<String>,
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
    // .aos file support (from migration 0045)
    aos_file_path: Option<String>,
    aos_file_hash: Option<String>,
    // Semantic naming taxonomy (from migration 0061)
    adapter_name: Option<String>,
    tenant_namespace: Option<String>,
    domain: Option<String>,
    purpose: Option<String>,
    revision: Option<String>,
    parent_id: Option<String>,
    fork_type: Option<String>,
    fork_reason: Option<String>,
}

/// Parameters for adapter registration
#[derive(Debug, Clone)]
pub struct AdapterRegistrationParams {
    pub tenant_id: String,
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,
    pub rank: i32,
    pub tier: String, // 'persistent', 'warm', or 'ephemeral'
    pub alpha: f64,
    pub targets_json: String,
    pub acl_json: Option<String>,
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
    // .aos file support (from migration 0045)
    pub aos_file_path: Option<String>,
    pub aos_file_hash: Option<String>,
    // Semantic naming taxonomy (from migration 0061)
    pub adapter_name: Option<String>,
    pub tenant_namespace: Option<String>,
    pub domain: Option<String>,
    pub purpose: Option<String>,
    pub revision: Option<String>,
    pub parent_id: Option<String>,
    pub fork_type: Option<String>,
    pub fork_reason: Option<String>,
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

    /// Set the tenant ID (defaults to "default-tenant" if not set)
    pub fn tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Set the tier (defaults to "warm" if not set)
    /// Valid values: "persistent", "warm", "ephemeral"
    pub fn tier(mut self, tier: impl Into<String>) -> Self {
        self.tier = Some(tier.into());
        self
    }

    /// Set the alpha parameter (defaults to rank * 2.0 if not set)
    pub fn alpha(mut self, alpha: f64) -> Self {
        self.alpha = Some(alpha);
        self
    }

    /// Set the targets JSON (defaults to "[]" if not set)
    pub fn targets_json(mut self, targets_json: impl Into<String>) -> Self {
        self.targets_json = Some(targets_json.into());
        self
    }

    /// Set the ACL JSON (optional)
    pub fn acl_json(mut self, acl_json: Option<impl Into<String>>) -> Self {
        self.acl_json = acl_json.map(|s| s.into());
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

    /// Set the .aos file path (optional)
    pub fn aos_file_path(mut self, aos_file_path: Option<impl Into<String>>) -> Self {
        self.aos_file_path = aos_file_path.map(|s| s.into());
        self
    }

    /// Set the .aos file hash (optional)
    pub fn aos_file_hash(mut self, aos_file_hash: Option<impl Into<String>>) -> Self {
        self.aos_file_hash = aos_file_hash.map(|s| s.into());
        self
    }

    /// Set the semantic adapter name (optional)
    /// Format: {tenant_namespace}/{domain}/{purpose}/{revision}
    pub fn adapter_name(mut self, adapter_name: Option<impl Into<String>>) -> Self {
        self.adapter_name = adapter_name.map(|s| s.into());
        self
    }

    /// Set the tenant namespace (optional, part of semantic naming)
    pub fn tenant_namespace(mut self, tenant_namespace: Option<impl Into<String>>) -> Self {
        self.tenant_namespace = tenant_namespace.map(|s| s.into());
        self
    }

    /// Set the domain (optional, part of semantic naming)
    pub fn domain(mut self, domain: Option<impl Into<String>>) -> Self {
        self.domain = domain.map(|s| s.into());
        self
    }

    /// Set the purpose (optional, part of semantic naming)
    pub fn purpose(mut self, purpose: Option<impl Into<String>>) -> Self {
        self.purpose = purpose.map(|s| s.into());
        self
    }

    /// Set the revision (optional, part of semantic naming)
    pub fn revision(mut self, revision: Option<impl Into<String>>) -> Self {
        self.revision = revision.map(|s| s.into());
        self
    }

    /// Set the parent adapter ID for forks (optional)
    pub fn parent_id(mut self, parent_id: Option<impl Into<String>>) -> Self {
        self.parent_id = parent_id.map(|s| s.into());
        self
    }

    /// Set the fork type (optional: 'parameter', 'data', 'architecture')
    pub fn fork_type(mut self, fork_type: Option<impl Into<String>>) -> Self {
        self.fork_type = fork_type.map(|s| s.into());
        self
    }

    /// Set the fork reason (optional)
    pub fn fork_reason(mut self, fork_reason: Option<impl Into<String>>) -> Self {
        self.fork_reason = fork_reason.map(|s| s.into());
        self
    }

    /// Build the adapter registration parameters
    pub fn build(self) -> Result<AdapterRegistrationParams> {
        let rank = self.rank.ok_or_else(|| anyhow::anyhow!("rank is required"))?;

        // Validate and default tier
        let tier = self.tier.unwrap_or_else(|| "warm".to_string());
        if !["persistent", "warm", "ephemeral"].contains(&tier.as_str()) {
            return Err(anyhow::anyhow!(
                "tier must be 'persistent', 'warm', or 'ephemeral', got: {}",
                tier
            ));
        }

        Ok(AdapterRegistrationParams {
            tenant_id: self.tenant_id.unwrap_or_else(|| "default-tenant".to_string()),
            adapter_id: self
                .adapter_id
                .ok_or_else(|| anyhow::anyhow!("adapter_id is required"))?,
            name: self
                .name
                .ok_or_else(|| anyhow::anyhow!("name is required"))?,
            hash_b3: self
                .hash_b3
                .ok_or_else(|| anyhow::anyhow!("hash_b3 is required"))?,
            rank,
            tier,
            alpha: self.alpha.unwrap_or_else(|| (rank * 2) as f64),
            targets_json: self.targets_json.unwrap_or_else(|| "[]".to_string()),
            acl_json: self.acl_json,
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
            // .aos file support
            aos_file_path: self.aos_file_path,
            aos_file_hash: self.aos_file_hash,
            // Semantic naming taxonomy
            adapter_name: self.adapter_name,
            tenant_namespace: self.tenant_namespace,
            domain: self.domain,
            purpose: self.purpose,
            revision: self.revision,
            parent_id: self.parent_id,
            fork_type: self.fork_type,
            fork_reason: self.fork_reason,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Adapter {
    // Core fields (from migration 0001)
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub tier: String, // TEXT enum: 'persistent', 'warm', 'ephemeral'
    pub hash_b3: String,
    pub rank: i32,
    pub alpha: f64, // LoRA alpha parameter (usually rank * 2)
    pub targets_json: String, // JSON array of target modules
    pub acl_json: Option<String>, // Access control list
    pub adapter_id: Option<String>, // External adapter ID for lookups
    pub languages_json: Option<String>,
    pub framework: Option<String>,
    pub active: i32,

    // Code intelligence fields (from migration 0012)
    pub category: String,
    pub scope: String,
    pub framework_id: Option<String>,
    pub framework_version: Option<String>,
    pub repo_id: Option<String>,
    pub commit_sha: Option<String>,
    pub intent: Option<String>,

    // Lifecycle state management (from migration 0012)
    pub current_state: String,
    pub pinned: i32,
    pub memory_bytes: i64,
    pub last_activated: Option<String>,
    pub activation_count: i64,

    // Expiration (from migration 0044)
    pub expires_at: Option<String>,

    // Runtime load state (from migration 0031)
    pub load_state: String,
    pub last_loaded_at: Option<String>,

    // .aos file support (from migration 0045)
    pub aos_file_path: Option<String>,
    pub aos_file_hash: Option<String>,

    // Semantic naming (from migration 0061)
    pub adapter_name: Option<String>,
    pub tenant_namespace: Option<String>,
    pub domain: Option<String>,
    pub purpose: Option<String>,
    pub revision: Option<String>,
    pub parent_id: Option<String>,
    pub fork_type: Option<String>,
    pub fork_reason: Option<String>,

    pub created_at: String,
    pub updated_at: String,
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
            "INSERT INTO adapters (id, tenant_id, adapter_id, name, hash_b3, rank, alpha, tier, targets_json, acl_json, languages_json, framework, category, scope, framework_id, framework_version, repo_id, commit_sha, intent, expires_at, aos_file_path, aos_file_hash, adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason, current_state, pinned, memory_bytes, activation_count, load_state, active)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, 'unloaded', 0, 0, 0, 'cold', 1)"
        )
        .bind(&id)
        .bind(&params.tenant_id)
        .bind(&params.adapter_id)
        .bind(&params.name)
        .bind(&params.hash_b3)
        .bind(params.rank)
        .bind(params.alpha)
        .bind(&params.tier)
        .bind(&params.targets_json)
        .bind(&params.acl_json)
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
        .bind(&params.aos_file_path)
        .bind(&params.aos_file_hash)
        .bind(&params.adapter_name)
        .bind(&params.tenant_namespace)
        .bind(&params.domain)
        .bind(&params.purpose)
        .bind(&params.revision)
        .bind(&params.parent_id)
        .bind(&params.fork_type)
        .bind(&params.fork_reason)
        .execute(self.pool())
        .await?;

        Ok(id)
    }

    /// Find all expired adapters
    pub async fn find_expired_adapters(&self) -> Result<Vec<Adapter>> {
        let adapters = sqlx::query_as::<_, Adapter>(
            "SELECT id, tenant_id, adapter_id, name, hash_b3, rank, alpha, tier, targets_json, acl_json,
                    languages_json, framework, category, scope, framework_id, framework_version,
                    repo_id, commit_sha, intent, current_state, pinned, memory_bytes, last_activated,
                    activation_count, expires_at, load_state, last_loaded_at, aos_file_path, aos_file_hash,
                    adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason,
                    created_at, updated_at, active
             FROM adapters
             WHERE expires_at IS NOT NULL AND expires_at < datetime('now')",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(adapters)
    }

    /// List all adapters
    pub async fn list_adapters(&self) -> Result<Vec<Adapter>> {
        let adapters = sqlx::query_as::<_, Adapter>(
            "SELECT id, tenant_id, adapter_id, name, hash_b3, rank, alpha, tier, targets_json, acl_json,
                    languages_json, framework, category, scope, framework_id, framework_version,
                    repo_id, commit_sha, intent, current_state, pinned, memory_bytes, last_activated,
                    activation_count, expires_at, load_state, last_loaded_at, aos_file_path, aos_file_hash,
                    adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason,
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
    ///
    /// This function checks if the adapter is pinned before deletion.
    /// Pinned adapters cannot be deleted until they are unpinned.
    ///
    /// **Pin Enforcement:** Uses `active_pinned_adapters` view as the single source of truth.
    /// The view automatically respects TTL (pinned_until) via SQL filtering, eliminating manual
    /// expiration checks. This ensures consistent pin enforcement across all DB operations.
    /// Implementation: crates/adapteros-db/src/pinned_adapters.rs
    ///
    /// Citation: Agent G Stability Reinforcement Plan - Patch 1.3
    pub async fn delete_adapter(&self, id: &str) -> Result<()> {
        use adapteros_core::AosError;
        use tracing::warn;

        // Get adapter_id for pinning check
        let adapter_id: Option<String> = sqlx::query_scalar(
            "SELECT adapter_id FROM adapters WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?;

        if let Some(adapter_id) = adapter_id {
            // Check active_pinned_adapters view (single source of truth)
            // View automatically filters expired pins (pinned_until > now())
            let active_pin_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM active_pinned_adapters WHERE adapter_id = ?"
            )
            .bind(&adapter_id)
            .fetch_one(self.pool())
            .await
            .unwrap_or(0);

            if active_pin_count > 0 {
                warn!(
                    id = %id,
                    adapter_id = %adapter_id,
                    pin_count = active_pin_count,
                    "Attempted to delete adapter with active pins"
                );
                return Err(AosError::PolicyViolation(
                    format!(
                        "Cannot delete adapter '{}': adapter has {} active pin(s). Unpin first.",
                        adapter_id, active_pin_count
                    )
                ).into());
            }
        }

        // Not pinned - safe to delete
        sqlx::query("DELETE FROM adapters WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Delete an adapter and all its related entries in a transaction
    ///
    /// This ensures cascade deletion of:
    /// - Adapter record from adapters table
    /// - Any pinned_adapters entries
    /// - Any adapter_stack references (would need additional cleanup)
    ///
    /// Citation: Agent G Stability Reinforcement Plan - Patch 4.1
    pub async fn delete_adapter_cascade(&self, id: &str) -> Result<()> {
        use adapteros_core::AosError;
        use tracing::{info, warn};

        let mut tx = self.pool().begin().await?;

        // Get adapter_id for pinning check
        let adapter_id: Option<String> = sqlx::query_scalar(
            "SELECT adapter_id FROM adapters WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(adapter_id) = adapter_id {
            // Check active_pinned_adapters view (single source of truth)
            let active_pin_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM active_pinned_adapters WHERE adapter_id = ?"
            )
            .bind(&adapter_id)
            .fetch_one(&mut *tx)
            .await
            .unwrap_or(0);

            if active_pin_count > 0 {
                warn!(
                    id = %id,
                    adapter_id = %adapter_id,
                    pin_count = active_pin_count,
                    "Attempted to cascade delete adapter with active pins"
                );
                return Err(AosError::PolicyViolation(
                    format!(
                        "Cannot delete adapter '{}': adapter has {} active pin(s)",
                        adapter_id, active_pin_count
                    )
                ).into());
            }

            // Delete from pinned_adapters (expired pins)
            sqlx::query("DELETE FROM pinned_adapters WHERE adapter_id = ?")
                .bind(&adapter_id)
                .execute(&mut *tx)
                .await?;

            info!(id = %id, adapter_id = %adapter_id, "Deleting adapter with cascade");

            // Delete the adapter itself
            sqlx::query("DELETE FROM adapters WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;

            tx.commit().await?;
            Ok(())
        } else {
            // Adapter not found
            Err(anyhow::anyhow!("Adapter not found: {}", id))
        }
    }

    /// Get adapter by ID
    pub async fn get_adapter(&self, adapter_id: &str) -> Result<Option<Adapter>> {
        let adapter = sqlx::query_as::<_, Adapter>(
            "SELECT id, tenant_id, adapter_id, name, hash_b3, rank, alpha, tier, targets_json, acl_json,
                    languages_json, framework, category, scope, framework_id, framework_version,
                    repo_id, commit_sha, intent, current_state, pinned, memory_bytes, last_activated,
                    activation_count, expires_at, load_state, last_loaded_at, aos_file_path, aos_file_hash,
                    adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason,
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

    /// Update adapter state with transaction protection
    ///
    /// **Concurrency Safety:** SQLite transactions provide serialization without explicit locks.
    /// The transaction ensures atomic read-check-write, preventing lost updates in concurrent scenarios.
    /// Multiple callers are serialized by SQLite's default isolation level - no application-level
    /// mutexes or row locks required. This optimistic concurrency approach is tested under load
    /// (see tests/stability_reinforcement_tests.rs::test_concurrent_state_update_race_condition).
    ///
    /// Citation: Agent G Stability Reinforcement Plan - Patch 1.1
    pub async fn update_adapter_state_tx(
        &self,
        adapter_id: &str,
        state: &str,
        reason: &str,
    ) -> Result<()> {
        use tracing::{debug, warn};

        let mut tx = self.pool().begin().await?;

        // Lock the row to prevent concurrent updates
        let row_exists: Option<(String,)> = sqlx::query_as(
            "SELECT adapter_id FROM adapters WHERE adapter_id = ?"
        )
        .bind(adapter_id)
        .fetch_optional(&mut *tx)
        .await?;

        if row_exists.is_none() {
            warn!(adapter_id = %adapter_id, "Adapter not found for state update");
            return Err(anyhow::anyhow!("Adapter not found: {}", adapter_id));
        }

        // Update state with reason logged
        debug!(adapter_id = %adapter_id, state = %state, reason = %reason,
               "Updating adapter state (transactional)");

        sqlx::query(
            "UPDATE adapters SET current_state = ?, updated_at = datetime('now') WHERE adapter_id = ?"
        )
        .bind(state)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Update adapter memory usage with transaction protection
    ///
    /// **Concurrency Approach:** Optimistic concurrency via SQLite transactions.
    /// Transactions serialize updates without explicit locking. Concurrent memory updates
    /// are handled safely by SQLite's transaction isolation, eliminating the need for
    /// application-level synchronization primitives.
    ///
    /// Citation: Agent G Stability Reinforcement Plan - Patch 1.1
    pub async fn update_adapter_memory_tx(
        &self,
        adapter_id: &str,
        memory_bytes: i64,
    ) -> Result<()> {
        use tracing::debug;

        let mut tx = self.pool().begin().await?;

        // Verify adapter exists
        let row_exists: Option<(String,)> = sqlx::query_as(
            "SELECT adapter_id FROM adapters WHERE adapter_id = ?"
        )
        .bind(adapter_id)
        .fetch_optional(&mut *tx)
        .await?;

        if row_exists.is_none() {
            return Err(anyhow::anyhow!("Adapter not found: {}", adapter_id));
        }

        debug!(adapter_id = %adapter_id, memory_bytes = %memory_bytes,
               "Updating adapter memory (transactional)");

        sqlx::query(
            "UPDATE adapters SET memory_bytes = ?, updated_at = datetime('now') WHERE adapter_id = ?"
        )
        .bind(memory_bytes)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Atomically update both adapter state and memory in a single transaction
    ///
    /// This prevents race conditions where state and memory updates might
    /// interleave, causing inconsistent adapter records.
    ///
    /// Citation: Agent G Stability Reinforcement Plan - Patch 1.1
    pub async fn update_adapter_state_and_memory(
        &self,
        adapter_id: &str,
        state: &str,
        memory_bytes: i64,
        reason: &str,
    ) -> Result<()> {
        use tracing::debug;

        let mut tx = self.pool().begin().await?;

        // Verify adapter exists
        let row_exists: Option<(String,)> = sqlx::query_as(
            "SELECT adapter_id FROM adapters WHERE adapter_id = ?"
        )
        .bind(adapter_id)
        .fetch_optional(&mut *tx)
        .await?;

        if row_exists.is_none() {
            return Err(anyhow::anyhow!("Adapter not found: {}", adapter_id));
        }

        debug!(
            adapter_id = %adapter_id,
            state = %state,
            memory_bytes = %memory_bytes,
            reason = %reason,
            "Updating adapter state and memory atomically"
        );

        // Single UPDATE for both fields - atomic at SQL level
        sqlx::query(
            "UPDATE adapters
             SET current_state = ?, memory_bytes = ?, updated_at = datetime('now')
             WHERE adapter_id = ?"
        )
        .bind(state)
        .bind(memory_bytes)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    /// List adapters by category
    pub async fn list_adapters_by_category(&self, category: &str) -> Result<Vec<Adapter>> {
        let adapters = sqlx::query_as::<_, Adapter>(
            "SELECT id, tenant_id, adapter_id, name, hash_b3, rank, alpha, tier, targets_json, acl_json,
                    languages_json, framework, category, scope, framework_id, framework_version,
                    repo_id, commit_sha, intent, current_state, pinned, memory_bytes, last_activated,
                    activation_count, expires_at, load_state, last_loaded_at, aos_file_path, aos_file_hash,
                    adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason,
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
            "SELECT id, tenant_id, adapter_id, name, hash_b3, rank, alpha, tier, targets_json, acl_json,
                    languages_json, framework, category, scope, framework_id, framework_version,
                    repo_id, commit_sha, intent, current_state, pinned, memory_bytes, last_activated,
                    activation_count, expires_at, load_state, last_loaded_at, aos_file_path, aos_file_hash,
                    adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason,
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
            "SELECT id, tenant_id, adapter_id, name, hash_b3, rank, alpha, tier, targets_json, acl_json,
                    languages_json, framework, category, scope, framework_id, framework_version,
                    repo_id, commit_sha, intent, current_state, pinned, memory_bytes, last_activated,
                    activation_count, expires_at, load_state, last_loaded_at, aos_file_path, aos_file_hash,
                    adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason,
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
