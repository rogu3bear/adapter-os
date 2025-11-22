# Secure Enclave Graceful Degradation

**Status:** Implemented
**Last Updated:** 2025-11-21
**Author:** James KC Auchterlonie

## Overview

The adapteros-secd service now supports graceful degradation on non-macOS platforms and macOS systems without the `secure-enclave` feature enabled. Instead of hard failures, the system falls back to a software-based cryptographic implementation that maintains security properties suitable for development and testing.

## Architecture

### Platform-Specific Backend Selection

The enclave implementation uses conditional compilation to select the appropriate backend:

```
┌─────────────────────────────────────────────────────────┐
│           Enclave Manager Selection                      │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  macOS + secure-enclave feature?                        │
│         ├─ YES → Hardware Secure Enclave (macos.rs)    │
│         └─ NO  → Software Fallback (stub.rs)           │
│                                                          │
│  Linux/Windows/other?                                   │
│         └─ Software Fallback (stub.rs)                 │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

### Hardware Enclave (macOS with feature)

**File:** `crates/adapteros-secd/src/enclave/macos.rs`

- Uses Security Framework to access Secure Enclave
- ECDSA signing (P-256)
- Keys stored in tamper-resistant hardware
- Attestation capabilities
- Production-grade security

### Software Fallback (All other platforms)

**File:** `crates/adapteros-secd/src/enclave/stub.rs`

- Platform-agnostic cryptography
- Ed25519 for signing
- ChaCha20-Poly1305 for encryption
- HKDF-SHA256 for key derivation with domain separation
- Keys stored in ephemeral process memory

## Security Properties

| Operation | macOS (HW) | Software Fallback |
|-----------|-----------|-------------------|
| **Signing** | ECDSA P-256 (Secure Enclave) | Ed25519 (software) |
| **Verification** | Hardware attestation | Software verification |
| **Encryption** | ChaCha20-Poly1305 | ChaCha20-Poly1305 |
| **Key Derivation** | Master key via Keychain | HKDF-SHA256 + domain separation |
| **Nonce Generation** | Deterministic (data-derived) | Deterministic (data-derived) |
| **Key Storage** | Tamper-resistant hardware | Process memory (ephemeral) |
| **Use Case** | Production (macOS only) | Development, testing, non-critical deployments |

## Implementation Details

### Root Key Initialization

The software fallback derives a root key from system entropy at initialization:

```rust
let mut root_key = [0u8; 32];
use rand::RngCore;
let mut rng = rand::thread_rng();
rng.fill_bytes(&mut root_key);
```

This uses the OS's cryptographically secure random source (via `rand` crate).

### Key Derivation with HKDF

All cryptographic keys are derived from the root key using HKDF-SHA256 with domain separation:

```rust
let hkdf = Hkdf::<Sha256>::new(None, &self.root_key);
let info = format!("adapteros:{}:{}", label, purpose);
hkdf.expand(info.as_bytes(), &mut output)?;
```

**Domain separation ensures:**
- Encryption keys for different labels are cryptographically independent
- Signing keys are derived separately from encryption keys
- No cross-purpose key reuse

### Deterministic Nonce Generation

Nonces for encryption are deterministic (matching macOS implementation):

```rust
let domain = format!("enclave-nonce:{}", label);
let seed = derive_seed(&B3Hash::hash(data), &domain);
let nonce = Nonce::from_slice(&seed[..12]);
```

This provides:
- Deterministic encryption (same plaintext → same ciphertext)
- No random nonce overhead
- Reproducibility across process restarts

### Signing and Verification

Ed25519 signatures are deterministic:

```rust
let signing_key = SigningKey::from_bytes(&key_bytes);
let signature = signing_key.sign(data);
```

Public keys can be extracted for verification:

```rust
let verifying_key = signing_key.verifying_key();
let public_key_bytes = verifying_key.to_bytes();
```

## Feature Flags

### Cargo Features

The `secure-enclave` feature controls platform-specific implementations:

```toml
[features]
default = []
# Enable macOS Secure Enclave support (requires macOS 10.12+)
secure-enclave = ["security-framework", "core-foundation"]
```

### Building with Feature Flags

```bash
# Development build (uses software fallback on non-macOS)
cargo build -p adapteros-secd

# Production macOS build (enables hardware Secure Enclave)
cargo build -p adapteros-secd --features secure-enclave

# Force software fallback on macOS (for testing)
cargo build -p adapteros-secd --no-default-features
```

## Logging and Diagnostics

### Log Output Examples

Software fallback initialization:
```
WARN  Secure Enclave not available: using software-based fallback (development/testing only)
INFO  Software fallback initialized with HKDF-derived keys (ChaCha20-Poly1305 + Ed25519)
```

Encryption with software fallback:
```
INFO  label=lora_delta plaintext_bytes=4096 ciphertext_bytes=4112 backend=software-fallback Encrypted payload with software-derived key
```

Key derivation:
```
DEBUG label=aos_bundle purpose=signing Derived key using HKDF with domain separation
```

### Monitoring

Check the enclave backend via logs:

```bash
# macOS hardware enclave (no fallback warning)
journalctl -u aos-secd | grep -v "software-based fallback"

# Software fallback (will see warnings)
journalctl -u aos-secd | grep "software-based fallback"
```

## Migration Path from Hardware to Software

If transitioning from macOS to another platform:

1. Data encrypted with hardware enclave remains encrypted with ChaCha20-Poly1305
2. Software fallback can decrypt existing ciphertexts if key derivation is deterministic
3. Signatures change from ECDSA to Ed25519 (incompatible with existing verifiers)

### Cross-Platform Considerations

**Not supported:**
- Migrating ECDSA signatures to Ed25519
- Decryption of data sealed with different root keys

**Supported:**
- Re-encrypting data with software fallback after migration
- Reading existing encrypted LoRA deltas (ChaCha20-Poly1305 is platform-agnostic)

## Production Recommendations

### For Non-macOS Deployments

1. **Use TPM/TEE integration** if available on target platform
2. **Encrypt key storage** using platform-specific key management
3. **Restrict process access** using security modules (SELinux, AppArmor)
4. **Monitor root key material** in process memory
5. **Use external HSM** for critical signing operations

### For macOS Deployments

1. **Enable secure-enclave feature** for production builds
2. **Validate attestation** of Secure Enclave key generation
3. **Audit Keychain access** logs
4. **Use code signing** to ensure process integrity

## Testing

### Unit Tests

Software fallback is tested via:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seal_unseal_roundtrip() {
        let mut manager = EnclaveManager::new().unwrap();
        let data = b"test data";
        let sealed = manager.seal_with_label("test", data).unwrap();
        let unsealed = manager.unseal_with_label("test", &sealed).unwrap();
        assert_eq!(data, unsealed.as_slice());
    }

    #[test]
    fn test_signing() {
        let mut manager = EnclaveManager::new().unwrap();
        let data = b"test data";
        let signature = manager.sign_with_label("test", data).unwrap();
        assert!(!signature.is_empty());
        assert_eq!(signature.len(), 64); // Ed25519 signature length
    }
}
```

### Integration Tests

Cross-platform testing:

```bash
# Test on Linux (software fallback)
cargo test -p adapteros-secd --lib

# Test on macOS without feature (software fallback)
cargo test -p adapteros-secd --lib --no-default-features

# Test on macOS with feature (hardware enclave)
cargo test -p adapteros-secd --lib --features secure-enclave
```

## Limitations and Caveats

### Software Fallback Limitations

| Limitation | Impact | Mitigation |
|-----------|--------|-----------|
| Keys in process memory | Memory disclosure risk | Use encrypted memory if available |
| No hardware attestation | Can't prove key security | Use external attestation service |
| Deterministic nonces | Side-channel if misused | Ensure data is not sensitive metadata |
| Ephemeral storage | Keys lost on restart | Re-derive from root key on startup |

### Hardware Enclave Advantages (macOS only)

- Keys never leave Secure Enclave
- Hardware-backed signing
- Tamper-resistant key storage
- Attestation support

## Troubleshooting

### Issue: "Secure Enclave not available: using software-based fallback"

**Cause:** Running on non-macOS platform or macOS without `secure-enclave` feature

**Resolution:**
- For development: This is expected and normal
- For production on macOS: Build with `--features secure-enclave`
- For production on Linux: Consider TPM/TEE integration

### Issue: Decryption failures after platform migration

**Cause:** Different root key initialization or key derivation changes

**Resolution:**
- Re-seal data with new backend
- Use encrypted key transport mechanism
- Implement custom key migration logic

## Related Documentation

- [CLAUDE.md](../CLAUDE.md) - Project standards and conventions
- [crates/adapteros-secd/src/enclave/](../crates/adapteros-secd/src/enclave/) - Implementation
- [Security Framework docs](https://developer.apple.com/documentation/security) - macOS Secure Enclave
- [HKDF RFC 5869](https://tools.ietf.org/html/rfc5869) - Key derivation standard

## References

- Ed25519-Dalek: https://docs.rs/ed25519-dalek/
- ChaCha20-Poly1305: https://docs.rs/chacha20poly1305/
- HKDF: https://docs.rs/hkdf/
- BLAKE3: https://blake3.io/
