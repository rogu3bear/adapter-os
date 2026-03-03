//! Admin and policy endpoints for adapteros server
//!
//! This crate provides administrative and policy management endpoints for the AdapterOS control plane.
//! It includes:
//! - User management (list users)
//! - Lifecycle control (shutdown, restart, drain, maintenance)
//! - Service control (start, stop, restart services via supervisor)
//! - Plugin management (enable, disable, status)
//! - System settings management (get/update configuration)
//!
//! ## Usage
//!
//! The crate exports handlers that are generic over `AdminAppState`. The main server
//! must implement this trait to provide access to required services (database, UDS client,
//! supervisor client, plugin registry, boot state manager).
//!
//! ```rust,ignore
//! use adapteros_server_api_admin::{admin_routes, AdminAppState};
//!
//! // Implement AdminAppState for your AppState
//! impl AdminAppState for MyAppState { ... }
//!
//! // Add admin routes to your router
//! let router = Router::new()
//!     .nest("/", admin_routes::<MyAppState>());
//! ```

// Axum handlers return (StatusCode, Json<ErrorResponse>) tuples which are large
// but this is the idiomatic axum pattern -- boxing would change all call sites.
#![allow(clippy::result_large_err)]

pub mod auth;
pub mod boot_state_impl;
pub mod db_impl;
pub mod handlers;
pub mod middleware;
pub mod plugin_impl;
pub mod policies;
pub mod routes;
pub mod state;
pub mod supervisor_impl;
pub mod types;
pub mod uds_impl;

// Re-export main entry points
pub use auth::AdminClaims;
pub use routes::{admin_routes, admin_status_routes, simple_admin_routes};
pub use state::AdminAppState;
pub use types::AdminErrorResponse;

// Re-export handlers for direct use
pub use handlers::{
    // Status
    admin_status,
    // Plugins
    disable_plugin,
    enable_plugin,
    // Services
    // Settings
    get_settings,
    list_plugins,
    // Users
    list_users,
    plugin_status,
    // Lifecycle
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
    AdminStatusResponse,
    MaintenanceScope,
    RequestMaintenanceBody,
    RequestShutdownBody,
    ServiceControlRequest,
    ServiceControlResponse,
    ShutdownMode,
    SystemConfigResponse,
    WorkerMaintenanceResult,
};

// Re-export lifecycle RuntimeMode for handlers
pub use handlers::lifecycle::RuntimeMode;
