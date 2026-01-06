# PRD: Implement Remaining Boot Invariants

**Status:** Draft
**Last Updated:** 2026-01-05
**Owner:** Engineering
**Related Docs:** `crates/adapteros-server/src/boot/invariants.rs`

---

## 1. Summary

Boot invariants cover security, data integrity, and runtime readiness checks. The current implementation only covers a subset. This PRD defines the remaining invariants and their failure modes so the server boot path is verifiably safe.

---

## 2. Problem Statement

Missing boot invariants allow the server to start with invalid or unsafe state. These gaps weaken security posture and complicate production incident response.

---

## 3. Goals

- Implement all documented invariants with explicit fail-open or fail-closed behavior.
- Emit structured logs for invariant failures and skips.
- Keep boot time impact under 500 ms.

---

## 4. Non-Goals

- Redesigning the boot lifecycle or configuration system.
- Adding new invariant categories beyond those documented.
- Making invariants configurable per tenant.

---

## 5. Proposed Approach

- Expand `invariants.rs` with the remaining checks grouped by category.
- Use a consistent `Invariant` structure with failure mode metadata.
- Wire invariants into the boot sequence in category order (security, data, runtime).
- Mark fail-open invariants with explicit WARN logs and metrics.

---

## 6. Acceptance Criteria

- All remaining invariants have check functions and IDs.
- Fail-open invariants log at WARN level with context.
- Fail-closed invariants abort boot in production mode.
- `aosctl doctor` surfaces invariant status.

---

## 7. Test Plan

- Unit tests for each invariant helper where possible.
- Integration test covering boot with a failing security invariant (should abort).
- Integration test covering fail-open runtime invariants (should warn and continue).

---

## 8. Rollout Plan

1. Add security and data integrity invariants.
2. Add runtime invariants with fail-open logging.
3. Update documentation and operator runbooks.

---

## 9. Follow-up Tasks (Tracked)

- TASK-1: Implement security invariants (SEC-*).
  - Acceptance: fail-closed checks block boot in production.
- TASK-2: Implement data integrity invariants (DAT-*).
  - Acceptance: schema and constraint checks pass on healthy DB.
- TASK-3: Implement runtime invariants (MEM/CON/LIF).
  - Acceptance: fail-open checks log WARN and emit metrics.
- TASK-4: Add invariant status output to `aosctl doctor`.
  - Acceptance: doctor output lists invariant IDs and status.
