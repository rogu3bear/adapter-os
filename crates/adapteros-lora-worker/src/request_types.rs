//! Request types and helper functions for inference requests.
//!
//! This module contains the core request types used for inference, including:
//! - InferenceRequest: Main request structure for inference
//! - RequestType: Enum for different request modes (Normal, PatchProposal)
//! - PatchProposalRequest: Parameters for patch proposal requests
//! - CancelTrainingRequest: Request to cancel training jobs
//! - PlacementReplay: Placement decision replay structure
//! - Helper functions for request validation and strict mode enforcement

use adapteros_api_types::RouterDecisionChainEntry;
use adapteros_config::PlacementWeights;
use adapteros_core::{
    determinism::DeterminismContext, AosError, BackendKind, FusionInterval, Result, SeedMode,
};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use adapteros_types::coreml::CoreMLMode;
use serde::{Deserialize, Serialize};

/// Main inference request structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub cpid: String,
    pub prompt: String,
    pub max_tokens: usize,
    /// Optional request identifier for tracing
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub require_evidence: bool,
    /// Enable reasoning-aware routing (pauses at reasoning spans to hot-swap adapters)
    #[serde(default)]
    pub reasoning_mode: bool,
    /// Optional: Request patch proposal mode
    #[serde(default)]
    pub request_type: RequestType,
    /// Stack ID for telemetry correlation
    #[serde(default)]
    pub stack_id: Option<String>,
    /// Stack version for telemetry correlation
    #[serde(default)]
    pub stack_version: Option<i64>,
    /// Optional domain hint for routing/package preference
    #[serde(default)]
    pub domain_hint: Option<String>,
    /// Sampling temperature (0.0 = deterministic, higher = more random)
    /// Defaults to manifest setting if not provided
    #[serde(default)]
    pub temperature: Option<f32>,
    /// Top-K sampling (limits vocabulary to K most likely tokens)
    #[serde(default)]
    pub top_k: Option<usize>,
    /// Top-P (nucleus) sampling (limits vocabulary to tokens with cumulative prob <= P)
    #[serde(default)]
    pub top_p: Option<f32>,
    /// Random seed for deterministic sampling (PRD-02: critical for replay)
    #[serde(default)]
    pub seed: Option<u64>,
    /// Router seed override for deterministic adapter selection (PRD-02: replay)
    /// When provided, overrides the manifest-derived router seed to reproduce
    /// exact routing decisions from a previous inference.
    #[serde(default)]
    pub router_seed: Option<String>,
    /// Seed mode provided by control plane
    #[serde(default)]
    pub seed_mode: Option<SeedMode>,
    /// Request-scoped seed provided by control plane (32 bytes)
    #[serde(default)]
    pub request_seed: Option<[u8; 32]>,
    /// Canonical determinism context supplied by control plane (optional)
    #[serde(default)]
    pub determinism: Option<DeterminismContext>,
    /// Fusion interval policy for aligning router gates with fused weights
    #[serde(default)]
    pub fusion_interval: Option<FusionInterval>,
    /// Backend profile requested by control plane
    #[serde(default)]
    pub backend_profile: Option<BackendKind>,
    /// CoreML mode applied by control plane
    #[serde(default)]
    pub coreml_mode: Option<CoreMLMode>,
    /// Pinned adapter IDs that receive prior boost in routing (CHAT-PIN-02)
    ///
    /// These adapters receive PINNED_BOOST (0.3) added to their prior scores
    /// before the router's scoring algorithm runs.
    #[serde(default)]
    pub pinned_adapter_ids: Option<Vec<String>>,
    /// Determinism mode for this request (strict, besteffort, relaxed)
    /// Controls router behavior for reproducibility vs performance tradeoffs
    #[serde(default = "default_determinism_mode")]
    pub determinism_mode: String,
    /// Routing determinism mode (deterministic|adaptive)
    #[serde(default)]
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,
    /// Strict mode flag (disables backend fallback when true)
    #[serde(default)]
    pub strict_mode: bool,
    /// Per-adapter strength overrides (multiplier applied to manifest lora_strength)
    #[serde(default)]
    pub adapter_strength_overrides: Option<std::collections::HashMap<String, f32>>,
    /// Effective adapter IDs (control-plane gate)
    #[serde(default)]
    pub effective_adapter_ids: Option<Vec<String>>,
    /// Placement override for replay
    #[serde(default)]
    pub placement: Option<PlacementReplay>,

    /// Optional routing policy resolved by control plane.
    /// Used to enforce allow/deny lists and max-adapter limits per token.
    #[serde(default)]
    pub routing_policy: Option<adapteros_api_types::RoutingPolicy>,

    /// Optional stop policy for deterministic stop control (PRD: Hard Deterministic Stop Controller)
    #[serde(default)]
    pub stop_policy: Option<adapteros_api_types::inference::StopPolicySpec>,

    /// Admin override flag to bypass cluster routing restrictions (debug only)
    #[serde(default)]
    pub admin_override: bool,
}

fn default_determinism_mode() -> String {
    "strict".to_string()
}

/// Returns true when strict determinism protections should be enforced.
#[allow(dead_code)]
pub(crate) fn strict_mode_enabled(strict_flag: bool, determinism_mode: &str) -> bool {
    strict_flag || determinism_mode.eq_ignore_ascii_case("strict")
}

/// In strict mode, ensure router decision chain has matching gates and adapters.
#[allow(dead_code)]
pub(crate) fn enforce_strict_router_chain(
    strict_mode: bool,
    base_only_request: bool,
    chain: &[RouterDecisionChainEntry],
) -> Result<()> {
    if !strict_mode || base_only_request {
        return Ok(());
    }

    for entry in chain {
        if entry.gates_q15.is_empty() {
            return Err(AosError::DeterminismViolation(
                "strict mode requires gates_q15 for every routed token".to_string(),
            ));
        }
        if entry.gates_q15.len() != entry.adapter_ids.len() {
            return Err(AosError::DeterminismViolation(
                "strict mode requires gates_q15 length to match adapter_ids".to_string(),
            ));
        }
    }

    Ok(())
}

/// Request type for different inference modes
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RequestType {
    #[default]
    Normal,
    PatchProposal(PatchProposalRequest),
}

/// Patch proposal request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchProposalRequest {
    /// Repository ID for context
    pub repo_id: String,
    /// Commit SHA for context (optional)
    pub commit_sha: Option<String>,
    /// Files to focus on
    pub target_files: Vec<String>,
    /// Issue description or prompt
    pub description: String,
}

// Forward declaration - PlacementTraceEntry is defined in response_types
// to avoid circular dependency, we re-export it here for convenience
pub use crate::response_types::PlacementTraceEntry;

/// Placement decision replay structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementReplay {
    pub mode: String,
    pub weights: PlacementWeights,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<PlacementTraceEntry>,
}

/// Request to cancel a training job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTrainingRequest {
    /// ID of the training job to cancel
    pub job_id: String,
    /// Optional reason for cancellation
    pub reason: Option<String>,
}

/// Response from training job cancellation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTrainingResponse {
    /// ID of the job that was cancelled
    pub job_id: String,
    /// Status: "cancelled", "not_found", "already_complete", "not_running"
    pub status: String,
    /// Number of tokens processed before cancellation (if available)
    pub tokens_processed: Option<u64>,
    /// Final loss value at cancellation (if available)
    pub final_loss: Option<f32>,
    /// Epoch at which training was stopped
    pub stopped_at_epoch: Option<u32>,
}
