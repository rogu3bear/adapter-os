//! Dataset Deduplication Edge Case Tests
//!
//! Validates workspace-scoped deduplication behavior:
//! 1. Duplicate upload in same workspace returns existing dataset_id
//! 2. Same content in different workspace creates new dataset_id
//! 3. Cross-workspace visibility is isolated

mod common;

use common::{create_test_workspace, setup_state, test_admin_claims};
use uuid::Uuid;

/// Helper to create a dataset with specific hash in a workspace
async fn create_dataset_with_hash(
    state: &adapteros_server_api::state::AppState,
    tenant_id: &str,
    workspace_id: &str,
    dataset_hash: &str,
) -> String {
    let dataset_id = format!("ds-{}", Uuid::now_v7());
    state
        .db
        .create_training_dataset_with_id(
            &dataset_id,
            "test dataset",
            None,
            "jsonl",
            "file_hash",
            "var/datasets",
            Some(tenant_id),
            Some(workspace_id),
            Some("ready"),
            Some(dataset_hash),
            None,
        )
        .await
        .expect("dataset created");
    dataset_id
}

#[tokio::test]
async fn test_duplicate_lookup_same_workspace_returns_existing() {
    // Setup
    let state = setup_state(None).await.expect("state setup");
    let claims = test_admin_claims();
    let workspace_id = create_test_workspace(&state, "Test Workspace", &claims.sub)
        .await
        .expect("workspace created");

    // Create first dataset with known hash
    let dataset_hash = "test_hash_for_dedup_same_workspace";
    let first_dataset_id =
        create_dataset_with_hash(&state, &claims.tenant_id, &workspace_id, dataset_hash).await;

    // Query for existing dataset with same hash + workspace
    let existing = state
        .db
        .get_dataset_by_hash_and_workspace(dataset_hash, &workspace_id)
        .await
        .expect("query should succeed");

    // Should find the existing dataset
    assert!(existing.is_some(), "Should find existing dataset");
    let found = existing.unwrap();
    assert_eq!(
        found.id, first_dataset_id,
        "Should return the original dataset_id"
    );
    assert_eq!(found.dataset_hash_b3, dataset_hash, "Hash should match");
    assert_eq!(
        found.workspace_id.as_deref(),
        Some(workspace_id.as_str()),
        "Workspace should match"
    );
}

#[tokio::test]
async fn test_same_content_different_workspace_creates_new_dataset() {
    // Setup
    let state = setup_state(None).await.expect("state setup");
    let claims = test_admin_claims();

    // Create two workspaces
    let workspace_a = create_test_workspace(&state, "Workspace A", &claims.sub)
        .await
        .expect("workspace A created");
    let workspace_b = create_test_workspace(&state, "Workspace B", &claims.sub)
        .await
        .expect("workspace B created");

    // Same hash for both datasets (simulates same file content)
    let dataset_hash = "identical_content_hash_cross_workspace";

    // Create dataset in workspace A
    let dataset_id_a =
        create_dataset_with_hash(&state, &claims.tenant_id, &workspace_a, dataset_hash).await;

    // Create dataset in workspace B with same hash
    let dataset_id_b =
        create_dataset_with_hash(&state, &claims.tenant_id, &workspace_b, dataset_hash).await;

    // Assertions
    assert_ne!(
        dataset_id_a, dataset_id_b,
        "Different workspaces should produce different dataset_ids"
    );

    // Verify each workspace returns its own dataset
    let found_a = state
        .db
        .get_dataset_by_hash_and_workspace(dataset_hash, &workspace_a)
        .await
        .expect("query A");
    let found_b = state
        .db
        .get_dataset_by_hash_and_workspace(dataset_hash, &workspace_b)
        .await
        .expect("query B");

    assert_eq!(
        found_a.as_ref().map(|d| d.id.as_str()),
        Some(dataset_id_a.as_str()),
        "Workspace A should find its dataset"
    );
    assert_eq!(
        found_b.as_ref().map(|d| d.id.as_str()),
        Some(dataset_id_b.as_str()),
        "Workspace B should find its dataset"
    );

    // Both have same hash
    assert_eq!(
        found_a.as_ref().map(|d| d.dataset_hash_b3.as_str()),
        Some(dataset_hash),
        "Workspace A dataset has correct hash"
    );
    assert_eq!(
        found_b.as_ref().map(|d| d.dataset_hash_b3.as_str()),
        Some(dataset_hash),
        "Workspace B dataset has correct hash"
    );
}

#[tokio::test]
async fn test_cross_workspace_visibility_isolated() {
    // Setup
    let state = setup_state(None).await.expect("state setup");
    let claims = test_admin_claims();

    // Create two workspaces
    let workspace_a = create_test_workspace(&state, "Isolated A", &claims.sub)
        .await
        .expect("workspace A created");
    let workspace_b = create_test_workspace(&state, "Isolated B", &claims.sub)
        .await
        .expect("workspace B created");

    // Create dataset only in workspace A
    let dataset_hash = "unique_hash_for_isolation_test";
    let _dataset_id_a =
        create_dataset_with_hash(&state, &claims.tenant_id, &workspace_a, dataset_hash).await;

    // Query from workspace B should NOT find the dataset
    let found_in_b = state
        .db
        .get_dataset_by_hash_and_workspace(dataset_hash, &workspace_b)
        .await
        .expect("query B should succeed");

    assert!(
        found_in_b.is_none(),
        "Workspace B should NOT see Workspace A's dataset"
    );

    // Query from workspace A should still find it
    let found_in_a = state
        .db
        .get_dataset_by_hash_and_workspace(dataset_hash, &workspace_a)
        .await
        .expect("query A should succeed");

    assert!(
        found_in_a.is_some(),
        "Workspace A should see its own dataset"
    );
}

#[tokio::test]
async fn test_list_datasets_respects_workspace_scope() {
    // Setup
    let state = setup_state(None).await.expect("state setup");
    let claims = test_admin_claims();

    // Create two workspaces
    let workspace_a = create_test_workspace(&state, "List Test A", &claims.sub)
        .await
        .expect("workspace A created");
    let workspace_b = create_test_workspace(&state, "List Test B", &claims.sub)
        .await
        .expect("workspace B created");

    // Create datasets in each workspace
    create_dataset_with_hash(&state, &claims.tenant_id, &workspace_a, "hash_a1").await;
    create_dataset_with_hash(&state, &claims.tenant_id, &workspace_a, "hash_a2").await;
    create_dataset_with_hash(&state, &claims.tenant_id, &workspace_b, "hash_b1").await;

    // List datasets for workspace A
    let datasets_a = state
        .db
        .list_training_datasets_for_workspace(&claims.tenant_id, &workspace_a, 100)
        .await
        .expect("list A");

    // List datasets for workspace B
    let datasets_b = state
        .db
        .list_training_datasets_for_workspace(&claims.tenant_id, &workspace_b, 100)
        .await
        .expect("list B");

    // Verify counts
    assert_eq!(datasets_a.len(), 2, "Workspace A should have 2 datasets");
    assert_eq!(datasets_b.len(), 1, "Workspace B should have 1 dataset");

    // Verify all datasets in A have workspace_id = workspace_a
    for ds in &datasets_a {
        assert_eq!(
            ds.workspace_id.as_deref(),
            Some(workspace_a.as_str()),
            "All datasets in list A should belong to workspace A"
        );
    }

    // Verify all datasets in B have workspace_id = workspace_b
    for ds in &datasets_b {
        assert_eq!(
            ds.workspace_id.as_deref(),
            Some(workspace_b.as_str()),
            "All datasets in list B should belong to workspace B"
        );
    }
}
