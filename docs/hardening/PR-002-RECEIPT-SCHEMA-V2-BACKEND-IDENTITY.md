# PR-002: Receipt Schema V2 with Backend Identity

## Summary

Extend the receipt digest to include backend identity fields (`backend_used`, `backend_attestation_b3`) enabling detection of backend substitution attacks while maintaining backward compatibility with v1 receipts.

## Problem Statement

Current receipt digest (`verify_receipt.rs:442-451`) does not bind the receipt to the specific backend that executed inference:

```rust
// Current v1 receipt digest - missing backend binding
B3Hash::hash_multi(&[
    context_digest,
    run_head,
    output_digest,
    // token accounting fields...
])
```

**Attack scenario**: A compromised worker could execute inference on MLX but report CoreML in the receipt, or vice versa. Since backends may have subtly different floating-point behavior, this could allow non-reproducible outputs to be certified as deterministic.

The `InferenceReceiptRef` in `evidence_envelope.rs` already has `backend_used` and `backend_attestation_b3` fields (PRD-DET-001), but these are not incorporated into the receipt digest computed during verification.

## Solution

1. Add `schema_version` field to `ReceiptDigests` (v1 = current, v2 = with backend)
2. For v2 receipts, include `backend_used` and `backend_attestation_b3` in digest computation
3. Populate backend fields from worker inference context
4. Maintain backward compatibility: v1 receipts verify without backend fields

---

## Implementation Details

### File Changes

#### 1. `crates/adapteros-cli/src/commands/verify_receipt.rs`

**Update `ReceiptDigests` struct**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceiptDigests {
    // Existing fields
    run_head_hash_hex: String,
    output_digest_hex: String,
    receipt_digest_hex: String,
    #[serde(default)]
    signature_b64: Option<String>,
    #[serde(default)]
    public_key_hex: Option<String>,
    #[serde(default)]
    logical_prompt_tokens: u32,
    #[serde(default)]
    prefix_cached_token_count: u32,
    #[serde(default)]
    billed_input_tokens: u32,
    #[serde(default)]
    logical_output_tokens: u32,
    #[serde(default)]
    billed_output_tokens: u32,

    // NEW: Schema version (defaults to 1 for backward compat)
    #[serde(default = "default_schema_version")]
    schema_version: u8,

    // NEW: Backend identity (v2+)
    #[serde(default)]
    backend_used: Option<String>,
    #[serde(default)]
    backend_attestation_b3_hex: Option<String>,
}

fn default_schema_version() -> u8 {
    1
}

/// Receipt schema versions
pub const RECEIPT_SCHEMA_V1: u8 = 1;
pub const RECEIPT_SCHEMA_V2: u8 = 2;
pub const RECEIPT_SCHEMA_CURRENT: u8 = RECEIPT_SCHEMA_V2;
```

**Update `ReceiptBundle` struct**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceiptBundle {
    // Existing fields...

    // NEW: Backend fields at bundle level (populated by worker)
    #[serde(default)]
    backend_used: Option<String>,
    #[serde(default)]
    backend_attestation_b3_hex: Option<String>,
}
```

**Add new `ReasonCode`**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReasonCode {
    ContextMismatch,
    TraceTamper,
    OutputMismatch,
    PolicyMismatch,
    BackendMismatch,
    SignatureInvalid,
    // NEW
    BackendAttestationMismatch,
    SchemaVersionUnsupported,
}
```

**Update `compute_receipt_digest` function** (new helper):

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
            // V1: Original digest without backend fields
            B3Hash::hash_multi(&[
                context_digest.as_bytes(),
                run_head.as_bytes(),
                output_digest.as_bytes(),
                &receipt.logical_prompt_tokens.to_le_bytes(),
                &receipt.prefix_cached_token_count.to_le_bytes(),
                &receipt.billed_input_tokens.to_le_bytes(),
                &receipt.logical_output_tokens.to_le_bytes(),
                &receipt.billed_output_tokens.to_le_bytes(),
            ])
        }
        RECEIPT_SCHEMA_V2 => {
            // V2: Include backend identity
            let backend_bytes = bundle.backend_used
                .as_deref()
                .unwrap_or("")
                .as_bytes();

            let attestation_bytes = bundle.backend_attestation_b3_hex
                .as_ref()
                .and_then(|h| hex::decode(h).ok())
                .unwrap_or_default();

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
                &[RECEIPT_SCHEMA_V2],  // Schema version byte
                &(backend_bytes.len() as u32).to_le_bytes(),
                backend_bytes,
                &(attestation_bytes.len() as u32).to_le_bytes(),
                &attestation_bytes,
            ])
        }
        _ => {
            // Unknown schema - return zero hash, will trigger mismatch
            tracing::warn!(
                schema_version = receipt.schema_version,
                "Unknown receipt schema version"
            );
            B3Hash::zero()
        }
    }
}
```

**Update `verify_bundle` function**:

```rust
fn verify_bundle(bundle: &ReceiptBundle) -> Result<ReceiptVerificationReport> {
    let mut reasons: Vec<ReasonCode> = Vec::new();

    // ... existing context and token verification ...

    // NEW: Validate schema version
    if receipt.schema_version > RECEIPT_SCHEMA_CURRENT {
        push_reason(&mut reasons, ReasonCode::SchemaVersionUnsupported);
    }

    // NEW: Backend attestation verification for v2+
    if receipt.schema_version >= RECEIPT_SCHEMA_V2 {
        // If bundle claims backend_used, verify consistency across tokens
        if let Some(ref expected_backend) = bundle.backend_used {
            if bundle.tokens.iter().any(|t| {
                t.backend_id.as_ref()
                    .map(|b| b.to_lowercase() != expected_backend.to_lowercase())
                    .unwrap_or(false)
            }) {
                push_reason(&mut reasons, ReasonCode::BackendMismatch);
            }
        }

        // Verify attestation matches if provided in both places
        if let (Some(ref bundle_att), Some(ref receipt_att)) =
            (&bundle.backend_attestation_b3_hex, &receipt.backend_attestation_b3_hex)
        {
            if bundle_att != receipt_att {
                push_reason(&mut reasons, ReasonCode::BackendAttestationMismatch);
            }
        }
    }

    // Compute receipt digest using schema-aware function
    let receipt_digest = compute_receipt_digest(
        &computed_context,
        &run_head,
        &output_digest,
        &bundle.receipt,
        bundle,
    );

    // ... rest of verification ...
}
```

#### 2. `crates/adapteros-lora-worker/src/evidence.rs`

**Populate backend fields during inference**:

```rust
use adapteros_core::backend::BackendKind;

pub struct InferenceEvidenceBuilder {
    // ... existing fields ...
    backend_used: Option<String>,
    backend_attestation: Option<B3Hash>,
}

impl InferenceEvidenceBuilder {
    pub fn with_backend(mut self, backend: BackendKind) -> Self {
        self.backend_used = Some(backend.as_str().to_string());
        self
    }

    pub fn with_backend_attestation(mut self, attestation: &DeterminismReport) -> Self {
        // Compute attestation hash from determinism report
        self.backend_attestation = Some(attestation.to_attestation_hash());
        self
    }

    pub fn build_receipt_bundle(self) -> ReceiptBundle {
        ReceiptBundle {
            // ... existing fields ...
            backend_used: self.backend_used,
            backend_attestation_b3_hex: self.backend_attestation.map(|h| h.to_hex()),
            receipt: ReceiptDigests {
                schema_version: RECEIPT_SCHEMA_V2,
                backend_used: self.backend_used.clone(),
                backend_attestation_b3_hex: self.backend_attestation.map(|h| h.to_hex()),
                // ... other fields ...
            },
        }
    }
}
```

#### 3. `crates/adapteros-lora-kernel-api/src/attestation.rs`

**Add attestation hash computation**:

```rust
use adapteros_core::B3Hash;

/// Determinism report for backend attestation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterminismReport {
    /// Backend identifier (metal, coreml, mlx)
    pub backend: String,
    /// Metallib hash for Metal backend (None for others)
    pub metallib_hash: Option<B3Hash>,
    /// RNG seeding method
    pub rng_seed_method: String,
    /// Floating point mode (strict, relaxed)
    pub floating_point_mode: String,
    /// Determinism level achieved
    pub determinism_level: String,
    /// Compiler flags used
    pub compiler_flags: Vec<String>,
}

impl DeterminismReport {
    /// Compute attestation hash for receipt binding.
    ///
    /// This hash uniquely identifies the backend configuration used for inference.
    pub fn to_attestation_hash(&self) -> B3Hash {
        let mut components: Vec<&[u8]> = vec![
            self.backend.as_bytes(),
            self.rng_seed_method.as_bytes(),
            self.floating_point_mode.as_bytes(),
            self.determinism_level.as_bytes(),
        ];

        // Include metallib hash if present
        if let Some(ref mlh) = self.metallib_hash {
            components.push(mlh.as_bytes());
        }

        // Include sorted compiler flags
        let mut flags_sorted: Vec<_> = self.compiler_flags.iter().collect();
        flags_sorted.sort();
        for flag in &flags_sorted {
            components.push(flag.as_bytes());
        }

        // Length-prefix each component for unambiguous parsing
        let mut buf = Vec::new();
        for component in components {
            buf.extend_from_slice(&(component.len() as u32).to_le_bytes());
            buf.extend_from_slice(component);
        }

        B3Hash::hash(&buf)
    }
}
```

#### 4. `crates/adapteros-server-api/src/handlers/run_evidence.rs`

**Include backend fields in export**:

```rust
fn build_receipt_bundle_from_trace(trace: &InferenceTrace) -> ReceiptBundle {
    ReceiptBundle {
        version: Some("aos-receipt-v2".to_string()),
        // ... existing fields ...
        backend_used: trace.backend_used.clone(),
        backend_attestation_b3_hex: trace.backend_attestation_b3.map(|h| h.to_hex()),
        receipt: ReceiptDigests {
            schema_version: RECEIPT_SCHEMA_V2,
            // ... existing fields ...
            backend_used: trace.backend_used.clone(),
            backend_attestation_b3_hex: trace.backend_attestation_b3.map(|h| h.to_hex()),
        },
    }
}
```

---

## Acceptance Criteria

- [ ] `ReceiptDigests` includes `schema_version` field (defaults to 1)
- [ ] V2 receipt digest computation includes `backend_used` and `backend_attestation_b3`
- [ ] V1 receipts without backend fields still verify correctly (backward compat)
- [ ] V2 receipts with mismatched backend produce `BackendMismatch` reason
- [ ] Worker populates backend fields from inference context
- [ ] Evidence export includes backend fields in receipt bundle
- [ ] `DeterminismReport.to_attestation_hash()` produces deterministic output
- [ ] Unknown schema versions produce `SchemaVersionUnsupported` reason

---

## Test Plan

### Unit Tests

**File**: `crates/adapteros-cli/tests/verify_receipt_v2_tests.rs`

```rust
#[test]
fn test_v1_receipt_backward_compat() {
    // Load a golden v1 receipt bundle (no backend fields)
    let bundle = load_golden_v1_bundle();

    let report = verify_bundle(&bundle).unwrap();

    assert!(report.reasons.is_empty(), "v1 receipt should still verify");
    assert!(report.receipt_digest.matches);
}

#[test]
fn test_v2_receipt_includes_backend() {
    let bundle = create_v2_bundle_with_backend("metal", Some(metallib_hash));

    let report = verify_bundle(&bundle).unwrap();

    assert!(report.reasons.is_empty());
    assert_eq!(bundle.receipt.schema_version, RECEIPT_SCHEMA_V2);
}

#[test]
fn test_backend_substitution_detected() {
    // Create bundle where tokens claim CoreML but bundle claims Metal
    let mut bundle = create_v2_bundle_with_backend("metal", None);
    bundle.tokens[0].backend_id = Some("coreml".to_string());

    let report = verify_bundle(&bundle).unwrap();

    assert!(report.reasons.iter().any(|r| matches!(r, ReasonCode::BackendMismatch)));
}

#[test]
fn test_different_backend_produces_different_digest() {
    let bundle_metal = create_v2_bundle_with_backend("metal", None);
    let bundle_coreml = create_v2_bundle_with_backend("coreml", None);

    // Same inputs, different backend -> different receipt digest
    assert_ne!(
        bundle_metal.receipt.receipt_digest_hex,
        bundle_coreml.receipt.receipt_digest_hex
    );
}

#[test]
fn test_attestation_hash_deterministic() {
    let report = DeterminismReport {
        backend: "metal".to_string(),
        metallib_hash: Some(B3Hash::hash(b"kernel-v1")),
        rng_seed_method: "hkdf".to_string(),
        floating_point_mode: "strict".to_string(),
        determinism_level: "bitexact".to_string(),
        compiler_flags: vec!["-O2".to_string(), "-fno-fast-math".to_string()],
    };

    let hash1 = report.to_attestation_hash();
    let hash2 = report.to_attestation_hash();

    assert_eq!(hash1, hash2);
}

#[test]
fn test_attestation_hash_varies_with_metallib() {
    let mut report1 = create_determinism_report();
    report1.metallib_hash = Some(B3Hash::hash(b"kernel-v1"));

    let mut report2 = create_determinism_report();
    report2.metallib_hash = Some(B3Hash::hash(b"kernel-v2"));

    assert_ne!(report1.to_attestation_hash(), report2.to_attestation_hash());
}
```

### Golden Tests

**File**: `crates/adapteros-cli/tests/golden_receipts/`

```
golden_receipts/
├── v1_valid.json           # Valid v1 receipt (no backend)
├── v1_tampered.json        # Tampered v1 receipt
├── v2_metal_valid.json     # Valid v2 receipt with Metal backend
├── v2_coreml_valid.json    # Valid v2 receipt with CoreML backend
└── v2_backend_mismatch.json # V2 with backend mismatch
```

```rust
#[test]
fn test_golden_v1_valid() {
    let bundle = load_bundle("golden_receipts/v1_valid.json");
    let report = verify_bundle(&bundle).unwrap();
    assert!(report.reasons.is_empty());
}

#[test]
fn test_golden_v2_metal_valid() {
    let bundle = load_bundle("golden_receipts/v2_metal_valid.json");
    let report = verify_bundle(&bundle).unwrap();
    assert!(report.reasons.is_empty());
    assert_eq!(bundle.backend_used, Some("metal".to_string()));
}
```

### Integration Tests

**File**: `tests/receipt_schema_v2_integration.rs`

```rust
#[tokio::test]
async fn test_inference_produces_v2_receipt() {
    let server = start_test_server().await;

    // Run inference
    let response = server.post("/v1/inference")
        .json(&inference_request())
        .send()
        .await;

    // Fetch receipt
    let receipt = server.get(&format!("/v1/trace/{}/receipt", response.trace_id))
        .send()
        .await
        .json::<ReceiptBundle>();

    assert_eq!(receipt.receipt.schema_version, RECEIPT_SCHEMA_V2);
    assert!(receipt.backend_used.is_some());
}

#[tokio::test]
async fn test_receipt_verification_via_api() {
    let server = start_test_server().await;

    // Run inference and get trace_id
    let trace_id = run_inference(&server).await;

    // Verify receipt via API
    let result = server.post("/v1/verify/receipt")
        .json(&json!({ "trace_id": trace_id }))
        .send()
        .await
        .json::<ReceiptVerificationReport>();

    assert!(result.reasons.is_empty());
}
```

---

## Migration Notes

### Backward Compatibility

- V1 receipts (no `schema_version` field) default to version 1
- V1 verification logic unchanged
- No database migration required
- Old CLI versions can verify v1 receipts
- New CLI can verify both v1 and v2 receipts

### Forward Compatibility

- Unknown schema versions (>2) produce `SchemaVersionUnsupported`
- Schema version included in digest prevents version downgrade attacks
- Receipt format is JSON, new fields are additive

### Rollout Sequence

1. Deploy CLI with v2 support (can verify v1 and v2)
2. Deploy worker with v2 receipt generation
3. Deploy server with v2 export support
4. Existing v1 receipts remain valid indefinitely
