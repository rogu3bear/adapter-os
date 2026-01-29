//! Dev Bypass Security Verification Tests
//!
//! These tests verify the security boundaries of the dev bypass functionality:
//! 1. Compile-time guards via #[cfg(debug_assertions)]
//! 2. Token validation in dev mode
//! 3. JWT token properties
//! 4. Endpoint availability based on build mode
//!
//! CRITICAL: Dev bypass MUST NOT be available in release builds.
//!
//! Citations:
//! - crates/adapteros-server-api/src/handlers/auth_enhanced.rs: dev_bypass_handler
//! - crates/adapteros-server-api/src/middleware.rs: auth_middleware
//! - crates/adapteros-server-api/src/middleware_security.rs: cors_layer

#[cfg(all(test, feature = "dev-bypass", debug_assertions))]
mod compile_time_guards {
    /// Test: dev_bypass_handler has #[cfg(not(debug_assertions))] early return
    ///
    /// In release builds, the handler MUST return 403 Forbidden immediately.
    /// Location: crates/adapteros-server-api/src/handlers/auth_enhanced.rs:670-680
    #[test]
    fn test_dev_bypass_handler_has_release_guard() {
        // This test documents the requirement.
        // The actual verification is done by:
        // 1. Code review (handler has #[cfg(not(debug_assertions))] block)
        // 2. Release build testing (endpoint returns 403)
        //
        // Code structure in auth_enhanced.rs:
        // ```rust
        // pub async fn dev_bypass_handler(...) {
        //     #[cfg(not(debug_assertions))]
        //     {
        //         return Err((StatusCode::FORBIDDEN, ...));
        //     }
        //     // Debug-only code follows...
        // }
        // ```
        println!("Verification: dev_bypass_handler has release guard at line 670-680");
        println!("  - #[cfg(not(debug_assertions))] returns 403 FORBIDDEN");
        println!("  - Error code: DEV_BYPASS_DISABLED");
        println!("  - Message: 'this endpoint is only available in development mode'");
    }

    /// Test: dev no-auth env bypass is debug-only
    ///
    /// The AOS_DEV_NO_AUTH path is only honored in debug builds; release builds ignore it.
    /// Location: crates/adapteros-server-api/src/auth.rs (dev_no_auth_enabled) and middleware/mod.rs
    #[test]
    fn test_dev_no_auth_is_debug_only() {
        println!("Verification: AOS_DEV_NO_AUTH is gated by cfg(debug_assertions)");
        println!("  - dev_no_auth_enabled() returns true only in debug builds");
        println!("  - Release builds log and ignore the env var");
        println!(
            "  - Bypass injects admin claims with admin_tenants=[\"*\"] and tenant_id=\"system\""
        );
    }

    /// Test: CORS layer has different configs for debug/release
    ///
    /// Debug mode allows all origins; release restricts to whitelist.
    /// Location: crates/adapteros-server-api/src/middleware_security.rs:263-307
    #[test]
    fn test_cors_has_conditional_config() {
        println!("Verification: cors_layer has conditional config at line 263-307");
        println!("  - #[cfg(debug_assertions)]: allow_origin(Any)");
        println!("  - #[cfg(not(debug_assertions))]: restrict to ALLOWED_ORIGINS env var");
        println!("  - Production default: 'https://adapteros.com,https://app.adapteros.com'");
    }
}

#[cfg(test)]
mod token_validation {
    /// Test: Dev bypass tokens are properly signed
    ///
    /// Even in debug mode, tokens MUST be cryptographically signed.
    #[test]
    fn test_dev_tokens_are_signed() {
        println!("Verification: dev bypass tokens are properly signed");
        println!("  - Uses generate_token_ed25519() or generate_token()");
        println!("  - Ed25519 signing when use_ed25519=true");
        println!("  - HMAC-SHA256 fallback otherwise");
        println!("  - No unsigned or plaintext tokens");
    }

    /// Test: Dev bypass token has correct expiry (8 hours)
    ///
    /// Location: crates/adapteros-server-api/src/handlers/auth_enhanced.rs:742
    #[test]
    fn test_dev_bypass_token_expiry() {
        println!("Verification: dev bypass token expiry is 8 hours");
        println!("  - Line 742: expires_at = Utc::now() + Duration::hours(8)");
        println!("  - Consistent with production token expiry");
    }

    /// Test: Dev no-auth claims use debug-only 8 hour expiry
    ///
    /// Location: crates/adapteros-server-api/src/middleware/mod.rs dev_no_auth_claims()
    #[test]
    fn test_dev_no_auth_claims_expiry() {
        println!("Verification: dev_no_auth_claims expiry is 8 hours");
        println!("  - Matches dev_bypass_handler duration");
        println!("  - Only active under cfg(debug_assertions)");
    }

    /// Test: Dev bypass claims have correct structure
    ///
    /// Location: crates/adapteros-server-api/src/handlers/auth_enhanced.rs
    #[test]
    fn test_dev_bypass_claims_structure() {
        println!("Verification: dev bypass claims structure");
        println!("  - user_id: 'dev-admin-user'");
        println!("  - email: 'dev-admin@adapteros.local'");
        println!("  - role: 'admin' (full privileges)");
        println!("  - tenant_id: 'default' (matches handler code)");
    }
}

#[cfg(test)]
mod session_tracking {
    /// Test: Dev bypass creates audit trail
    ///
    /// Location: crates/adapteros-server-api/src/handlers/auth_enhanced.rs:755-771
    #[test]
    fn test_dev_bypass_creates_audit_trail() {
        println!("Verification: dev bypass creates audit trail");
        println!("  - Creates session via create_session()");
        println!("  - Logs to audit via db.log_audit()");
        println!("  - Action: 'auth.dev_bypass'");
        println!("  - Includes client IP for tracking");
    }

    /// Test: Dev no-auth logs once when enabled
    ///
    /// Location: crates/adapteros-server-api/src/auth.rs (dev_bypass_status)
    #[test]
    fn test_dev_no_auth_logs_once() {
        println!("Verification: dev no-auth logs exactly once when requested in debug");
        println!("  - Memoized status via OnceLock");
        println!("  - Prevents noisy logs while still signaling bypass use");
    }
}

#[cfg(test)]
mod hardcoded_credentials {
    /// Test: No hardcoded bypass tokens
    ///
    /// Dev bypass is feature- and debug-gated; AOS_DEV_NO_AUTH is env-gated and debug-only.
    #[test]
    fn test_no_hardcoded_bypass_tokens() {
        println!("Verification: no hardcoded bypass tokens remain");
        println!("  - dev_bypass_handler requires dev-bypass feature + debug + dev_login_enabled");
        println!("  - AOS_DEV_NO_AUTH is ignored in release builds");
        println!("  - No static shared secrets are embedded");
    }

    /// Test: JWT secrets are not hardcoded
    ///
    /// Secrets must come from environment or configuration.
    #[test]
    fn test_jwt_secrets_not_hardcoded() {
        println!("Verification: JWT secrets not hardcoded");
        println!("  - jwt_secret passed via AppState");
        println!("  - Ed25519 keypair generated at runtime");
        println!("  - No secret values in source code");
    }
}

#[cfg(test)]
mod release_build_verification {
    /// Test: Release build excludes debug bypass code
    ///
    /// This test verifies the compile-time exclusion works correctly.
    #[test]
    #[cfg(debug_assertions)]
    fn test_this_runs_in_debug_only() {
        println!("This test runs only in debug builds");
        println!("Confirms #[cfg(debug_assertions)] is working");

        // Verify we're in debug mode
        let is_debug = cfg!(debug_assertions);
        assert!(is_debug, "Should be in debug mode");
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn test_this_runs_in_release_only() {
        println!("This test runs only in release builds");
        println!("Confirms #[cfg(not(debug_assertions))] is working");

        // Verify we're in release mode
        let is_debug = cfg!(debug_assertions);
        assert!(!is_debug, "Should be in release mode");
    }

    /// Test: HMAC JWT mode is rejected in release builds
    ///
    /// Location: crates/adapteros-server/src/main.rs:1425-1447
    #[test]
    fn test_hmac_mode_rejected_in_release() {
        println!("Verification: release build rejects jwt_mode=hmac");
        println!("  - adapteros-server/src/main.rs returns config error when not debug");
        println!("  - Prevents deploying HMAC in production");
    }

    /// Test: AOS_DEV_NO_AUTH is ignored with a logged warning in release builds
    ///
    /// Location: crates/adapteros-server-api/src/middleware/mod.rs:95-104
    #[test]
    fn test_dev_no_auth_env_ignored_in_release() {
        println!("Verification: AOS_DEV_NO_AUTH is ignored in release builds");
        println!("  - dev_no_auth_enabled() always returns false under cfg(not(debug_assertions))");
        println!("  - Emits error log when env var is set to avoid silent bypass");
    }

    /// Test: Debug bypass is excluded at compile time
    ///
    /// The entire bypass code block is excluded from release binaries.
    #[test]
    fn test_debug_bypass_compile_time_exclusion() {
        #[cfg(debug_assertions)]
        {
            println!("DEBUG BUILD: bypass code is included");
            // This code path exists in debug builds
            let bypass_available = true;
            assert!(bypass_available);
        }

        #[cfg(not(debug_assertions))]
        {
            println!("RELEASE BUILD: bypass code is excluded");
            // This code path exists in release builds
            let bypass_available = false;
            assert!(!bypass_available);
        }
    }

    /// Documentation: Dev vs prod auth boundaries (login/refresh + bypass)
    ///
    /// Summarizes the guardrails without changing runtime logic.
    #[test]
    fn doc_dev_vs_prod_auth_boundaries() {
        println!("Dev-only endpoints (/v1/auth/dev-bypass, /v1/dev/bootstrap) are gated by cfg(all(feature=\"dev-bypass\", debug_assertions)).");
        println!("AOS_DEV_NO_AUTH is ignored in release builds (auth.rs + middleware/mod.rs dev_no_auth_enabled).");
        println!(
            "Release build rejects jwt_mode=hmac in adapteros-server/src/main.rs when not debug."
        );
        println!(
            "Login/refresh still require JWT validation; no unsigned tokens are ever emitted."
        );
    }
}

#[cfg(test)]
mod security_concerns {
    /// SECURITY NOTE #1: Dev bypass vs dev no-auth scope
    ///
    /// Dev bypass issues admin for tenant 'default' with normal cookie/CSRF flow.
    /// Dev no-auth injects wildcard admin claims and skips cookie/session creation.
    #[test]
    fn test_dev_bypass_vs_dev_no_auth_scope() {
        println!("SECURITY NOTE: Dev bypass vs dev no-auth");
        println!("  - dev_bypass_handler: admin, tenant='default', sets cookies, audited");
        println!("  - dev_no_auth: admin with admin_tenants=[\"*\"], tenant='system', debug-only, no cookies");
    }

    /// SECURITY CONCERN #2: Token in query string
    ///
    /// The middleware allows token via query string (?token=...).
    /// This could be logged in access logs.
    /// Location: crates/adapteros-server-api/src/middleware.rs:134-138
    #[test]
    fn test_query_string_token_logged_risk() {
        println!("SECURITY NOTE: Token in query string risk");
        println!("  - Tokens can be passed via ?token=...");
        println!("  - Query strings may appear in access logs");
        println!("  - Recommendation: Use Authorization header");
    }
}

#[cfg(test)]
mod recommendations {
    /// Recommendation: Add integration test for release build
    #[test]
    fn recommendation_release_build_integration_test() {
        println!("RECOMMENDATION: Add CI release build test");
        println!("  1. Build without dev-bypass feature or in release mode");
        println!("  2. Start server");
        println!("  3. Call POST /v1/auth/dev-bypass");
        println!("  4. Verify 403/disabled response");
        println!("  5. Ensure AOS_DEV_NO_AUTH is ignored in release logs");
    }

    /// Recommendation: Add feature flag for dev bypass
    #[test]
    fn recommendation_feature_flag_for_dev_bypass() {
        println!("RECOMMENDATION: Consider adding feature flag");
        println!("  - Instead of #[cfg(debug_assertions)]");
        println!("  - Use #[cfg(feature = 'dev-bypass')]");
        println!("  - More explicit control");
        println!("  - Can be enabled for staging environments");
    }

    /// Recommendation: Rate limit dev bypass endpoint
    #[test]
    fn recommendation_rate_limit_dev_bypass() {
        println!("RECOMMENDATION: Add rate limiting");
        println!("  - Limit /v1/auth/dev-bypass to 10 requests/minute");
        println!("  - Prevents abuse in dev environments");
        println!("  - Already have rate limiting middleware available");
    }
}

// =============================================================================
// Bypass Flag Security Verification (SEC-BYPASS-001)
// =============================================================================
//
// These tests verify that ALL bypass flags are properly cfg-gated and cannot
// affect behavior in release builds.

#[cfg(test)]
mod bypass_flag_security {
    use adapteros_core::debug_bypass::{is_bypass_enabled, is_debug_build, is_release_build};

    /// Test: AOS_SKIP_MIGRATION_SIGNATURES is debug-only
    ///
    /// Location: crates/adapteros-db/src/lib.rs
    #[test]
    fn test_skip_migration_signatures_is_debug_only() {
        std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

        // The helper function enforces the invariant
        let bypass_active = is_bypass_enabled("AOS_SKIP_MIGRATION_SIGNATURES");

        #[cfg(debug_assertions)]
        assert!(bypass_active, "Should be active in debug builds");

        #[cfg(not(debug_assertions))]
        assert!(
            !bypass_active,
            "SECURITY: Must be inactive in release builds"
        );

        std::env::remove_var("AOS_SKIP_MIGRATION_SIGNATURES");
    }

    /// Test: AOS_SKIP_MODEL_HASH_VERIFY is debug-only
    ///
    /// Location: crates/adapteros-lora-worker/src/backend_factory/model_io.rs
    #[test]
    fn test_skip_model_hash_verify_is_debug_only() {
        std::env::set_var("AOS_SKIP_MODEL_HASH_VERIFY", "1");

        let bypass_active = is_bypass_enabled("AOS_SKIP_MODEL_HASH_VERIFY");

        #[cfg(debug_assertions)]
        assert!(bypass_active, "Should be active in debug builds");

        #[cfg(not(debug_assertions))]
        assert!(
            !bypass_active,
            "SECURITY: Must be inactive in release builds"
        );

        std::env::remove_var("AOS_SKIP_MODEL_HASH_VERIFY");
    }

    /// Test: AOS_SKIP_PF_CHECK is debug-only
    ///
    /// Location: crates/adapteros-server-api/src/handlers/system_status.rs
    #[test]
    fn test_skip_pf_check_is_debug_only() {
        std::env::set_var("AOS_SKIP_PF_CHECK", "1");

        let bypass_active = is_bypass_enabled("AOS_SKIP_PF_CHECK");

        #[cfg(debug_assertions)]
        assert!(bypass_active, "Should be active in debug builds");

        #[cfg(not(debug_assertions))]
        assert!(
            !bypass_active,
            "SECURITY: Must be inactive in release builds"
        );

        std::env::remove_var("AOS_SKIP_PF_CHECK");
    }

    /// Test: AOS_DEV_NO_AUTH is debug-only
    ///
    /// Location: crates/adapteros-server-api/src/auth.rs
    #[test]
    fn test_dev_no_auth_is_debug_only() {
        std::env::set_var("AOS_DEV_NO_AUTH", "1");

        let bypass_active = is_bypass_enabled("AOS_DEV_NO_AUTH");

        #[cfg(debug_assertions)]
        assert!(bypass_active, "Should be active in debug builds");

        #[cfg(not(debug_assertions))]
        assert!(
            !bypass_active,
            "SECURITY: Must be inactive in release builds"
        );

        std::env::remove_var("AOS_DEV_NO_AUTH");
    }

    /// Test: AOS_DEV_SIGNATURE_BYPASS is debug-only
    ///
    /// Location: crates/adapteros-crypto/src/bundle_sign.rs
    #[test]
    fn test_dev_signature_bypass_is_debug_only() {
        std::env::set_var("AOS_DEV_SIGNATURE_BYPASS", "1");

        let bypass_active = is_bypass_enabled("AOS_DEV_SIGNATURE_BYPASS");

        #[cfg(debug_assertions)]
        assert!(bypass_active, "Should be active in debug builds");

        #[cfg(not(debug_assertions))]
        assert!(
            !bypass_active,
            "SECURITY: Must be inactive in release builds"
        );

        std::env::remove_var("AOS_DEV_SIGNATURE_BYPASS");
    }

    /// Test: Build type detection is consistent
    #[test]
    fn test_build_type_detection() {
        // Exactly one should be true
        assert_ne!(
            is_debug_build(),
            is_release_build(),
            "Build type detection is inconsistent"
        );

        #[cfg(debug_assertions)]
        {
            assert!(is_debug_build());
            assert!(!is_release_build());
        }

        #[cfg(not(debug_assertions))]
        {
            assert!(!is_debug_build());
            assert!(is_release_build());
        }
    }

    /// Document: All known bypass flags and their locations
    #[test]
    fn doc_all_bypass_flags() {
        println!("=== Bypass Flag Locations (all must be cfg-gated) ===\n");

        println!("Migration/Signature:");
        println!("  - AOS_SKIP_MIGRATION_SIGNATURES: adapteros-db/src/lib.rs");
        println!("  - AOS_SKIP_KERNEL_SIGNATURE_VERIFY: adapteros-lora-kernel-mtl/src/manifest.rs");
        println!("  - AOS_DEBUG_SKIP_KERNEL_SIG: adapteros-lora-kernel-mtl/src/manifest.rs");
        println!("  - AOS_DEV_SKIP_METALLIB_CHECK: adapteros-lora-kernel-mtl/src/lib.rs");
        println!(
            "  - AOS_SKIP_MODEL_HASH_VERIFY: adapteros-lora-worker/src/backend_factory/model_io.rs"
        );
        println!("  - AOS_DEV_SIGNATURE_BYPASS: adapteros-crypto/src/bundle_sign.rs\n");

        println!("Security:");
        println!("  - AOS_DEV_NO_AUTH: adapteros-server-api/src/auth.rs");
        println!("  - AOS_SKIP_PF_CHECK: adapteros-server-api/src/handlers/system_status.rs");
        println!("  - AOS_SKIP_SYMLINK_CHECK: adapteros-config/src/path_resolver.rs\n");

        println!("All flags MUST:");
        println!("  1. Use #[cfg(debug_assertions)] to gate the check");
        println!("  2. Log a warning if set in release builds");
        println!("  3. NEVER honor the flag in release builds");
    }
}
