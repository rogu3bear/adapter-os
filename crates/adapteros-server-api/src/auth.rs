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

    // Convert Ed25519 private key to PEM format for JWT encoding
    let key_bytes = keypair.to_bytes();
    let token = encode(&header, &claims, &EncodingKey::from_ed_der(&key_bytes))?;
    Ok(token)
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
