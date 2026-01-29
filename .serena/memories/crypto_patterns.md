# AdapterOS Crypto System Patterns

## Overview

The `adapteros-crypto` crate (`/Users/star/Dev/adapter-os/crates/adapteros-crypto/`) provides production-grade cryptographic primitives for the AdapterOS deterministic ML inference platform. It emphasizes security, auditability, and fail-closed semantics.

## Core Dependencies

- **ed25519-dalek**: Ed25519 digital signatures
- **blake3**: Fast cryptographic hashing (via `adapteros-core::B3Hash`)
- **aes-gcm**: AES-256-GCM authenticated encryption
- **hkdf + sha2**: Key derivation (HKDF-SHA256)
- **hmac**: HMAC-SHA256 for tenant binding
- **zeroize**: Secure memory cleanup
- **x509-parser**: Certificate chain verification
- **security-framework** (macOS): Keychain and Secure Enclave integration

## 1. Ed25519 Signing Patterns

### Key Files
- `/Users/star/Dev/adapter-os/crates/adapteros-crypto/src/signature.rs`

### Core Types
```rust
pub struct Keypair { signing_key: SigningKey }
pub struct PublicKey { inner: Ed25519PublicKey }
pub struct Signature { inner: Ed25519Signature }
```

### Key Functions
- `Keypair::generate()` - Generate new keypair using OsRng
- `Keypair::from_bytes(&[u8; 32])` - Reconstruct from seed
- `keypair.sign(message: &[u8]) -> Signature` - Sign arbitrary data
- `public_key.verify(message, signature) -> Result<()>` - Constant-time verification
- `sign_bytes(keypair, message)` / `verify_signature(pubkey, message, sig)` - Convenience functions

### Security Features
- Uses `OsRng` for cryptographic randomness
- Constant-time signature verification (prevents timing attacks)
- Schema versioning (`SIG_SCHEMA_VERSION = 1`) for future compatibility
- Hex serialization for JSON compatibility

## 2. BLAKE3 Hashing Usage

### Key Usage Patterns
BLAKE3 (via `adapteros_core::B3Hash`) is used throughout for:

1. **Bundle Hashing**: Hash telemetry bundles before signing
2. **Key ID Derivation**: `kid = blake3(pubkey)[..32]` (128-bit, hex-encoded)
3. **Merkle Tree Construction**: Decision chain and bundle commit Merkle roots
4. **Audit Entry Hashing**: Chain integrity via `entry_hash = BLAKE3(canonical_entry)`
5. **Fingerprinting**: Key fingerprint = `BLAKE3(deterministic_signature)`
6. **Policy Hash**: Hash of policy configuration for attestation

### Deterministic Hashing
- Uses JCS (JSON Canonicalization Scheme - RFC 8785) via `serde_jcs` for deterministic serialization before hashing
- Critical for reproducible receipts and audit trails

## 3. Key Management and Storage

### Key Files
- `/Users/star/Dev/adapter-os/crates/adapteros-crypto/src/key_manager.rs`
- `/Users/star/Dev/adapter-os/crates/adapteros-crypto/src/key_provider.rs`
- `/Users/star/Dev/adapter-os/crates/adapteros-crypto/src/providers/`

### KeyManager (Unified Facade)
```rust
pub struct KeyManager {
    provider: Arc<RwLock<Box<dyn KeyProvider>>>,
    mode: KeyProviderMode,
    config: KeyManagerConfig,
}
```

**Key Precedence Order:**
1. Environment variable (`AOS_SIGNING_KEY`) - hex or base64 encoded
2. File path (requires `allow_insecure_keys = true`)
3. OS Keychain (macOS Keychain / Linux Secret Service)
4. KMS/HSM services

### KeyProvider Trait
```rust
#[async_trait]
pub trait KeyProvider: Send + Sync {
    async fn generate(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle>;
    async fn sign(&self, key_id: &str, msg: &[u8]) -> Result<Vec<u8>>;
    async fn seal(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>>;
    async fn unseal(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>>;
    async fn rotate(&self, key_id: &str) -> Result<RotationReceipt>;
    async fn attest(&self) -> Result<ProviderAttestation>;
}
```

### Supported Algorithms
```rust
pub enum KeyAlgorithm {
    Ed25519,           // Signing
    Aes256Gcm,         // Encryption
    ChaCha20Poly1305,  // Encryption (alternative)
}
```

### Provider Implementations
1. **FileProvider** (`providers/file.rs`): JSON keystore, 0600 permissions, dev only
2. **KeychainProvider** (`providers/keychain.rs`): 
   - macOS: Security Framework + Secure Enclave
   - Linux: Secret Service (D-Bus) or kernel keyring
   - Fallback: Password-based encrypted keystore (Argon2id KDF)
3. **EnvProvider** (`providers/env.rs`): From `AOS_SIGNING_KEY` env var
4. **KmsManager** (`providers/kms.rs`): External KMS/HSM integration

### Secure Memory Handling
`/Users/star/Dev/adapter-os/crates/adapteros-crypto/src/secret.rs`:
```rust
pub struct SecretKey<const N: usize>([u8; N]);  // ZeroizeOnDrop
pub struct KeyMaterial { inner: Vec<u8> }        // ZeroizeOnDrop
pub struct SensitiveData { inner: Vec<u8> }      // ZeroizeOnDrop
```
- Automatic zeroization on drop
- Debug output shows `[REDACTED]`
- Serialization intentionally fails (prevents accidental exposure)

## 4. Signature Verification

### Bundle Signing (`bundle_sign.rs`)
```rust
pub struct BundleSignature {
    pub bundle_hash: B3Hash,      // BLAKE3 of bundle content
    pub merkle_root: B3Hash,      // Merkle root of events
    pub signature: Signature,     // Ed25519 over bundle_hash
    pub public_key: PublicKey,
    pub schema_ver: u32,
    pub signed_at_us: u64,
    pub key_id: String,           // kid-{blake3(pubkey)[..32]}
}
```

**Key Functions:**
- `sign_bundle(bundle_hash, merkle_root, keypair) -> BundleSignature`
- `sign_and_save_bundle(...)` - Sign + atomic write to `var/signatures/`
- `verify_bundle_from_file(bundle_hash, signatures_dir)` - Load and verify
- `compute_key_id(public_key)` - Deterministic: `kid-{blake3(pubkey)[..32]}`

**Security:**
- Atomic file writes (temp + rename)
- File permissions: 0644 for signatures (public), 0600 for private keys
- Dev bypass via `AOS_DEV_SIGNATURE_BYPASS=1` (debug builds ONLY)

### Receipt Signing (`receipt_signing.rs`)
```rust
pub enum SigningMode {
    Production,   // Signing REQUIRED (fail-closed default)
    Development,  // Signing optional
}

pub struct SignedReceipt {
    pub digest: B3Hash,
    pub signature: Option<Signature>,
    pub public_key_hex: Option<String>,
    pub mode: SigningMode,
}
```

**Fail-Closed Semantics:**
- Production mode without keypair = error (not silent bypass)
- Emits telemetry event on signing failures
- Environment: `AOS_SIGNING_MODE=development` for testing

### Tenant-Bound Receipts (Patent 3535886.0002)
```rust
pub struct TenantBoundReceipt {
    pub receipt: SignedReceipt,
    pub tenant_id: String,
    pub tenant_binding_mac: String,  // HMAC-SHA256(digest || tenant_id)
    pub bound_at: String,
}
```
- HMAC binding ensures receipts are cryptographically tied to tenants
- Key derivation: `derive_tenant_key(master_key, tenant_id)` using HKDF-SHA256
- Constant-time MAC comparison

## 5. Adapter Signing and Audit Trails

### Decision Chain (`decision_chain.rs`)
Cryptographic audit trail for router decisions:
```rust
pub struct RouterEventDigest {
    pub step: usize,
    pub adapter_indices: Vec<u16>,
    pub gates_q15: Vec<i16>,
    pub entropy_q15: i16,
    pub policy_mask_digest_b3: Option<B3Hash>,
    pub adapter_training_digests: Option<Vec<B3Hash>>,  // Patent rectification
    pub previous_hash: Option<B3Hash>,  // Hash chain
}
```

**DecisionChainBuilder:**
- Maintains hash chain linking events
- `finalize()` computes Merkle root of all events
- `verify_chain()` validates hash chain integrity

### MerkleBundleCommits
```rust
pub struct MerkleBundleCommits {
    pub request_hash: B3Hash,
    pub manifest_hash: Option<B3Hash>,
    pub adapter_stack_stable_ids: Vec<String>,
    pub decision_chain_hash: B3Hash,
    pub backend_identity_hash: B3Hash,
    pub model_identity_hash: Option<B3Hash>,
}
```
- Combined hash signed in bundle signature
- Leaf hashes for Merkle tree: request, decision chain, backend identity, manifest, model, adapter IDs

### Crypto Audit Logger (`audit.rs`)
```rust
pub struct CryptoAuditEntry {
    pub operation: CryptoOperation,  // Encrypt, Decrypt, Sign, Verify, KeyGenerate, etc.
    pub key_id: Option<String>,
    pub result: OperationResult,
    pub signature: Vec<u8>,          // Ed25519 signature for tamper detection
    pub entry_hash: Option<Vec<u8>>, // BLAKE3 for chain integrity
    pub previous_hash: Option<Vec<u8>>,
    pub chain_sequence: Option<u64>,
}
```
- Immutable append-only log
- Each entry signed and hash-chained
- Queryable by operation, key_id, user_id, time range, result

## 6. Key Rotation

### Rotation Daemon (`rotation_daemon.rs`)
```rust
pub struct RotationPolicy {
    pub rotation_interval_secs: u64,  // Default: 90 days
    pub grace_period_secs: u64,       // Default: 7 days
    pub max_historical_keys: usize,   // Default: 10
    pub auto_rotate: bool,
}
```

**Architecture:**
- KEK (Key Encryption Key) encrypts DEKs
- DEK (Data Encryption Key) encrypts actual data
- Rotation: Generate new KEK -> Re-encrypt all DEKs -> Archive old KEK

**CryptoStore Trait:**
- `list_encrypted_deks()` - Get DEKs for re-encryption
- `update_dek_atomic()` - CAS update for concurrent safety

### Rotation Receipt
```rust
pub struct RotationReceipt {
    pub key_id: String,
    pub previous_key: KeyHandle,
    pub new_key: KeyHandle,
    pub timestamp: u64,
    pub signature: Vec<u8>,  // Ed25519 signed
}
```

## 7. Policy Enforcement (`policy_enforcement.rs`)

### CryptoPolicy
```rust
pub struct CryptoPolicy {
    pub approved_algorithms: HashSet<String>,  // ed25519, aes256gcm, chacha20poly1305
    pub banned_algorithms: HashSet<String>,    // md5, sha1, des, 3des, rc4
    pub min_key_sizes: HashMap<String, u32>,
    pub max_key_ages: HashMap<String, u64>,
    pub fips_mode: bool,
    pub require_hardware_backing: bool,
}
```

**Violation Types:**
- BannedAlgorithm, UnapprovedAlgorithm, InsufficientKeySize
- KeyAgeExceeded, UnpermittedOperation, FipsViolation, HardwareBackingRequired

## 8. Secure Enclave (SEP) Attestation

### macOS Hardware Security (`sep_attestation.rs`)
```rust
pub struct SepAttestation {
    pub public_key: Vec<u8>,           // P-256 ECDSA
    pub certificate_chain: Vec<Vec<u8>>, // X.509 DER
    pub nonce: Vec<u8>,
    pub chip_generation: SepChipGeneration,  // M1/M2/M3/M4
}
```

**Features:**
- Hardware-backed key generation (private keys never leave SEP)
- Attestation chain verification against Apple Root CA
- Graceful fallback on Intel Macs
- Configurable via `AOS_SEP_ROOT_CA_PATH`

## Security Considerations Summary

1. **Fail-Closed Defaults**: Production mode requires signing; missing keys = error
2. **Constant-Time Operations**: Signature verification, MAC comparison
3. **Memory Safety**: Zeroize-on-drop for all sensitive material
4. **Audit Trail**: Signed, hash-chained entries for tamper detection
5. **File Permissions**: 0600 for private keys, atomic writes
6. **Dev/Prod Separation**: Bypass flags only work in debug builds
7. **Key Isolation**: Keychain/SEP preferred over file-based storage
8. **Determinism**: JCS canonicalization for reproducible hashes
9. **Schema Versioning**: Future-proof signature and receipt formats
10. **Multi-Tenant Isolation**: HMAC-bound receipts with derived tenant keys
