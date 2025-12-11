//! Per-tenant encryption and digest helpers for at-rest protection.
//!
//! This module wraps the secd client to seal/unseal sensitive fields
//! using tenant-scoped keys and keyed BLAKE3 digests. It supports a
//! digest-only mode (no ciphertext persisted) for regulated tenants.

use adapteros_artifacts::secd_client::SecdClient;
use adapteros_core::{derive_seed, AosError, B3Hash, Result};
use base64::Engine;
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const ENV_ENABLE: &str = "AOS_CRYPTO_AT_REST";
const ENV_DIGEST_ONLY: &str = "AOS_CRYPTO_DIGEST_ONLY";
const ENV_SECD_SOCKET: &str = "AOS_SECD_SOCKET";
const ENV_FAKE_LOCAL: &str = "AOS_CRYPTO_FAKE";

/// Representation stored in TEXT columns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedField {
    pub ciphertext_b64: Option<String>,
    pub digest_hex: String,
    pub mode: String,
}

impl EncryptedField {
    pub fn new(ciphertext_b64: Option<String>, digest_hex: String, mode: &str) -> Self {
        Self {
            ciphertext_b64,
            digest_hex,
            mode: mode.to_string(),
        }
    }
}

#[derive(Clone)]
enum Backend {
    Secd(Arc<SecdClient>),
    Local,
}

/// At-rest crypto helper backed by aos-secd.
#[derive(Clone)]
pub struct CryptoAtRest {
    backend: Backend,
    digest_only: bool,
}

impl CryptoAtRest {
    /// Create helper from environment toggles. Returns None when disabled.
    pub fn from_env() -> Option<Self> {
        if std::env::var(ENV_ENABLE).unwrap_or_default() != "1" {
            return None;
        }
        let digest_only = std::env::var(ENV_DIGEST_ONLY).unwrap_or_default() == "1";
        let backend = if std::env::var(ENV_FAKE_LOCAL).unwrap_or_default() == "1" {
            Backend::Local
        } else {
            let socket = std::env::var(ENV_SECD_SOCKET)
                .unwrap_or_else(|_| "/var/run/aos-secd.sock".to_string());
            Backend::Secd(Arc::new(SecdClient::new(socket)))
        };
        Some(Self {
            backend,
            digest_only,
        })
    }

    /// Encrypt plaintext and compute keyed digest for a tenant.
    pub async fn seal(&self, tenant_id: &str, plaintext: &str) -> Result<EncryptedField> {
        let digest = self.digest(tenant_id, plaintext.as_bytes()).await?;
        let digest_hex = hex::encode(digest);

        if self.digest_only {
            return Ok(EncryptedField::new(None, digest_hex, "digest_only"));
        }

        let ciphertext = match &self.backend {
            Backend::Secd(client) => client.seal_tenant(tenant_id, plaintext.as_bytes()).await?,
            Backend::Local => self.local_seal(tenant_id, plaintext.as_bytes())?,
        };
        Ok(EncryptedField::new(
            Some(base64::engine::general_purpose::STANDARD.encode(ciphertext)),
            digest_hex,
            "ciphertext",
        ))
    }

    /// Decrypt an encrypted field. Returns Ok(None) when digest-only.
    pub async fn unseal(&self, tenant_id: &str, field: &EncryptedField) -> Result<Option<String>> {
        if field.ciphertext_b64.is_none() {
            return Ok(None);
        }
        let ciphertext = field
            .ciphertext_b64
            .as_ref()
            .ok_or_else(|| AosError::Crypto("ciphertext missing".into()))?;
        let sealed = base64::engine::general_purpose::STANDARD
            .decode(ciphertext)
            .map_err(|e| AosError::Crypto(format!("Invalid base64 ciphertext: {}", e)))?;
        let plaintext = match &self.backend {
            Backend::Secd(client) => client
                .unseal_tenant(tenant_id, &sealed)
                .await
                .map_err(|e| AosError::Crypto(format!("Unseal failed: {}", e)))?,
            Backend::Local => self.local_unseal(tenant_id, &sealed)?,
        };

        // Verify digest before returning.
        let digest = self.digest(tenant_id, &plaintext).await?;
        if hex::encode(digest) != field.digest_hex {
            return Err(AosError::Crypto(
                "digest mismatch during unseal verification".into(),
            ));
        }

        Ok(Some(String::from_utf8(plaintext).map_err(|e| {
            AosError::Crypto(format!("Invalid UTF-8 plaintext: {}", e))
        })?))
    }

    async fn digest(&self, tenant_id: &str, data: &[u8]) -> Result<[u8; 32]> {
        match &self.backend {
            Backend::Secd(client) => client.digest_tenant(tenant_id, data).await,
            Backend::Local => {
                let key = self.derive_local_key(tenant_id);
                let mut keyed = blake3::Hasher::new_keyed(&key);
                keyed.update(data);
                Ok(*keyed.finalize().as_bytes())
            }
        }
    }

    fn derive_local_key(&self, tenant_id: &str) -> [u8; 32] {
        derive_seed(&B3Hash::hash(tenant_id.as_bytes()), "aos-crypto-at-rest")
    }

    fn local_seal(&self, tenant_id: &str, data: &[u8]) -> Result<Vec<u8>> {
        let key_bytes = self.derive_local_key(tenant_id);
        let key = Key::from_slice(&key_bytes);
        let cipher = ChaCha20Poly1305::new(key);

        let nonce_seed = derive_seed(&B3Hash::hash(data), "aos-crypto-at-rest-nonce");
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes.copy_from_slice(&nonce_seed[..12]);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, data)
            .map_err(|e| AosError::Crypto(format!("Local encrypt failed: {}", e)))?;

        let mut sealed = Vec::with_capacity(12 + ciphertext.len());
        sealed.extend_from_slice(&nonce_bytes);
        sealed.extend_from_slice(&ciphertext);
        Ok(sealed)
    }

    fn local_unseal(&self, tenant_id: &str, sealed: &[u8]) -> Result<Vec<u8>> {
        if sealed.len() < 12 {
            return Err(AosError::Crypto("sealed payload too short".into()));
        }
        let (nonce_bytes, ciphertext) = sealed.split_at(12);

        let key_bytes = self.derive_local_key(tenant_id);
        let key = Key::from_slice(&key_bytes);
        let cipher = ChaCha20Poly1305::new(key);
        let nonce = Nonce::from_slice(nonce_bytes);

        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| AosError::Crypto(format!("Local decrypt failed: {}", e)))
    }

    /// Encode field as JSON for storage.
    pub fn encode(field: &EncryptedField) -> Result<String> {
        serde_json::to_string(field).map_err(AosError::Serialization)
    }

    /// Attempt to decode a stored JSON payload.
    pub fn decode(raw: &str) -> Option<EncryptedField> {
        serde_json::from_str(raw).ok()
    }
}

/// Global helper constructed lazily when enabled.
pub static CRYPTO_AT_REST: Lazy<Option<CryptoAtRest>> = Lazy::new(CryptoAtRest::from_env);

/// Non-cached helper for callers that need to honor runtime env changes.
pub fn crypto_from_env_runtime() -> Option<CryptoAtRest> {
    CryptoAtRest::from_env()
}

/// Redact PII-bearing fields before logging.
pub fn redact_for_log(value: &str) -> &'static str {
    if value.is_empty() {
        ""
    } else {
        "[redacted]"
    }
}
