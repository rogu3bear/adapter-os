# PRD: CoreML Determinism Implementation for Audit Trails

**Status:** Draft  
**Last Updated:** 2026-01-02  
**Owner:** Engineering  
**Related Docs:** docs/COREML_DETERMINISM_AUDIT_TRAILS.md, docs/COREML_BACKEND.md, docs/DETERMINISM.md

---

## 1. Summary

Deliver production-grade determinism for the CoreML backend, with enforceable ANE-only execution, deterministic scheduling rules, HKDF-seeded randomness, and verifiable audit receipts. This PRD defines implementation work, acceptance criteria, and phased rollout for audit-grade determinism without network dependencies.

---

## 2. Problem Statement

Regulated customers require cryptographic proof that inference outputs are reproducible, offline, and verifiable for 7+ years. CoreML on Apple Silicon provides deterministic execution through the ANE when configured correctly. The current system has individual deterministic components, but implementation gaps and incomplete enforcement prevent a single, auditable determinism guarantee.

---

## 3. Goals

1. Bit-exact reproducibility for CoreML inference (same manifest + same inputs -> same outputs).
2. Offline verification with no network calls.
3. Audit trail compliance across SOC 2, ISO 27001, HIPAA, ITAR, PCI DSS, and GDPR.
4. Fail-fast production enforcement when determinism cannot be guaranteed.
5. Determinism attestation embedded in signed receipts.

---

## 4. Non-Goals

- Cross-backend parity between CoreML, MLX, and Metal.
- GPU determinism.
- Training determinism.
- Performance optimization beyond ensuring IEEE 754 compliance.

---

## 5. Scope

### In Scope

- CoreML backend determinism enforcement in production mode.
- Deterministic router gates and tie-breaking invariants.
- HKDF-SHA256 seed derivation chain and logging.
- Determinism attestation report embedded in run receipts.
- Offline replay verification and evidence bundle structure.
- CI/runtime guardrails for forbidden compiler flags.

### Out of Scope

- Cross-backend deterministic comparisons.
- GPU kernel determinism and scheduling.
- Training-time determinism or dataset provenance changes.

---

## 6. Dependencies and Constraints

- Apple Neural Engine availability (M1+ hardware).
- macOS 15+ for MLTensor API; macOS 26+ for deterministic scheduling policy.
- No `-ffast-math` or equivalent flags across Rust, Swift, ObjC++, or Metal.
- HKDF-SHA256 seeded randomness with BLAKE3 manifest hash as root.

---

## 7. Requirements and Implementation Plan

### R1: ANE-Only Production Mode

**Requirement:** In production mode, enforce ANE-only compute units or fail fast.

**Implementation Tasks:**
- Enforce `ComputeUnits::CpuAndNeuralEngine` when `production_mode=true`.
- Fail fast if ANE is unavailable.
- Log warnings when configuration is overridden.

**Primary Files:**
- `crates/adapteros-lora-kernel-coreml/src/lib.rs`
- `docs/COREML_BACKEND.md` (behavior reference)

**Acceptance Criteria:**
- Production init fails when ANE unavailable.
- GPU or CPU-only configs are overridden and logged in production mode.

---

### R2: Q15 Gate Quantization Invariant

**Requirement:** Gate quantization uses denominator 32767.0 (never 32768).

**Implementation Tasks:**
- Add compile-time guard (const assert) for `ROUTER_GATE_Q15_DENOM == 32767.0`.
- Add round-trip unit test for 0.0 and 1.0.
- Ensure constants are not configurable.

**Primary Files:**
- `crates/adapteros-lora-router/src/quantization.rs`
- `crates/adapteros-lora-router/tests/determinism.rs`

**Acceptance Criteria:**
- Build fails if denominator changes.
- Unit tests pass on all platforms.

---

### R3: HKDF-SHA256 Seed Derivation Chain

**Requirement:** All randomness derives from manifest hash via HKDF-SHA256.

**Implementation Tasks:**
- Verify global seed derives from manifest BLAKE3.
- Enforce HKDF-SHA256 in all seed derivations.
- Emit determinism logs when `AOS_DEBUG_DETERMINISM=1`.
- Add or refresh golden vector tests for HKDF output stability.

**Primary Files:**
- `crates/adapteros-core/src/seed.rs`
- `crates/adapteros-core/tests/determinism.rs`

**Acceptance Criteria:**
- Golden vector test passes and catches algorithm drift.
- Seed registry prevents label reuse.

---

### R4: Deterministic Router Tie-Breaking

**Requirement:** Router sorting uses score DESC, index ASC with IEEE 754 total order.

**Implementation Tasks:**
- Confirm `total_cmp` used for all score comparisons.
- Add stress test with near-equal and NaN scores.
- Ensure no RNG used in tie-break path.

**Primary Files:**
- `crates/adapteros-lora-router/src/router.rs`
- `crates/adapteros-lora-router/tests/determinism.rs`

**Acceptance Criteria:**
- 1000-iteration determinism stress test passes.
- No RNG in the sort path.

---

### R5: macOS Version Gating for MLTensor

**Requirement:** Production mode disables MLTensor unless macOS 26+.

**Implementation Tasks:**
- Detect MLTensor API version at runtime.
- Disable MLTensor in production when version < Tahoe.
- Log warnings when MLTensor is disabled for determinism.

**Primary Files:**
- `crates/adapteros-lora-kernel-coreml/src/lib.rs`

**Acceptance Criteria:**
- Production mode on macOS 15 uses fallback path with warning.
- macOS 26+ uses deterministic scheduling policy when enabled.

---

### R6: Attestation Report Generation

**Requirement:** Every inference emits a DeterminismReport and embeds it in RunReceipt.

**Implementation Tasks:**
- Ensure CoreML backend populates DeterminismReport fields.
- Embed report in `RunReceipt.attestation` payload.
- Verify report is serialized and signed in receipt flow.

**Primary Files:**
- `crates/adapteros-lora-kernel-coreml/src/lib.rs`
- `crates/adapteros-lora-worker/src/lib.rs`
- `crates/adapteros-types/src/inference.rs`

**Acceptance Criteria:**
- DeterminismReport present in run receipts for CoreML.
- Receipt signature verifies offline.

---

### R7: Offline Replay Verification

**Requirement:** Replay harness verifies inference determinism without network calls.

**Implementation Tasks:**
- Ensure evidence bundle schema matches replay harness.
- Emit deterministic report outputs to `expected_report.json`.
- Add failure-mode mismatch reporting.

**Primary Files:**
- `docs/hardening/replay.md`
- `crates/adapteros-core/src/evidence_verifier.rs`
- `aosctl` replay command implementation

**Acceptance Criteria:**
- Replay succeeds using only local files.
- Mismatch produces detailed non-zero exit report.

---

### R8: Compiler Constraints

**Requirement:** Prohibit `-ffast-math` or equivalents in all compilation paths.

**Implementation Tasks:**
- Validate CI scans for forbidden flags across build artifacts.
- Enforce strict IEEE 754 in Metal shader compilation.
- Gate builds when fast-math flags are detected.

**Primary Files:**
- `crates/adapteros-lora-kernel-coreml/build.rs`
- `metal/src/kernels/*.metal`
- CI determinism checks (cargo test determinism_core_suite + router determinism + fast-math guard)

**Acceptance Criteria:**
- CI fails on forbidden flags.
- Metal kernels compile with strict IEEE mode.

---

## 8. Phased Delivery Plan

### Phase 1: Determinism Enforcement (Validation)

**Scope:**
- R1 ANE-only production enforcement.
- R2 Q15 invariant guard and tests.
- R3 HKDF seed chain verification and golden vectors.
- R4 router tie-breaking determinism tests.
- R5 MLTensor version gating in production mode.

**Deliverables:**
- Determinism enforcement code paths in CoreML backend.
- Router quantization compile-time guard and tests.
- HKDF golden vector tests and determinism logging.
- Router tie-break stress tests.

**Exit Criteria:**
- `cargo test --test determinism_core_suite -- --test-threads=8`, `cargo test -p adapteros-lora-router --test determinism`, and `bash scripts/check_fast_math_flags.sh` pass.
- CoreML production mode fails fast without ANE.
- Router tests pass across 1000 iterations.

---

### Phase 2: Attestation and Receipt Integrity

**Scope:**
- R6 determinism report completeness.
- Receipt embedding, signing, and offline verification.

**Deliverables:**
- DeterminismReport embedded in RunReceipt attestation payload.
- Receipt signature verified by `evidence_verifier`.
- API surfaces reflect determinism attestation fields.

**Exit Criteria:**
- Attested receipts verify offline.
- DeterminismReport fields match production constraints.

---

### Phase 3: Compliance Certification and Replay

**Scope:**
- R7 offline replay verification.
- Operational workflows for audit retention and replay.
- Documentation and training for compliance teams.

**Deliverables:**
- `aosctl replay --verify` passes offline with evidence bundles.
- Mismatch reporting includes precise error codes.
- Audit trail retention guidance and runbook updates.

**Exit Criteria:**
- Golden run replay passes without network access.
- Compliance artifacts complete and reviewable.

---

## 9. Test Plan

### Unit Tests

- Q15 round-trip tests for 0.0 and 1.0.
- HKDF golden vector stability tests.
- Router tie-breaking determinism tests.

### Integration Tests

- CoreML determinism across multiple runs.
- Production mode enforcement without ANE.
- Swift/ObjC++ equivalence within <=2 ULP.

### End-to-End Tests

- `cargo test --test determinism_core_suite -- --test-threads=8`
- `cargo test -p adapteros-lora-router --test determinism`
- `cargo test -p adapteros-lora-kernel-coreml --test determinism_tests`

---

## 10. Rollout Plan

1. Ship determinism enforcement behind production mode flag.
2. Validate determinism in staging with golden run suites.
3. Enable receipt attestation and offline replay in pilot accounts.
4. Require determinism policy for regulated enterprise tenants.

---

## 11. Metrics and Success Criteria

1. Bit-exact replay success rate: 100 percent on CoreML.
2. Offline replay verification succeeds without network access.
3. DeterminismReport present and signed in all CoreML run receipts.
4. Production mode fails fast when determinism cannot be guaranteed.

---

## 12. Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| ANE unavailable | High | Fail-fast with clear error message |
| macOS < 26 | Medium | Disable MLTensor in production mode |
| Q15 denominator drift | Critical | Compile-time assert and tests |
| HKDF algorithm drift | Critical | Golden vector tests in CI |
| Cross-backend replay attempted | High | Document single-backend requirement |

---

## 13. Open Questions

1. Which team owns receipt signature key management for 7-year retention?
2. Should determinism attestation be required for all tenants or only regulated tiers?
3. What is the official audit evidence bundle retention path (filesystem vs object store)?

---

## 14. References

- `docs/COREML_DETERMINISM_AUDIT_TRAILS.md`
- `docs/COREML_BACKEND.md`
- `docs/DETERMINISM.md`
- `docs/hardening/replay.md`
