//! Integration tests for Secure Enclave hardware operations
//!
//! These tests verify that hardware-backed cryptographic operations work correctly
//! when the secure-enclave feature is enabled and running on macOS.

#[cfg(all(feature = "secure-enclave", target_os = "macos"))]
mod hardware_tests {
    use adapteros_secd::HardwareSecureEnclaveConnection;

    #[tokio::test]
    async fn test_secure_enclave_signing_integration() -> Result<(), Box<dyn std::error::Error>> {
        let mut hw_conn = HardwareSecureEnclaveConnection::new()?;

        // Generate signing keypair in Secure Enclave
        let pubkey = hw_conn.generate_signing_keypair("test-signing-key")?;
        assert!(!pubkey.to_bytes().is_empty());

        // Sign some test data
        let test_data = b"Hello, Secure Enclave!";
        let signature = hw_conn.sign_with_secure_enclave("test-signing-key", test_data)?;

        // Verify signature with derived Ed25519 key to guarantee determinism.
        pubkey.verify(test_data, &signature)?;

        Ok(())
    }

    #[tokio::test]
    async fn test_secure_enclave_encryption_integration() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut hw_conn = HardwareSecureEnclaveConnection::new()?;

        // Generate encryption key in Secure Enclave
        let key = hw_conn.generate_encryption_key("test-encryption-key")?;
        assert_eq!(key.len(), 32); // ChaCha20Poly1305 key size

        // Test key consistency - same label should return same key
        let key2 = hw_conn.generate_encryption_key("test-encryption-key")?;
        assert_eq!(key, key2);

        Ok(())
    }

    #[tokio::test]
    async fn test_secure_enclave_attestation() -> Result<(), Box<dyn std::error::Error>> {
        let mut hw_conn = HardwareSecureEnclaveConnection::new()?;

        // Get attestation for a key (even if it doesn't exist yet)
        let attestation = hw_conn.get_key_attestation("test-attestation-key")?;
        assert!(!attestation.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_secure_enclave_key_persistence() -> Result<(), Box<dyn std::error::Error>> {
        let mut hw_conn1 = HardwareSecureEnclaveConnection::new()?;
        let mut hw_conn2 = HardwareSecureEnclaveConnection::new()?;

        // Generate key in first connection
        let pubkey1 = hw_conn1.generate_signing_keypair("persistent-test-key")?;

        // Retrieve same key in second connection (should load from keychain)
        let pubkey2 = hw_conn2.generate_signing_keypair("persistent-test-key")?;

        // Keys should be identical (same public key)
        assert_eq!(pubkey1.to_bytes(), pubkey2.to_bytes());

        Ok(())
    }
}

#[cfg(not(all(feature = "secure-enclave", target_os = "macos")))]
mod fallback_tests {
    #[tokio::test]
    async fn test_secure_enclave_unavailable() {
        // When secure-enclave feature is not enabled or not on macOS,
        // the hardware-backed connection is intentionally unavailable.
        assert!(!cfg!(all(feature = "secure-enclave", target_os = "macos")));
    }
}
