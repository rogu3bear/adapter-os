//! Federation Adversarial Tests
//!
//! Tests for federation and signature-related functionality to ensure
//! adversarial test coverage as required by patent gates.

use adapteros_core::{AosError, B3Hash};
use adapteros_crypto::{KeyPair, SignatureScheme};
use adapteros_federation::{FederationConfig, FederationManager};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_federation_malformed_config() {
        // Test federation configuration with malformed inputs
        let invalid_configs = vec![
            FederationConfig {
                peer_hosts: vec!["".to_string()], // Empty host
                ..Default::default()
            },
            FederationConfig {
                peer_hosts: vec!["invalid..host..name".to_string()], // Invalid hostname
                ..Default::default()
            },
            FederationConfig {
                peer_hosts: vec!["192.168.1.256".to_string()], // Invalid IP
                ..Default::default()
            },
        ];

        for config in invalid_configs {
            let result = FederationManager::new(config).await;
            assert!(result.is_err(), "Should reject malformed federation config");
        }
    }

    #[tokio::test]
    async fn test_signature_verification_adversarial() {
        // Test signature verification with adversarial inputs
        let keypair = KeyPair::generate_ed25519().unwrap();
        let message = b"test message";
        let signature = keypair.sign(message).unwrap();

        // Valid verification should work
        assert!(keypair.public_key().verify(message, &signature).is_ok());

        // Adversarial cases
        let adversarial_cases = vec![
            (b"", &signature),                  // Empty message
            (message, &[]),                     // Empty signature
            (b"different message", &signature), // Wrong message
        ];

        for (msg, sig) in adversarial_cases {
            assert!(
                keypair.public_key().verify(msg, sig).is_err(),
                "Should reject adversarial signature verification"
            );
        }
    }

    #[tokio::test]
    async fn test_federation_replay_attack_prevention() {
        // Test prevention of replay attacks in federation
        let config = FederationConfig {
            peer_hosts: vec!["localhost:8081".to_string()],
            enable_signature_verification: true,
            ..Default::default()
        };

        let manager = FederationManager::new(config).await.unwrap();

        // Simulate replay attack - same message twice
        let message = b"replay test";
        let hash = B3Hash::hash(message);

        // First time should work
        assert!(manager
            .verify_message_integrity(&hash, message)
            .await
            .is_ok());

        // Second time should be rejected (replay protection)
        assert!(manager
            .verify_message_integrity(&hash, message)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_federation_man_in_middle_protection() {
        // Test protection against man-in-the-middle attacks
        let config = FederationConfig {
            peer_hosts: vec!["localhost:8082".to_string()],
            enable_signature_verification: true,
            require_certificate_pinning: true,
            ..Default::default()
        };

        let manager = FederationManager::new(config).await.unwrap();

        // Test with invalid certificate
        let invalid_cert = b"invalid certificate data";
        assert!(manager
            .validate_peer_certificate(invalid_cert)
            .await
            .is_err());

        // Test with tampered certificate
        let valid_cert = manager.get_expected_certificate().await.unwrap();
        let mut tampered_cert = valid_cert.clone();
        tampered_cert[0] = tampered_cert[0].wrapping_add(1); // Tamper with cert

        assert!(manager
            .validate_peer_certificate(&tampered_cert)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_federation_dos_protection() {
        // Test protection against denial-of-service attacks
        let config = FederationConfig {
            peer_hosts: vec!["localhost:8083".to_string()],
            max_concurrent_connections: 10,
            connection_timeout_ms: 5000,
            ..Default::default()
        };

        let manager = FederationManager::new(config).await.unwrap();

        // Test connection limit enforcement
        let mut handles = vec![];

        // Try to exceed connection limit
        for i in 0..15 {
            let manager_clone = manager.clone();
            let handle =
                tokio::spawn(async move { manager_clone.establish_peer_connection(i).await });
            handles.push(handle);
        }

        let mut success_count = 0;
        let mut failure_count = 0;

        for handle in handles {
            match handle.await.unwrap() {
                Ok(_) => success_count += 1,
                Err(_) => failure_count += 1,
            }
        }

        // Should have exactly max_concurrent_connections successes
        assert_eq!(success_count, 10);
        assert_eq!(failure_count, 5);
    }

    #[tokio::test]
    async fn test_federation_message_tampering_detection() {
        // Test detection of message tampering during federation
        let config = FederationConfig {
            peer_hosts: vec!["localhost:8084".to_string()],
            enable_signature_verification: true,
            ..Default::default()
        };

        let manager = FederationManager::new(config).await.unwrap();

        let original_message = b"original federation message";
        let original_hash = B3Hash::hash(original_message);

        // Valid message should verify
        assert!(manager
            .verify_message_integrity(&original_hash, original_message)
            .await
            .is_ok());

        // Tampered message should be rejected
        let tampered_message = b"tampered federation message";
        assert!(manager
            .verify_message_integrity(&original_hash, tampered_message)
            .await
            .is_err());

        // Tampered hash should be rejected
        let wrong_hash = B3Hash::hash(b"wrong message");
        assert!(manager
            .verify_message_integrity(&wrong_hash, original_message)
            .await
            .is_err());
    }
}
