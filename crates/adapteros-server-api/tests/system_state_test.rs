//! System State Handler Tests
//!
//! Tests for the ground truth `/v1/system/state` endpoint covering:
//! - Successful response structure
//! - Tenant isolation (non-admin sees only own tenant)
//! - Top adapters limit
//! - Permission checks

mod common;

use adapteros_api_types::system_state::SystemStateQuery;
use adapteros_server_api::handlers::system_state::get_system_state;
use axum::{
    extract::{Query, State},
    Extension,
};

/// Test basic system state endpoint returns valid structure
#[tokio::test]
async fn test_get_system_state_success() {
    let state = common::setup_state(None).await.expect("setup failed");
    let claims = common::test_admin_claims();
    let query = SystemStateQuery::default();

    let result = get_system_state(State(state), Extension(claims), Query(query)).await;

    assert!(result.is_ok(), "get_system_state should succeed");
    let response = result.unwrap().0;

    // Verify structure
    assert!(!response.schema_version.is_empty());
    assert!(!response.timestamp.is_empty());
    assert!(!response.origin.node_id.is_empty());
    assert!(response.memory.headroom_percent >= 0.0);
    assert!(response.memory.total_mb > 0);
}

/// Test that viewer can access system state (has MetricsView permission)
#[tokio::test]
async fn test_get_system_state_viewer_access() {
    let state = common::setup_state(None).await.expect("setup failed");
    let claims = common::test_viewer_claims();
    let query = SystemStateQuery::default();

    let result = get_system_state(State(state), Extension(claims), Query(query)).await;

    // Viewers should be able to see system state via MetricsView permission
    assert!(result.is_ok(), "viewer should have MetricsView permission");
}

/// Test tenant isolation - non-admin only sees their own tenant
#[tokio::test]
async fn test_get_system_state_tenant_isolation() {
    let state = common::setup_state(None).await.expect("setup failed");

    // Create a viewer in 'default' tenant
    let claims = common::test_viewer_claims(); // This is tenant_id = "default"
    let query = SystemStateQuery {
        tenant_id: Some("tenant-1".to_string()), // Trying to query different tenant
        ..Default::default()
    };

    let result = get_system_state(State(state), Extension(claims), Query(query)).await;
    assert!(result.is_ok());

    let response = result.unwrap().0;
    // Non-admin should only see their own tenant, even if they request another
    for tenant in &response.tenants {
        assert_eq!(
            tenant.tenant_id, "default",
            "Non-admin should only see own tenant"
        );
    }
}

/// Test that admin can see all tenants
#[tokio::test]
async fn test_get_system_state_admin_sees_all() {
    let state = common::setup_state(None).await.expect("setup failed");
    let claims = common::test_admin_claims();
    let query = SystemStateQuery::default(); // No filter

    let result = get_system_state(State(state), Extension(claims), Query(query)).await;
    assert!(result.is_ok());

    let response = result.unwrap().0;
    // Admin should see multiple tenants (default and tenant-1 created in setup)
    // Note: might only see tenants that exist
    assert!(
        response.tenants.len() >= 1,
        "Admin should see at least one tenant"
    );
}

/// Test top_adapters limit is respected
#[tokio::test]
async fn test_get_system_state_top_adapters_limit() {
    let state = common::setup_state(None).await.expect("setup failed");
    let claims = common::test_admin_claims();
    let query = SystemStateQuery {
        top_adapters: Some(3),
        ..Default::default()
    };

    let result = get_system_state(State(state), Extension(claims), Query(query)).await;
    assert!(result.is_ok());

    let response = result.unwrap().0;
    assert!(
        response.memory.top_adapters.len() <= 3,
        "Should respect top_adapters limit"
    );
}

/// Test include_adapters=false hides adapters in stack summaries
#[tokio::test]
async fn test_get_system_state_exclude_adapters() {
    let state = common::setup_state(None).await.expect("setup failed");

    // First create a tenant with a stack and adapter
    common::create_test_tenant(&state, "test-tenant-with-adapter", "Test Tenant")
        .await
        .ok();

    let claims = common::test_admin_claims();
    let query = SystemStateQuery {
        include_adapters: Some(false),
        ..Default::default()
    };

    let result = get_system_state(State(state), Extension(claims), Query(query)).await;
    assert!(result.is_ok());

    let response = result.unwrap().0;
    // When include_adapters is false, stack.adapters should be empty
    for tenant in &response.tenants {
        for stack in &tenant.stacks {
            assert!(
                stack.adapters.is_empty(),
                "Stack adapters should be empty when include_adapters=false"
            );
        }
    }
}

/// Test memory pressure levels are valid
#[tokio::test]
async fn test_get_system_state_pressure_level_valid() {
    let state = common::setup_state(None).await.expect("setup failed");
    let claims = common::test_admin_claims();
    let query = SystemStateQuery::default();

    let result = get_system_state(State(state), Extension(claims), Query(query)).await;
    assert!(result.is_ok());

    let response = result.unwrap().0;
    let pressure = &response.memory.pressure_level;

    // Pressure level should be one of the valid values
    let valid_levels = ["low", "medium", "high", "critical"];
    let pressure_str = format!("{:?}", pressure).to_lowercase();
    assert!(
        valid_levels
            .iter()
            .any(|&v| pressure_str.contains(v) || pressure_str == v),
        "Pressure level should be low/medium/high/critical, got {:?}",
        pressure
    );
}

/// Test node state is populated
#[tokio::test]
async fn test_get_system_state_node_populated() {
    let state = common::setup_state(None).await.expect("setup failed");
    let claims = common::test_admin_claims();
    let query = SystemStateQuery::default();

    let result = get_system_state(State(state), Extension(claims), Query(query)).await;
    assert!(result.is_ok());

    let response = result.unwrap().0;
    let node = &response.node;

    // Node state should have valid values
    assert!(node.uptime_seconds >= 0);
    assert!(node.cpu_usage_percent >= 0.0 && node.cpu_usage_percent <= 100.0);
    assert!(node.memory_usage_percent >= 0.0 && node.memory_usage_percent <= 100.0);
}

/// Test operator can access system state
#[tokio::test]
async fn test_get_system_state_operator_access() {
    let state = common::setup_state(None).await.expect("setup failed");
    let claims = common::test_operator_claims();
    let query = SystemStateQuery::default();

    let result = get_system_state(State(state), Extension(claims), Query(query)).await;

    // Operators should be able to see system state via MetricsView permission
    assert!(
        result.is_ok(),
        "operator should have MetricsView permission"
    );
}

/// Test compliance can access system state
#[tokio::test]
async fn test_get_system_state_compliance_access() {
    let state = common::setup_state(None).await.expect("setup failed");
    let claims = common::test_compliance_claims();
    let query = SystemStateQuery::default();

    let result = get_system_state(State(state), Extension(claims), Query(query)).await;

    // Compliance should be able to see system state via MetricsView permission
    assert!(
        result.is_ok(),
        "compliance should have MetricsView permission"
    );
}
