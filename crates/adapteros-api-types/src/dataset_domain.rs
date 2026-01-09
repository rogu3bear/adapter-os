use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use adapteros_types::training::TRAINING_DATA_CONTRACT_VERSION;

fn default_split() -> String {
    "train".to_string()
}

fn default_weight() -> f32 {
    1.0
}

fn default_training_contract_version() -> String {
    TRAINING_DATA_CONTRACT_VERSION.to_string()
}

/// Canonical row schema shared with training workers and dataset services.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CanonicalRow {
    pub row_id: String,
    #[serde(default = "default_split")]
    pub split: String,
    pub prompt: String,
    pub response: String,
    #[serde(default = "default_weight")]
    pub weight: f32,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

/// Manifest summarizing a dataset version after normalization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DatasetManifest {
    pub dataset_id: String,
    pub dataset_version_id: String,
    #[serde(default = "default_training_contract_version")]
    pub training_contract_version: String,
    pub hash_b3: String,
    pub total_rows: usize,
    pub dropped_rows: usize,
    pub splits: HashMap<String, SplitStats>,
    pub normalization: NormalizationNotes,
}

/// Descriptor for a created dataset version.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DatasetVersionDescriptor {
    pub dataset_id: String,
    pub dataset_version_id: String,
    pub storage_path: String,
    pub hash_b3: String,
    pub manifest: DatasetManifest,
}

/// Split-level statistics emitted in manifests.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SplitStats {
    pub rows: usize,
    pub avg_prompt_chars: f64,
    pub avg_response_chars: f64,
}

/// Normalization metadata captured during ingest.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct NormalizationNotes {
    #[serde(default)]
    pub dialects_seen: Vec<String>,
    #[serde(default)]
    pub dropped_reasons: HashMap<String, usize>,
    #[serde(default)]
    pub decisions: Vec<String>,
}

/// Sampling options for deterministic row streaming.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SamplingConfig {
    /// Filter rows by split (e.g., train/eval)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split: Option<String>,
    /// Deterministic shuffle seed; same seed yields stable ordering
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shuffle_seed: Option<String>,
}
