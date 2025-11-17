# PRD 5.3.5: LoRA Hot-Swap Hallucination Rectification

**Purpose:** Address minor inaccuracies and overstatements identified in hallucination audit of PRD 5.3.4 implementation. Ensure all claims are verifiable and implementation is 100% grounded. No fabricated success metrics or untested assumptions.

**Last Updated:** 2025-11-17  
**Maintained by:** AI Assistant  
**Status:** Draft (Post-Hallucination Audit)

---

## Overview

Hallucination audit revealed ~5% overstatements in test realism, SLO claims, and verification status. Rectify to achieve fully auditable, production-ready hot-swap with proven metrics.

**Context:** PRD 5.3.4 fixed cut corners, but audit found:
- Test integration overstated (auth stubs, not real).
- SLO thresholds optimistic (1.2x vs realistic 1.5x).
- Miri/cleanliness claims unverified (not run).
- Test duration shortened without full verification.

**Goals:** Verifiable SLO, real e2e testing, proven safety. No hallucinations in docs/code.

---

## Requirements

### 1. Test Realism Rectification
**Description:** Replace auth stubs with real middleware integration. Ensure 90%+ real integration coverage.

**Functional:**
- **Auth Integration:** Use real `dual_auth_middleware` in test server setup (JWT validation, Claims extraction).
- **E2E Requests:** Full HTTP flow (middleware → handlers → DB) for inference/swap endpoints.
- **Panic Detection:** Enhanced `catch_unwind` with proper error propagation.
- **Setup:** Real AppState with seeded data (adapters, stacks, tenants).

**Non-Functional:**
- Coverage: Unit 100%, integration 90%+, e2e 80%+ (run with real DB pool).
- Auth: Supports "Bearer adapteros-local" and JWT flows.
- No Stubs: All mocks removed; real telemetry emission.

### 2. SLO Claims Calibration
**Description:** Adjust thresholds based on real load characteristics. Prove via multiple runs.

**Functional:**
- **Baseline:** Measure p95 across 10 runs (target <150ms for mock inference).
- **Load:** p95 ≤1.5x baseline (accounts for real overhead).
- **Metrics:** Export to `metrics` crate; log histograms.
- **Validation:** Asserts fail if SLO breached; retry on flakiness.

**Non-Functional:**
- Deterministic: Fixed seed for load gen (HKDF-derived).
- Performance: Test completes <5min in CI.

### 3. Safety Verification
**Description:** Run actual Miri checks; verify no UB in unsafe code.

**Functional:**
- **Miri Run:** `cargo +nightly miri test hotswap_load -- --nocapture` on KV/cache code.
- **Alignment:** Confirm Metal buffer asserts prevent UB (test on macOS).
- **Async Safety:** Verify `spawn_blocking` doesn't deadlock (loom test if possible).

**Non-Functional:**
- Clean: 0 Miri errors, 0 UB warnings.
- Platform: macOS for Metal; skip on others.

### 4. Documentation Accuracy
**Description:** Correct overstated claims in PRDs/docs.

**Functional:**
- Update PRD 5.3.4 acceptance criteria (1.5x SLO, "potential" Miri).
- CHANGELOG.md: Remove "100% SLO compliance"; add "test framework established".
- README.md: Clarify "hot-swap endpoints" with caveats.

**Non-Functional:**
- Deterministic: No subjective language ("robust" → "verifiably safe").
- Audit-Ready: All claims backable by test logs.

---

## Acceptance Criteria

- **Audit Clean:** Re-run hallucination audit; 0 inaccuracies (all claims verifiable via logs/code).
- **Test Pass:** `cargo test hotswap_load` succeeds with real auth; p95 metrics logged; no panics/5xx.
- **Safety Proven:** Miri clean on macOS; no UB in KV zeroize.
- **Docs Accurate:** PRDs match implementation; no overclaims.
- **Build Stable:** `cargo check/test` green; no flakiness in 3 runs.

**Out of Scope:** New features; multi-host testing.

---

## Implementation Plan

1. **Test Fixes (2 days):** Integrate real middleware, seed data, remove stubs.
2. **SLO Calibration (1 day):** Run 10 baselines, adjust asserts, add retries.
3. **Safety Verification (1 day):** Run Miri, fix any UB (unlikely).
4. **Docs Update (0.5 day):** Correct PRDs/docs based on real results.

**Total Effort:** 4.5 days. Low risk; mostly polish.

---

## References

- Original Audit: Inline in hallucination audit response.
- PRD 5.3.4: docs/PRD_5_3_4_HotSwap_CutCorners_Fixes.md.
- Citations: [source: tests/hotswap_load_test.rs L14-L160] (for current test).
