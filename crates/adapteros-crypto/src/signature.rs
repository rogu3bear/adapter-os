//! Ed25519 signature operations

use adapteros_core::{AosError, Result};
use base64::Engine;
use ed25519_dalek::{Signer, Verifier};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

pub use ed25519_dalek::{
    Signature as Ed25519Signature, SigningKey, VerifyingKey as Ed25519PublicKey,
};

/// Current signature schema version
pub const SIG_SCHEMA_VERSION: u8 = 1;

/// Keypair for signing
#[derive(Clone)]
pub struct Keypair {
    signing_key: SigningKey,
}

impl Keypair {
    /// Generate a new random keypair
    pub fn generate() -> Self {
        use rand::RngCore;
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        let signing_key = SigningKey::from_bytes(&seed);
        Self { signing_key }
    }

    /// Create from seed bytes
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(bytes);
        Self { signing_key }
    }

    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> Signature {
        Signature {
            inner: self.signing_key.sign(message),
        }
    }

    /// Get the public key
    pub fn public_key(&self) -> PublicKey {
        PublicKey {
            inner: self.signing_key.verifying_key(),
        }
    }

    /// Get the secret bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }
}

/// Public key for verification
#[derive(Clone, Debug)]
pub struct PublicKey {
    inner: Ed25519PublicKey,
}

impl PublicKey {
    /// Create from bytes
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self> {
        let inner = Ed25519PublicKey::from_bytes(bytes)
            .map_err(|e| AosError::Crypto(format!("Invalid public key: {}", e)))?;
        Ok(Self { inner })
    }

    /// Create from PEM format
    pub fn from_pem(pem: &str) -> Result<Self> {
        // Parse PEM format (simplified - assumes base64 encoded key)
        let pem_content = pem
            .replace("-----BEGIN PUBLIC KEY-----", "")
            .replace("-----END PUBLIC KEY-----", "")
            .replace(['\n', '\r'], "");

        let key_bytes = base64::engine::general_purpose::STANDARD
            .decode(&pem_content)
            .map_err(|e| AosError::Crypto(format!("Invalid PEM base64: {}", e)))?;

        if key_bytes.len() != 32 {
            return Err(AosError::Crypto(format!(
                "Invalid key length: {}",
                key_bytes.len()
            )));
        }

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);
        Self::from_bytes(&key_array)
    }

    /// Verify a signature with constant-time comparison
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<()> {
        // Use constant-time verification to prevent timing attacks
        self.inner.verify(message, &signature.inner).map_err(|e| {
            tracing::warn!(
                target: "security.signing",
                key_prefix = %hex::encode(&self.inner.to_bytes()[..4]),
                "Signature verification failed"
            );
            AosError::Crypto(format!("Signature verification failed: {}", e))
        })
    }

    /// Get the raw bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&hex::encode(self.to_bytes()))
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(s).map_err(serde::de::Error::custom)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("Invalid public key length"));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        PublicKey::from_bytes(&arr).map_err(serde::de::Error::custom)
    }
}

/// Signature
#[derive(Clone, Debug)]
pub struct Signature {
    inner: Ed25519Signature,
}

impl Signature {
    /// Create from bytes
    pub fn from_bytes(bytes: &[u8; 64]) -> Result<Self> {
        let inner = Ed25519Signature::from_bytes(bytes);
        Ok(Self { inner })
    }

    /// Get the raw bytes
    pub fn to_bytes(&self) -> [u8; 64] {
        self.inner.to_bytes()
    }
}

impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&hex::encode(self.to_bytes()))
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(s).map_err(serde::de::Error::custom)?;
        if bytes.len() != 64 {
            return Err(serde::de::Error::custom("Invalid signature length"));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&bytes);
        Signature::from_bytes(&arr).map_err(serde::de::Error::custom)
    }
}

/// Sign bytes with a keypair
pub fn sign_bytes(keypair: &Keypair, message: &[u8]) -> Signature {
    keypair.sign(message)
}

/// Verify a signature
pub fn verify_signature(
    public_key: &PublicKey,
    message: &[u8],
    signature: &Signature,
) -> Result<()> {
    public_key.verify(message, signature)
}

/// Generate a new Ed25519 keypair
pub fn generate_keypair() -> (SigningKey, Ed25519PublicKey) {
    use rand::RngCore;
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();
    (signing_key, verifying_key)
}

/// Verify signature schema version
pub fn verify_signature_schema_version(schema_version: u8) -> Result<()> {
    if schema_version != SIG_SCHEMA_VERSION {
        return Err(AosError::Crypto(format!(
            "Signature schema version mismatch: expected {}, got {}",
            SIG_SCHEMA_VERSION, schema_version
        )));
    }
    Ok(())
}

/// Sign data with a hex-encoded signing key
pub fn sign_data(data: &[u8], signing_key_hex: &str) -> Result<[u8; 64]> {
    let key_bytes = hex::decode(signing_key_hex)
        .map_err(|e| AosError::Crypto(format!("Invalid hex key: {}", e)))?;

    if key_bytes.len() != 32 {
        return Err(AosError::Crypto(format!(
            "Invalid key length: {}",
            key_bytes.len()
        )));
    }

    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&key_bytes);

    let signing_key = SigningKey::from_bytes(&key_array);
    let signature: Ed25519Signature = signing_key.sign(data);

    Ok(signature.to_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_verify() {
        let keypair = Keypair::generate();
        let message = b"test message";
        let signature = keypair.sign(message);
        let public_key = keypair.public_key();
        assert!(public_key.verify(message, &signature).is_ok());
    }

    #[test]
    fn test_verify_wrong_message() {
        let keypair = Keypair::generate();
        let signature = keypair.sign(b"message1");
        let public_key = keypair.public_key();
        assert!(public_key.verify(b"message2", &signature).is_err());
    }
}
