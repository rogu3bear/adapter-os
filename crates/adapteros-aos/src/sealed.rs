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

// =============================================================================
// Binary Sealed Container Format
// =============================================================================

/// Binary sealed container header (144 bytes, 64-byte aligned).
///
/// ## Binary Layout
/// ```text
/// | Offset | Size | Field                              |
/// |--------|------|------------------------------------|
/// | 0      | 4    | Magic: "SEAL"                      |
/// | 4      | 1    | Version                            |
/// | 5      | 3    | Reserved                           |
/// | 8      | 32   | Container integrity hash (BLAKE3)  |
/// | 40     | 8    | Payload offset (u64 LE)            |
/// | 48     | 8    | Payload size (u64 LE)              |
/// | 56     | 8    | Manifest offset (u64 LE)           |
/// | 64     | 8    | Manifest size (u64 LE)             |
/// | 72     | 64   | Ed25519 signature                  |
/// | 136    | 8    | Reserved (alignment)               |
/// ```
pub const SEALED_HEADER_SIZE: usize = 144;

/// Header for binary sealed container
#[derive(Debug, Clone, Copy)]
pub struct SealedContainerHeader {
    /// Container format version
    pub version: u8,
    /// BLAKE3 hash of (version + manifest + payload)
    pub integrity_hash: [u8; 32],
    /// Offset to payload (weights) in the file
    pub payload_offset: u64,
    /// Size of payload in bytes
    pub payload_size: u64,
    /// Offset to manifest (JSON metadata) in the file
    pub manifest_offset: u64,
    /// Size of manifest in bytes
    pub manifest_size: u64,
    /// Ed25519 signature over integrity_hash
    pub signature: [u8; 64],
}

impl SealedContainerHeader {
    /// Parse header from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < SEALED_HEADER_SIZE {
            return Err(AosError::InvalidSealedData {
                reason: format!(
                    "Header too small: expected {} bytes, got {}",
                    SEALED_HEADER_SIZE,
                    bytes.len()
                ),
            });
        }

        // Verify magic
        if &bytes[0..4] != SEALED_MAGIC {
            return Err(AosError::InvalidSealedData {
                reason: "Invalid magic bytes: expected SEAL".to_string(),
            });
        }

        let version = bytes[4];
        if version != SEALED_CONTAINER_VERSION {
            return Err(AosError::InvalidSealedData {
                reason: format!(
                    "Unsupported version: expected {}, got {}",
                    SEALED_CONTAINER_VERSION, version
                ),
            });
        }

        let mut integrity_hash = [0u8; 32];
        integrity_hash.copy_from_slice(&bytes[8..40]);

        let payload_offset = u64::from_le_bytes(bytes[40..48].try_into().unwrap());
        let payload_size = u64::from_le_bytes(bytes[48..56].try_into().unwrap());
        let manifest_offset = u64::from_le_bytes(bytes[56..64].try_into().unwrap());
        let manifest_size = u64::from_le_bytes(bytes[64..72].try_into().unwrap());

        let mut signature = [0u8; 64];
        signature.copy_from_slice(&bytes[72..136]);

        Ok(Self {
            version,
            integrity_hash,
            payload_offset,
            payload_size,
            manifest_offset,
            manifest_size,
            signature,
        })
    }

    /// Serialize header to bytes
    pub fn to_bytes(&self) -> [u8; SEALED_HEADER_SIZE] {
        let mut bytes = [0u8; SEALED_HEADER_SIZE];
        bytes[0..4].copy_from_slice(SEALED_MAGIC);
        bytes[4] = self.version;
        // bytes[5..8] reserved (zeros)
        bytes[8..40].copy_from_slice(&self.integrity_hash);
        bytes[40..48].copy_from_slice(&self.payload_offset.to_le_bytes());
        bytes[48..56].copy_from_slice(&self.payload_size.to_le_bytes());
        bytes[56..64].copy_from_slice(&self.manifest_offset.to_le_bytes());
        bytes[64..72].copy_from_slice(&self.manifest_size.to_le_bytes());
        bytes[72..136].copy_from_slice(&self.signature);
        // bytes[136..144] reserved (zeros for alignment)
        bytes
    }
}

// =============================================================================
// Verified Adapter (Registry Entry)
// =============================================================================

/// A verified adapter ready for routing.
///
/// Contains the adapter bundle along with verification metadata.
///
/// ## Receipt Binding
///
/// Two hashes are relevant for receipt integration:
///
/// - `integrity_hash`: BLAKE3 hash of the entire sealed container (version + manifest + payload).
///   Used to prove that a specific sealed binary was loaded and verified. This hash should be
///   recorded in audit logs for tamper evidence.
///
/// - `bundle.weights_hash`: BLAKE3 hash of just the weights payload. This is what flows into
///   `ContextAdapterEntryV1.adapter_hash` and ultimately into the receipt's `context_digest`.
///
/// The separation ensures:
/// 1. Receipts bind to the adapter weights used during inference
/// 2. Audit logs can prove which sealed containers were verified
#[derive(Debug, Clone)]
pub struct VerifiedAdapter {
    /// The unsealed adapter bundle (contains weights_hash for receipt binding)
    pub bundle: AdapterBundle,
    /// Container integrity hash (BLAKE3 of version + manifest + payload).
    /// Used for audit/tamper-evidence, not for receipt binding.
    pub integrity_hash: B3Hash,
    /// Public key that signed this adapter
    pub signer_pubkey: [u8; 32],
    /// Whether adapter is available for routing
    pub available: bool,
}

impl VerifiedAdapter {
    /// Get the adapter ID
    pub fn adapter_id(&self) -> &str {
        &self.bundle.adapter_id
    }

    /// Get the container integrity hash.
    ///
    /// This hash covers the entire sealed container (version + manifest + payload)
    /// and is used for audit/tamper-evidence. For receipt binding, use
    /// `weights_hash_for_receipt()` instead.
    pub fn integrity_hash(&self) -> &B3Hash {
        &self.integrity_hash
    }

    /// Get the weights hash for receipt binding.
    ///
    /// This is the hash that flows into `ContextAdapterEntryV1.adapter_hash`
    /// and ultimately into the receipt's `context_digest`. Use this when
    /// registering the adapter for routing.
    pub fn weights_hash_for_receipt(&self) -> &B3Hash {
        &self.bundle.weights_hash
    }

    /// Mark as unavailable (e.g., after eviction)
    pub fn mark_unavailable(&mut self) {
        self.available = false;
    }
}

// =============================================================================
// Sealed Adapter Loader
// =============================================================================

/// Loader for sealed adapter containers.
///
/// Performs cryptographic verification before making adapters
/// available for inference routing.
pub struct SealedAdapterLoader {
    /// Trusted public keys for signature verification
    trusted_pubkeys: Vec<VerifyingKey>,
}

/// Reason for adapter rejection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionReason {
    /// Container format is invalid
    InvalidFormat,
    /// Integrity hash mismatch (tampering detected)
    IntegrityMismatch,
    /// Signature verification failed
    SignatureInvalid,
    /// No trusted signer found
    UntrustedSigner,
    /// Payload hash mismatch
    PayloadCorrupted,
    /// Manifest parse error
    ManifestInvalid,
}

impl RejectionReason {
    /// Get string representation for logging
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidFormat => "invalid_format",
            Self::IntegrityMismatch => "integrity_mismatch",
            Self::SignatureInvalid => "signature_invalid",
            Self::UntrustedSigner => "untrusted_signer",
            Self::PayloadCorrupted => "payload_corrupted",
            Self::ManifestInvalid => "manifest_invalid",
        }
    }
}

/// Result of attempting to load a sealed adapter
#[derive(Debug)]
pub enum LoadResult {
    /// Adapter loaded and verified successfully
    Verified(Box<VerifiedAdapter>),
    /// Adapter rejected due to verification failure
    Rejected {
        reason: RejectionReason,
        message: String,
        /// Expected hash (if applicable)
        expected: Option<B3Hash>,
        /// Actual hash (if applicable)
        actual: Option<B3Hash>,
    },
}

impl LoadResult {
    /// Returns true if adapter was successfully verified
    pub fn is_verified(&self) -> bool {
        matches!(self, Self::Verified(_))
    }

    /// Extract verified adapter, panics if rejected
    pub fn unwrap(self) -> VerifiedAdapter {
        match self {
            Self::Verified(adapter) => *adapter,
            Self::Rejected {
                reason, message, ..
            } => {
                panic!(
                    "Attempted to unwrap rejected adapter: {:?} - {}",
                    reason, message
                )
            }
        }
    }

    /// Convert to Result
    pub fn into_result(self) -> Result<VerifiedAdapter> {
        match self {
            Self::Verified(adapter) => Ok(*adapter),
            Self::Rejected {
                reason,
                message,
                expected,
                actual,
            } => {
                if let (Some(expected), Some(actual)) = (expected, actual) {
                    Err(AosError::AdapterHashMismatch {
                        adapter_id: String::new(),
                        expected,
                        actual,
                    })
                } else {
                    Err(AosError::Crypto(format!("{:?}: {}", reason, message)))
                }
            }
        }
    }
}

impl SealedAdapterLoader {
    /// Create a new loader with trusted public keys
    pub fn new(trusted_pubkeys: Vec<VerifyingKey>) -> Self {
        Self { trusted_pubkeys }
    }

    /// Load and verify a sealed adapter from bytes.
    ///
    /// # Steps
    /// 1. Read sealed container header containing metadata and integrity_hash
    /// 2. Compute actual_hash over container payload using BLAKE3
    /// 3. Compare actual_hash against declared integrity_hash
    /// 4. If match, deserialize adapter weights and metadata
    /// 5. Return verified adapter ready for registry
    ///
    /// # Stop Conditions
    /// - Integrity verification succeeds and adapter loads
    /// - Hash mismatch detected (reject adapter)
    /// - Container format invalid
    pub fn load_from_bytes(&self, bytes: &[u8]) -> LoadResult {
        // Step 1: Parse header
        let header = match SealedContainerHeader::from_bytes(bytes) {
            Ok(h) => h,
            Err(e) => {
                return LoadResult::Rejected {
                    reason: RejectionReason::InvalidFormat,
                    message: e.to_string(),
                    expected: None,
                    actual: None,
                };
            }
        };

        // Validate offsets are within bounds
        let payload_end = header.payload_offset.saturating_add(header.payload_size);
        let manifest_end = header.manifest_offset.saturating_add(header.manifest_size);

        if payload_end as usize > bytes.len() || manifest_end as usize > bytes.len() {
            return LoadResult::Rejected {
                reason: RejectionReason::InvalidFormat,
                message: "Payload or manifest extends beyond file bounds".to_string(),
                expected: None,
                actual: None,
            };
        }

        let payload = &bytes[header.payload_offset as usize..payload_end as usize];
        let manifest_bytes = &bytes[header.manifest_offset as usize..manifest_end as usize];

        // Step 2: Compute actual integrity hash over (version + manifest + payload)
        let actual_hash = Self::compute_integrity_hash(header.version, manifest_bytes, payload);

        // Step 3: Compare against declared integrity_hash
        let declared_hash = B3Hash::from_bytes(header.integrity_hash);
        if actual_hash != declared_hash {
            return LoadResult::Rejected {
                reason: RejectionReason::IntegrityMismatch,
                message: format!(
                    "Container integrity hash mismatch: tampering detected. Expected {}, got {}",
                    declared_hash.to_hex(),
                    actual_hash.to_hex()
                ),
                expected: Some(declared_hash),
                actual: Some(actual_hash),
            };
        }

        // Step 4a: Verify signature over integrity_hash
        let signature = Signature::from_bytes(&header.signature);

        let mut signer_found = false;
        let mut signer_pubkey = [0u8; 32];

        for pubkey in &self.trusted_pubkeys {
            if pubkey.verify(actual_hash.as_bytes(), &signature).is_ok() {
                signer_found = true;
                signer_pubkey = pubkey.to_bytes();
                break;
            }
        }

        if !signer_found {
            return LoadResult::Rejected {
                reason: RejectionReason::UntrustedSigner,
                message: "No trusted public key verified the signature".to_string(),
                expected: None,
                actual: None,
            };
        }

        // Step 4b: Parse manifest
        let manifest: AdapterMetadata = match serde_json::from_slice(manifest_bytes) {
            Ok(m) => m,
            Err(e) => {
                return LoadResult::Rejected {
                    reason: RejectionReason::ManifestInvalid,
                    message: format!("Failed to parse manifest: {}", e),
                    expected: None,
                    actual: None,
                };
            }
        };

        // Step 4c: Compute and verify payload hash
        let weights_hash = B3Hash::hash(payload);

        // Step 5: Construct verified adapter bundle
        let bundle = AdapterBundle {
            adapter_id: manifest.name.clone(),
            base_model_hash: B3Hash::zero(), // Extracted from manifest if present
            metadata: manifest,
            weights_hash,
            weights_data: payload.to_vec(),
            config_hash: B3Hash::hash(manifest_bytes),
            config_data: manifest_bytes.to_vec(),
        };

        let verified = VerifiedAdapter {
            bundle,
            integrity_hash: actual_hash,
            signer_pubkey,
            available: true,
        };

        LoadResult::Verified(Box::new(verified))
    }

    /// Load and verify a sealed adapter from a file path.
    pub fn load_from_file(&self, path: impl AsRef<Path>) -> LoadResult {
        let bytes = match std::fs::read(path.as_ref()) {
            Ok(b) => b,
            Err(e) => {
                return LoadResult::Rejected {
                    reason: RejectionReason::InvalidFormat,
                    message: format!("Failed to read file: {}", e),
                    expected: None,
                    actual: None,
                };
            }
        };

        self.load_from_bytes(&bytes)
    }

    /// Compute integrity hash over container contents.
    fn compute_integrity_hash(version: u8, manifest: &[u8], payload: &[u8]) -> B3Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[version]);
        hasher.update(manifest);
        hasher.update(payload);
        B3Hash::from_bytes(hasher.finalize().into())
    }

    /// Add a trusted public key
    pub fn add_trusted_key(&mut self, pubkey: VerifyingKey) {
        if !self
            .trusted_pubkeys
            .iter()
            .any(|k| k.to_bytes() == pubkey.to_bytes())
        {
            self.trusted_pubkeys.push(pubkey);
        }
    }

    /// Check if a public key is trusted
    pub fn is_trusted(&self, pubkey: &VerifyingKey) -> bool {
        self.trusted_pubkeys
            .iter()
            .any(|k| k.to_bytes() == pubkey.to_bytes())
    }
}

// =============================================================================
// Binary Sealed Container Writer
// =============================================================================

impl SealedAdapterContainer {
    /// Write sealed container to binary format.
    ///
    /// Creates a binary container with:
    /// - 128-byte header with integrity hash and signature
    /// - Manifest JSON (metadata)
    /// - Payload (weights)
    pub fn write_binary(&self, path: impl AsRef<Path>) -> Result<u64> {
        let manifest_json = serde_json::to_vec(&self.manifest.metadata)?;
        let payload_data = &self.payload.weights_data;

        // Compute integrity hash
        let integrity_hash =
            Self::compute_container_hash(self.version, &self.manifest, &self.payload);

        // Layout: header (128) + manifest + payload
        let manifest_offset = SEALED_HEADER_SIZE as u64;
        let manifest_size = manifest_json.len() as u64;
        let payload_offset = manifest_offset + manifest_size;
        let payload_size = payload_data.len() as u64;

        let header = SealedContainerHeader {
            version: self.version,
            integrity_hash: *integrity_hash.as_bytes(),
            payload_offset,
            payload_size,
            manifest_offset,
            manifest_size,
            signature: self.manifest.signature,
        };

        let header_bytes = header.to_bytes();
        let total_size = SEALED_HEADER_SIZE as u64 + manifest_size + payload_size;

        // Write atomically
        let temp_path = path.as_ref().with_extension("sealed.tmp");
        let mut file = std::fs::File::create(&temp_path)
            .map_err(|e| AosError::Io(format!("Failed to create temp file: {}", e)))?;

        use std::io::Write;
        file.write_all(&header_bytes)
            .map_err(|e| AosError::Io(format!("Failed to write header: {}", e)))?;
        file.write_all(&manifest_json)
            .map_err(|e| AosError::Io(format!("Failed to write manifest: {}", e)))?;
        file.write_all(payload_data)
            .map_err(|e| AosError::Io(format!("Failed to write payload: {}", e)))?;
        file.sync_all()
            .map_err(|e| AosError::Io(format!("Failed to sync: {}", e)))?;

        std::fs::rename(&temp_path, path.as_ref())
            .map_err(|e| AosError::Io(format!("Failed to rename: {}", e)))?;

        Ok(total_size)
    }

    /// Read sealed container from binary format.
    pub fn read_binary(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = std::fs::read(path.as_ref())
            .map_err(|e| AosError::Io(format!("Failed to read file: {}", e)))?;

        Self::from_binary_bytes(&bytes)
    }

    /// Parse sealed container from binary bytes.
    pub fn from_binary_bytes(bytes: &[u8]) -> Result<Self> {
        let header = SealedContainerHeader::from_bytes(bytes)?;

        let payload_end = header.payload_offset.saturating_add(header.payload_size);
        let manifest_end = header.manifest_offset.saturating_add(header.manifest_size);

        if payload_end as usize > bytes.len() || manifest_end as usize > bytes.len() {
            return Err(AosError::InvalidSealedData {
                reason: "Payload or manifest extends beyond file".to_string(),
            });
        }

        let manifest_bytes = &bytes[header.manifest_offset as usize..manifest_end as usize];
        let payload_bytes = &bytes[header.payload_offset as usize..payload_end as usize];

        let metadata: AdapterMetadata = serde_json::from_slice(manifest_bytes)?;

        let weights_hash = B3Hash::hash(payload_bytes);
        let config_hash = B3Hash::hash(manifest_bytes);

        let payload = AdapterPayload {
            weights_hash,
            weights_data: payload_bytes.to_vec(),
            config_hash,
            config_data: manifest_bytes.to_vec(),
        };

        // Reconstruct manifest (signature from header)
        let manifest = SignedManifest {
            adapter_id: metadata.name.clone(),
            base_model_hash: B3Hash::zero(),
            payload_hash: payload.compute_hash(),
            metadata,
            signature: header.signature,
            signer_pubkey: [0u8; 32], // Will be verified during load
        };

        let container_hash = B3Hash::from_bytes(header.integrity_hash);

        Ok(Self {
            version: header.version,
            container_hash,
            manifest,
            payload,
            created_at: String::new(),
            sealer_pubkey: [0u8; 32],
        })
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

    // =========================================================================
    // SealedAdapterLoader Tests
    // =========================================================================

    fn create_test_sealed_bytes(signing_key: &SigningKey) -> Vec<u8> {
        let manifest = AdapterMetadata {
            name: "test-adapter".to_string(),
            version: "1.0.0".to_string(),
            ..Default::default()
        };
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let payload = b"test weights data".to_vec();

        // Compute integrity hash
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[SEALED_CONTAINER_VERSION]);
        hasher.update(&manifest_json);
        hasher.update(&payload);
        let integrity_hash: [u8; 32] = hasher.finalize().into();

        // Sign the integrity hash
        let signature = signing_key.sign(&integrity_hash);

        // Build binary container
        let manifest_offset = SEALED_HEADER_SIZE as u64;
        let manifest_size = manifest_json.len() as u64;
        let payload_offset = manifest_offset + manifest_size;
        let payload_size = payload.len() as u64;

        let header = SealedContainerHeader {
            version: SEALED_CONTAINER_VERSION,
            integrity_hash,
            payload_offset,
            payload_size,
            manifest_offset,
            manifest_size,
            signature: signature.to_bytes(),
        };

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&header.to_bytes());
        bytes.extend_from_slice(&manifest_json);
        bytes.extend_from_slice(&payload);

        bytes
    }

    #[test]
    fn test_sealed_loader_success() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let trusted_keys = vec![signing_key.verifying_key()];
        let loader = SealedAdapterLoader::new(trusted_keys);

        let bytes = create_test_sealed_bytes(&signing_key);
        let result = loader.load_from_bytes(&bytes);

        assert!(result.is_verified());
        let adapter = result.unwrap();
        assert_eq!(adapter.adapter_id(), "test-adapter");
        assert!(adapter.available);
    }

    #[test]
    fn test_sealed_loader_untrusted_key() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let wrong_key = SigningKey::generate(&mut OsRng);
        let trusted_keys = vec![wrong_key.verifying_key()];
        let loader = SealedAdapterLoader::new(trusted_keys);

        let bytes = create_test_sealed_bytes(&signing_key);
        let result = loader.load_from_bytes(&bytes);

        match result {
            LoadResult::Rejected { reason, .. } => {
                assert_eq!(reason, RejectionReason::UntrustedSigner);
            }
            _ => panic!("Expected rejection"),
        }
    }

    #[test]
    fn test_sealed_loader_tampered_payload() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let trusted_keys = vec![signing_key.verifying_key()];
        let loader = SealedAdapterLoader::new(trusted_keys);

        let mut bytes = create_test_sealed_bytes(&signing_key);

        // Tamper with payload (last bytes)
        let len = bytes.len();
        bytes[len - 1] ^= 0xFF;

        let result = loader.load_from_bytes(&bytes);

        match result {
            LoadResult::Rejected {
                reason,
                expected,
                actual,
                ..
            } => {
                assert_eq!(reason, RejectionReason::IntegrityMismatch);
                assert!(expected.is_some());
                assert!(actual.is_some());
                assert_ne!(expected, actual);
            }
            _ => panic!("Expected rejection due to integrity mismatch"),
        }
    }

    #[test]
    fn test_sealed_loader_invalid_magic() {
        let loader = SealedAdapterLoader::new(vec![]);

        let mut bytes = vec![0u8; SEALED_HEADER_SIZE + 100];
        bytes[0..4].copy_from_slice(b"FAKE"); // Wrong magic

        let result = loader.load_from_bytes(&bytes);

        match result {
            LoadResult::Rejected { reason, .. } => {
                assert_eq!(reason, RejectionReason::InvalidFormat);
            }
            _ => panic!("Expected rejection due to invalid format"),
        }
    }

    #[test]
    fn test_sealed_header_roundtrip() {
        let header = SealedContainerHeader {
            version: SEALED_CONTAINER_VERSION,
            integrity_hash: [42u8; 32],
            payload_offset: 1000,
            payload_size: 500,
            manifest_offset: 128,
            manifest_size: 872,
            signature: [7u8; 64],
        };

        let bytes = header.to_bytes();
        let parsed = SealedContainerHeader::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.version, header.version);
        assert_eq!(parsed.integrity_hash, header.integrity_hash);
        assert_eq!(parsed.payload_offset, header.payload_offset);
        assert_eq!(parsed.payload_size, header.payload_size);
        assert_eq!(parsed.manifest_offset, header.manifest_offset);
        assert_eq!(parsed.manifest_size, header.manifest_size);
        assert_eq!(parsed.signature, header.signature);
    }

    #[test]
    fn test_verified_adapter_mark_unavailable() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let trusted_keys = vec![signing_key.verifying_key()];
        let loader = SealedAdapterLoader::new(trusted_keys);

        let bytes = create_test_sealed_bytes(&signing_key);
        let result = loader.load_from_bytes(&bytes);

        let mut adapter = result.unwrap();
        assert!(adapter.available);

        adapter.mark_unavailable();
        assert!(!adapter.available);
    }

    #[test]
    fn test_load_result_into_result() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let trusted_keys = vec![signing_key.verifying_key()];
        let loader = SealedAdapterLoader::new(trusted_keys);

        // Success case
        let bytes = create_test_sealed_bytes(&signing_key);
        let result = loader.load_from_bytes(&bytes);
        assert!(result.into_result().is_ok());

        // Failure case - empty trusted keys
        let loader_no_keys = SealedAdapterLoader::new(vec![]);
        let bytes = create_test_sealed_bytes(&signing_key);
        let result = loader_no_keys.load_from_bytes(&bytes);
        assert!(result.into_result().is_err());
    }

    #[test]
    fn test_rejection_reason_as_str() {
        assert_eq!(RejectionReason::InvalidFormat.as_str(), "invalid_format");
        assert_eq!(
            RejectionReason::IntegrityMismatch.as_str(),
            "integrity_mismatch"
        );
        assert_eq!(
            RejectionReason::SignatureInvalid.as_str(),
            "signature_invalid"
        );
        assert_eq!(
            RejectionReason::UntrustedSigner.as_str(),
            "untrusted_signer"
        );
        assert_eq!(
            RejectionReason::PayloadCorrupted.as_str(),
            "payload_corrupted"
        );
        assert_eq!(
            RejectionReason::ManifestInvalid.as_str(),
            "manifest_invalid"
        );
    }
}
