//! # Adapter Storage: KV-SQL Dual-Write Pattern
//!
//! This module implements a **dual-write storage strategy** for adapters, writing to both
//! SQLite (SQL) and a key-value store (KV) simultaneously. This pattern supports the
//! ongoing migration from SQL-only to KV-primary storage.
//!
//! ## Module Structure
//!
//! - `aos_parser`: AOS file parsing utilities (hashing, manifest extraction)
//!
//! ## Why Dual-Write?
//!
//! 1. **Migration Safety**: Gradual rollout without data loss. SQL remains authoritative
//!    during transition; KV is validated against SQL reads.
//! 2. **Performance**: KV store offers O(1) lookups for hot-path operations like
//!    `get_adapter_for_tenant` during inference routing.
//! 3. **Rollback Capability**: If KV issues arise, SQL remains complete and authoritative.
//!
//! ## Modes
//!
//! - **Strict Atomic** (default): KV failures propagate errors and trigger SQL rollback.
//!   Use when KV is proven stable and you want ACID guarantees across both stores.
//! - **Best-Effort** (opt-out): KV writes are logged-on-failure but don't block operations.
//!   Only use for controlled rollouts or debugging.
//!
//! ## Configuration
//!
//! Strict mode is enabled by default. Set `AOS_ATOMIC_DUAL_WRITE_STRICT=0` to opt
//! into best-effort behavior for debugging only.
//!
//! ## Key Functions
//!
//! - `register_adapter_*`: Write to SQL, then KV (rollback SQL on strict KV failure)
//! - `get_adapter_*`: Read from SQL (KV reads coming in future phase)
//! - `update_adapter_*`: Update SQL, then sync to KV
//! - `delete_adapter_*`: Delete from SQL, then KV
//!
//! ## Metrics
//!
//! All dual-write operations emit metrics via `global_kv_metrics()`:
//! - `kv_write_success` / `kv_write_failure`
//! - `kv_write_latency_ms`
//! - `sql_rollback_triggered` (strict mode only)

// Submodules
mod aos_parser;

// Re-exports from submodules
pub use aos_parser::AosRegistrationMetadata;
use aos_parser::{
    compute_aos_file_hash, load_aos_registration_metadata, parse_aos_manifest_metadata,
    read_aos_manifest_bytes, read_aos_segment_count, read_single_file_adapter_metadata,
    ParsedAosManifestMetadata, SingleFileAdapterMetadata,
};

use crate::new_id;
use crate::{Db, WriteCapableDb};
use adapteros_aos::{compute_scope_hash, open_aos, BackendTag};
use adapteros_core::extract_repo_identifier_from_metadata;
use adapteros_core::{AdapterName, AosError, B3Hash, LifecycleState, Result};
use adapteros_id::IdPrefix;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::collections::HashMap;
use std::env;
use std::future::Future;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::adapters_kv::{AdapterKvOps, AdapterKvRepository};
use crate::kv_metrics::global_kv_metrics;
use crate::write_ack::{WriteAck, WriteAckStore, WriteStatus};
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
    "a.id, a.tenant_id, a.adapter_id, a.name, a.hash_b3, a.rank, a.alpha, a.lora_strength, a.tier, \
     a.targets_json, a.acl_json, a.languages_json, a.framework, a.category, a.scope, \
     a.framework_id, a.framework_version, a.repo_id, a.commit_sha, a.intent, \
     a.current_state, a.pinned, a.memory_bytes, a.last_activated, a.activation_count, \
     a.expires_at, a.load_state, a.last_loaded_at, a.aos_file_path, a.aos_file_hash, \
     a.adapter_name, a.tenant_namespace, a.domain, a.purpose, a.revision, a.parent_id, \
     a.fork_type, a.fork_reason, a.version, a.lifecycle_state, a.archived_at, a.archived_by, \
     a.archive_reason, a.purged_at, a.base_model_id, a.recommended_for_moe, a.manifest_schema_version, \
     a.content_hash_b3, a.metadata_json, a.provenance_json, a.repo_path, a.codebase_scope, \
     a.dataset_version_id, a.registration_timestamp, a.manifest_hash, \
     a.adapter_type, a.base_adapter_id, a.stream_session_id, a.versioning_threshold, a.coreml_package_hash, \
     a.training_dataset_hash_b3, a.adapter_version_id, a.effective_version_weight, a.stable_id, \
     a.created_at, a.updated_at, a.active";

tokio::task_local! {
    static TENANT_SCOPE_ACTIVE: bool;
}

// AOS file parsing functions have been moved to aos_parser module

#[derive(Debug, Clone)]
struct AdapterSessionContext {
    session_id: String,
    session_name: Option<String>,
    session_tags: Option<Vec<String>>,
}

fn normalize_session_tags(tags: &mut Vec<String>) {
    tags.iter_mut().for_each(|tag| {
        *tag = tag.trim().to_string();
    });
    tags.retain(|tag| !tag.is_empty());
    tags.sort();
    tags.dedup();
}

fn value_to_trimmed_string(value: &Value) -> Option<String> {
    match value {
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => None,
    }
}

fn parse_session_tags_value(value: &Value) -> Option<Vec<String>> {
    let mut tags = match value {
        Value::String(raw) => raw
            .split(',')
            .map(|tag| tag.trim().to_string())
            .collect::<Vec<String>>(),
        Value::Array(values) => values
            .iter()
            .filter_map(value_to_trimmed_string)
            .collect::<Vec<String>>(),
        _ => return None,
    };
    normalize_session_tags(&mut tags);
    if tags.is_empty() {
        None
    } else {
        Some(tags)
    }
}

fn parse_session_context(metadata_json: Option<&str>) -> Option<AdapterSessionContext> {
    let metadata_json = metadata_json?;
    let value: Value = serde_json::from_str(metadata_json).ok()?;
    let obj = value.as_object()?;
    let session_id = obj.get("session_id").and_then(value_to_trimmed_string)?;
    let session_name = obj.get("session_name").and_then(value_to_trimmed_string);
    let session_tags = obj.get("session_tags").and_then(parse_session_tags_value);

    Some(AdapterSessionContext {
        session_id,
        session_name,
        session_tags,
    })
}

/// Run an async operation with tenant-scope gating enabled for unscoped adapter queries.
pub async fn with_tenant_scope<F, Fut, T>(f: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    TENANT_SCOPE_ACTIVE.scope(true, f()).await
}

fn tenant_scope_active() -> bool {
    TENANT_SCOPE_ACTIVE.try_with(|flag| *flag).unwrap_or(false)
}

fn deny_unscoped_adapter_query(op: &str) -> Result<()> {
    if tenant_scope_active() {
        return Err(AosError::IsolationViolation(format!(
            "Unscoped adapter query blocked in tenant context: {op}"
        )));
    }
    Ok(())
}

/// Configuration for atomic dual-write behavior (SQL + KV)
#[derive(Debug, Clone, Default)]
pub struct AtomicDualWriteConfig {
    /// Require KV writes to succeed; if true, failures surface as errors
    /// and registration attempts to rollback SQL inserts.
    pub require_kv_success: bool,
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
    /// Defaults to strict mode unless explicitly disabled.
    pub fn from_env() -> Self {
        match env::var("AOS_ATOMIC_DUAL_WRITE_STRICT") {
            Ok(val) => match val.to_lowercase().as_str() {
                "0" | "false" | "no" => Self::best_effort(),
                "1" | "true" | "yes" => Self::strict_atomic(),
                _ => Self::strict_atomic(), // default to strict for unknown values
            },
            Err(_) => Self::strict_atomic(),
        }
    }
}

// AosRegistrationMetadata is now defined in and exported from aos_parser module

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
    lora_strength: Option<f32>,
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
    // MoE recommendation flag (0228)
    recommended_for_moe: Option<bool>,
    // Artifact hardening (from migration 0153)
    manifest_schema_version: Option<String>,
    content_hash_b3: Option<String>,
    provenance_json: Option<String>,
    metadata_json: Option<String>,
    // Scan root path (from migration 0243)
    repo_path: Option<String>,
    // Codebase adapter registration metadata (from migration 0231)
    codebase_scope: Option<String>,
    dataset_version_id: Option<String>,
    registration_timestamp: Option<String>,
    manifest_hash: Option<String>,
    // Codebase adapter type and stream binding (from migration 0261)
    adapter_type: Option<String>,
    base_adapter_id: Option<String>,
    stream_session_id: Option<String>,
    versioning_threshold: Option<i32>,
    coreml_package_hash: Option<String>,
    // Training dataset hash for lineage binding (from migration 0282)
    training_dataset_hash_b3: Option<String>,
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
    pub lora_strength: Option<f32>,
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
    // MoE recommendation flag (0228)
    pub recommended_for_moe: Option<bool>,
    // Artifact hardening (from migration 0153)
    pub manifest_schema_version: Option<String>,
    /// Content hash (BLAKE3 of manifest + weights) - required for deduplication
    pub content_hash_b3: String,
    pub provenance_json: Option<String>,
    pub metadata_json: Option<String>,
    // Scan root path (from migration 0243)
    pub repo_path: Option<String>,
    // Codebase adapter registration metadata (from migration 0231)
    /// Source repository/codebase reference for codebase adapters
    pub codebase_scope: Option<String>,
    /// Training dataset version ID for reproducibility
    pub dataset_version_id: Option<String>,
    /// ISO8601 timestamp when adapter was registered
    pub registration_timestamp: Option<String>,
    /// BLAKE3 hash of the adapter manifest for integrity verification
    pub manifest_hash: Option<String>,
    // Codebase adapter type and stream binding (from migration 0261)
    /// Adapter classification: "standard" (portable), "codebase" (stream-scoped), "core" (baseline)
    pub adapter_type: Option<String>,
    /// Base adapter ID for codebase adapters (the core adapter they extend as delta)
    pub base_adapter_id: Option<String>,
    /// Exclusive session binding for codebase adapters
    pub stream_session_id: Option<String>,
    /// Activation threshold for auto-versioning (default: 100)
    pub versioning_threshold: Option<i32>,
    /// BLAKE3 hash of fused CoreML package for deployment verification
    pub coreml_package_hash: Option<String>,
    /// BLAKE3 hash of training dataset content at training time.
    /// Used for receipt generation and lineage verification.
    /// (from migration 0282)
    pub training_dataset_hash_b3: Option<String>,
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

    /// Set the LoRA strength multiplier (0.0-1.0, optional)
    pub fn lora_strength(mut self, strength: Option<f32>) -> Self {
        self.lora_strength = strength;
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

    /// Set whether this adapter is recommended for MoE base models
    pub fn recommended_for_moe(mut self, recommended_for_moe: Option<bool>) -> Self {
        self.recommended_for_moe = recommended_for_moe;
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

    /// Set the content hash (optional; defaults to hash_b3 if omitted)
    /// BLAKE3 hash of manifest + weights for identity/deduplication
    pub fn content_hash_b3<T: Into<String>>(mut self, content_hash_b3: Option<T>) -> Self {
        self.content_hash_b3 = content_hash_b3.map(|s| s.into());
        self
    }

    /// Set the provenance JSON (optional, from migration 0153)
    /// Full training provenance embedded in the adapter
    pub fn provenance_json(mut self, provenance_json: Option<impl Into<String>>) -> Self {
        self.provenance_json = provenance_json.map(|s| s.into());
        self
    }

    /// Set arbitrary metadata JSON for adapter registration (optional)
    pub fn metadata_json(mut self, metadata_json: Option<impl Into<String>>) -> Self {
        self.metadata_json = metadata_json.map(|s| s.into());
        self
    }

    /// Apply .aos manifest/file metadata to the registration builder.
    ///
    /// Explicit values already set on the builder take precedence.
    pub fn with_aos_metadata(mut self, metadata: &AosRegistrationMetadata) -> Self {
        if self.aos_file_path.is_none() {
            self.aos_file_path = metadata.aos_file_path.clone();
        }
        if self.aos_file_hash.is_none() {
            self.aos_file_hash = metadata.aos_file_hash.clone();
        }
        if self.manifest_schema_version.is_none() {
            self.manifest_schema_version = metadata.manifest_schema_version.clone();
        }
        if self.content_hash_b3.is_none() {
            self.content_hash_b3 = metadata.content_hash_b3.clone();
        }
        if self.base_model_id.is_none() {
            self.base_model_id = metadata.base_model_id.clone();
        }
        if self.category.is_none() {
            self.category = metadata.category.clone();
        }
        if self.tier.is_none() {
            self.tier = metadata.tier.clone();
        }
        if self.metadata_json.is_none() {
            if let Some(ref manifest_metadata) = metadata.manifest_metadata {
                if let Ok(json) = serde_json::to_string(manifest_metadata) {
                    self.metadata_json = Some(json);
                }
            }
        }
        self
    }

    /// Set the repository scan root path (optional, from migration 0243)
    /// Canonicalized absolute path to the repository root used during code ingestion
    pub fn repo_path(mut self, repo_path: Option<impl Into<String>>) -> Self {
        self.repo_path = repo_path.map(|s| s.into());
        self
    }

    /// Set the codebase scope (optional, from migration 0231)
    /// Source repository/codebase reference for codebase adapters
    pub fn codebase_scope(mut self, codebase_scope: Option<impl Into<String>>) -> Self {
        self.codebase_scope = codebase_scope.map(|s| s.into());
        self
    }

    /// Set the dataset version ID (optional, from migration 0231)
    /// Training dataset version ID for reproducibility
    pub fn dataset_version_id(mut self, dataset_version_id: Option<impl Into<String>>) -> Self {
        self.dataset_version_id = dataset_version_id.map(|s| s.into());
        self
    }

    /// Set the registration timestamp (optional, from migration 0231)
    /// ISO8601 timestamp when adapter was registered
    pub fn registration_timestamp(
        mut self,
        registration_timestamp: Option<impl Into<String>>,
    ) -> Self {
        self.registration_timestamp = registration_timestamp.map(|s| s.into());
        self
    }

    /// Set the manifest hash (optional, from migration 0231)
    /// BLAKE3 hash of the adapter manifest for integrity verification
    pub fn manifest_hash(mut self, manifest_hash: Option<impl Into<String>>) -> Self {
        self.manifest_hash = manifest_hash.map(|s| s.into());
        self
    }

    /// Set the adapter type (optional, from migration 0261)
    /// Valid values: "standard", "codebase", "core"
    pub fn adapter_type(mut self, adapter_type: Option<impl Into<String>>) -> Self {
        self.adapter_type = adapter_type.map(|s| s.into());
        self
    }

    /// Set the base adapter ID for codebase adapters (optional, from migration 0261)
    /// Required for codebase adapters - the core adapter they extend as delta
    pub fn base_adapter_id(mut self, base_adapter_id: Option<impl Into<String>>) -> Self {
        self.base_adapter_id = base_adapter_id.map(|s| s.into());
        self
    }

    /// Set the stream session ID for exclusive binding (optional, from migration 0261)
    pub fn stream_session_id(mut self, stream_session_id: Option<impl Into<String>>) -> Self {
        self.stream_session_id = stream_session_id.map(|s| s.into());
        self
    }

    /// Set the versioning threshold for auto-versioning (optional, from migration 0261)
    /// Default: 100 activations
    pub fn versioning_threshold(mut self, versioning_threshold: Option<i32>) -> Self {
        self.versioning_threshold = versioning_threshold;
        self
    }

    /// Set the CoreML package hash for deployment verification (optional, from migration 0261)
    pub fn coreml_package_hash(mut self, coreml_package_hash: Option<impl Into<String>>) -> Self {
        self.coreml_package_hash = coreml_package_hash.map(|s| s.into());
        self
    }

    /// Set the training dataset hash for lineage binding (optional, from migration 0282)
    pub fn training_dataset_hash_b3(
        mut self,
        training_dataset_hash_b3: Option<impl Into<String>>,
    ) -> Self {
        self.training_dataset_hash_b3 = training_dataset_hash_b3.map(|s| s.into());
        self
    }

    /// Build the adapter registration parameters
    pub fn build(mut self) -> Result<AdapterRegistrationParams> {
        let rank = self
            .rank
            .ok_or_else(|| AosError::validation("rank is required"))?;

        // Validate and canonicalize .aos file path if provided
        let aos_file_path = match self.aos_file_path.as_ref() {
            Some(path_str) => {
                let path = Path::new(path_str);

                // Validate the file exists
                if !path.exists() {
                    return Err(AosError::validation(format!(
                        "aos_file_path does not exist: {}",
                        path_str
                    )));
                }

                // Validate it is a file, not a directory
                if !path.is_file() {
                    return Err(AosError::validation(format!(
                        "aos_file_path is not a file: {}",
                        path_str
                    )));
                }

                // Validate .aos extension
                match path.extension().and_then(|e| e.to_str()) {
                    Some("aos") => {}
                    _ => {
                        return Err(AosError::validation(format!(
                            "aos_file_path must have .aos extension: {}",
                            path_str
                        )));
                    }
                }

                // Canonicalize the path for consistency
                let canonical = path.canonicalize().map_err(|e| {
                    AosError::validation(format!(
                        "Failed to canonicalize aos_file_path {}: {}",
                        path_str, e
                    ))
                })?;

                Some(canonical.to_string_lossy().into_owned())
            }
            None => None,
        };

        if let Some(ref path_str) = aos_file_path {
            let needs_manifest_metadata = self.metadata_json.is_none()
                || self.base_model_id.is_none()
                || self.category.is_none()
                || self.tier.is_none()
                || self.manifest_schema_version.is_none()
                || self.content_hash_b3.is_none();
            if needs_manifest_metadata {
                if let Some(metadata) = load_aos_registration_metadata(Path::new(path_str)) {
                    self = self.with_aos_metadata(&metadata);
                }
            }
        }

        let mut aos_file_hash = self.aos_file_hash.clone();
        if let Some(ref path_str) = aos_file_path {
            let hash_missing = aos_file_hash
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .is_none();
            if hash_missing {
                let computed = compute_aos_file_hash(Path::new(path_str))?;
                aos_file_hash = Some(computed);
            }
        }

        // Validate and default tier
        let tier = self.tier.unwrap_or_else(|| "warm".to_string());
        if !["persistent", "warm", "ephemeral"].contains(&tier.as_str()) {
            return Err(AosError::validation(format!(
                "tier must be 'persistent', 'warm', or 'ephemeral', got: {}",
                tier
            )));
        }

        let hash_b3 = self
            .hash_b3
            .ok_or_else(|| AosError::validation("hash_b3 is required"))?;
        if hash_b3.is_empty() {
            return Err(AosError::validation("hash_b3 cannot be empty"));
        }

        let content_hash_b3 = match self.content_hash_b3 {
            Some(hash) => {
                if hash.is_empty() {
                    return Err(AosError::validation("content_hash_b3 cannot be empty"));
                }
                hash
            }
            None => hash_b3.clone(),
        };

        let provenance_json = self.provenance_json;
        let mut metadata_json = self.metadata_json;
        if metadata_json.is_none() {
            if let Some(ref provenance) = provenance_json {
                let is_object = serde_json::from_str::<Value>(provenance)
                    .ok()
                    .map(|val| val.is_object())
                    .unwrap_or(false);
                if is_object {
                    metadata_json = Some(provenance.clone());
                }
            }
        }

        let mut repo_id = self.repo_id.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        if repo_id.is_none() {
            repo_id = extract_repo_identifier_from_metadata(metadata_json.as_deref());
            if repo_id.is_none() {
                repo_id = extract_repo_identifier_from_metadata(provenance_json.as_deref());
            }
        }

        Ok(AdapterRegistrationParams {
            tenant_id: self
                .tenant_id
                .unwrap_or_else(|| "default-tenant".to_string()),
            adapter_id: self
                .adapter_id
                .ok_or_else(|| AosError::validation("adapter_id is required"))?,
            name: self
                .name
                .ok_or_else(|| AosError::validation("name is required"))?,
            hash_b3,
            rank,
            tier,
            alpha: self.alpha.unwrap_or_else(|| (rank * 2) as f64),
            lora_strength: Some(self.lora_strength.unwrap_or(1.0)),
            targets_json: self.targets_json.unwrap_or_else(|| "[]".to_string()),
            acl_json: self.acl_json,
            category: self.category.unwrap_or_else(|| "code".to_string()),
            scope: self.scope.unwrap_or_else(|| "global".to_string()),
            languages_json: self.languages_json,
            framework: self.framework,
            framework_id: self.framework_id,
            framework_version: self.framework_version,
            repo_id,
            commit_sha: self.commit_sha,
            intent: self.intent,
            expires_at: self.expires_at,
            aos_file_path,
            aos_file_hash,
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
            recommended_for_moe: self.recommended_for_moe,
            manifest_schema_version: self.manifest_schema_version,
            content_hash_b3,
            provenance_json,
            metadata_json,
            repo_path: self.repo_path,
            // Codebase adapter registration metadata
            codebase_scope: self.codebase_scope,
            dataset_version_id: self.dataset_version_id,
            registration_timestamp: self.registration_timestamp,
            manifest_hash: self.manifest_hash,
            // Codebase adapter type and stream binding
            adapter_type: self.adapter_type,
            base_adapter_id: self.base_adapter_id,
            stream_session_id: self.stream_session_id,
            versioning_threshold: self.versioning_threshold,
            coreml_package_hash: self.coreml_package_hash,
            training_dataset_hash_b3: self.training_dataset_hash_b3,
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
    pub lora_strength: Option<f32>, // LoRA strength multiplier [0.0,1.0]
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
    pub lifecycle_state: String, // draft/training/ready/active/deprecated/retired/failed

    // Archive/GC fields (from migration 0138)
    pub archived_at: Option<String>,    // When adapter was archived
    pub archived_by: Option<String>,    // User/system that initiated archive
    pub archive_reason: Option<String>, // Reason for archival (e.g., "tenant_archived")
    pub purged_at: Option<String>,      // When .aos file was deleted by GC

    // Base model reference (from migration 0098)
    pub base_model_id: Option<String>,
    #[sqlx(default)]
    pub recommended_for_moe: Option<bool>,
    // Artifact hardening (from migration 0153)
    pub manifest_schema_version: Option<String>,
    pub content_hash_b3: Option<String>,
    #[sqlx(default)]
    pub metadata_json: Option<String>,
    pub provenance_json: Option<String>,

    // Scan root path (from migration 0243)
    #[sqlx(default)]
    pub repo_path: Option<String>,

    // Drift tracking fields (for CLI diagnostics)
    #[sqlx(default)]
    pub drift_tier: Option<String>,
    #[sqlx(default)]
    pub drift_metric: Option<f64>,
    #[sqlx(default)]
    pub drift_loss_metric: Option<f64>,
    #[sqlx(default)]
    pub drift_reference_backend: Option<String>,
    #[sqlx(default)]
    pub drift_baseline_backend: Option<String>,
    #[sqlx(default)]
    pub drift_test_backend: Option<String>,

    // Codebase adapter registration metadata (from migration 0231)
    /// Source repository/codebase reference for codebase adapters
    #[sqlx(default)]
    pub codebase_scope: Option<String>,
    /// Training dataset version ID for reproducibility
    #[sqlx(default)]
    pub dataset_version_id: Option<String>,
    /// ISO8601 timestamp when adapter was registered
    #[sqlx(default)]
    pub registration_timestamp: Option<String>,
    /// BLAKE3 hash of the adapter manifest for integrity verification
    #[sqlx(default)]
    pub manifest_hash: Option<String>,

    // Codebase adapter type and stream binding (from migration 0261)
    /// Adapter classification: "standard" (portable), "codebase" (stream-scoped), "core" (baseline)
    #[sqlx(default)]
    pub adapter_type: Option<String>,
    /// Base adapter ID for codebase adapters (the core adapter they extend as delta)
    /// Distinct from parent_id which tracks version lineage
    #[sqlx(default)]
    pub base_adapter_id: Option<String>,
    /// Exclusive session binding for codebase adapters
    #[sqlx(default)]
    pub stream_session_id: Option<String>,
    /// Activation threshold for auto-versioning (default: 100)
    #[sqlx(default)]
    pub versioning_threshold: Option<i32>,
    /// BLAKE3 hash of fused CoreML package for deployment verification
    #[sqlx(default)]
    pub coreml_package_hash: Option<String>,
    /// BLAKE3 hash of training dataset content at training time.
    /// Used for receipt generation and lineage verification.
    /// (from migration 0282)
    #[sqlx(default)]
    pub training_dataset_hash_b3: Option<String>,

    /// Linked adapter version ID when this adapter maps to a unique repository version.
    /// Best-effort mapping by (tenant_id, repo_id, version).
    #[sqlx(default)]
    pub adapter_version_id: Option<String>,
    /// Effective canary multiplier from the linked adapter version.
    /// Neutral default is 1.0.
    #[sqlx(default)]
    pub effective_version_weight: Option<f64>,

    /// Monotonic stable ID per tenant for deterministic tie-breaking.
    /// Used by the router for consistent ordering when scores are equal.
    /// (from migration 0300)
    #[sqlx(default)]
    pub stable_id: Option<i64>,

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

/// Adapter file metadata for .aos files
///
/// This struct stores extended metadata about adapter archive files,
/// including file size, modification timestamps, segment counts, and
/// integrity verification data. Used for cache management, staleness
/// detection, and disk usage tracking.
///
/// # Database Table
///
/// Persisted in the `aos_adapter_metadata` table (migration 0045 + 0243).
///
/// # Example
///
/// ```ignore
/// use adapteros_db::AdapterFileMetadata;
///
/// let metadata = AdapterFileMetadata {
///     adapter_id: "my-adapter-id".to_string(),
///     aos_file_path: "/var/adapters/my-adapter.aos".to_string(),
///     aos_file_hash: "b3:abc123...".to_string(),
///     file_size_bytes: Some(1024 * 1024 * 50), // 50 MB
///     file_modified_at: Some("2024-01-15T10:30:00Z".to_string()),
///     segment_count: Some(3),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default, sqlx::FromRow)]
pub struct AdapterFileMetadata {
    /// Adapter ID (links to adapters table)
    pub adapter_id: String,
    /// Absolute path to the .aos file
    pub aos_file_path: String,
    /// BLAKE3 hash of the .aos file for integrity verification
    pub aos_file_hash: String,
    /// Path to extracted weights (if applicable)
    #[sqlx(default)]
    pub extracted_weights_path: Option<String>,
    /// Number of training examples used
    #[sqlx(default)]
    pub training_data_count: Option<i64>,
    /// Lineage version string for tracking
    #[sqlx(default)]
    pub lineage_version: Option<String>,
    /// Whether cryptographic signature is valid
    #[sqlx(default)]
    pub signature_valid: Option<bool>,
    /// File size in bytes
    #[sqlx(default)]
    pub file_size_bytes: Option<i64>,
    /// File modification timestamp (ISO 8601)
    #[sqlx(default)]
    pub file_modified_at: Option<String>,
    /// Number of segments in the .aos file
    #[sqlx(default)]
    pub segment_count: Option<i64>,
    /// Manifest schema version
    #[sqlx(default)]
    pub manifest_schema_version: Option<String>,
    /// Base model identifier
    #[sqlx(default)]
    pub base_model: Option<String>,
    /// Adapter category
    #[sqlx(default)]
    pub category: Option<String>,
    /// Adapter tier (ephemeral, warm, persistent)
    #[sqlx(default)]
    pub tier: Option<String>,
    /// Creation timestamp
    pub created_at: String,
    /// Last update timestamp
    pub updated_at: String,
}

/// Parameters for storing adapter file metadata
#[derive(Debug, Clone, Default)]
pub struct StoreAdapterFileMetadataParams {
    /// Adapter ID (required)
    pub adapter_id: String,
    /// Absolute path to the .aos file (required)
    pub aos_file_path: String,
    /// BLAKE3 hash of the .aos file (required)
    pub aos_file_hash: String,
    /// Path to extracted weights
    pub extracted_weights_path: Option<String>,
    /// Number of training examples
    pub training_data_count: Option<i64>,
    /// Lineage version
    pub lineage_version: Option<String>,
    /// Signature validity
    pub signature_valid: Option<bool>,
    /// File size in bytes
    pub file_size_bytes: Option<i64>,
    /// File modification timestamp (ISO 8601)
    pub file_modified_at: Option<String>,
    /// Number of segments in the .aos file
    pub segment_count: Option<i64>,
    /// Manifest schema version
    pub manifest_schema_version: Option<String>,
    /// Base model identifier
    pub base_model: Option<String>,
    /// Adapter category
    pub category: Option<String>,
    /// Adapter tier
    pub tier: Option<String>,
}

impl StoreAdapterFileMetadataParams {
    /// Create new parameters with required fields
    pub fn new(
        adapter_id: impl Into<String>,
        aos_file_path: impl Into<String>,
        aos_file_hash: impl Into<String>,
    ) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            aos_file_path: aos_file_path.into(),
            aos_file_hash: aos_file_hash.into(),
            ..Default::default()
        }
    }

    /// Set the file size in bytes
    pub fn file_size_bytes(mut self, size: i64) -> Self {
        self.file_size_bytes = Some(size);
        self
    }

    /// Set the file modification timestamp
    pub fn file_modified_at(mut self, timestamp: impl Into<String>) -> Self {
        self.file_modified_at = Some(timestamp.into());
        self
    }

    /// Set the segment count
    pub fn segment_count(mut self, count: i64) -> Self {
        self.segment_count = Some(count);
        self
    }

    /// Set the extracted weights path
    pub fn extracted_weights_path(mut self, path: impl Into<String>) -> Self {
        self.extracted_weights_path = Some(path.into());
        self
    }

    /// Set the training data count
    pub fn training_data_count(mut self, count: i64) -> Self {
        self.training_data_count = Some(count);
        self
    }

    /// Set the lineage version
    pub fn lineage_version(mut self, version: impl Into<String>) -> Self {
        self.lineage_version = Some(version.into());
        self
    }

    /// Set signature validity
    pub fn signature_valid(mut self, valid: bool) -> Self {
        self.signature_valid = Some(valid);
        self
    }

    /// Set the manifest schema version
    pub fn manifest_schema_version(mut self, version: impl Into<String>) -> Self {
        self.manifest_schema_version = Some(version.into());
        self
    }

    /// Set the base model identifier
    pub fn base_model(mut self, model: impl Into<String>) -> Self {
        self.base_model = Some(model.into());
        self
    }

    /// Set the adapter category
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Set the adapter tier
    pub fn tier(mut self, tier: impl Into<String>) -> Self {
        self.tier = Some(tier.into());
        self
    }
}

/// Parameters for updating .aos manifest metadata
///
/// This struct holds the metadata extracted from an .aos file's manifest
/// that should be persisted to the adapter record. The metadata is stored
/// as JSON in the `metadata_json` field.
///
/// # Example
///
/// ```ignore
/// use adapteros_db::AosMetadataUpdate;
/// use std::collections::HashMap;
///
/// let mut manifest_meta = HashMap::new();
/// manifest_meta.insert("scope_path".to_string(), "domain/group/scope/op".to_string());
/// manifest_meta.insert("training_backend".to_string(), "mlx".to_string());
///
/// let update = AosMetadataUpdate {
///     adapter_id: "my-adapter".to_string(),
///     tenant_id: "tenant-123".to_string(),
///     aos_file_path: Some("/path/to/adapter.aos".to_string()),
///     aos_file_hash: Some("b3:abc123...".to_string()),
///     manifest_metadata: Some(manifest_meta),
///     base_model_id: Some("qwen2.5-7b".to_string()),
///     manifest_schema_version: Some("1.0.0".to_string()),
///     content_hash_b3: Some("b3:weights...".to_string()),
///     provenance_json: None,
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct AosMetadataUpdate {
    /// Adapter ID (required)
    pub adapter_id: String,
    /// Tenant ID (required for dual-write)
    pub tenant_id: String,
    /// Path to the .aos file
    pub aos_file_path: Option<String>,
    /// BLAKE3 hash of the .aos file
    pub aos_file_hash: Option<String>,
    /// Metadata from the .aos manifest (stored as metadata_json)
    pub manifest_metadata: Option<std::collections::HashMap<String, String>>,
    /// Base model identifier from manifest
    pub base_model_id: Option<String>,
    /// Manifest schema version
    pub manifest_schema_version: Option<String>,
    /// Content hash (BLAKE3 of manifest + weights)
    pub content_hash_b3: Option<String>,
    /// Full training provenance JSON
    pub provenance_json: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AdapterMetadataPatch {
    pub aos_file_path: Option<String>,
    pub aos_file_hash: Option<String>,
    pub base_model_id: Option<String>,
    pub manifest_schema_version: Option<String>,
    pub content_hash_b3: Option<String>,
    pub metadata_json: Option<String>,
    pub provenance_json: Option<String>,
    pub repo_path: Option<String>,
    pub codebase_scope: Option<String>,
    pub dataset_version_id: Option<String>,
    pub registration_timestamp: Option<String>,
    pub manifest_hash: Option<String>,
    // Codebase adapter type and stream binding (from migration 0261)
    pub adapter_type: Option<String>,
    pub base_adapter_id: Option<String>,
    pub stream_session_id: Option<String>,
    pub versioning_threshold: Option<i32>,
    pub coreml_package_hash: Option<String>,
}

/// Patch payload for updating semantic alias fields.
#[derive(Debug, Clone, Default)]
pub struct AdapterAliasUpdate {
    pub adapter_name: Option<String>,
    pub tenant_namespace: Option<String>,
    pub domain: Option<String>,
    pub purpose: Option<String>,
    pub revision: Option<String>,
}

impl AdapterAliasUpdate {
    fn from_alias(alias: Option<&str>) -> Result<Self> {
        let trimmed = alias.map(str::trim).filter(|value| !value.is_empty());
        if let Some(alias) = trimmed {
            let parsed = AdapterName::parse(alias)?;
            return Ok(Self {
                adapter_name: Some(parsed.to_string()),
                tenant_namespace: Some(parsed.tenant().to_string()),
                domain: Some(parsed.domain().to_string()),
                purpose: Some(parsed.purpose().to_string()),
                revision: Some(parsed.revision().to_string()),
            });
        }

        Ok(Self::default())
    }

    fn matches_adapter(&self, adapter: &Adapter) -> bool {
        adapter.adapter_name == self.adapter_name
            && adapter.tenant_namespace == self.tenant_namespace
            && adapter.domain == self.domain
            && adapter.purpose == self.purpose
            && adapter.revision == self.revision
    }
}

/// Configuration for alias update gating behavior.
#[derive(Debug, Clone, Default)]
pub struct AliasUpdateGateConfig {
    /// Allow alias updates for Ready state when true.
    pub allow_ready: bool,
}

impl AdapterMetadataPatch {
    fn from_params(params: &AdapterRegistrationParams) -> Self {
        fn sanitize(value: Option<&str>) -> Option<String> {
            value
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| v.to_string())
        }

        Self {
            aos_file_path: sanitize(params.aos_file_path.as_deref()),
            aos_file_hash: sanitize(params.aos_file_hash.as_deref()),
            base_model_id: sanitize(params.base_model_id.as_deref()),
            manifest_schema_version: sanitize(params.manifest_schema_version.as_deref()),
            content_hash_b3: sanitize(Some(params.content_hash_b3.as_str())),
            metadata_json: sanitize(params.metadata_json.as_deref()),
            provenance_json: sanitize(params.provenance_json.as_deref()),
            repo_path: sanitize(params.repo_path.as_deref()),
            codebase_scope: sanitize(params.codebase_scope.as_deref()),
            dataset_version_id: sanitize(params.dataset_version_id.as_deref()),
            registration_timestamp: sanitize(params.registration_timestamp.as_deref()),
            manifest_hash: sanitize(params.manifest_hash.as_deref()),
            // Codebase adapter type and stream binding
            adapter_type: sanitize(params.adapter_type.as_deref()),
            base_adapter_id: sanitize(params.base_adapter_id.as_deref()),
            stream_session_id: sanitize(params.stream_session_id.as_deref()),
            versioning_threshold: params.versioning_threshold,
            coreml_package_hash: sanitize(params.coreml_package_hash.as_deref()),
        }
    }

    pub(crate) fn has_updates(&self) -> bool {
        self.aos_file_path.is_some()
            || self.aos_file_hash.is_some()
            || self.base_model_id.is_some()
            || self.manifest_schema_version.is_some()
            || self.content_hash_b3.is_some()
            || self.metadata_json.is_some()
            || self.provenance_json.is_some()
            || self.repo_path.is_some()
            || self.codebase_scope.is_some()
            || self.dataset_version_id.is_some()
            || self.registration_timestamp.is_some()
            || self.manifest_hash.is_some()
            || self.adapter_type.is_some()
            || self.base_adapter_id.is_some()
            || self.stream_session_id.is_some()
            || self.versioning_threshold.is_some()
            || self.coreml_package_hash.is_some()
    }
}

impl AosMetadataUpdate {
    /// Create a new AosMetadataUpdate with required fields
    pub fn new(adapter_id: impl Into<String>, tenant_id: impl Into<String>) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            tenant_id: tenant_id.into(),
            ..Default::default()
        }
    }

    /// Set the .aos file path
    pub fn aos_file_path(mut self, path: impl Into<String>) -> Self {
        self.aos_file_path = Some(path.into());
        self
    }

    /// Set the .aos file hash
    pub fn aos_file_hash(mut self, hash: impl Into<String>) -> Self {
        self.aos_file_hash = Some(hash.into());
        self
    }

    /// Set the manifest metadata
    pub fn manifest_metadata(
        mut self,
        metadata: std::collections::HashMap<String, String>,
    ) -> Self {
        self.manifest_metadata = Some(metadata);
        self
    }

    /// Set the base model identifier
    pub fn base_model_id(mut self, model: impl Into<String>) -> Self {
        self.base_model_id = Some(model.into());
        self
    }

    /// Set the manifest schema version
    pub fn manifest_schema_version(mut self, version: impl Into<String>) -> Self {
        self.manifest_schema_version = Some(version.into());
        self
    }

    /// Set the content hash
    pub fn content_hash_b3(mut self, hash: impl Into<String>) -> Self {
        self.content_hash_b3 = Some(hash.into());
        self
    }

    /// Set the provenance JSON
    pub fn provenance_json(mut self, json: impl Into<String>) -> Self {
        self.provenance_json = Some(json.into());
        self
    }
}

// ============================================================================
// AOS Metadata Validation
// ============================================================================

/// Validation result for .aos metadata
#[derive(Debug, Clone)]
pub struct AosMetadataValidation {
    /// Whether the validation passed
    pub is_valid: bool,
    /// List of validation errors (if any)
    pub errors: Vec<String>,
    /// List of validation warnings (non-fatal issues)
    pub warnings: Vec<String>,
}

impl AosMetadataValidation {
    /// Create a successful validation result
    pub fn valid() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Create a validation result with errors
    pub fn invalid(errors: Vec<String>) -> Self {
        Self {
            is_valid: false,
            errors,
            warnings: Vec::new(),
        }
    }

    /// Add a warning to the validation result
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }
}

/// Validate .aos file metadata format and content
///
/// This function validates the metadata associated with an .aos adapter file,
/// checking for required fields, format correctness, and logical consistency.
///
/// # Validation Rules
///
/// 1. **Required Fields**: `adapter_id`, `aos_file_path`, and `aos_file_hash` must be present
/// 2. **Path Format**: `aos_file_path` must be an absolute path with `.aos` extension
/// 3. **Hash Format**: `aos_file_hash` must be a valid BLAKE3 hash (64 hex characters)
/// 4. **File Size**: If provided, must be non-negative
/// 5. **Segment Count**: If provided, must be positive
/// 6. **Schema Version**: If provided, must be valid semver format
pub fn validate_aos_metadata(params: &StoreAdapterFileMetadataParams) -> AosMetadataValidation {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Validate required fields
    if params.adapter_id.is_empty() {
        errors.push("adapter_id is required".to_string());
    }

    if params.aos_file_path.is_empty() {
        errors.push("aos_file_path is required".to_string());
    } else {
        let path = Path::new(&params.aos_file_path);
        if !path.is_absolute() {
            errors.push(format!(
                "aos_file_path must be an absolute path: {}",
                params.aos_file_path
            ));
        }
        match path.extension().and_then(|e| e.to_str()) {
            Some("aos") => {}
            Some(ext) => {
                errors.push(format!(
                    "aos_file_path must have .aos extension, got .{}: {}",
                    ext, params.aos_file_path
                ));
            }
            None => {
                errors.push(format!(
                    "aos_file_path must have .aos extension: {}",
                    params.aos_file_path
                ));
            }
        }
    }

    if params.aos_file_hash.is_empty() {
        errors.push("aos_file_hash is required".to_string());
    } else {
        let hash = params
            .aos_file_hash
            .strip_prefix("b3:")
            .unwrap_or(&params.aos_file_hash);
        if hash.len() != 64 {
            errors.push(format!(
                "aos_file_hash must be 64 hex characters (BLAKE3), got {} characters",
                hash.len()
            ));
        } else if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            errors.push("aos_file_hash must contain only hexadecimal characters".to_string());
        }
    }

    if let Some(file_size) = params.file_size_bytes {
        if file_size < 0 {
            errors.push(format!("file_size_bytes cannot be negative: {}", file_size));
        } else if file_size == 0 {
            warnings.push("file_size_bytes is 0, which may indicate an empty file".to_string());
        }
    }

    if let Some(segment_count) = params.segment_count {
        if segment_count <= 0 {
            errors.push(format!(
                "segment_count must be positive, got: {}",
                segment_count
            ));
        }
    }

    if let Some(ref version) = params.manifest_schema_version {
        if !is_valid_semver(version) {
            errors.push(format!(
                "manifest_schema_version must be valid semver (e.g., '1.0.0'), got: {}",
                version
            ));
        }
    }

    if let Some(ref tier) = params.tier {
        if !["persistent", "warm", "ephemeral"].contains(&tier.as_str()) {
            errors.push(format!(
                "tier must be 'persistent', 'warm', or 'ephemeral', got: {}",
                tier
            ));
        }
    }

    if let Some(ref category) = params.category {
        let valid_categories = [
            "code",
            "framework",
            "codebase",
            "ephemeral",
            "docs",
            "documentation",
            "domain",
            "domain-adapter",
            "creative",
            "conversation",
            "analysis",
        ];
        if !valid_categories.contains(&category.as_str()) {
            warnings.push(format!(
                "category '{}' is non-standard (expected one of: {})",
                category,
                valid_categories.join(", ")
            ));
        }
    }

    if errors.is_empty() {
        let mut result = AosMetadataValidation::valid();
        result.warnings = warnings;
        result
    } else {
        let mut result = AosMetadataValidation::invalid(errors);
        result.warnings = warnings;
        result
    }
}

/// Check if a string is valid semver format
fn is_valid_semver(version: &str) -> bool {
    let parts: Vec<&str> = version.split('-').collect();
    let version_core = parts.first().unwrap_or(&"");
    let numbers: Vec<&str> = version_core.split('.').collect();
    if numbers.len() < 2 || numbers.len() > 3 {
        return false;
    }
    numbers.iter().all(|n| n.parse::<u32>().is_ok())
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

    /// Get tenant_id for an adapter by adapter_id (external ID) with tenant verification
    ///
    /// # Security
    /// This method requires a `requesting_tenant_id` to prevent cross-tenant information
    /// disclosure. The adapter's tenant_id is only returned if it matches the requesting
    /// tenant, enforcing tenant isolation.
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter's external ID
    /// * `requesting_tenant_id` - The tenant context making the request (for verification)
    ///
    /// # Returns
    /// * `Ok(Some(tenant_id))` - If adapter exists AND belongs to the requesting tenant
    /// * `Ok(None)` - If adapter doesn't exist OR belongs to a different tenant
    pub(crate) async fn get_adapter_tenant_id(
        &self,
        adapter_id: &str,
        requesting_tenant_id: &str,
    ) -> Result<Option<String>> {
        // SECURITY: Filter by requesting_tenant_id to prevent cross-tenant lookups
        let tenant_id: Option<String> = sqlx::query_scalar(
            "SELECT tenant_id FROM adapters WHERE adapter_id = ? AND tenant_id = ?",
        )
        .bind(adapter_id)
        .bind(requesting_tenant_id)
        .fetch_optional(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;
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
            .map_err(|e| AosError::database(format!("Failed to query adapter index: {}", e)))?;

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
            .map_err(|e| AosError::database(format!("Failed to get adapter: {}", e)))?
        {
            Some(b) => b,
            None => return Ok(None),
        };

        // Deserialize and convert to Adapter
        let adapter_kv: adapteros_storage::AdapterKv = bincode::deserialize(&bytes)
            .map_err(|e| AosError::database(format!("Failed to deserialize adapter: {}", e)))?;

        Ok(Some(adapter_kv.into()))
    }

    // =========================================================================
    // AOS File Metadata Storage Operations
    // =========================================================================

    /// Store .aos file metadata for an adapter
    ///
    /// Stores extended metadata about an .aos adapter file in the `aos_adapter_metadata` table.
    /// This metadata is used for cache management, staleness detection, and integrity verification.
    ///
    /// # Validation
    ///
    /// This method validates the metadata before storing. Invalid metadata will result in an error.
    /// Use [`validate_aos_metadata`] to pre-validate metadata if needed.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use adapteros_db::{Db, StoreAdapterFileMetadataParams};
    ///
    /// let params = StoreAdapterFileMetadataParams::new(
    ///     "adapter-123",
    ///     "/var/adapters/my-adapter.aos",
    ///     "b3:abc123..."
    /// )
    /// .file_size_bytes(1024 * 1024 * 50)
    /// .segment_count(3)
    /// .manifest_schema_version("1.0.0");
    ///
    /// db.store_adapter_file_metadata(params).await?;
    /// ```
    pub async fn store_adapter_file_metadata(
        &self,
        params: StoreAdapterFileMetadataParams,
    ) -> Result<()> {
        // Validate metadata before storing
        let validation = validate_aos_metadata(&params);
        if !validation.is_valid {
            return Err(AosError::validation(format!(
                "Invalid .aos metadata: {}",
                validation.errors.join("; ")
            )));
        }

        // Log warnings but continue
        for warning in &validation.warnings {
            warn!(adapter_id = %params.adapter_id, warning = %warning, "AOS metadata warning");
        }

        // Insert or update (upsert) the metadata
        sqlx::query(
            "INSERT INTO aos_adapter_metadata (
                adapter_id, aos_file_path, aos_file_hash, extracted_weights_path,
                training_data_count, lineage_version, signature_valid, file_size_bytes,
                file_modified_at, segment_count, manifest_schema_version, base_model,
                category, tier, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
            ON CONFLICT(adapter_id) DO UPDATE SET
                aos_file_path = excluded.aos_file_path,
                aos_file_hash = excluded.aos_file_hash,
                extracted_weights_path = excluded.extracted_weights_path,
                training_data_count = excluded.training_data_count,
                lineage_version = excluded.lineage_version,
                signature_valid = excluded.signature_valid,
                file_size_bytes = excluded.file_size_bytes,
                file_modified_at = excluded.file_modified_at,
                segment_count = excluded.segment_count,
                manifest_schema_version = excluded.manifest_schema_version,
                base_model = excluded.base_model,
                category = excluded.category,
                tier = excluded.tier,
                updated_at = datetime('now')",
        )
        .bind(&params.adapter_id)
        .bind(&params.aos_file_path)
        .bind(&params.aos_file_hash)
        .bind(&params.extracted_weights_path)
        .bind(params.training_data_count)
        .bind(&params.lineage_version)
        .bind(params.signature_valid)
        .bind(params.file_size_bytes)
        .bind(&params.file_modified_at)
        .bind(params.segment_count)
        .bind(&params.manifest_schema_version)
        .bind(&params.base_model)
        .bind(&params.category)
        .bind(&params.tier)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(format!("Failed to store .aos metadata: {}", e)))?;

        info!(
            adapter_id = %params.adapter_id,
            aos_file_path = %params.aos_file_path,
            "Stored .aos file metadata"
        );

        Ok(())
    }

    /// Retrieve .aos file metadata for an adapter
    ///
    /// Returns the extended metadata for an .aos adapter file from the `aos_adapter_metadata` table.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(metadata))` if metadata exists
    /// - `Ok(None)` if no metadata exists for the adapter
    /// - `Err(...)` on database error
    ///
    /// # Example
    ///
    /// ```ignore
    /// use adapteros_db::Db;
    ///
    /// if let Some(metadata) = db.get_adapter_file_metadata("adapter-123").await? {
    ///     println!("File size: {:?}", metadata.file_size_bytes);
    ///     println!("Segment count: {:?}", metadata.segment_count);
    /// }
    /// ```
    pub async fn get_adapter_file_metadata(
        &self,
        adapter_id: &str,
    ) -> Result<Option<AdapterFileMetadata>> {
        let metadata = sqlx::query_as::<_, AdapterFileMetadata>(
            "SELECT adapter_id, aos_file_path, aos_file_hash, extracted_weights_path,
                    training_data_count, lineage_version, signature_valid, file_size_bytes,
                    file_modified_at, segment_count, manifest_schema_version, base_model,
                    category, tier, created_at, updated_at
             FROM aos_adapter_metadata
             WHERE adapter_id = ?",
        )
        .bind(adapter_id)
        .fetch_optional(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(format!("Failed to get .aos metadata: {}", e)))?;

        Ok(metadata)
    }

    /// Delete .aos file metadata for an adapter
    ///
    /// Removes the extended metadata entry from the `aos_adapter_metadata` table.
    /// This is typically called during adapter purging or cleanup.
    ///
    /// # Returns
    ///
    /// `true` if metadata was deleted, `false` if no metadata existed
    pub async fn delete_adapter_file_metadata(&self, adapter_id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM aos_adapter_metadata WHERE adapter_id = ?")
            .bind(adapter_id)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| AosError::database(format!("Failed to delete .aos metadata: {}", e)))?;

        let deleted = result.rows_affected() > 0;
        if deleted {
            debug!(adapter_id = %adapter_id, "Deleted .aos file metadata");
        }

        Ok(deleted)
    }

    /// Update adapter metadata based on a parsed .aos manifest.
    ///
    /// Merges manifest metadata into the existing metadata_json and updates
    /// artifact fields (aos path/hash, base model, schema version, content hash).
    pub async fn update_adapter_aos_metadata(&self, update: AosMetadataUpdate) -> Result<()> {
        let adapter_id = update.adapter_id.trim();
        let tenant_id = update.tenant_id.trim();
        if adapter_id.is_empty() {
            return Err(AosError::Validation("adapter_id is required".to_string()));
        }
        if tenant_id.is_empty() {
            return Err(AosError::Validation("tenant_id is required".to_string()));
        }

        let adapter = self
            .get_adapter_for_tenant(tenant_id, adapter_id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        let manifest_metadata = update
            .manifest_metadata
            .as_ref()
            .filter(|meta| !meta.is_empty());
        let merged_metadata_json = if let Some(manifest_metadata) = manifest_metadata {
            let mut metadata_map = match adapter.metadata_json.as_deref() {
                Some(raw) => match serde_json::from_str::<Value>(raw) {
                    Ok(Value::Object(map)) => map,
                    Ok(_) => {
                        warn!(
                            adapter_id = %adapter_id,
                            "Existing metadata_json is not an object; replacing with manifest metadata"
                        );
                        serde_json::Map::new()
                    }
                    Err(err) => {
                        warn!(
                            adapter_id = %adapter_id,
                            error = %err,
                            "Failed to parse metadata_json; replacing with manifest metadata"
                        );
                        serde_json::Map::new()
                    }
                },
                None => serde_json::Map::new(),
            };

            for (key, value) in manifest_metadata {
                metadata_map.insert(key.clone(), Value::String(value.clone()));
            }

            Some(
                serde_json::to_string(&Value::Object(metadata_map))
                    .map_err(AosError::Serialization)?,
            )
        } else {
            None
        };

        let aos_file_path = update
            .aos_file_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                Path::new(value)
                    .canonicalize()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| value.to_string())
            });
        let aos_file_hash = update
            .aos_file_hash
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        let provenance_json = update
            .provenance_json
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        let base_model_id = update
            .base_model_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        let manifest_schema_version = update
            .manifest_schema_version
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        let content_hash_b3 = update
            .content_hash_b3
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());

        if self.storage_mode().write_to_sql() {
            let result = sqlx::query(
                "UPDATE adapters SET
                    aos_file_path = COALESCE(?, aos_file_path),
                    aos_file_hash = COALESCE(?, aos_file_hash),
                    metadata_json = COALESCE(?, metadata_json),
                    provenance_json = COALESCE(?, provenance_json),
                    base_model_id = COALESCE(?, base_model_id),
                    manifest_schema_version = COALESCE(?, manifest_schema_version),
                    content_hash_b3 = COALESCE(?, content_hash_b3),
                    updated_at = datetime('now')
                 WHERE tenant_id = ? AND adapter_id = ?",
            )
            .bind(&aos_file_path)
            .bind(&aos_file_hash)
            .bind(&merged_metadata_json)
            .bind(&provenance_json)
            .bind(&base_model_id)
            .bind(&manifest_schema_version)
            .bind(&content_hash_b3)
            .bind(tenant_id)
            .bind(adapter_id)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update .aos metadata: {}", e)))?;

            if result.rows_affected() == 0 {
                return Err(AosError::NotFound(format!(
                    "Adapter not found: {}",
                    adapter_id
                )));
            }
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for update_adapter_aos_metadata".to_string(),
            ));
        }

        if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
            if let Err(e) = repo
                .update_adapter_aos_metadata_kv(
                    adapter_id,
                    aos_file_path.as_deref(),
                    aos_file_hash.as_deref(),
                    merged_metadata_json.as_deref(),
                    provenance_json.as_deref(),
                    base_model_id.as_deref(),
                    manifest_schema_version.as_deref(),
                    content_hash_b3.as_deref(),
                )
                .await
            {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL metadata update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                    );
                    return Err(AosError::Database(format!(
                        "Metadata update succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write",
                        "Failed to update adapter metadata in KV backend"
                    );
                }
            } else {
                debug!(
                    adapter_id = %adapter_id,
                    tenant_id = %tenant_id,
                    mode = "dual-write",
                    "Adapter metadata updated in both SQL and KV backends"
                );
            }
        }

        Ok(())
    }

    /// Update adapter hash fields (content_hash_b3 and manifest_hash)
    ///
    /// Used by hash repair commands to populate missing hashes on legacy adapters.
    /// Only updates fields if the provided value is Some and non-empty.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant context (required for security isolation)
    /// * `adapter_id` - The adapter's internal ID (not adapter_id field)
    /// * `content_hash_b3` - Optional new content hash
    /// * `manifest_hash` - Optional new manifest hash
    pub async fn update_adapter_hashes(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        content_hash_b3: Option<&str>,
        manifest_hash: Option<&str>,
    ) -> Result<()> {
        // Only update non-empty values
        let content_hash = content_hash_b3.filter(|h| !h.trim().is_empty());
        let manifest = manifest_hash.filter(|h| !h.trim().is_empty());

        if content_hash.is_none() && manifest.is_none() {
            return Ok(()); // Nothing to update
        }

        let affected = sqlx::query(
            "UPDATE adapters
             SET content_hash_b3 = COALESCE(?, content_hash_b3),
                 manifest_hash = COALESCE(?, manifest_hash),
                 updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(content_hash)
        .bind(manifest)
        .bind(adapter_id)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(format!("Failed to update adapter hashes: {}", e)))?
        .rows_affected();

        if affected == 0 {
            return Err(AosError::NotFound(format!(
                "Adapter not found: {}",
                adapter_id
            )));
        }

        info!(
            adapter_id = %adapter_id,
            content_hash = ?content_hash,
            manifest_hash = ?manifest,
            code = "ADAPTER_HASHES_UPDATED",
            "Updated adapter hashes"
        );

        // KV dual-write (tenant_id already verified via parameter)
        if let Some(verified_tenant) = self.get_adapter_tenant_id(adapter_id, tenant_id).await? {
            if let Some(repo) = self.get_adapter_kv_repo(&verified_tenant) {
                let mut patch = AdapterMetadataPatch::default();
                if let Some(hash) = content_hash {
                    patch.content_hash_b3 = Some(hash.to_string());
                }
                if let Some(hash) = manifest {
                    patch.manifest_hash = Some(hash.to_string());
                }

                if patch.has_updates() {
                    if let Err(e) = repo.update_adapter_metadata_kv(adapter_id, &patch).await {
                        warn!(
                            error = %e,
                            adapter_id = %adapter_id,
                            tenant_id = %verified_tenant,
                            mode = "dual-write",
                            "Failed to update adapter hashes in KV backend"
                        );
                    } else {
                        debug!(
                            adapter_id = %adapter_id,
                            tenant_id = %tenant_id,
                            mode = "dual-write",
                            "Adapter hashes updated in both SQL and KV backends"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Update semantic adapter alias fields with lifecycle gating.
    pub async fn update_adapter_alias(&self, adapter_id: &str, alias: Option<&str>) -> Result<()> {
        self.update_adapter_alias_with_gate(adapter_id, alias, &AliasUpdateGateConfig::default())
            .await
    }

    /// Update semantic adapter alias fields with custom gating configuration.
    pub async fn update_adapter_alias_with_gate(
        &self,
        adapter_id: &str,
        alias: Option<&str>,
        gate: &AliasUpdateGateConfig,
    ) -> Result<()> {
        #[allow(deprecated)]
        let adapter = self
            .get_adapter(adapter_id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        self.update_adapter_alias_inner(adapter, adapter_id, alias, gate)
            .await
    }

    /// Tenant-scoped alias update with lifecycle gating.
    pub async fn update_adapter_alias_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        alias: Option<&str>,
    ) -> Result<()> {
        self.update_adapter_alias_for_tenant_with_gate(
            tenant_id,
            adapter_id,
            alias,
            &AliasUpdateGateConfig::default(),
        )
        .await
    }

    /// Tenant-scoped alias update with custom gating configuration.
    pub async fn update_adapter_alias_for_tenant_with_gate(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        alias: Option<&str>,
        gate: &AliasUpdateGateConfig,
    ) -> Result<()> {
        let adapter = self
            .get_adapter_for_tenant(tenant_id, adapter_id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        self.update_adapter_alias_inner(adapter, adapter_id, alias, gate)
            .await
    }

    async fn update_adapter_alias_inner(
        &self,
        adapter: Adapter,
        adapter_id: &str,
        alias: Option<&str>,
        gate: &AliasUpdateGateConfig,
    ) -> Result<()> {
        let update = AdapterAliasUpdate::from_alias(alias)?;
        if update.matches_adapter(&adapter) {
            return Ok(());
        }

        let lifecycle_state = LifecycleState::from_str(&adapter.lifecycle_state).map_err(|_| {
            AosError::Validation(format!(
                "Invalid lifecycle state '{}' for adapter {}",
                adapter.lifecycle_state, adapter_id
            ))
        })?;

        if !lifecycle_state.is_mutable() {
            match lifecycle_state {
                LifecycleState::Ready => {
                    if !gate.allow_ready {
                        return Err(AosError::PolicyViolation(format!(
                            "Alias update requires confirmation for adapter '{}' in ready state",
                            adapter_id
                        )));
                    }
                }
                LifecycleState::Active | LifecycleState::Deprecated => {
                    return Err(AosError::PolicyViolation(format!(
                        "Alias update blocked for adapter '{}' in {} state",
                        adapter_id,
                        lifecycle_state.as_str()
                    )));
                }
                LifecycleState::Retired | LifecycleState::Failed => {
                    return Err(AosError::PolicyViolation(format!(
                        "Alias update blocked for adapter '{}' in terminal {} state",
                        adapter_id,
                        lifecycle_state.as_str()
                    )));
                }
                _ => {
                    return Err(AosError::PolicyViolation(format!(
                        "Alias update not allowed for adapter '{}' in {} state",
                        adapter_id,
                        lifecycle_state.as_str()
                    )));
                }
            }
        }

        if self.storage_mode().write_to_sql() {
            let result = sqlx::query(
                "UPDATE adapters SET
                    adapter_name = ?,
                    tenant_namespace = ?,
                    domain = ?,
                    purpose = ?,
                    revision = ?,
                    updated_at = datetime('now')
                 WHERE tenant_id = ? AND adapter_id = ?",
            )
            .bind(&update.adapter_name)
            .bind(&update.tenant_namespace)
            .bind(&update.domain)
            .bind(&update.purpose)
            .bind(&update.revision)
            .bind(&adapter.tenant_id)
            .bind(adapter_id)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update adapter alias: {}", e)))?;

            if result.rows_affected() == 0 {
                return Err(AosError::NotFound(format!(
                    "Adapter not found: {}",
                    adapter_id
                )));
            }
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for update_adapter_alias".to_string(),
            ));
        }

        if let Some(repo) = self.get_adapter_kv_repo(&adapter.tenant_id) {
            if let Err(e) = repo.update_adapter_alias_kv(adapter_id, &update).await {
                self.record_kv_write_fallback("adapters.update_alias");
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %adapter.tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL alias update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                    );
                    return Err(AosError::Database(format!(
                        "Alias update succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %adapter.tenant_id,
                        mode = "dual-write",
                        "Failed to update adapter alias in KV backend"
                    );
                }
            } else {
                debug!(
                    adapter_id = %adapter_id,
                    tenant_id = %adapter.tenant_id,
                    mode = "dual-write",
                    "Adapter alias updated in both SQL and KV backends"
                );
            }
        }

        Ok(())
    }

    /// Update adapter display name (simple string) with lifecycle gating.
    ///
    /// Updates the `name` column shown in the UI. Use this for user-friendly
    /// renames (e.g. "My Adapter"). For semantic naming (tenant/domain/purpose/revision),
    /// use [`update_adapter_alias_for_tenant`].
    ///
    /// - `name: Some(s)` sets display name to trimmed `s` (empty string rejected)
    /// - `name: None` clears to default (adapter_id)
    /// - Blocked for Active, Deprecated, Retired, Failed (same gating as alias)
    pub async fn update_adapter_display_name_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        name: Option<&str>,
    ) -> Result<()> {
        let adapter = self
            .get_adapter_for_tenant(tenant_id, adapter_id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        let lifecycle_state = LifecycleState::from_str(&adapter.lifecycle_state).map_err(|_| {
            AosError::Validation(format!(
                "Invalid lifecycle state '{}' for adapter {}",
                adapter.lifecycle_state, adapter_id
            ))
        })?;

        if !lifecycle_state.is_mutable() {
            match lifecycle_state {
                LifecycleState::Active | LifecycleState::Deprecated => {
                    return Err(AosError::PolicyViolation(format!(
                        "Display name update blocked for adapter '{}' in {} state",
                        adapter_id,
                        lifecycle_state.as_str()
                    )));
                }
                LifecycleState::Retired | LifecycleState::Failed => {
                    return Err(AosError::PolicyViolation(format!(
                        "Display name update blocked for adapter '{}' in terminal {} state",
                        adapter_id,
                        lifecycle_state.as_str()
                    )));
                }
                _ => {
                    return Err(AosError::PolicyViolation(format!(
                        "Display name update not allowed for adapter '{}' in {} state",
                        adapter_id,
                        lifecycle_state.as_str()
                    )));
                }
            }
        }

        let display_name = match name {
            Some(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    return Err(AosError::Validation(
                        "Display name cannot be empty".to_string(),
                    ));
                }
                trimmed.to_string()
            }
            None => adapter
                .adapter_id
                .as_deref()
                .unwrap_or(adapter_id)
                .to_string(),
        };

        if self.storage_mode().write_to_sql() {
            let result = sqlx::query(
                "UPDATE adapters SET name = ?, updated_at = datetime('now')
                 WHERE tenant_id = ? AND adapter_id = ?",
            )
            .bind(&display_name)
            .bind(&adapter.tenant_id)
            .bind(adapter_id)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to update adapter display name: {}", e))
            })?;

            if result.rows_affected() == 0 {
                return Err(AosError::NotFound(format!(
                    "Adapter not found: {}",
                    adapter_id
                )));
            }
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for update_adapter_display_name".to_string(),
            ));
        }

        if let Some(repo) = self.get_adapter_kv_repo(&adapter.tenant_id) {
            if let Err(e) = repo
                .update_adapter_display_name_kv(adapter_id, &display_name)
                .await
            {
                self.record_kv_write_fallback("adapters.update_display_name");
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %adapter.tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL display name update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                    );
                    return Err(AosError::Database(format!(
                        "Display name update succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %adapter.tenant_id,
                        mode = "dual-write",
                        "Failed to update adapter display name in KV backend"
                    );
                }
            } else {
                debug!(
                    adapter_id = %adapter_id,
                    tenant_id = %adapter.tenant_id,
                    mode = "dual-write",
                    "Adapter display name updated in both SQL and KV backends"
                );
            }
        }

        Ok(())
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
    async fn store_aos_metadata_if_present(
        &self,
        adapter_internal_id: &str,
        params: &AdapterRegistrationParams,
    ) -> Result<()> {
        if !self.storage_mode().write_to_sql() {
            return Ok(());
        }

        let (aos_path, aos_hash) = match (&params.aos_file_path, &params.aos_file_hash) {
            (Some(path), Some(hash)) if !path.is_empty() && !hash.is_empty() => (path, hash),
            _ => return Ok(()),
        };

        let mut aos_path = aos_path.clone();
        if let Ok(canonical) = Path::new(&aos_path).canonicalize() {
            aos_path = canonical.to_string_lossy().into_owned();
        }

        let manifest_info = match parse_aos_manifest_metadata(Path::new(&aos_path)) {
            Ok(info) => Some(info),
            Err(err) => {
                warn!(
                    error = %err,
                    path = %aos_path,
                    "Failed to parse .aos manifest metadata"
                );
                None
            }
        };

        let manifest_schema_version = params.manifest_schema_version.clone().or_else(|| {
            manifest_info
                .as_ref()
                .and_then(|info| info.manifest_schema_version.clone())
        });
        let base_model_id = params.base_model_id.clone().or_else(|| {
            manifest_info
                .as_ref()
                .and_then(|info| info.base_model.clone())
        });
        let category = if params.category.trim().is_empty() {
            manifest_info
                .as_ref()
                .and_then(|info| info.category.clone())
        } else {
            Some(params.category.clone())
        };
        let tier = if params.tier.trim().is_empty() {
            manifest_info.as_ref().and_then(|info| info.tier.clone())
        } else {
            Some(params.tier.clone())
        };

        sqlx::query(
            "UPDATE adapters
             SET aos_file_path = ?,
                 aos_file_hash = ?,
                 manifest_schema_version = COALESCE(?, manifest_schema_version),
                 base_model_id = COALESCE(?, base_model_id),
                 metadata_json = COALESCE(?, metadata_json),
                 content_hash_b3 = ?,
                 manifest_hash = COALESCE(?, manifest_hash),
                 updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(&aos_path)
        .bind(&aos_hash)
        .bind(manifest_schema_version.as_deref())
        .bind(base_model_id.as_deref())
        .bind(&params.metadata_json)
        .bind(&params.content_hash_b3)
        .bind(&params.manifest_hash)
        .bind(adapter_internal_id)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(format!("Failed to update adapter AOS fields: {}", e)))?;

        let mut meta = StoreAdapterFileMetadataParams::new(
            adapter_internal_id.to_string(),
            aos_path.clone(),
            aos_hash.clone(),
        );

        if let Ok(fs_meta) = std::fs::metadata(&aos_path) {
            meta = meta.file_size_bytes(fs_meta.len() as i64);
            if let Ok(modified) = fs_meta.modified() {
                let modified: DateTime<Utc> = modified.into();
                meta = meta.file_modified_at(modified.to_rfc3339());
            }
        }
        match read_aos_segment_count(Path::new(&aos_path)) {
            Ok(Some(count)) => {
                meta = meta.segment_count(count);
            }
            Ok(None) => {}
            Err(err) => {
                warn!(
                    error = %err,
                    path = %aos_path,
                    "Failed to read .aos segment count"
                );
            }
        }

        match read_single_file_adapter_metadata(Path::new(&aos_path)).await {
            Ok(Some(metadata)) => {
                if let Some(count) = metadata.training_data_count {
                    meta = meta.training_data_count(count);
                }
                if let Some(version) = metadata.lineage_version {
                    meta = meta.lineage_version(version);
                }
                if let Some(valid) = metadata.signature_valid {
                    meta = meta.signature_valid(valid);
                }
            }
            Ok(None) => {}
            Err(err) => {
                warn!(
                    error = %err,
                    path = %aos_path,
                    "Failed to read single-file adapter metadata"
                );
            }
        }

        if let Some(ref info) = manifest_info {
            if meta.training_data_count.is_none() {
                if let Some(count) = info.training_data_count {
                    meta = meta.training_data_count(count);
                }
            }
        }

        if let Some(ref version) = manifest_schema_version {
            meta = meta.manifest_schema_version(version.clone());
        }

        if let Some(ref base_model) = base_model_id {
            meta = meta.base_model(base_model.clone());
        }

        if let Some(ref category) = category {
            meta = meta.category(category.clone());
        }

        if let Some(ref tier) = tier {
            meta = meta.tier(tier.clone());
        }

        self.store_adapter_file_metadata(meta).await
    }

    async fn persist_adapter_metadata_from_params(
        &self,
        adapter_internal_id: &str,
        params: &AdapterRegistrationParams,
    ) -> Result<()> {
        let patch = AdapterMetadataPatch::from_params(params);
        if !patch.has_updates() {
            return Ok(());
        }

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "UPDATE adapters SET
                    aos_file_path = COALESCE(?, aos_file_path),
                    aos_file_hash = COALESCE(?, aos_file_hash),
                    base_model_id = COALESCE(?, base_model_id),
                    manifest_schema_version = COALESCE(?, manifest_schema_version),
                    content_hash_b3 = COALESCE(?, content_hash_b3),
                    metadata_json = COALESCE(?, metadata_json),
                    provenance_json = COALESCE(?, provenance_json),
                    repo_path = COALESCE(?, repo_path),
                    codebase_scope = COALESCE(?, codebase_scope),
                    dataset_version_id = COALESCE(?, dataset_version_id),
                    registration_timestamp = COALESCE(?, registration_timestamp),
                    manifest_hash = COALESCE(?, manifest_hash),
                    updated_at = datetime('now')
                 WHERE id = ?",
            )
            .bind(&patch.aos_file_path)
            .bind(&patch.aos_file_hash)
            .bind(&patch.base_model_id)
            .bind(&patch.manifest_schema_version)
            .bind(&patch.content_hash_b3)
            .bind(&patch.metadata_json)
            .bind(&patch.provenance_json)
            .bind(&patch.repo_path)
            .bind(&patch.codebase_scope)
            .bind(&patch.dataset_version_id)
            .bind(&patch.registration_timestamp)
            .bind(&patch.manifest_hash)
            .bind(adapter_internal_id)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| {
                AosError::database(format!("Failed to persist adapter metadata: {}", e))
            })?;
        }

        if let Some(repo) = self.get_adapter_kv_repo(&params.tenant_id) {
            if let Err(e) = repo
                .update_adapter_metadata_kv(&params.adapter_id, &patch)
                .await
            {
                self.record_kv_write_fallback("adapters.persist_metadata");
                warn!(
                    error = %e,
                    adapter_id = %params.adapter_id,
                    "KV metadata update failed"
                );
            }
        }

        Ok(())
    }

    async fn update_adapter_aos_fields_if_missing(
        &self,
        adapter_internal_id: &str,
        existing: &Adapter,
        params: &AdapterRegistrationParams,
    ) -> Result<()> {
        if !self.storage_mode().write_to_sql() {
            return Ok(());
        }

        let mut new_path = params
            .aos_file_path
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        if let Some(ref mut path) = new_path {
            if let Ok(canonical) = Path::new(path).canonicalize() {
                *path = canonical.to_string_lossy().into_owned();
            }
        }

        let new_hash = params
            .aos_file_hash
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());

        let missing_path = existing
            .aos_file_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none();
        let missing_hash = existing
            .aos_file_hash
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none();

        let should_update_path = missing_path && new_path.is_some();
        let should_update_hash = missing_hash && new_hash.is_some();
        if !should_update_path && !should_update_hash {
            return Ok(());
        }

        sqlx::query(
            "UPDATE adapters
             SET aos_file_path = CASE WHEN aos_file_path IS NULL OR aos_file_path = '' THEN ? ELSE aos_file_path END,
                 aos_file_hash = CASE WHEN aos_file_hash IS NULL OR aos_file_hash = '' THEN ? ELSE aos_file_hash END,
                 updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(new_path.as_deref())
        .bind(new_hash.as_deref())
        .bind(adapter_internal_id)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| {
            AosError::database(format!(
                "Failed to update adapter .aos fields: {}",
                e
            ))
        })?;

        Ok(())
    }

    async fn record_adapter_session_membership(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        metadata_json: Option<&str>,
    ) -> Result<()> {
        let Some(context) = parse_session_context(metadata_json) else {
            return Ok(());
        };

        self.ensure_dataset_collection_session(
            &context.session_id,
            context.session_name.as_deref(),
            context.session_tags.as_deref(),
            Some(tenant_id),
        )
        .await?;

        self.link_adapter_to_collection_session(
            &context.session_id,
            adapter_id,
            Some("registered"),
            None,
        )
        .await?;

        Ok(())
    }

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
        mut params: AdapterRegistrationParams,
    ) -> Result<String> {
        if self.get_tenant(&params.tenant_id).await?.is_none() {
            return Err(AosError::validation(format!(
                "Tenant '{}' does not exist",
                params.tenant_id
            )));
        }

        let is_codebase_adapter = params
            .codebase_scope
            .as_ref()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
            || params
                .repo_id
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
            || params
                .repo_path
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
        if is_codebase_adapter && params.tenant_id != "system" {
            return Err(AosError::validation(
                "Codebase adapters must be registered under the system tenant".to_string(),
            ));
        }

        let hash_missing = params
            .aos_file_hash
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none();
        if hash_missing {
            if let Some(ref path) = params.aos_file_path {
                let computed = compute_aos_file_hash(Path::new(path))?;
                params.aos_file_hash = Some(computed);
            }
        }

        if let Some(aos_path) = params.aos_file_path.clone() {
            match read_aos_manifest_bytes(Path::new(&aos_path)) {
                Ok(Some(manifest_bytes)) => {
                    let manifest_hash = blake3::hash(&manifest_bytes).to_hex().to_string();
                    if params
                        .manifest_hash
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_none()
                    {
                        params.manifest_hash = Some(manifest_hash);
                    }

                    match serde_json::from_slice::<Value>(&manifest_bytes) {
                        Ok(manifest) => {
                            let manifest_schema_version = manifest
                                .get("schema_version")
                                .and_then(value_to_trimmed_string)
                                .or_else(|| {
                                    manifest
                                        .get("manifest_schema_version")
                                        .and_then(value_to_trimmed_string)
                                })
                                .or_else(|| {
                                    manifest.get("version").and_then(value_to_trimmed_string)
                                });

                            if params
                                .manifest_schema_version
                                .as_deref()
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .is_none()
                            {
                                if let Some(version) = manifest_schema_version {
                                    params.manifest_schema_version = Some(version);
                                }
                            }

                            let manifest_category =
                                manifest.get("category").and_then(value_to_trimmed_string);
                            if let Some(category) = manifest_category {
                                let current = params.category.trim();
                                if current.is_empty() || current == "code" {
                                    params.category = category;
                                }
                            }

                            let manifest_tier =
                                manifest.get("tier").and_then(value_to_trimmed_string);
                            if let Some(tier) = manifest_tier {
                                let current = params.tier.trim();
                                if current.is_empty() || current == "warm" {
                                    params.tier = tier;
                                }
                            }

                            if params
                                .metadata_json
                                .as_deref()
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .is_none()
                            {
                                if let Some(metadata_obj) =
                                    manifest.get("metadata").and_then(|v| v.as_object())
                                {
                                    if let Ok(json) = serde_json::to_string(metadata_obj) {
                                        params.metadata_json = Some(json);
                                    }
                                }
                            }

                            if params
                                .base_model_id
                                .as_deref()
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .is_none()
                            {
                                if let Some(model_id) = manifest
                                    .get("base_model_id")
                                    .and_then(value_to_trimmed_string)
                                {
                                    params.base_model_id = Some(model_id);
                                } else if let Some(model_name) =
                                    manifest.get("base_model").and_then(value_to_trimmed_string)
                                {
                                    let tenant_id = params.tenant_id.clone();
                                    match self
                                        .get_model_by_name_for_tenant(&tenant_id, &model_name)
                                        .await
                                    {
                                        Ok(Some(model)) => {
                                            params.base_model_id = Some(model.id);
                                        }
                                        Ok(None) => {
                                            warn!(
                                                base_model = %model_name,
                                                tenant_id = %tenant_id,
                                                "Manifest base model not found for tenant"
                                            );
                                        }
                                        Err(err) => {
                                            warn!(
                                                base_model = %model_name,
                                                tenant_id = %tenant_id,
                                                error = %err,
                                                "Failed to resolve manifest base model"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            warn!(
                                path = %aos_path,
                                error = %err,
                                "Failed to parse .aos manifest JSON"
                            );
                        }
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    warn!(
                        path = %aos_path,
                        error = %err,
                        "Failed to read .aos manifest bytes"
                    );
                }
            }
        }

        let normalized_hash_b3 = params
            .hash_b3
            .trim()
            .trim_start_matches("b3:")
            .to_ascii_lowercase();
        let normalized_content_hash = params
            .content_hash_b3
            .trim()
            .trim_start_matches("b3:")
            .to_ascii_lowercase();
        let content_hash_needs_compute =
            normalized_content_hash.is_empty() || normalized_content_hash == normalized_hash_b3;
        let manifest_hash_missing = params
            .manifest_hash
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none();

        if (content_hash_needs_compute || manifest_hash_missing) && params.aos_file_path.is_some() {
            if let Some(aos_path) = params.aos_file_path.clone() {
                match std::fs::read(&aos_path) {
                    Ok(bytes) => match open_aos(&bytes) {
                        Ok(view) => {
                            if manifest_hash_missing {
                                params.manifest_hash =
                                    Some(B3Hash::hash(view.manifest_bytes).to_hex());
                            }

                            let scope_path = serde_json::from_slice::<Value>(view.manifest_bytes)
                                .ok()
                                .and_then(|manifest| manifest.get("metadata").cloned())
                                .and_then(|meta| meta.get("scope_path").cloned())
                                .and_then(|val| val.as_str().map(|s| s.to_string()));

                            let canonical_segment = scope_path
                                .as_deref()
                                .map(compute_scope_hash)
                                .and_then(|scope_hash| {
                                    view.segments.iter().find(|seg| {
                                        seg.backend_tag == BackendTag::Canonical
                                            && seg.scope_hash == scope_hash
                                    })
                                })
                                .or_else(|| {
                                    view.segments
                                        .iter()
                                        .find(|seg| seg.backend_tag == BackendTag::Canonical)
                                })
                                .or_else(|| view.segments.first());

                            if let Some(segment) = canonical_segment {
                                if content_hash_needs_compute {
                                    params.content_hash_b3 =
                                        B3Hash::hash_multi(&[view.manifest_bytes, segment.payload])
                                            .to_hex();
                                }

                                let actual_weights_hash = B3Hash::hash(segment.payload).to_hex();
                                if !normalized_hash_b3.is_empty()
                                    && actual_weights_hash != normalized_hash_b3
                                {
                                    warn!(
                                        path = %aos_path,
                                        expected = %normalized_hash_b3,
                                        actual = %actual_weights_hash,
                                        "Weights hash does not match canonical segment"
                                    );
                                }
                            } else if content_hash_needs_compute {
                                warn!(
                                    path = %aos_path,
                                    "No segments found in .aos bundle; content hash not computed"
                                );
                            }
                        }
                        Err(err) => {
                            warn!(
                                path = %aos_path,
                                error = %err,
                                "Failed to parse .aos file for content hash"
                            );
                        }
                    },
                    Err(err) => {
                        warn!(
                            path = %aos_path,
                            error = %err,
                            "Failed to read .aos file for content hash"
                        );
                    }
                }
            }
        }

        if params.content_hash_b3.trim().is_empty() {
            params.content_hash_b3 = params.hash_b3.clone();
        }

        // Idempotency check: if adapter with same adapter_id exists, verify hash matches
        // This prevents duplicate registrations while allowing safe retries
        if let Some(existing) = self
            .get_adapter_for_tenant(&params.tenant_id, &params.adapter_id)
            .await?
        {
            if existing.hash_b3 == params.hash_b3 {
                // Exact match - return existing ID (idempotent)
                tracing::info!(
                    adapter_id = %params.adapter_id,
                    hash_b3 = %params.hash_b3,
                    existing_id = %existing.id,
                    "Adapter already registered with identical hash - returning existing ID"
                );
                let membership_adapter_id = existing
                    .adapter_id
                    .as_deref()
                    .unwrap_or(params.adapter_id.as_str());
                if let Err(e) = self
                    .record_adapter_session_membership(
                        membership_adapter_id,
                        &params.tenant_id,
                        params.metadata_json.as_deref(),
                    )
                    .await
                {
                    warn!(
                        error = %e,
                        adapter_id = %membership_adapter_id,
                        tenant_id = %params.tenant_id,
                        "Failed to record adapter session membership"
                    );
                }
                if let Err(e) = self
                    .store_aos_metadata_if_present(&existing.id, &params)
                    .await
                {
                    warn!(
                        error = %e,
                        adapter_id = %params.adapter_id,
                        internal_id = %existing.id,
                        "Failed to store .aos metadata for existing adapter"
                    );
                }
                if let Err(e) = self
                    .persist_adapter_metadata_from_params(&existing.id, &params)
                    .await
                {
                    warn!(
                        error = %e,
                        adapter_id = %params.adapter_id,
                        internal_id = %existing.id,
                        "Failed to persist metadata for existing adapter"
                    );
                }
                if let Err(e) = self
                    .update_adapter_aos_fields_if_missing(&existing.id, &existing, &params)
                    .await
                {
                    warn!(
                        error = %e,
                        adapter_id = %params.adapter_id,
                        internal_id = %existing.id,
                        "Failed to update .aos fields for existing adapter"
                    );
                }
                return Ok(existing.id);
            } else {
                // Hash mismatch - conflict error
                return Err(AosError::validation(format!(
                    "Adapter '{}' already registered with different hash (existing: {}, new: {}). \
                     Use a new adapter_id or update the existing adapter.",
                    params.adapter_id, existing.hash_b3, params.hash_b3
                )));
            }
        }

        // Deduplication check: if adapter with same content_hash_b3 exists, return existing ID
        // This prevents duplicate adapters with identical content (unique index on content_hash_b3)
        if let Some(existing) = self
            .find_adapter_by_content_hash(&params.content_hash_b3)
            .await?
        {
            tracing::info!(
                content_hash_b3 = %params.content_hash_b3,
                existing_id = %existing.id,
                existing_adapter_id = %existing.adapter_id.as_deref().unwrap_or("N/A"),
                "Adapter with identical content_hash_b3 already exists - returning existing ID"
            );
            let membership_adapter_id = existing
                .adapter_id
                .as_deref()
                .unwrap_or(params.adapter_id.as_str());
            if let Err(e) = self
                .record_adapter_session_membership(
                    membership_adapter_id,
                    &params.tenant_id,
                    params.metadata_json.as_deref(),
                )
                .await
            {
                warn!(
                    error = %e,
                    adapter_id = %membership_adapter_id,
                    tenant_id = %params.tenant_id,
                    "Failed to record adapter session membership for duplicate content hash"
                );
            }
            if let Err(e) = self
                .store_aos_metadata_if_present(&existing.id, &params)
                .await
            {
                warn!(
                    error = %e,
                    adapter_id = %params.adapter_id,
                    internal_id = %existing.id,
                    "Failed to store .aos metadata for duplicate content hash"
                );
            }
            if let Err(e) = self
                .persist_adapter_metadata_from_params(&existing.id, &params)
                .await
            {
                warn!(
                    error = %e,
                    adapter_id = %params.adapter_id,
                    internal_id = %existing.id,
                    "Failed to persist metadata for duplicate content hash"
                );
            }
            if let Err(e) = self
                .update_adapter_aos_fields_if_missing(&existing.id, &existing, &params)
                .await
            {
                warn!(
                    error = %e,
                    adapter_id = %params.adapter_id,
                    internal_id = %existing.id,
                    "Failed to update .aos fields for duplicate content hash"
                );
            }
            return Ok(existing.id);
        }

        let id = new_id(IdPrefix::Adp);
        // NOTE: stable_id is computed atomically in the INSERT statement using a subquery
        // to prevent race conditions. See INSERT ... (COALESCE((SELECT MAX(stable_id)...

        let mut dual_write_completed = false;
        let dual_write_timer =
            if self.storage_mode().write_to_sql() && self.storage_mode().write_to_kv() {
                Some(Instant::now())
            } else {
                None
            };

        // For dual-write mode with strict atomicity, we need to:
        // 1. Start a transaction BEFORE any writes
        // 2. Execute SQL insert within transaction (don't commit yet)
        // 3. Execute KV write
        // 4. If both succeed, commit the transaction
        // 5. If KV fails, rollback the transaction (not committed yet, so this works atomically)
        let kv_repo_available = self.get_adapter_kv_repo(&params.tenant_id).is_some();
        if self.storage_mode().write_to_kv()
            && self.dual_write_requires_strict()
            && !kv_repo_available
        {
            return Err(AosError::database(
                "KV backend unavailable for strict adapter registration".to_string(),
            ));
        }

        let needs_dual_write = self.storage_mode().write_to_sql()
            && self.storage_mode().write_to_kv()
            && kv_repo_available;

        // Write to SQL when allowed by storage mode
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                if needs_dual_write && self.dual_write_requires_strict() {
                    // Use transaction-based atomic dual-write for strict mode
                    let mut tx = sqlx::Acquire::begin(pool)
                        .await
                        .map_err(|e| AosError::database(e.to_string()))?;

                    // SQL insert within transaction (don't commit yet)
                    // CRITICAL: stable_id is computed atomically using a subquery to prevent race conditions
                    // The subquery runs within the same transaction, ensuring unique stable_ids per tenant
                    sqlx::query(
                        "INSERT INTO adapters (id, tenant_id, adapter_id, name, hash_b3, rank, alpha, lora_strength, tier, targets_json, acl_json, languages_json, framework, category, scope, framework_id, framework_version, repo_id, commit_sha, intent, expires_at, adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason, aos_file_path, aos_file_hash, base_model_id, recommended_for_moe, manifest_schema_version, content_hash_b3, metadata_json, provenance_json, repo_path, codebase_scope, dataset_version_id, registration_timestamp, manifest_hash, training_dataset_hash_b3, stable_id, version, lifecycle_state, current_state, pinned, memory_bytes, activation_count, load_state, active)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31, $32, $33, $34, $35, $36, $37, $38, $39, $40, $41, $42, $43, (COALESCE((SELECT MAX(stable_id) FROM adapters WHERE tenant_id = $2), 0) + 1), '1.0.0', 'draft', 'unloaded', 0, 0, 0, 'cold', 1)"
                    )
                    .bind(&id)
                    .bind(&params.tenant_id)
                    .bind(&params.adapter_id)
                    .bind(&params.name)
                    .bind(&params.hash_b3)
                    .bind(params.rank)
                    .bind(params.alpha)
                    .bind(&params.lora_strength)
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
                    .bind(params.recommended_for_moe.unwrap_or(true))
                    .bind(&params.manifest_schema_version)
                    .bind(&params.content_hash_b3)
                    .bind(&params.metadata_json)
                    .bind(&params.provenance_json)
                    .bind(&params.repo_path)
                    .bind(&params.codebase_scope)
                    .bind(&params.dataset_version_id)
                    .bind(&params.registration_timestamp)
                    .bind(&params.manifest_hash)
                    .bind(&params.training_dataset_hash_b3)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AosError::database(e.to_string()))?;

                    // KV write - if this fails, we can rollback SQL (not committed yet)
                    // Initialize WriteAck for tracking this dual-write operation
                    let mut write_ack = WriteAck::new("adapter", &id);
                    write_ack.sql_status = WriteStatus::Ok; // SQL insert succeeded (in transaction)

                    if let Some(repo) = self.get_adapter_kv_repo(&params.tenant_id) {
                        match repo.register_adapter_kv_with_id(&id, params.clone()).await {
                            Ok(_) => {
                                // Both succeeded, now commit the SQL transaction
                                tx.commit()
                                    .await
                                    .map_err(|e| AosError::database(e.to_string()))?;
                                dual_write_completed = true;
                                write_ack.kv_status = WriteStatus::Ok;
                                write_ack.complete();
                                debug!(adapter_id = %id, tenant_id = %params.tenant_id, mode = "dual-write-strict", "Adapter registered atomically in both SQL and KV backends");

                                // Record successful dual-write ack
                                if let Err(ack_err) = self.store_ack(&write_ack).await {
                                    warn!(
                                        error = %ack_err,
                                        adapter_id = %id,
                                        operation_id = %write_ack.operation_id,
                                        "Failed to store WriteAck for successful dual-write"
                                    );
                                }
                            }
                            Err(e) => {
                                // KV failed, rollback SQL (not committed yet, so this works atomically)
                                write_ack.kv_status = WriteStatus::Failed {
                                    error: e.to_string(),
                                };
                                write_ack.sql_status = WriteStatus::Pending; // Rolled back
                                write_ack.mark_degraded(
                                    "KV write failed, SQL rolled back in strict mode",
                                );
                                write_ack.complete();

                                error!(
                                    error = %e,
                                    adapter_id = %id,
                                    tenant_id = %params.tenant_id,
                                    mode = "dual-write-strict",
                                    operation_id = %write_ack.operation_id,
                                    "KV write failed in strict atomic mode - rolling back uncommitted SQL transaction"
                                );
                                if let Err(rollback_err) = tx.rollback().await {
                                    error!(
                                        error = %rollback_err,
                                        adapter_id = %id,
                                        "Transaction rollback failed after KV write failure - connection may be in inconsistent state"
                                    );
                                }

                                // Record failed dual-write ack for audit trail
                                if let Err(ack_err) = self.store_ack(&write_ack).await {
                                    warn!(
                                        error = %ack_err,
                                        adapter_id = %id,
                                        operation_id = %write_ack.operation_id,
                                        "Failed to store WriteAck for failed dual-write"
                                    );
                                }

                                return Err(AosError::database(format!(
                                    "KV write failed in strict mode for adapter_id={id}: {e}"
                                )));
                            }
                        }
                    }
                } else {
                    // Non-strict mode or SQL-only: use direct execute (auto-commit)
                    // CRITICAL: stable_id is computed atomically using a subquery to prevent race conditions
                    sqlx::query(
                        "INSERT INTO adapters (id, tenant_id, adapter_id, name, hash_b3, rank, alpha, lora_strength, tier, targets_json, acl_json, languages_json, framework, category, scope, framework_id, framework_version, repo_id, commit_sha, intent, expires_at, adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason, aos_file_path, aos_file_hash, base_model_id, recommended_for_moe, manifest_schema_version, content_hash_b3, metadata_json, provenance_json, repo_path, codebase_scope, dataset_version_id, registration_timestamp, manifest_hash, training_dataset_hash_b3, stable_id, version, lifecycle_state, current_state, pinned, memory_bytes, activation_count, load_state, active)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31, $32, $33, $34, $35, $36, $37, $38, $39, $40, $41, $42, $43, (COALESCE((SELECT MAX(stable_id) FROM adapters WHERE tenant_id = $2), 0) + 1), '1.0.0', 'draft', 'unloaded', 0, 0, 0, 'cold', 1)"
                    )
                    .bind(&id)
                    .bind(&params.tenant_id)
                    .bind(&params.adapter_id)
                    .bind(&params.name)
                    .bind(&params.hash_b3)
                    .bind(params.rank)
                    .bind(params.alpha)
                    .bind(&params.lora_strength)
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
                    .bind(params.recommended_for_moe.unwrap_or(true))
                    .bind(&params.manifest_schema_version)
                    .bind(&params.content_hash_b3)
                    .bind(&params.metadata_json)
                    .bind(&params.provenance_json)
                    .bind(&params.repo_path)
                    .bind(&params.codebase_scope)
                    .bind(&params.dataset_version_id)
                    .bind(&params.registration_timestamp)
                    .bind(&params.manifest_hash)
                    .bind(&params.training_dataset_hash_b3)
                    .execute(pool)
                    .await
                    .map_err(|e| AosError::database(e.to_string()))?;

                    // KV write (non-strict dual-write mode) - best effort, log on failure
                    // Initialize WriteAck for tracking this dual-write operation
                    let mut write_ack = WriteAck::new("adapter", &id);
                    write_ack.sql_status = WriteStatus::Ok; // SQL insert succeeded

                    if let Some(repo) = self.get_adapter_kv_repo(&params.tenant_id) {
                        if let Err(e) = repo.register_adapter_kv_with_id(&id, params.clone()).await
                        {
                            write_ack.kv_status = WriteStatus::Failed {
                                error: e.to_string(),
                            };
                            write_ack.mark_degraded("KV write failed in best-effort mode");
                            write_ack.complete();
                            warn!(
                                error = %e,
                                adapter_id = %id,
                                mode = "dual-write",
                                operation_id = %write_ack.operation_id,
                                "Failed to write adapter to KV backend"
                            );

                            // Record degraded dual-write ack for repair queue
                            if let Err(ack_err) = self.store_ack(&write_ack).await {
                                warn!(
                                    error = %ack_err,
                                    adapter_id = %id,
                                    operation_id = %write_ack.operation_id,
                                    "Failed to store WriteAck for degraded dual-write"
                                );
                            }
                        } else {
                            dual_write_completed = true;
                            write_ack.kv_status = WriteStatus::Ok;
                            write_ack.complete();
                            debug!(adapter_id = %id, tenant_id = %params.tenant_id, mode = "dual-write", "Adapter registered in both SQL and KV backends");

                            // Record successful dual-write ack
                            if let Err(ack_err) = self.store_ack(&write_ack).await {
                                warn!(
                                    error = %ack_err,
                                    adapter_id = %id,
                                    operation_id = %write_ack.operation_id,
                                    "Failed to store WriteAck for successful dual-write"
                                );
                            }
                        }
                    } else {
                        // KV repo not available - mark as unavailable
                        write_ack.kv_status = WriteStatus::Unavailable;
                        write_ack.complete();
                    }
                }
            } else if !self.storage_mode().write_to_kv() {
                // No SQL pool and not writing to KV means we cannot satisfy the write
                return Err(AosError::database(
                    "SQL backend unavailable for adapter registration".to_string(),
                ));
            } else {
                // SQL pool unavailable but KV is enabled - write to KV only
                if let Some(repo) = self.get_adapter_kv_repo(&params.tenant_id) {
                    repo.register_adapter_kv_with_id(&id, params.clone())
                        .await
                        .map_err(|e| AosError::database(e.to_string()))?;
                }
            }
        } else {
            // SQL writes disabled - write to KV only if enabled
            if let Some(repo) = self.get_adapter_kv_repo(&params.tenant_id) {
                repo.register_adapter_kv_with_id(&id, params.clone())
                    .await
                    .map_err(|e| AosError::database(e.to_string()))?;
            }
        }

        if dual_write_completed {
            if let Some(start) = dual_write_timer {
                global_kv_metrics().record_dual_write_lag(start.elapsed());
            }
        }

        if let Err(e) = self.store_aos_metadata_if_present(&id, &params).await {
            warn!(
                error = %e,
                adapter_id = %params.adapter_id,
                internal_id = %id,
                "Failed to store .aos metadata for new adapter"
            );
        }

        if let Err(e) = self
            .record_adapter_session_membership(
                &params.adapter_id,
                &params.tenant_id,
                params.metadata_json.as_deref(),
            )
            .await
        {
            warn!(
                error = %e,
                adapter_id = %params.adapter_id,
                tenant_id = %params.tenant_id,
                "Failed to record adapter session membership for new adapter"
            );
        }

        Ok(id.to_string())
    }

    /// Find all expired adapters
    pub async fn find_expired_adapters(&self) -> Result<Vec<Adapter>> {
        deny_unscoped_adapter_query("find_expired_adapters")?;
        let query = format!(
            "SELECT {} FROM adapters WHERE expires_at IS NOT NULL AND expires_at < datetime('now')",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .fetch_all(self.pool_result()?)
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
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
        deny_unscoped_adapter_query("list_adapters")?;
        let query = format!(
            "SELECT {} FROM adapters WHERE active = 1 ORDER BY tier ASC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .fetch_all(self.pool_result()?)
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
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
        if self.storage_mode().read_from_kv() {
            let mut adapters = Vec::new();

            let tenants = self.list_tenants().await?;
            for tenant in tenants {
                let tenant_adapters = self.list_adapters_for_tenant(&tenant.id).await?;
                adapters.extend(tenant_adapters);
            }

            if !adapters.is_empty() || !self.storage_mode().sql_fallback_enabled() {
                adapters.sort_by(|a, b| {
                    a.tier
                        .cmp(&b.tier)
                        .then_with(|| b.created_at.cmp(&a.created_at))
                });
                return Ok(adapters);
            }

            self.record_kv_read_fallback("adapters.list_all.system");
        }

        let query = format!(
            "SELECT {} FROM adapters WHERE active = 1 ORDER BY tier ASC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .fetch_all(self.pool_result()?)
            .await
            .map_err(|e| {
                AosError::database(format!("Failed to list all adapters (system): {}", e))
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
    /// Adaptive Query Planner (Phase 2, Item 8)
    ///
    /// Dynamically selects optimal index paths based on tenant data distribution.
    /// Currently enforces the Migration 0210 Golden Index.
    pub fn select_adapter_query_plan(&self, _tenant_id: &str) -> &'static str {
        "idx_adapters_tenant_active_tier_created"
    }

    /// Backward-compatible listing without pagination (defaults to full set).
    pub async fn list_adapters_for_tenant(&self, tenant_id: &str) -> Result<Vec<Adapter>> {
        self.list_adapters_for_tenant_impl(tenant_id, None, None, true)
            .await
    }

    /// List adapters for a tenant with optional limit/offset.
    pub async fn list_adapters_for_tenant_paged(
        &self,
        tenant_id: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<Adapter>> {
        self.list_adapters_for_tenant_impl(tenant_id, limit, offset, true)
            .await
    }

    /// List adapters for internal execution paths without adapter-listing quota checks.
    ///
    /// Use this for critical internal flows (e.g. inference routing metadata resolution)
    /// where failing closed on listing quota can incorrectly fail unrelated operations.
    pub async fn list_adapters_for_tenant_unthrottled(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<Adapter>> {
        self.list_adapters_for_tenant_impl(tenant_id, None, None, false)
            .await
    }

    async fn list_adapters_for_tenant_impl(
        &self,
        tenant_id: &str,
        limit: Option<usize>,
        offset: Option<usize>,
        enforce_rate_limit: bool,
    ) -> Result<Vec<Adapter>> {
        // Phase 2: Rate Limiting (public listing paths)
        if enforce_rate_limit {
            if !self.check_rate_limit(tenant_id) {
                return Err(AosError::QuotaExceeded {
                    resource: "adapter_listings".to_string(),
                    failure_code: Some("RATE_LIMIT_EXCEEDED".to_string()),
                });
            }
            self.increment_rate_limit(tenant_id);
        }

        // Try KV first if enabled
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                match repo
                    .list_adapters_for_tenant_kv(tenant_id, limit, offset)
                    .await
                {
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
        // Optimization: Use Migration 0210 composite index explicitly
        let mut query = format!(
            "SELECT {} FROM adapters INDEXED BY idx_adapters_tenant_active_tier_created \
             WHERE tenant_id = ? AND active = 1 \
             ORDER BY tier ASC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        if limit.is_some() {
            query.push_str(" LIMIT ?");
        }
        if offset.is_some() {
            query.push_str(" OFFSET ?");
        }

        #[cfg(test)]
        {
            let index_exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM sqlite_master WHERE type='index' AND name = ? LIMIT 1",
            )
            .bind("idx_adapters_tenant_active_tier_created")
            .fetch_optional(self.pool_result()?)
            .await?;

            if index_exists.is_none() {
                // In test environment, index might be missing if migration hasn't run.
                // Fallback to standard query to prevent test failures, but warn.
                warn!("idx_adapters_tenant_active_tier_created missing in test env");
            }
        }

        // Performance monitoring for tenant-scoped queries
        let start_time = std::time::Instant::now();

        // Phase 2: Execution Time Budgets
        let timeout_duration = self.get_query_timeout();
        let pool = self.pool_result()?.clone();
        let tenant_id_owned = tenant_id.to_string();

        let adapters_future = async move {
            let mut q = sqlx::query_as::<_, Adapter>(&query).bind(tenant_id_owned);
            if let Some(lim) = limit {
                q = q.bind(lim as i64);
            }
            if let Some(off) = offset {
                q = q.bind(off as i64);
            }
            q.fetch_all(&pool).await
        };

        let adapters = if timeout_duration.as_millis() > 0 {
            tokio::time::timeout(timeout_duration, adapters_future)
                .await
                .map_err(|_| {
                    AosError::PerformanceViolation(format!(
                        "Query timeout after {:?}",
                        timeout_duration
                    ))
                })?
                .map_err(|e| {
                    AosError::database(format!("Failed to list adapters for tenant: {}", e))
                })?
        } else {
            adapters_future.await.map_err(|e| {
                AosError::database(format!("Failed to list adapters for tenant: {}", e))
            })?
        };

        let execution_time = start_time.elapsed();

        // Record performance metrics if monitoring is enabled
        if let Some(monitor_guard) = self.performance_monitor() {
            if let Some(monitor) = monitor_guard.as_ref() {
                let mut monitor_clone = monitor.clone();
                let metrics = crate::QueryMetrics {
                    query_name: "list_adapters_for_tenant".to_string(),
                    execution_time_us: execution_time.as_micros() as u64,
                    rows_returned: Some(adapters.len() as i64),
                    used_index: true,
                    query_plan: Some("idx_adapters_tenant_active_tier_created".to_string()),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    tenant_id: Some(tenant_id.to_string()),
                };
                monitor_clone.record(metrics);

                // Update the monitor in the database
                if let Some(mut monitor_guard_mut) = self.performance_monitor_mut() {
                    if let Some(monitor_ref) = monitor_guard_mut.as_mut() {
                        *monitor_ref = monitor_clone;
                    }
                }
            }
        }

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
    /// FIXED: TOCTOU race condition - pin check and delete are now atomic within a transaction
    pub async fn delete_adapter(&self, id: &str) -> Result<()> {
        // Use a transaction to ensure atomicity of pin check + delete (TOCTOU fix)
        let mut tx = self
            .pool_result()?
            .begin()
            .await
            .map_err(|e| AosError::database(e.to_string()))?;

        // Get adapter_id and tenant_id for pinning check and KV dual-write
        let adapter_data: Option<(String, String)> =
            sqlx::query_as("SELECT adapter_id, tenant_id FROM adapters WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AosError::database(e.to_string()))?;

        let (adapter_id, tenant_id) = match adapter_data {
            Some((aid, tid)) => (aid, tid),
            None => {
                // Adapter doesn't exist - nothing to delete
                // Commit empty transaction (no-op but clean)
                tx.commit()
                    .await
                    .map_err(|e| AosError::database(e.to_string()))?;
                return Ok(());
            }
        };

        // Check active_pinned_adapters view (single source of truth) within same transaction
        // View automatically filters expired pins (pinned_until > now())
        let active_pin_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM active_pinned_adapters WHERE adapter_id = ?")
                .bind(&adapter_id)
                .fetch_one(&mut *tx)
                .await
                .unwrap_or(0);

        if active_pin_count > 0 {
            // Rollback transaction before returning error
            tx.rollback()
                .await
                .map_err(|e| AosError::database(e.to_string()))?;
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
        // FIXED (ADR-0023 Bug #1): Delete from KV first, then SQL to prevent race condition
        // where concurrent reads see SQL empty but KV still has stale data
        let kv_start = std::time::Instant::now();
        let kv_delete_result = if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
            repo.delete_adapter_kv(&adapter_id).await
        } else if self.dual_write_requires_strict() {
            Err(AosError::database(
                "KV delete failed (strict mode): KV backend not available".to_string(),
            ))
        } else {
            Ok(())
        };
        let kv_latency = kv_start.elapsed();

        // Handle KV delete result before SQL delete
        let kv_succeeded = match &kv_delete_result {
            Err(e) => {
                let err_msg = e.to_string();
                if self.dual_write_requires_strict() {
                    // Rollback SQL transaction on KV failure in strict mode
                    tx.rollback()
                        .await
                        .map_err(|e| AosError::database(e.to_string()))?;
                    error!(
                        error = %err_msg,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "KV delete failed before SQL delete (strict mode). Aborting to prevent inconsistency."
                    );
                    return Err(AosError::database(format!(
                        "KV delete failed (strict mode), aborting SQL delete: {err_msg}"
                    )));
                } else {
                    warn!(
                        error = %err_msg,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write",
                        "KV delete failed, continuing with SQL delete (non-strict mode)"
                    );
                }
                false
            }
            Ok(_) => true,
        };

        // Now delete from SQL within the same transaction (only if KV succeeded or non-strict mode)
        let sql_start = std::time::Instant::now();
        sqlx::query("DELETE FROM adapters WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::database(e.to_string()))?;

        // Commit the transaction (makes pin check + delete atomic)
        tx.commit()
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        let sql_latency = sql_start.elapsed();

        // Record dual-write latency lag (KV latency vs SQL latency)
        if kv_succeeded {
            let lag = if kv_latency > sql_latency {
                kv_latency.saturating_sub(sql_latency)
            } else {
                std::time::Duration::ZERO
            };
            global_kv_metrics().record_dual_write_lag(lag);
            debug!(
                adapter_id = %adapter_id,
                tenant_id = %tenant_id,
                mode = "dual-write",
                sql_latency_ms = sql_latency.as_millis() as u64,
                kv_latency_ms = kv_latency.as_millis() as u64,
                lag_ms = lag.as_millis() as u64,
                "Adapter deleted from both SQL and KV backends"
            );
        }

        // Audit log for adapter deletion
        let metadata = serde_json::json!({
            "adapter_id": adapter_id,
            "deletion_mode": "simple",
            "id": id
        });
        if let Err(e) = self
            .log_audit(
                "system",
                "system",
                &tenant_id,
                "adapter.delete_db",
                "adapter",
                Some(&adapter_id),
                "success",
                None,
                None,
                Some(&metadata.to_string()),
            )
            .await
        {
            warn!(
                adapter_id = %adapter_id,
                error = %e,
                "Failed to log adapter deletion audit (non-fatal)"
            );
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

        let mut tx = self.begin_write_tx().await?;

        // Get adapter_id and tenant_id for pinning check and KV dual-write
        let adapter_data: Option<(String, String)> =
            sqlx::query_as("SELECT adapter_id, tenant_id FROM adapters WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AosError::database(e.to_string()))?;

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
        .map_err(|e| AosError::database(e.to_string()))?;

        info!(id = %id, adapter_id = %adapter_id, "Deleting adapter with cascade");

        // FIXED (ADR-0023 Bug #1): Delete from KV first, then SQL to prevent race condition
        // where concurrent reads see SQL empty but KV still has stale data
        // Note: KV delete happens before transaction commit to ensure consistency
        let kv_start = std::time::Instant::now();
        let kv_delete_result = if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
            repo.delete_adapter_kv(&adapter_id).await
        } else {
            Ok(())
        };
        let kv_latency = kv_start.elapsed();

        // Handle KV delete result before SQL delete
        let kv_succeeded = match &kv_delete_result {
            Err(e) => {
                let err_msg = e.to_string();
                if self.dual_write_requires_strict() {
                    error!(
                        error = %err_msg,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "KV delete failed before SQL cascade delete (strict mode). Aborting to prevent inconsistency."
                    );
                    return Err(AosError::database(format!(
                        "KV delete failed (strict mode), aborting SQL cascade delete: {err_msg}"
                    )));
                } else {
                    warn!(
                        error = %err_msg,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write",
                        "KV delete failed, continuing with SQL cascade delete (non-strict mode)"
                    );
                }
                false
            }
            Ok(_) => true,
        };

        // Delete the adapter itself from SQL (only if KV succeeded or non-strict mode)
        let sql_start = std::time::Instant::now();
        sqlx::query("DELETE FROM adapters WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        let sql_latency = sql_start.elapsed();

        // Record dual-write latency lag (KV latency vs SQL latency)
        if kv_succeeded {
            let lag = if kv_latency > sql_latency {
                kv_latency.saturating_sub(sql_latency)
            } else {
                std::time::Duration::ZERO
            };
            global_kv_metrics().record_dual_write_lag(lag);
            debug!(
                adapter_id = %adapter_id,
                tenant_id = %tenant_id,
                mode = "dual-write",
                sql_latency_ms = sql_latency.as_millis() as u64,
                kv_latency_ms = kv_latency.as_millis() as u64,
                lag_ms = lag.as_millis() as u64,
                "Adapter cascade deleted from both SQL and KV backends"
            );
        }

        // Audit log for cascade adapter deletion
        let metadata = serde_json::json!({
            "adapter_id": adapter_id,
            "deletion_mode": "cascade",
            "id": id
        });
        if let Err(e) = self
            .log_audit(
                "system",
                "system",
                &tenant_id,
                "adapter.delete_db",
                "adapter",
                Some(&adapter_id),
                "success",
                None,
                None,
                Some(&metadata.to_string()),
            )
            .await
        {
            warn!(
                adapter_id = %adapter_id,
                error = %e,
                "Failed to log cascade adapter deletion audit (non-fatal)"
            );
        }

        Ok(())
    }

    /// Get adapter by ID (DEPRECATED - no tenant isolation)
    ///
    /// # Security Warning
    /// This method does NOT enforce tenant isolation. It can return adapters
    /// from ANY tenant, which is a security risk in multi-tenant environments.
    ///
    /// For tenant-scoped access, use [`get_adapter_for_tenant`] instead.
    ///
    /// # When to use this method
    /// - Internal system operations (migrations, garbage collection)
    /// - Test code where tenant context is not relevant
    /// - Admin-only operations with explicit authorization
    #[deprecated(
        since = "0.1.0",
        note = "Use get_adapter_for_tenant() for tenant-scoped access. This method lacks tenant isolation."
    )]
    pub async fn get_adapter(&self, adapter_id: &str) -> Result<Option<Adapter>> {
        deny_unscoped_adapter_query("get_adapter")?;
        // Try KV first if enabled
        if self.storage_mode().read_from_kv() {
            // SECURITY NOTE: This deprecated function uses direct tenant lookup for KV routing.
            // This is acceptable since the entire function is deprecated and for admin-only use.
            // Use a direct query to get tenant_id for KV lookup (admin path only).
            let tenant_result: Option<String> =
                sqlx::query_scalar("SELECT tenant_id FROM adapters WHERE adapter_id = ?")
                    .bind(adapter_id)
                    .fetch_optional(self.pool_result()?)
                    .await
                    .map_err(|e| AosError::database(e.to_string()))?;

            if let Some(tenant_id) = tenant_result {
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
            .fetch_optional(self.pool_result()?)
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        Ok(adapter)
    }

    /// Get adapter by ID scoped to a tenant (returns None on tenant mismatch).
    pub async fn get_adapter_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<Option<Adapter>> {
        // Try KV first if enabled (tenant-scoped repo avoids cross-tenant leakage)
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                match repo.get_adapter_kv(adapter_id).await {
                    Ok(Some(adapter)) => {
                        let content_hash_missing = adapter
                            .content_hash_b3
                            .as_ref()
                            .map(|h| h.trim().is_empty())
                            .unwrap_or(true);
                        let manifest_hash_missing = adapter
                            .manifest_hash
                            .as_ref()
                            .map(|h| h.trim().is_empty())
                            .unwrap_or(true);

                        if self.storage_mode().sql_fallback_enabled()
                            && (content_hash_missing || manifest_hash_missing)
                        {
                            self.record_kv_read_fallback(
                                "adapters.get_for_tenant.stale_hash_fields",
                            );
                            debug!(
                                adapter_id = %adapter_id,
                                tenant_id = %tenant_id,
                                mode = "kv-fallback",
                                content_hash_missing,
                                manifest_hash_missing,
                                "KV adapter missing hash fields, falling back to SQL (tenant-scoped)"
                            );
                        } else {
                            debug!(
                                adapter_id = %adapter_id,
                                tenant_id = %tenant_id,
                                mode = "kv-primary",
                                "Retrieved adapter from KV (tenant-scoped)"
                            );
                            return Ok(Some(adapter));
                        }
                    }
                    Ok(None) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("adapters.get_for_tenant.none");
                        debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-fallback", "KV returned None, falling back to SQL (tenant-scoped)");
                    }
                    Ok(None) => {
                        return Ok(None);
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("adapters.get_for_tenant.error");
                        warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-fallback", "KV read failed, falling back to SQL (tenant-scoped)");
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }

        // SQL fallback or primary read (tenant-scoped; supports adapter_id or internal id)
        let query = format!(
            "SELECT {} FROM adapters WHERE tenant_id = ? AND (adapter_id = ? OR id = ?) LIMIT 2",
            ADAPTER_SELECT_FIELDS
        );

        // Performance monitoring for tenant-scoped queries
        let start_time = std::time::Instant::now();
        let mut adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(tenant_id)
            .bind(adapter_id)
            .bind(adapter_id)
            .fetch_all(self.pool_result()?)
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        let execution_time = start_time.elapsed();

        // Record performance metrics if monitoring is enabled
        if let Some(monitor_guard) = self.performance_monitor() {
            if let Some(monitor) = monitor_guard.as_ref() {
                let mut monitor_clone = monitor.clone();
                let metrics = crate::QueryMetrics {
                    query_name: "get_adapter_for_tenant".to_string(),
                    execution_time_us: execution_time.as_micros() as u64,
                    rows_returned: Some(adapters.len() as i64),
                    used_index: true, // Should use composite index from migration 0210
                    query_plan: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    tenant_id: Some(tenant_id.to_string()),
                };
                monitor_clone.record(metrics);

                // Update the monitor in the database
                if let Some(mut monitor_guard_mut) = self.performance_monitor_mut() {
                    if let Some(monitor_ref) = monitor_guard_mut.as_mut() {
                        *monitor_ref = monitor_clone;
                    }
                }
            }
        }

        match adapters.len() {
            0 => Ok(None),
            1 => Ok(Some(adapters.remove(0))),
            _ => Err(AosError::validation(format!(
                "Ambiguous adapter_id '{}' for tenant '{}'",
                adapter_id, tenant_id
            ))),
        }
    }

    /// Get adapter by immutable `(repo_id, adapter_version_id)` for a tenant.
    ///
    /// Returns `None` when no adapter is linked to the provided version pair.
    pub async fn get_adapter_for_tenant_repo_version(
        &self,
        tenant_id: &str,
        repo_id: &str,
        adapter_version_id: &str,
    ) -> Result<Option<Adapter>> {
        let query = format!(
            "SELECT {} FROM adapters WHERE tenant_id = ? AND repo_id = ? AND adapter_version_id = ? LIMIT 2",
            ADAPTER_SELECT_FIELDS
        );

        let mut adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(tenant_id)
            .bind(repo_id)
            .bind(adapter_version_id)
            .fetch_all(self.pool_result()?)
            .await
            .map_err(|e| AosError::database(e.to_string()))?;

        match adapters.len() {
            0 => Ok(None),
            1 => Ok(Some(adapters.remove(0))),
            _ => Err(AosError::validation(format!(
                "Ambiguous adapter version binding '{}'@'{}' for tenant '{}'",
                repo_id, adapter_version_id, tenant_id
            ))),
        }
    }

    /// Find adapter by BLAKE3 hash for deduplication
    ///
    /// Returns an existing active adapter with the same hash_b3 within the specified tenant.
    ///
    /// # Security
    /// This method REQUIRES a tenant context to prevent cross-tenant hash discovery attacks.
    /// An attacker could otherwise probe for adapter existence across tenants by testing hashes.
    ///
    /// # Arguments
    /// * `hash_b3` - The BLAKE3 hash to search for
    /// * `tenant_hint` - REQUIRED tenant context for security isolation
    ///
    /// # Errors
    /// Returns an error if `tenant_hint` is None (security isolation requirement).
    pub async fn find_adapter_by_hash(
        &self,
        hash_b3: &str,
        tenant_hint: Option<&str>,
    ) -> Result<Option<Adapter>> {
        // SECURITY: Require tenant context to prevent cross-tenant hash discovery
        let tenant_id = match tenant_hint {
            Some(tid) => tid,
            None => {
                error!(
                    hash = %hash_b3,
                    "Hash lookup attempted without tenant context - rejecting for security isolation"
                );
                return Err(AosError::validation(
                    "Hash lookup requires tenant context (security isolation)".to_string(),
                ));
            }
        };

        // Delegate to tenant-scoped lookup which enforces proper isolation
        self.find_adapter_by_hash_for_tenant(tenant_id, hash_b3)
            .await
    }

    /// Find adapter by hash within a specific tenant (secure version)
    ///
    /// This is the recommended method for tenant-scoped hash lookups to prevent
    /// cross-tenant adapter discovery via hash collision.
    pub async fn find_adapter_by_hash_for_tenant(
        &self,
        tenant_id: &str,
        hash_b3: &str,
    ) -> Result<Option<Adapter>> {
        // Try KV first if enabled
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                match repo.find_adapter_by_hash_kv(hash_b3).await {
                    Ok(Some(adapter)) => {
                        debug!(tenant_id = %tenant_id, hash = %hash_b3, mode = "kv-primary", "Found adapter by hash in KV");
                        return Ok(Some(adapter));
                    }
                    Ok(None) if self.storage_mode().sql_fallback_enabled() => {
                        debug!(tenant_id = %tenant_id, hash = %hash_b3, mode = "kv-fallback", "Hash not found in KV, falling back to SQL");
                    }
                    Ok(None) => return Ok(None),
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        warn!(error = %e, tenant_id = %tenant_id, hash = %hash_b3, mode = "kv-fallback", "KV lookup failed, falling back to SQL");
                    }
                    Err(e) => return Err(AosError::database(format!("KV lookup failed: {}", e))),
                }
            }
        }

        let start_time = std::time::Instant::now();
        // Updated to use the covering index from migration 0210
        let query = format!(
            "SELECT {} FROM adapters INDEXED BY idx_adapters_tenant_hash_active_covering WHERE tenant_id = ? AND hash_b3 = ? AND active = 1 AND lifecycle_state != 'purged' LIMIT 1",
            ADAPTER_SELECT_FIELDS
        );
        let adapter: Option<Adapter> = sqlx::query_as::<_, Adapter>(&query)
            .bind(tenant_id)
            .bind(hash_b3)
            .fetch_optional(self.pool_result()?)
            .await
            .map_err(|e| {
                AosError::database(format!("Failed to find adapter by hash for tenant: {}", e))
            })?;
        let execution_time = start_time.elapsed();

        // Performance monitoring
        if let Some(monitor_guard) = self.performance_monitor() {
            if let Some(monitor) = monitor_guard.as_ref() {
                let mut monitor_clone = monitor.clone();
                let metrics = crate::QueryMetrics {
                    query_name: "find_adapter_by_hash_for_tenant".to_string(),
                    execution_time_us: execution_time.as_micros() as u64,
                    rows_returned: Some(if adapter.is_some() { 1 } else { 0 }),
                    used_index: true,
                    query_plan: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    tenant_id: Some(tenant_id.to_string()),
                };
                monitor_clone.record(metrics);

                if let Some(mut monitor_guard_mut) = self.performance_monitor_mut() {
                    if let Some(monitor_ref) = monitor_guard_mut.as_mut() {
                        *monitor_ref = monitor_clone;
                    }
                }
            }
        }

        Ok(adapter)
    }

    /// Find adapter by content hash (BLAKE3 of manifest + weights)
    ///
    /// Used for deduplication during registration - if an adapter with the same
    /// content hash already exists, we return the existing adapter instead of
    /// creating a duplicate.
    pub async fn find_adapter_by_content_hash(
        &self,
        content_hash_b3: &str,
    ) -> Result<Option<Adapter>> {
        // Try KV first if enabled (global lookup - no tenant scoping)
        if self.storage_mode().read_from_kv() {
            if let Some(kv) = self.kv_backend() {
                let repo = AdapterRepository::new(kv.backend().clone(), kv.index_manager().clone());
                match repo.find_by_content_hash(content_hash_b3).await {
                    Ok(Some(adapter_kv)) => {
                        let adapter: Adapter = adapter_kv.into();
                        // Filter for active adapters only (matching SQL behavior)
                        if adapter.active == 1 {
                            debug!(content_hash_b3 = %content_hash_b3, mode = "kv-primary", "Found adapter by content hash in KV");
                            return Ok(Some(adapter));
                        }
                        // Adapter exists but not active, fall through to SQL if enabled
                        if !self.storage_mode().sql_fallback_enabled() {
                            return Ok(None);
                        }
                    }
                    Ok(None) => {
                        // Not found in KV, fall through to SQL if enabled
                        if !self.storage_mode().sql_fallback_enabled() {
                            return Ok(None);
                        }
                    }
                    Err(e) => {
                        if self.storage_mode().sql_fallback_enabled() {
                            debug!(
                                content_hash_b3 = %content_hash_b3,
                                error = %e,
                                "KV lookup failed, falling back to SQL"
                            );
                        } else {
                            return Err(AosError::Database(format!(
                                "Failed to find adapter by content hash: {}",
                                e
                            )));
                        }
                    }
                }
            }
        }

        // SQL lookup using the unique index on content_hash_b3 (from migration 0153)
        let query = format!(
            "SELECT {} FROM adapters WHERE content_hash_b3 = ? AND active = 1 LIMIT 1",
            ADAPTER_SELECT_FIELDS
        );
        let adapter = sqlx::query_as::<_, Adapter>(&query)
            .bind(content_hash_b3)
            .fetch_optional(self.pool_result()?)
            .await
            .map_err(|e| {
                AosError::database(format!("Failed to find adapter by content hash: {}", e))
            })?;

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
        let id = new_id(IdPrefix::Adp);
        sqlx::query(
            "INSERT INTO adapter_activations (id, adapter_id, request_id, gate_value, selected) 
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(adapter_id)
        .bind(request_id)
        .bind(gate_value)
        .bind(if selected { 1 } else { 0 })
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;
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
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;
        Ok(activations)
    }

    /// Get adapter activation stats
    pub async fn get_adapter_stats(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<(i64, i64, f64)> {
        let row = sqlx::query(
            "SELECT 
                COUNT(aa.id) as total,
                SUM(aa.selected) as selected_count,
                AVG(aa.gate_value) as avg_gate
             FROM adapter_activations aa
             JOIN adapters a ON aa.adapter_id = a.id
             WHERE a.tenant_id = ? AND (a.adapter_id = ? OR a.id = ?)",
        )
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(adapter_id)
        .fetch_one(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        let total: i64 = row
            .try_get("total")
            .map_err(|e| AosError::database(e.to_string()))?;
        let selected: i64 = row.try_get("selected_count").unwrap_or(0);
        let avg_gate: f64 = row.try_get("avg_gate").unwrap_or(0.0);

        Ok((total, selected, avg_gate))
    }

    /// Get adapter latency stats from performance summary table
    /// Returns (avg_latency_ms, p95_latency_ms, p99_latency_ms) or None if no data
    pub async fn get_adapter_latency_stats(
        &self,
        adapter_id: &str,
    ) -> Result<Option<(f64, f64, f64)>> {
        let row = sqlx::query(
            r#"SELECT
                COALESCE(avg_latency_us, 0) / 1000.0 as avg_ms,
                COALESCE(p95_latency_us, 0) / 1000.0 as p95_ms,
                COALESCE(p99_latency_us, 0) / 1000.0 as p99_ms
            FROM adapter_performance_summary
            WHERE adapter_id = ?
            ORDER BY window_end DESC
            LIMIT 1"#,
        )
        .bind(adapter_id)
        .fetch_optional(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        match row {
            Some(r) => {
                let avg: f64 = r.try_get("avg_ms").unwrap_or(0.0);
                let p95: f64 = r.try_get("p95_ms").unwrap_or(0.0);
                let p99: f64 = r.try_get("p99_ms").unwrap_or(0.0);
                Ok(Some((avg, p95, p99)))
            }
            None => Ok(None),
        }
    }

    /// Get adapter memory usage from performance metrics (last hour average)
    /// Returns memory usage in MB or None if no data
    pub async fn get_adapter_memory_usage(&self, adapter_id: &str) -> Result<Option<f64>> {
        let row = sqlx::query_scalar::<_, f64>(
            r#"SELECT AVG(memory_used_bytes) / 1024.0 / 1024.0 as memory_mb
            FROM adapter_performance_metrics
            WHERE adapter_id = ? AND recorded_at > datetime('now', '-1 hour')"#,
        )
        .bind(adapter_id)
        .fetch_optional(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        Ok(row)
    }

    /// Update adapter state
    pub async fn update_adapter_state(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        state: &str,
        reason: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE adapters SET current_state = ?, updated_at = datetime('now') WHERE tenant_id = ? AND (adapter_id = ? OR id = ?)"
        )
        .bind(state)
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(adapter_id)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
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
                    return Err(AosError::database(format!(
                        "State update succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to update adapter state in KV backend");
                }
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, state = %state, mode = "dual-write", "Adapter state updated in both SQL and KV backends");
            }
        }

        Ok(())
    }

    // Pin/unpin functionality moved to pinned_adapters.rs

    /// Update adapter memory usage
    pub async fn update_adapter_memory(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        memory_bytes: i64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE adapters SET memory_bytes = ?, updated_at = datetime('now') WHERE tenant_id = ? AND (adapter_id = ? OR id = ?)"
        )
        .bind(memory_bytes)
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(adapter_id)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
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
                    return Err(AosError::database(format!(
                        "Memory update succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to update adapter memory in KV backend");
                }
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, memory_bytes = %memory_bytes, mode = "dual-write", "Adapter memory updated in both SQL and KV backends");
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
    pub(crate) async fn update_adapter_state_tx(
        &self,
        adapter_id: &str,
        state: &str,
        reason: &str,
    ) -> Result<()> {
        let mut tx = self.begin_write_tx().await?;

        // Lock the row and get tenant_id for KV dual-write
        let row_data: Option<(String, String)> =
            sqlx::query_as("SELECT adapter_id, tenant_id FROM adapters WHERE adapter_id = ?")
                .bind(adapter_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AosError::database(e.to_string()))?;

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
        .map_err(|e| AosError::database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::database(e.to_string()))?;

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
                    return Err(AosError::database(format!(
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
    pub(crate) async fn update_adapter_state_cas(
        &self,
        adapter_id: &str,
        expected_state: &str,
        new_state: &str,
        reason: &str,
    ) -> Result<bool> {
        let mut tx = self.begin_write_tx().await?;

        // Lock the row and verify current state
        let row_data: Option<(String, String, String)> = sqlx::query_as(
            "SELECT adapter_id, tenant_id, current_state FROM adapters WHERE adapter_id = ?",
        )
        .bind(adapter_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

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
        .map_err(|e| AosError::database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::database(e.to_string()))?;

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
                    return Err(AosError::database(format!(
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
        let mut tx = self.begin_write_tx().await?;

        // Verify adapter exists and get tenant_id for KV dual-write
        let row_data: Option<(String, String)> =
            sqlx::query_as("SELECT adapter_id, tenant_id FROM adapters WHERE adapter_id = ?")
                .bind(adapter_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AosError::database(e.to_string()))?;

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
        .map_err(|e| AosError::database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::database(e.to_string()))?;

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
                    return Err(AosError::database(format!(
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
    pub(crate) async fn update_adapter_state_and_memory(
        &self,
        adapter_id: &str,
        state: &str,
        memory_bytes: i64,
        reason: &str,
    ) -> Result<()> {
        let mut tx = self.begin_write_tx().await?;

        // Verify adapter exists and get tenant_id for KV dual-write
        let row_data: Option<(String, String)> =
            sqlx::query_as("SELECT adapter_id, tenant_id FROM adapters WHERE adapter_id = ?")
                .bind(adapter_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AosError::database(e.to_string()))?;

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
        .map_err(|e| AosError::database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::database(e.to_string()))?;

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
                    return Err(AosError::database(format!(
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
    pub async fn list_adapters_by_category(
        &self,
        tenant_id: &str,
        category: &str,
    ) -> Result<Vec<Adapter>> {
        // Try KV first if enabled
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                match repo.list_adapters_by_category_kv(tenant_id, category).await {
                    Ok(adapters) if !adapters.is_empty() => {
                        debug!(tenant_id = %tenant_id, category = %category, count = adapters.len(), mode = "kv-primary", "Retrieved adapters by category from KV");
                        return Ok(adapters);
                    }
                    Ok(_) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("adapters.list_by_category.empty");
                        debug!(tenant_id = %tenant_id, category = %category, mode = "kv-fallback", "KV returned empty list, falling back to SQL");
                    }
                    Ok(adapters) => {
                        return Ok(adapters);
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("adapters.list_by_category.error");
                        warn!(error = %e, tenant_id = %tenant_id, category = %category, mode = "kv-fallback", "KV read failed, falling back to SQL");
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }

        // SQL fallback or primary read
        let query = format!(
            "SELECT {} FROM adapters WHERE tenant_id = ? AND active = 1 AND category = ? ORDER BY activation_count DESC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(tenant_id)
            .bind(category)
            .fetch_all(self.pool_result()?)
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        Ok(adapters)
    }

    /// List adapters by scope (tenant-scoped)
    pub async fn list_adapters_by_scope(
        &self,
        tenant_id: &str,
        scope: &str,
    ) -> Result<Vec<Adapter>> {
        // Try KV first if enabled
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                match repo.list_adapters_by_scope_kv(tenant_id, scope).await {
                    Ok(adapters) if !adapters.is_empty() => {
                        debug!(tenant_id = %tenant_id, scope = %scope, count = adapters.len(), mode = "kv-primary", "Retrieved adapters by scope from KV");
                        return Ok(adapters);
                    }
                    Ok(_) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("adapters.list_by_scope.empty");
                        debug!(tenant_id = %tenant_id, scope = %scope, mode = "kv-fallback", "KV returned empty list, falling back to SQL");
                    }
                    Ok(adapters) => {
                        return Ok(adapters);
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("adapters.list_by_scope.error");
                        warn!(error = %e, tenant_id = %tenant_id, scope = %scope, mode = "kv-fallback", "KV read failed, falling back to SQL");
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }

        // SQL fallback or primary read (tenant-scoped)
        let query = format!(
            "SELECT {} FROM adapters WHERE tenant_id = ? AND active = 1 AND scope = ? ORDER BY activation_count DESC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(tenant_id)
            .bind(scope)
            .fetch_all(self.pool_result()?)
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        Ok(adapters)
    }

    /// List adapters by state (tenant-scoped)
    ///
    /// Uses the tenant-scoped state index (BY_TENANT_STATE) in KV mode for efficient
    /// O(1) lookups that avoid loading all adapters for a state across all tenants.
    pub async fn list_adapters_by_state(
        &self,
        tenant_id: &str,
        state: &str,
    ) -> Result<Vec<Adapter>> {
        // Try KV first if enabled - uses the tenant-scoped state index
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                match repo.list_adapters_by_state_kv(tenant_id, state).await {
                    Ok(adapters) if !adapters.is_empty() => {
                        debug!(
                            tenant_id = %tenant_id,
                            state = %state,
                            count = adapters.len(),
                            mode = "kv-primary",
                            "Retrieved adapters by state from KV (tenant-scoped index)"
                        );
                        return Ok(adapters);
                    }
                    Ok(_) => {
                        // Empty result from KV, fall through to SQL
                        debug!(
                            tenant_id = %tenant_id,
                            state = %state,
                            mode = "kv-empty",
                            "No adapters found in KV for state, checking SQL"
                        );
                    }
                    Err(e) => {
                        // KV read failed, fall through to SQL
                        warn!(
                            error = %e,
                            tenant_id = %tenant_id,
                            state = %state,
                            mode = "kv-fallback",
                            "KV state lookup failed, falling back to SQL"
                        );
                    }
                }
            }
        }

        // SQL fallback or primary read
        let query = format!(
            "SELECT {} FROM adapters WHERE tenant_id = ? AND active = 1 AND current_state = ? ORDER BY activation_count DESC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(tenant_id)
            .bind(state)
            .fetch_all(self.pool_result()?)
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        Ok(adapters)
    }

    /// Get adapter state summary
    pub async fn get_adapter_state_summary(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<(String, String, String, i64, i64, f64, Option<String>)>> {
        let summary = sqlx::query(
            "SELECT category, scope, current_state, COUNT(*) as count,
                    SUM(memory_bytes) as total_memory_bytes,
                    AVG(activation_count) as avg_activations,
                    MAX(last_activated) as most_recent_activation
             FROM adapters
             WHERE active = 1
               AND tenant_id = ?
             GROUP BY category, scope, current_state
             ORDER BY category, scope, current_state",
        )
        .bind(tenant_id)
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        let mut result = Vec::new();
        for row in summary {
            let category: String = row
                .try_get("category")
                .map_err(|e| AosError::database(e.to_string()))?;
            let scope: String = row
                .try_get("scope")
                .map_err(|e| AosError::database(e.to_string()))?;
            let state: String = row
                .try_get("current_state")
                .map_err(|e| AosError::database(e.to_string()))?;
            let count: i64 = row
                .try_get("count")
                .map_err(|e| AosError::database(e.to_string()))?;
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
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant context (required for security isolation)
    /// * `adapter_id` - The adapter's external ID
    pub async fn get_adapter_lineage(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<Vec<Adapter>> {
        // Try KV first if enabled
        if self.storage_mode().read_from_kv()
            && self
                .get_adapter_tenant_id(adapter_id, tenant_id)
                .await?
                .is_some()
        {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
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
            .fetch_all(self.pool_result()?)
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        Ok(adapters)
    }

    /// Get direct children of an adapter
    ///
    /// Returns all adapters that have this adapter as their parent_id.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant context (required for security isolation)
    /// * `adapter_id` - The adapter's external ID
    pub async fn get_adapter_children(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<Vec<Adapter>> {
        // Try KV first if enabled
        if self.storage_mode().read_from_kv()
            && self
                .get_adapter_tenant_id(adapter_id, tenant_id)
                .await?
                .is_some()
        {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
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

        // SQL fallback or primary read
        let query = format!(
            "SELECT {} FROM adapters WHERE parent_id = ? AND active = 1 ORDER BY revision ASC, created_at ASC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(adapter_id)
            .fetch_all(self.pool_result()?)
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
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
            .fetch_all(self.pool_result()?)
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
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
        .fetch_optional(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

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
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

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

        let min_rev = *revisions.iter().min().unwrap_or(&0);
        let max_rev = *revisions.iter().max().unwrap_or(&0);
        let gap = max_rev - min_rev;

        if gap > max_gap {
            return Err(AosError::validation(format!(
                "Revision gap ({}) exceeds maximum allowed ({}) for adapter family {}/{}/{}",
                gap, max_gap, tenant_namespace, domain, purpose
            )));
        }

        Ok(())
    }

    /// Update adapter tier
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant context (required for security isolation)
    /// * `adapter_id` - The adapter's external ID
    /// * `tier` - The new tier value
    pub async fn update_adapter_tier(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        tier: &str,
    ) -> Result<()> {
        // SECURITY: Update only within tenant scope
        sqlx::query(
            "UPDATE adapters SET tier = ?, updated_at = datetime('now') WHERE adapter_id = ? AND tenant_id = ?",
        )
        .bind(tier)
        .bind(adapter_id)
        .bind(tenant_id)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(format!("Failed to update adapter tier: {}", e)))?;

        // KV write (dual-write mode) - tenant verified via parameter
        if self
            .get_adapter_tenant_id(adapter_id, tenant_id)
            .await?
            .is_some()
        {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                if let Err(e) = repo.update_adapter_tier_kv(adapter_id, tier).await {
                    if self.dual_write_requires_strict() {
                        error!(
                            error = %e,
                            adapter_id = %adapter_id,
                            tenant_id = %tenant_id,
                            mode = "dual-write-strict",
                            "CONSISTENCY WARNING: SQL tier update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                        );
                        return Err(AosError::database(format!(
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

    /// Update runtime LoRA strength multiplier
    pub async fn update_adapter_strength(
        &self,
        adapter_id: &str,
        lora_strength: f32,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE adapters SET lora_strength = ?, updated_at = datetime('now') WHERE adapter_id = ?",
        )
        .bind(lora_strength)
        .bind(adapter_id)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(format!("Failed to update adapter strength: {}", e)))?;

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
            .fetch_optional(self.pool_result()?)
            .await
            .map_err(|e| AosError::database(e.to_string()))?
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
                    lora_strength: adapter.lora_strength,
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
                    recommended_for_moe: adapter.recommended_for_moe,
                    manifest_schema_version: adapter.manifest_schema_version.clone(),
                    // Use existing content_hash_b3 or fall back to hash_b3 for legacy adapters
                    content_hash_b3: adapter
                        .content_hash_b3
                        .clone()
                        .unwrap_or_else(|| adapter.hash_b3.clone()),
                    provenance_json: adapter.provenance_json.clone(),
                    metadata_json: adapter.metadata_json.clone(),
                    repo_path: adapter.repo_path.clone(),
                    // These fields may not exist on legacy adapters
                    codebase_scope: adapter.codebase_scope.clone(),
                    dataset_version_id: adapter.dataset_version_id.clone(),
                    registration_timestamp: adapter.registration_timestamp.clone(),
                    manifest_hash: adapter.manifest_hash.clone(),
                    // Codebase adapter type and stream binding
                    adapter_type: adapter.adapter_type.clone(),
                    base_adapter_id: adapter.base_adapter_id.clone(),
                    stream_session_id: adapter.stream_session_id.clone(),
                    versioning_threshold: adapter.versioning_threshold,
                    coreml_package_hash: adapter.coreml_package_hash.clone(),
                    training_dataset_hash_b3: adapter.training_dataset_hash_b3.clone(),
                };

                // Delete old KV entry then re-register and sync state/memory
                let _ = repo.delete_adapter_kv(adapter_id).await;
                repo.register_adapter_kv(params)
                    .await
                    .map_err(|e| AosError::database(format!("Failed to repair KV entry: {}", e)))?;
                repo.update_adapter_state_kv(
                    adapter_id,
                    &adapter.current_state,
                    "consistency_repair",
                )
                .await
                .map_err(|e| AosError::database(format!("Failed to repair KV state: {}", e)))?;
                repo.update_adapter_memory_kv(adapter_id, adapter.memory_bytes)
                    .await
                    .map_err(|e| {
                        AosError::database(format!("Failed to repair KV memory: {}", e))
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
                    lora_strength: adapter.lora_strength,
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
                    recommended_for_moe: adapter.recommended_for_moe,
                    manifest_schema_version: adapter.manifest_schema_version.clone(),
                    // Use existing content_hash_b3 or fall back to hash_b3 for legacy adapters
                    content_hash_b3: adapter
                        .content_hash_b3
                        .clone()
                        .unwrap_or_else(|| adapter.hash_b3.clone()),
                    provenance_json: adapter.provenance_json.clone(),
                    metadata_json: adapter.metadata_json.clone(),
                    repo_path: adapter.repo_path.clone(),
                    // These fields may not exist on legacy adapters
                    codebase_scope: adapter.codebase_scope.clone(),
                    dataset_version_id: adapter.dataset_version_id.clone(),
                    registration_timestamp: adapter.registration_timestamp.clone(),
                    manifest_hash: adapter.manifest_hash.clone(),
                    // Codebase adapter type and stream binding
                    adapter_type: adapter.adapter_type.clone(),
                    base_adapter_id: adapter.base_adapter_id.clone(),
                    stream_session_id: adapter.stream_session_id.clone(),
                    versioning_threshold: adapter.versioning_threshold,
                    coreml_package_hash: adapter.coreml_package_hash.clone(),
                    training_dataset_hash_b3: adapter.training_dataset_hash_b3.clone(),
                };

                repo.register_adapter_kv(params).await.map_err(|e| {
                    AosError::database(format!("Failed to create adapter in KV: {}", e))
                })?;
                repo.update_adapter_state_kv(
                    adapter_id,
                    &adapter.current_state,
                    "consistency_repair",
                )
                .await
                .map_err(|e| AosError::database(format!("Failed to sync state to KV: {}", e)))?;
                repo.update_adapter_memory_kv(adapter_id, adapter.memory_bytes)
                    .await
                    .map_err(|e| {
                        AosError::database(format!("Failed to sync memory to KV: {}", e))
                    })?;

                Ok(true)
            }
            Err(e) => Err(AosError::database(format!(
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

    /// Clean up orphaned adapters in KV that don't exist in SQL.
    ///
    /// During dual-write mode, inconsistencies can occur where an adapter
    /// exists in KV but not in SQL (e.g., from failed rollbacks, interrupted
    /// operations, or bugs). This method finds and removes such orphans.
    ///
    /// # Algorithm
    /// 1. List all adapter IDs from SQL for the tenant
    /// 2. List all adapter IDs from KV for the tenant
    /// 3. Find KV entries that don't exist in SQL
    /// 4. Delete each orphaned KV entry
    ///
    /// # Returns
    /// Count of orphaned KV entries that were deleted
    ///
    /// # Safety
    /// This operation is safe because SQL is the source of truth during
    /// the migration period. Any adapter in KV that doesn't exist in SQL
    /// is definitionally orphaned and should be cleaned up.
    pub async fn cleanup_orphaned_adapters(&self, tenant_id: &str) -> Result<u64> {
        // Get adapter IDs from SQL
        let sql_adapters = self.list_adapters_for_tenant(tenant_id).await?;
        let sql_ids: std::collections::HashSet<String> = sql_adapters
            .iter()
            .filter_map(|a| a.adapter_id.clone())
            .collect();

        // Get adapter IDs from KV
        let kv_repo = match self.get_adapter_kv_repo(tenant_id) {
            Some(repo) => repo,
            None => {
                // No KV repo configured, nothing to clean up
                return Ok(0);
            }
        };

        let kv_adapters = kv_repo
            .list_adapters_for_tenant_kv(tenant_id, None, None)
            .await?;
        let kv_ids: std::collections::HashSet<String> = kv_adapters
            .iter()
            .filter_map(|a| a.adapter_id.clone())
            .collect();

        // Find orphans: KV entries that don't exist in SQL
        let orphans: Vec<String> = kv_ids.difference(&sql_ids).cloned().collect();

        if orphans.is_empty() {
            debug!(
                tenant_id = %tenant_id,
                sql_count = sql_ids.len(),
                kv_count = kv_ids.len(),
                "No orphaned adapters found in KV"
            );
            return Ok(0);
        }

        info!(
            tenant_id = %tenant_id,
            orphan_count = orphans.len(),
            "Found orphaned adapters in KV, cleaning up"
        );

        let mut deleted = 0u64;
        for orphan_id in &orphans {
            match kv_repo.delete_adapter_kv(orphan_id).await {
                Ok(()) => {
                    deleted += 1;
                    debug!(
                        adapter_id = %orphan_id,
                        tenant_id = %tenant_id,
                        "Deleted orphaned adapter from KV"
                    );
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        adapter_id = %orphan_id,
                        tenant_id = %tenant_id,
                        "Failed to delete orphaned adapter from KV"
                    );
                }
            }
        }

        info!(
            tenant_id = %tenant_id,
            deleted = deleted,
            total_orphans = orphans.len(),
            "Orphan cleanup complete"
        );

        Ok(deleted)
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
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(format!("Failed to query adapters: {}", e)))?;

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
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(format!("Failed to archive adapters: {}", e)))?;

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
                // FIXED: Propagate error in strict mode instead of just logging
                if self.dual_write_requires_strict() {
                    error!(
                        tenant_id = %tenant_id,
                        success_count = kv_success_count,
                        error_count = kv_error_count,
                        mode = "dual-write-strict",
                        "KV archive failures in strict mode - SQL already committed, returning error"
                    );
                    return Err(AosError::database(format!(
                        "Strict dual-write failure: {kv_error_count} of {} adapters failed KV archive for tenant '{tenant_id}'. \
                         SQL writes committed; KV is inconsistent. Manual reconciliation required.",
                        affected_adapter_ids.len()
                    )));
                }
                warn!(
                    tenant_id = %tenant_id,
                    success_count = kv_success_count,
                    error_count = kv_error_count,
                    mode = "dual-write",
                    "Partial KV archive failure for tenant adapters (non-strict mode)"
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
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant context (required for security isolation)
    /// * `adapter_id` - The adapter's external ID
    /// * `archived_by` - Who is archiving
    /// * `reason` - Reason for archiving
    pub async fn archive_adapter(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        archived_by: &str,
        reason: &str,
    ) -> Result<()> {
        // SECURITY: Update only within tenant scope
        let affected = sqlx::query(
            "UPDATE adapters
             SET archived_at = datetime('now'),
                 archived_by = ?,
                 archive_reason = ?,
                 updated_at = datetime('now')
             WHERE adapter_id = ?
               AND tenant_id = ?
               AND archived_at IS NULL",
        )
        .bind(archived_by)
        .bind(reason)
        .bind(adapter_id)
        .bind(tenant_id)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(format!("Failed to archive adapter: {}", e)))?
        .rows_affected();

        if affected == 0 {
            return Err(AosError::NotFound(format!(
                "Adapter not found or already archived: {}",
                adapter_id
            )));
        }

        info!(adapter_id = %adapter_id, archived_by = %archived_by, "Archived adapter");

        // KV write (dual-write mode) - tenant verified via parameter
        if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
            if let Err(e) = repo
                .archive_adapter_kv(adapter_id, archived_by, reason)
                .await
            {
                warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to archive adapter in KV backend");
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Adapter archived in both SQL and KV backends");
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
            .fetch_all(self.pool_result()?)
            .await
            .map_err(|e| AosError::database(format!("Failed to find GC candidates: {}", e)))?;

        Ok(adapters)
    }

    /// Find adapters with missing content_hash_b3 or manifest_hash
    ///
    /// Returns adapters that need hash repair for preflight validation.
    /// Only includes adapters with a valid `.aos` file path that can be
    /// used to recompute the missing hashes.
    ///
    /// # Hash Repair Context
    ///
    /// As of preflight hardening, adapters require both `content_hash_b3` and
    /// `manifest_hash` to pass alias swap preflight checks. Older adapters
    /// registered before these fields were mandatory may be missing one or both.
    ///
    /// This query identifies repair candidates:
    /// - `content_hash_b3` is NULL or empty
    /// - OR `manifest_hash` is NULL or empty
    /// - AND `aos_file_path` is present (needed to recompute hashes)
    /// - AND adapter is not archived/purged (active or ready adapters only)
    ///
    /// # Arguments
    /// * `tenant_id` - Optional tenant filter; if None, queries all tenants
    /// * `limit` - Maximum number of adapters to return
    ///
    /// # Returns
    /// Adapters eligible for hash repair, ordered by created_at ascending.
    pub async fn find_adapters_with_missing_hashes(
        &self,
        tenant_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Adapter>> {
        let query = match tenant_id {
            Some(_) => format!(
                "SELECT {} FROM adapters
                 WHERE tenant_id = ?
                   AND aos_file_path IS NOT NULL
                   AND aos_file_path != ''
                   AND archived_at IS NULL
                   AND purged_at IS NULL
                   AND (
                       content_hash_b3 IS NULL
                       OR content_hash_b3 = ''
                       OR manifest_hash IS NULL
                       OR manifest_hash = ''
                   )
                 ORDER BY created_at ASC
                 LIMIT ?",
                ADAPTER_SELECT_FIELDS
            ),
            None => format!(
                "SELECT {} FROM adapters
                 WHERE aos_file_path IS NOT NULL
                   AND aos_file_path != ''
                   AND archived_at IS NULL
                   AND purged_at IS NULL
                   AND (
                       content_hash_b3 IS NULL
                       OR content_hash_b3 = ''
                       OR manifest_hash IS NULL
                       OR manifest_hash = ''
                   )
                 ORDER BY created_at ASC
                 LIMIT ?",
                ADAPTER_SELECT_FIELDS
            ),
        };

        let adapters = match tenant_id {
            Some(tid) => sqlx::query_as::<_, Adapter>(&query)
                .bind(tid)
                .bind(limit)
                .fetch_all(self.pool_result()?)
                .await
                .map_err(|e| {
                    AosError::database(format!(
                        "Failed to find adapters with missing hashes: {}",
                        e
                    ))
                })?,
            None => sqlx::query_as::<_, Adapter>(&query)
                .bind(limit)
                .fetch_all(self.pool_result()?)
                .await
                .map_err(|e| {
                    AosError::database(format!(
                        "Failed to find adapters with missing hashes: {}",
                        e
                    ))
                })?,
        };

        Ok(adapters)
    }

    /// Mark an adapter as purged after .aos file deletion
    ///
    /// Sets `purged_at` timestamp and clears `aos_file_path`.
    /// The record is preserved for audit purposes.
    ///
    /// # Point of No Return
    ///
    /// **WARNING: THIS IS AN IRREVERSIBLE OPERATION.**
    ///
    /// After this function executes successfully:
    /// - The adapter's `.aos` file reference is permanently cleared
    /// - `unarchive_adapter()` will fail for this adapter
    /// - The adapter can never be loaded again
    /// - Only the audit record remains in the database
    ///
    /// This boundary is enforced by:
    /// - Database trigger `prevent_purged_adapter_load` (migration 0138)
    /// - SQL WHERE clause `purged_at IS NULL` in `unarchive_adapter()`
    ///
    /// # Prerequisites
    ///
    /// CRITICAL: Call this ONLY AFTER successfully deleting the `.aos` file from disk.
    /// The `.aos` file MUST be deleted before calling this function to maintain
    /// consistency between filesystem and database state.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant context (required for security isolation)
    /// * `adapter_id` - The adapter's external ID
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Adapter is not archived (must be archived before purge)
    /// - Adapter is already purged
    /// - Database operation fails
    pub async fn mark_adapter_purged(&self, tenant_id: &str, adapter_id: &str) -> Result<()> {
        // Pre-check: Log that we're about to cross the point of no return
        warn!(
            adapter_id = %adapter_id,
            "POINT OF NO RETURN: About to mark adapter as purged. This is irreversible."
        );

        // SECURITY: Update only within tenant scope
        let affected = sqlx::query(
            "UPDATE adapters
             SET purged_at = datetime('now'),
                 aos_file_path = NULL,
                 updated_at = datetime('now')
             WHERE adapter_id = ?
               AND tenant_id = ?
               AND archived_at IS NOT NULL
               AND purged_at IS NULL",
        )
        .bind(adapter_id)
        .bind(tenant_id)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(format!("Failed to mark adapter purged: {}", e)))?
        .rows_affected();

        if affected == 0 {
            return Err(AosError::validation(format!(
                "Adapter {} is not archived or already purged. Cannot proceed with irreversible purge.",
                adapter_id
            )));
        }

        // Log completion of the irreversible operation
        info!(
            adapter_id = %adapter_id,
            "IRREVERSIBLE: Adapter marked as purged. Recovery is no longer possible."
        );

        // KV write (dual-write mode) - tenant verified via parameter
        if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
            if let Err(e) = repo.mark_adapter_purged_kv(adapter_id).await {
                warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to mark adapter purged in KV backend");
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Adapter marked purged in both SQL and KV backends");
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
                .fetch_optional(self.pool_result()?)
                .await
                .map_err(|e| AosError::database(e.to_string()))?;

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
    /// Restores an archived adapter to active state. This is the last opportunity
    /// to recover an adapter before the garbage collection purge makes it permanent.
    ///
    /// # Recovery Boundary
    ///
    /// This function succeeds only if `purged_at IS NULL`. Once an adapter has been
    /// purged via `mark_adapter_purged()`, recovery is impossible because:
    /// - The `.aos` file has been permanently deleted from disk
    /// - The `aos_file_path` column is NULL
    /// - The database trigger `prevent_purged_adapter_load` blocks any load attempts
    ///
    /// # State Transitions
    ///
    /// ```text
    /// Active → Archived → Active (this function)
    ///               ↓
    ///           Purged (IRREVERSIBLE - unarchive fails here)
    /// ```
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant context (required for security isolation)
    /// * `adapter_id` - The adapter's external ID
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Adapter is not archived (nothing to restore)
    /// - Adapter has been purged (point of no return crossed)
    /// - Database operation fails
    pub async fn unarchive_adapter(&self, tenant_id: &str, adapter_id: &str) -> Result<()> {
        // SECURITY: Update only within tenant scope
        let affected = sqlx::query(
            "UPDATE adapters
             SET archived_at = NULL,
                 archived_by = NULL,
                 archive_reason = NULL,
                 updated_at = datetime('now')
             WHERE adapter_id = ?
               AND tenant_id = ?
               AND archived_at IS NOT NULL
               AND purged_at IS NULL",
        )
        .bind(adapter_id)
        .bind(tenant_id)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(format!("Failed to unarchive adapter: {}", e)))?
        .rows_affected();

        if affected == 0 {
            // This is the enforcement point - purged adapters cannot be restored
            return Err(AosError::validation(format!(
                "Adapter {} is not archived or has crossed the point of no return (purged). Recovery is not possible.",
                adapter_id
            )));
        }

        info!(adapter_id = %adapter_id, "Unarchived adapter - successfully restored before purge");

        // KV write (dual-write mode) - tenant verified via parameter
        if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
            if let Err(e) = repo.unarchive_adapter_kv(adapter_id).await {
                warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to unarchive adapter in KV backend");
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Adapter unarchived in both SQL and KV backends");
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
        .fetch_one(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(format!("Failed to count archived adapters: {}", e)))?;

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
        .fetch_one(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(format!("Failed to count purged adapters: {}", e)))?;

        Ok(count)
    }

    // =========================================================================
    // Tenant-Scoped Adapter Operations
    // These methods validate tenant ownership before performing operations.
    // =========================================================================

    /// Update adapter state with tenant validation (transactional)
    pub(crate) async fn update_adapter_state_tx_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        state: &str,
        reason: &str,
    ) -> Result<()> {
        // Verify adapter belongs to tenant
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM adapters WHERE adapter_id = ? AND tenant_id = ?)",
        )
        .bind(adapter_id)
        .bind(tenant_id)
        .fetch_one(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        if !exists {
            return Err(AosError::NotFound(format!(
                "Adapter {} not found for tenant {}",
                adapter_id, tenant_id
            )));
        }

        self.update_adapter_state_tx(adapter_id, state, reason)
            .await
    }

    /// Update adapter state with CAS (compare-and-swap) and tenant validation
    pub(crate) async fn update_adapter_state_cas_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        expected_state: &str,
        new_state: &str,
        reason: &str,
    ) -> Result<bool> {
        // Verify adapter belongs to tenant
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM adapters WHERE adapter_id = ? AND tenant_id = ?)",
        )
        .bind(adapter_id)
        .bind(tenant_id)
        .fetch_one(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        if !exists {
            return Err(AosError::NotFound(format!(
                "Adapter {} not found for tenant {}",
                adapter_id, tenant_id
            )));
        }

        self.update_adapter_state_cas(adapter_id, expected_state, new_state, reason)
            .await
    }

    /// Update adapter memory with tenant validation
    pub async fn update_adapter_memory_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        memory_bytes: i64,
    ) -> Result<()> {
        // Verify adapter belongs to tenant and update atomically
        let rows_affected = sqlx::query(
            "UPDATE adapters SET memory_bytes = ?, updated_at = datetime('now')
             WHERE adapter_id = ? AND tenant_id = ?",
        )
        .bind(memory_bytes)
        .bind(adapter_id)
        .bind(tenant_id)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(e.to_string()))?
        .rows_affected();

        if rows_affected == 0 {
            return Err(AosError::NotFound(format!(
                "Adapter {} not found for tenant {}",
                adapter_id, tenant_id
            )));
        }

        Ok(())
    }

    /// Update adapter tier with tenant validation
    pub async fn update_adapter_tier_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        tier: &str,
    ) -> Result<()> {
        // Verify adapter belongs to tenant and update atomically
        let rows_affected = sqlx::query(
            "UPDATE adapters SET tier = ?, updated_at = datetime('now')
             WHERE adapter_id = ? AND tenant_id = ?",
        )
        .bind(tier)
        .bind(adapter_id)
        .bind(tenant_id)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(e.to_string()))?
        .rows_affected();

        if rows_affected == 0 {
            return Err(AosError::NotFound(format!(
                "Adapter {} not found for tenant {}",
                adapter_id, tenant_id
            )));
        }

        Ok(())
    }

    /// Update adapter hash fields with tenant validation
    pub async fn update_adapter_weight_hash_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        hash_b3: &str,
    ) -> Result<()> {
        let rows_affected = sqlx::query(
            "UPDATE adapters
             SET hash_b3 = ?,
                 content_hash_b3 = ?,
                 updated_at = datetime('now')
             WHERE adapter_id = ? AND tenant_id = ?",
        )
        .bind(hash_b3)
        .bind(hash_b3)
        .bind(adapter_id)
        .bind(tenant_id)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::database(e.to_string()))?
        .rows_affected();

        if rows_affected == 0 {
            return Err(AosError::NotFound(format!(
                "Adapter {} not found for tenant {}",
                adapter_id, tenant_id
            )));
        }

        let _ = self.ensure_consistency(adapter_id).await?;
        Ok(())
    }

    /// Delete adapter with tenant validation
    pub async fn delete_adapter_for_tenant(&self, tenant_id: &str, adapter_id: &str) -> Result<()> {
        // Verify adapter belongs to tenant and delete atomically
        let rows_affected =
            sqlx::query("DELETE FROM adapters WHERE adapter_id = ? AND tenant_id = ?")
                .bind(adapter_id)
                .bind(tenant_id)
                .execute(self.pool_result()?)
                .await
                .map_err(|e| AosError::database(e.to_string()))?
                .rows_affected();

        if rows_affected == 0 {
            return Err(AosError::NotFound(format!(
                "Adapter {} not found for tenant {}",
                adapter_id, tenant_id
            )));
        }

        // Clean up KV if enabled
        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                if let Err(e) = repo.delete_adapter_kv(adapter_id).await {
                    warn!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        "Failed to delete adapter from KV (SQL delete succeeded)"
                    );
                }
            }
        }

        Ok(())
    }

    /// Duplicate an adapter for the given tenant
    ///
    /// Creates a copy of an existing adapter with a new ID and name.
    /// The new adapter will have:
    /// - `parent_id` set to the source adapter's ID
    /// - `fork_type` set to "extension" (duplicate lineage)
    /// - A new unique ID and hash
    /// - Initial state set to 'cold'
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID (must match the source adapter's tenant)
    /// * `source_adapter_id` - The adapter ID to duplicate
    /// * `new_name` - Optional name for the duplicate (defaults to "{original_name} (copy)")
    ///
    /// # Returns
    /// The ID of the newly created adapter
    pub async fn duplicate_adapter_for_tenant(
        &self,
        tenant_id: &str,
        source_adapter_id: &str,
        new_name: Option<&str>,
    ) -> Result<Adapter> {
        // Fetch the source adapter with tenant validation
        let source = self
            .get_adapter_for_tenant(tenant_id, source_adapter_id)
            .await?
            .ok_or_else(|| {
                AosError::NotFound(format!(
                    "Adapter {} not found for tenant {}",
                    source_adapter_id, tenant_id
                ))
            })?;

        // Generate new identifiers
        let new_adapter_id = new_id(IdPrefix::Adp);
        let name = new_name
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{} (copy)", source.name));

        // Generate semantic alias fields for the duplicate when lineage metadata is available.
        // Keep adapter display name separate from semantic adapter_name taxonomy.
        let semantic_fields = match (
            source.tenant_namespace.as_deref().map(str::trim),
            source.domain.as_deref().map(str::trim),
            source.purpose.as_deref().map(str::trim),
        ) {
            (Some(tenant_namespace), Some(domain), Some(purpose))
                if !tenant_namespace.is_empty() && !domain.is_empty() && !purpose.is_empty() =>
            {
                let latest_row: Option<(String,)> = sqlx::query_as(
                    r#"
                    SELECT revision FROM adapters
                    WHERE tenant_namespace = ?1 AND domain = ?2 AND purpose = ?3
                      AND revision IS NOT NULL
                    ORDER BY CAST(REPLACE(REPLACE(revision, 'r', ''), 'R', '') AS INTEGER) DESC
                    LIMIT 1
                    "#,
                )
                .bind(tenant_namespace)
                .bind(domain)
                .bind(purpose)
                .fetch_optional(self.pool_result()?)
                .await
                .map_err(|e| AosError::database(e.to_string()))?;

                let next_revision_number = latest_row
                    .as_ref()
                    .and_then(|(revision,)| {
                        revision
                            .trim()
                            .trim_start_matches('r')
                            .trim_start_matches('R')
                            .parse::<u32>()
                            .ok()
                    })
                    .unwrap_or(0)
                    + 1;
                let revision = format!("r{:03}", next_revision_number);

                match AdapterName::new(tenant_namespace, domain, purpose, &revision) {
                    Ok(parsed) => Some((
                        parsed.to_string(),
                        tenant_namespace.to_string(),
                        domain.to_string(),
                        purpose.to_string(),
                        revision,
                    )),
                    Err(error) => {
                        warn!(
                            %error,
                            tenant_namespace = %tenant_namespace,
                            domain = %domain,
                            purpose = %purpose,
                            source_adapter_id = %source_adapter_id,
                            "Skipping semantic alias fields for duplicated adapter due invalid lineage metadata"
                        );
                        None
                    }
                }
            }
            _ => None,
        };

        // Generate a new hash for the duplicate
        let new_hash = {
            let mut hasher = blake3::Hasher::new();
            hasher.update(new_adapter_id.as_bytes());
            hasher.update(chrono::Utc::now().to_rfc3339().as_bytes());
            hasher.finalize().to_hex().to_string()
        };

        // Build registration params from source adapter
        let params = AdapterRegistrationParams {
            tenant_id: tenant_id.to_string(),
            adapter_id: new_adapter_id.clone(),
            name: name.clone(),
            hash_b3: new_hash,
            rank: source.rank,
            tier: source.tier.clone(),
            alpha: source.alpha,
            lora_strength: source.lora_strength,
            targets_json: source.targets_json.clone(),
            acl_json: source.acl_json.clone(),
            languages_json: source.languages_json.clone(),
            framework: source.framework.clone(),
            category: source.category.clone(),
            scope: source.scope.clone(),
            framework_id: source.framework_id.clone(),
            framework_version: source.framework_version.clone(),
            repo_id: source.repo_id.clone(),
            commit_sha: source.commit_sha.clone(),
            intent: source.intent.clone(),
            expires_at: None, // Don't copy expiration
            aos_file_path: source.aos_file_path.clone(),
            aos_file_hash: source.aos_file_hash.clone(),
            adapter_name: semantic_fields.as_ref().map(|fields| fields.0.clone()),
            tenant_namespace: semantic_fields.as_ref().map(|fields| fields.1.clone()),
            domain: semantic_fields.as_ref().map(|fields| fields.2.clone()),
            purpose: semantic_fields.as_ref().map(|fields| fields.3.clone()),
            revision: semantic_fields.as_ref().map(|fields| fields.4.clone()),
            parent_id: Some(source.id.clone()),
            fork_type: Some("extension".to_string()),
            fork_reason: Some("User-requested copy".to_string()),
            base_model_id: source.base_model_id.clone(),
            recommended_for_moe: source.recommended_for_moe,
            manifest_schema_version: source.manifest_schema_version.clone(),
            // Generate new content hash for duplicate (it's a distinct adapter even if weights are same)
            content_hash_b3: {
                let mut hasher = blake3::Hasher::new();
                hasher.update(b"duplicate:");
                hasher.update(new_adapter_id.as_bytes());
                hasher.update(
                    source
                        .content_hash_b3
                        .as_deref()
                        .unwrap_or(&source.hash_b3)
                        .as_bytes(),
                );
                hasher.finalize().to_hex().to_string()
            },
            provenance_json: source.provenance_json.clone(),
            metadata_json: source.metadata_json.clone(),
            repo_path: source.repo_path.clone(),
            // These fields may not exist on legacy adapters
            codebase_scope: source.codebase_scope.clone(),
            dataset_version_id: source.dataset_version_id.clone(),
            registration_timestamp: source.registration_timestamp.clone(),
            manifest_hash: source.manifest_hash.clone(),
            // Codebase adapter type and stream binding (from migration 0261)
            adapter_type: source.adapter_type.clone(),
            base_adapter_id: source.base_adapter_id.clone(),
            stream_session_id: source.stream_session_id.clone(),
            versioning_threshold: source.versioning_threshold,
            coreml_package_hash: source.coreml_package_hash.clone(),
            training_dataset_hash_b3: source.training_dataset_hash_b3.clone(),
        };

        // Register the new adapter
        self.register_adapter_extended(params).await?;

        // Preserve deterministic provenance requirements by cloning training snapshot evidence
        // from the source adapter when present.
        if let Some(source_snapshot) = self
            .get_adapter_training_snapshot(source_adapter_id)
            .await?
        {
            self.create_training_snapshot(crate::adapter_snapshots::CreateSnapshotParams {
                adapter_id: new_adapter_id.clone(),
                training_job_id: source_snapshot.training_job_id,
                collection_id: source_snapshot.collection_id,
                documents_json: source_snapshot.documents_json,
                chunk_manifest_hash: source_snapshot.chunk_manifest_hash,
                chunking_config_json: source_snapshot.chunking_config_json,
                dataset_id: source_snapshot.dataset_id,
                dataset_version_id: source_snapshot.dataset_version_id,
                dataset_hash_b3: source_snapshot.dataset_hash_b3,
            })
            .await?;
        }

        // Fetch and return the new adapter using tenant-scoped access
        self.get_adapter_for_tenant(tenant_id, &new_adapter_id)
            .await?
            .ok_or_else(|| AosError::database("Failed to retrieve duplicated adapter".to_string()))
    }
}

impl<'a> WriteCapableDb<'a> {
    /// Transactional adapter state update (global adapter_id scope).
    pub async fn update_adapter_state_tx(
        &self,
        adapter_id: &str,
        state: &str,
        reason: &str,
    ) -> Result<()> {
        self.db
            .update_adapter_state_tx(adapter_id, state, reason)
            .await
    }

    /// Non-transactional adapter state update (tenant scoped).
    pub async fn update_adapter_state(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        state: &str,
        reason: &str,
    ) -> Result<()> {
        self.db
            .update_adapter_state(tenant_id, adapter_id, state, reason)
            .await
    }

    /// Compare-and-swap adapter state transition.
    pub async fn update_adapter_state_cas(
        &self,
        adapter_id: &str,
        expected_state: &str,
        new_state: &str,
        reason: &str,
    ) -> Result<bool> {
        self.db
            .update_adapter_state_cas(adapter_id, expected_state, new_state, reason)
            .await
    }

    /// Atomically update state and memory for an adapter.
    pub async fn update_adapter_state_and_memory(
        &self,
        adapter_id: &str,
        state: &str,
        memory_bytes: i64,
        reason: &str,
    ) -> Result<()> {
        self.db
            .update_adapter_state_and_memory(adapter_id, state, memory_bytes, reason)
            .await
    }

    /// Transactional tenant-scoped adapter state update.
    pub async fn update_adapter_state_tx_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        state: &str,
        reason: &str,
    ) -> Result<()> {
        self.db
            .update_adapter_state_tx_for_tenant(tenant_id, adapter_id, state, reason)
            .await
    }

    /// Tenant-scoped CAS adapter state transition.
    pub async fn update_adapter_state_cas_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        expected_state: &str,
        new_state: &str,
        reason: &str,
    ) -> Result<bool> {
        self.db
            .update_adapter_state_cas_for_tenant(
                tenant_id,
                adapter_id,
                expected_state,
                new_state,
                reason,
            )
            .await
    }
}
