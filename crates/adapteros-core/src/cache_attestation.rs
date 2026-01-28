//! Cache Attestation for Provable Billing Credits
//!
//! This module implements cryptographic attestation for prefix cache credits
//! to prevent billing fraud. When a worker claims cached tokens (reducing
//! billed input tokens), it must provide a signed attestation proving the
//! cache hit was genuine.
//!
//! ## Security Model
//!
//! Without attestation, a malicious worker could claim arbitrary
//! `prefix_cached_token_count` values to reduce billing. The attestation
//! binds:
//! - The cache lookup key (BLAKE3 hash)
//! - The claimed token count
//! - The worker identity
//! - A logical timestamp (tick) for replay prevention
//!
//! The control plane verifies the attestation signature before accepting
//! cache credits in billing calculations.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use adapteros_core::cache_attestation::{CacheAttestation, CacheAttestationBuilder};
//! use adapteros_crypto::signature::Keypair;
//!
//! // Worker generates attestation on cache hit
//! let attestation = CacheAttestationBuilder::new()
//!     .cache_key_hash(cache_key.as_bytes())
//!     .token_count(150)
//!     .worker_id("worker-001")
//!     .timestamp_tick(42)
//!     .build()
//!     .sign(&worker_keypair)?;
//!
//! // Control plane verifies before accepting cache credits
//! attestation.verify(&worker_public_key)?;
//! ```
//!
//! ## Schema Versioning
//!
//! The attestation uses a versioned canonical byte format for forward
//! compatibility. The current schema version is 1.

use crate::{AosError, Result, B3Hash};
use serde::{Deserialize, Serialize};

/// Current attestation schema version.
///
/// Increment when changing the canonical byte format.
pub const CACHE_ATTESTATION_SCHEMA_VERSION: u8 = 1;

/// Cryptographic attestation for prefix cache credits.
///
/// This struct proves that a worker legitimately found a cache hit
/// for the claimed token count. The signature covers all fields
/// in canonical byte order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheAttestation {
    /// Schema version for forward compatibility
    pub schema_version: u8,
    /// BLAKE3 hash of the cache lookup key
    pub cache_key_hash: [u8; 32],
    /// Number of tokens claimed as cached
    pub token_count: u32,
    /// Identifier of the attesting worker
    pub worker_id: String,
    /// Logical tick for replay prevention (not wall time)
    pub timestamp_tick: u64,
    /// Ed25519 signature over the canonical bytes
    #[serde(with = "crate::serde_helpers::hex_bytes_64")]
    pub signature: [u8; 64],
}

impl CacheAttestation {
    /// Compute the canonical bytes for signing/verification.
    ///
    /// Format (deterministic):
    /// - 1 byte: schema_version
    /// - 32 bytes: cache_key_hash
    /// - 4 bytes: token_count (little-endian)
    /// - 4 bytes: worker_id length (little-endian)
    /// - N bytes: worker_id (UTF-8)
    /// - 8 bytes: timestamp_tick (little-endian)
    pub fn canonical_bytes(&self) -> Vec<u8> {
        Self::compute_canonical_bytes(
            self.schema_version,
            &self.cache_key_hash,
            self.token_count,
            &self.worker_id,
            self.timestamp_tick,
        )
    }

    /// Compute canonical bytes from components (for signing before attestation exists).
    fn compute_canonical_bytes(
        schema_version: u8,
        cache_key_hash: &[u8; 32],
        token_count: u32,
        worker_id: &str,
        timestamp_tick: u64,
    ) -> Vec<u8> {
        let worker_id_bytes = worker_id.as_bytes();
        let mut bytes = Vec::with_capacity(1 + 32 + 4 + 4 + worker_id_bytes.len() + 8);

        bytes.push(schema_version);
        bytes.extend_from_slice(cache_key_hash);
        bytes.extend_from_slice(&token_count.to_le_bytes());
        bytes.extend_from_slice(&(worker_id_bytes.len() as u32).to_le_bytes());
        bytes.extend_from_slice(worker_id_bytes);
        bytes.extend_from_slice(&timestamp_tick.to_le_bytes());

        bytes
    }

    /// Verify the attestation signature.
    ///
    /// Returns `Ok(())` if the signature is valid, `Err` otherwise.
    ///
    /// # Arguments
    ///
    /// * `public_key` - The worker's Ed25519 public key (32 bytes)
    ///
    /// # Errors
    ///
    /// Returns `AosError::Crypto` if:
    /// - The public key is invalid
    /// - The signature verification fails
    /// - Schema version is unsupported
    pub fn verify(&self, public_key: &[u8; 32]) -> Result<()> {
        // Check schema version
        if self.schema_version != CACHE_ATTESTATION_SCHEMA_VERSION {
            return Err(AosError::Crypto(format!(
                "Unsupported cache attestation schema version: {} (expected {})",
                self.schema_version, CACHE_ATTESTATION_SCHEMA_VERSION
            )));
        }

        // Import ed25519-dalek types
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};

        // Parse public key
        let verifying_key = VerifyingKey::from_bytes(public_key)
            .map_err(|e| AosError::Crypto(format!("Invalid public key: {}", e)))?;

        // Parse signature
        let signature = Signature::from_bytes(&self.signature);

        // Compute canonical bytes and verify
        let message = self.canonical_bytes();
        verifying_key
            .verify(&message, &signature)
            .map_err(|e| AosError::Crypto(format!("Cache attestation signature verification failed: {}", e)))?;

        Ok(())
    }

    /// Get the cache key as a B3Hash.
    pub fn cache_key_b3(&self) -> B3Hash {
        B3Hash::from_bytes(self.cache_key_hash)
    }
}

/// Builder for creating and signing cache attestations.
#[derive(Debug, Default)]
pub struct CacheAttestationBuilder {
    cache_key_hash: Option<[u8; 32]>,
    token_count: Option<u32>,
    worker_id: Option<String>,
    timestamp_tick: Option<u64>,
}

impl CacheAttestationBuilder {
    /// Create a new attestation builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the cache key hash (BLAKE3 of the lookup key).
    pub fn cache_key_hash(mut self, hash: &[u8; 32]) -> Self {
        self.cache_key_hash = Some(*hash);
        self
    }

    /// Set the cache key from a B3Hash.
    pub fn cache_key_b3(mut self, hash: &B3Hash) -> Self {
        self.cache_key_hash = Some(*hash.as_bytes());
        self
    }

    /// Set the claimed cached token count.
    pub fn token_count(mut self, count: u32) -> Self {
        self.token_count = Some(count);
        self
    }

    /// Set the worker identifier.
    pub fn worker_id(mut self, id: impl Into<String>) -> Self {
        self.worker_id = Some(id.into());
        self
    }

    /// Set the logical timestamp tick.
    pub fn timestamp_tick(mut self, tick: u64) -> Self {
        self.timestamp_tick = Some(tick);
        self
    }

    /// Build and sign the attestation.
    ///
    /// # Arguments
    ///
    /// * `signing_key` - The worker's Ed25519 signing key (32-byte seed)
    ///
    /// # Errors
    ///
    /// Returns `AosError::Validation` if any required field is missing.
    pub fn build_and_sign(self, signing_key: &[u8; 32]) -> Result<CacheAttestation> {
        let cache_key_hash = self.cache_key_hash.ok_or_else(|| {
            AosError::Validation("cache_key_hash is required".to_string())
        })?;
        let token_count = self.token_count.ok_or_else(|| {
            AosError::Validation("token_count is required".to_string())
        })?;
        let worker_id = self.worker_id.ok_or_else(|| {
            AosError::Validation("worker_id is required".to_string())
        })?;
        let timestamp_tick = self.timestamp_tick.ok_or_else(|| {
            AosError::Validation("timestamp_tick is required".to_string())
        })?;

        // Compute canonical bytes for signing
        let message = CacheAttestation::compute_canonical_bytes(
            CACHE_ATTESTATION_SCHEMA_VERSION,
            &cache_key_hash,
            token_count,
            &worker_id,
            timestamp_tick,
        );

        // Sign with Ed25519
        use ed25519_dalek::{Signer, SigningKey};
        let signing_key = SigningKey::from_bytes(signing_key);
        let signature = signing_key.sign(&message);

        Ok(CacheAttestation {
            schema_version: CACHE_ATTESTATION_SCHEMA_VERSION,
            cache_key_hash,
            token_count,
            worker_id,
            timestamp_tick,
            signature: signature.to_bytes(),
        })
    }
}

/// Convenience function to create and sign an attestation.
///
/// # Arguments
///
/// * `cache_key_hash` - BLAKE3 hash of the cache lookup key
/// * `token_count` - Number of cached tokens
/// * `worker_id` - Worker identifier
/// * `timestamp_tick` - Logical tick
/// * `signing_key` - Ed25519 signing key (32-byte seed)
pub fn create_cache_attestation(
    cache_key_hash: &[u8; 32],
    token_count: u32,
    worker_id: &str,
    timestamp_tick: u64,
    signing_key: &[u8; 32],
) -> Result<CacheAttestation> {
    CacheAttestationBuilder::new()
        .cache_key_hash(cache_key_hash)
        .token_count(token_count)
        .worker_id(worker_id)
        .timestamp_tick(timestamp_tick)
        .build_and_sign(signing_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use rand::RngCore;

    fn generate_keypair() -> (SigningKey, [u8; 32]) {
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        let signing_key = SigningKey::from_bytes(&seed);
        let public_key = signing_key.verifying_key().to_bytes();
        (signing_key, public_key)
    }

    #[test]
    fn test_attestation_sign_verify() {
        let (signing_key, public_key) = generate_keypair();
        let cache_key = [0xAB; 32];

        let attestation = CacheAttestationBuilder::new()
            .cache_key_hash(&cache_key)
            .token_count(100)
            .worker_id("worker-001")
            .timestamp_tick(42)
            .build_and_sign(&signing_key.to_bytes())
            .expect("should sign");

        assert!(attestation.verify(&public_key).is_ok());
    }

    #[test]
    fn test_attestation_wrong_key_fails() {
        let (signing_key, _) = generate_keypair();
        let (_, wrong_public_key) = generate_keypair();
        let cache_key = [0xAB; 32];

        let attestation = CacheAttestationBuilder::new()
            .cache_key_hash(&cache_key)
            .token_count(100)
            .worker_id("worker-001")
            .timestamp_tick(42)
            .build_and_sign(&signing_key.to_bytes())
            .expect("should sign");

        assert!(attestation.verify(&wrong_public_key).is_err());
    }

    #[test]
    fn test_attestation_tampered_fails() {
        let (signing_key, public_key) = generate_keypair();
        let cache_key = [0xAB; 32];

        let mut attestation = CacheAttestationBuilder::new()
            .cache_key_hash(&cache_key)
            .token_count(100)
            .worker_id("worker-001")
            .timestamp_tick(42)
            .build_and_sign(&signing_key.to_bytes())
            .expect("should sign");

        // Tamper with token count
        attestation.token_count = 200;

        assert!(attestation.verify(&public_key).is_err());
    }

    #[test]
    fn test_attestation_deterministic() {
        let seed = [0x42; 32];
        let cache_key = [0xAB; 32];

        let attestation1 = CacheAttestationBuilder::new()
            .cache_key_hash(&cache_key)
            .token_count(100)
            .worker_id("worker-001")
            .timestamp_tick(42)
            .build_and_sign(&seed)
            .expect("should sign");

        let attestation2 = CacheAttestationBuilder::new()
            .cache_key_hash(&cache_key)
            .token_count(100)
            .worker_id("worker-001")
            .timestamp_tick(42)
            .build_and_sign(&seed)
            .expect("should sign");

        assert_eq!(attestation1, attestation2);
    }

    #[test]
    fn test_canonical_bytes_format() {
        let cache_key = [0x01; 32];
        let attestation = CacheAttestation {
            schema_version: 1,
            cache_key_hash: cache_key,
            token_count: 256,
            worker_id: "w1".to_string(),
            timestamp_tick: 1000,
            signature: [0; 64],
        };

        let bytes = attestation.canonical_bytes();

        // Check structure
        assert_eq!(bytes[0], 1); // schema_version
        assert_eq!(&bytes[1..33], &cache_key); // cache_key_hash
        assert_eq!(&bytes[33..37], &256u32.to_le_bytes()); // token_count
        assert_eq!(&bytes[37..41], &2u32.to_le_bytes()); // worker_id length
        assert_eq!(&bytes[41..43], b"w1"); // worker_id
        assert_eq!(&bytes[43..51], &1000u64.to_le_bytes()); // timestamp_tick
    }

    #[test]
    fn test_builder_missing_fields() {
        let seed = [0x42; 32];

        // Missing cache_key_hash
        let result = CacheAttestationBuilder::new()
            .token_count(100)
            .worker_id("w1")
            .timestamp_tick(42)
            .build_and_sign(&seed);
        assert!(result.is_err());

        // Missing token_count
        let result = CacheAttestationBuilder::new()
            .cache_key_hash(&[0; 32])
            .worker_id("w1")
            .timestamp_tick(42)
            .build_and_sign(&seed);
        assert!(result.is_err());

        // Missing worker_id
        let result = CacheAttestationBuilder::new()
            .cache_key_hash(&[0; 32])
            .token_count(100)
            .timestamp_tick(42)
            .build_and_sign(&seed);
        assert!(result.is_err());

        // Missing timestamp_tick
        let result = CacheAttestationBuilder::new()
            .cache_key_hash(&[0; 32])
            .token_count(100)
            .worker_id("w1")
            .build_and_sign(&seed);
        assert!(result.is_err());
    }

    #[test]
    fn test_convenience_function() {
        let (signing_key, public_key) = generate_keypair();
        let cache_key = [0xCD; 32];

        let attestation = create_cache_attestation(
            &cache_key,
            50,
            "worker-xyz",
            999,
            &signing_key.to_bytes(),
        )
        .expect("should create");

        assert!(attestation.verify(&public_key).is_ok());
        assert_eq!(attestation.token_count, 50);
        assert_eq!(attestation.worker_id, "worker-xyz");
        assert_eq!(attestation.timestamp_tick, 999);
    }
}
