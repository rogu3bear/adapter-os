# Security Regression Testing Guide

**Copyright:** © 2025 JKCA. All rights reserved.

**Last Updated:** 2025-11-22

**Purpose:** Comprehensive guide to the security regression test suite, which automatically detects security regressions across the AdapterOS codebase.

---

## Overview

The security regression test suite provides automated detection of common security vulnerabilities and deviations from security best practices. Tests run on every PR to prevent regressions in security-critical components.

**Key Principles:**
- Fail fast on security issues
- Automated detection of common patterns
- Manual review gates for complex cases
- Continuous monitoring of dependencies
- Clear remediation guidance

---

## Test Categories

### 1. Unsafe Code Detection (`test_no_unsafe_in_public_api`)

**Purpose:** Prevent unsafe blocks in public APIs where Rust's memory safety guarantees are required.

**Scope:** Scans security-critical crates:
- `adapteros-crypto/src` - Cryptographic operations
- `adapteros-core/src` - Core types and error handling
- `adapteros-base-llm/src` - LLM base functionality
- `adapteros-domain/src` - Domain adapter layer

**Remediation:**
```rust
// ❌ BEFORE: Unsafe in public API
pub fn verify_signature(key: &PublicKey, msg: &[u8], sig: &Signature) -> Result<()> {
    unsafe { validate_pointer(key as *const _) }
    // ...
}

// ✅ AFTER: Safe wrapper around FFI boundary
pub fn verify_signature(key: &PublicKey, msg: &[u8], sig: &Signature) -> Result<()> {
    // Validation is safe - FFI is internal only
    validate_internal(key)?
    // ...
}
```

**Exception:** Unsafe code is allowed in:
- FFI crates (properly documented and reviewed)
- Contained within private functions with safety documentation
- With explicit safety comments explaining the invariants

---

### 2. Panic-Free Cryptography (`test_no_panics_in_crypto`)

**Purpose:** Prevent panics in cryptographic code that could be exploited for denial of service or leak timing information.

**Scope:** Security-critical files:
- `adapteros-crypto/src/signature.rs`
- `adapteros-crypto/src/secret.rs`
- `adapteros-crypto/src/bundle_sign.rs`
- `adapteros-crypto/src/envelope.rs`
- `adapteros-crypto/src/providers/keychain.rs`

**Prohibited Patterns:**
```rust
// ❌ FORBIDDEN in crypto code
signature.as_bytes().unwrap()           // May panic
secret_key.get(0).expect("len >= 32")   // May panic
result.unwrap_or_else(|_| panic!())     // Explicit panic
```

**Correct Pattern:**
```rust
// ✅ CORRECT error handling
match signature.as_bytes() {
    Some(bytes) => Ok(bytes),
    None => Err(AosError::Crypto("Invalid signature".into())),
}

signature.as_bytes()
    .ok_or_else(|| AosError::Crypto("Missing signature".into()))
```

**Rationale:**
- **DoS Prevention:** Panics can be triggered by adversarial input
- **Timing Leaks:** Error paths may have different timing
- **Stability:** Production systems must not crash from invalid input

---

### 3. Constant-Time Operations (`test_constant_time_operations`)

**Purpose:** Prevent timing side-channel attacks through careful timing analysis.

**Scope:** Checks for suspicious patterns in timing-sensitive code:
- Direct comparisons (==) with secrets
- Conditional branches on secret data
- Non-constant-time loops in cryptographic paths

**Timing Attack Example:**
```rust
// ❌ VULNERABLE: Timing leak via early exit
fn compare_secret(input: &[u8], secret: &[u8]) -> bool {
    if input.len() != secret.len() {
        return false;  // Early exit - timing leaked
    }
    input == secret    // Also vulnerable - stops on first mismatch
}

// ✅ SECURE: Constant-time comparison
fn compare_secret(input: &[u8], secret: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    input.ct_eq(secret).into()  // Constant time regardless of input
}
```

**Verification Tools:**
- Use `subtle::` crate for constant-time comparisons
- Use `zeroize::` crate for memory operations
- Manual timing analysis for complex code

---

### 4. Input Validation (`test_input_validation`)

**Purpose:** Ensure public APIs validate untrusted input before use.

**Scope:** Public functions in:
- `adapteros-crypto/src/signature.rs`
- `adapteros-crypto/src/secret.rs`
- `adapteros-core/src/lib.rs`

**Validation Pattern:**
```rust
// ✅ CORRECT: Input validation
pub fn verify_signature(key: &PublicKey, msg: &[u8], sig: &Signature) -> Result<()> {
    // Validate key integrity
    if key.len() != PUBLIC_KEY_SIZE {
        return Err(AosError::Validation("Invalid key size".into()));
    }

    // Validate message constraints
    if msg.len() > MAX_MESSAGE_SIZE {
        return Err(AosError::Validation("Message too large".into()));
    }

    // Validate signature format
    if sig.len() != SIGNATURE_SIZE {
        return Err(AosError::Validation("Invalid signature size".into()));
    }

    // Now safe to proceed with verification
    verify_internal(key, msg, sig)
}
```

**Common Checks:**
- Size constraints
- Format validation
- Type invariants
- Range validation
- Null pointer checks (if applicable)

---

### 5. Error Information Leakage (`test_error_information_leakage`)

**Purpose:** Prevent sensitive information leakage through error messages.

**Scope:** Error implementations in:
- `adapteros-domain/src/error.rs`
- `adapteros-base-llm/src/error.rs`
- `adapteros-crypto/src/lib.rs`

**Anti-Patterns:**
```rust
// ❌ LEAKED: Sensitive data in error messages
#[error("Failed to verify signature with key {key:?}")]
SignatureFailed { key: SecretKey<32> },

#[error("Decryption failed: {plaintext}")]
DecryptionFailed { plaintext: Vec<u8> },

// ❌ LEAKED: Including secret in error context
match decrypt_with_key(&ciphertext, secret_key) {
    Err(e) => Err(format!("Decryption failed with key {}: {}", secret_key, e)),
    Ok(v) => Ok(v),
}
```

**Correct Pattern:**
```rust
// ✅ SAFE: Generic error messages
#[error("Signature verification failed")]
SignatureFailed,

#[error("Decryption failed: invalid ciphertext format")]
DecryptionFailed,

// ✅ SAFE: Only error details, not secrets
match decrypt_with_key(&ciphertext, secret_key) {
    Err(e) => Err(format!("Decryption failed: {}", e.kind())),
    Ok(v) => Ok(v),
}
```

**Sensitive Keywords to Avoid:**
- `secret`, `key`, `password`, `token`, `credential`, `private`
- Internal state values
- Intermediate computation results
- User data

---

### 6. Secret Zeroization (`test_secret_zeroization`)

**Purpose:** Ensure sensitive data is zeroized in memory when no longer needed.

**Scope:** `crates/adapteros-crypto/src/secret.rs`

**Requirements:**
```rust
// ✅ CORRECT: Zeroization on drop
#[derive(Clone)]
pub struct SecretKey<const N: usize>([u8; N]);

impl<const N: usize> Zeroize for SecretKey<N> {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl<const N: usize> ZeroizeOnDrop for SecretKey<N> {}
```

**Verification:**
```bash
# Check zeroization is implemented
grep "impl ZeroizeOnDrop" crates/adapteros-crypto/src/secret.rs
grep "use zeroize::" crates/adapteros-crypto/src/secret.rs
```

---

## Running the Tests

### Run All Security Tests

```bash
# Full suite with output
cargo test --test security_regression_suite -- --nocapture

# Single test
cargo test --test security_regression_suite test_no_unsafe_in_public_api -- --nocapture

# Specific test with backtrace
RUST_BACKTRACE=1 cargo test --test security_regression_suite test_cryptographic_operations_validity
```

### CI/CD Integration

Tests run automatically on:
- Every push to `main` or `develop`
- Every pull request affecting security-critical crates
- Manual trigger via GitHub Actions

**View Results:**
```bash
# GitHub Actions
https://github.com/rogu3bear/aos/actions/workflows/security-regression-tests.yml

# Local CI
cargo test --workspace
```

---

## Test Coverage Matrix

| Test | Scope | Frequency | Fail Mode |
|------|-------|-----------|-----------|
| `test_no_unsafe_in_public_api` | 4 crates | Every PR | Hard fail |
| `test_no_panics_in_crypto` | 5 files | Every PR | Hard fail |
| `test_constant_time_operations` | 2 files | Every PR | Warning |
| `test_input_validation` | 3 files | Every PR | Information |
| `test_error_information_leakage` | 3 files | Every PR | Information |
| `test_secret_zeroization` | 1 file | Every PR | Hard fail |
| `test_cryptographic_operations_validity` | Integration | Every PR | Hard fail |
| `test_serialization_security` | Integration | Every PR | Hard fail |
| `test_dependency_audit` | Workspace | Weekly | Hard fail |
| `test_hkdf_seeding` | 2 files | Every PR | Information |
| `test_concurrent_signing_safety` | Integration | Every PR | Hard fail |
| `test_signature_tampering_detection` | Integration | Every PR | Hard fail |

---

## Common Issues & Solutions

### Issue: "Found unsafe blocks in public APIs"

**Cause:** Unsafe code detected in public function in security-critical crate.

**Solution:**
```rust
// Move unsafe to private module
mod unsafe_internals {
    pub unsafe fn ffi_call(ptr: *const u8) -> i32 { /* ... */ }
}

// Safe public wrapper
pub fn safe_wrapper(data: &[u8]) -> Result<i32> {
    unsafe { unsafe_internals::ffi_call(data.as_ptr()) }  // Safety documented
}
```

### Issue: "Found panic-prone operations in crypto code"

**Cause:** `unwrap()`, `expect()`, or `panic!()` in crypto module.

**Solution:**
```rust
// ❌ BEFORE
let sig_bytes = signature.as_bytes().unwrap();

// ✅ AFTER
let sig_bytes = signature.as_bytes()
    .ok_or_else(|| AosError::Crypto("Invalid signature".into()))?;
```

### Issue: "Potential timing-sensitive operations"

**Cause:** Direct comparison or conditional branch on secret data.

**Solution:**
```rust
// ❌ BEFORE
if derived_key == expected_key {
    // ...
}

// ✅ AFTER
use subtle::ConstantTimeEq;
if derived_key.ct_eq(&expected_key).into() {
    // ...
}
```

### Issue: "Potential sensitive data in error messages"

**Cause:** Error message includes secret-related variable or data.

**Solution:**
```rust
// ❌ BEFORE
#[error("Key derivation failed: {key:?}")]
DerivationFailed { key: Vec<u8> },

// ✅ AFTER
#[error("Key derivation failed")]
DerivationFailed,
```

---

## Security Policies

### Policy: Cryptographic Operations

All cryptographic operations must:
1. Use well-vetted libraries (ed25519-dalek, sha2, etc.)
2. Implement constant-time comparisons for secrets
3. Zeroize sensitive data on drop
4. Validate all inputs
5. Return errors, never panic
6. Be thoroughly tested

### Policy: Error Handling

All error messages must:
1. Not include secrets or keys
2. Not reveal internal state
3. Be user-actionable where possible
4. Use generic messages for security-sensitive failures
5. Include context for debugging without leaking secrets

### Policy: Memory Safety

All memory operations must:
1. Use safe Rust where possible
2. Isolate unsafe code in private modules
3. Document all safety invariants
4. Be reviewed by security team
5. Use `zeroize` for sensitive data

### Policy: Randomness

All randomness must:
1. Be derived from HKDF with domain separation
2. Never use `rand::thread_rng()` directly
3. Be seeded from manifest hash in deterministic contexts
4. Be documented with seed source
5. Pass statistical tests

---

## Adding New Tests

### Template for New Security Test

```rust
#[test]
fn test_new_security_property() {
    // 1. Setup
    let keypair = Keypair::generate();

    // 2. Action
    let result = perform_security_sensitive_operation(&keypair);

    // 3. Assertion
    assert!(
        result.is_err() || result.is_ok(),  // Define security property
        "Security property violated"
    );
}
```

### Checklist for New Tests

- [ ] Test has clear description in comments
- [ ] Test is deterministic (no flaky failures)
- [ ] Test checks both positive and negative cases
- [ ] Test validates error conditions
- [ ] Test documents security property being checked
- [ ] Test includes remediation guidance in comments
- [ ] Test is added to test suite summary
- [ ] Test runs in CI/CD pipeline

---

## Maintenance

### Regular Reviews

**Weekly:** Review GitHub Actions logs for new vulnerabilities
```bash
https://github.com/rogu3bear/aos/security/dependabot
```

**Monthly:** Run full dependency audit
```bash
cargo audit
cargo outdated
```

**Quarterly:** Manual security review of critical code
- Signature verification logic
- Key management code
- Error handling paths

### Updating Tests

When security requirements change:

1. Update CLAUDE.md with new policy
2. Add corresponding test case
3. Update this document
4. Review all existing code for compliance
5. Create PR with improvements
6. Add to release notes

---

## References

- [CLAUDE.md](../CLAUDE.md) - Security policies
- [docs/ARCHITECTURE_PATTERNS.md](./ARCHITECTURE_PATTERNS.md) - Patterns
- [crates/adapteros-crypto/src/lib.rs](../crates/adapteros-crypto/src/lib.rs) - Implementation
- [RFC 2104 - HMAC](https://tools.ietf.org/html/rfc2104)
- [RFC 5869 - HKDF](https://tools.ietf.org/html/rfc5869)
- [Ed25519 - Signing](https://ed25519.cr.yp.to/)

---

## Support

For security issues, contact: security@example.com

For test failures, file an issue with:
- Test name and output
- Affected file and line number
- Remediation attempt (if any)
- Security impact assessment
