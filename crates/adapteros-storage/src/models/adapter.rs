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
}
