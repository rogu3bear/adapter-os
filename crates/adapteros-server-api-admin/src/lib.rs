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

pub mod auth;
pub mod handlers;
pub mod middleware;
pub mod policies;
pub mod routes;
pub mod state;
pub mod types;

// Re-export main entry points
pub use auth::AdminClaims;
pub use routes::{admin_routes, admin_status_routes, simple_admin_routes};
pub use state::AdminAppState;
pub use types::AdminErrorResponse;

// Re-export handlers for direct use
pub use handlers::{
    // Lifecycle
    request_maintenance, request_shutdown, safe_restart,
    MaintenanceScope, RequestMaintenanceBody, RequestShutdownBody, ShutdownMode,
    WorkerMaintenanceResult,
    // Services
    get_service_logs, restart_service, start_essential_services, start_service,
    stop_essential_services, stop_service,
    LogsQuery, ServiceControlRequest, ServiceControlResponse,
    // Plugins
    disable_plugin, enable_plugin, list_plugins, plugin_status,
    // Settings
    get_settings, update_settings,
    // Users
    list_users,
    // Status
    admin_status, system_config, AdminStatusResponse, SystemConfigResponse,
};

// Re-export lifecycle RuntimeMode for handlers
pub use handlers::lifecycle::RuntimeMode;
