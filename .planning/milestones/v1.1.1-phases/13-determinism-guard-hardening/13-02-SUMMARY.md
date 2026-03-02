# Phase 13-02 Summary: Replay Guardrails in CI and Readiness

**Completed:** 2026-02-24
**Requirement:** DET-08
**Outcome:** Completed with explicit replay-guard readiness semantics and maintained CI merge-gate wiring

## Scope

Tighten replay determinism guardrails so merge-time CI and runtime readiness surfaces expose one explicit guard contract (including strict fail-closed behavior).

## Files Updated

- `.github/workflows/ci.yml`
- `crates/adapteros-server-api/src/handlers/health.rs`
- `crates/adapteros-server-api/tests/health_readyz_timeout_tests.rs`
- `crates/adapteros-server-api/tests/replay_guard_health_tests.rs`
- `tests/determinism_replay_harness.rs`

## Commands Executed (Exact)

1. Baseline CI/readiness guardrail inventory:
```bash
rg -n "replay-harness|determinism-gate|determinism_replay_harness|replay_determinism_tests|readyz|healthz" \
  .github/workflows/ci.yml \
  crates/adapteros-server-api/src/handlers/health.rs \
  crates/adapteros-server-api/tests/health_readyz_timeout_tests.rs
```

2. Replay harness execution (rectified invocation):
```bash
cargo test -p adapter-os --no-default-features --test determinism_replay_harness -- --test-threads=1 --nocapture 2>&1 \
  | tee var/evidence/phase13/13-02-determinism-replay-harness-package-nodflt.log
```

3. Server replay determinism suite:
```bash
cargo test -p adapteros-server-api --test replay_determinism_tests -- --test-threads=1
```

4. Fast-math determinism guard:
```bash
bash scripts/check_fast_math_flags.sh
```

5. Readiness structure + replay guard surfacing tests:
```bash
cargo test -p adapteros-server-api --test health_readyz_timeout_tests -- --test-threads=1
cargo test -p adapteros-server-api --test replay_guard_health_tests -- --test-threads=1 --nocapture
```

## Results

### Replay guardrails remain merge-gating in CI

`ci.yml` still includes determinism guard lanes with explicit replay coverage:
- `replay-harness`
- `determinism-gate`
- replay API determinism tests
- fast-math flag guard

### Runtime readiness now surfaces replay guard explicitly

`/readyz` now carries replay-guard semantics and metrics signals (`readyz_replay_guard_ok`, `readyz_replay_guard_age_seconds`) with strict-mode fail-closed readiness gating.

### Targeted guard tests passed

- `determinism_replay_harness` (package-scoped, no-default-features): `12` passed, `1` ignored.
- `replay_determinism_tests`: `32` passed.
- `health_readyz_timeout_tests`: `33` passed.
- `replay_guard_health_tests`: `5` passed.
- `check_fast_math_flags.sh`: `OK`.

### Execution note

Legacy workspace-scoped harness invocation can hit `SIGKILL` on this host due transitive compile fanout. The replay lane is now rectified to the package-scoped/no-default-features command, which is deterministic and passes locally.

Evidence:
- `var/evidence/phase13/13-02-baseline-replay-health.log`
- `var/evidence/phase13/13-02-determinism-replay-harness-package-nodflt.log`
- `var/evidence/phase13/13-02-determinism-replay-harness-sigkill.log` (legacy invocation diagnostic)
- `var/evidence/phase13/13-02-replay-determinism-tests.log`
- `var/evidence/phase13/13-02-fast-math-flags.log`
- `var/evidence/phase13/13-02-health-readyz-timeout-tests.log`
- `var/evidence/phase13/13-02-replay-guard-health-tests.log`

## Requirement Status Impact

- `DET-08` is satisfied in repo scope: replay determinism guardrails remain merge-gating in CI and are explicitly surfaced in runtime readiness/health semantics.
