//! Sampling and placement types for inference.

use adapteros_config::PlacementWeights as ConfigPlacementWeights;
use adapteros_core::{BackendKind, SeedMode};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Current sampling algorithm version for replay compatibility checking
pub const SAMPLING_ALGORITHM_VERSION: &str = "v1.0.0";

/// Maximum size for stored prompt/response text (64KB)
pub const MAX_REPLAY_TEXT_SIZE: usize = 64 * 1024;

/// Default maximum tokens to generate when not specified by request
pub const DEFAULT_MAX_TOKENS: usize = 512;

/// Maximum allowed max_tokens value to prevent resource exhaustion.
/// This is a reasonable upper bound based on typical model context limits.
/// Requests exceeding this will be rejected with a clear error message.
pub const MAX_TOKENS_LIMIT: usize = 16384;

/// Placement decision trace entry (per token)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct PlacementTraceEntry {
    pub step: usize,
    pub lane: String,
    pub score: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<f32>,
    pub utilization: f32,
}

/// Placement metadata captured for replay/audit.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlacementReplay {
    /// Mode applied (balanced/latency/energy/thermal/off)
    pub mode: String,
    /// Weights used for the cost model
    pub weights: PlacementWeightsSchema,
    /// Optional per-step device trace
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<PlacementTraceEntry>,
}

/// API-safe placement weights schema (decouples utoipa from config crate)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlacementWeightsSchema {
    pub latency: f32,
    pub energy: f32,
    pub thermal: f32,
}

impl From<ConfigPlacementWeights> for PlacementWeightsSchema {
    fn from(w: ConfigPlacementWeights) -> Self {
        Self {
            latency: w.latency,
            energy: w.energy,
            thermal: w.thermal,
        }
    }
}

impl From<PlacementWeightsSchema> for ConfigPlacementWeights {
    fn from(w: PlacementWeightsSchema) -> Self {
        ConfigPlacementWeights {
            latency: w.latency,
            energy: w.energy,
            thermal: w.thermal,
        }
    }
}

/// Sampling parameters for inference replay
///
/// Captures all parameters that affect token generation for reproducibility.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SamplingParams {
    /// Sampling temperature (0.0 - 2.0)
    pub temperature: f32,
    /// Top-K sampling (None to disable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
    /// Top-P nucleus sampling (None to disable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Maximum tokens to generate
    pub max_tokens: usize,
    /// Random seed for reproducibility (None for non-deterministic)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Error code captured for failed inference metadata (if any)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Seed mode applied for request seed derivation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_mode: Option<SeedMode>,
    /// Backend profile requested for execution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_profile: Option<BackendKind>,
    /// Request seed (hex) provided to worker
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_seed_hex: Option<String>,
    /// Placement metadata (device selection trace)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<PlacementReplay>,
    /// Canonical run envelope serialized for replay metadata
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_envelope: Option<adapteros_api_types::RunEnvelope>,
    /// BLAKE3 hashes of adapters used (ordered)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_hashes_b3: Option<Vec<String>>,
    /// BLAKE3 hash for the dataset manifest used by this request (if any)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset_hash_b3: Option<String>,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            temperature: 0.0,
            top_k: Some(50),
            top_p: Some(1.0),
            max_tokens: 512,
            seed: None,
            error_code: None,
            seed_mode: None,
            backend_profile: None,
            request_seed_hex: None,
            placement: None,
            run_envelope: None,
            adapter_hashes_b3: None,
            dataset_hash_b3: None,
        }
    }
}
