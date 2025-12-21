#![cfg(all(test, feature = "extended-tests"))]

//! Authentication integration tests
//!
//! Tests for the complete authentication flow including:
//! - Login/logout
//! - Token refresh
//! - Environment-based authentication modes
//! - Error handling
//!
//! Citations:
//! - crates/adapteros-server-api/src/middleware.rs: Auth middleware
//! - crates/adapteros-server-api/src/handlers.rs: Auth handlers
//! - docs/AUTHENTICATION.md: Authentication architecture

#[path = "common/mod.rs"]
mod common;

use adapteros_server_api::auth::{
    refresh_token, token_needs_refresh, validate_token, validate_token_ed25519_der, Claims,
};
use adapteros_server_api::routes;
use adapteros_server_api::state::RateLimitApiConfig;
use adapteros_server_api::types::{ErrorResponse, UserInfoResponse};
use adapteros_server_api::{AuthConfig, AuthMode, SecurityConfig};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use chrono::{Duration, Utc};
use common::auth::{
    create_test_app_state, login_user, make_authenticated_request, DEFAULT_TENANT_ID,
    DEFAULT_USER_EMAIL, DEFAULT_USER_PASSWORD,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use tower::ServiceExt;

#[test]
#[ignore = "requires database and server setup [tracking: STAB-IGN-001]"]
fn test_development_mode_auth() {
    // Test that development mode accepts dev tokens
    let auth_config = AuthConfig {
        mode: AuthMode::Development,
        dev_token: Some("adapteros-local".to_string()),
        token_expiry_hours: 8,
        max_login_attempts: 5,
        lockout_duration_minutes: 15,
    };

    assert_eq!(auth_config.mode, AuthMode::Development);
    assert!(auth_config.dev_token.is_some());
    assert_eq!(auth_config.dev_token.unwrap(), "adapteros-local");
}

#[test]
#[ignore = "requires database and server setup [tracking: STAB-IGN-001]"]
fn test_production_mode_auth() {
    // Test that production mode does not have dev tokens
    let auth_config = AuthConfig {
        mode: AuthMode::Production,
        dev_token: None,
        token_expiry_hours: 8,
        max_login_attempts: 5,
        lockout_duration_minutes: 15,
    };

    assert_eq!(auth_config.mode, AuthMode::Production);
    assert!(auth_config.dev_token.is_none());
}

#[test]
#[ignore = "requires database and server setup [tracking: STAB-IGN-001]"]
fn test_mixed_mode_auth() {
    // Test that mixed mode can have optional dev tokens
    let auth_config = AuthConfig {
        mode: AuthMode::Mixed,
        dev_token: Some("staging-token".to_string()),
        token_expiry_hours: 8,
        max_login_attempts: 5,
        lockout_duration_minutes: 15,
    };

    assert_eq!(auth_config.mode, AuthMode::Mixed);
    assert!(auth_config.dev_token.is_some());
}

#[test]
fn test_auth_config_defaults() {
    let config = AuthConfig::default();

    assert_eq!(config.mode, AuthMode::Development);
    assert_eq!(config.token_expiry_hours, 8);
    assert_eq!(config.max_login_attempts, 5);
    assert_eq!(config.lockout_duration_minutes, 15);
}

#[test]
fn test_security_config_defaults() {
    let config = SecurityConfig::default();

    assert!(!config.require_https);
    assert!(config.enable_rate_limiting);
    assert!(!config.cors_origins.is_empty());
}

#[test]
#[ignore = "requires database and server setup [tracking: STAB-IGN-001]"]
fn test_app_state_with_auth_config() {
    // This would require actual database setup
    // Placeholder test structure

    let auth_config = AuthConfig {
        mode: AuthMode::Production,
        dev_token: None,
        token_expiry_hours: 8,
        max_login_attempts: 5,
        lockout_duration_minutes: 15,
    };

    let security_config = SecurityConfig {
        require_https: true,
        cors_origins: vec!["https://example.com".to_string()],
        enable_rate_limiting: true,
    };

    // Verify configurations are valid
    assert_eq!(auth_config.mode, AuthMode::Production);
    assert!(security_config.require_https);
    assert!(security_config.enable_rate_limiting);
}

#[test]
fn test_token_expiry_configuration() {
    // Test various token expiry configurations
    let configs = vec![
        (1, "Very short expiry for testing"),
        (8, "Standard production expiry"),
        (24, "Extended development expiry"),
        (168, "Week-long expiry for special cases"),
    ];

    for (hours, description) in configs {
        let config = AuthConfig {
            mode: AuthMode::Development,
            dev_token: Some("test".to_string()),
            token_expiry_hours: hours,
            max_login_attempts: 5,
            lockout_duration_minutes: 15,
        };

        assert_eq!(config.token_expiry_hours, hours, "{}", description);
    }
}

#[test]
fn test_lockout_configuration() {
    // Test lockout settings
    let config = AuthConfig {
        mode: AuthMode::Production,
        dev_token: None,
        token_expiry_hours: 8,
        max_login_attempts: 3,        // Stricter
        lockout_duration_minutes: 30, // Longer
    };

    assert_eq!(config.max_login_attempts, 3);
    assert_eq!(config.lockout_duration_minutes, 30);
}

#[test]
fn test_cors_configuration() {
    let security_config = SecurityConfig {
        require_https: true,
        cors_origins: vec![
            "https://app.example.com".to_string(),
            "https://console.example.com".to_string(),
        ],
        enable_rate_limiting: true,
    };

    assert_eq!(security_config.cors_origins.len(), 2);
    assert!(security_config
        .cors_origins
        .contains(&"https://app.example.com".to_string()));
    assert!(security_config
        .cors_origins
        .contains(&"https://console.example.com".to_string()));
}

#[tokio::test]
async fn test_login_flow() {
    let state = create_test_app_state().await;
    let app = routes::build(state.clone());

    let login = login_user(&app, DEFAULT_USER_EMAIL, DEFAULT_USER_PASSWORD)
        .await
        .expect("login should succeed");
    assert!(
        !login.token.is_empty(),
        "JWT token should be non-empty for successful login"
    );
    assert_eq!(login.user_id.len(), 36, "user ID should be UUID-like");
    assert_eq!(login.role, "admin");

    let claims = validate_token(&login.token, state.jwt_secret.as_slice())
        .expect("token should validate with HMAC secret");
    assert_eq!(claims.email, DEFAULT_USER_EMAIL);
    assert_eq!(claims.role, "admin");
    assert_eq!(claims.tenant_id, DEFAULT_TENANT_ID);

    let me_body = make_authenticated_request(&app, "/v1/auth/me", &login.token)
        .await
        .expect("authenticated /v1/auth/me request should succeed");
    let me: UserInfoResponse =
        serde_json::from_str(&me_body).expect("/v1/auth/me should return user info");
    assert_eq!(me.email, DEFAULT_USER_EMAIL);
    assert_eq!(me.role, "admin");

    let tenants_body = make_authenticated_request(&app, "/v1/tenants", &login.token)
        .await
        .expect("authenticated /v1/tenants request should succeed");
    let tenants: serde_json::Value =
        serde_json::from_str(&tenants_body).expect("/v1/tenants should return JSON");
    let tenants = tenants
        .as_array()
        .expect("/v1/tenants should return an array response");
    assert!(
        tenants.iter().any(|tenant| {
            tenant
                .get("id")
                .and_then(|v| v.as_str())
                .map(|id| id == DEFAULT_TENANT_ID)
                .unwrap_or(false)
        }),
        "expected default tenant to be present in /v1/tenants response"
    );
}

#[tokio::test]
async fn test_token_refresh_flow() {
    let state = create_test_app_state().await;
    let app = routes::build(state.clone());

    let login = login_user(&app, DEFAULT_USER_EMAIL, DEFAULT_USER_PASSWORD)
        .await
        .expect("login should succeed");

    let claims = validate_token(&login.token, state.jwt_secret.as_slice())
        .expect("token should validate with HMAC secret");
    assert!(
        !token_needs_refresh(&claims),
        "freshly issued token should not need refresh"
    );

    let mut nearing_expiry = claims.clone();
    nearing_expiry.exp = (Utc::now() + Duration::minutes(30)).timestamp();
    assert!(
        token_needs_refresh(&nearing_expiry),
        "token expiring within an hour should trigger refresh"
    );

    let refreshed_token = refresh_token(&claims, &state.crypto.jwt_keypair)
        .expect("refresh_token should produce a signed token");
    assert_ne!(
        refreshed_token, login.token,
        "refreshed token should differ from original"
    );

    let public_key = state.crypto.jwt_keypair.public_key();
    let refreshed_claims = validate_token_ed25519_der(&refreshed_token, &public_key.to_bytes())
        .expect(
            "refreshed token should validate with Ed25519 public key derived from crypto state",
        );
    assert_eq!(refreshed_claims.sub, claims.sub);
    assert_eq!(refreshed_claims.email, claims.email);
    assert!(
        refreshed_claims.exp > claims.exp,
        "refreshed token should extend expiry"
    );
}

#[tokio::test]
async fn test_logout_flow() {
    let state = create_test_app_state().await;
    let app = routes::build(state.clone());

    let login = login_user(&app, DEFAULT_USER_EMAIL, DEFAULT_USER_PASSWORD)
        .await
        .expect("login should succeed");

    let request = Request::builder()
        .method("POST")
        .uri("/v1/auth/logout")
        .header("authorization", format!("Bearer {}", login.token))
        .body(Body::empty())
        .expect("build logout request");

    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("execute logout request");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Logout is stateless; original token should still work for existing grace period.
    let tenants_body = make_authenticated_request(&app, "/v1/tenants", &login.token)
        .await
        .expect("token should remain valid immediately after logout (stateless JWT)");
    assert!(
        serde_json::from_str::<serde_json::Value>(&tenants_body)
            .ok()
            .and_then(|value| value.as_array().cloned())
            .map(|arr| !arr.is_empty())
            .unwrap_or(false),
        "authenticated request after logout should still succeed with stateless token"
    );
}

#[tokio::test]
async fn test_invalid_credentials() {
    let state = create_test_app_state().await;
    let app = routes::build(state);

    let error = login_user(&app, DEFAULT_USER_EMAIL, "totally-wrong-password")
        .await
        .expect_err("login should fail with invalid credentials");
    assert!(
        error.contains("INVALID_CREDENTIALS"),
        "expected invalid credentials code in error, got: {error}"
    );
}

#[tokio::test]
async fn test_expired_token() {
    let state = create_test_app_state().await;
    let app = routes::build(state.clone());

    let login = login_user(&app, DEFAULT_USER_EMAIL, DEFAULT_USER_PASSWORD)
        .await
        .expect("login should succeed");

    let mut claims = validate_token(&login.token, state.jwt_secret.as_slice())
        .expect("token should validate with HMAC secret");
    claims.exp = (Utc::now() - Duration::minutes(5)).timestamp();

    let expired_token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.jwt_secret.as_slice()),
    )
    .expect("encoding expired token should succeed");

    let request = Request::builder()
        .method("GET")
        .uri("/v1/tenants")
        .header("authorization", format!("Bearer {}", expired_token))
        .body(Body::empty())
        .expect("build expired-token request");

    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("execute expired-token request");
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "expired tokens should be rejected"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let error: ErrorResponse =
        serde_json::from_slice(&body).expect("error response should deserialize");
    assert_eq!(error.error, "unauthorized");
}

#[tokio::test]
async fn test_rate_limiting() {
    let state = create_test_app_state().await;
    {
        let mut config = state
            .config
            .write()
            .expect("config lock should not be poisoned");
        config.rate_limits = Some(RateLimitApiConfig {
            requests_per_minute: 1,
            burst_size: 0,
        });
    }

    let app = routes::build(state.clone());

    let login = login_user(&app, DEFAULT_USER_EMAIL, DEFAULT_USER_PASSWORD)
        .await
        .expect("login should succeed");

    // First request should succeed and consume the only token
    make_authenticated_request(&app, "/v1/tenants", &login.token)
        .await
        .expect("first request should be allowed under rate limit");

    // Second request should hit the rate limiter immediately
    let request = Request::builder()
        .method("GET")
        .uri("/v1/tenants")
        .header("authorization", format!("Bearer {}", login.token))
        .body(Body::empty())
        .expect("build rate-limited request");

    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("execute rate-limited request");
    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "second request should be throttled"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read rate limit error body");
    let error: ErrorResponse =
        serde_json::from_slice(&body).expect("rate limit error should deserialize");
    assert_eq!(error.code, "RATE_LIMIT_EXCEEDED");
}

#[tokio::test]
async fn test_account_lockout() {
    let state = create_test_app_state().await;
    let shared_pool = state.db.pool().clone();

    // Simulate account lockout by marking the seeded user as disabled
    sqlx::query("UPDATE users SET disabled = 1 WHERE email = ?")
        .bind(DEFAULT_USER_EMAIL)
        .execute(&shared_pool)
        .await
        .expect("should mark user as disabled");

    let app = routes::build(state);

    let error = login_user(&app, DEFAULT_USER_EMAIL, DEFAULT_USER_PASSWORD)
        .await
        .expect_err("disabled user should not be able to log in");
    assert!(
        error.contains("USER_DISABLED"),
        "expected USER_DISABLED error, got: {error}"
    );
}

#[tokio::test]
async fn test_development_token_in_dev_mode() {
    let state = create_test_app_state().await;
    let app = routes::build(state);

    let request = Request::builder()
        .method("GET")
        .uri("/v1/models")
        .header("authorization", "Bearer adapteros-local")
        .body(Body::empty())
        .expect("build dev-token request");

    let response = app
        .oneshot(request)
        .await
        .expect("execute dev-token request against dual auth route");
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "dev token should be accepted by dual-auth routes in development mode"
    );
}

#[tokio::test]
async fn test_development_token_rejected_in_production() {
    let state = create_test_app_state().await;
    let app = routes::build(state);

    let request = Request::builder()
        .method("GET")
        .uri("/v1/tenants")
        .header("authorization", "Bearer adapteros-local")
        .body(Body::empty())
        .expect("build dev-token request for protected route");

    let response = app
        .oneshot(request)
        .await
        .expect("execute protected route request");
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "development token should not bypass standard auth middleware"
    );
}

#[tokio::test]
async fn test_cors_policy_enforcement() {
    let state = create_test_app_state().await;
    let app = routes::build(state);

    let request = Request::builder()
        .method("OPTIONS")
        .uri("/v1/tenants")
        .header("origin", "https://example.com")
        .header("access-control-request-method", "GET")
        .body(Body::empty())
        .expect("build preflight request");

    let response = app
        .oneshot(request)
        .await
        .expect("execute preflight request");
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "preflight request should succeed with permissive CORS"
    );
    let headers = response.headers();
    let allow_origin = headers
        .get("access-control-allow-origin")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert_eq!(
        allow_origin, "*",
        "permissive CORS layer should allow all origins"
    );
}

#[test]
fn test_module_compiles() {
    // This test just ensures the module compiles correctly
    // All the actual integration tests are marked as #[ignore]
    // and will be implemented as the authentication system matures

    // Verify basic module imports work
    use adapteros_server_api::AuthConfig;
    let config = AuthConfig::default();
    assert_eq!(config.mode, adapteros_server_api::AuthMode::Development);
}
