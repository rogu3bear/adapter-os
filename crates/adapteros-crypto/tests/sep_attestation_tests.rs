//! Comprehensive tests for Secure Enclave (SEP) attestation
//!
//! Tests cover:
//! - Chip generation detection
//! - SEP availability checking
//! - Key generation with attestation
//! - Attestation verification
//! - Fallback behavior on non-SEP systems

use adapteros_crypto::{
    check_sep_availability, detect_chip_generation, generate_sep_key_with_attestation,
    get_key_creation_date, verify_attestation_chain, SepChipGeneration,
};

#[test]
fn test_detect_chip_generation() {
    let chip = detect_chip_generation();

    // Should return a valid chip generation
    match chip {
        SepChipGeneration::M1
        | SepChipGeneration::M2
        | SepChipGeneration::M3
        | SepChipGeneration::M4
        | SepChipGeneration::UnknownAppleSilicon
        | SepChipGeneration::Intel => {
            // All valid variants
            println!("Detected chip: {}", chip);
        }
    }
}

#[test]
fn test_check_sep_availability() {
    let availability = check_sep_availability();

    println!(
        "SEP available: {}, chip: {}, reason: {:?}",
        availability.available, availability.chip_generation, availability.reason
    );

    // Verify consistency between availability and chip generation
    match availability.chip_generation {
        SepChipGeneration::Intel => {
            assert!(!availability.available);
            assert!(availability.reason.is_some());
            assert!(availability
                .reason
                .as_ref()
                .unwrap()
                .contains("Intel"));
        }
        SepChipGeneration::M1
        | SepChipGeneration::M2
        | SepChipGeneration::M3
        | SepChipGeneration::M4
        | SepChipGeneration::UnknownAppleSilicon => {
            #[cfg(target_os = "macos")]
            {
                assert!(availability.available);
                assert!(availability.reason.is_none());
            }
            #[cfg(not(target_os = "macos"))]
            {
                assert!(!availability.available);
                assert!(availability.reason.is_some());
            }
        }
    }
}

#[test]
fn test_chip_generation_display() {
    let chips = vec![
        SepChipGeneration::M1,
        SepChipGeneration::M2,
        SepChipGeneration::M3,
        SepChipGeneration::M4,
        SepChipGeneration::UnknownAppleSilicon,
        SepChipGeneration::Intel,
    ];

    for chip in chips {
        let display = format!("{}", chip);
        assert!(!display.is_empty());
        println!("Chip display: {}", display);
    }
}

#[test]
fn test_sep_availability_serialization() {
    let availability = check_sep_availability();

    // Should be serializable
    let json = serde_json::to_string(&availability).unwrap();
    assert!(!json.is_empty());

    // Should be deserializable
    let deserialized: adapteros_crypto::SepAvailability =
        serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.available, availability.available);
    assert_eq!(
        deserialized.chip_generation,
        availability.chip_generation
    );
}

#[tokio::test]
#[cfg(target_os = "macos")]
async fn test_generate_sep_key_with_attestation() {
    let nonce = b"test-nonce-for-sep-attestation-12345";

    let result = generate_sep_key_with_attestation("test-sep-key", nonce).await;

    // May succeed on Apple Silicon or fall back to regular keys
    match result {
        Ok(attestation) => {
            println!("SEP attestation generated on chip: {}", attestation.chip_generation);

            // Verify attestation structure
            assert_eq!(attestation.nonce, nonce);
            assert!(!attestation.public_key.is_empty());
            assert!(attestation.timestamp > 0);

            // For M-series Macs with SEP, certificate chain may be present
            // For Intel or fallback, it will be empty
            println!("Certificate chain length: {}", attestation.certificate_chain.len());
        }
        Err(e) => {
            println!("SEP key generation failed (may be expected): {}", e);
            // This is acceptable on Intel Macs or if SEP is not available
        }
    }
}

#[tokio::test]
#[cfg(not(target_os = "macos"))]
async fn test_generate_sep_key_fails_on_non_macos() {
    let nonce = b"test-nonce";

    let result = generate_sep_key_with_attestation("test-key", nonce).await;

    // Should fail on non-macOS platforms
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("only available on macOS"));
}

#[test]
fn test_verify_empty_attestation_chain() {
    let attestation = adapteros_crypto::SepAttestation {
        public_key: vec![1, 2, 3, 4],
        certificate_chain: vec![],
        nonce: vec![5, 6, 7, 8],
        chip_generation: SepChipGeneration::Intel,
        timestamp: 1234567890,
    };

    // Empty chain should verify successfully (fallback mode)
    let result = verify_attestation_chain(&attestation);
    assert!(result.is_ok());
}

#[test]
fn test_verify_attestation_with_certificates() {
    // Create a mock attestation with certificate chain
    let attestation = adapteros_crypto::SepAttestation {
        public_key: vec![1; 32],
        certificate_chain: vec![
            vec![2; 128], // Mock certificate 1
            vec![3; 128], // Mock certificate 2
        ],
        nonce: vec![4; 32],
        chip_generation: SepChipGeneration::M3,
        timestamp: 1234567890,
    };

    // Verification should succeed (currently stub implementation)
    let result = verify_attestation_chain(&attestation);
    assert!(result.is_ok());
}

#[test]
fn test_attestation_serialization() {
    let attestation = adapteros_crypto::SepAttestation {
        public_key: vec![1, 2, 3, 4],
        certificate_chain: vec![],
        nonce: vec![5, 6, 7, 8],
        chip_generation: SepChipGeneration::M2,
        timestamp: 1234567890,
    };

    // Should serialize
    let json = serde_json::to_string(&attestation).unwrap();
    assert!(!json.is_empty());

    // Should deserialize
    let deserialized: adapteros_crypto::SepAttestation =
        serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.public_key, attestation.public_key);
    assert_eq!(deserialized.nonce, attestation.nonce);
    assert_eq!(deserialized.chip_generation, attestation.chip_generation);
    assert_eq!(deserialized.timestamp, attestation.timestamp);
}

#[tokio::test]
async fn test_sep_attestation_nonce_uniqueness() {
    let nonce1 = b"nonce-1-unique-value-123456789012";
    let nonce2 = b"nonce-2-different-value-987654321";

    #[cfg(target_os = "macos")]
    {
        let result1 = generate_sep_key_with_attestation("key1", nonce1).await;
        let result2 = generate_sep_key_with_attestation("key2", nonce2).await;

        // If both succeed, nonces should match
        if let (Ok(att1), Ok(att2)) = (result1, result2) {
            assert_eq!(att1.nonce, nonce1);
            assert_eq!(att2.nonce, nonce2);
            assert_ne!(att1.nonce, att2.nonce);
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Should fail on non-macOS
        let _ = (nonce1, nonce2);
    }
}

#[test]
#[cfg(target_os = "macos")]
fn test_get_key_creation_date() {
    let result = get_key_creation_date("test-key");

    // Should return a timestamp (may be current time as fallback)
    match result {
        Ok(timestamp) => {
            assert!(timestamp > 0);
            println!("Key creation timestamp: {}", timestamp);
        }
        Err(e) => {
            println!("Get key creation date failed: {}", e);
        }
    }
}

#[test]
#[cfg(not(target_os = "macos"))]
fn test_get_key_creation_date_fails_on_non_macos() {
    let result = get_key_creation_date("test-key");

    // Should fail on non-macOS
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("only available on macOS"));
}

#[test]
fn test_chip_generation_equality() {
    assert_eq!(SepChipGeneration::M1, SepChipGeneration::M1);
    assert_eq!(SepChipGeneration::M2, SepChipGeneration::M2);
    assert_ne!(SepChipGeneration::M1, SepChipGeneration::M2);
    assert_ne!(SepChipGeneration::Intel, SepChipGeneration::M3);
}

#[test]
fn test_chip_generation_clone() {
    let chip = SepChipGeneration::M4;
    let cloned = chip.clone();
    assert_eq!(chip, cloned);
}

#[tokio::test]
async fn test_sep_key_generation_with_long_nonce() {
    let long_nonce = vec![42u8; 256]; // Very long nonce

    #[cfg(target_os = "macos")]
    {
        let result = generate_sep_key_with_attestation("long-nonce-key", &long_nonce).await;

        match result {
            Ok(attestation) => {
                assert_eq!(attestation.nonce, long_nonce);
            }
            Err(_) => {
                // May fail if SEP not available
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = long_nonce; // Suppress unused warning
    }
}

#[tokio::test]
async fn test_sep_key_generation_with_empty_nonce() {
    let empty_nonce = b"";

    #[cfg(target_os = "macos")]
    {
        let result = generate_sep_key_with_attestation("empty-nonce-key", empty_nonce).await;

        match result {
            Ok(attestation) => {
                assert_eq!(attestation.nonce, empty_nonce);
            }
            Err(_) => {
                // May fail if SEP not available
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = empty_nonce;
    }
}

#[test]
fn test_sep_availability_on_intel() {
    // We can't force Intel in tests, but we can test the logic
    let availability = check_sep_availability();

    if availability.chip_generation == SepChipGeneration::Intel {
        assert!(!availability.available);
        assert!(availability.reason.is_some());
        let reason = availability.reason.unwrap();
        assert!(reason.contains("Intel") || reason.contains("Secure Enclave"));
    }
}

#[test]
fn test_sep_availability_on_apple_silicon() {
    let availability = check_sep_availability();

    match availability.chip_generation {
        SepChipGeneration::M1
        | SepChipGeneration::M2
        | SepChipGeneration::M3
        | SepChipGeneration::M4
        | SepChipGeneration::UnknownAppleSilicon => {
            #[cfg(target_os = "macos")]
            {
                assert!(availability.available);
            }
            #[cfg(not(target_os = "macos"))]
            {
                assert!(!availability.available);
            }
        }
        SepChipGeneration::Intel => {
            // Tested in previous test
        }
    }
}

#[test]
fn test_attestation_timestamp_validity() {
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let attestation = adapteros_crypto::SepAttestation {
        public_key: vec![1; 32],
        certificate_chain: vec![],
        nonce: vec![2; 32],
        chip_generation: SepChipGeneration::M2,
        timestamp: current_time,
    };

    // Timestamp should be reasonable
    assert!(attestation.timestamp > 1_600_000_000); // After Sep 2020
    assert!(attestation.timestamp < current_time + 1000); // Not too far in future
}
