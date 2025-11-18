//! Unit tests for lifecycle management functionality

use adapteros_core::lifecycle::{LifecycleState, LifecycleTransition, SemanticVersion, TransitionReason};

#[test]
fn test_valid_transitions() {
    // Test valid transitions using is_valid()
    assert!(LifecycleTransition::new(LifecycleState::Draft, LifecycleState::Active).is_valid());
    assert!(LifecycleTransition::new(LifecycleState::Active, LifecycleState::Deprecated).is_valid());
    assert!(LifecycleTransition::new(LifecycleState::Deprecated, LifecycleState::Retired).is_valid());

    // Test invalid transitions
    assert!(!LifecycleTransition::new(LifecycleState::Active, LifecycleState::Retired).is_valid());
    assert!(!LifecycleTransition::new(LifecycleState::Retired, LifecycleState::Active).is_valid());
}

#[test]
fn test_semantic_version_increment() {
    let mut v1 = SemanticVersion::new(1, 0, 0);

    // Test patch increment
    v1.bump_patch();
    assert_eq!(v1, SemanticVersion::new(1, 0, 1));

    // Test minor increment
    let mut v2 = SemanticVersion::new(1, 0, 0);
    v2.bump_minor();
    assert_eq!(v2, SemanticVersion::new(1, 1, 0));

    // Test major increment
    let mut v3 = SemanticVersion::new(1, 0, 0);
    v3.bump_major();
    assert_eq!(v3, SemanticVersion::new(2, 0, 0));

    // Test string representation
    assert_eq!(SemanticVersion::new(1, 0, 0).to_string(), "1.0.0");
}

#[test]
fn test_semantic_version_comparison() {
    let v1_0_0 = SemanticVersion::new(1, 0, 0);
    let v1_0_1 = SemanticVersion::new(1, 0, 1);
    let v1_1_0 = SemanticVersion::new(1, 1, 0);
    let v2_0_0 = SemanticVersion::new(2, 0, 0);

    assert!(v1_0_0 < v1_0_1);
    assert!(v1_0_1 < v1_1_0);
    assert!(v1_1_0 < v2_0_0);
    assert!(v1_0_0 < v2_0_0);
}

#[test]
fn test_lifecycle_state_display() {
    assert_eq!(format!("{}", LifecycleState::Draft), "draft");
    assert_eq!(format!("{}", LifecycleState::Active), "active");
    assert_eq!(format!("{}", LifecycleState::Deprecated), "deprecated");
    assert_eq!(format!("{}", LifecycleState::Retired), "retired");
}

#[test]
fn test_transition_reason() {
    let reason = TransitionReason::new("Testing transition", "test-user");
    assert_eq!(reason.reason, "Testing transition");
    assert_eq!(reason.initiated_by, "test-user");
}
