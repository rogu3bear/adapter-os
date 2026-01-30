//! Middleware modules for AdapterOS API
//!
//! Provides cross-cutting concerns:
//! - Authentication and authorization
//! - API versioning and deprecation
//! - Request ID tracking
//! - Compression
//! - Caching (ETags, conditional requests)

use crate::auth::{
    validate_access_token_ed25519, validate_token, AuthMode, Claims, Principal, PrincipalType,
};
use crate::ip_extraction::{extract_client_ip, ClientIp};
use crate::security::is_token_revoked;
use crate::security::{
    get_session_by_id, get_tenant_token_baseline, update_session_activity,
    validate_tenant_isolation,
};
use crate::session_tokens::{
    decode_session_token_lock, strip_session_token_prefix, SessionTokenContext,
};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_core::identity::IdentityEnvelope;
use adapteros_core::tenant::TenantId;
use adapteros_db::auth_sessions_kv::AuthSessionKvRepository;
use adapteros_db::users::Role;
use axum::{
    extract::State,
    http::{header, HeaderMap, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use blake3::Hasher;
use chrono::{Duration, Utc};
use serde_json;
use std::str::FromStr;
use uuid::Uuid;

/// Raw ApiKey token attached to the request for downstream use (e.g., worker UDS)
#[derive(Clone)]
pub struct ApiKeyToken(pub String);

/// Extract auth_token from Cookie header
fn extract_token_from_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(token) = cookie.strip_prefix("auth_token=") {
                    return Some(token.to_string());
                }
            }
            None
        })
}

fn extract_cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            let prefix = format!("{name}=");
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(token) = cookie.strip_prefix(&prefix) {
                    return Some(token.to_string());
                }
            }
            None
        })
}

fn error_response(
    status: StatusCode,
    code: &'static str,
    message: impl Into<String>,
) -> (StatusCode, Json<ErrorResponse>) {
    (
        status,
        Json(ErrorResponse::new(message.into()).with_code(code)),
    )
}

fn unauthenticated(msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    error_response(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg)
}

fn session_expired(msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(
            ErrorResponse::new("session expired")
                .with_code("SESSION_EXPIRED")
                .with_string_details(msg.into()),
        ),
    )
}

fn token_revoked(msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(
            ErrorResponse::new("token revoked")
                .with_code("TOKEN_REVOKED")
                .with_string_details(msg.into()),
        ),
    )
}

fn forbidden(msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    error_response(StatusCode::FORBIDDEN, "FORBIDDEN", msg)
}

fn tenant_isolation_error(msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    error_response(StatusCode::FORBIDDEN, "TENANT_ISOLATION_ERROR", msg)
}

fn csrf_error(msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::FORBIDDEN,
        Json(
            ErrorResponse::new("csrf validation failed")
                .with_code("CSRF_ERROR")
                .with_string_details(msg.into()),
        ),
    )
}

fn token_missing() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(
            ErrorResponse::new("Auth token is missing from the request").with_code("TOKEN_MISSING"),
        ),
    )
}

fn token_expired(msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(
            ErrorResponse::new("Auth token is expired")
                .with_code("TOKEN_EXPIRED")
                .with_string_details(msg.into()),
        ),
    )
}

fn token_signature_invalid(msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(
            ErrorResponse::new("Auth token signature is invalid")
                .with_code("TOKEN_SIGNATURE_INVALID")
                .with_string_details(msg.into()),
        ),
    )
}

fn tenant_header_missing() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::BAD_REQUEST,
        Json(
            ErrorResponse::new("Tenant selection header is absent when required")
                .with_code("TENANT_HEADER_MISSING"),
        ),
    )
}

fn build_principal_from_claims(
    claims: &mut Claims,
    principal_type: PrincipalType,
    auth_mode: AuthMode,
) -> Principal {
    claims.auth_mode = auth_mode.clone();
    claims.principal_type = Some(principal_type.clone());
    Principal::from_claims(claims, principal_type, auth_mode)
}

pub mod audit;
pub mod caching;
pub mod canonicalization;
pub mod chain_builder;
pub mod compression;
pub mod context;
pub mod error_code_enforcement;
pub mod observability;
pub mod policy_enforcement;
pub mod request_id;
pub mod seed_isolation;
pub mod trace_context;
pub mod versioning;

pub use caching::{caching_middleware, CacheControl};
pub use canonicalization::{
    canonicalization_middleware, selective_canonicalization_middleware, CanonicalRequest,
    SkipCanonicalization,
};
pub use chain_builder::{
    api_key_chain, internal_chain, optional_auth_chain, protected_chain, MiddlewareChainConfig,
    ProtectedMiddlewareChain,
};
pub use compression::compression_middleware;
pub use error_code_enforcement::ErrorCodeEnforcementLayer;
pub use observability::observability_middleware;
pub use policy_enforcement::policy_enforcement_middleware;
pub use request_id::request_id_middleware;
pub use trace_context::{trace_context_middleware, TraceContextExtension};
pub use versioning::{versioning_middleware, ApiVersion, DeprecationInfo};

/// Result of authentication resolution.
///
/// This enum represents the three possible outcomes of attempting to authenticate
/// a request. The dev bypass path is only available in debug builds.
#[derive(Debug, Clone)]
pub enum AuthResolution {
    /// Successfully authenticated via JWT/session/API key
    Authenticated(Claims, Principal, AuthMode),
    /// Dev bypass enabled (debug builds only) - synthetic admin claims
    #[cfg(debug_assertions)]
    DevBypassed(Claims, Principal),
    /// No valid authentication present
    Unauthenticated,
}

impl AuthResolution {
    /// Returns true if the resolution represents an authenticated state
    /// (either real auth or dev bypass).
    #[allow(dead_code)]
    pub fn is_authenticated(&self) -> bool {
        match self {
            AuthResolution::Authenticated(_, _, _) => true,
            #[cfg(debug_assertions)]
            AuthResolution::DevBypassed(_, _) => true,
            AuthResolution::Unauthenticated => false,
        }
    }

    /// Inject the auth state into request extensions.
    ///
    /// This applies the claims, principal, auth mode, and identity envelope
    /// to the request for downstream handlers.
    pub fn inject_into_request(self, req: &mut Request<axum::body::Body>) {
        match self {
            AuthResolution::Authenticated(claims, principal, auth_mode) => {
                let tenant_id = claims.tenant_id.clone();
                req.extensions_mut().insert(auth_mode);
                req.extensions_mut().insert(principal);
                req.extensions_mut().insert(claims);
                req.extensions_mut().insert(IdentityEnvelope::new(
                    tenant_id,
                    "api".to_string(),
                    "middleware".to_string(),
                    IdentityEnvelope::default_revision(),
                ));
            }
            #[cfg(debug_assertions)]
            AuthResolution::DevBypassed(claims, principal) => {
                let tenant_id = claims.tenant_id.clone();
                req.extensions_mut().insert(AuthMode::DevBypass);
                req.extensions_mut().insert(principal);
                req.extensions_mut().insert(claims);
                req.extensions_mut().insert(IdentityEnvelope::new(
                    tenant_id,
                    "api".to_string(),
                    "middleware".to_string(),
                    IdentityEnvelope::default_revision(),
                ));
            }
            AuthResolution::Unauthenticated => {
                req.extensions_mut().insert(AuthMode::Unauthenticated);
            }
        }
    }
}

/// SECURITY: Dev no-auth bypass check is centralized here.
///
/// This is the SINGLE gateway for dev bypass - all auth middleware must use
/// `resolve_dev_bypass()` which calls this internally. Release builds always
/// return false, and debug builds only return true if AOS_DEV_NO_AUTH is set.
#[cfg(debug_assertions)]
fn dev_no_auth_enabled() -> bool {
    crate::auth::dev_no_auth_enabled()
}

#[cfg(debug_assertions)]
fn dev_no_auth_claims() -> Claims {
    let now = Utc::now();
    Claims {
        sub: "dev-no-auth".to_string(),
        email: "dev-no-auth@adapteros.local".to_string(),
        role: "admin".to_string(),
        roles: vec!["admin".to_string()],
        // Default tenant for dev no-auth (matches test harness seed data).
        tenant_id: "default".to_string(),
        // Dev mode: wildcard grants access to ALL tenants
        // SECURITY: This only works in debug builds via dev_no_auth_enabled() check
        // The "*" wildcard is recognized by check_tenant_access_core() in security/mod.rs
        admin_tenants: vec!["*".to_string()],
        device_id: None,
        session_id: Some("dev-session".to_string()),
        mfa_level: None,
        rot_id: None,
        exp: (now + Duration::hours(8)).timestamp(),
        iat: now.timestamp(),
        jti: Uuid::new_v4().to_string(),
        nbf: now.timestamp(),
        iss: crate::auth::JWT_ISSUER.to_string(),
        auth_mode: AuthMode::DevBypass,
        principal_type: Some(PrincipalType::DevBypass),
    }
}

#[cfg(debug_assertions)]
fn dev_no_auth_tenant_override(headers: &HeaderMap) -> Option<String> {
    let tenant_id = headers
        .get("X-Tenant-Id")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())?;

    if TenantId::new(tenant_id.to_string()).is_err() {
        tracing::warn!(
            tenant_id = %tenant_id,
            "Ignoring invalid dev no-auth tenant override"
        );
        return None;
    }

    Some(tenant_id.to_string())
}

/// Resolve dev bypass authentication.
///
/// SECURITY: This is the SINGLE entry point for dev bypass injection.
/// The `#[cfg(debug_assertions)]` gate ensures this code is completely
/// compiled out in release builds.
///
/// Returns `Some(AuthResolution::DevBypassed(...))` if bypass is enabled,
/// `None` otherwise.
#[cfg(debug_assertions)]
fn resolve_dev_bypass(headers: &HeaderMap) -> Option<AuthResolution> {
    if !dev_no_auth_enabled() {
        return None;
    }

    let mut claims = dev_no_auth_claims();
    if let Some(tenant_id) = dev_no_auth_tenant_override(headers) {
        tracing::debug!(tenant_id = %tenant_id, "Using dev no-auth tenant override");
        claims.tenant_id = tenant_id;
    }

    let principal =
        build_principal_from_claims(&mut claims, PrincipalType::DevBypass, AuthMode::DevBypass);

    Some(AuthResolution::DevBypassed(claims, principal))
}

#[cfg(not(debug_assertions))]
fn resolve_dev_bypass(_headers: &HeaderMap) -> Option<AuthResolution> {
    // Release builds: dev bypass is never available
    None
}

fn extract_tenant_id_from_path(path: &str) -> Option<String> {
    let mut segments = path.split('/').filter(|s| !s.is_empty());
    while let Some(seg) = segments.next() {
        if seg == "tenants" {
            if let Some(tid) = segments.next() {
                return Some(tid.to_string());
            }
        }
    }
    None
}

/// Enforce tenant isolation for any route containing `/tenants/{tenant_id}` in the path.
///
/// SECURITY: This runs after authentication and rejects cross-tenant access unless the caller
/// has explicit admin_tenants access (or dev wildcard in debug builds).
pub async fn tenant_route_guard_middleware(
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    if let Some(path_tenant) = extract_tenant_id_from_path(req.uri().path()) {
        let auth_mode = req
            .extensions()
            .get::<AuthMode>()
            .cloned()
            .unwrap_or(AuthMode::Unauthenticated);
        let principal = req.extensions().get::<Principal>().cloned();
        let claims = req
            .extensions()
            .get::<Claims>()
            .cloned()
            .ok_or_else(|| unauthenticated("missing auth claims for tenant-scoped route"))?;

        let principal_id = principal
            .as_ref()
            .map(|p| p.principal_id.as_str())
            .unwrap_or(&claims.sub);
        let principal_type = principal
            .as_ref()
            .map(|p| format!("{:?}", p.principal_type))
            .unwrap_or_else(|| "unknown".to_string());
        let is_cross_tenant = claims.tenant_id != path_tenant;

        if let Err((_, Json(err_body))) = validate_tenant_isolation(&claims, &path_tenant) {
            tracing::warn!(
                principal_id = %principal_id,
                principal_type = %principal_type,
                auth_mode = ?auth_mode,
                requested_tenant = %path_tenant,
                caller_tenant = %claims.tenant_id,
                code = %err_body.code,
                detail = ?err_body.details,
                "Tenant isolation rejected"
            );
            let detail = err_body
                .details
                .as_ref()
                .and_then(|d| d.as_str())
                .unwrap_or(err_body.message.as_str())
                .to_string();
            return Err(tenant_isolation_error(detail));
        }

        if is_cross_tenant {
            tracing::info!(
                principal_id = %principal_id,
                principal_type = %principal_type,
                auth_mode = ?auth_mode,
                requested_tenant = %path_tenant,
                caller_tenant = %claims.tenant_id,
                "Tenant isolation granted for cross-tenant access"
            );
        }
    }

    Ok(next.run(req).await)
}

/// Require X-Tenant-Id header for routes that need explicit tenant selection.
///
/// This middleware is for routes that operate across tenants or need the caller
/// to explicitly specify which tenant context to use. It rejects requests that
/// don't include the X-Tenant-Id header.
///
/// Use this for admin or system routes that require explicit tenant targeting.
pub async fn require_tenant_header_middleware(
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let tenant_header = req
        .headers()
        .get("X-Tenant-Id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    if tenant_header.is_none() {
        tracing::warn!(
            path = %req.uri().path(),
            "Tenant selection header is absent when required"
        );
        return Err(tenant_header_missing());
    }

    Ok(next.run(req).await)
}

/// Reject new work when the control plane is in maintenance or draining state.
pub async fn lifecycle_gate(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let path = req.uri().path();
    // Allow health and admin lifecycle endpoints to bypass the gate
    let bypass = path.starts_with("/healthz")
        || path.starts_with("/readyz")
        || path.starts_with("/system/ready")
        || path.starts_with("/v1/status")
        || path.starts_with("/admin/lifecycle");
    if bypass {
        return Ok(next.run(req).await);
    }

    if let Some(boot_state) = state.boot_state.as_ref() {
        let current = boot_state.current_state();
        if current.is_maintenance() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    ErrorResponse::new("maintenance")
                        .with_code("SERVICE_UNAVAILABLE")
                        .with_string_details("control plane in maintenance"),
                ),
            ));
        }
        if current.is_draining() || current.is_shutting_down() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    ErrorResponse::new("draining")
                        .with_code("SERVICE_UNAVAILABLE")
                        .with_string_details("control plane draining"),
                ),
            ));
        }
    }

    Ok(next.run(req).await)
}

fn unauthorized_api_key() -> (StatusCode, Json<ErrorResponse>) {
    unauthenticated("invalid api key")
}

async fn validate_api_key(
    state: &AppState,
    token: &str,
) -> Result<(Claims, ApiKeyToken), (StatusCode, Json<ErrorResponse>)> {
    let mut hasher = Hasher::new();
    hasher.update(token.as_bytes());
    let hash = hasher.finalize().to_hex().to_string();

    let record = state
        .db
        .get_api_key_by_hash(&hash, false)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "API key lookup failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?;

    let record = match record {
        Some(r) => r,
        None => {
            tracing::warn!(
                target: "security.api_key",
                hash_prefix = %&hash[..8],
                "API key not found"
            );
            return Err(unauthorized_api_key());
        }
    };

    let user = state.db.get_user(&record.user_id).await.map_err(|e| {
        tracing::warn!(error = %e, "Failed to load API key user");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    let user = match user {
        Some(u) => u,
        None => {
            tracing::warn!(
                target: "security.api_key",
                user_id = %record.user_id,
                key_id = %record.id,
                "API key user not found"
            );
            return Err(unauthorized_api_key());
        }
    };

    if user.tenant_id != record.tenant_id {
        tracing::warn!(
            user_tenant = %user.tenant_id,
            key_tenant = %record.tenant_id,
            "API key tenant mismatch"
        );
        return Err(unauthorized_api_key());
    }

    let scopes: Vec<String> = match serde_json::from_str(&record.scopes) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                target: "security.api_key",
                key_id = %record.id,
                error = %e,
                "API key has malformed scopes"
            );
            return Err(unauthorized_api_key());
        }
    };

    if scopes.is_empty() {
        tracing::warn!(
            target: "security.api_key",
            key_id = %record.id,
            "API key has empty scopes"
        );
        return Err(unauthorized_api_key());
    }

    let now = Utc::now();
    let primary_role = scopes
        .first()
        .cloned()
        .unwrap_or_else(|| "viewer".to_string());

    let claims = Claims {
        sub: record.user_id.clone(),
        email: user.email.clone(),
        role: primary_role,
        roles: scopes,
        tenant_id: record.tenant_id.clone(),
        admin_tenants: vec![],
        device_id: None,
        session_id: Some(format!("api-key:{}", record.id)),
        mfa_level: None,
        rot_id: None,
        exp: (now + Duration::days(365)).timestamp(),
        iat: now.timestamp(),
        jti: format!("api-key:{}", record.id),
        nbf: now.timestamp(),
        iss: crate::auth::JWT_ISSUER.to_string(),
        auth_mode: AuthMode::ApiKey,
        principal_type: Some(PrincipalType::ApiKey),
    };

    Ok((claims, ApiKeyToken(token.to_string())))
}

fn kv_repo(state: &AppState) -> Option<AuthSessionKvRepository> {
    state.db.kv_backend().map(|kv| {
        let backend: std::sync::Arc<dyn adapteros_db::KvBackend> = kv.clone();
        AuthSessionKvRepository::new(backend)
    })
}

async fn validate_access_token_with_session(
    state: &AppState,
    token: &str,
) -> Result<Claims, (StatusCode, Json<ErrorResponse>)> {
    let mut claims = if state.use_ed25519 {
        validate_access_token_ed25519(token, &state.ed25519_public_keys, &state.ed25519_public_key)
    } else {
        // Legacy HMAC path retains session validation below
        validate_token(token, &state.hmac_keys, state.jwt_secret.as_slice())
    }
    .map_err(|e| {
        tracing::warn!(error = %e, "Auth token signature is invalid");
        token_signature_invalid(e.to_string())
    })?;

    if claims.iss != crate::auth::JWT_ISSUER {
        tracing::warn!(iss = %claims.iss, "Invalid token issuer");
        return Err(unauthenticated("invalid issuer"));
    }

    if claims.tenant_id.is_empty() {
        tracing::warn!("Token missing tenant_id");
        return Err(unauthenticated("missing tenant"));
    }

    // Enforce tenant baseline (token issued_at must be >= tenant baseline)
    if let Some(baseline) = get_tenant_token_baseline(&state.db, &claims.tenant_id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "Failed to load tenant token baseline");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?
    {
        if let Ok(baseline_dt) = chrono::DateTime::parse_from_rfc3339(&baseline) {
            if claims.iat < baseline_dt.timestamp() {
                tracing::warn!(
                    tenant_id = %claims.tenant_id,
                    token_iat = claims.iat,
                    baseline = %baseline,
                    "Token issued before tenant baseline"
                );
                return Err(token_revoked("token issued before tenant baseline"));
            }
        } else {
            tracing::warn!(
                tenant_id = %claims.tenant_id,
                baseline = %baseline,
                "Unable to parse tenant token baseline"
            );
        }
    }

    let session_id = claims
        .session_id
        .clone()
        .ok_or_else(|| unauthenticated("missing session"))?;

    // Validate KV session linkage if backend is present
    let now_ts = Utc::now().timestamp();

    if let Some(repo) = kv_repo(state) {
        match repo.get_session(&session_id).await {
            Ok(Some(session)) => {
                let now = Utc::now().timestamp();
                // Use the longer session expiry (expires_at) for session validation
                let expiry = session.expires_at;
                if now >= expiry {
                    tracing::warn!(session_id = %session_id, "Session expired");
                    return Err(session_expired("session no longer valid"));
                }

                if session.locked {
                    tracing::warn!(session_id = %session_id, "Session locked");
                    return Err(session_expired("session locked"));
                }

                if let (Some(token_device), Some(session_device)) =
                    (claims.device_id.as_ref(), session.device_id.as_ref())
                {
                    if token_device != session_device {
                        tracing::warn!(
                            session_id = %session_id,
                            token_device = %token_device,
                            session_device = %session_device,
                            "Device mismatch for session"
                        );
                        return Err(unauthenticated("device mismatch"));
                    }
                }

                if let Err(e) = repo.update_activity(&session_id).await {
                    tracing::debug!(error = %e, "Failed to update session activity (KV)");
                }
            }
            Ok(None) => {
                tracing::warn!(session_id = %session_id, "Session not found in KV");
                return Err(session_expired("session not found"));
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load session from KV");
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
                ));
            }
        }
    } else {
        // SQL fallback
        match get_session_by_id(&state.db, &session_id).await {
            Ok(Some(session)) => {
                // expires_at is already a Unix timestamp (i64)
                let session_exp_ts = session.expires_at;

                if session.locked != 0 || session_exp_ts <= now_ts {
                    tracing::warn!(session_id = %session_id, "Session expired or locked (SQL)");
                    return Err(session_expired("session no longer valid"));
                }

                if let (Some(token_device), Some(session_device)) =
                    (claims.device_id.as_ref(), session.device_id.as_ref())
                {
                    if token_device != session_device {
                        tracing::warn!(
                            session_id = %session_id,
                            token_device = %token_device,
                            session_device = %session_device,
                            "Device mismatch for session (SQL)"
                        );
                        return Err(unauthenticated("device mismatch"));
                    }
                }

                if let Err(e) = update_session_activity(&state.db, &session_id).await {
                    tracing::debug!(error = %e, "Failed to update session activity (SQL)");
                }
            }
            Ok(None) => {
                tracing::warn!(session_id = %session_id, "Session not found in SQL");
                return Err(session_expired("session not found"));
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load session from SQL");
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
                ));
            }
        }
    }

    claims.auth_mode = AuthMode::BearerToken;
    claims.principal_type.get_or_insert(PrincipalType::User);

    Ok(claims)
}

/// Extract client IP address from request headers (applies to all routes)
pub async fn client_ip_middleware(mut req: Request<axum::body::Body>, next: Next) -> Response {
    // Extract and inject client IP into request extensions
    // Always insert a ClientIp - use extracted IP or fallback to "unknown"
    let ip = extract_client_ip(req.headers()).unwrap_or_else(|| "127.0.0.1".to_string());
    req.extensions_mut().insert(ClientIp(ip));
    next.run(req).await
}

/// CSRF double-submit check for cookie-authenticated mutations.
pub async fn csrf_middleware(
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let method = req.method().clone();
    let is_unsafe = matches!(
        method,
        axum::http::Method::POST
            | axum::http::Method::PUT
            | axum::http::Method::PATCH
            | axum::http::Method::DELETE
    );

    if is_unsafe {
        let has_auth_cookie = extract_cookie_value(req.headers(), "auth_token").is_some();
        if has_auth_cookie {
            let cookie_token = extract_cookie_value(req.headers(), "csrf_token");
            let header_token = req
                .headers()
                .get("X-CSRF-Token")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            if cookie_token.is_none() || header_token != cookie_token {
                tracing::warn!(
                    target: "security.csrf",
                    method = %method,
                    path = %req.uri().path(),
                    cookie_present = cookie_token.is_some(),
                    header_present = header_token.is_some(),
                    "CSRF validation failed"
                );
                return Err(csrf_error("Missing or invalid CSRF token"));
            }
        }
    }

    Ok(next.run(req).await)
}

/// Extract and validate JWT from Authorization header
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    // Check dev bypass FIRST (only in debug builds)
    if let Some(resolution) = resolve_dev_bypass(req.headers()) {
        tracing::info!("Dev no-auth bypass enabled; skipping authentication");
        resolution.inject_into_request(&mut req);
        return Ok(next.run(req).await);
    }

    let is_prod = state
        .runtime_mode
        .as_ref()
        .map(|m| m.is_prod())
        .unwrap_or(false);
    if is_prod && !state.use_ed25519 {
        tracing::error!("HMAC/HS256 auth disabled in production; set security.jwt_mode=eddsa");
        return Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "EdDSA is required in production",
        ));
    }

    // Extract client IP address from headers for audit logging
    if let Some(ip) = extract_client_ip(req.headers()) {
        req.extensions_mut().insert(ClientIp(ip));
    }
    req.extensions_mut().insert(AuthMode::Unauthenticated);

    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    let query_token = req.uri().query().and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.into_owned())
    });

    // Also try to extract token from cookies for browser-based authentication
    let cookie_token = extract_token_from_cookie(req.headers());

    enum AuthToken<'a> {
        ApiKey(&'a str),
        Jwt(&'a str, JwtSource),
        SessionJwt(&'a str, JwtSource),
    }

    #[derive(Clone, Copy)]
    enum JwtSource {
        Bearer,
        Query,
        Cookie,
    }

    let token = if let Some(header) = auth_header {
        if let Some(api_key) = header.strip_prefix("ApiKey ") {
            Some(AuthToken::ApiKey(api_key))
        } else if let Some(bearer) = header.strip_prefix("Bearer ") {
            // OpenAI-compatible clients use `Authorization: Bearer <api_key>`.
            // Heuristic: JWTs always contain '.' separators; API keys (base64url) do not.
            if let Some(session_token) = strip_session_token_prefix(bearer) {
                Some(AuthToken::SessionJwt(session_token, JwtSource::Bearer))
            } else if bearer.contains('.') {
                Some(AuthToken::Jwt(bearer, JwtSource::Bearer))
            } else {
                Some(AuthToken::ApiKey(bearer))
            }
        } else {
            None
        }
    } else if let Some(q) = query_token.as_deref() {
        Some(AuthToken::Jwt(q, JwtSource::Query))
    } else {
        cookie_token
            .as_deref()
            .map(|c| AuthToken::Jwt(c, JwtSource::Cookie))
    };

    if let Some(token) = token {
        match token {
            AuthToken::ApiKey(api_token) => {
                let (mut claims, api_key_token) = validate_api_key(&state, api_token).await?;
                let auth_mode = AuthMode::ApiKey;
                let principal = build_principal_from_claims(
                    &mut claims,
                    PrincipalType::ApiKey,
                    auth_mode.clone(),
                );
                let tenant_id = claims.tenant_id.clone();
                let token_exp = claims.exp;
                req.extensions_mut().insert(auth_mode.clone());
                req.extensions_mut().insert(principal);
                req.extensions_mut().insert(claims);
                req.extensions_mut().insert(api_key_token);
                let identity = IdentityEnvelope::new(
                    tenant_id,
                    "api".to_string(),
                    "middleware".to_string(), // or specific
                    IdentityEnvelope::default_revision(),
                );
                req.extensions_mut().insert(identity);

                let response = next.run(req).await;
                let now = Utc::now().timestamp();
                if now >= token_exp {
                    tracing::warn!(exp = token_exp, now = now, "Auth token is expired");
                    return Err(token_expired("token expired during request processing"));
                }

                return Ok(response);
            }
            AuthToken::SessionJwt(token, source) => {
                let auth_mode = match source {
                    JwtSource::Cookie => AuthMode::Cookie,
                    _ => AuthMode::BearerToken,
                };
                match validate_access_token_with_session(&state, token).await {
                    Ok(mut claims) => {
                        #[cfg(debug_assertions)]
                        tracing::debug!(
                            user_id = %claims.sub,
                            tenant_id = %claims.tenant_id,
                            admin_tenants = ?claims.admin_tenants,
                            jwt_algorithm = if state.use_ed25519 { "Ed25519" } else { "HMAC" },
                            "Session JWT validated successfully"
                        );

                        match is_token_revoked(&state.db, &claims.jti).await {
                            Ok(true) => {
                                tracing::warn!(jti = %claims.jti, user_id = %claims.sub, "Revoked session token used");
                                return Err(token_revoked("this token has been revoked"));
                            }
                            Ok(false) => { /* Token not revoked, continue */ }
                            Err(e) => {
                                tracing::warn!(error = %e, "Failed to check token revocation - denying access");
                                return Err((
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(
                                        ErrorResponse::new("internal error")
                                            .with_code("INTERNAL_ERROR"),
                                    ),
                                ));
                            }
                        }

                        let lock = decode_session_token_lock(token)
                            .map_err(|e| unauthenticated(format!("invalid session token: {e}")))?;
                        let principal = build_principal_from_claims(
                            &mut claims,
                            PrincipalType::User,
                            auth_mode.clone(),
                        );
                        let tenant_id = claims.tenant_id.clone();
                        let token_exp = claims.exp;
                        req.extensions_mut().insert(auth_mode.clone());
                        req.extensions_mut().insert(principal);
                        req.extensions_mut().insert(claims);
                        req.extensions_mut().insert(SessionTokenContext { lock });
                        let identity = IdentityEnvelope::new(
                            tenant_id,
                            "api".to_string(),
                            "middleware".to_string(), // or specific
                            IdentityEnvelope::default_revision(),
                        );
                        req.extensions_mut().insert(identity);

                        let response = next.run(req).await;
                        let now = Utc::now().timestamp();
                        if now >= token_exp {
                            tracing::warn!(exp = token_exp, now = now, "Auth token is expired");
                            return Err(token_expired("token expired during request processing"));
                        }

                        return Ok(response);
                    }
                    Err(err) => return Err(err),
                }
            }
            AuthToken::Jwt(token, source) => {
                let auth_mode = match source {
                    JwtSource::Cookie => AuthMode::Cookie,
                    _ => AuthMode::BearerToken,
                };
                match validate_access_token_with_session(&state, token).await {
                    Ok(mut claims) => {
                        #[cfg(debug_assertions)]
                        tracing::debug!(
                            user_id = %claims.sub,
                            tenant_id = %claims.tenant_id,
                            admin_tenants = ?claims.admin_tenants,
                            jwt_algorithm = if state.use_ed25519 { "Ed25519" } else { "HMAC" },
                            "JWT validated successfully"
                        );

                        match is_token_revoked(&state.db, &claims.jti).await {
                            Ok(true) => {
                                tracing::warn!(jti = %claims.jti, user_id = %claims.sub, "Revoked token used");
                                return Err(token_revoked("this token has been revoked"));
                            }
                            Ok(false) => { /* Token not revoked, continue */ }
                            Err(e) => {
                                tracing::warn!(error = %e, "Failed to check token revocation - denying access");
                                return Err((
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(
                                        ErrorResponse::new("internal error")
                                            .with_code("INTERNAL_ERROR"),
                                    ),
                                ));
                            }
                        }

                        let principal = build_principal_from_claims(
                            &mut claims,
                            PrincipalType::User,
                            auth_mode.clone(),
                        );
                        let tenant_id = claims.tenant_id.clone();
                        let token_exp = claims.exp;
                        req.extensions_mut().insert(auth_mode.clone());
                        req.extensions_mut().insert(principal);
                        req.extensions_mut().insert(claims);
                        let identity = IdentityEnvelope::new(
                            tenant_id,
                            "api".to_string(),
                            "middleware".to_string(), // or specific
                            IdentityEnvelope::default_revision(),
                        );
                        req.extensions_mut().insert(identity);

                        let response = next.run(req).await;
                        let now = Utc::now().timestamp();
                        if now >= token_exp {
                            tracing::warn!(exp = token_exp, now = now, "Auth token is expired");
                            return Err(token_expired("token expired during request processing"));
                        }

                        return Ok(response);
                    }
                    Err(err) => return Err(err),
                }
            }
        }
    }

    tracing::warn!("Auth token is missing from the request");
    Err(token_missing())
}

/// Extract and validate API key OR JWT from Authorization header
pub async fn dual_auth_middleware(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    // Check dev bypass FIRST (only in debug builds)
    if let Some(resolution) = resolve_dev_bypass(req.headers()) {
        tracing::info!("Dev no-auth bypass enabled; skipping authentication");
        resolution.inject_into_request(&mut req);
        return Ok(next.run(req).await);
    }

    let is_prod = state
        .runtime_mode
        .as_ref()
        .map(|m| m.is_prod())
        .unwrap_or(false);
    if is_prod && !state.use_ed25519 {
        tracing::error!("HMAC/HS256 auth disabled in production; set security.jwt_mode=eddsa");
        return Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "EdDSA is required in production",
        ));
    }

    // Extract client IP address from headers for audit logging
    if let Some(ip) = extract_client_ip(req.headers()) {
        req.extensions_mut().insert(ClientIp(ip));
    }
    req.extensions_mut().insert(AuthMode::Unauthenticated);

    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    let query_token = req.uri().query().and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.into_owned())
    });

    // Also try to extract token from cookies for browser-based authentication
    let cookie_token = extract_token_from_cookie(req.headers());

    enum AuthToken<'a> {
        ApiKey(&'a str),
        Jwt(&'a str, JwtSource),
        SessionJwt(&'a str, JwtSource),
    }

    #[derive(Clone, Copy)]
    enum JwtSource {
        Bearer,
        Query,
        Cookie,
    }

    let token = if let Some(header) = auth_header {
        if let Some(api_key) = header.strip_prefix("ApiKey ") {
            Some(AuthToken::ApiKey(api_key))
        } else if let Some(bearer) = header.strip_prefix("Bearer ") {
            // OpenAI-compatible clients use `Authorization: Bearer <api_key>`.
            // Heuristic: JWTs always contain '.' separators; API keys (base64url) do not.
            if let Some(session_token) = strip_session_token_prefix(bearer) {
                Some(AuthToken::SessionJwt(session_token, JwtSource::Bearer))
            } else if bearer.contains('.') {
                Some(AuthToken::Jwt(bearer, JwtSource::Bearer))
            } else {
                Some(AuthToken::ApiKey(bearer))
            }
        } else {
            None
        }
    } else if let Some(q) = query_token.as_deref() {
        Some(AuthToken::Jwt(q, JwtSource::Query))
    } else {
        cookie_token
            .as_deref()
            .map(|c| AuthToken::Jwt(c, JwtSource::Cookie))
    };

    if let Some(token) = token {
        match token {
            AuthToken::ApiKey(api_token) => {
                let (mut claims, api_key_token) = validate_api_key(&state, api_token).await?;
                let auth_mode = AuthMode::ApiKey;
                let principal = build_principal_from_claims(
                    &mut claims,
                    PrincipalType::ApiKey,
                    auth_mode.clone(),
                );
                let tenant_id = claims.tenant_id.clone();
                let token_exp = claims.exp;
                req.extensions_mut().insert(auth_mode.clone());
                req.extensions_mut().insert(principal);
                req.extensions_mut().insert(claims);
                req.extensions_mut().insert(api_key_token);
                let identity = IdentityEnvelope::new(
                    tenant_id,
                    "api".to_string(),
                    "middleware".to_string(), // or specific
                    IdentityEnvelope::default_revision(),
                );
                req.extensions_mut().insert(identity);

                let response = next.run(req).await;
                let now = Utc::now().timestamp();
                if now >= token_exp {
                    tracing::warn!(exp = token_exp, now = now, "Auth token is expired");
                    return Err(token_expired("token expired during request processing"));
                }

                return Ok(response);
            }
            AuthToken::SessionJwt(token, source) => {
                let auth_mode = match source {
                    JwtSource::Cookie => AuthMode::Cookie,
                    _ => AuthMode::BearerToken,
                };
                match validate_access_token_with_session(&state, token).await {
                    Ok(mut claims) => {
                        #[cfg(debug_assertions)]
                        tracing::debug!(
                            user_id = %claims.sub,
                            tenant_id = %claims.tenant_id,
                            admin_tenants = ?claims.admin_tenants,
                            jwt_algorithm = if state.use_ed25519 { "Ed25519" } else { "HMAC" },
                            "Session JWT validated successfully (dual auth)"
                        );

                        match is_token_revoked(&state.db, &claims.jti).await {
                            Ok(true) => {
                                tracing::warn!(jti = %claims.jti, user_id = %claims.sub, "Revoked session token used (dual auth)");
                                return Err(token_revoked("this token has been revoked"));
                            }
                            Ok(false) => { /* Token not revoked, continue */ }
                            Err(e) => {
                                tracing::warn!(error = %e, "Failed to check token revocation (dual auth) - denying access");
                                return Err((
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(
                                        ErrorResponse::new("internal error")
                                            .with_code("INTERNAL_ERROR"),
                                    ),
                                ));
                            }
                        }

                        let lock = decode_session_token_lock(token)
                            .map_err(|e| unauthenticated(format!("invalid session token: {e}")))?;
                        let principal = build_principal_from_claims(
                            &mut claims,
                            PrincipalType::User,
                            auth_mode.clone(),
                        );
                        let tenant_id = claims.tenant_id.clone();
                        let token_exp = claims.exp;
                        req.extensions_mut().insert(auth_mode.clone());
                        req.extensions_mut().insert(principal);
                        req.extensions_mut().insert(claims);
                        req.extensions_mut().insert(SessionTokenContext { lock });
                        let identity = IdentityEnvelope::new(
                            tenant_id,
                            "api".to_string(),
                            "middleware".to_string(), // or specific
                            IdentityEnvelope::default_revision(),
                        );
                        req.extensions_mut().insert(identity);

                        let response = next.run(req).await;
                        let now = Utc::now().timestamp();
                        if now >= token_exp {
                            tracing::warn!(exp = token_exp, now = now, "Auth token is expired");
                            return Err(token_expired("token expired during request processing"));
                        }

                        return Ok(response);
                    }
                    Err(err) => {
                        return Err(err);
                    }
                }
            }
            AuthToken::Jwt(token, source) => {
                let auth_mode = match source {
                    JwtSource::Cookie => AuthMode::Cookie,
                    _ => AuthMode::BearerToken,
                };
                match validate_access_token_with_session(&state, token).await {
                    Ok(mut claims) => {
                        #[cfg(debug_assertions)]
                        tracing::debug!(
                            user_id = %claims.sub,
                            tenant_id = %claims.tenant_id,
                            admin_tenants = ?claims.admin_tenants,
                            jwt_algorithm = if state.use_ed25519 { "Ed25519" } else { "HMAC" },
                            "JWT validated successfully (dual auth)"
                        );

                        match is_token_revoked(&state.db, &claims.jti).await {
                            Ok(true) => {
                                tracing::warn!(jti = %claims.jti, user_id = %claims.sub, "Revoked token used (dual auth)");
                                return Err(token_revoked("this token has been revoked"));
                            }
                            Ok(false) => { /* Token not revoked, continue */ }
                            Err(e) => {
                                tracing::warn!(error = %e, "Failed to check token revocation (dual auth) - denying access");
                                return Err((
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(
                                        ErrorResponse::new("internal error")
                                            .with_code("INTERNAL_ERROR"),
                                    ),
                                ));
                            }
                        }

                        let principal = build_principal_from_claims(
                            &mut claims,
                            PrincipalType::User,
                            auth_mode.clone(),
                        );
                        let tenant_id = claims.tenant_id.clone();
                        let token_exp = claims.exp;
                        req.extensions_mut().insert(auth_mode.clone());
                        req.extensions_mut().insert(principal);
                        req.extensions_mut().insert(claims);
                        let identity = IdentityEnvelope::new(
                            tenant_id,
                            "api".to_string(),
                            "middleware".to_string(), // or specific
                            IdentityEnvelope::default_revision(),
                        );
                        req.extensions_mut().insert(identity);

                        let response = next.run(req).await;
                        let now = Utc::now().timestamp();
                        if now >= token_exp {
                            tracing::warn!(exp = token_exp, now = now, "Auth token is expired");
                            return Err(token_expired("token expired during request processing"));
                        }

                        return Ok(response);
                    }
                    Err(err) => {
                        return Err(err);
                    }
                }
            }
        }
    }

    tracing::warn!("Auth token is missing from the request");
    Err(token_missing())
}

/// Optional authentication middleware - validates token if present, allows request if not
///
/// Unlike `auth_middleware`, this middleware does not reject unauthenticated requests.
/// It validates and injects Claims if a valid token is provided, but proceeds without
/// Claims if no token is present or token is invalid.
///
/// This is useful for endpoints that provide enhanced functionality when authenticated
/// but still work for anonymous users (e.g., public status endpoints with optional
/// tenant-specific data).
pub async fn optional_auth_middleware(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Check dev bypass FIRST (only in debug builds)
    if let Some(resolution) = resolve_dev_bypass(req.headers()) {
        tracing::debug!("Dev no-auth bypass enabled; injecting dev claims");
        resolution.inject_into_request(&mut req);
        return next.run(req).await;
    }

    let is_prod = state
        .runtime_mode
        .as_ref()
        .map(|m| m.is_prod())
        .unwrap_or(false);
    if is_prod && !state.use_ed25519 {
        tracing::error!("HMAC/HS256 auth disabled in production; set security.jwt_mode=eddsa");
        let (status, body) = error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "EdDSA is required in production",
        );
        return (status, body).into_response();
    }

    // Extract client IP address from headers for audit logging
    if let Some(ip) = extract_client_ip(req.headers()) {
        req.extensions_mut().insert(ClientIp(ip));
    }
    req.extensions_mut().insert(AuthMode::Unauthenticated);

    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    let query_token = req.uri().query().and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.into_owned())
    });

    // Also try to extract token from cookies for browser-based authentication
    let cookie_token = extract_token_from_cookie(req.headers());

    enum AuthToken<'a> {
        ApiKey(&'a str),
        Jwt(&'a str, JwtSource),
        SessionJwt(&'a str, JwtSource),
    }

    #[derive(Clone, Copy)]
    enum JwtSource {
        Bearer,
        Query,
        Cookie,
    }

    let token = if let Some(header) = auth_header {
        if let Some(api_key) = header.strip_prefix("ApiKey ") {
            Some(AuthToken::ApiKey(api_key))
        } else if let Some(bearer) = header.strip_prefix("Bearer ") {
            // OpenAI-compatible clients use `Authorization: Bearer <api_key>`.
            // Heuristic: JWTs always contain '.' separators; API keys (base64url) do not.
            if let Some(session_token) = strip_session_token_prefix(bearer) {
                Some(AuthToken::SessionJwt(session_token, JwtSource::Bearer))
            } else if bearer.contains('.') {
                Some(AuthToken::Jwt(bearer, JwtSource::Bearer))
            } else {
                Some(AuthToken::ApiKey(bearer))
            }
        } else {
            None
        }
    } else if let Some(q) = query_token.as_deref() {
        Some(AuthToken::Jwt(q, JwtSource::Query))
    } else {
        cookie_token
            .as_deref()
            .map(|c| AuthToken::Jwt(c, JwtSource::Cookie))
    };

    if let Some(token) = token {
        match token {
            AuthToken::ApiKey(api_token) => match validate_api_key(&state, api_token).await {
                Ok((mut claims, api_key_token)) => {
                    let auth_mode = AuthMode::ApiKey;
                    let principal = build_principal_from_claims(
                        &mut claims,
                        PrincipalType::ApiKey,
                        auth_mode.clone(),
                    );
                    let tenant_id = claims.tenant_id.clone();
                    req.extensions_mut().insert(auth_mode.clone());
                    req.extensions_mut().insert(principal);
                    req.extensions_mut().insert(claims);
                    req.extensions_mut().insert(api_key_token);
                    let identity = IdentityEnvelope::new(
                        tenant_id,
                        "api".to_string(),
                        "middleware".to_string(),
                        IdentityEnvelope::default_revision(),
                    );
                    req.extensions_mut().insert(identity);
                }
                Err(e) => {
                    tracing::debug!(error = ?e, "API key validation failed, proceeding unauthenticated");
                }
            },
            AuthToken::SessionJwt(token, source) => {
                let auth_mode = match source {
                    JwtSource::Cookie => AuthMode::Cookie,
                    _ => AuthMode::BearerToken,
                };
                match validate_access_token_with_session(&state, token).await {
                    Ok(mut claims) => {
                        let should_skip_auth = match is_token_revoked(&state.db, &claims.jti).await
                        {
                            Ok(true) => {
                                tracing::debug!(jti = %claims.jti, "Session token is revoked, proceeding without authentication");
                                true
                            }
                            Ok(false) => false,
                            Err(e) => {
                                tracing::warn!(error = %e, "Failed to check token revocation (optional auth) - proceeding without auth");
                                true
                            }
                        };

                        if should_skip_auth {
                            // skip
                        } else {
                            match decode_session_token_lock(token) {
                                Ok(lock) => {
                                    let principal = build_principal_from_claims(
                                        &mut claims,
                                        PrincipalType::User,
                                        auth_mode.clone(),
                                    );
                                    let tenant_id = claims.tenant_id.clone();
                                    req.extensions_mut().insert(auth_mode.clone());
                                    req.extensions_mut().insert(principal);
                                    req.extensions_mut().insert(claims);
                                    req.extensions_mut().insert(SessionTokenContext { lock });
                                    let identity = IdentityEnvelope::new(
                                        tenant_id,
                                        "api".to_string(),
                                        "middleware".to_string(),
                                        IdentityEnvelope::default_revision(),
                                    );
                                    req.extensions_mut().insert(identity);
                                }
                                Err(e) => {
                                    tracing::debug!(error = %e, "Session token lock invalid, proceeding without authentication");
                                }
                            }
                        }
                    }
                    Err(_e) => {
                        tracing::debug!(
                            "Token validation failed, proceeding without authentication"
                        );
                    }
                }
            }
            AuthToken::Jwt(token, source) => {
                let auth_mode = match source {
                    JwtSource::Cookie => AuthMode::Cookie,
                    _ => AuthMode::BearerToken,
                };
                match validate_access_token_with_session(&state, token).await {
                    Ok(mut claims) => {
                        let should_skip_auth = match is_token_revoked(&state.db, &claims.jti).await
                        {
                            Ok(true) => {
                                tracing::debug!(jti = %claims.jti, "Token is revoked, proceeding without authentication");
                                true
                            }
                            Ok(false) => false,
                            Err(e) => {
                                tracing::warn!(error = %e, "Failed to check token revocation (optional auth) - proceeding without auth");
                                true
                            }
                        };

                        if should_skip_auth {
                            // skip
                        } else {
                            #[cfg(debug_assertions)]
                            tracing::debug!(
                                user_id = %claims.sub,
                                tenant_id = %claims.tenant_id,
                                admin_tenants = ?claims.admin_tenants,
                                jwt_algorithm = if state.use_ed25519 { "Ed25519" } else { "HMAC" },
                                "JWT validated successfully (optional auth)"
                            );

                            let principal = build_principal_from_claims(
                                &mut claims,
                                PrincipalType::User,
                                auth_mode.clone(),
                            );
                            let tenant_id = claims.tenant_id.clone();
                            req.extensions_mut().insert(auth_mode.clone());
                            req.extensions_mut().insert(principal);
                            req.extensions_mut().insert(claims);
                            let identity = IdentityEnvelope::new(
                                tenant_id,
                                "api".to_string(),
                                "middleware".to_string(),
                                IdentityEnvelope::default_revision(),
                            );
                            req.extensions_mut().insert(identity);
                        }
                    }
                    Err(_e) => {
                        tracing::debug!(
                            "Token validation failed, proceeding without authentication"
                        );
                    }
                }
            }
        }
    }

    // Always proceed with request, regardless of authentication status
    next.run(req).await
}

/// Require specific role for access
pub fn require_role(
    claims: &Claims,
    required: Role,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let user_role = Role::from_str(&claims.role).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("invalid role").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Admin can access everything
    if user_role == Role::Admin {
        return Ok(());
    }

    // Check specific role requirements
    if user_role == required {
        return Ok(());
    }

    tracing::warn!(
        target: "security.authz",
        user_id = %claims.sub,
        user_role = %claims.role,
        required_role = ?required,
        "Authorization denied - insufficient role"
    );
    Err(forbidden(format!("required role: {:?}", required)))
}

/// Check if user has any of the specified roles
pub fn require_any_role(
    claims: &Claims,
    roles: &[Role],
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let user_role = Role::from_str(&claims.role).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("invalid role").with_code("INTERNAL_ERROR")),
        )
    })?;

    if user_role == Role::Admin || roles.contains(&user_role) {
        return Ok(());
    }

    tracing::warn!(
        target: "security.authz",
        user_id = %claims.sub,
        user_role = %claims.role,
        required_roles = ?roles,
        "Authorization denied - insufficient role"
    );
    Err(forbidden("insufficient permissions"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, routing::post, Router};
    use tower::ServiceExt;

    fn csrf_app() -> Router {
        Router::new()
            .route("/", post(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn(csrf_middleware))
    }

    #[tokio::test]
    async fn csrf_missing_header_is_rejected() {
        let req = Request::builder()
            .method(axum::http::Method::POST)
            .uri("/")
            .header(
                header::COOKIE,
                "auth_token=abc123; csrf_token=csrf-cookie-value",
            )
            .body(Body::empty())
            .unwrap();

        let resp = csrf_app().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn csrf_matching_header_is_allowed() {
        let req = Request::builder()
            .method(axum::http::Method::POST)
            .uri("/")
            .header(
                header::COOKIE,
                "auth_token=abc123; csrf_token=csrf-cookie-value",
            )
            .header("X-CSRF-Token", "csrf-cookie-value")
            .body(Body::empty())
            .unwrap();

        let resp = csrf_app().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
