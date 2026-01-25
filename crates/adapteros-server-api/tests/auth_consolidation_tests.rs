//! Tests for auth consolidation
//!
//! These tests verify the centralized authentication patterns:
//! 1. Public path allowlist from adapteros-auth crate
//! 2. AuthCtx extractor helper methods
//! 3. Boot invariant validation for auth configuration

use adapteros_auth::{
    is_public_path, AuthConfig, AuthError, AuthMode as AuthModeNew, JwtAlgorithm,
    DEFAULT_ACCESS_TOKEN_TTL_SECS, JWT_ISSUER,
};

// =============================================================================
// Public Path Allowlist Tests
// =============================================================================

#[test]
fn test_public_paths_health_endpoints() {
    // Health endpoints must be public for Kubernetes probes
    assert!(is_public_path("/healthz"));
    assert!(is_public_path("/healthz/live"));
    assert!(is_public_path("/healthz/all"));
    assert!(is_public_path("/readyz"));
    assert!(is_public_path("/livez"));
    assert!(is_public_path("/version"));
}

#[test]
fn test_public_paths_auth_endpoints() {
    // Auth endpoints that must work without existing auth
    assert!(is_public_path("/v1/auth/login"));
    assert!(is_public_path("/v1/auth/register"));
    assert!(is_public_path("/v1/auth/refresh"));
    assert!(is_public_path("/v1/auth/config"));
    assert!(is_public_path("/v1/auth/health"));
    assert!(is_public_path("/v1/auth/bootstrap"));
}

#[test]
fn test_protected_paths_require_auth() {
    // These paths must require authentication
    assert!(
        !is_public_path("/v1/auth/me"),
        "/v1/auth/me should require auth"
    );
    assert!(
        !is_public_path("/v1/auth/logout"),
        "/v1/auth/logout should require auth"
    );
    assert!(
        !is_public_path("/v1/auth/sessions"),
        "/v1/auth/sessions should require auth"
    );
    assert!(
        !is_public_path("/v1/adapters"),
        "/v1/adapters should require auth"
    );
    assert!(!is_public_path("/v1/chat"), "/v1/chat should require auth");
    assert!(
        !is_public_path("/v1/tenants"),
        "/v1/tenants should require auth"
    );
    assert!(
        !is_public_path("/v1/training"),
        "/v1/training should require auth"
    );
    assert!(
        !is_public_path("/v1/models"),
        "/v1/models should require auth"
    );
    assert!(
        !is_public_path("/v1/workers"),
        "/v1/workers should require auth"
    );
}

#[test]
fn test_public_paths_metrics_and_status() {
    // Metrics and status endpoints
    assert!(is_public_path("/metrics"));
    assert!(is_public_path("/v1/metrics"));
    assert!(is_public_path("/v1/invariants"));
    assert!(is_public_path("/v1/meta"));
    assert!(is_public_path("/v1/status"));
    assert!(is_public_path("/system/ready"));
}

#[test]
fn test_public_paths_documentation() {
    // OpenAPI and Swagger UI
    assert!(is_public_path("/swagger-ui"));
    assert!(is_public_path("/swagger-ui/index.html"));
    assert!(is_public_path("/api-doc"));
    assert!(is_public_path("/api-docs"));
    assert!(is_public_path("/openapi.json"));
}

#[test]
fn test_public_paths_prefix_matching() {
    // Ensure prefix matching works correctly
    assert!(is_public_path("/healthz/"));
    assert!(is_public_path("/healthz/component"));
    assert!(is_public_path("/static/js/main.js"));
    assert!(is_public_path("/assets/images/logo.png"));

    // But partial prefixes should NOT match
    assert!(!is_public_path("/health")); // Not /healthz
    assert!(!is_public_path("/v1/auth")); // Not a complete public auth path
}

// =============================================================================
// AuthConfig Boot Invariant Tests
// =============================================================================

#[test]
fn test_auth_config_defaults() {
    let config = AuthConfig::default();
    assert_eq!(config.jwt.issuer, JWT_ISSUER);
    assert_eq!(config.jwt.algorithm, JwtAlgorithm::Hs256);
    assert_eq!(
        config.jwt.access_token_ttl_secs,
        DEFAULT_ACCESS_TOKEN_TTL_SECS
    );
    assert!(!config.dev_bypass_allowed);
    assert!(!config.dev_login_enabled);
}

#[test]
fn test_boot_invariant_dev_bypass_rejected_in_release() {
    let config = AuthConfig {
        dev_bypass_allowed: true,
        ..Default::default()
    };

    // Simulating release build (is_release = true)
    let result = config.validate_boot_invariants(true);
    assert!(result.is_err());

    if let Err(AuthError::DevBypassInRelease) = result {
        // Expected
    } else {
        panic!("Expected DevBypassInRelease error");
    }
}

#[test]
fn test_boot_invariant_dev_bypass_allowed_in_debug() {
    let mut config = AuthConfig {
        dev_bypass_allowed: true,
        ..Default::default()
    };
    // Need some key material for validation to pass
    config.jwt.hmac_secret = Some(b"test-secret-for-debug".to_vec());

    // Simulating debug build (is_release = false)
    let result = config.validate_boot_invariants(false);
    assert!(
        result.is_ok(),
        "Dev bypass should be allowed in debug builds"
    );
}

#[test]
fn test_boot_invariant_eddsa_requires_key() {
    let config = AuthConfig {
        jwt: adapteros_auth::JwtConfig {
            algorithm: JwtAlgorithm::EdDSA,
            ..Default::default()
        },
        ..Default::default()
    };
    // No key configured

    let result = config.validate_boot_invariants(true);
    assert!(result.is_err());

    if let Err(AuthError::JwtModeNotConfigured) = result {
        // Expected
    } else {
        panic!("Expected JwtModeNotConfigured error");
    }
}

#[test]
fn test_boot_invariant_eddsa_with_key_path() {
    let config = AuthConfig {
        jwt: adapteros_auth::JwtConfig {
            algorithm: JwtAlgorithm::EdDSA,
            ed25519_key_path: Some("/path/to/key".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    // Should pass if key path is configured (actual file loading happens elsewhere)
    let result = config.validate_boot_invariants(false);
    assert!(result.is_ok());
}

#[test]
fn test_boot_invariant_eddsa_with_public_key() {
    let config = AuthConfig {
        jwt: adapteros_auth::JwtConfig {
            algorithm: JwtAlgorithm::EdDSA,
            ed25519_public_key: Some("-----BEGIN PUBLIC KEY-----...".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    // Should pass if public key is configured
    let result = config.validate_boot_invariants(false);
    assert!(result.is_ok());
}

#[test]
fn test_boot_invariant_cookie_security() {
    let config = AuthConfig {
        jwt: adapteros_auth::JwtConfig {
            hmac_secret: Some(b"test-secret-at-least-32-bytes-long".to_vec()),
            ..Default::default()
        },
        cookie: adapteros_auth::CookieConfig {
            same_site: "None".to_string(),
            secure: false, // Invalid: SameSite=None requires Secure
            ..Default::default()
        },
        ..Default::default()
    };

    // Should fail in release mode
    let result = config.validate_boot_invariants(true);
    assert!(result.is_err());
}

// =============================================================================
// AuthMode Tests
// =============================================================================

#[test]
fn test_auth_mode_is_authenticated() {
    assert!(AuthModeNew::BearerToken.is_authenticated());
    assert!(AuthModeNew::Cookie.is_authenticated());
    assert!(AuthModeNew::ApiKey.is_authenticated());
    assert!(AuthModeNew::DevBypass.is_authenticated());
    assert!(!AuthModeNew::Unauthenticated.is_authenticated());
}

#[test]
fn test_auth_mode_is_dev_bypass() {
    assert!(!AuthModeNew::BearerToken.is_dev_bypass());
    assert!(!AuthModeNew::Cookie.is_dev_bypass());
    assert!(!AuthModeNew::ApiKey.is_dev_bypass());
    assert!(AuthModeNew::DevBypass.is_dev_bypass());
    assert!(!AuthModeNew::Unauthenticated.is_dev_bypass());
}

#[test]
fn test_auth_mode_uses_token() {
    assert!(AuthModeNew::BearerToken.uses_token());
    assert!(AuthModeNew::Cookie.uses_token());
    assert!(AuthModeNew::ApiKey.uses_token());
    assert!(!AuthModeNew::DevBypass.uses_token());
    assert!(!AuthModeNew::Unauthenticated.uses_token());
}

// =============================================================================
// AuthError Status Code Tests
// =============================================================================

#[test]
fn test_auth_error_status_codes() {
    use axum::http::StatusCode;

    // 401 Unauthorized errors
    assert_eq!(
        AuthError::TokenMissing.status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        AuthError::TokenExpired.status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        AuthError::InvalidCredentials.status_code(),
        StatusCode::UNAUTHORIZED
    );

    // 403 Forbidden errors
    assert_eq!(
        AuthError::PermissionDenied("test".into()).status_code(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        AuthError::TenantIsolation.status_code(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(AuthError::CsrfError.status_code(), StatusCode::FORBIDDEN);

    // 400 Bad Request errors
    assert_eq!(
        AuthError::MissingField("test".into()).status_code(),
        StatusCode::BAD_REQUEST
    );

    // 500 Internal errors
    assert_eq!(
        AuthError::DatabaseError("test".into()).status_code(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn test_auth_error_codes() {
    assert_eq!(AuthError::TokenMissing.error_code(), "TOKEN_MISSING");
    assert_eq!(AuthError::TokenExpired.error_code(), "TOKEN_EXPIRED");
    assert_eq!(
        AuthError::TenantIsolation.error_code(),
        "TENANT_ISOLATION_ERROR"
    );
    assert_eq!(AuthError::CsrfError.error_code(), "CSRF_ERROR");
}

#[test]
fn test_auth_error_is_boot_fatal() {
    // Boot-fatal errors
    assert!(AuthError::DevBypassInRelease.is_boot_fatal());
    assert!(AuthError::JwtModeNotConfigured.is_boot_fatal());
    assert!(AuthError::ApiKeyModeNotConfigured.is_boot_fatal());

    // Non-boot-fatal errors
    assert!(!AuthError::TokenMissing.is_boot_fatal());
    assert!(!AuthError::TokenExpired.is_boot_fatal());
    assert!(!AuthError::PermissionDenied("test".into()).is_boot_fatal());
}
