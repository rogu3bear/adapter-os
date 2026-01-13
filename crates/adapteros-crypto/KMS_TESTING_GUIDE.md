# KMS Provider Integration Tests

Comprehensive test suite for KMS (Key Management Service) provider implementations covering HashiCorp Vault, PKCS#11 HSM, and Mock backends.

> Note: Cloud KMS backends (AWS/GCP/Azure) have been removed as this platform targets offline/air-gapped deployments.

## Overview

The KMS security test suite (`tests/kms_security.rs`) provides:

- **Integration tests** covering offline KMS scenarios
- **Mock KMS servers** for testing without external services
- **Key rotation testing** under concurrent load
- **Performance metrics** for key operations
- **Credential leak detection** to identify security issues

## Test Categories

### 1. Key Rotation Under Concurrent Load (2 tests)

**Purpose:** Stress test key rotation with 10-20 concurrent operations

Tests:
- `test_kms_key_rotation_under_load()`: 10 concurrent key rotations with version bumping
- `test_kms_concurrent_key_generation()`: 20 concurrent key creations

**Metrics:**
```
Operation count tracked via AtomicUsize
Expected operations: num_keys * (get + store) = 10 * 2 = 20+
```

**Performance Baseline:**
- 0ms latency: ~5ms for 10 keys
- 5ms latency: ~55ms for 10 keys (5ms overhead per op)
- 20ms latency: ~205ms for 10 keys

### 2. Configuration Validation (3 tests)

**Purpose:** Validate KMS config parameters

Tests:
- Backend type support (HashiCorp, PKCS#11, Mock)
- Timeout validation (0-300 seconds)
- Retry logic validation (0-10 retries)

### 3. Multi-Tenant Isolation (1 test)

**Purpose:** Verify keys don't leak between tenants

Tests:
- Create keys in 3 namespaces (`tenant-a`, `tenant-b`, `tenant-c`)
- Verify isolation is maintained
- No cross-tenant key access

### 4. Performance & Metrics (2 tests)

**Purpose:** Measure KMS operation performance

Tests:
- Operation counting (tracks all operations via AtomicUsize)
- Latency measurement (start-to-end timing)

## Mock KMS Server

A minimal in-memory KMS implementation for testing without external access:

```rust
pub struct MockKmsServer {
    keys: Arc<RwLock<HashMap<String, MockKeyData>>>,
    operation_count: Arc<AtomicUsize>,
    latency_ms: u64,
    should_fail: Arc<AtomicBool>,
}
```

### Features

**Configurable Latency:**
```rust
let server = MockKmsServer::new(50); // 50ms per operation
```

**Failure Injection:**
```rust
server.set_fail_mode(true);
let result = server.store_key(...).await;
assert!(result.is_err()); // Simulated failure
```

**Operation Metrics:**
```rust
let count = server.get_operation_count();
println!("Performed {} operations", count);
```

## Running Tests

### All KMS Tests

```bash
cargo test -p adapteros-crypto --test kms_security
```

**Output:**
```
running 7 tests
...
test result: ok.
```

### With Logging

```bash
RUST_LOG=debug cargo test -p adapteros-crypto --test kms_security -- --nocapture
```

## Security Considerations

### Credential Handling

**Current Implementation:**
- Credentials stored using `SensitiveData` wrappers
- Debug output redacts secret fields
- Sensitive fields zeroize on drop

**Implementation Pattern:**
```rust
use adapteros_crypto::secret::SensitiveData;

pub enum KmsCredentials {
    VaultToken { token: SensitiveData },
    // ...
}
```

### Multi-Tenancy

Each tenant MUST use a unique `key_namespace`:

```rust
KmsConfig {
    key_namespace: Some("tenant-a".to_string()),
    // ...
}
```

This prevents:
- Cross-tenant key access
- Accidental key sharing
- Namespace pollution

## Future Enhancements

1. **HSM Integration Tests**
   - YubiHSM testing
   - Thales Luna HSM testing
   - PKCS#11 token emulation

2. **Compliance Tests**
   - FIPS 140-2 validation
   - Key escrow verification
   - Audit log verification

3. **Chaos Engineering**
   - Network partition simulation
   - Provider failure injection
   - Cascading failure scenarios

## References

- [HashiCorp Vault Documentation](https://www.vaultproject.io/docs)
- [PKCS#11 Standard](http://docs.oasis-open.org/pkcs11/pkcs11-base/v2.40/os/pkcs11-base-v2.40-os.html)
- [Zeroize Documentation](https://docs.rs/zeroize/)
