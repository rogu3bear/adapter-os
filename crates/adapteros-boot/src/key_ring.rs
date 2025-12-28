//! Key ring for worker authentication with rotation support.
//!
//! This module provides a key ring that holds multiple Ed25519 keys for
//! worker authentication. During key rotation, both old and new keys are
//! valid, allowing workers to continue validating tokens signed with the
//! old key until the grace period expires.
//!
//! ## Key Rotation Flow
//!
//! 1. Control plane calls `rotate_signing_key()` with a grace period
//! 2. New key becomes the current signing key
//! 3. Old key is retained in `verifying_keys` for the grace period
//! 4. Tokens signed with old key are still valid during grace period
//! 5. After grace period, old key is removed on next rotation or cleanup
//!
//! ## Example
//!
//! ```rust,no_run
//! use adapteros_boot::key_ring::WorkerKeyRing;
//! use std::path::Path;
//!
//! // Load or create key ring from keys directory
//! let ring = WorkerKeyRing::load_or_create(Path::new("var/keys")).unwrap();
//!
//! // Generate token (uses current key)
//! let token = ring.generate_token("worker-1", "req-123", 45).unwrap();
//!
//! // Validate token (checks against all keys in ring by kid)
//! let claims = ring.validate_token(&token, Some("worker-1")).unwrap();
//! ```

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use ed25519_dalek::{SigningKey, VerifyingKey};
use lru::LruCache;
use serde::{Deserialize, Serialize};

use crate::error::WorkerAuthError;
use crate::worker_auth::{
    derive_kid_from_verifying_key, generate_worker_token, load_or_generate_worker_keypair,
    validate_worker_token, WorkerTokenClaims,
};

/// Default grace period for key rotation (5 minutes).
///
/// During this period, both old and new keys are valid.
/// This allows in-flight requests to complete without error.
pub const DEFAULT_ROTATION_GRACE_PERIOD_SECS: u64 = 300;

/// Metadata about key rotation history and current state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationMeta {
    /// When the current key became active
    pub current_key_active_since: DateTime<Utc>,

    /// When the last rotation occurred (None if never rotated)
    pub last_rotation: Option<DateTime<Utc>>,

    /// Keys scheduled for removal (kid -> expiry timestamp)
    pub pending_removal: HashMap<String, i64>,
}

impl Default for RotationMeta {
    fn default() -> Self {
        Self {
            current_key_active_since: Utc::now(),
            last_rotation: None,
            pending_removal: HashMap::new(),
        }
    }
}

/// Receipt returned after a successful key rotation.
#[derive(Debug, Clone)]
pub struct RotationReceipt {
    /// Key ID of the old (now deprecated) key
    pub old_kid: String,

    /// Key ID of the new current key
    pub new_kid: String,

    /// When the old key will be removed
    pub old_key_expires_at: DateTime<Utc>,

    /// Grace period in seconds
    pub grace_period_secs: u64,
}

/// Key ring for worker authentication with rotation support.
///
/// The key ring maintains:
/// - A current signing key (used for generating new tokens)
/// - A map of verifying keys (kid -> key) for validating tokens
/// - Rotation metadata for grace period management
pub struct WorkerKeyRing {
    /// Current signing key for generating tokens
    current: Arc<SigningKey>,

    /// Key ID of the current signing key
    current_kid: String,

    /// Map of all valid verifying keys (kid -> key)
    /// Includes the current key and any keys in grace period
    verifying_keys: HashMap<String, Arc<VerifyingKey>>,

    /// Rotation history and pending removals
    rotation_meta: RotationMeta,

    /// JTI cache for replay defense (shared across all key validations)
    jti_cache: RwLock<LruCache<String, i64>>,
}

impl std::fmt::Debug for WorkerKeyRing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerKeyRing")
            .field("current_kid", &self.current_kid)
            .field("verifying_key_count", &self.verifying_keys.len())
            .field("rotation_meta", &self.rotation_meta)
            .finish_non_exhaustive()
    }
}

impl WorkerKeyRing {
    /// Create a new key ring with the given signing key.
    ///
    /// # Arguments
    ///
    /// * `signing_key` - The Ed25519 signing key to use as the current key
    /// * `jti_cache_size` - Size of the JTI cache for replay defense
    pub fn new(signing_key: SigningKey, jti_cache_size: usize) -> Self {
        let verifying_key = signing_key.verifying_key();
        let kid = derive_kid_from_verifying_key(&verifying_key);

        let mut verifying_keys = HashMap::new();
        verifying_keys.insert(kid.clone(), Arc::new(verifying_key));

        let cache_size = std::num::NonZeroUsize::new(jti_cache_size)
            .unwrap_or(std::num::NonZeroUsize::new(10000).unwrap());

        Self {
            current: Arc::new(signing_key),
            current_kid: kid,
            verifying_keys,
            rotation_meta: RotationMeta::default(),
            jti_cache: RwLock::new(LruCache::new(cache_size)),
        }
    }

    /// Load or create a key ring from a keys directory.
    ///
    /// If a key file exists, it will be loaded. Otherwise, a new key will be generated.
    ///
    /// # Arguments
    ///
    /// * `keys_dir` - Path to the directory containing key files
    pub fn load_or_create(keys_dir: &Path) -> Result<Self, WorkerAuthError> {
        let signing_key = load_or_generate_worker_keypair(keys_dir)?;
        Ok(Self::new(signing_key, 10000))
    }

    /// Load or create a key ring with custom JTI cache size.
    pub fn load_or_create_with_cache_size(
        keys_dir: &Path,
        jti_cache_size: usize,
    ) -> Result<Self, WorkerAuthError> {
        let signing_key = load_or_generate_worker_keypair(keys_dir)?;
        Ok(Self::new(signing_key, jti_cache_size))
    }

    /// Get the current key ID.
    pub fn current_kid(&self) -> &str {
        &self.current_kid
    }

    /// Get the current signing key.
    pub fn current_signing_key(&self) -> Arc<SigningKey> {
        Arc::clone(&self.current)
    }

    /// Get the current verifying key.
    pub fn current_verifying_key(&self) -> Arc<VerifyingKey> {
        Arc::clone(
            self.verifying_keys
                .get(&self.current_kid)
                .expect("Current key must exist in verifying_keys"),
        )
    }

    /// Get a verifying key by key ID.
    pub fn get_verifying_key(&self, kid: &str) -> Option<Arc<VerifyingKey>> {
        self.verifying_keys.get(kid).cloned()
    }

    /// Get the number of active verifying keys.
    pub fn key_count(&self) -> usize {
        self.verifying_keys.len()
    }

    /// Generate a token using the current signing key.
    ///
    /// # Arguments
    ///
    /// * `worker_id` - Target worker ID
    /// * `request_id` - Unique request ID for replay defense
    /// * `ttl_seconds` - Token time-to-live in seconds
    pub fn generate_token(
        &self,
        worker_id: &str,
        request_id: &str,
        ttl_seconds: u64,
    ) -> Result<String, WorkerAuthError> {
        generate_worker_token(&self.current, worker_id, request_id, ttl_seconds)
    }

    /// Validate a token using the key ring.
    ///
    /// This method extracts the `kid` from the token header and looks up
    /// the corresponding verifying key. This allows tokens signed with
    /// older keys (during grace period) to still be validated.
    ///
    /// # Arguments
    ///
    /// * `token` - JWT string to validate
    /// * `expected_worker_id` - If Some, validates that `wid` matches
    pub fn validate_token(
        &self,
        token: &str,
        expected_worker_id: Option<&str>,
    ) -> Result<WorkerTokenClaims, WorkerAuthError> {
        // Extract kid from token header to find the right key
        let kid = extract_kid_from_token(token)?;

        let verifying_key = self
            .verifying_keys
            .get(&kid)
            .ok_or_else(|| WorkerAuthError::UnknownKeyId(kid.clone()))?;

        let mut cache = self.jti_cache.write().unwrap();
        validate_worker_token(token, verifying_key, expected_worker_id, &mut cache)
    }

    /// Add a verifying key to the ring (for workers receiving rotated keys).
    ///
    /// This is used by workers when they receive a new verifying key from
    /// the control plane during rotation.
    ///
    /// # Arguments
    ///
    /// * `verifying_key` - The new verifying key to add
    pub fn add_verifying_key(&mut self, verifying_key: VerifyingKey) -> String {
        let kid = derive_kid_from_verifying_key(&verifying_key);
        self.verifying_keys
            .insert(kid.clone(), Arc::new(verifying_key));
        kid
    }

    /// Rotate the signing key.
    ///
    /// This generates a new signing key, adds it to the ring, and schedules
    /// the old key for removal after the grace period.
    ///
    /// # Arguments
    ///
    /// * `grace_period_secs` - How long the old key should remain valid
    ///
    /// # Returns
    ///
    /// A receipt containing information about the rotation.
    pub fn rotate_signing_key(
        &mut self,
        grace_period_secs: u64,
    ) -> Result<RotationReceipt, WorkerAuthError> {
        // Generate new signing key
        let new_signing_key = SigningKey::generate(&mut rand::thread_rng());
        let new_verifying_key = new_signing_key.verifying_key();
        let new_kid = derive_kid_from_verifying_key(&new_verifying_key);

        // Calculate expiry time for old key
        let now = Utc::now();
        let expiry = now + chrono::Duration::seconds(grace_period_secs as i64);

        // Schedule old key for removal
        let old_kid = self.current_kid.clone();
        self.rotation_meta
            .pending_removal
            .insert(old_kid.clone(), expiry.timestamp());

        // Update current key
        self.current = Arc::new(new_signing_key);
        self.current_kid = new_kid.clone();

        // Add new key to verifying keys
        self.verifying_keys
            .insert(new_kid.clone(), Arc::new(new_verifying_key));

        // Update rotation metadata
        self.rotation_meta.last_rotation = Some(now);
        self.rotation_meta.current_key_active_since = now;

        Ok(RotationReceipt {
            old_kid,
            new_kid,
            old_key_expires_at: expiry,
            grace_period_secs,
        })
    }

    /// Clean up expired keys.
    ///
    /// This removes keys that have passed their grace period.
    /// Should be called periodically (e.g., on each rotation or via a timer).
    ///
    /// # Returns
    ///
    /// The number of keys removed.
    pub fn cleanup_expired_keys(&mut self) -> usize {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs() as i64;

        let expired: Vec<String> = self
            .rotation_meta
            .pending_removal
            .iter()
            .filter(|(_, &expiry)| expiry <= now)
            .map(|(kid, _)| kid.clone())
            .collect();

        let count = expired.len();

        for kid in &expired {
            // Don't remove the current key
            if *kid != self.current_kid {
                self.verifying_keys.remove(kid);
            }
            self.rotation_meta.pending_removal.remove(kid);
        }

        count
    }

    /// Get rotation metadata.
    pub fn rotation_meta(&self) -> &RotationMeta {
        &self.rotation_meta
    }

    /// Add a verifying key with a grace period for the current key.
    ///
    /// This is used during key update to add a new key while scheduling
    /// the current key for removal after the grace period.
    ///
    /// # Arguments
    ///
    /// * `verifying_key` - The new verifying key to add
    /// * `grace_period_secs` - How long the current key should remain valid
    ///
    /// # Returns
    ///
    /// The key ID (kid) of the newly added key.
    pub fn add_verifying_key_with_grace(
        &mut self,
        verifying_key: VerifyingKey,
        grace_period_secs: u64,
    ) -> String {
        let new_kid = derive_kid_from_verifying_key(&verifying_key);

        // Add the new key to verifying keys
        self.verifying_keys
            .insert(new_kid.clone(), Arc::new(verifying_key));

        // Schedule current key for removal after grace period
        let expiry = Utc::now().timestamp() + grace_period_secs as i64;
        self.rotation_meta
            .pending_removal
            .insert(self.current_kid.clone(), expiry);

        new_kid
    }

    /// Check if a nonce has already been seen (for replay attack prevention).
    ///
    /// # Arguments
    ///
    /// * `nonce` - The nonce to check
    ///
    /// # Returns
    ///
    /// `true` if the nonce has been seen and is still valid (not expired).
    pub fn has_seen_nonce(&self, nonce: &str) -> bool {
        let cache = self.jti_cache.read().unwrap();
        if let Some(&expiry) = cache.peek(nonce) {
            // Check if nonce is still valid (not expired)
            let now = Utc::now().timestamp();
            expiry > now
        } else {
            false
        }
    }

    /// Record a nonce to prevent replay attacks.
    ///
    /// This adds the nonce to the JTI cache with an expiry timestamp.
    /// The nonce will be rejected if seen again before expiry.
    ///
    /// # Arguments
    ///
    /// * `nonce` - The nonce string to record
    /// * `expiry` - Unix timestamp when this nonce entry should expire
    pub fn record_nonce(&mut self, nonce: &str, expiry: i64) {
        let mut cache = self.jti_cache.write().unwrap();
        cache.put(nonce.to_string(), expiry);
    }
}

/// Extract the key ID (kid) from a JWT token header.
///
/// # Arguments
///
/// * `token` - JWT string in format `header.payload.signature`
///
/// # Returns
///
/// The kid from the token header, or an error if parsing fails.
pub fn extract_kid_from_token(token: &str) -> Result<String, WorkerAuthError> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(WorkerAuthError::InvalidFormat);
    }

    let header_bytes = URL_SAFE_NO_PAD
        .decode(parts[0])
        .map_err(|e| WorkerAuthError::Base64Decode(e.to_string()))?;

    #[derive(Deserialize)]
    struct JwtHeader {
        kid: String,
    }

    let header: JwtHeader = serde_json::from_slice(&header_bytes)?;
    Ok(header.kid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_ring_creation() {
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let ring = WorkerKeyRing::new(signing_key, 1000);

        assert_eq!(ring.key_count(), 1);
        assert!(!ring.current_kid().is_empty());
    }

    #[test]
    fn test_token_generation_and_validation() {
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let ring = WorkerKeyRing::new(signing_key, 1000);

        let token = ring.generate_token("worker-1", "req-123", 45).unwrap();
        let claims = ring.validate_token(&token, Some("worker-1")).unwrap();

        assert_eq!(claims.wid, "worker-1");
        assert_eq!(claims.jti, "req-123");
    }

    #[test]
    fn test_key_rotation() {
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let mut ring = WorkerKeyRing::new(signing_key, 1000);

        let old_kid = ring.current_kid().to_string();

        // Generate token with old key
        let old_token = ring.generate_token("worker-1", "req-old", 45).unwrap();

        // Rotate key
        let receipt = ring.rotate_signing_key(300).unwrap();
        assert_eq!(receipt.old_kid, old_kid);
        assert_ne!(receipt.new_kid, old_kid);
        assert_eq!(ring.key_count(), 2); // Both keys should be present

        // Generate token with new key
        let new_token = ring.generate_token("worker-1", "req-new", 45).unwrap();

        // Both tokens should validate
        let old_claims = ring.validate_token(&old_token, Some("worker-1")).unwrap();
        assert_eq!(old_claims.jti, "req-old");

        let new_claims = ring.validate_token(&new_token, Some("worker-1")).unwrap();
        assert_eq!(new_claims.jti, "req-new");
    }

    #[test]
    fn test_unknown_kid_rejected() {
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let ring = WorkerKeyRing::new(signing_key, 1000);

        // Create token with a different key
        let other_key = SigningKey::generate(&mut rand::thread_rng());
        let other_token = generate_worker_token(&other_key, "worker-1", "req-123", 45).unwrap();

        // Should fail because kid is not in ring
        let result = ring.validate_token(&other_token, Some("worker-1"));
        assert!(matches!(result, Err(WorkerAuthError::UnknownKeyId(_))));
    }

    #[test]
    fn test_extract_kid_from_token() {
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let expected_kid = derive_kid_from_verifying_key(&signing_key.verifying_key());

        let token = generate_worker_token(&signing_key, "worker-1", "req-123", 45).unwrap();
        let extracted_kid = extract_kid_from_token(&token).unwrap();

        assert_eq!(extracted_kid, expected_kid);
    }

    #[test]
    fn test_add_verifying_key() {
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let mut ring = WorkerKeyRing::new(signing_key, 1000);

        // Generate a new verifying key (simulating receiving from CP)
        let other_key = SigningKey::generate(&mut rand::thread_rng());
        let other_verifying = other_key.verifying_key();

        let added_kid = ring.add_verifying_key(other_verifying);
        assert_eq!(ring.key_count(), 2);

        // Create token with the other key and validate with ring
        let token = generate_worker_token(&other_key, "worker-1", "req-123", 45).unwrap();
        let claims = ring.validate_token(&token, Some("worker-1")).unwrap();
        assert_eq!(claims.jti, "req-123");

        // The added key should be retrievable
        assert!(ring.get_verifying_key(&added_kid).is_some());
    }
}
