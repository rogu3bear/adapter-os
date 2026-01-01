//! Type conversions and From implementations.

use adapteros_api_types::{
    inference::{InferenceTrace, RouterCandidate, RouterDecision},
    InferRequest, InferResponse,
};

use super::context::InferenceResult;
use super::request::BatchInferItemRequest;
use crate::auth::Claims;

/// Convert from standard InferRequest + Claims to internal format
impl From<(&InferRequest, &Claims)> for super::context::InferenceRequestInternal {
    fn from((req, claims): (&InferRequest, &Claims)) -> Self {
        let is_admin = claims.role.eq_ignore_ascii_case("admin")
            || claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            cpid: claims.tenant_id.clone(),
            prompt: req.prompt.clone(),
            run_envelope: None,
            reasoning_mode: req.reasoning_mode.unwrap_or(false),
            admin_override: is_admin,
            stream: req.stream.unwrap_or(false),
            batch_item_id: None,
            rag_enabled: req.rag_enabled.unwrap_or(false),
            rag_collection_id: req.collection_id.clone(),
            dataset_version_id: req.dataset_version_id.clone(),
            adapter_stack: req.adapter_stack.clone(),
            adapters: req.adapters.clone(),
            stack_id: req.stack_id.clone(),
            domain_hint: req.domain.clone(),
            stack_version: None,
            stack_determinism_mode: None,
            stack_routing_determinism_mode: None,
            effective_adapter_ids: None, // Computed in InferenceCore
            adapter_strength_overrides: None,
            determinism_mode: None,
            routing_determinism_mode: req.routing_determinism_mode,
            seed_mode: None,
            request_seed: None,
            backend_profile: req.backend,
            coreml_mode: req.coreml_mode,
            max_tokens: req.max_tokens.unwrap_or(100),
            temperature: req.temperature.unwrap_or(0.7),
            top_k: req.top_k,
            top_p: req.top_p,
            seed: req.seed,
            router_seed: None,
            require_evidence: req.require_evidence.unwrap_or(false),
            session_id: req.session_id.clone(),
            pinned_adapter_ids: None, // Populated by InferenceCore from session
            chat_context_hash: None,
            claims: Some(claims.clone()),
            policy_mask_digest_b3: None, // Computed by handler from enforce_at_hook
            model: req.model.clone(),
            stop_policy: req.stop_policy.clone(),
            created_at: std::time::Instant::now(),
            worker_auth_token: None,
            utf8_healing: None,
        }
    }
}

/// Convert from batch item + Claims to internal format
impl From<(&BatchInferItemRequest, &Claims)> for super::context::InferenceRequestInternal {
    fn from((item, claims): (&BatchInferItemRequest, &Claims)) -> Self {
        let mut internal = Self::from((&item.request, claims));
        internal.batch_item_id = Some(item.id.clone());
        internal
    }
}

/// Convert InferenceResult to InferResponse for API compatibility
impl From<InferenceResult> for InferResponse {
    fn from(result: InferenceResult) -> Self {
        let model = result
            .deterministic_receipt
            .as_ref()
            .and_then(|receipt| receipt.model.clone());

        Self {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            id: result.request_id,
            text: result.text,
            tokens: vec![],
            tokens_generated: result.tokens_generated,
            finish_reason: result.finish_reason,
            latency_ms: result.latency_ms,
            run_receipt: None,
            deterministic_receipt: result.deterministic_receipt,
            run_envelope: result.run_envelope.clone(),
            adapters_used: result.adapters_used.clone(),
            citations: result.citations,
            trace: InferenceTrace {
                adapters_used: result.adapters_used,
                router_decisions: result
                    .router_decisions
                    .into_iter()
                    .map(|rd| RouterDecision {
                        step: rd.step,
                        input_token_id: rd.input_token_id,
                        candidate_adapters: rd
                            .candidates
                            .into_iter()
                            .map(|c| RouterCandidate {
                                adapter_idx: c.adapter_idx,
                                raw_score: c.raw_score,
                                gate_q15: c.gate_q15,
                            })
                            .collect(),
                        entropy: rd.entropy as f32,
                        tau: 1.0,            // Default tau
                        entropy_floor: 0.02, // Default entropy floor
                        stack_hash: None,
                        interval_id: rd.interval_id.clone(),
                        allowed_mask: None,
                        policy_mask_digest_b3: None,
                        policy_overrides_applied: None,
                        model_type: adapteros_api_types::inference::RouterModelType::Dense,
                    })
                    .collect(),
                router_decision_chain: result.router_decision_chain,
                latency_ms: result.latency_ms,
                fusion_intervals: None,
                model_type: result.model_type,
            },
            model,
            prompt_tokens: None,
            error: None,
            unavailable_pinned_adapters: result.unavailable_pinned_adapters,
            pinned_routing_fallback: result.pinned_routing_fallback,
            backend_used: result.backend_used,
            coreml_compute_preference: result.coreml_compute_preference,
            coreml_compute_units: result.coreml_compute_units,
            coreml_gpu_used: result.coreml_gpu_used,
            fallback_backend: result.fallback_backend,
            fallback_triggered: result.fallback_triggered,
            determinism_mode_applied: result.determinism_mode_applied,
            replay_guarantee: result.replay_guarantee,
            // Stop Controller fields
            stop_reason_code: result.stop_reason_code,
            stop_reason_token_index: result.stop_reason_token_index,
            stop_policy_digest_b3: result.stop_policy_digest_b3,
        }
    }
}
