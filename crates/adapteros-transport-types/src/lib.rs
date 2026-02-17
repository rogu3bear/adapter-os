//! Shared transport contracts for control-plane <-> worker communication.
//!
//! These types model the UDS wire payloads and are intentionally decoupled
//! from runtime-only worker fields.

use adapteros_api_types::{inference::StopPolicySpec, RoutingPolicy, RunEnvelope};
use adapteros_core::{determinism::DeterminismContext, BackendKind, FusionInterval, SeedMode};
use adapteros_types::{
    adapters::metadata::RoutingDeterminismMode, coreml::CoreMLMode, inference::ChatMessage,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_determinism_mode() -> String {
    "strict".to_string()
}

fn default_utf8_healing() -> bool {
    true
}

/// Request type for worker transport.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkerRequestType {
    /// Standard token generation request.
    #[default]
    Normal,
    /// Patch proposal generation request.
    PatchProposal(WorkerPatchProposalRequest),
}

/// Patch proposal parameters.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkerPatchProposalRequest {
    /// Repository ID for context.
    pub repo_id: String,
    /// Commit SHA for context (optional).
    pub commit_sha: Option<String>,
    /// Files to focus on.
    pub target_files: Vec<String>,
    /// Issue description or prompt.
    pub description: String,
}

/// Placement decision trace entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlacementTraceEntry {
    pub step: usize,
    pub lane: String,
    pub score: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<f32>,
    pub utilization: f32,
}

/// Placement weights used for replay/audit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlacementWeights {
    pub latency: f32,
    pub energy: f32,
    pub thermal: f32,
}

/// Placement metadata captured for replay/audit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlacementReplay {
    pub mode: String,
    pub weights: PlacementWeights,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<PlacementTraceEntry>,
}

/// Canonical worker inference request payload sent over UDS.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct WorkerInferenceRequest {
    pub cpid: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<ChatMessage>>,
    pub max_tokens: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_envelope: Option<RunEnvelope>,
    #[serde(default)]
    pub require_evidence: bool,
    #[serde(default)]
    pub reasoning_mode: bool,
    #[serde(default)]
    pub request_type: WorkerRequestType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router_seed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed_mode: Option<SeedMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_seed: Option<[u8; 32]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub determinism: Option<DeterminismContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fusion_interval: Option<FusionInterval>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_profile: Option<BackendKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_mode: Option<CoreMLMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_adapter_ids: Option<Vec<String>>,
    #[serde(default = "default_determinism_mode")]
    pub determinism_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,
    #[serde(default)]
    pub strict_mode: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_strength_overrides: Option<HashMap<String, f32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_adapter_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_stable_ids: Option<HashMap<String, u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<PlacementReplay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_policy: Option<RoutingPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_policy: Option<StopPolicySpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_mask_digest_b3: Option<[u8; 32]>,
    #[serde(default = "default_utf8_healing")]
    pub utf8_healing: bool,
    #[serde(default)]
    pub admin_override: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fim_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fim_suffix: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_inference_request_round_trip_defaults() {
        let request = WorkerInferenceRequest {
            cpid: "cpid-1".to_string(),
            prompt: "hello".to_string(),
            max_tokens: 32,
            request_id: Some("req-1".to_string()),
            run_envelope: None,
            require_evidence: false,
            reasoning_mode: false,
            request_type: WorkerRequestType::default(),
            stack_id: None,
            stack_version: None,
            session_id: Some("ses-1".to_string()),
            policy_id: None,
            domain_hint: None,
            temperature: Some(0.0),
            top_k: None,
            top_p: Some(1.0),
            seed: None,
            router_seed: None,
            seed_mode: None,
            request_seed: None,
            determinism: None,
            fusion_interval: None,
            backend_profile: None,
            coreml_mode: None,
            pinned_adapter_ids: None,
            determinism_mode: "strict".to_string(),
            routing_determinism_mode: None,
            strict_mode: true,
            adapter_strength_overrides: None,
            effective_adapter_ids: None,
            adapter_stable_ids: None,
            placement: None,
            routing_policy: None,
            stop_policy: None,
            policy_mask_digest_b3: None,
            utf8_healing: true,
            admin_override: false,
            fim_prefix: None,
            fim_suffix: None,
            messages: None,
        };

        let json = serde_json::to_string(&request).expect("serialize request");
        let decoded: WorkerInferenceRequest =
            serde_json::from_str(&json).expect("deserialize request");

        assert_eq!(decoded.cpid, request.cpid);
        assert_eq!(decoded.prompt, request.prompt);
        assert_eq!(decoded.session_id, request.session_id);
        assert_eq!(decoded.determinism_mode, "strict");
        assert!(decoded.utf8_healing);
        assert!(matches!(decoded.request_type, WorkerRequestType::Normal));
    }

    #[test]
    fn worker_inference_request_rejects_unknown_fields() {
        let payload = r#"{
            "cpid": "cpid-1",
            "prompt": "hello",
            "max_tokens": 8,
            "unknown_field": true
        }"#;

        let err = serde_json::from_str::<WorkerInferenceRequest>(payload).unwrap_err();
        assert!(
            err.to_string().contains("unknown field"),
            "error should mention unknown field: {err}"
        );
    }
}
