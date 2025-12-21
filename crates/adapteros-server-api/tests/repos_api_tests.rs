//! Integration tests for the adapter repositories API
//!
//! Tests the /v1/repos endpoint handlers for:
//! - Repository CRUD operations
//! - Adapter version management
//! - Version promotion and rollback workflows

mod common;

use axum::{
    extract::{Extension, Path, State},
    Json,
};
use common::{setup_state, test_admin_claims};
use uuid::Uuid;

use adapteros_server_api::handlers::repos::{
    create_repo, get_repo, list_repos, list_versions, update_repo, CreateRepoRequest,
    UpdateRepoRequest,
};

// =============================================================================
// Test Helpers
// =============================================================================

/// Create a test repository in the database
async fn create_test_repo(
    state: &adapteros_server_api::state::AppState,
    tenant_id: &str,
    name: &str,
) -> anyhow::Result<String> {
    let repo_id = Uuid::now_v7().to_string();

    adapteros_db::sqlx::query(
        "INSERT INTO adapter_repositories
         (id, tenant_id, name, default_branch, created_by, description, archived)
         VALUES (?, ?, ?, 'main', 'tester', 'Test repository', 0)",
    )
    .bind(&repo_id)
    .bind(tenant_id)
    .bind(name)
    .execute(state.db.pool())
    .await?;

    Ok(repo_id)
}

/// Create a test adapter version in the database
async fn create_test_adapter_version(
    state: &adapteros_server_api::state::AppState,
    tenant_id: &str,
    repo_id: &str,
    version: &str,
    release_state: &str,
) -> anyhow::Result<String> {
    let version_id = Uuid::now_v7().to_string();

    adapteros_db::sqlx::query(
        "INSERT INTO adapter_versions
         (id, tenant_id, repo_id, version, branch, branch_classification,
          adapter_trust_state, release_state, attach_mode, is_archived, coreml_used)
         VALUES (?, ?, ?, ?, 'main', 'development',
          'unknown', ?, 'free', 0, 0)",
    )
    .bind(&version_id)
    .bind(tenant_id)
    .bind(repo_id)
    .bind(version)
    .bind(release_state)
    .execute(state.db.pool())
    .await?;

    Ok(version_id)
}

// =============================================================================
// Repository CRUD Tests
// =============================================================================

#[tokio::test]
async fn test_create_repo() {
    let state = setup_state(None).await.unwrap();
    let claims = test_admin_claims();

    // Create a base model for the FK constraint
    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO models (id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3)
         VALUES ('qwen2.5-7b', 'Qwen 2.5 7B', 'test_hash_123', 'config_hash', 'tok_hash', 'tok_cfg_hash')"
    )
    .execute(state.db.pool())
    .await
    .unwrap();

    let req = CreateRepoRequest {
        tenant_id: claims.tenant_id.clone(),
        name: "test-repo-create".to_string(),
        base_model_id: Some("qwen2.5-7b".to_string()),
        description: Some("Test repository for create test".to_string()),
        default_branch: Some("main".to_string()),
    };

    let result = create_repo(State(state.clone()), Extension(claims), Json(req)).await;

    match &result {
        Ok(_) => {}
        Err(e) => {
            panic!("create_repo failed: {:?}", e);
        }
    }
    let (status, Json(response)) = result.unwrap();
    assert_eq!(
        status,
        axum::http::StatusCode::CREATED,
        "Should return CREATED status"
    );
    assert!(!response.repo_id.is_empty(), "repo_id should be returned");
}

#[tokio::test]
async fn test_list_repos() {
    let state = setup_state(None).await.unwrap();
    let claims = test_admin_claims();

    // Create a test repo first
    create_test_repo(&state, &claims.tenant_id, "list-test-repo")
        .await
        .unwrap();

    let result = list_repos(State(state.clone()), Extension(claims)).await;

    assert!(result.is_ok(), "list_repos should succeed");
    let repos = result.unwrap().0;
    assert!(!repos.is_empty(), "Should have at least one repository");
    assert!(
        repos.iter().any(|r| r.name == "list-test-repo"),
        "Should find the created repo"
    );
}

#[tokio::test]
async fn test_get_repo() {
    let state = setup_state(None).await.unwrap();
    let claims = test_admin_claims();

    // Create a test repo first
    let repo_id = create_test_repo(&state, &claims.tenant_id, "get-test-repo")
        .await
        .unwrap();

    let result = get_repo(
        State(state.clone()),
        Extension(claims),
        Path(repo_id.clone()),
    )
    .await;

    assert!(result.is_ok(), "get_repo should succeed");
    let repo = result.unwrap().0;
    assert_eq!(repo.id, repo_id, "Repo ID should match");
    assert_eq!(repo.name, "get-test-repo", "Repo name should match");
    assert_eq!(repo.default_branch, "main", "Default branch should be main");
}

#[tokio::test]
async fn test_get_repo_not_found() {
    let state = setup_state(None).await.unwrap();
    let claims = test_admin_claims();

    let result = get_repo(
        State(state.clone()),
        Extension(claims),
        Path("nonexistent-repo-id".to_string()),
    )
    .await;

    assert!(result.is_err(), "get_repo should fail for nonexistent repo");
}

#[tokio::test]
async fn test_update_repo() {
    let state = setup_state(None).await.unwrap();
    let claims = test_admin_claims();

    // Create a test repo first
    let repo_id = create_test_repo(&state, &claims.tenant_id, "update-test-repo")
        .await
        .unwrap();

    let req = UpdateRepoRequest {
        description: Some("Updated description".to_string()),
        default_branch: Some("develop".to_string()),
    };

    let result = update_repo(
        State(state.clone()),
        Extension(claims.clone()),
        Path(repo_id.clone()),
        Json(req),
    )
    .await;

    assert!(result.is_ok(), "update_repo should succeed");
    let repo = result.unwrap().0;
    assert_eq!(
        repo.description,
        Some("Updated description".to_string()),
        "Description should be updated"
    );
    assert_eq!(
        repo.default_branch, "develop",
        "Default branch should be updated"
    );
}

// =============================================================================
// Version Management Tests
// =============================================================================

#[tokio::test]
async fn test_list_versions() {
    let state = setup_state(None).await.unwrap();
    let claims = test_admin_claims();

    // Create a test repo with versions
    let repo_id = create_test_repo(&state, &claims.tenant_id, "versions-test-repo")
        .await
        .unwrap();

    // Create some test versions
    create_test_adapter_version(&state, &claims.tenant_id, &repo_id, "v0.1.0", "draft")
        .await
        .unwrap();
    create_test_adapter_version(&state, &claims.tenant_id, &repo_id, "v0.2.0", "ready")
        .await
        .unwrap();

    let result = list_versions(
        State(state.clone()),
        Extension(claims),
        Path(repo_id.clone()),
    )
    .await;

    match &result {
        Ok(_) => {}
        Err(e) => {
            panic!("list_versions failed: {:?}", e);
        }
    }
    let versions = result.unwrap().0;
    assert_eq!(versions.len(), 2, "Should have 2 versions");
    assert!(
        versions.iter().any(|v| v.version == "v0.1.0"),
        "Should have v0.1.0"
    );
    assert!(
        versions.iter().any(|v| v.version == "v0.2.0"),
        "Should have v0.2.0"
    );
}

#[tokio::test]
async fn test_list_versions_empty_repo() {
    let state = setup_state(None).await.unwrap();
    let claims = test_admin_claims();

    // Create a test repo without versions
    let repo_id = create_test_repo(&state, &claims.tenant_id, "empty-versions-repo")
        .await
        .unwrap();

    let result = list_versions(
        State(state.clone()),
        Extension(claims),
        Path(repo_id.clone()),
    )
    .await;

    assert!(
        result.is_ok(),
        "list_versions should succeed for empty repo"
    );
    let versions = result.unwrap().0;
    assert!(versions.is_empty(), "Should have no versions");
}

#[tokio::test]
async fn test_list_versions_repo_not_found() {
    let state = setup_state(None).await.unwrap();
    let claims = test_admin_claims();

    let result = list_versions(
        State(state.clone()),
        Extension(claims),
        Path("nonexistent-repo".to_string()),
    )
    .await;

    assert!(
        result.is_err(),
        "list_versions should fail for nonexistent repo"
    );
}

// =============================================================================
// Tenant Isolation Tests
// =============================================================================

#[tokio::test]
async fn test_list_repos_tenant_isolation() {
    let state = setup_state(None).await.unwrap();

    // Create repos for different tenants
    create_test_repo(&state, "tenant-1", "tenant1-repo")
        .await
        .unwrap();

    // Create another tenant and repo
    adapteros_db::sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-2")
        .bind("Test Tenant 2")
        .execute(state.db.pool())
        .await
        .unwrap();
    create_test_repo(&state, "tenant-2", "tenant2-repo")
        .await
        .unwrap();

    // List repos for tenant-1
    let claims = test_admin_claims(); // tenant-1
    let result = list_repos(State(state.clone()), Extension(claims)).await;

    let repos = result.unwrap().0;

    // Should see tenant-1's repo but not tenant-2's
    assert!(
        repos.iter().any(|r| r.name == "tenant1-repo"),
        "Should find tenant-1's repo"
    );
    assert!(
        !repos.iter().any(|r| r.name == "tenant2-repo"),
        "Should NOT find tenant-2's repo (tenant isolation)"
    );
}

#[tokio::test]
async fn test_get_repo_tenant_isolation() {
    let state = setup_state(None).await.unwrap();

    // Create tenant-2
    adapteros_db::sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-2")
        .bind("Test Tenant 2")
        .execute(state.db.pool())
        .await
        .unwrap();

    // Create a repo for tenant-2
    let repo_id = create_test_repo(&state, "tenant-2", "other-tenant-repo")
        .await
        .unwrap();

    // Try to access from tenant-1
    let claims = test_admin_claims(); // tenant-1
    let result = get_repo(State(state.clone()), Extension(claims), Path(repo_id)).await;

    assert!(
        result.is_err(),
        "Should not be able to access another tenant's repo"
    );
}

// =============================================================================
// Archived Repository Tests
// =============================================================================

#[tokio::test]
async fn test_update_archived_repo_rejected() {
    let state = setup_state(None).await.unwrap();
    let claims = test_admin_claims();

    // Create an archived repo
    let repo_id = Uuid::now_v7().to_string();
    adapteros_db::sqlx::query(
        "INSERT INTO adapter_repositories
         (id, tenant_id, name, default_branch, created_by, archived)
         VALUES (?, ?, ?, 'main', 'tester', 1)",
    )
    .bind(&repo_id)
    .bind(&claims.tenant_id)
    .bind("archived-repo")
    .execute(state.db.pool())
    .await
    .unwrap();

    // Try to update
    let req = UpdateRepoRequest {
        description: Some("Should fail".to_string()),
        default_branch: None,
    };

    let result = update_repo(
        State(state.clone()),
        Extension(claims),
        Path(repo_id),
        Json(req),
    )
    .await;

    assert!(result.is_err(), "Updating archived repo should be rejected");
}
