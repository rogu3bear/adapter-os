# PR-004: Root Seed Digest in Receipt

## Summary

Bind receipts to their seed lineage by including `root_seed_digest` (BLAKE3 hash of request seed) in the receipt, enabling detection of seed manipulation without exposing raw seed material.

## Problem Statement

Current receipts do not bind to the seed used for inference. This creates vulnerabilities:

1. **Seed substitution**: Attacker could claim a receipt was produced with a different seed
2. **Replay undetectability**: Cannot verify if a receipt was produced with the expected seed
3. **Forensic gap**: Cannot trace determinism failures back to seed derivation

The seed itself cannot be included (security risk - enables replay attacks), but a digest provides cryptographic binding without exposure.

## Solution

1. Add `root_seed_digest_hex` field to `ReceiptDigests`
2. Compute digest as `BLAKE3(request_seed)` at inference start
3. Include digest in receipt hash computation (v2+ schema)
4. Verify digest matches expected value during receipt verification

---

## Implementation Details

### File Changes

#### 1. `crates/adapteros-core/src/seed.rs`

**Add seed digest computation helper**:

```rust
/// Compute a digest of a seed for receipt binding.
///
/// This produces a BLAKE3 hash of the seed bytes, suitable for
/// inclusion in receipts without exposing the raw seed.
///
/// # Security Note
///
/// The digest is one-way: the original seed cannot be recovered.
/// However, if an attacker knows the seed, they can verify the digest.
/// This is acceptable since the digest is for binding, not secrecy.
pub fn compute_seed_digest(seed: &[u8; 32]) -> B3Hash {
    B3Hash::hash(seed)
}

/// Compute a seed digest from a TypedSeed.
pub fn compute_typed_seed_digest(seed: &TypedSeed) -> B3Hash {
    B3Hash::hash(seed.bytes())
}

/// Seed lineage information for receipt binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedLineage {
    /// BLAKE3 digest of the request seed (never raw seed)
    pub root_seed_digest: B3Hash,
    /// Seed mode used for derivation
    pub seed_mode: SeedMode,
    /// Whether seed was derived from manifest hash
    pub has_manifest_binding: bool,
    /// HKDF algorithm version used
    pub hkdf_version: u32,
}

impl SeedLineage {
    /// Create seed lineage from a TypedSeed and derivation context.
    pub fn from_typed_seed(
        seed: &TypedSeed,
        mode: SeedMode,
        has_manifest: bool,
    ) -> Self {
        Self {
            root_seed_digest: compute_typed_seed_digest(seed),
            seed_mode: mode,
            has_manifest_binding: has_manifest,
            hkdf_version: seed.version,
        }
    }

    /// Verify that a seed matches this lineage.
    pub fn verify_seed(&self, seed: &[u8; 32]) -> bool {
        let digest = B3Hash::hash(seed);
        digest == self.root_seed_digest
    }
}
```

#### 2. `crates/adapteros-cli/src/commands/verify_receipt.rs`

**Update `ReceiptDigests` struct**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceiptDigests {
    // ... existing fields ...

    // NEW: Root seed digest (v3+)
    #[serde(default)]
    root_seed_digest_hex: Option<String>,

    // NEW: Seed derivation metadata
    #[serde(default)]
    seed_mode: Option<String>,

    // NEW: Whether manifest was used in seed derivation
    #[serde(default)]
    has_manifest_binding: Option<bool>,
}

/// Receipt schema versions
pub const RECEIPT_SCHEMA_V1: u8 = 1;
pub const RECEIPT_SCHEMA_V2: u8 = 2;
pub const RECEIPT_SCHEMA_V3: u8 = 3;  // NEW: Adds seed digest
pub const RECEIPT_SCHEMA_CURRENT: u8 = RECEIPT_SCHEMA_V3;
```

**Add new `ReasonCode`**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReasonCode {
    // ... existing codes ...
    SeedDigestMismatch,
    SeedModeViolation,
}
```

**Update `compute_receipt_digest` function**:

```rust
fn compute_receipt_digest(
    context_digest: &B3Hash,
    run_head: &B3Hash,
    output_digest: &B3Hash,
    receipt: &ReceiptDigests,
    bundle: &ReceiptBundle,
) -> B3Hash {
    match receipt.schema_version {
        RECEIPT_SCHEMA_V1 => {
            // V1: Original
            // ...
        }
        RECEIPT_SCHEMA_V2 => {
            // V2: + backend identity
            // ...
        }
        RECEIPT_SCHEMA_V3 => {
            // V3: + seed digest
            let backend_bytes = bundle.backend_used
                .as_deref()
                .unwrap_or("")
                .as_bytes();

            let attestation_bytes = bundle.backend_attestation_b3_hex
                .as_ref()
                .and_then(|h| hex::decode(h).ok())
                .unwrap_or_default();

            let seed_digest_bytes = receipt.root_seed_digest_hex
                .as_ref()
                .and_then(|h| hex::decode(h).ok())
                .unwrap_or_else(|| vec![0u8; 32]);

            let seed_mode_bytes = receipt.seed_mode
                .as_deref()
                .unwrap_or("unknown")
                .as_bytes();

            let manifest_binding_byte = if receipt.has_manifest_binding.unwrap_or(false) {
                [1u8]
            } else {
                [0u8]
            };

            B3Hash::hash_multi(&[
                context_digest.as_bytes(),
                run_head.as_bytes(),
                output_digest.as_bytes(),
                &receipt.logical_prompt_tokens.to_le_bytes(),
                &receipt.prefix_cached_token_count.to_le_bytes(),
                &receipt.billed_input_tokens.to_le_bytes(),
                &receipt.logical_output_tokens.to_le_bytes(),
                &receipt.billed_output_tokens.to_le_bytes(),
                // V2 additions
                &[RECEIPT_SCHEMA_V3],
                &(backend_bytes.len() as u32).to_le_bytes(),
                backend_bytes,
                &(attestation_bytes.len() as u32).to_le_bytes(),
                &attestation_bytes,
                // V3 additions
                &seed_digest_bytes,
                &(seed_mode_bytes.len() as u32).to_le_bytes(),
                seed_mode_bytes,
                &manifest_binding_byte,
            ])
        }
        _ => B3Hash::zero(),
    }
}
```

**Add seed digest verification**:

```rust
/// Verify seed digest if expected seed is provided.
fn verify_seed_binding(
    bundle: &ReceiptBundle,
    expected_seed: Option<&[u8; 32]>,
) -> Option<ReasonCode> {
    // Only verify if we have both expected seed and receipt claims seed
    let Some(expected) = expected_seed else {
        return None;
    };

    let Some(ref claimed_digest) = bundle.receipt.root_seed_digest_hex else {
        // Receipt doesn't claim a seed - can't verify
        return None;
    };

    let expected_digest = compute_seed_digest(expected);
    let expected_hex = expected_digest.to_hex();

    if &expected_hex != claimed_digest {
        Some(ReasonCode::SeedDigestMismatch)
    } else {
        None
    }
}
```

#### 3. `crates/adapteros-lora-worker/src/generation.rs`

**Capture seed lineage at inference start**:

```rust
use adapteros_core::seed::{SeedLineage, TypedSeed};

pub struct InferenceContext {
    // ... existing fields ...

    /// Seed lineage for receipt binding
    seed_lineage: Option<SeedLineage>,
}

impl InferenceContext {
    pub fn new_with_seed(
        typed_seed: TypedSeed,
        seed_mode: SeedMode,
        has_manifest: bool,
    ) -> Self {
        let lineage = SeedLineage::from_typed_seed(&typed_seed, seed_mode, has_manifest);

        Self {
            // ... existing initialization ...
            seed_lineage: Some(lineage),
        }
    }

    /// Get seed lineage for receipt generation.
    pub fn seed_lineage(&self) -> Option<&SeedLineage> {
        self.seed_lineage.as_ref()
    }
}
```

#### 4. `crates/adapteros-lora-worker/src/evidence.rs`

**Include seed lineage in receipt bundle**:

```rust
impl InferenceEvidenceBuilder {
    pub fn with_seed_lineage(mut self, lineage: &SeedLineage) -> Self {
        self.root_seed_digest = Some(lineage.root_seed_digest);
        self.seed_mode = Some(lineage.seed_mode);
        self.has_manifest_binding = lineage.has_manifest_binding;
        self
    }

    pub fn build_receipt_bundle(self) -> ReceiptBundle {
        ReceiptBundle {
            // ... existing fields ...
            receipt: ReceiptDigests {
                schema_version: RECEIPT_SCHEMA_V3,
                // ... existing fields ...
                root_seed_digest_hex: self.root_seed_digest.map(|h| h.to_hex()),
                seed_mode: self.seed_mode.map(|m| m.as_str().to_string()),
                has_manifest_binding: Some(self.has_manifest_binding),
            },
        }
    }
}
```

#### 5. `crates/adapteros-cli/src/commands/verify_receipt.rs`

**Add CLI flag for expected seed verification**:

```rust
#[derive(Debug, Args)]
pub struct VerifyReceiptArgs {
    /// Path to receipt bundle JSON file
    #[arg(long)]
    bundle: PathBuf,

    /// Expected seed (hex) for digest verification
    #[arg(long)]
    expected_seed_hex: Option<String>,

    /// Require seed digest in receipt (fail if missing)
    #[arg(long)]
    require_seed_digest: bool,
}

pub async fn run(args: VerifyReceiptArgs) -> Result<()> {
    let bundle = load_bundle(&args.bundle)?;

    // Parse expected seed if provided
    let expected_seed: Option<[u8; 32]> = args.expected_seed_hex
        .as_ref()
        .map(|hex| {
            let bytes = hex::decode(hex)?;
            if bytes.len() != 32 {
                anyhow::bail!("Expected seed must be 32 bytes (64 hex chars)");
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            Ok(arr)
        })
        .transpose()?;

    let mut report = verify_bundle(&bundle)?;

    // Verify seed binding if expected seed provided
    if let Some(reason) = verify_seed_binding(&bundle, expected_seed.as_ref()) {
        report.reasons.push(reason);
    }

    // Check seed digest requirement
    if args.require_seed_digest && bundle.receipt.root_seed_digest_hex.is_none() {
        report.reasons.push(ReasonCode::SeedDigestMismatch);
        report.warnings.push("Receipt missing seed digest".to_string());
    }

    // Output report
    output_report(&report)?;

    if report.reasons.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("Receipt verification failed")
    }
}
```

---

## Acceptance Criteria

- [ ] `ReceiptDigests` includes `root_seed_digest_hex: Option<String>`
- [ ] Seed digest computed as `BLAKE3(request_seed)` - never raw bytes
- [ ] Schema v3 receipt includes seed digest in hash computation
- [ ] V1/V2 receipts without seed digest still verify (backward compat)
- [ ] `SeedLineage` struct captures all seed derivation context
- [ ] Worker populates seed lineage from inference context
- [ ] CLI `--expected-seed-hex` flag enables digest verification
- [ ] CLI `--require-seed-digest` fails if receipt lacks digest
- [ ] Different seeds produce different receipts (verified in tests)

---

## Test Plan

### Unit Tests

**File**: `crates/adapteros-core/tests/seed_lineage_tests.rs`

```rust
#[test]
fn test_seed_digest_deterministic() {
    let seed = [42u8; 32];
    let digest1 = compute_seed_digest(&seed);
    let digest2 = compute_seed_digest(&seed);
    assert_eq!(digest1, digest2);
}

#[test]
fn test_different_seeds_different_digests() {
    let seed1 = [1u8; 32];
    let seed2 = [2u8; 32];
    let digest1 = compute_seed_digest(&seed1);
    let digest2 = compute_seed_digest(&seed2);
    assert_ne!(digest1, digest2);
}

#[test]
fn test_seed_lineage_verification() {
    let seed = [42u8; 32];
    let typed_seed = TypedSeed::new(seed);
    let lineage = SeedLineage::from_typed_seed(&typed_seed, SeedMode::Strict, true);

    assert!(lineage.verify_seed(&seed));
    assert!(!lineage.verify_seed(&[0u8; 32]));
}

#[test]
fn test_seed_lineage_serialization() {
    let seed = [42u8; 32];
    let typed_seed = TypedSeed::new(seed);
    let lineage = SeedLineage::from_typed_seed(&typed_seed, SeedMode::Strict, true);

    let json = serde_json::to_string(&lineage).unwrap();
    let deserialized: SeedLineage = serde_json::from_str(&json).unwrap();

    assert_eq!(lineage.root_seed_digest, deserialized.root_seed_digest);
    assert_eq!(lineage.seed_mode, deserialized.seed_mode);
}
```

### Receipt Verification Tests

**File**: `crates/adapteros-cli/tests/verify_receipt_seed_tests.rs`

```rust
#[test]
fn test_v3_receipt_with_seed_digest() {
    let seed = [42u8; 32];
    let bundle = create_v3_bundle_with_seed(&seed);

    let report = verify_bundle(&bundle).unwrap();
    assert!(report.reasons.is_empty());
    assert!(bundle.receipt.root_seed_digest_hex.is_some());
}

#[test]
fn test_seed_digest_mismatch_detected() {
    let seed = [42u8; 32];
    let bundle = create_v3_bundle_with_seed(&seed);

    // Verify with wrong expected seed
    let wrong_seed = [0u8; 32];
    let reason = verify_seed_binding(&bundle, Some(&wrong_seed));

    assert!(matches!(reason, Some(ReasonCode::SeedDigestMismatch)));
}

#[test]
fn test_same_input_different_seed_different_receipt() {
    let seed1 = [1u8; 32];
    let seed2 = [2u8; 32];

    let bundle1 = create_v3_bundle_with_seed(&seed1);
    let bundle2 = create_v3_bundle_with_seed(&seed2);

    // Same context, different seed -> different receipt digest
    assert_eq!(bundle1.context.tenant_namespace, bundle2.context.tenant_namespace);
    assert_ne!(bundle1.receipt.receipt_digest_hex, bundle2.receipt.receipt_digest_hex);
}

#[test]
fn test_v2_receipt_backward_compat_no_seed() {
    let bundle = create_v2_bundle(); // No seed digest

    let report = verify_bundle(&bundle).unwrap();
    assert!(report.reasons.is_empty()); // Should still pass

    assert!(bundle.receipt.root_seed_digest_hex.is_none());
}

#[test]
fn test_require_seed_digest_flag() {
    let bundle = create_v2_bundle(); // No seed digest

    // Without flag: passes
    let report1 = verify_bundle_with_options(&bundle, VerifyOptions::default()).unwrap();
    assert!(report1.reasons.is_empty());

    // With flag: fails
    let report2 = verify_bundle_with_options(&bundle, VerifyOptions {
        require_seed_digest: true,
        ..Default::default()
    }).unwrap();
    assert!(!report2.reasons.is_empty());
}
```

### Integration Tests

**File**: `tests/seed_lineage_integration.rs`

```rust
#[tokio::test]
async fn test_inference_produces_v3_receipt_with_seed() {
    let server = start_test_server().await;

    let response = server.post("/v1/inference")
        .json(&inference_request())
        .send()
        .await;

    let receipt = server.get(&format!("/v1/trace/{}/receipt", response.trace_id))
        .send()
        .await
        .json::<ReceiptBundle>();

    assert_eq!(receipt.receipt.schema_version, RECEIPT_SCHEMA_V3);
    assert!(receipt.receipt.root_seed_digest_hex.is_some());
    assert!(receipt.receipt.seed_mode.is_some());
}

#[tokio::test]
async fn test_deterministic_replay_same_seed_same_receipt() {
    let server = start_test_server().await;
    let request = inference_request_with_fixed_seed([42u8; 32]);

    // Run twice with same seed
    let receipt1 = run_inference_get_receipt(&server, &request).await;
    let receipt2 = run_inference_get_receipt(&server, &request).await;

    // Should produce identical receipt digests
    assert_eq!(
        receipt1.receipt.receipt_digest_hex,
        receipt2.receipt.receipt_digest_hex
    );
    assert_eq!(
        receipt1.receipt.root_seed_digest_hex,
        receipt2.receipt.root_seed_digest_hex
    );
}
```

---

## Security Considerations

1. **Seed digest is one-way**: Cannot recover seed from digest
2. **No raw seed in receipt**: Only BLAKE3 hash included
3. **Binding not hiding**: If attacker knows seed, they can verify digest (acceptable)
4. **Replay detection**: Different seeds produce different receipts, enabling detection
5. **Manifest binding flag**: Indicates strength of seed derivation (weak if no manifest)

---

## Migration Notes

- V1/V2 receipts remain valid (no seed digest required)
- V3 receipts include seed digest in hash computation
- `--require-seed-digest` flag available for strict verification
- Seed lineage captured at inference start, before any sampling
