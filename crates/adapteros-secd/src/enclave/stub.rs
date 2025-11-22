use super::{EnclaveError, Result};
use adapteros_core::{derive_seed, B3Hash};
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce};
use ed25519_dalek::{SigningKey, VerifyingKey};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Software-based fallback for platforms without Secure Enclave support.
/// Provides graceful degradation while maintaining cryptographic security properties.
///
/// Security Model:
/// - Uses HKDF for key derivation with domain separation
/// - ChaCha20-Poly1305 for AEAD encryption (same as macOS implementation)
/// - Ed25519 for signing (platform-agnostic alternative to Secure Enclave ECDSA)
/// - In-memory key cache (ephemeral, cleared on process exit)
///
/// Limitations vs Hardware Enclave:
/// - Keys are in process memory (not in tamper-resistant hardware)
/// - No secure boot or attestation
/// - Suitable for development, testing, and non-production deployments
#[derive(Debug)]
pub struct EnclaveManager {
    /// Cache of derived keys by label for fast reuse
    key_cache: HashMap<String, Vec<u8>>,
    /// Cache of signing keys by label
    signing_key_cache: HashMap<String, SigningKey>,
    /// Root key for all derivations (derived from system entropy + timestamp)
    root_key: [u8; 32],
    /// Indicates this is a software fallback (for logging/attestation)
    is_software_fallback: bool,
}

impl EnclaveManager {
    /// Create a new enclave manager with software-based fallback
    pub fn new() -> Result<Self> {
        // Derive root key from system entropy (rand crate with secure OS randomness)
        let mut root_key = [0u8; 32];
        use rand::RngCore;
        let mut rng = rand::thread_rng();
        rng.fill_bytes(&mut root_key);

        warn!(
            "Secure Enclave not available: using software-based fallback (development/testing only)"
        );
        info!("Software fallback initialized with HKDF-derived keys (ChaCha20-Poly1305 + Ed25519)");

        Ok(Self {
            key_cache: HashMap::new(),
            signing_key_cache: HashMap::new(),
            root_key,
            is_software_fallback: true,
        })
    }

    /// Encrypt LoRA delta for secure at-rest storage
    pub fn seal_lora_delta(&mut self, delta: &[u8]) -> Result<Vec<u8>> {
        self.seal_with_label("lora_delta", delta)
    }

    /// Decrypt LoRA delta from secure storage
    pub fn unseal_lora_delta(&mut self, sealed: &[u8]) -> Result<Vec<u8>> {
        self.unseal_with_label("lora_delta", sealed)
    }

    /// Sign data using software-backed key dedicated to bundle signing
    pub fn sign_bundle(&mut self, bundle_hash: &[u8]) -> Result<Vec<u8>> {
        self.sign_with_label("aos_bundle", bundle_hash)
    }

    /// Seal arbitrary data with a derived key identified by label
    /// Uses deterministic nonce derived from data hash (same as macOS implementation)
    pub fn seal_with_label(&mut self, label: &str, data: &[u8]) -> Result<Vec<u8>> {
        let key_bytes = self.get_or_derive_encryption_key(label)?;
        let key = Key::from_slice(&key_bytes);
        let cipher = ChaCha20Poly1305::new(key);

        // Derive deterministic nonce using HKDF with domain separation
        // Matches macOS implementation for compatibility
        let domain = format!("enclave-nonce:{}", label);
        let seed = derive_seed(&B3Hash::hash(data), &domain);
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes.copy_from_slice(&seed[..12]);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher.encrypt(nonce, data).map_err(|e| {
            EnclaveError::OperationFailed(format!("Software fallback encryption failed: {}", e))
        })?;

        let mut sealed = Vec::with_capacity(12 + ciphertext.len());
        sealed.extend_from_slice(&nonce_bytes);
        sealed.extend_from_slice(&ciphertext);

        info!(
            label,
            plaintext_bytes = data.len(),
            ciphertext_bytes = sealed.len(),
            backend = "software-fallback",
            "Encrypted payload with software-derived key"
        );
        Ok(sealed)
    }

    /// Unseal data that was encrypted with `seal_with_label`
    pub fn unseal_with_label(&mut self, label: &str, sealed: &[u8]) -> Result<Vec<u8>> {
        if sealed.len() < 12 {
            return Err(EnclaveError::InvalidData(format!(
                "Sealed payload too short for label {} ({} bytes)",
                label,
                sealed.len()
            )));
        }

        let key_bytes = self.get_or_derive_encryption_key(label)?;
        let key = Key::from_slice(&key_bytes);
        let cipher = ChaCha20Poly1305::new(key);

        let (nonce_bytes, ciphertext) = sealed.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|e| {
            EnclaveError::OperationFailed(format!("Software fallback decryption failed: {}", e))
        })?;

        info!(
            label,
            sealed_bytes = sealed.len(),
            plaintext_bytes = plaintext.len(),
            backend = "software-fallback",
            "Decrypted payload with software-derived key"
        );
        Ok(plaintext)
    }

    /// Sign arbitrary data using a software signing key identified by label
    pub fn sign_with_label(&mut self, label: &str, data: &[u8]) -> Result<Vec<u8>> {
        let signing_key = self.get_or_derive_signing_key(label)?;
        use ed25519_dalek::Signer;
        let signature = signing_key.sign(data);
        Ok(signature.to_bytes().to_vec())
    }

    /// Return public key bytes for the specified label
    pub fn get_public_key(&mut self, label: &str) -> Result<Vec<u8>> {
        let signing_key = self.get_or_derive_signing_key(label)?;
        let verifying_key: VerifyingKey = signing_key.verifying_key();
        Ok(verifying_key.to_bytes().to_vec())
    }

    /// Get or derive an encryption key from the root key
    fn get_or_derive_encryption_key(&mut self, label: &str) -> Result<[u8; 32]> {
        let cache_key = format!("{}_encryption", label);

        if let Some(cached) = self.key_cache.get(&cache_key) {
            let mut key = [0u8; 32];
            if cached.len() == 32 {
                key.copy_from_slice(cached);
                return Ok(key);
            }
        }

        let derived = self.derive_key_with_hkdf(label, "encryption")?;
        self.key_cache.insert(cache_key, derived.to_vec());

        Ok(derived)
    }

    /// Get or derive a signing key from the root key
    fn get_or_derive_signing_key(&mut self, label: &str) -> Result<SigningKey> {
        let cache_key = format!("{}_signing", label);

        if let Some(cached) = self.signing_key_cache.get(&cache_key) {
            return Ok(SigningKey::from_bytes(&cached.to_bytes()));
        }

        let key_bytes = self.derive_key_with_hkdf(label, "signing")?;
        let signing_key = SigningKey::from_bytes(&key_bytes);

        self.signing_key_cache
            .insert(cache_key, signing_key.clone());

        Ok(signing_key)
    }

    /// Derive a key using HKDF with domain separation
    /// Uses the root key as IKM and label/domain as info for domain separation
    fn derive_key_with_hkdf(&self, label: &str, purpose: &str) -> Result<[u8; 32]> {
        let hkdf = Hkdf::<Sha256>::new(None, &self.root_key);

        // Domain separation: combine label and purpose
        let info = format!("adapteros:{}:{}", label, purpose);

        let mut output = [0u8; 32];
        hkdf.expand(info.as_bytes(), &mut output).map_err(|e| {
            EnclaveError::OperationFailed(format!(
                "HKDF key derivation failed for label {}: {}",
                label, e
            ))
        })?;

        debug!(
            label,
            purpose, "Derived key using HKDF with domain separation"
        );

        Ok(output)
    }
}

impl EnclaveManager {
    /// Returns true if this manager is using software fallback instead of hardware enclave
    pub fn is_software_fallback(&self) -> bool {
        self.is_software_fallback
    }
}

impl Default for EnclaveManager {
    fn default() -> Self {
        // Create with software fallback (safe for all platforms)
        Self::new().expect("Failed to create software-fallback EnclaveManager")
    }
}
