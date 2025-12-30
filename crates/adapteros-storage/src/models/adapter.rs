//! Adapter KV model
//!
//! This module defines the key-value representation of adapters,
//! matching the database schema from adapteros-db.

use serde::{Deserialize, Serialize};

/// Key-value representation of an adapter
///
/// This struct matches the Adapter struct from adapteros-db/src/adapters.rs
/// All fields are preserved for zero-loss migration from SQL to KV storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterKv {
    // Core fields (from migration 0001)
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub tier: String, // 'persistent', 'warm', 'ephemeral'
    pub hash_b3: String,
    pub rank: i32,
    pub alpha: f64,
    pub lora_strength: Option<f32>,
    pub targets_json: String,
    pub acl_json: Option<String>,
    pub adapter_id: Option<String>,
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

    // Arbitrary adapter metadata for search/filtering
    #[serde(default)]
    pub metadata_json: Option<String>,

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
    pub version: String,
    pub lifecycle_state: String,

    // Archive/GC state (from migration 0138)
    pub archived_at: Option<String>,
    pub archived_by: Option<String>,
    pub archive_reason: Option<String>,
    pub purged_at: Option<String>,

    // Base model and artifact hardening
    pub base_model_id: Option<String>,
    #[serde(default)]
    pub recommended_for_moe: Option<bool>,
    pub manifest_schema_version: Option<String>,
    pub content_hash_b3: Option<String>,
    pub provenance_json: Option<String>,

    // Drift detection fields
    pub drift_tier: Option<String>,
    pub drift_metric: Option<f64>,
    pub drift_loss_metric: Option<f64>,
    pub drift_reference_backend: Option<String>,
    pub drift_baseline_backend: Option<String>,
    pub drift_test_backend: Option<String>,

    // Scan root path (from migration 0243)
    #[serde(default)]
    pub repo_path: Option<String>,

    // Codebase adapter registration metadata (from migration 0248)
    #[serde(default)]
    pub codebase_scope: Option<String>,
    #[serde(default)]
    pub dataset_version_id: Option<String>,
    #[serde(default)]
    pub registration_timestamp: Option<String>,
    #[serde(default)]
    pub manifest_hash: Option<String>,

    // Codebase adapter type and stream binding (from migration 0261)
    /// Adapter classification: "standard" (portable), "codebase" (stream-scoped), "core" (baseline)
    #[serde(default)]
    pub adapter_type: Option<String>,
    /// Base adapter ID for codebase adapters (the core adapter they extend as delta)
    /// Distinct from parent_id which tracks version lineage (v1 -> v2 -> v3)
    #[serde(default)]
    pub base_adapter_id: Option<String>,
    /// Exclusive session binding for codebase adapters
    /// Only one codebase adapter can be active per session
    #[serde(default)]
    pub stream_session_id: Option<String>,
    /// Activation threshold for auto-versioning (default: 100)
    /// When activation_count >= versioning_threshold, system creates new version
    #[serde(default)]
    pub versioning_threshold: Option<i32>,
    /// BLAKE3 hash of fused CoreML package for deployment verification
    #[serde(default)]
    pub coreml_package_hash: Option<String>,

    // Timestamps
    pub created_at: String,
    pub updated_at: String,
}

impl AdapterKv {
    /// Canonical key identifier: prefer adapter_id, fall back to legacy UUID id.
    pub fn key_id(&self) -> &str {
        self.adapter_id.as_deref().unwrap_or(self.id.as_str())
    }

    /// Get the primary key for this adapter
    pub fn primary_key(&self) -> String {
        format!("adapter:{}", self.key_id())
    }

    /// Legacy primary key (uses internal UUID). Kept for backward-compat reads/writes.
    pub fn legacy_primary_key(&self) -> String {
        format!("adapter:{}", self.id)
    }

    /// Get the tenant-scoped key for this adapter
    pub fn tenant_key(&self) -> String {
        format!("tenant:{}:adapter:{}", self.tenant_id, self.key_id())
    }

    /// Get hash-based lookup key
    pub fn hash_key(&self) -> String {
        format!("adapter:hash:{}", self.hash_b3)
    }

    /// Get parent relationship key (for lineage traversal)
    pub fn parent_key(&self) -> Option<String> {
        self.parent_id
            .as_ref()
            .map(|pid| format!("adapter:{}:children", pid))
    }

    /// Get children relationship key (for lineage traversal)
    pub fn children_key(&self) -> String {
        format!("adapter:{}:children", self.id)
    }

    /// Entity ids to clean/update indexes (canonical first, then legacy if different).
    pub fn index_entity_ids(&self) -> Vec<String> {
        let mut ids = vec![self.key_id().to_string()];
        if self.id != self.key_id() {
            ids.push(self.id.clone());
        }
        ids
    }

    // =========================================================================
    // Codebase Adapter Helpers
    // =========================================================================

    /// Check if this is a codebase adapter
    pub fn is_codebase_adapter(&self) -> bool {
        self.adapter_type.as_deref() == Some("codebase")
    }

    /// Check if this is a core (baseline) adapter
    pub fn is_core_adapter(&self) -> bool {
        self.adapter_type.as_deref() == Some("core")
    }

    /// Check if this is a standard (portable) adapter
    pub fn is_standard_adapter(&self) -> bool {
        self.adapter_type.as_deref().unwrap_or("standard") == "standard"
    }

    /// Check if this codebase adapter should auto-version based on activation count
    pub fn should_auto_version(&self) -> bool {
        if !self.is_codebase_adapter() {
            return false;
        }
        let threshold = self.versioning_threshold.unwrap_or(100) as i64;
        self.activation_count >= threshold
    }

    /// Check if this adapter is bound to a session
    pub fn is_session_bound(&self) -> bool {
        self.stream_session_id.is_some()
    }

    /// Get base adapter relationship key (for delta lineage traversal)
    pub fn base_adapter_key(&self) -> Option<String> {
        self.base_adapter_id
            .as_ref()
            .map(|bid| format!("adapter:{}:derived", bid))
    }

    /// Get session binding key
    pub fn session_binding_key(&self) -> Option<String> {
        self.stream_session_id
            .as_ref()
            .map(|sid| format!("session:{}:codebase_adapter", sid))
    }
}
