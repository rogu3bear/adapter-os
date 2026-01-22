//! Centralized authentication configuration and enforcement for AdapterOS.
//!
//! This crate provides the single source of truth for authentication:
//!
//! - [`AuthMode`]: How a caller was authenticated (JWT, API key, cookie, dev bypass)
//! - [`AuthConfig`]: Configuration loaded at boot (issuer, audience, keys, TTLs)
//! - [`AuthState`]: Runtime state including verifiers and caches
//! - [`AuthError`]: Typed errors mapping to HTTP status codes
//!
//! # Boot Invariants
//!
//! The auth configuration is validated at boot time with the following rules:
//!
//! - **Release builds**: `DevBypass` mode fails boot immediately
//! - **JWT mode**: Requires valid issuer + audience + key material
//! - **API key mode**: Requires at least one key configured
//!
//! # Router Enforcement
//!
//! All routes are protected by default. Public paths must be explicitly allowlisted:
//!
//! - `/healthz`, `/readyz` - Health checks
//! - `/metrics` - Prometheus metrics
//! - `/v1/invariants` - Boot invariant status
//! - `/v1/auth/login`, `/v1/auth/register` - Auth endpoints
//!
//! # Usage
//!
//! ```rust,ignore
//! use adapteros_auth::{AuthConfig, AuthState, AuthMode};
//!
//! // Load config at boot
//! let config = AuthConfig::from_env_and_config(&config)?;
//!
//! // Validate boot invariants (fails in release if dev bypass enabled)
//! config.validate_boot_invariants()?;
//!
//! // Create runtime state
//! let state = AuthState::new(config)?;
//! ```

mod config;
mod context;
mod error;
mod mode;
mod public_paths;
mod state;

pub use config::{ApiKeyConfig, AuthConfig, CookieConfig, JwtAlgorithm, JwtConfig};
pub use context::{AuthContext, Principal, PrincipalType};
pub use error::{AuthError, AuthResult};
pub use mode::AuthMode;
pub use public_paths::{is_public_path, PUBLIC_PATHS};
pub use state::AuthState;

/// Re-export commonly used claim types
pub use context::Claims;

/// JWT issuer constant
pub const JWT_ISSUER: &str = "adapteros-server";

/// Default access token TTL in seconds (15 minutes)
pub const DEFAULT_ACCESS_TOKEN_TTL_SECS: u64 = 15 * 60;

/// Default session TTL in seconds (2 hours)
pub const DEFAULT_SESSION_TTL_SECS: u64 = 2 * 60 * 60;

/// Default refresh token TTL in seconds (7 days)
pub const DEFAULT_REFRESH_TOKEN_TTL_SECS: u64 = 7 * 24 * 60 * 60;

/// Check if dev bypass is active.
///
/// # Safety
///
/// This function is only meaningful in debug builds. In release builds,
/// it always returns `false` regardless of environment variables.
pub fn is_dev_bypass_active() -> bool {
    #[cfg(all(feature = "dev-bypass", debug_assertions))]
    {
        std::env::var("AOS_DEV_NO_AUTH")
            .map(|v| {
                let lower = v.to_ascii_lowercase();
                matches!(lower.as_str(), "1" | "true" | "yes" | "on")
            })
            .unwrap_or(false)
    }

    #[cfg(not(all(feature = "dev-bypass", debug_assertions)))]
    {
        false
    }
}

/// Check if dev bypass was requested via environment variable.
///
/// This returns true even in release builds if the env var is set,
/// allowing boot invariants to detect and reject the configuration.
pub fn dev_bypass_env_requested() -> bool {
    std::env::var("AOS_DEV_NO_AUTH")
        .map(|v| {
            let lower = v.to_ascii_lowercase();
            matches!(lower.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(JWT_ISSUER, "adapteros-server");
        assert_eq!(DEFAULT_ACCESS_TOKEN_TTL_SECS, 15 * 60);
        assert_eq!(DEFAULT_SESSION_TTL_SECS, 2 * 60 * 60);
    }

    #[test]
    fn test_dev_bypass_disabled_in_release() {
        // In test builds (which have debug_assertions), behavior depends on feature flag
        // This test just ensures the function doesn't panic
        let _ = is_dev_bypass_active();
    }
}
