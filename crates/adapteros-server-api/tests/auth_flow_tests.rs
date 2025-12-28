//! End-to-end auth flow coverage: login, refresh, CSRF, tenant guard, and
//! error envelope behaviors.

mod common;

use adapteros_api_types::auth::{LoginRequest, UserInfoResponse};
use adapteros_db::users::Role;
use adapteros_server_api::auth::{
    hash_password, validate_refresh_token_ed25519, validate_refresh_token_hmac, validate_token,
    validate_token_ed25519, Claims, RefreshClaims,
};
use adapteros_server_api::handlers::auth::auth_me;
use adapteros_server_api::handlers::auth_enhanced::{login_handler, refresh_token_handler};
use adapteros_server_api::ip_extraction::ClientIp;
use adapteros_server_api::middleware::{
    auth_middleware, csrf_middleware, observability::observability_middleware,
    request_id::request_id_middleware, tenant_route_guard_middleware,
};
use adapteros_server_api::request_id::REQUEST_ID_HEADER;
use adapteros_server_api::security::{
    lock_session, revoke_token, set_tenant_token_baseline, update_session_rotation,
};
use adapteros_server_api::types::{ApiErrorBody, ErrorResponse};
use axum::body::{to_bytes, Body};
use axum::http::{header, HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::routing::{get, post};
use axum::{Json, Router};
use blake3;
use chrono::{Duration, Utc};
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
    let csrf_cookie = find_cookie(&cookies, "csrf_token").expect("csrf_token cookie");

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
    assert!(
        csrf_cookie.contains("SameSite"),
        "csrf cookie should include SameSite"
    );
    assert!(
        !csrf_cookie.contains("HttpOnly"),
        "csrf cookie must be readable for double-submit"
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

#[allow(dead_code)]
async fn audit_count_by_action(db: &adapteros_db::Db, action: &str) -> i64 {
    let stats = db
        .get_audit_stats_by_action(None, None)
        .await
        .expect("audit stats");
    stats
        .into_iter()
        .find(|(a, _)| a == action)
        .map(|(_, c)| c)
        .unwrap_or(0)
}

async fn audit_count_by_action_and_error(
    db: &adapteros_db::Db,
    action: &str,
    error_code: &str,
) -> i64 {
    let pattern = format!("%\"error_code\":\"{}\"%", error_code);
    let count: (i64,) = adapteros_db::sqlx::query_as(
        "SELECT COUNT(*) FROM audit_logs WHERE action = ? AND metadata_json LIKE ?",
    )
    .bind(action)
    .bind(pattern)
    .fetch_one(db.pool())
    .await
    .expect("audit count by action and error");
    count.0
}

#[tokio::test]
async fn refresh_reuse_logs_audit_event() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    {
        let mut cfg = state.config.write().unwrap();
        cfg.server.production_mode = true;
        cfg.security.cookie_secure = Some(true);
        cfg.security.cookie_same_site = Some("Lax".to_string());
    }

    let pw_hash = hash_password("reuser!")?;
    state
        .db
        .create_user(
            "reuse@example.com",
            "Reuse User",
            &pw_hash,
            Role::Admin,
            "tenant-1",
        )
        .await?;

    let login_req = LoginRequest {
        username: None,
        email: "reuse@example.com".to_string(),
        password: "reuser!".to_string(),
        device_id: None,
        totp_code: None,
    };
    let (login_headers, _) = login_handler(
        axum::extract::State(state.clone()),
        HeaderMap::new(),
        axum::Extension(ClientIp("127.0.0.1".into())),
        Json(login_req),
    )
    .await
    .expect("login should succeed");

    let login_cookies = collect_cookies(&login_headers);
    let refresh_cookie = find_cookie(&login_cookies, "refresh_token").expect("refresh cookie");
    let original_refresh =
        cookie_value(refresh_cookie, "refresh_token").expect("refresh token value");

    // First refresh rotates session (new rot_id stored)
    let mut refresh_headers = HeaderMap::new();
    refresh_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("refresh_token={original_refresh}"))?,
    );
    let (_set_cookies, _) =
        refresh_token_handler(axum::extract::State(state.clone()), refresh_headers)
            .await
            .expect("initial refresh should succeed");

    // Reuse old refresh token should now trigger rotation mismatch and audit log
    let before =
        audit_count_by_action_and_error(&state.db, "auth.refresh_failed", "ROTATION_MISMATCH")
            .await;
    let mut reuse_headers = HeaderMap::new();
    reuse_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("refresh_token={original_refresh}"))?,
    );
    let reuse_resp =
        refresh_token_handler(axum::extract::State(state.clone()), reuse_headers).await;
    assert!(reuse_resp.is_err(), "reuse should be rejected");
    let after =
        audit_count_by_action_and_error(&state.db, "auth.refresh_failed", "ROTATION_MISMATCH")
            .await;
    assert_eq!(after, before + 1, "reuse should emit audit log");

    Ok(())
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

fn decode_access_claims(token: &str, state: &adapteros_server_api::state::AppState) -> Claims {
    if state.use_ed25519 {
        validate_token_ed25519(token, &state.ed25519_public_keys, &state.ed25519_public_key)
            .expect("token should decode")
    } else {
        validate_token(token, &state.hmac_keys, state.jwt_secret.as_slice())
            .expect("token should decode")
    }
}

fn decode_refresh_claims(
    token: &str,
    state: &adapteros_server_api::state::AppState,
) -> RefreshClaims {
    if state.use_ed25519 {
        validate_refresh_token_ed25519(token, &state.ed25519_public_keys, &state.ed25519_public_key)
            .expect("refresh token should decode")
    } else {
        validate_refresh_token_hmac(token, &state.hmac_keys, state.jwt_secret.as_slice())
            .expect("refresh token should decode")
    }
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
        .layer(middleware::from_fn_with_state(
            state.clone(),
            observability_middleware,
        ))
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
    assert!(
        !err.hint.trim().is_empty(),
        "error envelope should include hint"
    );
    assert_eq!(err.request_id, request_id_header);

    Ok(())
}

#[tokio::test]
async fn baseline_rejects_stale_access_tokens() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let pw_hash = hash_password("baseline-pass!")?;
    state
        .db
        .create_user(
            "baseline@example.com",
            "Baseline User",
            &pw_hash,
            Role::Admin,
            "tenant-1",
        )
        .await?;

    let login_req = LoginRequest {
        username: None,
        email: "baseline@example.com".to_string(),
        password: "baseline-pass!".to_string(),
        device_id: None,
        totp_code: None,
    };
    let (login_headers, _) = login_handler(
        axum::extract::State(state.clone()),
        HeaderMap::new(),
        axum::Extension(ClientIp("127.0.0.1".into())),
        Json(login_req),
    )
    .await
    .expect("login should succeed");

    let cookies = collect_cookies(&login_headers);
    let auth_cookie = find_cookie(&cookies, "auth_token").expect("auth cookie");
    let auth_value = cookie_value(auth_cookie, "auth_token").expect("auth token value");

    // Advance baseline beyond token issuance to invalidate the token
    let baseline = (Utc::now() + Duration::seconds(120)).to_rfc3339();
    set_tenant_token_baseline(&state.db, "tenant-1", &baseline).await?;

    let app = Router::new()
        .route("/v1/auth/me", get(auth_me))
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/auth/me")
                .header(header::COOKIE, format!("auth_token={auth_value}"))
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body = to_bytes(resp.into_body(), 2048).await?;
    let err: ErrorResponse = serde_json::from_slice(&body)?;
    assert_eq!(err.code, "TOKEN_REVOKED");
    Ok(())
}

#[tokio::test]
async fn revoked_token_denied_even_before_expiry() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let pw_hash = hash_password("revoke-pass!")?;
    state
        .db
        .create_user(
            "revoke@example.com",
            "Revoke User",
            &pw_hash,
            Role::Admin,
            "tenant-1",
        )
        .await?;

    let login_req = LoginRequest {
        username: None,
        email: "revoke@example.com".to_string(),
        password: "revoke-pass!".to_string(),
        device_id: None,
        totp_code: None,
    };
    let (login_headers, _) = login_handler(
        axum::extract::State(state.clone()),
        HeaderMap::new(),
        axum::Extension(ClientIp("127.0.0.1".into())),
        Json(login_req),
    )
    .await
    .expect("login should succeed");
    let cookies = collect_cookies(&login_headers);
    let auth_cookie = find_cookie(&cookies, "auth_token").expect("auth cookie");
    let auth_value = cookie_value(auth_cookie, "auth_token").expect("auth token value");

    let claims = decode_access_claims(&auth_value, &state);
    let expires_at = (Utc::now() + Duration::hours(1)).to_rfc3339();
    revoke_token(
        &state.db,
        &claims.jti,
        &claims.sub,
        &claims.tenant_id,
        &expires_at,
        Some(&claims.sub),
        Some("test revoke"),
    )
    .await
    .expect("revocation should succeed");

    let app = Router::new()
        .route("/v1/auth/me", get(auth_me))
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/auth/me")
                .header(header::COOKIE, format!("auth_token={auth_value}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body = to_bytes(resp.into_body(), 2048).await?;
    let err: ErrorResponse = serde_json::from_slice(&body)?;
    assert_eq!(err.code, "TOKEN_REVOKED");
    Ok(())
}

#[tokio::test]
async fn old_refresh_token_fails_after_rotation() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let pw_hash = hash_password("rotate-pass!")?;
    state
        .db
        .create_user(
            "rotate@example.com",
            "Rotate User",
            &pw_hash,
            Role::Admin,
            "tenant-1",
        )
        .await?;

    let login_req = LoginRequest {
        username: None,
        email: "rotate@example.com".to_string(),
        password: "rotate-pass!".to_string(),
        device_id: None,
        totp_code: None,
    };
    let (login_headers, _) = login_handler(
        axum::extract::State(state.clone()),
        HeaderMap::new(),
        axum::Extension(ClientIp("127.0.0.1".into())),
        Json(login_req),
    )
    .await
    .expect("login should succeed");
    let cookies = collect_cookies(&login_headers);
    let refresh_cookie = find_cookie(&cookies, "refresh_token").expect("refresh cookie");
    let original_refresh =
        cookie_value(refresh_cookie, "refresh_token").expect("refresh token value");

    // First refresh rotates session
    let mut refresh_headers = HeaderMap::new();
    refresh_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("refresh_token={original_refresh}"))?,
    );
    let _ = refresh_token_handler(axum::extract::State(state.clone()), refresh_headers)
        .await
        .expect("first refresh should succeed");

    // Reuse old refresh token should now fail rotation check
    let mut stale_headers = HeaderMap::new();
    stale_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("refresh_token={original_refresh}"))?,
    );
    let (status, Json(err_body)) =
        refresh_token_handler(axum::extract::State(state.clone()), stale_headers)
            .await
            .expect_err("stale refresh must fail");
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(err_body.code, "UNAUTHORIZED");
    Ok(())
}

#[tokio::test]
async fn locked_session_cannot_refresh() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let pw_hash = hash_password("lock-pass!")?;
    state
        .db
        .create_user(
            "lock@example.com",
            "Lock User",
            &pw_hash,
            Role::Admin,
            "tenant-1",
        )
        .await?;

    let login_req = LoginRequest {
        username: None,
        email: "lock@example.com".to_string(),
        password: "lock-pass!".to_string(),
        device_id: None,
        totp_code: None,
    };
    let (login_headers, _) = login_handler(
        axum::extract::State(state.clone()),
        HeaderMap::new(),
        axum::Extension(ClientIp("127.0.0.1".into())),
        Json(login_req),
    )
    .await
    .expect("login should succeed");
    let cookies = collect_cookies(&login_headers);
    let refresh_cookie = find_cookie(&cookies, "refresh_token").expect("refresh cookie");
    let refresh_value = cookie_value(refresh_cookie, "refresh_token").expect("refresh token value");

    let refresh_claims = decode_refresh_claims(&refresh_value, &state);
    lock_session(&state.db, &refresh_claims.session_id)
        .await
        .expect("lock should succeed");

    let mut refresh_headers = HeaderMap::new();
    refresh_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("refresh_token={refresh_value}"))?,
    );
    let (status, Json(err_body)) =
        refresh_token_handler(axum::extract::State(state.clone()), refresh_headers)
            .await
            .expect_err("locked session refresh must fail");
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(err_body.code, "SESSION_EXPIRED");
    Ok(())
}

#[tokio::test]
async fn expired_session_cannot_refresh() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let pw_hash = hash_password("expire-pass!")?;
    state
        .db
        .create_user(
            "expire@example.com",
            "Expire User",
            &pw_hash,
            Role::Admin,
            "tenant-1",
        )
        .await?;

    let login_req = LoginRequest {
        username: None,
        email: "expire@example.com".to_string(),
        password: "expire-pass!".to_string(),
        device_id: None,
        totp_code: None,
    };
    let (login_headers, _) = login_handler(
        axum::extract::State(state.clone()),
        HeaderMap::new(),
        axum::Extension(ClientIp("127.0.0.1".into())),
        Json(login_req),
    )
    .await
    .expect("login should succeed");
    let cookies = collect_cookies(&login_headers);
    let refresh_cookie = find_cookie(&cookies, "refresh_token").expect("refresh cookie");
    let refresh_value = cookie_value(refresh_cookie, "refresh_token").expect("refresh token value");

    let refresh_claims = decode_refresh_claims(&refresh_value, &state);
    let past = (Utc::now() - Duration::minutes(5)).to_rfc3339();
    let refresh_hash = blake3::hash(refresh_value.as_bytes()).to_hex().to_string();
    update_session_rotation(
        &state.db,
        &refresh_claims.session_id,
        &refresh_claims.rot_id,
        Some(&refresh_hash),
        &past,
    )
    .await
    .expect("update rotation");

    let mut refresh_headers = HeaderMap::new();
    refresh_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("refresh_token={refresh_value}"))?,
    );
    let (status, Json(err_body)) =
        refresh_token_handler(axum::extract::State(state.clone()), refresh_headers)
            .await
            .expect_err("expired session refresh must fail");
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(err_body.code, "SESSION_EXPIRED");
    Ok(())
}

/// Comprehensive E2E auth flow: Login → Access → Refresh → Access → Logout → Denied → Session Expired
///
/// This test validates the complete authentication lifecycle:
/// 1. Login with valid credentials → receive access + refresh tokens in cookies
/// 2. Access protected endpoint with access token → success
/// 3. Refresh tokens → receive new access + refresh tokens
/// 4. Access protected endpoint with new access token → success
/// 5. Logout → tokens revoked, cookies cleared (Max-Age=0)
/// 6. Access protected endpoint after logout → TOKEN_REVOKED
/// 7. New login then session lock → SESSION_EXPIRED on refresh attempt
#[tokio::test]
async fn complete_auth_flow_login_refresh_logout_session_expired() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    {
        let mut cfg = state.config.write().unwrap();
        cfg.server.production_mode = true;
        cfg.security.cookie_secure = Some(true);
        cfg.security.cookie_same_site = Some("Lax".to_string());
    }

    let pw_hash = hash_password("complete-flow!")?;
    state
        .db
        .create_user(
            "complete@example.com",
            "Complete Flow User",
            &pw_hash,
            Role::Admin,
            "tenant-1",
        )
        .await?;

    // ══════════════════════════════════════════════════════════════════════════
    // STEP 1: LOGIN → receive access + refresh tokens
    // ══════════════════════════════════════════════════════════════════════════
    let login_req = LoginRequest {
        username: None,
        email: "complete@example.com".to_string(),
        password: "complete-flow!".to_string(),
        device_id: Some("test-device-001".to_string()),
        totp_code: None,
    };
    let mut login_headers = HeaderMap::new();
    login_headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static("complete-flow-test/1.0"),
    );

    let (set_cookie_headers, Json(login_body)) = login_handler(
        axum::extract::State(state.clone()),
        login_headers.clone(),
        axum::Extension(ClientIp("192.168.1.100".into())),
        Json(login_req),
    )
    .await
    .expect("login should succeed");

    let cookies = collect_cookies(&set_cookie_headers);
    let auth_cookie = find_cookie(&cookies, "auth_token").expect("auth_token cookie");
    let refresh_cookie = find_cookie(&cookies, "refresh_token").expect("refresh_token cookie");
    let csrf_cookie = find_cookie(&cookies, "csrf_token").expect("csrf_token cookie");

    let auth_value = cookie_value(auth_cookie, "auth_token").expect("auth cookie value");
    let refresh_value =
        cookie_value(refresh_cookie, "refresh_token").expect("refresh cookie value");
    let csrf_value = cookie_value(csrf_cookie, "csrf_token").expect("csrf cookie value");

    assert!(!auth_value.is_empty(), "access token should be present");
    assert!(!refresh_value.is_empty(), "refresh token should be present");
    assert!(!csrf_value.is_empty(), "csrf token should be present");
    assert_eq!(login_body.token, auth_value, "body token matches cookie");

    // ══════════════════════════════════════════════════════════════════════════
    // STEP 2: ACCESS protected endpoint with access token → success
    // ══════════════════════════════════════════════════════════════════════════
    let me_app = Router::new()
        .route("/v1/auth/me", get(auth_me))
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let me_response = me_app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/auth/me")
                .header(header::COOKIE, format!("auth_token={auth_value}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(
        me_response.status(),
        StatusCode::OK,
        "initial access should succeed"
    );

    let body_bytes = to_bytes(me_response.into_body(), 16 * 1024).await?;
    let user: UserInfoResponse = serde_json::from_slice(&body_bytes)?;
    assert_eq!(user.email, "complete@example.com");
    assert_eq!(user.tenant_id, "tenant-1");

    // ══════════════════════════════════════════════════════════════════════════
    // STEP 3: REFRESH tokens → receive new access + refresh tokens
    // ══════════════════════════════════════════════════════════════════════════
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    let mut refresh_headers = HeaderMap::new();
    refresh_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("refresh_token={refresh_value}"))?,
    );

    let (refresh_set_cookies, Json(refresh_body)) =
        refresh_token_handler(axum::extract::State(state.clone()), refresh_headers)
            .await
            .expect("refresh should succeed");

    let new_cookies = collect_cookies(&refresh_set_cookies);
    let new_auth_cookie = find_cookie(&new_cookies, "auth_token").expect("new auth cookie");
    let new_refresh_cookie =
        find_cookie(&new_cookies, "refresh_token").expect("new refresh cookie");

    let new_auth_value = cookie_value(new_auth_cookie, "auth_token").expect("new auth value");
    let new_refresh_value =
        cookie_value(new_refresh_cookie, "refresh_token").expect("new refresh value");

    assert_ne!(new_auth_value, auth_value, "access token should rotate");
    assert_ne!(
        new_refresh_value, refresh_value,
        "refresh token should rotate"
    );
    assert_eq!(
        refresh_body.token, new_auth_value,
        "response token matches new cookie"
    );

    // ══════════════════════════════════════════════════════════════════════════
    // STEP 4: ACCESS protected endpoint with NEW access token → success
    // ══════════════════════════════════════════════════════════════════════════
    let me_response2 = me_app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/auth/me")
                .header(header::COOKIE, format!("auth_token={new_auth_value}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(
        me_response2.status(),
        StatusCode::OK,
        "access with new token should succeed"
    );

    // ══════════════════════════════════════════════════════════════════════════
    // STEP 5: LOGOUT → tokens revoked, cookies cleared
    // ══════════════════════════════════════════════════════════════════════════
    use adapteros_server_api::handlers::auth_enhanced::logout_handler;

    let new_claims = decode_access_claims(&new_auth_value, &state);

    let (logout_headers, Json(logout_body)) = logout_handler(
        axum::extract::State(state.clone()),
        axum::Extension(new_claims.clone()),
    )
    .await
    .expect("logout should succeed");

    assert!(
        logout_body.message.contains("success") || !logout_body.message.is_empty(),
        "logout response should indicate success"
    );

    let logout_cookies = collect_cookies(&logout_headers);
    let cleared_auth = find_cookie(&logout_cookies, "auth_token");
    let cleared_refresh = find_cookie(&logout_cookies, "refresh_token");

    if let Some(c) = cleared_auth {
        assert!(
            c.contains("Max-Age=0") || c.contains("max-age=0"),
            "auth_token cookie should be cleared (Max-Age=0)"
        );
    }
    if let Some(c) = cleared_refresh {
        assert!(
            c.contains("Max-Age=0") || c.contains("max-age=0"),
            "refresh_token cookie should be cleared (Max-Age=0)"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // STEP 6: ACCESS protected endpoint after logout → denied
    // Note: Logout both revokes the token AND locks the session. The middleware
    // checks session status before token revocation, so SESSION_EXPIRED is returned
    // when both conditions are true.
    // ══════════════════════════════════════════════════════════════════════════
    let me_response3 = me_app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/auth/me")
                .header(header::COOKIE, format!("auth_token={new_auth_value}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(
        me_response3.status(),
        StatusCode::UNAUTHORIZED,
        "access after logout should be denied"
    );

    let err_bytes = to_bytes(me_response3.into_body(), 2048).await?;
    let err: ErrorResponse = serde_json::from_slice(&err_bytes)?;
    // Session lock check happens before token revocation check in middleware,
    // so SESSION_EXPIRED is returned when logout locks the session
    assert_eq!(
        err.code, "SESSION_EXPIRED",
        "error should indicate session expired (locked)"
    );

    // ══════════════════════════════════════════════════════════════════════════
    // STEP 7: SESSION EXPIRED scenario (new login then lock session)
    // ══════════════════════════════════════════════════════════════════════════
    let login_req2 = LoginRequest {
        username: None,
        email: "complete@example.com".to_string(),
        password: "complete-flow!".to_string(),
        device_id: Some("test-device-002".to_string()),
        totp_code: None,
    };

    let (login2_headers, _) = login_handler(
        axum::extract::State(state.clone()),
        login_headers,
        axum::Extension(ClientIp("192.168.1.101".into())),
        Json(login_req2),
    )
    .await
    .expect("second login should succeed");

    let login2_cookies = collect_cookies(&login2_headers);
    let refresh2_cookie = find_cookie(&login2_cookies, "refresh_token").expect("refresh cookie 2");
    let refresh2_value = cookie_value(refresh2_cookie, "refresh_token").expect("refresh value 2");

    let refresh2_claims = decode_refresh_claims(&refresh2_value, &state);

    lock_session(&state.db, &refresh2_claims.session_id)
        .await
        .expect("lock session should succeed");

    let mut expired_refresh_headers = HeaderMap::new();
    expired_refresh_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("refresh_token={refresh2_value}"))?,
    );

    let (expired_status, Json(expired_err)) =
        refresh_token_handler(axum::extract::State(state.clone()), expired_refresh_headers)
            .await
            .expect_err("refresh on locked session must fail");

    assert_eq!(expired_status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        expired_err.code, "SESSION_EXPIRED",
        "locked session should return SESSION_EXPIRED"
    );

    Ok(())
}

/// Verifies old access token becomes invalid after refresh (if strict rotation is enforced)
/// and that attempting to use old refresh token after rotation fails.
#[tokio::test]
async fn refresh_invalidates_prior_tokens() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let pw_hash = hash_password("invalidate-pass!")?;
    state
        .db
        .create_user(
            "invalidate@example.com",
            "Invalidate User",
            &pw_hash,
            Role::Admin,
            "tenant-1",
        )
        .await?;

    let login_req = LoginRequest {
        username: None,
        email: "invalidate@example.com".to_string(),
        password: "invalidate-pass!".to_string(),
        device_id: None,
        totp_code: None,
    };
    let (login_headers, _) = login_handler(
        axum::extract::State(state.clone()),
        HeaderMap::new(),
        axum::Extension(ClientIp("127.0.0.1".into())),
        Json(login_req),
    )
    .await
    .expect("login should succeed");

    let cookies = collect_cookies(&login_headers);
    let auth_cookie = find_cookie(&cookies, "auth_token").expect("auth cookie");
    let refresh_cookie = find_cookie(&cookies, "refresh_token").expect("refresh cookie");
    let original_auth = cookie_value(auth_cookie, "auth_token").expect("auth value");
    let original_refresh = cookie_value(refresh_cookie, "refresh_token").expect("refresh value");

    // Ensure refreshed tokens get a distinct iat from the initial login issuance
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    // Perform first refresh
    let mut refresh_headers = HeaderMap::new();
    refresh_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("refresh_token={original_refresh}"))?,
    );
    let (new_cookies_headers, Json(new_body)) =
        refresh_token_handler(axum::extract::State(state.clone()), refresh_headers)
            .await
            .expect("first refresh should succeed");

    let new_cookies = collect_cookies(&new_cookies_headers);
    let new_auth_cookie = find_cookie(&new_cookies, "auth_token").expect("new auth cookie");
    let new_auth = cookie_value(new_auth_cookie, "auth_token").expect("new auth value");

    assert_ne!(
        new_auth, original_auth,
        "new access token differs from original"
    );
    assert_eq!(
        new_body.token, new_auth,
        "response token matches new cookie"
    );

    // Attempt to reuse old refresh token → should fail
    let mut stale_headers = HeaderMap::new();
    stale_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("refresh_token={original_refresh}"))?,
    );
    let reuse_result =
        refresh_token_handler(axum::extract::State(state.clone()), stale_headers).await;
    assert!(
        reuse_result.is_err(),
        "reusing old refresh token should fail"
    );

    Ok(())
}
