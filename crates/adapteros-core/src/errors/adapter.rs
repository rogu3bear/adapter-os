//! Adapter-related errors
//!
//! Covers adapter loading, hash verification, lifecycle, and segment operations.

use crate::B3Hash;
use thiserror::Error;

/// Adapter operation errors
#[derive(Error, Debug)]
pub enum AosAdapterError {
    /// Adapter is not loaded or not in a ready state for inference
    #[error("Adapter not loaded: {adapter_id} is in {current_state} state (requires: warm, hot, or resident)")]
    NotLoaded {
        adapter_id: String,
        current_state: String,
    },

    /// Requested adapter is not present in the current manifest
    #[error("Adapter '{adapter_id}' not found in manifest. Available adapters: {available:?}")]
    NotInManifest {
        adapter_id: String,
        available: Vec<String>,
    },

    /// Requested adapter is not part of the effective adapter set
    #[error("Adapter '{adapter_id}' is not in the effective adapter set: {effective_set:?}")]
    NotInEffectiveSet {
        adapter_id: String,
        effective_set: Vec<String>,
    },

    /// Adapter hash verification failed
    #[error("Adapter hash mismatch for {adapter_id}: expected {expected}, got {actual}")]
    HashMismatch {
        adapter_id: String,
        expected: B3Hash,
        actual: B3Hash,
    },

    /// Per-layer hash verification failed
    #[error(
        "Per-layer hash mismatch for {adapter_id} at {layer_id}: expected {expected}, got {actual}"
    )]
    LayerHashMismatch {
        adapter_id: String,
        layer_id: String,
        expected: B3Hash,
        actual: B3Hash,
    },

    /// Segment hash verification failed
    #[error("Segment hash mismatch for {segment_id}")]
    SegmentHashMismatch { segment_id: u32 },

    /// Required segment not found
    #[error("Missing segment for backend '{backend}' and scope '{scope_path}'")]
    MissingSegment { backend: String, scope_path: String },

    /// Canonical segment missing (needs retrain)
    #[error("Missing canonical segment (corrupted / needs retrain)")]
    MissingCanonicalSegment,

    /// Adapter lifecycle error
    #[error("Lifecycle error: {0}")]
    Lifecycle(String),

    /// Adapter not found
    #[error("Adapter not found: {0}")]
    NotFound(String),
}
