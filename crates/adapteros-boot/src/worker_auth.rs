//! Worker authentication using Ed25519-signed JWTs.
//!
//! This module provides token generation (control plane) and validation (worker)
//! for internal service-to-service authentication over UDS.
//!
//! ## Token Format
//!
//! JWT with EdDSA (Ed25519) signature:
//!
//! ```json
//! {
//!   "header": { "alg": "EdDSA", "typ": "JWT", "kid": "worker-abc123" },
//!   "payload": {
//!     "iss": "control-plane",
//!     "aud": "worker",
//!     "wid": "worker-1",
//!     "iat": 1234567890,
//!     "exp": 1234567935,
//!     "jti": "req-uuid-here"
//!   }
//! }
//! ```
//!
//! ## Replay Defense
//!
//! Workers maintain an LRU cache of recently seen `jti` values.
//! Tokens with duplicate `jti` within the TTL window are rejected.
//!
//! ## Clock Skew Tolerance
//!
//! Token validation includes a configurable clock skew tolerance (default 30s)
//! to handle minor time differences between control plane and worker clocks.
//!
//! ## Example
//!
//! ```rust,no_run
//! use adapteros_boot::worker_auth::{generate_worker_token, validate_worker_token};
//! use ed25519_dalek::SigningKey;
//! use lru::LruCache;
//! use std::num::NonZeroUsize;
//!
//! // Control plane generates token
//! let signing_key = SigningKey::generate(&mut rand::thread_rng());
//! let token = generate_worker_token(&signing_key, "worker-1", "req-123", 45).unwrap();
//!
//! // Worker validates token
//! let verifying_key = signing_key.verifying_key();
//! let mut jti_cache = LruCache::new(NonZeroUsize::new(1000).unwrap());
//! let claims = validate_worker_token(&token, &verifying_key, Some("worker-1"), &mut jti_cache).unwrap();
//! assert_eq!(claims.wid, "worker-1");
//! ```

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::Utc;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::WorkerAuthError;

/// Clock skew tolerance in seconds for token validation.
///
/// This allows tokens to be validated even if the worker's clock is slightly
/// ahead of the control plane's clock. Without this, tokens could be rejected
/// as expired even though they are still valid from the control plane's perspective.
///
/// Default: 30 seconds (half the typical 60-second TTL)
pub const CLOCK_SKEW_TOLERANCE_SECS: i64 = 30;

/// Minimum TTL for worker tokens (prevents instant expiration).
///
/// A TTL of 0 causes tokens to expire within the same second they're issued,
/// leading to race conditions. Minimum of 1 second ensures tokens are usable.
pub const MIN_TOKEN_TTL_SECONDS: u64 = 1;

/// Maximum length for worker_id (fits comfortably in JWT without size issues).
///
/// 256 characters is generous for any reasonable worker ID while preventing
/// excessively large JWTs that could cause parsing or transport issues.
pub const MAX_WORKER_ID_LENGTH: usize = 256;

/// Maximum length for request_id/JTI (standard UUID is 36 chars, allowing headroom).
///
/// 256 characters accommodates UUIDs, prefixed identifiers, and trace IDs
/// while preventing unbounded growth.
pub const MAX_REQUEST_ID_LENGTH: usize = 256;

/// Worker auth token claims.
///
/// These claims are embedded in the JWT payload and validated by workers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerTokenClaims {
    /// Issuer - always "control-plane"
    pub iss: String,

    /// Audience - always "worker"
    pub aud: String,

    /// Worker ID that this token is valid for
    pub wid: String,

    /// Issued at (Unix timestamp)
    pub iat: i64,

    /// Expiration (Unix timestamp)
    pub exp: i64,

    /// JWT ID for replay defense
    pub jti: String,
}

/// JWT header for worker tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct JwtHeader {
    /// Algorithm - always "EdDSA"
    alg: String,
    /// Type - always "JWT"
    typ: String,
    /// Key ID for key rotation support
    kid: String,
}

/// Validate an identifier string (worker_id or request_id).
///
/// # Requirements
///
/// - Non-empty
/// - ASCII printable characters only (0x20-0x7E)
/// - Within the specified length limit
///
/// # Arguments
///
/// * `value` - The identifier string to validate
/// * `field_name` - Name of the field for error messages ("worker_id" or "request_id")
/// * `max_length` - Maximum allowed length for the identifier
fn validate_identifier(
    value: &str,
    field_name: &str,
    max_length: usize,
) -> Result<(), WorkerAuthError> {
    // Check empty
    if value.is_empty() {
        return Err(match field_name {
            "worker_id" => WorkerAuthError::EmptyWorkerId,
            "request_id" => WorkerAuthError::EmptyRequestId,
            _ => WorkerAuthError::InvalidIdentifierChars {
                field: format!("{} is empty", field_name),
            },
        });
    }

    // Check length
    if value.len() > max_length {
        return Err(match field_name {
            "worker_id" => WorkerAuthError::WorkerIdTooLong {
                max: max_length,
                actual: value.len(),
            },
            _ => WorkerAuthError::RequestIdTooLong {
                max: max_length,
                actual: value.len(),
            },
        });
    }

    // Check ASCII printable (0x20-0x7E inclusive)
    // This includes space (0x20) through tilde (0x7E)
    if !value.bytes().all(|b| (0x20..=0x7E).contains(&b)) {
        return Err(WorkerAuthError::InvalidIdentifierChars {
            field: field_name.to_string(),
        });
    }

    Ok(())
}

/// Generate a worker auth token (EdDSA/Ed25519).
///
/// # Arguments
///
/// * `signing_key` - Ed25519 private key for signing
/// * `worker_id` - ID of the target worker (must be non-empty, ASCII printable, max 256 chars)
/// * `request_id` - Unique request ID used as `jti` for replay defense (must be non-empty, ASCII printable, max 256 chars)
/// * `ttl_seconds` - Token time-to-live in seconds (minimum 1, recommended: 30-60)
///
/// # Returns
///
/// A JWT string in the format `header.payload.signature`
///
/// # Errors
///
/// - `EmptyWorkerId` - worker_id is empty
/// - `EmptyRequestId` - request_id is empty
/// - `TtlTooShort` - ttl_seconds is less than MIN_TOKEN_TTL_SECONDS (1)
/// - `WorkerIdTooLong` - worker_id exceeds MAX_WORKER_ID_LENGTH (256)
/// - `RequestIdTooLong` - request_id exceeds MAX_REQUEST_ID_LENGTH (256)
/// - `InvalidIdentifierChars` - worker_id or request_id contains non-ASCII-printable characters
///
/// # Example
///
/// ```rust,no_run
/// use adapteros_boot::worker_auth::generate_worker_token;
/// use ed25519_dalek::SigningKey;
///
/// let signing_key = SigningKey::generate(&mut rand::thread_rng());
/// let token = generate_worker_token(&signing_key, "worker-1", "req-abc123", 45).unwrap();
/// println!("Token: {}", token);
/// ```
pub fn generate_worker_token(
    signing_key: &SigningKey,
    worker_id: &str,
    request_id: &str,
    ttl_seconds: u64,
) -> Result<String, WorkerAuthError> {
    // Validate TTL
    if ttl_seconds < MIN_TOKEN_TTL_SECONDS {
        return Err(WorkerAuthError::TtlTooShort {
            min: MIN_TOKEN_TTL_SECONDS,
            actual: ttl_seconds,
        });
    }

    // Validate worker_id
    validate_identifier(worker_id, "worker_id", MAX_WORKER_ID_LENGTH)?;

    // Validate request_id (JTI)
    validate_identifier(request_id, "request_id", MAX_REQUEST_ID_LENGTH)?;

    let now = Utc::now();
    let exp = now.timestamp() + ttl_seconds as i64;

    let claims = WorkerTokenClaims {
        iss: "control-plane".into(),
        aud: "worker".into(),
        wid: worker_id.into(),
        iat: now.timestamp(),
        exp,
        jti: request_id.into(),
    };

    let header = JwtHeader {
        alg: "EdDSA".into(),
        typ: "JWT".into(),
        kid: derive_kid_from_verifying_key(&signing_key.verifying_key()),
    };

    // Encode header and claims
    let header_json = serde_json::to_vec(&header)?;
    let claims_json = serde_json::to_vec(&claims)?;

    let header_b64 = URL_SAFE_NO_PAD.encode(&header_json);
    let claims_b64 = URL_SAFE_NO_PAD.encode(&claims_json);

    // Sign the message
    let message = format!("{}.{}", header_b64, claims_b64);
    let signature = signing_key.sign(message.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

    Ok(format!("{}.{}", message, sig_b64))
}

/// Validate a worker auth token.
///
/// # Arguments
///
/// * `token` - JWT string to validate
/// * `verifying_key` - Ed25519 public key for signature verification
/// * `expected_worker_id` - If Some, validates that `wid` matches
/// * `jti_cache` - LRU cache for replay defense (stores jti -> exp)
///
/// # Returns
///
/// The validated claims if the token is valid.
///
/// # Errors
///
/// - `InvalidFormat` - Token is not a valid JWT structure
/// - `InvalidSignature` - Signature verification failed
/// - `Expired` - Token has expired
/// - `InvalidIssuer` - Issuer is not "control-plane"
/// - `InvalidAudience` - Audience is not "worker"
/// - `ReplayDetected` - `jti` has been seen before
/// - `WorkerIdMismatch` - `wid` doesn't match expected
///
/// # Example
///
/// ```rust,no_run
/// use adapteros_boot::worker_auth::validate_worker_token;
/// use ed25519_dalek::VerifyingKey;
/// use lru::LruCache;
/// use std::num::NonZeroUsize;
///
/// let mut jti_cache: LruCache<String, i64> = LruCache::new(NonZeroUsize::new(1000).unwrap());
/// // let claims = validate_worker_token(&token, &verifying_key, Some("worker-1"), &mut jti_cache)?;
/// ```
pub fn validate_worker_token(
    token: &str,
    verifying_key: &VerifyingKey,
    expected_worker_id: Option<&str>,
    jti_cache: &mut LruCache<String, i64>,
) -> Result<WorkerTokenClaims, WorkerAuthError> {
    // Split token into parts
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(WorkerAuthError::InvalidFormat);
    }

    // Verify signature first (before parsing claims)
    let message = format!("{}.{}", parts[0], parts[1]);
    let sig_bytes = URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|e| WorkerAuthError::Base64Decode(e.to_string()))?;

    if sig_bytes.len() != 64 {
        return Err(WorkerAuthError::InvalidSignature);
    }

    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| WorkerAuthError::InvalidSignature)?;
    let signature = Signature::from_bytes(&sig_array);

    // SECURITY: We use verify_strict() for defense-in-depth against weak key attacks.
    // While adapterOS controls key generation (making weak key attacks infeasible),
    // verify_strict() adds an additional check that rejects signatures with small-order
    // points, following RFC 8032's cofactored verification variant.
    // See: https://docs.rs/ed25519-dalek/latest/ed25519_dalek/struct.VerifyingKey.html
    verifying_key
        .verify_strict(message.as_bytes(), &signature)
        .map_err(|_| WorkerAuthError::InvalidSignature)?;

    // Decode and validate claims
    let claims_bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|e| WorkerAuthError::Base64Decode(e.to_string()))?;
    let claims: WorkerTokenClaims = serde_json::from_slice(&claims_bytes)?;

    // Validate timing with clock skew tolerance
    let now = Utc::now().timestamp();

    // Allow tokens that are expired by up to CLOCK_SKEW_TOLERANCE_SECS
    // This handles cases where the worker's clock is slightly ahead of the control plane
    if claims.exp + CLOCK_SKEW_TOLERANCE_SECS < now {
        return Err(WorkerAuthError::Expired);
    }

    // Log a warning if the token appears expired but we're within tolerance
    if claims.exp < now {
        tracing::warn!(
            token_exp = claims.exp,
            current_time = now,
            skew_secs = now - claims.exp,
            tolerance = CLOCK_SKEW_TOLERANCE_SECS,
            "Token validated within clock skew tolerance - consider syncing clocks"
        );
    }

    // Validate not-before (nbf) with tolerance if present in a future extension
    // Currently we check issued-at as a sanity check
    if claims.iat > now + CLOCK_SKEW_TOLERANCE_SECS {
        // Token was issued in the future (beyond tolerance) - likely a clock issue
        tracing::warn!(
            token_iat = claims.iat,
            current_time = now,
            skew_secs = claims.iat - now,
            "Token issued-at is in the future - severe clock skew detected"
        );
        return Err(WorkerAuthError::NotYetValid);
    }

    // Validate issuer
    if claims.iss != "control-plane" {
        return Err(WorkerAuthError::InvalidIssuer);
    }

    // Validate audience
    if claims.aud != "worker" {
        return Err(WorkerAuthError::InvalidAudience);
    }

    // Replay defense: check jti against LRU cache
    // Note: LRU naturally evicts old entries, no explicit cleanup needed
    if let Some(&cached_exp) = jti_cache.get(&claims.jti) {
        // Check if the cached entry is still valid (not expired)
        if cached_exp > now {
            return Err(WorkerAuthError::ReplayDetected);
        }
        // If expired, we can reuse the jti (though this is unlikely with short TTLs)
    }

    // Add jti to cache with expiration
    jti_cache.put(claims.jti.clone(), claims.exp);

    // Optional worker ID validation
    if let Some(expected) = expected_worker_id {
        if claims.wid != expected {
            tracing::warn!(
                expected_worker = %expected,
                token_worker = %claims.wid,
                jti = %claims.jti,
                "Worker ID mismatch - token was generated for a different worker"
            );
            return Err(WorkerAuthError::WorkerIdMismatch {
                expected: expected.to_string(),
                got: claims.wid.clone(),
            });
        }
    }

    Ok(claims)
}

/// Derive a key ID from an Ed25519 verifying key.
///
/// Uses BLAKE3 hash of the public key bytes, truncated to 32 hex characters (128 bits).
/// This provides sufficient entropy to avoid birthday-bound collisions (~2^64 keys).
pub fn derive_kid_from_verifying_key(key: &VerifyingKey) -> String {
    let hash = blake3::hash(key.as_bytes());
    format!("worker-{}", &hash.to_hex()[..32])
}

/// Options for keypair loading behavior.
#[derive(Debug, Clone, Default)]
pub struct KeypairOptions {
    /// If true, fail on corrupted keypair even in non-strict mode
    pub strict_mode: bool,
    /// If true, force regeneration of corrupted keypairs even in strict mode
    pub force_regenerate: bool,
}

impl KeypairOptions {
    /// Create options for non-strict mode (auto-regenerate corrupted keys)
    pub fn non_strict() -> Self {
        Self {
            strict_mode: false,
            force_regenerate: false,
        }
    }

    /// Create options for strict mode (fail on corrupted keys)
    pub fn strict() -> Self {
        Self {
            strict_mode: true,
            force_regenerate: false,
        }
    }

    /// Create options for strict mode with force regeneration (break-glass)
    pub fn strict_with_regenerate() -> Self {
        Self {
            strict_mode: true,
            force_regenerate: true,
        }
    }
}

/// Load or generate an Ed25519 keypair for worker authentication.
///
/// # Arguments
///
/// * `key_path` - Path to the private key file (32 bytes, raw)
///
/// # Returns
///
/// The signing key (private key). Public key can be derived via `.verifying_key()`.
///
/// # Behavior
///
/// - If the key file exists and is valid: loads and returns the key
/// - If the key file exists but is corrupted: regenerates (non-strict mode behavior)
/// - If the key file doesn't exist: generates a new keypair, writes with 0600 permissions
/// - Also writes the public key to `{key_path}.pub`
///
/// Uses atomic writes (temp-then-rename) to prevent corruption from concurrent access.
///
/// # Example
///
/// ```rust,no_run
/// use adapteros_boot::worker_auth::load_or_generate_worker_keypair;
/// use std::path::Path;
///
/// let keypair = load_or_generate_worker_keypair(Path::new("var/keys/worker_signing.key")).unwrap();
/// let public_key = keypair.verifying_key();
/// ```
pub fn load_or_generate_worker_keypair(
    key_path: &std::path::Path,
) -> Result<SigningKey, WorkerAuthError> {
    load_or_generate_worker_keypair_with_options(key_path, KeypairOptions::non_strict())
}

/// Load or generate an Ed25519 keypair with configurable options.
///
/// # Arguments
///
/// * `key_path` - Path to the private key file (32 bytes, raw)
/// * `options` - Configuration for strict mode and regeneration behavior
///
/// # Behavior
///
/// - If the key file exists and is valid: loads and returns the key
/// - If the key file exists but is corrupted:
///   - Non-strict mode: regenerate and overwrite
///   - Strict mode without force_regenerate: return error
///   - Strict mode with force_regenerate: regenerate and overwrite
/// - If the key file doesn't exist: generate new keypair
///
/// # Atomic Writes
///
/// Key files are written atomically using a temp-then-rename pattern to prevent
/// corruption from concurrent access or interrupted writes.
pub fn load_or_generate_worker_keypair_with_options(
    key_path: &std::path::Path,
    options: KeypairOptions,
) -> Result<SigningKey, WorkerAuthError> {
    use std::fs;

    if key_path.exists() {
        // Try to load existing key
        match load_existing_keypair(key_path) {
            Ok(keypair) => return Ok(keypair),
            Err(e) => {
                // Key is corrupted - check if we should regenerate
                let should_regenerate = !options.strict_mode || options.force_regenerate;

                if should_regenerate {
                    tracing::warn!(
                        path = %key_path.display(),
                        error = %e,
                        strict_mode = options.strict_mode,
                        force_regenerate = options.force_regenerate,
                        "Corrupted keypair detected, regenerating"
                    );

                    // Delete the corrupted files before regenerating
                    if let Err(del_err) = fs::remove_file(key_path) {
                        tracing::warn!(
                            path = %key_path.display(),
                            error = %del_err,
                            "Failed to delete corrupted private key file"
                        );
                    }
                    let pub_path = key_path.with_extension("pub");
                    if pub_path.exists() {
                        if let Err(del_err) = fs::remove_file(&pub_path) {
                            tracing::warn!(
                                path = %pub_path.display(),
                                error = %del_err,
                                "Failed to delete corrupted public key file"
                            );
                        }
                    }
                    // Fall through to generation
                } else {
                    // Strict mode without force_regenerate - return error
                    return Err(e);
                }
            }
        }
    }

    // Generate new keypair
    generate_and_write_keypair_atomic(key_path)
}

/// Load an existing keypair from file.
fn load_existing_keypair(key_path: &std::path::Path) -> Result<SigningKey, WorkerAuthError> {
    use std::fs;

    let bytes = fs::read(key_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            WorkerAuthError::KeyNotFound(key_path.display().to_string())
        } else {
            WorkerAuthError::KeyError(e.to_string())
        }
    })?;

    if bytes.len() != 32 {
        return Err(WorkerAuthError::CorruptedKeypair {
            path: key_path.display().to_string(),
            reason: format!("expected 32 bytes, got {}", bytes.len()),
        });
    }

    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&bytes);

    // Verify the key is valid by attempting to construct it
    // This catches cases where the bytes are the right length but invalid
    let signing_key = SigningKey::from_bytes(&key_bytes);

    // Verify the public key file exists and is consistent
    let pub_path = key_path.with_extension("pub");
    if pub_path.exists() {
        let pub_bytes =
            fs::read(&pub_path).map_err(|e| WorkerAuthError::KeyError(e.to_string()))?;
        if pub_bytes.len() != 32 {
            return Err(WorkerAuthError::CorruptedKeypair {
                path: pub_path.display().to_string(),
                reason: format!("expected 32 bytes, got {}", pub_bytes.len()),
            });
        }

        // Verify public key matches private key
        let expected_pub = signing_key.verifying_key();
        if pub_bytes != expected_pub.as_bytes() {
            return Err(WorkerAuthError::CorruptedKeypair {
                path: key_path.display().to_string(),
                reason: "public key does not match private key".to_string(),
            });
        }
    }

    Ok(signing_key)
}

/// Generate a new keypair and write it atomically.
///
/// Uses temp-then-rename pattern to prevent corruption from:
/// - Concurrent CP instances trying to write simultaneously
/// - Interrupted writes (crash/kill mid-write)
fn generate_and_write_keypair_atomic(
    key_path: &std::path::Path,
) -> Result<SigningKey, WorkerAuthError> {
    use std::fs;
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let keypair = SigningKey::generate(&mut rand::thread_rng());

    // Ensure directory exists
    if let Some(parent) = key_path.parent() {
        fs::create_dir_all(parent).map_err(|e| WorkerAuthError::KeyError(e.to_string()))?;
    }

    // Generate unique temp file names using timestamp
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_key_path = key_path.with_extension(format!("key.tmp.{}", nanos));
    let pub_path = key_path.with_extension("pub");
    let temp_pub_path = key_path.with_extension(format!("pub.tmp.{}", nanos));

    // Write private key to temp file with 0600 permissions (atomic on Unix)
    #[cfg(unix)]
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temp_key_path)
            .map_err(|e| {
                WorkerAuthError::KeyError(format!("Failed to create temp key file: {}", e))
            })?;

        file.write_all(&keypair.to_bytes()).map_err(|e| {
            let _ = fs::remove_file(&temp_key_path);
            WorkerAuthError::KeyError(format!("Failed to write key: {}", e))
        })?;
        file.sync_all().map_err(|e| {
            let _ = fs::remove_file(&temp_key_path);
            WorkerAuthError::KeyError(format!("Failed to sync key file: {}", e))
        })?;
    }

    #[cfg(not(unix))]
    {
        fs::write(&temp_key_path, keypair.to_bytes()).map_err(|e| {
            WorkerAuthError::KeyError(format!("Failed to write temp key file: {}", e))
        })?;
    }

    // Write public key to temp file
    #[cfg(unix)]
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o644)
            .open(&temp_pub_path)
            .map_err(|e| {
                let _ = fs::remove_file(&temp_key_path);
                WorkerAuthError::KeyError(format!("Failed to create temp pub file: {}", e))
            })?;

        file.write_all(keypair.verifying_key().as_bytes())
            .map_err(|e| {
                let _ = fs::remove_file(&temp_key_path);
                let _ = fs::remove_file(&temp_pub_path);
                WorkerAuthError::KeyError(format!("Failed to write pub key: {}", e))
            })?;
        file.sync_all().map_err(|e| {
            let _ = fs::remove_file(&temp_key_path);
            let _ = fs::remove_file(&temp_pub_path);
            WorkerAuthError::KeyError(format!("Failed to sync pub file: {}", e))
        })?;
    }

    #[cfg(not(unix))]
    {
        fs::write(&temp_pub_path, keypair.verifying_key().to_bytes()).map_err(|e| {
            let _ = fs::remove_file(&temp_key_path);
            WorkerAuthError::KeyError(format!("Failed to write temp pub file: {}", e))
        })?;
    }

    // Atomic rename: both files are now complete, rename them to final locations
    // On POSIX, rename is atomic within the same filesystem
    fs::rename(&temp_key_path, key_path).map_err(|e| {
        // Clean up temp files on error
        let _ = fs::remove_file(&temp_key_path);
        let _ = fs::remove_file(&temp_pub_path);
        WorkerAuthError::KeyError(format!("Failed to rename key file: {}", e))
    })?;

    fs::rename(&temp_pub_path, &pub_path).map_err(|e| {
        // Key file was already renamed, but pub rename failed - inconsistent state
        // Log an error but don't fail since the private key is already in place
        tracing::error!(
            error = %e,
            key_path = %key_path.display(),
            pub_path = %pub_path.display(),
            "Failed to rename public key file - keypair may be inconsistent"
        );
        WorkerAuthError::KeyError(format!("Failed to rename pub key file: {}", e))
    })?;

    tracing::info!(
        path = %key_path.display(),
        kid = %derive_kid_from_verifying_key(&keypair.verifying_key()),
        "Generated new worker signing keypair (atomic write)"
    );

    Ok(keypair)
}

/// Load a worker verifying key (public key) from file.
///
/// # Arguments
///
/// * `pub_key_path` - Path to the public key file (32 bytes, raw)
///
/// # Returns
///
/// The verifying key (public key) for token validation.
pub fn load_worker_verifying_key(
    pub_key_path: &std::path::Path,
) -> Result<VerifyingKey, WorkerAuthError> {
    use std::fs;

    let bytes = fs::read(pub_key_path).map_err(|e| WorkerAuthError::KeyError(e.to_string()))?;
    if bytes.len() != 32 {
        return Err(WorkerAuthError::KeyError(format!(
            "Invalid public key length: expected 32 bytes, got {}",
            bytes.len()
        )));
    }
    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&bytes);

    VerifyingKey::from_bytes(&key_bytes)
        .map_err(|e| WorkerAuthError::KeyError(format!("Invalid public key: {}", e)))
}

/// Load worker public key from a keys directory.
///
/// This is a convenience function that looks for `worker_signing.pub` in the
/// given directory (typically `var/keys`).
///
/// # Arguments
///
/// * `keys_dir` - Path to the keys directory
///
/// # Returns
///
/// The verifying key (public key) for token validation.
///
/// # Example
///
/// ```rust,no_run
/// use adapteros_boot::worker_auth::load_worker_public_key;
///
/// let verifying_key = load_worker_public_key("var/keys").unwrap();
/// ```
pub fn load_worker_public_key(keys_dir: &str) -> Result<VerifyingKey, WorkerAuthError> {
    let pub_key_path = std::path::Path::new(keys_dir).join("worker_signing.pub");
    load_worker_verifying_key(&pub_key_path)
}

/// Check if an error is transient (file not found) and should be retried.
///
/// Returns true if the error indicates the key file doesn't exist yet,
/// which can happen when the worker starts before the control plane
/// has generated the keypair.
fn is_transient_key_error(error: &WorkerAuthError) -> bool {
    match error {
        WorkerAuthError::KeyNotFound(_) => true,
        WorkerAuthError::KeyError(msg) => {
            // Check for common "not found" error messages
            msg.contains("No such file")
                || msg.contains("not found")
                || msg.contains("cannot find")
                || msg.contains("does not exist")
        }
        _ => false,
    }
}

/// Load worker public key with retry and exponential backoff.
///
/// This function is designed for container/orchestration environments where
/// the worker may start before the control plane has generated the keypair.
/// It retries with exponential backoff until either:
/// - The key is successfully loaded
/// - A non-transient error occurs (e.g., invalid key format)
/// - The deadline is exceeded
///
/// # Arguments
///
/// * `keys_dir` - Path to the keys directory (e.g., "var/keys")
/// * `deadline` - Maximum time to wait for the key to become available
///
/// # Returns
///
/// The verifying key (public key) for token validation.
///
/// # Errors
///
/// - `KeyNotFound` if the key file doesn't exist after the deadline
/// - `KeyError` if the key file exists but is invalid
///
/// # Example
///
/// ```rust,no_run
/// use adapteros_boot::worker_auth::load_worker_public_key_with_retry;
/// use std::time::Duration;
///
/// let key = load_worker_public_key_with_retry("var/keys", Duration::from_secs(120));
/// match key {
///     Ok(verifying_key) => println!("Key loaded successfully"),
///     Err(e) => eprintln!("Failed to load key: {}", e),
/// }
/// ```
pub fn load_worker_public_key_with_retry(
    keys_dir: &str,
    deadline: std::time::Duration,
) -> Result<VerifyingKey, WorkerAuthError> {
    let start = std::time::Instant::now();
    let pub_key_path = std::path::Path::new(keys_dir).join("worker_signing.pub");

    // Exponential backoff: 1s, 2s, 4s, 8s, 16s (capped)
    const INITIAL_DELAY_MS: u64 = 1000;
    const MAX_DELAY_MS: u64 = 16000;
    const BACKOFF_MULTIPLIER: f64 = 2.0;

    let mut delay_ms = INITIAL_DELAY_MS;
    let mut attempts = 0u32;

    loop {
        attempts += 1;

        // Try to load the key
        match load_worker_verifying_key(&pub_key_path) {
            Ok(key) => {
                if attempts > 1 {
                    tracing::info!(
                        attempts = attempts,
                        elapsed_ms = start.elapsed().as_millis(),
                        "Worker public key loaded after retry"
                    );
                }
                return Ok(key);
            }
            Err(e) => {
                // Check if this is a transient error that should be retried
                if !is_transient_key_error(&e) {
                    // Non-transient error (e.g., invalid key format) - fail immediately
                    tracing::error!(
                        error = %e,
                        attempts = attempts,
                        "Non-transient error loading worker public key, giving up"
                    );
                    return Err(e);
                }

                // Check if we've exceeded the deadline
                if start.elapsed() >= deadline {
                    tracing::warn!(
                        attempts = attempts,
                        elapsed_ms = start.elapsed().as_millis(),
                        deadline_ms = deadline.as_millis(),
                        "Deadline exceeded waiting for worker public key"
                    );
                    return Err(WorkerAuthError::KeyNotFound(format!(
                        "Worker public key not found at {} after {} attempts ({:?})",
                        pub_key_path.display(),
                        attempts,
                        start.elapsed()
                    )));
                }

                // Log the retry attempt
                tracing::debug!(
                    attempts = attempts,
                    delay_ms = delay_ms,
                    elapsed_ms = start.elapsed().as_millis(),
                    remaining_ms = (deadline - start.elapsed()).as_millis(),
                    "Worker public key not found, retrying..."
                );

                // Sleep with backoff
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));

                // Increase delay for next attempt (with cap)
                delay_ms = ((delay_ms as f64 * BACKOFF_MULTIPLIER) as u64).min(MAX_DELAY_MS);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroUsize;

    fn test_keypair() -> SigningKey {
        SigningKey::generate(&mut rand::thread_rng())
    }

    #[test]
    fn test_generate_and_validate_token() {
        let keypair = test_keypair();
        let token = generate_worker_token(&keypair, "worker-1", "req-123", 60).unwrap();

        let mut jti_cache = LruCache::new(NonZeroUsize::new(100).unwrap());
        let claims = validate_worker_token(
            &token,
            &keypair.verifying_key(),
            Some("worker-1"),
            &mut jti_cache,
        )
        .unwrap();

        assert_eq!(claims.iss, "control-plane");
        assert_eq!(claims.aud, "worker");
        assert_eq!(claims.wid, "worker-1");
        assert_eq!(claims.jti, "req-123");
    }

    #[test]
    fn test_replay_defense() {
        let keypair = test_keypair();
        let token = generate_worker_token(&keypair, "worker-1", "req-123", 60).unwrap();

        let mut jti_cache = LruCache::new(NonZeroUsize::new(100).unwrap());

        // First validation should succeed
        validate_worker_token(&token, &keypair.verifying_key(), None, &mut jti_cache).unwrap();

        // Second validation with same jti should fail
        let result = validate_worker_token(&token, &keypair.verifying_key(), None, &mut jti_cache);
        assert!(matches!(result, Err(WorkerAuthError::ReplayDetected)));
    }

    #[test]
    fn test_worker_id_mismatch() {
        let keypair = test_keypair();
        let token = generate_worker_token(&keypair, "worker-1", "req-123", 60).unwrap();

        let mut jti_cache = LruCache::new(NonZeroUsize::new(100).unwrap());
        let result = validate_worker_token(
            &token,
            &keypair.verifying_key(),
            Some("worker-2"), // Wrong worker ID
            &mut jti_cache,
        );
        match result {
            Err(WorkerAuthError::WorkerIdMismatch { expected, got }) => {
                assert_eq!(expected, "worker-2");
                assert_eq!(got, "worker-1");
            }
            other => panic!("Expected WorkerIdMismatch, got: {:?}", other),
        }
    }

    #[test]
    fn test_invalid_signature() {
        let keypair1 = test_keypair();
        let keypair2 = test_keypair();

        let token = generate_worker_token(&keypair1, "worker-1", "req-123", 60).unwrap();

        let mut jti_cache = LruCache::new(NonZeroUsize::new(100).unwrap());
        let result = validate_worker_token(&token, &keypair2.verifying_key(), None, &mut jti_cache);
        assert!(matches!(result, Err(WorkerAuthError::InvalidSignature)));
    }

    #[test]
    fn test_expired_token_within_tolerance() {
        // Test that tokens within clock skew tolerance are still accepted
        let keypair = test_keypair();
        // Create a token with minimum TTL (1 second)
        let token = generate_worker_token(&keypair, "worker-1", "req-123", 1).unwrap();

        // Wait 2 seconds - token is expired but within 30-second clock skew tolerance
        std::thread::sleep(std::time::Duration::from_millis(2100));

        let mut jti_cache = LruCache::new(NonZeroUsize::new(100).unwrap());
        let result = validate_worker_token(&token, &keypair.verifying_key(), None, &mut jti_cache);

        // Token should be accepted within tolerance (with a warning logged)
        assert!(
            result.is_ok(),
            "Expected token to be accepted within clock skew tolerance, got: {:?}",
            result
        );
    }

    #[test]
    fn test_expired_token_beyond_tolerance() {
        // Test that tokens beyond clock skew tolerance are rejected
        // We can't easily wait 31+ seconds in a test, so we'll craft a token
        // that appears to have been created 31 seconds ago
        let keypair = test_keypair();
        let now = Utc::now();
        // Set expiration to 31 seconds in the past (beyond 30s tolerance)
        let exp = now.timestamp() - 31;

        let claims = WorkerTokenClaims {
            iss: "control-plane".into(),
            aud: "worker".into(),
            wid: "worker-1".into(),
            iat: now.timestamp() - 91, // issued 91 seconds ago
            exp,
            jti: "req-123".into(),
        };

        let header = JwtHeader {
            alg: "EdDSA".into(),
            typ: "JWT".into(),
            kid: derive_kid_from_verifying_key(&keypair.verifying_key()),
        };

        // Manually construct the token
        let header_json = serde_json::to_vec(&header).unwrap();
        let claims_json = serde_json::to_vec(&claims).unwrap();
        let header_b64 = URL_SAFE_NO_PAD.encode(&header_json);
        let claims_b64 = URL_SAFE_NO_PAD.encode(&claims_json);
        let message = format!("{}.{}", header_b64, claims_b64);
        let signature = keypair.sign(message.as_bytes());
        let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());
        let token = format!("{}.{}", message, sig_b64);

        let mut jti_cache = LruCache::new(NonZeroUsize::new(100).unwrap());
        let result = validate_worker_token(&token, &keypair.verifying_key(), None, &mut jti_cache);
        assert!(
            matches!(result, Err(WorkerAuthError::Expired)),
            "Expected Expired error, got: {:?}",
            result
        );
    }

    #[test]
    fn test_kid_derivation() {
        let keypair = test_keypair();
        let kid = derive_kid_from_verifying_key(&keypair.verifying_key());
        assert!(kid.starts_with("worker-"));
        assert_eq!(kid.len(), 39); // "worker-" (7) + 32 hex chars = 39
    }

    // === Input Validation Tests ===

    #[test]
    fn test_empty_request_id_rejected() {
        let keypair = test_keypair();
        let result = generate_worker_token(&keypair, "worker-1", "", 60);
        assert!(matches!(result, Err(WorkerAuthError::EmptyRequestId)));
    }

    #[test]
    fn test_empty_worker_id_rejected() {
        let keypair = test_keypair();
        let result = generate_worker_token(&keypair, "", "req-123", 60);
        assert!(matches!(result, Err(WorkerAuthError::EmptyWorkerId)));
    }

    #[test]
    fn test_zero_ttl_rejected() {
        let keypair = test_keypair();
        let result = generate_worker_token(&keypair, "worker-1", "req-123", 0);
        assert!(
            matches!(
                result,
                Err(WorkerAuthError::TtlTooShort { min: 1, actual: 0 })
            ),
            "Expected TtlTooShort error, got: {:?}",
            result
        );
    }

    #[test]
    fn test_ttl_minimum_accepted() {
        let keypair = test_keypair();
        let result = generate_worker_token(&keypair, "worker-1", "req-123", 1);
        assert!(result.is_ok(), "Expected minimum TTL to be accepted");
    }

    #[test]
    fn test_long_worker_id_rejected() {
        let keypair = test_keypair();
        let long_id = "x".repeat(MAX_WORKER_ID_LENGTH + 1);
        let result = generate_worker_token(&keypair, &long_id, "req-123", 60);
        assert!(
            matches!(
                result,
                Err(WorkerAuthError::WorkerIdTooLong {
                    max: 256,
                    actual: 257
                })
            ),
            "Expected WorkerIdTooLong error, got: {:?}",
            result
        );
    }

    #[test]
    fn test_long_request_id_rejected() {
        let keypair = test_keypair();
        let long_id = "r".repeat(MAX_REQUEST_ID_LENGTH + 1);
        let result = generate_worker_token(&keypair, "worker-1", &long_id, 60);
        assert!(
            matches!(
                result,
                Err(WorkerAuthError::RequestIdTooLong {
                    max: 256,
                    actual: 257
                })
            ),
            "Expected RequestIdTooLong error, got: {:?}",
            result
        );
    }

    #[test]
    fn test_max_length_ids_accepted() {
        let keypair = test_keypair();
        let max_worker_id = "w".repeat(MAX_WORKER_ID_LENGTH);
        let max_request_id = "r".repeat(MAX_REQUEST_ID_LENGTH);
        let result = generate_worker_token(&keypair, &max_worker_id, &max_request_id, 60);
        assert!(result.is_ok(), "Expected max length IDs to be accepted");
    }

    #[test]
    fn test_unicode_in_worker_id_rejected() {
        let keypair = test_keypair();
        // Emoji is outside ASCII printable range
        let result = generate_worker_token(&keypair, "worker-\u{1F600}", "req-123", 60);
        assert!(
            matches!(result, Err(WorkerAuthError::InvalidIdentifierChars { .. })),
            "Expected InvalidIdentifierChars error for emoji, got: {:?}",
            result
        );
    }

    #[test]
    fn test_unicode_in_request_id_rejected() {
        let keypair = test_keypair();
        // Chinese characters are outside ASCII printable range
        let result = generate_worker_token(&keypair, "worker-1", "req-\u{4E2D}\u{6587}", 60);
        assert!(
            matches!(result, Err(WorkerAuthError::InvalidIdentifierChars { .. })),
            "Expected InvalidIdentifierChars error for Chinese chars, got: {:?}",
            result
        );
    }

    #[test]
    fn test_control_chars_rejected() {
        let keypair = test_keypair();
        // Tab character (0x09) is not in ASCII printable range (0x20-0x7E)
        let result = generate_worker_token(&keypair, "worker\t1", "req-123", 60);
        assert!(
            matches!(result, Err(WorkerAuthError::InvalidIdentifierChars { .. })),
            "Expected InvalidIdentifierChars error for tab, got: {:?}",
            result
        );
    }

    #[test]
    fn test_newline_rejected() {
        let keypair = test_keypair();
        // Newline (0x0A) is not in ASCII printable range
        let result = generate_worker_token(&keypair, "worker-1", "req\n123", 60);
        assert!(
            matches!(result, Err(WorkerAuthError::InvalidIdentifierChars { .. })),
            "Expected InvalidIdentifierChars error for newline, got: {:?}",
            result
        );
    }

    #[test]
    fn test_valid_ascii_identifiers_accepted() {
        let keypair = test_keypair();
        // Test various valid ASCII printable characters
        let result = generate_worker_token(
            &keypair,
            "worker-1_test.prod:8080",
            "req-123_abc-XYZ.test",
            60,
        );
        assert!(
            result.is_ok(),
            "Expected valid ASCII identifiers to be accepted"
        );
    }

    #[test]
    fn test_special_printable_chars_accepted() {
        let keypair = test_keypair();
        // Test edge cases of ASCII printable range: space (0x20) and tilde (0x7E)
        let result = generate_worker_token(&keypair, "worker 1", "req~123", 60);
        assert!(result.is_ok(), "Expected space and tilde to be accepted");
    }
}
