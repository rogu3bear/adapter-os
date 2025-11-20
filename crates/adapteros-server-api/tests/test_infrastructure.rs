/// Test Infrastructure Validation
///
/// This test validates that all test utilities are working correctly.
mod common;

#[tokio::test]
async fn test_setup_test_db() {
    let db = common::setup_test_db()
        .await
        .expect("Failed to create test database");

    // Verify adapters table exists
    let result = adapteros_db::sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='adapters'",
    )
    .fetch_one(db.pool())
    .await;
    assert!(result.is_ok(), "adapters table should exist");

    // Verify aos_adapter_metadata table exists
    let result = adapteros_db::sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='aos_adapter_metadata'",
    )
    .fetch_one(db.pool())
    .await;
    assert!(result.is_ok(), "aos_adapter_metadata table should exist");

    // Verify indices exist
    let result = adapteros_db::sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_adapters_aos_file_hash'",
    )
    .fetch_one(db.pool())
    .await;
    assert!(
        result.is_ok(),
        "idx_adapters_aos_file_hash index should exist"
    );
}

#[test]
fn test_create_test_jwt() {
    // Test Admin JWT
    let admin_jwt = common::create_test_jwt("admin-user", "Admin", None, None);
    assert!(!admin_jwt.is_empty(), "Admin JWT should not be empty");

    // Test Operator JWT
    let operator_jwt =
        common::create_test_jwt("operator-user", "Operator", Some("my-tenant"), None);
    assert!(!operator_jwt.is_empty(), "Operator JWT should not be empty");

    // Test Viewer JWT
    let viewer_jwt = common::create_test_jwt("viewer-user", "Viewer", None, None);
    assert!(!viewer_jwt.is_empty(), "Viewer JWT should not be empty");
}

#[test]
fn test_admin_claims() {
    let claims = common::test_admin_claims();
    assert_eq!(claims.role, "Admin");
    assert_eq!(claims.tenant_id, "tenant-1");
    assert!(!claims.sub.is_empty());
}

#[test]
fn test_operator_claims() {
    let claims = common::test_operator_claims();
    assert_eq!(claims.role, "Operator");
    assert_eq!(claims.tenant_id, "test-tenant");
    assert_eq!(claims.sub, "operator-user");
}

#[test]
fn test_viewer_claims() {
    let claims = common::test_viewer_claims();
    assert_eq!(claims.role, "Viewer");
    assert_eq!(claims.tenant_id, "test-tenant");
    assert_eq!(claims.sub, "viewer-user");
}

#[test]
fn test_create_test_aos_file() {
    let aos_file = common::create_test_aos_file();

    // Verify file has content
    assert!(!aos_file.is_empty(), ".aos file should not be empty");

    // Verify file has header (at least 8 bytes)
    assert!(
        aos_file.len() >= 8,
        ".aos file should have at least 8-byte header"
    );

    // Verify header structure
    let manifest_offset = u32::from_le_bytes([aos_file[0], aos_file[1], aos_file[2], aos_file[3]]);
    let manifest_len = u32::from_le_bytes([aos_file[4], aos_file[5], aos_file[6], aos_file[7]]);

    assert!(manifest_offset > 0, "Manifest offset should be positive");
    assert!(manifest_len > 0, "Manifest length should be positive");

    // Verify manifest is JSON-parseable
    let manifest_start = manifest_offset as usize;
    let manifest_end = manifest_start + manifest_len as usize;
    assert!(
        manifest_end <= aos_file.len(),
        "Manifest should fit within file"
    );

    let manifest_bytes = &aos_file[manifest_start..manifest_end];
    let manifest_str = std::str::from_utf8(manifest_bytes).expect("Manifest should be valid UTF-8");
    let manifest: serde_json::Value =
        serde_json::from_str(manifest_str).expect("Manifest should be valid JSON");

    // Verify manifest fields
    assert_eq!(manifest["version"], "1.0.0");
    assert_eq!(manifest["name"], "test-adapter");
    assert_eq!(manifest["model_type"], "lora");
}
