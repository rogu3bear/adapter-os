//! Adapter record domain types

use serde::{Deserialize, Serialize};

/// Comprehensive adapter record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AdapterRecord {
    /// Unique database identifier
    pub id: String,
    /// Tenant this adapter belongs to
    pub tenant_id: String,
    /// Human-readable adapter name
    pub name: String,
    /// Adapter tier (e.g., production, staging)
    pub tier: String,
    /// BLAKE3 hash of adapter weights
    pub hash_b3: String,
    /// Routing priority rank
    pub rank: i32,
    /// LoRA scaling factor
    pub alpha: f64,
    /// JSON-encoded target configurations
    pub targets_json: String,
    /// JSON-encoded access control list
    pub acl_json: Option<String>,
    /// External adapter identifier
    pub adapter_id: Option<String>,
    /// JSON-encoded supported languages
    pub languages_json: Option<String>,
    /// Framework name (e.g., mlx, coreml)
    pub framework: Option<String>,
    /// Whether the adapter is currently active
    pub active: bool,
    /// Adapter category classification
    pub category: String,
    /// Scope of adapter applicability
    pub scope: String,
    /// Framework-specific identifier
    pub framework_id: Option<String>,
    /// Framework version requirement
    pub framework_version: Option<String>,
    /// Source repository identifier
    pub repo_id: Option<String>,
    /// Git commit SHA of adapter source
    pub commit_sha: Option<String>,
    /// Declared intent or purpose
    pub intent: Option<String>,
    /// Current operational state
    pub current_state: String,
    /// Whether the adapter is pinned in memory
    pub pinned: bool,
    /// Memory footprint in bytes
    pub memory_bytes: i64,
    /// Last activation timestamp
    pub last_activated: Option<String>,
    /// Total activation count
    pub activation_count: i64,
    /// Expiration timestamp
    pub expires_at: Option<String>,
    /// Current load state (loaded, unloaded, loading)
    pub load_state: String,
    /// Last load timestamp
    pub last_loaded_at: Option<String>,
    /// Canonical adapter name
    pub adapter_name: Option<String>,
    /// Tenant namespace for isolation
    pub tenant_namespace: Option<String>,
    /// Domain classification
    pub domain: Option<String>,
    /// Adapter purpose description
    pub purpose: Option<String>,
    /// Version revision identifier
    pub revision: Option<String>,
    /// Parent adapter ID for forks
    pub parent_id: Option<String>,
    /// Type of fork relationship
    pub fork_type: Option<String>,
    /// Reason for forking
    pub fork_reason: Option<String>,
    /// Semantic version string
    pub version: String,
    /// Lifecycle state (active, deprecated, archived)
    pub lifecycle_state: String,
    /// LoRA strength multiplier
    pub lora_strength: Option<f32>,
    /// Archive timestamp
    pub archived_at: Option<String>,
    /// Person/system who archived it
    pub archived_by: Option<String>,
    /// Reason for archiving
    pub archive_reason: Option<String>,
    /// Purge timestamp
    pub purged_at: Option<String>,
}
