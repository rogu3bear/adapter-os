# CoreML Backend Determinism for Production Audit Trails

**Status:** Reference Documentation  
**Last Updated:** 2026-01-02  
**Author:** Engineering  
**Related PRDs:** PRD 7 (Deterministic Adapter Loading), PRD 8 (Plugin Isolation)

---

## 1. Problem Statement

### Business Context

Enterprise customers in regulated industries (finance, healthcare, defense) require cryptographic proof that AI inference outputs are reproducible. When auditors ask "Can you prove this exact output came from this exact computation?", the answer must be yes, without network access, for 7+ years, under regulatory scrutiny.

### Technical Challenge

GPU execution produces non-deterministic results due to:
- Floating-point rounding order variance across runs
- Parallel reduction non-determinism
- Driver-dependent transcendental function implementations
- Thread scheduling affecting accumulator order

CoreML on Apple Silicon provides a unique opportunity: the Apple Neural Engine (ANE) offers bit-exact reproducibility when properly configured.

### Current State

- CoreML backend exists with ANE/GPU/CPU compute unit selection
- HKDF-SHA256 seed derivation implemented in adapteros-core
- Q15 gate quantization in router (denominator = 32767.0)
- Production mode flag exists but enforcement is incomplete
- macOS 15+ MLTensor API available; macOS 26+ enhanced scheduling

### Gap

No unified PRD documents the complete determinism architecture, acceptance criteria, and verification strategy for CoreML-based inference.

---

## 2. Goals

1. Bit-Exact Reproducibility: Same manifest + same inputs -> identical outputs across runs, machines, and time
2. Offline Verification: Prove inference integrity without network calls (air-gapped environments)
3. Audit Trail Compliance: Meet SOC 2, ISO 27001, HIPAA, ITAR, PCI DSS, GDPR audit requirements
4. Production Enforcement: Fail-fast when determinism cannot be guaranteed
5. Attestation: Generate cryptographically signed receipts proving execution parameters

---

## 3. Non-Goals

- Cross-Backend Parity: CoreML, MLX, and Metal may produce different outputs for same inputs (acceptable; single backend per deployment)
- GPU Determinism: GPU execution remains non-deterministic; not in scope
- Training Determinism: Focus on inference only; training determinism is separate
- Performance Optimization: Accept 5-10% overhead from IEEE 754 compliance

---

## 4. Target Personas

| Persona            | Need                                    | Success Metric                                |
|--------------------|-----------------------------------------|-----------------------------------------------|
| Compliance Officer | Prove AI meets audit requirements       | Signed receipts verifiable 7+ years later     |
| Security Auditor   | Detect tampering or drift               | Golden run comparison catches any regression  |
| Defense Contractor | Air-gapped operation with offline proof | aosctl replay --verify passes without network |
| MLOps/SRE          | Reproduce production bugs               | Same seed + manifest -> identical failure     |
| Enterprise Sales   | Answer "Is your AI deterministic?"      | Reference golden run verification flow        |

---

## 5. Requirements

### R1: ANE-Only Production Mode

**Requirement:** When production_mode=true, enforce ANE-only execution or fail fast.

**Implementation:**

```rust
// crates/adapteros-lora-kernel-coreml/src/lib.rs
if production_mode && !ane_status.available {
    return Err("Production mode requires ANE for guaranteed determinism");
}
let compute_units = ComputeUnits::CpuAndNeuralEngine; // Force ANE
```

**Rationale:** GPU execution is non-deterministic. ANE provides bit-exact results.

**Acceptance Criteria:**
- CoreMLBackend::new(_, true) fails if ANE unavailable
- Production mode overrides any GPU compute unit configuration
- Warning logged when configuration is overridden

---

### R2: Q15 Gate Quantization Invariant

**Requirement:** All router gates use Q15 fixed-point with denominator = 32767.0 (not 32768).

**Implementation:**

```rust
// crates/adapteros-lora-router/src/quantization.rs
pub const ROUTER_GATE_Q15_DENOM: f32 = 32767.0;  // CRITICAL: Never change
pub const ROUTER_GATE_Q15_MAX: i16 = 32767;

pub fn quantize_gate(gate: f32) -> i16 {
    (gate * ROUTER_GATE_Q15_DENOM).round().clamp(0, 32767) as i16
}
```

**Rationale:**
- 32768 would overflow i16 range
- 32767.0 ensures 1.0 maps to exactly 32767 with no precision loss
- Eliminates cross-machine floating-point drift

**Acceptance Criteria:**
- Round-trip test: f32 -> Q15 -> f32 returns exact value for 0.0 and 1.0
- Constants hardcoded (no configuration)
- Compile-time assertion prevents denominator changes

---

### R3: HKDF-SHA256 Seed Derivation Chain

**Requirement:** All randomness derives from manifest hash via HKDF-SHA256.

**Implementation:**

```text
// crates/adapteros-core/src/seed.rs
Global Seed = BLAKE3(manifest_content)
Router Seed = HKDF-SHA256(global_seed, "router:{manifest_hash}")
Sampler Seed = HKDF-SHA256(global_seed, "sample:{step}")
```

**Invariants:**
1. Same inputs -> Same seed (deterministic)
2. No seed reuse (registry tracks label+nonce pairs)
3. HKDF-SHA256 only (other KDFs break replay compatibility)
4. 32-byte output (ChaCha20Rng requirement)

**Acceptance Criteria:**
- Golden vector test catches any HKDF algorithm drift
- AOS_DEBUG_DETERMINISM=1 logs all seed derivations
- Different labels produce cryptographically distinct seeds

---

### R4: Deterministic Router Tie-Breaking

**Requirement:** Router sorting uses canonical ordering: score DESC, index ASC.

**Implementation:**

```rust
// crates/adapteros-lora-router/src/router.rs
scores.sort_by(|a, b| {
    let cmp = b.1.total_cmp(&a.1);  // Score DESC (IEEE 754 total order)
    if cmp == Ordering::Equal {
        a.0.cmp(&b.0)  // Index ASC (deterministic tie-break)
    } else {
        cmp
    }
});
```

**Rationale:**
- total_cmp() handles NaN deterministically
- Index-based secondary sort eliminates RNG dependency
- Produces identical results across platforms

**Acceptance Criteria:**
- 1000-iteration stress test with near-equal scores passes
- No RNG used in tie-breaking path
- Cross-instance determinism verified

---

### R5: macOS Version Gating for MLTensor

**Requirement:** MLTensor deterministic scheduling requires macOS 26+ (Tahoe).

**Implementation:**

```rust
pub enum MltensorApiVersion {
    NotAvailable = 0,  // < macOS 15
    Sequoia = 1,       // macOS 15.x (basic MLTensor)
    Tahoe = 2,         // macOS 26+ (MLComputePolicy API)
}

// In production mode, disable MLTensor unless Tahoe
if production_mode && version != Tahoe {
    use_mltensor = false;
    warn!("Disabling MLTensor; deterministic scheduling requires macOS 26+");
}
```

**Rationale:** Only macOS 26+ provides per-operation compute unit selection via MLComputePolicy.

**Acceptance Criteria:**
- Production mode on macOS 15 uses fallback path
- Warning logged when MLTensor disabled
- Version detection returns correct enum value

---

### R6: Attestation Report Generation

**Requirement:** Every inference produces a DeterminismReport for audit.

**Implementation:**

```rust
pub struct DeterminismReport {
    backend_type: BackendType::CoreML,
    rng_seed_method: RngSeedingMethod::HkdfSeeded,
    floating_point_mode: FloatingPointMode::Deterministic,
    deterministic: bool,  // true iff ANE + ANE-only + Tahoe MLTensor
}
```

**Contents:**
- Backend identification
- RNG seeding method (must be HKDF, not SystemEntropy)
- Floating-point mode (must be Deterministic, not FastMath)
- Overall determinism flag

**Acceptance Criteria:**
- Attestation embedded in RunReceipt
- Receipt signature verifiable offline
- 7-year retention compliant (immutable storage)

---

### R7: Offline Replay Verification

**Requirement:** Replay harness verifies inference without network calls.

**Command:**

```bash
aosctl replay --dir <evidence_bundle> --verify --report <output_path>
```

**Evidence Bundle Contents:**
- context_manifest.json (base model, adapters, worker)
- token_trace.json (per-token gates, adapter IDs)
- input_tokens.json
- expected_report.json (golden output)

**Acceptance Criteria:**
- Replay reads only local files
- Exit code 0 if all outputs match
- Detailed mismatch report generated on failure

---

### R8: Compiler Constraints

**Requirement:** No -ffast-math or equivalent flags in any compilation path.

**Rationale:**
- -ffast-math allows operation reordering (breaks associativity)
- Enables non-IEEE 754 compliant optimizations
- Produces non-reproducible results

**Acceptance Criteria:**
- CI checks for forbidden compiler flags
- Metal shader compilation uses strict IEEE mode
- Build fails if -ffast-math detected

---

## 6. Failure Modes and Error Codes

| Error Code                  | Condition                                 | Recovery                     |
|-----------------------------|-------------------------------------------|------------------------------|
| E2001: DeterminismViolation | Non-reproducible behavior detected        | Fail-fast, quarantine        |
| BOOT_SEED_FAILED            | Seed initialization failed at boot        | Block startup                |
| CACHE_KEY_NONDETERMINISTIC  | Cache key contains random/time components | Invalidate + regenerate      |
| RECEIPT_MISMATCH            | Replay output differs from golden         | Investigation required       |
| POLICY_DIVERGENCE           | Policy result differs from expectation    | Quarantine                   |
| ANE_UNAVAILABLE             | Neural Engine not available               | Fail-fast in production mode |
| MACOS_VERSION_UNSUPPORTED   | < macOS 15 for MLTensor                   | Fallback to legacy path      |

---

## 7. Performance Characteristics

| Component             | Overhead                   | Acceptable |
|-----------------------|----------------------------|------------|
| Q15 Quantization      | Negligible (O(1) per gate) | Yes        |
| HKDF Seed Derivation  | Sub-microsecond            | Yes        |
| Router Decision       | < 100us typical            | Yes        |
| Router % of Inference | 1.4-2.8%                   | Yes        |
| Evidence Chain        | 1-2us per token            | Yes        |
| IEEE 754 Compliance   | 5-10% fp32 slowdown        | Yes        |
| ANE vs GPU Power      | 50% reduction              | Yes        |

**Target Metrics:**
- Throughput: >=40 tokens/second
- Token latency: <=25ms
- Hot-swap latency: <100ms p95
- Memory overhead: <=10%

---

## 8. Test Plan

### Unit Tests

| Test                | Location                                         | Verifies                  |
|---------------------|--------------------------------------------------|---------------------------|
| Q15 round-trip      | crates/adapteros-lora-router/src/quantization.rs | 0.0->0, 1.0->32767        |
| HKDF golden vector  | crates/adapteros-core/tests/determinism.rs       | Algorithm stability       |
| Router tie-breaking | crates/adapteros-lora-router/tests/determinism.rs | Score DESC, index ASC    |
| Seed derivation     | crates/adapteros-core/tests/determinism.rs       | Same inputs -> same output |

### Integration Tests

| Test                        | Location                                             | Verifies                |
|-----------------------------|------------------------------------------------------|-------------------------|
| CoreML determinism          | crates/adapteros-lora-kernel-coreml/tests/determinism_tests.rs | Bit-exact across 5 runs |
| Swift/ObjC++ equivalence    | crates/adapteros-lora-kernel-coreml/tests/determinism_tests.rs | <=2 ULP difference      |
| Production mode enforcement | crates/adapteros-lora-kernel-coreml/tests/           | Fails without ANE       |
| Replay verification         | tests/determinism_core_suite.rs                      | PRD 8 compliance        |

### E2E Tests

```bash
# Determinism verification
cargo test --test determinism_core_suite -- --test-threads=8

# Router determinism
cargo test -p adapteros-lora-router --test determinism

# CoreML bit-exact
cargo test -p adapteros-lora-kernel-coreml --test determinism_tests
```

---

## 9. Configuration

### Environment Variables

```text
AOS_DEBUG_DETERMINISM=1      # Log seed derivations
AOS_PRODUCTION_MODE=true     # Enforce ANE-only
```

### Config File (configs/cp.toml)

```toml
[execution]
production_mode = true       # Enforce determinism
seed_mode = "strict"         # Require manifest hash

[coreml]
compute_units = "cpu_and_neural_engine"  # ANE-only
```

### API (Tenant Settings)

```json
{
  "determinism_policy": {
    "seed_mode": "strict",
    "routing_determinism_mode": "deterministic",
    "require_attestation": true
  }
}
```

---

## 10. Rollout Plan

### Phase 1: Validation (Complete)

- Q15 quantization with 32767.0 denominator
- HKDF-SHA256 seed derivation
- Router tie-breaking (score DESC, index ASC)
- CoreML ANE detection and enforcement

### Phase 2: Attestation (In Progress)

- DeterminismReport generation
- Receipt signing with Ed25519
- Offline replay harness

### Phase 3: Compliance Certification

- SOC 2 Type II audit preparation
- 7-year retention implementation
- Air-gapped deployment validation

---

## 11. Risk Assessment

| Risk                                 | Likelihood | Impact   | Mitigation                          |
|--------------------------------------|------------|----------|-------------------------------------|
| ANE unavailable on target hardware   | Low        | High     | Fail-fast with clear error message  |
| macOS version too old                | Medium     | Medium   | Fallback path with warning          |
| Cross-backend replay attempted       | Medium     | High     | Document single-backend requirement |
| Q15 denominator accidentally changed | Low        | Critical | Compile-time assertion              |
| HKDF algorithm changed               | Low        | Critical | Golden vector test in CI            |

---

## 12. Success Criteria

1. Bit-Exact Replay: Same manifest + seed produces identical output 100% of the time
2. Offline Verification: aosctl replay --verify works without network
3. Audit Compliance: Receipts verifiable by external auditor
4. Production Enforcement: Fails fast when determinism cannot be guaranteed
5. Documentation: Complete API/configuration reference

---

## 13. References

- docs/COREML_BACKEND.md - Backend architecture
- crates/adapteros-core/src/seed.rs - HKDF implementation
- crates/adapteros-lora-router/src/quantization.rs - Q15 implementation
- docs/DETERMINISM.md - Full determinism architecture
- docs/hardening/replay.md - Offline replay harness
- golden_runs/README.md - Golden run verification
