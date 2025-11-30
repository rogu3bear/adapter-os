# Security Regression Test Suite

**Date Created:** 2025-11-22
**Status:** Production Ready
**Maintenance:** James KC Auchterlonie

---

## Quick Start

Run the complete security regression test suite:

```bash
# Run all security tests
cargo test --test security_regression_suite -- --nocapture

# Run specific test
cargo test --test security_regression_suite test_no_unsafe_in_public_api -- --nocapture

# Run with backtrace for debugging
RUST_BACKTRACE=1 cargo test --test security_regression_suite
```

---

## Files Created

1. **Test Suite:**
   - `/tests/security_regression_suite.rs` - Comprehensive test suite (12 tests)
   - `/tests/security_regression_suite.config.toml` - Configuration file
   - `/.github/workflows/security-regression-tests.yml` - CI/CD workflow

2. **Documentation:**
   - `/docs/SECURITY_TESTING.md` - Complete testing guide
   - `SECURITY_TEST_README.md` - This file

---

## Test Coverage

The suite includes 12 automated security tests organized into 3 categories:

### Category 1: Static Analysis (Compile-Time Detection)

1. **test_no_unsafe_in_public_api**
   - Scans security-critical crates for unsafe blocks in public functions
   - Scope: adapteros-crypto, adapteros-core, adapteros-base-llm, adapteros-domain
   - Fail Mode: Hard fail on detection

2. **test_no_panics_in_crypto**
   - Validates that cryptographic code doesn't use unwrap/expect/panic
   - Scope: adapteros-crypto/src/signature.rs, secret.rs, bundle_sign.rs, etc.
   - Fail Mode: Hard fail on detection

3. **test_constant_time_operations**
   - Checks for timing-sensitive operations that could leak information
   - Scope: Cryptographic comparison operations
   - Fail Mode: Warning (requires manual review)

4. **test_input_validation**
   - Ensures public APIs validate untrusted input
   - Scope: signature.rs, secret.rs, core/lib.rs
   - Fail Mode: Information (informational only)

5. **test_error_information_leakage**
   - Validates error messages don't include sensitive data
   - Scope: Error implementations across security-critical modules
   - Fail Mode: Information (requires review)

6. **test_hkdf_seeding**
   - Ensures randomness uses HKDF, not direct RNG
   - Scope: Router, deterministic executor
   - Fail Mode: Information (advisory)

### Category 2: Runtime Validation (Execution-Time Tests)

7. **test_secret_zeroization**
   - Validates that SecretKey implements ZeroizeOnDrop
   - Verifies memory safety for sensitive data
   - Fail Mode: Hard fail on missing zeroization

8. **test_cryptographic_operations_validity**
   - Tests core cryptographic properties
   - Validates signing/verification correctness
   - Fail Mode: Hard fail on incorrect behavior

9. **test_serialization_security**
   - Ensures secrets cannot be serialized
   - Verifies debug output is redacted
   - Fail Mode: Hard fail on serialization success

10. **test_concurrent_signing_safety**
    - Validates thread-safe cryptographic operations
    - Tests concurrent signing with shared keypairs
    - Fail Mode: Hard fail on panic or corruption

11. **test_signature_tampering_detection**
    - Verifies that tampered signatures fail verification
    - Tests single-bit corruption detection
    - Fail Mode: Hard fail on false verification

### Category 3: Configuration & Dependency Checks

12. **test_dependency_audit**
    - Checks workspace has required security crates
    - Validates presence of: zeroize, ed25519, sha2, thiserror
    - Fail Mode: Hard fail on missing dependency

---

## Running in CI/CD

The suite runs automatically on:

1. **Every PR** affecting security-critical crates:
   - `crates/adapteros-crypto/**`
   - `crates/adapteros-core/**`
   - `crates/adapteros-db/**`
   - `crates/adapteros-domain/**`
   - `crates/adapteros-base-llm/**`
   - `crates/adapteros-deterministic-exec/**`

2. **Every push** to main/develop branches

3. **View results:**
   - GitHub Actions: https://github.com/rogu3bear/aos/actions/workflows/security-regression-tests.yml
   - PR comments: Automated security test report posted on every PR

---

## Configuration

The test suite is configured via `/tests/security_regression_suite.config.toml`:

```toml
[test]
fail_fast = true           # Stop on first security failure
timeout_seconds = 300      # Maximum test duration
parallel_workers = 1       # Serialize for consistency

[security_critical_crates]
crates = ["adapteros-crypto", "adapteros-core", ...]

[crypto_operations]
require_constant_time = ["signature_verify", "compare_secret", ...]
require_zeroization = ["SecretKey", "KeyMaterial", "SensitiveData"]
require_error_handling = ["sign_bytes", "verify_signature", ...]

[panic_detection]
forbidden_patterns = ["unwrap()", "expect(", "panic!"]

[ci_schedule]
on_push = ["main", "develop"]
on_pull_request = true
frequency = "every_pr"
```

---

## Remediation Guide

When tests fail, use this guide to fix issues:

### Unsafe Code in Public API

**Error:** "Found unsafe blocks in public APIs"

**Fix:**
```rust
// Move unsafe to private module
mod unsafe_internals {
    pub unsafe fn ffi_call(ptr: *const u8) -> i32 { /* ... */ }
}

// Safe public wrapper
pub fn safe_wrapper(data: &[u8]) -> Result<i32> {
    unsafe { unsafe_internals::ffi_call(data.as_ptr()) }
}
```

### Panics in Crypto Code

**Error:** "Found panic-prone operations in crypto code"

**Fix:**
```rust
// ❌ BEFORE
let sig = signature.unwrap();

// ✅ AFTER
let sig = signature.ok_or_else(|| AosError::Crypto("Invalid sig".into()))?;
```

### Timing-Sensitive Operations

**Error:** "Potential timing-sensitive operations"

**Fix:**
```rust
// ❌ BEFORE
if key == expected {
    return Ok(());
}

// ✅ AFTER
use subtle::ConstantTimeEq;
if key.ct_eq(&expected).into() {
    return Ok(());
}
```

### Error Information Leakage

**Error:** "Potential sensitive data in error messages"

**Fix:**
```rust
// ❌ BEFORE
#[error("Failed with key {key:?}")]
Failed { key: Vec<u8> },

// ✅ AFTER
#[error("Failed")]
Failed,
```

---

## Integration with Development Workflow

### Before Committing

```bash
# Run security tests locally
cargo test --test security_regression_suite

# Run full test suite
cargo test --workspace

# Run linting
cargo fmt --all && cargo clippy --workspace -- -D warnings
```

### In Pull Requests

1. Tests run automatically on PR creation
2. Results posted as PR comment
3. All security tests must pass before merge
4. Review warnings for additional issues

### Continuous Monitoring

- Weekly: Dependency audit (cargo audit)
- Monthly: Manual security review of critical code
- Quarterly: Full codebase security assessment

---

## Performance

- **Test Suite Duration:** ~15-30 seconds
- **CI/CD Run Time:** ~5 minutes (including compilation)
- **Resource Usage:** Low memory, CPU-bound

---

## Troubleshooting

### Issue: Tests timeout

**Solution:** Increase timeout in config.toml or run specific test:
```bash
cargo test --test security_regression_suite test_cryptographic_operations_validity
```

### Issue: Cannot find crypto crate

**Solution:** Ensure crypto crate is in workspace:
```bash
cargo build -p adapteros-crypto
```

### Issue: Test reports not posting to PR

**Solution:** Check GitHub Actions permissions:
- Ensure workflow has `write` permission for pull-requests
- Verify GitHub token is valid

---

## Policy Updates

When security policies change:

1. Update `/CLAUDE.md` with new policy
2. Add corresponding test case to `security_regression_suite.rs`
3. Update `/docs/SECURITY_TESTING.md` with remediation guide
4. Update `/tests/security_regression_suite.config.toml` with new rules
5. Review all existing code for compliance
6. Create PR with improvements and link to security policy

---

## Next Steps

1. **Review:** Read `/docs/SECURITY_TESTING.md` for detailed test documentation
2. **Configure:** Customize `/tests/security_regression_suite.config.toml` as needed
3. **Run Locally:** Execute `cargo test --test security_regression_suite`
4. **Review Results:** Check for any warnings or failures
5. **Fix Issues:** Follow remediation guide to address findings
6. **Merge:** Once all security tests pass, PR is ready for review

---

## References

- [Security Testing Guide](./docs/SECURITY_TESTING.md)
- [Developer Guide](./CLAUDE.md)
- [GitHub Actions Workflow](../.github/workflows/security-regression-tests.yml)
- [Architecture Patterns](./docs/ARCHITECTURE_PATTERNS.md)

---

## Support

For questions or issues with the security test suite:

1. Check `/docs/SECURITY_TESTING.md` for detailed documentation
2. Review existing test implementations for patterns
3. Contact security team for policy clarifications
4. Open issue with test output and reproduction steps

---

**Maintained by:** James KC Auchterlonie
**Last Updated:** 2025-11-22
**Copyright:** © 2025 JKCA. All rights reserved.
