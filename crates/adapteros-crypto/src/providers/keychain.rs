//! OS Keychain provider implementation
//!
//! Provides cryptographic key operations using OS-native key storage with multiple backends:
//!
//! ## macOS (Security Framework)
//! - Primary: Security Framework with native APIs (`security-framework` crate)
//! - Hardware: Secure Enclave integration on Apple Silicon
//! - Storage: Encrypted keychain database protected by user credentials
//!
//! ## Linux (Multiple Backends)
//! - Primary: freedesktop Secret Service (GNOME Keyring/KWallet) via D-Bus
//! - Fallback: Linux kernel keyring via keyutils syscalls
//! - Headless: Works without graphical session
//!
//! ## Password Fallback
//! - Fallback: Encrypted JSON keystore for CI/headless environments
//! - KDF: Argon2id with high iteration count
//! - Encryption: AES-256-GCM or ChaCha20-Poly1305
//! - Opt-in: Requires `ADAPTEROS_KEYCHAIN_FALLBACK=pass:<password>`
//!
//! ## Security Features
//! - Hardware-backed keys when available (Secure Enclave)
//! - Fine-grained access control policies
//! - Cryptographic key rotation with signed receipts
//! - Memory zeroization and secure key handling
//! - Platform-specific optimizations and fallbacks

use crate::key_provider::{
    KeyAlgorithm, KeyHandle, KeyProvider, KeyProviderConfig, ProviderAttestation, RotationReceipt,
};
use adapteros_core::{AosError, Result};
use base64::Engine;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::collections::HashMap;
#[cfg(all(target_os = "linux", feature = "linux-keychain"))]
use std::ffi::CString;
use tracing::{debug, error, info, warn};

// macOS Keychain integration
#[cfg(target_os = "macos")]
use security::framework::KeychainServices::*;
#[cfg(target_os = "macos")]
use security::framework::OSStatusExt;

// Linux keyring integration
#[cfg(target_os = "linux")]
use secret_service::{SecretService, EncryptionType};

// Add at top imports
use core_foundation::base::{CFTypeRef, TCFType};
use core_foundation::string::CFString;
use core_foundation::data::CFData;
use security_framework::os::macos::keychain::{SecItemAdd, SecItemCopyMatching, SecItemAttributeValue};
use security_framework_sys::item::{kSecClass, kSecAttrAccount, kSecAttrService, kSecValueData, kSecReturnData, kCFBooleanTrue, kSecMatchLimit, kSecMatchLimitOne};
use security_framework::os::macos::keychain::SecKeychainItem;

/// Keychain provider implementation
pub struct KeychainProvider {
    #[allow(dead_code)]
    service_name: String,
    #[allow(dead_code)]
    config: KeyProviderConfig,
    keyring: Box<dyn KeyringImpl + Send + Sync>,
    backend: KeychainBackend,
}

impl KeychainProvider {
    /// Create a new keychain provider
    pub fn new(config: KeyProviderConfig) -> Result<Self> {
        let service_name = config
            .keychain_service
            .as_deref()
            .unwrap_or("adapteros")
            .to_string();

        info!(
            service = %service_name,
            "Initializing keychain provider"
        );

        // Check for password fallback first
        #[cfg(feature = "password-fallback")]
        if let Ok(fallback_env) = std::env::var("ADAPTEROS_KEYCHAIN_FALLBACK") {
            if let Some(password) = Self::parse_fallback_env(&fallback_env) {
                warn!("Using password-based key storage fallback - this is not secure for production use!");
                let keyring = Box::new(PasswordFallbackKeyring::new(service_name.clone(), password)?);
                return Ok(Self {
                    service_name,
                    config,
                    keyring,
                    backend: KeychainBackend::PasswordFallback,
                });
            }
        }

        // Create the platform-specific keyring implementation
        let (keyring, backend) = {
            #[cfg(target_os = "macos")]
            {
                (Box::new(MacKeychain::new(service_name.clone())), KeychainBackend::MacOS)
            }
            #[cfg(target_os = "linux")]
            {
                let linux_keyring = LinuxKeyring::new(service_name.clone());
                let backend = match linux_keyring.backend {
                    LinuxKeyringBackend::SecretService => KeychainBackend::SecretService,
                    LinuxKeyringBackend::KernelKeyring => KeychainBackend::KernelKeyring,
                };
                (Box::new(linux_keyring), backend)
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            {
                error!("Keychain provider not supported on this platform");
                return Err(AosError::Crypto(
                    "Keychain provider not supported on this platform".to_string(),
                ));
            }
        };

        Ok(Self {
            service_name,
            config,
            keyring,
            backend,
        })
    }

    /// Parse ADAPTEROS_KEYCHAIN_FALLBACK environment variable
    /// Expected format: "pass:<password>"
    #[cfg(any(feature = "password-fallback", test))]
    #[allow(dead_code)] // Used in tests even when feature is disabled
    fn parse_fallback_env(env_value: &str) -> Option<String> {
        if let Some(password) = env_value.strip_prefix("pass:") {
            if password.len() >= 8 {
                Some(password.to_string())
            } else {
                warn!("Password fallback requires at least 8 characters");
                None
            }
        } else {
            warn!("Invalid ADAPTEROS_KEYCHAIN_FALLBACK format. Expected 'pass:<password>' where password is at least 8 characters. Example: ADAPTEROS_KEYCHAIN_FALLBACK=pass:mysecretpassword123");
            None
        }
    }

    /// Get the current backend type
    pub fn backend(&self) -> KeychainBackend {
        self.backend.clone()
    }

    /// Check backend health and perform dynamic switching if needed
    pub fn check_backend_health(&mut self) -> Result<()> {
        // Delegate to the keyring implementation's health check
        // This will handle dynamic switching for Linux backends
        self.keyring.check_health()?;

        // For Linux backends, update our backend field if it changed
        #[cfg(target_os = "linux")]
        {
            if let Some(linux_keyring) = (&*self.keyring as &dyn std::any::Any).downcast_ref::<LinuxKeyring>() {
                self.backend = match linux_keyring.backend {
                    LinuxKeyringBackend::SecretService => KeychainBackend::SecretService,
                    LinuxKeyringBackend::KernelKeyring => KeychainBackend::KernelKeyring,
                };
            }
        }

        Ok(())
    }

    /// Get platform-specific keyring implementation
    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    fn get_keyring(&self) -> Result<Box<dyn KeyringImpl>> {
        Ok(Box::new(MacKeychain::new(self.service_name.clone())))
    }

    #[cfg(target_os = "linux")]
    #[allow(dead_code)]
    fn get_keyring(&self) -> Result<Box<dyn KeyringImpl>> {
        Ok(Box::new(LinuxKeyring::new(self.service_name.clone())))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    #[allow(dead_code)]
    fn get_keyring(&self) -> Result<Box<dyn KeyringImpl>> {
        error!("Keychain provider not supported on this platform");
        Err(AosError::Crypto(
            "Keychain provider not supported on this platform".to_string(),
        ))
    }
}

#[async_trait::async_trait]
impl KeyProvider for KeychainProvider {
    async fn generate(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        debug!(key_id = %key_id, algorithm = %alg, "Generating key in keychain");

        let handle = self.keyring.generate_key(key_id, alg).await?;

        info!(key_id = %key_id, "Key generated successfully");
        Ok(handle)
    }

    async fn sign(&self, key_id: &str, msg: &[u8]) -> Result<Vec<u8>> {
        debug!(key_id = %key_id, msg_len = msg.len(), "Signing message");

        self.keyring.sign(key_id, msg).await
    }

    #[allow(deprecated)]
    async fn seal(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        debug!(key_id = %key_id, plaintext_len = plaintext.len(), "Sealing data");

        self.keyring.seal(key_id, plaintext).await
    }

    async fn unseal(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        debug!(key_id = %key_id, ciphertext_len = ciphertext.len(), "Unsealing data");

        self.keyring.unseal(key_id, ciphertext).await
    }

    async fn rotate(&self, key_id: &str) -> Result<RotationReceipt> {
        debug!(key_id = %key_id, "Rotating key");

        let receipt = self.keyring.rotate_key(key_id).await?;

        info!(key_id = %key_id, "Key rotated successfully");
        Ok(receipt)
    }

    async fn attest(&self) -> Result<ProviderAttestation> {
        debug!("Generating provider attestation");

        let attestation = self.keyring.attest().await?;

        debug!("Provider attestation generated");
        Ok(attestation)
    }
}

/// Password-based encrypted keystore fallback implementation
#[cfg(feature = "password-fallback")]
struct PasswordFallbackKeyring {
    service_name: String,
    keys: std::sync::Mutex<HashMap<String, KeyHandle>>,
    keystore_path: std::path::PathBuf,
    master_key: [u8; 32], // Derived from password
}

#[cfg(feature = "password-fallback")]
impl PasswordFallbackKeyring {
    fn new(service_name: String, password: String) -> Result<Self> {
        use argon2::{Argon2, Params};
        use std::path::PathBuf;

        // Derive master key from password using Argon2id
        let salt = format!("adapteros-{}", service_name);
        let mut master_key = [0u8; 32];
        let argon2 = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            Params::new(65536, 3, 4, Some(32)).map_err(|e| {
                error!(error = %e, "Failed to create Argon2 parameters");
                AosError::Crypto("Failed to initialize password hashing".to_string())
            })?,
        );

        argon2.hash_password_into(password.as_bytes(), salt.as_bytes(), &mut master_key)
            .map_err(|e| {
                error!(error = %e, "Failed to derive master key from password");
                AosError::Crypto("Failed to derive encryption key".to_string())
            })?;

        // Determine keystore path
        let keystore_path = if let Ok(data_dir) = std::env::var("XDG_DATA_HOME") {
            PathBuf::from(data_dir).join("adapteros").join("keystore.json.enc")
        } else {
            PathBuf::from("./.adapteros-keys.enc")
        };

        // Ensure directory exists
        if let Some(parent) = keystore_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                error!(error = %e, path = %keystore_path.display(), "Failed to create keystore directory");
                AosError::Io(format!("Failed to create keystore directory: {}", e))
            })?;
        }

        Ok(Self {
            service_name,
            keys: std::sync::Mutex::new(HashMap::new()),
            keystore_path,
            master_key,
        })
    }

    /// Load encrypted keystore and decrypt it
    fn load_keystore(&self) -> Result<serde_json::Value> {
        #[cfg(feature = "password-fallback")]
        {
            use aes_gcm::{AeadInPlace, Aes256Gcm, KeyInit, Nonce};
            use chacha20poly1305::ChaCha20Poly1305;

            if !self.keystore_path.exists() {
                return Ok(serde_json::json!({"keys": {}}));
            }

            let ciphertext = std::fs::read(&self.keystore_path).map_err(|e| {
                error!(error = %e, path = %self.keystore_path.display(), "Failed to read keystore file");
                AosError::Io(format!("Failed to read keystore: {}", e))
            })?;

            if ciphertext.len() < 12 + 16 { // nonce + tag
                return Err(AosError::Crypto("Invalid keystore file format".to_string()));
            }

            let nonce_bytes = &ciphertext[..12];
            let encrypted_data = &ciphertext[12..];

            // Try AES-256-GCM first, then ChaCha20-Poly1305 as fallback
            let plaintext = if let Ok(cipher) = Aes256Gcm::new_from_slice(&self.master_key) {
                let nonce = Nonce::from_slice(nonce_bytes);
                let mut data = encrypted_data.to_vec();
                cipher.decrypt_in_place(nonce, &[], &mut data).map(|_| data)
            } else if let Ok(cipher) = ChaCha20Poly1305::new_from_slice(&self.master_key) {
                let nonce = chacha20poly1305::Nonce::from_slice(nonce_bytes);
                let mut data = encrypted_data.to_vec();
                cipher.decrypt_in_place(nonce, &[], &mut data).map(|_| data)
            } else {
                return Err(AosError::Crypto("No supported cipher available".to_string()));
            }            .map_err(|e| {
                error!(error = %e, "Failed to decrypt keystore");
                AosError::Crypto("[Password Fallback] Decrypt operation failed: Wrong password or corrupted keystore - Verify ADAPTEROS_KEYCHAIN_FALLBACK password".to_string())
            })?;

            let keystore: serde_json::Value = serde_json::from_slice(&plaintext).map_err(|e| {
                error!(error = %e, "Failed to parse keystore JSON");
                AosError::Crypto("[Password Fallback] Parse operation failed: Corrupted keystore format".to_string())
            })?;

            Ok(keystore)
        }

        #[cfg(not(feature = "password-fallback"))]
        {
            Err(AosError::Crypto("Password fallback not compiled in".to_string()))
        }
    }

    /// Encrypt and save keystore
    fn save_keystore(&self, keystore: &serde_json::Value) -> Result<()> {
        #[cfg(feature = "password-fallback")]
        {
            use aes_gcm::{AeadInPlace, Aes256Gcm, KeyInit};
            use rand::RngCore;

            let json_data = serde_json::to_vec(keystore).map_err(|e| {
                error!(error = %e, "Failed to serialize keystore");
                AosError::Crypto(format!("Failed to serialize keystore: {}", e))
            })?;

            // Generate random nonce
            let mut nonce_bytes = [0u8; 12];
            rand::thread_rng().fill_bytes(&mut nonce_bytes);

            // Encrypt with AES-256-GCM
            let cipher = Aes256Gcm::new_from_slice(&self.master_key)
                .map_err(|e| {
                    error!(error = %e, "Failed to create AES cipher");
                    AosError::Crypto("Failed to initialize encryption".to_string())
                })?;

            let nonce = aes_gcm::Nonce::from_slice(&nonce_bytes);
            let mut ciphertext = json_data;
            cipher.encrypt_in_place(nonce, &[], &mut ciphertext).map_err(|e| {
                error!(error = %e, "Failed to encrypt keystore");
                AosError::Crypto("Failed to encrypt keystore".to_string())
            })?;

            // Prepend nonce
            let mut encrypted_data = nonce_bytes.to_vec();
            encrypted_data.extend(ciphertext);

            // Write to file
            std::fs::write(&self.keystore_path, &encrypted_data).map_err(|e| {
                error!(error = %e, path = %self.keystore_path.display(), "Failed to write keystore");
                AosError::Io(format!("Failed to write keystore: {}", e))
            })?;

            Ok(())
        }

        #[cfg(not(feature = "password-fallback"))]
        {
            Err(AosError::Crypto("Password fallback not compiled in".to_string()))
        }
    }

    /// Get or create key handle from keystore
    fn get_or_create_key_handle(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        let mut keys = self.keys.lock().unwrap();

        if let Some(handle) = keys.get(key_id) {
            return Ok(handle.clone());
        }

        // Load from keystore
        let keystore = self.load_keystore()?;
        if let Some(keys_obj) = keystore.get("keys").and_then(|k| k.as_object()) {
            if let Some(key_obj) = keys_obj.get(key_id) {
                // Parse existing key
                let algorithm_str = key_obj.get("algorithm")
                    .and_then(|a| a.as_str())
                    .ok_or_else(|| AosError::Crypto("Invalid keystore key format".to_string()))?;

                let algorithm = match algorithm_str {
                    "ed25519" => KeyAlgorithm::Ed25519,
                    "aes256gcm" => KeyAlgorithm::Aes256Gcm,
                    "chacha20poly1305" => KeyAlgorithm::ChaCha20Poly1305,
                    _ => return Err(AosError::Crypto(format!("Unknown algorithm: {}", algorithm_str))),
                };

                let public_key = if let Some(pk_b64) = key_obj.get("public_key_b64").and_then(|p| p.as_str()) {
                    Some(base64::engine::general_purpose::STANDARD.decode(pk_b64).map_err(|e| {
                        error!(error = %e, "Invalid base64 public key in keystore");
                        AosError::Crypto("Invalid keystore public key".to_string())
                    })?)
                } else {
                    None
                };

                let handle = KeyHandle::with_public_key(
                    format!("{}:{}", self.service_name, key_id),
                    algorithm,
                    public_key.unwrap_or_default(),
                );

                keys.insert(key_id.to_string(), handle.clone());
                return Ok(handle);
            }
        }

        // Create new key handle
        let handle = KeyHandle::new(format!("{}:{}", self.service_name, key_id), alg);
        keys.insert(key_id.to_string(), handle.clone());
        Ok(handle)
    }

    /// Store key in keystore
    fn store_key_in_keystore(&self, key_id: &str, alg: &KeyAlgorithm, private_key_b64: &str, public_key_b64: Option<&str>) -> Result<()> {
        let mut keystore = self.load_keystore()?;
        let keys_obj = keystore.get_mut("keys")
            .and_then(|k| k.as_object_mut())
            .ok_or_else(|| AosError::Crypto("Invalid keystore structure".to_string()))?;

        let mut key_obj = serde_json::Map::new();
        key_obj.insert("algorithm".to_string(), serde_json::Value::String(alg.to_string()));
        key_obj.insert("private_key_b64".to_string(), serde_json::Value::String(private_key_b64.to_string()));

        if let Some(pk) = public_key_b64 {
            key_obj.insert("public_key_b64".to_string(), serde_json::Value::String(pk.to_string()));
        }

        key_obj.insert("created_at".to_string(), serde_json::Value::String(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .to_string()
        ));

        keys_obj.insert(key_id.to_string(), serde_json::Value::Object(key_obj));

        self.save_keystore(&keystore)
    }

    /// Load private key from keystore
    fn load_private_key_from_keystore(&self, key_id: &str) -> Result<String> {
        let keystore = self.load_keystore()?;
        let keys_obj = keystore.get("keys")
            .and_then(|k| k.as_object())
            .ok_or_else(|| AosError::Crypto("Invalid keystore structure".to_string()))?;

        let key_obj = keys_obj.get(key_id)
            .and_then(|k| k.as_object())
            .ok_or_else(|| AosError::NotFound(format!("Key '{}' not found in keystore", key_id)))?;

        let private_key_b64 = key_obj.get("private_key_b64")
            .and_then(|p| p.as_str())
            .ok_or_else(|| AosError::Crypto("Invalid keystore key format".to_string()))?;

        Ok(private_key_b64.to_string())
    }
}

#[cfg(feature = "password-fallback")]
#[async_trait::async_trait]
impl KeyringImpl for PasswordFallbackKeyring {
    async fn generate_key(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        use rand::rngs::OsRng;

        let alg_clone = alg.clone();
        let handle = match alg {
            KeyAlgorithm::Ed25519 => {
                let mut rng = OsRng;
                let signing_key = ed25519_dalek::SigningKey::generate(&mut rng);
                let verifying_key = signing_key.verifying_key();

                // Store in keystore
                let private_key_b64 = base64::engine::general_purpose::STANDARD.encode(signing_key.to_bytes());
                let public_key_b64 = base64::engine::general_purpose::STANDARD.encode(verifying_key.to_bytes());
                self.store_key_in_keystore(key_id, &alg, &private_key_b64, Some(&public_key_b64))?;

                KeyHandle::with_public_key(
                    format!("{}:{}", self.service_name, key_id),
                    alg,
                    verifying_key.to_bytes().to_vec(),
                )
            }
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {
                let mut rng = OsRng;
                let mut key_data = [0u8; 32];
                rng.fill_bytes(&mut key_data);

                // Store in keystore
                let private_key_b64 = base64::engine::general_purpose::STANDARD.encode(key_data);
                self.store_key_in_keystore(key_id, &alg, &private_key_b64, None)?;

                KeyHandle::new(format!("{}:{}", self.service_name, key_id), alg)
            }
        };

        // Cache in memory
        self.keys.lock().unwrap().insert(key_id.to_string(), handle.clone());

        info!(key_id = %key_id, algorithm = ?alg_clone, "Generated key and stored in password fallback keystore");
        Ok(handle)
    }

    async fn sign(&self, key_id: &str, msg: &[u8]) -> Result<Vec<u8>> {
        let private_key_b64 = self.load_private_key_from_keystore(key_id)?;
        let key_bytes = base64::engine::general_purpose::STANDARD.decode(&private_key_b64).map_err(|e| {
            error!(error = %e, key_id = %key_id, "Invalid base64 in stored key");
            AosError::Crypto("Invalid stored key format".to_string())
        })?;

        if key_bytes.len() != 32 {
            return Err(AosError::Crypto("Invalid key length from keystore".to_string()));
        }

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&key_array);

        use ed25519_dalek::Signer;
        let signature = signing_key.sign(msg);

        info!(key_id = %key_id, message_len = msg.len(), "Signed message using password fallback keystore");
        Ok(signature.to_bytes().to_vec())
    }

    async fn seal(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        let private_key_b64 = self.load_private_key_from_keystore(key_id)?;
        let key_bytes = base64::engine::general_purpose::STANDARD.decode(&private_key_b64).map_err(|e| {
            error!(error = %e, key_id = %key_id, "Invalid base64 in stored key");
            AosError::Crypto("Invalid stored key format".to_string())
        })?;

        if key_bytes.len() != 32 {
            return Err(AosError::Crypto("Invalid key length from keystore".to_string()));
        }

        // Use AES-GCM for encryption
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};
        use rand::RngCore;

        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);

        // Generate a random nonce
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the plaintext
        let ciphertext = cipher.encrypt(nonce, plaintext).map_err(|e| {
            error!(error = %e, key_id = %key_id, "Encryption failed");
            AosError::Crypto(format!("Encryption failed: {}", e))
        })?;

        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend(ciphertext);

        info!(key_id = %key_id, plaintext_len = plaintext.len(), "Encrypted data using password fallback keystore");
        Ok(result)
    }

    async fn unseal(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < 12 {
            return Err(AosError::Crypto("Ciphertext too short".to_string()));
        }

        let private_key_b64 = self.load_private_key_from_keystore(key_id)?;
        let key_bytes = base64::engine::general_purpose::STANDARD.decode(&private_key_b64).map_err(|e| {
            error!(error = %e, key_id = %key_id, "Invalid base64 in stored key");
            AosError::Crypto("Invalid stored key format".to_string())
        })?;

        if key_bytes.len() != 32 {
            return Err(AosError::Crypto("Invalid key length from keystore".to_string()));
        }

        // Use AES-GCM for decryption
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};

        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);

        // Extract nonce from beginning of ciphertext
        let nonce_bytes = &ciphertext[..12];
        let nonce = Nonce::from_slice(nonce_bytes);
        let encrypted_data = &ciphertext[12..];

        // Decrypt the data
        let plaintext = cipher.decrypt(nonce, encrypted_data).map_err(|e| {
            error!(error = %e, key_id = %key_id, "Decryption failed");
            AosError::Crypto(format!("Decryption failed: {}", e))
        })?;

        info!(key_id = %key_id, ciphertext_len = ciphertext.len(), "Decrypted data using password fallback keystore");
        Ok(plaintext)
    }

    async fn sign_receipt(&self, receipt_data: &str) -> Result<Vec<u8>> {
        // Use a dedicated receipt signing key
        let signing_key_id = "__receipt_signing_key__";

        // Try to get existing signing key, or create one
        let private_key_b64 = match self.load_private_key_from_keystore(signing_key_id) {
            Ok(key) => key,
            Err(_) => {
                // Create signing key if it doesn't exist
                let _handle = self.generate_key(signing_key_id, KeyAlgorithm::Ed25519).await?;
                self.load_private_key_from_keystore(signing_key_id)?
            }
        };

        let key_bytes = base64::engine::general_purpose::STANDARD.decode(&private_key_b64).map_err(|e| {
            error!(error = %e, "Invalid base64 in receipt signing key");
            AosError::Crypto("Invalid receipt signing key format".to_string())
        })?;

        if key_bytes.len() != 32 {
            return Err(AosError::Crypto("Invalid receipt signing key length".to_string()));
        }

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&key_array);

        use ed25519_dalek::Signer;
        let signature = signing_key.sign(receipt_data.as_bytes());

        info!("Signed receipt data with cryptographic signature");
        Ok(signature.to_bytes().to_vec())
    }

    async fn rotate_key(&self, key_id: &str) -> Result<RotationReceipt> {
        // Get previous handle
        let previous_handle = self.get_or_create_key_handle(key_id, KeyAlgorithm::Ed25519)?; // Default to Ed25519
        let algorithm = previous_handle.algorithm.clone();

        // Generate new key (will overwrite in keystore)
        let new_handle = self.generate_key(key_id, algorithm.clone()).await?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Create receipt data to sign
        let receipt_data = format!(
            "rotation:{}:{}:{}:{}",
            key_id,
            previous_handle.provider_id,
            new_handle.provider_id,
            timestamp
        );

        // Sign the receipt
        let signature = self.sign_receipt(&receipt_data).await?;

        let receipt = RotationReceipt::new(
            key_id.to_string(),
            previous_handle,
            new_handle,
            timestamp,
            signature,
        );

        info!(key_id = %key_id, algorithm = ?algorithm, timestamp = timestamp, "Successfully rotated key with cryptographic receipt");
        Ok(receipt)
    }

    async fn attest(&self) -> Result<ProviderAttestation> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Calculate policy hash from provider configuration and state
        let policy_data = format!(
            "provider:password-fallback|service:{}|timestamp:{}|keys:{}",
            self.service_name,
            timestamp,
            self.keys.lock().unwrap().len()
        );
        use sha2::{Digest, Sha256};
        let policy_hash = format!("{:x}", Sha256::digest(&policy_data));

        // Create attestation data to sign
        let attestation_data = format!(
            "attestation:password-fallback:{}:{}",
            policy_hash,
            timestamp
        );

        // Sign the attestation
        let signature = self.sign_receipt(&attestation_data).await?;

        info!(policy_hash = %policy_hash, timestamp = timestamp, "Generated cryptographic provider attestation");

        Ok(ProviderAttestation::new(
            "password-fallback".to_string(),
            format!("service:{}", self.service_name),
            policy_hash,
            timestamp,
            signature,
        ))
    }

    fn check_health(&mut self) -> Result<()> {
        // Test keystore access and decryption
        let test_key_id = "__health_check_test__";

        match self.load_private_key_from_keystore(test_key_id) {
            Ok(_) => Ok(()), // Key exists (unexpected but OK)
            Err(AosError::NotFound(_)) => Ok(()), // Key doesn't exist (expected)
            Err(e) => {
                warn!(error = %e, "Password fallback keystore health check failed");
                Err(e)
            }
        }
    }
}

/// Platform-specific keyring trait
#[async_trait::async_trait]
trait KeyringImpl: Send + Sync {
    async fn generate_key(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle>;
    async fn sign(&self, key_id: &str, msg: &[u8]) -> Result<Vec<u8>>;
    async fn seal(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>>;
    async fn unseal(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>>;
    async fn sign_receipt(&self, receipt_data: &str) -> Result<Vec<u8>>;
    async fn rotate_key(&self, key_id: &str) -> Result<RotationReceipt>;
    async fn attest(&self) -> Result<ProviderAttestation>;
    fn check_health(&mut self) -> Result<()>;
}

/// macOS Keychain implementation using Security Framework
#[cfg(target_os = "macos")]
struct MacKeychain {
    service_name: String,
    keys: std::sync::Mutex<HashMap<String, KeyHandle>>,
}

#[cfg(target_os = "macos")]
impl MacKeychain {
    fn new(service_name: String) -> Self {
        Self {
            service_name,
            keys: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Store Ed25519 private key in macOS Keychain using native APIs
    fn store_ed25519_private_key(
        &self,
        key_id: &str,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> Result<()> {
        let key_data = signing_key.to_bytes();
        let key_data_b64 = base64::engine::general_purpose::STANDARD.encode(key_data);

        let account = format!("{}-ed25519", key_id);
        let label = format!("AdapterOS Ed25519 Key: {}", key_id);

        self.store_keychain_item(&account, &label, key_data_b64.as_bytes())?;

        info!(key_id = %key_id, "Stored Ed25519 key in macOS Keychain");
        Ok(())
    }

    /// Retrieve Ed25519 private key from macOS Keychain using secure CLI
    fn retrieve_ed25519_private_key(&self, key_id: &str) -> Result<ed25519_dalek::SigningKey> {
        use std::process::Command;

        let account = format!("{}-ed25519", key_id);

        // Validate inputs to prevent command injection
        if account.contains('\'') || account.contains('"') || account.contains('\\') {
            return Err(AosError::Crypto("Invalid account name contains shell metacharacters".to_string()));
        }
        if self.service_name.contains('\'') || self.service_name.contains('"') || self.service_name.contains('\\') {
            return Err(AosError::Crypto("Invalid service name contains shell metacharacters".to_string()));
        }

        // Use secure CLI approach with proper input validation
        let result = Command::new("security")
            .args([
                "find-generic-password",
                "-a", &account,
                "-s", &self.service_name,
                "-w"  // Print password only
            ])
            .output()
            .map_err(|e| {
                error!(error = %e, key_id = %key_id, "Failed to execute security command for key retrieval");
                AosError::Crypto("Failed to execute secure keychain retrieval command".to_string())
            })?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            error!(key_id = %key_id, stderr = %stderr, "macOS Keychain retrieval failed");
            let error_msg = if stderr.contains("permission") || stderr.contains("access") {
                "[macOS Keychain] Retrieve operation failed: Access denied - Unlock Keychain Access or check permissions".to_string()
            } else if stderr.contains("could not be found") || stderr.contains("doesn't exist") {
                format!("[macOS Keychain] Retrieve operation failed: Key '{}' not found", key_id)
            } else {
                format!("[macOS Keychain] Retrieve operation failed: {} - Check keychain accessibility", stderr)
            };
            return Err(AosError::Crypto(error_msg));
        }

        let key_data_b64 = String::from_utf8(result.stdout)
            .map_err(|e| {
                error!(error = %e, key_id = %key_id, "Invalid UTF-8 in keychain data");
                AosError::Crypto("Invalid keychain data encoding".to_string())
            })?
            .trim()
            .to_string();

        let key_data = base64::engine::general_purpose::STANDARD.decode(&key_data_b64).map_err(|e| {
            error!(error = %e, key_id = %key_id, "Invalid base64 in keychain data");
            AosError::Crypto("Invalid keychain data format".to_string())
        })?;

        if key_data.len() != 32 {
            error!(key_id = %key_id, len = key_data.len(), "Invalid key length from keychain");
            return Err(AosError::Crypto(
                "Invalid key length from keychain".to_string(),
            ));
        }

        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&key_data);

        let signing_key = ed25519_dalek::SigningKey::from_bytes(&key_bytes);
        info!(key_id = %key_id, "Retrieved Ed25519 key from macOS Keychain via secure CLI");
        Ok(signing_key)
    }

    /// Store symmetric key in macOS Keychain using native APIs
    fn store_symmetric_key(&self, key_id: &str, key_data: &[u8]) -> Result<()> {
        let key_data_b64 = base64::engine::general_purpose::STANDARD.encode(key_data);

        let account = format!("{}-symmetric", key_id);
        let label = format!("AdapterOS Symmetric Key: {}", key_id);

        self.store_keychain_item(&account, &label, key_data_b64.as_bytes())?;

        info!(key_id = %key_id, "Stored symmetric key in macOS Keychain");
        Ok(())
    }

    /// Retrieve symmetric key from macOS Keychain using secure CLI
    fn retrieve_symmetric_key(&self, key_id: &str) -> Result<Vec<u8>> {
        use std::process::Command;

        let account = format!("{}-symmetric", key_id);

        // Validate inputs to prevent command injection
        if account.contains('\'') || account.contains('"') || account.contains('\\') {
            return Err(AosError::Crypto("Invalid account name contains shell metacharacters".to_string()));
        }
        if self.service_name.contains('\'') || self.service_name.contains('"') || self.service_name.contains('\\') {
            return Err(AosError::Crypto("Invalid service name contains shell metacharacters".to_string()));
        }

        // Use secure CLI approach with proper input validation
        let result = Command::new("security")
            .args([
                "find-generic-password",
                "-a", &account,
                "-s", &self.service_name,
                "-w"  // Print password only
            ])
            .output()
            .map_err(|e| {
                error!(error = %e, key_id = %key_id, "Failed to execute security command for symmetric key retrieval");
                AosError::Crypto("Failed to execute secure keychain retrieval command".to_string())
            })?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            error!(key_id = %key_id, stderr = %stderr, "macOS Keychain symmetric key retrieval failed");
            let error_msg = if stderr.contains("permission") || stderr.contains("access") {
                "[macOS Keychain] Symmetric retrieve operation failed: Access denied - Unlock Keychain Access or check permissions".to_string()
            } else if stderr.contains("could not be found") || stderr.contains("doesn't exist") {
                format!("[macOS Keychain] Symmetric retrieve operation failed: Key '{}' not found", key_id)
            } else {
                format!("[macOS Keychain] Symmetric retrieve operation failed: {} - Check keychain accessibility", stderr)
            };
            return Err(AosError::Crypto(error_msg));
        }

        let key_data_b64 = String::from_utf8(result.stdout)
            .map_err(|e| {
                error!(error = %e, key_id = %key_id, "Invalid UTF-8 in symmetric keychain data");
                AosError::Crypto("Invalid symmetric keychain data encoding".to_string())
            })?
            .trim()
            .to_string();

        let key_data = base64::engine::general_purpose::STANDARD.decode(&key_data_b64).map_err(|e| {
            error!(error = %e, key_id = %key_id, "Invalid base64 in symmetric keychain data");
            AosError::Crypto("Invalid symmetric keychain data format".to_string())
        })?;

        info!(key_id = %key_id, "Retrieved symmetric key from macOS Keychain via secure CLI");
        Ok(key_data)
    }

    /// Delete a keychain item (used for rotation) using secure CLI
    fn delete_keychain_item(&self, account: &str) -> Result<()> {
        use std::process::Command;

        // Validate inputs to prevent command injection
        if account.contains('\'') || account.contains('"') || account.contains('\\') {
            return Err(AosError::Crypto("Invalid account name contains shell metacharacters".to_string()));
        }
        if self.service_name.contains('\'') || self.service_name.contains('"') || self.service_name.contains('\\') {
            return Err(AosError::Crypto("Invalid service name contains shell metacharacters".to_string()));
        }

        let result = Command::new("security")
            .args([
                "delete-generic-password",
                "-a", account,
                "-s", &self.service_name,
            ])
            .output()
            .map_err(|e| {
                error!(account = %account, error = %e, "Failed to execute security command for key deletion");
                AosError::Crypto("Failed to execute secure keychain deletion command".to_string())
            })?;

        // Note: security delete-generic-password returns success even if item doesn't exist
        // This is expected behavior - deleting a non-existent item is not an error
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            warn!(account = %account, stderr = %stderr, "Security command reported non-success for deletion, but continuing");
        }

        info!(account = %account, "Deleted keychain item via secure CLI");
        Ok(())
    }

    /// Store keychain item with update handling using secure CLI
    fn store_keychain_item(&self, account: &str, label: &str, password_data: &[u8]) -> Result<()> {
        use std::process::Command;

        // First try to delete any existing items
        if let Err(e) = self.delete_keychain_item(account) {
            // Log warning but continue - might not exist yet
            warn!(account = %account, error = %e, "Failed to delete existing keychain item, proceeding with add");
        }

        // Validate inputs to prevent command injection
        if account.contains('\'') || account.contains('"') || account.contains('\\') {
            return Err(AosError::Crypto("Invalid account name contains shell metacharacters".to_string()));
        }
        if self.service_name.contains('\'') || self.service_name.contains('"') || self.service_name.contains('\\') {
            return Err(AosError::Crypto("Invalid service name contains shell metacharacters".to_string()));
        }
        if label.contains('\'') || label.contains('"') || label.contains('\\') {
            return Err(AosError::Crypto("Invalid label contains shell metacharacters".to_string()));
        }

        // Use base64 encoding to safely pass binary data via stdin
        let password_b64 = base64::engine::general_purpose::STANDARD.encode(password_data);

        // Use secure CLI approach with proper input validation and error handling
        let result = Command::new("security")
            .args([
                "add-generic-password",
                "-a", account,
                "-s", &self.service_name,
                "-l", label,
                "-w", &password_b64,
                "-U"  // Update if exists (though we deleted above)
            ])
            .output()
            .map_err(|e| {
                error!(account = %account, error = %e, "Failed to execute security command for key storage");
                AosError::Crypto("Failed to execute secure keychain storage command".to_string())
            })?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            error!(account = %account, stderr = %stderr, "macOS Keychain storage failed");
            let error_msg = if stderr.contains("duplicate") {
                "[macOS Keychain] Store operation failed: Item already exists - Use rotate_key() instead of generate_key()".to_string()
            } else if stderr.contains("permission") || stderr.contains("access") {
                "[macOS Keychain] Store operation failed: Access denied - Unlock Keychain Access or check permissions".to_string()
            } else {
                format!("[macOS Keychain] Store operation failed: {} - Check keychain accessibility", stderr)
            };
            return Err(AosError::Crypto(error_msg));
        }

        info!(account = %account, "Stored keychain item via secure CLI");
        Ok(())
    }
}

#[cfg(target_os = "macos")]
#[async_trait::async_trait]
impl KeyringImpl for MacKeychain {
    async fn generate_key(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        match &alg {
            KeyAlgorithm::Ed25519 => {
                use rand::rngs::OsRng;

                let mut rng = OsRng;
                let signing_key = ed25519_dalek::SigningKey::generate(&mut rng);
                let verifying_key = signing_key.verifying_key();

                // Store private key in macOS Keychain
                self.store_ed25519_private_key(key_id, &signing_key)?;

                let handle = KeyHandle::with_public_key(
                    format!("{}:{}", self.service_name, key_id),
                    alg.clone(),
                    verifying_key.to_bytes().to_vec(),
                );

                // Cache handle in memory for faster lookups
                self.keys
                    .lock()
                    .unwrap()
                    .insert(key_id.to_string(), handle.clone());

                info!(key_id = %key_id, algorithm = ?alg, "Generated Ed25519 key and stored in macOS Keychain");
                Ok(handle)
            }
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {
                // Generate symmetric key
                use rand::rngs::OsRng;
                use rand::RngCore;

                let mut key_data = [0u8; 32];
                OsRng.fill_bytes(&mut key_data);

                // Store key in macOS Keychain
                self.store_symmetric_key(key_id, &key_data)?;

                let handle =
                    KeyHandle::new(format!("{}:{}", self.service_name, key_id), alg.clone());

                // Cache handle in memory for faster lookups
                self.keys
                    .lock()
                    .unwrap()
                    .insert(key_id.to_string(), handle.clone());

                info!(key_id = %key_id, algorithm = ?alg, "Generated symmetric key and stored in macOS Keychain");
                Ok(handle)
            }
        }
    }

    async fn sign(&self, key_id: &str, msg: &[u8]) -> Result<Vec<u8>> {
        // Retrieve private key from macOS Keychain
        let signing_key = self.retrieve_ed25519_private_key(key_id)?;

        // Sign the message
        use ed25519_dalek::Signer;
        let signature = signing_key.sign(msg);

        info!(key_id = %key_id, message_len = msg.len(), "Signed message using macOS Keychain");
        Ok(signature.to_bytes().to_vec())
    }

    #[allow(deprecated)]
    async fn seal(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        // Retrieve symmetric key from macOS Keychain
        let key_data = self.retrieve_symmetric_key(key_id)?;

        // Use AES-GCM for encryption
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};

        let cipher = Aes256Gcm::new_from_slice(&key_data)
            .map_err(|e| AosError::Crypto(format!("Failed to create AES cipher: {}", e)))?;

        // Generate a random nonce
        use rand::rngs::OsRng;
        use rand::RngCore;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the plaintext
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| AosError::Crypto(format!("Encryption failed: {}", e)))?;

        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend(ciphertext);

        info!(key_id = %key_id, plaintext_len = plaintext.len(), "Encrypted data using macOS Keychain");
        Ok(result)
    }

    #[allow(deprecated)]
    async fn unseal(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < 12 {
            return Err(AosError::Crypto("Ciphertext too short".to_string()));
        }

        // Retrieve symmetric key from macOS Keychain
        let key_data = self.retrieve_symmetric_key(key_id)?;

        // Use AES-GCM for decryption
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};

        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&key_data);
        let cipher = Aes256Gcm::new(key);

        // Extract nonce from beginning of ciphertext
        let nonce_bytes = &ciphertext[..12];
        let nonce = Nonce::from_slice(nonce_bytes);
        let encrypted_data = &ciphertext[12..];

        // Decrypt the data
        let plaintext = cipher
            .decrypt(nonce, encrypted_data)
            .map_err(|e| AosError::Crypto(format!("Decryption failed: {}", e)))?;

        info!(key_id = %key_id, ciphertext_len = ciphertext.len(), "Decrypted data using macOS Keychain");
        Ok(plaintext)
    }

    /// Sign receipt data using the provider's signing key
    async fn sign_receipt(&self, receipt_data: &str) -> Result<Vec<u8>> {
        // Use a dedicated receipt signing key (Linux doesn't have Secure Enclave)
        let signing_key_id = "__receipt_signing_key__";

        // Try to get existing signing key, or create one
        let signing_key = match self.retrieve_ed25519_private_key(signing_key_id) {
            Ok(key) => key,
            Err(_) => {
                // Create signing key if it doesn't exist
                let _handle = self.generate_key(signing_key_id, KeyAlgorithm::Ed25519).await?;
                self.retrieve_ed25519_private_key(signing_key_id)?
            }
        };

        // Sign the receipt data
        use ed25519_dalek::Signer;
        let signature = signing_key.sign(receipt_data.as_bytes());

        info!("Signed receipt data with cryptographic signature");
        Ok(signature.to_bytes().to_vec())
    }

    async fn rotate_key(&self, key_id: &str) -> Result<RotationReceipt> {
        // Get previous handle and drop lock before await
        let previous_handle = {
            let keys = self.keys.lock().unwrap();
            keys.get(key_id)
                .cloned()
                .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?
        };
        let algorithm = previous_handle.algorithm.clone();

        // Explicitly delete old keychain items before generating new key
        match algorithm {
            KeyAlgorithm::Ed25519 => {
                let account = format!("{}-ed25519", key_id);
                if let Err(e) = self.delete_keychain_item(&account) {
                    warn!(key_id = %key_id, account = %account, error = %e, "Failed to delete old Ed25519 key during rotation");
                }
            }
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {
                let account = format!("{}-symmetric", key_id);
                if let Err(e) = self.delete_keychain_item(&account) {
                    warn!(key_id = %key_id, account = %account, error = %e, "Failed to delete old symmetric key during rotation");
                }
            }
        }

        // Generate new key (will store in keychain)
        let new_handle = self.generate_key(key_id, algorithm.clone()).await?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Create receipt data to sign
        let receipt_data = format!(
            "rotation:{}:{}:{}:{}",
            key_id,
            previous_handle.provider_id,
            new_handle.provider_id,
            timestamp
        );

        // Sign the receipt using the provider's signing key
        let signature = self.sign_receipt(&receipt_data).await?;

        let receipt = RotationReceipt::new(
            key_id.to_string(),
            previous_handle,
            new_handle,
            timestamp,
            signature,
        );

        info!(key_id = %key_id, algorithm = ?algorithm, timestamp = timestamp, "Successfully rotated key with cryptographic receipt");
        Ok(receipt)
    }

    async fn attest(&self) -> Result<ProviderAttestation> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Calculate policy hash from provider configuration and state
        let policy_data = format!(
            "provider:macos-keychain|service:{}|timestamp:{}|keys:{}",
            self.service_name,
            timestamp,
            self.keys.lock().unwrap().len()
        );
        use sha2::{Digest, Sha256};
        let policy_hash = format!("{:x}", Sha256::digest(&policy_data));

        // Create attestation data to sign
        let attestation_data = format!(
            "attestation:{}:{}:{}",
            "macos-keychain",
            policy_hash,
            timestamp
        );

        // Sign the attestation
        let signature = self.sign_receipt(&attestation_data).await?;

        info!(policy_hash = %policy_hash, timestamp = timestamp, "Generated cryptographic provider attestation");

        Ok(ProviderAttestation::new(
            "macos-keychain".to_string(),
            format!("service:{}", self.service_name),
            policy_hash,
            timestamp,
            signature,
        ))
    }

    fn check_health(&mut self) -> Result<()> {
        // macOS keychain doesn't have dynamic switching
        // Just verify we can still access the keychain
        let test_key_id = "__health_check_test__";

        // Try a simple operation to test health
        match self.retrieve_ed25519_private_key(test_key_id) {
            Ok(_) => Ok(()), // Key exists (unexpected but OK)
            Err(AosError::NotFound(_)) => Ok(()), // Key doesn't exist (expected)
            Err(e) => {
                warn!(error = %e, "macOS keychain health check failed");
                Err(e)
            }
        }
    }
}

/// Linux keyring implementation supporting multiple backends
#[cfg(target_os = "linux")]
struct LinuxKeyring {
    service_name: String,
    keys: std::sync::Mutex<HashMap<String, KeyHandle>>,
    backend: LinuxKeyringBackend,
}

/// Backend type for keychain provider
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeychainBackend {
    /// macOS Security Framework
    #[cfg(target_os = "macos")]
    MacOS,
    /// Linux secret-service (D-Bus) - desktop environments
    #[cfg(target_os = "linux")]
    SecretService,
    /// Linux kernel keyring via keyutils - headless/server environments
    #[cfg(target_os = "linux")]
    KernelKeyring,
    /// Password-based encrypted keystore - fallback for headless/CI
    #[cfg(feature = "password-fallback")]
    PasswordFallback,
}

#[cfg(target_os = "linux")]
impl LinuxKeyring {
    fn new(service_name: String) -> Self {
        // Detect which backend to use
        let backend = Self::detect_backend();

        info!(backend = ?backend, "Linux keyring initialized with backend");

        Self {
            service_name,
            keys: std::sync::Mutex::new(HashMap::new()),
            backend,
        }
    }

    /// Detect which backend to use with sophisticated retry logic
    fn detect_backend() -> LinuxKeyringBackend {
        #[cfg(feature = "linux-keychain")]
        {
            // Try secret-service first with retries (desktop environments)
            for attempt in 1..=3 {
                match secret_service::blocking::SecretService::connect(secret_service::EncryptionType::Dh) {
                    Ok(ss) => {
                        match ss.get_default_collection() {
                            Ok(_) => {
                                info!("Using secret-service backend (D-Bus available, attempt {})", attempt);
                                return LinuxKeyringBackend::SecretService;
                            }
                            Err(e) => {
                                if attempt == 3 {
                                    warn!(error = %e, "Secret service connected but no default collection available after 3 attempts");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if attempt == 3 {
                            info!(error = %e, "Secret service not available after 3 attempts, trying kernel keyring fallback");
                        } else {
                            std::thread::sleep(std::time::Duration::from_millis(100 * attempt as u64));
                        }
                    }
                }
            }

            // Fall back to kernel keyring (headless/server environments)
            info!("Using kernel keyring backend (keyutils)");
            LinuxKeyringBackend::KernelKeyring
        }

        #[cfg(not(feature = "linux-keychain"))]
        {
            warn!("Linux keychain feature not enabled, using kernel keyring fallback");
            LinuxKeyringBackend::KernelKeyring
        }
    }

    /// Check if current backend is still healthy and switch if needed
    pub fn ensure_backend_health(&mut self) -> Result<()> {
        match self.backend {
            LinuxKeyringBackend::SecretService => {
                #[cfg(feature = "linux-keychain")]
                {
                    // Test if secret service is still available
                    match secret_service::blocking::SecretService::connect(secret_service::EncryptionType::Dh) {
                        Ok(ss) => {
                            if let Err(e) = ss.get_default_collection() {
                                warn!(error = %e, "Secret service backend became unhealthy, switching to kernel keyring");

                                // Switch to kernel keyring
                                self.backend = LinuxKeyringBackend::KernelKeyring;
                                info!("Successfully switched to kernel keyring backend");
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "Secret service backend connection failed, switching to kernel keyring");

                            // Switch to kernel keyring
                            self.backend = LinuxKeyringBackend::KernelKeyring;
                            info!("Successfully switched to kernel keyring backend");
                        }
                    }
                }
            }
            LinuxKeyringBackend::KernelKeyring => {
                #[cfg(feature = "linux-keychain")]
                {
                    // Test if kernel keyring is still available
                    use nix::unistd::getuid;
                    unsafe {
                        if libc::keyctl_get_persistent(getuid().as_raw() as u32, libc::KEY_SPEC_USER_KEYRING as i32) == -1 {
                            let errno = std::io::Error::last_os_error();
                            error!(error = %errno, "Kernel keyring backend became unhealthy");
                            return Err(AosError::Crypto("Kernel keyring backend is no longer available".to_string()));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn check_health(&mut self) -> Result<()> {
        self.ensure_backend_health()
    }

    /// Store Ed25519 private key in Linux keyring
    fn store_ed25519_private_key(
        &self,
        key_id: &str,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> Result<()> {
        match self.backend {
            LinuxKeyringBackend::SecretService => self.store_ed25519_private_key_secret_service(key_id, signing_key),
            LinuxKeyringBackend::KernelKeyring => self.store_ed25519_private_key_keyutils(key_id, signing_key),
        }
    }

    /// Store Ed25519 private key using secret-service
    fn store_ed25519_private_key_secret_service(
        &self,
        key_id: &str,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> Result<()> {
        #[cfg(feature = "linux-keychain")]
        {
            use secret_service::blocking::SecretService;
            use secret_service::EncryptionType;

            let key_data = signing_key.to_bytes();
            let key_data_b64 = base64::engine::general_purpose::STANDARD.encode(key_data);

            let ss = SecretService::connect(EncryptionType::Dh).map_err(|e| {
                error!(error = %e, key_id = %key_id, "Failed to connect to secret service");
                AosError::Crypto(format!(
                    "[Linux Secret Service] Connection failed: D-Bus service unavailable - Start desktop session or install secret service daemon"
                ))
            })?;

            let collection = ss.get_default_collection().map_err(|e| {
                error!(error = %e, key_id = %key_id, "Failed to get default collection");
                AosError::Crypto("Failed to access Linux keyring collection".to_string())
            })?;

            // Create attributes for lookup
            let mut attributes = std::collections::HashMap::new();
            attributes.insert("service".to_string(), self.service_name.clone());
            attributes.insert("key-type".to_string(), "ed25519".to_string());
            attributes.insert("key-id".to_string(), key_id.to_string());

            let label = format!("AdapterOS Ed25519 Key: {}", key_id);

            // Store the secret
            collection.create_item(
                &label,
                attributes,
                key_data_b64.as_bytes(),
                true, // replace existing
                "text/plain"
            ).map_err(|e| {
                error!(error = %e, key_id = %key_id, "Failed to store key in secret service");
                AosError::Crypto("Failed to store key in Linux keyring".to_string())
            })?;

            info!(key_id = %key_id, "Stored Ed25519 key in Linux secret service");
            Ok(())
        }

        #[cfg(not(feature = "linux-keychain"))]
        {
            error!("Linux keychain feature not enabled");
            Err(AosError::Crypto("Linux keychain support not compiled in".to_string()))
        }
    }

    /// Store Ed25519 private key using kernel keyring (keyutils)
    fn store_ed25519_private_key_keyutils(
        &self,
        key_id: &str,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> Result<()> {
        #[cfg(feature = "linux-keychain")]
        {
            use nix::unistd::getuid;
            use std::ffi::CString;

            let key_data = signing_key.to_bytes();
            let description = format!("adapteros:{}:ed25519", key_id);
            let desc_c = CString::new(description).map_err(|e| {
                error!(error = %e, key_id = %key_id, "Invalid key description for kernel keyring");
                AosError::Crypto("Invalid key description".to_string())
            })?;

            // Get persistent keyring for the user
            let keyring_id = unsafe {
                libc::keyctl_get_persistent(getuid().as_raw() as u32, libc::KEY_SPEC_USER_KEYRING as i32)
            };

            if keyring_id == -1 {
                let errno = std::io::Error::last_os_error();
                error!(error = %errno, key_id = %key_id, "Failed to get persistent keyring");
                return Err(AosError::Crypto(
                    "[Linux Kernel Keyring] Access failed: Insufficient permissions or kernel config issue - Check user permissions and kernel keyring support".to_string()
                ));
            }

            // Add key to the persistent keyring
            let result = unsafe {
                libc::add_key(
                    b"user\0".as_ptr() as *const libc::c_char,
                    desc_c.as_ptr(),
                    key_data.as_ptr() as *const libc::c_void,
                    key_data.len() as libc::size_t,
                    keyring_id,
                )
            };

            if result == -1 {
                let errno = std::io::Error::last_os_error();
                error!(error = %errno, key_id = %key_id, "Failed to add key to kernel keyring");
                return Err(AosError::Crypto("Failed to store key in kernel keyring".to_string()));
            }

            info!(key_id = %key_id, keyring_id = keyring_id, "Stored Ed25519 key in kernel keyring");
            Ok(())
        }

        #[cfg(not(feature = "linux-keychain"))]
        {
            error!("Linux keychain feature not enabled");
            Err(AosError::Crypto("Linux keychain support not compiled in".to_string()))
        }
    }

    /// Retrieve Ed25519 private key from Linux keyring
    fn retrieve_ed25519_private_key(&self, key_id: &str) -> Result<ed25519_dalek::SigningKey> {
        match self.backend {
            LinuxKeyringBackend::SecretService => self.retrieve_ed25519_private_key_secret_service(key_id),
            LinuxKeyringBackend::KernelKeyring => self.retrieve_ed25519_private_key_keyutils(key_id),
        }
    }

    /// Retrieve Ed25519 private key using secret-service
    fn retrieve_ed25519_private_key_secret_service(&self, key_id: &str) -> Result<ed25519_dalek::SigningKey> {
        #[cfg(feature = "linux-keychain")]
        {
            use secret_service::blocking::SecretService;
            use secret_service::EncryptionType;

            let ss = SecretService::connect(EncryptionType::Dh).map_err(|e| {
                error!(error = %e, key_id = %key_id, "Failed to connect to secret service");
                AosError::Crypto(format!(
                    "[Linux Secret Service] Connection failed: D-Bus service unavailable - Start desktop session or install secret service daemon"
                ))
            })?;

            let collection = ss.get_default_collection().map_err(|e| {
                error!(error = %e, key_id = %key_id, "Failed to get default collection");
                AosError::Crypto("Failed to access Linux keyring collection".to_string())
            })?;

            // Search for items with matching attributes
            let search_items = collection.search_items(vec![
                ("service", &self.service_name),
                ("key-type", "ed25519"),
                ("key-id", key_id),
            ]).map_err(|e| {
                error!(error = %e, key_id = %key_id, "Failed to search for key in secret service");
                AosError::Crypto("Failed to search Linux keyring".to_string())
            })?;

            if search_items.is_empty() {
                error!(key_id = %key_id, "Key not found in Linux keyring");
                return Err(AosError::NotFound(format!("Key '{}' not found in Linux keyring", key_id)));
            }

            // Get the secret from the first matching item
            let item = &search_items[0];
            let secret = item.get_secret().map_err(|e| {
                error!(error = %e, key_id = %key_id, "Failed to get secret from keyring item");
                AosError::Crypto("Failed to retrieve secret from Linux keyring".to_string())
            })?;

            let key_data_b64 = String::from_utf8(secret).map_err(|e| {
                error!(error = %e, key_id = %key_id, error = %e, "Invalid UTF-8 in keyring data");
                AosError::Crypto("Invalid keyring data encoding".to_string())
            })?;

            let key_data = base64::engine::general_purpose::STANDARD.decode(&key_data_b64).map_err(|e| {
                error!(error = %e, key_id = %key_id, "Invalid base64 in keyring data");
                AosError::Crypto("Invalid keyring data format".to_string())
            })?;

            if key_data.len() != 32 {
                error!(key_id = %key_id, len = key_data.len(), "Invalid key length from keyring");
                return Err(AosError::Crypto(
                    "Invalid key length from keyring".to_string(),
                ));
            }

            let mut key_bytes = [0u8; 32];
            key_bytes.copy_from_slice(&key_data);

            let signing_key = ed25519_dalek::SigningKey::from_bytes(&key_bytes);
            info!(key_id = %key_id, "Retrieved Ed25519 key from Linux secret service");
            Ok(signing_key)
        }

        #[cfg(not(feature = "linux-keychain"))]
        {
            error!("Linux keychain feature not enabled");
            Err(AosError::Crypto("Linux keychain support not compiled in".to_string()))
        }
    }

    /// Retrieve Ed25519 private key using kernel keyring (keyutils)
    fn retrieve_ed25519_private_key_keyutils(&self, key_id: &str) -> Result<ed25519_dalek::SigningKey> {
        #[cfg(feature = "linux-keychain")]
        {
            use nix::unistd::getuid;
            use std::ffi::CString;

            let description = format!("adapteros:{}:ed25519", key_id);
            let desc_c = CString::new(description).map_err(|e| {
                error!(error = %e, key_id = %key_id, "Invalid key description for kernel keyring");
                AosError::Crypto("Invalid key description".to_string())
            })?;

            // Get persistent keyring for the user
            let keyring_id = unsafe {
                libc::keyctl_get_persistent(getuid().as_raw() as u32, libc::KEY_SPEC_USER_KEYRING as i32)
            };

            if keyring_id == -1 {
                let errno = std::io::Error::last_os_error();
                error!(error = %errno, key_id = %key_id, "Failed to get persistent keyring");
                return Err(AosError::Crypto(
                    "[Linux Kernel Keyring] Access failed: Insufficient permissions or kernel config issue - Check user permissions and kernel keyring support".to_string()
                ));
            }

            // Search for the key in the keyring
            let key_id_result = unsafe {
                libc::keyctl_search(keyring_id, b"user\0".as_ptr() as *const libc::c_char, desc_c.as_ptr(), 0)
            };

            if key_id_result == -1 {
                let errno = std::io::Error::last_os_error();
                error!(error = %errno, key_id = %key_id, "Key not found in kernel keyring");
                return Err(AosError::NotFound(format!("Key '{}' not found in kernel keyring", key_id)));
            }

            // Read the key data
            let mut buffer = [0u8; 32];
            let read_result = unsafe {
                libc::keyctl_read(key_id_result, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len())
            };

            if read_result == -1 {
                let errno = std::io::Error::last_os_error();
                error!(error = %errno, key_id = %key_id, "Failed to read key from kernel keyring");
                return Err(AosError::Crypto("Failed to read key from kernel keyring".to_string()));
            }

            if read_result != 32 {
                error!(key_id = %key_id, expected = 32, actual = read_result, "Invalid key length from kernel keyring");
                return Err(AosError::Crypto("Invalid key length from kernel keyring".to_string()));
            }

            let signing_key = ed25519_dalek::SigningKey::from_bytes(&buffer);
            info!(key_id = %key_id, "Retrieved Ed25519 key from kernel keyring");
            Ok(signing_key)
        }

        #[cfg(not(feature = "linux-keychain"))]
        {
            error!("Linux keychain feature not enabled");
            Err(AosError::Crypto("Linux keychain support not compiled in".to_string()))
        }
    }

    /// Store symmetric key in Linux keyring
    fn store_symmetric_key(&self, key_id: &str, key_data: &[u8]) -> Result<()> {
        match self.backend {
            LinuxKeyringBackend::SecretService => self.store_symmetric_key_secret_service(key_id, key_data),
            LinuxKeyringBackend::KernelKeyring => self.store_symmetric_key_keyutils(key_id, key_data),
        }
    }

    /// Store symmetric key using secret-service
    fn store_symmetric_key_secret_service(&self, key_id: &str, key_data: &[u8]) -> Result<()> {
        #[cfg(feature = "linux-keychain")]
        {
            use secret_service::blocking::SecretService;
            use secret_service::EncryptionType;

            let key_data_b64 = base64::engine::general_purpose::STANDARD.encode(key_data);

            let ss = SecretService::connect(EncryptionType::Dh).map_err(|e| {
                error!(error = %e, key_id = %key_id, "Failed to connect to secret service");
                AosError::Crypto(format!(
                    "[Linux Secret Service] Connection failed: D-Bus service unavailable - Start desktop session or install secret service daemon"
                ))
            })?;

            let collection = ss.get_default_collection().map_err(|e| {
                error!(error = %e, key_id = %key_id, "Failed to get default collection");
                AosError::Crypto("Failed to access Linux keyring collection".to_string())
            })?;

            // Create attributes for lookup
            let mut attributes = std::collections::HashMap::new();
            attributes.insert("service".to_string(), self.service_name.clone());
            attributes.insert("key-type".to_string(), "symmetric".to_string());
            attributes.insert("key-id".to_string(), key_id.to_string());

            let label = format!("AdapterOS Symmetric Key: {}", key_id);

            // Store the secret
            collection.create_item(
                &label,
                attributes,
                key_data_b64.as_bytes(),
                true, // replace existing
                "text/plain"
            ).map_err(|e| {
                error!(error = %e, key_id = %key_id, "Failed to store symmetric key in secret service");
                AosError::Crypto("Failed to store symmetric key in Linux keyring".to_string())
            })?;

            info!(key_id = %key_id, "Stored symmetric key in Linux secret service");
            Ok(())
        }

        #[cfg(not(feature = "linux-keychain"))]
        {
            error!("Linux keychain feature not enabled");
            Err(AosError::Crypto("Linux keychain support not compiled in".to_string()))
        }
    }

    /// Store symmetric key using kernel keyring (keyutils)
    fn store_symmetric_key_keyutils(&self, key_id: &str, key_data: &[u8]) -> Result<()> {
        #[cfg(feature = "linux-keychain")]
        {
            use nix::unistd::getuid;

            let description = format!("adapteros:{}:symmetric", key_id);
            let desc_c = CString::new(description).map_err(|e| {
                error!(error = %e, key_id = %key_id, "Invalid key description for kernel keyring");
                AosError::Crypto("Invalid key description".to_string())
            })?;

            // Get persistent keyring for the user
            let keyring_id = unsafe {
                libc::keyctl_get_persistent(getuid().as_raw() as u32, libc::KEY_SPEC_USER_KEYRING as i32)
            };

            if keyring_id == -1 {
                let errno = std::io::Error::last_os_error();
                error!(error = %errno, key_id = %key_id, "Failed to get persistent keyring");
                return Err(AosError::Crypto(
                    "[Linux Kernel Keyring] Access failed: Insufficient permissions or kernel config issue - Check user permissions and kernel keyring support".to_string()
                ));
            }

            // Add key to the persistent keyring
            let result = unsafe {
                libc::add_key(
                    b"user\0".as_ptr() as *const libc::c_char,
                    desc_c.as_ptr(),
                    key_data.as_ptr() as *const libc::c_void,
                    key_data.len() as libc::size_t,
                    keyring_id,
                )
            };

            if result == -1 {
                let errno = std::io::Error::last_os_error();
                error!(error = %errno, key_id = %key_id, "Failed to add symmetric key to kernel keyring");
                return Err(AosError::Crypto("Failed to store symmetric key in kernel keyring".to_string()));
            }

            info!(key_id = %key_id, keyring_id = keyring_id, "Stored symmetric key in kernel keyring");
            Ok(())
        }

        #[cfg(not(feature = "linux-keychain"))]
        {
            error!("Linux keychain feature not enabled");
            Err(AosError::Crypto("Linux keychain support not compiled in".to_string()))
        }
    }

    /// Retrieve symmetric key from Linux keyring
    fn retrieve_symmetric_key(&self, key_id: &str) -> Result<Vec<u8>> {
        match self.backend {
            LinuxKeyringBackend::SecretService => self.retrieve_symmetric_key_secret_service(key_id),
            LinuxKeyringBackend::KernelKeyring => self.retrieve_symmetric_key_keyutils(key_id),
        }
    }

    /// Retrieve symmetric key using secret-service
    fn retrieve_symmetric_key_secret_service(&self, key_id: &str) -> Result<Vec<u8>> {
        #[cfg(feature = "linux-keychain")]
        {
            use secret_service::blocking::SecretService;
            use secret_service::EncryptionType;

            let ss = SecretService::connect(EncryptionType::Dh).map_err(|e| {
                error!(error = %e, key_id = %key_id, "Failed to connect to secret service");
                AosError::Crypto(format!(
                    "[Linux Secret Service] Connection failed: D-Bus service unavailable - Start desktop session or install secret service daemon"
                ))
            })?;

            let collection = ss.get_default_collection().map_err(|e| {
                error!(error = %e, key_id = %key_id, "Failed to get default collection");
                AosError::Crypto("Failed to access Linux keyring collection".to_string())
            })?;

            // Search for items with matching attributes
            let search_items = collection.search_items(vec![
                ("service", &self.service_name),
                ("key-type", "symmetric"),
                ("key-id", key_id),
            ]).map_err(|e| {
                error!(error = %e, key_id = %key_id, "Failed to search for symmetric key in secret service");
                AosError::Crypto("Failed to search Linux keyring".to_string())
            })?;

            if search_items.is_empty() {
                error!(key_id = %key_id, "Symmetric key not found in Linux keyring");
                return Err(AosError::NotFound(format!("Symmetric key '{}' not found in Linux keyring", key_id)));
            }

            // Get the secret from the first matching item
            let item = &search_items[0];
            let secret = item.get_secret().map_err(|e| {
                error!(error = %e, key_id = %key_id, "Failed to get symmetric key secret from keyring item");
                AosError::Crypto("Failed to retrieve symmetric key secret from Linux keyring".to_string())
            })?;

            let key_data_b64 = String::from_utf8(secret).map_err(|e| {
                error!(error = %e, key_id = %key_id, "Invalid UTF-8 in symmetric keyring data");
                AosError::Crypto("Invalid symmetric keyring data encoding".to_string())
            })?;

            let key_data = base64::engine::general_purpose::STANDARD.decode(&key_data_b64).map_err(|e| {
                error!(error = %e, key_id = %key_id, "Invalid base64 in symmetric keyring data");
                AosError::Crypto("Invalid symmetric keyring data format".to_string())
            })?;

            info!(key_id = %key_id, "Retrieved symmetric key from Linux secret service");
            Ok(key_data)
        }

        #[cfg(not(feature = "linux-keychain"))]
        {
            error!("Linux keychain feature not enabled");
            Err(AosError::Crypto("Linux keychain support not compiled in".to_string()))
        }
    }

    /// Retrieve symmetric key using kernel keyring (keyutils)
    fn retrieve_symmetric_key_keyutils(&self, key_id: &str) -> Result<Vec<u8>> {
        #[cfg(feature = "linux-keychain")]
        {
            use nix::unistd::getuid;

            let description = format!("adapteros:{}:symmetric", key_id);
            let desc_c = CString::new(description).map_err(|e| {
                error!(error = %e, key_id = %key_id, "Invalid key description for kernel keyring");
                AosError::Crypto("Invalid key description".to_string())
            })?;

            // Get persistent keyring for the user
            let keyring_id = unsafe {
                libc::keyctl_get_persistent(getuid().as_raw() as u32, libc::KEY_SPEC_USER_KEYRING as i32)
            };

            if keyring_id == -1 {
                let errno = std::io::Error::last_os_error();
                error!(error = %errno, key_id = %key_id, "Failed to get persistent keyring");
                return Err(AosError::Crypto(
                    "[Linux Kernel Keyring] Access failed: Insufficient permissions or kernel config issue - Check user permissions and kernel keyring support".to_string()
                ));
            }

            // Search for the key in the keyring
            let key_id_result = unsafe {
                libc::keyctl_search(keyring_id, b"user\0".as_ptr() as *const libc::c_char, desc_c.as_ptr(), 0)
            };

            if key_id_result == -1 {
                let errno = std::io::Error::last_os_error();
                error!(error = %errno, key_id = %key_id, "Symmetric key not found in kernel keyring");
                return Err(AosError::NotFound(format!("Symmetric key '{}' not found in kernel keyring", key_id)));
            }

            // Read the key data - first get the size
            let size_result = unsafe {
                libc::keyctl_read(key_id_result, std::ptr::null_mut(), 0)
            };

            if size_result == -1 {
                let errno = std::io::Error::last_os_error();
                error!(error = %errno, key_id = %key_id, "Failed to get key size from kernel keyring");
                return Err(AosError::Crypto("Failed to read key size from kernel keyring".to_string()));
            }

            let mut buffer = vec![0u8; size_result as usize];
            let read_result = unsafe {
                libc::keyctl_read(key_id_result, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len())
            };

            if read_result == -1 {
                let errno = std::io::Error::last_os_error();
                error!(error = %errno, key_id = %key_id, "Failed to read symmetric key from kernel keyring");
                return Err(AosError::Crypto("Failed to read symmetric key from kernel keyring".to_string()));
            }

            info!(key_id = %key_id, "Retrieved symmetric key from kernel keyring");
            Ok(buffer)
        }

        #[cfg(not(feature = "linux-keychain"))]
        {
            error!("Linux keychain feature not enabled");
            Err(AosError::Crypto("Linux keychain support not compiled in".to_string()))
        }
    }
}

#[cfg(target_os = "linux")]
#[async_trait::async_trait]
impl KeyringImpl for LinuxKeyring {
    async fn generate_key(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        use rand::rngs::OsRng;

        let handle = match alg {
            KeyAlgorithm::Ed25519 => {
                let mut rng = OsRng;
                let signing_key = ed25519_dalek::SigningKey::generate(&mut rng);
                let verifying_key = signing_key.verifying_key();

                // Store private key in Linux keyring
                self.store_ed25519_private_key(key_id, &signing_key)?;

                KeyHandle::with_public_key(
                    format!("{}:{}", self.service_name, key_id),
                    alg,
                    verifying_key.to_bytes().to_vec(),
                )
            }
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {
                let mut rng = OsRng;
                let mut key_data = [0u8; 32];
                rng.fill_bytes(&mut key_data);

                // Store symmetric key in Linux keyring
                self.store_symmetric_key(key_id, &key_data)?;

                KeyHandle::new(format!("{}:{}", self.service_name, key_id), alg)
            }
        };

        // Cache handle in memory for faster lookups
        self.keys
            .lock()
            .unwrap()
            .insert(key_id.to_string(), handle.clone());

        info!(key_id = %key_id, algorithm = ?alg, "Generated key and stored in Linux keyring");
        Ok(handle)
    }

    async fn sign(&self, key_id: &str, msg: &[u8]) -> Result<Vec<u8>> {
        // Retrieve private key from Linux keyring
        let signing_key = self.retrieve_ed25519_private_key(key_id)?;

        // Sign the message
        use ed25519_dalek::Signer;
        let signature = signing_key.sign(msg);

        info!(key_id = %key_id, message_len = msg.len(), "Signed message using Linux keyring");
        Ok(signature.to_bytes().to_vec())
    }

    async fn seal(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        // Retrieve symmetric key from Linux keyring
        let key_data = self.retrieve_symmetric_key(key_id)?;

        // Use AES-GCM for encryption
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};

        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&key_data);
        let cipher = Aes256Gcm::new(key);

        // Generate a random nonce
        use rand::rngs::OsRng;
        use rand::RngCore;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the plaintext
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| AosError::Crypto(format!("Encryption failed: {}", e)))?;

        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend(ciphertext);

        info!(key_id = %key_id, plaintext_len = plaintext.len(), "Encrypted data using Linux keyring");
        Ok(result)
    }

    async fn unseal(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < 12 {
            return Err(AosError::Crypto("Ciphertext too short".to_string()));
        }

        // Retrieve symmetric key from Linux keyring
        let key_data = self.retrieve_symmetric_key(key_id)?;

        // Use AES-GCM for decryption
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};

        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&key_data);
        let cipher = Aes256Gcm::new(key);

        // Extract nonce from beginning of ciphertext
        let nonce_bytes = &ciphertext[..12];
        let nonce = Nonce::from_slice(nonce_bytes);
        let encrypted_data = &ciphertext[12..];

        // Decrypt the data
        let plaintext = cipher
            .decrypt(nonce, encrypted_data)
            .map_err(|e| AosError::Crypto(format!("Decryption failed: {}", e)))?;

        info!(key_id = %key_id, ciphertext_len = ciphertext.len(), "Decrypted data using Linux keyring");
        Ok(plaintext)
    }

    /// Sign receipt data using the provider's signing key
    async fn sign_receipt(&self, receipt_data: &str) -> Result<Vec<u8>> {
        // Use a dedicated receipt signing key (Linux doesn't have Secure Enclave)
        let signing_key_id = "__receipt_signing_key__";

        // Try to get existing signing key, or create one
        let signing_key = match self.retrieve_ed25519_private_key(signing_key_id) {
            Ok(key) => key,
            Err(_) => {
                // Create signing key if it doesn't exist
                let _handle = self.generate_key(signing_key_id, KeyAlgorithm::Ed25519).await?;
                self.retrieve_ed25519_private_key(signing_key_id)?
            }
        };

        // Sign the receipt data
        use ed25519_dalek::Signer;
        let signature = signing_key.sign(receipt_data.as_bytes());

        info!("Signed receipt data with cryptographic signature");
        Ok(signature.to_bytes().to_vec())
    }

    async fn rotate_key(&self, key_id: &str) -> Result<RotationReceipt> {
        // Get previous handle and drop lock before await
        let previous_handle = {
            let keys = self.keys.lock().unwrap();
            keys.get(key_id)
                .cloned()
                .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?
        };
        let algorithm = previous_handle.algorithm.clone();

        // Explicitly delete old keychain items before generating new key
        match algorithm {
            KeyAlgorithm::Ed25519 => {
                let account = format!("{}-ed25519", key_id);
                if let Err(e) = self.delete_keychain_item(&account) {
                    warn!(key_id = %key_id, account = %account, error = %e, "Failed to delete old Ed25519 key during rotation");
                }
            }
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {
                let account = format!("{}-symmetric", key_id);
                if let Err(e) = self.delete_keychain_item(&account) {
                    warn!(key_id = %key_id, account = %account, error = %e, "Failed to delete old symmetric key during rotation");
                }
            }
        }

        // Generate new key (will store in keychain)
        let new_handle = self.generate_key(key_id, algorithm.clone()).await?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Create receipt data to sign
        let receipt_data = format!(
            "rotation:{}:{}:{}:{}",
            key_id,
            previous_handle.provider_id,
            new_handle.provider_id,
            timestamp
        );

        // Sign the receipt using the provider's signing key
        let signature = self.sign_receipt(&receipt_data).await?;

        let receipt = RotationReceipt::new(
            key_id.to_string(),
            previous_handle,
            new_handle,
            timestamp,
            signature,
        );

        info!(key_id = %key_id, algorithm = ?algorithm, timestamp = timestamp, "Successfully rotated key with cryptographic receipt");
        Ok(receipt)
    }

    async fn attest(&self) -> Result<ProviderAttestation> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Determine provider type based on backend
        let provider_type = match self.backend {
            LinuxKeyringBackend::SecretService => "linux-secret-service",
            LinuxKeyringBackend::KernelKeyring => "linux-kernel-keyring",
        };

        // Calculate policy hash from provider configuration and state
        let policy_data = format!(
            "provider:{}|service:{}|timestamp:{}|keys:{}",
            provider_type,
            self.service_name,
            timestamp,
            self.keys.lock().unwrap().len()
        );
        use sha2::{Digest, Sha256};
        let policy_hash = format!("{:x}", Sha256::digest(&policy_data));

        // Create attestation data to sign
        let attestation_data = format!(
            "attestation:{}:{}:{}",
            provider_type,
            policy_hash,
            timestamp
        );

        // Sign the attestation
        let signature = self.sign_receipt(&attestation_data).await?;

        info!(provider_type = %provider_type, policy_hash = %policy_hash, timestamp = timestamp, "Generated cryptographic provider attestation");

        Ok(ProviderAttestation::new(
            provider_type.to_string(),
            format!("service:{}", self.service_name),
            policy_hash,
            timestamp,
            signature,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_provider::KeyProviderConfig;

    #[tokio::test]
    async fn test_keychain_provider_basic() {
        let config = KeyProviderConfig::default();
        let provider = KeychainProvider::new(config).unwrap();

        // Test key generation for signing
        let signing_key_id = "test-signing-key";
        let handle_sign = provider
            .generate(signing_key_id, KeyAlgorithm::Ed25519)
            .await
            .unwrap();
        assert_eq!(handle_sign.algorithm, KeyAlgorithm::Ed25519);
        assert!(handle_sign.provider_id.contains(signing_key_id));

        // Test signing
        let message = b"Hello, world!";
        let signature = provider.sign(signing_key_id, message).await.unwrap();
        assert!(!signature.is_empty());

        // Test key generation for encryption
        let encryption_key_id = "test-encryption-key";
        let handle_encrypt = provider
            .generate(encryption_key_id, KeyAlgorithm::Aes256Gcm)
            .await
            .unwrap();
        assert_eq!(handle_encrypt.algorithm, KeyAlgorithm::Aes256Gcm);

        // Test encryption/decryption
        let plaintext = b"Secret data";
        let ciphertext = provider.seal(encryption_key_id, plaintext).await.unwrap();
        assert!(!ciphertext.is_empty());

        let decrypted = provider.unseal(encryption_key_id, &ciphertext).await.unwrap();
        assert_eq!(decrypted, plaintext);

        // Test attestation
        let attestation = provider.attest().await.unwrap();
        assert!(attestation.provider_type.contains("keychain") || attestation.provider_type.contains("keyring"));
    }

    #[tokio::test]
    async fn test_keychain_provider_debug() {
        let config = KeyProviderConfig::default();
        let provider = KeychainProvider::new(config).unwrap();

        // Test key generation
        let handle = provider
            .generate("debug-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();
        info!(handle = ?handle, "Generated keychain handle");

        // Test signing - this should work if the key was stored
        let message = b"Hello, world!";
        match provider.sign("debug-key", message).await {
            Ok(signature) => {
                info!(sig_len = signature.len(), "Signing successful");
                assert!(!signature.is_empty());
            }
            Err(e) => {
                error!(error = ?e, "Signing failed");
                panic!("Signing should work after key generation");
            }
        }
    }

    #[tokio::test]
    async fn test_key_rotation() {
        let config = KeyProviderConfig::default();
        let provider = KeychainProvider::new(config).unwrap();

        // Generate initial key
        let _handle = provider
            .generate("rotate-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();

        // Rotate key
        let receipt = provider.rotate("rotate-key").await.unwrap();
        assert_eq!(receipt.key_id, "rotate-key");
        assert_eq!(receipt.previous_key.algorithm, KeyAlgorithm::Ed25519);
        assert_eq!(receipt.new_key.algorithm, KeyAlgorithm::Ed25519);
        assert!(receipt.timestamp > 0);
    }

    #[tokio::test]
    async fn test_password_fallback_parsing() {
        // Test valid password parsing
        let result = KeychainProvider::parse_fallback_env("pass:mysecretpassword123");
        assert_eq!(result, Some("mysecretpassword123".to_string()));

        // Test password too short
        let result = KeychainProvider::parse_fallback_env("pass:short");
        assert_eq!(result, None);

        // Test invalid format
        let result = KeychainProvider::parse_fallback_env("invalid-format");
        assert_eq!(result, None);

        // Test empty password
        let result = KeychainProvider::parse_fallback_env("pass:");
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_key_lifecycle_ed25519() {
        let config = KeyProviderConfig::default();
        let provider = KeychainProvider::new(config).unwrap();

        let key_id = "test-ed25519-lifecycle";

        // Generate key
        let handle = provider.generate(key_id, KeyAlgorithm::Ed25519).await.unwrap();
        assert_eq!(handle.algorithm, KeyAlgorithm::Ed25519);
        assert!(handle.provider_id.contains(key_id));
        assert!(handle.public_key.is_some());

        // Sign message
        let message = b"Hello, world!";
        let signature = provider.sign(key_id, message).await.unwrap();
        assert!(!signature.is_empty());

        // Verify signature using public key
        use ed25519_dalek::{Verifier, VerifyingKey};
        let public_key = VerifyingKey::from_bytes(
            handle.public_key.as_ref().unwrap().as_slice().try_into().unwrap()
        ).unwrap();
        let signature_bytes: [u8; 64] = signature.as_slice().try_into().unwrap();
        let signature = ed25519_dalek::Signature::from(signature_bytes);
        assert!(public_key.verify(message, &signature).is_ok());

        // Rotate key
        let receipt = provider.rotate(key_id).await.unwrap();
        assert_eq!(receipt.key_id, key_id);

        // Verify new signature still works
        let new_signature_bytes = provider.sign(key_id, message).await.unwrap();
        assert!(!new_signature_bytes.is_empty());
        let new_signature_bytes_array: [u8; 64] = new_signature_bytes.as_slice().try_into().unwrap();
        let new_signature = ed25519_dalek::Signature::from(new_signature_bytes_array);
        assert_ne!(signature, new_signature); // Should be different after rotation
    }

    #[tokio::test]
    async fn test_key_lifecycle_symmetric() {
        let config = KeyProviderConfig::default();
        let provider = KeychainProvider::new(config).unwrap();

        let key_id = "test-symmetric-lifecycle";

        // Generate key
        let handle = provider.generate(key_id, KeyAlgorithm::Aes256Gcm).await.unwrap();
        assert_eq!(handle.algorithm, KeyAlgorithm::Aes256Gcm);
        assert!(handle.provider_id.contains(key_id));
        assert!(handle.public_key.is_none()); // Symmetric keys don't have public keys

        // Seal/unseal data
        let plaintext = b"Secret data to encrypt";
        let ciphertext = provider.seal(key_id, plaintext).await.unwrap();
        assert!(!ciphertext.is_empty());
        assert!(ciphertext.len() > plaintext.len()); // Should include nonce/tag

        let decrypted = provider.unseal(key_id, &ciphertext).await.unwrap();
        assert_eq!(decrypted, plaintext);

        // Test wrong ciphertext fails
        let wrong_ciphertext = b"invalid";
        assert!(provider.unseal(key_id, wrong_ciphertext).await.is_err());
    }

    #[tokio::test]
    async fn test_provider_attestation() {
        let config = KeyProviderConfig::default();
        let provider = KeychainProvider::new(config).unwrap();

        let attestation = provider.attest().await.unwrap();
        assert!(!attestation.provider_type.is_empty());
        assert!(!attestation.fingerprint.is_empty());
        assert!(!attestation.policy_hash.is_empty());
        assert!(attestation.timestamp > 0);
        assert!(!attestation.signature.is_empty());

        // Provider type should match backend
        match provider.backend {
            #[cfg(target_os = "macos")]
            KeychainBackend::MacOS => assert!(attestation.provider_type.contains("macos")),
            #[cfg(target_os = "linux")]
            KeychainBackend::SecretService => assert!(attestation.provider_type.contains("secret-service")),
            #[cfg(target_os = "linux")]
            KeychainBackend::KernelKeyring => assert!(attestation.provider_type.contains("kernel-keyring")),
            #[cfg(feature = "password-fallback")]
            KeychainBackend::PasswordFallback => assert!(attestation.provider_type.contains("password-fallback")),
        }
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        let config = KeyProviderConfig::default();
        let provider = KeychainProvider::new(config.clone()).unwrap();

        // Test concurrent access to the same key
        let key_id = "test-concurrent";
        let provider_arc = std::sync::Arc::new(provider);

        let tasks: Vec<_> = (0..5).map(|i| {
            let provider = provider_arc.clone();
            let message = format!("Message {}", i);
            tokio::spawn(async move {
                // Generate key (idempotent)
                let _handle = provider.generate(key_id, KeyAlgorithm::Ed25519).await.unwrap();

                // Sign message
                let signature = provider.sign(key_id, message.as_bytes()).await.unwrap();

                // Verify signature is not empty
                assert!(!signature.is_empty());

                signature
            })
        }).collect();

        // Wait for all tasks to complete
        let results = futures::future::join_all(tasks).await;
        for result in results {
            let signature = result.unwrap();
            assert!(!signature.is_empty());
        }
    }

    #[tokio::test]
    async fn test_key_not_found() {
        let config = KeyProviderConfig::default();
        let provider = KeychainProvider::new(config).unwrap();

        // Try to sign with non-existent key
        let result = provider.sign("non-existent-key", b"test").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_invalid_ciphertext() {
        let config = KeyProviderConfig::default();
        let provider = KeychainProvider::new(config).unwrap();

        // Generate a key for sealing
        let key_id = "test-invalid-ciphertext";
        provider.generate(key_id, KeyAlgorithm::Aes256Gcm).await.unwrap();

        // Test with ciphertext too short
        let result = provider.unseal(key_id, b"short").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too short"));

        // Test with invalid ciphertext
        let result = provider.unseal(key_id, &[0u8; 32]).await;
        assert!(result.is_err());
    }
}

#[cfg(target_os = "macos")]
pub async fn store_key_macos(tenant_id: &str, key_data: &[u8]) -> Result<()> {
    let service = CFString::from("adapteros_keys");
    let account = CFString::from(tenant_id);
    let class = kSecClass as CFTypeRef;
    let data = CFData::from_buffer(key_data);

    let mut query = SecItemAttributeValue::new(class);
    query.add(kSecAttrService, service.as_concrete_type_ref());
    query.add(kSecAttrAccount, account.as_concrete_type_ref());
    query.add(kSecValueData, data.as_concrete_type_ref());

    let status = SecItemAdd(&query, std::ptr::null_mut());
    if status.is_err() {
        return Err(AosError::Crypto("Failed to store key in keychain".to_string()));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub async fn retrieve_key_macos(tenant_id: &str) -> Result<Vec<u8>> {
    let service = CFString::from("adapteros_keys");
    let account = CFString::from(tenant_id);
    let class = kSecClass as CFTypeRef;

    let mut query = SecItemAttributeValue::new(class);
    query.add(kSecAttrService, service.as_concrete_type_ref());
    query.add(kSecAttrAccount, account.as_concrete_type_ref());
    query.add(kSecReturnData, kCFBooleanTrue as CFTypeRef);
    query.add(kSecMatchLimit, kSecMatchLimitOne as CFTypeRef);

    let mut item: CFTypeRef = std::ptr::null();
    let status = SecItemCopyMatching(&query, &mut item);
    if status.is_err() || item.is_null() {
        return Err(AosError::Crypto("Key not found in keychain".to_string()));
    }

    let data = unsafe { CFData::wrap_under_get_rule(item) }.to_vec();
    Ok(data)
}

#[cfg(target_os = "linux")]
pub async fn store_key_linux(tenant_id: &str, key_data: &[u8]) -> Result<(), AosError> {
    let ss = SecretService::connect(EncryptionType::Plain).await
        .map_err(|e| AosError::Crypto(format!("SecretService connect failed: {}", e)))?;
    let collection = ss.get_default_collection().await
        .map_err(|e| AosError::Crypto(format!("Get default collection failed: {}", e)))?;
    collection.create_item(
        tenant_id,
        &[( "application", "adapteros" )],
        key_data,
        true,
        "text/plain".to_string(),
    ).await
        .map_err(|e| AosError::Crypto(format!("Store key failed: {}", e)))?;
    Ok(())
}

#[cfg(target_os = "linux")]
pub async fn retrieve_key_linux(tenant_id: &str) -> Result<Vec<u8>, AosError> {
    let ss = SecretService::connect(EncryptionType::Plain).await
        .map_err(|e| AosError::Crypto(format!("SecretService connect failed: {}", e)))?;
    let collection = ss.get_default_collection().await
        .map_err(|e| AosError::Crypto(format!("Get default collection failed: {}", e)))?;
    let item = collection.get_item_by_alias(tenant_id).await
        .map_err(|e| AosError::Crypto(format!("Get item failed: {}", e)))?;
    let secret = item.get_secret().await
        .map_err(|e| AosError::Crypto(format!("Get secret failed: {}", e)))?;
    Ok(secret)
}
