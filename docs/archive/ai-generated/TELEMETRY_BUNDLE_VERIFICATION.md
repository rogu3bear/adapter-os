# Telemetry Bundle Verification Flow

**Purpose:** Document the cryptographic signing and verification flow for telemetry bundles in AdapterOS.

**Last Updated:** 2025-11-16

---

## Table of Contents

- [Overview](#overview)
- [Ed25519 Keypair Storage](#ed25519-keypair-storage)
- [Bundle Signing Flow](#bundle-signing-flow)
- [Metadata Structure](#metadata-structure)
- [Verification Flow](#verification-flow)
- [Complete Workflow](#complete-workflow)
- [Security Properties](#security-properties)
- [Testing](#testing)
- [References](#references)

---

## Overview

AdapterOS telemetry bundles are cryptographically signed using Ed25519 digital signatures to ensure:

1. **Integrity:** Bundles cannot be tampered with after signing
2. **Authenticity:** Bundles can be verified as originating from a specific node
3. **Non-repudiation:** Signed bundles provide cryptographic proof of origin
4. **Verifiability:** Bundles can be verified without access to the signing keypair

**Key Insight:** The Ed25519 public key is embedded in `bundle.meta.json`, enabling verification across process restarts and node boundaries without requiring the private signing key.

---

## Ed25519 Keypair Storage

### Location

**Path:** `var/keys/telemetry_signing.key`

**Format:** 32-byte raw Ed25519 secret key (binary)

**Permissions:** `0o600` (owner read/write only)

### Generation and Loading Logic

**Source:** `crates/adapteros-telemetry/src/lib.rs:72-123`

```rust
fn load_or_generate_signing_key(key_path: &Path) -> Result<Keypair> {
    if key_path.exists() {
        // Load existing 32-byte key
        let key_bytes = fs::read(key_path)
            .map_err(|e| AosError::Io(format!("Failed to read key: {}", e)))?;

        // Validate length
        if key_bytes.len() != 32 {
            return Err(AosError::Crypto(
                format!("Invalid key length: expected 32, got {}", key_bytes.len())
            ));
        }

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);
        Ok(Keypair::from_bytes(&key_array))
    } else {
        // Generate new keypair using cryptographically secure RNG
        let keypair = Keypair::generate();

        // Create parent directory if needed
        if let Some(parent) = key_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Save to disk
        fs::write(key_path, keypair.to_bytes())?;

        // Set Unix permissions to 0o600 (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(key_path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(key_path, perms)?;
        }

        info!("Generated new Ed25519 signing keypair at {}", key_path.display());
        Ok(keypair)
    }
}
```

### Initialization

**Source:** `crates/adapteros-telemetry/src/lib.rs:153-154`

```rust
pub fn new(config: TelemetryConfig) -> Result<Self> {
    // ...
    let signing_keypair = load_or_generate_signing_key(&key_path)?;
    // Keypair stored in background thread for signing
}
```

**Key Properties:**

- **Persistent:** Keypair survives process restarts
- **Auto-generated:** Created on first run if not found
- **Secure storage:** 0o600 permissions prevent unauthorized access
- **Single instance:** One keypair per AdapterOS node

---

## Bundle Signing Flow

### Signing Target: Merkle Root

AdapterOS signs the **Merkle root** of event hashes, not individual events. This provides:

- **Efficient verification:** Single signature covers all events
- **Content integrity:** Any event modification changes the Merkle root
- **Deterministic:** Same events always produce same Merkle root

### Merkle Root Computation

**Source:** `crates/adapteros-telemetry/src/lib.rs:377-406`

```rust
fn finalize_bundle(
    path: &Path,
    event_hashes: &[B3Hash],
    signing_keypair: &Keypair,
) -> Result<()> {
    // 1. Compute Merkle root from event hashes
    let merkle_root = if event_hashes.is_empty() {
        B3Hash::hash(b"empty")
    } else {
        // Combine all event hashes
        let mut combined = Vec::new();
        for hash in event_hashes {
            combined.extend_from_slice(hash.as_bytes());
        }
        // Hash the combined bytes to create merkle root
        B3Hash::hash(&combined)
    };

    // 2. Sign the merkle root
    let signature = sign_bundle_merkle_root(&merkle_root, signing_keypair)?;
    let public_key = signing_keypair.public_key();

    // 3. Write metadata file (.meta.json)
    let meta_path = path.with_extension("meta.json");
    let mut meta_file = fs::File::create(&meta_path)?;
    let metadata = BundleMetadata {
        event_count: event_hashes.len(),
        merkle_root,
        signature: Some(hex::encode(&signature)),
        public_key: Some(hex::encode(public_key.to_bytes())),
    };
    serde_json::to_writer_pretty(&mut meta_file, &metadata)?;

    Ok(())
}
```

### Signing Function

**Source:** `crates/adapteros-telemetry/src/lib.rs:408-416`

```rust
fn sign_bundle_merkle_root(merkle_root: &B3Hash, keypair: &Keypair) -> Result<Vec<u8>> {
    // Sign the 32-byte merkle root with Ed25519
    let signature = keypair.sign(merkle_root.as_bytes());
    Ok(signature.to_bytes().to_vec())
}
```

**Ed25519 Signature Properties:**

- **Signature size:** 64 bytes (constant)
- **Deterministic:** Same message + keypair = same signature
- **Fast:** ~40,000 signatures/second on modern hardware
- **Secure:** 128-bit security level (equivalent to 256-bit symmetric key)

### When Signing Occurs

**Source:** `crates/adapteros-telemetry/src/lib.rs:358-359`

```rust
// Called when bundle rotates (max events/bytes reached)
finalize_bundle(&current_bundle_path, &event_hashes, &self.signing_keypair)?;
```

**Rotation Triggers:**

- Max events reached (default: 10,000)
- Max bytes reached (default: 10 MB)
- Manual flush requested
- Writer shutdown

---

## Metadata Structure

### Schema

**Source:** `crates/adapteros-telemetry/src/lib.rs:452-458`

```rust
#[derive(Debug, Serialize, Deserialize)]
struct BundleMetadata {
    event_count: usize,
    merkle_root: B3Hash,
    signature: Option<String>,  // Ed25519 signature (64 bytes hex)
    public_key: Option<String>, // Ed25519 public key (32 bytes hex)
}
```

### File Naming Convention

```
var/tenant-a/bundles/
├── bundle_000000.ndjson       # Event data (NDJSON format)
└── bundle_000000.meta.json    # Metadata with signature
```

### Example Metadata File

```json
{
  "event_count": 42,
  "merkle_root": "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456",
  "signature": "e5f6g7h8i9j0k1l2m3n4o5p6q7r8s9t0u1v2w3x4y5z6a7b8c9d0e1f2g3h4i5j6k7l8m9n0o1p2q3r4s5t6u7v8w9x0y1z2",
  "public_key": "i9j0k1l2m3n4o5p6q7r8s9t0u1v2w3x4y5z6a7b8c9d0e1f2g3h4i5j6k7l8"
}
```

### Critical Field: `public_key`

**Purpose:** Enable signature verification without access to the signing keypair

**Why It's Needed:**

- Verification can occur after process restart
- Remote nodes can verify bundles without the private key
- Compliance/audit tools can verify bundles independently
- No need to distribute or store the private signing key

**Encoding:** Hex-encoded 32-byte Ed25519 public key

---

## Verification Flow

### Verification Function

**Source:** `crates/adapteros-telemetry/src/lib.rs:418-450`

```rust
pub fn verify_bundle_signature(
    merkle_root: &B3Hash,
    signature_hex: &str,
    public_key_hex: &str,
) -> Result<bool> {
    use adapteros_crypto::{PublicKey, Signature};

    // 1. Decode hex strings to bytes
    let signature_bytes = hex::decode(signature_hex)
        .map_err(|e| AosError::Validation(format!("Invalid signature hex: {}", e)))?;
    let public_key_bytes = hex::decode(public_key_hex)
        .map_err(|e| AosError::Validation(format!("Invalid public key hex: {}", e)))?;

    // 2. Validate lengths
    if signature_bytes.len() != 64 {
        return Err(AosError::Validation(
            format!("Invalid signature length: expected 64, got {}", signature_bytes.len())
        ));
    }
    if public_key_bytes.len() != 32 {
        return Err(AosError::Validation(
            format!("Invalid public key length: expected 32, got {}", public_key_bytes.len())
        ));
    }

    // 3. Convert to Ed25519 types
    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(&signature_bytes);
    let signature = Signature::from_bytes(&sig_array)?;

    let mut pk_array = [0u8; 32];
    pk_array.copy_from_slice(&public_key_bytes);
    let public_key = PublicKey::from_bytes(&pk_array)?;

    // 4. Verify signature using Ed25519 (constant-time)
    public_key.verify(merkle_root.as_bytes(), &signature)?;

    Ok(true)
}
```

### Verification Requirements

**Inputs (all from `bundle.meta.json`):**

1. `merkle_root` - BLAKE3 hash to verify against
2. `signature` - Ed25519 signature (hex)
3. `public_key` - Ed25519 public key (hex)

**Does NOT require:**

- Signing keypair (`var/keys/telemetry_signing.key`)
- In-memory state from TelemetryWriter
- Access to original bundle events

### Security Properties

**Constant-time verification:**

- Prevents timing attacks
- Implemented by `ed25519_dalek` crate
- All verification paths take same amount of time

**Cryptographic guarantees:**

- Signature validity proves message was signed by holder of private key
- Signature cannot be forged without private key
- Message cannot be modified without detection
- Public key cannot be substituted (changes signature validity)

---

## Complete Workflow

```
┌─────────────────────────────────────────────────────┐
│ 1. Initialization (Process Start)                   │
├─────────────────────────────────────────────────────┤
│ • Load or generate keypair from:                    │
│   var/keys/telemetry_signing.key (32 bytes)         │
│ • Store keypair in TelemetryWriter background thread│
│ • Keypair persists for entire process lifetime      │
└─────────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────────┐
│ 2. Event Collection                                  │
├─────────────────────────────────────────────────────┤
│ • Events written to bundle_000000.ndjson (NDJSON)    │
│ • Event hashes computed and collected in memory      │
│ • Bundle rotates when max_events/max_bytes reached   │
└─────────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────────┐
│ 3. Bundle Finalization (On Rotation)                │
├─────────────────────────────────────────────────────┤
│ • Compute Merkle root from event hashes              │
│   - Empty bundle: hash(b"empty")                     │
│   - Non-empty: hash(hash1 || hash2 || ... || hashN) │
│ • Sign merkle_root with Ed25519 keypair              │
│   - signature = keypair.sign(merkle_root.as_bytes()) │
│ • Extract public key from keypair                    │
│   - public_key = keypair.public_key()                │
│ • Create BundleMetadata:                             │
│   - merkle_root (BLAKE3 hash, 32 bytes)              │
│   - signature (Ed25519, 64 bytes hex)                │
│   - public_key (Ed25519, 32 bytes hex)               │
│   - event_count, sequence_no                         │
│ • Write bundle_000000.meta.json to disk              │
└─────────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────────┐
│ 4. Verification (No Keypair Required)                │
├─────────────────────────────────────────────────────┤
│ • Load bundle_000000.meta.json from disk             │
│ • Extract fields:                                    │
│   - merkle_root (message to verify)                  │
│   - signature (Ed25519 signature)                    │
│   - public_key (Ed25519 public key)                  │
│ • Decode hex strings to byte arrays                  │
│ • Call: public_key.verify(merkle_root, signature)    │
│ • Constant-time Ed25519 verification                 │
│ • Return: Ok(true) or Err(verification failed)       │
└─────────────────────────────────────────────────────┘
```

### Data Flow Diagram

```
[TelemetryWriter]
       ↓
   (keypair in memory)
       ↓
   Events → bundle_000000.ndjson
       ↓
   Event hashes → Merkle root
       ↓
   Sign(merkle_root, keypair) → signature
       ↓
   keypair.public_key() → public_key
       ↓
   BundleMetadata {
     merkle_root,
     signature,      ← Ed25519 signature (64 bytes hex)
     public_key,     ← Ed25519 public key (32 bytes hex)
   }
       ↓
   bundle_000000.meta.json
       ↓
   [Disk Storage]
       ↓
   [Verification] (reads from disk)
       ↓
   verify(merkle_root, signature, public_key)
       ↓
   Ok(true) ✅ or Err(verification failed) ❌
```

---

## Security Properties

### Integrity Protection

**Property:** Bundles cannot be modified after signing

**Mechanism:**

1. Any event modification changes its hash
2. Changed hash changes the Merkle root
3. Changed Merkle root invalidates the signature
4. Verification detects the tampering

**Attack Resistance:**

- ❌ Cannot modify events without detection
- ❌ Cannot reorder events without detection
- ❌ Cannot add/remove events without detection
- ❌ Cannot substitute public key (invalidates signature)

### Authenticity

**Property:** Bundles can be verified as originating from a specific node

**Mechanism:**

- Each node has unique Ed25519 keypair
- Public key embedded in metadata identifies the signer
- Signature proves bundle was signed by holder of private key

**Use Cases:**

- Multi-node deployments: identify which node produced which bundle
- Audit trails: cryptographic proof of bundle origin
- Compliance: demonstrate chain of custody

### Non-Repudiation

**Property:** Signer cannot deny having signed the bundle

**Mechanism:**

- Ed25519 signatures are non-repudiable
- Only holder of private key can produce valid signature
- Public key proves which key signed the bundle

**Legal/Compliance:**

- Audit logs with cryptographic proof
- Incident investigation with tamper evidence
- Regulatory compliance (GDPR, HIPAA, SOC2)

### Verifiability Without Private Key

**Property:** Anyone with `bundle.meta.json` can verify the bundle

**Mechanism:**

- Public key embedded in metadata (no private key needed)
- Verification uses only public information
- Enables distributed verification

**Benefits:**

- Remote nodes can verify bundles
- Compliance tools can audit bundles
- Verifiable across process restarts
- No need to distribute private signing key

---

## Testing

### Integration Tests

**Location:** `tests/telemetry_signature_verification.rs`

**Test Coverage:**

1. **Basic Signing and Verification**
   - Create bundle with events
   - Verify signature with correct metadata
   - Test assertion: signature verification succeeds

2. **Verification After Process Restart**
   - Write bundle and drop TelemetryWriter (keypair lost)
   - Load metadata from disk
   - Verify signature with only public key
   - Test assertion: verification works without in-memory keypair

3. **Chain Verification**
   - Create multiple bundles with `prev_bundle_hash` links
   - Verify each bundle's signature
   - Verify chain integrity
   - Test assertion: all signatures valid, chain unbroken

4. **Failure Cases** (see `tests/telemetry_signature_verification_strict.rs`)
   - Tampered signature
   - Wrong public key
   - Modified merkle root
   - Missing public key in metadata
   - Test assertion: verification fails with descriptive errors

### Running Tests

```bash
# Run all telemetry signature tests
cargo test --test telemetry_signature_verification

# Run strict failure tests
cargo test --test telemetry_signature_verification_strict

# Run with output
cargo test --test telemetry_signature_verification -- --nocapture
```

### Test Assertions

```rust
// ✅ Valid signature should verify
assert!(verify_bundle_signature(&merkle_root, &signature, &public_key).is_ok());

// ❌ Tampered signature should fail
let tampered_sig = "0000000000000000000000000000000000000000000000000000000000000000";
assert!(verify_bundle_signature(&merkle_root, tampered_sig, &public_key).is_err());

// ❌ Wrong public key should fail
let wrong_pk = "1111111111111111111111111111111111111111111111111111111111111111";
assert!(verify_bundle_signature(&merkle_root, &signature, wrong_pk).is_err());
```

---

## References

### Code Locations

- **Keypair loading:** `crates/adapteros-telemetry/src/lib.rs:72-123`
- **Signing function:** `crates/adapteros-telemetry/src/lib.rs:408-416`
- **Bundle finalization:** `crates/adapteros-telemetry/src/lib.rs:377-406`
- **Verification function:** `crates/adapteros-telemetry/src/lib.rs:418-450`
- **Metadata schema:** `crates/adapteros-telemetry/src/lib.rs:452-458`
- **Advanced crypto:** `crates/adapteros-crypto/src/bundle_sign.rs`
- **Ed25519 primitives:** `crates/adapteros-crypto/src/signature.rs`

### Dependencies

- **`ed25519-dalek`** - Ed25519 signatures (constant-time implementation)
- **`blake3`** - BLAKE3 hashing for content addressing
- **`hex`** - Hex encoding/decoding for metadata
- **`serde_json`** - JSON serialization for metadata

### Related Documentation

- **CLAUDE.md** - Developer guide and coding standards
- **CONTRIBUTING.md** - Contribution guidelines
- **docs/ARCHITECTURE_INDEX.md** - Complete architecture reference
- **crates/adapteros-policy/src/packs/artifacts.rs** - Artifacts Ruleset #13 (Ed25519 signing)

### External Resources

- **Ed25519:** [RFC 8032](https://tools.ietf.org/html/rfc8032)
- **BLAKE3:** [BLAKE3 specification](https://github.com/BLAKE3-team/BLAKE3-specs)
- **Digital Signatures:** [NIST FIPS 186-4](https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.186-4.pdf)

---

**Last Updated:** 2025-11-16
**Maintainer:** AdapterOS Core Team
**Version:** 1.0
