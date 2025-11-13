# AdapterOS Cryptographic Architecture

## Overview

AdapterOS provides a comprehensive cryptographic foundation with hardware-backed security, cross-platform key management, and deterministic execution guarantees. This document outlines the cryptographic subsystems and their integration.

## Core Cryptographic Operations

### Digital Signatures (Ed25519)

**Purpose:** Identity verification, bundle signing, and cryptographic receipts

```rust
use adapteros_crypto::{KeyAlgorithm, KeyProvider};

let provider = KeyProvider::new(KeyProviderConfig::default()).await?;
let key_handle = provider.generate("my-key", KeyAlgorithm::Ed25519).await?;
let signature = provider.sign("my-key", b"Hello, world!").await?;
```

**Properties:**
- Ed25519 elliptic curve signatures
- Deterministic signatures (same message = same signature)
- 32-byte public keys, 64-byte signatures
- Hardware acceleration when available

### Symmetric Encryption (AES-256-GCM)

**Purpose:** Data confidentiality, envelope encryption, secure communication

```rust
use adapteros_crypto::envelope::Envelope;

let envelope = Envelope::encrypt(b"secret data", "recipient-key-id").await?;
let plaintext = envelope.decrypt("recipient-key-id").await?;
```

**Properties:**
- AES-256-GCM authenticated encryption
- 96-bit nonces (automatically generated)
- 128-bit authentication tags
- No padding required

### Hashing (BLAKE3)

**Purpose:** Content addressing, integrity verification, deterministic identifiers

```rust
use adapteros_core::B3Hash;

let hash = B3Hash::hash(b"data");
let hex_string = hash.to_hex();
```

**Properties:**
- 256-bit output
- Cryptographically secure
- High performance (SIMD optimized)
- Used for all content addressing in AdapterOS

## Keychain Provider System

### Cross-Platform Key Storage

AdapterOS implements a sophisticated keychain provider that abstracts platform-specific secure storage:

```rust
use adapteros_crypto::key_provider::{KeyProvider, KeyProviderConfig};

let config = KeyProviderConfig::default();
let provider = KeyProvider::new(config).await?;

// Generate and use keys
let handle = provider.generate("app-signing-key", KeyAlgorithm::Ed25519).await?;
let signature = provider.sign("app-signing-key", data).await?;
```

### Platform-Specific Backends

#### macOS (Security Framework)
- **Primary:** Secure CLI commands to system keychain
- **Hardware:** Secure Enclave integration for receipt signing
- **Security:** Command injection prevention with input validation
- **Access:** User login credentials protect stored keys

#### Linux (Multiple Options)
- **Primary:** D-Bus Secret Service (GNOME Keyring/KWallet)
- **Fallback:** Linux kernel keyring (headless/server environments)
- **Password:** Argon2id-based encrypted JSON keystore for CI
- **Auto-detection:** Graceful fallback between backends

### Key Lifecycle Management

#### Key Generation
```rust
// Automatic algorithm selection based on use case
let signing_key = provider.generate("signing-key", KeyAlgorithm::Ed25519).await?;
let encryption_key = provider.generate("encrypt-key", KeyAlgorithm::Aes256Gcm).await?;
```

#### Key Rotation
```rust
// Cryptographic rotation with signed receipts
let receipt = provider.rotate_key("signing-key").await?;
println!("Key rotated: {}", receipt.timestamp);
```

#### Key Attestation
```rust
// Provider state verification
let attestation = provider.attest().await?;
println!("Provider: {}, Hardware: {}", attestation.provider_type, attestation.hardware_backed);
```

## Security Properties

### Threat Model

**Assumptions:**
- OS keychain backends are trustworthy
- Hardware (Secure Enclave) is not compromised
- User credentials are not compromised

**Protections:**
- Memory zeroization after use
- No plaintext key material in logs
- Hardware-backed operations when available
- Fine-grained access controls

### Deterministic Execution

All cryptographic operations support deterministic execution for reproducibility:

```rust
use adapteros_deterministic_exec::GlobalSeed;

// Set global seed for deterministic randomness
let seed = GlobalSeed::get_or_init(seed_hash);

// All subsequent crypto operations are deterministic
let key = provider.generate("deterministic-key", KeyAlgorithm::Ed25519).await?;
```

## Integration Points

### Bundle Signing
```rust
use adapteros_crypto::bundle_sign::{BundleSigner, SigningKey};

let signer = BundleSigner::new(provider);
let signed_bundle = signer.sign_bundle(bundle_data).await?;
```

### Telemetry Security
```rust
use adapteros_telemetry::secure::{SecureTelemetry, TelemetryEnvelope};

let secure_telemetry = SecureTelemetry::new(provider);
let envelope = secure_telemetry.seal_telemetry(telemetry_data).await?;
```

### API Authentication
```rust
use adapteros_crypto::auth::{JwtSigner, Claims};

let jwt_signer = JwtSigner::new(provider, "api-keys");
let token = jwt_signer.sign_jwt(claims).await?;
```

## Configuration

### Environment Variables

| Variable | Purpose | Example |
|----------|---------|---------|
| `ADAPTEROS_KEYCHAIN_FALLBACK` | Enable password-based keystore | `pass:mysecret123` |
| `ADAPTEROS_DETERMINISTIC_SEED` | Global randomness seed | `hex:0123456789abcdef...` |

### Configuration File

```toml
[crypto]
# Key provider settings
key_provider = "keychain"
keychain_service = "adapteros"

# Key rotation settings
rotation_interval_hours = 24
max_key_age_days = 365

# Hardware security
require_hardware_security = false  # Set to true for production
```

## Testing and Validation

### Cryptographic Correctness
```bash
# Run crypto test suite
cargo test -p adapteros-crypto --lib

# Test keychain provider
cargo test -p adapteros-crypto --features password-fallback
```

### Security Auditing
```bash
# Check for hardcoded secrets
cargo audit

# Run security linter
cargo clippy --workspace -- -D warnings
```

## Error Handling

### Common Errors

**Keychain Access Denied:**
```rust
// macOS: Unlock keychain
Command::new("security").arg("unlock-keychain").status()?;

// Linux: Start keyring daemon
Command::new("gnome-keyring-daemon").arg("--start").status()?;
```

**Invalid Key Format:**
```rust
// Keys must be 32 bytes for Ed25519
if key_bytes.len() != 32 {
    return Err(AosError::Crypto("Invalid Ed25519 key length".to_string()));
}
```

**Hardware Unavailable:**
```rust
// Graceful fallback to software crypto
if !hardware_available() {
    warn!("Hardware security unavailable, using software fallback");
}
```

## Performance Considerations

### Hardware Acceleration
- Secure Enclave: ~10x faster than software Ed25519
- AES-NI: Hardware accelerated AES operations
- SIMD: BLAKE3 optimized for modern CPUs

### Memory Management
- Zero-copy operations where possible
- Automatic cleanup of sensitive data
- Memory locking for critical operations

## References

- [Ed25519: RFC 8032](https://tools.ietf.org/rfc/rfc8032.txt)
- [AES-GCM: NIST SP 800-38D](https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.800-38D.pdf)
- [BLAKE3 Specification](https://github.com/BLAKE3-team/BLAKE3-specs/blob/master/blake3.pdf)
- [AdapterOS Security Ruleset #14](../docs/SECURITY_RULESET.md#rule-14)
