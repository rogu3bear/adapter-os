//! Runtime authentication state.
//!
//! [`AuthState`] holds the runtime state for authentication including
//! key material, verifiers, and caches.

use crate::config::{AuthConfig, JwtAlgorithm};
use crate::error::{AuthError, AuthResult};
use std::sync::Arc;

/// Runtime authentication state.
///
/// This struct is created at boot time after configuration validation
/// and provides the runtime state needed for authentication operations.
#[derive(Clone)]
pub struct AuthState {
    /// The validated configuration
    config: Arc<AuthConfig>,

    /// Whether Ed25519 signing is enabled
    use_ed25519: bool,

    /// JWT secret for HMAC signing (HS256 mode)
    jwt_secret: Arc<Vec<u8>>,

    /// Primary key ID for signing
    primary_kid: String,

    /// Ed25519 public keys for verification (kid, PEM)
    ed25519_public_keys: Vec<(String, String)>,

    /// HMAC keys for verification (kid, secret)
    hmac_keys: Vec<(String, Vec<u8>)>,
}

impl std::fmt::Debug for AuthState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthState")
            .field("use_ed25519", &self.use_ed25519)
            .field("primary_kid", &self.primary_kid)
            .field("ed25519_key_count", &self.ed25519_public_keys.len())
            .field("hmac_key_count", &self.hmac_keys.len())
            .finish()
    }
}

impl AuthState {
    /// Create a new AuthState from validated configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if key material cannot be loaded.
    pub fn new(config: AuthConfig) -> AuthResult<Self> {
        let use_ed25519 = config.jwt.algorithm == JwtAlgorithm::EdDSA;

        // Get JWT secret
        let jwt_secret = config.jwt.hmac_secret.clone().unwrap_or_default();

        // Get primary key ID
        let primary_kid = config
            .jwt
            .primary_kid
            .clone()
            .unwrap_or_else(|| "default".to_string());

        // Clone key rotation data
        let ed25519_public_keys = config.jwt.ed25519_public_keys.clone();
        let hmac_keys = config.jwt.hmac_keys.clone();

        tracing::info!(
            use_ed25519 = use_ed25519,
            primary_kid = %primary_kid,
            ed25519_key_count = ed25519_public_keys.len(),
            hmac_key_count = hmac_keys.len(),
            "AuthState initialized"
        );

        Ok(Self {
            config: Arc::new(config),
            use_ed25519,
            jwt_secret: Arc::new(jwt_secret),
            primary_kid,
            ed25519_public_keys,
            hmac_keys,
        })
    }

    /// Get the configuration.
    pub fn config(&self) -> &AuthConfig {
        &self.config
    }

    /// Check if Ed25519 signing is enabled.
    pub fn use_ed25519(&self) -> bool {
        self.use_ed25519
    }

    /// Get the JWT secret for HMAC signing.
    pub fn jwt_secret(&self) -> &[u8] {
        &self.jwt_secret
    }

    /// Get the primary key ID.
    pub fn primary_kid(&self) -> &str {
        &self.primary_kid
    }

    /// Get Ed25519 public keys for verification.
    pub fn ed25519_public_keys(&self) -> &[(String, String)] {
        &self.ed25519_public_keys
    }

    /// Get HMAC keys for verification.
    pub fn hmac_keys(&self) -> &[(String, Vec<u8>)] {
        &self.hmac_keys
    }

    /// Get access token TTL in seconds.
    pub fn access_token_ttl_secs(&self) -> u64 {
        self.config.jwt.access_token_ttl_secs
    }

    /// Get refresh token TTL in seconds.
    pub fn refresh_token_ttl_secs(&self) -> u64 {
        self.config.jwt.refresh_token_ttl_secs
    }

    /// Get session TTL in seconds.
    pub fn session_ttl_secs(&self) -> u64 {
        self.config.jwt.session_ttl_secs
    }

    /// Get the issuer string.
    pub fn issuer(&self) -> &str {
        &self.config.jwt.issuer
    }

    /// Get clock skew tolerance in seconds.
    pub fn clock_skew_secs(&self) -> u64 {
        self.config.jwt.clock_skew_secs
    }

    /// Check if dev bypass is allowed.
    pub fn dev_bypass_allowed(&self) -> bool {
        self.config.dev_bypass_allowed
    }

    /// Check if dev login is enabled.
    pub fn dev_login_enabled(&self) -> bool {
        self.config.dev_login_enabled
    }

    /// Check if dev login is actually allowed (considering build mode).
    pub fn dev_login_allowed(&self) -> bool {
        self.config.dev_login_allowed()
    }

    /// Get lockout threshold.
    pub fn lockout_threshold(&self) -> u32 {
        self.config.lockout_threshold
    }

    /// Get lockout cooldown in seconds.
    pub fn lockout_cooldown_secs(&self) -> u64 {
        self.config.lockout_cooldown_secs
    }

    /// Get cookie configuration.
    pub fn cookie_same_site(&self) -> &str {
        &self.config.cookie.same_site
    }

    /// Get cookie secure flag.
    pub fn cookie_secure(&self) -> bool {
        self.config.cookie.secure
    }

    /// Get cookie domain.
    pub fn cookie_domain(&self) -> Option<&str> {
        self.config.cookie.domain.as_deref()
    }

    /// Validate that the state is properly configured for production.
    ///
    /// This performs additional runtime checks beyond boot invariants.
    pub fn validate_for_production(&self) -> AuthResult<()> {
        // In production, Ed25519 should be enabled
        if !self.use_ed25519 {
            tracing::warn!("HMAC/HS256 auth is enabled; Ed25519 is recommended for production");
        }

        // Verify we have key material for the configured algorithm
        if self.use_ed25519 && self.ed25519_public_keys.is_empty() {
            return Err(AuthError::CryptoError(
                "Ed25519 mode enabled but no public keys available".to_string(),
            ));
        }

        if !self.use_ed25519 && self.jwt_secret.is_empty() && self.hmac_keys.is_empty() {
            return Err(AuthError::CryptoError(
                "HMAC mode enabled but no secret available".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_state_creation() {
        let mut config = AuthConfig::default();
        config.jwt.hmac_secret = Some(b"test-secret".to_vec());

        let state = AuthState::new(config).unwrap();
        assert!(!state.use_ed25519());
        assert_eq!(state.jwt_secret(), b"test-secret");
    }

    #[test]
    fn test_auth_state_ed25519() {
        let mut config = AuthConfig::default();
        config.jwt.algorithm = JwtAlgorithm::EdDSA;
        config.jwt.ed25519_public_keys = vec![("key-1".to_string(), "pem-data".to_string())];

        let state = AuthState::new(config).unwrap();
        assert!(state.use_ed25519());
        assert_eq!(state.ed25519_public_keys().len(), 1);
    }

    #[test]
    fn test_auth_state_accessors() {
        let config = AuthConfig::default();
        let state = AuthState::new(config).unwrap();

        assert_eq!(state.issuer(), "adapteros-server");
        assert_eq!(state.access_token_ttl_secs(), 15 * 60);
        assert_eq!(state.session_ttl_secs(), 2 * 60 * 60);
    }
}
