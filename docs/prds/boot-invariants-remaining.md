# PRD: Remaining Boot Invariants (SEC/DAT/MEM/CON/LIF)

**Status:** Draft
**Last Updated:** 2026-01-05
**Owner:** Engineering
**Related Docs:**
- docs/BOOT_PHASES.md
- docs/BOOT_WALKTHROUGH.md
- docs/SECURITY.md
- crates/adapteros-server/src/boot/invariants.rs

---

## 1. Summary

Boot invariant analysis found 28 additional checks that are documented but not enforced at startup. This PRD defines the missing invariants, their failure modes (fail-open vs fail-closed), and a staged implementation plan that preserves deterministic boot behavior while avoiding false positives in development.

---

## 2. Problem Statement

The server can currently boot with invalid or unsafe state because only a subset of boot invariants are enforced. Missing invariants allow security, data integrity, memory, concurrency, or lifecycle issues to pass silently, which can surface later as hard-to-debug runtime failures. The invariant list already exists in code comments, but enforcement and reporting are incomplete.

---

## 3. Goals

1. Implement all 28 documented boot invariants with explicit failure modes.
2. Fail closed for P0 security and data integrity invariants in production.
3. Fail open (warn) for runtime invariants where false positives are likely.
4. Provide consistent logging and metrics for invariant checks.
5. Make `aosctl doctor` report invariant status and summaries.

---

## 4. Non-Goals

- Rewriting the boot pipeline or replacing the existing invariant report structure.
- Changing RBAC, auth, or key management behavior beyond boot validation.
- Implementing new invariants outside the documented list.
- Building new storage backends or migration systems.

---

## 5. Scope

### In Scope

- Security invariants SEC-006 through SEC-014.
- Data integrity invariants DAT-001 through DAT-007.
- Memory invariants MEM-001 through MEM-004.
- Concurrency invariants CON-001 through CON-004.
- Lifecycle invariants LIF-001 through LIF-004.
- Logging, metrics, and doctor reporting for the above.

### Out of Scope

- Adding new policy packs beyond boot invariants.
- Modifying the deterministic seed or router behavior.
- Network-level changes or new external dependencies.

---

## 6. Proposed Approach

### 6.1 Invariant Checks

- Keep `validate_boot_invariants` as the single entry point.
- Implement each invariant as a small, testable function that returns either:
  - Pass
  - Violation (fatal or warning)
  - Skipped (explicit escape hatch)
- Use the existing `InvariantReport` to track pass/fail/skip counts.

### 6.2 Failure Modes

- Fail closed in production for SEC-* and DAT-* unless explicitly marked FAILS OPEN.
- Fail open (warn) for invariants marked FAILS OPEN in the list.
- Always log fail-open violations with invariant ID and remediation.

### 6.3 Data and Dependency Access

- Use `BootContext` (or an equivalent structure) to pass in:
  - Config
  - DB handles
  - Runtime subsystem handles (cache, allocator, etc.)
  - Boot phase tracker
- Avoid global mutable access and keep checks deterministic.

### 6.4 Metrics

- Extend boot invariant metrics to include:
  - Per-invariant latency buckets (p50, p95, p99)
  - Total boot invariant wall time
- Ensure metrics are flushed after the metrics exporter initializes.

### 6.5 Reporting

- Extend `aosctl doctor` to include:
  - Invariant pass/fail summary
  - Fail-open warnings
  - Skipped checks with explicit IDs

---

## 7. Work Breakdown (Follow-Up Tasks)

Each task below should be tracked as a separate PR with focused scope.

### Task A: Security Invariants

**Target:** SEC-006 through SEC-014

**Acceptance Criteria:**
- Each SEC-* invariant is implemented and mapped to a concrete code path.
- Fail-closed behavior in production for all SEC-* unless marked FAILS OPEN.
- Unit tests cover pass/fail outcomes for each check.
- `aosctl doctor` surfaces SEC-* status summary.

### Task B: Data Integrity Invariants

**Target:** DAT-001 through DAT-007

**Acceptance Criteria:**
- Each DAT-* invariant validates actual DB state or migration metadata.
- DAT-007 remains fail-open with explicit warning logs.
- Integration tests validate at least one failing and one passing case per invariant.

### Task C: Runtime Invariants (Memory + Concurrency)

**Target:** MEM-001..MEM-004, CON-001..CON-004

**Acceptance Criteria:**
- Fail-open behavior with WARN logs for MEM-003, MEM-004, CON-001, CON-003.
- Checks are safe under load and do not panic in dev mode.
- Load-test proof that checks do not add >250ms boot latency.

### Task D: Lifecycle Invariants

**Target:** LIF-001..LIF-004

**Acceptance Criteria:**
- Boot phase ordering and executor init are validated during startup.
- Fail-open behavior for LIF-001 and LIF-004 is logged with remediation hints.
- Integration tests validate boot ordering violations are surfaced.

### Task E: Metrics and Reporting

**Target:** metrics + `aosctl doctor`

**Acceptance Criteria:**
- Metrics exported for total invariant time and per-invariant latency.
- `aosctl doctor` shows a summary table including pass/fail/skip counts.
- Output stays stable for scripting (documented fields or JSON output).

---

## 8. Overall Acceptance Criteria

- All 28 invariants are implemented with explicit failure modes.
- Fail-open checks log warnings with invariant IDs and remediation.
- Production boot fails fast on fatal SEC/DAT violations.
- Boot time increase remains under 500ms on M1.
- `aosctl doctor` reports invariant status.

---

## 9. Test Plan

- Unit tests per invariant check (pass/fail cases).
- Integration test suite for boot invariants:
  - `cargo test -p adapteros-server --test boot_invariants_tests`
- Manual verification in dev mode using `AOS_DEV_NO_AUTH=1 ./start` with logs.

---

## 10. Rollout Plan

1. Land invariants in phases (Security -> Data -> Runtime -> Lifecycle).
2. Ship fail-open warnings first in dev, then enforce in production.
3. Enable metrics and doctor reporting in the same release as each phase.
4. Monitor boot latency and warning volume; adjust thresholds if needed.

---

## 11. Risks and Open Questions

- Some invariants depend on runtime state that may not be available at boot.
- False positives may block production if checks are too strict.
- Need clear guidance on what data sources are authoritative for each check.

