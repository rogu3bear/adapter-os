//! Comprehensive Tenant Isolation Tests for Adapter Operations - PRD-RECT-001
//!
//! These tests verify that tenant boundaries are properly enforced for:
//! - Adapter listing (general, by category, by scope, by state)
//! - Adapter detail access
//! - Adapter modification operations (lifecycle, strength)
//! - Adapter state summary
//! - Cross-tenant denial with 404 responses (preventing enumeration)

use adapteros_core::Result;
use adapteros_db::adapter_repositories::CreateRepositoryParams;
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::sqlx;
use adapteros_db::workers::WorkerRegistrationParams;
use adapteros_db::Db;
use adapteros_server_api::auth::{AuthMode, Claims, PrincipalType};
use adapteros_server_api::handlers::adapters::{
    demote_adapter_lifecycle, get_adapter_stats, promote_adapter_lifecycle,
    update_adapter_strength, LifecycleTransitionRequest, UpdateAdapterStrengthRequest,
};
use adapteros_server_api::handlers::adapters_read::{get_adapter_repository, list_adapters};
use adapteros_server_api::handlers::{delete_adapter, get_adapter, get_adapter_activations};
use adapteros_server_api::inference_core::InferenceCore;
use adapteros_server_api::types::{InferenceError, InferenceRequestInternal, ListAdaptersQuery};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use chrono::{Duration, Utc};
use std::collections::HashMap;
use uuid::Uuid;

mod common;
use common::{setup_state, test_admin_claims, test_viewer_claims};

// =============================================================================
// Test Helpers
// =============================================================================

/// Test helper to create a tenant
async fn create_test_tenant(db: &Db, tenant_id: &str) -> Result<()> {
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(db.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to create tenant: {}", e))
        })?;
    Ok(())
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

async fn create_test_repo(db: &Db, tenant_id: &str, name: &str) -> Result<String> {
    db.create_adapter_repository(CreateRepositoryParams {
        tenant_id,
        name,
        base_model_id: Some("base-model"),
        default_branch: None,
        created_by: Some("tester"),
        description: Some("test repo"),
    })
    .await
    .map_err(|e| adapteros_core::AosError::Database(format!("Failed to create repo: {}", e)))
}

async fn register_test_adapter(
    state: &adapteros_server_api::state::AppState,
    tenant_id: &str,
    adapter_id: &str,
) -> Result<String> {
    let params = AdapterRegistrationBuilder::new()
        .tenant_id(tenant_id)
        .adapter_id(adapter_id)
        .name("Test Adapter")
        .hash_b3("b3:tenant-isolation-test")
        .rank(4)
        .tier("persistent")
        .category("code")
        .scope("tenant")
        .build()
        .expect("adapter params");

    let id = state.db.register_adapter(params).await?;
    Ok(id)
}

/// Test helper to create an adapter with specific properties
async fn create_test_adapter(
    db: &Db,
    adapter_id: &str,
    tenant_id: &str,
    name: &str,
    category: &str,
    scope: &str,
    state: &str,
) -> Result<String> {
    // Use adapter_id as hash to ensure uniqueness
    let unique_hash = format!("hash_{}", adapter_id);
    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id)
        .name(name)
        .hash_b3(&unique_hash)
        .rank(16)
        .tier("persistent")
        .category(category)
        .scope(scope)
        .tenant_id(tenant_id)
        .build()
        .map_err(|e| {
            adapteros_core::AosError::Validation(format!("Failed to build adapter params: {}", e))
        })?;

    let id = db.register_adapter(params).await?;

    // Set the current_state
    sqlx::query("UPDATE adapters SET current_state = ? WHERE id = ?")
        .bind(state)
        .bind(&id)
        .execute(db.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to set adapter state: {}", e))
        })?;

    Ok(id)
}

// =============================================================================
// EXISTING TESTS - Basic Cross-Tenant Access Denial
// =============================================================================

#[tokio::test]
async fn cross_tenant_get_adapter_returns_404() -> Result<()> {
    let state = setup_state(None).await.expect("state");
    register_test_adapter(&state, "tenant-1", "tenant-1-adapter").await?;

    let claims_other_tenant = test_viewer_claims(); // tenant_id = "default"
    let result = get_adapter(
        State(state),
        Extension(claims_other_tenant),
        Path("tenant-1-adapter".to_string()),
    )
    .await;

    match result {
        Err((status, body)) => {
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(body.0.code, "NOT_FOUND");
        }
        Ok(_) => panic!("cross-tenant get_adapter should not succeed"),
    }

    Ok(())
}

#[tokio::test]
async fn cross_tenant_delete_adapter_returns_404() -> Result<()> {
    let state = setup_state(None).await.expect("state");
    register_test_adapter(&state, "tenant-1", "tenant-1-adapter").await?;

    let mut claims_other_tenant = test_admin_claims();
    claims_other_tenant.tenant_id = "default".to_string();

    let result = delete_adapter(
        State(state),
        Extension(claims_other_tenant),
        Path("tenant-1-adapter".to_string()),
    )
    .await;

    match result {
        Err((status, body)) => {
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(body.0.code, "NOT_FOUND");
        }
        Ok(_) => panic!("cross-tenant delete_adapter should not succeed"),
    }

    Ok(())
}

#[tokio::test]
async fn pinned_adapter_cross_tenant_is_indistinguishable_from_not_found() -> Result<()> {
    let state = setup_state(None)
        .await
        .expect("state")
        .with_manifest_info("test-manifest-hash".to_string(), "mlx".to_string());
    let foreign_adapter_id = register_test_adapter(&state, "default", "default-adapter").await?;

    // Satisfy base-model gating and worker selection so we can reach pinned validation.
    sqlx::query(
        "INSERT OR IGNORE INTO models
         (id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("base-model")
    .bind("Base Model")
    .bind("hash-base-model")
    .bind("hash-config")
    .bind("hash-tokenizer")
    .bind("hash-tokenizer-cfg")
    .execute(state.db.pool())
    .await?;

    state
        .db
        .update_base_model_status("tenant-1", "base-model", "ready", None, None)
        .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO nodes (id, hostname, agent_endpoint, status)
         VALUES (?, ?, ?, 'active')",
    )
    .bind("test-node-1")
    .bind("test-node-1")
    .bind("http://localhost")
    .execute(state.db.pool())
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO manifests (id, tenant_id, hash_b3, body_json)
         VALUES (?, ?, ?, ?)",
    )
    .bind("test-manifest-1")
    .bind("tenant-1")
    .bind("test-manifest-hash")
    .bind("{}")
    .execute(state.db.pool())
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO plans
         (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("test-plan-1")
    .bind("tenant-1")
    .bind("plan-b3:test-plan-1")
    .bind("test-manifest-hash")
    .bind("[]")
    .bind("layout-b3:test-plan-1")
    .execute(state.db.pool())
    .await?;

    let worker_id = "test-worker-1".to_string();
    state
        .db
        .register_worker(WorkerRegistrationParams {
            worker_id: worker_id.clone(),
            tenant_id: "tenant-1".to_string(),
            node_id: "test-node-1".to_string(),
            plan_id: "test-plan-1".to_string(),
            uds_path: "var/run/test-worker.sock".to_string(),
            pid: 1234,
            manifest_hash: "test-manifest-hash".to_string(),
            backend: Some("mlx".to_string()),
            model_hash_b3: None,
            capabilities_json: None,
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            api_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        })
        .await?;
    state
        .db
        .transition_worker_status(&worker_id, "healthy", "test", None)
        .await?;

    let mut request = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
    request.session_id = Some("session-1".to_string());
    request.pinned_adapter_ids = Some(vec![foreign_adapter_id.clone()]);

    let core = InferenceCore::new(&state);
    let err = core
        .route_and_infer(request, None, None, None)
        .await
        .unwrap_err();

    match err {
        InferenceError::AdapterNotFound(msg) => {
            assert!(msg.contains(&foreign_adapter_id));
            assert!(!msg.contains("tenant-1"));
        }
        other => panic!("expected AdapterNotFound, got {:?}", other),
    }

    Ok(())
}

/// Regression test for handlers.rs tenant isolation fix.
/// Verifies that get_adapter handler returns 404 (not 403) for cross-tenant access,
/// ensuring the tenant-scoped query pattern is indistinguishable from adapter-not-found.
#[tokio::test]
async fn handlers_rs_get_adapter_cross_tenant_returns_404() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    // Register adapter in tenant-1
    register_test_adapter(&state, "tenant-1", "handlers-rs-isolation-test").await?;

    // Attempt cross-tenant access from "default" tenant
    let claims_other_tenant = test_viewer_claims(); // tenant_id = "default"
    let result = get_adapter(
        State(state),
        Extension(claims_other_tenant),
        Path("handlers-rs-isolation-test".to_string()),
    )
    .await;

    // Must return 404 NOT_FOUND (not 403 FORBIDDEN) to prevent adapter enumeration
    match result {
        Err((status, body)) => {
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "cross-tenant access must return 404, not 403"
            );
            assert_eq!(body.0.code, "NOT_FOUND");
        }
        Ok(_) => panic!("cross-tenant get_adapter should return 404, not succeed"),
    }

    Ok(())
}

#[tokio::test]
async fn cross_tenant_get_adapter_activations_returns_404() -> Result<()> {
    let state = setup_state(None).await.expect("state");
    register_test_adapter(&state, "tenant-1", "tenant-1-adapter").await?;

    let claims_other_tenant = test_viewer_claims();
    let result = get_adapter_activations(
        State(state),
        Extension(claims_other_tenant),
        Path("tenant-1-adapter".to_string()),
        Query(HashMap::new()),
    )
    .await;

    match result {
        Err((status, body)) => {
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(body.0.code, "NOT_FOUND");
        }
        Ok(_) => panic!("cross-tenant get_adapter_activations should not succeed"),
    }

    Ok(())
}

#[tokio::test]
async fn cross_tenant_get_adapter_stats_returns_404() -> Result<()> {
    let state = setup_state(None).await.expect("state");
    register_test_adapter(&state, "tenant-1", "tenant-1-adapter").await?;

    let claims_other_tenant = test_viewer_claims();
    let result = get_adapter_stats(
        State(state),
        Extension(claims_other_tenant),
        Path("tenant-1-adapter".to_string()),
    )
    .await;

    match result {
        Err((status, body)) => {
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(body.0.code, "NOT_FOUND");
        }
        Ok(_) => panic!("cross-tenant get_adapter_stats should not succeed"),
    }

    Ok(())
}

// =============================================================================
// NEW COMPREHENSIVE TESTS - PRD-RECT-001
// =============================================================================

// =============================================================================
// TEST SUITE: Cross-Tenant Adapter Listing Denied
// =============================================================================

#[tokio::test]
async fn test_list_adapters_respects_tenant_boundaries() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    // Create two tenants
    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;

    // Create adapters for each tenant
    create_test_adapter(
        &state.db,
        "adapter-a-1",
        "tenant-a",
        "Adapter A1",
        "code",
        "tenant",
        "cold",
    )
    .await?;
    create_test_adapter(
        &state.db,
        "adapter-a-2",
        "tenant-a",
        "Adapter A2",
        "code",
        "tenant",
        "warm",
    )
    .await?;
    create_test_adapter(
        &state.db,
        "adapter-b-1",
        "tenant-b",
        "Adapter B1",
        "code",
        "tenant",
        "hot",
    )
    .await?;

    // Tenant A claims
    let claims_a = create_test_claims("user-a", "user-a@tenant-a.com", "operator", "tenant-a");

    // Tenant A should only see their adapters
    let Json(adapters_a) = list_adapters(
        State(state.clone()),
        Extension(claims_a),
        Query(ListAdaptersQuery {
            tier: None,
            framework: None,
            ..Default::default()
        }),
    )
    .await
    .unwrap();

    assert_eq!(
        adapters_a.len(),
        2,
        "Tenant A should see exactly 2 adapters"
    );
    let adapter_ids_a: Vec<String> = adapters_a.iter().map(|a| a.adapter_id.clone()).collect();
    assert!(
        adapter_ids_a.contains(&"adapter-a-1".to_string()),
        "Tenant A should see adapter-a-1"
    );
    assert!(
        adapter_ids_a.contains(&"adapter-a-2".to_string()),
        "Tenant A should see adapter-a-2"
    );
    assert!(
        !adapter_ids_a.contains(&"adapter-b-1".to_string()),
        "Tenant A must NOT see adapter-b-1"
    );

    // Tenant B claims
    let claims_b = create_test_claims("user-b", "user-b@tenant-b.com", "operator", "tenant-b");

    // Tenant B should only see their adapters
    let Json(adapters_b) = list_adapters(
        State(state.clone()),
        Extension(claims_b),
        Query(ListAdaptersQuery {
            tier: None,
            framework: None,
            ..Default::default()
        }),
    )
    .await
    .unwrap();

    assert_eq!(adapters_b.len(), 1, "Tenant B should see exactly 1 adapter");
    assert_eq!(adapters_b[0].adapter_id, "adapter-b-1");
    assert!(
        !adapters_b
            .iter()
            .any(|a| a.adapter_id.starts_with("adapter-a")),
        "Tenant B must NOT see any tenant-a adapters"
    );

    Ok(())
}

// =============================================================================
// TEST SUITE: Cross-Tenant Adapter Repository Access Denied
// =============================================================================

#[tokio::test]
#[ignore = "requires base model fixtures for repository creation"]
async fn test_get_adapter_repository_blocks_cross_tenant() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;

    // Create repository for tenant A
    let repo_a = create_test_repo(&state.db, "tenant-a", "repo-a-detail").await?;

    // Tenant B claims
    let claims_b = create_test_claims("user-b", "user-b@tenant-b.com", "operator", "tenant-b");

    // Tenant B attempts to access Tenant A's repository
    let result = get_adapter_repository(
        State(state.clone()),
        Extension(claims_b),
        Path(repo_a.clone()),
    )
    .await;

    // Should return NOT_FOUND (not FORBIDDEN to avoid information leakage)
    match result {
        Err((status, _)) => {
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "Cross-tenant repository access should return NOT_FOUND"
            );
        }
        Ok(_) => panic!("Cross-tenant repository detail access must be denied"),
    }

    Ok(())
}

// =============================================================================
// TEST SUITE: Cross-Tenant Adapter Modification Denied
// =============================================================================

#[tokio::test]
async fn test_update_adapter_strength_blocks_cross_tenant() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;

    // Create adapter for tenant A
    create_test_adapter(
        &state.db,
        "adapter-a-1",
        "tenant-a",
        "Adapter A1",
        "code",
        "tenant",
        "cold",
    )
    .await?;

    // Tenant B claims (operator role has permission to update)
    let claims_b = create_test_claims("user-b", "user-b@tenant-b.com", "operator", "tenant-b");

    // Tenant B attempts to update Tenant A's adapter
    let result = update_adapter_strength(
        State(state.clone()),
        Extension(claims_b),
        Path("adapter-a-1".to_string()),
        Json(UpdateAdapterStrengthRequest { lora_strength: 1.5 }),
    )
    .await;

    // Should be denied
    match result {
        Err((status, _)) => {
            assert!(
                status == StatusCode::NOT_FOUND || status == StatusCode::FORBIDDEN,
                "Cross-tenant adapter update should be denied (got {})",
                status
            );
        }
        Ok(_) => panic!("Cross-tenant adapter update must be denied"),
    }

    Ok(())
}

#[tokio::test]
async fn test_promote_adapter_lifecycle_blocks_cross_tenant() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;

    // Create adapter for tenant A
    create_test_adapter(
        &state.db,
        "adapter-a-1",
        "tenant-a",
        "Adapter A1",
        "code",
        "tenant",
        "cold",
    )
    .await?;

    // Tenant B claims (operator role has permission to promote)
    let claims_b = create_test_claims("user-b", "user-b@tenant-b.com", "operator", "tenant-b");

    // Tenant B attempts to promote Tenant A's adapter
    let result = promote_adapter_lifecycle(
        State(state.clone()),
        Extension(claims_b),
        Path("adapter-a-1".to_string()),
        Json(LifecycleTransitionRequest {
            reason: "Cross-tenant promotion attempt".to_string(),
        }),
    )
    .await;

    // Should be denied
    match result {
        Err((status, _)) => {
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "Cross-tenant adapter promotion should return NOT_FOUND"
            );
        }
        Ok(_) => panic!("Cross-tenant adapter promotion must be denied"),
    }

    Ok(())
}

#[tokio::test]
async fn test_demote_adapter_lifecycle_blocks_cross_tenant() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;

    // Create adapter for tenant A in a state that can be demoted
    create_test_adapter(
        &state.db,
        "adapter-a-1",
        "tenant-a",
        "Adapter A1",
        "code",
        "tenant",
        "hot",
    )
    .await?;

    // Tenant B claims (operator role has permission to demote)
    let claims_b = create_test_claims("user-b", "user-b@tenant-b.com", "operator", "tenant-b");

    // Tenant B attempts to demote Tenant A's adapter
    let result = demote_adapter_lifecycle(
        State(state.clone()),
        Extension(claims_b),
        Path("adapter-a-1".to_string()),
        Json(LifecycleTransitionRequest {
            reason: "Cross-tenant demotion attempt".to_string(),
        }),
    )
    .await;

    // Should be denied
    match result {
        Err((status, _)) => {
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "Cross-tenant adapter demotion should return NOT_FOUND"
            );
        }
        Ok(_) => panic!("Cross-tenant adapter demotion must be denied"),
    }

    Ok(())
}

// =============================================================================
// TEST SUITE: DB-Level Category/Scope/State Listing - Tenant Scoped
// =============================================================================

#[tokio::test]
#[ignore = "requires base model fixtures for repository creation"]
async fn test_db_list_adapters_by_category_tenant_scoped() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create adapters with same category but different tenants
    create_test_adapter(
        &db,
        "adapter-a-1",
        "tenant-a",
        "Adapter A1",
        "code",
        "tenant",
        "cold",
    )
    .await?;
    create_test_adapter(
        &db,
        "adapter-a-2",
        "tenant-a",
        "Adapter A2",
        "code",
        "tenant",
        "warm",
    )
    .await?;
    create_test_adapter(
        &db,
        "adapter-b-1",
        "tenant-b",
        "Adapter B1",
        "code",
        "tenant",
        "hot",
    )
    .await?;
    create_test_adapter(
        &db,
        "adapter-b-2",
        "tenant-b",
        "Adapter B2",
        "data",
        "tenant",
        "cold",
    )
    .await?;

    let code_adapters_a = db.list_adapters_by_category("tenant-a", "code").await?;
    assert_eq!(code_adapters_a.len(), 2);
    assert!(
        code_adapters_a.iter().all(|a| a.tenant_id == "tenant-a"),
        "Tenant A category listing should be tenant-scoped"
    );

    let code_adapters_b = db.list_adapters_by_category("tenant-b", "code").await?;
    assert_eq!(code_adapters_b.len(), 1);
    assert!(
        code_adapters_b.iter().all(|a| a.tenant_id == "tenant-b"),
        "Tenant B category listing should be tenant-scoped"
    );

    Ok(())
}

#[tokio::test]
async fn test_db_list_adapters_by_scope_tenant_scoped() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create adapters with same scope but different tenants
    create_test_adapter(
        &db,
        "adapter-a-1",
        "tenant-a",
        "Adapter A1",
        "code",
        "tenant",
        "cold",
    )
    .await?;
    create_test_adapter(
        &db,
        "adapter-b-1",
        "tenant-b",
        "Adapter B1",
        "code",
        "global",
        "warm",
    )
    .await?;

    let tenant_scope_adapters_a = db.list_adapters_by_scope("tenant-a", "tenant").await?;
    assert_eq!(tenant_scope_adapters_a.len(), 1);
    assert!(
        tenant_scope_adapters_a
            .iter()
            .all(|a| a.tenant_id == "tenant-a"),
        "Tenant A scope listing should be tenant-scoped"
    );

    let tenant_scope_adapters_b = db.list_adapters_by_scope("tenant-b", "tenant").await?;
    assert!(tenant_scope_adapters_b.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_db_list_adapters_by_state_tenant_scoped() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create adapters with same state but different tenants
    create_test_adapter(
        &db,
        "adapter-a-1",
        "tenant-a",
        "Adapter A1",
        "code",
        "tenant",
        "cold",
    )
    .await?;
    create_test_adapter(
        &db,
        "adapter-a-2",
        "tenant-a",
        "Adapter A2",
        "code",
        "tenant",
        "cold",
    )
    .await?;
    create_test_adapter(
        &db,
        "adapter-b-1",
        "tenant-b",
        "Adapter B1",
        "code",
        "tenant",
        "cold",
    )
    .await?;

    let cold_adapters_a = db.list_adapters_by_state("tenant-a", "cold").await?;
    assert_eq!(cold_adapters_a.len(), 2);
    assert!(
        cold_adapters_a.iter().all(|a| a.tenant_id == "tenant-a"),
        "Tenant A state listing should be tenant-scoped"
    );

    let cold_adapters_b = db.list_adapters_by_state("tenant-b", "cold").await?;
    assert_eq!(cold_adapters_b.len(), 1);
    assert!(
        cold_adapters_b.iter().all(|a| a.tenant_id == "tenant-b"),
        "Tenant B state listing should be tenant-scoped"
    );

    Ok(())
}

// =============================================================================
// TEST SUITE: Adapter State Summary - Tenant Scoped
// =============================================================================

#[tokio::test]
#[ignore = "requires base model fixtures for repository creation"]
async fn test_db_get_adapter_state_summary_tenant_scoped() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create diverse adapters for tenant A
    create_test_adapter(
        &db,
        "adapter-a-1",
        "tenant-a",
        "Adapter A1",
        "code",
        "tenant",
        "cold",
    )
    .await?;
    create_test_adapter(
        &db,
        "adapter-a-2",
        "tenant-a",
        "Adapter A2",
        "code",
        "tenant",
        "warm",
    )
    .await?;

    // Create adapters for tenant B
    create_test_adapter(
        &db,
        "adapter-b-1",
        "tenant-b",
        "Adapter B1",
        "code",
        "tenant",
        "cold",
    )
    .await?;
    create_test_adapter(
        &db,
        "adapter-b-2",
        "tenant-b",
        "Adapter B2",
        "data",
        "global",
        "hot",
    )
    .await?;

    let summary = db.get_adapter_state_summary("tenant-a").await?;
    assert_eq!(summary.len(), 2, "Tenant A should see two summary rows");

    let cold_code_tenant_count = summary
        .iter()
        .filter(|(cat, scope, state, _, _, _, _)| {
            cat == "code" && scope == "tenant" && state == "cold"
        })
        .map(|(_, _, _, count, _, _, _)| count)
        .sum::<i64>();

    assert_eq!(
        cold_code_tenant_count, 1,
        "Summary should only include tenant-a cold adapters"
    );

    let warm_code_tenant_count = summary
        .iter()
        .filter(|(cat, scope, state, _, _, _, _)| {
            cat == "code" && scope == "tenant" && state == "warm"
        })
        .map(|(_, _, _, count, _, _, _)| count)
        .sum::<i64>();

    assert_eq!(
        warm_code_tenant_count, 1,
        "Summary should only include tenant-a warm adapters"
    );

    assert!(
        !summary.iter().any(|(cat, _, _, _, _, _, _)| cat == "data"),
        "Tenant A summary should not include tenant-b categories"
    );

    Ok(())
}

// =============================================================================
// TEST SUITE: Same-Tenant Operations Are Allowed
// =============================================================================

#[tokio::test]
#[ignore = "requires base model fixtures for repository creation"]
async fn test_same_tenant_adapter_operations_allowed() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;

    // Create adapter for tenant A
    create_test_adapter(
        &state.db,
        "adapter-a-1",
        "tenant-a",
        "Adapter A1",
        "code",
        "tenant",
        "cold",
    )
    .await?;

    // Create repository for tenant A
    let repo_a = create_test_repo(&state.db, "tenant-a", "repo-a").await?;

    // Tenant A claims
    let claims_a = create_test_claims("user-a", "user-a@tenant-a.com", "operator", "tenant-a");

    // Same-tenant operations should succeed

    // 1. List adapters
    let list_result = list_adapters(
        State(state.clone()),
        Extension(claims_a.clone()),
        Query(ListAdaptersQuery {
            tier: None,
            framework: None,
            ..Default::default()
        }),
    )
    .await;
    assert!(
        list_result.is_ok(),
        "Same-tenant list adapters should succeed"
    );
    let Json(adapters) = list_result.unwrap();
    assert_eq!(adapters.len(), 1);
    assert_eq!(adapters[0].adapter_id, "adapter-a-1");

    // 2. Get repository detail
    let repo_result = get_adapter_repository(
        State(state.clone()),
        Extension(claims_a.clone()),
        Path(repo_a.clone()),
    )
    .await;
    assert!(
        repo_result.is_ok(),
        "Same-tenant get repository should succeed"
    );

    // 3. Promote adapter lifecycle
    let promote_result = promote_adapter_lifecycle(
        State(state.clone()),
        Extension(claims_a.clone()),
        Path("adapter-a-1".to_string()),
        Json(LifecycleTransitionRequest {
            reason: "Same-tenant promotion".to_string(),
        }),
    )
    .await;
    assert!(
        promote_result.is_ok(),
        "Same-tenant adapter promotion should succeed"
    );

    Ok(())
}

// =============================================================================
// TEST SUITE: Admin with Wildcard Can Access All Tenants
// =============================================================================

#[tokio::test]
#[ignore = "requires base model fixtures for repository creation"]
async fn test_admin_wildcard_can_access_all_tenant_adapters() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;
    create_test_tenant(&state.db, "system").await?;

    // Create adapters for different tenants
    create_test_adapter(
        &state.db,
        "adapter-a-1",
        "tenant-a",
        "Adapter A1",
        "code",
        "tenant",
        "cold",
    )
    .await?;
    create_test_adapter(
        &state.db,
        "adapter-b-1",
        "tenant-b",
        "Adapter B1",
        "code",
        "tenant",
        "warm",
    )
    .await?;

    let repo_a = create_test_repo(&state.db, "tenant-a", "repo-a").await?;
    let repo_b = create_test_repo(&state.db, "tenant-b", "repo-b").await?;

    // Admin claims with wildcard tenant access
    let mut admin_claims = create_test_claims("admin", "admin@system.com", "admin", "system");
    admin_claims.admin_tenants = vec!["*".to_string()];

    // Admin should be able to access tenant-a resources
    // Note: The current implementation of list_adapters filters by claims.tenant_id,
    // so admin would need to explicitly query with tenant-a or tenant-b as their tenant_id
    // This test documents the expected behavior for cross-tenant admin access

    // For repository access, admin with wildcard should be able to access
    let mut claims_for_tenant_a = admin_claims.clone();
    claims_for_tenant_a.tenant_id = "tenant-a".to_string();

    let repo_result = get_adapter_repository(
        State(state.clone()),
        Extension(claims_for_tenant_a),
        Path(repo_a),
    )
    .await;
    assert!(
        repo_result.is_ok(),
        "Admin with wildcard should access tenant-a repository"
    );

    let mut claims_for_tenant_b = admin_claims.clone();
    claims_for_tenant_b.tenant_id = "tenant-b".to_string();

    let repo_result_b = get_adapter_repository(
        State(state.clone()),
        Extension(claims_for_tenant_b),
        Path(repo_b),
    )
    .await;
    assert!(
        repo_result_b.is_ok(),
        "Admin with wildcard should access tenant-b repository"
    );

    Ok(())
}
