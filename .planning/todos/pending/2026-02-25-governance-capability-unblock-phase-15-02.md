---
created: 2026-02-25T20:40:00Z
title: Unblock governance capability for phase 15-02
area: planning
files:
  - scripts/ci/check_governance_preflight.sh
  - scripts/ci/execute_governance_required_checks.sh
  - .planning/phases/18-capability-unlock-and-canonical-enforcement/18-03-PLAN.md
  - var/evidence/governance-enforcement-exec-20260226T003700Z/command.log
  - var/evidence/governance-enforcement-rerun-20260227T080938Z/preflight-before.log
---

## Problem

Canonical required-check enforcement still cannot execute the capable write/readback branch because governance preflight returns `blocked_external` (exit `20`, HTTP 403). The executable enforcement flow now exists, but runtime capability remains externally blocked.

## Solution

Re-run the executable enforcement flow after account/repository capability changes:

```bash
bash scripts/ci/execute_governance_required_checks.sh \
  --repo rogu3bear/adapter-os \
  --branch main \
  --required-context 'FFI AddressSanitizer (push)' \
  --manifest docs/governance/target-manifest.json \
  --output-dir var/evidence/governance-enforcement-exec-<UTCSTAMP>
```

When the command returns `status=enforced_verified` (exit `0`), reconcile capable-branch artifacts and close remaining external-blocker debt language.

## Latest Attempt (2026-02-27)

- Command rerun completed with `status=blocked_external` (`exit 20`).
- Fresh evidence directory: `var/evidence/governance-enforcement-rerun-20260227T080938Z/`.
- Safety contract preserved: `blocked-write-attempts.txt` recorded `write_attempts=0` and no rollback attempts.
