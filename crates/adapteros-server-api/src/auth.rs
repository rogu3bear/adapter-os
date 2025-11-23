use adapteros_crypto::Keypair;
use anyhow::Result;
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

/// JWT claims structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user_id
    pub email: String,
    pub role: String,
    pub tenant_id: String,
    pub exp: i64,
    pub iat: i64,
    pub jti: String, // JWT ID for token tracking and revocation
    pub nbf: i64,    // Not Before timestamp
}

/// Hash a password using Argon2id
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("failed to hash password: {}", e))?
        .to_string();
    Ok(password_hash)
}

/// Verify a password against a hash
pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    let parsed_hash =
        PasswordHash::new(hash).map_err(|e| anyhow::anyhow!("failed to parse hash: {}", e))?;
    let argon2 = Argon2::default();
    Ok(argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// Generate a JWT token with Ed25519 signing
pub fn generate_token_ed25519(
    user_id: &str,
    email: &str,
    role: &str,
    tenant_id: &str,
    keypair: &Keypair,
) -> Result<String> {
    let now = Utc::now();
    let exp = now + Duration::hours(8); // 8 hour expiry
    let nbf = now; // Token valid immediately

    // Generate unique JWT ID using BLAKE3
    let jti = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(user_id.as_bytes());
        hasher.update(&now.timestamp().to_le_bytes());
        hasher.update(tenant_id.as_bytes());
        hex::encode(hasher.finalize().as_bytes())
    };

    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        role: role.to_string(),
        tenant_id: tenant_id.to_string(),
        exp: exp.timestamp(),
        iat: now.timestamp(),
        jti,
        nbf: nbf.timestamp(),
    };

    // Use Ed25519 algorithm for signing
    let mut header = Header::new(Algorithm::EdDSA);
    header.typ = Some("JWT".to_string());

    // Convert raw Ed25519 private key bytes to PKCS#8 DER format
    // The raw key is 32 bytes, but from_ed_der expects PKCS#8 DER encoding
    let raw_key = keypair.to_bytes();
    let der_key = encode_ed25519_pkcs8_der(&raw_key);
    let token = encode(&header, &claims, &EncodingKey::from_ed_der(&der_key))?;
    Ok(token)
}

/// Encode a raw Ed25519 public key into PEM format for JWT validation
pub fn encode_ed25519_public_key_pem(public_key_bytes: &[u8]) -> String {
    // X.509 SubjectPublicKeyInfo structure for Ed25519 public keys:
    // SEQUENCE {
    //   SEQUENCE { OBJECT IDENTIFIER 1.3.101.112 }  -- AlgorithmIdentifier
    //   BIT STRING <public-key-bytes>
    // }

    // Pre-computed DER for SubjectPublicKeyInfo with Ed25519 OID
    let der_prefix: [u8; 12] = [
        0x30, 0x2a,              // SEQUENCE, length 42
        0x30, 0x05,              // SEQUENCE (AlgorithmIdentifier)
        0x06, 0x03,              // OID, length 3
        0x2b, 0x65, 0x70,        // OID: 1.3.101.112 (Ed25519)
        0x03, 0x21,              // BIT STRING, length 33 (32 bytes + 1 leading 0x00)
        0x00,                    // No unused bits in the bit string
    ];

    let mut der_encoded = Vec::new();
    der_encoded.extend_from_slice(&der_prefix);
    der_encoded.extend_from_slice(public_key_bytes);

    // Encode as PEM
    format!(
        "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----",
        base64_encode(&der_encoded)
    )
}

/// Base64 encode bytes (standard Base64 without padding)
fn base64_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let mut i = 0;

    while i < data.len() {
        let b1 = data[i];
        i += 1;

        let (b2, b3) = if i < data.len() {
            let b2 = data[i];
            i += 1;
            let b3 = if i < data.len() {
                let b3 = data[i];
                i += 1;
                b3
            } else {
                0
            };
            (b2, b3)
        } else {
            (0, 0)
        };

        let idx1 = (b1 >> 2) as usize;
        let idx2 = (((b1 & 0x03) << 4) | ((b2 >> 4) & 0x0f)) as usize;
        let idx3 = if i - 1 < data.len() {
            (((b2 & 0x0f) << 2) | ((b3 >> 6) & 0x03)) as usize
        } else {
            64
        };
        let idx4 = if i < data.len() {
            (b3 & 0x3f) as usize
        } else {
            64
        };

        result.push(BASE64_CHARS[idx1] as char);
        result.push(BASE64_CHARS[idx2] as char);
        result.push(if idx3 == 64 { '=' } else { BASE64_CHARS[idx3] as char });
        result.push(if idx4 == 64 { '=' } else { BASE64_CHARS[idx4] as char });
    }

    result
}

/// Encode a raw 32-byte Ed25519 private key into PKCS#8 DER format
///
/// PKCS#8 structure for Ed25519:
/// ```asn1
/// SEQUENCE {
///   INTEGER 0                          -- version
///   SEQUENCE {
///     OBJECT IDENTIFIER 1.3.101.112    -- Ed25519 OID
///   }
///   OCTET STRING {
///     OCTET STRING <32-byte-key>       -- wrapped private key
///   }
/// }
/// ```
fn encode_ed25519_pkcs8_der(raw_key: &[u8; 32]) -> Vec<u8> {
    // PKCS#8 header for Ed25519 (16 bytes prefix)
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

/// Generate a JWT token (HMAC-SHA256 fallback for compatibility)
pub fn generate_token(
    user_id: &str,
    email: &str,
    role: &str,
    tenant_id: &str,
    secret: &[u8],
) -> Result<String> {
    let now = Utc::now();
    let exp = now + Duration::hours(8); // 8 hour expiry
    let nbf = now; // Token valid immediately

    // Generate unique JWT ID using BLAKE3
    let jti = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(user_id.as_bytes());
        hasher.update(&now.timestamp().to_le_bytes());
        hasher.update(tenant_id.as_bytes());
        hex::encode(hasher.finalize().as_bytes())
    };

    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        role: role.to_string(),
        tenant_id: tenant_id.to_string(),
        exp: exp.timestamp(),
        iat: now.timestamp(),
        jti,
        nbf: nbf.timestamp(),
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret),
    )?;
    Ok(token)
}

/// Validate and decode a JWT token with Ed25519
pub fn validate_token_ed25519(token: &str, public_key_pem: &str) -> Result<Claims> {
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_nbf = true; // Validate "not before" timestamp

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_ed_pem(public_key_pem.as_bytes())?,
        &validation,
    )?;
    Ok(token_data.claims)
}

/// Validate and decode a JWT token (HMAC-SHA256 fallback)
pub fn validate_token(token: &str, secret: &[u8]) -> Result<Claims> {
    let mut validation = Validation::default();
    validation.validate_nbf = true; // Validate "not before" timestamp

    let token_data = decode::<Claims>(token, &DecodingKey::from_secret(secret), &validation)?;
    Ok(token_data.claims)
}

/// Refresh a JWT token (generate new token with updated expiry)
pub fn refresh_token(claims: &Claims, keypair: &Keypair) -> Result<String> {
    generate_token_ed25519(
        &claims.sub,
        &claims.email,
        &claims.role,
        &claims.tenant_id,
        keypair,
    )
}

/// Check if a token is about to expire (within 1 hour)
pub fn token_needs_refresh(claims: &Claims) -> bool {
    let now = Utc::now().timestamp();
    let time_until_expiry = claims.exp - now;
    time_until_expiry < 3600 // Less than 1 hour remaining
}
