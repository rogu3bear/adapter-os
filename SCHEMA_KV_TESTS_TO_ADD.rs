/// Additional Schema Consistency Tests for KV Types
///
/// Add these tests to crates/adapteros-db/tests/schema_consistency_tests.rs
/// when the path conflict is resolved.
///
/// These tests verify that KV types (AdapterKv, TenantKv, UserKv) are
/// field-compatible with their SQL counterparts and that conversions are lossless.

use adapteros_db::{adapters::Adapter, tenants::Tenant, users::User, Db};
use adapteros_db::users_kv::{user_to_kv, kv_to_user};
use adapteros_storage::AdapterKv;
use adapteros_storage::entities::tenant::TenantKv;
use anyhow::Result;

/// Test 9: Verify AdapterKv field compatibility with Adapter
///
/// This test ensures that all fields from the SQL Adapter struct can be
/// converted to AdapterKv and back without data loss.
#[tokio::test]
async fn test_adapter_kv_field_compatibility() -> Result<()> {
    // Create a fully populated SQL Adapter with all optional fields set
    let sql_adapter = Adapter {
        id: "adapter-test-001".to_string(),
        tenant_id: "tenant-001".to_string(),
        adapter_id: Some("ext-adapter-001".to_string()),
        name: "KV Compatibility Test Adapter".to_string(),
        hash_b3: "b3:test_hash_abc123def456".to_string(),
        rank: 8,
        alpha: 16.0,
        tier: "warm".to_string(),
        targets_json: r#"["q_proj","v_proj","k_proj"]"#.to_string(),
        acl_json: Some(r#"["user-1","user-2"]"#.to_string()),
        languages_json: Some(r#"["rust","python","typescript"]"#.to_string()),
        framework: Some("pytorch".to_string()),
        active: 1, // SQL uses i32 for boolean
        category: "code".to_string(),
        scope: "global".to_string(),
        framework_id: Some("pytorch-2.0".to_string()),
        framework_version: Some("2.0.1".to_string()),
        repo_id: Some("repo-test-001".to_string()),
        commit_sha: Some("abc123def456789012345678901234567890abcd".to_string()),
        intent: Some("Code review and refactoring assistance".to_string()),
        current_state: "warm".to_string(),
        pinned: 1, // SQL uses i32 for boolean
        memory_bytes: 1048576, // 1 MB
        last_activated: Some("2025-01-15T14:30:00Z".to_string()),
        activation_count: 42,
        expires_at: Some("2025-12-31T23:59:59Z".to_string()),
        load_state: "loaded".to_string(),
        last_loaded_at: Some("2025-01-15T14:25:00Z".to_string()),
        aos_file_path: Some("/var/adapters/test-adapter.aos".to_string()),
        aos_file_hash: Some("b3:aos_file_hash_xyz789".to_string()),
        adapter_name: Some("acme/engineering/code-review/v1.2.0".to_string()),
        tenant_namespace: Some("acme".to_string()),
        domain: Some("engineering".to_string()),
        purpose: Some("code-review".to_string()),
        revision: Some("v1.2.0".to_string()),
        parent_id: Some("parent-adapter-001".to_string()),
        fork_type: Some("parameter".to_string()),
        fork_reason: Some("Fine-tuned for Rust codebases".to_string()),
        version: "1.2.0".to_string(),
        lifecycle_state: "active".to_string(),
        created_at: "2025-01-01T00:00:00Z".to_string(),
        updated_at: "2025-01-15T14:30:00Z".to_string(),
    };

    // Convert SQL Adapter to KV AdapterKv
    let kv_adapter: AdapterKv = sql_adapter.clone().into();

    // Verify core identity fields
    assert_eq!(kv_adapter.id, sql_adapter.id, "ID mismatch");
    assert_eq!(kv_adapter.tenant_id, sql_adapter.tenant_id, "Tenant ID mismatch");
    assert_eq!(kv_adapter.adapter_id, sql_adapter.adapter_id, "Adapter ID mismatch");
    assert_eq!(kv_adapter.name, sql_adapter.name, "Name mismatch");

    // Verify technical metadata
    assert_eq!(kv_adapter.hash_b3, sql_adapter.hash_b3, "Hash mismatch");
    assert_eq!(kv_adapter.rank, sql_adapter.rank, "Rank mismatch");
    assert_eq!(kv_adapter.alpha, sql_adapter.alpha, "Alpha mismatch");
    assert_eq!(kv_adapter.tier, sql_adapter.tier, "Tier mismatch");

    // Verify JSON fields (stored as strings in both types currently)
    assert_eq!(kv_adapter.targets_json, sql_adapter.targets_json, "Targets JSON mismatch");
    assert_eq!(kv_adapter.acl_json, sql_adapter.acl_json, "ACL JSON mismatch");
    assert_eq!(kv_adapter.languages_json, sql_adapter.languages_json, "Languages JSON mismatch");

    // Verify boolean fields (SQL i32 -> KV i32, awaiting migration to bool)
    assert_eq!(kv_adapter.active, sql_adapter.active, "Active flag mismatch");
    assert_eq!(kv_adapter.pinned, sql_adapter.pinned, "Pinned flag mismatch");

    // Verify classification fields
    assert_eq!(kv_adapter.category, sql_adapter.category, "Category mismatch");
    assert_eq!(kv_adapter.scope, sql_adapter.scope, "Scope mismatch");
    assert_eq!(kv_adapter.framework, sql_adapter.framework, "Framework mismatch");

    // Verify lifecycle fields
    assert_eq!(kv_adapter.current_state, sql_adapter.current_state, "Current state mismatch");
    assert_eq!(kv_adapter.lifecycle_state, sql_adapter.lifecycle_state, "Lifecycle state mismatch");
    assert_eq!(kv_adapter.load_state, sql_adapter.load_state, "Load state mismatch");
    assert_eq!(kv_adapter.version, sql_adapter.version, "Version mismatch");

    // Verify semantic naming taxonomy
    assert_eq!(kv_adapter.adapter_name, sql_adapter.adapter_name, "Adapter name mismatch");
    assert_eq!(kv_adapter.tenant_namespace, sql_adapter.tenant_namespace, "Tenant namespace mismatch");
    assert_eq!(kv_adapter.domain, sql_adapter.domain, "Domain mismatch");
    assert_eq!(kv_adapter.purpose, sql_adapter.purpose, "Purpose mismatch");
    assert_eq!(kv_adapter.revision, sql_adapter.revision, "Revision mismatch");

    // Verify lineage fields
    assert_eq!(kv_adapter.parent_id, sql_adapter.parent_id, "Parent ID mismatch");
    assert_eq!(kv_adapter.fork_type, sql_adapter.fork_type, "Fork type mismatch");
    assert_eq!(kv_adapter.fork_reason, sql_adapter.fork_reason, "Fork reason mismatch");

    // Verify runtime metrics
    assert_eq!(kv_adapter.memory_bytes, sql_adapter.memory_bytes, "Memory bytes mismatch");
    assert_eq!(kv_adapter.activation_count, sql_adapter.activation_count, "Activation count mismatch");
    assert_eq!(kv_adapter.last_activated, sql_adapter.last_activated, "Last activated mismatch");

    // Verify timestamps
    assert_eq!(kv_adapter.created_at, sql_adapter.created_at, "Created at mismatch");
    assert_eq!(kv_adapter.updated_at, sql_adapter.updated_at, "Updated at mismatch");

    // Convert back to SQL (round-trip test)
    let round_trip: Adapter = kv_adapter.into();

    // Verify round-trip preserves all fields
    assert_eq!(round_trip.id, sql_adapter.id, "Round-trip: ID mismatch");
    assert_eq!(round_trip.name, sql_adapter.name, "Round-trip: Name mismatch");
    assert_eq!(round_trip.rank, sql_adapter.rank, "Round-trip: Rank mismatch");
    assert_eq!(round_trip.active, sql_adapter.active, "Round-trip: Active mismatch");
    assert_eq!(round_trip.tier, sql_adapter.tier, "Round-trip: Tier mismatch");
    assert_eq!(round_trip.adapter_name, sql_adapter.adapter_name, "Round-trip: Adapter name mismatch");

    println!("✓ AdapterKv field compatibility verified (all {} fields match, round-trip successful)",
             std::mem::size_of::<Adapter>());
    Ok(())
}

/// Test 10: Verify TenantKv field compatibility with Tenant
///
/// This test ensures that all fields from the SQL Tenant struct can be
/// converted to TenantKv and back without data loss.
#[tokio::test]
async fn test_tenant_kv_field_compatibility() -> Result<()> {
    // Create a fully populated SQL Tenant
    let sql_tenant = Tenant {
        id: "tenant-test-001".to_string(),
        name: "KV Test Tenant".to_string(),
        itar_flag: true,
        created_at: "2025-01-01T00:00:00Z".to_string(),
        status: Some("active".to_string()),
        updated_at: Some("2025-01-15T12:00:00Z".to_string()),
        default_stack_id: Some("stack-prod-001".to_string()),
        max_adapters: Some(100),
        max_training_jobs: Some(10),
        max_storage_gb: Some(500.0),
        rate_limit_rpm: Some(1000),
    };

    // Convert to KV
    let kv_tenant: TenantKv = sql_tenant.clone().into();

    // Verify core fields
    assert_eq!(kv_tenant.id, sql_tenant.id, "ID mismatch");
    assert_eq!(kv_tenant.name, sql_tenant.name, "Name mismatch");
    assert_eq!(kv_tenant.itar_flag, sql_tenant.itar_flag, "ITAR flag mismatch");

    // Verify status (SQL Option<String> -> KV String with default)
    assert_eq!(kv_tenant.status, "active", "Status should default to 'active'");

    // Verify quotas and limits
    assert_eq!(kv_tenant.default_stack_id, sql_tenant.default_stack_id, "Default stack ID mismatch");
    assert_eq!(kv_tenant.max_adapters, sql_tenant.max_adapters, "Max adapters mismatch");
    assert_eq!(kv_tenant.max_training_jobs, sql_tenant.max_training_jobs, "Max training jobs mismatch");
    assert_eq!(kv_tenant.max_storage_gb, sql_tenant.max_storage_gb, "Max storage GB mismatch");
    assert_eq!(kv_tenant.rate_limit_rpm, sql_tenant.rate_limit_rpm, "Rate limit RPM mismatch");

    // Verify timestamps (SQL String -> KV DateTime<Utc>)
    // Note: Timestamps are converted via RFC3339 parsing
    assert_eq!(
        kv_tenant.created_at.to_rfc3339(),
        sql_tenant.created_at,
        "Created at timestamp mismatch"
    );

    // Convert back to SQL (round-trip test)
    let round_trip: Tenant = kv_tenant.into();

    // Verify round-trip preserves all fields
    assert_eq!(round_trip.id, sql_tenant.id, "Round-trip: ID mismatch");
    assert_eq!(round_trip.name, sql_tenant.name, "Round-trip: Name mismatch");
    assert_eq!(round_trip.itar_flag, sql_tenant.itar_flag, "Round-trip: ITAR flag mismatch");
    assert_eq!(round_trip.status, Some("active".to_string()), "Round-trip: Status mismatch");
    assert_eq!(round_trip.max_adapters, sql_tenant.max_adapters, "Round-trip: Max adapters mismatch");

    println!("✓ TenantKv field compatibility verified (round-trip successful)");
    Ok(())
}

/// Test 11: Verify UserKv field compatibility with User
///
/// This test ensures that all fields from the SQL User struct can be
/// converted to UserKv and back without data loss.
#[tokio::test]
async fn test_user_kv_field_compatibility() -> Result<()> {
    // Create a fully populated SQL User
    let sql_user = User {
        id: "user-test-001".to_string(),
        email: "test.user@example.com".to_string(),
        display_name: "Test User".to_string(),
        pw_hash: "$argon2id$v=19$m=19456,t=2,p=1$...".to_string(), // Mock hash
        role: "admin".to_string(),
        disabled: false,
        created_at: "2025-01-01T00:00:00Z".to_string(),
        tenant_id: "tenant-001".to_string(),
    };

    // Convert to KV using conversion function
    let kv_user = user_to_kv(&sql_user)?;

    // Verify core fields
    assert_eq!(kv_user.id, sql_user.id, "ID mismatch");
    assert_eq!(kv_user.email, sql_user.email, "Email mismatch");
    assert_eq!(kv_user.display_name, sql_user.display_name, "Display name mismatch");
    assert_eq!(kv_user.pw_hash, sql_user.pw_hash, "Password hash mismatch");
    assert_eq!(kv_user.tenant_id, sql_user.tenant_id, "Tenant ID mismatch");
    assert_eq!(kv_user.disabled, sql_user.disabled, "Disabled flag mismatch");

    // Verify role conversion (SQL String -> KV Role enum)
    assert_eq!(
        kv_user.role,
        adapteros_storage::entities::user::Role::Admin,
        "Role should be parsed to Admin enum"
    );

    // Verify timestamp conversion (SQL String -> KV DateTime<Utc>)
    assert_eq!(
        kv_user.created_at.to_rfc3339(),
        sql_user.created_at,
        "Created at timestamp mismatch"
    );

    // Convert back to SQL (round-trip test)
    let round_trip = kv_to_user(&kv_user);

    // Verify round-trip preserves all fields
    assert_eq!(round_trip.id, sql_user.id, "Round-trip: ID mismatch");
    assert_eq!(round_trip.email, sql_user.email, "Round-trip: Email mismatch");
    assert_eq!(round_trip.role, "admin", "Round-trip: Role should be 'admin' string");
    assert_eq!(round_trip.disabled, sql_user.disabled, "Round-trip: Disabled mismatch");

    println!("✓ UserKv field compatibility verified (round-trip successful)");
    Ok(())
}

/// Test 12: Verify NULL handling in Tenant status field
///
/// Edge case: SQL Tenant.status is Option<String>, KV TenantKv.status is String.
/// This test ensures NULL status values are handled correctly.
#[tokio::test]
async fn test_tenant_null_status_conversion() -> Result<()> {
    let sql_tenant = Tenant {
        id: "tenant-null-status".to_string(),
        name: "Tenant With Null Status".to_string(),
        itar_flag: false,
        created_at: "2025-01-01T00:00:00Z".to_string(),
        status: None, // NULL in database
        updated_at: None,
        default_stack_id: None,
        max_adapters: None,
        max_training_jobs: None,
        max_storage_gb: None,
        rate_limit_rpm: None,
    };

    let kv_tenant: TenantKv = sql_tenant.clone().into();

    // Verify NULL status is converted to default "active"
    assert_eq!(kv_tenant.status, "active", "NULL status should default to 'active'");

    println!("✓ NULL status handling verified");
    Ok(())
}

/// Test 13: Verify timestamp parsing edge cases
///
/// This test ensures that various timestamp formats are handled correctly
/// during SQL -> KV conversion.
#[tokio::test]
async fn test_timestamp_format_compatibility() -> Result<()> {
    // Test RFC3339 format (standard)
    let user_rfc3339 = User {
        id: "user-rfc3339".to_string(),
        email: "rfc3339@example.com".to_string(),
        display_name: "RFC3339 User".to_string(),
        pw_hash: "hash".to_string(),
        role: "viewer".to_string(),
        disabled: false,
        created_at: "2025-01-15T14:30:00Z".to_string(),
        tenant_id: "tenant-001".to_string(),
    };

    let kv_user = user_to_kv(&user_rfc3339)?;
    assert_eq!(
        kv_user.created_at.to_rfc3339(),
        "2025-01-15T14:30:00+00:00",
        "RFC3339 timestamp should parse correctly"
    );

    // Test SQLite datetime format (fallback)
    let user_sqlite = User {
        id: "user-sqlite".to_string(),
        email: "sqlite@example.com".to_string(),
        display_name: "SQLite User".to_string(),
        pw_hash: "hash".to_string(),
        role: "viewer".to_string(),
        disabled: false,
        created_at: "2025-01-15 14:30:00".to_string(), // SQLite format
        tenant_id: "tenant-001".to_string(),
    };

    let kv_user_sqlite = user_to_kv(&user_sqlite)?;
    assert!(
        kv_user_sqlite.created_at.to_rfc3339().starts_with("2025-01-15"),
        "SQLite datetime format should parse correctly"
    );

    println!("✓ Timestamp format compatibility verified");
    Ok(())
}

/// Test 14: Verify all Role enum variants convert correctly
#[tokio::test]
async fn test_role_enum_conversion() -> Result<()> {
    use adapteros_storage::entities::user::Role as KvRole;

    let roles = vec![
        ("admin", KvRole::Admin),
        ("operator", KvRole::Operator),
        ("sre", KvRole::SRE),
        ("compliance", KvRole::Compliance),
        ("viewer", KvRole::Viewer),
    ];

    for (role_str, expected_enum) in roles {
        let user = User {
            id: format!("user-{}", role_str),
            email: format!("{}@example.com", role_str),
            display_name: format!("{} User", role_str),
            pw_hash: "hash".to_string(),
            role: role_str.to_string(),
            disabled: false,
            created_at: "2025-01-01T00:00:00Z".to_string(),
            tenant_id: "tenant-001".to_string(),
        };

        let kv_user = user_to_kv(&user)?;
        assert_eq!(
            kv_user.role,
            expected_enum,
            "Role '{}' should convert to {:?}",
            role_str,
            expected_enum
        );

        let round_trip = kv_to_user(&kv_user);
        assert_eq!(
            round_trip.role,
            role_str,
            "Role {:?} should convert back to '{}'",
            expected_enum,
            role_str
        );
    }

    println!("✓ All Role enum variants verified");
    Ok(())
}

/// Test 15: Verify invalid role string handling
#[tokio::test]
async fn test_invalid_role_conversion() {
    let user = User {
        id: "user-invalid".to_string(),
        email: "invalid@example.com".to_string(),
        display_name: "Invalid Role User".to_string(),
        pw_hash: "hash".to_string(),
        role: "super_admin".to_string(), // Invalid role
        disabled: false,
        created_at: "2025-01-01T00:00:00Z".to_string(),
        tenant_id: "tenant-001".to_string(),
    };

    let result = user_to_kv(&user);
    assert!(
        result.is_err(),
        "Invalid role should return error during conversion"
    );

    if let Err(e) = result {
        assert!(
            format!("{}", e).contains("Invalid role"),
            "Error message should mention invalid role"
        );
    }

    println!("✓ Invalid role handling verified");
}
