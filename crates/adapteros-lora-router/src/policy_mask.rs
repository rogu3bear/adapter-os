use std::collections::HashSet;

use adapteros_core::B3Hash;

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
            .collect::<std::collections::HashMap<_, _>>();

        if let Some(allow_ids) = allowed_adapter_ids {
            overrides_applied.allow_list = true;
            allowed.fill(false);
            for adapter_id in allow_ids {
                if let Some(idx) = id_to_index.get(adapter_id.as_str()) {
                    if let Some(slot) = allowed.get_mut(*idx) {
                        *slot = true;
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
