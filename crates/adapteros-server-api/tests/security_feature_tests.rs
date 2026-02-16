//! Comprehensive Security Feature Tests
//!
//! Tests for security subsystem features:
//! 1. JWT validation edge cases
//! 2. Token revocation
//! 3. Rate limiting
//! 4. IP access control
//! 5. Path traversal prevention
//! 6. Proper 401/403 response codes
//!
//! These tests validate the security controls in:
//! - crates/adapteros-server-api/src/security/mod.rs
//! - crates/adapteros-server-api/src/security/token_revocation.rs
//! - crates/adapteros-server-api/src/security/rate_limiting.rs
//! - crates/adapteros-server-api/src/security/ip_access_control.rs
//! - crates/adapteros-server-api/src/auth.rs

use adapteros_crypto::Keypair;
use adapteros_db::Db;
use adapteros_server_api::auth::{
    derive_kid_from_str, encode_ed25519_public_key_pem, generate_token_ed25519,
    validate_token_ed25519, AuthMode, Claims, PrincipalType, JWT_ISSUER,
};
use adapteros_server_api::security::{
    add_ip_rule, check_ip_access, check_login_lockout, check_rate_limit, cleanup_expired_ip_rules,
    cleanup_expired_revocations, create_session, is_account_locked, is_token_revoked,
    remove_ip_rule, reset_rate_limit, revoke_all_user_tokens, revoke_token, track_auth_attempt,
    update_rate_limit, validate_tenant_isolation, AccessDecision,
};
use chrono::{Duration, Utc};

async fn init_test_db() -> Db {
    let db = Db::connect("sqlite::memory:?cache=shared")
        .await
        .expect("Failed to create test database");

    // Create required tables
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS revoked_tokens (
            jti TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            tenant_id TEXT NOT NULL,
            revoked_at TEXT NOT NULL DEFAULT (datetime('now')),
            revoked_by TEXT,
            reason TEXT,
            expires_at TEXT NOT NULL
        )",
    )
    .execute(db.pool())
    .await
    .expect("Failed to create revoked_tokens table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS rate_limit_buckets (
            tenant_id TEXT PRIMARY KEY,
            requests_count INTEGER NOT NULL DEFAULT 0,
            window_start TEXT NOT NULL DEFAULT (datetime('now')),
            window_size_seconds INTEGER NOT NULL DEFAULT 60,
            max_requests INTEGER NOT NULL DEFAULT 1000,
            last_updated TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(db.pool())
    .await
    .expect("Failed to create rate_limit_buckets table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS ip_access_control (
            id TEXT PRIMARY KEY,
            ip_address TEXT NOT NULL,
            ip_range TEXT,
            list_type TEXT NOT NULL CHECK(list_type IN ('allow', 'deny')),
            tenant_id TEXT,
            active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            created_by TEXT NOT NULL,
            expires_at TEXT,
            reason TEXT
        )",
    )
    .execute(db.pool())
    .await
    .expect("Failed to create ip_access_control table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS auth_attempts (
            id TEXT PRIMARY KEY,
            email TEXT NOT NULL,
            ip_address TEXT NOT NULL,
            success INTEGER NOT NULL,
            attempted_at TEXT NOT NULL DEFAULT (datetime('now')),
            failure_reason TEXT
        )",
    )
    .execute(db.pool())
    .await
    .expect("Failed to create auth_attempts table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            email TEXT UNIQUE NOT NULL,
            display_name TEXT NOT NULL,
            pw_hash TEXT NOT NULL,
            role TEXT NOT NULL,
            disabled INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            tenant_id TEXT DEFAULT 'default',
            failed_attempts INTEGER NOT NULL DEFAULT 0,
            last_failed_at TEXT,
            lockout_until TEXT,
            token_rotated_at TEXT
        )",
    )
    .execute(db.pool())
    .await
    .expect("Failed to create users table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS auth_sessions (
            jti TEXT PRIMARY KEY,
            session_id TEXT,
            user_id TEXT NOT NULL,
            tenant_id TEXT NOT NULL,
            device_id TEXT,
            rot_id TEXT,
            refresh_hash TEXT,
            refresh_expires_at TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            expires_at TEXT NOT NULL,
            ip_address TEXT,
            user_agent TEXT,
            last_activity TEXT NOT NULL DEFAULT (datetime('now')),
            locked INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(db.pool())
    .await
    .expect("Failed to create user_sessions table");

    db
}

#[cfg(test)]
mod jwt_validation_edge_cases {
    use super::*;

    #[test]
    fn test_malformed_token_rejected() {
        let keypair = Keypair::generate();
        let malformed_token = "not.a.valid.jwt.token";

        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);
        let result =
            validate_token_ed25519(malformed_token, &[(kid, public_pem.clone())], &public_pem);

        assert!(result.is_err(), "Malformed token should be rejected");
    }

    #[test]
    fn test_empty_token_rejected() {
        let keypair = Keypair::generate();
        let empty_token = "";

        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);
        let result = validate_token_ed25519(empty_token, &[(kid, public_pem.clone())], &public_pem);

        assert!(result.is_err(), "Empty token should be rejected");
    }

    #[test]
    fn test_wrong_signature_rejected() {
        let keypair_signer = Keypair::generate();
        let keypair_validator = Keypair::generate();

        // Generate token with one keypair
        let token = generate_token_ed25519(
            "user-4",
            "user4@example.com",
            "operator",
            "tenant-a",
            &keypair_signer,
            3600,
        )
        .expect("Failed to generate token");

        // Try to validate with different keypair
        let public_pem = encode_ed25519_public_key_pem(&keypair_validator.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);
        let result = validate_token_ed25519(&token, &[(kid, public_pem.clone())], &public_pem);

        assert!(
            result.is_err(),
            "Token with wrong signature should be rejected"
        );
    }

    #[test]
    fn test_valid_token_accepted() {
        let keypair = Keypair::generate();

        // Generate a valid token
        let token = generate_token_ed25519(
            "user-5",
            "user5@example.com",
            "operator",
            "tenant-a",
            &keypair,
            3600,
        )
        .expect("Failed to generate token");

        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);
        let result = validate_token_ed25519(&token, &[(kid, public_pem.clone())], &public_pem);

        assert!(result.is_ok(), "Valid token should be accepted");
        let claims = result.unwrap();
        assert_eq!(claims.sub, "user-5");
        assert_eq!(claims.email, "user5@example.com");
        assert_eq!(claims.tenant_id, "tenant-a");
    }

    #[test]
    fn test_token_validation_rejects_tampering() {
        let keypair = Keypair::generate();

        let token = generate_token_ed25519(
            "user-6",
            "user6@example.com",
            "operator",
            "tenant-a",
            &keypair,
            3600,
        )
        .expect("Failed to generate token");

        // Tamper with the token by modifying a character in the middle
        let mut tampered = token.clone();
        let bytes = unsafe { tampered.as_bytes_mut() };
        if let Some(byte) = bytes.get_mut(50) {
            *byte = if *byte == b'A' { b'B' } else { b'A' };
        }

        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);
        let result = validate_token_ed25519(&tampered, &[(kid, public_pem.clone())], &public_pem);

        assert!(result.is_err(), "Tampered token should be rejected");
    }
}

#[cfg(test)]
mod token_revocation_tests {
    use super::*;

    #[tokio::test]
    async fn test_token_revocation() {
        let db = init_test_db().await;
        let jti = "test-jti-revoke";
        let user_id = "user-revoke-1";
        let tenant_id = "tenant-revoke";
        let expires_at = (Utc::now() + Duration::hours(8)).to_rfc3339();

        // Initially not revoked
        let is_revoked = is_token_revoked(&db, jti)
            .await
            .expect("Failed to check token revocation");
        assert!(!is_revoked, "Token should not be revoked initially");

        // Revoke token
        revoke_token(
            &db,
            jti,
            user_id,
            tenant_id,
            &expires_at,
            Some("admin"),
            Some("logout"),
        )
        .await
        .expect("Failed to revoke token");

        // Now revoked
        let is_revoked = is_token_revoked(&db, jti)
            .await
            .expect("Failed to check token revocation after revoke");
        assert!(is_revoked, "Token should be revoked");
    }

    #[tokio::test]
    async fn test_revoke_all_user_tokens() {
        let db = init_test_db().await;
        let user_id = "user-revoke-all";
        let tenant_id = "tenant-revoke-all";

        // Seed the user so token rotation updates succeed
        sqlx::query(
            "INSERT OR IGNORE INTO users (id, email, display_name, pw_hash, role, tenant_id)
             VALUES (?, ?, ?, 'test-hash', 'admin', ?)",
        )
        .bind(user_id)
        .bind("user-revoke-all@example.com")
        .bind("Revoke All User")
        .bind(tenant_id)
        .execute(db.pool())
        .await
        .expect("Failed to insert test user");

        // Create multiple sessions
        for i in 1..=3 {
            let jti = format!("jti-revoke-all-{}", i);
            let expires_at = (Utc::now() + Duration::hours(8)).timestamp();

            create_session(
                &db,
                &jti,
                user_id,
                tenant_id,
                expires_at,
                Some("192.168.1.1"),
                Some("test-agent"),
            )
            .await
            .expect("Failed to create session");
        }

        // Revoke all user tokens
        let count = revoke_all_user_tokens(&db, user_id, tenant_id, "admin", "security incident")
            .await
            .expect("Failed to revoke all user tokens");

        assert_eq!(count, 3, "Should revoke all 3 tokens");

        // Verify all are revoked
        for i in 1..=3 {
            let jti = format!("jti-revoke-all-{}", i);
            let is_revoked = is_token_revoked(&db, &jti)
                .await
                .expect("Failed to check token");
            assert!(is_revoked, "Token {} should be revoked", i);
        }
    }

    #[tokio::test]
    async fn test_duplicate_revocation_idempotent() {
        let db = init_test_db().await;
        let jti = "test-jti-duplicate";
        let user_id = "user-dup";
        let tenant_id = "tenant-dup";
        let expires_at = (Utc::now() + Duration::hours(8)).to_rfc3339();

        // Revoke once
        revoke_token(
            &db,
            jti,
            user_id,
            tenant_id,
            &expires_at,
            Some("admin"),
            Some("logout"),
        )
        .await
        .expect("First revocation failed");

        // Revoke again (should be idempotent due to ON CONFLICT DO NOTHING)
        let result = revoke_token(
            &db,
            jti,
            user_id,
            tenant_id,
            &expires_at,
            Some("admin"),
            Some("logout"),
        )
        .await;
        assert!(result.is_ok(), "Duplicate revocation should be idempotent");

        // Still revoked
        let is_revoked = is_token_revoked(&db, jti)
            .await
            .expect("Failed to check token");
        assert!(is_revoked);
    }

    #[tokio::test]
    async fn test_cleanup_expired_revocations() {
        let db = init_test_db().await;
        let past_expiry = (Utc::now() - Duration::hours(1)).to_rfc3339();
        let future_expiry = (Utc::now() + Duration::hours(1)).to_rfc3339();

        // Add expired revocation
        revoke_token(
            &db,
            "expired-jti",
            "user-1",
            "tenant-a",
            &past_expiry,
            None,
            Some("test"),
        )
        .await
        .expect("Failed to add expired revocation");

        // Add valid revocation
        revoke_token(
            &db,
            "valid-jti",
            "user-2",
            "tenant-a",
            &future_expiry,
            None,
            Some("test"),
        )
        .await
        .expect("Failed to add valid revocation");

        // Cleanup
        let count = cleanup_expired_revocations(&db)
            .await
            .expect("Failed to cleanup");
        assert_eq!(count, 1, "Should clean up 1 expired revocation");

        // Verify expired is gone
        let is_revoked = is_token_revoked(&db, "expired-jti")
            .await
            .expect("Failed to check expired token");
        assert!(!is_revoked, "Expired revocation should be cleaned up");

        // Verify valid still exists
        let is_revoked = is_token_revoked(&db, "valid-jti")
            .await
            .expect("Failed to check valid token");
        assert!(is_revoked, "Valid revocation should still exist");
    }
}

#[cfg(test)]
mod rate_limiting_tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limit_basic() {
        let db = init_test_db().await;

        // First request should succeed
        let result = check_rate_limit(&db, "tenant-rate-1")
            .await
            .expect("Failed to check rate limit");
        assert!(result.allowed, "First request should be allowed");
        assert_eq!(result.current_count, 1);
    }

    #[tokio::test]
    async fn test_rate_limit_exceeded() {
        let db = init_test_db().await;
        let tenant_id = "tenant-rate-exceed";

        // Set low limit
        update_rate_limit(&db, tenant_id, 3)
            .await
            .expect("Failed to set rate limit");

        // Make requests up to limit
        for i in 1..=3 {
            let result = check_rate_limit(&db, tenant_id)
                .await
                .expect("Failed to check rate limit");
            assert!(result.allowed, "Request {} should be allowed", i);
            assert_eq!(result.current_count, i as i64);
        }

        // Exceed limit
        let result = check_rate_limit(&db, tenant_id)
            .await
            .expect("Failed to check rate limit");
        assert!(!result.allowed, "Request should be denied after limit");
        assert_eq!(result.current_count, 4);
        assert_eq!(result.limit, 3);
    }

    #[tokio::test]
    async fn test_rate_limit_reset() {
        let db = init_test_db().await;
        let tenant_id = "tenant-rate-reset";

        update_rate_limit(&db, tenant_id, 5)
            .await
            .expect("Failed to set rate limit");

        // Make some requests
        for _ in 0..3 {
            check_rate_limit(&db, tenant_id)
                .await
                .expect("Failed to check rate limit");
        }

        // Reset
        reset_rate_limit(&db, tenant_id)
            .await
            .expect("Failed to reset rate limit");

        // Next request should start from 1
        let result = check_rate_limit(&db, tenant_id)
            .await
            .expect("Failed to check rate limit after reset");
        assert!(result.allowed);
        assert_eq!(result.current_count, 1, "Count should reset to 1");
    }

    #[tokio::test]
    async fn test_rate_limit_per_tenant_isolation() {
        let db = init_test_db().await;

        update_rate_limit(&db, "tenant-a", 2)
            .await
            .expect("Failed to set limit for tenant-a");
        update_rate_limit(&db, "tenant-b", 2)
            .await
            .expect("Failed to set limit for tenant-b");

        // Tenant A uses its quota
        check_rate_limit(&db, "tenant-a").await.expect("Failed");
        check_rate_limit(&db, "tenant-a").await.expect("Failed");

        // Tenant B should still have quota
        let result = check_rate_limit(&db, "tenant-b")
            .await
            .expect("Failed to check rate limit for tenant-b");
        assert!(result.allowed, "Tenant B should have independent quota");
        assert_eq!(result.current_count, 1);
    }
}

#[cfg(test)]
mod ip_access_control_tests {
    use super::*;

    #[tokio::test]
    async fn test_ip_denylist() {
        let db = init_test_db().await;

        // Add to denylist
        add_ip_rule(
            &db,
            "192.168.1.100",
            None,
            "deny",
            Some("tenant-a"),
            "admin",
            Some("suspicious activity"),
            None,
        )
        .await
        .expect("Failed to add IP rule");

        // Check denied
        let decision = check_ip_access(&db, "192.168.1.100", Some("tenant-a"))
            .await
            .expect("Failed to check IP access");
        assert_eq!(decision, AccessDecision::Deny);

        // Different IP allowed
        let decision = check_ip_access(&db, "192.168.1.101", Some("tenant-a"))
            .await
            .expect("Failed to check IP access");
        assert_eq!(decision, AccessDecision::Allow);
    }

    #[tokio::test]
    async fn test_ip_allowlist() {
        let db = init_test_db().await;

        // Add to allowlist
        add_ip_rule(
            &db,
            "10.0.0.1",
            None,
            "allow",
            Some("tenant-b"),
            "admin",
            Some("corporate office"),
            None,
        )
        .await
        .expect("Failed to add IP rule");

        // Allowlisted IP allowed
        let decision = check_ip_access(&db, "10.0.0.1", Some("tenant-b"))
            .await
            .expect("Failed to check IP access");
        assert_eq!(decision, AccessDecision::Allow);

        // Non-allowlisted IP denied when allowlist exists
        let decision = check_ip_access(&db, "10.0.0.2", Some("tenant-b"))
            .await
            .expect("Failed to check IP access");
        assert_eq!(decision, AccessDecision::Deny);
    }

    #[tokio::test]
    async fn test_denylist_overrides_allowlist() {
        let db = init_test_db().await;
        let ip = "10.0.0.5";

        // Add to allowlist
        add_ip_rule(
            &db,
            ip,
            None,
            "allow",
            Some("tenant-c"),
            "admin",
            Some("office"),
            None,
        )
        .await
        .expect("Failed to add to allowlist");

        // Also add to denylist
        add_ip_rule(
            &db,
            ip,
            None,
            "deny",
            Some("tenant-c"),
            "admin",
            Some("blocked"),
            None,
        )
        .await
        .expect("Failed to add to denylist");

        // Denylist should win
        let decision = check_ip_access(&db, ip, Some("tenant-c"))
            .await
            .expect("Failed to check IP access");
        assert_eq!(
            decision,
            AccessDecision::Deny,
            "Denylist should override allowlist"
        );
    }

    #[tokio::test]
    async fn test_remove_ip_rule() {
        let db = init_test_db().await;

        // Add rule
        let rule_id = add_ip_rule(
            &db,
            "192.168.1.200",
            None,
            "deny",
            Some("tenant-d"),
            "admin",
            Some("test"),
            None,
        )
        .await
        .expect("Failed to add IP rule");

        // Verify denied
        let decision = check_ip_access(&db, "192.168.1.200", Some("tenant-d"))
            .await
            .expect("Failed to check IP access");
        assert_eq!(decision, AccessDecision::Deny);

        // Remove rule
        remove_ip_rule(&db, &rule_id)
            .await
            .expect("Failed to remove IP rule");

        // Now allowed
        let decision = check_ip_access(&db, "192.168.1.200", Some("tenant-d"))
            .await
            .expect("Failed to check IP access after removal");
        assert_eq!(decision, AccessDecision::Allow);
    }

    #[tokio::test]
    async fn test_cleanup_expired_ip_rules() {
        let db = init_test_db().await;
        let past_expiry = (Utc::now() - Duration::hours(1)).to_rfc3339();
        let future_expiry = (Utc::now() + Duration::hours(1)).to_rfc3339();

        // Add expired rule
        add_ip_rule(
            &db,
            "192.168.2.1",
            None,
            "deny",
            Some("tenant-e"),
            "admin",
            Some("expired"),
            Some(&past_expiry),
        )
        .await
        .expect("Failed to add expired rule");

        // Add valid rule
        add_ip_rule(
            &db,
            "192.168.2.2",
            None,
            "deny",
            Some("tenant-e"),
            "admin",
            Some("valid"),
            Some(&future_expiry),
        )
        .await
        .expect("Failed to add valid rule");

        // Cleanup
        let count = cleanup_expired_ip_rules(&db)
            .await
            .expect("Failed to cleanup");
        assert_eq!(count, 1, "Should cleanup 1 expired rule");

        // Verify expired rule is gone
        let decision = check_ip_access(&db, "192.168.2.1", Some("tenant-e"))
            .await
            .expect("Failed to check IP");
        assert_eq!(
            decision,
            AccessDecision::Allow,
            "Expired rule should be removed"
        );

        // Verify valid rule still active
        let decision = check_ip_access(&db, "192.168.2.2", Some("tenant-e"))
            .await
            .expect("Failed to check IP");
        assert_eq!(
            decision,
            AccessDecision::Deny,
            "Valid rule should still be active"
        );
    }
}

#[cfg(test)]
mod auth_attempt_tracking_tests {
    use super::*;

    #[tokio::test]
    async fn test_track_successful_login() {
        let db = init_test_db().await;

        track_auth_attempt(&db, "user@example.com", "192.168.1.1", true, None)
            .await
            .expect("Failed to track auth attempt");

        let locked = is_account_locked(&db, "user@example.com", "192.168.1.1")
            .await
            .expect("Failed to check lock status");
        assert!(
            !locked,
            "Account should not be locked after successful login"
        );
    }

    #[tokio::test]
    async fn test_account_lockout_after_failed_attempts() {
        let db = init_test_db().await;

        // Insert user first
        sqlx::query(
            "INSERT INTO users (id, email, display_name, pw_hash, role, tenant_id)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("user-lockout")
        .bind("lockout@example.com")
        .bind("Test User")
        .bind("hash")
        .bind("operator")
        .bind("default")
        .execute(db.pool())
        .await
        .expect("Failed to insert user");

        // Make 5 failed attempts (lockout threshold)
        for _ in 0..5 {
            track_auth_attempt(
                &db,
                "lockout@example.com",
                "192.168.1.2",
                false,
                Some("invalid password"),
            )
            .await
            .expect("Failed to track auth attempt");
        }

        // Check if locked
        let locked = is_account_locked(&db, "lockout@example.com", "192.168.1.2")
            .await
            .expect("Failed to check lock status");
        assert!(locked, "Account should be locked after 5 failed attempts");

        // Verify lockout details
        let lockout_state = check_login_lockout(&db, "lockout@example.com", "192.168.1.2")
            .await
            .expect("Failed to check lockout state")
            .expect("Lockout state should exist");

        assert!(
            lockout_state.until > Utc::now(),
            "Lockout should be in effect"
        );
    }

    #[tokio::test]
    async fn test_successful_login_resets_failed_attempts() {
        let db = init_test_db().await;

        // Insert user
        sqlx::query(
            "INSERT INTO users (id, email, display_name, pw_hash, role, tenant_id)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("user-reset")
        .bind("reset@example.com")
        .bind("Test User")
        .bind("hash")
        .bind("operator")
        .bind("default")
        .execute(db.pool())
        .await
        .expect("Failed to insert user");

        // Make 3 failed attempts
        for _ in 0..3 {
            track_auth_attempt(
                &db,
                "reset@example.com",
                "192.168.1.3",
                false,
                Some("bad password"),
            )
            .await
            .expect("Failed to track failed attempt");
        }

        // Successful login
        track_auth_attempt(&db, "reset@example.com", "192.168.1.3", true, None)
            .await
            .expect("Failed to track successful attempt");

        // Account should not be locked
        let locked = is_account_locked(&db, "reset@example.com", "192.168.1.3")
            .await
            .expect("Failed to check lock status");
        assert!(
            !locked,
            "Account should not be locked after successful login"
        );
    }
}

#[cfg(test)]
mod tenant_isolation_validation_tests {
    use super::*;

    #[test]
    fn test_same_tenant_allowed() {
        let claims = Claims {
            sub: "user-1".to_string(),
            email: "user@tenant-a.com".to_string(),
            role: "operator".to_string(),
            roles: vec!["operator".to_string()],
            tenant_id: "tenant-a".to_string(),
            admin_tenants: vec![],
            device_id: None,
            session_id: Some("sess-1".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: (Utc::now() + Duration::hours(1)).timestamp(),
            iat: Utc::now().timestamp(),
            jti: "jti-1".to_string(),
            nbf: Utc::now().timestamp(),
            iss: JWT_ISSUER.to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        };

        let result = validate_tenant_isolation(&claims, "tenant-a");
        assert!(result.is_ok(), "Same tenant access should be allowed");
    }

    #[test]
    fn test_different_tenant_denied() {
        let claims = Claims {
            sub: "user-2".to_string(),
            email: "user@tenant-a.com".to_string(),
            role: "operator".to_string(),
            roles: vec!["operator".to_string()],
            tenant_id: "tenant-a".to_string(),
            admin_tenants: vec![],
            device_id: None,
            session_id: Some("sess-2".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: (Utc::now() + Duration::hours(1)).timestamp(),
            iat: Utc::now().timestamp(),
            jti: "jti-2".to_string(),
            nbf: Utc::now().timestamp(),
            iss: JWT_ISSUER.to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        };

        let result = validate_tenant_isolation(&claims, "tenant-b");
        assert!(result.is_err(), "Different tenant access should be denied");

        // Verify it's a 403 Forbidden
        if let Err(err) = result {
            assert_eq!(err.status, axum::http::StatusCode::FORBIDDEN);
            assert_eq!(err.code, "TENANT_ISOLATION_ERROR");
        }
    }

    #[test]
    fn test_admin_with_explicit_access_allowed() {
        let claims = Claims {
            sub: "admin-1".to_string(),
            email: "admin@system.com".to_string(),
            role: "admin".to_string(),
            roles: vec!["admin".to_string()],
            tenant_id: "system".to_string(),
            admin_tenants: vec!["tenant-a".to_string(), "tenant-b".to_string()],
            device_id: None,
            session_id: Some("sess-3".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: (Utc::now() + Duration::hours(1)).timestamp(),
            iat: Utc::now().timestamp(),
            jti: "jti-3".to_string(),
            nbf: Utc::now().timestamp(),
            iss: JWT_ISSUER.to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        };

        let result = validate_tenant_isolation(&claims, "tenant-a");
        assert!(
            result.is_ok(),
            "Admin with explicit access should be allowed"
        );
    }

    #[test]
    fn test_admin_without_explicit_access_denied() {
        let claims = Claims {
            sub: "admin-2".to_string(),
            email: "admin@system.com".to_string(),
            role: "admin".to_string(),
            roles: vec!["admin".to_string()],
            tenant_id: "system".to_string(),
            admin_tenants: vec!["tenant-a".to_string()], // Only has access to tenant-a
            device_id: None,
            session_id: Some("sess-4".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: (Utc::now() + Duration::hours(1)).timestamp(),
            iat: Utc::now().timestamp(),
            jti: "jti-4".to_string(),
            nbf: Utc::now().timestamp(),
            iss: JWT_ISSUER.to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        };

        let result = validate_tenant_isolation(&claims, "tenant-c");
        assert!(
            result.is_err(),
            "Admin without explicit access should be denied"
        );
    }
}

#[cfg(test)]
mod http_status_code_tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn test_tenant_isolation_returns_403_forbidden() {
        let claims = Claims {
            sub: "user-403".to_string(),
            email: "user@tenant-a.com".to_string(),
            role: "operator".to_string(),
            roles: vec!["operator".to_string()],
            tenant_id: "tenant-a".to_string(),
            admin_tenants: vec![],
            device_id: None,
            session_id: Some("sess-403".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: (Utc::now() + Duration::hours(1)).timestamp(),
            iat: Utc::now().timestamp(),
            jti: "jti-403".to_string(),
            nbf: Utc::now().timestamp(),
            iss: JWT_ISSUER.to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        };

        match validate_tenant_isolation(&claims, "tenant-b") {
            Err(err) => {
                assert_eq!(
                    err.status,
                    StatusCode::FORBIDDEN,
                    "Should return 403 Forbidden for tenant isolation violation"
                );
            }
            Ok(_) => panic!("Expected tenant isolation error"),
        }
    }

    #[test]
    fn test_invalid_token_returns_error() {
        let keypair = Keypair::generate();
        let malformed_token = "invalid.jwt.token";

        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);
        let result =
            validate_token_ed25519(malformed_token, &[(kid, public_pem.clone())], &public_pem);

        assert!(
            result.is_err(),
            "Invalid token should return error (401 in middleware)"
        );
    }
}
