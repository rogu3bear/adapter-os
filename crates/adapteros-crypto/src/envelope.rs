//! Envelope encryption for artifacts using AES-GCM

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use adapteros_core::{AosError, Result};
use rand::RngCore;

const NONCE_SIZE: usize = 12;

/// Encrypt data with AES-256-GCM
///
/// Returns (ciphertext, nonce) tuple
pub fn encrypt_envelope(key: &[u8; 32], plaintext: &[u8]) -> Result<(Vec<u8>, [u8; NONCE_SIZE])> {
    let cipher = Aes256Gcm::new(key.into());

    // Generate random nonce
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| AosError::Crypto(format!("Encryption failed: {}", e)))?;

    Ok((ciphertext, nonce_bytes))
}

/// Decrypt data with AES-256-GCM
pub fn decrypt_envelope(
    key: &[u8; 32],
    ciphertext: &[u8],
    nonce: &[u8; NONCE_SIZE],
) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(key.into());
    let nonce = Nonce::from_slice(nonce);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| AosError::Crypto(format!("Decryption failed: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let key = [42u8; 32];
        let plaintext = b"secret data";

        let (ciphertext, nonce) =
            encrypt_envelope(&key, plaintext).expect("Test encryption should succeed");
        assert_ne!(ciphertext.as_slice(), plaintext);

        let decrypted =
            decrypt_envelope(&key, &ciphertext, &nonce).expect("Test decryption should succeed");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_wrong_key() {
        let key1 = [42u8; 32];
        let key2 = [43u8; 32];
        let plaintext = b"secret data";

        let (ciphertext, nonce) =
            encrypt_envelope(&key1, plaintext).expect("Test encryption should succeed");
        assert!(decrypt_envelope(&key2, &ciphertext, &nonce).is_err());
    }
}
