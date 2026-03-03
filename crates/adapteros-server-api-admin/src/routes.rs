//! Admin routes
//!
//! Router configuration for administrative endpoints.
//!
//! Routes are organized by functionality:
//! - Status: Simple health/status endpoints (no auth required)
//! - Admin: User management, lifecycle, settings (protected)
//! - Services: Service control via supervisor (protected)
//! - Plugins: Plugin management (protected)

use axum::{
    routing::{get, post},
    Router,
};

use crate::handlers::{
    admin_status,
    // Plugin handlers
    disable_plugin,
    enable_plugin,
    // Service handlers
    // Settings handlers
    get_settings,
    list_plugins,
    // User handlers
    list_users,
    plugin_status,
    // Lifecycle handlers
    request_maintenance,
    request_shutdown,
    restart_service,
    safe_restart,
    start_essential_services,
    start_service,
    stop_essential_services,
    stop_service,
    system_config,
    update_settings,
};
use crate::state::AdminAppState;

/// Build the admin router with status endpoints only
///
/// Returns a router with simple status endpoints that don't require AppState:
/// - `GET /admin/status` - Admin status
/// - `GET /admin/config` - System configuration
pub fn admin_status_routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/admin/status", get(admin_status))
        .route("/admin/config", get(system_config))
}

/// Build the full admin router
///
/// Returns a router with all administrative endpoints.
/// Note: This requires AppState to implement AdminAppState trait.
///
/// ## Routes
///
/// ### Status (no auth)
/// - `GET /admin/status` - Admin status
/// - `GET /admin/config` - System configuration
///
/// ### Users (admin only)
/// - `GET /v1/admin/users` - List users with pagination
///
/// ### Lifecycle (admin only)
/// - `POST /admin/lifecycle/request-shutdown` - Request shutdown
/// - `POST /admin/lifecycle/request-maintenance` - Request maintenance mode
/// - `POST /admin/lifecycle/safe-restart` - Safe restart (maintenance + drain; supervisor restart)
///
/// ### Services (NodeManage permission)
/// - `POST /v1/services/:id/start` - Start a service
/// - `POST /v1/services/:id/stop` - Stop a service
/// - `POST /v1/services/:id/restart` - Restart a service
/// - `POST /v1/services/essential/start` - Start all essential services
/// - `POST /v1/services/essential/stop` - Stop all essential services
///
/// ### Plugins (viewer+ for read, operator+ for write)
/// - `GET /v1/plugins` - List all plugins
/// - `GET /v1/plugins/:name` - Get plugin status
/// - `POST /v1/plugins/:name/enable` - Enable a plugin
/// - `POST /v1/plugins/:name/disable` - Disable a plugin
///
/// ### Settings (admin only)
/// - `GET /v1/settings` - Get system settings
/// - `PUT /v1/settings` - Update system settings
pub fn admin_routes<S>() -> Router<S>
where
    S: AdminAppState,
{
    Router::new()
        // Status routes (simple, no complex state)
        .route("/admin/status", get(admin_status))
        .route("/admin/config", get(system_config))
        // User management routes
        .route("/v1/admin/users", get(list_users::<S>))
        // Lifecycle routes
        .route(
            "/admin/lifecycle/request-shutdown",
            post(request_shutdown::<S>),
        )
        .route(
            "/admin/lifecycle/request-maintenance",
            post(request_maintenance::<S>),
        )
        .route("/admin/lifecycle/safe-restart", post(safe_restart::<S>))
        // Service control routes
        .route("/v1/services/:service_id/start", post(start_service::<S>))
        .route("/v1/services/:service_id/stop", post(stop_service::<S>))
        .route(
            "/v1/services/:service_id/restart",
            post(restart_service::<S>),
        )
        .route(
            "/v1/services/essential/start",
            post(start_essential_services::<S>),
        )
        .route(
            "/v1/services/essential/stop",
            post(stop_essential_services::<S>),
        )
        // Plugin routes
        .route("/v1/plugins", get(list_plugins::<S>))
        .route("/v1/plugins/:name", get(plugin_status::<S>))
        .route("/v1/plugins/:name/enable", post(enable_plugin::<S>))
        .route("/v1/plugins/:name/disable", post(disable_plugin::<S>))
        // Settings routes
        .route(
            "/v1/settings",
            get(get_settings::<S>).put(update_settings::<S>),
        )
}

/// Alias for backward compatibility
pub use admin_status_routes as simple_admin_routes;
