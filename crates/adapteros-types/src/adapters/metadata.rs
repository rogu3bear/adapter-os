//! Core adapter metadata types

use serde::{Deserialize, Serialize};

/// Core adapter metadata (canonical representation)
///
/// This is the single source of truth for adapter metadata across the system.
/// Other crates may extend this with domain-specific fields, but core properties
/// are defined here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AdapterMetadata {
    /// Unique adapter identifier (BLAKE3 hash)
    pub adapter_id: String,

    /// Human-readable adapter name
    pub name: String,

    /// BLAKE3 hash of adapter weights
    pub hash_b3: String,

    /// LoRA rank
    pub rank: i32,

    /// Memory tier (0 = Metal, 1 = System RAM, 2 = Disk)
    pub tier: i32,

    /// Programming languages this adapter is trained for
    pub languages: Vec<String>,

    /// Framework identifier (e.g., "qwen2.5-7b")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,

    /// Adapter version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Creation timestamp (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,

    /// Last update timestamp (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl AdapterMetadata {
    /// Create new adapter metadata
    pub fn new(
        adapter_id: impl Into<String>,
        name: impl Into<String>,
        hash_b3: impl Into<String>,
        rank: i32,
        tier: i32,
    ) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            name: name.into(),
            hash_b3: hash_b3.into(),
            rank,
            tier,
            languages: Vec::new(),
            framework: None,
            version: None,
            created_at: None,
            updated_at: None,
        }
    }

    /// Add languages
    pub fn with_languages(mut self, languages: Vec<String>) -> Self {
        self.languages = languages;
        self
    }

    /// Add framework
    pub fn with_framework(mut self, framework: String) -> Self {
        self.framework = Some(framework);
        self
    }

    /// Add version
    pub fn with_version(mut self, version: String) -> Self {
        self.version = Some(version);
        self
    }

    /// Add timestamps
    pub fn with_timestamps(mut self, created_at: String, updated_at: String) -> Self {
        self.created_at = Some(created_at);
        self.updated_at = Some(updated_at);
        self
    }
}

/// Adapter registration request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RegisterAdapterRequest {
    /// Unique adapter identifier
    pub adapter_id: String,

    /// Human-readable name
    pub name: String,

    /// BLAKE3 hash of weights
    pub hash_b3: String,

    /// LoRA rank
    pub rank: i32,

    /// Memory tier
    pub tier: i32,

    /// Programming languages
    pub languages: Vec<String>,

    /// Framework identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
}

impl From<AdapterMetadata> for RegisterAdapterRequest {
    fn from(metadata: AdapterMetadata) -> Self {
        Self {
            adapter_id: metadata.adapter_id,
            name: metadata.name,
            hash_b3: metadata.hash_b3,
            rank: metadata.rank,
            tier: metadata.tier,
            languages: metadata.languages,
            framework: metadata.framework,
        }
    }
}

/// Adapter lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    /// Registered but not yet loaded
    Registered,

    /// Currently loading into memory
    Loading,

    /// Loaded and ready for inference
    Active,

    /// Temporarily inactive (not unloaded)
    Inactive,

    /// Being unloaded from memory
    Unloading,

    /// Unloaded from memory
    Unloaded,

    /// Marked for deletion
    Expired,

    /// Error state
    Error,
}

impl LifecycleState {
    /// Check if the adapter is usable for inference
    pub fn is_usable(&self) -> bool {
        matches!(self, LifecycleState::Active)
    }

    /// Check if the adapter is in a transitional state
    pub fn is_transitional(&self) -> bool {
        matches!(self, LifecycleState::Loading | LifecycleState::Unloading)
    }
}
