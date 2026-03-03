# Phase 13-01 Summary: Determinism Diagnostics Freshness Contract

**Completed:** 2026-02-24
**Requirement:** DET-07
**Outcome:** Completed with explicit machine-readable freshness semantics (`fresh`/`stale`/`unknown`)

## Scope

Harden `/v1/diagnostics/determinism-status` so determinism freshness is explicit and actionable for operators/automation, with targeted regression coverage and runbook alignment.

## Files Updated

- `crates/adapteros-api-types/src/diagnostics.rs`
- `crates/adapteros-server-api/src/handlers/diagnostics.rs`
- `crates/adapteros-server-api/tests/diag_status_freshness_tests.rs`
- `docs/runbooks/DETERMINISM_VIOLATION.md`
- `crates/adapteros-server-api/src/routes/mod.rs`

## Commands Executed (Exact)

1. Baseline writer/reader/schema contract:
```bash
rg -n "INSERT INTO determinism_checks|determinism-status|last_run|result|divergences" \
  crates/adapteros-cli/src/commands/diag.rs \
  crates/adapteros-server-api/src/handlers/diagnostics.rs \
  migrations/20260217090000_determinism_checks.sql
```

2. Targeted freshness tests:
```bash
cargo test -p adapteros-server-api --test diag_status_freshness_tests -- --test-threads=1 --nocapture
```

3. Producer/runbook alignment check:
```bash
rg -n "/v1/diagnostics/determinism-status|determinism_checks" \
  docs/runbooks/DETERMINISM_VIOLATION.md \
  crates/adapteros-cli/src/commands/diag.rs \
  crates/adapteros-server-api/src/handlers/diagnostics.rs
```

## Results

### Determinism freshness contract is explicit and typed

`DeterminismStatusResponse` now includes typed freshness fields:
- `freshness_status`: `fresh | stale | unknown`
- `freshness_reason`: machine-readable reason enum
- `freshness_age_seconds` + `freshness_threshold_seconds`

### Freshness evaluation is fail-safe and reasoned

The diagnostics handler now classifies determinism freshness from persisted `determinism_checks.last_run` with explicit reasons for:
- missing row / missing timestamp,
- invalid timestamp,
- future timestamp,
- stale timestamp,
- recent (fresh) timestamp,
- query failure.

### Regression coverage and docs are aligned

- `diag_status_freshness_tests` passed (`7` tests).
- Runbook references `/v1/diagnostics/determinism-status` and freshness remediation semantics that match handler behavior.

Evidence:
- `var/evidence/phase13/13-01-baseline-contract.log`
- `var/evidence/phase13/13-01-diag-status-freshness.log`
- `var/evidence/phase13/13-01-runbook-alignment.log`

## Requirement Status Impact

- `DET-07` is satisfied: determinism diagnostics freshness is explicit, machine-readable, and regression-tested.
