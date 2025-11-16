//! Integration tests for adapter taxonomy and naming
//!
//! Tests the complete naming system including:
//! - Semantic name validation
//! - Lineage tracking
//! - Parent-child relationships
//! - Stack naming validation
//! - Registry integration

use adapteros_core::{AdapterName, AosError, B3Hash, ForkType, StackName};
use adapteros_registry::Registry;
use tempfile::tempdir;

#[test]
fn test_adapter_name_validation() {
    // Valid names
    let valid_names = vec![
        "shop-floor/hydraulics/troubleshooting/r001",
        "dentist-office/scheduling/appointment-booking/r042",
        "global/code/rust-analyzer/r015",
        "acme-corp/legal/contract-review/r003",
        "ab/cd/ef/r123",
    ];

    for name in valid_names {
        let parsed = AdapterName::parse(name);
        assert!(
            parsed.is_ok(),
            "Failed to parse valid name '{}': {:?}",
            name,
            parsed.err()
        );
    }

    // Invalid names
    let invalid_names = vec![
        "too-short/a/b/r001",         // domain too short
        "tenant/domain/purpose/r1",   // revision too short
        "tenant/domain/purpose/001",  // missing 'r' prefix
        "Tenant/domain/purpose/r001", // uppercase
        "system/domain/purpose/r001", // reserved tenant
        "tenant/domain/purpose",      // missing revision
        "a--b/domain/purpose/r001",   // consecutive hyphens
    ];

    for name in invalid_names {
        let parsed = AdapterName::parse(name);
        assert!(
            parsed.is_err(),
            "Should reject invalid name '{}', but got: {:?}",
            name,
            parsed
        );
    }
}

#[test]
fn test_adapter_name_components() {
    let name = AdapterName::parse("shop-floor/hydraulics/troubleshooting/r042").unwrap();

    assert_eq!(name.tenant(), "shop-floor");
    assert_eq!(name.domain(), "hydraulics");
    assert_eq!(name.purpose(), "troubleshooting");
    assert_eq!(name.revision(), "r042");
    assert_eq!(name.revision_number().unwrap(), 42);
    assert_eq!(name.base_path(), "shop-floor/hydraulics/troubleshooting");
    assert_eq!(
        name.to_string(),
        "shop-floor/hydraulics/troubleshooting/r042"
    );
    assert_eq!(
        name.display_name(),
        "shop-floor/hydraulics/troubleshooting (rev 42)"
    );
}

#[test]
fn test_adapter_lineage_tracking() {
    let name1 = AdapterName::parse("tenant/domain/purpose/r001").unwrap();
    let name2 = AdapterName::parse("tenant/domain/purpose/r002").unwrap();
    let name3 = AdapterName::parse("tenant/domain/other/r001").unwrap();

    assert!(name1.is_same_lineage(&name2), "Should be same lineage");
    assert!(
        !name1.is_same_lineage(&name3),
        "Should be different lineage"
    );

    // Test next revision
    let next = name1.next_revision().unwrap();
    assert_eq!(next.revision(), "r002");
    assert_eq!(next.revision_number().unwrap(), 2);
    assert!(name1.is_same_lineage(&next));
}

#[test]
fn test_stack_name_validation() {
    // Valid stack names
    let valid = vec![
        "stack.safe-default-v2", // modified to avoid reserved
        "stack.dentist-office",
        "stack.shop-floor-nightshift",
        "stack.acme-corp.production",
        "stack.global.code-review",
    ];

    for name in valid {
        let parsed = StackName::parse(name);
        assert!(
            parsed.is_ok(),
            "Failed to parse valid stack name '{}': {:?}",
            name,
            parsed.err()
        );
    }

    // Invalid stack names
    let invalid = vec![
        "not-a-stack",           // missing prefix
        "stack",                 // no namespace
        "stack.",                // empty namespace
        "stack.a",               // namespace too short
        "stack.safe-default",    // reserved
        "stack.tenant.id.extra", // too many components
        "stack.tenant--id",      // consecutive hyphens
    ];

    for name in invalid {
        let parsed = StackName::parse(name);
        assert!(
            parsed.is_err(),
            "Should reject invalid stack name '{}', but got: {:?}",
            name,
            parsed
        );
    }
}

#[test]
fn test_stack_name_components() {
    let name1 = StackName::parse("stack.shop-floor-nightshift").unwrap();
    assert_eq!(name1.namespace(), "shop-floor-nightshift");
    assert_eq!(name1.identifier(), None);
    assert_eq!(name1.to_string(), "stack.shop-floor-nightshift");

    let name2 = StackName::parse("stack.acme-corp.production").unwrap();
    assert_eq!(name2.namespace(), "acme-corp");
    assert_eq!(name2.identifier(), Some("production"));
    assert_eq!(name2.to_string(), "stack.acme-corp.production");
}

#[test]
fn test_registry_semantic_name_registration() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_registry.db");
    let registry = Registry::open(&db_path).expect("Failed to open registry");

    // Register tenant
    registry
        .register_tenant("tenant-a", 1000, 1000)
        .expect("Failed to register tenant");

    // Create semantic name
    let name = AdapterName::parse("tenant-a/domain/purpose/r001").unwrap();
    let hash = B3Hash::hash(b"test_adapter_data");

    // Register adapter with semantic name
    registry
        .register_adapter_with_name(
            "adapter-1",
            Some(&name),
            &hash,
            "persistent",
            16,
            &vec!["tenant-a".to_string()],
            None,
            None,
        )
        .expect("Failed to register adapter");

    // Lookup by semantic name
    let found = registry
        .get_adapter_by_name("tenant-a/domain/purpose/r001")
        .expect("Failed to get adapter")
        .expect("Adapter not found");

    assert_eq!(found.id, "adapter-1");
    assert_eq!(found.semantic_name.unwrap().to_string(), name.to_string());
    assert_eq!(found.display_name(), "tenant-a/domain/purpose (rev 1)");
}

#[test]
fn test_registry_lineage_tracking() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_lineage.db");
    let registry = Registry::open(&db_path).expect("Failed to open registry");

    registry
        .register_tenant("tenant-a", 1000, 1000)
        .expect("Failed to register tenant");

    // Register parent adapter
    let parent_name = AdapterName::parse("tenant-a/domain/purpose/r001").unwrap();
    let parent_hash = B3Hash::hash(b"parent_adapter");

    registry
        .register_adapter_with_name(
            "parent-id",
            Some(&parent_name),
            &parent_hash,
            "persistent",
            16,
            &vec!["tenant-a".to_string()],
            None,
            None,
        )
        .expect("Failed to register parent");

    // Register child adapter with parent reference
    let child_name = AdapterName::parse("tenant-a/domain/purpose/r002").unwrap();
    let child_hash = B3Hash::hash(b"child_adapter");

    registry
        .register_adapter_with_name(
            "child-id",
            Some(&child_name),
            &child_hash,
            "persistent",
            16,
            &vec!["tenant-a".to_string()],
            Some("parent-id"),
            Some(ForkType::Extension),
        )
        .expect("Failed to register child");

    // Verify lineage
    let child = registry
        .get_adapter("child-id")
        .expect("Failed to get child")
        .expect("Child not found");

    assert_eq!(child.parent_id, Some("parent-id".to_string()));
    assert_eq!(child.fork_type, Some(ForkType::Extension));

    // List adapters in lineage
    let lineage = registry
        .list_adapters_in_lineage("tenant-a", "domain", "purpose")
        .expect("Failed to list lineage");

    assert_eq!(lineage.len(), 2);
    assert_eq!(lineage[0].id, "child-id"); // Latest first
    assert_eq!(lineage[1].id, "parent-id");
}

#[test]
fn test_registry_next_revision() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_revision.db");
    let registry = Registry::open(&db_path).expect("Failed to open registry");

    registry
        .register_tenant("tenant-a", 1000, 1000)
        .expect("Failed to register tenant");

    // No adapters yet, should return 1
    let next = registry
        .next_revision_number("tenant-a", "domain", "purpose")
        .expect("Failed to get next revision");
    assert_eq!(next, 1);

    // Register r001
    let name1 = AdapterName::parse("tenant-a/domain/purpose/r001").unwrap();
    registry
        .register_adapter_with_name(
            "adapter-1",
            Some(&name1),
            &B3Hash::hash(b"data1"),
            "persistent",
            16,
            &vec!["tenant-a".to_string()],
            None,
            None,
        )
        .expect("Failed to register adapter");

    let next = registry
        .next_revision_number("tenant-a", "domain", "purpose")
        .expect("Failed to get next revision");
    assert_eq!(next, 2);

    // Register r002
    let name2 = AdapterName::parse("tenant-a/domain/purpose/r002").unwrap();
    registry
        .register_adapter_with_name(
            "adapter-2",
            Some(&name2),
            &B3Hash::hash(b"data2"),
            "persistent",
            16,
            &vec!["tenant-a".to_string()],
            None,
            None,
        )
        .expect("Failed to register adapter");

    let next = registry
        .next_revision_number("tenant-a", "domain", "purpose")
        .expect("Failed to get next revision");
    assert_eq!(next, 3);

    // Get latest revision
    let latest = registry
        .get_latest_revision("tenant-a", "domain", "purpose")
        .expect("Failed to get latest")
        .expect("No latest found");
    assert_eq!(latest.id, "adapter-2");
}

#[test]
fn test_registry_duplicate_name_rejection() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_duplicate.db");
    let registry = Registry::open(&db_path).expect("Failed to open registry");

    registry
        .register_tenant("tenant-a", 1000, 1000)
        .expect("Failed to register tenant");

    let name = AdapterName::parse("tenant-a/domain/purpose/r001").unwrap();

    // First registration should succeed
    registry
        .register_adapter_with_name(
            "adapter-1",
            Some(&name),
            &B3Hash::hash(b"data1"),
            "persistent",
            16,
            &vec![],
            None,
            None,
        )
        .expect("Failed to register first adapter");

    // Duplicate name should fail
    let result = registry.register_adapter_with_name(
        "adapter-2",
        Some(&name),
        &B3Hash::hash(b"data2"),
        "persistent",
        16,
        &vec![],
        None,
        None,
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, AosError::Registry(_)));
    assert!(err.to_string().contains("already exists"));
}

#[test]
fn test_registry_invalid_parent_rejection() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_invalid_parent.db");
    let registry = Registry::open(&db_path).expect("Failed to open registry");

    registry
        .register_tenant("tenant-a", 1000, 1000)
        .expect("Failed to register tenant");

    let name = AdapterName::parse("tenant-a/domain/purpose/r001").unwrap();

    // Try to register with non-existent parent
    let result = registry.register_adapter_with_name(
        "adapter-1",
        Some(&name),
        &B3Hash::hash(b"data1"),
        "persistent",
        16,
        &vec![],
        Some("non-existent-parent"),
        Some(ForkType::Extension),
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, AosError::Registry(_)));
    assert!(err.to_string().contains("does not exist"));
}

#[test]
fn test_registry_fork_type_required_with_parent() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_fork_type.db");
    let registry = Registry::open(&db_path).expect("Failed to open registry");

    registry
        .register_tenant("tenant-a", 1000, 1000)
        .expect("Failed to register tenant");

    // Register parent
    let parent_name = AdapterName::parse("tenant-a/domain/purpose/r001").unwrap();
    registry
        .register_adapter_with_name(
            "parent-id",
            Some(&parent_name),
            &B3Hash::hash(b"parent"),
            "persistent",
            16,
            &vec![],
            None,
            None,
        )
        .expect("Failed to register parent");

    // Try to register child without fork_type
    let child_name = AdapterName::parse("tenant-a/domain/purpose/r002").unwrap();
    let result = registry.register_adapter_with_name(
        "child-id",
        Some(&child_name),
        &B3Hash::hash(b"child"),
        "persistent",
        16,
        &vec![],
        Some("parent-id"),
        None, // Missing fork_type
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, AosError::Registry(_)));
    assert!(err.to_string().contains("fork_type must be specified"));
}

#[test]
fn test_adapter_record_helpers() {
    let name1 = AdapterName::parse("tenant/domain/purpose/r001").unwrap();
    let name2 = AdapterName::parse("tenant/domain/purpose/r002").unwrap();
    let name3 = AdapterName::parse("tenant/other/purpose/r001").unwrap();

    let record1 = adapteros_registry::AdapterRecord {
        id: "adapter-1".to_string(),
        hash: B3Hash::hash(b"data1"),
        tier: "persistent".to_string(),
        rank: 16,
        acl: vec![],
        activation_pct: 0.0,
        registered_at: "2025-01-01T00:00:00Z".to_string(),
        semantic_name: Some(name1.clone()),
        parent_id: None,
        fork_type: None,
        fork_reason: None,
    };

    let record2 = adapteros_registry::AdapterRecord {
        id: "adapter-2".to_string(),
        hash: B3Hash::hash(b"data2"),
        tier: "persistent".to_string(),
        rank: 16,
        acl: vec![],
        activation_pct: 0.0,
        registered_at: "2025-01-01T00:00:00Z".to_string(),
        semantic_name: Some(name2.clone()),
        parent_id: Some("adapter-1".to_string()),
        fork_type: Some(ForkType::Extension),
        fork_reason: Some("Bug fixes".to_string()),
    };

    let record3 = adapteros_registry::AdapterRecord {
        id: "adapter-3".to_string(),
        hash: B3Hash::hash(b"data3"),
        tier: "persistent".to_string(),
        rank: 16,
        acl: vec![],
        activation_pct: 0.0,
        registered_at: "2025-01-01T00:00:00Z".to_string(),
        semantic_name: Some(name3.clone()),
        parent_id: None,
        fork_type: None,
        fork_reason: None,
    };

    // Test display_name
    assert_eq!(record1.display_name(), "tenant/domain/purpose (rev 1)");

    // Test is_in_lineage
    assert!(record1.is_in_lineage(&record2));
    assert!(!record1.is_in_lineage(&record3));

    // Test is_descendant_of
    assert!(record2.is_descendant_of("adapter-1"));
    assert!(!record1.is_descendant_of("adapter-2"));
}

#[test]
fn test_fork_type_serialization() {
    assert_eq!(ForkType::Independent.as_str(), "independent");
    assert_eq!(ForkType::Extension.as_str(), "extension");

    assert_eq!(
        ForkType::from_str("independent").unwrap(),
        ForkType::Independent
    );
    assert_eq!(
        ForkType::from_str("extension").unwrap(),
        ForkType::Extension
    );

    assert!(ForkType::from_str("invalid").is_err());
}
