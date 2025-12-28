//! Replay guarantee computation and runtime guards.
//!
//! This module provides utilities for computing replay guarantees based on
//! determinism mode and execution path, as well as enforcing strict runtime
//! guards on worker responses.

use crate::types::InferenceError;
use adapteros_api_types::inference::{
    ReplayGuarantee, RouterDecisionChainEntry as ApiRouterDecisionChainEntry,
};
use adapteros_core::{determinism_mode::DeterminismMode, BackendKind, B3Hash};
use std::str::FromStr;

/// Compute replay guarantee based on determinism mode and execution path
pub fn compute_replay_guarantee(
    mode: DeterminismMode,
    fallback_triggered: bool,
    prompt_truncated: bool,
    response_truncated: bool,
    seed_present: bool,
) -> ReplayGuarantee {
    match mode {
        DeterminismMode::Strict => {
            if fallback_triggered || prompt_truncated || response_truncated || !seed_present {
                ReplayGuarantee::Approximate
            } else {
                ReplayGuarantee::Exact
            }
        }
        DeterminismMode::BestEffort => ReplayGuarantee::Approximate,
        DeterminismMode::Relaxed => ReplayGuarantee::None,
    }
}

/// Enforce strict determinism runtime guards on worker responses.
///
/// - Known backend identifier is required (fails on unknown/blank backend)
/// - Backend version (kernel_version_id) must match the running build
/// - Router decision chain with Q15 gates must be present when adapters are used
/// - Canonical manifest hash is required to bind seeds/context
pub fn enforce_strict_runtime_guards(
    mode: DeterminismMode,
    backend_used: &Option<String>,
    backend_version: &Option<String>,
    router_chain: &Option<Vec<ApiRouterDecisionChainEntry>>,
    adapters_used: &[String],
    manifest_hash: Option<&B3Hash>,
) -> Result<(), InferenceError> {
    if mode != DeterminismMode::Strict {
        return Ok(());
    }

    if manifest_hash.is_none() {
        return Err(InferenceError::ValidationError(
            "Strict determinism mode requires canonical manifest context".to_string(),
        ));
    }

    let backend_name = backend_used.as_ref().ok_or_else(|| {
        InferenceError::WorkerError(
            "Strict determinism mode requires a reported backend (backend_used)".to_string(),
        )
    })?;

    BackendKind::from_str(backend_name).map_err(|e| {
        InferenceError::WorkerError(format!(
            "Strict determinism mode requires a known backend: {}",
            e
        ))
    })?;

    let kernel_version_id = backend_version.as_ref().ok_or_else(|| {
        InferenceError::WorkerError(
            "Strict determinism mode requires kernel_version_id from worker".to_string(),
        )
    })?;

    if kernel_version_id != adapteros_core::version::VERSION {
        return Err(InferenceError::WorkerError(format!(
            "kernel_version_id mismatch: expected {}, got {}",
            adapteros_core::version::VERSION,
            kernel_version_id
        )));
    }

    // Only enforce routing evidence when adapters are active; base-only requests
    // do not emit router decisions.
    if adapters_used.is_empty() {
        return Ok(());
    }

    let chain = router_chain.as_ref().ok_or_else(|| {
        InferenceError::WorkerError(
            "Strict determinism mode requires router_decision_chain with Q15 gates".to_string(),
        )
    })?;

    if chain.is_empty() {
        return Err(InferenceError::WorkerError(
            "Strict determinism mode requires non-empty router_decision_chain".to_string(),
        ));
    }

    for entry in chain {
        if entry.gates_q15.is_empty() {
            return Err(InferenceError::WorkerError(
                "Strict determinism mode forbids float-only gates; Q15 gates missing".to_string(),
            ));
        }
        if entry.adapter_indices.len() != entry.gates_q15.len() {
            return Err(InferenceError::WorkerError(format!(
                "Router decision gate count mismatch (indices={}, gates={})",
                entry.adapter_indices.len(),
                entry.gates_q15.len()
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_api_types::inference::RouterDecisionChainEntry as ApiRouterDecisionChainEntry;

    #[test]
    fn test_compute_replay_guarantee_exact_strict() {
        let guarantee = compute_replay_guarantee(
            DeterminismMode::Strict,
            false, // fallback_triggered
            false, // prompt_truncated
            false, // response_truncated
            true,  // seed_present
        );
        assert_eq!(guarantee, ReplayGuarantee::Exact);
    }

    #[test]
    fn test_compute_replay_guarantee_fallback_degrades_to_approximate() {
        let guarantee = compute_replay_guarantee(
            DeterminismMode::Strict,
            true, // fallback_triggered
            false,
            false,
            true,
        );
        assert_eq!(guarantee, ReplayGuarantee::Approximate);
    }

    #[test]
    fn test_compute_replay_guarantee_missing_seed_degrades() {
        let guarantee = compute_replay_guarantee(
            DeterminismMode::Strict,
            false,
            false,
            false,
            false, // seed not present
        );
        assert_eq!(guarantee, ReplayGuarantee::Approximate);
    }

    #[test]
    fn test_compute_replay_guarantee_best_effort_always_approximate() {
        let guarantee =
            compute_replay_guarantee(DeterminismMode::BestEffort, false, false, false, true);
        assert_eq!(guarantee, ReplayGuarantee::Approximate);
    }

    #[test]
    fn test_compute_replay_guarantee_relaxed_always_none() {
        let guarantee =
            compute_replay_guarantee(DeterminismMode::Relaxed, false, false, false, true);
        assert_eq!(guarantee, ReplayGuarantee::None);
    }

    #[test]
    fn strict_runtime_guard_rejects_unknown_backend() {
        let manifest = B3Hash::hash(b"manifest");
        let chain = vec![ApiRouterDecisionChainEntry {
            step: 0,
            input_token_id: Some(1),
            adapter_indices: vec![0],
            adapter_ids: vec!["a".into()],
            gates_q15: vec![123],
            entropy: 0.0,
            decision_hash: None,
            previous_hash: None,
            entry_hash: "h".into(),
            policy_mask_digest: None,
            policy_overrides_applied: None,
        }];

        let err = enforce_strict_runtime_guards(
            DeterminismMode::Strict,
            &Some("mystery".into()),
            &Some(adapteros_core::version::VERSION.to_string()),
            &Some(chain),
            &[String::from("adapter-a")],
            Some(&manifest),
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("known backend"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn strict_runtime_guard_allows_base_only_without_chain() {
        let manifest = B3Hash::hash(b"manifest");
        enforce_strict_runtime_guards(
            DeterminismMode::Strict,
            &Some("coreml".into()),
            &Some(adapteros_core::version::VERSION.to_string()),
            &None,
            &[],
            Some(&manifest),
        )
        .expect("base-only strict mode should not require router chain");
    }

    #[test]
    fn test_strict_runtime_guard_missing_manifest_fails() {
        let err = enforce_strict_runtime_guards(
            DeterminismMode::Strict,
            &Some("mlx".into()),
            &Some(adapteros_core::version::VERSION.to_string()),
            &None,
            &[],
            None, // No manifest
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("manifest"),
            "Should mention manifest: {}",
            err
        );
    }

    #[test]
    fn test_strict_runtime_guard_missing_backend_fails() {
        let manifest = B3Hash::hash(b"manifest");
        let err = enforce_strict_runtime_guards(
            DeterminismMode::Strict,
            &None, // No backend
            &Some(adapteros_core::version::VERSION.to_string()),
            &None,
            &[],
            Some(&manifest),
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("backend"),
            "Should mention backend: {}",
            err
        );
    }

    #[test]
    fn test_strict_runtime_guard_version_mismatch_fails() {
        let manifest = B3Hash::hash(b"manifest");
        let err = enforce_strict_runtime_guards(
            DeterminismMode::Strict,
            &Some("coreml".into()),
            &Some("0.0.0-mismatch".to_string()), // Wrong version
            &None,
            &[],
            Some(&manifest),
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("mismatch"),
            "Should mention mismatch: {}",
            err
        );
    }

    #[test]
    fn test_strict_runtime_guard_empty_chain_with_adapters_fails() {
        let manifest = B3Hash::hash(b"manifest");
        let err = enforce_strict_runtime_guards(
            DeterminismMode::Strict,
            &Some("coreml".into()),
            &Some(adapteros_core::version::VERSION.to_string()),
            &Some(vec![]),              // Empty chain
            &["adapter-a".to_string()], // But adapters used
            Some(&manifest),
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("non-empty"),
            "Should require non-empty chain: {}",
            err
        );
    }

    #[test]
    fn test_strict_runtime_guard_gate_count_mismatch_fails() {
        let manifest = B3Hash::hash(b"manifest");
        let chain = vec![ApiRouterDecisionChainEntry {
            step: 0,
            input_token_id: Some(1),
            adapter_indices: vec![0, 1], // 2 indices
            adapter_ids: vec!["a".into(), "b".into()],
            gates_q15: vec![100], // Only 1 gate - mismatch!
            entropy: 0.0,
            decision_hash: None,
            previous_hash: None,
            entry_hash: "h".into(),
            policy_mask_digest: None,
            policy_overrides_applied: None,
        }];

        let err = enforce_strict_runtime_guards(
            DeterminismMode::Strict,
            &Some("coreml".into()),
            &Some(adapteros_core::version::VERSION.to_string()),
            &Some(chain),
            &["adapter-a".to_string()],
            Some(&manifest),
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("mismatch"),
            "Should mention gate count mismatch: {}",
            err
        );
    }
}
