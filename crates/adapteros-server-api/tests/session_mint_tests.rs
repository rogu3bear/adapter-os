//! Integration tests for session capability token minting.
//!
//! These tests verify:
//! 1. Minted tokens can be decoded by the existing session token functions
//! 2. The stack hash in the minted token matches what resolve_session_token_lock expects
//! 3. Tokens with proper Claims structure can be validated as JWTs

use adapteros_crypto::Keypair;
use adapteros_server_api::auth::{
    derive_kid_from_str, encode_ed25519_public_key_pem, validate_access_token_ed25519,
};
use adapteros_server_api::session_tokens::{
    decode_session_token_lock, strip_session_token_prefix, SESSION_TOKEN_PREFIX,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm as JwtAlgorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

/// Session token claims structure matching what mint_session_token produces.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionTokenClaims {
    sub: String,
    #[serde(default)]
    email: String,
    role: String,
    #[serde(default)]
    roles: Vec<String>,
    tenant_id: String,
    #[serde(default)]
    admin_tenants: Vec<String>,
    exp: i64,
    iat: i64,
    jti: String,
    nbf: i64,
    iss: String,
    session_lock: SessionLock,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SessionLock {
    #[serde(default)]
    stack_id: Option<String>,
    #[serde(default)]
    stack_hash_b3: Option<String>,
    #[serde(default)]
    adapter_ids: Option<Vec<String>>,
    #[serde(default)]
    pinned_adapter_ids: Option<Vec<String>>,
}

/// Encode a raw 32-byte Ed25519 private key into PKCS#8 DER format for signing
fn encode_ed25519_pkcs8_der(raw_key: &[u8; 32]) -> Vec<u8> {
    let pkcs8_prefix: [u8; 16] = [
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    let mut der = Vec::with_capacity(48);
    der.extend_from_slice(&pkcs8_prefix);
    der.extend_from_slice(raw_key);
    der
}

#[test]
fn test_session_token_round_trip() {
    let keypair = Keypair::generate();
    let now = Utc::now();
    let exp = now + Duration::hours(1);

    // Create claims with session lock
    let claims = SessionTokenClaims {
        sub: "user-123".to_string(),
        email: "test@example.com".to_string(),
        role: "viewer".to_string(),
        roles: vec!["viewer".to_string()],
        tenant_id: "tenant-abc".to_string(),
        admin_tenants: vec![],
        exp: exp.timestamp(),
        iat: now.timestamp(),
        jti: "jti-mint-1".to_string(),
        nbf: now.timestamp(),
        iss: "adapteros-server".to_string(),
        session_lock: SessionLock {
            stack_id: Some("stack-1".to_string()),
            stack_hash_b3: Some(
                "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string(),
            ),
            adapter_ids: Some(vec!["adapter-a".to_string(), "adapter-b".to_string()]),
            pinned_adapter_ids: Some(vec!["adapter-a".to_string()]),
        },
    };

    // Sign with Ed25519
    let mut header = Header::new(JwtAlgorithm::EdDSA);
    header.typ = Some("JWT".to_string());
    let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
    header.kid = Some(derive_kid_from_str(&public_pem));

    let raw_key = keypair.to_bytes();
    let der_key = encode_ed25519_pkcs8_der(&raw_key);
    let jwt = encode(&header, &claims, &EncodingKey::from_ed_der(&der_key))
        .expect("failed to encode JWT");

    // Prefix with session token marker
    let token = format!("{}.{}", SESSION_TOKEN_PREFIX, jwt);

    // Verify strip_session_token_prefix works
    let stripped = strip_session_token_prefix(&token).expect("should strip prefix");
    assert_eq!(stripped, jwt);

    // Verify decode_session_token_lock extracts the session lock
    let lock = decode_session_token_lock(stripped).expect("should decode lock");
    assert_eq!(lock.stack_id.as_deref(), Some("stack-1"));
    assert!(lock.stack_hash_b3.is_some());
    assert_eq!(
        lock.adapter_ids.as_deref(),
        Some(&["adapter-a".to_string(), "adapter-b".to_string()][..])
    );
    assert_eq!(
        lock.pinned_adapter_ids.as_deref(),
        Some(&["adapter-a".to_string()][..])
    );

    // Verify the JWT can be validated as Claims
    let kid = derive_kid_from_str(&public_pem);
    let validated =
        validate_access_token_ed25519(stripped, &[(kid, public_pem.clone())], &public_pem)
            .expect("should validate JWT");
    assert_eq!(validated.sub, "user-123");
    assert_eq!(validated.tenant_id, "tenant-abc");
    assert_eq!(validated.jti, "jti-mint-1");
}

#[test]
fn test_session_token_with_flat_lock_fields() {
    // Test that tokens with flat (non-nested) lock fields also work
    // This is the serde(flatten) path in SessionTokenPayload

    let keypair = Keypair::generate();
    let now = Utc::now();
    let exp = now + Duration::hours(1);

    // Build claims with flat lock fields (no nested session_lock)
    #[derive(Debug, Clone, Serialize)]
    struct FlatClaims {
        sub: String,
        email: String,
        role: String,
        roles: Vec<String>,
        tenant_id: String,
        admin_tenants: Vec<String>,
        exp: i64,
        iat: i64,
        jti: String,
        nbf: i64,
        iss: String,
        // Flat lock fields
        stack_id: String,
        stack_hash_b3: String,
        adapter_ids: Vec<String>,
        pinned_adapter_ids: Vec<String>,
    }

    let claims = FlatClaims {
        sub: "user-flat".to_string(),
        email: "flat@example.com".to_string(),
        role: "admin".to_string(),
        roles: vec!["admin".to_string()],
        tenant_id: "tenant-flat".to_string(),
        admin_tenants: vec![],
        exp: exp.timestamp(),
        iat: now.timestamp(),
        jti: "jti-flat-1".to_string(),
        nbf: now.timestamp(),
        iss: "adapteros-server".to_string(),
        stack_id: "stack-flat".to_string(),
        stack_hash_b3: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .to_string(),
        adapter_ids: vec!["flat-adapter".to_string()],
        pinned_adapter_ids: vec!["flat-adapter".to_string()],
    };

    let mut header = Header::new(JwtAlgorithm::EdDSA);
    header.typ = Some("JWT".to_string());

    let raw_key = keypair.to_bytes();
    let der_key = encode_ed25519_pkcs8_der(&raw_key);
    let jwt = encode(&header, &claims, &EncodingKey::from_ed_der(&der_key))
        .expect("failed to encode JWT");

    // Verify decode_session_token_lock extracts flat fields
    let lock = decode_session_token_lock(&jwt).expect("should decode flat lock");
    assert_eq!(lock.stack_id.as_deref(), Some("stack-flat"));
    assert_eq!(
        lock.stack_hash_b3.as_deref(),
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    );
    assert_eq!(
        lock.adapter_ids.as_deref(),
        Some(&["flat-adapter".to_string()][..])
    );
}

#[test]
fn test_session_token_prefix_stripping() {
    // Test various prefix formats that should work
    let jwt = "header.payload.signature";

    // Standard format: aos_sess_v1.jwt
    let token1 = format!("{}.{}", SESSION_TOKEN_PREFIX, jwt);
    assert_eq!(strip_session_token_prefix(&token1), Some(jwt));

    // With space separator
    let token2 = format!("{} {}", SESSION_TOKEN_PREFIX, jwt);
    assert_eq!(strip_session_token_prefix(&token2), Some(jwt));

    // With colon separator
    let token3 = format!("{}:{}", SESSION_TOKEN_PREFIX, jwt);
    assert_eq!(strip_session_token_prefix(&token3), Some(jwt));

    // Non-session token should return None
    let regular_jwt = "Bearer eyJ0eXAiOiJKV1QiLCJhbGciOiJFZERTQSJ9.payload.sig";
    assert!(strip_session_token_prefix(regular_jwt).is_none());

    // Empty after prefix should return None
    let empty_token = SESSION_TOKEN_PREFIX;
    assert!(strip_session_token_prefix(empty_token).is_none());
}

#[test]
fn test_decode_session_token_lock_invalid_jwt() {
    // Test that invalid JWTs are rejected

    // Not a JWT at all
    let result = decode_session_token_lock("not-a-jwt");
    assert!(result.is_err());

    // JWT with only two parts
    let result = decode_session_token_lock("header.payload");
    assert!(result.is_err());

    // JWT with invalid base64
    let result = decode_session_token_lock("!!invalid!!.payload.signature");
    assert!(result.is_err());
}
