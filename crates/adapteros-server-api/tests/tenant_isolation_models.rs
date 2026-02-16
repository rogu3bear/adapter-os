//! Tenant Isolation Tests for Base Model Operations
//!
//! PRD-RECT-001: Cross-tenant denial tests for base model lifecycle operations.
//! These tests verify that:
//! - Tenants cannot list models belonging to other tenants
//! - Tenants cannot access model details for other tenants' models
//! - Model listing API respects tenant boundaries
//! - Global models (tenant_id = NULL) are visible to all tenants
//! - Handler-level validation enforces tenant isolation

use adapteros_core::{AosError, Result};
use adapteros_db::models::ModelRegistrationBuilder;
use adapteros_db::sqlx;
use adapteros_db::Db;
use adapteros_server_api::auth::{AuthMode, Claims, PrincipalType};
use adapteros_server_api::handlers::models::{
    get_model_status, import_model, list_models_with_stats, load_model, unload_model,
    validate_model, ImportModelRequest,
};
use adapteros_server_api::ip_extraction::ClientIp;
use adapteros_server_api::state::AppState;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{Duration, Utc};
mod common;
use common::setup_state;
use uuid::Uuid;

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
        .map_err(|e| AosError::Database(format!("Failed to create tenant: {}", e)))?;
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

/// Test helper to create a tenant-scoped model
async fn create_test_model(
    db: &Db,
    model_id: &str,
    name: &str,
    tenant_id: Option<&str>,
) -> Result<String> {
    let params = ModelRegistrationBuilder::new()
        .name(name)
        .hash_b3(format!("hash_{}", model_id))
        .config_hash_b3(format!("config_hash_{}", model_id))
        .tokenizer_hash_b3(format!("tokenizer_hash_{}", model_id))
        .tokenizer_cfg_hash_b3(format!("tokenizer_cfg_hash_{}", model_id))
        .license_hash_b3(Some(format!("license_hash_{}", model_id)))
        .metadata_json(Some(r#"{"architecture": "transformer"}"#))
        .build()
        .map_err(|e| AosError::Validation(format!("Failed to build model params: {}", e)))?;

    let id = db.register_model(params).await?;

    // Update tenant_id if specified; if None, make the model global (tenant_id = NULL)
    match tenant_id {
        Some(tid) => {
            sqlx::query("UPDATE models SET tenant_id = ? WHERE id = ?")
                .bind(tid)
                .bind(&id)
                .execute(db.pool())
                .await
                .map_err(|e| AosError::Database(format!("Failed to set tenant_id: {}", e)))?;
        }
        None => {
            sqlx::query("UPDATE models SET tenant_id = NULL WHERE id = ?")
                .bind(&id)
                .execute(db.pool())
                .await
                .map_err(|e| AosError::Database(format!("Failed to set tenant_id NULL: {}", e)))?;
        }
    }

    Ok(id)
}

// =============================================================================
// TEST SUITE: Database-Level Model Isolation
// =============================================================================

#[tokio::test]
async fn test_get_model_for_tenant_respects_tenant_isolation() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create tenant-scoped models
    let model_a_id = create_test_model(&db, "model-a", "Model A", Some("tenant-a")).await?;
    let model_b_id = create_test_model(&db, "model-b", "Model B", Some("tenant-b")).await?;

    // Tenant A should see their own model
    let model_a = db.get_model_for_tenant("tenant-a", &model_a_id).await?;
    assert!(model_a.is_some(), "tenant-a should see its own model");
    assert_eq!(
        model_a.unwrap().tenant_id,
        Some("tenant-a".to_string()),
        "Model should belong to tenant-a"
    );

    // Tenant B should NOT see tenant A's model
    let cross_tenant_a = db.get_model_for_tenant("tenant-b", &model_a_id).await?;
    assert!(
        cross_tenant_a.is_none(),
        "tenant-b must not see tenant-a's model"
    );

    // Tenant B should see their own model
    let model_b = db.get_model_for_tenant("tenant-b", &model_b_id).await?;
    assert!(model_b.is_some(), "tenant-b should see its own model");
    assert_eq!(
        model_b.unwrap().tenant_id,
        Some("tenant-b".to_string()),
        "Model should belong to tenant-b"
    );

    // Tenant A should NOT see tenant B's model
    let cross_tenant_b = db.get_model_for_tenant("tenant-a", &model_b_id).await?;
    assert!(
        cross_tenant_b.is_none(),
        "tenant-a must not see tenant-b's model"
    );

    Ok(())
}

#[tokio::test]
async fn test_get_model_by_name_for_tenant_respects_tenant_isolation() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create tenant-scoped models with distinct names
    create_test_model(&db, "model-a-1", "tenant-a-model", Some("tenant-a")).await?;
    create_test_model(&db, "model-b-1", "tenant-b-model", Some("tenant-b")).await?;

    // Set import_status to 'available' for both models so they can be found by name
    sqlx::query("UPDATE models SET import_status = 'available'")
        .execute(db.pool())
        .await?;

    // Tenant A should resolve to their own model and not see tenant-b's
    let model_a = db
        .get_model_by_name_for_tenant("tenant-a", "tenant-a-model")
        .await?;
    assert!(model_a.is_some(), "tenant-a should resolve tenant-a-model");
    assert_eq!(
        model_a.as_ref().unwrap().tenant_id,
        Some("tenant-a".to_string()),
        "Should resolve to tenant-a's model"
    );
    assert!(
        db.get_model_by_name_for_tenant("tenant-a", "tenant-b-model")
            .await?
            .is_none(),
        "tenant-a should not resolve tenant-b's model"
    );

    // Tenant B should resolve to their own model and not see tenant-a's
    let model_b = db
        .get_model_by_name_for_tenant("tenant-b", "tenant-b-model")
        .await?;
    assert!(model_b.is_some(), "tenant-b should resolve tenant-b-model");
    assert_eq!(
        model_b.as_ref().unwrap().tenant_id,
        Some("tenant-b".to_string()),
        "Should resolve to tenant-b's model"
    );
    assert!(
        db.get_model_by_name_for_tenant("tenant-b", "tenant-a-model")
            .await?
            .is_none(),
        "tenant-b should not resolve tenant-a's model"
    );

    Ok(())
}

#[tokio::test]
async fn test_global_models_visible_to_all_tenants() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create a global model (tenant_id = NULL)
    let global_model_id = create_test_model(&db, "global-model", "Global Model", None).await?;

    // Set import_status to 'available' so it can be found by name
    sqlx::query("UPDATE models SET import_status = 'available' WHERE id = ?")
        .bind(&global_model_id)
        .execute(db.pool())
        .await?;

    // Tenant A should see the global model
    let model_a = db
        .get_model_for_tenant("tenant-a", &global_model_id)
        .await?;
    assert!(model_a.is_some(), "tenant-a should see global model");
    assert_eq!(
        model_a.unwrap().tenant_id,
        None,
        "Global model should have NULL tenant_id"
    );

    // Tenant B should also see the global model
    let model_b = db
        .get_model_for_tenant("tenant-b", &global_model_id)
        .await?;
    assert!(model_b.is_some(), "tenant-b should see global model");
    assert_eq!(
        model_b.unwrap().tenant_id,
        None,
        "Global model should have NULL tenant_id"
    );

    // Both tenants should resolve global model by name
    let model_a_by_name = db
        .get_model_by_name_for_tenant("tenant-a", "Global Model")
        .await?;
    assert!(
        model_a_by_name.is_some(),
        "tenant-a should resolve global model by name"
    );

    let model_b_by_name = db
        .get_model_by_name_for_tenant("tenant-b", "Global Model")
        .await?;
    assert!(
        model_b_by_name.is_some(),
        "tenant-b should resolve global model by name"
    );

    Ok(())
}

// =============================================================================
// TEST SUITE: Handler-Level Model Access Control
// =============================================================================

#[tokio::test]
async fn test_load_model_denies_cross_tenant_access() -> Result<()> {
    let state: AppState = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;

    // Create model for tenant-a
    let model_a_id = create_test_model(&state.db, "model-a", "Model A", Some("tenant-a")).await?;

    // Set model_path so validation passes
    sqlx::query(
        "UPDATE models SET model_path = '/tmp/dummy', import_status = 'available' WHERE id = ?",
    )
    .bind(&model_a_id)
    .execute(state.db.pool())
    .await?;

    // Tenant B tries to load tenant A's model
    let claims_b = create_test_claims("user-b", "user-b@tenant-b.com", "operator", "tenant-b");

    let result = load_model(
        State(state.clone()),
        Extension(claims_b),
        Extension(ClientIp("127.0.0.1".to_string())),
        Path(model_a_id.clone()),
    )
    .await;

    // Should return 404 NOT_FOUND (model doesn't exist for tenant-b)
    match result {
        Err(err) => {
            assert_eq!(
                err.status,
                StatusCode::NOT_FOUND,
                "Cross-tenant load should return 404"
            );
        }
        Ok(_) => panic!("Cross-tenant load should be denied"),
    }

    Ok(())
}

#[tokio::test]
async fn test_unload_model_denies_cross_tenant_access() -> Result<()> {
    let state: AppState = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;

    // Create model for tenant-a
    let model_a_id = create_test_model(&state.db, "model-a", "Model A", Some("tenant-a")).await?;

    // Tenant B tries to unload tenant A's model
    let claims_b = create_test_claims("user-b", "user-b@tenant-b.com", "operator", "tenant-b");

    let result = unload_model(
        State(state.clone()),
        Extension(claims_b),
        Extension(ClientIp("127.0.0.1".to_string())),
        Path(model_a_id.clone()),
    )
    .await;

    // Should return 404 NOT_FOUND
    match result {
        Err((status, _)) => {
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "Cross-tenant unload should return 404"
            );
        }
        Ok(_) => panic!("Cross-tenant unload should be denied"),
    }

    Ok(())
}

#[tokio::test]
async fn test_get_model_status_denies_cross_tenant_access() -> Result<()> {
    let state: AppState = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;

    // Create model for tenant-a
    let model_a_id = create_test_model(&state.db, "model-a", "Model A", Some("tenant-a")).await?;

    // Tenant B tries to get status of tenant A's model
    let claims_b = create_test_claims("user-b", "user-b@tenant-b.com", "viewer", "tenant-b");

    let result = get_model_status(
        State(state.clone()),
        Extension(claims_b),
        Path(model_a_id.clone()),
    )
    .await;

    // Should return 404 NOT_FOUND
    match result {
        Err((status, _)) => {
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "Cross-tenant status check should return 404"
            );
        }
        Ok(_) => panic!("Cross-tenant status check should be denied"),
    }

    Ok(())
}

#[tokio::test]
async fn test_validate_model_denies_cross_tenant_access() -> Result<()> {
    let state: AppState = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;

    // Create model for tenant-a
    let model_a_id = create_test_model(&state.db, "model-a", "Model A", Some("tenant-a")).await?;

    // Tenant B tries to validate tenant A's model
    let claims_b = create_test_claims("user-b", "user-b@tenant-b.com", "viewer", "tenant-b");

    let result = validate_model(
        State(state.clone()),
        Extension(claims_b),
        Path(model_a_id.clone()),
    )
    .await;

    // Should return 404 NOT_FOUND
    match result {
        Err((status, _)) => {
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "Cross-tenant validation should return 404"
            );
        }
        Ok(_) => panic!("Cross-tenant validation should be denied"),
    }

    Ok(())
}

#[tokio::test]
async fn test_import_model_scoped_to_tenant() -> Result<()> {
    let state: AppState = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;

    // Use a subdirectory within the resolved model cache root to satisfy security checks
    let model_cache_root = adapteros_config::resolve_base_model_location(None, None, false)
        .map(|loc| loc.cache_root)
        .unwrap_or_else(|_| std::path::PathBuf::from("var/models"));

    std::fs::create_dir_all(&model_cache_root).unwrap();

    let test_model_dir = model_cache_root.join(format!("test-import-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&test_model_dir).unwrap();

    // Create required mock files for validation
    std::fs::write(test_model_dir.join("config.json"), "{}").unwrap();
    std::fs::write(test_model_dir.join("tokenizer.json"), "{}").unwrap();
    std::fs::write(test_model_dir.join("model.safetensors"), "").unwrap();

    // Tenant A imports a model
    let claims_a = create_test_claims("user-a", "user-a@tenant-a.com", "operator", "tenant-a");

    let result = import_model(
        State(state.clone()),
        Extension(claims_a.clone()),
        Extension(ClientIp("127.0.0.1".to_string())),
        Json(ImportModelRequest {
            model_name: "test-model".to_string(),
            model_path: test_model_dir.to_string_lossy().to_string(),
            format: "mlx".to_string(),
            backend: "mlx".to_string(),
            capabilities: Some(vec!["chat".to_string()]),
            metadata: None,
        }),
    )
    .await;

    assert!(result.is_ok(), "Import should succeed");
    let import_id = result.unwrap().0.import_id;

    // Verify model belongs to tenant-a
    let model = state.db.get_model(&import_id).await?.unwrap();
    assert_eq!(
        model.tenant_id,
        Some("tenant-a".to_string()),
        "Imported model should belong to tenant-a"
    );

    // Tenant B should NOT see this model
    let cross_tenant = state
        .db
        .get_model_for_tenant("tenant-b", &import_id)
        .await?;
    assert!(
        cross_tenant.is_none(),
        "tenant-b should not see tenant-a's imported model"
    );

    // Cleanup
    std::fs::remove_dir_all(&test_model_dir).ok();

    Ok(())
}

#[tokio::test]
async fn test_list_models_respects_tenant_boundaries() -> Result<()> {
    let state: AppState = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;

    // Create models for each tenant
    create_test_model(&state.db, "model-a-1", "Model A1", Some("tenant-a")).await?;
    create_test_model(&state.db, "model-a-2", "Model A2", Some("tenant-a")).await?;
    create_test_model(&state.db, "model-b-1", "Model B1", Some("tenant-b")).await?;
    create_test_model(&state.db, "model-global", "Global Model", None).await?;

    // Tenant A lists models
    let claims_a = create_test_claims("user-a", "user-a@tenant-a.com", "viewer", "tenant-a");
    let result_a = list_models_with_stats(State(state.clone()), Extension(claims_a))
        .await
        .unwrap()
        .0;

    // Filter to tenant A's models and global models
    let tenant_a_models: Vec<_> = result_a
        .models
        .iter()
        .filter(|m| m.tenant_id.as_deref() == Some("tenant-a") || m.tenant_id.is_none())
        .collect();

    // Should see 2 own models + 1 global
    assert!(
        tenant_a_models.len() >= 3,
        "tenant-a should see at least 3 models (2 own + 1 global)"
    );

    // Should NOT see tenant B's model
    let has_tenant_b_model = result_a
        .models
        .iter()
        .any(|m| m.tenant_id.as_deref() == Some("tenant-b"));
    assert!(
        !has_tenant_b_model,
        "tenant-a should not see tenant-b's models"
    );

    // Tenant B lists models
    let claims_b = create_test_claims("user-b", "user-b@tenant-b.com", "viewer", "tenant-b");
    let result_b = list_models_with_stats(State(state.clone()), Extension(claims_b))
        .await
        .unwrap()
        .0;

    // Should NOT see tenant A's models
    let has_tenant_a_model = result_b
        .models
        .iter()
        .any(|m| m.tenant_id.as_deref() == Some("tenant-a"));
    assert!(
        !has_tenant_a_model,
        "tenant-b should not see tenant-a's models"
    );

    Ok(())
}

// =============================================================================
// TEST SUITE: Global Models Accessible to All Tenants
// =============================================================================

#[tokio::test]
async fn test_global_model_accessible_to_all_tenants_via_handlers() -> Result<()> {
    let state: AppState = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;

    // Create a global model (tenant_id = NULL)
    let global_model_id = create_test_model(&state.db, "global", "Global Model", None).await?;

    // Tenant A can get status of global model
    let claims_a = create_test_claims("user-a", "user-a@tenant-a.com", "viewer", "tenant-a");
    let result_a = get_model_status(
        State(state.clone()),
        Extension(claims_a),
        Path(global_model_id.clone()),
    )
    .await;
    assert!(
        result_a.is_ok(),
        "tenant-a should access global model status"
    );

    // Tenant B can also get status of global model
    let claims_b = create_test_claims("user-b", "user-b@tenant-b.com", "viewer", "tenant-b");
    let result_b = get_model_status(
        State(state.clone()),
        Extension(claims_b),
        Path(global_model_id.clone()),
    )
    .await;
    assert!(
        result_b.is_ok(),
        "tenant-b should access global model status"
    );

    Ok(())
}

// =============================================================================
// TEST SUITE: Multiple Tenants with Same Model Names
// =============================================================================

#[tokio::test]
async fn test_tenants_resolve_models_by_name_with_isolation() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;
    create_test_tenant(&db, "tenant-c").await?;

    // Create models with distinct names across tenants
    let model_a_id = create_test_model(&db, "model-a", "tenant-a-model", Some("tenant-a")).await?;
    let model_b_id = create_test_model(&db, "model-b", "tenant-b-model", Some("tenant-b")).await?;
    let model_c_id = create_test_model(&db, "model-c", "tenant-c-model", Some("tenant-c")).await?;

    // Set all to available
    sqlx::query("UPDATE models SET import_status = 'available'")
        .execute(db.pool())
        .await?;

    // Each tenant should resolve to their own model
    let resolved_a = db
        .get_model_by_name_for_tenant("tenant-a", "tenant-a-model")
        .await?
        .unwrap();
    assert_eq!(
        resolved_a.id, model_a_id,
        "tenant-a should resolve to its own model"
    );
    assert!(db
        .get_model_by_name_for_tenant("tenant-a", "tenant-b-model")
        .await?
        .is_none());

    let resolved_b = db
        .get_model_by_name_for_tenant("tenant-b", "tenant-b-model")
        .await?
        .unwrap();
    assert_eq!(
        resolved_b.id, model_b_id,
        "tenant-b should resolve to its own model"
    );
    assert!(db
        .get_model_by_name_for_tenant("tenant-b", "tenant-c-model")
        .await?
        .is_none());

    let resolved_c = db
        .get_model_by_name_for_tenant("tenant-c", "tenant-c-model")
        .await?
        .unwrap();
    assert_eq!(
        resolved_c.id, model_c_id,
        "tenant-c should resolve to its own model"
    );
    assert!(db
        .get_model_by_name_for_tenant("tenant-c", "tenant-a-model")
        .await?
        .is_none());

    Ok(())
}

// =============================================================================
// TEST SUITE: Admin Cross-Tenant Access
// =============================================================================

#[tokio::test]
async fn test_admin_listing_scoped_to_primary_tenant() -> Result<()> {
    let state: AppState = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "system").await?;
    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;

    // Create models for different tenants
    let model_a_id = create_test_model(&state.db, "model-a", "Model A", Some("tenant-a")).await?;
    let model_b_id = create_test_model(&state.db, "model-b", "Model B", Some("tenant-b")).await?;

    // Admin scoped to tenant-a; list_models_with_stats filters by primary tenant_id
    let mut admin_claims = create_test_claims("admin", "admin@tenant-a.com", "admin", "tenant-a");
    admin_claims.admin_tenants = vec!["*".to_string()]; // wildcard currently unused by handler

    // Admin can list models for their primary tenant
    let result = list_models_with_stats(State(state.clone()), Extension(admin_claims.clone()))
        .await
        .unwrap()
        .0;

    let has_model_a = result.models.iter().any(|m| m.id == model_a_id);
    let has_model_b = result.models.iter().any(|m| m.id == model_b_id);

    assert!(has_model_a, "Admin should see tenant-a's model");
    assert!(
        !has_model_b,
        "Admin listing is scoped to primary tenant; should not see tenant-b's model"
    );

    Ok(())
}

// =============================================================================
// TEST SUITE: Edge Cases
// =============================================================================

#[tokio::test]
async fn test_empty_tenant_sees_only_global_models() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-empty").await?;
    create_test_tenant(&db, "tenant-a").await?;

    // Create model for tenant-a
    create_test_model(&db, "model-a", "Model A", Some("tenant-a")).await?;

    // Create global model
    let global_id = create_test_model(&db, "global", "Global", None).await?;

    // Set to available
    sqlx::query("UPDATE models SET import_status = 'available'")
        .execute(db.pool())
        .await?;

    // Empty tenant should only see global model
    let empty_model_by_id = db.get_model_for_tenant("tenant-empty", &global_id).await?;
    assert!(
        empty_model_by_id.is_some(),
        "Empty tenant should see global model"
    );

    let empty_model_by_name = db
        .get_model_by_name_for_tenant("tenant-empty", "Global")
        .await?;
    assert!(
        empty_model_by_name.is_some(),
        "Empty tenant should resolve global model by name"
    );

    // Empty tenant should NOT see tenant-a's model by name
    let cross_tenant = db
        .get_model_by_name_for_tenant("tenant-empty", "Model A")
        .await?;
    assert!(
        cross_tenant.is_none(),
        "Empty tenant should not see other tenant's model"
    );

    Ok(())
}

#[tokio::test]
async fn test_model_with_null_tenant_id_treated_as_global() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Explicitly create model with NULL tenant_id
    let params = ModelRegistrationBuilder::new()
        .name("null-tenant-model")
        .hash_b3("hash-null")
        .config_hash_b3("config-hash-null")
        .tokenizer_hash_b3("tokenizer-hash-null")
        .tokenizer_cfg_hash_b3("tokenizer-cfg-hash-null")
        .build()
        .unwrap();

    let null_model_id = db.register_model(params).await?;

    // Make the model explicitly global
    sqlx::query("UPDATE models SET tenant_id = NULL WHERE id = ?")
        .bind(&null_model_id)
        .execute(db.pool())
        .await?;

    // Verify tenant_id is NULL
    let model = db.get_model(&null_model_id).await?.unwrap();
    assert_eq!(model.tenant_id, None, "Model should have NULL tenant_id");

    // Both tenants should see this model
    let seen_by_a = db.get_model_for_tenant("tenant-a", &null_model_id).await?;
    assert!(seen_by_a.is_some(), "tenant-a should see NULL-tenant model");

    let seen_by_b = db.get_model_for_tenant("tenant-b", &null_model_id).await?;
    assert!(seen_by_b.is_some(), "tenant-b should see NULL-tenant model");

    Ok(())
}

// =============================================================================
// TEST SUITE: Viewer Role Cannot Modify Models
// =============================================================================

#[tokio::test]
async fn test_viewer_cannot_load_model() -> Result<()> {
    let state: AppState = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;

    let model_id = create_test_model(&state.db, "model", "Model", Some("tenant-a")).await?;

    // Viewer tries to load model
    let claims = create_test_claims("viewer", "viewer@tenant-a.com", "viewer", "tenant-a");

    let result = load_model(
        State(state.clone()),
        Extension(claims),
        Extension(ClientIp("127.0.0.1".to_string())),
        Path(model_id),
    )
    .await;

    // Should return 403 FORBIDDEN (no permission)
    match result {
        Err(err) => {
            assert_eq!(
                err.status,
                StatusCode::FORBIDDEN,
                "Viewer should not have permission to load model"
            );
        }
        Ok(_) => panic!("Viewer should not be able to load model"),
    }

    Ok(())
}

#[tokio::test]
async fn test_viewer_cannot_import_model() -> Result<()> {
    let state: AppState = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;

    let temp_dir = std::env::temp_dir().join(format!("aos-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    // Viewer tries to import model
    let claims = create_test_claims("viewer", "viewer@tenant-a.com", "viewer", "tenant-a");

    let result = import_model(
        State(state.clone()),
        Extension(claims),
        Extension(ClientIp("127.0.0.1".to_string())),
        Json(ImportModelRequest {
            model_name: "test".to_string(),
            model_path: temp_dir.to_string_lossy().to_string(),
            format: "mlx".to_string(),
            backend: "mlx".to_string(),
            capabilities: None,
            metadata: None,
        }),
    )
    .await;

    // Should return 403 FORBIDDEN
    match result {
        Err((status, _)) => {
            assert_eq!(
                status,
                StatusCode::FORBIDDEN,
                "Viewer should not have permission to import model"
            );
        }
        Ok(_) => panic!("Viewer should not be able to import model"),
    }

    std::fs::remove_dir_all(&temp_dir).ok();

    Ok(())
}

// =============================================================================
// TEST SUITE: Handler-Level Model Status Tenant Isolation (PRD-RECT-002)
// =============================================================================

#[tokio::test]
async fn test_get_all_models_status_cross_tenant_filtered() -> Result<()> {
    use adapteros_server_api::handlers::models::get_all_models_status;
    use axum::extract::Query;
    use std::collections::HashMap;

    let state: AppState = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;

    // Create models for each tenant
    let model_a_id = create_test_model(&state.db, "model-a", "Model A", Some("tenant-a")).await?;
    let model_b_id = create_test_model(&state.db, "model-b", "Model B", Some("tenant-b")).await?;

    // Set both models to available
    sqlx::query("UPDATE models SET import_status = 'available'")
        .execute(state.db.pool())
        .await?;

    // Create base_model_status records for both tenants
    sqlx::query(
        "INSERT INTO base_model_status (tenant_id, model_id, status, updated_at) VALUES (?, ?, 'loaded', datetime('now'))",
    )
    .bind("tenant-a")
    .bind(&model_a_id)
    .execute(state.db.pool())
    .await?;

    sqlx::query(
        "INSERT INTO base_model_status (tenant_id, model_id, status, updated_at) VALUES (?, ?, 'loaded', datetime('now'))",
    )
    .bind("tenant-b")
    .bind(&model_b_id)
    .execute(state.db.pool())
    .await?;

    // Non-admin from tenant-a should only see tenant-a's model status
    let claims_a = create_test_claims("user-a", "user-a@tenant-a.com", "operator", "tenant-a");
    let empty_query: HashMap<String, String> = HashMap::new();

    let result = get_all_models_status(
        State(state.clone()),
        Extension(claims_a),
        Query(empty_query),
    )
    .await;

    assert!(result.is_ok(), "get_all_models_status should succeed");
    let statuses = result.unwrap().0;

    // Should only see tenant-a's model status
    let has_tenant_a = statuses.models.iter().any(|s| s.model_id == model_a_id);
    let has_tenant_b = statuses.models.iter().any(|s| s.model_id == model_b_id);

    assert!(has_tenant_a, "tenant-a should see their own model status");
    assert!(
        !has_tenant_b,
        "tenant-a should NOT see tenant-b's model status"
    );

    Ok(())
}

#[tokio::test]
async fn test_get_base_model_status_cross_tenant_denied() -> Result<()> {
    use adapteros_server_api::handlers::{get_base_model_status, ListJobsQuery};
    use axum::extract::Query;

    let state: AppState = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "tenant-a").await?;
    create_test_tenant(&state.db, "tenant-b").await?;

    // Create model and status for tenant-b
    let model_b_id = create_test_model(&state.db, "model-b", "Model B", Some("tenant-b")).await?;

    // Set model to available
    sqlx::query(
        "UPDATE models SET import_status = 'available', model_path = '/tmp/dummy' WHERE id = ?",
    )
    .bind(&model_b_id)
    .execute(state.db.pool())
    .await?;

    // Create base_model_status for tenant-b
    sqlx::query(
        "INSERT INTO base_model_status (tenant_id, model_id, status, updated_at) VALUES (?, ?, 'loaded', datetime('now'))",
    )
    .bind("tenant-b")
    .bind(&model_b_id)
    .execute(state.db.pool())
    .await?;

    // Non-admin from tenant-a tries to query tenant-b's base model status
    let claims_a = create_test_claims("user-a", "user-a@tenant-a.com", "operator", "tenant-a");

    let result = get_base_model_status(
        State(state.clone()),
        Some(Extension(claims_a)),
        Query(ListJobsQuery {
            tenant_id: Some("tenant-b".to_string()),
        }),
    )
    .await;

    // Should return 404 NOT_FOUND (to prevent enumeration)
    match result {
        Err((status, _)) => {
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "Cross-tenant base model status query should return 404"
            );
        }
        Ok(_) => panic!("Non-admin should not access another tenant's base model status"),
    }

    Ok(())
}

#[tokio::test]
async fn test_admin_with_admin_tenants_can_access_cross_tenant_model_status() -> Result<()> {
    use adapteros_server_api::handlers::{get_base_model_status, ListJobsQuery};
    use axum::extract::Query;

    let state: AppState = setup_state(None).await.expect("state");

    create_test_tenant(&state.db, "system").await?;
    create_test_tenant(&state.db, "tenant-a").await?;

    // Create model and status for tenant-a
    let model_a_id = create_test_model(&state.db, "model-a", "Model A", Some("tenant-a")).await?;

    // Set model to available
    sqlx::query(
        "UPDATE models SET import_status = 'available', model_path = '/tmp/dummy' WHERE id = ?",
    )
    .bind(&model_a_id)
    .execute(state.db.pool())
    .await?;

    // Create base_model_status for tenant-a
    sqlx::query(
        "INSERT INTO base_model_status (tenant_id, model_id, status, updated_at) VALUES (?, ?, 'loaded', datetime('now'))",
    )
    .bind("tenant-a")
    .bind(&model_a_id)
    .execute(state.db.pool())
    .await?;

    // Admin from system with admin_tenants grant for tenant-a
    let mut admin_claims = create_test_claims("admin", "admin@system.com", "admin", "system");
    admin_claims.admin_tenants = vec!["tenant-a".to_string()];

    let result = get_base_model_status(
        State(state.clone()),
        Some(Extension(admin_claims)),
        Query(ListJobsQuery {
            tenant_id: Some("tenant-a".to_string()),
        }),
    )
    .await;

    // Admin with proper grant should succeed
    assert!(
        result.is_ok(),
        "Admin with admin_tenants grant should access cross-tenant model status"
    );

    Ok(())
}
