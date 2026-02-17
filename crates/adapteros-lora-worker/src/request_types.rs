//! Request types and helper functions for inference requests.
//!
//! This module contains the core request types used for inference, including:
//! - InferenceRequest: Main request structure for inference
//! - RequestType: Enum for different request modes (Normal, PatchProposal)
//! - PatchProposalRequest: Parameters for patch proposal requests
//! - CancelTrainingRequest: Request to cancel training jobs
//! - PlacementReplay: Placement decision replay structure
//! - Helper functions for request validation and strict mode enforcement

use adapteros_api_types::{RouterDecisionChainEntry, RunEnvelope};
use adapteros_config::PlacementWeights;
use adapteros_core::{
    determinism::DeterminismContext, AosError, BackendKind, FusionInterval, Result, SeedMode,
};
use adapteros_transport_types::WorkerInferenceRequest;
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use adapteros_types::coreml::CoreMLMode;
use adapteros_types::inference::ChatMessage;
use serde::{Deserialize, Serialize};

/// Main inference request structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub cpid: String,
    pub prompt: String,
    /// Structured chat messages for model-aware template formatting.
    /// When present, the worker uses `ChatTemplateEngine` to format these
    /// instead of applying the legacy tokenizer-based chat template to `prompt`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<ChatMessage>>,
    pub max_tokens: usize,
    /// Optional request identifier for tracing
    #[serde(default)]
    pub request_id: Option<String>,
    /// Canonical run envelope propagated from control plane
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_envelope: Option<RunEnvelope>,
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
    /// Chat session ID for KV cache persistence across turns.
    /// When present, the worker uses a persistent per-session KV cache
    /// for O(1) token generation instead of reprocessing the full context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Execution policy ID for audit/trace binding
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_id: Option<String>,
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
    /// Per-adapter stable IDs for deterministic tie-breaking (score DESC, stable_id ASC).
    ///
    /// Keys may be either internal adapter UUIDs (`id`) or external adapter IDs (`adapter_id`).
    /// Values are DB-issued, per-tenant monotonic sequences. A value of `0` indicates a
    /// legacy adapter that predates stable_id assignment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_stable_ids: Option<std::collections::HashMap<String, u64>>,
    /// Per-adapter version-aware canary multipliers (neutral default 1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_version_weights: Option<std::collections::HashMap<String, f32>>,
    /// Placement override for replay
    #[serde(default)]
    pub placement: Option<PlacementReplay>,

    /// Optional routing policy resolved by control plane.
    /// Used to enforce allow/deny lists and max-adapter limits per token.
    #[serde(default)]
    pub routing_policy: Option<adapteros_api_types::RoutingPolicy>,
    /// BLAKE3 digest of policy decisions applied during request processing
    #[serde(default)]
    pub policy_mask_digest_b3: Option<[u8; 32]>,

    /// Optional stop policy for deterministic stop control (PRD: Hard Deterministic Stop Controller)
    #[serde(default)]
    pub stop_policy: Option<adapteros_api_types::inference::StopPolicySpec>,
    /// Enable UTF-8 token healing (default: true)
    #[serde(default = "default_utf8_healing")]
    pub utf8_healing: bool,

    /// FIM prefix (code before cursor). When both `fim_prefix` and `fim_suffix`
    /// are present, the worker builds a FIM token sequence instead of tokenizing
    /// `prompt` directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fim_prefix: Option<String>,

    /// FIM suffix (code after cursor).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fim_suffix: Option<String>,

    /// Admin override flag to bypass cluster routing restrictions (debug only)
    #[serde(default)]
    pub admin_override: bool,

    /// Arrival timestamp for queue timing measurement (set by UDS server)
    /// This is NOT serialized - it's set internally when the request arrives
    #[serde(skip)]
    pub arrival_instant: Option<std::time::Instant>,
}

fn default_determinism_mode() -> String {
    "strict".to_string()
}

fn default_utf8_healing() -> bool {
    true
}

impl From<adapteros_transport_types::PlacementReplay> for PlacementReplay {
    fn from(value: adapteros_transport_types::PlacementReplay) -> Self {
        Self {
            mode: value.mode,
            weights: PlacementWeights {
                latency: value.weights.latency,
                energy: value.weights.energy,
                thermal: value.weights.thermal,
            },
            trace: value
                .trace
                .into_iter()
                .map(|entry| PlacementTraceEntry {
                    step: entry.step,
                    lane: entry.lane,
                    score: entry.score,
                    temperature_c: entry.temperature_c,
                    utilization: entry.utilization,
                })
                .collect(),
        }
    }
}

impl From<WorkerInferenceRequest> for InferenceRequest {
    fn from(req: WorkerInferenceRequest) -> Self {
        Self {
            cpid: req.cpid,
            prompt: req.prompt,
            messages: req.messages,
            max_tokens: req.max_tokens,
            request_id: req.request_id,
            run_envelope: req.run_envelope,
            require_evidence: req.require_evidence,
            reasoning_mode: req.reasoning_mode,
            request_type: req.request_type,
            stack_id: req.stack_id,
            stack_version: req.stack_version,
            session_id: req.session_id,
            policy_id: req.policy_id,
            domain_hint: req.domain_hint,
            temperature: req.temperature,
            top_k: req.top_k,
            top_p: req.top_p,
            seed: req.seed,
            router_seed: req.router_seed,
            seed_mode: req.seed_mode,
            request_seed: req.request_seed,
            determinism: req.determinism,
            fusion_interval: req.fusion_interval,
            backend_profile: req.backend_profile,
            coreml_mode: req.coreml_mode,
            pinned_adapter_ids: req.pinned_adapter_ids,
            determinism_mode: req.determinism_mode,
            routing_determinism_mode: req.routing_determinism_mode,
            strict_mode: req.strict_mode,
            adapter_strength_overrides: req.adapter_strength_overrides,
            effective_adapter_ids: req.effective_adapter_ids,
            adapter_stable_ids: req.adapter_stable_ids,
            adapter_version_weights: req.adapter_version_weights,
            placement: req.placement.map(PlacementReplay::from),
            routing_policy: req.routing_policy,
            policy_mask_digest_b3: req.policy_mask_digest_b3,
            stop_policy: req.stop_policy,
            utf8_healing: req.utf8_healing,
            fim_prefix: req.fim_prefix,
            fim_suffix: req.fim_suffix,
            admin_override: req.admin_override,
            arrival_instant: None,
        }
    }
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

/// Request type for different inference modes.
pub type RequestType = adapteros_transport_types::WorkerRequestType;

/// Patch proposal request parameters.
pub type PatchProposalRequest = adapteros_transport_types::WorkerPatchProposalRequest;

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

#[cfg(test)]
mod tests {
    use super::InferenceRequest;
    use adapteros_api_types::API_SCHEMA_VERSION;

    #[test]
    fn inference_request_accepts_run_envelope_with_unknown_fields() {
        let json = format!(
            r#"{{
  "cpid": "tenant-1",
  "prompt": "hello",
  "max_tokens": 4,
  "run_envelope": {{
    "run_id": "run-1",
    "schema_version": "{schema_version}",
    "workspace_id": "tenant-1",
    "actor": {{
      "subject": "user-1",
      "roles": ["user"],
      "principal_type": "user",
      "auth_mode": "bearer"
    }},
    "reasoning_mode": false,
    "determinism_version": "v1",
    "created_at": "2024-01-01T00:00:00Z",
    "unknown_field": "ignore-me"
  }},
  "unknown_top_level": "ignored"
}}"#,
            schema_version = API_SCHEMA_VERSION
        );

        let request: InferenceRequest =
            serde_json::from_str(&json).expect("request should deserialize");
        let envelope = request
            .run_envelope
            .expect("run_envelope should be present");
        assert_eq!(envelope.run_id, "run-1");
        assert_eq!(envelope.schema_version, API_SCHEMA_VERSION);
        assert_eq!(envelope.workspace_id, "tenant-1");
    }
}
