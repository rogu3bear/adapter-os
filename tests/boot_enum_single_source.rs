//! Compile-time guard test ensuring a single source of truth for boot state enums.
//!
//! This test ensures that `BootPhase` (from adapteros-boot) and `BootState`
//! (from adapteros-server-api) are the same type. The test itself is trivial,
//! but the win is that all crates compile against the same type, preventing
//! enum drift.
//!
//! If this test fails to compile, it means the enum consolidation has regressed
//! and there are multiple definitions of the boot state enum.

use adapteros_boot::BootPhase;
use adapteros_server_api::boot_state::BootState;

/// Compile-time assertion that BootPhase and BootState are the same type.
///
/// This function accepts a `BootPhase` and returns a `BootState`. If they are
/// different types, this will fail to compile.
fn assert_same_type(phase: BootPhase) -> BootState {
    phase
}

#[test]
fn boot_state_is_single_source() {
    // Verify that BootPhase and BootState are interchangeable
    let phase = BootPhase::Ready;
    let state: BootState = assert_same_type(phase);

    // Verify they have the same variants and string representation
    assert_eq!(phase.as_str(), state.as_str());
    assert_eq!(phase, state);
}

#[test]
fn boot_phase_variants_match_expected() {
    // Verify all expected variants exist and serialize to expected strings
    // This catches accidental variant additions or removals
    let expected_strings = [
        ("stopped", BootPhase::Stopped),
        ("starting", BootPhase::Starting),
        ("db-connecting", BootPhase::DbConnecting),
        ("migrating", BootPhase::Migrating),
        ("seeding", BootPhase::Seeding),
        ("loading-policies", BootPhase::LoadingPolicies),
        ("starting-backend", BootPhase::StartingBackend),
        ("loading-base-models", BootPhase::LoadingBaseModels),
        ("loading-adapters", BootPhase::LoadingAdapters),
        ("worker-discovery", BootPhase::WorkerDiscovery),
        ("ready", BootPhase::Ready),
        ("fully-ready", BootPhase::FullyReady),
        ("degraded", BootPhase::Degraded),
        ("failed", BootPhase::Failed),
        ("maintenance", BootPhase::Maintenance),
        ("draining", BootPhase::Draining),
        ("stopping", BootPhase::Stopping),
    ];

    for (expected_str, phase) in expected_strings {
        assert_eq!(
            phase.as_str(),
            expected_str,
            "BootPhase::{:?} should serialize to '{}'",
            phase,
            expected_str
        );
    }
}

#[test]
fn boot_state_serde_stability() {
    // Verify serialization produces stable kebab-case strings
    // This is critical for /readyz and boot report JSON output
    use serde_json;

    let phase = BootPhase::LoadingBaseModels;
    let serialized = serde_json::to_string(&phase).unwrap();
    assert_eq!(serialized, "\"loading-base-models\"");

    // Verify round-trip
    let deserialized: BootPhase = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, phase);
}
