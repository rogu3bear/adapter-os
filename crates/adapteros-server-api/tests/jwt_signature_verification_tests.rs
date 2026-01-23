//! Comprehensive JWT signature verification tests for AdapterOS authentication.
//!
//! This test module validates critical security properties of JWT signature verification:
//!
//! 1. Valid Ed25519-signed tokens are accepted
//! 2. Valid HMAC-signed tokens are accepted
//! 3. Tampered tokens (modified payload) are REJECTED
//! 4. Tokens signed with wrong key are REJECTED
//! 5. Expired tokens are REJECTED
//! 6. Tokens with invalid issuer are REJECTED
//! 7. Tokens with future `nbf` (not-before) are handled correctly
//!
//! These tests exercise the actual validation functions (not mocks) to ensure
//! cryptographic security guarantees hold in production.

use adapteros_crypto::Keypair;
use adapteros_server_api::auth::{
    derive_kid_from_bytes, derive_kid_from_str, encode_ed25519_public_key_pem, generate_token,
    generate_token_ed25519, issue_access_token_ed25519, issue_access_token_hmac,
    issue_refresh_token_ed25519, issue_refresh_token_hmac, validate_access_token_ed25519,
    validate_refresh_token_ed25519, validate_refresh_token_hmac, validate_token, JWT_ISSUER,
};
use base64::Engine;
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

// =============================================================================
// Test Claims structure for manual token construction
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestClaims {
    pub sub: String,
    pub email: String,
    pub role: String,
    #[serde(default)]
    pub roles: Vec<String>,
    pub tenant_id: String,
    #[serde(default)]
    pub admin_tenants: Vec<String>,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub mfa_level: Option<String>,
    #[serde(default)]
    pub rot_id: Option<String>,
    pub exp: i64,
    pub iat: i64,
    pub jti: String,
    #[serde(default)]
    pub nbf: i64,
    #[serde(default = "default_issuer")]
    pub iss: String,
}

fn default_issuer() -> String {
    JWT_ISSUER.to_string()
}

/// Encode a raw 32-byte Ed25519 private key into PKCS#8 DER format for signing
fn encode_ed25519_pkcs8_der(raw_key: &[u8; 32]) -> Vec<u8> {
    let pkcs8_prefix: [u8; 16] = [
        0x30, 0x2e, // SEQUENCE, 46 bytes total
        0x02, 0x01, 0x00, // INTEGER 0 (version)
        0x30, 0x05, // SEQUENCE, 5 bytes
        0x06, 0x03, 0x2b, 0x65, 0x70, // OID 1.3.101.112 (Ed25519)
        0x04, 0x22, // OCTET STRING, 34 bytes
        0x04, 0x20, // OCTET STRING, 32 bytes (the key)
    ];

    let mut der = Vec::with_capacity(48);
    der.extend_from_slice(&pkcs8_prefix);
    der.extend_from_slice(raw_key);
    der
}

// =============================================================================
// Ed25519 Signature Verification Tests
// =============================================================================

mod ed25519_tests {
    use super::*;

    /// Test 1: Valid Ed25519-signed tokens are accepted
    #[test]
    fn valid_ed25519_token_is_accepted() {
        let keypair = Keypair::generate();
        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);

        let token = generate_token_ed25519(
            "user-valid",
            "valid@example.com",
            "admin",
            "tenant-1",
            &keypair,
            3600, // 1 hour
        )
        .expect("Token generation should succeed");

        let result =
            validate_access_token_ed25519(&token, &[(kid, public_pem.clone())], &public_pem);

        assert!(result.is_ok(), "Valid token should be accepted");
        let claims = result.unwrap();
        assert_eq!(claims.sub, "user-valid");
        assert_eq!(claims.tenant_id, "tenant-1");
        assert_eq!(claims.iss, JWT_ISSUER);
    }

    /// Test 2: Valid Ed25519 access token with full claims is accepted
    #[test]
    fn valid_ed25519_access_token_with_full_claims() {
        let keypair = Keypair::generate();
        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);
        let roles = vec!["admin".to_string(), "operator".to_string()];
        let admin_tenants = vec!["tenant-a".to_string(), "tenant-b".to_string()];

        let token = issue_access_token_ed25519(
            "user-full",
            "full@example.com",
            "admin",
            &roles,
            "tenant-main",
            &admin_tenants,
            Some("device-123"),
            "session-abc",
            Some("strong"),
            &keypair,
            Some(3600),
        )
        .expect("Token generation should succeed");

        let result =
            validate_access_token_ed25519(&token, &[(kid, public_pem.clone())], &public_pem);

        assert!(result.is_ok(), "Valid full claims token should be accepted");
        let claims = result.unwrap();
        assert_eq!(claims.sub, "user-full");
        assert_eq!(claims.device_id.as_deref(), Some("device-123"));
        assert_eq!(claims.session_id.as_deref(), Some("session-abc"));
        assert_eq!(claims.mfa_level.as_deref(), Some("strong"));
        assert!(claims.roles.contains(&"admin".to_string()));
        assert!(claims.roles.contains(&"operator".to_string()));
    }

    /// Test 3: Tampered token (modified payload) is REJECTED
    #[test]
    fn tampered_ed25519_token_payload_is_rejected() {
        let keypair = Keypair::generate();
        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);

        let token = generate_token_ed25519(
            "user-original",
            "original@example.com",
            "viewer",
            "tenant-1",
            &keypair,
            3600,
        )
        .expect("Token generation should succeed");

        // Tamper with the payload by modifying a character in the middle part (payload)
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT should have 3 parts");

        // Decode payload, modify, re-encode
        let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1])
            .expect("Payload should be valid base64");
        let mut payload_json: serde_json::Value =
            serde_json::from_slice(&payload_bytes).expect("Payload should be valid JSON");

        // Tamper: change the user ID
        payload_json["sub"] = serde_json::json!("user-attacker");

        let tampered_payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload_json).unwrap());

        let tampered_token = format!("{}.{}.{}", parts[0], tampered_payload, parts[2]);

        let result = validate_access_token_ed25519(
            &tampered_token,
            &[(kid, public_pem.clone())],
            &public_pem,
        );

        assert!(
            result.is_err(),
            "Tampered token MUST be rejected - signature no longer matches payload"
        );
    }

    /// Test 4: Token signed with wrong key is REJECTED
    #[test]
    fn ed25519_token_signed_with_wrong_key_is_rejected() {
        let signing_keypair = Keypair::generate();
        let validation_keypair = Keypair::generate(); // Different keypair

        // Sign with one key
        let token = generate_token_ed25519(
            "user-wrong-key",
            "wrong-key@example.com",
            "admin",
            "tenant-1",
            &signing_keypair,
            3600,
        )
        .expect("Token generation should succeed");

        // Try to validate with different key
        let wrong_public_pem =
            encode_ed25519_public_key_pem(&validation_keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&wrong_public_pem);

        let result = validate_access_token_ed25519(
            &token,
            &[(kid, wrong_public_pem.clone())],
            &wrong_public_pem,
        );

        assert!(
            result.is_err(),
            "Token signed with wrong key MUST be rejected"
        );
    }

    /// Test 5: Expired token is REJECTED
    #[test]
    fn expired_ed25519_token_is_rejected() {
        let keypair = Keypair::generate();
        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);

        // Create a token that expired 2 minutes ago (beyond 60s leeway)
        let now = Utc::now();
        let expired_claims = TestClaims {
            sub: "user-expired".to_string(),
            email: "expired@example.com".to_string(),
            role: "viewer".to_string(),
            roles: vec!["viewer".to_string()],
            tenant_id: "tenant-1".to_string(),
            admin_tenants: vec![],
            device_id: None,
            session_id: Some("sess-1".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: (now - Duration::seconds(120)).timestamp(), // Expired 2 minutes ago
            iat: (now - Duration::hours(1)).timestamp(),
            jti: "jti-expired".to_string(),
            nbf: (now - Duration::hours(1)).timestamp(),
            iss: JWT_ISSUER.to_string(),
        };

        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".to_string());
        header.kid = Some(kid.clone());

        let der_key = encode_ed25519_pkcs8_der(&keypair.to_bytes());
        let token = encode(
            &header,
            &expired_claims,
            &EncodingKey::from_ed_der(&der_key),
        )
        .expect("Token encoding should succeed");

        let result =
            validate_access_token_ed25519(&token, &[(kid, public_pem.clone())], &public_pem);

        assert!(
            result.is_err(),
            "Expired token (beyond leeway) MUST be rejected"
        );
    }

    /// Test 5b: Token expired within leeway is accepted (60 second tolerance)
    #[test]
    fn ed25519_token_expired_within_leeway_is_accepted() {
        let keypair = Keypair::generate();
        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);

        // Create a token that expired 30 seconds ago (within 60s leeway)
        let now = Utc::now();
        let claims = TestClaims {
            sub: "user-leeway".to_string(),
            email: "leeway@example.com".to_string(),
            role: "viewer".to_string(),
            roles: vec!["viewer".to_string()],
            tenant_id: "tenant-1".to_string(),
            admin_tenants: vec![],
            device_id: None,
            session_id: Some("sess-1".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: (now - Duration::seconds(30)).timestamp(), // Expired 30 seconds ago
            iat: (now - Duration::hours(1)).timestamp(),
            jti: "jti-leeway".to_string(),
            nbf: (now - Duration::hours(1)).timestamp(),
            iss: JWT_ISSUER.to_string(),
        };

        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".to_string());
        header.kid = Some(kid.clone());

        let der_key = encode_ed25519_pkcs8_der(&keypair.to_bytes());
        let token = encode(&header, &claims, &EncodingKey::from_ed_der(&der_key))
            .expect("Token encoding should succeed");

        let result =
            validate_access_token_ed25519(&token, &[(kid, public_pem.clone())], &public_pem);

        assert!(
            result.is_ok(),
            "Token expired within leeway should be accepted (clock skew tolerance)"
        );
    }

    /// Test 6: Token with invalid issuer is REJECTED
    #[test]
    fn ed25519_token_with_invalid_issuer_is_rejected() {
        let keypair = Keypair::generate();
        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);

        let now = Utc::now();
        let claims = TestClaims {
            sub: "user-bad-issuer".to_string(),
            email: "bad-issuer@example.com".to_string(),
            role: "viewer".to_string(),
            roles: vec!["viewer".to_string()],
            tenant_id: "tenant-1".to_string(),
            admin_tenants: vec![],
            device_id: None,
            session_id: Some("sess-1".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: (now + Duration::hours(1)).timestamp(),
            iat: now.timestamp(),
            jti: "jti-bad-iss".to_string(),
            nbf: now.timestamp(),
            iss: "malicious-issuer.com".to_string(), // WRONG ISSUER
        };

        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".to_string());
        header.kid = Some(kid.clone());

        let der_key = encode_ed25519_pkcs8_der(&keypair.to_bytes());
        let token = encode(&header, &claims, &EncodingKey::from_ed_der(&der_key))
            .expect("Token encoding should succeed");

        let result =
            validate_access_token_ed25519(&token, &[(kid, public_pem.clone())], &public_pem);

        assert!(
            result.is_err(),
            "Token with invalid issuer MUST be rejected"
        );
    }

    /// Test 7: Token with future `nbf` (not-before) beyond leeway is rejected
    #[test]
    fn ed25519_token_with_future_nbf_beyond_leeway_is_rejected() {
        let keypair = Keypair::generate();
        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);

        let now = Utc::now();
        let claims = TestClaims {
            sub: "user-future-nbf".to_string(),
            email: "future-nbf@example.com".to_string(),
            role: "viewer".to_string(),
            roles: vec!["viewer".to_string()],
            tenant_id: "tenant-1".to_string(),
            admin_tenants: vec![],
            device_id: None,
            session_id: Some("sess-1".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: (now + Duration::hours(2)).timestamp(),
            iat: now.timestamp(),
            jti: "jti-future".to_string(),
            nbf: (now + Duration::seconds(120)).timestamp(), // NOT valid for 2 minutes
            iss: JWT_ISSUER.to_string(),
        };

        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".to_string());
        header.kid = Some(kid.clone());

        let der_key = encode_ed25519_pkcs8_der(&keypair.to_bytes());
        let token = encode(&header, &claims, &EncodingKey::from_ed_der(&der_key))
            .expect("Token encoding should succeed");

        let result =
            validate_access_token_ed25519(&token, &[(kid, public_pem.clone())], &public_pem);

        assert!(
            result.is_err(),
            "Token with future nbf beyond leeway MUST be rejected"
        );
    }

    /// Test 7b: Token with future `nbf` within leeway is accepted
    #[test]
    fn ed25519_token_with_future_nbf_within_leeway_is_accepted() {
        let keypair = Keypair::generate();
        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);

        let now = Utc::now();
        let claims = TestClaims {
            sub: "user-nbf-leeway".to_string(),
            email: "nbf-leeway@example.com".to_string(),
            role: "viewer".to_string(),
            roles: vec!["viewer".to_string()],
            tenant_id: "tenant-1".to_string(),
            admin_tenants: vec![],
            device_id: None,
            session_id: Some("sess-1".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: (now + Duration::hours(2)).timestamp(),
            iat: now.timestamp(),
            jti: "jti-nbf-leeway".to_string(),
            nbf: (now + Duration::seconds(30)).timestamp(), // 30s in future, within 60s leeway
            iss: JWT_ISSUER.to_string(),
        };

        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".to_string());
        header.kid = Some(kid.clone());

        let der_key = encode_ed25519_pkcs8_der(&keypair.to_bytes());
        let token = encode(&header, &claims, &EncodingKey::from_ed_der(&der_key))
            .expect("Token encoding should succeed");

        let result =
            validate_access_token_ed25519(&token, &[(kid, public_pem.clone())], &public_pem);

        assert!(
            result.is_ok(),
            "Token with future nbf within leeway should be accepted (clock skew tolerance)"
        );
    }

    /// Test 8: Completely invalid/malformed token is rejected
    #[test]
    fn malformed_token_is_rejected() {
        let keypair = Keypair::generate();
        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);

        let malformed_tokens = vec![
            "not-a-jwt",
            "only.two.parts",
            "too.many.parts.here.now",
            "",
            "....",
            "header.payload.",    // missing signature
            ".payload.signature", // missing header
        ];

        for malformed in malformed_tokens {
            let result = validate_access_token_ed25519(
                malformed,
                &[(kid.clone(), public_pem.clone())],
                &public_pem,
            );
            assert!(
                result.is_err(),
                "Malformed token '{}' MUST be rejected",
                malformed
            );
        }
    }

    /// Test 9: Token with wrong algorithm (HMAC instead of EdDSA) is rejected
    #[test]
    fn ed25519_validation_rejects_hmac_token() {
        let keypair = Keypair::generate();
        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);

        // Generate an HMAC token
        let hmac_secret = b"some-hmac-secret-key";
        let token = generate_token(
            "user-hmac",
            "hmac@example.com",
            "viewer",
            "tenant-1",
            hmac_secret,
            3600,
        )
        .expect("HMAC token generation should succeed");

        // Try to validate with Ed25519 validator
        let result =
            validate_access_token_ed25519(&token, &[(kid, public_pem.clone())], &public_pem);

        assert!(
            result.is_err(),
            "HMAC token MUST be rejected by Ed25519 validator (algorithm mismatch)"
        );
    }

    /// Test 10: Ed25519 refresh token validation
    #[test]
    fn valid_ed25519_refresh_token_is_accepted() {
        let keypair = Keypair::generate();
        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);
        let roles = vec!["viewer".to_string()];

        let token = issue_refresh_token_ed25519(
            "user-refresh",
            "tenant-refresh",
            &roles,
            Some("device-xyz"),
            "session-refresh",
            "rot-id-1",
            &keypair,
            Some(7200),
        )
        .expect("Refresh token generation should succeed");

        let result =
            validate_refresh_token_ed25519(&token, &[(kid, public_pem.clone())], &public_pem);

        assert!(result.is_ok(), "Valid refresh token should be accepted");
        let claims = result.unwrap();
        assert_eq!(claims.sub, "user-refresh");
        assert_eq!(claims.session_id, "session-refresh");
        assert_eq!(claims.rot_id, "rot-id-1");
        assert_eq!(claims.device_id.as_deref(), Some("device-xyz"));
    }

    /// Test 11: Ed25519 refresh token with wrong key is rejected
    #[test]
    fn ed25519_refresh_token_wrong_key_is_rejected() {
        let signing_keypair = Keypair::generate();
        let wrong_keypair = Keypair::generate();
        let wrong_pem = encode_ed25519_public_key_pem(&wrong_keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&wrong_pem);
        let roles = vec!["viewer".to_string()];

        let token = issue_refresh_token_ed25519(
            "user-refresh-wrong",
            "tenant-1",
            &roles,
            None,
            "session-1",
            "rot-1",
            &signing_keypair,
            Some(7200),
        )
        .expect("Token generation should succeed");

        let result =
            validate_refresh_token_ed25519(&token, &[(kid, wrong_pem.clone())], &wrong_pem);

        assert!(
            result.is_err(),
            "Refresh token signed with wrong key MUST be rejected"
        );
    }
}

// =============================================================================
// HMAC Signature Verification Tests
// =============================================================================

mod hmac_tests {
    use super::*;
    use base64::Engine;

    /// Test 1: Valid HMAC-signed token is accepted
    #[test]
    fn valid_hmac_token_is_accepted() {
        let secret = b"secure-hmac-secret-key-256-bits!";
        let kid = derive_kid_from_bytes(secret);

        let token = generate_token(
            "user-hmac-valid",
            "hmac-valid@example.com",
            "admin",
            "tenant-1",
            secret,
            3600,
        )
        .expect("Token generation should succeed");

        let result = validate_token(&token, &[(kid, secret.to_vec())], secret);

        assert!(result.is_ok(), "Valid HMAC token should be accepted");
        let claims = result.unwrap();
        assert_eq!(claims.sub, "user-hmac-valid");
        assert_eq!(claims.iss, JWT_ISSUER);
    }

    /// Test 2: Valid HMAC access token with full claims
    #[test]
    fn valid_hmac_access_token_with_full_claims() {
        let secret = b"hmac-secret-for-full-claims-test";
        let kid = derive_kid_from_bytes(secret);
        let roles = vec!["admin".to_string(), "dev".to_string()];
        let admin_tenants = vec!["tenant-x".to_string()];

        let token = issue_access_token_hmac(
            "user-hmac-full",
            "hmac-full@example.com",
            "admin",
            &roles,
            "tenant-main",
            &admin_tenants,
            Some("device-hmac"),
            "session-hmac",
            Some("basic"),
            secret,
            Some(3600),
        )
        .expect("Token generation should succeed");

        let result = validate_token(&token, &[(kid, secret.to_vec())], secret);

        assert!(result.is_ok(), "Valid full HMAC token should be accepted");
        let claims = result.unwrap();
        assert_eq!(claims.sub, "user-hmac-full");
        assert_eq!(claims.device_id.as_deref(), Some("device-hmac"));
        assert_eq!(claims.session_id.as_deref(), Some("session-hmac"));
    }

    /// Test 3: Tampered HMAC token (modified payload) is REJECTED
    #[test]
    fn tampered_hmac_token_payload_is_rejected() {
        let secret = b"hmac-secret-for-tampering-test!!";
        let kid = derive_kid_from_bytes(secret);

        let token = generate_token(
            "user-original-hmac",
            "original-hmac@example.com",
            "viewer",
            "tenant-1",
            secret,
            3600,
        )
        .expect("Token generation should succeed");

        // Tamper with the payload
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);

        let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1])
            .expect("Payload should be valid base64");
        let mut payload_json: serde_json::Value =
            serde_json::from_slice(&payload_bytes).expect("Payload should be valid JSON");

        // Tamper: privilege escalation attempt
        payload_json["role"] = serde_json::json!("super_admin");
        payload_json["sub"] = serde_json::json!("user-attacker");

        let tampered_payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload_json).unwrap());

        let tampered_token = format!("{}.{}.{}", parts[0], tampered_payload, parts[2]);

        let result = validate_token(&tampered_token, &[(kid, secret.to_vec())], secret);

        assert!(
            result.is_err(),
            "Tampered HMAC token MUST be rejected - HMAC no longer matches"
        );
    }

    /// Test 4: HMAC token signed with wrong secret is REJECTED
    #[test]
    fn hmac_token_signed_with_wrong_secret_is_rejected() {
        let signing_secret = b"correct-signing-secret-key-here";
        let wrong_secret = b"wrong-validation-secret-key-bad";
        let kid = derive_kid_from_bytes(wrong_secret);

        let token = generate_token(
            "user-wrong-secret",
            "wrong-secret@example.com",
            "admin",
            "tenant-1",
            signing_secret,
            3600,
        )
        .expect("Token generation should succeed");

        let result = validate_token(&token, &[(kid, wrong_secret.to_vec())], wrong_secret);

        assert!(
            result.is_err(),
            "HMAC token signed with wrong secret MUST be rejected"
        );
    }

    /// Test 5: Expired HMAC token is REJECTED
    #[test]
    fn expired_hmac_token_is_rejected() {
        let secret = b"hmac-secret-for-expired-test!!!!";
        let kid = derive_kid_from_bytes(secret);

        let now = Utc::now();
        let claims = TestClaims {
            sub: "user-expired-hmac".to_string(),
            email: "expired-hmac@example.com".to_string(),
            role: "viewer".to_string(),
            roles: vec!["viewer".to_string()],
            tenant_id: "tenant-1".to_string(),
            admin_tenants: vec![],
            device_id: None,
            session_id: Some("sess-1".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: (now - Duration::seconds(120)).timestamp(), // Expired 2 minutes ago
            iat: (now - Duration::hours(1)).timestamp(),
            jti: "jti-expired-hmac".to_string(),
            nbf: (now - Duration::hours(1)).timestamp(),
            iss: JWT_ISSUER.to_string(),
        };

        let mut header = Header::new(Algorithm::HS256);
        header.typ = Some("JWT".to_string());
        header.kid = Some(kid.clone());

        let token = encode(&header, &claims, &EncodingKey::from_secret(secret))
            .expect("Token encoding should succeed");

        let result = validate_token(&token, &[(kid, secret.to_vec())], secret);

        assert!(
            result.is_err(),
            "Expired HMAC token (beyond leeway) MUST be rejected"
        );
    }

    /// Test 6: HMAC token with invalid issuer is REJECTED
    #[test]
    fn hmac_token_with_invalid_issuer_is_rejected() {
        let secret = b"hmac-secret-for-issuer-test!!!!!";
        let kid = derive_kid_from_bytes(secret);

        let now = Utc::now();
        let claims = TestClaims {
            sub: "user-bad-iss-hmac".to_string(),
            email: "bad-iss-hmac@example.com".to_string(),
            role: "viewer".to_string(),
            roles: vec!["viewer".to_string()],
            tenant_id: "tenant-1".to_string(),
            admin_tenants: vec![],
            device_id: None,
            session_id: Some("sess-1".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: (now + Duration::hours(1)).timestamp(),
            iat: now.timestamp(),
            jti: "jti-bad-iss-hmac".to_string(),
            nbf: now.timestamp(),
            iss: "evil-issuer.attacker.com".to_string(), // WRONG ISSUER
        };

        let mut header = Header::new(Algorithm::HS256);
        header.typ = Some("JWT".to_string());
        header.kid = Some(kid.clone());

        let token = encode(&header, &claims, &EncodingKey::from_secret(secret))
            .expect("Token encoding should succeed");

        let result = validate_token(&token, &[(kid, secret.to_vec())], secret);

        assert!(
            result.is_err(),
            "HMAC token with invalid issuer MUST be rejected"
        );
    }

    /// Test 7: HMAC token with future nbf beyond leeway is rejected
    #[test]
    fn hmac_token_with_future_nbf_beyond_leeway_is_rejected() {
        let secret = b"hmac-secret-for-nbf-test!!!!!!!!";
        let kid = derive_kid_from_bytes(secret);

        let now = Utc::now();
        let claims = TestClaims {
            sub: "user-future-nbf-hmac".to_string(),
            email: "future-nbf-hmac@example.com".to_string(),
            role: "viewer".to_string(),
            roles: vec!["viewer".to_string()],
            tenant_id: "tenant-1".to_string(),
            admin_tenants: vec![],
            device_id: None,
            session_id: Some("sess-1".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: (now + Duration::hours(2)).timestamp(),
            iat: now.timestamp(),
            jti: "jti-future-hmac".to_string(),
            nbf: (now + Duration::seconds(120)).timestamp(), // NOT valid for 2 minutes
            iss: JWT_ISSUER.to_string(),
        };

        let mut header = Header::new(Algorithm::HS256);
        header.typ = Some("JWT".to_string());
        header.kid = Some(kid.clone());

        let token = encode(&header, &claims, &EncodingKey::from_secret(secret))
            .expect("Token encoding should succeed");

        let result = validate_token(&token, &[(kid, secret.to_vec())], secret);

        assert!(
            result.is_err(),
            "HMAC token with future nbf beyond leeway MUST be rejected"
        );
    }

    /// Test 8: HMAC refresh token validation
    #[test]
    fn valid_hmac_refresh_token_is_accepted() {
        let secret = b"hmac-secret-for-refresh-test!!!!";
        let kid = derive_kid_from_bytes(secret);
        let roles = vec!["admin".to_string()];

        let token = issue_refresh_token_hmac(
            "user-refresh-hmac",
            "tenant-refresh-hmac",
            &roles,
            Some("device-hmac-refresh"),
            "session-refresh-hmac",
            "rot-hmac-1",
            secret,
            Some(7200),
        )
        .expect("Refresh token generation should succeed");

        let result = validate_refresh_token_hmac(&token, &[(kid, secret.to_vec())], secret);

        assert!(
            result.is_ok(),
            "Valid HMAC refresh token should be accepted"
        );
        let claims = result.unwrap();
        assert_eq!(claims.sub, "user-refresh-hmac");
        assert_eq!(claims.session_id, "session-refresh-hmac");
        assert_eq!(claims.rot_id, "rot-hmac-1");
    }

    /// Test 9: HMAC refresh token with wrong secret is rejected
    #[test]
    fn hmac_refresh_token_wrong_secret_is_rejected() {
        let signing_secret = b"correct-refresh-signing-secret!!";
        let wrong_secret = b"wrong-refresh-validation-secret!";
        let kid = derive_kid_from_bytes(wrong_secret);
        let roles = vec!["viewer".to_string()];

        let token = issue_refresh_token_hmac(
            "user-refresh-wrong",
            "tenant-1",
            &roles,
            None,
            "session-1",
            "rot-1",
            signing_secret,
            Some(7200),
        )
        .expect("Token generation should succeed");

        let result =
            validate_refresh_token_hmac(&token, &[(kid, wrong_secret.to_vec())], wrong_secret);

        assert!(
            result.is_err(),
            "HMAC refresh token signed with wrong secret MUST be rejected"
        );
    }

    /// Test 10: HMAC validation rejects Ed25519 token (algorithm mismatch)
    #[test]
    fn hmac_validation_rejects_ed25519_token() {
        let keypair = Keypair::generate();
        let secret = b"hmac-secret-for-algo-mismatch!!!";
        let kid = derive_kid_from_bytes(secret);

        // Generate an Ed25519 token
        let token = generate_token_ed25519(
            "user-ed25519",
            "ed25519@example.com",
            "viewer",
            "tenant-1",
            &keypair,
            3600,
        )
        .expect("Ed25519 token generation should succeed");

        // Try to validate with HMAC validator
        let result = validate_token(&token, &[(kid, secret.to_vec())], secret);

        assert!(
            result.is_err(),
            "Ed25519 token MUST be rejected by HMAC validator (algorithm mismatch)"
        );
    }
}

// =============================================================================
// Key Rotation and Key ID (kid) Tests
// =============================================================================

mod key_rotation_tests {
    use super::*;

    /// Test: Token signed with rotated-out key is rejected when key removed from keyring
    #[test]
    fn ed25519_token_with_removed_key_is_rejected() {
        let old_keypair = Keypair::generate();
        let new_keypair = Keypair::generate();

        // Sign with old key
        let token = generate_token_ed25519(
            "user-old-key",
            "old-key@example.com",
            "viewer",
            "tenant-1",
            &old_keypair,
            3600,
        )
        .expect("Token generation should succeed");

        // Validate with keyring that only contains new key (old key rotated out)
        let new_pem = encode_ed25519_public_key_pem(&new_keypair.public_key().to_bytes());
        let new_kid = derive_kid_from_str(&new_pem);

        let result = validate_access_token_ed25519(
            &token,
            &[(new_kid, new_pem.clone())],
            &new_pem, // fallback also uses new key
        );

        assert!(
            result.is_err(),
            "Token signed with rotated-out key MUST be rejected"
        );
    }

    /// Test: Token with old key is still valid during rotation grace period (both keys in keyring)
    #[test]
    fn ed25519_token_valid_during_key_rotation_grace_period() {
        let old_keypair = Keypair::generate();
        let new_keypair = Keypair::generate();

        let old_pem = encode_ed25519_public_key_pem(&old_keypair.public_key().to_bytes());
        let new_pem = encode_ed25519_public_key_pem(&new_keypair.public_key().to_bytes());
        let old_kid = derive_kid_from_str(&old_pem);
        let new_kid = derive_kid_from_str(&new_pem);

        // Sign with old key
        let token = generate_token_ed25519(
            "user-rotation",
            "rotation@example.com",
            "viewer",
            "tenant-1",
            &old_keypair,
            3600,
        )
        .expect("Token generation should succeed");

        // Validate with keyring containing both keys
        let keys = vec![
            (new_kid, new_pem.clone()), // New key first (primary)
            (old_kid, old_pem.clone()), // Old key still present for grace period
        ];

        let result = validate_access_token_ed25519(&token, &keys, &new_pem);

        assert!(
            result.is_ok(),
            "Token signed with old key should be valid during rotation grace period"
        );
    }

    /// Test: HMAC key rotation with kid selection
    #[test]
    fn hmac_key_rotation_with_kid_selection() {
        let old_secret = b"old-hmac-secret-key-for-rotation";
        let new_secret = b"new-hmac-secret-key-for-rotation";

        let old_kid = derive_kid_from_bytes(old_secret);
        let new_kid = derive_kid_from_bytes(new_secret);

        // Sign with old key
        let token = generate_token(
            "user-hmac-rotation",
            "hmac-rotation@example.com",
            "viewer",
            "tenant-1",
            old_secret,
            3600,
        )
        .expect("Token generation should succeed");

        // Validate with keyring containing both keys
        let keys = vec![
            (new_kid, new_secret.to_vec()),
            (old_kid, old_secret.to_vec()),
        ];

        let result = validate_token(&token, &keys, new_secret);

        assert!(
            result.is_ok(),
            "HMAC token with old key should be valid when key still in keyring"
        );
    }

    /// Test: Token without kid uses fallback key
    #[test]
    fn token_without_kid_uses_fallback() {
        let keypair = Keypair::generate();
        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());

        let now = Utc::now();
        let claims = TestClaims {
            sub: "user-no-kid".to_string(),
            email: "no-kid@example.com".to_string(),
            role: "viewer".to_string(),
            roles: vec!["viewer".to_string()],
            tenant_id: "tenant-1".to_string(),
            admin_tenants: vec![],
            device_id: None,
            session_id: Some("sess-1".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: (now + Duration::hours(1)).timestamp(),
            iat: now.timestamp(),
            jti: "jti-no-kid".to_string(),
            nbf: now.timestamp(),
            iss: JWT_ISSUER.to_string(),
        };

        // Create token WITHOUT kid in header
        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".to_string());
        // Intentionally NOT setting header.kid

        let der_key = encode_ed25519_pkcs8_der(&keypair.to_bytes());
        let token = encode(&header, &claims, &EncodingKey::from_ed_der(&der_key))
            .expect("Token encoding should succeed");

        // Validate with empty keyring but correct fallback
        let result = validate_access_token_ed25519(&token, &[], &public_pem);

        assert!(
            result.is_ok(),
            "Token without kid should use fallback key for validation"
        );
    }
}

// =============================================================================
// Signature Manipulation Attack Tests
// =============================================================================

mod signature_attack_tests {
    use super::*;
    use base64::Engine;

    /// Test: Signature stripping attack (removing signature) is rejected
    #[test]
    fn signature_stripping_attack_is_rejected() {
        let keypair = Keypair::generate();
        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);

        let token = generate_token_ed25519(
            "user-sig-strip",
            "sig-strip@example.com",
            "admin",
            "tenant-1",
            &keypair,
            3600,
        )
        .expect("Token generation should succeed");

        let parts: Vec<&str> = token.split('.').collect();

        // Attack: remove signature entirely
        let stripped_token = format!("{}.{}.", parts[0], parts[1]);

        let result = validate_access_token_ed25519(
            &stripped_token,
            &[(kid, public_pem.clone())],
            &public_pem,
        );

        assert!(
            result.is_err(),
            "Signature stripping attack MUST be rejected"
        );
    }

    /// Test: Signature replacement with different key's signature is rejected
    #[test]
    fn signature_replacement_attack_is_rejected() {
        let victim_keypair = Keypair::generate();
        let attacker_keypair = Keypair::generate();
        let victim_pem = encode_ed25519_public_key_pem(&victim_keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&victim_pem);

        // Generate victim's token
        let victim_token = generate_token_ed25519(
            "user-victim",
            "victim@example.com",
            "viewer",
            "tenant-1",
            &victim_keypair,
            3600,
        )
        .expect("Token generation should succeed");

        // Generate attacker's token with elevated privileges
        let attacker_token = generate_token_ed25519(
            "user-attacker",
            "attacker@example.com",
            "super_admin",
            "tenant-1",
            &attacker_keypair,
            3600,
        )
        .expect("Token generation should succeed");

        // Attack: Take header+payload from victim, signature from attacker
        let victim_parts: Vec<&str> = victim_token.split('.').collect();
        let attacker_parts: Vec<&str> = attacker_token.split('.').collect();

        let mixed_token = format!(
            "{}.{}.{}",
            victim_parts[0], victim_parts[1], attacker_parts[2]
        );

        let result =
            validate_access_token_ed25519(&mixed_token, &[(kid, victim_pem.clone())], &victim_pem);

        assert!(
            result.is_err(),
            "Signature replacement attack MUST be rejected"
        );
    }

    /// Test: Algorithm confusion attack (none algorithm) is rejected
    #[test]
    fn algorithm_none_attack_is_rejected() {
        let keypair = Keypair::generate();
        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);

        let now = Utc::now();

        // Craft a token with "alg": "none"
        let header = serde_json::json!({
            "alg": "none",
            "typ": "JWT"
        });

        let claims = serde_json::json!({
            "sub": "user-none-alg",
            "email": "none-alg@example.com",
            "role": "super_admin",
            "roles": ["super_admin"],
            "tenant_id": "tenant-1",
            "admin_tenants": [],
            "exp": (now + Duration::hours(1)).timestamp(),
            "iat": now.timestamp(),
            "jti": "jti-none-alg",
            "nbf": now.timestamp(),
            "iss": JWT_ISSUER
        });

        let header_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&header).unwrap());
        let payload_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&claims).unwrap());

        // "none" algorithm means empty signature
        let none_token = format!("{}. {}.", header_b64, payload_b64);

        let result =
            validate_access_token_ed25519(&none_token, &[(kid, public_pem.clone())], &public_pem);

        assert!(result.is_err(), "Algorithm 'none' attack MUST be rejected");
    }

    /// Test: Bit-flipped signature is rejected
    #[test]
    fn bit_flipped_signature_is_rejected() {
        let keypair = Keypair::generate();
        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);

        let token = generate_token_ed25519(
            "user-bitflip",
            "bitflip@example.com",
            "admin",
            "tenant-1",
            &keypair,
            3600,
        )
        .expect("Token generation should succeed");

        let parts: Vec<&str> = token.split('.').collect();

        // Decode signature, flip a bit, re-encode
        let mut sig_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[2])
            .expect("Signature should be valid base64");

        // Flip a bit in the middle of the signature
        sig_bytes[32] ^= 0x01;

        let corrupted_sig = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&sig_bytes);
        let corrupted_token = format!("{}.{}.{}", parts[0], parts[1], corrupted_sig);

        let result = validate_access_token_ed25519(
            &corrupted_token,
            &[(kid, public_pem.clone())],
            &public_pem,
        );

        assert!(result.is_err(), "Bit-flipped signature MUST be rejected");
    }
}
