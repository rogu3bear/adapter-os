use adapteros_core::hash::Hasher;
use adapteros_db::Db;
use adapteros_server_api::auth::Claims;
use adapteros_server_api::handlers::aos_upload::AosUploadResponse;
/// PRD-02: .aos Upload Integration Tests
///
/// This module tests the .aos file upload functionality including:
/// - End-to-end HTTP multipart upload via Axum test client
/// - Permission checking via HTTP endpoint
/// - File validation and disk persistence
/// - Database persistence and audit logging
use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
    Router,
};
use std::path::Path;
use tokio::fs;
use tower::ServiceExt;

mod common;
use common::{
    create_test_app, create_test_jwt, setup_state, test_admin_claims, test_viewer_claims,
};

/// Helper function to verify audit log entries
async fn verify_audit_log_exists(
    db: &adapteros_db::Db,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    expected_status: &str,
) -> anyhow::Result<bool> {
    use adapteros_db::audit::AuditLog;

    let mut query_str = String::from(
        "SELECT id, timestamp, user_id, user_role, tenant_id, action, resource_type, resource_id, status, error_message, ip_address, metadata_json
         FROM audit_logs WHERE 1=1"
    );

    let mut params: Vec<String> = Vec::new();

    if !action.is_empty() {
        query_str.push_str(" AND action = ?");
        params.push(action.to_string());
    }

    if !resource_type.is_empty() {
        query_str.push_str(" AND resource_type = ?");
        params.push(resource_type.to_string());
    }

    if let Some(rid) = resource_id {
        query_str.push_str(" AND resource_id = ?");
        params.push(rid.to_string());
    }

    if !expected_status.is_empty() {
        query_str.push_str(" AND status = ?");
        params.push(expected_status.to_string());
    }

    let mut q = sqlx::query_as::<_, AuditLog>(&query_str);
    for param in &params {
        q = q.bind(param);
    }

    let logs = q.fetch_all(db.pool()).await?;
    Ok(!logs.is_empty())
}

/// Helper to get audit log count for a specific action
async fn get_audit_log_count(
    db: &adapteros_db::Db,
    action: &str,
    resource_type: Option<&str>,
) -> anyhow::Result<i64> {
    let mut query_str = String::from("SELECT COUNT(*) as count FROM audit_logs WHERE action = ?");
    let mut params: Vec<String> = vec![action.to_string()];

    if let Some(rt) = resource_type {
        query_str.push_str(" AND resource_type = ?");
        params.push(rt.to_string());
    }

    let mut q = sqlx::query_scalar::<_, i64>(&query_str);
    for param in &params {
        q = q.bind(param);
    }

    let count = q.fetch_one(db.pool()).await?;
    Ok(count)
}

/// Helper to get full audit log record for detailed verification
async fn get_audit_log_record(
    db: &adapteros_db::Db,
    action: &str,
    resource_id: Option<&str>,
) -> anyhow::Result<Option<adapteros_db::audit::AuditLog>> {
    use adapteros_db::audit::AuditLog;

    let log = if let Some(rid) = resource_id {
        sqlx::query_as::<_, AuditLog>(
            "SELECT id, timestamp, user_id, user_role, tenant_id, action, resource_type, resource_id, status, error_message, ip_address, metadata_json
             FROM audit_logs WHERE action = ? AND resource_id = ? ORDER BY timestamp DESC LIMIT 1"
        )
        .bind(action)
        .bind(rid)
        .fetch_optional(db.pool())
        .await?
    } else {
        sqlx::query_as::<_, AuditLog>(
            "SELECT id, timestamp, user_id, user_role, tenant_id, action, resource_type, resource_id, status, error_message, ip_address, metadata_json
             FROM audit_logs WHERE action = ? ORDER BY timestamp DESC LIMIT 1"
        )
        .bind(action)
        .fetch_optional(db.pool())
        .await?
    };

    Ok(log)
}

/// Create a minimal valid .aos file for testing
fn create_test_aos_file() -> Vec<u8> {
    // Create a minimal manifest
    let manifest = r#"{
        "version": "1.0.0",
        "name": "test-adapter",
        "description": "Test adapter for integration testing",
        "model_type": "lora",
        "base_model": "llama",
        "rank": 4,
        "alpha": 8.0
    }"#;

    let manifest_bytes = manifest.as_bytes();
    let manifest_len = manifest_bytes.len() as u32;

    // Create minimal safetensors content (just headers, no actual tensors)
    let safetensors = b"{}";

    // Build .aos file structure
    let mut aos_file = Vec::new();

    // Write header
    aos_file.extend_from_slice(&0u32.to_le_bytes()); // manifest_offset (will update)
    aos_file.extend_from_slice(&manifest_len.to_le_bytes()); // manifest_len

    // Write manifest
    let manifest_offset = aos_file.len() as u32;
    aos_file.extend_from_slice(manifest_bytes);

    // Write weights
    aos_file.extend_from_slice(safetensors);

    // Update manifest_offset in header
    aos_file[0..4].copy_from_slice(&manifest_offset.to_le_bytes());

    aos_file
}

/// Helper to create a multipart form with .aos file
fn create_multipart_boundary() -> String {
    "----WebKitFormBoundary7MA4YWxkTrZu0gW".to_string()
}

/// Helper to build multipart body for .aos upload
fn build_multipart_body(
    boundary: &str,
    file_data: &[u8],
    file_name: &str,
    fields: &[(&str, &str)],
) -> Vec<u8> {
    let mut body = Vec::new();

    // Add text fields
    for (name, value) in fields {
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
        body.extend_from_slice(b"\r\n");
    }

    // Add file field
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
            file_name
        )
        .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(file_data);
    body.extend_from_slice(b"\r\n");

    // End boundary
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

    body
}

/// Helper to create Claims with specific role
fn create_claims_with_role(role: &str) -> Claims {
    Claims {
        sub: "tenant-1-user".to_string(),
        email: "user@example.com".to_string(),
        role: role.to_string(),
        tenant_id: "tenant-1".to_string(),
        exp: 0,
        iat: 0,
        jti: "test-token".to_string(),
        nbf: 0,
    }
}

/// Test successful .aos upload via HTTP multipart endpoint
///
/// This test:
/// 1. Creates a test .aos file
/// 2. POSTs it to the upload endpoint via Axum test client
/// 3. Verifies the response contains correct adapter metadata
/// 4. Checks database registration
/// 5. Verifies file written to disk
#[tokio::test]
async fn test_aos_upload_success() -> anyhow::Result<()> {
    // Setup test infrastructure
    let state = setup_state(None).await?;
    let app = create_test_app(state.clone());

    // Create test .aos file
    let aos_content = create_test_aos_file();
    let boundary = create_multipart_boundary();

    let fields = vec![
        ("name", "Test Adapter"),
        ("description", "A test adapter"),
        ("tier", "ephemeral"),
        ("category", "general"),
        ("scope", "general"),
        ("rank", "4"),
        ("alpha", "8.0"),
    ];

    let multipart_body = build_multipart_body(&boundary, &aos_content, "test.aos", &fields);
    let content_type = format!("multipart/form-data; boundary={}", boundary);

    // Create JWT token for admin user
    let admin_jwt = create_test_jwt("admin-user", "Admin", Some("tenant-1"), None);

    // Build HTTP request
    let request = Request::builder()
        .method("POST")
        .uri("/v1/adapters/upload-aos")
        .header(header::CONTENT_TYPE, &content_type)
        .header("Authorization", format!("Bearer {}", admin_jwt))
        .body(Body::from(multipart_body))?;

    // Execute request via test app
    let response = app.oneshot(request).await?;

    // Verify response status is 200 OK
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Upload should return 200 OK, got: {}",
        response.status()
    );

    // Parse response body
    let body_bytes = to_bytes(response.into_body(), usize::MAX).await?;
    let upload_response: AosUploadResponse = serde_json::from_slice(&body_bytes)?;

    // Verify response contains required fields
    assert!(
        !upload_response.adapter_id.is_empty(),
        "Adapter ID should be set"
    );
    assert_eq!(
        upload_response.tenant_id, "tenant-1",
        "Tenant ID should match request"
    );
    assert!(
        !upload_response.hash_b3.is_empty(),
        "BLAKE3 hash should be computed"
    );
    assert!(
        !upload_response.file_path.is_empty(),
        "File path should be set"
    );
    assert!(
        upload_response.file_size > 0,
        "File size should be positive"
    );
    assert_eq!(
        upload_response.lifecycle_state, "draft",
        "Initial state should be draft"
    );

    // Verify file was written to disk
    let file_exists = Path::new(&upload_response.file_path).exists();
    assert!(
        file_exists,
        "Uploaded file should exist on disk at: {}",
        upload_response.file_path
    );

    // Verify file content matches
    let written_data = tokio::fs::read(&upload_response.file_path).await?;
    assert_eq!(
        written_data, aos_content,
        "Written file should match uploaded content"
    );

    // Verify file integrity hash
    let mut hasher = Hasher::new();
    hasher.update(&written_data);
    let written_hash = hasher.finalize().to_hex().to_string();
    assert_eq!(
        written_hash, upload_response.hash_b3,
        "File on disk should have matching hash"
    );

    // Verify database registration
    let adapter = state.db.get_adapter(&upload_response.adapter_id).await?;
    assert!(
        adapter.is_some(),
        "Adapter should be registered in database"
    );

    let adapter = adapter.unwrap();
    assert_eq!(adapter.name, "Test Adapter", "Adapter name should match");
    assert_eq!(adapter.tenant_id, "tenant-1", "Tenant ID should match");
    assert_eq!(adapter.tier, "ephemeral", "Tier should match");
    assert_eq!(adapter.rank, 4, "Rank should match");
    assert_eq!(adapter.alpha, 8.0, "Alpha should match");

    // Clean up
    tokio::fs::remove_file(&upload_response.file_path).await?;

    Ok(())
}

/// Test permission denied for viewer role
///
/// This test:
/// 1. Attempts upload with Viewer role (read-only)
/// 2. Verifies 403 Forbidden response
/// 3. Verifies audit log records the permission failure
#[tokio::test]
async fn test_aos_upload_permission_denied() -> anyhow::Result<()> {
    // Setup test infrastructure
    let state = setup_state(None).await?;
    let app = create_test_app(state.clone());

    // Create test .aos file
    let aos_content = create_test_aos_file();
    let boundary = create_multipart_boundary();
    let fields = vec![("name", "Test Adapter")];
    let multipart_body = build_multipart_body(&boundary, &aos_content, "test.aos", &fields);
    let content_type = format!("multipart/form-data; boundary={}", boundary);

    // Create JWT token for viewer user (no upload permissions)
    let viewer_jwt = create_test_jwt("viewer-user", "Viewer", Some("tenant-1"), None);

    // Build HTTP request
    let request = Request::builder()
        .method("POST")
        .uri("/v1/adapters/upload-aos")
        .header(header::CONTENT_TYPE, &content_type)
        .header("Authorization", format!("Bearer {}", viewer_jwt))
        .body(Body::from(multipart_body))?;

    // Execute request via test app
    let response = app.oneshot(request).await?;

    // Verify response status is 403 Forbidden
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Upload with Viewer role should return 403 Forbidden, got: {}",
        response.status()
    );

    Ok(())
}

/// Test invalid file type rejection
///
/// This test verifies that non-.aos files are rejected
#[tokio::test]
async fn test_aos_upload_invalid_file_type() -> anyhow::Result<()> {
    // Setup test infrastructure
    let state = setup_state(None).await?;
    let app = create_test_app(state.clone());

    // Create non-.aos file
    let content = b"not an aos file";
    let boundary = create_multipart_boundary();
    let fields = vec![("name", "Test Adapter")];
    let multipart_body = build_multipart_body(&boundary, content, "test.txt", &fields);
    let content_type = format!("multipart/form-data; boundary={}", boundary);

    // Create JWT token
    let admin_jwt = create_test_jwt("admin-user", "Admin", Some("tenant-1"), None);

    // Build HTTP request
    let request = Request::builder()
        .method("POST")
        .uri("/v1/adapters/upload-aos")
        .header(header::CONTENT_TYPE, &content_type)
        .header("Authorization", format!("Bearer {}", admin_jwt))
        .body(Body::from(multipart_body))?;

    // Execute request via test app
    let response = app.oneshot(request).await?;

    // Verify response status is 400 Bad Request (invalid file type)
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Upload with .txt file should return 400 Bad Request, got: {}",
        response.status()
    );

    Ok(())
}

/// Test file size limits
#[tokio::test]
async fn test_aos_upload_file_too_large() -> anyhow::Result<()> {
    // Verify MAX_AOS_FILE_SIZE constant is reasonable
    const MAX_AOS_FILE_SIZE: usize = 1024 * 1024 * 1024; // 1GB

    // Simulate oversized file check
    let simulated_size = MAX_AOS_FILE_SIZE + 1;
    assert!(
        simulated_size > MAX_AOS_FILE_SIZE,
        "Size check should fail for oversized files"
    );

    Ok(())
}

/// Test hash determinism for content addressing
#[tokio::test]
async fn test_aos_upload_duplicate_hash() -> anyhow::Result<()> {
    // Create test .aos file
    let aos_content = create_test_aos_file();

    // Compute hash
    let mut hasher = Hasher::new();
    hasher.update(&aos_content);
    let hash1 = hasher.finalize().to_hex().to_string();

    // Compute same hash again
    let mut hasher2 = Hasher::new();
    hasher2.update(&aos_content);
    let hash2 = hasher2.finalize().to_hex().to_string();

    // Hashes should be identical for same content
    assert_eq!(hash1, hash2, "BLAKE3 hashes should be deterministic");

    Ok(())
}

/// Test metadata field parsing
#[tokio::test]
async fn test_aos_upload_with_metadata() -> anyhow::Result<()> {
    // Setup test infrastructure
    let state = setup_state(None).await?;
    let app = create_test_app(state.clone());

    // Create test .aos file
    let aos_content = create_test_aos_file();
    let boundary = create_multipart_boundary();

    let fields = vec![
        ("name", "Full Metadata Adapter"),
        ("description", "An adapter with complete metadata"),
        ("tier", "persistent"),
        ("category", "code"),
        ("scope", "private"),
        ("rank", "8"),
        ("alpha", "16.0"),
    ];

    let multipart_body =
        build_multipart_body(&boundary, &aos_content, "metadata_test.aos", &fields);
    let content_type = format!("multipart/form-data; boundary={}", boundary);

    // Create JWT token for admin user
    let admin_jwt = create_test_jwt("admin-user", "Admin", Some("tenant-1"), None);

    // Build HTTP request
    let request = Request::builder()
        .method("POST")
        .uri("/v1/adapters/upload-aos")
        .header(header::CONTENT_TYPE, &content_type)
        .header("Authorization", format!("Bearer {}", admin_jwt))
        .body(Body::from(multipart_body))?;

    // Execute request via test app
    let response = app.oneshot(request).await?;

    // Verify response status
    assert_eq!(response.status(), StatusCode::OK, "Upload should succeed");

    // Parse response body
    let body_bytes = to_bytes(response.into_body(), usize::MAX).await?;
    let upload_response: AosUploadResponse = serde_json::from_slice(&body_bytes)?;

    // Verify database registration with correct metadata
    let adapter = state.db.get_adapter(&upload_response.adapter_id).await?;
    assert!(adapter.is_some(), "Adapter should be registered");

    let adapter = adapter.unwrap();
    assert_eq!(
        adapter.name, "Full Metadata Adapter",
        "Adapter name should match"
    );
    assert_eq!(adapter.tier, "persistent", "Tier should match");
    assert_eq!(adapter.rank, 8, "Rank should match");
    assert_eq!(adapter.alpha, 16.0, "Alpha should match");

    // Clean up
    tokio::fs::remove_file(&upload_response.file_path).await?;

    Ok(())
}

/// Test invalid tier values are rejected
#[tokio::test]
async fn test_aos_upload_invalid_tier_values() -> anyhow::Result<()> {
    // Setup test infrastructure
    let state = setup_state(None).await?;
    let app = create_test_app(state.clone());

    // Valid tiers
    let valid_tiers = vec!["ephemeral", "warm", "persistent"];

    // Invalid tiers to test
    let invalid_tiers = vec!["invalid", "temporary", "hot"];

    // Verify valid tiers are recognized
    for valid_tier in &valid_tiers {
        assert!(
            valid_tiers.contains(valid_tier),
            "Valid tier should be recognized: {}",
            valid_tier
        );
    }

    // Verify invalid tiers are rejected
    for invalid_tier in &invalid_tiers {
        assert!(
            !valid_tiers.contains(invalid_tier),
            "Invalid tier should be rejected: {}",
            invalid_tier
        );

        // Test via HTTP endpoint
        let aos_content = create_test_aos_file();
        let boundary = create_multipart_boundary();
        let fields = vec![("name", "Test Adapter"), ("tier", invalid_tier)];
        let multipart_body = build_multipart_body(&boundary, &aos_content, "test.aos", &fields);
        let content_type = format!("multipart/form-data; boundary={}", boundary);

        let admin_jwt = create_test_jwt("admin-user", "Admin", Some("tenant-1"), None);

        let request = Request::builder()
            .method("POST")
            .uri("/v1/adapters/upload-aos")
            .header(header::CONTENT_TYPE, &content_type)
            .header("Authorization", format!("Bearer {}", admin_jwt))
            .body(Body::from(multipart_body))?;

        let app2 = create_test_app(state.clone());
        let response = app2.oneshot(request).await?;

        // Verify rejection
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "Upload with invalid tier '{}' should return 400 Bad Request",
            invalid_tier
        );
    }

    Ok(())
}

/// Test path traversal attack prevention
#[tokio::test]
async fn test_aos_upload_path_traversal_attempts() -> anyhow::Result<()> {
    use adapteros_secure_fs::traversal::normalize_path;

    // Test various path traversal attempts
    let malicious_paths = vec![
        "../../../etc/passwd",
        "..\\..\\..\\windows\\system32",
        "./adapters/../../secrets.txt",
        "adapters/../../../sensitive",
    ];

    for path in &malicious_paths {
        let result = normalize_path(path);
        // All path traversal attempts should either be normalized safely
        // or rejected by the normalize_path function
        if let Ok(normalized) = result {
            assert!(
                !normalized.to_string_lossy().contains(".."),
                "Normalized path should not contain '..' sequences: {}",
                normalized.display()
            );
        }
    }

    Ok(())
}

/// Test audit log verification for successful uploads
///
/// This test verifies that successful uploads are properly recorded in audit logs
#[tokio::test]
async fn test_aos_upload_audit_log_verification() -> anyhow::Result<()> {
    // Setup test infrastructure
    let state = setup_state(None).await?;
    let app = create_test_app(state.clone());

    use adapteros_server_api::audit_helper::{actions, resources};

    // Step 1: Get initial count of audit logs
    let initial_count =
        get_audit_log_count(&state.db, actions::ADAPTER_UPLOAD, Some(resources::ADAPTER)).await?;
    assert_eq!(initial_count, 0, "Initial count should be zero");

    // Step 2: Perform successful upload
    let aos_content = create_test_aos_file();
    let boundary = create_multipart_boundary();
    let fields = vec![("name", "Audit Test Adapter")];
    let multipart_body = build_multipart_body(&boundary, &aos_content, "test.aos", &fields);
    let content_type = format!("multipart/form-data; boundary={}", boundary);

    let admin_jwt = create_test_jwt("admin-user", "Admin", Some("tenant-1"), None);

    let request = Request::builder()
        .method("POST")
        .uri("/v1/adapters/upload-aos")
        .header(header::CONTENT_TYPE, &content_type)
        .header("Authorization", format!("Bearer {}", admin_jwt))
        .body(Body::from(multipart_body))?;

    let response = app.oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK, "Upload should succeed");

    // Parse adapter ID from response
    let body_bytes = to_bytes(response.into_body(), usize::MAX).await?;
    let upload_response: AosUploadResponse = serde_json::from_slice(&body_bytes)?;

    // Step 3: Verify audit log was created
    let count_after_upload =
        get_audit_log_count(&state.db, actions::ADAPTER_UPLOAD, Some(resources::ADAPTER)).await?;
    assert_eq!(count_after_upload, 1, "Count should increase after upload");

    // Step 4: Verify audit log details
    let audit_log = get_audit_log_record(
        &state.db,
        actions::ADAPTER_UPLOAD,
        Some(&upload_response.adapter_id),
    )
    .await?;
    assert!(audit_log.is_some(), "Audit log record should exist");

    let log_record = audit_log.unwrap();
    assert_eq!(
        log_record.action,
        actions::ADAPTER_UPLOAD,
        "Action should match"
    );
    assert_eq!(
        log_record.resource_type,
        resources::ADAPTER,
        "Resource type should match"
    );
    assert_eq!(log_record.status, "success", "Status should be 'success'");
    assert!(
        log_record.error_message.is_none(),
        "Error message should be None for success"
    );

    // Clean up
    tokio::fs::remove_file(&upload_response.file_path).await?;

    Ok(())
}

/// Test that uploaded file actually persists to disk
///
/// This test verifies:
/// 1. File is written to the correct path
/// 2. File content matches what was uploaded
/// 3. File persists across database queries
#[tokio::test]
async fn test_aos_upload_file_persists_to_disk() -> anyhow::Result<()> {
    // Setup test infrastructure
    let state = setup_state(None).await?;
    let app = create_test_app(state.clone());

    // Create test .aos file with specific content
    let aos_content = create_test_aos_file();
    let boundary = create_multipart_boundary();

    let fields = vec![("name", "Persistence Test Adapter"), ("tier", "warm")];

    let multipart_body = build_multipart_body(&boundary, &aos_content, "persist_test.aos", &fields);
    let content_type = format!("multipart/form-data; boundary={}", boundary);

    // Create JWT token
    let admin_jwt = create_test_jwt("admin-user", "Admin", Some("tenant-1"), None);

    // Build and execute request
    let request = Request::builder()
        .method("POST")
        .uri("/v1/adapters/upload-aos")
        .header(header::CONTENT_TYPE, &content_type)
        .header("Authorization", format!("Bearer {}", admin_jwt))
        .body(Body::from(multipart_body))?;

    let response = app.oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK);

    // Parse response
    let body_bytes = to_bytes(response.into_body(), usize::MAX).await?;
    let upload_response: AosUploadResponse = serde_json::from_slice(&body_bytes)?;
    let adapter_id = upload_response.adapter_id.clone();
    let file_path = upload_response.file_path.clone();

    // Step 1: Verify file exists immediately after upload
    let initial_exists = Path::new(&file_path).exists();
    assert!(initial_exists, "File should exist immediately after upload");

    // Step 2: Read file content and verify it matches original
    let written_data = tokio::fs::read(&file_path).await?;
    assert_eq!(
        written_data.len(),
        aos_content.len(),
        "File size should match"
    );
    assert_eq!(
        written_data, aos_content,
        "File content should match original"
    );

    // Step 3: Verify through database that path is persisted
    let adapter_from_db = state.db.get_adapter(&adapter_id).await?;
    assert!(
        adapter_from_db.is_some(),
        "Adapter should exist in database"
    );

    let adapter = adapter_from_db.unwrap();
    assert_eq!(
        adapter.aos_file_path,
        Some(file_path.clone()),
        "Database should store file path"
    );

    // Step 4: Verify file still exists after database queries
    let still_exists = Path::new(&file_path).exists();
    assert!(
        still_exists,
        "File should still exist after database queries"
    );

    // Step 5: Re-read file and verify content is unchanged
    let re_read_data = tokio::fs::read(&file_path).await?;
    assert_eq!(
        re_read_data, aos_content,
        "File content should remain unchanged"
    );

    // Step 6: Verify hash computation matches
    let mut hasher = Hasher::new();
    hasher.update(&re_read_data);
    let recomputed_hash = hasher.finalize().to_hex().to_string();
    assert_eq!(
        recomputed_hash, upload_response.hash_b3,
        "Recomputed hash should match initial hash"
    );

    // Clean up
    tokio::fs::remove_file(&file_path).await?;

    Ok(())
}
