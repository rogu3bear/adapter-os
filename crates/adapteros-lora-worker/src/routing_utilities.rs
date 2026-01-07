//! Routing utilities for router usage summarization and fusion intervals.
//!
//! This module provides helper functions for:
//! - Summarizing router usage from decisions
//! - Computing fusion interval hashes for deterministic weight fusion
//! - Grouping router decisions into fusion intervals

use crate::response_types::RouterSummary;
use adapteros_api_types::inference::{FusionIntervalTrace, RouterDecision};
use adapteros_core::{B3Hash, FusionInterval};
use serde::Serialize;

/// Summarize router usage for telemetry and replay.
///
/// Base-only requests produce empty adapter usage to make it explicit that the
/// base model handled the request without any adapter contribution.
pub fn summarize_router_usage(
    base_only_request: bool,
    active_ids: &[String],
    k_sparse: usize,
    router_decisions: Option<&[RouterDecision]>,
) -> RouterSummary {
    if base_only_request {
        return RouterSummary {
            adapters_used: Vec::new(),
            avg_activations: Vec::new(),
        };
    }

    if let Some(decisions) = router_decisions {
        let mut used: Vec<String> = decisions
            .iter()
            .flat_map(|d| d.candidate_adapters.iter())
            .filter_map(|c| active_ids.get(c.adapter_idx as usize))
            .cloned()
            .collect();
        used.sort();
        used.dedup();
        if !used.is_empty() {
            let take = used.len().min(k_sparse);
            return RouterSummary {
                adapters_used: used.into_iter().take(take).collect(),
                avg_activations: vec![0.33; take],
            };
        }
    }

    let adapters_used: Vec<String> = active_ids.iter().take(k_sparse).cloned().collect();
    let activation_len = adapters_used.len();
    RouterSummary {
        adapters_used,
        avg_activations: if activation_len == 0 {
            Vec::new()
        } else {
            vec![0.33; activation_len]
        },
    }
}

#[derive(Serialize)]
struct FusionCandidateMaterial {
    adapter_idx: u16,
    raw_score: f32,
    gate_q15: i16,
}

#[derive(Serialize)]
struct FusionDecisionMaterial {
    step: usize,
    input_token_id: Option<u32>,
    candidate_adapters: Vec<FusionCandidateMaterial>,
    entropy: f32,
    tau: f32,
    entropy_floor: f32,
    stack_hash: Option<String>,
    policy_mask_digest_b3: Option<B3Hash>,
    policy_overrides_applied: Option<adapteros_api_types::inference::PolicyOverrideFlags>,
    interval_id: Option<String>,
}

#[derive(Serialize)]
struct FusionIntervalMaterial {
    base_model_hash: B3Hash,
    interval_id: String,
    decisions: Vec<FusionDecisionMaterial>,
}

pub(crate) fn fused_hash_for_interval(
    base_model_hash: &B3Hash,
    interval_id: &str,
    decisions: &[RouterDecision],
) -> B3Hash {
    let material = FusionIntervalMaterial {
        base_model_hash: *base_model_hash,
        interval_id: interval_id.to_string(),
        decisions: decisions
            .iter()
            .map(|decision| FusionDecisionMaterial {
                step: decision.step,
                input_token_id: decision.input_token_id,
                candidate_adapters: decision
                    .candidate_adapters
                    .iter()
                    .map(|c| FusionCandidateMaterial {
                        adapter_idx: c.adapter_idx,
                        raw_score: c.raw_score,
                        gate_q15: c.gate_q15,
                    })
                    .collect(),
                entropy: decision.entropy,
                tau: decision.tau,
                entropy_floor: decision.entropy_floor,
                stack_hash: decision.stack_hash.clone(),
                policy_mask_digest_b3: decision.policy_mask_digest_b3,
                policy_overrides_applied: decision.policy_overrides_applied.clone(),
                interval_id: decision.interval_id.clone(),
            })
            .collect(),
    };

    // Canonical JSON ensures platform-stable byte layout for replay hashing.
    let canonical_bytes =
        serde_jcs::to_vec(&material).expect("fusion interval hash serialization must succeed");
    B3Hash::hash(&canonical_bytes)
}

pub(crate) fn fusion_intervals_for_mode(
    mode: FusionInterval,
    router_decisions: Option<&[RouterDecision]>,
    base_model_hash: &B3Hash,
) -> Option<Vec<FusionIntervalTrace>> {
    let decisions = router_decisions?;
    if decisions.is_empty() {
        return None;
    }

    let mut intervals = Vec::new();
    let mut start_idx = 0usize;
    let mut current_interval = decisions[0]
        .interval_id
        .clone()
        .unwrap_or_else(|| mode.interval_id_for_step(decisions[0].step));

    let mut push_bucket = |interval_id: &str, bucket: &[RouterDecision]| {
        if bucket.is_empty() {
            return;
        }
        let hash = fused_hash_for_interval(base_model_hash, interval_id, bucket);
        let start = bucket.first().map(|d| d.step).unwrap_or(0);
        let end = bucket.last().map(|d| d.step).unwrap_or(start);
        intervals.push(FusionIntervalTrace {
            interval_id: interval_id.to_string(),
            start_token: start,
            end_token: end,
            fused_weight_hash: hash,
        });
    };

    for (idx, decision) in decisions.iter().enumerate().skip(1) {
        let interval_id = decision
            .interval_id
            .clone()
            .unwrap_or_else(|| mode.interval_id_for_step(decision.step));

        if interval_id != current_interval {
            push_bucket(&current_interval, &decisions[start_idx..idx]);
            start_idx = idx;
            current_interval = interval_id;
        }
    }

    push_bucket(&current_interval, &decisions[start_idx..]);

    Some(intervals)
}

#[cfg(test)]
mod fusion_interval_tests {
    use super::*;
    use adapteros_api_types::inference::RouterCandidate;

    fn sample_decisions() -> Vec<RouterDecision> {
        vec![
            RouterDecision {
                step: 0,
                input_token_id: Some(1),
                candidate_adapters: vec![
                    RouterCandidate {
                        adapter_idx: 0,
                        raw_score: 0.8,
                        gate_q15: 20000,
                    },
                    RouterCandidate {
                        adapter_idx: 1,
                        raw_score: 0.2,
                        gate_q15: 5000,
                    },
                ],
                entropy: 0.4,
                tau: 1.0,
                entropy_floor: 0.1,
                allowed_mask: None,
                stack_hash: Some("stack-a".to_string()),
                policy_mask_digest_b3: None,
                policy_overrides_applied: None,
                interval_id: None,
                model_type: adapteros_api_types::inference::RouterModelType::Dense,
                backend_type: None,
            },
            RouterDecision {
                step: 1,
                input_token_id: Some(2),
                candidate_adapters: vec![RouterCandidate {
                    adapter_idx: 0,
                    raw_score: 0.5,
                    gate_q15: 15000,
                }],
                entropy: 0.5,
                tau: 1.0,
                entropy_floor: 0.1,
                allowed_mask: None,
                stack_hash: Some("stack-a".to_string()),
                policy_mask_digest_b3: None,
                policy_overrides_applied: None,
                interval_id: None,
                model_type: adapteros_api_types::inference::RouterModelType::Dense,
                backend_type: None,
            },
        ]
    }

    #[test]
    fn per_request_creates_single_interval() {
        let base = B3Hash::hash(b"base");
        let decisions = sample_decisions();
        let intervals = fusion_intervals_for_mode(
            FusionInterval::PerRequest,
            Some(decisions.as_slice()),
            &base,
        )
        .expect("intervals exist");

        assert_eq!(intervals.len(), 1);
        assert_eq!(intervals[0].interval_id, "request-0");
        assert_eq!(intervals[0].start_token, 0);
        assert_eq!(intervals[0].end_token, 1);
    }

    #[test]
    fn per_token_creates_interval_per_step() {
        let base = B3Hash::hash(b"base");
        let decisions = sample_decisions();
        let intervals =
            fusion_intervals_for_mode(FusionInterval::PerToken, Some(decisions.as_slice()), &base)
                .expect("intervals exist");

        assert_eq!(intervals.len(), decisions.len());
        assert_eq!(intervals[0].interval_id, "token-0");
        assert_eq!(intervals[1].interval_id, "token-1");
    }

    #[test]
    fn fused_hash_is_stable_for_same_inputs() {
        let base = B3Hash::hash(b"base");
        let decisions = sample_decisions();
        let first = fusion_intervals_for_mode(
            FusionInterval::PerRequest,
            Some(decisions.as_slice()),
            &base,
        )
        .expect("intervals");
        let second = fusion_intervals_for_mode(
            FusionInterval::PerRequest,
            Some(decisions.as_slice()),
            &base,
        )
        .expect("intervals");

        assert_eq!(
            first[0].fused_weight_hash, second[0].fused_weight_hash,
            "same inputs must produce identical fused hash"
        );
    }

    #[test]
    fn provided_interval_ids_are_honored() {
        let base = B3Hash::hash(b"base");
        let mut decisions = sample_decisions();
        decisions
            .iter_mut()
            .for_each(|d| d.interval_id = Some("segment-0".to_string()));

        let intervals =
            fusion_intervals_for_mode(FusionInterval::PerToken, Some(decisions.as_slice()), &base)
                .expect("intervals");

        assert_eq!(intervals.len(), 1, "custom interval ids control grouping");
        assert_eq!(intervals[0].interval_id, "segment-0");
        assert_eq!(intervals[0].start_token, 0);
        assert_eq!(intervals[0].end_token, 1);
    }
}

#[cfg(test)]
mod router_summary_tests {
    use super::summarize_router_usage;
    use adapteros_api_types::inference::{RouterCandidate, RouterDecision};

    #[test]
    fn base_only_summary_is_empty() {
        let empty_decisions: Vec<RouterDecision> = Vec::new();
        let summary = summarize_router_usage(true, &[], 2, Some(empty_decisions.as_slice()));
        assert!(summary.adapters_used.is_empty());
        assert!(summary.avg_activations.is_empty());
    }

    #[test]
    fn summarize_uses_active_ids_when_present() {
        let decisions = vec![RouterDecision {
            step: 0,
            input_token_id: None,
            candidate_adapters: vec![RouterCandidate {
                adapter_idx: 1,
                raw_score: 0.2,
                gate_q15: 1000,
            }],
            entropy: 0.0,
            tau: 0.0,
            entropy_floor: 0.0,
            stack_hash: None,
            interval_id: None,
            allowed_mask: None,
            policy_mask_digest_b3: None,
            policy_overrides_applied: None,
            model_type: adapteros_api_types::inference::RouterModelType::Dense,
            backend_type: None,
        }];
        let active_ids = vec!["adapter-a".to_string(), "adapter-b".to_string()];
        let summary = summarize_router_usage(false, &active_ids, 2, Some(decisions.as_slice()));
        assert_eq!(summary.adapters_used, vec!["adapter-b".to_string()]);
        assert_eq!(summary.avg_activations.len(), 1);
    }
}
