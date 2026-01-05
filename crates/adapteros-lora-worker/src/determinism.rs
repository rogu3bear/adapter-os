//! Determinism guard functions for strict mode and reproducibility
//!
//! This module provides:
//! - init_determinism_guards for worker initialization
//! - determinism_guards_enabled to check if guards are active
//! - determinism_violation_count to track violations
//! - Helper functions for strict mode enforcement
#![allow(clippy::assigning_clones)]
#![allow(clippy::cloned_ref_to_slice_refs)]

use adapteros_core::Result;
use tracing::info;

/// Initialize determinism guards for the worker
pub fn init_determinism_guards() -> Result<()> {
    // Initialize strict mode from environment variables
    // strict_mode::init_strict_mode();  // Temporarily disabled due to dependency issues

    // Initialize runtime guards
    // let guard_config = runtime_guards::GuardConfig {
    //     enabled: true,
    //     strict_mode: strict_mode::is_strict_mode(),
    //     max_violations: if strict_mode::is_strict_mode() { 1 } else { 10 },
    //     log_violations: true,
    // };

    // runtime_guards::init_guards(guard_config);

    info!("Determinism guards initialization temporarily disabled due to dependency issues");

    Ok(())
}

/// Check if determinism guards are enabled
pub fn determinism_guards_enabled() -> bool {
    // runtime_guards::guards_enabled()  // Temporarily disabled due to dependency issues
    false
}

/// Get current violation count
pub fn determinism_violation_count() -> u64 {
    // runtime_guards::violation_count()  // Temporarily disabled due to dependency issues
    0
}

#[cfg(test)]
mod strict_mode_guard_tests {
    use crate::{enforce_strict_router_chain, strict_mode_enabled};
    use adapteros_api_types::inference::RouterDecisionChainEntry;

    #[test]
    fn detects_strict_mode() {
        assert!(strict_mode_enabled(true, ""));
        assert!(strict_mode_enabled(false, "strict"));
        assert!(!strict_mode_enabled(false, "relaxed"));
    }

    #[test]
    fn strict_router_chain_requires_q15_gates() {
        let entry = RouterDecisionChainEntry {
            step: 0,
            input_token_id: Some(1),
            adapter_indices: vec![0, 1],
            adapter_ids: vec!["a".into(), "b".into()],
            gates_q15: vec![123, 456],
            entropy: 0.0,
            decision_hash: None,
            previous_hash: None,
            entry_hash: "h".into(),
            policy_mask_digest_b3: None,
            policy_overrides_applied: None,
        };

        // Happy path
        enforce_strict_router_chain(true, false, &[entry.clone()]).unwrap();

        // Missing gates should fail
        let mut missing = entry.clone();
        missing.gates_q15.clear();
        assert!(enforce_strict_router_chain(true, false, &[missing]).is_err());

        // Mismatched gate count should fail
        let mut mismatched = entry;
        mismatched.gates_q15 = vec![123];
        assert!(enforce_strict_router_chain(true, false, &[mismatched]).is_err());
    }
}
