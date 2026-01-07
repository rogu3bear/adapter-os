# Determinism Hardening Review

> **Status**: Implementation Review
> **Author**: Engineering Review
> **Date**: 2025-01-06
> **Related**: PRD-DET-001, PRD-DET-002

---

## Table of Contents

1. [Determinism Contract](#1-determinism-contract)
2. [Gap Analysis Table](#2-gap-analysis-table)
3. [Top 3 PRs Implementation Plan](#3-top-3-prs-implementation-plan)
4. [Runtime Enforcement Design](#4-runtime-enforcement-design)
5. [Test Plan Additions](#5-test-plan-additions)
6. [Risks and Unknowns](#6-risks-and-unknowns)

---

## 1. Determinism Contract

### 1.1 Definition

**Determinism in AdapterOS** means: given identical inputs (model weights, adapter parameters, prompt tokens, and configuration), the system produces bit-identical outputs across:

- Multiple inference runs on the same hardware
- System restarts with the same seed
- Replay from recorded telemetry traces

### 1.2 Input Variance Specification

| Input Category | Allowed Variance | Strictness |
|----------------|------------------|------------|
| Prompt tokens | None (must be identical) | Required |
| Model weights | None (hash-verified) | Required |
| Adapter weights | None (hash-verified) | Required |
| Global seed | None (HKDF-SHA256 derived) | Required |
| Temperature | None | Required |
| Top-p/Top-k | None | Required |
| Stop sequences | None (digest-bound) | Required |
| Backend type | None (attestation-bound) | Required |
| Metallib hash | None (verified for Metal) | Required |
| System timestamp | Varies | Tolerated (not in hash) |
| Request ID | Varies | Tolerated (not in output hash) |

### 1.3 Output Bit-Identity Requirements

| Output | Bit-Identity Required | Verification Method |
|--------|----------------------|---------------------|
| Token sequence | Yes | `output_digest` B3Hash |
| Token probabilities | Yes (Q15 quantized) | Decision hash chain |
| Router gate values | Yes (Q15 @ 32767.0) | `gates_q15` in trace |
| Adapter selection | Yes | Index list in receipt |
| Stop reason | Yes | `stop_reason_code` |
| Token timing | No | Variable (not hashed) |

### 1.4 Seed Modes

```
┌─────────────────┬────────────────────────────────────────────────────────┐
│ Mode            │ Behavior                                               │
├─────────────────┼────────────────────────────────────────────────────────┤
│ Strict          │ Requires manifest hash; fails if missing or fallback   │
│                 │ attempted. Production inference MUST use this mode.    │
├─────────────────┼────────────────────────────────────────────────────────┤
│ BestEffort      │ Uses manifest hash when present; deterministic         │
│                 │ fallback seed when missing. For dev/testing only.      │
├─────────────────┼────────────────────────────────────────────────────────┤
│ NonDeterministic│ Random seed from system entropy. For benchmarking      │
│                 │ only. Receipts marked non-deterministic.               │
└─────────────────┴────────────────────────────────────────────────────────┘
```

### 1.5 Backend Determinism Levels

| Backend | DeterminismLevel | Conditions |
|---------|------------------|------------|
| Metal | `BitExact` | `metallib_verified=true`, `-fno-fast-math` |
| MLX | `BitExact` | HKDF-seeded, deterministic mode |
| CoreML | `BoundedTolerance` | ANE available, no strict guarantee |
| Mock | `BitExact` | Testing only |

### 1.6 Critical Invariants

1. **HKDF-SHA256**: All seed derivation uses HKDF-SHA256 with version tracking (`HKDF_ALGORITHM_VERSION = 2`)
2. **Q15 Denominator**: Router gates use `32767.0` (NOT 32768) — this is precision-critical
3. **Tie-breaking**: Router sorts by score DESC, then index ASC for deterministic ordering
4. **TypedSeed Validation**: Seeds have BLAKE3 checksum; validation fails closed in strict mode
5. **Forbidden Flags**: `-ffast-math`, `-funsafe-math-optimizations` are rejected

### 1.7 Contract Violations

A **determinism violation** occurs when:

- Same inputs produce different output hashes across runs
- Backend attestation fails validation
- Seed version mismatch at FFI boundary
- Q15 conversion uses wrong denominator
- Forbidden compiler flags detected in attestation

**Response to violation**: Fail inference request with `AosError::DeterminismViolation`, log evidence, emit telemetry event.

---

## 2. Gap Analysis Table

| # | Symptom | Evidence (file:line) | Severity | Proposed Fix | Affected Crates | Acceptance Criteria | Test Coverage |
|---|---------|----------------------|----------|--------------|-----------------|---------------------|---------------|
| G1 | Backend substitution undetectable in decision hash | `router.rs:1240` — `compute_decision_hash()` missing backend param | S1 | Add `backend_identity_hash` to DecisionHash | `adapteros-lora-router` | Different backend → different `combined_hash` | `test_decision_hash_changes_with_backend_identity` |
| G2 | Strict mode accepts fallback seed | `deterministic-exec/seed.rs:420` — `init_with_fallback()` ignores mode | S2 | Add `init_with_fallback_checked()` with mode enforcement | `adapteros-deterministic-exec` | Strict + None seed → `Err` | `test_strict_mode_rejects_fallback` |
| G3 | CoreML determinism level defaults to `None` | `attestation.rs:107-112` — Default impl returns `None` | S2 | CoreML should default to `BoundedTolerance` when ANE detected | `adapteros-lora-kernel-api` | ANE present → `BoundedTolerance` | `test_coreml_determinism_level_with_ane` |
| G4 | No runtime enforcement of Q15 denominator in FFI | Metal shaders use `32767.0` but Rust doesn't verify | S1 | Add compile-time assertion in quantization module | `adapteros-lora-router` | Build fails if constant changes | Compile-time check |
| G5 | Seed lineage not bound to receipt | `evidence_envelope.rs` — no `seed_lineage` field | S2 | Add `seed_lineage_hash` to InferenceReceiptRef | `adapteros-core` | Replay with wrong seed → different receipt | `test_seed_lineage_bound_to_receipt` |
| G6 | Router decision telemetry lacks backend context | `events.rs` — `RouterDecision` missing backend info | S2 | Add `backend_type` field to RouterDecision event | `adapteros-trace` | Event includes backend | `test_router_decision_includes_backend` |
| G7 | Dual-write drift detection incomplete | `atomic_dual_write_tests.rs` — no cross-backend comparison | S1 | Add drift detection between backends | `adapteros-db` | Drift detected → alert | `test_dual_write_backend_drift_detection` |

### Severity Definitions

- **S0**: Production blocker — determinism guarantee completely broken
- **S1**: High — silent determinism drift possible, requires immediate fix
- **S2**: Medium — edge case or defense-in-depth gap

### Gap Status (Post PRD-DET-001 Implementation)

| Gap | Status | Notes |
|-----|--------|-------|
| Evidence chain backend binding | ✅ IMPLEMENTED | `backend_used`, `backend_attestation_b3` added to InferenceReceiptRef |
| Schema version bump | ✅ IMPLEMENTED | `EVIDENCE_ENVELOPE_SCHEMA_VERSION = 2` |
| Backend attestation hash | ✅ IMPLEMENTED | `DeterminismReport::to_attestation_hash()` exists |
| G1 (Decision hash backend) | ⏳ PENDING | PR #2 spec complete in PRD-DET-001 |
| G2 (Strict mode fallback) | ⏳ PENDING | PR #3 spec complete in PRD-DET-001 |
| G3-G7 | ⏳ PENDING | Additional gaps identified |

---

## 3. Top 3 PRs Implementation Plan

### PR-A: Seed Propagation & Lineage Binding

**Objective**: Ensure seed lineage is cryptographically bound to inference receipts for complete replay verification.

#### Files to Modify

| File | Change |
|------|--------|
| `crates/adapteros-core/src/evidence_envelope.rs` | Add `seed_lineage_hash: Option<B3Hash>` to InferenceReceiptRef |
| `crates/adapteros-core/src/seed.rs` | Add `SeedLineage::to_binding_hash()` method |
| `crates/adapteros-lora-worker/src/inference_pipeline.rs` | Pass seed lineage to receipt builder |
| `crates/adapteros-deterministic-exec/src/seed.rs` | Wire `init_with_fallback_checked()` (from PRD-DET-001 PR #3) |

#### Implementation Steps

1. Add `SeedLineage::to_binding_hash()`:
```rust
impl SeedLineage {
    /// Compute binding hash for receipt inclusion.
    /// Includes: global_seed, derived_seeds map, version.
    pub fn to_binding_hash(&self) -> B3Hash {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(self.global_seed.as_bytes());
        // Sort keys for deterministic ordering
        let mut keys: Vec<_> = self.derived_seeds.keys().collect();
        keys.sort();
        for key in keys {
            buf.extend_from_slice(key.as_bytes());
            buf.extend_from_slice(self.derived_seeds[key].as_bytes());
        }
        B3Hash::hash(&buf)
    }
}
```

2. Add field to InferenceReceiptRef (after `backend_attestation_b3`):
```rust
/// Hash of seed lineage for replay verification.
/// Verifies that replay uses identical seed derivation chain.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub seed_lineage_hash: Option<B3Hash>,
```

3. Update `encode_scope_payload()` to include seed lineage hash.

4. Wire strict mode rejection via `init_with_fallback_checked()`.

#### Acceptance Criteria

- [ ] Replay with different seed → different receipt digest
- [ ] Strict mode + None seed → `SeedError::ValidationError`
- [ ] Seed lineage hash changes if any derived seed changes
- [ ] Backward compat: v2 receipts deserialize with None default

---

### PR-B: Runtime Backend Attestation

**Objective**: Enforce determinism attestation at runtime for all backends.

#### Files to Modify

| File | Change |
|------|--------|
| `crates/adapteros-lora-kernel-api/src/attestation.rs` | Add `validate_for_inference()` method |
| `crates/adapteros-lora-worker/src/backend_factory.rs` | Call attestation validation before inference |
| `crates/adapteros-lora-router/src/router.rs` | Add `backend_identity_hash` to DecisionHash (PRD-DET-001 PR #2) |
| `crates/adapteros-policy/src/packs/determinism.rs` | Add attestation check to policy hook |

#### Implementation Steps

1. Add runtime validation method:
```rust
impl DeterminismReport {
    /// Validate attestation for production inference.
    /// Returns error if determinism requirements not met.
    pub fn validate_for_inference(&self, required_level: DeterminismLevel) -> Result<()> {
        // First, run existing validation
        self.validate()?;

        // Check required level
        if self.determinism_level < required_level {
            return Err(AosError::DeterminismViolation(format!(
                "Backend determinism level {:?} < required {:?}",
                self.determinism_level, required_level
            )));
        }

        // Metal-specific: require metallib verification for BitExact
        if self.backend_type == BackendType::Metal
            && required_level == DeterminismLevel::BitExact
            && !self.metallib_verified {
            return Err(AosError::DeterminismViolation(
                "Metal backend requires verified metallib for BitExact".into()
            ));
        }

        Ok(())
    }
}
```

2. Add enforcement in backend factory:
```rust
// In create_backend()
let attestation = backend.get_determinism_report();
let required_level = config.determinism_level.unwrap_or(DeterminismLevel::BoundedTolerance);
attestation.validate_for_inference(required_level)?;
```

3. Wire DecisionHash backend binding per PRD-DET-001 PR #2.

#### Acceptance Criteria

- [ ] Inference fails if attestation validation fails
- [ ] Metal without verified metallib → error in BitExact mode
- [ ] CoreML + BoundedTolerance required → passes (ANE ok)
- [ ] Decision hash includes backend identity
- [ ] Different backend → different combined_hash

---

### PR-C: Evidence Completeness & Receipt Binding

**Objective**: Complete the evidence chain with full determinism binding.

#### Files to Modify

| File | Change |
|------|--------|
| `crates/adapteros-trace/src/events.rs` | Add `backend_type` to RouterDecision |
| `crates/adapteros-db/src/routing_decisions.rs` | Store backend type in DB |
| `crates/adapteros-core/src/evidence_envelope.rs` | Bump to v3 with seed_lineage_hash |

#### Implementation Steps

1. Update RouterDecision event:
```rust
pub struct RouterDecision {
    // ... existing fields ...

    /// Backend type used for this routing decision
    pub backend_type: Option<String>,
}
```

2. Update routing_decisions table schema (migration):
```sql
ALTER TABLE routing_decisions ADD COLUMN backend_type TEXT;
```

3. Add dual-write drift detection (per PRD-DET-002):
```rust
/// Check for drift between dual-written records.
pub fn detect_backend_drift(
    primary: &RoutingDecision,
    secondary: &RoutingDecision,
) -> Option<DriftReport> {
    if primary.backend_type != secondary.backend_type {
        return Some(DriftReport {
            field: "backend_type".to_string(),
            primary_value: primary.backend_type.clone(),
            secondary_value: secondary.backend_type.clone(),
        });
    }
    None
}
```

#### Acceptance Criteria

- [ ] RouterDecision events include backend_type
- [ ] Routing decisions persisted with backend type
- [ ] Dual-write drift detected and logged
- [ ] Schema v3 with complete evidence binding

---

## 4. Runtime Enforcement Design

### 4.1 Enforcement Points

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         INFERENCE REQUEST                                    │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ EP-1: Seed Initialization                                                    │
│ ─────────────────────────                                                    │
│ Location: adapteros-deterministic-exec/src/seed.rs:init_with_fallback_checked│
│ Check: SeedMode::Strict requires explicit seed                               │
│ Action: Return SeedError::ValidationError if fallback attempted              │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ EP-2: Backend Attestation                                                    │
│ ───────────────────────                                                      │
│ Location: adapteros-lora-worker/src/backend_factory.rs:create_backend        │
│ Check: DeterminismReport.validate_for_inference(required_level)              │
│ Action: Return AosError::DeterminismViolation if validation fails            │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ EP-3: TypedSeed FFI Boundary                                                 │
│ ─────────────────────────                                                    │
│ Location: adapteros-core/src/seed.rs:TypedSeed::validate                     │
│ Check: Version == HKDF_ALGORITHM_VERSION, checksum valid                     │
│ Action: Return SeedError::VersionMismatch or ValidationError                 │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ EP-4: Policy Hook (OnBeforeInference)                                        │
│ ────────────────────────────────────                                         │
│ Location: adapteros-policy/src/packs/determinism.rs:execute                  │
│ Check: SeedMode, BackendType, DeterminismLevel                               │
│ Action: PolicyDecision::Deny with reason code                                │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ EP-5: Receipt Finalization                                                   │
│ ────────────────────────                                                     │
│ Location: adapteros-core/src/evidence_envelope.rs:new_inference              │
│ Check: All required fields populated (backend_used, attestation, lineage)    │
│ Action: Emit incomplete_receipt telemetry event if fields missing            │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 4.2 Violation Response Matrix

| Enforcement Point | Violation Type | Response | Artifact |
|-------------------|----------------|----------|----------|
| EP-1 | Strict mode fallback | Fail request, return error | Error log with mode/seed context |
| EP-2 | Attestation invalid | Fail request, return error | DeterminismReport dump to telemetry |
| EP-3 | Seed version mismatch | Fail request, return error | Expected vs actual version |
| EP-4 | Policy denial | Fail request, return error | PolicyAuditDecision with reason |
| EP-5 | Incomplete receipt | Warn, continue (degraded) | Telemetry event with missing fields |

### 4.3 Tamper-Evident Chain Linking

```
┌────────────────────┐    ┌────────────────────┐    ┌────────────────────┐
│ EvidenceEnvelope   │───▶│ EvidenceEnvelope   │───▶│ EvidenceEnvelope   │
│ (Inference #1)     │    │ (Inference #2)     │    │ (Inference #3)     │
├────────────────────┤    ├────────────────────┤    ├────────────────────┤
│ schema_version: 2  │    │ schema_version: 2  │    │ schema_version: 2  │
│ previous_root: None│    │ previous_root: R1  │    │ previous_root: R2  │
│ root: R1           │    │ root: R2           │    │ root: R3           │
│ signature: S1      │    │ signature: S2      │    │ signature: S3      │
├────────────────────┤    ├────────────────────┤    ├────────────────────┤
│ InferenceReceiptRef│    │ InferenceReceiptRef│    │ InferenceReceiptRef│
│ ├─ trace_id        │    │ ├─ trace_id        │    │ ├─ trace_id        │
│ ├─ output_digest   │    │ ├─ output_digest   │    │ ├─ output_digest   │
│ ├─ backend_used    │    │ ├─ backend_used    │    │ ├─ backend_used    │
│ ├─ backend_attest  │    │ ├─ backend_attest  │    │ ├─ backend_attest  │
│ └─ seed_lineage    │    │ └─ seed_lineage    │    │ └─ seed_lineage    │
└────────────────────┘    └────────────────────┘    └────────────────────┘
```

### 4.4 Verification Flow

```rust
/// Verify evidence chain integrity.
pub fn verify_evidence_chain(envelopes: &[EvidenceEnvelope]) -> ChainVerificationResult {
    let mut previous_root: Option<B3Hash> = None;

    for (i, env) in envelopes.iter().enumerate() {
        // 1. Check chain linking
        if env.previous_root != previous_root {
            return ChainVerificationResult::broken(i, "Chain link mismatch");
        }

        // 2. Verify signature
        if !verify_signature(env) {
            return ChainVerificationResult::broken(i, "Invalid signature");
        }

        // 3. Verify root hash
        let computed_root = env.compute_root();
        if computed_root != env.root {
            return ChainVerificationResult::broken(i, "Root hash mismatch");
        }

        previous_root = Some(env.root.clone());
    }

    ChainVerificationResult::valid()
}
```

---

## 5. Test Plan Additions

### Test Matrix

| # | Test Name | Type | What It Tests | File Location |
|---|-----------|------|---------------|---------------|
| T1 | `test_seed_collision_detection` | Unit | Same (label, request_id) → collision error | `seed.rs` |
| T2 | `test_replay_with_wrong_seed_fails_verification` | Integration | Replay produces different receipt → verification fails | `determinism_tests.rs` |
| T3 | `test_backend_selection_determinism` | Integration | Same inputs → same backend selection across runs | `backend_factory_tests.rs` |
| T4 | `test_policy_pack_determinism_enforcement` | Unit | Determinism policy denies non-deterministic config | `determinism.rs` |
| T5 | `test_q15_denominator_locked_compile_time` | Compile | Q15 constant != 32767.0 → compile error | `constants.rs` |
| T6 | `test_evidence_chain_tamper_detection` | Integration | Modified envelope → verification fails | `evidence_envelope_tests.rs` |
| T7 | `test_strict_mode_no_fallback` | Unit | Strict + None seed → error | `seed_strict_tests.rs` |
| T8 | `test_metallib_verification_required_for_bitexact` | Unit | Metal + BitExact + unverified → error | `attestation_tests.rs` |
| T9 | `test_seed_lineage_receipt_binding` | Unit | Different lineage → different receipt digest | `evidence_envelope_tests.rs` |
| T10 | `test_dual_write_drift_detection` | Integration | Backend drift → DriftReport emitted | `dual_write_tests.rs` |

### Test Implementations

#### T1: Seed Collision Detection

```rust
#[test]
fn test_seed_collision_detection() {
    let global = B3Hash::hash(b"test-manifest");

    // First derivation succeeds
    let seed1 = derive_seed_with_registry(&global, "router", 12345);
    assert!(seed1.is_ok());

    // Same label + request_id → collision
    let seed2 = derive_seed_with_registry(&global, "router", 12345);
    assert!(matches!(seed2, Err(SeedError::CollisionDetected { .. })));

    // Different request_id → ok
    let seed3 = derive_seed_with_registry(&global, "router", 12346);
    assert!(seed3.is_ok());
}
```

#### T2: Replay Verification

```rust
#[test]
fn test_replay_with_wrong_seed_fails_verification() {
    let original_seed = [42u8; 32];
    let wrong_seed = [99u8; 32];

    // Run inference with original seed
    let receipt1 = run_inference_with_seed(original_seed, "test prompt");

    // Replay with wrong seed
    let receipt2 = run_inference_with_seed(wrong_seed, "test prompt");

    // Receipts must differ
    assert_ne!(receipt1.receipt_digest, receipt2.receipt_digest,
        "Wrong seed must produce different receipt");
    assert_ne!(receipt1.seed_lineage_hash, receipt2.seed_lineage_hash,
        "Seed lineage hash must differ");
}
```

#### T3: Backend Selection Determinism

```rust
#[test]
fn test_backend_selection_determinism() {
    let config = InferenceConfig {
        seed_mode: SeedMode::Strict,
        seed: Some([42u8; 32]),
        ..Default::default()
    };

    let mut backend_types = Vec::new();
    for _ in 0..10 {
        let factory = BackendFactory::new(config.clone());
        let backend = factory.create_backend().unwrap();
        backend_types.push(backend.get_type());
    }

    // All runs must select same backend
    assert!(backend_types.windows(2).all(|w| w[0] == w[1]),
        "Backend selection must be deterministic");
}
```

#### T4: Policy Pack Enforcement

```rust
#[test]
fn test_policy_pack_determinism_enforcement() {
    let policy = DeterminismPolicyPack::new();

    // Strict mode + deterministic backend → allow
    let ctx_ok = PolicyContext {
        seed_mode: SeedMode::Strict,
        backend_type: BackendType::Metal,
        determinism_level: DeterminismLevel::BitExact,
    };
    assert_eq!(policy.evaluate(&ctx_ok), PolicyDecision::Allow);

    // Strict mode + non-deterministic backend → deny
    let ctx_deny = PolicyContext {
        seed_mode: SeedMode::Strict,
        backend_type: BackendType::CoreML,
        determinism_level: DeterminismLevel::None,
    };
    assert_eq!(policy.evaluate(&ctx_deny), PolicyDecision::Deny);
}
```

#### T5: Q15 Compile-Time Guard

```rust
// In constants.rs
pub const ROUTER_GATE_Q15_DENOM: f32 = 32767.0;

// Compile-time assertion
const _: () = {
    // This will fail to compile if denominator is wrong
    assert!(
        (ROUTER_GATE_Q15_DENOM - 32767.0).abs() < f32::EPSILON,
        "Q15 denominator MUST be exactly 32767.0"
    );
};
```

#### T6: Evidence Chain Tamper Detection

```rust
#[test]
fn test_evidence_chain_tamper_detection() {
    // Create valid chain
    let env1 = EvidenceEnvelope::new_inference("tenant".into(), receipt1, None);
    let env2 = EvidenceEnvelope::new_inference("tenant".into(), receipt2, Some(env1.root.clone()));

    // Verify valid chain
    let result = verify_evidence_chain(&[env1.clone(), env2.clone()]);
    assert!(result.is_valid());

    // Tamper with envelope
    let mut tampered = env2.clone();
    tampered.inference_receipt_ref.as_mut().unwrap().backend_used = "tampered".into();

    // Tampered chain must fail
    let result = verify_evidence_chain(&[env1, tampered]);
    assert!(!result.is_valid());
}
```

#### T7-T10: Additional Tests

See PRD-DET-001 for T7 (`test_strict_mode_rejects_fallback`) and T8 implementation.

```rust
// T9: Seed Lineage Receipt Binding
#[test]
fn test_seed_lineage_receipt_binding() {
    let lineage1 = SeedLineage::new([1u8; 32]);
    let lineage2 = SeedLineage::new([2u8; 32]);

    let mut receipt1 = sample_inference_ref();
    receipt1.seed_lineage_hash = Some(lineage1.to_binding_hash());

    let mut receipt2 = sample_inference_ref();
    receipt2.seed_lineage_hash = Some(lineage2.to_binding_hash());

    let env1 = EvidenceEnvelope::new_inference("t".into(), receipt1, None);
    let env2 = EvidenceEnvelope::new_inference("t".into(), receipt2, None);

    assert_ne!(env1.digest(), env2.digest(),
        "Different seed lineage must produce different digest");
}

// T10: Dual-Write Drift Detection
#[test]
fn test_dual_write_drift_detection() {
    let primary = RoutingDecision {
        backend_type: Some("metal".to_string()),
        ..Default::default()
    };
    let secondary = RoutingDecision {
        backend_type: Some("coreml".to_string()),
        ..Default::default()
    };

    let drift = detect_backend_drift(&primary, &secondary);
    assert!(drift.is_some());
    assert_eq!(drift.unwrap().field, "backend_type");
}
```

---

## 6. Risks and Unknowns

### 6.1 Technical Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| CoreML ANE availability varies by device | High | Medium | Fallback to BoundedTolerance; document device requirements |
| Metallib hash changes with SDK updates | Medium | High | Pin SDK version; add hash to attestation |
| Q15 overflow on extreme gate values | Low | High | Clamping enforced; fuzz testing |
| Schema migration breaks old receipts | Medium | Medium | `#[serde(default)]` for backward compat |
| Strict mode breaks existing deployments | Medium | Medium | Opt-in via config; default BestEffort initially |

### 6.2 Open Questions

1. **CoreML Determinism**: Apple does not guarantee ANE determinism. Should we mark CoreML as `DeterminismLevel::None` always, or `BoundedTolerance` when ANE is detected?

   **Recommendation**: Use `BoundedTolerance` when ANE detected, but document that bit-exact replay is not guaranteed.

2. **Multi-GPU Setups**: How do we ensure determinism when multiple GPUs are available?

   **Recommendation**: Add `device_id` to attestation (already present) and require explicit device pinning in strict mode.

3. **Seed Registry Memory Growth**: The seed collision registry grows unbounded during long runs.

   **Recommendation**: Add TTL-based eviction or per-request scoping.

4. **Backward Compatibility Window**: How long do we support v1 schema receipts?

   **Recommendation**: 6-month deprecation window; v1 receipts deserialize but emit warning.

### 6.3 Dependencies

| Dependency | Type | Risk |
|------------|------|------|
| MLX unified memory model | External | Low — MLX designed for determinism |
| Metal shader compiler | External | Medium — Apple may change behavior |
| CoreML ANE scheduling | External | High — No determinism guarantee from Apple |
| BLAKE3 crate | Rust | Low — Stable, widely used |
| HKDF crate | Rust | Low — Well-audited |

### 6.4 Rollout Recommendations

1. **Phase 1** (Week 1-2): Implement PR-A (seed propagation) — lowest risk
2. **Phase 2** (Week 2-3): Implement PR-B (runtime attestation) — requires testing
3. **Phase 3** (Week 3-4): Implement PR-C (evidence completeness) — schema change
4. **Phase 4** (Week 4): Enable strict mode in staging
5. **Phase 5** (Week 5): Production rollout with monitoring

---

## Appendix A: Implementation Status Checklist

### PRD-DET-001 PRs

- [x] PR #1: Evidence Chain Backend Binding — **IMPLEMENTED** (schema v2, fields added)
- [ ] PR #2: Decision Hash Backend Identity — spec complete, implementation pending
- [ ] PR #3: Strict Mode Fallback Rejection — spec complete, implementation pending

### Additional Gaps (This Review)

- [ ] G3: CoreML determinism level default
- [ ] G5: Seed lineage receipt binding
- [ ] G6: Router decision backend context
- [ ] G7: Dual-write drift detection

### Tests

- [ ] T1: Seed collision detection
- [ ] T2: Replay verification
- [ ] T3: Backend selection determinism
- [ ] T4: Policy pack enforcement
- [x] T5: Q15 compile-time guard — existing test `test_q15_denominator_is_32767`
- [ ] T6: Evidence chain tamper detection
- [ ] T7: Strict mode no fallback
- [ ] T8: Metallib verification for BitExact
- [ ] T9: Seed lineage receipt binding
- [ ] T10: Dual-write drift detection

---

*End of Determinism Hardening Review*
