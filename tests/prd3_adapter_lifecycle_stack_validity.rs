//! PRD 3: Adapter Lifecycle & Stack Validity Tests
//!
//! Tests for:
//! - Adapter lifecycle state transitions
//! - Stack deduplication and sorting
//! - Stack activation validation
//! - Hash stability

use adapteros_core::{AdapterLifecycleState, StackSpec};
use std::collections::HashMap;

#[test]
fn test_lifecycle_state_valid_transitions() {
    // Registered → Loaded → Active → Unloaded
    assert!(AdapterLifecycleState::Registered.can_transition_to(&AdapterLifecycleState::Loaded));
    assert!(AdapterLifecycleState::Loaded.can_transition_to(&AdapterLifecycleState::Active));
    assert!(AdapterLifecycleState::Active.can_transition_to(&AdapterLifecycleState::Unloaded));
}

#[test]
fn test_lifecycle_state_invalid_transitions() {
    // Skip transitions not allowed
    assert!(!AdapterLifecycleState::Registered.can_transition_to(&AdapterLifecycleState::Active));
    assert!(!AdapterLifecycleState::Registered
        .can_transition_to(&AdapterLifecycleState::Unloaded));

    // Backwards transitions not allowed
    assert!(!AdapterLifecycleState::Loaded.can_transition_to(&AdapterLifecycleState::Registered));
    assert!(!AdapterLifecycleState::Active.can_transition_to(&AdapterLifecycleState::Loaded));
    assert!(!AdapterLifecycleState::Active.can_transition_to(&AdapterLifecycleState::Registered));

    // No transitions from Unloaded
    assert!(!AdapterLifecycleState::Unloaded
        .can_transition_to(&AdapterLifecycleState::Registered));
    assert!(!AdapterLifecycleState::Unloaded.can_transition_to(&AdapterLifecycleState::Loaded));
    assert!(!AdapterLifecycleState::Unloaded.can_transition_to(&AdapterLifecycleState::Active));
}

#[test]
fn test_lifecycle_state_can_be_in_stack() {
    assert!(!AdapterLifecycleState::Registered.can_be_in_stack());
    assert!(AdapterLifecycleState::Loaded.can_be_in_stack());
    assert!(AdapterLifecycleState::Active.can_be_in_stack());
    assert!(!AdapterLifecycleState::Unloaded.can_be_in_stack());
}

#[test]
fn test_stack_spec_deduplication() {
    let mut hashes = HashMap::new();
    hashes.insert("adapter-1".to_string(), "hash-1".to_string());
    hashes.insert("adapter-2".to_string(), "hash-2".to_string());

    let spec = StackSpec::new(
        "tenant-1".to_string(),
        vec![
            "adapter-1".to_string(),
            "adapter-2".to_string(),
            "adapter-1".to_string(), // duplicate
        ],
        &hashes,
    )
    .unwrap();

    // Should deduplicate
    assert_eq!(spec.adapter_ids.len(), 2);
    assert!(spec.adapter_ids.contains(&"adapter-1".to_string()));
    assert!(spec.adapter_ids.contains(&"adapter-2".to_string()));
}

#[test]
fn test_stack_spec_sorting() {
    let mut hashes = HashMap::new();
    hashes.insert("adapter-a".to_string(), "hash-a".to_string());
    hashes.insert("adapter-b".to_string(), "hash-b".to_string());
    hashes.insert("adapter-c".to_string(), "hash-c".to_string());

    let spec = StackSpec::new(
        "tenant-1".to_string(),
        vec![
            "adapter-c".to_string(),
            "adapter-a".to_string(),
            "adapter-b".to_string(),
        ],
        &hashes,
    )
    .unwrap();

    // Should be sorted lexicographically
    assert_eq!(
        spec.adapter_ids,
        vec!["adapter-a", "adapter-b", "adapter-c"]
    );
}

#[test]
fn test_stack_spec_hash_stability() {
    let mut hashes = HashMap::new();
    hashes.insert("adapter-1".to_string(), "hash-1".to_string());
    hashes.insert("adapter-2".to_string(), "hash-2".to_string());

    // Create two stacks with different input order
    let spec1 = StackSpec::new(
        "tenant-1".to_string(),
        vec!["adapter-1".to_string(), "adapter-2".to_string()],
        &hashes,
    )
    .unwrap();

    let spec2 = StackSpec::new(
        "tenant-1".to_string(),
        vec!["adapter-2".to_string(), "adapter-1".to_string()], // reversed
        &hashes,
    )
    .unwrap();

    // Hashes should be identical (order-independent)
    assert_eq!(spec1.hash, spec2.hash);
}

#[test]
fn test_stack_spec_missing_hash() {
    let mut hashes = HashMap::new();
    hashes.insert("adapter-1".to_string(), "hash-1".to_string());

    let result = StackSpec::new(
        "tenant-1".to_string(),
        vec!["adapter-1".to_string(), "adapter-999".to_string()],
        &hashes,
    );

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .contains("Missing hash for adapter: adapter-999"));
}

#[test]
fn test_stack_spec_generation_advance() {
    let mut hashes = HashMap::new();
    hashes.insert("adapter-1".to_string(), "hash-1".to_string());

    let mut spec = StackSpec::new(
        "tenant-1".to_string(),
        vec!["adapter-1".to_string()],
        &hashes,
    )
    .unwrap();

    assert_eq!(spec.generation, 0);

    spec.advance_generation();
    assert_eq!(spec.generation, 1);

    spec.advance_generation();
    assert_eq!(spec.generation, 2);
}

#[test]
fn test_stack_spec_validate() {
    let mut hashes = HashMap::new();
    hashes.insert("adapter-1".to_string(), "hash-1".to_string());
    hashes.insert("adapter-2".to_string(), "hash-2".to_string());

    let spec = StackSpec::new(
        "tenant-1".to_string(),
        vec!["adapter-1".to_string(), "adapter-2".to_string()],
        &hashes,
    )
    .unwrap();

    // Should be valid (sorted, no duplicates)
    assert!(spec.validate().is_ok());

    // Manually break sorting
    let mut broken_spec = spec.clone();
    broken_spec.adapter_ids = vec!["adapter-2".to_string(), "adapter-1".to_string()];
    assert!(broken_spec.validate().is_err());

    // Manually add duplicate
    let mut dup_spec = spec.clone();
    dup_spec.adapter_ids = vec!["adapter-1".to_string(), "adapter-1".to_string()];
    assert!(dup_spec.validate().is_err());
}

#[test]
fn test_stack_spec_has_duplicates() {
    let mut hashes = HashMap::new();
    hashes.insert("adapter-1".to_string(), "hash-1".to_string());
    hashes.insert("adapter-2".to_string(), "hash-2".to_string());

    let spec = StackSpec::new(
        "tenant-1".to_string(),
        vec!["adapter-1".to_string(), "adapter-2".to_string()],
        &hashes,
    )
    .unwrap();

    assert!(!spec.has_duplicates());

    // Manually add duplicate
    let mut dup_spec = spec.clone();
    dup_spec.adapter_ids.push("adapter-1".to_string());
    assert!(dup_spec.has_duplicates());
}
