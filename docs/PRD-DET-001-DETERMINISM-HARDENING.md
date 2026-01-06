# PRD-DET-001: Determinism Hardening

## Overview

This document specifies three PRs to address gaps in AdapterOS's determinism enforcement identified during the determinism hardening review.

## Executive Summary

| PR | Title | Severity | Files Changed |
|----|-------|----------|---------------|
| #1 | Evidence Chain Backend Binding | S1 | `evidence_envelope.rs` |
| #2 | Decision Hash Backend Identity | S1 | `types.rs`, `router.rs` |
| #3 | Strict Mode Fallback Rejection | S2 | `seed.rs` |

---

## PR #1: Evidence Chain Backend Binding

### Problem Statement

The `InferenceReceiptRef` in the tamper-evident evidence chain does not include backend identity information. This means backend substitution (e.g., swapping Metal for CoreML) is undetectable in the evidence chain, even though `DeterministicReceipt` in the API response includes `backend_used`.

### Acceptance Criteria

- [ ] `InferenceReceiptRef` includes `backend_used: String` field
- [ ] `InferenceReceiptRef` includes `backend_attestation_b3: Option<B3Hash>` field
- [ ] `encode_scope_payload()` serializes backend fields into canonical bytes
- [ ] `EVIDENCE_ENVELOPE_SCHEMA_VERSION` bumped to 2
- [ ] Tests prove different `backend_used` → different envelope digest
- [ ] Backward compatibility: v1 receipts deserialize with empty defaults

### File Changes

#### `crates/adapteros-core/src/evidence_envelope.rs`

**Change 1: Bump schema version**
```rust
// Line 34-35 - BEFORE:
/// Schema version for forward compatibility
pub const EVIDENCE_ENVELOPE_SCHEMA_VERSION: u8 = 1;

// AFTER:
/// Schema version for forward compatibility
///
/// Version history:
/// - v1: Initial schema with telemetry, policy, inference scopes
/// - v2: Added backend_used and backend_attestation_b3 to InferenceReceiptRef (PRD-DET-001)
pub const EVIDENCE_ENVELOPE_SCHEMA_VERSION: u8 = 2;
```

**Change 2: Add backend fields to InferenceReceiptRef**
```rust
// After line 147 (model_cache_identity_v2_digest_b3), add:

    // --- Backend identity (PRD-DET-001: Determinism hardening) ---
    /// Backend used for inference (e.g., "metal", "coreml", "mlx").
    ///
    /// This field binds the receipt to the specific backend that executed
    /// the inference, ensuring backend substitution is detectable in the
    /// tamper-evident evidence chain.
    #[serde(default)]
    pub backend_used: String,

    /// BLAKE3 hash of backend attestation report for integrity verification.
    ///
    /// Computed from the `DeterminismReport` canonical bytes, including:
    /// - metallib_hash (for Metal backend)
    /// - rng_seed_method
    /// - floating_point_mode
    /// - determinism_level
    /// - compiler_flags
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_attestation_b3: Option<B3Hash>,
```

**Change 3: Update encode_scope_payload for Inference scope**
```rust
// After the Model identity encoding block (line ~399), add:

                    // Backend identity (PRD-DET-001: v2 schema addition)
                    encode_str(bytes, &r.backend_used);
                    match &r.backend_attestation_b3 {
                        Some(h) => {
                            bytes.push(1); // present marker
                            bytes.extend_from_slice(h.as_bytes());
                        }
                        None => {
                            bytes.push(0); // absent marker
                        }
                    }
```

**Change 4: Update sample_inference_ref() test helper**
```rust
    fn sample_inference_ref() -> InferenceReceiptRef {
        InferenceReceiptRef {
            // ... existing fields ...
            backend_used: "metal".to_string(),
            backend_attestation_b3: Some(B3Hash::hash(b"metal-attestation")),
        }
    }
```

**Change 5: Add new tests**
```rust
    // ==========================================================================
    // PRD-DET-001: Backend identity binding tests
    // ==========================================================================

    #[test]
    fn test_backend_used_changes_receipt_digest() {
        let mut ref1 = sample_inference_ref();
        ref1.backend_used = "metal".to_string();

        let mut ref2 = sample_inference_ref();
        ref2.backend_used = "coreml".to_string();

        let env1 = EvidenceEnvelope::new_inference("tenant-1".to_string(), ref1, None);
        let mut env2 = EvidenceEnvelope::new_inference("tenant-1".to_string(), ref2, None);
        env2.created_at = env1.created_at.clone();

        assert_ne!(env1.digest(), env2.digest(),
            "Different backend_used must produce different digest");
    }

    #[test]
    fn test_backend_attestation_changes_receipt_digest() {
        let mut ref1 = sample_inference_ref();
        ref1.backend_attestation_b3 = Some(B3Hash::hash(b"attestation-1"));

        let mut ref2 = sample_inference_ref();
        ref2.backend_attestation_b3 = Some(B3Hash::hash(b"attestation-2"));

        let env1 = EvidenceEnvelope::new_inference("tenant-1".to_string(), ref1, None);
        let mut env2 = EvidenceEnvelope::new_inference("tenant-1".to_string(), ref2, None);
        env2.created_at = env1.created_at.clone();

        assert_ne!(env1.digest(), env2.digest(),
            "Different backend_attestation_b3 must produce different digest");
    }

    #[test]
    fn test_v1_schema_backward_compat_deserialization() {
        let json = r#"{
            "trace_id": "trace-old",
            "run_head_hash": "0000000000000000000000000000000000000000000000000000000000000000",
            "output_digest": "0000000000000000000000000000000000000000000000000000000000000000",
            "receipt_digest": "0000000000000000000000000000000000000000000000000000000000000000",
            "logical_prompt_tokens": 10,
            "prefix_cached_token_count": 0,
            "billed_input_tokens": 10,
            "logical_output_tokens": 5,
            "billed_output_tokens": 5
        }"#;

        let parsed: InferenceReceiptRef =
            serde_json::from_str(json).expect("v1 schema should deserialize");

        assert_eq!(parsed.backend_used, "", "backend_used defaults to empty");
        assert!(parsed.backend_attestation_b3.is_none(),
            "backend_attestation defaults to None");
    }
```

### Smoke Test Checklist

- [ ] `cargo test -p adapteros-core evidence_envelope` passes
- [ ] `cargo test -p adapteros-core test_backend_used_changes_receipt_digest` passes
- [ ] `cargo test -p adapteros-core test_v1_schema_backward_compat` passes

---

## PR #2: Decision Hash Backend Identity

### Problem Statement

The `DecisionHash` computed by the router does not include backend identity. This means changing the backend (Metal → CoreML) produces the same `combined_hash`, breaking the determinism contract.

### Acceptance Criteria

- [ ] `DecisionHash` includes `backend_identity_hash: Option<String>` field
- [ ] `compute_decision_hash()` accepts optional `backend_identity_hash` param
- [ ] `combined_hash` computation includes backend hash when present
- [ ] Tests prove different backend → different `combined_hash`
- [ ] Backward compatibility: callers without backend context pass `None`

### File Changes

#### `crates/adapteros-lora-router/src/types.rs`

**Change 1: Add backend field to DecisionHash**
```rust
// Around line 390-406, add new field:

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionHash {
    pub input_hash: String,
    pub output_hash: String,
    pub reasoning_hash: Option<String>,
    pub combined_hash: String,
    pub tau: f32,
    pub eps: f32,
    pub k: usize,

    /// Hash of backend identity for complete determinism binding (PRD-DET-001).
    /// When present, included in combined_hash computation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_identity_hash: Option<String>,
}
```

#### `crates/adapteros-lora-router/src/router.rs`

**Change 2: Update compute_decision_hash signature**
```rust
// Around line 1206, update signature:

fn compute_decision_hash(
    &self,
    features: &[f32],
    priors: &[f32],
    indices: &[u16],
    gates_q15: &[i16],
    reasoning_hash: Option<&B3Hash>,
    backend_identity_hash: Option<&B3Hash>,  // NEW PARAM (PRD-DET-001)
) -> DecisionHash {
```

**Change 3: Include backend in combined hash computation**
```rust
// Around line 1240-1248, update combined hash:

        // Combine all hashes including backend (PRD-DET-001)
        let mut combined_bytes = Vec::new();
        combined_bytes.extend_from_slice(input_hash.as_bytes());
        combined_bytes.extend_from_slice(output_hash.as_bytes());
        if let Some(reasoning) = reasoning_hash {
            combined_bytes.extend_from_slice(reasoning.as_bytes());
        }
        if let Some(backend) = backend_identity_hash {
            combined_bytes.extend_from_slice(backend.as_bytes());
        }
        let combined_hash = B3Hash::hash(&combined_bytes);

        DecisionHash {
            input_hash: input_hash.to_short_hex(),
            output_hash: output_hash.to_short_hex(),
            reasoning_hash: reasoning_hash.map(|h| h.to_short_hex()),
            combined_hash: combined_hash.to_short_hex(),
            tau: self.tau,
            eps: self.eps,
            k: self.k,
            backend_identity_hash: backend_identity_hash.map(|h| h.to_short_hex()),
        }
```

**Change 4: Update all callers to pass None for backward compat**

Search for all calls to `compute_decision_hash` and add `, None` as the last argument.

#### `crates/adapteros-lora-router/tests/decision_hash_tests.rs` (NEW FILE)

```rust
//! PRD-DET-001: Decision hash tests for backend identity binding

use adapteros_lora_router::*;

#[test]
fn test_decision_hash_changes_with_backend_identity() {
    let router = KSparseRouter::new(/* ... */);
    let features = vec![0.5, 0.3, 0.2];
    let priors = vec![1.0, 1.0, 1.0];
    let indices = vec![0, 1];
    let gates_q15 = vec![16384, 8192];

    let backend1 = B3Hash::hash(b"metal");
    let backend2 = B3Hash::hash(b"coreml");

    let hash1 = router.compute_decision_hash(
        &features, &priors, &indices, &gates_q15, None, Some(&backend1)
    );
    let hash2 = router.compute_decision_hash(
        &features, &priors, &indices, &gates_q15, None, Some(&backend2)
    );

    assert_ne!(hash1.combined_hash, hash2.combined_hash,
        "Different backend must produce different combined_hash");
    assert_eq!(hash1.input_hash, hash2.input_hash,
        "Input hash should be identical");
}

#[test]
fn test_decision_hash_backward_compat_no_backend() {
    let router = KSparseRouter::new(/* ... */);
    let features = vec![0.5, 0.3, 0.2];
    let priors = vec![1.0, 1.0, 1.0];
    let indices = vec![0, 1];
    let gates_q15 = vec![16384, 8192];

    // Without backend (backward compat)
    let hash = router.compute_decision_hash(
        &features, &priors, &indices, &gates_q15, None, None
    );

    assert!(hash.backend_identity_hash.is_none());
    assert!(!hash.combined_hash.is_empty());
}
```

### Smoke Test Checklist

- [ ] `cargo test -p adapteros-lora-router` passes
- [ ] `cargo test -p adapteros-lora-router test_decision_hash_changes_with_backend` passes
- [ ] Existing router tests still pass (backward compat)

---

## PR #3: Strict Mode Fallback Rejection

### Problem Statement

`GlobalSeedManager::init_with_fallback()` uses a deterministic fallback seed regardless of `SeedMode`. In strict mode, this should fail closed instead of silently using the fallback.

### Acceptance Criteria

- [ ] New method `init_with_fallback_checked(seed, mode)` that checks mode
- [ ] In `SeedMode::Strict`, returns `Err` when no primary seed provided
- [ ] In `SeedMode::BestEffort`, uses fallback (existing behavior)
- [ ] Tests prove strict mode fails without primary seed
- [ ] Existing callers continue working (use new method explicitly)

### File Changes

#### `crates/adapteros-deterministic-exec/src/seed.rs`

**Change 1: Add new method with mode checking**
```rust
// After line 426 (init_with_fallback), add:

    /// Initialize global seed with mode-aware fallback handling (PRD-DET-001).
    ///
    /// In `SeedMode::Strict`, this method fails if no primary seed is provided.
    /// In other modes, it falls back to a deterministic HKDF-derived seed.
    pub fn init_with_fallback_checked(
        &self,
        primary_seed: Option<[u8; 32]>,
        mode: adapteros_core::seed::SeedMode,
    ) -> Result<[u8; 32], SeedError> {
        use adapteros_core::seed::SeedMode;

        match (primary_seed, mode) {
            (Some(seed), _) => {
                // Primary seed available - use it
                let fallback_rng = ChaCha20Rng::from_seed(seed);
                *self.fallback_rng.lock() = Some(fallback_rng);
                info!(seed_source = "primary", "Initialized global seed manager");
                Ok(seed)
            }
            (None, SeedMode::Strict) => {
                // STRICT MODE: Fail closed, no fallback allowed
                error!(
                    mode = "strict",
                    "No primary seed provided - failing closed (PRD-DET-001)"
                );
                Err(SeedError::ValidationError(
                    "Strict mode requires explicit seed; fallback disallowed".to_string()
                ))
            }
            (None, _) => {
                // BestEffort/NonDeterministic: use fallback (existing behavior)
                self.init_with_fallback(None)
            }
        }
    }
```

#### `crates/adapteros-deterministic-exec/tests/seed_strict_mode_tests.rs` (NEW FILE)

```rust
//! PRD-DET-001: Strict mode seed tests

use adapteros_core::seed::SeedMode;
use adapteros_deterministic_exec::seed::{GlobalSeedManager, SeedError};

#[test]
fn test_strict_mode_rejects_fallback() {
    let manager = GlobalSeedManager::new();

    let result = manager.init_with_fallback_checked(None, SeedMode::Strict);

    assert!(result.is_err(), "Strict mode must reject fallback");
    let err = result.unwrap_err();
    assert!(matches!(err, SeedError::ValidationError(_)));
    assert!(err.to_string().contains("Strict mode"));
}

#[test]
fn test_strict_mode_accepts_primary_seed() {
    let manager = GlobalSeedManager::new();
    let seed = [42u8; 32];

    let result = manager.init_with_fallback_checked(Some(seed), SeedMode::Strict);

    assert!(result.is_ok(), "Strict mode must accept primary seed");
    assert_eq!(result.unwrap(), seed);
}

#[test]
fn test_best_effort_mode_allows_fallback() {
    let manager = GlobalSeedManager::new();

    let result = manager.init_with_fallback_checked(None, SeedMode::BestEffort);

    assert!(result.is_ok(), "BestEffort mode must allow fallback");
}

#[test]
fn test_non_deterministic_mode_allows_fallback() {
    let manager = GlobalSeedManager::new();

    let result = manager.init_with_fallback_checked(None, SeedMode::NonDeterministic);

    assert!(result.is_ok(), "NonDeterministic mode must allow fallback");
}
```

### Smoke Test Checklist

- [ ] `cargo test -p adapteros-deterministic-exec seed` passes
- [ ] `cargo test -p adapteros-deterministic-exec test_strict_mode_rejects_fallback` passes
- [ ] Existing seed tests still pass

---

## Integration Points

### Wiring Backend Identity Through the Stack

After implementing PRs #1-3, wire the backend identity through:

1. **Backend Factory** (`backend_factory.rs`):
   - Compute `DeterminismReport` hash when creating backend
   - Store as `backend_attestation_b3`

2. **Inference Pipeline** (`inference_pipeline.rs`):
   - Pass backend attestation hash to router via decision context
   - Include in `InferenceReceiptRef` when finalizing

3. **InferResponse Builder**:
   - Populate `deterministic_receipt.backend_used` from pipeline context

### Example Integration Code

```rust
// In inference_pipeline.rs, when building receipt:
let receipt_ref = InferenceReceiptRef {
    // ... existing fields ...
    backend_used: self.backend_type.as_str().to_string(),
    backend_attestation_b3: Some(self.backend_attestation_hash),
};
```

---

## Test Matrix

| Test | PR | Type | Assertion |
|------|----|----- |-----------|
| `test_backend_used_changes_receipt_digest` | #1 | Unit | Different backend → different digest |
| `test_v1_schema_backward_compat` | #1 | Unit | Old JSON deserializes |
| `test_decision_hash_changes_with_backend` | #2 | Unit | Different backend → different combined_hash |
| `test_strict_mode_rejects_fallback` | #3 | Unit | Strict + None → Err |
| `test_strict_mode_accepts_primary_seed` | #3 | Unit | Strict + Some → Ok |

---

## Rollout Plan

1. **Phase 1**: Merge PR #1 (evidence chain) - low risk, additive only
2. **Phase 2**: Merge PR #2 (decision hash) - requires caller updates
3. **Phase 3**: Merge PR #3 (strict mode) - requires config review
4. **Phase 4**: Wire integration points
5. **Phase 5**: Enable strict mode in production

---

## Risks

| Risk | Mitigation |
|------|------------|
| Schema version bump breaks old receipts | Backward compat via `#[serde(default)]` |
| Router callers need update for new param | Add param as last optional, default None |
| Strict mode breaks existing deployments | Opt-in via config, default BestEffort |
