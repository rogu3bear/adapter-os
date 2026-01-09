//! Preprocessed example contract (tokenized + cached features).
//!
//! Defines the versioned contract emitted by preprocessing pipelines.

use serde::{Deserialize, Serialize};

/// Current preprocessed example schema version.
pub const PREPROCESSED_EXAMPLE_SCHEMA_VERSION: &str = "1.0";
/// Preprocessed feature dtype (stable representation).
pub const PREPROCESSED_FEATURE_DTYPE_F32: &str = "f32";
/// Preprocessed feature backend (CoreML).
pub const PREPROCESSED_FEATURE_BACKEND_COREML: &str = "coreml";

fn default_preprocessed_schema_version() -> String {
    PREPROCESSED_EXAMPLE_SCHEMA_VERSION.to_string()
}

fn default_feature_dtype() -> String {
    PREPROCESSED_FEATURE_DTYPE_F32.to_string()
}

fn default_backend() -> String {
    PREPROCESSED_FEATURE_BACKEND_COREML.to_string()
}

/// A single preprocessed training example (tokenized + features).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub struct PreprocessedExampleV1 {
    /// Contract schema version.
    #[serde(default = "default_preprocessed_schema_version")]
    pub schema_version: String,
    /// Input token IDs (prompt).
    pub input_tokens: Vec<u32>,
    /// Target token IDs (completion).
    pub target_tokens: Vec<u32>,
    /// Attention mask aligned with `input_tokens` (1 = real token, 0 = pad).
    pub attention_mask: Vec<u8>,
    /// Precomputed feature tensor data (flattened).
    pub features: Vec<f32>,
    /// Feature tensor shape (e.g., [hidden_dim]).
    pub feature_shape: Vec<u32>,
    /// Feature dtype (always "f32").
    #[serde(default = "default_feature_dtype")]
    pub feature_dtype: String,
    /// Feature backend (always "coreml").
    #[serde(default = "default_backend")]
    pub backend: String,
    /// BLAKE3 hex hash of features (f32 LE bytes).
    pub feature_hash: String,
}

