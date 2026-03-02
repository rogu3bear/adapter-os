# Local CI Policy

This repository uses local, script-driven gates as the canonical validation path.

## Policy

- Remote GitHub Actions workflow execution is not used for release governance in this repo.
- Required checks must run locally via repository scripts.
- Governance preflight is optional in local mode and defaults to disabled (`LOCAL_RELEASE_GOVERNANCE_MODE=off`).

## Required Local Entrypoints

- `bash scripts/ci/local_required_checks.sh`
- `bash scripts/ci/local_release_gate.sh`
- `bash scripts/ci/local_release_gate_prod.sh` (canonical prod mode wrapper)

## Included Local Gates

- Port drift contract: `scripts/contracts/check_port_contract.sh`
- Rectification contracts: `scripts/contracts/check_all.sh`
- Formatting/linting/tests (selected lanes) via local required checks
- Release smoke lane via `local_release_gate.sh`

## Notes

For a production cut gate, run:

```bash
bash scripts/ci/local_release_gate_prod.sh
```

`local_release_gate_prod.sh` enforces:
- inference lane enabled (`LOCAL_RELEASE_RUN_INFERENCE=1`)
- exhaustive clippy scope (`LOCAL_REQUIRED_CLIPPY_SCOPE=all-targets`)
- strict route coverage closure checks (`ROUTE_COVERAGE_STRICT_OPENAPI_ONLY=1`, `ROUTE_COVERAGE_STRICT_PARAM_MISMATCH=1`)
- full MVP smoke (no `MVP_SMOKE_SKIP_*` shortcuts)
- strict inference smoke (`SMOKE_INFERENCE_STRICT=1`)
- release artifact integrity (`scripts/release/sbom.sh` with `SBOM_REQUIRE_SIGNING=1`)
- release verification log capture to `.planning/prod-cut/evidence/release/release_verification.log`
- governance mode defaults to `off` (set `LOCAL_RELEASE_GOVERNANCE_MODE=warn` or `enforce` to re-enable GitHub preflight lane)

`local_release_gate.sh` will auto-start a local backend for MVP smoke when the API is not already healthy on `AOS_SERVER_PORT`, and auto-stop it on exit if it started it.

If you need exhaustive clippy coverage (tests/examples included), run:

```bash
LOCAL_REQUIRED_CLIPPY_SCOPE=all-targets bash scripts/ci/local_required_checks.sh
```

Default local required clippy scope is workspace `lib/bin` targets on default feature sets.
