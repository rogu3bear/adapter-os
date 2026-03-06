# Rectification Gap Report

Scope: local/dev/test defaults, local-only release gates, and drift guardrails.

## Claim-vs-Source Matrix

| Claim | Source | Severity | Status | Notes |
|---|---|---|---|---|
| Canonical local ports use pane `18080-18119` defaults | `docs/PORTS.md`, `scripts/lib/ports.sh`, `crates/adapteros-core/src/defaults.rs` | High | In progress | Port-pane defaults implemented in shell and Rust defaults. |
| Startup/config scripts derive defaults from one source of truth | `start`, `scripts/check-config.sh`, `scripts/free-ports.sh`, `scripts/service-manager.sh`, `scripts/ui-dev.sh`, `scripts/lib/ports.sh` | High | In progress | Port helper wired into startup/config scripts. |
| Integration/load defaults use canonical control-plane URL contract | `scripts/run_load_tests.sh`, `tests/integration/README.md`, `tests/integration/README_LOAD_TESTS.md`, `tests/integration/test_utils.rs` | High | In progress | Legacy `18083` drift corrected back to control-plane lane. |
| No remote GitHub-hosted workflow gate is required | `docs/LOCAL_CI_POLICY.md`, `scripts/ci/local_required_checks.sh`, `scripts/ci/local_release_gate.sh` | High | In progress | Local gates are canonical release path. |
| Port drift guardrail fails on legacy localhost literals | `scripts/contracts/check_port_contract.sh`, `scripts/contracts/check_all.sh` | High | In progress | Guard is wired into contract suite and local gates. |
| Docs reference canonical local gate path | `MVP_PROD_CHECKLIST.md`, `docs/governance/README.md`, `docs/CONFIGURATION.md` | Medium | In progress | Workflow-centric wording replaced with local-gate policy in key docs. |

## Open Items

- Run full local release gate and capture remaining failures.
- Resolve any clippy/test failures surfaced by `scripts/ci/local_required_checks.sh`.
- Keep contract artifacts regenerated and staged with code changes.
