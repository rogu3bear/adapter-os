//! Adapter entity KV schema
//!
//! This module defines the canonical adapter entity for key-value storage,
//! replacing the SQL `adapters` table.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Canonical adapter entity for KV storage
///
/// This struct represents the authoritative schema for adapter entities in the
/// key-value storage backend. It includes all fields from the SQL `adapters` table
/// with proper type conversions.
///
/// **Key Design:**
/// - Primary key: `tenant_id/adapter/{id}`
/// - Secondary indexes:
///   - `tenant_id/adapter-by-name/{name}` -> `{id}`
///   - `tenant_id/adapter-by-hash/{hash_b3}` -> `{id}`
///   - `tenant_id/adapters-by-tier/{tier}` -> Set<{id}>
///   - `tenant_id/adapters-by-state/{lifecycle_state}` -> Set<{id}>
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdapterKv {
    // Core identity
    pub id: String,
    pub tenant_id: String,
    pub adapter_id: Option<String>,
    pub name: String,

    // Technical metadata
    pub hash_b3: String,
    pub rank: i32,
    pub alpha: f64,
    pub lora_strength: Option<f32>,
    pub tier: String, // persistent | warm | ephemeral

    // Configuration
    pub targets: Vec<String>,
    pub acl: Option<Vec<String>>,
    pub languages: Option<Vec<String>>,

    // Classification
    pub category: String,
    pub scope: String,
    pub framework: Option<String>,
    pub framework_id: Option<String>,
    pub framework_version: Option<String>,
    #[serde(default = "default_recommended_for_moe")]
    pub recommended_for_moe: bool,

    // Lifecycle and state
    pub lifecycle_state: String, // draft | active | deprecated | retired
    pub current_state: String,   // unloaded | cold | warm | hot | resident
    pub load_state: String,
    pub version: String,
    pub active: bool,

    // Semantic naming taxonomy (from migration 0061)
    pub adapter_name: Option<String>,
    pub tenant_namespace: Option<String>,
    pub domain: Option<String>,
    pub purpose: Option<String>,
    pub revision: Option<String>,

    // Lineage tracking
    pub parent_id: Option<String>,
    pub fork_type: Option<String>, // parameter | data | architecture
    pub fork_reason: Option<String>,

    // Source tracking
    pub repo_id: Option<String>,
    pub commit_sha: Option<String>,
    pub intent: Option<String>,

    // Runtime metrics
    pub memory_bytes: i64,
    pub activation_count: i64,
    pub last_activated: Option<DateTime<Utc>>,
    pub last_loaded_at: Option<DateTime<Utc>>,

    // Persistence
    pub pinned: bool,
    pub expires_at: Option<DateTime<Utc>>,

    // File references
    pub aos_file_path: Option<String>,
    pub aos_file_hash: Option<String>,
    #[serde(default)]
    pub metadata_json: Option<String>,

    // Archive/GC state (from migration 0138)
    pub archived_at: Option<DateTime<Utc>>,
    pub archived_by: Option<String>,
    pub archive_reason: Option<String>,
    pub purged_at: Option<DateTime<Utc>>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_recommended_for_moe() -> bool {
    true
}

impl AdapterKv {
    /// Check if the adapter has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            expires_at < Utc::now()
        } else {
            false
        }
    }

    /// Check if the adapter is ephemeral
    pub fn is_ephemeral(&self) -> bool {
        self.tier == "ephemeral"
    }

    /// Check if the adapter is persistent
    pub fn is_persistent(&self) -> bool {
        self.tier == "persistent"
    }

    /// Get the fully qualified adapter name
    pub fn fqn(&self) -> String {
        if let (Some(tenant_ns), Some(domain), Some(purpose), Some(revision)) = (
            &self.tenant_namespace,
            &self.domain,
            &self.purpose,
            &self.revision,
        ) {
            format!("{}/{}/{}/{}", tenant_ns, domain, purpose, revision)
        } else {
            self.name.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_fqn() {
        let adapter = AdapterKv {
            id: "test-id".to_string(),
            tenant_id: "tenant-1".to_string(),
            adapter_id: None,
            name: "test-adapter".to_string(),
            hash_b3: "abc123".to_string(),
            rank: 8,
            alpha: 16.0,
            lora_strength: None,
            tier: "warm".to_string(),
            targets: vec!["q_proj".to_string(), "v_proj".to_string()],
            acl: None,
            languages: Some(vec!["rust".to_string()]),
            category: "code".to_string(),
            scope: "global".to_string(),
            framework: None,
            framework_id: None,
            framework_version: None,
            recommended_for_moe: true,
            lifecycle_state: "active".to_string(),
            current_state: "warm".to_string(),
            load_state: "loaded".to_string(),
            version: "1.0.0".to_string(),
            active: true,
            adapter_name: Some("code-adapter".to_string()),
            tenant_namespace: Some("default".to_string()),
            domain: Some("coding".to_string()),
            purpose: Some("rust-expert".to_string()),
            revision: Some("v1".to_string()),
            parent_id: None,
            fork_type: None,
            fork_reason: None,
            repo_id: None,
            commit_sha: None,
            intent: None,
            memory_bytes: 1048576,
            activation_count: 42,
            last_activated: None,
            last_loaded_at: None,
            pinned: false,
            expires_at: None,
            aos_file_path: Some("/var/adapters/test.aos".to_string()),
            aos_file_hash: None,
            metadata_json: None,
            archived_at: None,
            archived_by: None,
            archive_reason: None,
            purged_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert_eq!(adapter.fqn(), "default/coding/rust-expert/v1");
    }

    #[test]
    fn test_adapter_is_expired() {
        let mut adapter = AdapterKv {
            id: "test-id".to_string(),
            tenant_id: "tenant-1".to_string(),
            adapter_id: None,
            name: "test-adapter".to_string(),
            hash_b3: "abc123".to_string(),
            rank: 8,
            alpha: 16.0,
            lora_strength: None,
            tier: "ephemeral".to_string(),
            targets: vec![],
            acl: None,
            languages: None,
            category: "code".to_string(),
            scope: "global".to_string(),
            framework: None,
            framework_id: None,
            framework_version: None,
            recommended_for_moe: true,
            lifecycle_state: "active".to_string(),
            current_state: "warm".to_string(),
            load_state: "loaded".to_string(),
            version: "1.0.0".to_string(),
            active: true,
            adapter_name: None,
            tenant_namespace: None,
            domain: None,
            purpose: None,
            revision: None,
            parent_id: None,
            fork_type: None,
            fork_reason: None,
            repo_id: None,
            commit_sha: None,
            intent: None,
            memory_bytes: 1048576,
            activation_count: 0,
            last_activated: None,
            last_loaded_at: None,
            pinned: false,
            expires_at: None,
            aos_file_path: None,
            aos_file_hash: None,
            metadata_json: None,
            archived_at: None,
            archived_by: None,
            archive_reason: None,
            purged_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Not expired if no expiry set
        assert!(!adapter.is_expired());

        // Expired if expiry is in the past
        adapter.expires_at = Some(Utc::now() - chrono::Duration::hours(1));
        assert!(adapter.is_expired());

        // Not expired if expiry is in the future
        adapter.expires_at = Some(Utc::now() + chrono::Duration::hours(1));
        assert!(!adapter.is_expired());
    }
}
