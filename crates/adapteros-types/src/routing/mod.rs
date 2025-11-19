//! Router decision and candidate types
//!
//! This module contains the canonical routing types used across the system.

use serde::{Deserialize, Serialize};

/// Candidate adapter entry for router trace
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RouterCandidate {
    /// Adapter index in the stack
    pub adapter_idx: u16,

    /// Raw floating-point score
    pub raw_score: f32,

    /// Quantized gate value (Q15 format: signed 16-bit)
    pub gate_q15: i16,
}

/// Router decision at a specific inference step
///
/// This is the canonical schema for router decisions, used for:
/// - Inference trace logging
/// - Telemetry events
/// - Deterministic replay verification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RouterDecision {
    /// Inference step number (0-indexed)
    pub step: usize,

    /// Input token ID at this step (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_token_id: Option<u32>,

    /// Candidate adapters considered by the router
    pub candidate_adapters: Vec<RouterCandidate>,

    /// Entropy of the router distribution
    pub entropy: f32,

    /// Temperature parameter (tau)
    pub tau: f32,

    /// Entropy floor to prevent single-adapter collapse
    pub entropy_floor: f32,

    /// BLAKE3 hash of the active adapter stack (for verification)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_hash: Option<String>,
}

impl RouterDecision {
    /// Create a new router decision
    pub fn new(
        step: usize,
        candidate_adapters: Vec<RouterCandidate>,
        entropy: f32,
        tau: f32,
        entropy_floor: f32,
    ) -> Self {
        Self {
            step,
            input_token_id: None,
            candidate_adapters,
            entropy,
            tau,
            entropy_floor,
            stack_hash: None,
        }
    }

    /// Add input token ID
    pub fn with_token_id(mut self, token_id: u32) -> Self {
        self.input_token_id = Some(token_id);
        self
    }

    /// Add stack hash for verification
    pub fn with_stack_hash(mut self, hash: String) -> Self {
        self.stack_hash = Some(hash);
        self
    }

    /// Get the top K selected adapters
    pub fn top_k(&self, k: usize) -> Vec<&RouterCandidate> {
        self.candidate_adapters.iter().take(k).collect()
    }
}
