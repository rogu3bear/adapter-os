# PRD: Sync PolicyId Enum with CLI Range Checks

## Problem

The `PolicyId` enum defines 29 policy packs, but CLI validation still hard-codes a 1..=25 range and the list output reports a fixed total of 20. This prevents access to policies 26-29 and allows documentation and logs to drift from the enum source of truth. The CLI also performs unsafe transmute-based conversion.

## Non-goals

- Changing policy pack implementations or enforcement logic.
- Adding or removing policy packs.
- Generating shell completions or other CLI tooling.
- Reworking database schema or API payloads.

## Proposed Approach

- Add `#[repr(u8)]` and a safe `TryFrom<u8>` implementation to `PolicyId` with range validation derived from `PolicyId::all()`.
- Provide `PolicyId::count()` and `PolicyId::max_id()` helpers to keep callers aligned with the enum.
- Update CLI parsing to use `PolicyId::try_from` and remove unsafe transmute usage.
- Update CLI list output to report totals from `PolicyId::count()`.
- Update policy-related docs/tests/log strings to avoid hard-coded counts.

## Acceptance Criteria

- `aosctl policy explain 26` and `aosctl policy explain 29` succeed.
- `aosctl policy explain 0`, `aosctl policy explain 30`, and larger invalid IDs return a validation error with the current max bound.
- `aosctl policy list` prints `Total: X / 29 policies` (derived from `PolicyId::count()`).
- No unsafe transmute remains in policy ID parsing paths.
- Adding policy 30 only requires editing the enum/list source (no CLI range updates).

## Test Plan

- `cargo test -p adapteros-policy`
- `cargo test -p adapteros-cli`
- Manual CLI checks:
  - `./aosctl policy explain 26`
  - `./aosctl policy explain 29`
  - `./aosctl policy explain 0` (expect error)
  - `./aosctl policy explain 999` (expect error)

## Rollout Plan

- Ship in the next release without migration steps.
- Monitor CLI usage for validation errors related to policy IDs.
- Add release note entry if required by the release process.
