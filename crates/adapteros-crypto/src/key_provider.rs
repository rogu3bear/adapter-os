//! Key provider abstraction for cryptographic operations
//!
//! This module provides a unified interface for key management across different
//! backends (OS keychains, KMS/HSM, file-based for development).

use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Supported key algorithms
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyAlgorithm {
    /// Ed25519 signature algorithm
    Ed25519,
    /// AES-256-GCM encryption
    Aes256Gcm,
    /// ChaCha20-Poly1305 encryption
    ChaCha20Poly1305,
}

impl fmt::Display for KeyAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyAlgorithm::Ed25519 => write!(f, "ed25519"),
            KeyAlgorithm::Aes256Gcm => write!(f, "aes256gcm"),
            KeyAlgorithm::ChaCha20Poly1305 => write!(f, "chacha20poly1305"),
        }
    }
}

/// Handle to a cryptographic key stored in a provider
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyHandle {
    /// Provider-specific key identifier
    pub provider_id: String,
    /// Key algorithm
    pub algorithm: KeyAlgorithm,
    /// Public key bytes (if applicable)
    pub public_key: Option<Vec<u8>>,
}

impl KeyHandle {
    /// Create a new key handle
    pub fn new(provider_id: String, algorithm: KeyAlgorithm) -> Self {
        Self {
            provider_id,
            algorithm,
            public_key: None,
        }
    }

    /// Create a new key handle with public key
    pub fn with_public_key(
        provider_id: String,
        algorithm: KeyAlgorithm,
        public_key: Vec<u8>,
    ) -> Self {
        Self {
            provider_id,
            algorithm,
            public_key: Some(public_key),
        }
    }
}

/// Receipt documenting a key rotation operation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RotationReceipt {
    /// Key identifier that was rotated
    pub key_id: String,
    /// Previous key handle (before rotation)
    pub previous_key: KeyHandle,
    /// New key handle (after rotation)
    pub new_key: KeyHandle,
    /// Timestamp of rotation (Unix timestamp)
    pub timestamp: u64,
    /// Cryptographic signature of the receipt
    pub signature: Vec<u8>,
}

impl RotationReceipt {
    /// Create a new rotation receipt
    pub fn new(
        key_id: String,
        previous_key: KeyHandle,
        new_key: KeyHandle,
        timestamp: u64,
        signature: Vec<u8>,
    ) -> Self {
        Self {
            key_id,
            previous_key,
            new_key,
            timestamp,
            signature,
        }
    }
}

/// Attestation of a key provider's state and configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderAttestation {
    /// Provider type identifier
    pub provider_type: String,
    /// Provider version or fingerprint
    pub fingerprint: String,
    /// Policy hash that this provider enforces
    pub policy_hash: String,
    /// Timestamp of attestation
    pub timestamp: u64,
    /// Cryptographic signature
    pub signature: Vec<u8>,
}

impl ProviderAttestation {
    /// Create a new provider attestation
    pub fn new(
        provider_type: String,
        fingerprint: String,
        policy_hash: String,
        timestamp: u64,
        signature: Vec<u8>,
    ) -> Self {
        Self {
            provider_type,
            fingerprint,
            policy_hash,
            timestamp,
            signature,
        }
    }
}

/// Unified interface for cryptographic key operations across different backends
#[async_trait::async_trait]
pub trait KeyProvider: Send + Sync {
    /// Generate a new key with the specified algorithm
    async fn generate(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle>;

    /// Sign a message using the specified key
    async fn sign(&self, key_id: &str, msg: &[u8]) -> Result<Vec<u8>>;

    /// Encrypt plaintext using the specified key (AEAD)
    async fn seal(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>>;

    /// Decrypt ciphertext using the specified key (AEAD)
    async fn unseal(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>>;

    /// Rotate the specified key, returning a signed receipt
    async fn rotate(&self, key_id: &str) -> Result<RotationReceipt>;

    /// Generate attestation of provider state and configuration
    async fn attest(&self) -> Result<ProviderAttestation>;
}

/// Key provider mode enumeration
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyProviderMode {
    /// OS keychain (macOS Keychain, Linux keyring)
    Keychain,
    /// External KMS/HSM service
    Kms,
    /// File-based provider (development only, requires --allow-insecure-keys)
    File,
}

impl fmt::Display for KeyProviderMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyProviderMode::Keychain => write!(f, "keychain"),
            KeyProviderMode::Kms => write!(f, "kms"),
            KeyProviderMode::File => write!(f, "file"),
        }
    }
}

/// Configuration for key provider setup
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyProviderConfig {
    /// Provider mode
    pub mode: KeyProviderMode,
    /// Keychain service name (for Keychain mode)
    pub keychain_service: Option<String>,
    /// KMS endpoint (for KMS mode)
    pub kms_endpoint: Option<String>,
    /// File-based keystore path (for File mode)
    pub file_path: Option<std::path::PathBuf>,
    /// Rotation interval in seconds
    pub rotation_interval_secs: Option<u64>,
}

impl Default for KeyProviderConfig {
    fn default() -> Self {
        Self {
            mode: KeyProviderMode::Keychain,
            keychain_service: Some("adapteros".to_string()),
            kms_endpoint: None,
            file_path: None,
            rotation_interval_secs: Some(86400), // 24 hours
        }
    }
}
