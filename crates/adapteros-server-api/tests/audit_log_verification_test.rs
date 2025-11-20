/// Audit Log Verification Tests for PRD-2
///
/// This module provides comprehensive testing of audit log functionality:
/// - Successful and failure action logging
/// - Audit log retrieval and filtering
/// - Field validation for audit records
/// - Count verification before and after operations
use adapteros_db::{audit::AuditLog, Db};
use adapteros_server_api::audit_helper::{actions, log_failure, log_success, resources};
use adapteros_server_api::auth::Claims;
use chrono::{Duration, Utc};

/// Create test claims
fn create_test_claims(role: &str) -> Claims {
    Claims {
        sub: "test-user-123".to_string(),
        email: "test@example.com".to_string(),
        role: role.to_string(),
        tenant_id: "test-tenant".to_string(),
        exp: (Utc::now() + Duration::hours(8)).timestamp(),
        iat: Utc::now().timestamp(),
        jti: "test-jti".to_string(),
        nbf: Utc::now().timestamp(),
    }
}

/// Helper to verify audit log exists
async fn verify_audit_log_exists(
    db: &Db,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    expected_status: &str,
) -> anyhow::Result<bool> {
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

/// Helper to get audit log count
async fn get_audit_log_count(
    db: &Db,
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

/// Helper to get full audit log record
async fn get_audit_log_record(
    db: &Db,
    action: &str,
    resource_id: Option<&str>,
) -> anyhow::Result<Option<AuditLog>> {
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

/// Test successful audit logging
#[tokio::test]
async fn test_audit_log_success() -> anyhow::Result<()> {
    // Setup test database with audit_logs table
    let db = Db::connect(":memory:").await?;

    // Create audit_logs table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS audit_logs (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            user_id TEXT NOT NULL,
            user_role TEXT NOT NULL,
            tenant_id TEXT NOT NULL,
            action TEXT NOT NULL,
            resource_type TEXT NOT NULL,
            resource_id TEXT,
            status TEXT NOT NULL,
            error_message TEXT,
            ip_address TEXT,
            metadata_json TEXT
        )",
    )
    .execute(db.pool())
    .await?;

    let claims = create_test_claims("Admin");

    // Step 1: Log a successful action
    let result = log_success(
        &db,
        &claims,
        actions::ADAPTER_UPLOAD,
        resources::ADAPTER,
        Some("test-adapter-1"),
    )
    .await;
    assert!(result.is_ok(), "Success logging should not fail");

    // Step 2: Verify log was created
    let log_exists = verify_audit_log_exists(
        &db,
        actions::ADAPTER_UPLOAD,
        resources::ADAPTER,
        Some("test-adapter-1"),
        "success",
    )
    .await?;
    assert!(log_exists, "Audit log should exist for successful action");

    // Step 3: Get detailed record and verify fields
    let log_record =
        get_audit_log_record(&db, actions::ADAPTER_UPLOAD, Some("test-adapter-1")).await?;
    assert!(log_record.is_some(), "Audit log record should exist");

    let log = log_record.unwrap();
    assert_eq!(log.action, actions::ADAPTER_UPLOAD, "Action should match");
    assert_eq!(
        log.resource_type,
        resources::ADAPTER,
        "Resource type should match"
    );
    assert_eq!(
        log.resource_id,
        Some("test-adapter-1".to_string()),
        "Resource ID should match"
    );
    assert_eq!(log.status, "success", "Status should be success");
    assert_eq!(log.user_id, claims.sub, "User ID should match");
    assert_eq!(log.user_role, claims.role, "User role should match");
    assert_eq!(log.tenant_id, claims.tenant_id, "Tenant ID should match");
    assert!(
        log.error_message.is_none(),
        "Error message should be None for success"
    );
    assert!(!log.id.is_empty(), "ID should be set");
    assert!(!log.timestamp.is_empty(), "Timestamp should be set");

    Ok(())
}

/// Test failure audit logging
#[tokio::test]
async fn test_audit_log_failure() -> anyhow::Result<()> {
    // Setup test database with audit_logs table
    let db = Db::connect(":memory:").await?;

    // Create audit_logs table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS audit_logs (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            user_id TEXT NOT NULL,
            user_role TEXT NOT NULL,
            tenant_id TEXT NOT NULL,
            action TEXT NOT NULL,
            resource_type TEXT NOT NULL,
            resource_id TEXT,
            status TEXT NOT NULL,
            error_message TEXT,
            ip_address TEXT,
            metadata_json TEXT
        )",
    )
    .execute(db.pool())
    .await?;

    let claims = create_test_claims("Viewer");

    // Step 1: Log a failed action
    let error_msg = "Permission denied: Viewer cannot upload adapters";
    let result = log_failure(
        &db,
        &claims,
        actions::ADAPTER_UPLOAD,
        resources::ADAPTER,
        Some("test-adapter-2"),
        error_msg,
    )
    .await;
    assert!(result.is_ok(), "Failure logging should not fail");

    // Step 2: Verify log was created
    let log_exists = verify_audit_log_exists(
        &db,
        actions::ADAPTER_UPLOAD,
        resources::ADAPTER,
        Some("test-adapter-2"),
        "failure",
    )
    .await?;
    assert!(log_exists, "Audit log should exist for failed action");

    // Step 3: Get detailed record and verify fields
    let log_record =
        get_audit_log_record(&db, actions::ADAPTER_UPLOAD, Some("test-adapter-2")).await?;
    assert!(log_record.is_some(), "Audit log record should exist");

    let log = log_record.unwrap();
    assert_eq!(log.action, actions::ADAPTER_UPLOAD, "Action should match");
    assert_eq!(log.status, "failure", "Status should be failure");
    assert_eq!(
        log.error_message,
        Some(error_msg.to_string()),
        "Error message should match"
    );
    assert_eq!(log.user_id, claims.sub, "User ID should match");
    assert_eq!(
        log.user_role, claims.role,
        "User role should match (Viewer)"
    );

    Ok(())
}

/// Test audit log count increases
#[tokio::test]
async fn test_audit_log_count_increases() -> anyhow::Result<()> {
    // Setup test database with audit_logs table
    let db = Db::connect(":memory:").await?;

    // Create audit_logs table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS audit_logs (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            user_id TEXT NOT NULL,
            user_role TEXT NOT NULL,
            tenant_id TEXT NOT NULL,
            action TEXT NOT NULL,
            resource_type TEXT NOT NULL,
            resource_id TEXT,
            status TEXT NOT NULL,
            error_message TEXT,
            ip_address TEXT,
            metadata_json TEXT
        )",
    )
    .execute(db.pool())
    .await?;

    let claims = create_test_claims("Admin");

    // Step 1: Get initial count
    let initial_count =
        get_audit_log_count(&db, actions::ADAPTER_UPLOAD, Some(resources::ADAPTER)).await?;
    assert_eq!(initial_count, 0, "Initial count should be zero");

    // Step 2: Log first successful action
    log_success(
        &db,
        &claims,
        actions::ADAPTER_UPLOAD,
        resources::ADAPTER,
        Some("adapter-1"),
    )
    .await?;

    // Step 3: Verify count increased to 1
    let count_after_first =
        get_audit_log_count(&db, actions::ADAPTER_UPLOAD, Some(resources::ADAPTER)).await?;
    assert_eq!(count_after_first, 1, "Count should increase to 1");

    // Step 4: Log second successful action
    log_success(
        &db,
        &claims,
        actions::ADAPTER_UPLOAD,
        resources::ADAPTER,
        Some("adapter-2"),
    )
    .await?;

    // Step 5: Verify count increased to 2
    let count_after_second =
        get_audit_log_count(&db, actions::ADAPTER_UPLOAD, Some(resources::ADAPTER)).await?;
    assert_eq!(count_after_second, 2, "Count should increase to 2");

    // Step 6: Log a different action
    log_success(
        &db,
        &claims,
        actions::ADAPTER_DELETE,
        resources::ADAPTER,
        Some("adapter-3"),
    )
    .await?;

    // Step 7: Verify count for UPLOAD hasn't changed
    let count_upload_only =
        get_audit_log_count(&db, actions::ADAPTER_UPLOAD, Some(resources::ADAPTER)).await?;
    assert_eq!(count_upload_only, 2, "Upload count should still be 2");

    // Step 8: Verify count for DELETE is 1
    let count_delete =
        get_audit_log_count(&db, actions::ADAPTER_DELETE, Some(resources::ADAPTER)).await?;
    assert_eq!(count_delete, 1, "Delete count should be 1");

    Ok(())
}

/// Test audit log filtering by status
#[tokio::test]
async fn test_audit_log_filter_by_status() -> anyhow::Result<()> {
    // Setup test database with audit_logs table
    let db = Db::connect(":memory:").await?;

    // Create audit_logs table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS audit_logs (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            user_id TEXT NOT NULL,
            user_role TEXT NOT NULL,
            tenant_id TEXT NOT NULL,
            action TEXT NOT NULL,
            resource_type TEXT NOT NULL,
            resource_id TEXT,
            status TEXT NOT NULL,
            error_message TEXT,
            ip_address TEXT,
            metadata_json TEXT
        )",
    )
    .execute(db.pool())
    .await?;

    let admin_claims = create_test_claims("Admin");
    let viewer_claims = create_test_claims("Viewer");

    // Log one success
    log_success(
        &db,
        &admin_claims,
        actions::ADAPTER_UPLOAD,
        resources::ADAPTER,
        Some("success-1"),
    )
    .await?;

    // Log one failure
    log_failure(
        &db,
        &viewer_claims,
        actions::ADAPTER_UPLOAD,
        resources::ADAPTER,
        Some("failure-1"),
        "Permission denied",
    )
    .await?;

    // Log another success
    log_success(
        &db,
        &admin_claims,
        actions::ADAPTER_UPLOAD,
        resources::ADAPTER,
        Some("success-2"),
    )
    .await?;

    // Step 1: Verify we can find all logs
    let total_exists =
        verify_audit_log_exists(&db, actions::ADAPTER_UPLOAD, resources::ADAPTER, None, "").await?;
    assert!(
        total_exists,
        "Should find logs when filtering by action alone"
    );

    // Step 2: Verify we can find only success logs
    let success_exists = verify_audit_log_exists(
        &db,
        actions::ADAPTER_UPLOAD,
        resources::ADAPTER,
        None,
        "success",
    )
    .await?;
    assert!(success_exists, "Should find success logs");

    // Step 3: Verify we can find only failure logs
    let failure_exists = verify_audit_log_exists(
        &db,
        actions::ADAPTER_UPLOAD,
        resources::ADAPTER,
        None,
        "failure",
    )
    .await?;
    assert!(failure_exists, "Should find failure logs");

    // Step 4: Verify counts are correct
    let success_count =
        get_audit_log_count(&db, actions::ADAPTER_UPLOAD, Some(resources::ADAPTER)).await?;
    assert_eq!(success_count, 3, "Total count should be 3");

    Ok(())
}

/// Test audit log field validation
#[tokio::test]
async fn test_audit_log_field_validation() -> anyhow::Result<()> {
    // Setup test database with audit_logs table
    let db = Db::connect(":memory:").await?;

    // Create audit_logs table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS audit_logs (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            user_id TEXT NOT NULL,
            user_role TEXT NOT NULL,
            tenant_id TEXT NOT NULL,
            action TEXT NOT NULL,
            resource_type TEXT NOT NULL,
            resource_id TEXT,
            status TEXT NOT NULL,
            error_message TEXT,
            ip_address TEXT,
            metadata_json TEXT
        )",
    )
    .execute(db.pool())
    .await?;

    let claims = create_test_claims("SRE");

    // Log an action with specific values
    log_success(
        &db,
        &claims,
        actions::TRAINING_START,
        resources::TRAINING_JOB,
        Some("job-abc123"),
    )
    .await?;

    // Retrieve and validate every field
    let log = get_audit_log_record(&db, actions::TRAINING_START, Some("job-abc123"))
        .await?
        .expect("Should find log");

    // Required fields validation
    assert!(!log.id.is_empty(), "ID must not be empty");
    assert!(!log.timestamp.is_empty(), "Timestamp must not be empty");
    assert!(!log.user_id.is_empty(), "User ID must not be empty");
    assert!(!log.user_role.is_empty(), "User role must not be empty");
    assert!(!log.tenant_id.is_empty(), "Tenant ID must not be empty");
    assert!(!log.action.is_empty(), "Action must not be empty");
    assert!(
        !log.resource_type.is_empty(),
        "Resource type must not be empty"
    );
    assert!(!log.status.is_empty(), "Status must not be empty");

    // Field value validation
    assert_eq!(log.user_id, "test-user-123", "User ID should match claims");
    assert_eq!(log.user_role, "SRE", "Role should be SRE");
    assert_eq!(
        log.tenant_id, "test-tenant",
        "Tenant ID should match claims"
    );
    assert_eq!(log.action, actions::TRAINING_START, "Action should match");
    assert_eq!(
        log.resource_type,
        resources::TRAINING_JOB,
        "Resource type should match"
    );
    assert_eq!(
        log.resource_id,
        Some("job-abc123".to_string()),
        "Resource ID should match"
    );
    assert_eq!(log.status, "success", "Status should be success");

    // Optional fields for success should be None
    assert!(
        log.error_message.is_none(),
        "Success logs should have no error message"
    );
    assert!(
        log.ip_address.is_none(),
        "IP address not required in this test"
    );
    assert!(
        log.metadata_json.is_none(),
        "Metadata not required in this test"
    );

    Ok(())
}
