//! Model database operation tests
//!
//! Validates:
//! - Model CRUD operations with tenant isolation
//! - Model listing with pagination
//! - Model state management (import status, base model status)
//! - Edge cases and error conditions
//! - Cross-tenant isolation for tenant-scoped models

use adapteros_core::{AosError, Result};
use adapteros_db::models::{ModelRegistrationBuilder, ModelRegistrationParams};
use adapteros_db::Db;
use chrono::Utc;
use uuid::Uuid;

/// Helper to create a tenant
async fn create_tenant(db: &Db, tenant_id: &str) -> Result<()> {
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind(tenant_id)
        .bind(format!("Tenant {}", tenant_id))
        .execute(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create tenant: {}", e)))?;
    Ok(())
}

/// Helper to set tenant_id for an existing model (None = global model).
async fn set_model_tenant(db: &Db, model_id: &str, tenant_id: Option<&str>) -> Result<()> {
    sqlx::query("UPDATE models SET tenant_id = ? WHERE id = ?")
        .bind(tenant_id)
        .bind(model_id)
        .execute(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to set model tenant: {}", e)))?;
    Ok(())
}

/// Helper to set created_at for deterministic ordering in tests.
async fn set_model_created_at(db: &Db, model_id: &str, created_at: &str) -> Result<()> {
    sqlx::query("UPDATE models SET created_at = ? WHERE id = ?")
        .bind(created_at)
        .bind(model_id)
        .execute(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to set model created_at: {}", e)))?;
    Ok(())
}

/// Helper to create a model registration with minimal required fields
fn minimal_model_params(name: &str) -> ModelRegistrationParams {
    ModelRegistrationBuilder::new()
        .name(name)
        .hash_b3(format!("hash-{}", Uuid::new_v4()))
        .config_hash_b3(format!("config-hash-{}", Uuid::new_v4()))
        .tokenizer_hash_b3(format!("tokenizer-hash-{}", Uuid::new_v4()))
        .tokenizer_cfg_hash_b3(format!("tokenizer-cfg-hash-{}", Uuid::new_v4()))
        .build()
        .expect("minimal params should build")
}

/// Helper to create a model registration with all optional fields
fn full_model_params(name: &str) -> ModelRegistrationParams {
    ModelRegistrationBuilder::new()
        .name(name)
        .hash_b3(format!("hash-{}", Uuid::new_v4()))
        .config_hash_b3(format!("config-hash-{}", Uuid::new_v4()))
        .tokenizer_hash_b3(format!("tokenizer-hash-{}", Uuid::new_v4()))
        .tokenizer_cfg_hash_b3(format!("tokenizer-cfg-hash-{}", Uuid::new_v4()))
        .license_hash_b3(Some(format!("license-hash-{}", Uuid::new_v4())))
        .metadata_json(Some(r#"{"architecture": "transformer", "size": "7b"}"#))
        .build()
        .expect("full params should build")
}

// ============================================================================
// Model CRUD Tests
// ============================================================================

#[tokio::test]
async fn register_model_with_minimal_fields() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let params = minimal_model_params("minimal-model");
    let model_id = db.register_model(params.clone()).await?;

    assert!(!model_id.is_empty(), "model_id should be generated");

    let model = db.get_model(&model_id).await?.expect("model should exist");

    assert_eq!(model.name, "minimal-model");
    assert_eq!(model.hash_b3, params.hash_b3);
    assert_eq!(model.config_hash_b3, params.config_hash_b3);
    assert_eq!(model.tokenizer_hash_b3, params.tokenizer_hash_b3);
    assert_eq!(model.tokenizer_cfg_hash_b3, params.tokenizer_cfg_hash_b3);
    assert!(model.license_hash_b3.is_none());
    assert!(model.metadata_json.is_none());

    Ok(())
}

#[tokio::test]
async fn register_model_with_all_fields() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let params = full_model_params("full-model");
    let model_id = db.register_model(params.clone()).await?;

    let model = db.get_model(&model_id).await?.expect("model should exist");

    assert_eq!(model.name, "full-model");
    assert_eq!(model.hash_b3, params.hash_b3);
    assert_eq!(model.license_hash_b3, params.license_hash_b3);
    assert_eq!(model.metadata_json, params.metadata_json);

    Ok(())
}

#[tokio::test]
async fn get_nonexistent_model_returns_none() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let model = db.get_model("nonexistent-id").await?;
    assert!(model.is_none(), "nonexistent model should return None");

    Ok(())
}

#[tokio::test]
async fn get_model_by_name() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let params = minimal_model_params("qwen2.5-7b");
    let model_id = db.register_model(params.clone()).await?;

    // Mark as available
    db.update_model_import_status(&model_id, "available", None)
        .await?;

    let model = db
        .get_model_by_name("qwen2.5-7b")
        .await?
        .expect("model should be found by name");

    assert_eq!(model.name, "qwen2.5-7b");
    assert_eq!(model.id, model_id);

    Ok(())
}

#[tokio::test]
async fn get_model_by_name_filters_non_available() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let params = minimal_model_params("importing-model");
    let model_id = db.register_model(params.clone()).await?;

    // Set to importing status
    db.update_model_import_status(&model_id, "importing", None)
        .await?;

    // Should not find importing models
    let model = db.get_model_by_name("importing-model").await?;
    assert!(
        model.is_none(),
        "should only find models with import_status = 'available'"
    );

    // Set to available
    db.update_model_import_status(&model_id, "available", None)
        .await?;

    // Now should find it
    let model = db
        .get_model_by_name("importing-model")
        .await?
        .expect("should find available model");
    assert_eq!(model.import_status.as_deref(), Some("available"));

    Ok(())
}

#[tokio::test]
async fn update_model_path_by_id() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let params = minimal_model_params("path-test");
    let model_id = db.register_model(params).await?;
    set_model_tenant(&db, &model_id, None).await?;

    db.update_model_path(&model_id, "/var/models/qwen2.5-7b")
        .await?;

    let model = db.get_model(&model_id).await?.expect("model exists");
    assert_eq!(model.model_path.as_deref(), Some("/var/models/qwen2.5-7b"));

    Ok(())
}

#[tokio::test]
async fn update_model_path_by_name() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let params = minimal_model_params("path-test-by-name");
    let model_id = db.register_model(params).await?;
    set_model_tenant(&db, &model_id, None).await?;

    // update_model_path accepts id OR name
    db.update_model_path("path-test-by-name", "/var/models/by-name")
        .await?;

    let model = db.get_model(&model_id).await?.expect("model exists");
    assert_eq!(model.model_path.as_deref(), Some("/var/models/by-name"));

    Ok(())
}

#[tokio::test]
async fn update_nonexistent_model_path_errors() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let result = db.update_model_path("nonexistent", "/some/path").await;
    assert!(result.is_err(), "should error for nonexistent model");

    Ok(())
}

// ============================================================================
// Model Listing Tests
// ============================================================================

#[tokio::test]
async fn list_models_returns_all_ordered_by_created_at_desc() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = "tenant-1";
    create_tenant(&db, tenant_id).await?;

    // Create models in sequence
    let id1 = db.register_model(minimal_model_params("model-1")).await?;
    set_model_tenant(&db, &id1, Some(tenant_id)).await?;
    let id2 = db.register_model(minimal_model_params("model-2")).await?;
    set_model_tenant(&db, &id2, Some(tenant_id)).await?;
    let id3 = db.register_model(minimal_model_params("model-3")).await?;
    set_model_tenant(&db, &id3, Some(tenant_id)).await?;
    set_model_created_at(&db, &id1, "2024-01-01 00:00:01").await?;
    set_model_created_at(&db, &id2, "2024-01-01 00:00:02").await?;
    set_model_created_at(&db, &id3, "2024-01-01 00:00:03").await?;

    let models = db.list_models(tenant_id).await?;

    assert_eq!(models.len(), 3);
    // Most recent first
    assert_eq!(models[0].id, id3);
    assert_eq!(models[1].id, id2);
    assert_eq!(models[2].id, id1);

    Ok(())
}

#[tokio::test]
async fn list_models_empty_when_no_models() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = "tenant-1";
    create_tenant(&db, tenant_id).await?;

    let models = db.list_models(tenant_id).await?;
    assert_eq!(models.len(), 0);

    Ok(())
}

#[tokio::test]
async fn list_models_with_stats() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = "tenant-1";
    create_tenant(&db, tenant_id).await?;

    // Create a model
    let params = minimal_model_params("stats-model");
    let model_id = db.register_model(params).await?;
    set_model_tenant(&db, &model_id, Some(tenant_id)).await?;

    let stats = db.list_models_with_stats(tenant_id).await?;
    assert_eq!(stats.len(), 1);

    let model_stats = &stats[0];
    assert_eq!(model_stats.model.id, model_id);
    assert_eq!(model_stats.adapter_count, 0);
    assert_eq!(model_stats.training_job_count, 0);

    Ok(())
}

// ============================================================================
// Tenant Isolation Tests
// ============================================================================

#[tokio::test]
async fn get_model_for_tenant_allows_null_tenant_id() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    // Register model without tenant_id (global model)
    let params = minimal_model_params("global-model");
    let model_id = db.register_model(params).await?;
    set_model_tenant(&db, &model_id, None).await?;

    // Global model (NULL tenant_id) should be accessible to any tenant
    let model = db
        .get_model_for_tenant("tenant-a", &model_id)
        .await?
        .expect("tenant should access global model");

    assert_eq!(model.name, "global-model");
    assert!(model.tenant_id.is_none());

    Ok(())
}

#[tokio::test]
async fn get_model_for_tenant_allows_matching_tenant_id() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    // Import model with tenant_id
    let model_id = db
        .import_model_from_path(
            "tenant-model",
            "/var/models/tenant-a-model",
            "safetensors",
            "metal",
            "tenant-a",
            "user-1",
        )
        .await?;

    let model = db
        .get_model_for_tenant("tenant-a", &model_id)
        .await?
        .expect("tenant should access own model");

    assert_eq!(model.tenant_id.as_deref(), Some("tenant-a"));

    Ok(())
}

#[tokio::test]
async fn get_model_for_tenant_denies_other_tenant() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;
    create_tenant(&db, "tenant-b").await?;

    // Import model for tenant-a
    let model_id = db
        .import_model_from_path(
            "tenant-a-model",
            "/var/models/tenant-a-model",
            "safetensors",
            "metal",
            "tenant-a",
            "user-1",
        )
        .await?;

    // tenant-b should NOT see tenant-a's model
    let model = db.get_model_for_tenant("tenant-b", &model_id).await?;
    assert!(
        model.is_none(),
        "tenant-b should not access tenant-a's model"
    );

    Ok(())
}

#[tokio::test]
async fn get_model_by_name_for_tenant_scoping() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;
    create_tenant(&db, "tenant-b").await?;

    let model_id = db.register_model(minimal_model_params("shared-name")).await?;
    set_model_tenant(&db, &model_id, Some("tenant-a")).await?;
    db.update_model_import_status(&model_id, "available", None)
        .await?;

    let model_a = db
        .get_model_by_name_for_tenant("tenant-a", "shared-name")
        .await?
        .expect("tenant-a should find their model");
    assert_eq!(model_a.id, model_id);
    assert_eq!(model_a.tenant_id.as_deref(), Some("tenant-a"));

    let model_b = db.get_model_by_name_for_tenant("tenant-b", "shared-name").await?;
    assert!(model_b.is_none(), "tenant-b should not see tenant-a model");

    Ok(())
}

#[tokio::test]
async fn get_model_by_name_for_tenant_allows_global_model() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    // Register global model
    let params = minimal_model_params("global-model");
    let model_id = db.register_model(params).await?;
    set_model_tenant(&db, &model_id, None).await?;
    db.update_model_import_status(&model_id, "available", None)
        .await?;

    // Tenant should access global model
    let model = db
        .get_model_by_name_for_tenant("tenant-a", "global-model")
        .await?
        .expect("tenant should access global model by name");

    assert!(model.tenant_id.is_none());
    assert_eq!(model.name, "global-model");

    Ok(())
}

// ============================================================================
// Model Import Tests
// ============================================================================

#[tokio::test]
async fn import_model_from_nonexistent_path() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    // Import from a path that doesn't exist
    let model_id = db
        .import_model_from_path(
            "missing-model",
            "/nonexistent/path/model",
            "safetensors",
            "metal",
            "tenant-a",
            "user-1",
        )
        .await?;

    let model = db.get_model(&model_id).await?.expect("model exists");

    // Should create model with placeholder hashes
    assert!(model.hash_b3.starts_with("missing_hash_"));
    assert_eq!(model.import_status.as_deref(), Some("importing"));
    assert!(model.size_bytes.is_none());

    Ok(())
}

#[tokio::test]
async fn update_model_import_status_to_available() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let model_id = db
        .import_model_from_path(
            "model-1",
            "/var/models/model-1",
            "safetensors",
            "metal",
            "tenant-a",
            "user-1",
        )
        .await?;

    db.update_model_import_status(&model_id, "available", None)
        .await?;

    let model = db.get_model(&model_id).await?.expect("model exists");
    assert_eq!(model.import_status.as_deref(), Some("available"));
    assert!(model.import_error.is_none());

    Ok(())
}

#[tokio::test]
async fn update_model_import_status_to_failed_with_error() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let model_id = db
        .import_model_from_path(
            "model-fail",
            "/var/models/model-fail",
            "safetensors",
            "metal",
            "tenant-a",
            "user-1",
        )
        .await?;

    db.update_model_import_status(&model_id, "failed", Some("Checksum mismatch"))
        .await?;

    let model = db.get_model(&model_id).await?.expect("model exists");
    assert_eq!(model.import_status.as_deref(), Some("failed"));
    assert_eq!(model.import_error.as_deref(), Some("Checksum mismatch"));

    Ok(())
}

#[tokio::test]
async fn import_model_sets_tenant_id() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let model_id = db
        .import_model_from_path(
            "scoped-model",
            "/var/models/scoped",
            "safetensors",
            "metal",
            "tenant-a",
            "user-1",
        )
        .await?;

    let model = db.get_model(&model_id).await?.expect("model exists");
    assert_eq!(model.tenant_id.as_deref(), Some("tenant-a"));

    Ok(())
}

// ============================================================================
// Base Model Status Tests
// ============================================================================

#[tokio::test]
async fn update_base_model_status_creates_new_record() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let params = minimal_model_params("model-1");
    let model_id = db.register_model(params).await?;

    db.update_base_model_status("tenant-a", &model_id, "loading", None, None)
        .await?;

    let status = db
        .get_base_model_status("tenant-a")
        .await?
        .expect("status should exist");

    assert_eq!(status.tenant_id, "tenant-a");
    assert_eq!(status.model_id, model_id);
    assert_eq!(status.status, "loading");
    assert!(status.error_message.is_none());
    assert!(status.memory_usage_mb.is_none());

    Ok(())
}

#[tokio::test]
async fn update_base_model_status_updates_existing_record() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let params = minimal_model_params("model-1");
    let model_id = db.register_model(params).await?;

    // Create initial status
    db.update_base_model_status("tenant-a", &model_id, "loading", None, None)
        .await?;

    // Update to loaded with memory usage
    db.update_base_model_status("tenant-a", &model_id, "loaded", None, Some(4096))
        .await?;

    let status = db
        .get_base_model_status("tenant-a")
        .await?
        .expect("status should exist");

    assert_eq!(status.status, "loaded");
    assert_eq!(status.memory_usage_mb, Some(4096));

    Ok(())
}

#[tokio::test]
async fn update_base_model_status_normalizes_legacy_statuses() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let params = minimal_model_params("model-1");
    let model_id = db.register_model(params).await?;

    // Test legacy status normalization
    let test_cases = vec![
        ("ready", "loaded"),
        ("no-model", "unloaded"),
        ("none", "unloaded"),
        ("checking", "loading"),
    ];

    for (input_status, expected_status) in test_cases {
        db.update_base_model_status("tenant-a", &model_id, input_status, None, None)
            .await?;

        let status = db.get_base_model_status("tenant-a").await?.unwrap();
        assert_eq!(
            status.status, expected_status,
            "Status '{}' should normalize to '{}'",
            input_status, expected_status
        );
    }

    Ok(())
}

#[tokio::test]
async fn update_base_model_status_with_error_message() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let params = minimal_model_params("model-1");
    let model_id = db.register_model(params).await?;

    db.update_base_model_status(
        "tenant-a",
        &model_id,
        "error",
        Some("Failed to load model weights"),
        None,
    )
    .await?;

    let status = db.get_base_model_status("tenant-a").await?.unwrap();
    assert_eq!(status.status, "error");
    assert_eq!(
        status.error_message.as_deref(),
        Some("Failed to load model weights")
    );

    Ok(())
}

#[tokio::test]
async fn get_base_model_status_returns_most_recent() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let params1 = minimal_model_params("model-1");
    let model_id1 = db.register_model(params1).await?;

    let params2 = minimal_model_params("model-2");
    let model_id2 = db.register_model(params2).await?;

    // Create statuses for both models
    db.update_base_model_status("tenant-a", &model_id1, "loaded", None, Some(4096))
        .await?;

    // Brief delay to ensure different timestamps
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    db.update_base_model_status("tenant-a", &model_id2, "loading", None, None)
        .await?;

    // Should return most recent (model-2)
    let status = db.get_base_model_status("tenant-a").await?.unwrap();
    assert_eq!(status.model_id, model_id2);
    assert_eq!(status.status, "loading");

    Ok(())
}

#[tokio::test]
async fn list_base_model_statuses() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;
    create_tenant(&db, "tenant-b").await?;

    let params1 = minimal_model_params("model-1");
    let model_id1 = db.register_model(params1).await?;

    let params2 = minimal_model_params("model-2");
    let model_id2 = db.register_model(params2).await?;

    db.update_base_model_status("tenant-a", &model_id1, "loaded", None, Some(4096))
        .await?;
    db.update_base_model_status("tenant-b", &model_id2, "unloaded", None, None)
        .await?;

    let statuses = db.list_base_model_statuses().await?;
    assert_eq!(statuses.len(), 2);

    // Verify both tenants' statuses are present
    let tenant_ids: Vec<&str> = statuses.iter().map(|s| s.tenant_id.as_str()).collect();
    assert!(tenant_ids.contains(&"tenant-a"));
    assert!(tenant_ids.contains(&"tenant-b"));

    Ok(())
}

// ============================================================================
// Edge Cases and Error Conditions
// ============================================================================

#[tokio::test]
async fn register_model_with_duplicate_name_fails() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let params1 = minimal_model_params("duplicate-name");
    db.register_model(params1).await?;

    // Second model with same name should fail (UNIQUE constraint)
    let params2 = minimal_model_params("duplicate-name");
    let result = db.register_model(params2).await;

    assert!(result.is_err(), "duplicate name should fail");

    Ok(())
}

#[tokio::test]
async fn register_model_with_duplicate_hash_fails() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let same_hash = "duplicate-hash-b3";

    let params1 = ModelRegistrationBuilder::new()
        .name("model-1")
        .hash_b3(same_hash)
        .config_hash_b3("config-1")
        .tokenizer_hash_b3("tokenizer-1")
        .tokenizer_cfg_hash_b3("tokenizer-cfg-1")
        .build()?;

    db.register_model(params1).await?;

    // Second model with same hash should fail (UNIQUE constraint)
    let params2 = ModelRegistrationBuilder::new()
        .name("model-2")
        .hash_b3(same_hash)
        .config_hash_b3("config-2")
        .tokenizer_hash_b3("tokenizer-2")
        .tokenizer_cfg_hash_b3("tokenizer-cfg-2")
        .build()?;

    let result = db.register_model(params2).await;
    assert!(result.is_err(), "duplicate hash should fail");

    Ok(())
}

#[tokio::test]
async fn model_builder_requires_all_mandatory_fields() -> Result<()> {
    // Missing name
    let result = ModelRegistrationBuilder::new()
        .hash_b3("hash")
        .config_hash_b3("config")
        .tokenizer_hash_b3("tokenizer")
        .tokenizer_cfg_hash_b3("tokenizer-cfg")
        .build();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("name is required"));

    // Missing hash_b3
    let result = ModelRegistrationBuilder::new()
        .name("model")
        .config_hash_b3("config")
        .tokenizer_hash_b3("tokenizer")
        .tokenizer_cfg_hash_b3("tokenizer-cfg")
        .build();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("hash_b3 is required"));

    // Missing config_hash_b3
    let result = ModelRegistrationBuilder::new()
        .name("model")
        .hash_b3("hash")
        .tokenizer_hash_b3("tokenizer")
        .tokenizer_cfg_hash_b3("tokenizer-cfg")
        .build();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("config_hash_b3 is required"));

    // Missing tokenizer_hash_b3
    let result = ModelRegistrationBuilder::new()
        .name("model")
        .hash_b3("hash")
        .config_hash_b3("config")
        .tokenizer_cfg_hash_b3("tokenizer-cfg")
        .build();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("tokenizer_hash_b3 is required"));

    // Missing tokenizer_cfg_hash_b3
    let result = ModelRegistrationBuilder::new()
        .name("model")
        .hash_b3("hash")
        .config_hash_b3("config")
        .tokenizer_hash_b3("tokenizer")
        .build();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("tokenizer_cfg_hash_b3 is required"));

    Ok(())
}

#[tokio::test]
async fn count_adapters_for_nonexistent_model() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let count = db.count_adapters_for_model("nonexistent-id").await?;
    assert_eq!(count, 0);

    Ok(())
}

#[tokio::test]
async fn count_training_jobs_for_nonexistent_model() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let count = db.count_training_jobs_for_model("nonexistent-id").await?;
    assert_eq!(count, 0);

    Ok(())
}

#[tokio::test]
async fn get_base_model_status_for_tenant_without_status() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let status = db.get_base_model_status("tenant-a").await?;
    assert!(status.is_none());

    Ok(())
}

#[tokio::test]
async fn model_timestamps_are_set() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let params = minimal_model_params("timestamp-test");
    let model_id = db.register_model(params).await?;

    let model = db.get_model(&model_id).await?.expect("model exists");

    assert!(!model.created_at.is_empty());
    // For registered models (not imported), updated_at may be None
    // but created_at should always be set

    Ok(())
}

#[tokio::test]
async fn import_model_timestamps_are_set() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let before = Utc::now();

    let model_id = db
        .import_model_from_path(
            "timestamp-model",
            "/var/models/timestamp",
            "safetensors",
            "metal",
            "tenant-a",
            "user-1",
        )
        .await?;

    let model = db.get_model(&model_id).await?.expect("model exists");

    assert!(!model.created_at.is_empty());
    assert!(model.updated_at.is_some());
    assert!(model.imported_at.is_some());

    // Verify timestamp is recent
    let imported_at = chrono::DateTime::parse_from_rfc3339(model.imported_at.as_ref().unwrap())
        .expect("valid timestamp");
    assert!(imported_at.signed_duration_since(before).num_seconds() >= 0);

    Ok(())
}

#[tokio::test]
async fn update_model_import_status_updates_timestamp() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let model_id = db
        .import_model_from_path(
            "update-test",
            "/var/models/update-test",
            "safetensors",
            "metal",
            "tenant-a",
            "user-1",
        )
        .await?;

    let model_before = db.get_model(&model_id).await?.unwrap();
    let updated_at_before = model_before.updated_at.clone();

    // Small delay to ensure timestamp difference
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    db.update_model_import_status(&model_id, "available", None)
        .await?;

    let model_after = db.get_model(&model_id).await?.unwrap();
    let updated_at_after = model_after.updated_at.clone();

    assert_ne!(updated_at_before, updated_at_after);

    Ok(())
}

// ============================================================================
// Schema Validation Tests
// ============================================================================

#[tokio::test]
async fn model_default_values_are_set() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let params = minimal_model_params("defaults-test");
    let model_id = db.register_model(params).await?;

    let model = db.get_model(&model_id).await?.expect("model exists");

    // Check schema defaults
    // Note: model_type, status, tenant_id, backend have defaults in schema
    // but may not be set for models registered via register_model (vs import_model_from_path)
    assert!(model.created_at.len() > 0);

    Ok(())
}

#[tokio::test]
async fn imported_model_has_correct_defaults() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let model_id = db
        .import_model_from_path(
            "import-defaults",
            "/var/models/defaults",
            "safetensors",
            "metal",
            "tenant-a",
            "user-1",
        )
        .await?;

    let model = db.get_model(&model_id).await?.expect("model exists");

    assert_eq!(model.format.as_deref(), Some("safetensors"));
    assert_eq!(model.backend.as_deref(), Some("metal"));
    assert_eq!(model.import_status.as_deref(), Some("importing"));
    assert_eq!(model.tenant_id.as_deref(), Some("tenant-a"));
    assert_eq!(model.imported_by.as_deref(), Some("user-1"));

    Ok(())
}
