//! Trust state derivation functions for dataset versions.
//!
//! These functions determine the trust state and safety status of dataset versions
//! based on validation results and safety signals.

// ============================================================================
// Safety Status Derivation
// ============================================================================

/// Derive aggregate safety status from individual signals.
///
/// Returns:
/// - `"block"` if any signal is "block" or "unsafe"
/// - `"warn"` if any signal is "warn"
/// - `"unknown"` if all signals are "unknown"
/// - `"clean"` otherwise
pub(crate) fn derive_overall_safety_status(
    pii_status: &str,
    toxicity_status: &str,
    leak_status: &str,
    anomaly_status: &str,
) -> String {
    let signals = [pii_status, toxicity_status, leak_status, anomaly_status];
    if signals
        .iter()
        .any(|s| s.eq_ignore_ascii_case("block") || s.eq_ignore_ascii_case("unsafe"))
    {
        "block".to_string()
    } else if signals.iter().any(|s| s.eq_ignore_ascii_case("warn")) {
        "warn".to_string()
    } else if signals.iter().all(|s| s.eq_ignore_ascii_case("unknown")) {
        "unknown".to_string()
    } else {
        "clean".to_string()
    }
}

// ============================================================================
// Trust State Derivation
// ============================================================================

/// Derive trust_state for a dataset version.
///
/// Canonical semantics:
/// - `allowed`: validation passed and no safety warnings.
/// - `allowed_with_warning`: validation passed but at least one safety signal warned.
/// - `needs_approval`: validation is pending/validating or any safety signal is unresolved.
/// - `blocked`: validation failed/invalid or any safety signal blocked.
/// - `unknown`: trust not evaluated (explicit `validation_status == unknown`).
///
/// Training gates block `blocked`, `needs_approval`, and `unknown`. Adapter trust
/// aggregates per-dataset trust using `map_dataset_trust_to_adapter_trust` in
/// `adapter_repositories.rs` (priority: blocked > warn > unknown > allowed).
pub(crate) fn derive_trust_state(
    validation_status: &str,
    pii_status: &str,
    toxicity_status: &str,
    leak_status: &str,
    anomaly_status: &str,
    override_state: Option<&str>,
) -> String {
    if let Some(ov) = override_state {
        return ov.trim().to_ascii_lowercase();
    }

    let validation_lower = validation_status.trim().to_ascii_lowercase();
    if validation_lower == "invalid" || validation_lower == "failed" {
        return "blocked".to_string();
    }

    let safety_block = [pii_status, toxicity_status, leak_status, anomaly_status]
        .iter()
        .any(|s| s.eq_ignore_ascii_case("block") || s.eq_ignore_ascii_case("unsafe"));
    if safety_block {
        return "blocked".to_string();
    }

    let safety_warn = [pii_status, toxicity_status, leak_status, anomaly_status]
        .iter()
        .any(|s| s.eq_ignore_ascii_case("warn"));

    let safety_unknown = [pii_status, toxicity_status, leak_status, anomaly_status]
        .iter()
        .any(|s| s.eq_ignore_ascii_case("unknown"));

    if validation_lower == "unknown" {
        return "unknown".to_string();
    }

    if validation_lower == "pending" || validation_lower == "validating" {
        return "needs_approval".to_string();
    }

    if safety_unknown {
        return "needs_approval".to_string();
    }

    if safety_warn {
        "allowed_with_warning".to_string()
    } else {
        "allowed".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // Trust State Derivation Tests
    // ============================================================================

    #[test]
    fn trust_blocked_on_validation_failed() {
        let state = derive_trust_state("failed", "clean", "clean", "clean", "clean", None);
        assert_eq!(state, "blocked");
    }

    #[test]
    fn trust_blocked_on_validation_invalid() {
        let state = derive_trust_state("invalid", "clean", "clean", "clean", "clean", None);
        assert_eq!(state, "blocked");
    }

    #[test]
    fn trust_blocked_on_safety_block() {
        let state = derive_trust_state("passed", "block", "clean", "clean", "clean", None);
        assert_eq!(state, "blocked");
    }

    #[test]
    fn trust_blocked_on_safety_unsafe() {
        let state = derive_trust_state("passed", "clean", "unsafe", "clean", "clean", None);
        assert_eq!(state, "blocked");
    }

    #[test]
    fn trust_allowed_with_warning_on_warn() {
        let state = derive_trust_state("passed", "warn", "clean", "clean", "clean", None);
        assert_eq!(state, "allowed_with_warning");
    }

    #[test]
    fn trust_needs_approval_on_pending() {
        let state = derive_trust_state("pending", "clean", "clean", "clean", "clean", None);
        assert_eq!(state, "needs_approval");
    }

    #[test]
    fn trust_needs_approval_on_unknown_safety() {
        let state = derive_trust_state("passed", "unknown", "clean", "clean", "clean", None);
        assert_eq!(state, "needs_approval");
    }

    #[test]
    fn trust_unknown_on_unknown_validation() {
        let state = derive_trust_state("unknown", "clean", "clean", "clean", "clean", None);
        assert_eq!(state, "unknown");
    }

    #[test]
    fn trust_allowed_when_all_clean() {
        let state = derive_trust_state("passed", "clean", "clean", "clean", "clean", None);
        assert_eq!(state, "allowed");
    }

    #[test]
    fn trust_override_takes_precedence() {
        let state = derive_trust_state(
            "failed",
            "block",
            "unsafe",
            "warn",
            "unknown",
            Some("allowed"),
        );
        assert_eq!(state, "allowed");
    }

    // ============================================================================
    // Safety Status Derivation Tests
    // ============================================================================

    #[test]
    fn safety_block_on_block() {
        let safety = derive_overall_safety_status("block", "clean", "clean", "clean");
        assert_eq!(safety, "block");
    }

    #[test]
    fn safety_block_on_unsafe() {
        let safety = derive_overall_safety_status("clean", "unsafe", "clean", "clean");
        assert_eq!(safety, "block");
    }

    #[test]
    fn safety_warn_on_warn() {
        let safety = derive_overall_safety_status("clean", "warn", "clean", "clean");
        assert_eq!(safety, "warn");
    }

    #[test]
    fn safety_unknown_when_all_unknown() {
        let safety = derive_overall_safety_status("unknown", "unknown", "unknown", "unknown");
        assert_eq!(safety, "unknown");
    }

    #[test]
    fn safety_clean_when_all_clean() {
        let safety = derive_overall_safety_status("clean", "clean", "clean", "clean");
        assert_eq!(safety, "clean");
    }
}
