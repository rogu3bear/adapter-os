//! Enhanced authentication handlers
//!
//! This module provides comprehensive authentication functionality including:
//! - Login/logout with session management
//! - Token refresh with rotation
//! - Multi-tenant support with tenant switching
//! - MFA (TOTP and backup codes)
//! - Session listing and revocation
//! - Development bypass endpoints

mod config;
#[cfg(all(feature = "dev-bypass", debug_assertions))]
mod dev_bypass;
mod health;
mod helpers;
mod login;
mod logout;
mod mfa;
mod refresh;
mod sessions;
mod tenants;
mod tokens;
mod types;

// Re-export all handlers for route registration
pub use config::{__path_get_auth_config_handler, get_auth_config_handler};
#[cfg(all(feature = "dev-bypass", debug_assertions))]
pub use dev_bypass::{
    __path_dev_bootstrap_handler, __path_dev_bypass_handler, dev_bootstrap_handler,
    dev_bypass_handler,
};
pub use health::{__path_auth_health_handler, auth_health_handler};
pub use login::{__path_bootstrap_admin_handler, bootstrap_admin_handler, login_handler};
pub use logout::{__path_logout_handler, logout_handler};
pub use mfa::{
    __path_mfa_disable_handler, __path_mfa_start_handler, __path_mfa_status_handler,
    __path_mfa_verify_handler, mfa_disable_handler, mfa_start_handler, mfa_status_handler,
    mfa_verify_handler,
};
pub use refresh::{__path_refresh_token_handler, refresh_token_handler};
pub use sessions::{
    __path_list_sessions_handler, __path_revoke_session_handler, list_sessions_handler,
    revoke_session_handler,
};
pub use tenants::{
    __path_list_user_tenants_handler, __path_switch_tenant_handler, list_user_tenants_handler,
    switch_tenant_handler,
};

// Re-export types needed by other modules
pub use types::{
    AuthConfigResponse, AuthHealthResponse, BootstrapRequest, BootstrapResponse, LogoutResponse,
    RefreshResponse, SessionInfo, SessionsResponse,
};

#[cfg(all(feature = "dev-bypass", debug_assertions))]
pub use types::{DevBootstrapRequest, DevBootstrapResponse};

