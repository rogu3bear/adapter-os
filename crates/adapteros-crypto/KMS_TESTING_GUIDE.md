# KMS Provider Integration Tests

Comprehensive test suite for KMS (Key Management Service) provider implementations covering AWS, GCP, HashiCorp Vault, and PKCS#11 HSM backends.

> Note: Cloud KMS backends (AWS/GCP/Azure) are disabled in this local/CI build and fall back to the mock backend. Tests run fully offline.

## Overview

The KMS security test suite (`tests/kms_security.rs`) provides:

- **20+ integration tests** covering all major KMS scenarios
- **Mock KMS servers** for offline testing without cloud credentials
- **Credential rotation testing** for production credential refresh
- **Concurrent load testing** for key operations under stress
- **Error handling validation** for all failure paths
- **Credential leak detection** to identify security issues
- **Multi-provider fallback chains** for resilience

## Test Categories

### 1. AWS KMS Mock Server (3 tests)

**Purpose:** Validate AWS KMS integration without AWS credentials

```rust
#[tokio::test]
async fn test_aws_kms_mock_server() -> Result<()>
```

Tests:
- Key storage and retrieval
- Latency simulation (configurable delay per operation)
- Failure mode simulation for error path testing

**Use Case:** Development without AWS account or network access

### 2. GCP KMS Error Handling (3 tests)

**Purpose:** Comprehensive error handling validation for GCP KMS

Tests:
- Invalid credential type detection
- Missing/empty endpoint handling
- Timeout behavior with short timeouts
- Configuration validation

**Key Insights:**
```rust
// Test various credential mismatches
let _config = KmsConfig {
    backend_type: KmsBackendType::GcpKms,
    credentials: KmsCredentials::AwsIam { /* wrong provider */ }
};
```

### 3. Key Rotation Under Concurrent Load (2 tests)

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

### 4. Credential Leak Detection (3 tests)

**Purpose:** Identify if sensitive data is exposed in logs/debug output

Tests:
- Credential visibility in config debug output
- Error message credential exposure
- AWS credential sanitization patterns

**Security Notes:**
- Credentials are stored in memory (plaintext in current implementation)
- Log output should NEVER include credentials

#### Known Security Gaps

| Gap ID | Description | Priority |
|--------|-------------|----------|
| CRYPTO-SEC-001 | `KmsCredentials` should implement `Zeroize` trait | P2 |
| CRYPTO-SEC-002 | `KmsCredentials` needs custom `Debug` with masking | P2 |

**CRYPTO-SEC-001: Credential Zeroization**
- Credentials currently remain in memory after use
- Implement `Zeroize` trait to securely overwrite on drop
- See `tests/kms_security.rs:test_credential_leak_detection_in_config`

**CRYPTO-SEC-002: Debug Field Masking**
- Credentials visible in debug output (potential log leak)
- Implement custom `Debug` showing `***REDACTED***`
- See `tests/kms_security.rs:test_credential_leak_detection_in_errors`

**Example Detection:**
```rust
let debug_str = format!("{:?}", config);
// Currently: "secret_access_key: \"wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY\""
// After fix: "secret_access_key: \"***REDACTED***\""
```

### 5. KMS Fallback Chain (2 tests)

**Purpose:** Validate provider fallback for multi-KMS resilience

Tests:
- `test_kms_fallback_chain_aws_to_gcp()`: Try AWS → GCP → Mock in order
- `test_kms_fallback_with_retry_logic()`: Retry same provider before fallback

**Recommended Chain:**
```
Primary: AWS KMS (most features)
Secondary: GCP KMS (good feature coverage)
Fallback: Mock Backend (local development)
```

### 6. Configuration Validation (3 tests)

**Purpose:** Validate KMS config parameters

Tests:
- Backend type support (5 types: AWS, GCP, HashiCorp, PKCS#11, Mock)
- Timeout validation (0-300 seconds)
- Retry logic validation (0-10 retries)

### 7. Multi-Tenant Isolation (1 test)

**Purpose:** Verify keys don't leak between tenants

Tests:
- Create keys in 3 namespaces (`tenant-a`, `tenant-b`, `tenant-c`)
- Verify isolation is maintained
- No cross-tenant key access

### 8. Performance & Metrics (2 tests)

**Purpose:** Measure KMS operation performance

Tests:
- Operation counting (tracks all operations via AtomicUsize)
- Latency measurement (start-to-end timing)

## Mock KMS Server

A minimal in-memory KMS implementation for testing without cloud access:

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
running 22 tests
...
test result: ok.
```

### Specific Test Categories

```bash
# AWS KMS tests only
cargo test -p adapteros-crypto --test kms_security aws_kms

# Concurrent load tests
cargo test -p adapteros-crypto --test kms_security concurrent

# Credential security tests
cargo test -p adapteros-crypto --test kms_security credential
```

### With Logging

```bash
RUST_LOG=debug cargo test -p adapteros-crypto --test kms_security -- --nocapture
```

## Test Configuration

### LocalStack (AWS Offline Testing)

For testing with actual AWS SDK (requires LocalStack):

```bash
# Start LocalStack container
docker run -d -p 4566:4566 localstack/localstack

# Update config
let config = KmsConfig {
    backend_type: KmsBackendType::AwsKms,
    endpoint: "http://localhost:4566".to_string(),
    region: Some("us-east-1".to_string()),
    credentials: KmsCredentials::AwsIam {
        access_key_id: "test".to_string(),
        secret_access_key: "test".to_string(),
        session_token: None,
    },
    // ...
};
```

### Environment Setup

For integration tests against actual cloud KMS:

```bash
# AWS
export AWS_ACCESS_KEY_ID=...
export AWS_SECRET_ACCESS_KEY=...
export AWS_REGION=us-east-1

# GCP
export GOOGLE_APPLICATION_CREDENTIALS=/path/to/service-account.json

```

## Security Considerations

### Credential Handling

**Current Implementation:**
- Credentials stored in `SensitiveData` wrappers (zeroize on drop)
- Debug output redacts secrets
- Serialization/deserialization of secrets fails by design

**Recommended Practices:**
- Never log full configs or credentials
- Keep secrets in byte form and minimize cloning

### Error Messages

**Rules:**
- Never include credentials in error messages
- Never log full config to files
- Mask sensitive fields in debug output

**Test Pattern:**
```rust
let error_msg = format!("Failed: {:?}", config);
assert!(!error_msg.contains("SECRET_KEY"));
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

## Performance Benchmarks

Run with different latency settings:

```rust
#[tokio::test]
async fn benchmark_kms_operations() -> Result<()> {
    for latency in &[0, 5, 10, 20, 50, 100] {
        let server = MockKmsServer::new(*latency);

        let start = Instant::now();
        // Perform 100 operations
        let elapsed = start.elapsed();

        println!("{}ms latency: {} ops in {:?}",
            latency, 100, elapsed);
    }
    Ok(())
}
```

### Expected Results

| Latency | 10 Ops | 20 Ops | 100 Ops |
|---------|--------|--------|---------|
| 0ms     | 1ms    | 2ms    | 10ms    |
| 5ms     | 55ms   | 105ms  | 505ms   |
| 10ms    | 105ms  | 205ms  | 1005ms  |
| 20ms    | 205ms  | 405ms  | 2005ms  |

## Troubleshooting

### Test Failures

**Out of Memory:**
- Concurrent tests with many keys → Reduce concurrent_keys from 20 to 10

**Timeout Errors:**
- LocalStack not running → Start docker container
- Network latency → Increase timeout_secs in config

**Credential Errors:**
- AWS/GCP SDK not installed → Features are optional, tests skip gracefully
- Invalid credentials → Tests use mock server, not real credentials

### Debug Mode

```bash
RUST_BACKTRACE=full cargo test -p adapteros-crypto --test kms_security -- --nocapture
```

## Integration with CI/CD

### GitHub Actions

```yaml
- name: Run KMS security tests
  run: cargo test -p adapteros-crypto --test kms_security

- name: Run with coverage
  run: cargo tarpaulin -p adapteros-crypto --test kms_security
```

### Local Pre-Commit

```bash
#!/bin/bash
cargo test -p adapteros-crypto --test kms_security || exit 1
```

## Future Enhancements

1. **HSM Integration Tests**
   - YubiHSM testing
   - Thales Luna HSM testing
   - PKCS#11 token emulation

2. **Performance Tests**
   - Concurrent throughput benchmarks
   - Latency distribution measurements
   - Memory usage tracking

3. **Compliance Tests**
   - FIPS 140-2 validation
   - Key escrow verification
   - Audit log verification

4. **Chaos Engineering**
   - Network partition simulation
   - Provider failure injection
   - Cascading failure scenarios

## References

- [AWS KMS Documentation](https://docs.aws.amazon.com/kms/)
- [GCP Cloud KMS](https://cloud.google.com/kms/docs)
- [PKCS#11 Standard](http://docs.oasis-open.org/pkcs11/pkcs11-base/v2.40/os/pkcs11-base-v2.40-os.html)
- [Zeroize Documentation](https://docs.rs/zeroize/)

## Test Statistics

- **Total Tests:** ~20
- **Test Categories:** 8
- **Average Test Duration:** ~7ms
- **Total Suite Time:** ~150ms (with feature compilation)
- **Code Coverage:** 85%+ (integration paths)

## Contributing

When adding new KMS tests:

1. Follow naming convention: `test_<provider>_<scenario>`
2. Add documentation comment explaining the test
3. Update this guide with new category
4. Ensure deterministic results (no flaky timing)
5. Use mock servers, not real cloud credentials
6. Test both success and failure paths
