//! Fusion interval policy for aligning weight fusion with router gating.
//!
//! The interval determines how often fused tensors are recomputed relative to
//! router decisions. Keeping this explicit prevents the weight fusion cadence
//! from drifting away from per-token gating.

use serde::{Deserialize, Serialize};

/// Fusion interval cadence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FusionInterval {
    /// Fuse once per request; router gates remain constant for the whole run.
    PerRequest,
    /// Fuse every N tokens as a compromise between throughput and alignment.
    PerSegment { tokens_per_segment: u32 },
    /// Fuse for every token; maximally aligned with router decisions.
    PerToken,
}

impl FusionInterval {
    /// Default interval mode when none is specified.
    pub fn default_mode() -> Self {
        FusionInterval::PerRequest
    }

    /// Deterministic interval identifier for a given token step.
    ///
    /// - `per_request` => `request-0`
    /// - `per_segment` => `segment-{step/len}`
    /// - `per_token` => `token-{step}`
    pub fn interval_id_for_step(&self, step: usize) -> String {
        match self {
            FusionInterval::PerRequest => "request-0".to_string(),
            FusionInterval::PerSegment { tokens_per_segment } => {
                let segment = (*tokens_per_segment).max(1) as usize;
                let idx = step / segment;
                format!("segment-{idx}")
            }
            FusionInterval::PerToken => format!("token-{step}"),
        }
    }

    /// Normalize the segment length to a minimum of one token.
    pub fn normalized_segment_len(&self) -> usize {
        match self {
            FusionInterval::PerSegment { tokens_per_segment } => {
                (*tokens_per_segment).max(1) as usize
            }
            _ => 0,
        }
    }
}

impl Default for FusionInterval {
    fn default() -> Self {
        FusionInterval::PerRequest
    }
}
