# PRD: Sync PolicyId Enum with CLI Range Checks

**Status:** Draft
**Last Updated:** 2026-01-05
**Owner:** Engineering
**Related Docs:** `crates/adapteros-policy/src/registry.rs`, `crates/adapteros-cli/src/commands/policy.rs`

---

## 1. Summary

The policy registry contains more policies than the CLI accepts by ID due to hardcoded ranges. This PRD aligns CLI validation, display counts, and documentation with the `PolicyId` enum so policy additions no longer require manual range updates.

---

## 2. Problem Statement

`PolicyId` includes policies 26-29, but CLI parsing still enforces `1..=25` and prints a fixed total count. This blocks access to newer policies and causes documentation drift.

---

## 3. Goals

- Accept all valid `PolicyId` values in CLI commands.
- Derive counts and max IDs from `PolicyId` rather than hardcoded numbers.
- Remove unsafe conversions without an explicit representation.

---

## 4. Non-Goals

- Reworking policy pack implementations or enforcement logic.
- Redesigning policy CLI output formatting.
- Changing the policy registry ordering.

---

## 5. Proposed Approach

- Add `#[repr(u8)]` to `PolicyId` and implement `TryFrom<u8>`.
- Expose `PolicyId::count()` and `PolicyId::max_id()` helpers.
- Update CLI parsing to use `PolicyId::try_from` and dynamic error messages.
- Replace fixed totals in CLI output with `PolicyId::count()`.
- Update policy documentation text to remove hardcoded counts.

---

## 6. Acceptance Criteria

- `aosctl policy explain 26` and `aosctl policy explain 29` succeed.
- CLI error messages use the enum-derived max ID.
- `aosctl policy list` reports totals using `PolicyId::count()`.
- No unsafe `transmute` usage for policy ID parsing.
- Documentation no longer claims a fixed policy count.

---

## 7. Test Plan

- Unit tests for `PolicyId::try_from` with valid and invalid IDs.
- Policy registry tests confirm counts are derived from `PolicyId::count()`.
- CLI smoke test for policy explain accepts IDs 1..max.

---

## 8. Rollout Plan

1. Land enum and CLI parsing changes.
2. Update docs and CI guard messaging.
3. Monitor CLI usage for invalid ID errors.
