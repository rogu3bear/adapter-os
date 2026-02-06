//! Authentication and password verification module.
//!
//! # Security Guarantees
//!
//! This module provides timing-safe password verification to prevent side-channel attacks:
//!
//! - **Constant-time verification**: Password comparisons use Argon2's built-in constant-time
//!   comparison (via the `subtle` crate internally) to prevent timing attacks.
//! - **Timing-consistent failures**: Invalid hash formats and failed verifications execute
//!   a hardened Argon2 hash to maintain consistent timing regardless of failure reason.
//! - **Single entry point**: All password verification goes through [`verify_password`],
//!   ensuring consistent security properties across the codebase.
//!
//! # Implementation Notes
//!
//! The `argon2` crate (v0.5+) uses `subtle::ConstantTimeEq` internally for password
//! verification, providing timing-safe comparison without explicit constant-time code here.

use adapteros_crypto::Keypair;
use adapteros_db::auth_sessions_kv::AuthSessionKvRepository;
use anyhow::{anyhow, Result};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm as Argon2Algorithm, Argon2, Params, Version,
};
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};
use blake3;
use chrono::{Duration, Utc};
use jsonwebtoken::{
    decode, encode, Algorithm as JwtAlgorithm, DecodingKey, EncodingKey, Header, Validation,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::OnceLock;
#[cfg(not(debug_assertions))]
use tracing::error;
#[cfg(debug_assertions)]
use tracing::warn;
use adapteros_id::{IdPrefix, TypedId};

const ARGON2_MEMORY_KIB: u32 = 64 * 1024; // 64 MiB
const ARGON2_ITERATIONS: u32 = 3;
const ARGON2_PARALLELISM: u32 = 1;

const DEFAULT_ACCESS_TOKEN_TTL_SECS: u64 = 15 * 60; // 15 minutes
pub const DEFAULT_SESSION_TTL_SECS: u64 = 2 * 60 * 60; // 2 hours
pub const JWT_ISSUER: &str = "adapteros-server";

fn default_issuer() -> String {
    JWT_ISSUER.to_string()
}

fn hash_token_hex(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}

/// Derive a key ID from arbitrary bytes.
///
/// Uses BLAKE3 hash truncated to 16 bytes (128 bits / 32 hex chars).
/// This provides sufficient entropy to avoid birthday-bound collisions.
pub fn derive_kid_from_bytes(data: &[u8]) -> String {
    let hash = blake3::hash(data);
    hex::encode(&hash.as_bytes()[..16])
}

pub fn derive_kid_from_str(data: &str) -> String {
    derive_kid_from_bytes(data.as_bytes())
}

/// How a caller was authenticated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    #[serde(alias = "jwt", alias = "bearer")]
    #[default]
    BearerToken,
    Cookie,
    ApiKey,
    DevBypass,
    Unauthenticated,
}

impl AuthMode {
    pub fn is_authenticated(&self) -> bool {
        !matches!(self, AuthMode::Unauthenticated)
    }
}

/// Logical identity category of the caller.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalType {
    #[default]
    User,
    ApiKey,
    DevBypass,
    InternalService,
}

/// Normalized principal representation derived from claims/API keys/dev bypass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal {
    pub principal_type: PrincipalType,
    pub principal_id: String,
    pub tenant_id: String,
    pub admin_tenants: Vec<String>,
    pub session_id: Option<String>,
    pub device_id: Option<String>,
    pub mfa_level: Option<String>,
    pub jti: String,
    pub auth_mode: AuthMode,
}

impl Principal {
    pub fn from_claims(
        claims: &Claims,
        principal_type: PrincipalType,
        auth_mode: AuthMode,
    ) -> Self {
        Self {
            principal_type,
            principal_id: claims.sub.clone(),
            tenant_id: claims.tenant_id.clone(),
            admin_tenants: claims.admin_tenants.clone(),
            session_id: claims.session_id.clone(),
            device_id: claims.device_id.clone(),
            mfa_level: claims.mfa_level.clone(),
            jti: claims.jti.clone(),
            auth_mode,
        }
    }
}

pub use async_trait::async_trait;

impl<S> FromRequestParts<S> for Principal
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(principal) = parts.extensions.get::<Principal>() {
            Ok(principal.clone())
        } else {
            Err((
                StatusCode::UNAUTHORIZED,
                "Authentication required".to_string(),
            ))
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DevBypassStatus {
    pub active: bool,
    pub build: &'static str,
    pub env_requested: bool,
}

/// Check if production mode is enabled via AOS_PRODUCTION_MODE env var.
///
/// SECURITY: Production mode blocks dev bypass even in debug builds.
fn is_production_mode() -> bool {
    env::var("AOS_PRODUCTION_MODE")
        .map(|v| {
            let lower = v.to_ascii_lowercase();
            matches!(lower.as_str(), "1" | "true")
        })
        .unwrap_or(false)
}

fn dev_bypass_requested() -> (bool, bool) {
    let env_present = env::var("AOS_DEV_NO_AUTH").is_ok();
    let requested = env::var("AOS_DEV_NO_AUTH")
        .map(|v| {
            let lower = v.to_ascii_lowercase();
            matches!(lower.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false);
    (env_present, requested)
}

/// Shared, single-evaluated dev-bypass status (logs exactly once).
pub(crate) fn dev_bypass_status() -> &'static DevBypassStatus {
    static STATUS: OnceLock<DevBypassStatus> = OnceLock::new();
    STATUS.get_or_init(|| {
        #[cfg_attr(not(debug_assertions), allow(unused_variables))]
        let (env_present, requested) = dev_bypass_requested();
        let production_mode = is_production_mode();

        #[cfg(debug_assertions)]
        {
            // SECURITY: Production mode blocks dev bypass even in debug builds
            if production_mode && requested {
                warn!(
                    "AOS_DEV_NO_AUTH requested but AOS_PRODUCTION_MODE=1 is set - \
                     dev bypass is BLOCKED in production mode even for debug builds. \
                     This is a security measure to prevent accidental auth bypass in production."
                );
                return DevBypassStatus {
                    active: false,
                    build: "debug",
                    env_requested: true,
                };
            }

            if requested {
                warn!(
                    "Dev auth bypass ENABLED via AOS_DEV_NO_AUTH (debug build only) — auth_mode=dev_bypass; unsafe for production"
                );
                return DevBypassStatus {
                    active: true,
                    build: "debug",
                    env_requested: true,
                };
            }

            DevBypassStatus {
                active: false,
                build: "debug",
                env_requested: env_present,
            }
        }

        #[cfg(not(debug_assertions))]
        {
            if env_present {
                error!(
                    "AOS_DEV_NO_AUTH detected in release build - this flag is ignored in production"
                );
            }

            DevBypassStatus {
                active: false,
                build: "release",
                env_requested: env_present,
            }
        }
    })
}

/// Enable dev bypass from config at server startup.
///
/// SECURITY: This must be called BEFORE any auth middleware is invoked
/// (i.e., before the first request is processed) and BEFORE `dev_bypass_status()`
/// is ever called. Sets the environment variable so that when `dev_bypass_status()`
/// initializes its OnceLock, it sees the env var and enables bypass.
///
/// Only effective in debug builds; release builds ignore this entirely.
#[cfg(debug_assertions)]
pub fn set_dev_bypass_from_config(enabled: bool) {
    if enabled {
        // Set env var BEFORE dev_bypass_status() is called anywhere.
        // The OnceLock in dev_bypass_status() will pick this up on first access.
        std::env::set_var("AOS_DEV_NO_AUTH", "1");
    }
}

#[cfg(not(debug_assertions))]
pub fn set_dev_bypass_from_config(_enabled: bool) {
    // No-op in release builds - bypass is never allowed
}

/// Check if dev bypass mode is enabled.
///
/// Returns true if running in dev mode with `AOS_DEV_NO_AUTH=1` or
/// `security.dev_bypass=true` in config (debug builds only).
///
/// Use this to skip non-essential boot phases and background tasks
/// for faster development iteration.
#[cfg(debug_assertions)]
pub fn is_dev_bypass_enabled() -> bool {
    dev_bypass_status().active
}

#[cfg(not(debug_assertions))]
pub fn is_dev_bypass_enabled() -> bool {
    false
}

/// Access token claims (used across the API as `Claims`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessClaims {
    pub sub: String, // user_id
    #[serde(default)]
    pub email: String,
    pub role: String,
    #[serde(default)]
    pub roles: Vec<String>, // Multiple roles support
    pub tenant_id: String,
    #[serde(default)]
    pub admin_tenants: Vec<String>, // Tenants this admin can access (empty = own tenant only)
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
    pub jti: String, // JWT ID for token tracking and revocation
    #[serde(default)]
    pub nbf: i64, // Not Before timestamp
    #[serde(default = "default_issuer")]
    pub iss: String,
    #[serde(default)]
    pub auth_mode: AuthMode,
    #[serde(default)]
    pub principal_type: Option<PrincipalType>,
}

/// Refresh token claims used for session renewal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshClaims {
    pub sub: String,
    pub tenant_id: String,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub device_id: Option<String>,
    pub session_id: String,
    pub rot_id: String,
    pub exp: i64,
    pub iat: i64,
    #[serde(default = "default_issuer")]
    pub iss: String,
}

/// Backward-compatible alias (many handlers expect `Claims`)
pub type Claims = AccessClaims;

/// SECURITY: Dev no-auth bypass is only available in debug builds.
pub(crate) fn dev_no_auth_enabled() -> bool {
    dev_bypass_status().active
}

/// Password verification result with upgrade hint
pub struct PasswordVerification {
    pub valid: bool,
    pub needs_rehash: bool,
}

fn strong_argon2() -> Result<Argon2<'static>> {
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        None,
    )
    .map_err(|e| anyhow!("invalid argon2 params: {}", e))?;
    Ok(Argon2::new(
        Argon2Algorithm::Argon2id,
        Version::V0x13,
        params,
    ))
}

fn argon2_needs_upgrade(parsed_hash: &PasswordHash) -> bool {
    if !parsed_hash
        .algorithm
        .as_str()
        .eq_ignore_ascii_case("argon2id")
    {
        return true;
    }

    let params = &parsed_hash.params;
    let mem_ok = params
        .get("m")
        .and_then(|v| v.decimal().ok())
        .map(|m| u64::from(m) >= ARGON2_MEMORY_KIB as u64)
        .unwrap_or(false);
    let time_ok = params
        .get("t")
        .and_then(|v| v.decimal().ok())
        .map(|t| u64::from(t) >= ARGON2_ITERATIONS as u64)
        .unwrap_or(false);
    let parallel_ok = params
        .get("p")
        .and_then(|v| v.decimal().ok())
        .map(|p| u64::from(p) >= ARGON2_PARALLELISM as u64)
        .unwrap_or(false);

    !(mem_ok && time_ok && parallel_ok)
}

/// Hash a password using hardened Argon2id parameters
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = strong_argon2()?;
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow!("failed to hash password: {}", e))?
        .to_string();
    Ok(password_hash)
}

/// Verify a password against a hash and signal when rehashing is needed.
///
/// This is the **single canonical entry point** for all password verification in the system.
/// Do not implement alternative verification paths.
///
/// # Security Properties
///
/// - **Constant-time comparison**: Uses Argon2's built-in `subtle::ConstantTimeEq` for
///   timing-safe password verification, preventing timing side-channel attacks.
/// - **Timing-consistent failures**: Executes a hardened Argon2 hash on invalid formats
///   and failed verifications to prevent timing oracle attacks that could reveal whether
///   a hash format is valid.
/// - **Legacy support**: Handles bcrypt hashes (`$2a$`, `$2b$`, `$2y$`) with automatic
///   upgrade signaling via `needs_rehash`.
///
/// # Timing Behavior
///
/// All code paths execute approximately the same amount of cryptographic work:
/// - **Valid format, correct password**: Argon2 verify (constant-time internally)
/// - **Valid format, wrong password**: Argon2 verify + hardened hash
/// - **Invalid format**: Hardened hash (timing-equivalent to verification)
/// - **Legacy bcrypt**: bcrypt verify (constant-time internally)
///
/// # Returns
///
/// - `valid`: Whether the password matches the hash
/// - `needs_rehash`: Whether the hash should be upgraded (legacy format or weak params)
pub fn verify_password(password: &str, hash: &str) -> Result<PasswordVerification> {
    let parsed_hash = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => {
            // Try legacy bcrypt directly when password-hash parsing fails (bcrypt feature not enabled there)
            if hash.starts_with("$2a$") || hash.starts_with("$2b$") || hash.starts_with("$2y$") {
                let valid = bcrypt::verify(password, hash).unwrap_or(false);
                return Ok(PasswordVerification {
                    valid,
                    needs_rehash: valid,
                });
            }

            // Consume time comparable to a real verification
            let _ = strong_argon2()?
                .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng));
            return Ok(PasswordVerification {
                valid: false,
                needs_rehash: false,
            });
        }
    };

    // Legacy bcrypt support (upgrade on next successful login)
    let alg = parsed_hash.algorithm.as_str().to_ascii_lowercase();
    if alg == "bcrypt" || alg.starts_with("2a") || alg.starts_with("2b") || alg.starts_with("2y") {
        let valid = bcrypt::verify(password, hash).unwrap_or(false);
        return Ok(PasswordVerification {
            valid,
            needs_rehash: valid,
        });
    }

    // Argon2 verification path
    let argon2 = Argon2::default();
    let valid = argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok();

    let needs_rehash = valid && argon2_needs_upgrade(&parsed_hash);

    if !valid {
        // Perform a hardened hash to keep timing consistent with failures
        let _ =
            strong_argon2()?.hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng));
    }

    Ok(PasswordVerification {
        valid,
        needs_rehash,
    })
}

/// Generate a JWT token with Ed25519 signing
pub fn generate_token_ed25519(
    user_id: &str,
    email: &str,
    role: &str,
    tenant_id: &str,
    keypair: &Keypair,
    token_ttl_seconds: u64,
) -> Result<String> {
    let kid = derive_kid_from_str(&encode_ed25519_public_key_pem(
        &keypair.public_key().to_bytes(),
    ));
    generate_token_ed25519_with_admin_tenants_mfa(
        user_id,
        email,
        role,
        tenant_id,
        &[],
        keypair,
        token_ttl_seconds,
        None,
        Some(&kid),
    )
}

/// Generate a JWT token with Ed25519 signing and admin tenant access
pub fn generate_token_ed25519_with_admin_tenants(
    user_id: &str,
    email: &str,
    role: &str,
    tenant_id: &str,
    admin_tenants: &[String],
    keypair: &Keypair,
    token_ttl_seconds: u64,
) -> Result<String> {
    let kid = derive_kid_from_str(&encode_ed25519_public_key_pem(
        &keypair.public_key().to_bytes(),
    ));
    generate_token_ed25519_with_admin_tenants_mfa(
        user_id,
        email,
        role,
        tenant_id,
        admin_tenants,
        keypair,
        token_ttl_seconds,
        None,
        Some(&kid),
    )
}

/// Generate a JWT token with Ed25519 signing, admin tenants, and optional MFA level
#[allow(clippy::too_many_arguments)]
pub fn generate_token_ed25519_with_admin_tenants_mfa(
    user_id: &str,
    email: &str,
    role: &str,
    tenant_id: &str,
    admin_tenants: &[String],
    keypair: &Keypair,
    token_ttl_seconds: u64,
    mfa_level: Option<&str>,
    kid: Option<&str>,
) -> Result<String> {
    let now = Utc::now();
    let ttl_secs = if token_ttl_seconds == 0 {
        8 * 3600
    } else {
        token_ttl_seconds
    };
    let exp = now + Duration::seconds(ttl_secs as i64); // 8 hour expiry
    let nbf = now; // Token valid immediately

    // Generate unique JWT ID using BLAKE3
    let jti = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(user_id.as_bytes());
        hasher.update(&now.timestamp().to_le_bytes());
        hasher.update(tenant_id.as_bytes());
        // Add a per-token nonce to avoid collisions when tokens are minted in the same second
        let nonce = TypedId::new(IdPrefix::Tok);
        hasher.update(nonce.as_str().as_bytes());
        hex::encode(hasher.finalize().as_bytes())
    };

    let claims = AccessClaims {
        sub: user_id.to_string(),
        email: email.to_string(),
        role: role.to_string(),
        roles: vec![role.to_string()], // Initialize roles with the primary role
        tenant_id: tenant_id.to_string(),
        admin_tenants: admin_tenants.to_vec(),
        exp: exp.timestamp(),
        iat: now.timestamp(),
        jti,
        nbf: nbf.timestamp(),
        device_id: None,
        session_id: None,
        mfa_level: mfa_level.map(|s| s.to_string()),
        rot_id: None,
        iss: JWT_ISSUER.to_string(),
        auth_mode: AuthMode::BearerToken,
        principal_type: Some(PrincipalType::User),
    };

    // Use Ed25519 algorithm for signing
    let mut header = Header::new(JwtAlgorithm::EdDSA);
    header.typ = Some("JWT".to_string());
    if let Some(k) = kid {
        header.kid = Some(k.to_string());
    }

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
        0x30, 0x2a, // SEQUENCE, length 42
        0x30, 0x05, // SEQUENCE (AlgorithmIdentifier)
        0x06, 0x03, // OID, length 3
        0x2b, 0x65, 0x70, // OID: 1.3.101.112 (Ed25519)
        0x03, 0x21, // BIT STRING, length 33 (32 bytes + 1 leading 0x00)
        0x00, // No unused bits in the bit string
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
        result.push(if idx3 == 64 {
            '='
        } else {
            BASE64_CHARS[idx3] as char
        });
        result.push(if idx4 == 64 {
            '='
        } else {
            BASE64_CHARS[idx4] as char
        });
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
    token_ttl_seconds: u64,
) -> Result<String> {
    let kid = derive_kid_from_bytes(secret);
    generate_token_with_admin_tenants_mfa(
        user_id,
        email,
        role,
        tenant_id,
        &[],
        secret,
        token_ttl_seconds,
        None,
        Some(&kid),
    )
}

/// Generate a JWT token with admin tenant access (HMAC-SHA256 fallback)
pub fn generate_token_with_admin_tenants(
    user_id: &str,
    email: &str,
    role: &str,
    tenant_id: &str,
    admin_tenants: &[String],
    secret: &[u8],
    token_ttl_seconds: u64,
) -> Result<String> {
    let kid = derive_kid_from_bytes(secret);
    generate_token_with_admin_tenants_mfa(
        user_id,
        email,
        role,
        tenant_id,
        admin_tenants,
        secret,
        token_ttl_seconds,
        None,
        Some(&kid),
    )
}

/// Generate a JWT token with admin tenant access (HMAC) and optional MFA level
#[allow(clippy::too_many_arguments)]
pub fn generate_token_with_admin_tenants_mfa(
    user_id: &str,
    email: &str,
    role: &str,
    tenant_id: &str,
    admin_tenants: &[String],
    secret: &[u8],
    token_ttl_seconds: u64,
    mfa_level: Option<&str>,
    kid: Option<&str>,
) -> Result<String> {
    let now = Utc::now();
    let ttl_secs = if token_ttl_seconds == 0 {
        8 * 3600
    } else {
        token_ttl_seconds
    };
    let exp = now + Duration::seconds(ttl_secs as i64); // 8 hour expiry
    let nbf = now; // Token valid immediately

    // Generate unique JWT ID using BLAKE3
    let jti = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(user_id.as_bytes());
        hasher.update(&now.timestamp().to_le_bytes());
        hasher.update(tenant_id.as_bytes());
        hex::encode(hasher.finalize().as_bytes())
    };

    let claims = AccessClaims {
        sub: user_id.to_string(),
        email: email.to_string(),
        role: role.to_string(),
        roles: vec![role.to_string()], // Initialize roles with the primary role
        tenant_id: tenant_id.to_string(),
        admin_tenants: admin_tenants.to_vec(),
        exp: exp.timestamp(),
        iat: now.timestamp(),
        jti,
        nbf: nbf.timestamp(),
        device_id: None,
        session_id: None,
        mfa_level: mfa_level.map(|s| s.to_string()),
        rot_id: None,
        iss: JWT_ISSUER.to_string(),
        auth_mode: AuthMode::BearerToken,
        principal_type: Some(PrincipalType::User),
    };

    let mut header = Header::default();
    if let Some(k) = kid {
        header.kid = Some(k.to_string());
    }

    let token = encode(&header, &claims, &EncodingKey::from_secret(secret))?;
    Ok(token)
}

/// Validate and decode a JWT token with Ed25519
pub fn validate_token_ed25519(
    token: &str,
    keys: &[(String, String)],
    fallback_pem: &str,
) -> Result<Claims> {
    validate_access_token_ed25519(token, keys, fallback_pem)
}

/// Validate and decode a JWT token (HMAC-SHA256 fallback)
pub fn validate_token(
    token: &str,
    keys: &[(String, Vec<u8>)],
    fallback_secret: &[u8],
) -> Result<Claims> {
    let secret = select_hmac_key(token, keys, fallback_secret);
    let mut validation = Validation::default();
    validation.validate_nbf = true; // Validate "not before" timestamp
    validation.leeway = 60; // SECURITY: 60 second clock skew tolerance
    validation.set_issuer(&[JWT_ISSUER]);

    let token_data = decode::<Claims>(token, &DecodingKey::from_secret(secret), &validation)?;
    Ok(token_data.claims)
}

/// Issue an access token with short TTL (default 15m)
#[allow(clippy::too_many_arguments)]
pub fn issue_access_token_ed25519(
    user_id: &str,
    email: &str,
    role: &str,
    roles: &[String],
    tenant_id: &str,
    admin_tenants: &[String],
    device_id: Option<&str>,
    session_id: &str,
    mfa_level: Option<&str>,
    keypair: &Keypair,
    override_ttl_secs: Option<u64>,
) -> Result<String> {
    let now = Utc::now();
    let ttl = override_ttl_secs.unwrap_or(DEFAULT_ACCESS_TOKEN_TTL_SECS);
    let exp = now + Duration::seconds(ttl as i64);
    let nbf = now;

    // Align JWT ID with session identifier so revocation and session checks share the same key.
    let jti = session_id.to_string();

    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        role: role.to_string(),
        roles: if roles.is_empty() {
            vec![role.to_string()]
        } else {
            roles.to_vec()
        },
        tenant_id: tenant_id.to_string(),
        admin_tenants: admin_tenants.to_vec(),
        device_id: device_id.map(|s| s.to_string()),
        session_id: Some(session_id.to_string()),
        mfa_level: mfa_level.map(|s| s.to_string()),
        rot_id: None,
        exp: exp.timestamp(),
        iat: now.timestamp(),
        jti,
        nbf: nbf.timestamp(),
        iss: JWT_ISSUER.to_string(),
        auth_mode: AuthMode::BearerToken,
        principal_type: Some(PrincipalType::User),
    };

    let mut header = Header::new(JwtAlgorithm::EdDSA);
    header.typ = Some("JWT".to_string());
    header.kid = Some(derive_kid_from_str(&encode_ed25519_public_key_pem(
        &keypair.public_key().to_bytes(),
    )));

    let raw_key = keypair.to_bytes();
    let der_key = encode_ed25519_pkcs8_der(&raw_key);
    encode(&header, &claims, &EncodingKey::from_ed_der(&der_key)).map_err(Into::into)
}

/// Issue an access token using HMAC-SHA256 (debug/dev only)
#[allow(clippy::too_many_arguments)]
pub fn issue_access_token_hmac(
    user_id: &str,
    email: &str,
    role: &str,
    roles: &[String],
    tenant_id: &str,
    admin_tenants: &[String],
    device_id: Option<&str>,
    session_id: &str,
    mfa_level: Option<&str>,
    secret: &[u8],
    override_ttl_secs: Option<u64>,
) -> Result<String> {
    let now = Utc::now();
    let ttl = override_ttl_secs.unwrap_or(DEFAULT_ACCESS_TOKEN_TTL_SECS);
    let exp = now + Duration::seconds(ttl as i64);
    let nbf = now;

    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        role: role.to_string(),
        roles: if roles.is_empty() {
            vec![role.to_string()]
        } else {
            roles.to_vec()
        },
        tenant_id: tenant_id.to_string(),
        admin_tenants: admin_tenants.to_vec(),
        device_id: device_id.map(|s| s.to_string()),
        session_id: Some(session_id.to_string()),
        mfa_level: mfa_level.map(|s| s.to_string()),
        rot_id: None,
        exp: exp.timestamp(),
        iat: now.timestamp(),
        jti: session_id.to_string(),
        nbf: nbf.timestamp(),
        iss: JWT_ISSUER.to_string(),
        auth_mode: AuthMode::BearerToken,
        principal_type: Some(PrincipalType::User),
    };

    let mut header = Header::new(JwtAlgorithm::HS256);
    header.typ = Some("JWT".to_string());
    header.kid = Some(derive_kid_from_bytes(secret));
    encode(&header, &claims, &EncodingKey::from_secret(secret)).map_err(Into::into)
}

/// Issue a refresh token with session TTL (default 2h) embedding device_id + rot_id
#[allow(clippy::too_many_arguments)]
pub fn issue_refresh_token_ed25519(
    user_id: &str,
    tenant_id: &str,
    roles: &[String],
    device_id: Option<&str>,
    session_id: &str,
    rot_id: &str,
    keypair: &Keypair,
    override_ttl_secs: Option<u64>,
) -> Result<String> {
    let ttl = override_ttl_secs.unwrap_or(DEFAULT_SESSION_TTL_SECS);
    let (claims, _exp_ts) = build_refresh_claims(
        user_id, tenant_id, roles, device_id, session_id, rot_id, ttl,
    );
    let mut header = Header::new(JwtAlgorithm::EdDSA);
    header.typ = Some("JWT".to_string());
    header.kid = Some(derive_kid_from_str(&encode_ed25519_public_key_pem(
        &keypair.public_key().to_bytes(),
    )));

    let raw_key = keypair.to_bytes();
    let der_key = encode_ed25519_pkcs8_der(&raw_key);
    encode(&header, &claims, &EncodingKey::from_ed_der(&der_key)).map_err(Into::into)
}

/// Issue a refresh token using HMAC-SHA256 (debug/dev only)
#[allow(clippy::too_many_arguments)]
pub fn issue_refresh_token_hmac(
    user_id: &str,
    tenant_id: &str,
    roles: &[String],
    device_id: Option<&str>,
    session_id: &str,
    rot_id: &str,
    secret: &[u8],
    override_ttl_secs: Option<u64>,
) -> Result<String> {
    let ttl = override_ttl_secs.unwrap_or(DEFAULT_SESSION_TTL_SECS);
    let (claims, _exp_ts) = build_refresh_claims(
        user_id, tenant_id, roles, device_id, session_id, rot_id, ttl,
    );
    let mut header = Header::new(JwtAlgorithm::HS256);
    header.typ = Some("JWT".to_string());
    header.kid = Some(derive_kid_from_bytes(secret));
    encode(&header, &claims, &EncodingKey::from_secret(secret)).map_err(Into::into)
}

fn build_refresh_claims(
    user_id: &str,
    tenant_id: &str,
    roles: &[String],
    device_id: Option<&str>,
    session_id: &str,
    rot_id: &str,
    ttl_secs: u64,
) -> (RefreshClaims, i64) {
    let now = Utc::now();
    let exp_ts = (now + Duration::seconds(ttl_secs as i64)).timestamp();
    (
        RefreshClaims {
            sub: user_id.to_string(),
            tenant_id: tenant_id.to_string(),
            roles: roles.to_vec(),
            device_id: device_id.map(|s| s.to_string()),
            session_id: session_id.to_string(),
            rot_id: rot_id.to_string(),
            exp: exp_ts,
            iat: now.timestamp(),
            iss: JWT_ISSUER.to_string(),
        },
        exp_ts,
    )
}

fn select_ed25519_key<'a>(token: &str, keys: &'a [(String, String)], fallback: &'a str) -> &'a str {
    if let Ok(header) = jsonwebtoken::decode_header(token) {
        if let Some(kid) = header.kid {
            if let Some((_, pem)) = keys.iter().find(|(k, _)| *k == kid) {
                return pem;
            }
        }
    }
    fallback
}

fn select_hmac_key<'a>(token: &str, keys: &'a [(String, Vec<u8>)], fallback: &'a [u8]) -> &'a [u8] {
    if let Ok(header) = jsonwebtoken::decode_header(token) {
        if let Some(kid) = header.kid {
            if let Some((_, secret)) = keys.iter().find(|(k, _)| *k == kid) {
                return secret.as_slice();
            }
        }
    }
    fallback
}

/// Validate and decode access token (Ed25519) with kid-aware key selection
pub fn validate_access_token_ed25519(
    token: &str,
    keys: &[(String, String)],
    fallback_pem: &str,
) -> Result<Claims> {
    let pem = select_ed25519_key(token, keys, fallback_pem);
    let mut validation = Validation::new(JwtAlgorithm::EdDSA);
    validation.validate_nbf = true;
    validation.leeway = 60;
    validation.set_issuer(&[JWT_ISSUER]);
    decode::<Claims>(
        token,
        &DecodingKey::from_ed_pem(pem.as_bytes())?,
        &validation,
    )
    .map(|td| td.claims)
    .map_err(Into::into)
}

/// Validate and decode refresh token (Ed25519) with kid-aware key selection
pub fn validate_refresh_token_ed25519(
    token: &str,
    keys: &[(String, String)],
    fallback_pem: &str,
) -> Result<RefreshClaims> {
    let pem = select_ed25519_key(token, keys, fallback_pem);
    let mut validation = Validation::new(JwtAlgorithm::EdDSA);
    validation.validate_nbf = true;
    validation.leeway = 60;
    validation.set_issuer(&[JWT_ISSUER]);
    decode::<RefreshClaims>(
        token,
        &DecodingKey::from_ed_pem(pem.as_bytes())?,
        &validation,
    )
    .map(|td| td.claims)
    .map_err(Into::into)
}

/// Validate and decode refresh token (HMAC-SHA256) with kid-aware key selection
pub fn validate_refresh_token_hmac(
    token: &str,
    keys: &[(String, Vec<u8>)],
    fallback_secret: &[u8],
) -> Result<RefreshClaims> {
    let secret = select_hmac_key(token, keys, fallback_secret);
    let mut validation = Validation::new(JwtAlgorithm::HS256);
    validation.validate_nbf = true;
    validation.leeway = 60;
    validation.set_issuer(&[JWT_ISSUER]);
    decode::<RefreshClaims>(token, &DecodingKey::from_secret(secret), &validation)
        .map(|td| td.claims)
        .map_err(Into::into)
}

/// Issue refresh token and persist session metadata to KV.
#[allow(clippy::too_many_arguments)]
pub async fn issue_refresh_token_ed25519_with_kv(
    repo: &AuthSessionKvRepository,
    user_id: &str,
    tenant_id: &str,
    roles: &[String],
    device_id: Option<&str>,
    session_id: &str,
    rot_id: &str,
    keypair: &Keypair,
    override_ttl_secs: Option<u64>,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(String, i64, String)> {
    let ttl = override_ttl_secs.unwrap_or(DEFAULT_SESSION_TTL_SECS);
    let (claims, exp_ts) = build_refresh_claims(
        user_id, tenant_id, roles, device_id, session_id, rot_id, ttl,
    );

    let mut header = Header::new(JwtAlgorithm::EdDSA);
    header.typ = Some("JWT".to_string());
    let refresh_kid = derive_kid_from_str(&encode_ed25519_public_key_pem(
        &keypair.public_key().to_bytes(),
    ));
    header.kid = Some(refresh_kid);

    let raw_key = keypair.to_bytes();
    let der_key = encode_ed25519_pkcs8_der(&raw_key);
    let token = encode(&header, &claims, &EncodingKey::from_ed_der(&der_key))?;
    let refresh_hash = hash_token_hex(&token);

    repo.create_session_with_device(
        session_id,
        user_id,
        tenant_id,
        device_id,
        Some(rot_id),
        exp_ts,
        Some(&refresh_hash),
        false,
        ip_address,
        user_agent,
    )
    .await?;

    Ok((token, exp_ts, refresh_hash))
}

/// Issue refresh token (HMAC) and persist session metadata to KV.
#[allow(clippy::too_many_arguments)]
pub async fn issue_refresh_token_hmac_with_kv(
    repo: &AuthSessionKvRepository,
    user_id: &str,
    tenant_id: &str,
    roles: &[String],
    device_id: Option<&str>,
    session_id: &str,
    rot_id: &str,
    secret: &[u8],
    override_ttl_secs: Option<u64>,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(String, i64, String)> {
    let ttl = override_ttl_secs.unwrap_or(DEFAULT_SESSION_TTL_SECS);
    let (claims, exp_ts) = build_refresh_claims(
        user_id, tenant_id, roles, device_id, session_id, rot_id, ttl,
    );

    let mut header = Header::new(JwtAlgorithm::HS256);
    header.typ = Some("JWT".to_string());
    header.kid = Some(derive_kid_from_bytes(secret));
    let token = encode(&header, &claims, &EncodingKey::from_secret(secret))?;
    let refresh_hash = hash_token_hex(&token);

    repo.create_session_with_device(
        session_id,
        user_id,
        tenant_id,
        device_id,
        Some(rot_id),
        exp_ts,
        Some(&refresh_hash),
        false,
        ip_address,
        user_agent,
    )
    .await?;

    Ok((token, exp_ts, refresh_hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_token_includes_session_device_and_issuer() {
        let keypair = Keypair::generate();
        let session_id = "sess-123";
        let device_id = Some("device-abc");
        let roles = vec!["admin".to_string(), "dev".to_string()];

        let token = issue_access_token_ed25519(
            "user-1",
            "user@example.com",
            "admin",
            &roles,
            "tenant-a",
            &[],
            device_id,
            session_id,
            Some("strong"),
            &keypair,
            Some(60),
        )
        .expect("access token");

        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);
        let claims =
            validate_access_token_ed25519(&token, &[(kid, public_pem.clone())], &public_pem)
                .expect("validate access");

        assert_eq!(claims.session_id.as_deref(), Some(session_id));
        assert_eq!(claims.device_id.as_deref(), device_id);
        assert_eq!(claims.tenant_id, "tenant-a");
        assert_eq!(claims.iss, JWT_ISSUER);
        assert!(claims.exp > claims.iat);
        assert!(claims.roles.contains(&"admin".to_string()));
    }

    #[test]
    fn access_token_sets_session_id_and_principal_type() {
        let keypair = Keypair::generate();
        let session_id = "sess-principal";
        let roles = vec!["admin".to_string()];

        let token = issue_access_token_ed25519(
            "user-claims",
            "user@example.com",
            "admin",
            &roles,
            "tenant-check",
            &[],
            None,
            session_id,
            None,
            &keypair,
            Some(120),
        )
        .expect("access token");

        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);
        let claims =
            validate_access_token_ed25519(&token, &[(kid, public_pem.clone())], &public_pem)
                .expect("validate access");

        assert_eq!(claims.session_id.as_deref(), Some(session_id));
        assert_eq!(claims.jti, session_id.to_string());
        assert_eq!(claims.principal_type, Some(PrincipalType::User));
    }

    #[test]
    fn refresh_token_carries_rotation_and_device() {
        let keypair = Keypair::generate();
        let roles = vec!["viewer".to_string()];

        let token = issue_refresh_token_ed25519(
            "user-2",
            "tenant-b",
            &roles,
            Some("device-xyz"),
            "sess-456",
            "rot-1",
            &keypair,
            Some(120),
        )
        .expect("refresh token");

        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);
        let claims =
            validate_refresh_token_ed25519(&token, &[(kid, public_pem.clone())], &public_pem)
                .expect("validate refresh");

        assert_eq!(claims.session_id, "sess-456");
        assert_eq!(claims.rot_id, "rot-1");
        assert_eq!(claims.device_id.as_deref(), Some("device-xyz"));
        assert_eq!(claims.tenant_id, "tenant-b");
        assert_eq!(claims.iss, JWT_ISSUER);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn access_token_invalid_issuer_rejected() {
        let keypair = Keypair::generate();
        let now = Utc::now();
        let mut claims = Claims {
            sub: "user-x".to_string(),
            email: "x@example.com".to_string(),
            role: "viewer".to_string(),
            roles: vec!["viewer".to_string()],
            tenant_id: "tenant-z".to_string(),
            admin_tenants: vec![],
            device_id: None,
            session_id: Some("sess-x".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: (now + Duration::seconds(60)).timestamp(),
            iat: now.timestamp(),
            jti: "jti-1".to_string(),
            nbf: now.timestamp(),
            iss: "unexpected-issuer".to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        };

        let mut header = Header::new(JwtAlgorithm::EdDSA);
        header.typ = Some("JWT".to_string());
        let der_key = encode_ed25519_pkcs8_der(&keypair.to_bytes());
        let token = encode(&header, &claims, &EncodingKey::from_ed_der(&der_key)).unwrap();

        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);
        let result =
            validate_access_token_ed25519(&token, &[(kid, public_pem.clone())], &public_pem);
        assert!(result.is_err());

        // ensure we don't mutate original claims in test
        claims.iss = JWT_ISSUER.to_string();
    }

    #[test]
    fn hmac_access_token_carries_session_and_device() {
        let roles = vec!["operator".to_string()];
        let token = issue_access_token_hmac(
            "user-hmac",
            "hmac@example.com",
            "operator",
            &roles,
            "tenant-h",
            &[],
            Some("device-hmac"),
            "sess-hmac",
            None,
            b"secret-key",
            Some(120),
        )
        .expect("hmac access token");

        let hmac_keys = vec![(derive_kid_from_bytes(b"secret-key"), b"secret-key".to_vec())];
        let claims =
            validate_token(&token, &hmac_keys, b"secret-key").expect("validate hmac token");
        assert_eq!(claims.session_id.as_deref(), Some("sess-hmac"));
        assert_eq!(claims.device_id.as_deref(), Some("device-hmac"));
        assert_eq!(claims.tenant_id, "tenant-h");
    }

    #[test]
    fn ed25519_kid_selects_matching_key() {
        let keypair_primary = Keypair::generate();
        let keypair_secondary = Keypair::generate();
        let roles = vec!["viewer".to_string()];

        let token = issue_access_token_ed25519(
            "user-ed",
            "ed@example.com",
            "viewer",
            &roles,
            "tenant-ed",
            &[],
            None,
            "sess-ed",
            None,
            &keypair_primary,
            Some(60),
        )
        .expect("access token");

        let pem_primary = encode_ed25519_public_key_pem(&keypair_primary.public_key().to_bytes());
        let pem_secondary =
            encode_ed25519_public_key_pem(&keypair_secondary.public_key().to_bytes());
        let kid_primary = derive_kid_from_str(&pem_primary);
        let kid_secondary = derive_kid_from_str(&pem_secondary);

        // Provide keys out of order to ensure kid selection picks the correct one.
        let keys = vec![
            (kid_secondary.clone(), pem_secondary.clone()),
            (kid_primary.clone(), pem_primary.clone()),
        ];

        let claims =
            validate_access_token_ed25519(&token, &keys, &pem_secondary).expect("validate access");
        assert_eq!(claims.tenant_id, "tenant-ed");
    }

    #[test]
    fn hmac_kid_selects_matching_secret() {
        let roles = vec!["viewer".to_string()];
        let old_secret = b"old-secret";
        let new_secret = b"new-secret";

        let token = issue_access_token_hmac(
            "user-hmac-rot",
            "hmac@example.com",
            "viewer",
            &roles,
            "tenant-hmac-rot",
            &[],
            None,
            "sess-rot",
            None,
            old_secret,
            Some(60),
        )
        .expect("access token");

        let keys = vec![
            (derive_kid_from_bytes(new_secret), new_secret.to_vec()),
            (derive_kid_from_bytes(old_secret), old_secret.to_vec()),
        ];

        let claims =
            validate_token(&token, &keys, new_secret).expect("validate rotated hmac token");
        assert_eq!(claims.tenant_id, "tenant-hmac-rot");
    }

    #[test]
    fn ed25519_token_without_kid_uses_fallback() {
        let keypair = Keypair::generate();
        let now = Utc::now();
        let claims = Claims {
            sub: "user-kidless".to_string(),
            email: "kidless@example.com".to_string(),
            role: "viewer".to_string(),
            roles: vec!["viewer".to_string()],
            tenant_id: "tenant-kidless".to_string(),
            admin_tenants: vec![],
            device_id: None,
            session_id: Some("sess-kidless".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: (now + Duration::seconds(60)).timestamp(),
            iat: now.timestamp(),
            jti: "jti-kidless".to_string(),
            nbf: now.timestamp(),
            iss: JWT_ISSUER.to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        };

        let mut header = Header::new(JwtAlgorithm::EdDSA);
        header.typ = Some("JWT".to_string());
        // Intentionally omit kid to exercise fallback path
        let der_key = encode_ed25519_pkcs8_der(&keypair.to_bytes());
        let token = encode(&header, &claims, &EncodingKey::from_ed_der(&der_key)).unwrap();

        let public_pem = encode_ed25519_public_key_pem(&keypair.public_key().to_bytes());
        let kid = derive_kid_from_str(&public_pem);
        let validated =
            validate_access_token_ed25519(&token, &[(kid, public_pem.clone())], &public_pem)
                .expect("validate without kid");

        assert_eq!(validated.tenant_id, "tenant-kidless");
        assert_eq!(validated.session_id.as_deref(), Some("sess-kidless"));
    }

    #[test]
    fn ed25519_secondary_key_validates_with_kid() {
        let primary = Keypair::generate();
        let secondary = Keypair::generate();
        let roles = vec!["viewer".to_string()];

        let token = issue_access_token_ed25519(
            "user-secondary",
            "secondary@example.com",
            "viewer",
            &roles,
            "tenant-secondary",
            &[],
            None,
            "sess-secondary",
            None,
            &secondary,
            Some(120),
        )
        .expect("secondary access token");

        let pem_primary = encode_ed25519_public_key_pem(&primary.public_key().to_bytes());
        let pem_secondary = encode_ed25519_public_key_pem(&secondary.public_key().to_bytes());

        let keys = vec![
            (derive_kid_from_str(&pem_primary), pem_primary.clone()),
            (derive_kid_from_str(&pem_secondary), pem_secondary.clone()),
        ];

        let claims =
            validate_access_token_ed25519(&token, &keys, &pem_primary).expect("validate secondary");
        assert_eq!(claims.tenant_id, "tenant-secondary");
    }

    #[test]
    fn ed25519_token_with_removed_kid_is_rejected() {
        let old_key = Keypair::generate();
        let new_key = Keypair::generate();
        let roles = vec!["viewer".to_string()];

        let token = issue_access_token_ed25519(
            "user-old",
            "old@example.com",
            "viewer",
            &roles,
            "tenant-old",
            &[],
            None,
            "sess-old",
            None,
            &old_key,
            Some(60),
        )
        .expect("old access token");

        let pem_new = encode_ed25519_public_key_pem(&new_key.public_key().to_bytes());
        let keys = vec![(derive_kid_from_str(&pem_new), pem_new.clone())];

        let result = validate_access_token_ed25519(&token, &keys, &pem_new);
        assert!(
            result.is_err(),
            "token signed with removed kid should fail validation"
        );
    }
}

/// Refresh a JWT token (generate new token with updated expiry)
pub fn refresh_token(claims: &Claims, keypair: &Keypair, token_ttl_seconds: u64) -> Result<String> {
    generate_token_ed25519(
        &claims.sub,
        &claims.email,
        &claims.role,
        &claims.tenant_id,
        keypair,
        token_ttl_seconds,
    )
}

/// Check if a token is about to expire (within 1 hour)
pub fn token_needs_refresh(claims: &Claims) -> bool {
    let now = Utc::now().timestamp();
    let time_until_expiry = claims.exp - now;
    time_until_expiry < 3600 // Less than 1 hour remaining
}
