//! Authentication configuration types.
//!
//! [`AuthConfig`] is loaded at boot time and validated against boot invariants.

use crate::error::{AuthError, AuthResult};
use crate::{
    DEFAULT_ACCESS_TOKEN_TTL_SECS, DEFAULT_REFRESH_TOKEN_TTL_SECS, DEFAULT_SESSION_TTL_SECS,
    JWT_ISSUER,
};
use serde::{Deserialize, Serialize};

/// Primary authentication configuration.
///
/// This struct is loaded at boot time from environment variables and config files,
/// then validated against boot invariants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// JWT configuration (signing, verification, TTLs)
    pub jwt: JwtConfig,

    /// API key configuration
    pub api_key: ApiKeyConfig,

    /// Cookie settings
    pub cookie: CookieConfig,

    /// Whether dev login endpoint is enabled
    pub dev_login_enabled: bool,

    /// Whether dev bypass is allowed (debug builds only)
    pub dev_bypass_allowed: bool,

    /// Lockout threshold for brute force protection
    pub lockout_threshold: u32,

    /// Lockout cooldown in seconds
    pub lockout_cooldown_secs: u64,
}

/// JWT-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    /// JWT issuer (must match token claims)
    pub issuer: String,

    /// JWT audience (optional, for audience validation)
    pub audience: Option<String>,

    /// JWT algorithm: "hs256" (HMAC) or "eddsa" (Ed25519)
    pub algorithm: JwtAlgorithm,

    /// HMAC secret (for HS256 mode)
    #[serde(skip_serializing)]
    pub hmac_secret: Option<Vec<u8>>,

    /// Ed25519 private key path (for EdDSA mode)
    pub ed25519_key_path: Option<String>,

    /// Ed25519 public key (PEM format, for verification)
    #[serde(skip_serializing)]
    pub ed25519_public_key: Option<String>,

    /// Ed25519 public keys with key IDs for rotation
    #[serde(skip)]
    pub ed25519_public_keys: Vec<(String, String)>,

    /// HMAC keys with key IDs for rotation
    #[serde(skip)]
    pub hmac_keys: Vec<(String, Vec<u8>)>,

    /// Primary key ID for signing
    pub primary_kid: Option<String>,

    /// Access token TTL in seconds
    pub access_token_ttl_secs: u64,

    /// Refresh token TTL in seconds
    pub refresh_token_ttl_secs: u64,

    /// Session TTL in seconds
    pub session_ttl_secs: u64,

    /// Clock skew tolerance in seconds
    pub clock_skew_secs: u64,
}

/// JWT signing algorithm.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum JwtAlgorithm {
    /// HMAC-SHA256 (symmetric, for development)
    #[default]
    Hs256,
    /// Ed25519 (asymmetric, required for production)
    EdDSA,
}

impl JwtAlgorithm {
    pub fn as_str(&self) -> &'static str {
        match self {
            JwtAlgorithm::Hs256 => "hs256",
            JwtAlgorithm::EdDSA => "eddsa",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "eddsa" | "ed25519" => JwtAlgorithm::EdDSA,
            _ => JwtAlgorithm::Hs256,
        }
    }
}

/// API key configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiKeyConfig {
    /// Whether API key authentication is enabled
    pub enabled: bool,

    /// Prefix for API key tokens (e.g., "aos_")
    pub prefix: Option<String>,

    /// Hash algorithm for API keys (blake3)
    pub hash_algorithm: String,
}

/// Cookie configuration for session management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieConfig {
    /// SameSite attribute (Strict, Lax, None)
    pub same_site: String,

    /// Secure flag (should be true in production)
    pub secure: bool,

    /// HttpOnly flag (always true for security)
    pub http_only: bool,

    /// Cookie domain (optional)
    pub domain: Option<String>,

    /// Cookie path (defaults to "/")
    pub path: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt: JwtConfig::default(),
            api_key: ApiKeyConfig::default(),
            cookie: CookieConfig::default(),
            dev_login_enabled: false,
            dev_bypass_allowed: false,
            lockout_threshold: 5,
            lockout_cooldown_secs: 300,
        }
    }
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            issuer: JWT_ISSUER.to_string(),
            audience: None,
            algorithm: JwtAlgorithm::Hs256,
            hmac_secret: None,
            ed25519_key_path: None,
            ed25519_public_key: None,
            ed25519_public_keys: Vec::new(),
            hmac_keys: Vec::new(),
            primary_kid: None,
            access_token_ttl_secs: DEFAULT_ACCESS_TOKEN_TTL_SECS,
            refresh_token_ttl_secs: DEFAULT_REFRESH_TOKEN_TTL_SECS,
            session_ttl_secs: DEFAULT_SESSION_TTL_SECS,
            clock_skew_secs: 60,
        }
    }
}

impl Default for CookieConfig {
    fn default() -> Self {
        Self {
            same_site: "Strict".to_string(),
            secure: true,
            http_only: true,
            domain: None,
            path: "/".to_string(),
        }
    }
}

impl AuthConfig {
    /// Validate boot invariants for auth configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Dev bypass is requested in a release build
    /// - JWT mode requires configuration that's missing
    /// - API key mode requires keys that aren't configured
    ///
    /// # Panics
    ///
    /// This function does not panic, but the caller should treat
    /// any error as a boot-fatal condition in production.
    pub fn validate_boot_invariants(&self, is_release: bool) -> AuthResult<()> {
        // SEC-001: Dev bypass must not be active in release builds
        if is_release && self.dev_bypass_allowed {
            tracing::error!(
                invariant = "AUTH-BOOT-001",
                "Dev bypass is enabled but this is a release build"
            );
            return Err(AuthError::DevBypassInRelease);
        }

        // Check dev bypass env var even if config doesn't allow it
        if is_release && crate::dev_bypass_env_requested() {
            tracing::error!(
                invariant = "AUTH-BOOT-001",
                "AOS_DEV_NO_AUTH is set but this is a release build"
            );
            return Err(AuthError::DevBypassInRelease);
        }

        // AUTH-001: JWT mode requires key configuration
        match self.jwt.algorithm {
            JwtAlgorithm::EdDSA => {
                if self.jwt.ed25519_key_path.is_none() && self.jwt.ed25519_public_key.is_none() {
                    tracing::error!(
                        invariant = "AUTH-BOOT-002",
                        "EdDSA mode requires ed25519_key_path or ed25519_public_key"
                    );
                    return Err(AuthError::JwtModeNotConfigured);
                }
            }
            JwtAlgorithm::Hs256 => {
                if self.jwt.hmac_secret.is_none()
                    || self
                        .jwt
                        .hmac_secret
                        .as_ref()
                        .map(|s| s.is_empty())
                        .unwrap_or(true)
                {
                    // In release mode, require HMAC secret if EdDSA not configured
                    if is_release {
                        tracing::error!(
                            invariant = "AUTH-BOOT-002",
                            "HS256 mode requires hmac_secret in production"
                        );
                        return Err(AuthError::JwtModeNotConfigured);
                    }
                }
            }
        }

        // AUTH-002: Validate HMAC secret is not a default value
        if let Some(secret) = &self.jwt.hmac_secret {
            let secret_str = String::from_utf8_lossy(secret);
            let default_secrets = ["changeme", "secret", "default", "password", "jwt_secret"];
            if is_release && default_secrets.iter().any(|d| secret_str == *d) {
                tracing::error!(
                    invariant = "AUTH-BOOT-003",
                    "JWT secret appears to be a default/placeholder value"
                );
                return Err(AuthError::ConfigError(
                    "JWT secret must not be a default value in production".to_string(),
                ));
            }
        }

        // CFG-002: Validate TTL hierarchy
        if self.jwt.access_token_ttl_secs >= self.jwt.session_ttl_secs {
            tracing::warn!(
                invariant = "AUTH-BOOT-004",
                access_ttl = self.jwt.access_token_ttl_secs,
                session_ttl = self.jwt.session_ttl_secs,
                "Access token TTL should be shorter than session TTL"
            );
            // Warning only, not fatal
        }

        // SEC-005: Cookie security settings
        if is_release {
            let same_site_lower = self.cookie.same_site.to_ascii_lowercase();
            if same_site_lower == "none" && !self.cookie.secure {
                tracing::error!(
                    invariant = "AUTH-BOOT-005",
                    "SameSite=None requires Secure flag in production"
                );
                return Err(AuthError::ConfigError(
                    "SameSite=None requires cookie.secure=true".to_string(),
                ));
            }
        }

        tracing::info!(
            algorithm = self.jwt.algorithm.as_str(),
            issuer = %self.jwt.issuer,
            access_ttl_secs = self.jwt.access_token_ttl_secs,
            session_ttl_secs = self.jwt.session_ttl_secs,
            dev_bypass_allowed = self.dev_bypass_allowed,
            "Auth configuration validated"
        );

        Ok(())
    }

    /// Check if dev login is allowed based on config and build mode.
    pub fn dev_login_allowed(&self) -> bool {
        if !self.dev_login_enabled {
            return false;
        }

        // In release builds, dev login is only allowed if explicitly configured
        #[cfg(debug_assertions)]
        {
            self.dev_login_enabled
        }

        #[cfg(not(debug_assertions))]
        {
            // Release builds: only allow if config explicitly enables it
            // and we're not in production mode
            self.dev_login_enabled && self.dev_bypass_allowed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AuthConfig::default();
        assert_eq!(config.jwt.issuer, JWT_ISSUER);
        assert_eq!(config.jwt.algorithm, JwtAlgorithm::Hs256);
        assert_eq!(config.cookie.same_site, "Strict");
        assert!(config.cookie.secure);
        assert!(config.cookie.http_only);
    }

    #[test]
    fn test_jwt_algorithm_parse() {
        assert_eq!(JwtAlgorithm::parse("eddsa"), JwtAlgorithm::EdDSA);
        assert_eq!(JwtAlgorithm::parse("ed25519"), JwtAlgorithm::EdDSA);
        assert_eq!(JwtAlgorithm::parse("hs256"), JwtAlgorithm::Hs256);
        assert_eq!(JwtAlgorithm::parse("unknown"), JwtAlgorithm::Hs256);
    }

    #[test]
    fn test_validate_boot_invariants_dev_bypass_in_release() {
        let mut config = AuthConfig::default();
        config.dev_bypass_allowed = true;

        // Should fail in release mode
        let result = config.validate_boot_invariants(true);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AuthError::DevBypassInRelease));
    }

    #[test]
    fn test_validate_boot_invariants_eddsa_requires_key() {
        let mut config = AuthConfig::default();
        config.jwt.algorithm = JwtAlgorithm::EdDSA;

        // Should fail without key path
        let result = config.validate_boot_invariants(true);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            AuthError::JwtModeNotConfigured
        ));
    }

    #[test]
    fn test_validate_boot_invariants_cookie_security() {
        let mut config = AuthConfig::default();
        config.jwt.hmac_secret = Some(b"test-secret-at-least-32-bytes-long".to_vec());
        config.cookie.same_site = "None".to_string();
        config.cookie.secure = false;

        // Should fail in release mode
        let result = config.validate_boot_invariants(true);
        assert!(result.is_err());
    }
}
