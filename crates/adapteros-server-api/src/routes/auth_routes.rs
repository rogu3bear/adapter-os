//! Authentication and authorization routes.
//!
//! This module contains all routes for:
//! - `/v1/auth/*` - Login, logout, MFA, sessions
//! - `/v1/api-keys/*` - API key management

use crate::handlers;
use crate::handlers::auth;
use crate::state::AppState;
use axum::{
    routing::{delete, get, post},
    Router,
};

/// Build the protected auth routes subrouter.
///
/// These routes require authentication and are merged into the protected routes.
/// Public auth routes (login, bootstrap, config) remain in the main build function.
pub fn protected_auth_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/v1/auth/logout",
            post(handlers::auth_enhanced::logout_handler),
        )
        .route("/v1/auth/me", get(auth::auth_me))
        .route(
            "/v1/auth/mfa/status",
            get(handlers::auth_enhanced::mfa_status_handler),
        )
        .route(
            "/v1/auth/mfa/start",
            post(handlers::auth_enhanced::mfa_start_handler),
        )
        .route(
            "/v1/auth/mfa/verify",
            post(handlers::auth_enhanced::mfa_verify_handler),
        )
        .route(
            "/v1/auth/mfa/disable",
            post(handlers::auth_enhanced::mfa_disable_handler),
        )
        .route(
            "/v1/api-keys",
            get(handlers::api_keys::list_api_keys).post(handlers::api_keys::create_api_key),
        )
        .route(
            "/v1/api-keys/{id}",
            delete(handlers::api_keys::revoke_api_key),
        )
        .route(
            "/v1/auth/sessions",
            get(handlers::auth_enhanced::list_sessions_handler),
        )
        .route(
            "/v1/auth/sessions/{jti}",
            delete(handlers::auth_enhanced::revoke_session_handler),
        )
        .route(
            "/v1/auth/tenants",
            get(handlers::auth_enhanced::list_user_tenants_handler),
        )
        .route(
            "/v1/auth/tenants/switch",
            post(handlers::auth_enhanced::switch_tenant_handler),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protected_auth_routes_builds() {
        // Verify routes compile and build without panic
        let _router: Router<AppState> = protected_auth_routes();
    }
}
