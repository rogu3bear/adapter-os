//! Adapter resolution and validation helpers.
//!
//! This module provides utilities for mapping router decisions and
//! working with adapter data in the inference path.

use adapteros_api_types::inference::{
    RouterDecision as ApiRouterDecision, RouterDecisionChainEntry as ApiRouterDecisionChainEntry,
};
use adapteros_telemetry::{RouterDecisionChainEntry, RouterDecisionHash};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use adapteros_types::routing::{RouterCandidate, RouterDecision, RouterModelType};
use std::str::FromStr;

/// Map API router decisions to internal routing types with optional policy mask digest.
pub fn map_router_decisions(
    events: &[ApiRouterDecision],
    policy_mask_digest: Option<[u8; 32]>,
) -> Vec<RouterDecision> {
    // policy_mask_digest is already [u8; 32] which matches adapteros_types::routing::B3Hash
    events
        .iter()
        .map(|d| RouterDecision {
            step: d.step,
            input_token_id: d.input_token_id,
            candidate_adapters: d
                .candidate_adapters
                .iter()
                .map(|c| RouterCandidate {
                    adapter_idx: c.adapter_idx,
                    raw_score: c.raw_score,
                    gate_q15: c.gate_q15,
                })
                .collect(),
            entropy: d.entropy as f64,
            tau: d.tau as f64,
            entropy_floor: d.entropy_floor as f64,
            stack_hash: d.stack_hash.clone(),
            interval_id: d.interval_id.clone(),
            allowed_mask: None,
            policy_mask_digest_b3: policy_mask_digest,
            policy_overrides_applied: None,
            model_type: RouterModelType::Dense,
            backend_type: d.backend_type.clone(), // PRD-DET-001: G6
        })
        .collect()
}

/// Map API router decision chain to telemetry decision chain entries.
pub fn map_router_decision_chain(
    chain: Option<Vec<ApiRouterDecisionChainEntry>>,
) -> Option<Vec<RouterDecisionChainEntry>> {
    chain.map(|entries| {
        entries
            .into_iter()
            .map(|e| RouterDecisionChainEntry {
                step: e.step,
                input_token_id: e.input_token_id,
                adapter_indices: e.adapter_indices,
                adapter_ids: e.adapter_ids,
                gates_q15: e.gates_q15,
                entropy: e.entropy,
                decision_hash: e.decision_hash.map(|h| RouterDecisionHash {
                    input_hash: h.input_hash,
                    output_hash: h.output_hash,
                    reasoning_hash: h.reasoning_hash,
                    combined_hash: h.combined_hash,
                    tau: h.tau,
                    eps: h.eps,
                    k: h.k,
                }),
                previous_hash: e.previous_hash,
                entry_hash: e.entry_hash,
            })
            .collect()
    })
}

/// Parse routing determinism mode from optional string.
pub fn parse_routing_mode(raw: &Option<String>) -> Option<RoutingDeterminismMode> {
    raw.as_deref()
        .and_then(|s| RoutingDeterminismMode::from_str(s).ok())
}
