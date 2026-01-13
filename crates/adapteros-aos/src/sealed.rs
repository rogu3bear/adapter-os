//! Sealed Adapter Containers (Patent 3535886.0002 Claims 12-13)
//!
//! This module implements cryptographically sealed adapter containers with
//! signed manifests and integrity verification.
//!
//! ## Features
//!
//! - **Integrity Hash**: BLAKE3 hash of entire container contents
//! - **Signed Manifest**: Ed25519 signature over adapter metadata and weights hash
//! - **Verification**: Verify container hasn't been tampered with
//! - **Sealing Authority**: Track who sealed the container
//!
//! ## Example
//!
//! ```ignore
//! use adapteros_aos::sealed::{SealedAdapterContainer, AdapterPayload};
//!
//! // Seal an adapter
//! let container = SealedAdapterContainer::seal(
//!     &adapter_bundle,
//!     &signing_key,
//! )?;
//!
//! // Save to disk
//! container.write_to_file("adapter.sealed.aos")?;
//!
//! // Load and verify
//! let loaded = SealedAdapterContainer::read_from_file("adapter.sealed.aos")?;
//! loaded.verify(&[trusted_pubkey])?;
//!
//! // Extract adapter
//! let adapter = loaded.unseal(&[trusted_pubkey])?;
//! ```

use adapteros_core::{AosError, B3Hash, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Sealed container format version
pub const SEALED_CONTAINER_VERSION: u8 = 1;

/// Magic bytes for sealed container format: "SEAL"
pub const SEALED_MAGIC: &[u8; 4] = b"SEAL";

// =============================================================================
// Sealed Adapter Container
// =============================================================================

/// Sealed adapter container format.
///
/// A sealed container provides cryptographic guarantees about adapter integrity:
/// 1. Container hash covers all contents
/// 2. Manifest is signed by a trusted authority
/// 3. Payload hash is bound to the signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealedAdapterContainer {
    /// Container format version
    pub version: u8,
    /// Container integrity hash (BLAKE3 of version + manifest + payload)
    pub container_hash: B3Hash,
    /// Signed manifest with adapter metadata
    pub manifest: SignedManifest,
    /// Adapter payload (weights and config)
    pub payload: AdapterPayload,
    /// Creation timestamp (ISO 8601)
    pub created_at: String,
    /// Sealing authority public key (for audit)
    pub sealer_pubkey: [u8; 32],
}

impl SealedAdapterContainer {
    /// Seal an adapter bundle into a container.
    ///
    /// # Arguments
    /// * `bundle` - The adapter data to seal
    /// * `signing_key` - Ed25519 signing key for the manifest
    ///
    /// # Returns
    /// A sealed container with signed manifest and integrity hash.
    pub fn seal(bundle: &AdapterBundle, signing_key: &SigningKey) -> Result<Self> {
        let now = chrono::Utc::now().to_rfc3339();

        // Create payload
        let payload = AdapterPayload {
            weights_hash: bundle.weights_hash,
            weights_data: bundle.weights_data.clone(),
            config_hash: bundle.config_hash,
            config_data: bundle.config_data.clone(),
        };

        // Create and sign manifest
        let manifest = SignedManifest::new(
            bundle.adapter_id.clone(),
            bundle.base_model_hash,
            payload.compute_hash(),
            bundle.metadata.clone(),
            signing_key,
        )?;

        // Compute container hash
        let container_hash =
            Self::compute_container_hash(SEALED_CONTAINER_VERSION, &manifest, &payload);

        Ok(Self {
            version: SEALED_CONTAINER_VERSION,
            container_hash,
            manifest,
            payload,
            created_at: now,
            sealer_pubkey: signing_key.verifying_key().to_bytes(),
        })
    }

    /// Verify container integrity and manifest signature.
    ///
    /// # Arguments
    /// * `trusted_pubkeys` - List of trusted signing authority public keys
    ///
    /// # Returns
    /// Ok(()) if verification passes, Err otherwise.
    pub fn verify(&self, trusted_pubkeys: &[VerifyingKey]) -> Result<()> {
        // 1. Verify container hash
        let expected_hash =
            Self::compute_container_hash(self.version, &self.manifest, &self.payload);
        if self.container_hash != expected_hash {
            return Err(AosError::Crypto(
                "Container hash mismatch: content has been modified".to_string(),
            ));
        }

        // 2. Verify payload hash matches manifest
        let payload_hash = self.payload.compute_hash();
        if payload_hash != self.manifest.payload_hash {
            return Err(AosError::Crypto(
                "Payload hash mismatch: weights have been modified".to_string(),
            ));
        }

        // 3. Verify manifest signature with a trusted key
        self.manifest.verify(trusted_pubkeys)?;

        Ok(())
    }

    /// Extract adapter after verification.
    ///
    /// # Arguments
    /// * `trusted_pubkeys` - List of trusted signing authority public keys
    ///
    /// # Returns
    /// The adapter bundle if verification passes.
    pub fn unseal(&self, trusted_pubkeys: &[VerifyingKey]) -> Result<AdapterBundle> {
        // Verify first
        self.verify(trusted_pubkeys)?;

        // Extract bundle
        Ok(AdapterBundle {
            adapter_id: self.manifest.adapter_id.clone(),
            base_model_hash: self.manifest.base_model_hash,
            metadata: self.manifest.metadata.clone(),
            weights_hash: self.payload.weights_hash,
            weights_data: self.payload.weights_data.clone(),
            config_hash: self.payload.config_hash,
            config_data: self.payload.config_data.clone(),
        })
    }

    /// Compute container hash from components.
    fn compute_container_hash(
        version: u8,
        manifest: &SignedManifest,
        payload: &AdapterPayload,
    ) -> B3Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[version]);
        hasher.update(manifest.compute_unsigned_hash().as_bytes());
        hasher.update(&manifest.signature);
        hasher.update(payload.compute_hash().as_bytes());
        B3Hash::from_bytes(hasher.finalize().into())
    }

    /// Write sealed container to a file.
    pub fn write_to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| AosError::Io(format!("Failed to serialize container: {}", e)))?;
        std::fs::write(path, json)
            .map_err(|e| AosError::Io(format!("Failed to write container: {}", e)))?;
        Ok(())
    }

    /// Read sealed container from a file.
    pub fn read_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let data = std::fs::read(path)
            .map_err(|e| AosError::Io(format!("Failed to read container: {}", e)))?;
        serde_json::from_slice(&data)
            .map_err(|e| AosError::Io(format!("Failed to deserialize container: {}", e)))
    }

    /// Get the adapter ID from this container.
    pub fn adapter_id(&self) -> &str {
        &self.manifest.adapter_id
    }

    /// Get the base model hash this adapter is compatible with.
    pub fn base_model_hash(&self) -> &B3Hash {
        &self.manifest.base_model_hash
    }

    /// Check if this container was sealed by a specific public key.
    pub fn was_sealed_by(&self, pubkey: &VerifyingKey) -> bool {
        self.sealer_pubkey == pubkey.to_bytes()
    }
}

// =============================================================================
// Signed Manifest
// =============================================================================

/// Signed manifest containing adapter metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedManifest {
    /// Adapter identifier
    pub adapter_id: String,
    /// Base model compatibility hash
    pub base_model_hash: B3Hash,
    /// Payload hash (covers weights and config)
    pub payload_hash: B3Hash,
    /// Adapter metadata
    pub metadata: AdapterMetadata,
    /// Ed25519 signature over manifest fields
    #[serde(with = "signature_serde")]
    pub signature: [u8; 64],
    /// Public key that signed this manifest (for verification)
    pub signer_pubkey: [u8; 32],
}

impl SignedManifest {
    /// Create a new signed manifest.
    pub fn new(
        adapter_id: String,
        base_model_hash: B3Hash,
        payload_hash: B3Hash,
        metadata: AdapterMetadata,
        signing_key: &SigningKey,
    ) -> Result<Self> {
        let mut manifest = Self {
            adapter_id,
            base_model_hash,
            payload_hash,
            metadata,
            signature: [0u8; 64],
            signer_pubkey: signing_key.verifying_key().to_bytes(),
        };

        // Sign the unsigned hash
        let unsigned_hash = manifest.compute_unsigned_hash();
        let signature = signing_key.sign(unsigned_hash.as_bytes());
        manifest.signature = signature.to_bytes();

        Ok(manifest)
    }

    /// Compute hash of manifest fields (excluding signature).
    pub fn compute_unsigned_hash(&self) -> B3Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.adapter_id.as_bytes());
        hasher.update(b"\x00");
        hasher.update(self.base_model_hash.as_bytes());
        hasher.update(self.payload_hash.as_bytes());
        // Hash metadata deterministically
        if let Ok(metadata_json) = serde_json::to_string(&self.metadata) {
            hasher.update(metadata_json.as_bytes());
        }
        hasher.update(&self.signer_pubkey);
        B3Hash::from_bytes(hasher.finalize().into())
    }

    /// Verify manifest signature against trusted public keys.
    pub fn verify(&self, trusted_pubkeys: &[VerifyingKey]) -> Result<()> {
        let unsigned_hash = self.compute_unsigned_hash();
        let signature = Signature::from_bytes(&self.signature);

        // Try each trusted key
        for pubkey in trusted_pubkeys {
            if pubkey.verify(unsigned_hash.as_bytes(), &signature).is_ok() {
                // Also verify the signer matches
                if pubkey.to_bytes() == self.signer_pubkey {
                    return Ok(());
                }
            }
        }

        Err(AosError::Crypto(
            "Manifest signature verification failed: no trusted key matched".to_string(),
        ))
    }
}

// =============================================================================
// Adapter Payload
// =============================================================================

/// Adapter payload containing weights and config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterPayload {
    /// BLAKE3 hash of weights data
    pub weights_hash: B3Hash,
    /// Raw weights data (may be compressed)
    #[serde(with = "base64_serde")]
    pub weights_data: Vec<u8>,
    /// BLAKE3 hash of config data
    pub config_hash: B3Hash,
    /// Config data (JSON)
    #[serde(with = "base64_serde")]
    pub config_data: Vec<u8>,
}

impl AdapterPayload {
    /// Compute hash of entire payload.
    pub fn compute_hash(&self) -> B3Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.weights_hash.as_bytes());
        hasher.update(&self.weights_data);
        hasher.update(self.config_hash.as_bytes());
        hasher.update(&self.config_data);
        B3Hash::from_bytes(hasher.finalize().into())
    }

    /// Verify payload integrity.
    pub fn verify(&self) -> Result<()> {
        let weights_hash = B3Hash::hash(&self.weights_data);
        if weights_hash != self.weights_hash {
            return Err(AosError::Crypto("Weights hash mismatch".to_string()));
        }

        let config_hash = B3Hash::hash(&self.config_data);
        if config_hash != self.config_hash {
            return Err(AosError::Crypto("Config hash mismatch".to_string()));
        }

        Ok(())
    }
}

// =============================================================================
// Adapter Metadata
// =============================================================================

/// Adapter metadata for manifest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdapterMetadata {
    /// Human-readable name
    #[serde(default)]
    pub name: String,
    /// Description
    #[serde(default)]
    pub description: String,
    /// Version string
    #[serde(default)]
    pub version: String,
    /// Author or organization
    #[serde(default)]
    pub author: String,
    /// License identifier (SPDX)
    #[serde(default)]
    pub license: String,
    /// Adapter tier (tier_0, tier_1, tier_2)
    #[serde(default)]
    pub tier: String,
    /// Language specializations
    #[serde(default)]
    pub languages: Vec<String>,
    /// Framework specializations
    #[serde(default)]
    pub frameworks: Vec<String>,
    /// LoRA rank
    #[serde(default)]
    pub lora_rank: Option<u32>,
    /// LoRA alpha
    #[serde(default)]
    pub lora_alpha: Option<f32>,
    /// Training timestamp
    #[serde(default)]
    pub trained_at: Option<String>,
    /// Training dataset hash
    #[serde(default)]
    pub dataset_hash: Option<String>,
    /// Custom tags
    #[serde(default)]
    pub tags: Vec<String>,
}

// =============================================================================
// Adapter Bundle (Input/Output)
// =============================================================================

/// Adapter bundle for sealing/unsealing.
#[derive(Debug, Clone)]
pub struct AdapterBundle {
    /// Adapter identifier
    pub adapter_id: String,
    /// Base model compatibility hash
    pub base_model_hash: B3Hash,
    /// Adapter metadata
    pub metadata: AdapterMetadata,
    /// Hash of weights data
    pub weights_hash: B3Hash,
    /// Raw weights data
    pub weights_data: Vec<u8>,
    /// Hash of config data
    pub config_hash: B3Hash,
    /// Config data (JSON)
    pub config_data: Vec<u8>,
}

impl AdapterBundle {
    /// Create a new adapter bundle.
    pub fn new(
        adapter_id: String,
        base_model_hash: B3Hash,
        metadata: AdapterMetadata,
        weights_data: Vec<u8>,
        config_data: Vec<u8>,
    ) -> Self {
        let weights_hash = B3Hash::hash(&weights_data);
        let config_hash = B3Hash::hash(&config_data);

        Self {
            adapter_id,
            base_model_hash,
            metadata,
            weights_hash,
            weights_data,
            config_hash,
            config_data,
        }
    }
}

// =============================================================================
// Serde Helpers
// =============================================================================

mod base64_serde {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        STANDARD.encode(bytes).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(deserializer)?;
        STANDARD.decode(&s).map_err(serde::de::Error::custom)
    }
}

mod signature_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error> {
        hex::encode(bytes).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 64], D::Error> {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("invalid signature length"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn make_test_bundle() -> AdapterBundle {
        AdapterBundle::new(
            "test-adapter".to_string(),
            B3Hash::hash(b"base-model-v1"),
            AdapterMetadata {
                name: "Test Adapter".to_string(),
                version: "1.0.0".to_string(),
                ..Default::default()
            },
            b"fake weights data".to_vec(),
            b"{\"config\": true}".to_vec(),
        )
    }

    #[test]
    fn test_seal_and_verify() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let bundle = make_test_bundle();

        let container = SealedAdapterContainer::seal(&bundle, &signing_key).unwrap();

        // Verify with correct key
        let trusted_keys = vec![signing_key.verifying_key()];
        assert!(container.verify(&trusted_keys).is_ok());
    }

    #[test]
    fn test_verify_wrong_key() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let wrong_key = SigningKey::generate(&mut OsRng);
        let bundle = make_test_bundle();

        let container = SealedAdapterContainer::seal(&bundle, &signing_key).unwrap();

        // Verify with wrong key should fail
        let wrong_keys = vec![wrong_key.verifying_key()];
        assert!(container.verify(&wrong_keys).is_err());
    }

    #[test]
    fn test_unseal() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let bundle = make_test_bundle();

        let container = SealedAdapterContainer::seal(&bundle, &signing_key).unwrap();

        let trusted_keys = vec![signing_key.verifying_key()];
        let extracted = container.unseal(&trusted_keys).unwrap();

        assert_eq!(extracted.adapter_id, bundle.adapter_id);
        assert_eq!(extracted.weights_data, bundle.weights_data);
    }

    #[test]
    fn test_tampered_payload() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let bundle = make_test_bundle();

        let mut container = SealedAdapterContainer::seal(&bundle, &signing_key).unwrap();

        // Tamper with payload
        container.payload.weights_data = b"tampered data".to_vec();

        let trusted_keys = vec![signing_key.verifying_key()];
        assert!(container.verify(&trusted_keys).is_err());
    }

    #[test]
    fn test_tampered_container_hash() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let bundle = make_test_bundle();

        let mut container = SealedAdapterContainer::seal(&bundle, &signing_key).unwrap();

        // Tamper with container hash
        container.container_hash = B3Hash::hash(b"tampered");

        let trusted_keys = vec![signing_key.verifying_key()];
        assert!(container.verify(&trusted_keys).is_err());
    }

    #[test]
    fn test_payload_verify() {
        let payload = AdapterPayload {
            weights_hash: B3Hash::hash(b"weights"),
            weights_data: b"weights".to_vec(),
            config_hash: B3Hash::hash(b"config"),
            config_data: b"config".to_vec(),
        };

        assert!(payload.verify().is_ok());
    }

    #[test]
    fn test_metadata_serialization() {
        let metadata = AdapterMetadata {
            name: "Test".to_string(),
            version: "1.0".to_string(),
            lora_rank: Some(16),
            languages: vec!["rust".to_string(), "python".to_string()],
            ..Default::default()
        };

        let json = serde_json::to_string(&metadata).unwrap();
        let parsed: AdapterMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, metadata.name);
        assert_eq!(parsed.lora_rank, Some(16));
    }

    #[test]
    fn test_was_sealed_by() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let other_key = SigningKey::generate(&mut OsRng);
        let bundle = make_test_bundle();

        let container = SealedAdapterContainer::seal(&bundle, &signing_key).unwrap();

        assert!(container.was_sealed_by(&signing_key.verifying_key()));
        assert!(!container.was_sealed_by(&other_key.verifying_key()));
    }
}
