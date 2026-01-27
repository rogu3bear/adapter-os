//! Unit tests for admin status handlers.
//!
//! These tests verify the JSON structure and content of admin handler responses.

use adapteros_server_api_admin::handlers::status::{
    admin_status, system_config, AdminStatusResponse, SystemConfigResponse,
};
use axum::response::IntoResponse;

/// Helper to extract JSON from an axum response
async fn extract_json<T: serde::de::DeserializeOwned>(response: impl IntoResponse) -> T {
    let response = response.into_response();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    serde_json::from_slice(&body).expect("deserialize JSON")
}

#[tokio::test]
async fn test_admin_status_returns_expected_structure() {
    let response = admin_status().await;
    let json: AdminStatusResponse = extract_json(response).await;

    assert_eq!(json.status, "ok");
    assert_eq!(json.version, env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn test_admin_status_version_is_semver() {
    let response = admin_status().await;
    let json: AdminStatusResponse = extract_json(response).await;

    // Verify the version string follows semver pattern (e.g., "0.1.0")
    let parts: Vec<&str> = json.version.split('.').collect();
    assert_eq!(
        parts.len(),
        3,
        "version should have 3 parts separated by dots"
    );
    for part in parts {
        assert!(
            part.parse::<u32>().is_ok(),
            "each version part should be a number"
        );
    }
}

#[tokio::test]
async fn test_system_config_returns_expected_structure() {
    let response = system_config().await;
    let json: SystemConfigResponse = extract_json(response).await;

    assert_eq!(json.config_version, "1.0");
    assert!(json.policies_enabled);
}

#[tokio::test]
async fn test_system_config_policies_enabled_is_true() {
    let response = system_config().await;
    let json: SystemConfigResponse = extract_json(response).await;

    // Verify policies_enabled is explicitly true
    assert_eq!(json.policies_enabled, true);
}

#[tokio::test]
async fn test_admin_status_json_serialization() {
    let response = admin_status().await;
    let response = response.into_response();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let body_str = String::from_utf8(body.to_vec()).expect("valid UTF-8");

    // Verify the JSON contains expected keys
    assert!(body_str.contains("\"status\""));
    assert!(body_str.contains("\"version\""));
    assert!(body_str.contains("\"ok\""));
}

#[tokio::test]
async fn test_system_config_json_serialization() {
    let response = system_config().await;
    let response = response.into_response();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let body_str = String::from_utf8(body.to_vec()).expect("valid UTF-8");

    // Verify the JSON contains expected keys
    assert!(body_str.contains("\"config_version\""));
    assert!(body_str.contains("\"policies_enabled\""));
    assert!(body_str.contains("\"1.0\""));
    assert!(body_str.contains("true"));
}
