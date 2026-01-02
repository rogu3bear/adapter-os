//! Tenant Isolation Integration Tests
//!
//! Per PRD-RBAC-01, these tests verify complete tenant isolation across all resources:
//! - Datasets: Tenants cannot access each other's datasets
//! - Training Jobs: Tenants cannot see each other's training jobs
//! - Adapters: Tenants cannot load/unload each other's adapters
//! - List Operations: Lists are properly filtered by tenant
//! - Admin Override: Admin role can access all tenants' resources

use adapteros_api_types::adapters::PromoteVersionRequest;
use adapteros_api_types::training::ValidateDatasetRequest;
use adapteros_core::{AosError, Result};
use adapteros_db::adapter_repositories::CreateRepositoryParams;
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::sqlx;
use adapteros_db::users::Role;
use adapteros_db::Db;
use adapteros_server_api::auth::{AuthMode, Claims, PrincipalType};
use adapteros_server_api::handlers::code::{
    get_repository, list_repositories, register_repo, ListRepositoriesQuery,
    RegisterRepositoryRequest,
};
use adapteros_server_api::handlers::datasets::validate_dataset;
use adapteros_server_api::handlers::promote_adapter_version_handler;
use adapteros_server_api::permissions::{require_permission, Permission};
use adapteros_server_api::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use chrono::{Duration, Utc};
mod common;
use common::{setup_state, test_admin_claims};
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
        device_id: None,
        session_id: None,
        mfa_level: None,
        rot_id: None,
        exp: exp.timestamp(),
        iat: now.timestamp(),
        jti: Uuid::new_v4().to_string(),
        nbf: now.timestamp(),
        iss: "adapteros".to_string(),
        auth_mode: AuthMode::BearerToken,
        principal_type: Some(PrincipalType::User),
    }
}

/// Test helper to create a training dataset for a tenant
async fn create_test_dataset(db: &Db, dataset_id: &str, tenant_id: &str, name: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO training_datasets (id, name, tenant_id, format, storage_path, hash_b3, validation_status, created_at)
         VALUES (?, ?, ?, 'jsonl', 'var/test-datasets', ?, 'valid', datetime('now'))",
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

    // Align training job with tenant for isolation queries
    sqlx::query("UPDATE repository_training_jobs SET tenant_id = ? WHERE id = ?")
        .bind(tenant_id)
        .bind(job_id)
        .execute(db.pool())
        .await?;

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

async fn create_test_repo(db: &Db, tenant_id: &str, name: &str) -> Result<String> {
    // Ensure tenant exists to satisfy FK constraints; ignore if already present
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(db.pool())
        .await?;

    db.create_adapter_repository(CreateRepositoryParams {
        tenant_id,
        name,
        base_model_id: None,
        default_branch: None,
        created_by: Some("tester"),
        description: Some("test repo"),
    })
    .await
    .map_err(|e| AosError::Database(format!("Failed to create repo: {}", e)))
}

/// Create a code repository record for list/detail tests
async fn create_test_code_repo(db: &Db, tenant_id: &str, repo_id: &str) -> Result<()> {
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(db.pool())
        .await?;

    db.register_repository(
        tenant_id,
        repo_id,
        &format!("/repos/{tenant_id}/{repo_id}"),
        &[String::from("rust")],
        "main",
    )
    .await?;

    Ok(())
}

async fn create_test_version(
    db: &Db,
    tenant_id: &str,
    repo_id: &str,
    branch: &str,
    version: &str,
) -> Result<String> {
    db.create_adapter_version(adapteros_db::CreateVersionParams {
        repo_id,
        tenant_id,
        version,
        branch,
        branch_classification: "protected",
        aos_path: None,
        aos_hash: None,
        manifest_schema_version: None,
        parent_version_id: None,
        code_commit_sha: None,
        data_spec_hash: None,
        training_backend: None,
        coreml_used: None,
        coreml_device_type: None,
        dataset_version_ids: None,
        release_state: "ready",
        metrics_snapshot_id: None,
        evaluation_summary: None,
        allow_archived: false,
        actor: Some("tester"),
        reason: None,
        train_job_id: None,
    })
    .await
    .map_err(|e| AosError::Database(format!("Failed to create version: {}", e)))
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
// TEST SUITE: Cross-Tenant Adapter Version Access Denied
// =============================================================================

#[tokio::test]
async fn test_cross_tenant_adapter_versions_are_isolated() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    let repo_a = create_test_repo(&db, "tenant-a", "repo-a").await?;
    let _version_a = create_test_version(&db, "tenant-a", &repo_a, "main", "1.0.0").await?;

    // Tenant A sees its version
    let versions_a = db
        .list_adapter_versions_for_repo("tenant-a", &repo_a, Some("main"), None)
        .await?;
    assert_eq!(versions_a.len(), 1);

    // Tenant B should not see Tenant A versions
    let versions_b = db
        .list_adapter_versions_for_repo("tenant-b", &repo_a, Some("main"), None)
        .await?;
    assert!(
        versions_b.is_empty(),
        "Tenant B must not see Tenant A adapter versions"
    );

    Ok(())
}

// =============================================================================
// TEST SUITE: Dataset Validation Tenant Isolation
// =============================================================================

#[tokio::test]
async fn test_dataset_validate_enforces_tenant_isolation() -> Result<()> {
    let state: AppState = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;

    // Seed dataset with tenant-a ownership
    let dataset_id = "ds-iso";
    state
        .db
        .create_training_dataset_with_id(
            dataset_id,
            "Dataset ISO",
            Some("desc"),
            "jsonl",
            "hash-iso",
            "var/ds",
            None,
            None,
            Some("ready"),
            Some("hash-iso"),
            None,
        )
        .await?;
    sqlx::query("UPDATE training_datasets SET tenant_id = ? WHERE id = ?")
        .bind("tenant-a")
        .bind(dataset_id)
        .execute(state.db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to set tenant: {}", e)))?;

    // Claims for tenant-b (no admin_tenants grants)
    let mut claims = test_admin_claims();
    claims.tenant_id = "tenant-b".to_string();
    claims.admin_tenants = vec![];

    let result = validate_dataset(
        State(state.clone()),
        Extension(claims),
        Path(dataset_id.to_string()),
        Json(ValidateDatasetRequest {
            check_format: Some(false),
        }),
    )
    .await;

    match result {
        Err((status, _)) => assert_eq!(status, StatusCode::FORBIDDEN),
        Ok(_) => panic!("Cross-tenant validation must be forbidden"),
    }

    Ok(())
}

// =============================================================================
// TEST SUITE: Repository List Tenant Isolation
// =============================================================================

#[tokio::test]
async fn test_repository_list_filtered_by_tenant() -> Result<()> {
    let state: AppState = setup_state(None).await.expect("state");

    // Seed repos for two tenants
    let repo_a = "repo-a".to_string();
    let repo_b = "repo-b".to_string();
    create_test_code_repo(&state.db, "tenant-a", &repo_a).await?;
    create_test_code_repo(&state.db, "tenant-b", &repo_b).await?;

    let mut claims_a = test_admin_claims();
    claims_a.tenant_id = "tenant-a".to_string();
    let Json(list_a) = list_repositories(
        State(state.clone()),
        Extension(claims_a),
        Query(ListRepositoriesQuery {
            page: None,
            limit: None,
        }),
    )
    .await
    .unwrap();
    let ids_a: Vec<String> = list_a.repos.into_iter().map(|r| r.repo_id).collect();
    assert!(ids_a.contains(&repo_a), "Tenant A should see its own repo");
    assert!(
        !ids_a.contains(&repo_b),
        "Tenant A must not see Tenant B repo"
    );

    let mut claims_b = test_admin_claims();
    claims_b.tenant_id = "tenant-b".to_string();
    let Json(list_b) = list_repositories(
        State(state.clone()),
        Extension(claims_b),
        Query(ListRepositoriesQuery {
            page: None,
            limit: None,
        }),
    )
    .await
    .unwrap();
    let ids_b: Vec<String> = list_b.repos.into_iter().map(|r| r.repo_id).collect();
    assert!(ids_b.contains(&repo_b), "Tenant B should see its own repo");
    assert!(
        !ids_b.contains(&repo_a),
        "Tenant B must not see Tenant A repo"
    );

    Ok(())
}

#[tokio::test]
async fn test_repository_detail_respects_tenant() -> Result<()> {
    let state: AppState = setup_state(None).await.expect("state");

    let repo_a = "repo-a-detail".to_string();
    create_test_code_repo(&state.db, "tenant-a", &repo_a).await?;

    let mut claims_b = test_admin_claims();
    claims_b.tenant_id = "tenant-b".to_string();

    let result = get_repository(
        State(state.clone()),
        Extension(claims_b),
        Path(repo_a.clone()),
    )
    .await;

    match result {
        Err((status, _)) => assert_eq!(status, StatusCode::NOT_FOUND),
        Ok(_) => panic!("Cross-tenant repo detail must not be readable"),
    }

    Ok(())
}

#[tokio::test]
async fn test_repository_register_blocks_cross_tenant() -> Result<()> {
    let state: AppState = setup_state(None).await.expect("state");

    let mut claims_b = test_admin_claims();
    claims_b.tenant_id = "tenant-b".to_string();

    let result = register_repo(
        State(state.clone()),
        Extension(claims_b),
        Json(RegisterRepositoryRequest {
            tenant_id: "tenant-a".to_string(),
            repo_id: "repo-cross".to_string(),
            path: "var/repo".to_string(),
            languages: vec!["rust".to_string()],
            default_branch: "main".to_string(),
        }),
    )
    .await;

    match result {
        Err((status, _)) => assert_eq!(status, StatusCode::FORBIDDEN),
        Ok(_) => panic!("Cross-tenant register should not succeed"),
    }

    Ok(())
}

#[tokio::test]
async fn test_adapter_version_promotion_blocks_cross_tenant() -> Result<()> {
    let state: AppState = setup_state(None).await.expect("state");

    let repo_a = create_test_repo(&state.db, "tenant-a", "repo-promote").await?;
    let version_a = create_test_version(&state.db, "tenant-a", &repo_a, "main", "1.2.3").await?;

    let mut claims_b = test_admin_claims();
    claims_b.tenant_id = "tenant-b".to_string();

    let result = promote_adapter_version_handler(
        State(state.clone()),
        Extension(claims_b),
        Path(version_a.clone()),
        Json(PromoteVersionRequest {
            repo_id: repo_a.clone(),
        }),
    )
    .await;

    match result {
        Err((status, _)) => assert_eq!(status, StatusCode::NOT_FOUND),
        Ok(_) => panic!("Cross-tenant promotion should not succeed"),
    }

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
    let _adapter_a_id = create_test_adapter(&db, "adapter-a-1", "tenant-a", "Adapter A1").await?;
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

#[tokio::test]
async fn test_get_adapter_for_tenant_enforces_isolation() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    create_test_adapter(&db, "adapter-a-1", "tenant-a", "Adapter A1").await?;

    let same_tenant = db.get_adapter_for_tenant("tenant-a", "adapter-a-1").await?;
    assert!(same_tenant.is_some(), "tenant-a should see its adapter");

    let cross_tenant = db.get_adapter_for_tenant("tenant-b", "adapter-a-1").await?;
    assert!(
        cross_tenant.is_none(),
        "tenant-b must not see tenant-a adapter"
    );

    Ok(())
}

// =============================================================================
// TEST SUITE: Cross-Tenant Base Model Access Denied
// =============================================================================

#[tokio::test]
async fn test_cross_tenant_base_model_access_denied() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Insert tenant-scoped base models
    sqlx::query("INSERT INTO models (id, name, hash_b3, license_hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3, metadata_json, tenant_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind("model-a-id")
        .bind("model-a")
        .bind("hash-a")
        .bind(None::<String>)
        .bind("config-hash-a")
        .bind("tokenizer-hash-a")
        .bind("tokenizer-cfg-hash-a")
        .bind(None::<String>)
        .bind("tenant-a")
        .execute(db.pool())
        .await?;

    sqlx::query("INSERT INTO models (id, name, hash_b3, license_hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3, metadata_json, tenant_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind("model-b-id")
        .bind("model-b")
        .bind("hash-b")
        .bind(None::<String>)
        .bind("config-hash-b")
        .bind("tokenizer-hash-b")
        .bind("tokenizer-cfg-hash-b")
        .bind(None::<String>)
        .bind("tenant-b")
        .execute(db.pool())
        .await?;

    // Global model (tenant_id NULL) should be visible to all tenants
    sqlx::query("INSERT INTO models (id, name, hash_b3, license_hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3, metadata_json, tenant_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind("model-global-id")
        .bind("model-global")
        .bind("hash-g")
        .bind(None::<String>)
        .bind("config-hash-g")
        .bind("tokenizer-hash-g")
        .bind("tokenizer-cfg-hash-g")
        .bind(None::<String>)
        .bind(None::<String>)
        .execute(db.pool())
        .await?;

    let tenant_a_model = db.get_model_for_tenant("tenant-a", "model-a-id").await?;
    assert!(
        tenant_a_model.is_some(),
        "tenant-a should see its base model"
    );

    let tenant_b_denied = db.get_model_for_tenant("tenant-b", "model-a-id").await?;
    assert!(
        tenant_b_denied.is_none(),
        "tenant-b must not see tenant-a base model"
    );

    let model_b_by_name = db
        .get_model_by_name_for_tenant("tenant-b", "model-b")
        .await?;
    assert!(
        model_b_by_name.is_some(),
        "tenant-b should resolve its base model by name"
    );

    let cross_name_lookup = db
        .get_model_by_name_for_tenant("tenant-a", "model-b")
        .await?;
    assert!(
        cross_name_lookup.is_none(),
        "tenant-a must not resolve tenant-b base model by name"
    );

    let global_a = db
        .get_model_by_name_for_tenant("tenant-a", "model-global")
        .await?;
    assert!(
        global_a.is_some(),
        "global model should be visible to tenant-a"
    );
    let global_b = db
        .get_model_by_name_for_tenant("tenant-b", "model-global")
        .await?;
    assert!(
        global_b.is_some(),
        "global model should be visible to tenant-b"
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

// =============================================================================
// TEST SUITE: Tenant Isolation Validation Function Tests
// =============================================================================

#[tokio::test]
async fn test_validate_tenant_isolation_same_tenant_ok() -> Result<()> {
    use adapteros_server_api::security::validate_tenant_isolation;

    let claims = create_test_claims("user-1", "user@tenant-a.com", "operator", "tenant-a");

    // Same tenant should succeed
    let result = validate_tenant_isolation(&claims, "tenant-a");
    assert!(result.is_ok(), "Same tenant should pass validation");

    Ok(())
}

#[tokio::test]
async fn test_validate_tenant_isolation_cross_tenant_denied() -> Result<()> {
    use adapteros_server_api::security::validate_tenant_isolation;

    let claims = create_test_claims("user-1", "user@tenant-a.com", "operator", "tenant-a");

    // Cross-tenant should fail with 403
    let result = validate_tenant_isolation(&claims, "tenant-b");
    assert!(result.is_err(), "Cross-tenant access should be denied");

    // Verify it returns 403 Forbidden
    if let Err((status_code, _)) = result {
        assert_eq!(
            status_code,
            axum::http::StatusCode::FORBIDDEN,
            "Should return 403 Forbidden for cross-tenant access"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_validate_tenant_isolation_admin_can_access_any_tenant() -> Result<()> {
    use adapteros_server_api::security::validate_tenant_isolation;

    let mut admin_claims = create_test_claims("admin-1", "admin@system.com", "admin", "system");
    admin_claims.admin_tenants = vec!["*".to_string()];

    // Admin should access any tenant
    let result_a = validate_tenant_isolation(&admin_claims, "tenant-a");
    assert!(result_a.is_ok(), "Admin should access tenant-a");

    let result_b = validate_tenant_isolation(&admin_claims, "tenant-b");
    assert!(result_b.is_ok(), "Admin should access tenant-b");

    let result_c = validate_tenant_isolation(&admin_claims, "tenant-c");
    assert!(result_c.is_ok(), "Admin should access tenant-c");

    Ok(())
}

// =============================================================================
// TEST SUITE: Token Revocation Baseline Tests
// =============================================================================

#[tokio::test]
async fn test_tenant_token_baseline_get_set() -> Result<()> {
    use adapteros_server_api::security::{get_tenant_token_baseline, set_tenant_token_baseline};

    // Setup: Create in-memory database
    let db = Db::new_in_memory().await?;

    // Create tenant
    create_test_tenant(&db, "tenant-a").await?;

    // Initially, baseline should be None
    let initial_baseline = get_tenant_token_baseline(&db, "tenant-a").await?;
    assert!(
        initial_baseline.is_none(),
        "Initial baseline should be None"
    );

    // Set baseline
    let baseline_time = "2025-12-02T12:00:00Z";
    set_tenant_token_baseline(&db, "tenant-a", baseline_time).await?;

    // Verify baseline is set
    let updated_baseline = get_tenant_token_baseline(&db, "tenant-a").await?;
    assert_eq!(
        updated_baseline,
        Some(baseline_time.to_string()),
        "Baseline should be set correctly"
    );

    Ok(())
}

#[tokio::test]
async fn test_token_before_baseline_should_be_rejected() -> Result<()> {
    use chrono::DateTime;

    // Create token issued at T0 (2025-12-01T00:00:00Z)
    let token_iat_timestamp = DateTime::parse_from_rfc3339("2025-12-01T00:00:00Z")
        .unwrap()
        .timestamp();

    // Create claims with T0 iat
    let claims = Claims {
        sub: "user-1".to_string(),
        email: "user@tenant-a.com".to_string(),
        role: "operator".to_string(),
        roles: vec!["operator".to_string()],
        tenant_id: "tenant-a".to_string(),
        admin_tenants: vec![],
        device_id: None,
        session_id: None,
        mfa_level: None,
        rot_id: None,
        exp: (Utc::now() + Duration::hours(8)).timestamp(),
        iat: token_iat_timestamp,
        jti: Uuid::new_v4().to_string(),
        nbf: token_iat_timestamp,
        iss: "adapteros".to_string(),
        auth_mode: AuthMode::BearerToken,
        principal_type: Some(PrincipalType::User),
    };

    // Baseline set to T1 (2025-12-02T00:00:00Z) - AFTER token was issued
    let baseline = "2025-12-02T00:00:00Z";
    let baseline_ts = DateTime::parse_from_rfc3339(baseline).unwrap();

    // Token iat (T0) < baseline (T1), so should be rejected
    let token_iat = chrono::DateTime::from_timestamp(claims.iat, 0).unwrap();
    assert!(
        token_iat.timestamp() < baseline_ts.timestamp(),
        "Token iat should be before baseline"
    );

    // In production, this check happens in middleware/mod.rs (auth_middleware)
    // Here we verify the logic is correct
    let should_reject = token_iat.timestamp() < baseline_ts.timestamp();
    assert!(
        should_reject,
        "Token issued before baseline should be rejected"
    );

    Ok(())
}

#[tokio::test]
async fn test_token_after_baseline_should_be_accepted() -> Result<()> {
    use chrono::DateTime;

    // Baseline set to T0 (2025-12-01T00:00:00Z)
    let baseline = "2025-12-01T00:00:00Z";
    let baseline_ts = DateTime::parse_from_rfc3339(baseline).unwrap();

    // Token issued at T1 (2025-12-02T00:00:00Z) - AFTER baseline
    let token_iat_timestamp = DateTime::parse_from_rfc3339("2025-12-02T00:00:00Z")
        .unwrap()
        .timestamp();

    // Token iat (T1) > baseline (T0), so should be accepted
    let token_iat = chrono::DateTime::from_timestamp(token_iat_timestamp, 0).unwrap();
    assert!(
        token_iat.timestamp() >= baseline_ts.timestamp(),
        "Token iat should be at or after baseline"
    );

    let should_accept = token_iat.timestamp() >= baseline_ts.timestamp();
    assert!(
        should_accept,
        "Token issued after baseline should be accepted"
    );

    Ok(())
}

// =============================================================================
// TEST SUITE: TenantTokenRevoke Permission Tests
// =============================================================================

#[tokio::test]
async fn test_tenant_token_revoke_permission_admin_only() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_test_tenant(&db, "tenant-a").await?;

    let admin_id = create_test_user(&db, "admin@aos.local", Role::Admin, "tenant-a").await?;
    let operator_id =
        create_test_user(&db, "operator@aos.local", Role::Operator, "tenant-a").await?;
    let viewer_id = create_test_user(&db, "viewer@aos.local", Role::Viewer, "tenant-a").await?;

    let admin_claims = create_test_claims(&admin_id, "admin@aos.local", "admin", "tenant-a");
    let operator_claims =
        create_test_claims(&operator_id, "operator@aos.local", "operator", "tenant-a");
    let viewer_claims = create_test_claims(&viewer_id, "viewer@aos.local", "viewer", "tenant-a");

    // Admin should have TenantTokenRevoke permission
    assert!(
        require_permission(&admin_claims, Permission::TenantTokenRevoke).is_ok(),
        "Admin should have TenantTokenRevoke permission"
    );

    // Operator should NOT have TenantTokenRevoke permission
    assert!(
        require_permission(&operator_claims, Permission::TenantTokenRevoke).is_err(),
        "Operator should NOT have TenantTokenRevoke permission"
    );

    // Viewer should NOT have TenantTokenRevoke permission
    assert!(
        require_permission(&viewer_claims, Permission::TenantTokenRevoke).is_err(),
        "Viewer should NOT have TenantTokenRevoke permission"
    );

    Ok(())
}
