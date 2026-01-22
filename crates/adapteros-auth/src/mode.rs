//! Authentication mode types.
//!
//! [`AuthMode`] represents how a caller was authenticated.

use serde::{Deserialize, Serialize};

/// How a caller was authenticated.
///
/// This enum is used throughout the system to track the authentication method
/// and apply appropriate authorization rules.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    /// JWT bearer token in Authorization header or query param
    #[serde(alias = "jwt", alias = "bearer")]
    #[default]
    BearerToken,

    /// JWT token from auth_token cookie
    Cookie,

    /// API key authentication (hashed and validated against database)
    ApiKey,

    /// Development bypass mode (debug builds only, disabled in release)
    ///
    /// # Security
    ///
    /// This mode is ONLY available when:
    /// - The `dev-bypass` feature is enabled
    /// - Running a debug build (`debug_assertions`)
    /// - `AOS_DEV_NO_AUTH=1` environment variable is set
    ///
    /// In release builds, this mode is rejected at boot time.
    DevBypass,

    /// No authentication present (used for public endpoints)
    Unauthenticated,
}

impl AuthMode {
    /// Returns true if this mode represents an authenticated state.
    pub fn is_authenticated(&self) -> bool {
        !matches!(self, AuthMode::Unauthenticated)
    }

    /// Returns true if this is dev bypass mode.
    pub fn is_dev_bypass(&self) -> bool {
        matches!(self, AuthMode::DevBypass)
    }

    /// Returns true if this mode uses a token (JWT or API key).
    pub fn uses_token(&self) -> bool {
        matches!(
            self,
            AuthMode::BearerToken | AuthMode::Cookie | AuthMode::ApiKey
        )
    }

    /// Returns the auth mode as a string suitable for logging.
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthMode::BearerToken => "bearer_token",
            AuthMode::Cookie => "cookie",
            AuthMode::ApiKey => "api_key",
            AuthMode::DevBypass => "dev_bypass",
            AuthMode::Unauthenticated => "unauthenticated",
        }
    }
}

impl std::fmt::Display for AuthMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_mode_is_authenticated() {
        assert!(AuthMode::BearerToken.is_authenticated());
        assert!(AuthMode::Cookie.is_authenticated());
        assert!(AuthMode::ApiKey.is_authenticated());
        assert!(AuthMode::DevBypass.is_authenticated());
        assert!(!AuthMode::Unauthenticated.is_authenticated());
    }

    #[test]
    fn test_auth_mode_is_dev_bypass() {
        assert!(!AuthMode::BearerToken.is_dev_bypass());
        assert!(AuthMode::DevBypass.is_dev_bypass());
    }

    #[test]
    fn test_auth_mode_uses_token() {
        assert!(AuthMode::BearerToken.uses_token());
        assert!(AuthMode::Cookie.uses_token());
        assert!(AuthMode::ApiKey.uses_token());
        assert!(!AuthMode::DevBypass.uses_token());
        assert!(!AuthMode::Unauthenticated.uses_token());
    }

    #[test]
    fn test_auth_mode_serde() {
        let mode = AuthMode::BearerToken;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"bearer_token\"");

        let parsed: AuthMode = serde_json::from_str("\"jwt\"").unwrap();
        assert_eq!(parsed, AuthMode::BearerToken);

        let parsed: AuthMode = serde_json::from_str("\"bearer\"").unwrap();
        assert_eq!(parsed, AuthMode::BearerToken);
    }
}
