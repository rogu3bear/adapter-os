# Determinism Regression Harness

This repository maintains a small, CPU-only regression harness for determinism-critical invariants.

Non-goals:
- End-to-end inference
- Real workers/GPUs
- Timing assertions

## What It Covers

- Stable ordering (`stable_id`) invariants
- Q15 gate determinism invariants (denominator = `32767.0`)
- V7 receipt digest stability (golden)
- Cache credit attestation enforcement (P0-1)
- Stop-controller sentinel encoding (deterministic `None` encoding)

## Run Locally

Run the minimal set of regression tests (serial execution):

```bash
cargo test -p adapteros-core --test determinism_regression_harness -- --test-threads=1

cargo test -p adapteros-db --test cache_attestation_enforcement -- --test-threads=1
```

## CI

The CI job `determinism-gate` runs only the commands above.
