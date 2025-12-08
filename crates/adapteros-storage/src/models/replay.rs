//! Replay KV models mirroring SQL replay tables.

use serde::{Deserialize, Serialize};

/// Replay metadata stored in KV
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayMetadataKv {
    pub id: String,
    pub inference_id: String,
    pub tenant_id: String,
    pub manifest_hash: String,
    pub base_model_id: Option<String>,
    pub router_seed: Option<String>,
    pub sampling_params_json: String,
    pub backend: String,
    pub backend_version: Option<String>,
    pub sampling_algorithm_version: String,
    pub rag_snapshot_hash: Option<String>,
    pub adapter_ids_json: Option<String>,
    pub base_only: Option<bool>,
    pub prompt_text: String,
    pub prompt_truncated: i32,
    pub response_text: Option<String>,
    pub response_truncated: i32,
    pub rag_doc_ids_json: Option<String>,
    pub chat_context_hash: Option<String>,
    pub replay_status: String,
    pub latency_ms: Option<i32>,
    pub tokens_generated: Option<i32>,
    pub determinism_mode: Option<String>,
    pub fallback_triggered: Option<bool>,
    pub replay_guarantee: Option<String>,
    pub execution_policy_id: Option<String>,
    pub execution_policy_version: Option<i32>,
    pub created_at: String,
}

/// Replay execution stored in KV
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayExecutionKv {
    pub id: String,
    pub original_inference_id: String,
    pub tenant_id: String,
    pub replay_mode: String,
    pub prompt_text: String,
    pub sampling_params_json: String,
    pub backend: String,
    pub manifest_hash: String,
    pub router_seed: Option<String>,
    pub adapter_ids_json: Option<String>,
    pub response_text: Option<String>,
    pub response_truncated: i32,
    pub tokens_generated: Option<i32>,
    pub latency_ms: Option<i32>,
    pub match_status: String,
    pub divergence_details_json: Option<String>,
    pub rag_reproducibility_score: Option<f64>,
    pub missing_doc_ids_json: Option<String>,
    pub executed_at: String,
    pub executed_by: Option<String>,
    pub error_message: Option<String>,
}

/// Replay session stored in KV
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaySessionKv {
    pub id: String,
    pub tenant_id: String,
    pub cpid: String,
    pub plan_id: String,
    pub snapshot_at: String,
    pub seed_global_b3: String,
    pub manifest_hash_b3: String,
    pub policy_hash_b3: String,
    pub kernel_hash_b3: Option<String>,
    pub telemetry_bundle_ids_json: String,
    pub adapter_state_json: String,
    pub routing_decisions_json: String,
    pub inference_traces_json: Option<String>,
    pub rng_state_json: String,
    pub signature: String,
    pub rag_state_json: Option<String>,
    pub created_at: String,
}
