use adapteros_policy::registry::{PolicyId, POLICY_INDEX};

/// Test that the policy registry contains exactly 20 policies
#[test]
fn test_policy_registry_count() {
    assert_eq!(
        POLICY_INDEX.len(),
        20,
        "Policy registry must contain exactly 20 policies"
    );
}

/// Test that all policy IDs are unique
#[test]
fn test_policy_ids_unique() {
    let mut ids = std::collections::HashSet::new();
    for policy in POLICY_INDEX.iter() {
        assert!(
            ids.insert(policy.id),
            "Duplicate policy ID found: {:?}",
            policy.id
        );
    }
}

/// Test that all canonical policy names are present
#[test]
fn test_canonical_policy_names() {
    let expected_policies = [
        PolicyId::Egress,
        PolicyId::Determinism,
        PolicyId::Router,
        PolicyId::Evidence,
        PolicyId::Refusal,
        PolicyId::Numeric,
        PolicyId::Rag,
        PolicyId::Isolation,
        PolicyId::Telemetry,
        PolicyId::Retention,
        PolicyId::Performance,
        PolicyId::Memory,
        PolicyId::Artifacts,
        PolicyId::Secrets,
        PolicyId::BuildRelease,
        PolicyId::Compliance,
        PolicyId::Incident,
        PolicyId::Output,
        PolicyId::Adapters,
        PolicyId::DeterministicIo,
    ];

    assert_eq!(
        expected_policies.len(),
        20,
        "Expected exactly 20 canonical policies"
    );

    for expected_id in expected_policies.iter() {
        assert!(
            POLICY_INDEX.iter().any(|policy| policy.id == *expected_id),
            "Missing canonical policy: {:?}",
            expected_id
        );
    }
}

/// Test that no unexpected policies are present
#[test]
fn test_no_unexpected_policies() {
    let expected_policies = [
        PolicyId::Egress,
        PolicyId::Determinism,
        PolicyId::Router,
        PolicyId::Evidence,
        PolicyId::Refusal,
        PolicyId::Numeric,
        PolicyId::Rag,
        PolicyId::Isolation,
        PolicyId::Telemetry,
        PolicyId::Retention,
        PolicyId::Performance,
        PolicyId::Memory,
        PolicyId::Artifacts,
        PolicyId::Secrets,
        PolicyId::BuildRelease,
        PolicyId::Compliance,
        PolicyId::Incident,
        PolicyId::Output,
        PolicyId::Adapters,
        PolicyId::DeterministicIo,
    ];

    for policy in POLICY_INDEX.iter() {
        assert!(
            expected_policies.contains(&policy.id),
            "Unexpected policy found: {:?}",
            policy.id
        );
    }
}

/// Test that policy names are non-empty
#[test]
fn test_policy_names_non_empty() {
    for policy in POLICY_INDEX.iter() {
        assert!(
            !policy.name.is_empty(),
            "Policy name cannot be empty for {:?}",
            policy.id
        );
    }
}

/// Test that policy descriptions are non-empty
#[test]
fn test_policy_descriptions_non_empty() {
    for policy in POLICY_INDEX.iter() {
        assert!(
            !policy.description.is_empty(),
            "Policy description cannot be empty for {:?}",
            policy.id
        );
    }
}

/// Test that policy severities are valid
#[test]
fn test_policy_severities_valid() {
    use adapteros_policy::registry::Severity;

    for policy in POLICY_INDEX.iter() {
        match policy.severity {
            Severity::Critical | Severity::High | Severity::Medium | Severity::Low => {
                // Valid severity
            }
            _ => {
                panic!(
                    "Invalid severity for policy {:?}: {:?}",
                    policy.id, policy.severity
                );
            }
        }
    }
}

/// Test that the policy registry is deterministic (same order every time)
#[test]
fn test_policy_registry_deterministic() {
    let first_run: Vec<_> = POLICY_INDEX.iter().map(|p| p.id).collect();
    let second_run: Vec<_> = POLICY_INDEX.iter().map(|p| p.id).collect();

    assert_eq!(
        first_run, second_run,
        "Policy registry must be deterministic"
    );
}

/// Test that policy IDs match their string representations
#[test]
fn test_policy_id_string_consistency() {
    for policy in POLICY_INDEX.iter() {
        let id_string = format!("{:?}", policy.id);
        assert!(
            policy
                .name
                .to_lowercase()
                .contains(&id_string.to_lowercase())
                || id_string
                    .to_lowercase()
                    .contains(&policy.name.to_lowercase()),
            "Policy ID {:?} and name '{}' should be consistent",
            policy.id,
            policy.name
        );
    }
}

/// Test that the policy registry can be serialized and deserialized
#[test]
fn test_policy_registry_serialization() {
    use serde_json;

    // Test that we can serialize the registry
    let serialized =
        serde_json::to_string(&POLICY_INDEX).expect("Failed to serialize policy registry");
    assert!(
        !serialized.is_empty(),
        "Serialized policy registry should not be empty"
    );

    // Test that we can deserialize it back
    let deserialized: Vec<_> =
        serde_json::from_str(&serialized).expect("Failed to deserialize policy registry");
    assert_eq!(
        deserialized.len(),
        POLICY_INDEX.len(),
        "Deserialized registry should have same length"
    );

    // Test that the content is the same
    for (original, deserialized) in POLICY_INDEX.iter().zip(deserialized.iter()) {
        assert_eq!(
            original.id, deserialized.id,
            "Policy ID should match after deserialization"
        );
        assert_eq!(
            original.name, deserialized.name,
            "Policy name should match after deserialization"
        );
        assert_eq!(
            original.description, deserialized.description,
            "Policy description should match after deserialization"
        );
        assert_eq!(
            original.severity, deserialized.severity,
            "Policy severity should match after deserialization"
        );
    }
}

/// Test that the policy registry is sorted by ID for deterministic ordering
#[test]
fn test_policy_registry_sorted() {
    let mut sorted_policies: Vec<_> = POLICY_INDEX.iter().map(|p| p.id).collect();
    sorted_policies.sort();

    let current_order: Vec<_> = POLICY_INDEX.iter().map(|p| p.id).collect();

    assert_eq!(
        current_order, sorted_policies,
        "Policy registry should be sorted by ID for deterministic ordering"
    );
}

/// Test that the policy registry contains no deprecated or placeholder policies
#[test]
fn test_no_deprecated_policies() {
    let deprecated_keywords = [
        "deprecated",
        "placeholder",
        "todo",
        "fixme",
        "unimplemented",
    ];

    for policy in POLICY_INDEX.iter() {
        let name_lower = policy.name.to_lowercase();
        let desc_lower = policy.description.to_lowercase();

        for keyword in deprecated_keywords.iter() {
            assert!(
                !name_lower.contains(keyword),
                "Policy name '{}' contains deprecated keyword '{}'",
                policy.name,
                keyword
            );
            assert!(
                !desc_lower.contains(keyword),
                "Policy description for '{}' contains deprecated keyword '{}'",
                policy.name,
                keyword
            );
        }
    }
}

/// Test that the policy registry is complete and ready for production
#[test]
fn test_policy_registry_production_ready() {
    // Check that all policies have meaningful names
    for policy in POLICY_INDEX.iter() {
        assert!(
            policy.name.len() > 2,
            "Policy name '{}' is too short",
            policy.name
        );
        assert!(
            policy.description.len() > 10,
            "Policy description for '{}' is too short",
            policy.name
        );
    }

    // Check that we have a good distribution of severities
    let mut severity_counts = std::collections::HashMap::new();
    for policy in POLICY_INDEX.iter() {
        *severity_counts.entry(policy.severity).or_insert(0) += 1;
    }

    // We should have at least some Critical and High severity policies
    assert!(
        severity_counts
            .get(&adapteros_policy::registry::Severity::Critical)
            .unwrap_or(&0)
            > &0,
        "Should have at least one Critical severity policy"
    );
    assert!(
        severity_counts
            .get(&adapteros_policy::registry::Severity::High)
            .unwrap_or(&0)
            > &0,
        "Should have at least one High severity policy"
    );

    // Total should be exactly 20
    let total: usize = severity_counts.values().sum();
    assert_eq!(total, 20, "Total policy count should be exactly 20");
}
