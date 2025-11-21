//! Canonical AdapterInfo type for adapter identification and basic metadata

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Canonical adapter information type
///
/// This is the single source of truth for adapter identification across the system.
/// Contains basic identifying information common to all adapter representations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AdapterInfo {
    /// Unique adapter identifier
    pub id: String,

    /// Human-readable adapter name
    #[serde(default)]
    pub name: String,

    /// Memory tier (e.g., "tier_0", "tier_1", "persistent", "ephemeral")
    #[serde(default)]
    pub tier: String,

    /// LoRA rank
    #[serde(default)]
    pub rank: u32,

    /// Adapter version
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Activation percentage (0.0 to 1.0 or 0.0 to 100.0)
    #[serde(default)]
    pub activation_pct: f32,

    /// Memory usage in megabytes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u64>,

    /// Whether adapter is currently loaded
    #[serde(default)]
    pub loaded: bool,

    /// Framework identifier (e.g., "qwen2.5-7b")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,

    /// Programming languages this adapter supports
    #[serde(default)]
    pub languages: Vec<String>,

    /// BLAKE3 hash of adapter weights (as string)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_b3: Option<String>,

    /// Time-to-live in hours
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_hours: Option<u32>,

    /// Creation timestamp (ISO 8601)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

impl Default for AdapterInfo {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            tier: "tier_0".to_string(),
            rank: 0,
            version: None,
            activation_pct: 0.0,
            memory_mb: None,
            loaded: false,
            framework: None,
            languages: Vec::new(),
            hash_b3: None,
            ttl_hours: None,
            created_at: None,
        }
    }
}

impl AdapterInfo {
    /// Create new adapter info with required fields
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Default::default()
        }
    }

    /// Set name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set tier
    pub fn with_tier(mut self, tier: impl Into<String>) -> Self {
        self.tier = tier.into();
        self
    }

    /// Set rank
    pub fn with_rank(mut self, rank: u32) -> Self {
        self.rank = rank;
        self
    }

    /// Set version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Set activation percentage
    pub fn with_activation_pct(mut self, pct: f32) -> Self {
        self.activation_pct = pct;
        self
    }

    /// Set memory usage
    pub fn with_memory_mb(mut self, memory_mb: u64) -> Self {
        self.memory_mb = Some(memory_mb);
        self
    }

    /// Set loaded state
    pub fn with_loaded(mut self, loaded: bool) -> Self {
        self.loaded = loaded;
        self
    }
}

/// Canonical adapter metrics for telemetry
///
/// Tracks adapter activation and performance metrics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct AdapterMetrics {
    /// Total number of activations
    pub activations_total: u64,

    /// Total number of evictions
    pub evictions_total: u64,

    /// Current number of active adapters
    pub active_adapters: f64,

    /// Activations broken down by adapter ID
    #[serde(default)]
    pub activations_by_adapter: HashMap<String, u64>,
}

impl AdapterMetrics {
    /// Create new empty metrics
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an activation for an adapter
    pub fn record_activation(&mut self, adapter_id: &str) {
        self.activations_total += 1;
        *self
            .activations_by_adapter
            .entry(adapter_id.to_string())
            .or_insert(0) += 1;
    }

    /// Record an eviction
    pub fn record_eviction(&mut self) {
        self.evictions_total += 1;
    }
}

/// Canonical adapter state for hot-swap and lifecycle tracking
///
/// Represents the runtime state of an adapter in the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AdapterState {
    /// Unique adapter identifier
    pub id: String,

    /// BLAKE3 hash (as hex string for serialization)
    pub hash: String,

    /// VRAM usage in megabytes
    pub vram_mb: u64,

    /// Whether adapter is currently active
    pub active: bool,

    /// Memory tier (persistent, ephemeral, etc.)
    #[serde(default)]
    pub tier: String,

    /// LoRA rank
    #[serde(default)]
    pub rank: u32,

    /// Activation percentage
    #[serde(default)]
    pub activation_pct: f32,

    /// Quality delta from baseline
    #[serde(default)]
    pub quality_delta: f32,

    /// Last activation timestamp (unix millis)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activation: Option<u64>,

    /// Whether adapter is pinned (cannot be evicted)
    #[serde(default)]
    pub pinned: bool,
}

impl AdapterState {
    /// Create new adapter state
    pub fn new(id: impl Into<String>, hash: impl Into<String>, vram_mb: u64) -> Self {
        Self {
            id: id.into(),
            hash: hash.into(),
            vram_mb,
            active: false,
            tier: String::new(),
            rank: 0,
            activation_pct: 0.0,
            quality_delta: 0.0,
            last_activation: None,
            pinned: false,
        }
    }

    /// Set active state
    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Set tier
    pub fn with_tier(mut self, tier: impl Into<String>) -> Self {
        self.tier = tier.into();
        self
    }

    /// Set rank
    pub fn with_rank(mut self, rank: u32) -> Self {
        self.rank = rank;
        self
    }

    /// Set pinned state
    pub fn with_pinned(mut self, pinned: bool) -> Self {
        self.pinned = pinned;
        self
    }
}

impl Default for AdapterState {
    fn default() -> Self {
        Self {
            id: String::new(),
            hash: String::new(),
            vram_mb: 0,
            active: false,
            tier: String::new(),
            rank: 0,
            activation_pct: 0.0,
            quality_delta: 0.0,
            last_activation: None,
            pinned: false,
        }
    }
}
