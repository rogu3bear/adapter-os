use std::collections::HashSet;

use adapteros_api_types::RoutingPolicy;
use adapteros_core::{AosError, B3Hash, Result};
use smallvec::SmallVec;

use crate::Decision;

/// Flags indicating which policy overrides were applied when building the mask.
#[derive(Debug, Clone, Default)]
pub struct PolicyOverrideFlags {
    /// True when an allowlist constrained the effective set.
    pub allow_list: bool,
    /// True when a denylist removed adapters from the effective set.
    pub deny_list: bool,
    /// True when trust-state rules blocked adapters.
    pub trust_state: bool,
}

/// Deterministic allow/deny mask derived from routing policy state.
#[derive(Debug, Clone)]
pub struct PolicyMask {
    /// Per-adapter allow bit aligned with adapter ordering.
    pub allowed: Vec<bool>,
    /// Digest binding policy state to the mask bits.
    pub digest: B3Hash,
    /// Which override sources were applied when producing the mask.
    pub overrides_applied: PolicyOverrideFlags,
}

impl PolicyMask {
    /// Build an allow-all mask that still binds to the adapter list and policy digest.
    pub fn allow_all(adapter_ids: &[String], context_policy_digest: Option<B3Hash>) -> Self {
        let allowed = vec![true; adapter_ids.len()];
        let digest = compute_policy_mask_digest(context_policy_digest, adapter_ids, &allowed);
        Self {
            allowed,
            digest,
            overrides_applied: PolicyOverrideFlags::default(),
        }
    }

    /// Build a deny-all mask for fail-closed behavior when policy input is missing.
    pub fn deny_all(adapter_ids: &[String], context_policy_digest: Option<B3Hash>) -> Self {
        let allowed = vec![false; adapter_ids.len()];
        let digest = compute_policy_mask_digest(context_policy_digest, adapter_ids, &allowed);
        Self {
            allowed,
            digest,
            overrides_applied: PolicyOverrideFlags::default(),
        }
    }

    /// Build a policy mask from allow/deny lists and optional index gates.
    ///
    /// - `adapter_ids` must align with router ordering.
    /// - `allowed_adapter_ids` and `denied_adapter_ids` are string IDs.
    /// - `allowed_indices_gate` lets callers pre-restrict by index (e.g., active stack).
    /// - `trust_blocked_indices` allows injecting trust_state-derived blocks.
    /// - `context_policy_digest` binds the mask to upstream policy state.
    pub fn build(
        adapter_ids: &[String],
        allowed_adapter_ids: Option<&[String]>,
        denied_adapter_ids: Option<&[String]>,
        allowed_indices_gate: Option<&HashSet<usize>>,
        trust_blocked_indices: Option<&HashSet<usize>>,
        context_policy_digest: Option<B3Hash>,
    ) -> Self {
        let mut overrides_applied = PolicyOverrideFlags::default();
        let mut allowed = match allowed_indices_gate {
            Some(indices) => {
                overrides_applied.allow_list = true;
                adapter_ids
                    .iter()
                    .enumerate()
                    .map(|(idx, _)| indices.contains(&idx))
                    .collect::<Vec<bool>>()
            }
            None => vec![true; adapter_ids.len()],
        };

        let id_to_index = adapter_ids
            .iter()
            .enumerate()
            .map(|(idx, id)| (id.as_str(), idx))
            .collect::<std::collections::BTreeMap<_, _>>();

        if let Some(allow_ids) = allowed_adapter_ids {
            overrides_applied.allow_list = true;
            allowed.fill(false);
            for adapter_id in allow_ids {
                if let Some(idx) = id_to_index.get(adapter_id.as_str()) {
                    if let Some(slot) = allowed.get_mut(*idx) {
                        *slot = true;
                    }
                } else {
                    // #164: Log warning when allowlist references non-existent adapter
                    tracing::warn!(
                        adapter_id = %adapter_id,
                        "Allowlist references adapter ID not found in registry - ignored"
                    );
                }
            }
        }

        if let Some(deny_ids) = denied_adapter_ids {
            overrides_applied.deny_list = true;
            for adapter_id in deny_ids {
                if let Some(idx) = id_to_index.get(adapter_id.as_str()) {
                    if let Some(slot) = allowed.get_mut(*idx) {
                        *slot = false;
                    }
                } else {
                    // #164: Log warning when denylist references non-existent adapter
                    tracing::warn!(
                        adapter_id = %adapter_id,
                        "Denylist references adapter ID not found in registry - ignored"
                    );
                }
            }
        }

        if let Some(blocked) = trust_blocked_indices {
            overrides_applied.trust_state = true;
            for idx in blocked {
                if let Some(slot) = allowed.get_mut(*idx) {
                    *slot = false;
                }
            }
        }

        let digest = compute_policy_mask_digest(context_policy_digest, adapter_ids, &allowed);

        Self {
            allowed,
            digest,
            overrides_applied,
        }
    }
}

/// Compute a deterministic digest for the policy mask.
///
/// `policy_mask_digest = H(context_policy_digest ∥ adapter_id_list ∥ allowed_bits)`
pub fn compute_policy_mask_digest(
    context_policy_digest: Option<B3Hash>,
    adapter_ids: &[String],
    allowed_bits: &[bool],
) -> B3Hash {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(
        context_policy_digest
            .unwrap_or_else(B3Hash::zero)
            .as_bytes(),
    );
    for id in adapter_ids {
        bytes.extend_from_slice(id.as_bytes());
        bytes.push(0);
    }
    bytes.extend(allowed_bits.iter().map(|b| if *b { 1u8 } else { 0u8 }));
    B3Hash::hash(&bytes)
}

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
    adapter_clusters: &[Option<String>],
    policy: Option<&RoutingPolicy>,
) -> Result<Decision> {
    let policy_mask_digest_b3 = decision.policy_mask_digest_b3;
    let policy_overrides_applied = decision.policy_overrides_applied.clone();

    if adapter_ids.len() != adapter_clusters.len() {
        return Err(AosError::PolicyViolation(
            "RoutingPolicy adapter ids/clusters length mismatch".to_string(),
        ));
    }

    let Some(policy) = policy else {
        return Ok(decision);
    };

    let allowed_set = policy
        .allowed_adapter_ids
        .as_ref()
        .map(|ids| ids.iter().collect::<HashSet<&String>>());
    let denied_set = policy
        .denied_adapter_ids
        .as_ref()
        .map(|ids| ids.iter().collect::<HashSet<&String>>());
    let allowed_clusters = policy
        .allowed_clusters
        .as_ref()
        .map(|ids| ids.iter().collect::<HashSet<&String>>());
    let denied_clusters = policy
        .denied_clusters
        .as_ref()
        .map(|ids| ids.iter().collect::<HashSet<&String>>());

    let mut filtered_indices = SmallVec::<[u16; 8]>::new();
    let mut filtered_gates = SmallVec::<[i16; 8]>::new();
    let mut filtered_candidates = Vec::new();
    let mut cluster_filtered = 0usize;

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

        let cluster = adapter_clusters
            .get(*adapter_idx as usize)
            .and_then(|c| c.as_ref());

        if let Some(allowed) = &allowed_clusters {
            if cluster.map(|c| !allowed.contains(c)).unwrap_or(true) {
                cluster_filtered += 1;
                continue;
            }
        }

        if let Some(denied) = &denied_clusters {
            if cluster.map(|c| denied.contains(c)).unwrap_or(false) {
                cluster_filtered += 1;
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
        let reason = if cluster_filtered > 0 {
            "Routing policy denied all adapters for this token (clusters)".to_string()
        } else {
            "Routing policy denied all adapters for this token".to_string()
        };
        return Err(AosError::PolicyViolation(reason));
    }

    Ok(Decision {
        indices: filtered_indices,
        gates_q15: filtered_gates,
        entropy: decision.entropy,
        candidates: filtered_candidates,
        decision_hash: decision.decision_hash,
        policy_mask_digest_b3,
        policy_overrides_applied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that non-existent adapter IDs in allowlist are handled gracefully (#164)
    #[test]
    fn allowlist_nonexistent_adapter_handled() {
        let adapter_ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let policy_digest = B3Hash::hash(b"policy-state");

        // Reference a non-existent adapter "z" - should not panic, just ignore
        let mask = PolicyMask::build(
            &adapter_ids,
            Some(&vec!["a".to_string(), "z".to_string()]),
            None,
            None,
            None,
            Some(policy_digest),
        );

        // Only "a" should be allowed (the valid one from the allowlist)
        assert_eq!(mask.allowed, vec![true, false, false]);
        assert!(mask.overrides_applied.allow_list);
    }

    /// Test that non-existent adapter IDs in denylist are handled gracefully (#164)
    #[test]
    fn denylist_nonexistent_adapter_handled() {
        let adapter_ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let policy_digest = B3Hash::hash(b"policy-state");

        // Reference a non-existent adapter "z" - should not panic, just ignore
        let mask = PolicyMask::build(
            &adapter_ids,
            None,
            Some(&vec!["b".to_string(), "nonexistent".to_string()]),
            None,
            None,
            Some(policy_digest),
        );

        // "b" should be denied, "a" and "c" allowed, "nonexistent" ignored
        assert_eq!(mask.allowed, vec![true, false, true]);
        assert!(mask.overrides_applied.deny_list);
    }

    #[test]
    fn deny_mask_excludes_adapters() {
        let adapter_ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let policy_digest = B3Hash::hash(b"policy-state");

        let mask = PolicyMask::build(
            &adapter_ids,
            None,
            Some(&vec!["b".to_string()]),
            None,
            None,
            Some(policy_digest),
        );

        assert_eq!(mask.allowed, vec![true, false, true]);
        assert!(mask.overrides_applied.deny_list);
        assert!(!mask.overrides_applied.trust_state);
    }

    #[test]
    fn allowlist_limits_set() {
        let adapter_ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let policy_digest = B3Hash::hash(b"policy-state");

        let mask = PolicyMask::build(
            &adapter_ids,
            Some(&vec!["c".to_string()]),
            None,
            None,
            None,
            Some(policy_digest),
        );

        assert_eq!(mask.allowed, vec![false, false, true]);
        assert!(mask.overrides_applied.allow_list);
    }

    #[test]
    fn digest_changes_with_policy() {
        let adapter_ids = vec!["a".to_string(), "b".to_string()];
        let base_digest = B3Hash::hash(b"policy-state");
        let mask_a = PolicyMask::build(
            &adapter_ids,
            None,
            Some(&vec!["b".to_string()]),
            None,
            None,
            Some(base_digest),
        );

        let mask_b = PolicyMask::build(
            &adapter_ids,
            None,
            Some(&vec!["a".to_string()]),
            None,
            None,
            Some(base_digest),
        );

        assert_ne!(mask_a.digest, mask_b.digest);
    }
}
