# PRD: PolicyId and CLI Range Sync

**Status:** Draft  
**Last Updated:** 2026-01-05  
**Owner:** Engineering  
**Related Docs:** crates/adapteros-policy/src/registry.rs, crates/adapteros-cli/src/commands/policy.rs, docs/POLICIES.md

---

## 1. Summary

The `PolicyId` enum currently has 29 entries, but CLI validation and messaging still assume a smaller, hardcoded range. This PRD defines changes that derive ranges and display counts from the enum itself to prevent drift and keep policy tooling in sync.

---

## 2. Problem Statement

Hardcoded ranges in `aosctl policy` prevent access to newer policy IDs and can mislead operators. Counts in CLI output and documentation become stale as new policies are added.

---

## 3. Goals

1. CLI accepts all valid `PolicyId` values.
2. Range validation and totals derive from the enum, not hardcoded values.
3. Unsafe conversions are removed in favor of safe `TryFrom`.

---

## 4. Non-Goals

- Changing policy semantics or enforcement behavior.
- Reworking registry layouts beyond range handling.

---

## 5. Proposed Approach

- Add `#[repr(u8)]`, `count()`, `max_id()`, and `TryFrom<u8>` to `PolicyId`.
- Update CLI parsing to use `PolicyId::try_from` and dynamic max ID.
- Update CLI totals to use `PolicyId::count()`.
- Update tests and CI scripts to validate counts dynamically.

---

## 6. Requirements and Implementation Plan

### R1: Safe PolicyId Conversion

**Requirement:** Convert numeric IDs using `TryFrom<u8>`.

**Implementation Tasks:**
- Add `#[repr(u8)]` to `PolicyId`.
- Implement `TryFrom<u8>` with dynamic range checks.

**Acceptance Criteria:**
- No `unsafe` conversion paths remain for PolicyId IDs.

---

### R2: CLI Range Validation

**Requirement:** CLI range checks use `PolicyId::max_id()`.

**Implementation Tasks:**
- Update `policy explain` and other commands to parse IDs via `PolicyId::try_from`.
- Provide consistent validation errors for out-of-range IDs.

**Acceptance Criteria:**
- `aosctl policy explain 26` and `29` succeed.
- `aosctl policy explain 0` and `999` fail with range errors.

---

### R3: Dynamic Totals and Messaging

**Requirement:** CLI totals reflect `PolicyId::count()`.

**Implementation Tasks:**
- Replace hardcoded totals in CLI output.
- Remove hardcoded ranges from help text.

**Acceptance Criteria:**
- `aosctl policy list` prints the current total policy count.

---

### R4: Tests and CI Guardrails

**Requirement:** Tests and scripts validate policy counts dynamically.

**Implementation Tasks:**
- Update policy validation tests to use `PolicyId::count()`.
- Update policy registry CI script messaging to avoid hardcoded counts.

**Acceptance Criteria:**
- Adding a new policy only requires updating the enum and `PolicyId::all()`.

---

## 7. Test Plan

```bash
# Valid policy IDs
for i in $(seq 1 29); do
  aosctl policy explain $i || exit 1
done

# Invalid IDs
aosctl policy explain 0 && exit 1
aosctl policy explain 999 && exit 1
```

---

## 8. Rollout Plan

1. Land enum and CLI updates in a single release.
2. Monitor CLI usage for validation errors.
3. Update external documentation as policy counts evolve.

---

## 9. Open Questions

1. Should public documentation avoid specific policy counts to prevent drift?
