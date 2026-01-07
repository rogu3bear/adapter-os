//! Router decision and candidate types
//!
//! This module contains the canonical routing types used across the system.

/// Fixed-size BLAKE3 hash (32 bytes) used for routing digests.
pub type B3Hash = [u8; 32];
use serde::{Deserialize, Serialize};

/// Routing model type for decision traces.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouterModelType {
    /// Standard dense adapter stack.
    Dense,
}

impl RouterModelType {
    /// Default dense value used by serde when the field is absent.
    pub fn dense() -> Self {
        RouterModelType::Dense
    }
}

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
    pub entropy: f64,

    /// Temperature parameter (tau)
    pub tau: f64,

    /// Entropy floor to prevent single-adapter collapse
    pub entropy_floor: f64,

    /// Optional per-adapter allow mask aligned with adapter ordering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_mask: Option<Vec<bool>>,

    /// BLAKE3 hash of the active adapter stack (for verification)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_hash: Option<String>,

    /// Fusion interval identifier that was active for this decision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_id: Option<String>,

    /// Digest binding routing policy context to the applied mask (if any).
    #[serde(skip_serializing_if = "Option::is_none", alias = "policy_mask_digest")]
    pub policy_mask_digest_b3: Option<B3Hash>,

    /// Flags indicating which policy overrides were applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_overrides_applied: Option<PolicyOverrideFlags>,

    /// Model type for this decision
    #[serde(default = "RouterModelType::dense")]
    pub model_type: RouterModelType,

    /// Backend type used for this routing decision (PRD-DET-001: G6).
    ///
    /// Binds the routing decision to the specific backend that executed
    /// the inference. Enables detection of backend substitution in telemetry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_type: Option<String>,
}

impl RouterDecision {
    /// Create a new router decision
    pub fn new(
        step: usize,
        candidate_adapters: Vec<RouterCandidate>,
        entropy: f64,
        tau: f64,
        entropy_floor: f64,
    ) -> Self {
        Self {
            step,
            input_token_id: None,
            candidate_adapters,
            entropy,
            tau,
            entropy_floor,
            allowed_mask: None,
            stack_hash: None,
            interval_id: None,
            policy_mask_digest_b3: None,
            policy_overrides_applied: None,
            model_type: RouterModelType::Dense,
            backend_type: None,
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

    /// Add backend type for determinism binding (PRD-DET-001: G6)
    pub fn with_backend_type(mut self, backend: String) -> Self {
        self.backend_type = Some(backend);
        self
    }

    /// Get the top K selected adapters
    pub fn top_k(&self, k: usize) -> Vec<&RouterCandidate> {
        self.candidate_adapters.iter().take(k).collect()
    }
}

// =============================================================================
// Drift Detection (PRD-DET-001: G7)
// =============================================================================

/// Report of drift between dual-written routing decisions.
///
/// This is used by the dual-write consistency checker to detect
/// when primary and secondary records have diverged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftReport {
    /// Field that diverged
    pub field: String,
    /// Value in primary record
    pub primary_value: Option<String>,
    /// Value in secondary record
    pub secondary_value: Option<String>,
}

/// Check for drift between dual-written routing decision records (PRD-DET-001: G7).
///
/// This function compares two routing decisions (typically from primary and
/// secondary storage) and detects any divergence in determinism-critical fields.
///
/// # Arguments
///
/// * `primary` - The routing decision from primary storage
/// * `secondary` - The routing decision from secondary storage
///
/// # Returns
///
/// * `None` if records are consistent
/// * `Some(DriftReport)` if divergence detected
pub fn detect_backend_drift(
    primary: &RouterDecision,
    secondary: &RouterDecision,
) -> Option<DriftReport> {
    // Check backend_type first (most critical for determinism)
    if primary.backend_type != secondary.backend_type {
        return Some(DriftReport {
            field: "backend_type".to_string(),
            primary_value: primary.backend_type.clone(),
            secondary_value: secondary.backend_type.clone(),
        });
    }

    // Check entropy (should be identical for same inputs)
    if (primary.entropy - secondary.entropy).abs() > 1e-9 {
        return Some(DriftReport {
            field: "entropy".to_string(),
            primary_value: Some(primary.entropy.to_string()),
            secondary_value: Some(secondary.entropy.to_string()),
        });
    }

    // Check step alignment
    if primary.step != secondary.step {
        return Some(DriftReport {
            field: "step".to_string(),
            primary_value: Some(primary.step.to_string()),
            secondary_value: Some(secondary.step.to_string()),
        });
    }

    None
}

/// Flags describing which policy overrides affected routing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PolicyOverrideFlags {
    /// True when an allowlist restricted the effective set.
    pub allow_list: bool,
    /// True when a denylist removed adapters from the effective set.
    pub deny_list: bool,
    /// True when trust-state rules blocked adapters.
    pub trust_state: bool,
}
