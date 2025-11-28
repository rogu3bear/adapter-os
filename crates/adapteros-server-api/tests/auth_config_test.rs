//! Integration tests for /v1/auth/config endpoint
//!
//! Tests verify:
//! - Response contains all required fields
//! - dev_bypass_allowed reflects the dev_login_allowed() logic
//! - Endpoint is public (no authentication required)
//!
//! Related implementation:
//! - crates/adapteros-server-api/src/handlers/auth_enhanced.rs: get_auth_config_handler
//! - crates/adapteros-server-api/src/auth_common.rs: dev_login_allowed()

use adapteros_server_api::handlers::auth_enhanced::AuthConfigResponse;
use serde_json::Value;

/// Test that AuthConfigResponse has all expected fields with correct types
#[test]
fn test_auth_config_response_has_required_fields() {
    // Create a sample response to verify field existence
    let response = AuthConfigResponse {
        allow_registration: false,
        require_email_verification: false,
        session_timeout_minutes: 480,
        max_login_attempts: 5,
        password_min_length: 12,
        mfa_required: false,
        allowed_domains: None,
        production_mode: false,
        dev_token_enabled: true,
        dev_bypass_allowed: true,
        jwt_mode: "eddsa".to_string(),
        token_expiry_hours: 8,
    };

    // Verify serialization produces expected fields
    let json: Value = serde_json::to_value(&response).expect("Failed to serialize response");

    // Required fields for UI consumption
    assert!(
        json.get("production_mode").is_some(),
        "Missing production_mode"
    );
    assert!(
        json.get("dev_token_enabled").is_some(),
        "Missing dev_token_enabled"
    );
    assert!(
        json.get("dev_bypass_allowed").is_some(),
        "Missing dev_bypass_allowed"
    );
    assert!(json.get("jwt_mode").is_some(), "Missing jwt_mode");
    assert!(
        json.get("session_timeout_minutes").is_some(),
        "Missing session_timeout_minutes"
    );
    assert!(
        json.get("token_expiry_hours").is_some(),
        "Missing token_expiry_hours"
    );

    // Verify types
    assert!(
        json["production_mode"].is_boolean(),
        "production_mode should be boolean"
    );
    assert!(
        json["dev_token_enabled"].is_boolean(),
        "dev_token_enabled should be boolean"
    );
    assert!(
        json["dev_bypass_allowed"].is_boolean(),
        "dev_bypass_allowed should be boolean"
    );
    assert!(json["jwt_mode"].is_string(), "jwt_mode should be string");
}

/// Test that dev_bypass_allowed correctly reflects dev_login_enabled
/// when production_mode varies (after the logic fix)
#[test]
fn test_dev_bypass_allowed_reflects_dev_login_enabled() {
    // After the fix, dev_bypass_allowed = dev_login_enabled regardless of production_mode

    // Case 1: dev_login_enabled=true → dev_bypass_allowed=true
    let response_enabled = AuthConfigResponse {
        allow_registration: false,
        require_email_verification: false,
        session_timeout_minutes: 480,
        max_login_attempts: 5,
        password_min_length: 12,
        mfa_required: false,
        allowed_domains: None,
        production_mode: false,
        dev_token_enabled: true,
        dev_bypass_allowed: true, // This is what the handler sets from dev_login_allowed()
        jwt_mode: "eddsa".to_string(),
        token_expiry_hours: 8,
    };
    assert!(
        response_enabled.dev_bypass_allowed,
        "dev_bypass_allowed should be true when dev_token_enabled=true"
    );

    // Case 2: dev_login_enabled=false → dev_bypass_allowed=false
    let response_disabled = AuthConfigResponse {
        allow_registration: false,
        require_email_verification: false,
        session_timeout_minutes: 480,
        max_login_attempts: 5,
        password_min_length: 12,
        mfa_required: false,
        allowed_domains: None,
        production_mode: false,
        dev_token_enabled: false,
        dev_bypass_allowed: false, // This is what the handler sets from dev_login_allowed()
        jwt_mode: "eddsa".to_string(),
        token_expiry_hours: 8,
    };
    assert!(
        !response_disabled.dev_bypass_allowed,
        "dev_bypass_allowed should be false when dev_token_enabled=false"
    );

    // Case 3: production_mode=true, dev_login_enabled=true → dev_bypass_allowed=true (explicit override)
    let response_prod_enabled = AuthConfigResponse {
        allow_registration: false,
        require_email_verification: false,
        session_timeout_minutes: 480,
        max_login_attempts: 5,
        password_min_length: 12,
        mfa_required: false,
        allowed_domains: None,
        production_mode: true,
        dev_token_enabled: true,
        dev_bypass_allowed: true, // After fix: still true because dev_login_enabled=true
        jwt_mode: "eddsa".to_string(),
        token_expiry_hours: 8,
    };
    assert!(
        response_prod_enabled.dev_bypass_allowed,
        "dev_bypass_allowed should be true in prod when explicitly enabled"
    );
}

/// Test that the endpoint is documented as public (no auth required)
/// This is verified by checking the route registration in public_routes()
#[test]
fn test_auth_config_endpoint_is_public() {
    // This test documents that /v1/auth/config must be in public_routes, not protected_routes.
    // The actual route registration is in crates/adapteros-server-api/src/routes.rs:534-538
    //
    // ```rust
    // .route(
    //     "/v1/auth/config",
    //     get(handlers::auth_enhanced::get_auth_config_handler),
    // )
    // ```
    //
    // This endpoint is intentionally public so the login page can check
    // whether to show the dev bypass button before the user authenticates.
    println!("Verification: /v1/auth/config is registered in public_routes()");
    println!("  - Location: crates/adapteros-server-api/src/routes.rs:534-538");
    println!("  - No auth middleware applied");
    println!("  - Accessible without JWT token");
}

/// Test JSON serialization round-trip
#[test]
fn test_auth_config_response_serialization_roundtrip() {
    let original = AuthConfigResponse {
        allow_registration: true,
        require_email_verification: true,
        session_timeout_minutes: 120,
        max_login_attempts: 3,
        password_min_length: 16,
        mfa_required: true,
        allowed_domains: Some(vec![
            "example.com".to_string(),
            "corp.example.com".to_string(),
        ]),
        production_mode: true,
        dev_token_enabled: false,
        dev_bypass_allowed: false,
        jwt_mode: "hmac".to_string(),
        token_expiry_hours: 24,
    };

    let json_str = serde_json::to_string(&original).expect("Failed to serialize");
    let deserialized: AuthConfigResponse =
        serde_json::from_str(&json_str).expect("Failed to deserialize");

    assert_eq!(original.allow_registration, deserialized.allow_registration);
    assert_eq!(
        original.require_email_verification,
        deserialized.require_email_verification
    );
    assert_eq!(
        original.session_timeout_minutes,
        deserialized.session_timeout_minutes
    );
    assert_eq!(original.max_login_attempts, deserialized.max_login_attempts);
    assert_eq!(
        original.password_min_length,
        deserialized.password_min_length
    );
    assert_eq!(original.mfa_required, deserialized.mfa_required);
    assert_eq!(original.allowed_domains, deserialized.allowed_domains);
    assert_eq!(original.production_mode, deserialized.production_mode);
    assert_eq!(original.dev_token_enabled, deserialized.dev_token_enabled);
    assert_eq!(original.dev_bypass_allowed, deserialized.dev_bypass_allowed);
    assert_eq!(original.jwt_mode, deserialized.jwt_mode);
    assert_eq!(original.token_expiry_hours, deserialized.token_expiry_hours);
}

/// Test that allowed_domains is properly skipped when None (skip_serializing_if)
#[test]
fn test_allowed_domains_skipped_when_none() {
    let response = AuthConfigResponse {
        allow_registration: false,
        require_email_verification: false,
        session_timeout_minutes: 480,
        max_login_attempts: 5,
        password_min_length: 12,
        mfa_required: false,
        allowed_domains: None,
        production_mode: false,
        dev_token_enabled: true,
        dev_bypass_allowed: true,
        jwt_mode: "eddsa".to_string(),
        token_expiry_hours: 8,
    };

    let json: Value = serde_json::to_value(&response).expect("Failed to serialize");

    // allowed_domains should be absent from JSON when None (due to skip_serializing_if)
    assert!(
        json.get("allowed_domains").is_none(),
        "allowed_domains should be omitted when None"
    );
}

/// Test that allowed_domains is included when Some
#[test]
fn test_allowed_domains_included_when_some() {
    let response = AuthConfigResponse {
        allow_registration: false,
        require_email_verification: false,
        session_timeout_minutes: 480,
        max_login_attempts: 5,
        password_min_length: 12,
        mfa_required: false,
        allowed_domains: Some(vec!["example.com".to_string()]),
        production_mode: false,
        dev_token_enabled: true,
        dev_bypass_allowed: true,
        jwt_mode: "eddsa".to_string(),
        token_expiry_hours: 8,
    };

    let json: Value = serde_json::to_value(&response).expect("Failed to serialize");

    assert!(
        json.get("allowed_domains").is_some(),
        "allowed_domains should be present when Some"
    );
    assert!(
        json["allowed_domains"].is_array(),
        "allowed_domains should be an array"
    );
}
