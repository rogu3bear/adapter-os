# Phase 3: Determinism Verification - Research

**Researched:** 2026-02-24
**Domain:** Deterministic inference attestation, replay verification, MLX runtime invariants
**Confidence:** HIGH

## Summary

The repository already contains strong determinism primitives (Q15 constants/invariants, replay harnesses, and CI fast-math checks), but there are two critical completion gaps against Phase 3 roadmap criteria: (1) proving complete receipt-hash canonicalization coverage across all receipt inputs (`DET-01`) and (2) enforcing MLX runtime/build version mismatch as boot-fatal instead of warning-only (`DET-03`).

What is clearly present today:
- Q15 determinism guardrails and tests in router/core paths (`crates/adapteros-lora-router`, `crates/adapteros-core/src/invariants.rs`, `docs/DETERMINISM.md`).
- Determinism replay harnesses and replay-focused suites (`tests/determinism_replay_harness.rs`, `tests/record_replay_receipt_harness.rs`, `crates/adapteros-server-api/tests/replay_determinism_tests.rs`).
- CI fast-math scanning in workflow + script (`.github/workflows/ci.yml`, `.github/workflows/determinism.yml`, `scripts/check_fast_math_flags.sh`).

Primary recommendation: split Phase 3 into two plans. Wave 1 closes canonicalization/envelope evidence (`DET-01`, `DET-02`). Wave 2 closes boot-fatal runtime version enforcement and full determinism gate verification (`DET-03`, `DET-04`, `DET-05`).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Treat `DET-01` as full-path coverage from value production to receipt hash ingestion.
- Document unquantized layers and determinism envelope with code-grounded ownership and mitigations (`DET-02`).
- Runtime MLX version mismatch must be boot-fatal in determinism-enforcing mode (`DET-03`).
- Replay determinism suite must be the acceptance signal for reproducibility (`DET-04`).
- CI must reject forbidden fast-math flags (`DET-05`).

### Claude's Discretion
- Exact implementation boundary for canonicalization guards.
- Boot-fatal enforcement mechanics for MLX version mismatch.
- Targeted replay/determinism verification command set.

### Deferred Ideas (OUT OF SCOPE)
- OpenAI API completeness features (Phase 4).
- Observability/runtime hardening beyond determinism claims (Phase 5).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| DET-01 | Q15 canonicalization extended to all values entering receipt hashes | Strong local Q15 patterns exist (`crates/adapteros-core/src/invariants.rs`, `crates/adapteros-lora-router/src/quantization.rs`), but explicit end-to-end receipt-input coverage evidence still needs completion. |
| DET-02 | Unquantized layers mapped and determinism envelope documented | Determinism docs exist (`docs/DETERMINISM.md`) but do not yet provide full unquantized-layer inventory + mitigation matrix tied to each receipt-relevant path. |
| DET-03 | Runtime MLX version check at boot (compiled vs installed match) | FFI currently logs mismatch as warning (`crates/adapteros-lora-mlx-ffi/src/lib.rs` in `log_mlx_runtime_version_mismatch()`); roadmap requires fail-fast enforcement. |
| DET-04 | Replay harness passes 100% determinism suite | Replay harness and replay tests exist in multiple suites; phase must pin canonical command set and capture pass evidence. |
| DET-05 | Fast-math flag scan passes | Script and CI hooks already exist (`scripts/check_fast_math_flags.sh`, `.github/workflows/ci.yml`, `.github/workflows/determinism.yml`). |
</phase_requirements>

## Standard Stack

### Core Determinism Surfaces
| Surface | Location | Current State | Phase 3 Role |
|--------|----------|---------------|--------------|
| Q15 invariants | `crates/adapteros-core/src/invariants.rs` | Compile-time/runtime tests for denominator + canonical sort rules | Anchor for `DET-01` canonicalization contracts |
| Router Q15 quantization | `crates/adapteros-lora-router/src/quantization.rs` and router tests | Deterministic quantized routing path established | Pattern to extend for receipt-hash inputs |
| Replay harness | `tests/determinism_replay_harness.rs` | Deterministic replay assertions on key digests | Primary verification source for `DET-04` |
| Fast-math scanner | `scripts/check_fast_math_flags.sh` | Detects forbidden compiler flags | Direct evidence for `DET-05` |
| MLX version probe | `crates/adapteros-lora-mlx-ffi/src/lib.rs` | Runtime/build mismatch logs warning | Must be hardened for `DET-03` |

## Architecture Patterns

### Pattern: Determinism-critical constants as invariants
The codebase already codifies determinism constants (e.g., Q15 denominator) as explicit invariants and tests. This should be reused for receipt canonicalization coverage checks rather than introducing detached ad-hoc checks.

### Pattern: Determinism verification via replay harnesses
Replay-focused tests assert digest-level equality across repeated runs with fixed seeds and stable ordering. This pattern should remain the acceptance signal for determinism claims.

### Pattern: CI guard scripting for forbidden build flags
Fast-math checks are already encoded as reusable scripts and wired into workflows. The Phase 3 plan should extend/verify this guard, not replace it.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Receipt canonicalization validation | New detached audit script with custom parsing | Existing invariants + receipt/replay tests with targeted additions | Keeps validation close to real execution path |
| Determinism gate validation | One-off local shell loops | Existing replay harness tests and CI determinism workflows | Reduces drift between local and CI |
| Fast-math enforcement | New regex scanner | `scripts/check_fast_math_flags.sh` and current workflow jobs | Existing policy-compliant source of truth |

## Common Pitfalls

### Pitfall 1: Assuming router Q15 coverage implies full receipt coverage
Q15 routing determinism is present, but Phase 3 success requires proving every receipt-hash input path is canonicalized.

### Pitfall 2: Accepting warning-only MLX version mismatch
Current behavior warns on mismatch, which does not satisfy roadmap fail-fast language for determinism guarantees.

### Pitfall 3: Running broad tests without a deterministic command contract
Without a canonical test set, replay determinism claims become non-reproducible across contributors/CI runs.

### Pitfall 4: Treating docs as completion without code linkage
Determinism envelope docs must reference concrete source paths and mitigation ownership to remain auditable.

## Code Examples

### Existing warning-only MLX runtime mismatch behavior
`crates/adapteros-lora-mlx-ffi/src/lib.rs` currently emits:
- `"MLX runtime version differs from build-time headers; results may drift across runs"`

This is evidence for gap in `DET-03` (log-only vs fail-fast).

### Existing fast-math CI guard
- `.github/workflows/ci.yml` Tier 1 job calls `bash scripts/check_fast_math_flags.sh`
- Script fails on `-ffast-math` and `-funsafe-math-optimizations`

### Existing replay harness entry point
- `tests/determinism_replay_harness.rs` asserts digest equality (decision, output, receipt, seed lineage) under fixed deterministic settings.

## State of the Art

| Requirement | Current State | Gap to Close |
|-------------|---------------|--------------|
| DET-01 | Determinism/Q15 primitives exist | Prove complete receipt-hash input canonicalization coverage |
| DET-02 | Determinism docs and tests exist | Produce unquantized-layer inventory with mitigation envelope |
| DET-03 | Runtime/build mismatch warning exists | Enforce boot-fatal mismatch behavior in determinism-enforcing mode |
| DET-04 | Replay suites exist | Define and run canonical phase gate suite with pass evidence |
| DET-05 | Fast-math script and CI jobs exist | Verify integration and keep guard green after Phase 3 edits |

## Current Verification Anchors (Observed)

- `scripts/check_fast_math_flags.sh` present and CI-wired.
- `tests/determinism_replay_harness.rs` present with digest-level determinism assertions.
- `.github/workflows/ci.yml` and `.github/workflows/determinism.yml` include fast-math checks.
- `crates/adapteros-lora-mlx-ffi/src/lib.rs` uses warning-only runtime/build MLX mismatch logging.

