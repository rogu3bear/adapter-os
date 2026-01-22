//! Authentication handlers.
//!
//! Full auth flows are enabled, with dev-bypass-only endpoints available in debug builds.
//! Some endpoints (bootstrap, MFA, tenant switch) remain stubbed until implemented.

mod config;
#[cfg(all(feature = "dev-bypass", debug_assertions))]
mod dev_bypass;
mod health;
mod login;
mod register;
mod refresh;
mod sessions;
mod stubs;
mod tenants;
mod types;

// Active handlers
pub use config::{__path_get_auth_config_handler, get_auth_config_handler};
pub use health::{__path_auth_health_handler, auth_health_handler};
pub use login::{__path_login_handler, login_handler};
pub use register::{__path_register_handler, register_handler};
pub use refresh::{__path_refresh_token_handler, refresh_token_handler};
pub use sessions::{
    __path_list_sessions_handler, __path_logout_handler, __path_revoke_session_handler,
    list_sessions_handler, logout_handler, revoke_session_handler,
};
pub use tenants::{__path_list_user_tenants_handler, list_user_tenants_handler};

#[cfg(all(feature = "dev-bypass", debug_assertions))]
pub use dev_bypass::{
    __path_dev_bootstrap_handler, __path_dev_bypass_handler, dev_bootstrap_handler,
    dev_bypass_handler,
};

// Stub handlers (return "use dev bypass" errors)
pub use stubs::{
    __path_bootstrap_admin_handler, __path_mfa_disable_handler, __path_mfa_start_handler,
    __path_mfa_status_handler, __path_mfa_verify_handler, __path_switch_tenant_handler,
};
pub use stubs::{
    bootstrap_admin_handler, mfa_disable_handler, mfa_start_handler, mfa_status_handler,
    mfa_verify_handler, switch_tenant_handler,
};

// Re-export types for OpenAPI schema
pub use types::{
    AuthConfigResponse, AuthHealthResponse, BootstrapRequest, BootstrapResponse, LogoutResponse,
    RefreshResponse, RegisterRequest, RegisterResponse, SessionInfo, SessionsResponse,
};

#[cfg(all(feature = "dev-bypass", debug_assertions))]
pub use types::{DevBootstrapRequest, DevBootstrapResponse};
