//! Determinism mode helpers for inference execution.
//!
//! This module provides utilities for resolving and validating determinism
//! modes across the stack > tenant > global precedence hierarchy.

use crate::types::InferenceError;
use adapteros_core::determinism_mode::DeterminismMode;

/// Resolve determinism mode using stack > tenant > global precedence
pub fn resolve_determinism_mode(
    stack_mode: Option<&str>,
    tenant_mode: Option<&str>,
    global_mode: &str,
) -> DeterminismMode {
    if let Some(mode) = stack_mode {
        return DeterminismMode::from(mode);
    }
    if let Some(mode) = tenant_mode {
        return DeterminismMode::from(mode);
    }
    DeterminismMode::from(global_mode)
}

/// Compute strict_mode flag for worker/coordinator behavior
pub fn compute_strict_mode(mode: DeterminismMode, allow_fallback: bool) -> bool {
    mode == DeterminismMode::Strict || !allow_fallback
}

/// Validate strict mode requirements (seed required)
pub fn validate_strict_mode_constraints(
    mode: DeterminismMode,
    seed: Option<u64>,
) -> Result<(), InferenceError> {
    if mode == DeterminismMode::Strict && seed.is_none() {
        return Err(InferenceError::ValidationError(
            "Strict determinism mode requires a seed".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_determinism_mode_stack_takes_precedence() {
        let mode = resolve_determinism_mode(Some("strict"), Some("relaxed"), "besteffort");
        assert_eq!(mode, DeterminismMode::Strict);
    }

    #[test]
    fn test_resolve_determinism_mode_tenant_fallback() {
        let mode = resolve_determinism_mode(None, Some("relaxed"), "strict");
        assert_eq!(mode, DeterminismMode::Relaxed);
    }

    #[test]
    fn test_resolve_determinism_mode_global_fallback() {
        let mode = resolve_determinism_mode(None, None, "strict");
        assert_eq!(mode, DeterminismMode::Strict);
    }

    #[test]
    fn test_compute_strict_mode_strict_mode() {
        let strict_mode = compute_strict_mode(DeterminismMode::Strict, true);
        assert!(strict_mode, "Strict mode should always return true");
    }

    #[test]
    fn test_compute_strict_mode_with_fallback_disabled() {
        let strict_mode = compute_strict_mode(DeterminismMode::BestEffort, false);
        assert!(strict_mode, "Fallback disabled should enable strict mode");
    }

    #[test]
    fn test_compute_strict_mode_besteffort_with_fallback() {
        let strict_mode = compute_strict_mode(DeterminismMode::BestEffort, true);
        assert!(
            !strict_mode,
            "BestEffort with fallback should not be strict"
        );
    }

    #[test]
    fn test_validate_strict_mode_constraints_requires_seed() {
        let result = validate_strict_mode_constraints(DeterminismMode::Strict, None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("seed"));
    }

    #[test]
    fn test_validate_strict_mode_constraints_with_seed() {
        let result = validate_strict_mode_constraints(DeterminismMode::Strict, Some(12345));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_strict_mode_constraints_relaxed_no_seed() {
        // Relaxed mode doesn't require seed
        let result = validate_strict_mode_constraints(DeterminismMode::Relaxed, None);
        assert!(result.is_ok());
    }
}
