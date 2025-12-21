use super::{EnclaveError, Result};
use adapteros_core::{derive_seed, B3Hash};
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce};
use core_foundation::base::CFOptionFlags;
use security_framework::access_control::{ProtectionMode, SecAccessControl};
use security_framework::base::Error as SecurityError;
use security_framework::item::{ItemClass, ItemSearchOptions, Location, Reference, SearchResult};
use security_framework::key::{Algorithm, GenerateKeyOptions, KeyType, SecKey, Token};
use std::collections::HashMap;

const ACCESS_CONTROL_PRIVATE_KEY_USAGE: CFOptionFlags = 1 << 30;
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

impl From<SecurityError> for EnclaveError {
    fn from(err: SecurityError) -> Self {
        EnclaveError::Security(err.to_string())
    }
}

/// Manages Secure Enclave operations on macOS
#[derive(Debug)]
pub struct EnclaveManager {
    /// Cache of key references by label for fast reuse
    key_cache: HashMap<String, SecKey>,
}

impl EnclaveManager {
    /// Create a new enclave manager
    pub fn new() -> Result<Self> {
        Ok(Self {
            key_cache: HashMap::new(),
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

    /// Sign data using Secure Enclave-backed key dedicated to bundle signing
    pub fn sign_bundle(&mut self, bundle_hash: &[u8]) -> Result<Vec<u8>> {
        self.sign_with_label("aos_bundle", bundle_hash)
    }

    /// Seal arbitrary data with a Secure Enclave–derived key identified by label
    #[allow(deprecated)]
    pub fn seal_with_label(&mut self, label: &str, data: &[u8]) -> Result<Vec<u8>> {
        let key_bytes = self.get_or_create_encryption_key(label)?;
        let key = Key::from_slice(&key_bytes);
        let cipher = ChaCha20Poly1305::new(key);

        // Derive deterministic nonce using HKDF with domain separation
        // Combines "enclave-nonce" domain with label for unique, reproducible nonces
        let domain = format!("enclave-nonce:{}", label);
        let seed = derive_seed(&B3Hash::hash(data), &domain);
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes.copy_from_slice(&seed[..12]);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, data)
            .map_err(|e| EnclaveError::OperationFailed(format!("Encryption failed: {}", e)))?;

        let mut sealed = Vec::with_capacity(12 + ciphertext.len());
        sealed.extend_from_slice(&nonce_bytes);
        sealed.extend_from_slice(&ciphertext);

        tracing::info!(
            label,
            plaintext_bytes = data.len(),
            ciphertext_bytes = sealed.len(),
            "Encrypted payload with Secure Enclave–derived key"
        );
        Ok(sealed)
    }

    /// Unseal data that was encrypted with `seal_with_label`
    #[allow(deprecated)]
    pub fn unseal_with_label(&mut self, label: &str, sealed: &[u8]) -> Result<Vec<u8>> {
        if sealed.len() < 12 {
            return Err(EnclaveError::InvalidData(format!(
                "Sealed payload too short for label {} ({} bytes)",
                label,
                sealed.len()
            )));
        }

        let key_bytes = self.get_or_create_encryption_key(label)?;
        let key = Key::from_slice(&key_bytes);
        let cipher = ChaCha20Poly1305::new(key);

        let (nonce_bytes, ciphertext) = sealed.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| EnclaveError::OperationFailed(format!("Decryption failed: {}", e)))?;

        tracing::info!(
            label,
            sealed_bytes = sealed.len(),
            plaintext_bytes = plaintext.len(),
            "Decrypted payload with Secure Enclave–derived key"
        );
        Ok(plaintext)
    }

    /// Compute keyed digest using a Secure Enclave–derived key for the label
    pub fn digest_with_label(&mut self, label: &str, data: &[u8]) -> Result<[u8; 32]> {
        use blake3::Hasher;

        let key = self.get_or_create_encryption_key(label)?;
        let mut keyed = Hasher::new_keyed(&key);
        keyed.update(data);
        Ok(*keyed.finalize().as_bytes())
    }

    /// Ensure encryption key exists (idempotent derivation/creation)
    pub fn ensure_encryption_key(&mut self, label: &str) -> Result<()> {
        let _ = self.get_or_create_encryption_key(label)?;
        Ok(())
    }

    /// Export derived encryption key bytes (used behind permission guard)
    pub fn export_encryption_key(&mut self, label: &str) -> Result<[u8; 32]> {
        self.get_or_create_encryption_key(label)
    }

    /// Sign arbitrary data using a Secure Enclave signing key identified by label
    pub fn sign_with_label(&mut self, label: &str, data: &[u8]) -> Result<Vec<u8>> {
        let key = self.get_or_create_signing_key(label)?;
        key.create_signature(Algorithm::ECDSASignatureMessageX962SHA256, data)
            .map_err(|e| {
                EnclaveError::OperationFailed(format!(
                    "Secure Enclave signing failed for label {}: {}",
                    label, e
                ))
            })
    }

    /// Return DER-encoded public key for the specified label
    pub fn get_public_key(&mut self, label: &str) -> Result<Vec<u8>> {
        let key = self.get_or_create_signing_key(label)?;
        let public_key = key
            .public_key()
            .ok_or_else(|| EnclaveError::OperationFailed("Failed to extract public key".into()))?;

        let external = public_key.external_representation().ok_or_else(|| {
            EnclaveError::OperationFailed("Failed to export public key representation".into())
        })?;

        Ok(external.to_vec())
    }

    fn get_or_create_signing_key(&mut self, label: &str) -> Result<SecKey> {
        let cache_label = format!("{}_signing", label);
        if let Some(key) = self.key_cache.get(&cache_label) {
            return Ok(key.clone());
        }

        match self.load_key(&cache_label, ItemClass::key()) {
            Ok(existing) => {
                self.key_cache.insert(cache_label.clone(), existing.clone());
                Ok(existing)
            }
            Err(EnclaveError::KeyNotFound(_)) => {
                let key = self.create_enclave_signing_key(&cache_label)?;
                self.key_cache.insert(cache_label, key.clone());
                Ok(key)
            }
            Err(err) => Err(err),
        }
    }

    fn get_or_create_secure_key(&mut self, label: &str) -> Result<SecKey> {
        if let Some(key) = self.key_cache.get(label) {
            return Ok(key.clone());
        }

        match self.load_key(label, ItemClass::key()) {
            Ok(existing) => {
                self.key_cache.insert(label.to_string(), existing.clone());
                Ok(existing)
            }
            Err(EnclaveError::KeyNotFound(_)) => {
                let key = self.create_enclave_signing_key(label)?;
                self.key_cache.insert(label.to_string(), key.clone());
                Ok(key)
            }
            Err(err) => Err(err),
        }
    }

    fn get_or_create_encryption_key(&mut self, label: &str) -> Result<[u8; 32]> {
        let cache_label = format!("{}_encryption", label);
        let key = self.get_or_create_secure_key(&cache_label)?;
        self.derive_encryption_key_from_master(&key, label)
    }

    fn create_enclave_signing_key(&self, label: &str) -> Result<SecKey> {
        let access_control = SecAccessControl::create_with_protection(
            Some(ProtectionMode::AccessibleWhenUnlockedThisDeviceOnly),
            ACCESS_CONTROL_PRIVATE_KEY_USAGE,
        )
        .map_err(|e| {
            EnclaveError::OperationFailed(format!(
                "Failed to configure Secure Enclave access control: {}",
                e
            ))
        })?;

        let mut options = GenerateKeyOptions {
            key_type: None,
            size_in_bits: None,
            label: None,
            token: None,
            location: None,
            access_control: None,
        };
        options
            .set_label(label)
            .set_token(Token::SecureEnclave)
            .set_key_type(KeyType::ec())
            .set_size_in_bits(256)
            .set_location(Location::DefaultFileKeychain)
            .set_access_control(access_control);

        let dictionary = options.to_dictionary();

        SecKey::generate(dictionary).map_err(|e| {
            EnclaveError::OperationFailed(format!(
                "Secure Enclave key generation failed for {}: {}",
                label, e
            ))
        })
    }

    fn derive_encryption_key_from_master(
        &self,
        master_key: &SecKey,
        label: &str,
    ) -> Result<[u8; 32]> {
        let context = format!("adapteros:{}:chacha20", label);
        let signature = master_key
            .create_signature(
                Algorithm::ECDSASignatureMessageX962SHA256,
                context.as_bytes(),
            )
            .map_err(|e| {
                EnclaveError::OperationFailed(format!(
                    "Secure Enclave signature for key derivation failed: {}",
                    e
                ))
            })?;

        let mut digest_input = signature.clone();
        digest_input.extend_from_slice(context.as_bytes());
        let digest = B3Hash::hash(&digest_input);

        tracing::debug!(label, "Derived ChaCha20 key using Secure Enclave signature");
        Ok(*digest.as_bytes())
    }

    fn load_key(&self, label: &str, class: ItemClass) -> Result<SecKey> {
        let mut search = ItemSearchOptions::new();
        search.class(class);
        search.label(label);
        search.load_refs(true);

        match search.search() {
            Ok(results) => {
                for item in results {
                    if let SearchResult::Ref(reference) = item {
                        if let Reference::Key(key) = reference {
                            return Ok(key);
                        }
                    }
                }
                Err(EnclaveError::KeyNotFound(label.to_string()))
            }
            Err(err) if err.code() == ERR_SEC_ITEM_NOT_FOUND => {
                Err(EnclaveError::KeyNotFound(label.to_string()))
            }
            Err(err) => Err(EnclaveError::Security(err.to_string())),
        }
    }
}

impl Default for EnclaveManager {
    fn default() -> Self {
        Self::new().expect("Failed to create EnclaveManager")
    }
}
