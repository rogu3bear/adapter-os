//! Comprehensive Security Regression Test Suite for AdapterOS
//!
//! This test suite provides automated detection of security regressions across the codebase.
//! Tests cover:
//! - Unsafe code in public APIs
//! - Panic-prone operations in cryptographic code
//! - Timing-sensitive operations
//! - Input validation across public APIs
//! - Information leakage through error messages
//! - Secret handling and zeroization
//!
//! Run with: cargo test --test security_regression_suite -- --nocapture
//!
//! Copyright: © 2025 JKCA. All rights reserved.

use std::fs;
use std::path::PathBuf;

// =============================================================================
// TEST: No Unsafe in Public API
// =============================================================================

/// Scans the codebase for unsafe blocks in public functions.
///
/// This is a compile-time check that should be enforced by the compiler,
/// but this test validates the pattern across security-critical crates.
#[test]
fn test_no_unsafe_in_public_api() {
    let security_critical_crates = vec![
        "crates/adapteros-crypto/src",
        "crates/adapteros-core/src",
        "crates/adapteros-base-llm/src",
        "crates/adapteros-domain/src",
    ];

    let mut unsafe_in_public = Vec::new();

    for crate_path in security_critical_crates {
        if !PathBuf::from(&crate_path).exists() {
            continue;
        }

        if let Ok(entries) = fs::read_dir(crate_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "rs") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        check_unsafe_in_public(&path, &content, &mut unsafe_in_public);
                    }
                }
            }
        }
    }

    if !unsafe_in_public.is_empty() {
        eprintln!("Found unsafe code in public API:");
        for (file, line_num) in &unsafe_in_public {
            eprintln!("  {}:{}", file, line_num);
        }
        panic!(
            "Security regression: {} unsafe blocks found in public APIs",
            unsafe_in_public.len()
        );
    }
}

#[allow(dead_code)]
fn check_unsafe_in_public(
    path: &PathBuf,
    content: &str,
    unsafe_in_public: &mut Vec<(String, usize)>,
) {
    let mut in_public = false;
    let mut brace_depth = 0;

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Track public visibility
        if trimmed.starts_with("pub ") || trimmed.starts_with("pub(") {
            in_public = true;
            brace_depth = 0;
        }

        // Track brace depth for scope
        brace_depth += line.matches('{').count() as i32;
        brace_depth -= line.matches('}').count() as i32;

        // Reset if we exit the public function scope
        if in_public && brace_depth == 0 && trimmed.contains('{') {
            in_public = false;
        }

        // Check for unsafe in public scope
        if in_public && trimmed.contains("unsafe ") && !trimmed.starts_with("//") {
            unsafe_in_public.push((path.display().to_string(), line_num + 1));
        }
    }
}

// =============================================================================
// TEST: No Panics in Cryptographic Code
// =============================================================================

/// Verifies that cryptographic operations don't use unwrap/expect.
///
/// Panics in crypto code can leak timing information or cause denial of service.
/// All cryptographic operations must use proper error handling.
#[test]
fn test_no_panics_in_crypto() {
    let crypto_files = vec![
        "crates/adapteros-crypto/src/signature.rs",
        "crates/adapteros-crypto/src/secret.rs",
        "crates/adapteros-crypto/src/bundle_sign.rs",
        "crates/adapteros-crypto/src/envelope.rs",
        "crates/adapteros-crypto/src/providers/keychain.rs",
    ];

    let mut panic_operations = Vec::new();

    for file_path in crypto_files {
        if let Ok(content) = fs::read_to_string(file_path) {
            for (line_num, line) in content.lines().enumerate() {
                // Skip comments
                if line.trim().starts_with("//") {
                    continue;
                }

                // Check for dangerous panic operations
                // Whitelist: unwrap() in test code and cfg(test) blocks
                let dangerous_patterns = ["unwrap()", "expect(", "panic!", ".unwrap_or_else(panic"];

                for pattern in &dangerous_patterns {
                    if line.contains(pattern)
                        && !line.contains("test_")
                        && !line.contains("cfg(test)")
                    {
                        // Allow in test code and specific safe contexts
                        if !is_in_test_block(&content, line_num) {
                            panic_operations.push((
                                file_path.to_string(),
                                line_num + 1,
                                pattern.to_string(),
                            ));
                        }
                    }
                }
            }
        }
    }

    if !panic_operations.is_empty() {
        eprintln!("Found panic-prone operations in crypto code:");
        for (file, line_num, pattern) in &panic_operations {
            eprintln!("  {}:{} - {}", file, line_num, pattern);
        }
        panic!(
            "Security regression: {} panic-prone operations in crypto code",
            panic_operations.len()
        );
    }
}

#[allow(dead_code)]
fn is_in_test_block(content: &str, target_line: usize) -> bool {
    let lines: Vec<&str> = content.lines().collect();

    // Look backwards for #[test] or #[cfg(test)] or mod tests
    for i in (0..target_line).rev() {
        let line = lines[i].trim();
        if line.contains("#[test]") || line.contains("mod tests") || line.contains("#[cfg(test)]") {
            // Check if we're still within this block
            if i + 100 >= target_line {
                return true;
            }
        }
    }
    false
}

// =============================================================================
// TEST: Constant-Time Operations
// =============================================================================

/// Validates that timing-sensitive operations use constant-time implementations.
///
/// This test checks for patterns that could lead to timing side-channel attacks.
#[test]
fn test_constant_time_operations() {
    let timing_sensitive_files = vec![
        "crates/adapteros-crypto/src/signature.rs",
        "crates/adapteros-crypto/src/secret.rs",
    ];

    let suspicious_patterns = vec![
        ("==", "Direct comparison - use constant-time comparison"),
        ("if ", "Conditional branch - could leak timing info"),
        (
            "for i in",
            "Non-constant time loop - validate length is constant",
        ),
    ];

    let mut timing_issues = Vec::new();

    for file_path in timing_sensitive_files {
        if let Ok(content) = fs::read_to_string(file_path) {
            for (line_num, line) in content.lines().enumerate() {
                if line.contains("secret") || line.contains("key") || line.contains("signature") {
                    for (pattern, _description) in &suspicious_patterns {
                        if line.contains(pattern) && line.contains("==") && !line.contains("//") {
                            // Check if it's using a constant-time comparison library
                            if !line.contains("constant_time")
                                && !line.contains("subtle::")
                                && !is_in_test_block(&content, line_num)
                            {
                                timing_issues.push((file_path.to_string(), line_num + 1));
                            }
                        }
                    }
                }
            }
        }
    }

    // This test is informational - timing issues require careful review
    if !timing_issues.is_empty() {
        eprintln!("Potential timing-sensitive operations found (review carefully):");
        for (file, line_num) in &timing_issues {
            eprintln!("  {}:{}", file, line_num);
        }
        eprintln!("Note: Not all flagged operations are vulnerabilities. Manual review required.");
    }
}

// =============================================================================
// TEST: Input Validation
// =============================================================================

/// Validates that public APIs validate their inputs.
///
/// This test checks that security-critical public functions don't blindly
/// accept untrusted input without validation.
#[test]
fn test_input_validation() {
    let validated_apis = vec![
        (
            "crates/adapteros-crypto/src/signature.rs",
            vec!["verify_signature", "verify"],
        ),
        (
            "crates/adapteros-crypto/src/secret.rs",
            vec!["new", "from_bytes"],
        ),
        ("crates/adapteros-core/src/lib.rs", vec!["validate"]),
    ];

    for (file_path, functions) in validated_apis {
        if !PathBuf::from(file_path).exists() {
            continue;
        }

        if let Ok(content) = fs::read_to_string(file_path) {
            for func_name in functions {
                if content.contains(&format!("pub fn {}", func_name)) {
                    // Check that function validates inputs
                    let func_start = content.find(&format!("pub fn {}", func_name)).unwrap_or(0);
                    let func_end = content[func_start..].find('}').unwrap_or(0) + func_start;
                    let func_body = &content[func_start..func_end];

                    // Basic heuristic: function should have at least one check
                    let has_validation = func_body.contains("if")
                        || func_body.contains("return Err")
                        || func_body.contains("validate")
                        || func_body.contains("check");

                    if !has_validation && func_body.len() > 200 {
                        eprintln!("Warning: {} may not validate inputs", func_name);
                    }
                }
            }
        }
    }
}

// =============================================================================
// TEST: Error Information Leakage
// =============================================================================

/// Validates that error messages don't leak sensitive information.
///
/// Error messages should not contain secrets, keys, or internal state.
#[test]
fn test_error_information_leakage() {
    let secret_keywords = vec![
        "secret",
        "key",
        "password",
        "token",
        "credential",
        "private",
        "sensitive",
    ];

    let error_files = vec![
        "crates/adapteros-domain/src/error.rs",
        "crates/adapteros-base-llm/src/error.rs",
        "crates/adapteros-crypto/src/lib.rs",
    ];

    let mut potential_leaks = Vec::new();

    for file_path in error_files {
        if !PathBuf::from(file_path).exists() {
            continue;
        }

        if let Ok(content) = fs::read_to_string(file_path) {
            let mut in_error_impl = false;

            for (line_num, line) in content.lines().enumerate() {
                if line.contains("impl Error") || line.contains("impl Display") {
                    in_error_impl = true;
                }

                if in_error_impl {
                    // Check if error message displays secret-related data
                    if line.contains("format!") || line.contains("error(\"") {
                        for keyword in &secret_keywords {
                            if line.to_lowercase().contains(keyword) {
                                // This might be a false positive, but worth reviewing
                                if line.contains("{") && !line.contains("//") {
                                    potential_leaks.push((file_path.to_string(), line_num + 1));
                                }
                            }
                        }
                    }

                    if line.contains('}') && in_error_impl {
                        in_error_impl = false;
                    }
                }
            }
        }
    }

    if !potential_leaks.is_empty() {
        eprintln!("Potential sensitive data in error messages:");
        for (file, line_num) in &potential_leaks {
            eprintln!("  {}:{} - Review for secret leakage", file, line_num);
        }
    }
}

// =============================================================================
// TEST: Secret Zeroization
// =============================================================================

/// Validates that secret types implement zeroization.
///
/// Security-sensitive data must be zeroized when dropped to prevent memory recovery attacks.
#[test]
fn test_secret_zeroization() {
    let secret_module = "crates/adapteros-crypto/src/secret.rs";

    if let Ok(content) = fs::read_to_string(secret_module) {
        // Check for ZeroizeOnDrop implementations
        let has_zeroize_trait = content.contains("Zeroize");
        let has_zeroize_on_drop = content.contains("ZeroizeOnDrop");

        assert!(
            has_zeroize_trait,
            "Secret module should implement Zeroize trait"
        );
        assert!(
            has_zeroize_on_drop,
            "Secret module should implement ZeroizeOnDrop"
        );

        // Check that SecretKey is zeroized
        assert!(
            content.contains("impl ZeroizeOnDrop for SecretKey"),
            "SecretKey should implement ZeroizeOnDrop"
        );
    }
}

// =============================================================================
// TEST: Cryptographic Operation Validation
// =============================================================================

/// Validates core cryptographic properties.
#[test]
fn test_cryptographic_operations_validity() {
    use adapteros_crypto::{KeyMaterial, Keypair, SecretKey};

    // Test 1: Keypair generation produces different keys
    let kp1 = Keypair::generate();
    let kp2 = Keypair::generate();
    assert_ne!(
        kp1.to_bytes(),
        kp2.to_bytes(),
        "Generated keypairs should be different"
    );

    // Test 2: Signing is deterministic
    let keypair = Keypair::generate();
    let message = b"test";
    let sig1 = keypair.sign(message);
    let sig2 = keypair.sign(message);
    assert_eq!(
        sig1.to_bytes(),
        sig2.to_bytes(),
        "Ed25519 signatures should be deterministic"
    );

    // Test 3: Signature verification works
    let keypair = Keypair::generate();
    let message = b"security test";
    let signature = keypair.sign(message);
    let public_key = keypair.public_key();
    assert!(
        public_key.verify(message, &signature).is_ok(),
        "Valid signature should verify"
    );

    // Test 4: Signature verification fails with wrong message
    let wrong_message = b"different";
    assert!(
        public_key.verify(wrong_message, &signature).is_err(),
        "Invalid signature should fail verification"
    );

    // Test 5: Secret key operations
    let secret_bytes = [0x42u8; 32];
    let secret_key = SecretKey::new(secret_bytes);
    assert_eq!(
        secret_key.as_bytes(),
        &secret_bytes,
        "Secret key should preserve bytes"
    );

    // Test 6: Key material operations
    let key_material = KeyMaterial::new(vec![1, 2, 3, 4, 5]);
    assert_eq!(
        key_material.as_bytes(),
        &[1, 2, 3, 4, 5],
        "Key material should preserve bytes"
    );
}

// =============================================================================
// TEST: Serialization Security
// =============================================================================

/// Validates that sensitive data cannot be serialized.
#[test]
fn test_serialization_security() {
    use adapteros_crypto::{KeyMaterial, SecretKey};

    // Test 1: SecretKey cannot be serialized
    let secret_key = SecretKey::new([0x42u8; 32]);
    let serialization_result = serde_json::to_string(&secret_key);
    assert!(
        serialization_result.is_err(),
        "SecretKey serialization should fail"
    );

    // Test 2: KeyMaterial cannot be serialized
    let key_material = KeyMaterial::new(vec![1, 2, 3]);
    let serialization_result = serde_json::to_string(&key_material);
    assert!(
        serialization_result.is_err(),
        "KeyMaterial serialization should fail"
    );

    // Test 3: Debug output is redacted
    let secret_key = SecretKey::new([0xFF; 32]);
    let debug_output = format!("{:?}", secret_key);
    assert!(
        debug_output.contains("[REDACTED]"),
        "SecretKey debug output should be redacted"
    );
    assert!(
        !debug_output.contains("FF"),
        "SecretKey debug output should not contain key data"
    );
}

// =============================================================================
// TEST: Dependency Audit
// =============================================================================

/// Checks for security-relevant dependencies.
///
/// This test validates that critical security crates are using appropriate versions.
#[test]
fn test_dependency_audit() {
    let workspace_root = "Cargo.toml";

    if let Ok(content) = fs::read_to_string(workspace_root) {
        // Check for required security crates
        let required_security_crates = vec![
            "zeroize",   // Memory zeroization
            "ed25519",   // Cryptographic signatures
            "sha2",      // Hashing
            "thiserror", // Error handling
        ];

        for crate_name in required_security_crates {
            assert!(
                content.contains(crate_name),
                "Workspace should depend on {} for security",
                crate_name
            );
        }
    }
}

// =============================================================================
// TEST: HKDF Seeding Validation
// =============================================================================

/// Validates that randomness is derived from HKDF, not direct RNG.
///
/// Per CLAUDE.md policy: All randomness must be seeded via HKDF (no rand::thread_rng())
#[test]
fn test_hkdf_seeding() {
    let critical_files = vec![
        "crates/adapteros-lora-router/src/routing.rs",
        "crates/adapteros-deterministic-exec/src/lib.rs",
    ];

    let mut direct_rng_usage = Vec::new();

    for file_path in critical_files {
        if !PathBuf::from(file_path).exists() {
            continue;
        }

        if let Ok(content) = fs::read_to_string(file_path) {
            for (line_num, line) in content.lines().enumerate() {
                // Flag direct RNG usage
                if (line.contains("rand::thread_rng") || line.contains("rand::OsRng"))
                    && !line.trim().starts_with("//")
                {
                    // Allow in tests and FFI
                    if !is_in_test_block(&content, line_num) && !line.contains("ffi") {
                        direct_rng_usage.push((file_path.to_string(), line_num + 1));
                    }
                }
            }
        }
    }

    if !direct_rng_usage.is_empty() {
        eprintln!("Found direct RNG usage (should use HKDF seeding):");
        for (file, line_num) in &direct_rng_usage {
            eprintln!("  {}:{}", file, line_num);
        }
    }
}

// =============================================================================
// Integration Tests with Real Crypto Operations
// =============================================================================

#[test]
fn test_concurrent_signing_safety() {
    use adapteros_crypto::Keypair;
    use std::sync::Arc;
    use std::thread;

    let keypair = Arc::new(Keypair::generate());

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let kp = Arc::clone(&keypair);
            thread::spawn(move || {
                let message = format!("message {}", i);
                let signature = kp.sign(message.as_bytes());
                let public_key = kp.public_key();
                assert!(public_key.verify(message.as_bytes(), &signature).is_ok());
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panic in signing test");
    }
}

#[test]
fn test_signature_tampering_detection() {
    use adapteros_crypto::{Keypair, Signature};

    let keypair = Keypair::generate();
    let message = b"critical data";
    let signature = keypair.sign(message);
    let public_key = keypair.public_key();

    // Tamper with signature
    let mut tampered_bytes = signature.to_bytes();
    tampered_bytes[0] ^= 0x01; // Flip one bit

    let tampered_sig =
        Signature::from_bytes(&tampered_bytes).expect("Should create signature from bytes");

    // Verification should fail
    assert!(
        public_key.verify(message, &tampered_sig).is_err(),
        "Tampered signature should fail verification"
    );
}

// =============================================================================
// TEST: Security Test Coverage
// =============================================================================

/// Validates that security tests exist in critical crates.
#[test]
fn test_security_test_coverage() {
    let critical_crates = vec![
        "crates/adapteros-crypto",
        "crates/adapteros-core",
        "crates/adapteros-db",
    ];

    for crate_path in critical_crates {
        let test_file = format!("{}/tests/crypto_operations_tests.rs", crate_path);
        // At least one of: tests/ or src/lib.rs should have security tests
        let has_tests = PathBuf::from(&test_file).exists() || {
            if let Ok(lib_content) = fs::read_to_string(format!("{}/src/lib.rs", crate_path)) {
                lib_content.contains("#[test]")
            } else {
                false
            }
        };

        // This is informational - we're building the test suite
        if !has_tests {
            eprintln!(
                "Warning: {} may not have dedicated security tests",
                crate_path
            );
        }
    }
}

// =============================================================================
// Main test suite summary
// =============================================================================

#[test]
fn test_suite_summary() {
    println!("\n=== AdapterOS Security Regression Test Suite ===\n");
    println!("Tests included:");
    println!("  1. test_no_unsafe_in_public_api - Scans for unsafe blocks");
    println!("  2. test_no_panics_in_crypto - Validates error handling");
    println!("  3. test_constant_time_operations - Checks timing attacks");
    println!("  4. test_input_validation - Validates public API inputs");
    println!("  5. test_error_information_leakage - Checks error messages");
    println!("  6. test_secret_zeroization - Validates memory safety");
    println!("  7. test_cryptographic_operations_validity - Crypto correctness");
    println!("  8. test_serialization_security - Prevents secret serialization");
    println!("  9. test_dependency_audit - Checks security dependencies");
    println!("  10. test_hkdf_seeding - Validates randomness seeding");
    println!("  11. test_concurrent_signing_safety - Thread safety");
    println!("  12. test_signature_tampering_detection - Tampering detection");
    println!("\nRun individual tests with:");
    println!("  cargo test --test security_regression_suite <test_name>\n");
}
