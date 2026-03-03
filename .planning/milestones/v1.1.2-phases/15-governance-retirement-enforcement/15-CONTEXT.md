---
phase: "15"
name: "Governance Retirement Enforcement"
created: 2026-02-25
---

# Phase 15: Governance Retirement Enforcement — Context

## Decisions

- Repository target is fixed: `rogu3bear/adapter-os`, branch `main`.
- Capability gate is mandatory before any write: run `check_governance_preflight.sh`; if exit `20` (`blocked_external`), do not PATCH branch protection.
- Policy mode is fixed to preserve + add: required contexts must become `union(existing, checklist_set)` with `strict=true`.
- Checklist canonical contexts are fixed:
  - `CI`
  - `Stability Gate`
  - `Integration Tests`
  - `Cross-Hardware Determinism`
  - `Migration Testing`
  - `Security Regression Tests`
  - `Check Merge Conflicts`
  - `FFI AddressSanitizer (push)`
- Evidence must be immutable and timestamped under `var/evidence/governance-retirement-<UTCSTAMP>/`.
- Planning/audit closure edits are allowed only after proof of capability and successful read/write/read verification.

## Discretion Areas

- Exact UTC timestamp format and helper commands used to materialize evidence paths.
- Validation script shape for preserve+add checks (shell + `jq` vs compact one-liners), as long as outputs are deterministic and archived.
- Whether docs are edited directly or by small scripted replacements, as long as wording rules are satisfied.

## Deferred Ideas

- Continuous drift detection for required-check policy across multiple branches/repos (out of scope for this patch milestone).
- Converting governance retirement verification into a scheduled automation after closure proof is complete.
