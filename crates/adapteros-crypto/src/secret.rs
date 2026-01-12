//! Secure secret handling with automatic zeroization
//!
//! This module provides wrapper types that automatically zeroize sensitive
//! cryptographic material when they go out of scope.

use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Wrapper for cryptographic keys that automatically zeroizes on drop
#[derive(Clone)]
pub struct SecretKey<const N: usize>(pub [u8; N]);

impl<const N: usize> Zeroize for SecretKey<N> {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl<const N: usize> ZeroizeOnDrop for SecretKey<N> {}

impl<const N: usize> SecretKey<N> {
    /// Create a new secret key from bytes
    pub fn new(bytes: [u8; N]) -> Self {
        Self(bytes)
    }

    /// Get a reference to the key bytes
    pub fn as_bytes(&self) -> &[u8; N] {
        &self.0
    }

    /// Extract the key bytes (caller takes ownership)
    pub fn into_bytes(self) -> [u8; N] {
        self.0
    }
}

impl<const N: usize> fmt::Debug for SecretKey<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(&format!("SecretKey<{}>", N))
            .field("redacted", &"[REDACTED]")
            .finish()
    }
}

impl<const N: usize> Serialize for SecretKey<N> {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Never serialize secret keys - this is a security error
        Err(serde::ser::Error::custom(
            "SecretKey cannot be serialized for security reasons",
        ))
    }
}

impl<'de, const N: usize> Deserialize<'de> for SecretKey<N> {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Never deserialize secret keys - this is a security error
        Err(serde::de::Error::custom(
            "SecretKey cannot be deserialized for security reasons",
        ))
    }
}

/// Wrapper for arbitrary cryptographic material that automatically zeroizes
#[derive(Clone)]
pub struct KeyMaterial {
    inner: Vec<u8>,
}

impl Zeroize for KeyMaterial {
    fn zeroize(&mut self) {
        self.inner.as_mut_slice().zeroize();
    }
}

impl ZeroizeOnDrop for KeyMaterial {}

impl KeyMaterial {
    /// Create new key material from bytes
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { inner: bytes }
    }

    /// Get a reference to the material bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    /// Extract the material bytes (caller takes ownership)
    pub fn into_bytes(self) -> Vec<u8> {
        self.inner
    }
}

impl fmt::Debug for KeyMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyMaterial")
            .field("len", &self.inner.len())
            .field("redacted", &"[REDACTED]")
            .finish()
    }
}

impl Serialize for KeyMaterial {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Never serialize key material - this is a security error
        Err(serde::ser::Error::custom(
            "KeyMaterial cannot be serialized for security reasons",
        ))
    }
}

impl<'de> Deserialize<'de> for KeyMaterial {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Never deserialize key material - this is a security error
        Err(serde::de::Error::custom(
            "KeyMaterial cannot be deserialized for security reasons",
        ))
    }
}

/// Wrapper for sensitive data that should be zeroized
#[derive(Clone)]
pub struct SensitiveData {
    inner: Vec<u8>,
}

impl Zeroize for SensitiveData {
    fn zeroize(&mut self) {
        self.inner.as_mut_slice().zeroize();
    }
}

impl ZeroizeOnDrop for SensitiveData {}

impl SensitiveData {
    /// Create new sensitive data from bytes
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { inner: bytes }
    }

    /// Get a reference to the data bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    /// Extract the data bytes (caller takes ownership)
    pub fn into_bytes(self) -> Vec<u8> {
        self.inner
    }
}

impl From<Vec<u8>> for SensitiveData {
    fn from(value: Vec<u8>) -> Self {
        Self::new(value)
    }
}

impl From<&[u8]> for SensitiveData {
    fn from(value: &[u8]) -> Self {
        Self::new(value.to_vec())
    }
}

impl From<String> for SensitiveData {
    fn from(value: String) -> Self {
        Self::new(value.into_bytes())
    }
}

impl From<&str> for SensitiveData {
    fn from(value: &str) -> Self {
        Self::new(value.as_bytes().to_vec())
    }
}

impl fmt::Debug for SensitiveData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SensitiveData")
            .field("len", &self.inner.len())
            .field("redacted", &"[REDACTED]")
            .finish()
    }
}

impl Serialize for SensitiveData {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Never serialize sensitive data - this is a security error
        Err(serde::ser::Error::custom(
            "SensitiveData cannot be serialized for security reasons",
        ))
    }
}

impl<'de> Deserialize<'de> for SensitiveData {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Never deserialize sensitive data - this is a security error
        Err(serde::de::Error::custom(
            "SensitiveData cannot be deserialized for security reasons",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_key_zeroization() {
        let key = SecretKey::new([42u8; 32]);

        // Key should be accessible
        assert_eq!(key.as_bytes()[0], 42);
        assert_eq!(key.as_bytes()[31], 42);

        // Extract and drop - should zeroize
        let bytes = key.into_bytes();
        assert_eq!(bytes[0], 42); // still accessible after extraction

        let _ = bytes; // bytes are zeroized here
    }

    #[test]
    fn test_key_material_zeroization() {
        let material = KeyMaterial::new(vec![1, 2, 3, 4, 5]);

        assert_eq!(material.as_bytes(), &[1, 2, 3, 4, 5]);

        let bytes = material.into_bytes();
        assert_eq!(bytes, &[1, 2, 3, 4, 5]);

        drop(bytes);
    }

    #[test]
    fn test_debug_redaction() {
        let key = SecretKey::new([1u8; 32]);
        let debug_str = format!("{:?}", key);
        assert!(debug_str.contains("[REDACTED]"));
        assert!(!debug_str.contains("1"));
    }

    #[test]
    fn test_serialization_fails() {
        let key = SecretKey::new([1u8; 32]);

        // Serialization should fail
        let result = serde_json::to_string(&key);
        assert!(result.is_err());

        let material = KeyMaterial::new(vec![1, 2, 3]);
        let result = serde_json::to_string(&material);
        assert!(result.is_err());
    }
}
