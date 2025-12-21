//! Property-based security tests for cryptographic operations
//!
//! This test suite uses proptest to verify security properties across
//! a broad range of inputs. Each property test is designed to ensure
//! fundamental cryptographic invariants hold universally.
//!
//! Run with multiple seeds:
//!   PROPTEST_CASES=10000 cargo test --test property_security

use adapteros_crypto::{
    decrypt_envelope, encrypt_envelope, sign_bytes, verify_signature, Keypair, PublicKey, Signature,
};
use proptest::{prop_assert, prop_assert_eq, prop_assert_ne, proptest};
use std::sync::Arc;
use std::sync::Mutex;

// ============================================================================
// Property: Encryption-Decryption Roundtrip
// ============================================================================

#[test]
fn prop_encryption_decryption_roundtrip() {
    proptest!(|(
        key: [u8; 32],
        plaintext: Vec<u8>,
    )| {
        // Encrypt the plaintext
        let (ciphertext, nonce) = encrypt_envelope(&key, &plaintext)
            .expect("Encryption should always succeed");

        // Ciphertext should not be equal to plaintext
        prop_assert_ne!(ciphertext.as_slice(), plaintext.as_slice());

        // Decrypt and verify roundtrip
        let decrypted = decrypt_envelope(&key, &ciphertext, &nonce)
            .expect("Decryption with correct key should succeed");

        prop_assert_eq!(decrypted, plaintext, "Decrypted data must match original plaintext");
    });
}

#[test]
fn prop_encryption_randomness() {
    proptest!(|(
        key: [u8; 32],
        plaintext: Vec<u8>,
    )| {
        // Encrypt twice with same key
        let (ct1, nonce1) = encrypt_envelope(&key, &plaintext)
            .expect("First encryption should succeed");
        let (ct2, nonce2) = encrypt_envelope(&key, &plaintext)
            .expect("Second encryption should succeed");

        // Nonces must be different (astronomical probability of collision)
        prop_assert_ne!(nonce1, nonce2, "Nonces should be unique");

        // Ciphertexts must be different due to different nonces
        prop_assert_ne!(ct1, ct2, "Ciphertexts should be different with different nonces");
    });
}

#[test]
fn prop_wrong_key_decryption_fails() {
    proptest!(|(
        key1: [u8; 32],
        mut key2: [u8; 32],
        plaintext: Vec<u8>,
    )| {
        // Ensure keys are different
        if key1 == key2 {
            key2[0] ^= 0xFF;
        }

        let (ciphertext, nonce) = encrypt_envelope(&key1, &plaintext)
            .expect("Encryption should succeed");

        // Decryption with wrong key must fail
        let result = decrypt_envelope(&key2, &ciphertext, &nonce);
        prop_assert!(result.is_err(), "Decryption with wrong key must fail");
    });
}

#[test]
fn prop_corrupted_ciphertext_fails() {
    proptest!(|(
        key: [u8; 32],
        plaintext: Vec<u8>,
        bit_flip_index: usize,
    )| {
        if !plaintext.is_empty() {
            let (mut ciphertext, nonce) = encrypt_envelope(&key, &plaintext)
                .expect("Encryption should succeed");

            // Corrupt ciphertext if it's large enough
            if bit_flip_index < ciphertext.len() {
                ciphertext[bit_flip_index] ^= 0x01; // Flip one bit

                // Decryption must fail
                let result = decrypt_envelope(&key, &ciphertext, &nonce);
                prop_assert!(result.is_err(), "Decryption of tampered ciphertext must fail");
            }
        }
    });
}

#[test]
fn prop_empty_plaintext_roundtrip() {
    proptest!(|(
        key: [u8; 32],
    )| {
        let plaintext = vec![];
        let (ciphertext, nonce) = encrypt_envelope(&key, &plaintext)
            .expect("Encryption of empty plaintext should succeed");

        let decrypted = decrypt_envelope(&key, &ciphertext, &nonce)
            .expect("Decryption should succeed");

        prop_assert_eq!(decrypted, plaintext);
    });
}

// ============================================================================
// Property: Signature Verification
// ============================================================================

#[test]
fn prop_valid_signature_always_verifies() {
    proptest!(|(
        message: Vec<u8>,
    )| {
        let keypair = Keypair::generate();
        let public_key = keypair.public_key();
        let signature = keypair.sign(&message);

        let result = verify_signature(&public_key, &message, &signature);
        prop_assert!(result.is_ok(), "Valid signature must verify");
    });
}

#[test]
fn prop_invalid_signature_never_verifies() {
    proptest!(|(
        message: Vec<u8>,
    )| {
        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();
        let public_key2 = keypair2.public_key();

        let signature = keypair1.sign(&message);

        let result = verify_signature(&public_key2, &message, &signature);
        prop_assert!(result.is_err(), "Signature from different key must not verify");
    });
}

#[test]
fn prop_modified_message_fails_verification() {
    proptest!(|(
        mut message: Vec<u8>,
        bit_index: usize,
    )| {
        if !message.is_empty() {
            let keypair = Keypair::generate();
            let public_key = keypair.public_key();
            let signature = keypair.sign(&message);

            // Modify message if index is within bounds
            if bit_index < message.len() {
                message[bit_index] ^= 0x01; // Flip one bit

                let result = verify_signature(&public_key, &message, &signature);
                prop_assert!(result.is_err(), "Modified message must fail verification");
            }
        }
    });
}

#[test]
fn prop_modified_signature_fails_verification() {
    proptest!(|(
        message: Vec<u8>,
        bit_index: u8,
    )| {
        let keypair = Keypair::generate();
        let public_key = keypair.public_key();
        let signature = keypair.sign(&message);

        // Corrupt the signature
        let mut sig_bytes = signature.to_bytes();
        let idx = (bit_index as usize) % 64;
        sig_bytes[idx] ^= 0x01; // Flip one bit

        let corrupted_sig = Signature::from_bytes(&sig_bytes)
            .expect("Should create signature from bytes");

        let result = verify_signature(&public_key, &message, &corrupted_sig);
        prop_assert!(result.is_err(), "Corrupted signature must fail verification");
    });
}

#[test]
fn prop_empty_message_signing() {
    let keypair = Keypair::generate();
    let public_key = keypair.public_key();
    let message = vec![];

    let signature = keypair.sign(&message);
    let result = verify_signature(&public_key, &message, &signature);

    assert!(result.is_ok(), "Empty message signing must work");
}

#[test]
fn prop_signature_determinism() {
    proptest!(|(
        seed: [u8; 32],
        message: Vec<u8>,
    )| {
        let keypair1 = Keypair::from_bytes(&seed);
        let keypair2 = Keypair::from_bytes(&seed);

        let sig1 = keypair1.sign(&message);
        let sig2 = keypair2.sign(&message);

        // Same key must produce same signature
        prop_assert_eq!(sig1.to_bytes(), sig2.to_bytes());

        // Both must verify with public key
        let public_key = keypair1.public_key();
        prop_assert!(public_key.verify(&message, &sig1).is_ok());
        prop_assert!(public_key.verify(&message, &sig2).is_ok());
    });
}

// ============================================================================
// Property: Key Generation Uniqueness
// ============================================================================

#[test]
fn prop_key_generation_uniqueness() {
    proptest!(|(
        iterations in 10..100usize,
    )| {
        let mut keys_seen = std::collections::HashSet::new();

        for _ in 0..iterations {
            let keypair = Keypair::generate();
            let key_bytes = keypair.to_bytes();
            let key_hex = hex::encode(key_bytes);

            prop_assert!(
                !keys_seen.contains(&key_hex),
                "All generated keys must be unique"
            );
            keys_seen.insert(key_hex);
        }
    });
}

#[test]
fn prop_different_seeds_different_keys() {
    proptest!(|(
        mut seed1: [u8; 32],
        mut seed2: [u8; 32],
    )| {
        // Ensure seeds are different
        if seed1 == seed2 {
            seed2[0] ^= 0xFF;
        }

        let keypair1 = Keypair::from_bytes(&seed1);
        let keypair2 = Keypair::from_bytes(&seed2);

        // Keys must be different
        prop_assert_ne!(
            keypair1.to_bytes(),
            keypair2.to_bytes(),
            "Different seeds must produce different keys"
        );

        // Public keys must be different
        let pk1 = keypair1.public_key();
        let pk2 = keypair2.public_key();
        prop_assert_ne!(pk1.to_bytes(), pk2.to_bytes());
    });
}

#[test]
fn prop_public_key_derivation_determinism() {
    proptest!(|(
        seed: [u8; 32],
    )| {
        let keypair1 = Keypair::from_bytes(&seed);
        let keypair2 = Keypair::from_bytes(&seed);

        let pk1 = keypair1.public_key();
        let pk2 = keypair2.public_key();

        prop_assert_eq!(pk1.to_bytes(), pk2.to_bytes());
    });
}

#[test]
fn prop_public_key_reconstruction() {
    proptest!(|(
        seed: [u8; 32],
        message: Vec<u8>,
    )| {
        let keypair = Keypair::from_bytes(&seed);
        let pk1 = keypair.public_key();
        let pk_bytes = pk1.to_bytes();

        // Reconstruct public key from bytes
        let pk2 = PublicKey::from_bytes(&pk_bytes)
            .expect("Should reconstruct public key");

        // Both should verify the same signature
        let signature = keypair.sign(&message);
        let result1 = pk1.verify(&message, &signature);
        let result2 = pk2.verify(&message, &signature);

        prop_assert!(result1.is_ok() && result2.is_ok());
    });
}

// ============================================================================
// Property: Error Handling Completeness
// ============================================================================

#[test]
fn prop_encryption_no_panics() {
    proptest!(|(
        key: [u8; 32],
        plaintext: Vec<u8>,
    )| {
        // Should not panic even with arbitrary inputs
        let result = encrypt_envelope(&key, &plaintext);
        prop_assert!(result.is_ok() || result.is_err());
    });
}

#[test]
fn prop_decryption_invalid_input() {
    proptest!(|(
        key: [u8; 32],
        ciphertext: Vec<u8>,
    )| {
        // Use a valid nonce to establish baseline
        let valid_nonce = [0u8; 12];

        // Try decryption - should not panic regardless of ciphertext
        let result = decrypt_envelope(&key, &ciphertext, &valid_nonce);
        prop_assert!(result.is_ok() || result.is_err());
    });
}

#[test]
fn prop_invalid_public_key_bytes() {
    proptest!(|(
        bytes: [u8; 32],
    )| {
        // Try to create public key from arbitrary bytes
        // Some may succeed, some may fail, but none should panic
        let result = PublicKey::from_bytes(&bytes);
        prop_assert!(result.is_ok() || result.is_err());
    });
}

#[test]
fn prop_invalid_signature_bytes() {
    proptest!(|(
        bytes: [u8; 64],
    )| {
        // Try to create signature from arbitrary bytes
        // Should not panic
        let result = Signature::from_bytes(&bytes);
        prop_assert!(result.is_ok() || result.is_err());
    });
}

#[test]
fn prop_verification_no_panics() {
    proptest!(|(
        seed: [u8; 32],
        message: Vec<u8>,
        sig_bytes: [u8; 64],
    )| {
        let keypair = Keypair::from_bytes(&seed);
        let public_key = keypair.public_key();

        // Create signature from arbitrary bytes
        if let Ok(signature) = Signature::from_bytes(&sig_bytes) {
            // This should not panic
            let _ = verify_signature(&public_key, &message, &signature);
        }
    });
}

// ============================================================================
// Property: Thread Safety & Concurrent Operations
// ============================================================================

#[test]
fn prop_concurrent_signing_consistency() {
    proptest!(|(
        seed: [u8; 32],
        num_threads in 2..8usize,
        messages_per_thread in 2..10usize,
    )| {
        use std::thread;

        let keypair = Arc::new(Keypair::from_bytes(&seed));
        let results = Arc::new(Mutex::new(Vec::new()));

        let handles: Vec<_> = (0..num_threads)
            .map(|thread_id| {
                let kp = Arc::clone(&keypair);
                let res = Arc::clone(&results);

                thread::spawn(move || {
                    for msg_id in 0..messages_per_thread {
                        let message = format!("t{}m{}", thread_id, msg_id).into_bytes();
                        let signature = kp.sign(&message);
                        res.lock().unwrap().push((message, signature));
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("Thread should not panic");
        }

        // Verify all signatures are correct
        let public_key = keypair.public_key();
        let collected = results.lock().unwrap();

        for (message, signature) in collected.iter() {
            let verify_result = verify_signature(&public_key, message, signature);
            prop_assert!(verify_result.is_ok(), "Concurrent signature must verify");
        }
    });
}

#[test]
fn prop_concurrent_encryption_decryption() {
    proptest!(|(
        key: [u8; 32],
        num_threads in 2..6usize,
        iterations in 2..5usize,
    )| {
        use std::thread;

        let key = Arc::new(key);
        let results = Arc::new(Mutex::new(Vec::new()));
        let errors = Arc::new(Mutex::new(Vec::new()));

        let handles: Vec<_> = (0..num_threads)
            .map(|thread_id| {
                let k = Arc::clone(&key);
                let res = Arc::clone(&results);
                let errs = Arc::clone(&errors);

                thread::spawn(move || {
                    for iter in 0..iterations {
                        let plaintext = format!("t{}i{}", thread_id, iter).into_bytes();
                        match encrypt_envelope(&k, &plaintext) {
                            Ok((ciphertext, nonce)) => {
                                match decrypt_envelope(&k, &ciphertext, &nonce) {
                                    Ok(decrypted) => {
                                        if decrypted != plaintext {
                                            errs.lock().unwrap().push((
                                                thread_id,
                                                "Roundtrip mismatch".to_string(),
                                            ));
                                        } else {
                                            res.lock().unwrap().push((thread_id, iter));
                                        }
                                    }
                                    Err(e) => {
                                        errs.lock()
                                            .unwrap()
                                            .push((thread_id, format!("Decrypt error: {}", e)));
                                    }
                                }
                            }
                            Err(e) => {
                                errs.lock()
                                    .unwrap()
                                    .push((thread_id, format!("Encrypt error: {}", e)));
                            }
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("Thread should not panic");
        }

        let errors = errors.lock().unwrap();
        prop_assert!(errors.is_empty(), "No concurrent encryption/decryption errors: {:?}", errors);

        let results = results.lock().unwrap();
        prop_assert_eq!(results.len(), num_threads * iterations);
    });
}

#[test]
fn prop_concurrent_key_generation_uniqueness() {
    proptest!(|(
        num_threads in 2..6usize,
    )| {
        use std::thread;

        let keys = Arc::new(Mutex::new(Vec::new()));

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let k = Arc::clone(&keys);
                thread::spawn(move || {
                    let keypair = Keypair::generate();
                    k.lock().unwrap().push(keypair.to_bytes());
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("Thread should not panic");
        }

        let collected = keys.lock().unwrap();
        let mut seen = std::collections::HashSet::new();

        for key_bytes in collected.iter() {
            let hex_key = hex::encode(key_bytes);
            prop_assert!(!seen.contains(&hex_key), "All keys must be unique");
            seen.insert(hex_key);
        }
    });
}

// ============================================================================
// Property: Size and Bounds Checks
// ============================================================================

#[test]
fn prop_signature_always_64_bytes() {
    proptest!(|(
        message: Vec<u8>,
    )| {
        let keypair = Keypair::generate();
        let signature = keypair.sign(&message);

        prop_assert_eq!(signature.to_bytes().len(), 64);
    });
}

#[test]
fn prop_public_key_always_32_bytes() {
    let keypair = Keypair::generate();
    let public_key = keypair.public_key();

    assert_eq!(public_key.to_bytes().len(), 32);
}

#[test]
fn prop_ciphertext_longer_than_plaintext() {
    proptest!(|(
        key: [u8; 32],
        plaintext: Vec<u8>,
    )| {
        if !plaintext.is_empty() {
            let (ciphertext, _) = encrypt_envelope(&key, &plaintext)
                .expect("Encryption should succeed");

            // Ciphertext should include 16-byte authentication tag
            prop_assert!(ciphertext.len() >= plaintext.len() + 16,
                "Ciphertext must be at least 16 bytes longer (for auth tag)");
        }
    });
}

#[test]
fn prop_empty_plaintext_produces_auth_tag() {
    proptest!(|(
        key: [u8; 32],
    )| {
        let plaintext = vec![];
        let (ciphertext, _) = encrypt_envelope(&key, &plaintext)
            .expect("Encryption should succeed");

        // Even for empty plaintext, we get the authentication tag
        prop_assert_eq!(ciphertext.len(), 16,
            "Empty plaintext still produces 16-byte auth tag");
    });
}

// ============================================================================
// Integration Property Tests
// ============================================================================

#[test]
fn prop_sign_and_verify_integration() {
    proptest!(|(
        seed: [u8; 32],
        message: Vec<u8>,
    )| {
        let keypair = Keypair::from_bytes(&seed);
        let public_key = keypair.public_key();

        // Use the convenience functions
        let signature = sign_bytes(&keypair, &message);
        let verify_result = verify_signature(&public_key, &message, &signature);

        prop_assert!(verify_result.is_ok());
    });
}

#[test]
fn prop_encrypt_decrypt_with_different_keys() {
    proptest!(|(
        key1: [u8; 32],
        mut key2: [u8; 32],
        mut key3: [u8; 32],
        plaintext: Vec<u8>,
    )| {
        // Ensure all keys are different
        if key1 == key2 {
            key2[0] ^= 0xFF;
        }
        if key1 == key3 || key2 == key3 {
            key3[0] ^= 0xAA;
        }

        let (ciphertext, nonce) = encrypt_envelope(&key1, &plaintext)
            .expect("Encryption should succeed");

        // Decryption with key1 should succeed
        let decrypt_key1 = decrypt_envelope(&key1, &ciphertext, &nonce);
        prop_assert!(decrypt_key1.is_ok());

        // Decryption with key2 should fail
        let decrypt_key2 = decrypt_envelope(&key2, &ciphertext, &nonce);
        prop_assert!(decrypt_key2.is_err());

        // Decryption with key3 should fail
        let decrypt_key3 = decrypt_envelope(&key3, &ciphertext, &nonce);
        prop_assert!(decrypt_key3.is_err());
    });
}
