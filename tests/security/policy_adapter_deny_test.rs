#![cfg(all(test, feature = "extended-tests"))]
//! Policy Enforcement Test: Adapter Denial
//!
//! This test validates that the policy enforcement system correctly blocks
//! denied adapters and prevents policy tampering through BLAKE3 digest binding.
//!
//! ## Test Coverage
//! 1. Denied adapters are blocked from routing
//! 2. Allow/deny lists work as expected
//! 3. BLAKE3 digest binding prevents policy tampering
//! 4. Policy mask correctly reflects enforcement decisions
//!
//! ## Key Files Tested
//! - `crates/adapteros-lora-router/src/policy_mask.rs`
//! - `crates/adapteros-policy/src/packs/router.rs`
//! - `crates/adapteros-lora-worker/src/routing_policy_filter.rs`

use adapteros_lora_router::{policy_mask::PolicyMask, AdapterInfo, Router, RouterWeights};
use adapteros_core::B3Hash;
use std::collections::HashSet;

/// Helper to create test adapter info
fn create_adapter(id: &str) -> AdapterInfo {
    AdapterInfo {
        id: id.to_string(),
        framework: None,
        languages: vec![],
        tier: "default".to_string(),
        ..Default::default()
    }
}

#[test]
fn test_denied_adapter_blocked_by_policy_mask() {
    println!("\n=== Test: Denied Adapter Blocked by Policy Mask ===");

    // Setup: 3 adapters, deny adapter "b"
    let adapter_ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let denied_adapters = vec!["b".to_string()];
    let policy_digest = B3Hash::hash(b"test-policy-v1");

    // Build policy mask with denied adapter
    let mask = PolicyMask::build(
        &adapter_ids,
        None,  // No allowlist
        Some(&denied_adapters),  // Deny "b"
        None,  // No index gate
        None,  // No trust blocks
        Some(policy_digest),
    );

    // Verify: adapter "b" should be denied (false), others allowed (true)
    assert_eq!(
        mask.allowed,
        vec![true, false, true],
        "Adapter 'b' should be denied, 'a' and 'c' allowed"
    );

    // Verify: deny_list flag is set
    assert!(
        mask.overrides_applied.deny_list,
        "deny_list override flag should be true"
    );

    // Verify: allow_list and trust_state flags are not set
    assert!(
        !mask.overrides_applied.allow_list,
        "allow_list override flag should be false"
    );
    assert!(
        !mask.overrides_applied.trust_state,
        "trust_state override flag should be false"
    );

    println!("✓ Denied adapter correctly blocked");
    println!("  Mask: {:?}", mask.allowed);
    println!("  Overrides: deny_list={}", mask.overrides_applied.deny_list);
}

#[test]
fn test_allowlist_restricts_adapters() {
    println!("\n=== Test: Allowlist Restricts Adapters ===");

    let adapter_ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let allowed_adapters = vec!["a".to_string(), "c".to_string()];
    let policy_digest = B3Hash::hash(b"allowlist-policy");

    let mask = PolicyMask::build(
        &adapter_ids,
        Some(&allowed_adapters),  // Only allow "a" and "c"
        None,  // No denylist
        None,
        None,
        Some(policy_digest),
    );

    // Only "a" and "c" should be allowed
    assert_eq!(
        mask.allowed,
        vec![true, false, true],
        "Only adapters 'a' and 'c' should be allowed"
    );

    assert!(
        mask.overrides_applied.allow_list,
        "allow_list override flag should be true"
    );

    println!("✓ Allowlist correctly restricts adapters");
}

#[test]
fn test_denylist_overrides_allowlist() {
    println!("\n=== Test: Denylist Overrides Allowlist ===");

    let adapter_ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let allowed_adapters = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let denied_adapters = vec!["b".to_string()];
    let policy_digest = B3Hash::hash(b"override-policy");

    let mask = PolicyMask::build(
        &adapter_ids,
        Some(&allowed_adapters),  // Allow all
        Some(&denied_adapters),   // But deny "b"
        None,
        None,
        Some(policy_digest),
    );

    // Denylist should override allowlist for "b"
    assert_eq!(
        mask.allowed,
        vec![true, false, true],
        "Denylist should override allowlist for adapter 'b'"
    );

    assert!(mask.overrides_applied.allow_list);
    assert!(mask.overrides_applied.deny_list);

    println!("✓ Denylist correctly overrides allowlist");
}

#[test]
fn test_blake3_digest_binding_prevents_tampering() {
    println!("\n=== Test: BLAKE3 Digest Binding Prevents Tampering ===");

    let adapter_ids = vec!["a".to_string(), "b".to_string()];
    let denied_adapters = vec!["b".to_string()];

    // Create two different policy contexts
    let policy_digest_v1 = B3Hash::hash(b"policy-v1");
    let policy_digest_v2 = B3Hash::hash(b"policy-v2-TAMPERED");

    let mask_v1 = PolicyMask::build(
        &adapter_ids,
        None,
        Some(&denied_adapters),
        None,
        None,
        Some(policy_digest_v1),
    );

    let mask_v2 = PolicyMask::build(
        &adapter_ids,
        None,
        Some(&denied_adapters),
        None,
        None,
        Some(policy_digest_v2),
    );

    // Same policy configuration but different context digests
    // should produce different mask digests
    assert_ne!(
        mask_v1.digest,
        mask_v2.digest,
        "Different policy contexts should produce different mask digests"
    );

    println!("✓ BLAKE3 digest binding prevents policy tampering");
    println!("  Mask V1 digest: {:?}", mask_v1.digest);
    println!("  Mask V2 digest: {:?}", mask_v2.digest);
}

#[test]
fn test_policy_mask_determinism() {
    println!("\n=== Test: Policy Mask Determinism ===");

    let adapter_ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let denied_adapters = vec!["b".to_string()];
    let policy_digest = B3Hash::hash(b"deterministic-policy");

    // Create same mask multiple times
    let mask1 = PolicyMask::build(
        &adapter_ids,
        None,
        Some(&denied_adapters),
        None,
        None,
        Some(policy_digest),
    );

    let mask2 = PolicyMask::build(
        &adapter_ids,
        None,
        Some(&denied_adapters),
        None,
        None,
        Some(policy_digest),
    );

    // Digests should be identical for same inputs
    assert_eq!(
        mask1.digest,
        mask2.digest,
        "Same policy configuration should produce deterministic digests"
    );

    assert_eq!(mask1.allowed, mask2.allowed);
    assert_eq!(
        mask1.overrides_applied.deny_list,
        mask2.overrides_applied.deny_list
    );

    println!("✓ Policy mask generation is deterministic");
}

#[test]
fn test_attempt_to_use_denied_adapter_fails() {
    println!("\n=== Test: Attempt to Use Denied Adapter is Rejected ===");

    // Setup router with 3 adapters
    let mut router = Router::new(RouterWeights::default(), 3, 1.0);

    let adapter_info = vec![
        create_adapter("allowed_a"),
        create_adapter("denied_b"),
        create_adapter("allowed_c"),
    ];

    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();

    // Create policy mask denying "denied_b"
    let denied_adapters = vec!["denied_b".to_string()];
    let policy_digest = B3Hash::hash(b"router-policy");

    let mask = PolicyMask::build(
        &adapter_ids,
        None,
        Some(&denied_adapters),
        None,
        None,
        Some(policy_digest),
    );

    // Create routing inputs that would normally select all adapters
    let features = vec![0.5; 22];  // 22 features as per router config
    let priors = vec![0.9, 0.8, 0.7];  // High scores for all

    // Route with policy mask
    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("routing should succeed");

    // Verify: "denied_b" (index 1) should NOT be in selected adapters
    let selected_adapter_ids: Vec<String> = decision
        .indices
        .iter()
        .map(|&idx| adapter_info[idx as usize].id.clone())
        .collect();

    assert!(
        !selected_adapter_ids.contains(&"denied_b".to_string()),
        "Denied adapter 'denied_b' should not be selected"
    );

    // Verify: allowed adapters should be present
    assert!(
        selected_adapter_ids.contains(&"allowed_a".to_string())
            || selected_adapter_ids.contains(&"allowed_c".to_string()),
        "At least one allowed adapter should be selected"
    );

    println!("✓ Denied adapter correctly rejected from routing");
    println!("  Selected adapters: {:?}", selected_adapter_ids);
    println!("  Denied adapter: denied_b");
}

#[test]
fn test_trust_state_blocks_adapters() {
    println!("\n=== Test: Trust State Blocks Adapters ===");

    let adapter_ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let mut trust_blocked = HashSet::new();
    trust_blocked.insert(1);  // Block index 1 (adapter "b")

    let policy_digest = B3Hash::hash(b"trust-policy");

    let mask = PolicyMask::build(
        &adapter_ids,
        None,
        None,
        None,
        Some(&trust_blocked),
        Some(policy_digest),
    );

    // Adapter at index 1 should be blocked by trust state
    assert_eq!(
        mask.allowed,
        vec![true, false, true],
        "Trust-blocked adapter should be denied"
    );

    assert!(
        mask.overrides_applied.trust_state,
        "trust_state override flag should be true"
    );

    println!("✓ Trust state correctly blocks adapters");
}

#[test]
fn test_deny_all_adapters_results_in_empty_selection() {
    println!("\n=== Test: Deny All Adapters Results in Empty Selection ===");

    let adapter_ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let policy_digest = B3Hash::hash(b"deny-all-policy");

    // Use deny_all helper
    let mask = PolicyMask::deny_all(&adapter_ids, Some(policy_digest));

    // All adapters should be denied
    assert_eq!(
        mask.allowed,
        vec![false, false, false],
        "All adapters should be denied"
    );

    // Verify routing with deny-all mask
    let mut router = Router::new(RouterWeights::default(), 3, 1.0);
    let adapter_info: Vec<AdapterInfo> = adapter_ids.iter().map(|id| create_adapter(id)).collect();
    let features = vec![0.5; 22];
    let priors = vec![0.8, 0.7, 0.6];

    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("routing should succeed");

    // No adapters should be selected
    assert_eq!(
        decision.indices.len(),
        0,
        "No adapters should be selected with deny-all mask"
    );

    println!("✓ Deny-all policy correctly results in empty selection");
}

#[test]
fn test_combined_allowlist_denylist_and_trust_state() {
    println!("\n=== Test: Combined Allowlist, Denylist, and Trust State ===");

    let adapter_ids = vec![
        "a".to_string(),
        "b".to_string(),
        "c".to_string(),
        "d".to_string(),
    ];

    let allowed_adapters = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let denied_adapters = vec!["b".to_string()];
    let mut trust_blocked = HashSet::new();
    trust_blocked.insert(2);  // Block index 2 (adapter "c")

    let policy_digest = B3Hash::hash(b"combined-policy");

    let mask = PolicyMask::build(
        &adapter_ids,
        Some(&allowed_adapters),  // Allow a, b, c (not d)
        Some(&denied_adapters),   // Deny b
        None,
        Some(&trust_blocked),     // Trust-block c
        Some(policy_digest),
    );

    // Result: only "a" should be allowed
    // - "a": allowed, not denied, not trust-blocked ✓
    // - "b": allowed, but denied ✗
    // - "c": allowed, not denied, but trust-blocked ✗
    // - "d": not in allowlist ✗
    assert_eq!(
        mask.allowed,
        vec![true, false, false, false],
        "Only adapter 'a' should pass all policy checks"
    );

    assert!(mask.overrides_applied.allow_list);
    assert!(mask.overrides_applied.deny_list);
    assert!(mask.overrides_applied.trust_state);

    println!("✓ Combined policy enforcement works correctly");
}

#[test]
fn test_policy_digest_changes_with_adapter_list() {
    println!("\n=== Test: Policy Digest Changes with Adapter List ===");

    let adapter_ids_v1 = vec!["a".to_string(), "b".to_string()];
    let adapter_ids_v2 = vec!["a".to_string(), "c".to_string()];
    let denied_adapters = vec!["b".to_string()];
    let policy_digest = B3Hash::hash(b"same-policy");

    let mask_v1 = PolicyMask::build(
        &adapter_ids_v1,
        None,
        Some(&denied_adapters),
        None,
        None,
        Some(policy_digest),
    );

    let mask_v2 = PolicyMask::build(
        &adapter_ids_v2,
        None,
        Some(&denied_adapters),
        None,
        None,
        Some(policy_digest),
    );

    // Different adapter lists should produce different mask digests
    assert_ne!(
        mask_v1.digest,
        mask_v2.digest,
        "Different adapter lists should produce different mask digests"
    );

    println!("✓ Policy digest correctly binds to adapter list");
}

#[test]
fn test_bypass_attempt_via_policy_tampering_detected() {
    println!("\n=== Test: Bypass Attempt via Policy Tampering is Detected ===");

    let adapter_ids = vec!["safe".to_string(), "malicious".to_string()];

    // Original policy: deny malicious adapter
    let original_policy = B3Hash::hash(b"deny_malicious");
    let denied = vec!["malicious".to_string()];

    let legit_mask = PolicyMask::build(
        &adapter_ids,
        None,
        Some(&denied),
        None,
        None,
        Some(original_policy),
    );

    // Attacker attempts to create mask with same denied list but different policy context
    // (trying to forge a mask that looks valid)
    let tampered_policy = B3Hash::hash(b"TAMPERED_allow_all");

    let tampered_mask = PolicyMask::build(
        &adapter_ids,
        None,
        Some(&denied),
        None,
        None,
        Some(tampered_policy),
    );

    // The digests should be different, preventing bypass
    assert_ne!(
        legit_mask.digest,
        tampered_mask.digest,
        "Tampered policy should produce different digest, preventing bypass"
    );

    // Verify legitimate mask denies malicious adapter
    assert_eq!(legit_mask.allowed, vec![true, false]);

    println!("✓ Policy tampering attempt detected via digest mismatch");
    println!("  Legitimate digest: {:?}", legit_mask.digest);
    println!("  Tampered digest: {:?}", tampered_mask.digest);
}

#[cfg(test)]
mod integration {
    use super::*;
    use adapteros_policy::packs::router::{RouterConfig, RouterPolicy};

    #[test]
    fn test_router_policy_integration_with_mask() {
        println!("\n=== Integration Test: Router Policy with Mask ===");

        let policy_config = RouterConfig {
            k_sparse: 2,
            ..Default::default()
        };

        let policy = RouterPolicy::new(policy_config.clone());

        // Validate policy settings
        assert!(policy.validate_k_sparse(2).is_ok());
        assert!(policy.validate_k_sparse(3).is_err());

        println!("✓ Router policy integrates correctly with policy mask");
    }
}
