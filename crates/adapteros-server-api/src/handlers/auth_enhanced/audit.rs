//! Authentication audit logging for security compliance.
//!
//! Provides structured logging for all security-relevant authentication events.
//! Log levels follow security best practices:
//! - `info!` - Successful operations (login, logout, refresh)
//! - `warn!` - Failed operations that may indicate attacks (failed login, lockout, token reuse)
//! - `error!` - System errors during auth operations
//!
//! # Security Considerations
//!
//! - NEVER log passwords, tokens, or secrets
//! - Log emails at warn/error level only (helps debugging without excessive exposure)
//! - Always include IP address for security correlation
//! - Include request_id when available for distributed tracing

use tracing::{info, warn};

/// Authentication event types for audit logging.
///
/// These map to specific security events that should be tracked for compliance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthEvent {
    /// User successfully logged in
    LoginSuccess,
    /// Login failed due to invalid credentials
    LoginFailedInvalidCredentials,
    /// Login failed because account is disabled
    LoginFailedAccountDisabled,
    /// Login blocked due to rate limiting or lockout
    LoginBlockedLockout,
    /// User successfully logged out
    LogoutSuccess,
    /// Token refresh successful
    TokenRefreshSuccess,
    /// Token refresh failed - invalid/expired token
    TokenRefreshFailed,
    /// Token refresh blocked - rotation mismatch (potential replay attack)
    TokenRefreshRotationMismatch,
    /// Session revoked by user
    SessionRevoked,
    /// User registration successful
    RegistrationSuccess,
    /// Rate limit exceeded for tenant
    RateLimitExceeded,
    /// IP denied by access control
    IpAccessDenied,
    /// Cross-tenant access attempt denied
    CrossTenantAccessDenied,
    /// Cross-tenant access granted (admin)
    CrossTenantAccessGranted,
    /// Dev bypass login (development only)
    DevBypassLogin,
    /// Initial admin user bootstrapped
    BootstrapSuccess,
    /// Bootstrap failed (system already initialized)
    BootstrapFailedAlreadyInitialized,
    /// Tenant switch successful
    TenantSwitchSuccess,
    /// Tenant switch failed - unauthorized access
    TenantSwitchDenied,
}

impl AuthEvent {
    /// Get the event name for logging.
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthEvent::LoginSuccess => "auth.login.success",
            AuthEvent::LoginFailedInvalidCredentials => "auth.login.failed.invalid_credentials",
            AuthEvent::LoginFailedAccountDisabled => "auth.login.failed.account_disabled",
            AuthEvent::LoginBlockedLockout => "auth.login.blocked.lockout",
            AuthEvent::LogoutSuccess => "auth.logout.success",
            AuthEvent::TokenRefreshSuccess => "auth.token_refresh.success",
            AuthEvent::TokenRefreshFailed => "auth.token_refresh.failed",
            AuthEvent::TokenRefreshRotationMismatch => "auth.token_refresh.rotation_mismatch",
            AuthEvent::SessionRevoked => "auth.session.revoked",
            AuthEvent::RegistrationSuccess => "auth.registration.success",
            AuthEvent::RateLimitExceeded => "auth.rate_limit.exceeded",
            AuthEvent::IpAccessDenied => "auth.ip_access.denied",
            AuthEvent::CrossTenantAccessDenied => "auth.cross_tenant.denied",
            AuthEvent::CrossTenantAccessGranted => "auth.cross_tenant.granted",
            AuthEvent::DevBypassLogin => "auth.dev_bypass.login",
            AuthEvent::BootstrapSuccess => "auth.bootstrap.success",
            AuthEvent::BootstrapFailedAlreadyInitialized => {
                "auth.bootstrap.failed.already_initialized"
            }
            AuthEvent::TenantSwitchSuccess => "auth.tenant_switch.success",
            AuthEvent::TenantSwitchDenied => "auth.tenant_switch.denied",
        }
    }

    /// Returns true if this is a failure/warning event.
    pub fn is_failure(&self) -> bool {
        matches!(
            self,
            AuthEvent::LoginFailedInvalidCredentials
                | AuthEvent::LoginFailedAccountDisabled
                | AuthEvent::LoginBlockedLockout
                | AuthEvent::TokenRefreshFailed
                | AuthEvent::TokenRefreshRotationMismatch
                | AuthEvent::RateLimitExceeded
                | AuthEvent::IpAccessDenied
                | AuthEvent::CrossTenantAccessDenied
                | AuthEvent::BootstrapFailedAlreadyInitialized
                | AuthEvent::TenantSwitchDenied
        )
    }
}

/// Log an authentication event with structured fields.
///
/// This is the primary entry point for auth audit logging. It uses appropriate
/// log levels based on the event type.
///
/// # Arguments
///
/// * `event` - The type of authentication event
/// * `user_id` - User ID (if known)
/// * `email` - User email (only logged for failures to aid debugging)
/// * `tenant_id` - Tenant ID (if known)
/// * `ip_address` - Client IP address
/// * `session_id` - Session ID (if applicable)
/// * `details` - Additional context (e.g., failure reason)
///
/// # Example
///
/// ```ignore
/// log_auth_event(
///     AuthEvent::LoginSuccess,
///     Some(&user.id),
///     None,  // Don't log email on success
///     Some(&user.tenant_id),
///     Some(&ip_address),
///     Some(&session_id),
///     None,
/// );
/// ```
#[allow(clippy::too_many_arguments)]
pub fn log_auth_event(
    event: AuthEvent,
    user_id: Option<&str>,
    email: Option<&str>,
    tenant_id: Option<&str>,
    ip_address: Option<&str>,
    session_id: Option<&str>,
    details: Option<&str>,
) {
    let event_name = event.as_str();
    let user_id = user_id.unwrap_or("-");
    let tenant_id = tenant_id.unwrap_or("-");
    let ip_address = ip_address.unwrap_or("-");
    let session_id = session_id.unwrap_or("-");

    if event.is_failure() {
        // For failures, include email to aid debugging (security trade-off)
        let email = email.unwrap_or("-");
        if let Some(details) = details {
            warn!(
                event = %event_name,
                user_id = %user_id,
                email = %email,
                tenant_id = %tenant_id,
                ip = %ip_address,
                session_id = %session_id,
                details = %details,
                "Authentication event"
            );
        } else {
            warn!(
                event = %event_name,
                user_id = %user_id,
                email = %email,
                tenant_id = %tenant_id,
                ip = %ip_address,
                session_id = %session_id,
                "Authentication event"
            );
        }
    } else {
        // For success events, don't log email (privacy)
        if let Some(details) = details {
            info!(
                event = %event_name,
                user_id = %user_id,
                tenant_id = %tenant_id,
                ip = %ip_address,
                session_id = %session_id,
                details = %details,
                "Authentication event"
            );
        } else {
            info!(
                event = %event_name,
                user_id = %user_id,
                tenant_id = %tenant_id,
                ip = %ip_address,
                session_id = %session_id,
                "Authentication event"
            );
        }
    }
}

/// Log a lockout event with detailed context.
///
/// Lockouts are security-critical and deserve detailed logging.
pub fn log_lockout_event(
    email: &str,
    ip_address: &str,
    reason: &str,
    lockout_duration_minutes: Option<i64>,
) {
    if let Some(duration) = lockout_duration_minutes {
        warn!(
            event = "auth.lockout.triggered",
            email = %email,
            ip = %ip_address,
            reason = %reason,
            duration_minutes = %duration,
            "Account lockout triggered"
        );
    } else {
        warn!(
            event = "auth.lockout.triggered",
            email = %email,
            ip = %ip_address,
            reason = %reason,
            "Account lockout triggered"
        );
    }
}

/// Log a rate limit event.
pub fn log_rate_limit_event(
    tenant_id: &str,
    ip_address: Option<&str>,
    current_count: i64,
    limit: i64,
) {
    let ip = ip_address.unwrap_or("-");
    warn!(
        event = "auth.rate_limit.exceeded",
        tenant_id = %tenant_id,
        ip = %ip,
        current_count = %current_count,
        limit = %limit,
        "Rate limit exceeded"
    );
}

/// Log cross-tenant access attempt.
pub fn log_cross_tenant_access(
    user_id: &str,
    user_email: &str,
    user_role: &str,
    user_tenant: &str,
    resource_tenant: &str,
    admin_tenants: &[String],
    granted: bool,
    reason: Option<&str>,
) {
    let event = if granted {
        AuthEvent::CrossTenantAccessGranted
    } else {
        AuthEvent::CrossTenantAccessDenied
    };

    if granted {
        info!(
            event = %event.as_str(),
            user_id = %user_id,
            user_role = %user_role,
            user_tenant = %user_tenant,
            resource_tenant = %resource_tenant,
            admin_tenants = ?admin_tenants,
            reason = %reason.unwrap_or("-"),
            "Cross-tenant access"
        );
    } else {
        warn!(
            event = %event.as_str(),
            user_id = %user_id,
            email = %user_email,
            user_role = %user_role,
            user_tenant = %user_tenant,
            resource_tenant = %resource_tenant,
            admin_tenants = ?admin_tenants,
            reason = %reason.unwrap_or("tenant_isolation_violation"),
            "Cross-tenant access denied"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_event_names() {
        assert_eq!(AuthEvent::LoginSuccess.as_str(), "auth.login.success");
        assert_eq!(
            AuthEvent::LoginFailedInvalidCredentials.as_str(),
            "auth.login.failed.invalid_credentials"
        );
        assert_eq!(
            AuthEvent::TokenRefreshRotationMismatch.as_str(),
            "auth.token_refresh.rotation_mismatch"
        );
    }

    #[test]
    fn test_is_failure() {
        assert!(!AuthEvent::LoginSuccess.is_failure());
        assert!(!AuthEvent::LogoutSuccess.is_failure());
        assert!(!AuthEvent::TokenRefreshSuccess.is_failure());
        assert!(AuthEvent::LoginFailedInvalidCredentials.is_failure());
        assert!(AuthEvent::LoginBlockedLockout.is_failure());
        assert!(AuthEvent::TokenRefreshRotationMismatch.is_failure());
        assert!(AuthEvent::CrossTenantAccessDenied.is_failure());
    }
}
