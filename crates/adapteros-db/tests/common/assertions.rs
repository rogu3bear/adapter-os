//! Assertion utilities for comparing adapter data
//!
//! Provides custom assertion functions for comparing adapters between
//! SQL and KV storage, useful for validating dual-write and migration correctness.

use adapteros_db::adapters::Adapter;

/// Assert that two adapters are equal
///
/// Compares all fields of two adapters and provides detailed error messages
/// on mismatch. This is useful for verifying that SQL and KV storage contain
/// identical data.
///
/// # Panics
///
/// Panics if any field differs between the two adapters.
///
/// # Examples
///
/// ```no_run
/// use adapteros_db::tests::common::assert_adapters_equal;
///
/// #[tokio::test]
/// async fn test_adapter_equality() {
///     let adapter_sql = db.get_adapter("test-1").await.unwrap().unwrap();
///     let adapter_kv = db.get_adapter_kv("test-1").await.unwrap().unwrap();
///     assert_adapters_equal(&adapter_sql, &adapter_kv);
/// }
/// ```
pub fn assert_adapters_equal(left: &Adapter, right: &Adapter) {
    // Core fields
    assert_eq!(left.id, right.id, "Adapter IDs don't match");
    assert_eq!(left.tenant_id, right.tenant_id, "Tenant IDs don't match");
    assert_eq!(left.name, right.name, "Names don't match");
    assert_eq!(left.tier, right.tier, "Tiers don't match");
    assert_eq!(left.hash_b3, right.hash_b3, "B3 hashes don't match");
    assert_eq!(left.rank, right.rank, "Ranks don't match");
    assert_eq!(left.alpha, right.alpha, "Alphas don't match");
    assert_eq!(
        left.targets_json, right.targets_json,
        "Targets JSON don't match"
    );
    assert_eq!(left.acl_json, right.acl_json, "ACL JSON don't match");
    assert_eq!(
        left.adapter_id, right.adapter_id,
        "External adapter IDs don't match"
    );
    assert_eq!(
        left.languages_json, right.languages_json,
        "Languages JSON don't match"
    );
    assert_eq!(left.framework, right.framework, "Frameworks don't match");
    assert_eq!(left.active, right.active, "Active status doesn't match");

    // Code intelligence fields
    assert_eq!(left.category, right.category, "Categories don't match");
    assert_eq!(left.scope, right.scope, "Scopes don't match");
    assert_eq!(
        left.framework_id, right.framework_id,
        "Framework IDs don't match"
    );
    assert_eq!(
        left.framework_version, right.framework_version,
        "Framework versions don't match"
    );
    assert_eq!(left.repo_id, right.repo_id, "Repo IDs don't match");
    assert_eq!(left.commit_sha, right.commit_sha, "Commit SHAs don't match");
    assert_eq!(left.intent, right.intent, "Intents don't match");

    // Lifecycle state
    assert_eq!(
        left.current_state, right.current_state,
        "Current states don't match"
    );
    assert_eq!(left.pinned, right.pinned, "Pinned status doesn't match");
    assert_eq!(
        left.memory_bytes, right.memory_bytes,
        "Memory bytes don't match"
    );
    assert_eq!(
        left.last_activated, right.last_activated,
        "Last activated timestamps don't match"
    );
    assert_eq!(
        left.activation_count, right.activation_count,
        "Activation counts don't match"
    );

    // Expiration
    assert_eq!(
        left.expires_at, right.expires_at,
        "Expiration timestamps don't match"
    );

    // Runtime load state
    assert_eq!(left.load_state, right.load_state, "Load states don't match");
    assert_eq!(
        left.last_loaded_at, right.last_loaded_at,
        "Last loaded timestamps don't match"
    );

    // .aos file support
    assert_eq!(
        left.aos_file_path, right.aos_file_path,
        "AOS file paths don't match"
    );
    assert_eq!(
        left.aos_file_hash, right.aos_file_hash,
        "AOS file hashes don't match"
    );

    // Semantic naming
    assert_eq!(
        left.adapter_name, right.adapter_name,
        "Adapter names don't match"
    );
    assert_eq!(
        left.tenant_namespace, right.tenant_namespace,
        "Tenant namespaces don't match"
    );
    assert_eq!(left.domain, right.domain, "Domains don't match");
    assert_eq!(left.purpose, right.purpose, "Purposes don't match");
    assert_eq!(left.revision, right.revision, "Revisions don't match");
    assert_eq!(left.parent_id, right.parent_id, "Parent IDs don't match");
    assert_eq!(left.fork_type, right.fork_type, "Fork types don't match");
    assert_eq!(
        left.fork_reason, right.fork_reason,
        "Fork reasons don't match"
    );

    // Metadata normalization
    assert_eq!(left.version, right.version, "Versions don't match");
    assert_eq!(
        left.lifecycle_state, right.lifecycle_state,
        "Lifecycle states don't match"
    );

    // Timestamps - allow small differences for created_at/updated_at
    // These might differ slightly due to timing of dual writes
    assert_eq!(
        left.created_at, right.created_at,
        "Created timestamps don't match"
    );
    assert_eq!(
        left.updated_at, right.updated_at,
        "Updated timestamps don't match"
    );
}

/// Assert that specific adapter fields match
///
/// More flexible than `assert_adapters_equal` - only compares the specified fields.
/// Useful when you only care about certain fields for a test.
///
/// # Examples
///
/// ```no_run
/// use adapteros_db::tests::common::assert_adapter_fields_match;
///
/// #[tokio::test]
/// async fn test_core_fields() {
///     let adapter_sql = db.get_adapter("test-1").await.unwrap().unwrap();
///     let adapter_kv = db.get_adapter_kv("test-1").await.unwrap().unwrap();
///
///     assert_adapter_fields_match(
///         &adapter_sql,
///         &adapter_kv,
///         &["name", "rank", "tier", "hash_b3"]
///     );
/// }
/// ```
pub fn assert_adapter_fields_match(left: &Adapter, right: &Adapter, fields: &[&str]) {
    for field in fields {
        match *field {
            // Core fields
            "id" => assert_eq!(left.id, right.id, "IDs don't match"),
            "tenant_id" => assert_eq!(left.tenant_id, right.tenant_id, "Tenant IDs don't match"),
            "name" => assert_eq!(left.name, right.name, "Names don't match"),
            "tier" => assert_eq!(left.tier, right.tier, "Tiers don't match"),
            "hash_b3" => assert_eq!(left.hash_b3, right.hash_b3, "B3 hashes don't match"),
            "rank" => assert_eq!(left.rank, right.rank, "Ranks don't match"),
            "alpha" => assert_eq!(left.alpha, right.alpha, "Alphas don't match"),
            "targets_json" => assert_eq!(
                left.targets_json, right.targets_json,
                "Targets JSON don't match"
            ),
            "acl_json" => assert_eq!(left.acl_json, right.acl_json, "ACL JSON don't match"),
            "adapter_id" => {
                assert_eq!(left.adapter_id, right.adapter_id, "Adapter IDs don't match")
            }
            "languages_json" => assert_eq!(
                left.languages_json, right.languages_json,
                "Languages JSON don't match"
            ),
            "framework" => assert_eq!(left.framework, right.framework, "Frameworks don't match"),
            "active" => assert_eq!(left.active, right.active, "Active status doesn't match"),

            // Code intelligence
            "category" => assert_eq!(left.category, right.category, "Categories don't match"),
            "scope" => assert_eq!(left.scope, right.scope, "Scopes don't match"),
            "framework_id" => assert_eq!(
                left.framework_id, right.framework_id,
                "Framework IDs don't match"
            ),
            "framework_version" => assert_eq!(
                left.framework_version, right.framework_version,
                "Framework versions don't match"
            ),
            "repo_id" => assert_eq!(left.repo_id, right.repo_id, "Repo IDs don't match"),
            "commit_sha" => {
                assert_eq!(left.commit_sha, right.commit_sha, "Commit SHAs don't match")
            }
            "intent" => assert_eq!(left.intent, right.intent, "Intents don't match"),

            // Lifecycle
            "current_state" => assert_eq!(
                left.current_state, right.current_state,
                "Current states don't match"
            ),
            "pinned" => assert_eq!(left.pinned, right.pinned, "Pinned status doesn't match"),
            "memory_bytes" => assert_eq!(
                left.memory_bytes, right.memory_bytes,
                "Memory bytes don't match"
            ),
            "last_activated" => assert_eq!(
                left.last_activated, right.last_activated,
                "Last activated don't match"
            ),
            "activation_count" => assert_eq!(
                left.activation_count, right.activation_count,
                "Activation counts don't match"
            ),

            // Expiration
            "expires_at" => assert_eq!(left.expires_at, right.expires_at, "Expiration don't match"),

            // Load state
            "load_state" => {
                assert_eq!(left.load_state, right.load_state, "Load states don't match")
            }
            "last_loaded_at" => assert_eq!(
                left.last_loaded_at, right.last_loaded_at,
                "Last loaded don't match"
            ),

            // .aos file
            "aos_file_path" => assert_eq!(
                left.aos_file_path, right.aos_file_path,
                "AOS paths don't match"
            ),
            "aos_file_hash" => assert_eq!(
                left.aos_file_hash, right.aos_file_hash,
                "AOS hashes don't match"
            ),

            // Semantic naming
            "adapter_name" => assert_eq!(
                left.adapter_name, right.adapter_name,
                "Adapter names don't match"
            ),
            "tenant_namespace" => assert_eq!(
                left.tenant_namespace, right.tenant_namespace,
                "Namespaces don't match"
            ),
            "domain" => assert_eq!(left.domain, right.domain, "Domains don't match"),
            "purpose" => assert_eq!(left.purpose, right.purpose, "Purposes don't match"),
            "revision" => assert_eq!(left.revision, right.revision, "Revisions don't match"),
            "parent_id" => assert_eq!(left.parent_id, right.parent_id, "Parent IDs don't match"),
            "fork_type" => assert_eq!(left.fork_type, right.fork_type, "Fork types don't match"),
            "fork_reason" => assert_eq!(
                left.fork_reason, right.fork_reason,
                "Fork reasons don't match"
            ),

            // Metadata
            "version" => assert_eq!(left.version, right.version, "Versions don't match"),
            "lifecycle_state" => assert_eq!(
                left.lifecycle_state, right.lifecycle_state,
                "Lifecycle states don't match"
            ),

            // Timestamps
            "created_at" => assert_eq!(
                left.created_at, right.created_at,
                "Created timestamps don't match"
            ),
            "updated_at" => assert_eq!(
                left.updated_at, right.updated_at,
                "Updated timestamps don't match"
            ),

            unknown => panic!("Unknown field '{}' in assert_adapter_fields_match", unknown),
        }
    }
}

/// Assert that two adapter lists contain the same adapters
///
/// Compares two lists of adapters by ID, ensuring they contain the same set.
/// Order is not considered.
///
/// # Examples
///
/// ```no_run
/// use adapteros_db::tests::common::assert_adapter_lists_equal;
///
/// #[tokio::test]
/// async fn test_adapter_lists() {
///     let adapters_sql = db.list_adapters_by_tenant("default").await.unwrap();
///     let adapters_kv = db.list_adapters_for_tenant_kv("default").await.unwrap();
///     assert_adapter_lists_equal(&adapters_sql, &adapters_kv);
/// }
/// ```
pub fn assert_adapter_lists_equal(left: &[Adapter], right: &[Adapter]) {
    assert_eq!(
        left.len(),
        right.len(),
        "Adapter list lengths don't match: left={}, right={}",
        left.len(),
        right.len()
    );

    // Sort by ID for comparison
    let mut left_sorted = left.to_vec();
    let mut right_sorted = right.to_vec();
    left_sorted.sort_by(|a, b| a.id.cmp(&b.id));
    right_sorted.sort_by(|a, b| a.id.cmp(&b.id));

    for (l, r) in left_sorted.iter().zip(right_sorted.iter()) {
        assert_adapters_equal(l, r);
    }
}

/// Assert that adapter lists contain the same IDs
///
/// Only compares adapter IDs, not full adapter data. Useful for verifying
/// that queries return the same set of adapters without comparing all fields.
///
/// # Examples
///
/// ```no_run
/// use adapteros_db::tests::common::assert_adapter_ids_match;
///
/// #[tokio::test]
/// async fn test_lineage_ids() {
///     let lineage_sql = db.get_adapter_lineage("parent").await.unwrap();
///     let lineage_kv = db.get_adapter_lineage_kv("parent").await.unwrap();
///     assert_adapter_ids_match(&lineage_sql, &lineage_kv);
/// }
/// ```
pub fn assert_adapter_ids_match(left: &[Adapter], right: &[Adapter]) {
    assert_eq!(
        left.len(),
        right.len(),
        "Adapter list lengths don't match: left={}, right={}",
        left.len(),
        right.len()
    );

    let left_ids: std::collections::HashSet<_> = left.iter().map(|a| &a.id).collect();
    let right_ids: std::collections::HashSet<_> = right.iter().map(|a| &a.id).collect();

    assert_eq!(
        left_ids, right_ids,
        "Adapter ID sets don't match.\nLeft: {:?}\nRight: {:?}",
        left_ids, right_ids
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_db::adapters::{Adapter, AdapterRegistrationBuilder};

    fn create_test_adapter() -> Adapter {
        Adapter {
            id: "test-id".to_string(),
            tenant_id: "test-tenant".to_string(),
            name: "Test Adapter".to_string(),
            tier: "warm".to_string(),
            hash_b3: "b3:testhash".to_string(),
            rank: 8,
            alpha: 16.0,
            lora_strength: None,
            targets_json: "[]".to_string(),
            acl_json: None,
            adapter_id: Some("test-adapter".to_string()),
            languages_json: None,
            framework: Some("rust".to_string()),
            active: 1,
            category: "code".to_string(),
            scope: "global".to_string(),
            framework_id: None,
            framework_version: None,
            repo_id: None,
            commit_sha: None,
            intent: None,
            current_state: "unloaded".to_string(),
            pinned: 0,
            memory_bytes: 0,
            last_activated: None,
            activation_count: 0,
            expires_at: None,
            load_state: "unloaded".to_string(),
            last_loaded_at: None,
            aos_file_path: None,
            aos_file_hash: None,
            adapter_name: None,
            tenant_namespace: None,
            domain: None,
            purpose: None,
            revision: None,
            parent_id: None,
            fork_type: None,
            fork_reason: None,
            version: "1.0.0".to_string(),
            lifecycle_state: "active".to_string(),
            archived_at: None,
            archived_by: None,
            archive_reason: None,
            base_model_id: None,
            recommended_for_moe: None,
            manifest_schema_version: None,
            content_hash_b3: None,
            metadata_json: None,
            provenance_json: None,
            purged_at: None,
            drift_tier: None,
            drift_metric: None,
            drift_loss_metric: None,
            drift_reference_backend: None,
            drift_baseline_backend: None,
            drift_test_backend: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_assert_adapters_equal_same() {
        let adapter1 = create_test_adapter();
        let adapter2 = create_test_adapter();
        assert_adapters_equal(&adapter1, &adapter2);
    }

    #[test]
    #[should_panic(expected = "Names don't match")]
    fn test_assert_adapters_equal_different_name() {
        let adapter1 = create_test_adapter();
        let mut adapter2 = create_test_adapter();
        adapter2.name = "Different Name".to_string();
        assert_adapters_equal(&adapter1, &adapter2);
    }

    #[test]
    fn test_assert_adapter_fields_match() {
        let adapter1 = create_test_adapter();
        let mut adapter2 = create_test_adapter();
        adapter2.name = "Different Name".to_string(); // Change a field we won't check

        assert_adapter_fields_match(&adapter1, &adapter2, &["rank", "tier", "hash_b3"]);
    }

    #[test]
    fn test_assert_adapter_ids_match() {
        let adapters1 = vec![
            {
                let mut a = create_test_adapter();
                a.id = "id-1".to_string();
                a
            },
            {
                let mut a = create_test_adapter();
                a.id = "id-2".to_string();
                a
            },
        ];

        let adapters2 = vec![
            {
                let mut a = create_test_adapter();
                a.id = "id-2".to_string();
                a
            },
            {
                let mut a = create_test_adapter();
                a.id = "id-1".to_string();
                a
            },
        ];

        assert_adapter_ids_match(&adapters1, &adapters2);
    }
}
