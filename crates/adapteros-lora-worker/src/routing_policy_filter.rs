use std::collections::HashSet;

use adapteros_api_types::RoutingPolicy;
use adapteros_core::{AosError, Result};
use adapteros_lora_router::Decision;
use smallvec::SmallVec;

/// Deterministically filter a router Decision using a RoutingPolicy.
///
/// - Runs after router scoring and before kernel execution.
/// - Preserves original decision order; only removes entries.
/// - Does not renormalize gates to avoid changing deterministic gate values.
/// - Returns PolicyViolation when no adapters remain after filtering.
///
/// Limitations / future hooks:
/// - Only ID-based allow/deny and max-adapters cap; no tag/tier/stack checks yet.
/// - Gate values are left as-is; optional renormalization could be added later.
/// - Max cap truncates deterministically using router ordering for stability.
pub fn filter_decision_by_policy(
    decision: Decision,
    adapter_ids: &[String],
    policy: Option<&RoutingPolicy>,
) -> Result<Decision> {
    let Some(policy) = policy else {
        return Ok(decision);
    };

    let allowed_set = policy
        .allowed_adapter_ids
        .as_ref()
        .map(|ids| ids.iter().cloned().collect::<HashSet<String>>());
    let denied_set = policy
        .denied_adapter_ids
        .as_ref()
        .map(|ids| ids.iter().cloned().collect::<HashSet<String>>());

    let mut filtered_indices = SmallVec::<[u16; 8]>::new();
    let mut filtered_gates = SmallVec::<[i16; 8]>::new();
    let mut filtered_candidates = Vec::new();

    for (i, (adapter_idx, gate_q15)) in decision
        .indices
        .iter()
        .zip(decision.gates_q15.iter())
        .enumerate()
    {
        let adapter_id = adapter_ids.get(*adapter_idx as usize).ok_or_else(|| {
            AosError::PolicyViolation(format!(
                "RoutingPolicy adapter index {} out of bounds",
                adapter_idx
            ))
        })?;

        if let Some(allowed) = &allowed_set {
            if !allowed.contains(adapter_id) {
                continue;
            }
        }

        if let Some(denied) = &denied_set {
            if denied.contains(adapter_id) {
                continue;
            }
        }

        filtered_indices.push(*adapter_idx);
        filtered_gates.push(*gate_q15);
        if let Some(candidate) = decision.candidates.get(i) {
            filtered_candidates.push(candidate.clone());
        }
    }

    if let Some(max) = policy.max_adapters_per_token {
        if max == 0 {
            return Err(AosError::PolicyViolation(
                "Routing policy rejected all adapters (max_adapters_per_token=0)".to_string(),
            ));
        }
        if filtered_indices.len() > max {
            filtered_indices.truncate(max);
            filtered_gates.truncate(max);
            filtered_candidates.truncate(max);
        }
    }

    if filtered_indices.is_empty() {
        return Err(AosError::PolicyViolation(
            "Routing policy denied all adapters for this token".to_string(),
        ));
    }

    Ok(Decision {
        indices: filtered_indices,
        gates_q15: filtered_gates,
        entropy: decision.entropy,
        candidates: filtered_candidates,
        decision_hash: decision.decision_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_api_types::RoutingPolicy;
    use adapteros_core::AosError;

    fn decision(indices: &[u16]) -> Decision {
        Decision {
            indices: SmallVec::from_slice(indices),
            gates_q15: SmallVec::from_slice(&[1200, 1100, 1000]),
            entropy: 0.0,
            candidates: Vec::new(),
            decision_hash: None,
        }
    }

    #[test]
    fn policy_allow_and_cap_preserves_order() {
        let adapter_ids = vec![
            "a-primary".to_string(),
            "b-secondary".to_string(),
            "c-third".to_string(),
        ];
        let policy = RoutingPolicy {
            allowed_adapter_ids: Some(vec!["b-secondary".to_string(), "a-primary".to_string()]),
            denied_adapter_ids: None,
            max_adapters_per_token: Some(1),
            ..Default::default()
        };

        let filtered = filter_decision_by_policy(decision(&[0, 1, 2]), &adapter_ids, Some(&policy))
            .expect("policy should allow one adapter");

        // Order comes from router decision; cap truncates without reordering.
        assert_eq!(filtered.indices.as_slice(), &[0u16]);
        assert_eq!(filtered.gates_q15.as_slice(), &[1200i16]);
    }

    #[test]
    fn policy_denies_all_errors() {
        let adapter_ids = vec!["only".to_string()];
        let policy = RoutingPolicy {
            denied_adapter_ids: Some(vec!["only".to_string()]),
            max_adapters_per_token: None,
            ..Default::default()
        };

        let result = filter_decision_by_policy(decision(&[0]), &adapter_ids, Some(&policy));
        assert!(matches!(result, Err(AosError::PolicyViolation(_))));
    }
}
