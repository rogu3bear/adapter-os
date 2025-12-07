//! End-to-end auth flow coverage: login, refresh, CSRF, tenant guard, and
//! error envelope behaviors.

mod common;

use adapteros_api_types::auth::{LoginRequest, UserInfoResponse};
use adapteros_db::users::Role;
use adapteros_server_api::auth::{hash_password, Claims};
use adapteros_server_api::handlers::auth::auth_me;
use adapteros_server_api::handlers::auth_enhanced::{login_handler, refresh_token_handler};
use adapteros_server_api::ip_extraction::ClientIp;
use adapteros_server_api::middleware::{
    auth_middleware, csrf_middleware, observability::observability_middleware,
    request_id::request_id_middleware, tenant_route_guard_middleware,
};
use adapteros_server_api::request_id::REQUEST_ID_HEADER;
use adapteros_server_api::types::{ApiErrorBody, ErrorResponse};
use axum::body::{to_bytes, Body};
use axum::http::{header, HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::routing::{get, post};
use axum::{Json, Router};
use tower::ServiceExt;

use common::setup_state;

fn cookie_value(set_cookie: &str, name: &str) -> Option<String> {
    set_cookie
        .split(';')
        .next()
        .and_then(|pair| pair.split_once('='))
        .filter(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

fn find_cookie<'a>(cookies: &'a [String], name: &str) -> Option<&'a String> {
    cookies.iter().find(|c| c.starts_with(&format!("{name}=")))
}

fn collect_cookies(headers: &HeaderMap) -> Vec<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .collect()
}

#[tokio::test]
async fn login_and_me_with_cookie_tokens() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    {
        let mut cfg = state.config.write().unwrap();
        cfg.server.production_mode = true;
        cfg.security.cookie_secure = Some(true);
        cfg.security.cookie_same_site = Some("Lax".to_string());
    }

    let pw_hash = hash_password("p@ssword!")?;
    state
        .db
        .create_user(
            "flow@example.com",
            "Flow User",
            &pw_hash,
            Role::Admin,
            "tenant-1",
        )
        .await?;

    let login_req = LoginRequest {
        username: None,
        email: "flow@example.com".to_string(),
        password: "p@ssword!".to_string(),
        device_id: None,
        totp_code: None,
    };
    let mut login_headers = HeaderMap::new();
    login_headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static("auth-flow-test"),
    );

    let (set_cookie_headers, Json(login_body)) = login_handler(
        axum::extract::State(state.clone()),
        login_headers,
        axum::Extension(ClientIp("127.0.0.1".into())),
        Json(login_req),
    )
    .await
    .expect("login should succeed");

    let cookies = collect_cookies(&set_cookie_headers);
    let auth_cookie = find_cookie(&cookies, "auth_token").expect("auth_token cookie");
    let refresh_cookie = find_cookie(&cookies, "refresh_token").expect("refresh_token cookie");

    assert!(
        auth_cookie.contains("HttpOnly") && refresh_cookie.contains("HttpOnly"),
        "auth and refresh cookies must be HttpOnly"
    );
    assert!(
        auth_cookie.contains("Secure") && refresh_cookie.contains("Secure"),
        "Secure flag should be set when cookie_secure is true"
    );
    assert!(
        auth_cookie.contains("SameSite=Lax") && refresh_cookie.contains("SameSite=Lax"),
        "SameSite=Lax expected by default"
    );

    let auth_value = cookie_value(auth_cookie, "auth_token").expect("auth cookie value");
    let refresh_value =
        cookie_value(refresh_cookie, "refresh_token").expect("refresh cookie value");
    let cookie_header = format!("auth_token={auth_value}; refresh_token={refresh_value}");

    let me_app = Router::new()
        .route("/v1/auth/me", get(auth_me))
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let me_response = me_app
        .oneshot(
            Request::builder()
                .uri("/v1/auth/me")
                .header(header::COOKIE, cookie_header)
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(me_response.status(), StatusCode::OK);

    let body_bytes = to_bytes(me_response.into_body(), 16 * 1024).await?;
    let user: UserInfoResponse = serde_json::from_slice(&body_bytes)?;
    assert_eq!(user.email, "flow@example.com");
    assert_eq!(user.tenant_id, "tenant-1");
    assert_eq!(user.role.to_lowercase(), "admin");
    assert!(user.admin_tenants.is_empty());
    assert!(
        !user.permissions.is_empty(),
        "permissions should be populated"
    );
    assert_eq!(login_body.token, auth_value, "body token matches cookie");

    Ok(())
}

#[tokio::test]
async fn refresh_flow_sets_new_tokens_and_csrf_cookie() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    {
        let mut cfg = state.config.write().unwrap();
        cfg.server.production_mode = true;
        cfg.security.cookie_secure = Some(true);
        cfg.security.cookie_same_site = Some("Lax".to_string());
    }

    let pw_hash = hash_password("refresh-me!")?;
    state
        .db
        .create_user(
            "refresh@example.com",
            "Refresh User",
            &pw_hash,
            Role::Admin,
            "tenant-1",
        )
        .await?;

    let login_req = LoginRequest {
        username: None,
        email: "refresh@example.com".to_string(),
        password: "refresh-me!".to_string(),
        device_id: None,
        totp_code: None,
    };
    let (login_headers, Json(login_body)) = login_handler(
        axum::extract::State(state.clone()),
        HeaderMap::new(),
        axum::Extension(ClientIp("127.0.0.1".into())),
        Json(login_req),
    )
    .await
    .expect("login should succeed");

    let login_cookies = collect_cookies(&login_headers);
    let refresh_cookie = find_cookie(&login_cookies, "refresh_token").expect("refresh cookie");
    let refresh_value = cookie_value(refresh_cookie, "refresh_token").expect("refresh token value");

    let mut refresh_headers = HeaderMap::new();
    refresh_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("refresh_token={refresh_value}"))?,
    );

    // Ensure refreshed tokens get a distinct iat from the initial login issuance
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    let (refresh_set_cookies, Json(refresh_body)) =
        refresh_token_handler(axum::extract::State(state.clone()), refresh_headers)
            .await
            .expect("refresh should succeed");

    let new_cookies = collect_cookies(&refresh_set_cookies);
    let refreshed_auth = find_cookie(&new_cookies, "auth_token").expect("new auth cookie");
    let refreshed_refresh = find_cookie(&new_cookies, "refresh_token").expect("new refresh cookie");
    let csrf_cookie = find_cookie(&new_cookies, "csrf_token").expect("csrf cookie");

    assert_ne!(
        refresh_body.token, login_body.token,
        "access token should rotate on refresh"
    );
    assert!(refreshed_auth.contains("HttpOnly"));
    assert!(refreshed_refresh.contains("HttpOnly"));
    assert!(refreshed_auth.contains("Secure"));
    assert!(refreshed_refresh.contains("Secure"));
    assert!(
        csrf_cookie.contains("SameSite"),
        "csrf cookie should include SameSite"
    );
    assert!(
        !csrf_cookie.contains("HttpOnly"),
        "csrf cookie must be readable for double-submit"
    );

    Ok(())
}

#[tokio::test]
async fn csrf_protects_unsafe_requests() {
    let app = Router::new()
        .route("/protected", post(|| async { StatusCode::OK }))
        .layer(middleware::from_fn(csrf_middleware));

    let missing_header_req = Request::builder()
        .method("POST")
        .uri("/protected")
        .header("Cookie", "auth_token=jwt-abc; csrf_token=csrf-123")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(missing_header_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let err: ErrorResponse =
        serde_json::from_slice(&to_bytes(resp.into_body(), 1024).await.unwrap())
            .expect("parse error body");
    assert_eq!(err.code, "CSRF_ERROR");

    let ok_req = Request::builder()
        .method("POST")
        .uri("/protected")
        .header("Cookie", "auth_token=jwt-abc; csrf_token=csrf-123")
        .header("X-CSRF-Token", "csrf-123")
        .body(Body::empty())
        .unwrap();
    let ok_resp = app.oneshot(ok_req).await.unwrap();
    assert_eq!(ok_resp.status(), StatusCode::OK);
}

fn tenant_app(claims: Claims) -> Router {
    let inject_claims = move |mut req: Request<Body>, next: Next| {
        let c = claims.clone();
        async move {
            req.extensions_mut().insert(c);
            next.run(req).await
        }
    };

    Router::new()
        .route(
            "/v1/tenants/{tenant_id}/router/config",
            get(|| async { StatusCode::OK }),
        )
        .layer(middleware::from_fn(tenant_route_guard_middleware))
        .layer(middleware::from_fn(inject_claims))
}

#[tokio::test]
async fn tenant_guard_allows_same_tenant() {
    let mut claims = common::test_viewer_claims();
    claims.tenant_id = "tenant-1".to_string();
    let app = tenant_app(claims);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/tenants/tenant-1/router/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn tenant_guard_blocks_cross_tenant_non_admin() {
    let mut claims = common::test_viewer_claims();
    claims.tenant_id = "tenant-a".to_string();
    let app = tenant_app(claims);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/tenants/tenant-b/router/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body_bytes = to_bytes(resp.into_body(), 1024).await.unwrap();
    let err: ErrorResponse = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(err.code, "TENANT_ISOLATION_ERROR");
}

#[tokio::test]
async fn tenant_guard_allows_admin_with_grant() {
    let mut claims = common::test_admin_claims();
    claims.admin_tenants = vec!["tenant-b".to_string()];
    claims.tenant_id = "system".to_string();
    let app = tenant_app(claims);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/tenants/tenant-b/router/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn unauthorized_errors_include_request_id_envelope() -> anyhow::Result<()> {
    let state = setup_state(None).await?;

    let app = Router::new()
        .route("/protected", get(|| async { StatusCode::OK }))
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(middleware::from_fn(observability_middleware))
        .layer(middleware::from_fn(request_id_middleware));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/protected")
                .body(Body::empty())
                .unwrap(),
        )
        .await?;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let (parts, body) = resp.into_parts();
    let request_id_header = parts
        .headers
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .expect("request id header");
    let err: ApiErrorBody = serde_json::from_slice(&to_bytes(body, 1024).await?)?;
    assert_eq!(err.code, "UNAUTHORIZED");
    assert_eq!(err.request_id, request_id_header);

    Ok(())
}
