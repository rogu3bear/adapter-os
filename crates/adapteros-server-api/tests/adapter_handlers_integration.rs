//! Integration tests for adapter lifecycle handlers
//!
//! Tests adapter creation, update, deletion, listing, and lifecycle operations
//! with proper tenant isolation and permission checks.

use adapteros_core::Result;
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_server_api::handlers::adapters::{
    update_adapter_strength, UpdateAdapterStrengthRequest,
};
use adapteros_server_api::handlers::{delete_adapter, get_adapter, list_adapters};
use adapteros_server_api::types::ListAdaptersQuery;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;

mod common;
use common::{create_test_adapter_default, setup_state, test_admin_claims, test_viewer_claims};

/// Test listing adapters returns only tenant-scoped results
#[tokio::test]
async fn list_adapters_returns_tenant_scoped_results() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    // Create adapters for two different tenants
    create_test_adapter_default(&state, "tenant1-adapter-1", "tenant-1").await?;
    create_test_adapter_default(&state, "tenant1-adapter-2", "tenant-1").await?;
    create_test_adapter_default(&state, "default-adapter-1", "default").await?;

    // List with tenant-1 credentials
    let claims = test_admin_claims(); // tenant-1
    let result = list_adapters(
        State(state.clone()),
        Extension(claims),
        Query(ListAdaptersQuery {
            tier: None,
            framework: None,
            ..Default::default()
        }),
    )
    .await;

    let adapters = result.expect("list should succeed").0;
    assert_eq!(adapters.len(), 2, "should only see tenant-1 adapters");
    assert!(adapters.iter().all(|a| a.adapter_id.starts_with("tenant1")));

    // List with default tenant credentials
    let default_claims = test_viewer_claims(); // default tenant
    let result2 = list_adapters(
        State(state.clone()),
        Extension(default_claims),
        Query(ListAdaptersQuery {
            tier: None,
            framework: None,
            ..Default::default()
        }),
    )
    .await;

    let adapters2 = result2.expect("list should succeed").0;
    assert_eq!(adapters2.len(), 1, "should only see default adapter");
    assert_eq!(adapters2[0].adapter_id, "default-adapter-1");

    Ok(())
}

/// Test getting a specific adapter
#[tokio::test]
async fn get_adapter_returns_adapter_details() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-1")
        .adapter_id("test-adapter-get")
        .name("Test Get Adapter")
        .hash_b3("b3:test-get-hash")
        .rank(8)
        .tier("persistent")
        .category("code")
        .scope("tenant")
        .build()
        .expect("adapter params");

    state.db.register_adapter(params).await?;

    let claims = test_admin_claims();
    let result = get_adapter(
        State(state),
        Extension(claims),
        Path("test-adapter-get".to_string()),
    )
    .await;

    let adapter = result.expect("get should succeed").0;
    assert_eq!(adapter.adapter_id, "test-adapter-get");
    assert_eq!(adapter.name, "Test Get Adapter");
    assert_eq!(adapter.rank, 8);

    Ok(())
}

/// Test cross-tenant adapter access returns 404
#[tokio::test]
async fn get_adapter_cross_tenant_returns_404() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-1")
        .adapter_id("tenant1-private")
        .name("Private Adapter")
        .hash_b3("b3:private-hash")
        .rank(4)
        .tier("persistent")
        .category("code")
        .scope("tenant")
        .build()
        .expect("adapter params");

    state.db.register_adapter(params).await?;

    // Try to access from different tenant
    let other_tenant_claims = test_viewer_claims(); // default tenant
    let result = get_adapter(
        State(state),
        Extension(other_tenant_claims),
        Path("tenant1-private".to_string()),
    )
    .await;

    match result {
        Err((status, body)) => {
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(body.0.code, "NOT_FOUND");
        }
        Ok(_) => panic!("cross-tenant access should fail"),
    }

    Ok(())
}

/// Test updating adapter strength
#[tokio::test]
async fn update_adapter_strength_succeeds() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-1")
        .adapter_id("test-adapter-update")
        .name("Original Name")
        .hash_b3("b3:update-hash")
        .rank(4)
        .tier("persistent")
        .category("code")
        .scope("tenant")
        .build()
        .expect("adapter params");

    state.db.register_adapter(params).await?;

    let update_req = UpdateAdapterStrengthRequest { lora_strength: 1.5 };

    let claims = test_admin_claims();
    let result = update_adapter_strength(
        State(state.clone()),
        Extension(claims.clone()),
        Path("test-adapter-update".to_string()),
        axum::Json(update_req),
    )
    .await;

    assert!(result.is_ok(), "update should succeed");
    let response = result.unwrap().0;
    assert_eq!(response.lora_strength, Some(1.5));

    // Verify update was applied
    let get_result = get_adapter(
        State(state),
        Extension(claims),
        Path("test-adapter-update".to_string()),
    )
    .await;

    let adapter = get_result.expect("get should succeed").0;
    assert_eq!(adapter.lora_strength, Some(1.5));

    Ok(())
}

/// Test deleting an adapter
#[tokio::test]
async fn delete_adapter_removes_adapter() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-1")
        .adapter_id("test-adapter-delete")
        .name("To Be Deleted")
        .hash_b3("b3:delete-hash")
        .rank(4)
        .tier("persistent")
        .category("code")
        .scope("tenant")
        .build()
        .expect("adapter params");

    state.db.register_adapter(params).await?;

    let claims = test_admin_claims();

    // Verify adapter exists
    let before = get_adapter(
        State(state.clone()),
        Extension(claims.clone()),
        Path("test-adapter-delete".to_string()),
    )
    .await;
    assert!(before.is_ok());

    // Delete adapter
    let delete_result = delete_adapter(
        State(state.clone()),
        Extension(claims.clone()),
        Path("test-adapter-delete".to_string()),
    )
    .await;

    assert!(delete_result.is_ok(), "delete should succeed");

    // Verify adapter is gone
    let after = get_adapter(
        State(state),
        Extension(claims),
        Path("test-adapter-delete".to_string()),
    )
    .await;

    match after {
        Err((status, _)) => {
            assert_eq!(status, StatusCode::NOT_FOUND);
        }
        Ok(_) => panic!("adapter should be deleted"),
    }

    Ok(())
}

/// Test cross-tenant delete returns 404
#[tokio::test]
async fn delete_adapter_cross_tenant_returns_404() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-1")
        .adapter_id("tenant1-protected")
        .name("Protected Adapter")
        .hash_b3("b3:protected-hash")
        .rank(4)
        .tier("persistent")
        .category("code")
        .scope("tenant")
        .build()
        .expect("adapter params");

    state.db.register_adapter(params).await?;

    // Try to delete from different tenant
    let mut other_claims = test_admin_claims();
    other_claims.tenant_id = "default".to_string();

    let result = delete_adapter(
        State(state),
        Extension(other_claims),
        Path("tenant1-protected".to_string()),
    )
    .await;

    match result {
        Err((status, body)) => {
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(body.0.code, "NOT_FOUND");
        }
        Ok(_) => panic!("cross-tenant delete should fail"),
    }

    Ok(())
}

/// Test filtering adapters by tier
#[tokio::test]
async fn list_adapters_filters_by_tier() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    // Create adapters with different tiers
    let tier1_params = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-1")
        .adapter_id("tier1-adapter")
        .name("Tier 1 Adapter")
        .hash_b3("b3:tier1-hash")
        .rank(16)
        .tier("warm")
        .category("code")
        .scope("tenant")
        .build()
        .expect("tier1 adapter params");

    state.db.register_adapter(tier1_params).await?;

    let tier2_params = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-1")
        .adapter_id("tier2-adapter")
        .name("Tier 2 Adapter")
        .hash_b3("b3:tier2-hash")
        .rank(16)
        .tier("ephemeral")
        .category("code")
        .scope("tenant")
        .build()
        .expect("tier2 adapter params");

    state.db.register_adapter(tier2_params).await?;

    let claims = test_admin_claims();

    // Filter by tier 1
    let result = list_adapters(
        State(state.clone()),
        Extension(claims.clone()),
        Query(ListAdaptersQuery {
            tier: Some("warm".to_string()),
            framework: None,
            ..Default::default()
        }),
    )
    .await;

    let adapters = result.expect("list should succeed").0;
    assert_eq!(adapters.len(), 1);
    assert_eq!(adapters[0].tier, "warm");

    // Filter by tier 2
    let result2 = list_adapters(
        State(state),
        Extension(claims),
        Query(ListAdaptersQuery {
            tier: Some("ephemeral".to_string()),
            framework: None,
            ..Default::default()
        }),
    )
    .await;

    let adapters2 = result2.expect("list should succeed").0;
    assert_eq!(adapters2.len(), 1);
    assert_eq!(adapters2[0].tier, "ephemeral");

    Ok(())
}

/// Test non-existent adapter returns 404
#[tokio::test]
async fn get_nonexistent_adapter_returns_404() -> Result<()> {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    let result = get_adapter(
        State(state),
        Extension(claims),
        Path("nonexistent-adapter".to_string()),
    )
    .await;

    match result {
        Err((status, body)) => {
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(body.0.code, "NOT_FOUND");
        }
        Ok(_) => panic!("should return 404 for nonexistent adapter"),
    }

    Ok(())
}

/// Test adapter permissions - viewer cannot delete
#[tokio::test]
async fn viewer_cannot_delete_adapter() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("default")
        .adapter_id("protected-adapter")
        .name("Protected")
        .hash_b3("b3:protected")
        .rank(4)
        .tier("persistent")
        .category("code")
        .scope("tenant")
        .build()
        .expect("adapter params");

    state.db.register_adapter(params).await?;

    let viewer_claims = test_viewer_claims(); // viewer role

    let result = delete_adapter(
        State(state),
        Extension(viewer_claims),
        Path("protected-adapter".to_string()),
    )
    .await;

    match result {
        Err((status, _)) => {
            assert!(
                status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED,
                "viewer should not have permission"
            );
        }
        Ok(_) => panic!("viewer should not be able to delete"),
    }

    Ok(())
}
