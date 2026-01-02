//! Core adapter metadata types

use crate::training::LoraTier;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// Routing determinism mode (UI toggle)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(sqlx::Type))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "server", sqlx(type_name = "TEXT", rename_all = "snake_case"))]
pub enum RoutingDeterminismMode {
    /// Deterministic routing with stable tie-breaking
    Deterministic,
    /// Adaptive routing that allows relaxed tie-breaking
    Adaptive,
}

impl RoutingDeterminismMode {
    /// Return the canonical string value for this mode.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Deterministic => "deterministic",
            Self::Adaptive => "adaptive",
        }
    }
}

impl fmt::Display for RoutingDeterminismMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RoutingDeterminismMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "deterministic" => Ok(Self::Deterministic),
            "adaptive" => Ok(Self::Adaptive),
            other => Err(format!("invalid routing_determinism_mode: {}", other)),
        }
    }
}

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

    /// Logical domain for routing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,

    /// Logical group (purpose) for routing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,

    /// Logical scope (legacy)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    /// Adapter strength multiplier [0.0, 1.0]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lora_strength: Option<f32>,

    /// Operation identifier inside the scope
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,

    /// Derived scope path: domain/group/scope/operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_path: Option<String>,

    /// LoRA tier (micro/standard/max)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lora_tier: Option<LoraTier>,

    /// Backend tag for selected segment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_tag: Option<String>,

    /// Segment identifier from .aos index
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment_id: Option<u32>,
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
            domain: None,
            group: None,
            scope: None,
            lora_strength: None,
            operation: None,
            scope_path: None,
            lora_tier: None,
            backend_tag: None,
            segment_id: None,
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
