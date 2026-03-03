//! Integration tests for auth cookie flow
//!
//! These tests verify that:
//! 1. Login sets httpOnly cookies (auth_token, refresh_token, csrf_token)
//! 2. Logout clears httpOnly cookies
//! 3. Refresh updates auth_token cookie

mod common;

use adapteros_api_types::auth::LoginRequest;
use adapteros_server_api::routes;
use axum::{
    body::Body,
    http::{header::HeaderValue, Request, StatusCode},
};
use tower::ServiceExt;

/// Test that login handler returns Set-Cookie headers for auth
#[tokio::test]
async fn test_login_sets_auth_cookies() {
    let _guard = common::TestkitEnvGuard::enabled(false).await;
    let state = common::setup_state(None)
        .await
        .expect("Failed to setup state");

    // Create a test user with a known password
    let password = "test-password-123";
    let email = "cookie-test@example.com";
    let user_id = "cookie-test-user";

    // Hash the password using Argon2id
    let pw_hash =
        adapteros_server_api::auth::hash_password(password).expect("Failed to hash password");

    // Insert user into database
    adapteros_db::sqlx::query(
        "INSERT OR REPLACE INTO users (id, email, display_name, pw_hash, role, tenant_id)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(email)
    .bind("Cookie Test User")
    .bind(&pw_hash)
    .bind("admin")
    .bind("default")
    .execute(state.db.pool_result().expect("db pool"))
    .await
    .expect("Failed to insert test user");

    // Build the app router
    let app = routes::build(state.clone());

    // Create login request
    let login_req = LoginRequest {
        email: email.to_string(),
        password: password.to_string(),
        username: None,
        device_id: None,
        totp_code: None,
    };

    let request = Request::builder()
        .method("POST")
        .uri("/v1/auth/login")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&login_req).unwrap()))
        .unwrap();

    // Make the request
    let response = app.oneshot(request).await.expect("Failed to make request");

    // Verify response status
    assert_eq!(response.status(), StatusCode::OK, "Login should succeed");

    // Collect all Set-Cookie headers
    let cookies: Vec<String> = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v: &HeaderValue| v.to_str().ok())
        .map(|s: &str| s.to_string())
        .collect();

    // Verify auth_token cookie is set
    let has_auth_token = cookies
        .iter()
        .any(|c: &String| c.starts_with("auth_token="));
    assert!(
        has_auth_token,
        "Login should set auth_token cookie. Cookies: {:?}",
        cookies
    );

    // Verify refresh_token cookie is set
    let has_refresh_token = cookies
        .iter()
        .any(|c: &String| c.starts_with("refresh_token="));
    assert!(
        has_refresh_token,
        "Login should set refresh_token cookie. Cookies: {:?}",
        cookies
    );

    // Verify csrf_token cookie is set
    let has_csrf_token = cookies
        .iter()
        .any(|c: &String| c.starts_with("csrf_token="));
    assert!(
        has_csrf_token,
        "Login should set csrf_token cookie. Cookies: {:?}",
        cookies
    );

    // Verify auth_token has HttpOnly flag
    let auth_cookie = cookies
        .iter()
        .find(|c: &&String| c.starts_with("auth_token="))
        .expect("auth_token should exist");
    assert!(
        auth_cookie.contains("HttpOnly"),
        "auth_token should be HttpOnly"
    );

    // Verify refresh_token has HttpOnly flag
    let refresh_cookie = cookies
        .iter()
        .find(|c: &&String| c.starts_with("refresh_token="))
        .expect("refresh_token should exist");
    assert!(
        refresh_cookie.contains("HttpOnly"),
        "refresh_token should be HttpOnly"
    );

    // Verify csrf_token does NOT have HttpOnly (needed for JS access)
    let csrf_cookie = cookies
        .iter()
        .find(|c: &&String| c.starts_with("csrf_token="))
        .expect("csrf_token should exist");
    assert!(
        !csrf_cookie.contains("HttpOnly"),
        "csrf_token should NOT be HttpOnly so JavaScript can read it"
    );
}

/// Test that logout handler clears auth cookies
#[tokio::test]
async fn test_logout_clears_auth_cookies() {
    let _guard = common::TestkitEnvGuard::enabled(false).await;
    let state = common::setup_state(None)
        .await
        .expect("Failed to setup state");

    // Create a test user with a known password
    let password = "test-password-123";
    let email = "logout-test@example.com";
    let user_id = "logout-test-user";

    let pw_hash =
        adapteros_server_api::auth::hash_password(password).expect("Failed to hash password");

    adapteros_db::sqlx::query(
        "INSERT OR REPLACE INTO users (id, email, display_name, pw_hash, role, tenant_id)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(email)
    .bind("Logout Test User")
    .bind(&pw_hash)
    .bind("admin")
    .bind("default")
    .execute(state.db.pool_result().expect("db pool"))
    .await
    .expect("Failed to insert test user");

    let app = routes::build(state.clone());

    // First, login to get cookies
    let login_req = LoginRequest {
        email: email.to_string(),
        password: password.to_string(),
        username: None,
        device_id: None,
        totp_code: None,
    };

    let login_request = Request::builder()
        .method("POST")
        .uri("/v1/auth/login")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&login_req).unwrap()))
        .unwrap();

    let login_response = app
        .clone()
        .oneshot(login_request)
        .await
        .expect("Login failed");
    assert_eq!(login_response.status(), StatusCode::OK);

    // Extract auth_token cookie for the logout request
    let login_cookies: Vec<String> = login_response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v: &HeaderValue| v.to_str().ok())
        .map(|s: &str| s.to_string())
        .collect();

    let auth_token_cookie = login_cookies
        .iter()
        .find(|c: &&String| c.starts_with("auth_token="))
        .expect("Should have auth_token");

    // Extract the csrf_token cookie for CSRF validation
    let csrf_token_cookie = login_cookies
        .iter()
        .find(|c: &&String| c.starts_with("csrf_token="))
        .expect("Should have csrf_token");

    // Extract just the cookie values (before the first semicolon)
    let auth_token_value = auth_token_cookie
        .split(';')
        .next()
        .expect("Cookie should have value");

    let csrf_token_value = csrf_token_cookie
        .split(';')
        .next()
        .expect("Cookie should have value");

    // Extract the actual CSRF token value (after the =)
    let csrf_token = csrf_token_value
        .strip_prefix("csrf_token=")
        .expect("Should have csrf_token= prefix");

    // Build new app instance for logout
    let app = routes::build(state.clone());

    // Combine both cookies in the Cookie header
    let cookie_header = format!("{}; {}", auth_token_value, csrf_token_value);

    // Now logout with the auth cookie and CSRF token
    let logout_request = Request::builder()
        .method("POST")
        .uri("/v1/auth/logout")
        .header("Cookie", &cookie_header)
        .header("X-CSRF-Token", csrf_token)
        .body(Body::empty())
        .unwrap();

    let logout_response = app.oneshot(logout_request).await.expect("Logout failed");

    // Logout should succeed
    assert_eq!(
        logout_response.status(),
        StatusCode::OK,
        "Logout should succeed"
    );

    // Collect Set-Cookie headers from logout response
    let logout_cookies: Vec<String> = logout_response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v: &HeaderValue| v.to_str().ok())
        .map(|s: &str| s.to_string())
        .collect();

    // Verify cookies are cleared (Max-Age=0 or empty value)
    let auth_cleared = logout_cookies.iter().any(|c: &String| {
        c.starts_with("auth_token=") && (c.contains("Max-Age=0") || c.contains("auth_token=;"))
    });
    assert!(
        auth_cleared,
        "Logout should clear auth_token cookie. Cookies: {:?}",
        logout_cookies
    );

    let refresh_cleared = logout_cookies.iter().any(|c: &String| {
        c.starts_with("refresh_token=")
            && (c.contains("Max-Age=0") || c.contains("refresh_token=;"))
    });
    assert!(
        refresh_cleared,
        "Logout should clear refresh_token cookie. Cookies: {:?}",
        logout_cookies
    );
}

/// Test that login with invalid credentials does not set cookies
#[tokio::test]
async fn test_login_invalid_credentials_no_cookies() {
    let _guard = common::TestkitEnvGuard::enabled(false).await;
    let state = common::setup_state(None)
        .await
        .expect("Failed to setup state");

    let app = routes::build(state.clone());

    let login_req = LoginRequest {
        email: "nonexistent@example.com".to_string(),
        password: "wrong-password".to_string(),
        username: None,
        device_id: None,
        totp_code: None,
    };

    let request = Request::builder()
        .method("POST")
        .uri("/v1/auth/login")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&login_req).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.expect("Request failed");

    // Should return 401
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Should NOT set any auth cookies
    let cookies: Vec<String> = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v: &HeaderValue| v.to_str().ok())
        .map(|s: &str| s.to_string())
        .collect();

    let has_auth_token = cookies
        .iter()
        .any(|c: &String| c.starts_with("auth_token=") && !c.contains("=;"));
    assert!(
        !has_auth_token,
        "Failed login should not set auth_token cookie"
    );
}
