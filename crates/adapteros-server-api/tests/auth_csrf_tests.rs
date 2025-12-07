//! CSRF and cookie auth end-to-end verification.
//!
//! Covers:
//! - Login sets auth, refresh, and csrf cookies with expected attributes.
//! - CSRF middleware blocks unsafe requests with cookies but missing header.
//! - CSRF middleware allows matching header.
//! - CLI-style Authorization header (no cookies) is not blocked.

mod common;

use adapteros_api_types::auth::LoginRequest;
use adapteros_db::users::Role;
use adapteros_server_api::auth::hash_password;
use adapteros_server_api::handlers::auth::auth_login;
use adapteros_server_api::middleware::csrf_middleware;
use adapteros_server_api::types::ErrorResponse;
use axum::body::to_bytes;
use axum::body::Body;
use axum::http::header::SET_COOKIE;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::routing::post;
use axum::{extract::State, Json, Router};
use tower::ServiceExt;

fn csrf_app() -> Router {
    Router::new()
        .route("/csrf-protected", post(|| async { StatusCode::OK }))
        .layer(middleware::from_fn(csrf_middleware))
}

#[tokio::test]
async fn login_sets_auth_refresh_and_csrf_cookies() -> anyhow::Result<()> {
    let state = common::setup_state(None).await?;
    let pw_hash = hash_password("p@ssword!")?;

    state
        .db
        .create_user(
            "csrf@example.com",
            "CSRF Test",
            &pw_hash,
            Role::Admin,
            "tenant-1",
        )
        .await?;

    let request = LoginRequest {
        username: None,
        email: "csrf@example.com".to_string(),
        password: "p@ssword!".to_string(),
        device_id: None,
        totp_code: None,
    };

    let (headers, Json(_body)) = auth_login(State(state.clone()), Json(request))
        .await
        .expect("login should succeed");

    let cookies: Vec<String> = headers
        .get_all(SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect();

    let auth_cookie = cookies
        .iter()
        .find(|c| c.starts_with("auth_token="))
        .expect("auth_token cookie present");
    let refresh_cookie = cookies
        .iter()
        .find(|c| c.starts_with("refresh_token="))
        .expect("refresh_token cookie present");
    let csrf_cookie = cookies
        .iter()
        .find(|c| c.starts_with("csrf_token="))
        .expect("csrf_token cookie present");

    assert!(
        auth_cookie.contains("HttpOnly") && refresh_cookie.contains("HttpOnly"),
        "auth/refresh cookies must be HttpOnly"
    );
    assert!(
        !csrf_cookie.contains("HttpOnly"),
        "csrf cookie must be readable by JS for double-submit"
    );
    assert!(
        csrf_cookie.contains("SameSite") && csrf_cookie.contains("Max-Age"),
        "csrf cookie should include SameSite and Max-Age"
    );

    Ok(())
}

#[tokio::test]
async fn csrf_blocks_missing_header_when_cookies_present() {
    let app = csrf_app();
    let req = Request::builder()
        .method("POST")
        .uri("/csrf-protected")
        .header("Cookie", "auth_token=jwt-abc; csrf_token=csrf-123")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let body_bytes = to_bytes(resp.into_body(), 16 * 1024).await.unwrap();
    let err: ErrorResponse = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(err.code, "CSRF_ERROR");
}

#[tokio::test]
async fn csrf_allows_matching_header() {
    let app = csrf_app();
    let req = Request::builder()
        .method("POST")
        .uri("/csrf-protected")
        .header("Cookie", "auth_token=jwt-abc; csrf_token=csrf-123")
        .header("X-CSRF-Token", "csrf-123")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn csrf_does_not_block_authorization_header_only() {
    let app = csrf_app();
    let req = Request::builder()
        .method("POST")
        .uri("/csrf-protected")
        .header("Authorization", "Bearer cli-token")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
