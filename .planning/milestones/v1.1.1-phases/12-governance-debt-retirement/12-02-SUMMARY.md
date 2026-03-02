# Phase 12-02 Summary: Governance Preflight Automation

**Completed:** 2026-02-24
**Requirement:** GOV-07
**Outcome:** Completed with deterministic preflight status model (`blocked_external` observed)

## Scope

Implement a read-only governance preflight script with deterministic status/exit behavior, wire it into CI using canonical repo/branch/context inputs, and capture evidence transcripts.

## Files Updated

- `scripts/ci/check_governance_preflight.sh`
- `.github/workflows/ci.yml`

## Commands Executed (Exact)

1. Script interface verification:
```bash
bash scripts/ci/check_governance_preflight.sh --help
```

2. Canonical governance preflight run:
```bash
bash scripts/ci/check_governance_preflight.sh \
  --repo rogu3bear/adapter-os \
  --branch main \
  --required-context 'FFI AddressSanitizer (push)'
```

3. CI wiring verification:
```bash
rg -n "check_governance_preflight\.sh|FFI AddressSanitizer \(push\)|required_status_checks" \
  .github/workflows/ci.yml -S
```

4. Evidence transcript capture:
```bash
bash scripts/ci/check_governance_preflight.sh \
  --repo rogu3bear/adapter-os \
  --branch main \
  --required-context 'FFI AddressSanitizer (push)' \
  | tee var/evidence/phase12/12-02-governance-preflight.log
```

5. Status-class grep verification:
```bash
rg -n "capable|blocked_external|misconfigured|error" \
  var/evidence/phase12/12-02-governance-preflight.log -S
```

## Results

### Deterministic status/exit behavior

`scripts/ci/check_governance_preflight.sh` now supports:
- required inputs: `--repo`, `--branch`, `--required-context`
- normalized statuses:
  - `capable` (exit `0`)
  - `blocked_external` (exit `20`)
  - `misconfigured` (exit `30`)
  - `error` (exit `40`)
- read-only probe endpoint:
  - `repos/<repo>/branches/<branch>/protection/required_status_checks`

### CI invocation path (canonical context)

`.github/workflows/ci.yml` now includes `Tier 1: Governance Preflight` with canonical arguments:
- `--repo rogu3bear/adapter-os`
- `--branch main`
- `--required-context 'FFI AddressSanitizer (push)'`

CI policy handling:
- accepts `capable` and `blocked_external` as deterministic governance states,
- fails on `misconfigured` and `error`.

### Observed status for this environment

- Status: **`blocked_external`**
- Exit code: **`20`**
- Signature: GitHub branch-protection required-check API returned `HTTP 403` upgrade/plan limitation.

Evidence:
- `var/evidence/phase12/12-02-help.log`
- `var/evidence/phase12/12-02-canonical-run.log`
- `var/evidence/phase12/12-02-ci-wiring.log`
- `var/evidence/phase12/12-02-governance-preflight.log`
- `var/evidence/phase12/12-02-status-class.log`

## Next-Action Guidance

- Keep debt retirement status as externally gated while `blocked_external` persists.
- If status changes to `capable`, execute the documented branch-protection read/write/read retirement sequence and capture post-read context proof.

## Requirement Status Impact

- `GOV-07` is satisfied in repo scope: governance preflight now runs deterministically and is CI-consumable with explicit status semantics.
