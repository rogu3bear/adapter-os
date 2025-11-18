//! Unit tests for lifecycle management functionality

use adapteros_core::lifecycle::{LifecycleState, LifecycleTransition, SemanticVersion, TransitionReason};

#[test]
fn test_valid_transitions() {
    // Test valid transitions
    assert!(LifecycleTransition::new(LifecycleState::Draft, LifecycleState::Active).is_ok());
    assert!(LifecycleTransition::new(LifecycleState::Active, LifecycleState::Deprecated).is_ok());
    assert!(LifecycleTransition::new(LifecycleState::Deprecated, LifecycleState::Retired).is_ok());

    // Test invalid transitions
    assert!(LifecycleTransition::new(LifecycleState::Active, LifecycleState::Retired).is_err());
    assert!(LifecycleTransition::new(LifecycleState::Retired, LifecycleState::Active).is_err());
}

#[test]
fn test_semantic_version_increment() {
    let v1 = SemanticVersion::new(1, 0, 0);

    // Test patch increment
    assert_eq!(v1.increment_patch(), SemanticVersion::new(1, 0, 1));

    // Test minor increment
    assert_eq!(v1.increment_minor(), SemanticVersion::new(1, 1, 0));

    // Test major increment
    assert_eq!(v1.increment_major(), SemanticVersion::new(2, 0, 0));

    // Test string representation
    assert_eq!(v1.to_string(), "1.0.0");
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
    assert_eq!(format!("{}", LifecycleState::Draft), "Draft");
    assert_eq!(format!("{}", LifecycleState::Active), "Active");
    assert_eq!(format!("{}", LifecycleState::Deprecated), "Deprecated");
    assert_eq!(format!("{}", LifecycleState::Retired), "Retired");
}

#[test]
fn test_transition_reason() {
    let reason = TransitionReason::Manual("Testing transition".to_string());
    assert_eq!(format!("{}", reason), "Manual: Testing transition");

    let reason = TransitionReason::Automatic("Auto-deployment".to_string());
    assert_eq!(format!("{}", reason), "Automatic: Auto-deployment");
}
