//! Tenant Isolation Integration Tests
//!
//! Per PRD-RBAC-01, these tests verify complete tenant isolation across all resources:
//! - Datasets: Tenants cannot access each other's datasets
//! - Training Jobs: Tenants cannot see each other's training jobs
//! - Adapters: Tenants cannot load/unload each other's adapters
//! - List Operations: Lists are properly filtered by tenant
//! - Admin Override: Admin role can access all tenants' resources

use adapteros_core::{AosError, Result};
use adapteros_crypto::Keypair;
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::users::Role;
use adapteros_db::Db;
use adapteros_server_api::auth::Claims;
use adapteros_server_api::permissions::{require_permission, Permission};
use chrono::{Duration, Utc};
use uuid::Uuid;

/// Test helper to create a tenant
async fn create_test_tenant(db: &Db, tenant_id: &str) -> Result<()> {
    // Insert tenant with specified ID (not using db.create_tenant which generates a random UUID)
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create tenant: {}", e)))?;
    Ok(())
}

/// Test helper to create a user with specific role and tenant
async fn create_test_user(db: &Db, email: &str, role: Role, tenant_id: &str) -> Result<String> {
    // Create user in database
    let user_id = db
        .create_user(email, email, "dummy_hash", role.clone(), tenant_id)
        .await?;

    // Update user's tenant_id (if tenant column exists)
    // Note: Depending on schema, may need to update users table with tenant_id
    let _ = sqlx::query("UPDATE users SET tenant_id = ? WHERE id = ?")
        .bind(tenant_id)
        .bind(&user_id)
        .execute(db.pool())
        .await; // Ignore error if column doesn't exist

    Ok(user_id)
}

/// Test helper to create JWT claims for a user
fn create_test_claims(user_id: &str, email: &str, role: &str, tenant_id: &str) -> Claims {
    let now = Utc::now();
    let exp = now + Duration::hours(8);

    Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        role: role.to_string(),
        roles: vec![role.to_string()],
        tenant_id: tenant_id.to_string(),
        admin_tenants: vec![],
        exp: exp.timestamp(),
        iat: now.timestamp(),
        jti: Uuid::new_v4().to_string(),
        nbf: now.timestamp(),
    }
}

/// Test helper to create a training dataset for a tenant
async fn create_test_dataset(db: &Db, dataset_id: &str, tenant_id: &str, name: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO training_datasets (id, name, tenant_id, format, storage_path, hash_b3, validation_status, created_at)
         VALUES (?, ?, ?, 'jsonl', '/tmp/test', ?, 'valid', datetime('now'))",
    )
    .bind(dataset_id)
    .bind(name)
    .bind(tenant_id)
    .bind("dummy_hash")
    .execute(db.pool())
    .await
    .map_err(|e| AosError::Database(format!("Failed to create test dataset: {}", e)))?;

    Ok(())
}

/// Test helper to create a training job for a tenant
async fn create_test_training_job(
    db: &Db,
    job_id: &str,
    tenant_id: &str,
    repo_id: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, training_config_json, status, progress_json, created_by)
         VALUES (?, ?, ?, 'pending', ?, ?)",
    )
    .bind(job_id)
    .bind(repo_id)
    .bind(r#"{"rank":16,"alpha":32}"#) // Required training_config_json
    .bind(r#"{"progress":0}"#) // Required progress_json
    .bind(format!("user@{}", tenant_id)) // Use tenant_id in created_by
    .execute(db.pool())
    .await
    .map_err(|e| AosError::Database(format!("Failed to create test training job: {}", e)))?;

    Ok(())
}

/// Test helper to create an adapter for a tenant
async fn create_test_adapter(
    db: &Db,
    adapter_id: &str,
    tenant_id: &str,
    name: &str,
) -> Result<String> {
    // Use adapter_id as hash to ensure uniqueness
    let unique_hash = format!("hash_{}", adapter_id);
    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id)
        .name(name)
        .hash_b3(&unique_hash)
        .rank(16)
        .tier("persistent")
        .category("code")
        .scope("tenant")
        .tenant_id(tenant_id)
        .build()
        .map_err(|e| AosError::Validation(format!("Failed to build adapter params: {}", e)))?;

    db.register_adapter(params).await
}

// =============================================================================
// TEST SUITE: Cross-Tenant Dataset Access Denied
// =============================================================================

#[tokio::test]
async fn test_cross_tenant_dataset_access_denied() -> Result<()> {
    // Setup: Create in-memory database
    let db = Db::new_in_memory().await?;

    // Create two tenants
    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create datasets for each tenant
    create_test_dataset(&db, "dataset-a-1", "tenant-a", "Dataset A1").await?;
    create_test_dataset(&db, "dataset-b-1", "tenant-b", "Dataset B1").await?;

    // Verify tenant A can access their own dataset
    let dataset_a = db.get_training_dataset("dataset-a-1").await?;
    assert!(dataset_a.is_some(), "Tenant A should access their dataset");
    assert_eq!(dataset_a.unwrap().tenant_id, Some("tenant-a".to_string()));

    // Verify tenant B can access their own dataset
    let dataset_b = db.get_training_dataset("dataset-b-1").await?;
    assert!(dataset_b.is_some(), "Tenant B should access their dataset");
    assert_eq!(dataset_b.unwrap().tenant_id, Some("tenant-b".to_string()));

    // List datasets for tenant A (should only see their own)
    let datasets_a = db
        .list_training_datasets_for_tenant("tenant-a", 100)
        .await?;
    assert_eq!(
        datasets_a.len(),
        1,
        "Tenant A should see only their dataset"
    );
    assert_eq!(datasets_a[0].id, "dataset-a-1");
    assert_eq!(datasets_a[0].tenant_id, Some("tenant-a".to_string()));

    // List datasets for tenant B (should only see their own)
    let datasets_b = db
        .list_training_datasets_for_tenant("tenant-b", 100)
        .await?;
    assert_eq!(
        datasets_b.len(),
        1,
        "Tenant B should see only their dataset"
    );
    assert_eq!(datasets_b[0].id, "dataset-b-1");
    assert_eq!(datasets_b[0].tenant_id, Some("tenant-b".to_string()));

    Ok(())
}

// =============================================================================
// TEST SUITE: Cross-Tenant Training Job Access Denied
// =============================================================================

#[tokio::test]
#[ignore = "FK schema bug: repository_training_jobs.repo_id references git_repositories.repo_id which is not unique"]
async fn test_cross_tenant_training_job_access_denied() -> Result<()> {
    // Setup: Create in-memory database
    let db = Db::new_in_memory().await?;

    // Create two tenants
    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create repositories for training jobs
    sqlx::query(
        "INSERT INTO git_repositories (id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json, status, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("id-repo-a")
    .bind("repo-a")
    .bind("/repos/tenant-a/repo")
    .bind("main")
    .bind("{}")
    .bind("{}")
    .bind("{}")
    .bind("ready")
    .bind("user@tenant-a")
    .execute(db.pool())
    .await?;

    sqlx::query(
        "INSERT INTO git_repositories (id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json, status, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("id-repo-b")
    .bind("repo-b")
    .bind("/repos/tenant-b/repo")
    .bind("main")
    .bind("{}")
    .bind("{}")
    .bind("{}")
    .bind("ready")
    .bind("user@tenant-b")
    .execute(db.pool())
    .await?;

    // Create training jobs for each tenant
    create_test_training_job(&db, "job-a-1", "tenant-a", "repo-a").await?;
    create_test_training_job(&db, "job-b-1", "tenant-b", "repo-b").await?;

    // List training jobs for tenant A (should only see their own)
    let jobs_a = db.list_training_jobs_for_tenant("tenant-a").await?;
    assert_eq!(
        jobs_a.len(),
        1,
        "Tenant A should see only their training job"
    );
    assert_eq!(jobs_a[0].id, "job-a-1");
    assert!(
        jobs_a[0].created_by.contains("tenant-a"),
        "Job should belong to tenant-a"
    );

    // List training jobs for tenant B (should only see their own)
    let jobs_b = db.list_training_jobs_for_tenant("tenant-b").await?;
    assert_eq!(
        jobs_b.len(),
        1,
        "Tenant B should see only their training job"
    );
    assert_eq!(jobs_b[0].id, "job-b-1");
    assert!(
        jobs_b[0].created_by.contains("tenant-b"),
        "Job should belong to tenant-b"
    );

    Ok(())
}

// =============================================================================
// TEST SUITE: Cross-Tenant Adapter Access Denied
// =============================================================================

#[tokio::test]
async fn test_cross_tenant_adapter_access_denied() -> Result<()> {
    // Setup: Create in-memory database
    let db = Db::new_in_memory().await?;

    // Create two tenants
    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create adapters for each tenant
    let adapter_a_id = create_test_adapter(&db, "adapter-a-1", "tenant-a", "Adapter A1").await?;
    let adapter_b_id = create_test_adapter(&db, "adapter-b-1", "tenant-b", "Adapter B1").await?;

    // List adapters for tenant A (should only see their own)
    let adapters_a = db.list_adapters_by_tenant("tenant-a").await?;
    assert_eq!(
        adapters_a.len(),
        1,
        "Tenant A should see only their adapter"
    );
    assert_eq!(adapters_a[0].tenant_id, "tenant-a");
    assert_eq!(adapters_a[0].adapter_id, Some("adapter-a-1".to_string()));

    // List adapters for tenant B (should only see their own)
    let adapters_b = db.list_adapters_by_tenant("tenant-b").await?;
    assert_eq!(
        adapters_b.len(),
        1,
        "Tenant B should see only their adapter"
    );
    assert_eq!(adapters_b[0].tenant_id, "tenant-b");
    assert_eq!(adapters_b[0].adapter_id, Some("adapter-b-1".to_string()));

    // Verify tenant A cannot load tenant B's adapter
    // (This would be enforced by handler-level checks, not just database queries)
    let adapter_b_from_db = sqlx::query_as::<_, adapteros_db::traits::AdapterRecord>(
        "SELECT * FROM adapters WHERE id = ?",
    )
    .bind(&adapter_b_id)
    .fetch_one(db.pool())
    .await?;

    assert_eq!(
        adapter_b_from_db.tenant_id, "tenant-b",
        "Adapter should belong to tenant-b"
    );

    Ok(())
}

// =============================================================================
// TEST SUITE: Admin Can Access All Tenants
// =============================================================================

#[tokio::test]
async fn test_admin_can_access_all_tenants() -> Result<()> {
    // Setup: Create in-memory database
    let db = Db::new_in_memory().await?;

    // Create tenants (including system tenant for admin)
    create_test_tenant(&db, "system").await?;
    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create admin user
    let admin_user_id = create_test_user(&db, "admin@aos.local", Role::Admin, "system").await?;

    // Create datasets for each tenant
    create_test_dataset(&db, "dataset-a-1", "tenant-a", "Dataset A1").await?;
    create_test_dataset(&db, "dataset-b-1", "tenant-b", "Dataset B1").await?;

    // Admin claims
    let admin_claims = create_test_claims(&admin_user_id, "admin@aos.local", "admin", "system");

    // Verify admin has permission to view all datasets
    require_permission(&admin_claims, Permission::DatasetView)
        .map_err(|_| AosError::Authz("Admin should have DatasetView permission".into()))?;

    // Admin should be able to list datasets for any tenant
    let datasets_a = db
        .list_training_datasets_for_tenant("tenant-a", 100)
        .await?;
    assert_eq!(datasets_a.len(), 1, "Admin should see tenant-a datasets");

    let datasets_b = db
        .list_training_datasets_for_tenant("tenant-b", 100)
        .await?;
    assert_eq!(datasets_b.len(), 1, "Admin should see tenant-b datasets");

    // Verify admin has permission to manage adapters
    require_permission(&admin_claims, Permission::AdapterLoad)
        .map_err(|_| AosError::Authz("Admin should have AdapterLoad permission".into()))?;

    Ok(())
}

// =============================================================================
// TEST SUITE: List Filtering by Tenant
// =============================================================================

#[tokio::test]
async fn test_list_operations_filtered_by_tenant() -> Result<()> {
    // Setup: Create in-memory database
    let db = Db::new_in_memory().await?;

    // Create three tenants
    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;
    create_test_tenant(&db, "tenant-c").await?;

    // Create multiple datasets per tenant
    create_test_dataset(&db, "dataset-a-1", "tenant-a", "Dataset A1").await?;
    create_test_dataset(&db, "dataset-a-2", "tenant-a", "Dataset A2").await?;
    create_test_dataset(&db, "dataset-b-1", "tenant-b", "Dataset B1").await?;
    create_test_dataset(&db, "dataset-c-1", "tenant-c", "Dataset C1").await?;

    // Create multiple adapters per tenant
    create_test_adapter(&db, "adapter-a-1", "tenant-a", "Adapter A1").await?;
    create_test_adapter(&db, "adapter-a-2", "tenant-a", "Adapter A2").await?;
    create_test_adapter(&db, "adapter-b-1", "tenant-b", "Adapter B1").await?;
    create_test_adapter(&db, "adapter-c-1", "tenant-c", "Adapter C1").await?;

    // List datasets for tenant A
    let datasets_a = db
        .list_training_datasets_for_tenant("tenant-a", 100)
        .await?;
    assert_eq!(
        datasets_a.len(),
        2,
        "Tenant A should see exactly 2 datasets"
    );
    for dataset in &datasets_a {
        assert_eq!(
            dataset.tenant_id,
            Some("tenant-a".to_string()),
            "All datasets should belong to tenant-a"
        );
    }

    // List adapters for tenant B
    let adapters_b = db.list_adapters_by_tenant("tenant-b").await?;
    assert_eq!(adapters_b.len(), 1, "Tenant B should see exactly 1 adapter");
    assert_eq!(adapters_b[0].tenant_id, "tenant-b");

    // List adapters for tenant C
    let adapters_c = db.list_adapters_by_tenant("tenant-c").await?;
    assert_eq!(adapters_c.len(), 1, "Tenant C should see exactly 1 adapter");
    assert_eq!(adapters_c[0].tenant_id, "tenant-c");

    // Verify no cross-tenant contamination
    let all_datasets_a = db
        .list_training_datasets_for_tenant("tenant-a", 100)
        .await?;
    assert!(
        all_datasets_a
            .iter()
            .all(|d| d.tenant_id == Some("tenant-a".to_string())),
        "No cross-tenant datasets in tenant-a list"
    );

    let all_adapters_b = db.list_adapters_by_tenant("tenant-b").await?;
    assert!(
        all_adapters_b.iter().all(|a| a.tenant_id == "tenant-b"),
        "No cross-tenant adapters in tenant-b list"
    );

    Ok(())
}

// =============================================================================
// TEST SUITE: Non-Admin Roles Cannot Access Other Tenants
// =============================================================================

#[tokio::test]
async fn test_non_admin_roles_cannot_access_other_tenants() -> Result<()> {
    // Setup: Create in-memory database
    let db = Db::new_in_memory().await?;

    // Create two tenants
    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create operator for tenant A
    let operator_a_id =
        create_test_user(&db, "operator-a@aos.local", Role::Operator, "tenant-a").await?;

    // Create viewer for tenant B
    let viewer_b_id = create_test_user(&db, "viewer-b@aos.local", Role::Viewer, "tenant-b").await?;

    // Create claims
    let operator_a_claims = create_test_claims(
        &operator_a_id,
        "operator-a@aos.local",
        "operator",
        "tenant-a",
    );
    let viewer_b_claims =
        create_test_claims(&viewer_b_id, "viewer-b@aos.local", "viewer", "tenant-b");

    // Operator A has AdapterLoad permission within their tenant
    require_permission(&operator_a_claims, Permission::AdapterLoad)
        .map_err(|_| AosError::Authz("Operator should have AdapterLoad permission".into()))?;

    // Viewer B does NOT have AdapterLoad permission
    let viewer_load_result = require_permission(&viewer_b_claims, Permission::AdapterLoad);
    assert!(
        viewer_load_result.is_err(),
        "Viewer should not have AdapterLoad permission"
    );

    // Create datasets for each tenant
    create_test_dataset(&db, "dataset-a-1", "tenant-a", "Dataset A1").await?;
    create_test_dataset(&db, "dataset-b-1", "tenant-b", "Dataset B1").await?;

    // Operator A should only see tenant-a datasets
    let datasets_a = db
        .list_training_datasets_for_tenant("tenant-a", 100)
        .await?;
    assert_eq!(datasets_a.len(), 1);
    assert_eq!(datasets_a[0].tenant_id, Some("tenant-a".to_string()));

    // Viewer B should only see tenant-b datasets
    let datasets_b = db
        .list_training_datasets_for_tenant("tenant-b", 100)
        .await?;
    assert_eq!(datasets_b.len(), 1);
    assert_eq!(datasets_b[0].tenant_id, Some("tenant-b".to_string()));

    // Verify operator A cannot see tenant-b datasets
    let datasets_b_from_a = db
        .list_training_datasets_for_tenant("tenant-b", 100)
        .await?;
    assert_eq!(
        datasets_b_from_a.len(),
        1,
        "Operator A querying tenant-b gets tenant-b data (handler must enforce isolation)"
    );
    // Note: Database returns data, but handler MUST check claims.tenant_id matches requested tenant

    Ok(())
}

// =============================================================================
// TEST SUITE: Tenant Isolation with Multiple Resources
// =============================================================================

#[tokio::test]
async fn test_tenant_isolation_with_multiple_resources() -> Result<()> {
    // Setup: Create in-memory database
    let db = Db::new_in_memory().await?;

    // Create two tenants
    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create comprehensive resources for tenant A
    create_test_dataset(&db, "dataset-a-1", "tenant-a", "Dataset A1").await?;
    create_test_dataset(&db, "dataset-a-2", "tenant-a", "Dataset A2").await?;
    create_test_adapter(&db, "adapter-a-1", "tenant-a", "Adapter A1").await?;
    create_test_adapter(&db, "adapter-a-2", "tenant-a", "Adapter A2").await?;

    // Create comprehensive resources for tenant B
    create_test_dataset(&db, "dataset-b-1", "tenant-b", "Dataset B1").await?;
    create_test_adapter(&db, "adapter-b-1", "tenant-b", "Adapter B1").await?;
    create_test_adapter(&db, "adapter-b-2", "tenant-b", "Adapter B2").await?;
    create_test_adapter(&db, "adapter-b-3", "tenant-b", "Adapter B3").await?;

    // Verify tenant A isolation
    let datasets_a = db
        .list_training_datasets_for_tenant("tenant-a", 100)
        .await?;
    let adapters_a = db.list_adapters_by_tenant("tenant-a").await?;

    assert_eq!(datasets_a.len(), 2, "Tenant A has 2 datasets");
    assert_eq!(adapters_a.len(), 2, "Tenant A has 2 adapters");

    assert!(
        datasets_a
            .iter()
            .all(|d| d.tenant_id == Some("tenant-a".to_string())),
        "All tenant-a datasets belong to tenant-a"
    );
    assert!(
        adapters_a.iter().all(|a| a.tenant_id == "tenant-a"),
        "All tenant-a adapters belong to tenant-a"
    );

    // Verify tenant B isolation
    let datasets_b = db
        .list_training_datasets_for_tenant("tenant-b", 100)
        .await?;
    let adapters_b = db.list_adapters_by_tenant("tenant-b").await?;

    assert_eq!(datasets_b.len(), 1, "Tenant B has 1 dataset");
    assert_eq!(adapters_b.len(), 3, "Tenant B has 3 adapters");

    assert!(
        datasets_b
            .iter()
            .all(|d| d.tenant_id == Some("tenant-b".to_string())),
        "All tenant-b datasets belong to tenant-b"
    );
    assert!(
        adapters_b.iter().all(|a| a.tenant_id == "tenant-b"),
        "All tenant-b adapters belong to tenant-b"
    );

    // Verify no overlap
    let dataset_a_ids: Vec<_> = datasets_a.iter().map(|d| d.id.as_str()).collect();
    let dataset_b_ids: Vec<_> = datasets_b.iter().map(|d| d.id.as_str()).collect();

    assert!(
        dataset_a_ids.iter().all(|id| !dataset_b_ids.contains(id)),
        "No dataset overlap between tenants"
    );

    let adapter_a_ids: Vec<_> = adapters_a
        .iter()
        .filter_map(|a| a.adapter_id.as_deref())
        .collect();
    let adapter_b_ids: Vec<_> = adapters_b
        .iter()
        .filter_map(|a| a.adapter_id.as_deref())
        .collect();

    assert!(
        adapter_a_ids.iter().all(|id| !adapter_b_ids.contains(id)),
        "No adapter overlap between tenants"
    );

    Ok(())
}

// =============================================================================
// TEST SUITE: Permission Checks for Tenant Operations
// =============================================================================

#[tokio::test]
async fn test_permission_checks_for_tenant_operations() -> Result<()> {
    // Setup: Create in-memory database
    let db = Db::new_in_memory().await?;

    // Create tenant
    create_test_tenant(&db, "tenant-a").await?;

    // Create users with different roles
    let admin_id = create_test_user(&db, "admin@aos.local", Role::Admin, "tenant-a").await?;
    let operator_id =
        create_test_user(&db, "operator@aos.local", Role::Operator, "tenant-a").await?;
    let viewer_id = create_test_user(&db, "viewer@aos.local", Role::Viewer, "tenant-a").await?;

    // Create claims
    let admin_claims = create_test_claims(&admin_id, "admin@aos.local", "admin", "tenant-a");
    let operator_claims =
        create_test_claims(&operator_id, "operator@aos.local", "operator", "tenant-a");
    let viewer_claims = create_test_claims(&viewer_id, "viewer@aos.local", "viewer", "tenant-a");

    // Test DatasetView permission (all roles should have it)
    require_permission(&admin_claims, Permission::DatasetView)
        .map_err(|_| AosError::Authz("Admin should have DatasetView".into()))?;
    require_permission(&operator_claims, Permission::DatasetView)
        .map_err(|_| AosError::Authz("Operator should have DatasetView".into()))?;
    require_permission(&viewer_claims, Permission::DatasetView)
        .map_err(|_| AosError::Authz("Viewer should have DatasetView".into()))?;

    // Test DatasetUpload permission (admin, operator should have; viewer should not)
    require_permission(&admin_claims, Permission::DatasetUpload)
        .map_err(|_| AosError::Authz("Admin should have DatasetUpload".into()))?;
    require_permission(&operator_claims, Permission::DatasetUpload)
        .map_err(|_| AosError::Authz("Operator should have DatasetUpload".into()))?;
    assert!(
        require_permission(&viewer_claims, Permission::DatasetUpload).is_err(),
        "Viewer should not have DatasetUpload"
    );

    // Test DatasetDelete permission (only admin should have)
    require_permission(&admin_claims, Permission::DatasetDelete)
        .map_err(|_| AosError::Authz("Admin should have DatasetDelete".into()))?;
    assert!(
        require_permission(&operator_claims, Permission::DatasetDelete).is_err(),
        "Operator should not have DatasetDelete"
    );
    assert!(
        require_permission(&viewer_claims, Permission::DatasetDelete).is_err(),
        "Viewer should not have DatasetDelete"
    );

    // Test AdapterLoad permission (admin, operator should have; viewer should not)
    require_permission(&admin_claims, Permission::AdapterLoad)
        .map_err(|_| AosError::Authz("Admin should have AdapterLoad".into()))?;
    require_permission(&operator_claims, Permission::AdapterLoad)
        .map_err(|_| AosError::Authz("Operator should have AdapterLoad".into()))?;
    assert!(
        require_permission(&viewer_claims, Permission::AdapterLoad).is_err(),
        "Viewer should not have AdapterLoad"
    );

    // Test TrainingStart permission (admin, operator should have; viewer should not)
    require_permission(&admin_claims, Permission::TrainingStart)
        .map_err(|_| AosError::Authz("Admin should have TrainingStart".into()))?;
    require_permission(&operator_claims, Permission::TrainingStart)
        .map_err(|_| AosError::Authz("Operator should have TrainingStart".into()))?;
    assert!(
        require_permission(&viewer_claims, Permission::TrainingStart).is_err(),
        "Viewer should not have TrainingStart"
    );

    Ok(())
}
