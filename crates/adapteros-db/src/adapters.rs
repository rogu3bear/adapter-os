use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::env;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::adapters_kv::{AdapterKvOps, AdapterKvRepository};
use crate::kv_metrics::global_kv_metrics;
use adapteros_storage::repos::AdapterRepository;

/// Standard adapter SELECT fields for all queries
///
/// This constant ensures all adapter queries return the same columns
/// in the same order, matching the Adapter struct field order.
use crate::constants::ADAPTER_COLUMNS;

/// Alias for backwards compatibility
const ADAPTER_SELECT_FIELDS: &str = ADAPTER_COLUMNS;

/// Adapter columns with table alias `a.` for recursive lineage queries
const ADAPTER_COLUMNS_ALIAS_A: &str =
    "a.id, a.tenant_id, a.adapter_id, a.name, a.hash_b3, a.rank, a.alpha, a.tier, \
     a.targets_json, a.acl_json, a.languages_json, a.framework, a.category, a.scope, \
     a.framework_id, a.framework_version, a.repo_id, a.commit_sha, a.intent, \
     a.current_state, a.pinned, a.memory_bytes, a.last_activated, a.activation_count, \
     a.expires_at, a.load_state, a.last_loaded_at, a.aos_file_path, a.aos_file_hash, \
     a.adapter_name, a.tenant_namespace, a.domain, a.purpose, a.revision, a.parent_id, \
     a.fork_type, a.fork_reason, a.version, a.lifecycle_state, a.archived_at, a.archived_by, \
     a.archive_reason, a.purged_at, a.base_model_id, a.manifest_schema_version, \
     a.content_hash_b3, a.provenance_json, a.created_at, a.updated_at, a.active";

/// Configuration for atomic dual-write behavior (SQL + KV)
#[derive(Debug, Clone)]
pub struct AtomicDualWriteConfig {
    /// Require KV writes to succeed; if true, failures surface as errors
    /// and registration attempts to rollback SQL inserts.
    pub require_kv_success: bool,
}

impl Default for AtomicDualWriteConfig {
    fn default() -> Self {
        Self {
            require_kv_success: false,
        }
    }
}

impl AtomicDualWriteConfig {
    /// Best-effort mode: KV failures are logged but do not fail the operation.
    pub fn best_effort() -> Self {
        Self::default()
    }

    /// Strict mode: KV failures propagate errors; registration attempts rollback.
    pub fn strict_atomic() -> Self {
        Self {
            require_kv_success: true,
        }
    }

    /// Convenience predicate
    pub fn is_strict(&self) -> bool {
        self.require_kv_success
    }

    /// Load from environment variable `AOS_ATOMIC_DUAL_WRITE_STRICT`.
    /// Accepts: "1", "true", "yes" (case-insensitive) for strict mode.
    pub fn from_env() -> Self {
        match env::var("AOS_ATOMIC_DUAL_WRITE_STRICT") {
            Ok(val) if matches!(val.to_lowercase().as_str(), "1" | "true" | "yes") => {
                Self::strict_atomic()
            }
            _ => Self::best_effort(),
        }
    }
}

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
    // Base model reference (from migration 0098)
    base_model_id: Option<String>,
    // Artifact hardening (from migration 0153)
    manifest_schema_version: Option<String>,
    content_hash_b3: Option<String>,
    provenance_json: Option<String>,
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
    // Base model reference (from migration 0098)
    pub base_model_id: Option<String>,
    // Artifact hardening (from migration 0153)
    pub manifest_schema_version: Option<String>,
    pub content_hash_b3: Option<String>,
    pub provenance_json: Option<String>,
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

    /// Set the .aos file hash (optional, BLAKE3 hash of the file)
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

    /// Set the base model ID (optional, from migration 0098)
    /// Links this adapter to the base model it was trained from
    pub fn base_model_id(mut self, base_model_id: Option<impl Into<String>>) -> Self {
        self.base_model_id = base_model_id.map(|s| s.into());
        self
    }

    /// Set the manifest schema version (optional, from migration 0153)
    /// Semantic versioning string (e.g., "1.0.0")
    pub fn manifest_schema_version(
        mut self,
        manifest_schema_version: Option<impl Into<String>>,
    ) -> Self {
        self.manifest_schema_version = manifest_schema_version.map(|s| s.into());
        self
    }

    /// Set the content hash (optional, from migration 0153)
    /// BLAKE3 hash of manifest + weights for identity/deduplication
    pub fn content_hash_b3(mut self, content_hash_b3: Option<impl Into<String>>) -> Self {
        self.content_hash_b3 = content_hash_b3.map(|s| s.into());
        self
    }

    /// Set the provenance JSON (optional, from migration 0153)
    /// Full training provenance embedded in the adapter
    pub fn provenance_json(mut self, provenance_json: Option<impl Into<String>>) -> Self {
        self.provenance_json = provenance_json.map(|s| s.into());
        self
    }

    /// Build the adapter registration parameters
    pub fn build(self) -> Result<AdapterRegistrationParams> {
        let rank = self
            .rank
            .ok_or_else(|| AosError::Validation("rank is required".into()))?;

        // Validate and default tier
        let tier = self.tier.unwrap_or_else(|| "warm".to_string());
        if !["persistent", "warm", "ephemeral"].contains(&tier.as_str()) {
            return Err(AosError::Validation(format!(
                "tier must be 'persistent', 'warm', or 'ephemeral', got: {}",
                tier
            )));
        }

        Ok(AdapterRegistrationParams {
            tenant_id: self
                .tenant_id
                .unwrap_or_else(|| "default-tenant".to_string()),
            adapter_id: self
                .adapter_id
                .ok_or_else(|| AosError::Validation("adapter_id is required".into()))?,
            name: self
                .name
                .ok_or_else(|| AosError::Validation("name is required".into()))?,
            hash_b3: self
                .hash_b3
                .ok_or_else(|| AosError::Validation("hash_b3 is required".into()))?,
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
            base_model_id: self.base_model_id,
            manifest_schema_version: self.manifest_schema_version,
            content_hash_b3: self.content_hash_b3,
            provenance_json: self.provenance_json,
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
    pub alpha: f64,                 // LoRA alpha parameter (usually rank * 2)
    pub targets_json: String,       // JSON array of target modules
    pub acl_json: Option<String>,   // Access control list
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

    // Metadata normalization (from migration 0068)
    pub version: String,         // Semantic version or monotonic
    pub lifecycle_state: String, // draft/active/deprecated/retired

    // Archive/GC fields (from migration 0138)
    pub archived_at: Option<String>,    // When adapter was archived
    pub archived_by: Option<String>,    // User/system that initiated archive
    pub archive_reason: Option<String>, // Reason for archival (e.g., "tenant_archived")
    pub purged_at: Option<String>,      // When .aos file was deleted by GC

    // Base model reference (from migration 0098)
    pub base_model_id: Option<String>,

    // Artifact hardening (from migration 0153)
    pub manifest_schema_version: Option<String>,
    pub content_hash_b3: Option<String>,
    pub provenance_json: Option<String>,

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
    /// Get an AdapterKvRepository if KV writes are enabled
    pub(crate) fn get_adapter_kv_repo(&self, tenant_id: &str) -> Option<AdapterKvRepository> {
        if self.storage_mode().write_to_kv() {
            self.kv_backend().map(|kv| {
                let repo = AdapterRepository::new(kv.backend().clone(), kv.index_manager().clone());
                AdapterKvRepository::new_with_locks(
                    Arc::new(repo),
                    tenant_id.to_string(),
                    kv.increment_locks().clone(),
                )
            })
        } else {
            None
        }
    }

    /// Get tenant_id for an adapter by adapter_id (external ID)
    ///
    /// Returns None if adapter doesn't exist
    pub(crate) async fn get_adapter_tenant_id(&self, adapter_id: &str) -> Result<Option<String>> {
        let tenant_id: Option<String> =
            sqlx::query_scalar("SELECT tenant_id FROM adapters WHERE adapter_id = ?")
                .bind(adapter_id)
                .fetch_optional(&*self.pool())
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(tenant_id)
    }

    /// Get adapter directly from KV without SQL tenant lookup
    ///
    /// This is used for KV-only adapters that don't exist in SQL.
    /// It queries the BY_ADAPTER_ID index to find the adapter.
    async fn get_adapter_from_kv_direct(&self, adapter_id: &str) -> Result<Option<Adapter>> {
        use adapteros_storage::kv::indexing::adapter_indexes;

        let kv = match self.kv_backend() {
            Some(kv) => kv,
            None => return Ok(None),
        };

        // Query BY_ADAPTER_ID index to find the internal UUID
        let internal_ids = kv
            .index_manager()
            .query_index(adapter_indexes::BY_ADAPTER_ID, adapter_id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to query adapter index: {}", e)))?;

        let internal_id = match internal_ids.first() {
            Some(id) => id,
            None => return Ok(None),
        };

        // Load adapter by internal UUID
        let key = format!("adapter:{}", internal_id);
        let bytes = match kv
            .backend()
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get adapter: {}", e)))?
        {
            Some(b) => b,
            None => return Ok(None),
        };

        // Deserialize and convert to Adapter
        let adapter_kv: adapteros_storage::AdapterKv = bincode::deserialize(&bytes)
            .map_err(|e| AosError::Database(format!("Failed to deserialize adapter: {}", e)))?;

        Ok(Some(adapter_kv.into()))
    }

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
        let mut sql_inserted = false;
        let mut dual_write_completed = false;
        let dual_write_timer =
            if self.storage_mode().write_to_sql() && self.storage_mode().write_to_kv() {
                Some(Instant::now())
            } else {
                None
            };

        // Write to SQL when allowed by storage mode
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                sqlx::query(
                    "INSERT INTO adapters (id, tenant_id, adapter_id, name, hash_b3, rank, alpha, tier, targets_json, acl_json, languages_json, framework, category, scope, framework_id, framework_version, repo_id, commit_sha, intent, expires_at, adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason, aos_file_path, aos_file_hash, base_model_id, manifest_schema_version, content_hash_b3, provenance_json, version, lifecycle_state, current_state, pinned, memory_bytes, activation_count, load_state, active)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31, $32, $33, $34, '1.0.0', 'active', 'unloaded', 0, 0, 0, 'cold', 1)"
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
                .bind(&params.adapter_name)
                .bind(&params.tenant_namespace)
                .bind(&params.domain)
                .bind(&params.purpose)
                .bind(&params.revision)
                .bind(&params.parent_id)
                .bind(&params.fork_type)
                .bind(&params.fork_reason)
                .bind(&params.aos_file_path)
                .bind(&params.aos_file_hash)
                .bind(&params.base_model_id)
                .bind(&params.manifest_schema_version)
                .bind(&params.content_hash_b3)
                .bind(&params.provenance_json)
                .execute(pool)
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;
                sql_inserted = true;
            } else if !self.storage_mode().write_to_kv() {
                // No SQL pool and not writing to KV means we cannot satisfy the write
                return Err(AosError::Database(
                    "SQL backend unavailable for adapter registration".to_string(),
                ));
            }
        }

        // KV write (dual-write mode) - use same ID as SQL for consistency
        if let Some(repo) = self.get_adapter_kv_repo(&params.tenant_id) {
            if let Err(e) = repo.register_adapter_kv_with_id(&id, params.clone()).await {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %id,
                        tenant_id = %params.tenant_id,
                        mode = "dual-write-strict",
                        "KV write failed in strict atomic mode - rolling back SQL insert"
                    );

                    if sql_inserted {
                        match self.pool_opt() {
                            Some(pool) => {
                                if let Err(rollback_err) =
                                    sqlx::query("DELETE FROM adapters WHERE id = ?")
                                        .bind(&id)
                                        .execute(pool)
                                        .await
                                {
                                    error!(
                                        original_error = %e,
                                        rollback_error = %rollback_err,
                                        adapter_id = %id,
                                        tenant_id = %params.tenant_id,
                                        "CRITICAL: Failed to rollback SQL insert after KV failure"
                                    );
                                    return Err(AosError::Database(format!(
                                        "KV write failed and rollback failed (adapter_id: {id}). Manual repair required."
                                    )));
                                }
                            }
                            None => {
                                error!(
                                    original_error = %e,
                                    adapter_id = %id,
                                    tenant_id = %params.tenant_id,
                                    "KV write failed but SQL pool unavailable; cannot rollback SQL insert"
                                );
                                return Err(AosError::Database(format!(
                                    "KV write failed in strict mode for adapter_id={id} and SQL rollback unavailable (no SQL pool): {e}"
                                )));
                            }
                        }
                    }

                    return Err(AosError::Database(format!(
                        "KV write failed in strict mode for adapter_id={id}: {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %id, mode = "dual-write", "Failed to write adapter to KV backend");
                }
            } else {
                dual_write_completed = sql_inserted;
                debug!(adapter_id = %id, tenant_id = %params.tenant_id, mode = "dual-write", "Adapter registered in both SQL and KV backends");
            }
        }

        if dual_write_completed {
            if let Some(start) = dual_write_timer {
                global_kv_metrics().record_dual_write_lag(start.elapsed());
            }
        }

        Ok(id)
    }

    /// Find all expired adapters
    pub async fn find_expired_adapters(&self) -> Result<Vec<Adapter>> {
        let query = format!(
            "SELECT {} FROM adapters WHERE expires_at IS NOT NULL AND expires_at < datetime('now')",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(adapters)
    }

    /// List all adapters (DEPRECATED - use list_adapters_for_tenant instead)
    ///
    /// WARNING: This method returns ALL adapters across ALL tenants without filtering.
    /// This breaks multi-tenant isolation and should only be used in very specific cases
    /// like system administration or migration scripts where cross-tenant access is required.
    ///
    /// For normal operations, use `list_adapters_for_tenant()` which enforces tenant isolation.
    #[deprecated(
        since = "0.3.0",
        note = "Use list_adapters_for_tenant() for tenant isolation"
    )]
    pub async fn list_adapters(&self) -> Result<Vec<Adapter>> {
        let query = format!(
            "SELECT {} FROM adapters WHERE active = 1 ORDER BY tier ASC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(adapters)
    }

    /// List ALL adapters across ALL tenants for system-level operations.
    ///
    /// This method is explicitly designed for system-level operations that require
    /// cross-tenant visibility, such as:
    /// - Cleanup jobs and garbage collection
    /// - System monitoring and health checks
    /// - Lifecycle management and state recovery
    /// - Administrative dashboards
    /// - Migration scripts
    ///
    /// For normal tenant-scoped operations, use `list_adapters_for_tenant()` instead.
    ///
    /// # Returns
    /// Vector of all active adapters ordered by tier (ascending) and creation date (descending)
    pub async fn list_all_adapters_system(&self) -> Result<Vec<Adapter>> {
        let query = format!(
            "SELECT {} FROM adapters WHERE active = 1 ORDER BY tier ASC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to list all adapters (system): {}", e))
            })?;
        Ok(adapters)
    }

    /// List adapters for a specific tenant
    ///
    /// This is the RECOMMENDED method for listing adapters as it enforces tenant isolation.
    /// Only returns adapters belonging to the specified tenant.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID to filter by
    ///
    /// # Returns
    /// Vector of adapters belonging to the tenant, ordered by tier (ascending) and creation date (descending)
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let adapters = db.list_adapters_for_tenant("tenant-123").await?;
    /// for adapter in adapters {
    ///     println!("Adapter: {} ({})", adapter.name, adapter.id);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_adapters_for_tenant(&self, tenant_id: &str) -> Result<Vec<Adapter>> {
        // Try KV first if enabled
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                match repo.list_adapters_for_tenant_kv(tenant_id).await {
                    Ok(adapters) if !adapters.is_empty() => {
                        debug!(tenant_id = %tenant_id, count = adapters.len(), mode = "kv-primary", "Retrieved adapters from KV");
                        return Ok(adapters);
                    }
                    Ok(_) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("adapters.list_for_tenant.empty");
                        debug!(tenant_id = %tenant_id, mode = "kv-fallback", "KV returned empty list, falling back to SQL");
                    }
                    Ok(adapters) => {
                        return Ok(adapters);
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("adapters.list_for_tenant.error");
                        warn!(error = %e, tenant_id = %tenant_id, mode = "kv-fallback", "KV read failed, falling back to SQL");
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }

        // SQL fallback or primary read
        let query = format!(
            "SELECT {} FROM adapters WHERE tenant_id = ? AND active = 1 ORDER BY tier ASC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(tenant_id)
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to list adapters for tenant: {}", e))
            })?;
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
        // Get adapter_id and tenant_id for pinning check and KV dual-write
        let adapter_data: Option<(String, String)> =
            sqlx::query_as("SELECT adapter_id, tenant_id FROM adapters WHERE id = ?")
                .bind(id)
                .fetch_optional(&*self.pool())
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;

        let (adapter_id, tenant_id) = match adapter_data {
            Some((aid, tid)) => (aid, tid),
            None => {
                // Adapter doesn't exist - nothing to delete
                return Ok(());
            }
        };

        // Check active_pinned_adapters view (single source of truth)
        // View automatically filters expired pins (pinned_until > now())
        let active_pin_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM active_pinned_adapters WHERE adapter_id = ?")
                .bind(&adapter_id)
                .fetch_one(&*self.pool())
                .await
                .unwrap_or(0);

        if active_pin_count > 0 {
            warn!(
                id = %id,
                adapter_id = %adapter_id,
                pin_count = active_pin_count,
                "Attempted to delete adapter with active pins"
            );
            return Err(AosError::PolicyViolation(format!(
                "Cannot delete adapter '{}': adapter has {} active pin(s). Unpin first.",
                adapter_id, active_pin_count
            )));
        }

        // Not pinned - safe to delete
        sqlx::query("DELETE FROM adapters WHERE id = ?")
            .bind(id)
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
            if let Err(e) = repo.delete_adapter_kv(&adapter_id).await {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL delete committed but KV delete failed in strict mode. KV entry may be orphaned."
                    );
                    return Err(AosError::Database(format!(
                        "Adapter deleted in SQL but KV delete failed (strict mode): {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to delete adapter from KV backend");
                }
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Adapter deleted from both SQL and KV backends");
            }
        }

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
        use tracing::info;

        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Get adapter_id and tenant_id for pinning check and KV dual-write
        let adapter_data: Option<(String, String)> =
            sqlx::query_as("SELECT adapter_id, tenant_id FROM adapters WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;

        let (adapter_id, tenant_id) = match adapter_data {
            Some((aid, tid)) => (aid, tid),
            None => {
                return Err(AosError::NotFound(format!("Adapter not found: {}", id)));
            }
        };

        // Check active_pinned_adapters view (single source of truth)
        let active_pin_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM active_pinned_adapters WHERE adapter_id = ?")
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
            return Err(AosError::PolicyViolation(format!(
                "Cannot delete adapter '{}': adapter has {} active pin(s)",
                adapter_id, active_pin_count
            )));
        }

        // Delete from pinned_adapters (expired pins)
        // Use subquery to find adapter_pk from adapters.id where adapter_id matches
        sqlx::query(
            "DELETE FROM pinned_adapters WHERE adapter_pk IN
             (SELECT id FROM adapters WHERE adapter_id = ?)",
        )
        .bind(&adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        info!(id = %id, adapter_id = %adapter_id, "Deleting adapter with cascade");

        // Delete the adapter itself
        sqlx::query("DELETE FROM adapters WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode) - after transaction commit
        if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
            if let Err(e) = repo.delete_adapter_kv(&adapter_id).await {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL cascade delete committed but KV delete failed in strict mode. KV entry may be orphaned."
                    );
                    return Err(AosError::Database(format!(
                        "Cascade delete succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to cascade delete adapter from KV backend");
                }
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Adapter cascade deleted from both SQL and KV backends");
            }
        }

        Ok(())
    }

    /// Get adapter by ID
    pub async fn get_adapter(&self, adapter_id: &str) -> Result<Option<Adapter>> {
        // Try KV first if enabled
        if self.storage_mode().read_from_kv() {
            // First try to get tenant_id from SQL for tenant-scoped KV lookup
            if let Some(tenant_id) = self.get_adapter_tenant_id(adapter_id).await? {
                if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
                    match repo.get_adapter_kv(adapter_id).await {
                        Ok(Some(adapter)) => {
                            debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-primary", "Retrieved adapter from KV");
                            return Ok(Some(adapter));
                        }
                        Ok(None) if self.storage_mode().sql_fallback_enabled() => {
                            debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-fallback", "KV returned None, falling back to SQL");
                        }
                        Ok(None) => {
                            return Ok(None);
                        }
                        Err(e) if self.storage_mode().sql_fallback_enabled() => {
                            warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-fallback", "KV read failed, falling back to SQL");
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
            } else {
                // SQL doesn't have this adapter - try direct KV lookup for KV-only data
                // This handles the case where data exists only in KV (e.g., during migration)
                if let Some(adapter) = self.get_adapter_from_kv_direct(adapter_id).await? {
                    debug!(adapter_id = %adapter_id, mode = "kv-direct", "Retrieved KV-only adapter");
                    return Ok(Some(adapter));
                }
            }
        }

        // SQL fallback or primary read
        let query = format!(
            "SELECT {} FROM adapters WHERE adapter_id = ?",
            ADAPTER_SELECT_FIELDS
        );
        let adapter = sqlx::query_as::<_, Adapter>(&query)
            .bind(adapter_id)
            .fetch_optional(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(adapter)
    }

    /// Find adapter by BLAKE3 hash for deduplication
    ///
    /// Returns an existing active adapter with the same hash_b3, enabling
    /// content-addressed deduplication during import.
    pub async fn find_adapter_by_hash(&self, hash_b3: &str) -> Result<Option<Adapter>> {
        // Try KV first if enabled
        if self.storage_mode().read_from_kv() {
            // Note: We need to check across all tenants for hash lookup, so we'll try SQL
            // or we'd need to iterate through tenants. For now, fall through to SQL.
            // TODO: Add multi-tenant hash index to KV backend
            if self.storage_mode().sql_fallback_enabled() {
                debug!(hash_b3 = %hash_b3, mode = "sql-required", "Hash lookup requires cross-tenant search, using SQL");
            }
        }

        // SQL fallback or primary read (needed for cross-tenant hash lookup)
        let query = format!(
            "SELECT {} FROM adapters WHERE hash_b3 = ? AND active = 1 LIMIT 1",
            ADAPTER_SELECT_FIELDS
        );
        let adapter = sqlx::query_as::<_, Adapter>(&query)
            .bind(hash_b3)
            .fetch_optional(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to find adapter by hash: {}", e)))?;
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
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
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
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
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
        .fetch_one(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        let total: i64 = row
            .try_get("total")
            .map_err(|e| AosError::Database(e.to_string()))?;
        let selected: i64 = row.try_get("selected_count").unwrap_or(0);
        let avg_gate: f64 = row.try_get("avg_gate").unwrap_or(0.0);

        Ok((total, selected, avg_gate))
    }

    /// Update adapter state
    pub async fn update_adapter_state(
        &self,
        adapter_id: &str,
        state: &str,
        reason: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE adapters SET current_state = ?, updated_at = datetime('now') WHERE adapter_id = ?"
        )
        .bind(state)
        .bind(adapter_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(tenant_id) = self.get_adapter_tenant_id(adapter_id).await? {
            if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
                if let Err(e) = repo
                    .update_adapter_state_kv(adapter_id, state, reason)
                    .await
                {
                    if self.dual_write_requires_strict() {
                        error!(
                            error = %e,
                            adapter_id = %adapter_id,
                            tenant_id = %tenant_id,
                            mode = "dual-write-strict",
                            "CONSISTENCY WARNING: SQL state update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                        );
                        return Err(AosError::Database(format!(
                            "State update succeeded in SQL but failed in KV (strict mode): {e}"
                        )));
                    } else {
                        warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to update adapter state in KV backend");
                    }
                } else {
                    debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, state = %state, mode = "dual-write", "Adapter state updated in both SQL and KV backends");
                }
            }
        }

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
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(tenant_id) = self.get_adapter_tenant_id(adapter_id).await? {
            if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
                if let Err(e) = repo
                    .update_adapter_memory_kv(adapter_id, memory_bytes)
                    .await
                {
                    if self.dual_write_requires_strict() {
                        error!(
                            error = %e,
                            adapter_id = %adapter_id,
                            tenant_id = %tenant_id,
                            mode = "dual-write-strict",
                            "CONSISTENCY WARNING: SQL memory update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                        );
                        return Err(AosError::Database(format!(
                            "Memory update succeeded in SQL but failed in KV (strict mode): {e}"
                        )));
                    } else {
                        warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to update adapter memory in KV backend");
                    }
                } else {
                    debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, memory_bytes = %memory_bytes, mode = "dual-write", "Adapter memory updated in both SQL and KV backends");
                }
            }
        }

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
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Lock the row and get tenant_id for KV dual-write
        let row_data: Option<(String, String)> =
            sqlx::query_as("SELECT adapter_id, tenant_id FROM adapters WHERE adapter_id = ?")
                .bind(adapter_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;

        let tenant_id = match row_data {
            Some((_, tid)) => tid,
            None => {
                warn!(adapter_id = %adapter_id, "Adapter not found for state update");
                return Err(AosError::NotFound(format!(
                    "Adapter not found: {}",
                    adapter_id
                )));
            }
        };

        // Update state with reason logged
        debug!(adapter_id = %adapter_id, state = %state, reason = %reason,
               "Updating adapter state (transactional)");

        sqlx::query(
            "UPDATE adapters SET current_state = ?, updated_at = datetime('now') WHERE adapter_id = ?"
        )
        .bind(state)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode) - after transaction commit
        if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
            if let Err(e) = repo
                .update_adapter_state_kv(adapter_id, state, reason)
                .await
            {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL state update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                    );
                    return Err(AosError::Database(format!(
                        "State update (tx) succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to update adapter state in KV backend");
                }
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, state = %state, mode = "dual-write", "Adapter state updated in both SQL and KV backends (tx)");
            }
        }

        Ok(())
    }

    /// Compare-and-swap (CAS) update of adapter state
    ///
    /// Atomically updates the adapter state only if the current state matches the expected state.
    /// This prevents TOCTOU (Time-of-Check-to-Time-of-Use) race conditions where two concurrent
    /// requests might both read the same state and try to transition, causing invalid state sequences.
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter to update
    /// * `expected_state` - The state we expect the adapter to be in
    /// * `new_state` - The state to transition to
    /// * `reason` - Human-readable reason for the transition (audit trail)
    ///
    /// # Returns
    /// * `Ok(true)` - State was updated successfully
    /// * `Ok(false)` - State was not updated because current state != expected_state
    /// * `Err(AosError::NotFound)` - Adapter doesn't exist
    /// * `Err(AosError::Database)` - Database error
    ///
    /// # Example
    /// ```ignore
    /// // Only promote from cold to warm if still in cold state
    /// let updated = db.update_adapter_state_cas(
    ///     "adapter-123", "cold", "warm", "promoting for inference"
    /// ).await?;
    /// if !updated {
    ///     // Another request already changed the state - retry or handle conflict
    /// }
    /// ```
    pub async fn update_adapter_state_cas(
        &self,
        adapter_id: &str,
        expected_state: &str,
        new_state: &str,
        reason: &str,
    ) -> Result<bool> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Lock the row and verify current state
        let row_data: Option<(String, String, String)> = sqlx::query_as(
            "SELECT adapter_id, tenant_id, current_state FROM adapters WHERE adapter_id = ?",
        )
        .bind(adapter_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        let (tenant_id, current_state) = match row_data {
            Some((_, tid, state)) => (tid, state),
            None => {
                warn!(adapter_id = %adapter_id, "Adapter not found for CAS state update");
                return Err(AosError::NotFound(format!(
                    "Adapter not found: {}",
                    adapter_id
                )));
            }
        };

        // CAS check: only update if current state matches expected
        if current_state != expected_state {
            debug!(
                adapter_id = %adapter_id,
                expected = %expected_state,
                actual = %current_state,
                "CAS state update rejected: state mismatch"
            );
            return Ok(false);
        }

        // State matches, proceed with update
        debug!(
            adapter_id = %adapter_id,
            old_state = %expected_state,
            new_state = %new_state,
            reason = %reason,
            "CAS state update: transitioning"
        );

        sqlx::query(
            "UPDATE adapters SET current_state = ?, updated_at = datetime('now') WHERE adapter_id = ? AND current_state = ?",
        )
        .bind(new_state)
        .bind(adapter_id)
        .bind(expected_state) // Double-check in WHERE clause for atomicity
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode) - after transaction commit
        if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
            if let Err(e) = repo
                .update_adapter_state_kv(adapter_id, new_state, reason)
                .await
            {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL state update committed but KV write failed in strict mode (CAS). Use ensure_consistency() to repair."
                    );
                    return Err(AosError::Database(format!(
                        "State update (CAS) succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to update adapter state in KV backend (CAS)");
                }
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, new_state = %new_state, mode = "dual-write", "Adapter state updated in both SQL and KV backends (CAS)");
            }
        }

        info!(
            adapter_id = %adapter_id,
            old_state = %expected_state,
            new_state = %new_state,
            reason = %reason,
            "Adapter state CAS update successful"
        );

        Ok(true)
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
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Verify adapter exists and get tenant_id for KV dual-write
        let row_data: Option<(String, String)> =
            sqlx::query_as("SELECT adapter_id, tenant_id FROM adapters WHERE adapter_id = ?")
                .bind(adapter_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;

        let tenant_id = match row_data {
            Some((_, tid)) => tid,
            None => {
                return Err(AosError::NotFound(format!(
                    "Adapter not found: {}",
                    adapter_id
                )));
            }
        };

        debug!(adapter_id = %adapter_id, memory_bytes = %memory_bytes,
               "Updating adapter memory (transactional)");

        sqlx::query(
            "UPDATE adapters SET memory_bytes = ?, updated_at = datetime('now') WHERE adapter_id = ?"
        )
        .bind(memory_bytes)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode) - after transaction commit
        if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
            if let Err(e) = repo
                .update_adapter_memory_kv(adapter_id, memory_bytes)
                .await
            {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL memory update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                    );
                    return Err(AosError::Database(format!(
                        "Memory update (tx) succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to update adapter memory in KV backend");
                }
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, memory_bytes = %memory_bytes, mode = "dual-write", "Adapter memory updated in both SQL and KV backends (tx)");
            }
        }

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
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Verify adapter exists and get tenant_id for KV dual-write
        let row_data: Option<(String, String)> =
            sqlx::query_as("SELECT adapter_id, tenant_id FROM adapters WHERE adapter_id = ?")
                .bind(adapter_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;

        let tenant_id = match row_data {
            Some((_, tid)) => tid,
            None => {
                return Err(AosError::NotFound(format!(
                    "Adapter not found: {}",
                    adapter_id
                )));
            }
        };

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
             WHERE adapter_id = ?",
        )
        .bind(state)
        .bind(memory_bytes)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // KV write (dual-write mode) - after transaction commit
        if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
            if let Err(e) = repo
                .update_adapter_state_and_memory_kv(adapter_id, state, memory_bytes, reason)
                .await
            {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL state/memory update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                    );
                    return Err(AosError::Database(format!(
                        "State/memory update succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to update adapter state/memory in KV backend");
                }
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, state = %state, memory_bytes = %memory_bytes, mode = "dual-write", "Adapter state/memory updated in both SQL and KV backends");
            }
        }

        Ok(())
    }

    /// List adapters by category
    pub async fn list_adapters_by_category(&self, category: &str) -> Result<Vec<Adapter>> {
        // Try KV first if enabled
        // Note: This is a cross-tenant query, so we need to aggregate across all tenants
        // For now, we'll use SQL for simplicity. KV optimization would require a global index.
        if self.storage_mode().read_from_kv() {
            if self.storage_mode().sql_fallback_enabled() {
                debug!(category = %category, mode = "sql-required", "Category lookup requires cross-tenant search, using SQL");
            }
            // TODO: Add category index to KV backend for cross-tenant queries
        }

        // SQL fallback or primary read
        let query = format!(
            "SELECT {} FROM adapters WHERE active = 1 AND category = ? ORDER BY activation_count DESC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(category)
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(adapters)
    }

    /// List adapters by scope
    pub async fn list_adapters_by_scope(&self, scope: &str) -> Result<Vec<Adapter>> {
        // Try KV first if enabled
        // Note: This is a cross-tenant query, so we need to aggregate across all tenants
        // For now, we'll use SQL for simplicity. KV optimization would require a global index.
        if self.storage_mode().read_from_kv() {
            if self.storage_mode().sql_fallback_enabled() {
                debug!(scope = %scope, mode = "sql-required", "Scope lookup requires cross-tenant search, using SQL");
            }
            // TODO: Add scope index to KV backend for cross-tenant queries
        }

        // SQL fallback or primary read
        let query = format!(
            "SELECT {} FROM adapters WHERE active = 1 AND scope = ? ORDER BY activation_count DESC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(scope)
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(adapters)
    }

    /// List adapters by state
    pub async fn list_adapters_by_state(&self, state: &str) -> Result<Vec<Adapter>> {
        // Try KV first if enabled
        // Note: This is a cross-tenant query, so we need to aggregate across all tenants
        // For now, we'll use SQL for simplicity. KV optimization would require a global index.
        if self.storage_mode().read_from_kv() {
            if self.storage_mode().sql_fallback_enabled() {
                debug!(state = %state, mode = "sql-required", "State lookup requires cross-tenant search, using SQL");
            }
            // TODO: Add state index to KV backend for cross-tenant queries
        }

        // SQL fallback or primary read
        let query = format!(
            "SELECT {} FROM adapters WHERE active = 1 AND current_state = ? ORDER BY activation_count DESC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(state)
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
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
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        let mut result = Vec::new();
        for row in summary {
            let category: String = row
                .try_get("category")
                .map_err(|e| AosError::Database(e.to_string()))?;
            let scope: String = row
                .try_get("scope")
                .map_err(|e| AosError::Database(e.to_string()))?;
            let state: String = row
                .try_get("current_state")
                .map_err(|e| AosError::Database(e.to_string()))?;
            let count: i64 = row
                .try_get("count")
                .map_err(|e| AosError::Database(e.to_string()))?;
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

    // ============================================================================
    // Adapter Lineage Queries
    // ============================================================================

    /// Get full lineage tree for an adapter (ancestors and descendants)
    ///
    /// Returns all adapters in the lineage tree, including:
    /// - Ancestors (parent, grandparent, etc.)
    /// - The adapter itself
    /// - Descendants (children, grandchildren, etc.)
    ///
    /// Uses recursive CTEs to traverse parent_id relationships.
    pub async fn get_adapter_lineage(&self, adapter_id: &str) -> Result<Vec<Adapter>> {
        // Try KV first if enabled
        if self.storage_mode().read_from_kv() {
            if let Some(tenant_id) = self.get_adapter_tenant_id(adapter_id).await? {
                if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
                    match repo.get_adapter_lineage_kv(adapter_id).await {
                        Ok(adapters) if !adapters.is_empty() => {
                            debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, count = adapters.len(), mode = "kv-primary", "Retrieved lineage from KV");
                            return Ok(adapters);
                        }
                        Ok(_) if self.storage_mode().sql_fallback_enabled() => {
                            debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-fallback", "KV returned empty lineage, falling back to SQL");
                        }
                        Ok(adapters) => {
                            return Ok(adapters);
                        }
                        Err(e) if self.storage_mode().sql_fallback_enabled() => {
                            warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-fallback", "KV lineage read failed, falling back to SQL");
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
            }
        }

        // SQL fallback or primary read
        let query = format!(
            "WITH RECURSIVE
             -- Get ancestors (walk up parent_id chain)
             ancestors AS (
                 SELECT {}, 1 as depth
                 FROM adapters
                 WHERE adapter_id = ?
                 UNION ALL
                 SELECT {}, anc.depth + 1
                 FROM adapters a
                 JOIN ancestors anc ON a.id = anc.parent_id
                 WHERE anc.depth < 10  -- Prevent infinite loops
             ),
             -- Get descendants (walk down parent_id references)
             descendants AS (
                 SELECT {}, 1 as depth
                 FROM adapters
                 WHERE adapter_id = ?
                 UNION ALL
                 SELECT {}, desc.depth + 1
                FROM adapters a
                JOIN descendants desc ON a.parent_id = desc.id
                 WHERE desc.depth < 10  -- Prevent infinite loops
             )
             SELECT DISTINCT {}
             FROM (
                 SELECT * FROM ancestors
                 UNION
                 SELECT * FROM descendants
             )
             ORDER BY created_at ASC",
            ADAPTER_SELECT_FIELDS,
            ADAPTER_COLUMNS_ALIAS_A,
            ADAPTER_SELECT_FIELDS,
            ADAPTER_COLUMNS_ALIAS_A,
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(adapter_id)
            .bind(adapter_id)
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(adapters)
    }

    /// Get direct children of an adapter
    ///
    /// Returns all adapters that have this adapter as their parent_id.
    pub async fn get_adapter_children(&self, adapter_id: &str) -> Result<Vec<Adapter>> {
        // Try KV first if enabled
        if self.storage_mode().read_from_kv() {
            if let Some(tenant_id) = self.get_adapter_tenant_id(adapter_id).await? {
                if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
                    match repo.get_adapter_children_kv(adapter_id).await {
                        Ok(adapters) if !adapters.is_empty() => {
                            debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, count = adapters.len(), mode = "kv-primary", "Retrieved children from KV");
                            return Ok(adapters);
                        }
                        Ok(_) if self.storage_mode().sql_fallback_enabled() => {
                            debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-fallback", "KV returned empty children list, falling back to SQL");
                        }
                        Ok(adapters) => {
                            return Ok(adapters);
                        }
                        Err(e) if self.storage_mode().sql_fallback_enabled() => {
                            warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-fallback", "KV children read failed, falling back to SQL");
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
            }
        }

        // SQL fallback or primary read
        let query = format!(
            "SELECT {} FROM adapters WHERE parent_id = ? AND active = 1 ORDER BY revision ASC, created_at ASC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(adapter_id)
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(adapters)
    }

    /// Get lineage path from root to adapter
    ///
    /// Returns ordered list of adapters from root ancestor to the specified adapter,
    /// tracing the parent_id chain upwards.
    pub async fn get_lineage_path(&self, adapter_id: &str) -> Result<Vec<Adapter>> {
        let query = format!(
            "WITH RECURSIVE lineage AS (
                 SELECT {}, 0 as depth
                 FROM adapters
                 WHERE adapter_id = ?
                 UNION ALL
                 SELECT a.id, a.tenant_id, a.adapter_id, a.name, a.hash_b3, a.rank, a.alpha, a.tier, a.targets_json, a.acl_json,
                        a.languages_json, a.framework, a.category, a.scope, a.framework_id, a.framework_version,
                        a.repo_id, a.commit_sha, a.intent, a.current_state, a.pinned, a.memory_bytes, a.last_activated,
                        a.activation_count, a.expires_at, a.load_state, a.last_loaded_at, a.aos_file_path, a.aos_file_hash,
                        a.adapter_name, a.tenant_namespace, a.domain, a.purpose, a.revision, a.parent_id, a.fork_type, a.fork_reason,
                        a.created_at, a.updated_at, a.active, a.version, a.lifecycle_state, lin.depth + 1
                 FROM adapters a
                 JOIN lineage lin ON a.adapter_id = lin.parent_id
                 WHERE lin.depth < 10  -- Prevent infinite loops
             )
             SELECT {}
             FROM lineage
             ORDER BY depth DESC",
            ADAPTER_SELECT_FIELDS, ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(adapter_id)
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(adapters)
    }

    /// Find latest revision number for a given adapter family
    ///
    /// Searches for adapters with matching tenant_namespace, domain, and purpose,
    /// and returns the highest revision number found (e.g., "r042" -> 42).
    ///
    /// Returns None if no adapters found or if revisions don't follow rNNN format.
    pub async fn find_latest_revision(
        &self,
        tenant_namespace: &str,
        domain: &str,
        purpose: &str,
    ) -> Result<Option<i32>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT revision FROM adapters
             WHERE tenant_namespace = ? AND domain = ? AND purpose = ? AND active = 1
             ORDER BY revision DESC
             LIMIT 1",
        )
        .bind(tenant_namespace)
        .bind(domain)
        .bind(purpose)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        if let Some((revision_str,)) = row {
            // Parse revision string (e.g., "r042" -> 42)
            if let Some(stripped) = revision_str.strip_prefix('r') {
                if let Ok(rev_num) = stripped.parse::<i32>() {
                    return Ok(Some(rev_num));
                }
            }
        }

        Ok(None)
    }

    /// Validate revision gap constraint
    ///
    /// Ensures that the difference between the highest and lowest revision numbers
    /// in an adapter family does not exceed max_gap (default: 5).
    ///
    /// Returns Ok(()) if constraint is satisfied, Err otherwise.
    pub async fn validate_revision_gap(
        &self,
        tenant_namespace: &str,
        domain: &str,
        purpose: &str,
        max_gap: i32,
    ) -> Result<()> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT revision FROM adapters
             WHERE tenant_namespace = ? AND domain = ? AND purpose = ? AND active = 1
             ORDER BY revision ASC",
        )
        .bind(tenant_namespace)
        .bind(domain)
        .bind(purpose)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        if rows.len() < 2 {
            return Ok(()); // No gap if only 0-1 adapters
        }

        let mut revisions: Vec<i32> = Vec::new();
        for (revision_str,) in rows {
            if let Some(stripped) = revision_str.strip_prefix('r') {
                if let Ok(rev_num) = stripped.parse::<i32>() {
                    revisions.push(rev_num);
                }
            }
        }

        if revisions.is_empty() {
            return Ok(());
        }

        let min_rev = *revisions.iter().min().unwrap();
        let max_rev = *revisions.iter().max().unwrap();
        let gap = max_rev - min_rev;

        if gap > max_gap {
            return Err(AosError::Validation(format!(
                "Revision gap ({}) exceeds maximum allowed ({}) for adapter family {}/{}/{}",
                gap, max_gap, tenant_namespace, domain, purpose
            )));
        }

        Ok(())
    }

    /// Update adapter tier
    pub async fn update_adapter_tier(&self, adapter_id: &str, tier: &str) -> Result<()> {
        sqlx::query(
            "UPDATE adapters SET tier = ?, updated_at = datetime('now') WHERE adapter_id = ?",
        )
        .bind(tier)
        .bind(adapter_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update adapter tier: {}", e)))?;

        // KV write (dual-write mode)
        if let Some(tenant_id) = self.get_adapter_tenant_id(adapter_id).await? {
            if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
                if let Err(e) = repo.update_adapter_tier_kv(adapter_id, tier).await {
                    if self.dual_write_requires_strict() {
                        error!(
                            error = %e,
                            adapter_id = %adapter_id,
                            tenant_id = %tenant_id,
                            mode = "dual-write-strict",
                            "CONSISTENCY WARNING: SQL tier update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                        );
                        return Err(AosError::Database(format!(
                            "Tier update succeeded in SQL but failed in KV (strict mode): {e}"
                        )));
                    } else {
                        warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to update adapter tier in KV backend");
                    }
                } else {
                    debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, tier = %tier, mode = "dual-write", "Adapter tier updated in both SQL and KV backends");
                }
            }
        }

        Ok(())
    }

    /// Ensure consistency between SQL and KV storage for a single adapter.
    ///
    /// Returns:
    /// - Ok(true) if adapter is consistent or was repaired
    /// - Ok(false) if adapter not found in SQL
    pub async fn ensure_consistency(&self, adapter_id: &str) -> Result<bool> {
        // Get adapter from SQL (source of truth during migration)
        let query = format!(
            "SELECT {} FROM adapters WHERE adapter_id = ?",
            ADAPTER_SELECT_FIELDS
        );
        let adapter = match sqlx::query_as::<_, Adapter>(&query)
            .bind(adapter_id)
            .fetch_optional(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?
        {
            Some(a) => a,
            None => return Ok(false),
        };

        // If KV is not available, consider consistent
        let repo = match self.get_adapter_kv_repo(&adapter.tenant_id) {
            Some(r) => r,
            None => return Ok(true),
        };

        // Check KV entry
        match repo.get_adapter_kv(adapter_id).await {
            Ok(Some(kv_adapter)) => {
                let fields_match = kv_adapter.hash_b3 == adapter.hash_b3
                    && kv_adapter.tier == adapter.tier
                    && kv_adapter.current_state == adapter.current_state
                    && kv_adapter.memory_bytes == adapter.memory_bytes;

                if fields_match {
                    return Ok(true);
                }

                // Repair by re-registering from SQL data
                warn!(
                    adapter_id = %adapter_id,
                    tenant_id = %adapter.tenant_id,
                    "Inconsistency detected between SQL and KV - repairing from SQL"
                );

                let params = AdapterRegistrationParams {
                    tenant_id: adapter.tenant_id.clone(),
                    adapter_id: adapter
                        .adapter_id
                        .clone()
                        .unwrap_or_else(|| adapter_id.to_string()),
                    name: adapter.name.clone(),
                    hash_b3: adapter.hash_b3.clone(),
                    rank: adapter.rank,
                    tier: adapter.tier.clone(),
                    alpha: adapter.alpha,
                    targets_json: adapter.targets_json.clone(),
                    acl_json: adapter.acl_json.clone(),
                    languages_json: adapter.languages_json.clone(),
                    framework: adapter.framework.clone(),
                    category: adapter.category.clone(),
                    scope: adapter.scope.clone(),
                    framework_id: adapter.framework_id.clone(),
                    framework_version: adapter.framework_version.clone(),
                    repo_id: adapter.repo_id.clone(),
                    commit_sha: adapter.commit_sha.clone(),
                    intent: adapter.intent.clone(),
                    expires_at: adapter.expires_at.clone(),
                    aos_file_path: adapter.aos_file_path.clone(),
                    aos_file_hash: adapter.aos_file_hash.clone(),
                    adapter_name: adapter.adapter_name.clone(),
                    tenant_namespace: adapter.tenant_namespace.clone(),
                    domain: adapter.domain.clone(),
                    purpose: adapter.purpose.clone(),
                    revision: adapter.revision.clone(),
                    parent_id: adapter.parent_id.clone(),
                    fork_type: adapter.fork_type.clone(),
                    fork_reason: adapter.fork_reason.clone(),
                    base_model_id: adapter.base_model_id.clone(),
                    manifest_schema_version: adapter.manifest_schema_version.clone(),
                    content_hash_b3: adapter.content_hash_b3.clone(),
                    provenance_json: adapter.provenance_json.clone(),
                };

                // Delete old KV entry then re-register and sync state/memory
                let _ = repo.delete_adapter_kv(adapter_id).await;
                repo.register_adapter_kv(params)
                    .await
                    .map_err(|e| AosError::Database(format!("Failed to repair KV entry: {}", e)))?;
                repo.update_adapter_state_kv(
                    adapter_id,
                    &adapter.current_state,
                    "consistency_repair",
                )
                .await
                .map_err(|e| AosError::Database(format!("Failed to repair KV state: {}", e)))?;
                repo.update_adapter_memory_kv(adapter_id, adapter.memory_bytes)
                    .await
                    .map_err(|e| {
                        AosError::Database(format!("Failed to repair KV memory: {}", e))
                    })?;

                Ok(true)
            }
            Ok(None) => {
                // Missing in KV - create it
                warn!(
                    adapter_id = %adapter_id,
                    tenant_id = %adapter.tenant_id,
                    "Adapter missing in KV - creating from SQL"
                );

                let params = AdapterRegistrationParams {
                    tenant_id: adapter.tenant_id.clone(),
                    adapter_id: adapter
                        .adapter_id
                        .clone()
                        .unwrap_or_else(|| adapter_id.to_string()),
                    name: adapter.name.clone(),
                    hash_b3: adapter.hash_b3.clone(),
                    rank: adapter.rank,
                    tier: adapter.tier.clone(),
                    alpha: adapter.alpha,
                    targets_json: adapter.targets_json.clone(),
                    acl_json: adapter.acl_json.clone(),
                    languages_json: adapter.languages_json.clone(),
                    framework: adapter.framework.clone(),
                    category: adapter.category.clone(),
                    scope: adapter.scope.clone(),
                    framework_id: adapter.framework_id.clone(),
                    framework_version: adapter.framework_version.clone(),
                    repo_id: adapter.repo_id.clone(),
                    commit_sha: adapter.commit_sha.clone(),
                    intent: adapter.intent.clone(),
                    expires_at: adapter.expires_at.clone(),
                    aos_file_path: adapter.aos_file_path.clone(),
                    aos_file_hash: adapter.aos_file_hash.clone(),
                    adapter_name: adapter.adapter_name.clone(),
                    tenant_namespace: adapter.tenant_namespace.clone(),
                    domain: adapter.domain.clone(),
                    purpose: adapter.purpose.clone(),
                    revision: adapter.revision.clone(),
                    parent_id: adapter.parent_id.clone(),
                    fork_type: adapter.fork_type.clone(),
                    fork_reason: adapter.fork_reason.clone(),
                    base_model_id: adapter.base_model_id.clone(),
                    manifest_schema_version: adapter.manifest_schema_version.clone(),
                    content_hash_b3: adapter.content_hash_b3.clone(),
                    provenance_json: adapter.provenance_json.clone(),
                };

                repo.register_adapter_kv(params).await.map_err(|e| {
                    AosError::Database(format!("Failed to create adapter in KV: {}", e))
                })?;
                repo.update_adapter_state_kv(
                    adapter_id,
                    &adapter.current_state,
                    "consistency_repair",
                )
                .await
                .map_err(|e| AosError::Database(format!("Failed to sync state to KV: {}", e)))?;
                repo.update_adapter_memory_kv(adapter_id, adapter.memory_bytes)
                    .await
                    .map_err(|e| {
                        AosError::Database(format!("Failed to sync memory to KV: {}", e))
                    })?;

                Ok(true)
            }
            Err(e) => Err(AosError::Database(format!(
                "Consistency check failed: {}",
                e
            ))),
        }
    }

    /// Batch ensure consistency for multiple adapters
    pub async fn ensure_consistency_batch(
        &self,
        adapter_ids: &[String],
    ) -> Vec<(String, Result<bool>)> {
        let mut results = Vec::new();

        for adapter_id in adapter_ids {
            let res = self.ensure_consistency(adapter_id).await;
            results.push((adapter_id.clone(), res));
        }

        results
    }

    /// Validate consistency for all adapters in a tenant
    ///
    /// Returns (consistent, inconsistent, errors)
    pub async fn validate_tenant_consistency(
        &self,
        tenant_id: &str,
        repair: bool,
    ) -> Result<(usize, usize, usize)> {
        let adapters = self.list_adapters_for_tenant(tenant_id).await?;

        let mut consistent = 0usize;
        let mut inconsistent = 0usize;
        let mut errors = 0usize;

        for adapter in adapters {
            if let Some(adapter_id) = &adapter.adapter_id {
                if repair {
                    match self.ensure_consistency(adapter_id).await {
                        Ok(true) => consistent += 1,
                        Ok(false) => {}
                        Err(_) => {
                            inconsistent += 1;
                            errors += 1;
                        }
                    }
                } else {
                    // Check-only path (no repair)
                    match self.get_adapter_kv_repo(&adapter.tenant_id) {
                        None => {
                            consistent += 1;
                        }
                        Some(repo) => match repo.get_adapter_kv(adapter_id).await {
                            Ok(Some(kv_adapter)) => {
                                let fields_match = kv_adapter.hash_b3 == adapter.hash_b3
                                    && kv_adapter.tier == adapter.tier
                                    && kv_adapter.current_state == adapter.current_state
                                    && kv_adapter.memory_bytes == adapter.memory_bytes;

                                if fields_match {
                                    consistent += 1;
                                } else {
                                    inconsistent += 1;
                                }
                            }
                            Ok(None) => {
                                inconsistent += 1;
                            }
                            Err(_) => {
                                inconsistent += 1;
                                errors += 1;
                            }
                        },
                    }
                }
            }
        }

        Ok((consistent, inconsistent, errors))
    }

    // =========================================================================
    // Archive & Garbage Collection Operations (from migration 0138)
    // =========================================================================

    /// Archive adapters for a tenant (cascade from tenant archival)
    ///
    /// Sets `archived_at` timestamp for all active, non-archived adapters
    /// belonging to the tenant. Does NOT delete .aos files - that's handled by GC.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant whose adapters to archive
    /// * `archived_by` - User/system initiating the archive
    /// * `reason` - Human-readable reason (e.g., "tenant_archived")
    ///
    /// # Returns
    /// Number of adapters archived
    pub async fn archive_adapters_for_tenant(
        &self,
        tenant_id: &str,
        archived_by: &str,
        reason: &str,
    ) -> Result<u64> {
        // First, get the list of adapter IDs that will be affected (for KV dual-write)
        let affected_adapter_ids: Vec<String> = sqlx::query_scalar(
            "SELECT adapter_id FROM adapters
             WHERE tenant_id = ?
               AND archived_at IS NULL
               AND active = 1",
        )
        .bind(tenant_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to query adapters: {}", e)))?;

        let result = sqlx::query(
            "UPDATE adapters
             SET archived_at = datetime('now'),
                 archived_by = ?,
                 archive_reason = ?,
                 updated_at = datetime('now')
             WHERE tenant_id = ?
               AND archived_at IS NULL
               AND active = 1",
        )
        .bind(archived_by)
        .bind(reason)
        .bind(tenant_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to archive adapters: {}", e)))?;

        info!(
            tenant_id = %tenant_id,
            archived_by = %archived_by,
            count = result.rows_affected(),
            "Archived adapters for tenant"
        );

        // KV write (dual-write mode) - archive each adapter in KV backend
        if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
            let mut kv_success_count = 0u64;
            let mut kv_error_count = 0u64;

            for adapter_id in &affected_adapter_ids {
                match repo
                    .archive_adapter_kv(adapter_id, archived_by, reason)
                    .await
                {
                    Ok(()) => {
                        kv_success_count += 1;
                    }
                    Err(e) => {
                        kv_error_count += 1;
                        warn!(
                            error = %e,
                            adapter_id = %adapter_id,
                            tenant_id = %tenant_id,
                            mode = "dual-write",
                            "Failed to archive adapter in KV backend"
                        );
                    }
                }
            }

            if kv_error_count > 0 {
                warn!(
                    tenant_id = %tenant_id,
                    success_count = kv_success_count,
                    error_count = kv_error_count,
                    mode = "dual-write",
                    "Partial KV archive failure for tenant adapters"
                );
            } else if kv_success_count > 0 {
                debug!(
                    tenant_id = %tenant_id,
                    count = kv_success_count,
                    mode = "dual-write",
                    "Archived adapters in both SQL and KV backends"
                );
            }
        }

        Ok(result.rows_affected())
    }

    /// Archive a single adapter
    ///
    /// Sets `archived_at` timestamp. Does NOT delete .aos file.
    pub async fn archive_adapter(
        &self,
        adapter_id: &str,
        archived_by: &str,
        reason: &str,
    ) -> Result<()> {
        let affected = sqlx::query(
            "UPDATE adapters
             SET archived_at = datetime('now'),
                 archived_by = ?,
                 archive_reason = ?,
                 updated_at = datetime('now')
             WHERE adapter_id = ?
               AND archived_at IS NULL",
        )
        .bind(archived_by)
        .bind(reason)
        .bind(adapter_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to archive adapter: {}", e)))?
        .rows_affected();

        if affected == 0 {
            return Err(AosError::NotFound(format!(
                "Adapter not found or already archived: {}",
                adapter_id
            )));
        }

        info!(adapter_id = %adapter_id, archived_by = %archived_by, "Archived adapter");

        // KV write (dual-write mode)
        if let Some(tenant_id) = self.get_adapter_tenant_id(adapter_id).await? {
            if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
                if let Err(e) = repo
                    .archive_adapter_kv(adapter_id, archived_by, reason)
                    .await
                {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to archive adapter in KV backend");
                } else {
                    debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Adapter archived in both SQL and KV backends");
                }
            }
        }

        Ok(())
    }

    /// Find archived adapters eligible for garbage collection
    ///
    /// Returns adapters where:
    /// - `archived_at` is older than `min_age_days`
    /// - `purged_at` is NULL (not yet purged)
    /// - `aos_file_path` is NOT NULL (file reference exists)
    ///
    /// # Arguments
    /// * `min_age_days` - Minimum days since archival before eligible for GC
    /// * `limit` - Maximum number of adapters to return
    pub async fn find_archived_adapters_for_gc(
        &self,
        min_age_days: u32,
        limit: i64,
    ) -> Result<Vec<Adapter>> {
        let query = format!(
            "SELECT {} FROM adapters
             WHERE archived_at IS NOT NULL
               AND purged_at IS NULL
               AND aos_file_path IS NOT NULL
               AND datetime(archived_at, '+{} days') <= datetime('now')
             ORDER BY archived_at ASC
             LIMIT ?",
            ADAPTER_SELECT_FIELDS, min_age_days
        );

        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(limit)
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to find GC candidates: {}", e)))?;

        Ok(adapters)
    }

    /// Mark an adapter as purged after .aos file deletion
    ///
    /// Sets `purged_at` timestamp and clears `aos_file_path`.
    /// The record is preserved for audit purposes.
    ///
    /// CRITICAL: Call this AFTER successfully deleting the .aos file from disk.
    pub async fn mark_adapter_purged(&self, adapter_id: &str) -> Result<()> {
        let affected = sqlx::query(
            "UPDATE adapters
             SET purged_at = datetime('now'),
                 aos_file_path = NULL,
                 updated_at = datetime('now')
             WHERE adapter_id = ?
               AND archived_at IS NOT NULL
               AND purged_at IS NULL",
        )
        .bind(adapter_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to mark adapter purged: {}", e)))?
        .rows_affected();

        if affected == 0 {
            return Err(AosError::Validation(format!(
                "Adapter {} is not archived or already purged",
                adapter_id
            )));
        }

        info!(adapter_id = %adapter_id, "Marked adapter as purged");

        // KV write (dual-write mode)
        if let Some(tenant_id) = self.get_adapter_tenant_id(adapter_id).await? {
            if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
                if let Err(e) = repo.mark_adapter_purged_kv(adapter_id).await {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to mark adapter purged in KV backend");
                } else {
                    debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Adapter marked purged in both SQL and KV backends");
                }
            }
        }

        Ok(())
    }

    /// Check if an adapter is loadable (not archived/purged)
    ///
    /// Returns `true` if the adapter exists and is neither archived nor purged.
    /// Used by the loader to reject attempts to load unavailable adapters.
    pub async fn is_adapter_loadable(&self, adapter_id: &str) -> Result<bool> {
        let result: Option<(Option<String>, Option<String>)> =
            sqlx::query_as("SELECT archived_at, purged_at FROM adapters WHERE adapter_id = ?")
                .bind(adapter_id)
                .fetch_optional(&*self.pool())
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;

        match result {
            Some((archived_at, purged_at)) => {
                // Loadable if not archived AND not purged
                Ok(archived_at.is_none() && purged_at.is_none())
            }
            None => Err(AosError::NotFound(format!(
                "Adapter not found: {}",
                adapter_id
            ))),
        }
    }

    /// Unarchive an adapter (restore from archived state)
    ///
    /// Only works if adapter is archived but NOT purged.
    /// Cannot restore a purged adapter as the .aos file has been deleted.
    pub async fn unarchive_adapter(&self, adapter_id: &str) -> Result<()> {
        let affected = sqlx::query(
            "UPDATE adapters
             SET archived_at = NULL,
                 archived_by = NULL,
                 archive_reason = NULL,
                 updated_at = datetime('now')
             WHERE adapter_id = ?
               AND archived_at IS NOT NULL
               AND purged_at IS NULL",
        )
        .bind(adapter_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to unarchive adapter: {}", e)))?
        .rows_affected();

        if affected == 0 {
            return Err(AosError::Validation(format!(
                "Adapter {} is not archived or has been purged (cannot restore)",
                adapter_id
            )));
        }

        info!(adapter_id = %adapter_id, "Unarchived adapter");

        // KV write (dual-write mode)
        if let Some(tenant_id) = self.get_adapter_tenant_id(adapter_id).await? {
            if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
                if let Err(e) = repo.unarchive_adapter_kv(adapter_id).await {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to unarchive adapter in KV backend");
                } else {
                    debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Adapter unarchived in both SQL and KV backends");
                }
            }
        }

        Ok(())
    }

    /// Count archived adapters for a tenant
    ///
    /// Returns the number of adapters that are archived but not yet purged.
    pub async fn count_archived_adapters(&self, tenant_id: &str) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM adapters
             WHERE tenant_id = ?
               AND archived_at IS NOT NULL
               AND purged_at IS NULL",
        )
        .bind(tenant_id)
        .fetch_one(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to count archived adapters: {}", e)))?;

        Ok(count)
    }

    /// Count purged adapters for a tenant
    ///
    /// Returns the number of adapters that have been purged (file deleted, record kept).
    pub async fn count_purged_adapters(&self, tenant_id: &str) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM adapters
             WHERE tenant_id = ?
               AND purged_at IS NOT NULL",
        )
        .bind(tenant_id)
        .fetch_one(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to count purged adapters: {}", e)))?;

        Ok(count)
    }
}
