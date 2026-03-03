//! Dataset Trust System Integration Tests
//!
//! Tests for dataset trust derivation, override, and propagation to linked adapters.

use adapteros_core::Result;
use adapteros_db::Db;
use uuid::Uuid;

/// Default tenant ID seeded by seed_dev_data()
const DEFAULT_TENANT: &str = "default";

/// Helper to create a test database with migrations and seeded data
async fn setup_test_db() -> Result<Db> {
    let db = Db::new_in_memory().await?;
    db.seed_dev_data().await?;
    Ok(db)
}

/// Helper to create a test dataset with tenant_id properly set
async fn create_test_dataset(db: &Db, tenant_id: &str, name: &str) -> String {
    let id = Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO training_datasets (
            id, name, tenant_id, description, format, hash_b3, storage_path,
            validation_status, created_at
        ) VALUES (?, ?, ?, ?, 'jsonl', 'hash123', '/var/datasets/test', 'pending', datetime('now'))",
    )
    .bind(&id)
    .bind(name)
    .bind(tenant_id)
    .bind("Test dataset for trust integration")
    .execute(db.pool_result().unwrap())
    .await
    .expect("dataset created");
    id
}

/// Helper to create a dataset version with specified tenant
async fn create_test_version(db: &Db, dataset_id: &str, tenant_id: &str) -> String {
    db.create_training_dataset_version(
        dataset_id,
        Some(tenant_id),
        Some("v1"),
        "/var/datasets/test/v1",
        "version_hash_123",
        None,
        None,
        None, // No created_by to avoid FK constraint on users table
    )
    .await
    .expect("version created")
}

#[tokio::test]
async fn test_dataset_version_trust_derivation_allowed() -> Result<()> {
    let db = setup_test_db().await?;

    let dataset_id = create_test_dataset(&db, DEFAULT_TENANT, "trust-test-allowed").await;
    let version_id = create_test_version(&db, &dataset_id, DEFAULT_TENANT).await;

    // First, set all safety signals to clean (they default to "unknown" which causes needs_approval)
    db.update_dataset_version_safety_status(
        &version_id,
        Some("clean"), // pii
        Some("clean"), // toxicity
        Some("clean"), // leak
        Some("clean"), // anomaly
    )
    .await?;

    // Now update validation to valid state
    let trust_state = db
        .update_dataset_version_structural_validation(&version_id, "valid", None)
        .await?;

    // With valid validation and clean safety signals, trust should be "allowed"
    assert_eq!(
        trust_state, "allowed",
        "Valid dataset with clean safety should be allowed"
    );

    // Verify effective trust state
    let effective = db.get_effective_trust_state(&version_id).await?;
    assert_eq!(effective, Some("allowed".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_dataset_version_trust_derivation_blocked_on_invalid() -> Result<()> {
    let db = setup_test_db().await?;

    let dataset_id = create_test_dataset(&db, DEFAULT_TENANT, "trust-test-blocked").await;
    let version_id = create_test_version(&db, &dataset_id, DEFAULT_TENANT).await;

    // Update validation to invalid state
    let trust_state = db
        .update_dataset_version_structural_validation(
            &version_id,
            "invalid",
            Some(r#"["Invalid JSON at line 5"]"#),
        )
        .await?;

    // Invalid validation should result in blocked trust
    assert_eq!(trust_state, "blocked", "Invalid dataset should be blocked");

    Ok(())
}

#[tokio::test]
async fn test_dataset_version_trust_derivation_with_safety_warning() -> Result<()> {
    let db = setup_test_db().await?;

    let dataset_id = create_test_dataset(&db, DEFAULT_TENANT, "trust-test-warning").await;
    let version_id = create_test_version(&db, &dataset_id, DEFAULT_TENANT).await;

    // First make validation valid
    db.update_dataset_version_structural_validation(&version_id, "valid", None)
        .await?;

    // Now add a safety warning for PII, with other statuses clean
    let trust_state = db
        .update_dataset_version_safety_status(
            &version_id,
            Some("warn"),  // pii_status - warning
            Some("clean"), // toxicity_status - clean
            Some("clean"), // leak_status - clean
            Some("clean"), // anomaly_status - clean
        )
        .await?;

    // Safety warning should result in "allowed_with_warning"
    assert_eq!(
        trust_state, "allowed_with_warning",
        "Dataset with safety warning should be allowed_with_warning"
    );

    Ok(())
}

#[tokio::test]
async fn test_dataset_version_trust_derivation_blocked_on_safety() -> Result<()> {
    let db = setup_test_db().await?;

    let dataset_id = create_test_dataset(&db, DEFAULT_TENANT, "trust-test-safety-block").await;
    let version_id = create_test_version(&db, &dataset_id, DEFAULT_TENANT).await;

    // First make validation valid
    db.update_dataset_version_structural_validation(&version_id, "valid", None)
        .await?;

    // Now add a safety block (e.g., high toxicity)
    let trust_state = db
        .update_dataset_version_safety_status(
            &version_id,
            None,          // pii_status
            Some("block"), // toxicity_status - blocks
            None,          // leak_status
            None,          // anomaly_status
        )
        .await?;

    // Safety block should result in "blocked"
    assert_eq!(
        trust_state, "blocked",
        "Dataset with safety block should be blocked"
    );

    Ok(())
}

#[tokio::test]
async fn test_dataset_version_trust_override() -> Result<()> {
    let db = setup_test_db().await?;

    let dataset_id = create_test_dataset(&db, DEFAULT_TENANT, "trust-test-override").await;
    let version_id = create_test_version(&db, &dataset_id, DEFAULT_TENANT).await;

    // Make the dataset blocked through invalid validation
    db.update_dataset_version_structural_validation(
        &version_id,
        "invalid",
        Some(r#"["Test error"]"#),
    )
    .await?;

    // Verify it's initially blocked
    let initial_trust = db.get_effective_trust_state(&version_id).await?;
    assert_eq!(initial_trust, Some("blocked".to_string()));

    // Apply an admin override to allow the dataset
    let override_id = db
        .create_dataset_version_override(
            &version_id,
            "allowed",
            Some("Manually reviewed and approved by admin"),
            "admin@test.com",
        )
        .await?;

    assert!(!override_id.is_empty(), "Override should be created");

    // Verify the effective trust state is now "allowed" due to override
    let effective_trust = db.get_effective_trust_state(&version_id).await?;
    assert_eq!(
        effective_trust,
        Some("allowed".to_string()),
        "Override should change effective trust to allowed"
    );

    Ok(())
}

#[tokio::test]
async fn test_dataset_version_trust_needs_approval_on_pending() -> Result<()> {
    let db = setup_test_db().await?;

    let dataset_id = create_test_dataset(&db, DEFAULT_TENANT, "trust-test-pending").await;
    let version_id = create_test_version(&db, &dataset_id, DEFAULT_TENANT).await;

    // Fetch the version - by default validation_status is 'pending' and trust_state is 'unknown'
    let version = db
        .get_training_dataset_version(&version_id)
        .await?
        .expect("version exists");

    // A newly created dataset version should have trust_state = 'unknown' (default)
    // The trust derivation logic returns 'needs_approval' for pending validation,
    // but the initial trust_state column defaults to 'unknown' before derivation runs
    assert_eq!(
        version.trust_state, "unknown",
        "Newly created version should have unknown trust state by default"
    );

    // Now trigger trust derivation by calling update (with same pending status)
    let derived_trust = db
        .update_dataset_version_structural_validation(&version_id, "pending", None)
        .await?;

    // After derivation runs, pending validation with unknown safety should give needs_approval
    assert_eq!(
        derived_trust, "needs_approval",
        "Pending validation should result in needs_approval trust state"
    );

    Ok(())
}

#[tokio::test]
async fn test_latest_trusted_version_selection() -> Result<()> {
    let db = setup_test_db().await?;

    let dataset_id = create_test_dataset(&db, DEFAULT_TENANT, "trust-test-selection").await;

    // Create v1 - make it blocked
    let v1_id = db
        .create_training_dataset_version(
            &dataset_id,
            Some(DEFAULT_TENANT),
            Some("v1"),
            "/var/datasets/v1",
            "hash_v1",
            None,
            None,
            None, // No created_by to avoid FK constraint
        )
        .await?;
    db.update_dataset_version_structural_validation(&v1_id, "invalid", None)
        .await?;

    // Create v2 - make it allowed (requires valid validation + clean safety)
    let v2_id = db
        .create_training_dataset_version(
            &dataset_id,
            Some(DEFAULT_TENANT),
            Some("v2"),
            "/var/datasets/v2",
            "hash_v2",
            None,
            None,
            None, // No created_by to avoid FK constraint
        )
        .await?;
    // Set all safety statuses to clean first
    db.update_dataset_version_safety_status(
        &v2_id,
        Some("clean"),
        Some("clean"),
        Some("clean"),
        Some("clean"),
    )
    .await?;
    // Then set validation to valid - this triggers trust derivation to "allowed"
    db.update_dataset_version_structural_validation(&v2_id, "valid", None)
        .await?;

    // Create v3 - make it blocked
    let v3_id = db
        .create_training_dataset_version(
            &dataset_id,
            Some(DEFAULT_TENANT),
            Some("v3"),
            "/var/datasets/v3",
            "hash_v3",
            None,
            None,
            None, // No created_by to avoid FK constraint
        )
        .await?;
    db.update_dataset_version_structural_validation(&v3_id, "invalid", None)
        .await?;

    // Get latest trusted version - should be v2 (the only allowed one)
    let (trusted_version, trust_state) = db
        .get_latest_trusted_dataset_version_for_dataset(&dataset_id)
        .await?
        .expect("should find a trusted version");

    assert_eq!(
        trusted_version.id, v2_id,
        "Latest trusted version should be v2"
    );
    assert_eq!(
        trust_state, "allowed",
        "Trust state should be allowed for v2"
    );

    Ok(())
}

#[tokio::test]
async fn test_trust_override_persists_across_queries() -> Result<()> {
    let db = setup_test_db().await?;

    let dataset_id = create_test_dataset(&db, DEFAULT_TENANT, "trust-override-persist").await;
    let version_id = create_test_version(&db, &dataset_id, DEFAULT_TENANT).await;

    // Apply an override
    db.create_dataset_version_override(
        &version_id,
        "allowed_with_warning",
        Some("Test override"),
        "admin@test.com",
    )
    .await?;

    // Query multiple times and verify consistency
    for _ in 0..3 {
        let effective = db.get_effective_trust_state(&version_id).await?;
        assert_eq!(
            effective,
            Some("allowed_with_warning".to_string()),
            "Override should persist across queries"
        );
    }

    Ok(())
}
