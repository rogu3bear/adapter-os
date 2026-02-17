//! Tenant management routes.
//!
//! This module contains all routes for:
//! - `/v1/tenants/*` - Tenant CRUD, policies, usage, execution policies

use crate::handlers;
use crate::state::AppState;
use axum::{
    routing::{delete, get, post, put},
    Router,
};

/// Build the tenant routes subrouter.
///
/// These routes require authentication and are merged into the protected routes.
pub fn tenant_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/v1/tenants",
            get(handlers::list_tenants).post(handlers::create_tenant),
        )
        .route("/v1/tenants/{tenant_id}", put(handlers::update_tenant))
        .route(
            "/v1/tenants/{tenant_id}/pause",
            post(handlers::pause_tenant),
        )
        .route(
            "/v1/tenants/{tenant_id}/archive",
            post(handlers::archive_tenant),
        )
        .route(
            "/v1/tenants/{tenant_id}/policies",
            post(handlers::assign_tenant_policies),
        )
        .route(
            "/v1/tenants/{tenant_id}/adapters",
            post(handlers::assign_tenant_adapters),
        )
        .route(
            "/v1/tenants/{tenant_id}/usage",
            get(handlers::tenants::get_tenant_usage),
        )
        // Dedicated tenant resource metrics endpoint
        .route(
            "/v1/tenants/{tenant_id}/metrics",
            get(handlers::tenants::get_tenant_metrics),
        )
        .route(
            "/v1/tenants/{tenant_id}/default-stack",
            get(handlers::get_default_stack)
                .put(handlers::set_default_stack)
                .delete(handlers::clear_default_stack),
        )
        .route(
            "/v1/tenants/{tenant_id}/router/config",
            get(handlers::router_config::get_router_config),
        )
        .route(
            "/v1/tenants/{tenant_id}/policy-bindings",
            get(handlers::list_tenant_policy_bindings),
        )
        .route(
            "/v1/tenants/{tenant_id}/policy-bindings/{policy_pack_id}/toggle",
            post(handlers::toggle_tenant_policy),
        )
        .route(
            "/v1/tenants/{tenant_id}/revoke-all-tokens",
            post(handlers::tenants::revoke_tenant_tokens),
        )
        // Tenant execution policy routes
        .route(
            "/v1/tenants/{tenant_id}/execution-policy",
            get(handlers::execution_policy::get_execution_policy)
                .post(handlers::execution_policy::create_execution_policy),
        )
        .route(
            "/v1/tenants/{tenant_id}/execution-policy/{policy_id}",
            put(handlers::execution_policy::update_execution_policy)
                .delete(handlers::execution_policy::deactivate_execution_policy),
        )
        .route(
            "/v1/tenants/{tenant_id}/execution-policy/history",
            get(handlers::execution_policy::get_execution_policy_history),
        )
        // Tenant settings
        .route(
            "/v1/tenants/{tenant_id}/settings",
            get(handlers::tenant_settings::get_tenant_settings)
                .put(handlers::tenant_settings::update_tenant_settings),
        )
        // Event application endpoint
        .route(
            "/v1/tenants/{tenant_id}/events",
            post(handlers::event_applier::apply_tenant_event),
        )
        // Tenant weight encryption key management
        .route(
            "/v1/tenants/{tenant_id}/encryption/keys",
            get(handlers::weight_encryption::list_tenant_keys)
                .post(handlers::weight_encryption::register_tenant_key),
        )
        .route(
            "/v1/tenants/{tenant_id}/encryption/keys/{key_id}",
            delete(handlers::weight_encryption::revoke_tenant_key),
        )
        .route(
            "/v1/tenants/{tenant_id}/encryption/status",
            get(handlers::weight_encryption::get_encryption_status),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_routes_builds() {
        // Verify routes compile and build without panic
        let _router: Router<AppState> = tenant_routes();
    }
}
